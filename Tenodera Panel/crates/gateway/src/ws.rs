use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Query, State, WebSocketUpgrade, ws::{Message, WebSocket}},
    http::StatusCode,
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::{mpsc, Mutex};

use tenodera_protocol::message;

use crate::AppState;
use crate::bridge_transport::{AgentConnection, BridgeProcess};
use crate::hosts_config::{self, Transport};

#[derive(Deserialize)]
pub struct WsParams {
    pub session_id: Option<String>,
}

/// GET /api/ws?session_id=… — upgrade to WebSocket for channel transport.
/// Requires a valid session. Returns 401 if session is missing or invalid.
pub async fn ws_upgrade(
    State(state): State<Arc<AppState>>,
    Query(params): Query<WsParams>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, StatusCode> {
    let session_id = params.session_id.ok_or(StatusCode::UNAUTHORIZED)?;
    let session = state
        .sessions
        .get(&session_id)
        .await
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Check idle timeout
    let elapsed = session.created_at.elapsed().as_secs();
    if elapsed > state.config.idle_timeout_secs {
        state.sessions.remove(&session_id).await;
        return Err(StatusCode::UNAUTHORIZED);
    }

    let user = session.user.clone();
    let password = session.password.clone();
    tracing::info!(user = %user, "WS upgrade authorized");

    Ok(ws.on_upgrade(move |socket| handle_socket(state, socket, user, password)))
}

/// Spawn a task forwarding messages from a bridge to the WebSocket sink.
fn spawn_bridge_forwarder(
    label: String,
    mut from_bridge: mpsc::Receiver<message::Message>,
    sink: Arc<Mutex<futures::stream::SplitSink<WebSocket, Message>>>,
) {
    tokio::spawn(async move {
        while let Some(msg) = from_bridge.recv().await {
            match serde_json::to_string(&msg) {
                Ok(json) => {
                    let mut s = sink.lock().await;
                    if s.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, bridge = %label, "failed to serialize bridge msg");
                }
            }
        }
        tracing::debug!(bridge = %label, "bridge→ws forwarder ended");
    });
}

async fn handle_socket(state: Arc<AppState>, socket: WebSocket, user: String, password: String) {
    let (sink, mut stream) = socket.split();

    tracing::debug!(user = %user, "new WebSocket connection");

    // Try to spawn local bridge subprocess
    let bridge_bin = &state.config.bridge_bin;
    let local_bridge = match BridgeProcess::spawn(bridge_bin).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, bin = %bridge_bin, "failed to spawn bridge");
            let err_msg = message::Message::AuthResult {
                success: false,
                problem: Some(format!("bridge-spawn-failed: {e}")),
                user: None,
            };
            let mut sink = sink;
            let _ = sink
                .send(Message::Text(serde_json::to_string(&err_msg).unwrap().into()))
                .await;
            return;
        }
    };

    tracing::info!(bin = %bridge_bin, "local bridge spawned");

    // Decompose local bridge
    let BridgeProcess { child: local_child, to_bridge: local_sender, from_bridge: local_receiver } = local_bridge;

    // Keep all bridge child processes alive for the session
    let mut _children = vec![local_child];

    // Shared WebSocket sink
    let sink = Arc::new(Mutex::new(sink));

    // Forward local bridge → WebSocket
    spawn_bridge_forwarder("local".into(), local_receiver, sink.clone());

    // Channel → bridge sender routing table
    let mut channel_routes: HashMap<String, mpsc::Sender<message::Message>> = HashMap::new();

    // Remote bridge/agent senders keyed by host ID
    let mut remote_senders: HashMap<String, mpsc::Sender<message::Message>> = HashMap::new();

    // Main loop: route WebSocket client messages to the correct bridge
    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            Message::Text(text) => {
                match serde_json::from_str::<message::Message>(&text) {
                    Ok(message::Message::Ping) => {
                        let pong = serde_json::to_string(&message::Message::Pong)
                            .expect("pong serialization");
                        let mut s = sink.lock().await;
                        let _ = s.send(Message::Text(pong.into())).await;
                    }
                    Ok(message::Message::Open { channel, options }) => {
                        let host_id = options.extra.get("host")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());

                        let target_sender = if let Some(ref hid) = host_id {
                            // Route to remote bridge/agent
                            if let Some(sender) = remote_senders.get(hid) {
                                sender.clone()
                            } else {
                                // Connect to remote host
                                match connect_remote(hid, &user, &password).await {
                                    Ok((sender, from_remote, child)) => {
                                        if let Some(c) = child {
                                            _children.push(c);
                                        }
                                        spawn_bridge_forwarder(
                                            format!("remote:{hid}"),
                                            from_remote,
                                            sink.clone(),
                                        );
                                        remote_senders.insert(hid.clone(), sender.clone());
                                        sender
                                    }
                                    Err(e) => {
                                        tracing::error!(host = %hid, error = %e, "remote connect failed");
                                        let close = message::Message::Close {
                                            channel: channel.clone(),
                                            problem: Some(format!("connect-failed: {e}")),
                                        };
                                        let json = serde_json::to_string(&close).unwrap();
                                        let mut s = sink.lock().await;
                                        let _ = s.send(Message::Text(json.into())).await;
                                        continue;
                                    }
                                }
                            }
                        } else {
                            // Route to local bridge
                            local_sender.clone()
                        };

                        // Store channel → bridge mapping
                        channel_routes.insert(channel.clone(), target_sender.clone());

                        // Inject authenticated user and strip host field before forwarding
                        let open_msg = {
                            let mut clean = options.clone();
                            clean.extra.remove("host");
                            clean.extra.insert(
                                "_user".into(),
                                serde_json::Value::String(user.clone()),
                            );
                            message::Message::Open { channel, options: clean }
                        };

                        if target_sender.send(open_msg).await.is_err() {
                            tracing::warn!("bridge channel closed on Open");
                        }
                    }
                    Ok(parsed) => {
                        // Data, Close, Control — route to owning bridge
                        let ch = match &parsed {
                            message::Message::Data { channel, .. } => Some(channel.clone()),
                            message::Message::Close { channel, .. } => Some(channel.clone()),
                            message::Message::Control { channel, .. } => Some(channel.clone()),
                            _ => None,
                        };

                        let is_close = matches!(&parsed, message::Message::Close { .. });

                        if let Some(ch) = ch {
                            let sender = channel_routes
                                .get(&ch)
                                .cloned()
                                .unwrap_or_else(|| local_sender.clone());

                            if sender.send(parsed).await.is_err() {
                                tracing::warn!(channel = %ch, "bridge closed");
                            }

                            if is_close {
                                channel_routes.remove(&ch);
                            }
                        } else {
                            // Forward unknown messages to local bridge
                            let _ = local_sender.send(parsed).await;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, raw = %text, "invalid message from client");
                    }
                }
            }
            Message::Close(_) => {
                tracing::debug!("WebSocket closed by client");
                break;
            }
            _ => {}
        }
    }

    // Clean up — dropping senders and children kills bridge processes
    drop(local_sender);
    drop(remote_senders);
    drop(channel_routes);
    tracing::debug!("WebSocket connection ended");
}

/// Connect to a remote host — either via agent WebSocket or legacy SSH bridge.
/// Returns a unified (sender, receiver) pair regardless of transport,
/// plus an optional child process to keep alive (SSH bridge only).
async fn connect_remote(
    host_id: &str,
    session_user: &str,
    session_password: &str,
) -> anyhow::Result<(
    mpsc::Sender<message::Message>,
    mpsc::Receiver<message::Message>,
    Option<tokio::process::Child>,
)> {
    let host = hosts_config::find_host(host_id)
        .ok_or_else(|| anyhow::anyhow!("host not found: {host_id}"))?;

    match host.transport {
        Transport::Agent => {
            tracing::info!(
                host = %host_id,
                address = %host.address,
                port = host.agent_port,
                "connecting to agent via WebSocket"
            );
            let conn = AgentConnection::connect(
                &host.address,
                host.agent_port,
                &host.api_key,
                host.agent_tls,
            ).await?;
            Ok((conn.to_agent, conn.from_agent, None))
        }
        Transport::Ssh => {
            let ssh_user = host.effective_user(session_user);
            tracing::info!(
                host = %host_id,
                address = %host.address,
                ssh_user = %ssh_user,
                "connecting to agent via SSH tunnel"
            );
            let (conn, child) = AgentConnection::connect_via_ssh_tunnel(
                ssh_user,
                session_password,
                &host.address,
                host.ssh_port,
                host.agent_port,
            ).await?;
            Ok((conn.to_agent, conn.from_agent, Some(child)))
        }
    }
}
