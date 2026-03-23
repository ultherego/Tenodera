use std::process::Stdio;

use tenodera_protocol::message::Message;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use futures::{SinkExt, StreamExt};

/// Handle to a running tenodera-bridge subprocess.
/// Manages stdin/stdout communication with the bridge process.
pub struct BridgeProcess {
    /// Keep alive — child is killed on drop.
    pub child: Child,
    /// Send messages TO the bridge (gateway → bridge stdin)
    pub to_bridge: mpsc::Sender<Message>,
    /// Receive messages FROM the bridge (bridge stdout → gateway)
    pub from_bridge: mpsc::Receiver<Message>,
}

/// Handle to a remote tenodera-agent WebSocket connection.
pub struct AgentConnection {
    /// Send messages TO the agent (gateway → agent WS)
    pub to_agent: mpsc::Sender<Message>,
    /// Receive messages FROM the agent (agent WS → gateway)
    pub from_agent: mpsc::Receiver<Message>,
}

impl BridgeProcess {
    /// Spawn a local tenodera-bridge process.
    pub async fn spawn(bridge_bin: &str) -> anyhow::Result<Self> {
        let cmd = Command::new(bridge_bin);
        Self::spawn_command(cmd).await
    }

    /// Spawn a tenodera-bridge on a remote host via SSH.
    /// Uses sshpass with the session password (Cockpit model).
    pub async fn spawn_remote(
        ssh_user: &str,
        password: &str,
        address: &str,
        ssh_port: u16,
        remote_bridge_bin: &str,
    ) -> anyhow::Result<Self> {
        let mut cmd = Command::new("sshpass");
        cmd.env("SSHPASS", password);
        cmd.args([
            "-e",
            "ssh",
            "-o", "StrictHostKeyChecking=accept-new",
            "-p", &ssh_port.to_string(),
            &format!("{ssh_user}@{address}"),
            remote_bridge_bin,
        ]);
        Self::spawn_command(cmd).await
    }

    async fn spawn_command(mut cmd: Command) -> anyhow::Result<Self> {
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()?;

        let stdin = child.stdin.take().expect("bridge stdin");
        let stdout = child.stdout.take().expect("bridge stdout");

        let (to_bridge_tx, mut to_bridge_rx) = mpsc::channel::<Message>(256);
        let (from_bridge_tx, from_bridge_rx) = mpsc::channel::<Message>(256);

        // Task: write messages to bridge stdin
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(msg) = to_bridge_rx.recv().await {
                match serde_json::to_string(&msg) {
                    Ok(json) => {
                        let line = format!("{json}\n");
                        if stdin.write_all(line.as_bytes()).await.is_err() {
                            break;
                        }
                        let _ = stdin.flush().await;
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "failed to serialize message to bridge");
                    }
                }
            }
        });

        // Task: read messages from bridge stdout
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                if line.is_empty() {
                    continue;
                }
                match serde_json::from_str::<Message>(&line) {
                    Ok(msg) => {
                        if from_bridge_tx.send(msg).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, raw = %line, "invalid message from bridge");
                    }
                }
            }
            tracing::debug!("bridge stdout reader ended");
        });

        Ok(Self {
            child,
            to_bridge: to_bridge_tx,
            from_bridge: from_bridge_rx,
        })
    }
}

impl AgentConnection {
    /// Connect to a remote tenodera-agent over WebSocket.
    pub async fn connect(
        address: &str,
        port: u16,
        api_key: &str,
        use_tls: bool,
    ) -> anyhow::Result<Self> {
        let scheme = if use_tls { "wss" } else { "ws" };
        let url = if api_key.is_empty() {
            format!("{scheme}://{address}:{port}/ws")
        } else {
            format!("{scheme}://{address}:{port}/ws?api_key={api_key}")
        };

        let (ws_stream, _response) = tokio_tungstenite::connect_async(&url).await
            .map_err(|e| anyhow::anyhow!("WebSocket connect to {url} failed: {e}"))?;

        let (mut ws_sink, mut ws_stream_rx) = ws_stream.split();

        let (to_agent_tx, mut to_agent_rx) = mpsc::channel::<Message>(256);
        let (from_agent_tx, from_agent_rx) = mpsc::channel::<Message>(256);

        // Task: write messages to agent WebSocket
        tokio::spawn(async move {
            while let Some(msg) = to_agent_rx.recv().await {
                match serde_json::to_string(&msg) {
                    Ok(json) => {
                        use tokio_tungstenite::tungstenite::Message as WsMsg;
                        if ws_sink.send(WsMsg::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "failed to serialize message to agent");
                    }
                }
            }
        });

        // Task: read messages from agent WebSocket
        tokio::spawn(async move {
            while let Some(Ok(ws_msg)) = ws_stream_rx.next().await {
                use tokio_tungstenite::tungstenite::Message as WsMsg;
                match ws_msg {
                    WsMsg::Text(text) => {
                        match serde_json::from_str::<Message>(&text) {
                            Ok(msg) => {
                                if from_agent_tx.send(msg).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "invalid message from agent");
                            }
                        }
                    }
                    WsMsg::Close(_) => break,
                    _ => {}
                }
            }
            tracing::debug!("agent WebSocket reader ended");
        });

        Ok(Self {
            to_agent: to_agent_tx,
            from_agent: from_agent_rx,
        })
    }

    /// Connect to a remote tenodera-agent via SSH port-forwarding tunnel.
    /// Uses sshpass with the session password (Cockpit model).
    /// No API key needed because the agent listens on localhost only.
    pub async fn connect_via_ssh_tunnel(
        ssh_user: &str,
        password: &str,
        address: &str,
        ssh_port: u16,
        agent_port: u16,
    ) -> anyhow::Result<(Self, Child)> {
        // Bind to port 0 so the OS picks a free port, then release it for SSH.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let local_port = listener.local_addr()?.port();
        drop(listener);

        let forward_spec = format!("{local_port}:127.0.0.1:{agent_port}");
        tracing::info!(
            %ssh_user, %address, %ssh_port, %forward_spec,
            "opening SSH tunnel to agent"
        );

        let child = Command::new("sshpass")
            .env("SSHPASS", password)
            .args([
                "-e",
                "ssh",
                "-N",   // no remote command — tunnel only
                "-o", "StrictHostKeyChecking=accept-new",
                "-o", "ExitOnForwardFailure=yes",
                "-p", &ssh_port.to_string(),
                "-L", &forward_spec,
                &format!("{ssh_user}@{address}"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| anyhow::anyhow!("failed to spawn SSH tunnel: {e}"))?;

        // Wait for the tunnel to be ready by polling the local port.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if tokio::time::Instant::now() > deadline {
                anyhow::bail!(
                    "SSH tunnel to {address}:{agent_port} did not become ready in 10s"
                );
            }
            match tokio::net::TcpStream::connect(("127.0.0.1", local_port)).await {
                Ok(_) => break,
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
            }
        }

        tracing::info!(local_port, "SSH tunnel ready, connecting to agent");

        let conn = Self::connect("127.0.0.1", local_port, "", false).await?;
        Ok((conn, child))
    }
}
