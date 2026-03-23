use std::sync::Arc;

use axum::extract::{
    State, WebSocketUpgrade,
    ws::{Message, WebSocket},
};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::protocol::message;
use crate::router::Router;

/// Shared state for the agent server.
pub struct AgentState {
    // Reserved for future per-connection state.
}

/// GET /ws — upgrade to WebSocket. The panel connects here.
pub async fn ws_upgrade(
    State(_state): State<Arc<AgentState>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

/// GET /health — liveness probe.
pub async fn health() -> &'static str {
    "ok"
}

/// Handle a single WebSocket connection from the panel.
/// Each connection gets its own Router with all handlers registered.
async fn handle_socket(socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();

    tracing::info!("panel connected via WebSocket");

    // Channel for outgoing messages (handlers → WS)
    let (out_tx, mut out_rx) = mpsc::channel::<message::Message>(256);

    let mut router = Router::new(out_tx.clone());
    router.register_defaults();

    // Spawn: forward handler output → WebSocket
    let writer = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            match serde_json::to_string(&msg) {
                Ok(json) => {
                    if sink.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to serialize outgoing message");
                }
            }
        }
    });

    // Main loop: read from WebSocket, route through handlers
    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            Message::Text(text) => {
                let parsed = match serde_json::from_str::<message::Message>(&text) {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::warn!(error = %e, "invalid message from panel");
                        continue;
                    }
                };

                match parsed {
                    message::Message::Ping => {
                        let pong = serde_json::to_string(&message::Message::Pong).unwrap();
                        // Send pong directly through the out channel
                        let _ = out_tx.send(message::Message::Pong).await;
                        let _ = pong; // avoid unused warning
                    }
                    other => {
                        let responses = router.handle(other).await;
                        for resp in responses {
                            if out_tx.send(resp).await.is_err() {
                                tracing::error!("output channel closed");
                                break;
                            }
                        }
                    }
                }
            }
            Message::Close(_) => {
                tracing::debug!("WebSocket closed by panel");
                break;
            }
            _ => {}
        }
    }

    drop(out_tx);
    let _ = writer.await;
    tracing::info!("panel disconnected");
}
