use serde::{Deserialize, Serialize};

use crate::protocol::channel::{ChannelId, ChannelOpenOptions};

/// Top-level envelope for all messages on the WebSocket transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Message {
    Open {
        channel: ChannelId,
        #[serde(flatten)]
        options: ChannelOpenOptions,
    },
    Ready {
        channel: ChannelId,
    },
    Data {
        channel: ChannelId,
        data: serde_json::Value,
    },
    Control {
        channel: ChannelId,
        command: String,
        #[serde(flatten)]
        extra: serde_json::Map<String, serde_json::Value>,
    },
    Close {
        channel: ChannelId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        problem: Option<String>,
    },
    Ping,
    Pong,
}
