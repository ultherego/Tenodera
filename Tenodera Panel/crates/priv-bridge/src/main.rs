use std::io::{self, BufRead, Write};

use tenodera_protocol::message::Message;
use tracing_subscriber::EnvFilter;

/// Privileged bridge — runs as root, accepts only a whitelisted
/// set of operations via stdin/stdout JSON protocol.
///
/// Security boundary: this process MUST validate every request
/// against a strict allowlist before executing.
fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("tenodera_priv_bridge=debug".parse()?))
        .with_writer(io::stderr)
        .init();

    tracing::info!(uid = nix::unistd::getuid().as_raw(), "tenodera-priv-bridge started");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }

        let msg: Message = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = %e, "invalid message");
                continue;
            }
        };

        let responses = handle_privileged(msg);

        for resp in responses {
            let json = serde_json::to_string(&resp)?;
            writeln!(stdout, "{json}")?;
            stdout.flush()?;
        }
    }

    Ok(())
}

/// Allowlist-based dispatch for privileged operations.
fn handle_privileged(msg: Message) -> Vec<Message> {
    match msg {
        Message::Open { channel, options } => {
            let allowed = matches!(
                options.payload.as_str(),
                "systemd.unit.action" | "package.updates"
            );

            if !allowed {
                tracing::warn!(payload = %options.payload, "denied: payload not in allowlist");
                return vec![Message::Close {
                    channel,
                    problem: Some("access-denied".into()),
                }];
            }

            // TODO: dispatch to privileged handlers
            tracing::info!(payload = %options.payload, "privileged op (stub)");
            vec![
                Message::Ready { channel: channel.clone() },
                Message::Close { channel, problem: None },
            ]
        }
        Message::Ping => vec![Message::Pong],
        _ => vec![],
    }
}
