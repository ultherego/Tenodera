use crate::protocol::channel::ChannelOpenOptions;
use crate::protocol::message::Message;
use tokio::io::AsyncWriteExt;

use crate::handler::ChannelHandler;

pub struct SuperuserVerifyHandler;

#[async_trait::async_trait]
impl ChannelHandler for SuperuserVerifyHandler {
    fn payload_type(&self) -> &str {
        "superuser.verify"
    }

    async fn open(&self, channel: &str, options: &ChannelOpenOptions) -> Vec<Message> {
        let password = options
            .extra
            .get("password")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let result = if password.is_empty() {
            serde_json::json!({ "ok": false, "error": "password required" })
        } else {
            verify_sudo_password(password).await
        };

        vec![
            Message::Ready {
                channel: channel.to_string(),
            },
            Message::Data {
                channel: channel.to_string(),
                data: result,
            },
            Message::Close {
                channel: channel.to_string(),
                problem: None,
            },
        ]
    }
}

async fn verify_sudo_password(password: &str) -> serde_json::Value {
    let child = tokio::process::Command::new("sudo")
        .args(["-S", "-k", "true"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(e) => return serde_json::json!({ "ok": false, "error": e.to_string() }),
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(format!("{password}\n").as_bytes()).await;
        drop(stdin);
    }

    match child.wait_with_output().await {
        Ok(out) if out.status.success() => {
            serde_json::json!({ "ok": true })
        }
        Ok(_) => {
            serde_json::json!({ "ok": false, "error": "incorrect password" })
        }
        Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
    }
}
