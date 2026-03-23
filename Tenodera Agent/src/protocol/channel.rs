use serde::{Deserialize, Serialize};

pub type ChannelId = String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelState {
    Opening,
    Ready,
    Closing,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelOpenOptions {
    pub payload: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superuser: Option<SuperuserMode>,

    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SuperuserMode {
    Require,
    Try,
}
