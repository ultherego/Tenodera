use std::path::PathBuf;

use tenodera_protocol::channel::ChannelOpenOptions;
use tenodera_protocol::message::Message;
use serde_json::{json, Value};

use crate::handler::ChannelHandler;

// ── Types ───────────────────────────────────────────────────────

#[derive(serde::Serialize, serde::Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Transport {
    #[default]
    Ssh,
    Agent,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct HostEntry {
    id: String,
    name: String,
    address: String,
    /// SSH user override. Empty = use the logged-in session user.
    #[serde(default)]
    user: String,
    #[serde(default = "default_ssh_port")]
    ssh_port: u16,
    added_at: String,
    /// Transport method: "agent" (default) or "ssh" (legacy).
    #[serde(default)]
    transport: Transport,
    /// Agent WebSocket port (default: 9091).
    #[serde(default = "default_agent_port")]
    agent_port: u16,
    /// API key for authenticating with the agent.
    #[serde(default)]
    api_key: String,
    /// Use TLS when connecting to the agent (default: true).
    #[serde(default = "default_agent_tls")]
    agent_tls: bool,
    // Keep port for backward compat during migration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
}

fn default_ssh_port() -> u16 {
    22
}

fn default_agent_port() -> u16 {
    9091
}

fn default_agent_tls() -> bool {
    true
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct HostsConfig {
    hosts: Vec<HostEntry>,
}

// ── Config persistence ──────────────────────────────────────────

fn config_path() -> PathBuf {
    PathBuf::from("/etc/tenodera/hosts.json")
}

fn load_config() -> HostsConfig {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_config(config: &HostsConfig) -> Result<(), String> {
    let path = config_path();
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

// ── Handler ─────────────────────────────────────────────────────

pub struct HostsManageHandler;

#[async_trait::async_trait]
impl ChannelHandler for HostsManageHandler {
    fn payload_type(&self) -> &str {
        "hosts.manage"
    }

    async fn open(&self, channel: &str, _options: &ChannelOpenOptions) -> Vec<Message> {
        vec![Message::Ready {
            channel: channel.to_string(),
        }]
    }

    async fn data(&self, channel: &str, data: &Value) -> Vec<Message> {
        let action = data.get("action").and_then(|a| a.as_str()).unwrap_or("");
        let user = data.get("_user").and_then(|u| u.as_str()).unwrap_or("");

        let result = match action {
            "list" => action_list(),
            "add" => {
                let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let address = data.get("address").and_then(|v| v.as_str()).unwrap_or("");
                let user_field = data.get("user").and_then(|v| v.as_str()).unwrap_or("");
                let ssh_port = data
                    .get("ssh_port")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(22) as u16;
                let transport = parse_transport(data.get("transport").and_then(|v| v.as_str()).unwrap_or("ssh"));
                let agent_port = data.get("agent_port").and_then(|v| v.as_u64()).unwrap_or(9091) as u16;
                let api_key = data.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
                let agent_tls = data.get("agent_tls").and_then(|v| v.as_bool()).unwrap_or(false);
                let r = action_add(name, address, user_field, ssh_port, transport, agent_port, api_key, agent_tls);
                let ok = r.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                crate::audit::log(user, "host.add", address, ok, name);
                r
            }
            "edit" => {
                let id = data.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let address = data.get("address").and_then(|v| v.as_str()).unwrap_or("");
                let user_field = data.get("user").and_then(|v| v.as_str()).unwrap_or("");
                let ssh_port = data
                    .get("ssh_port")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(22) as u16;
                let transport = parse_transport(data.get("transport").and_then(|v| v.as_str()).unwrap_or("ssh"));
                let agent_port = data.get("agent_port").and_then(|v| v.as_u64()).unwrap_or(9091) as u16;
                let api_key = data.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
                let agent_tls = data.get("agent_tls").and_then(|v| v.as_bool()).unwrap_or(false);
                let r = action_edit(id, name, address, user_field, ssh_port, transport, agent_port, api_key, agent_tls);
                let ok = r.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                crate::audit::log(user, "host.edit", address, ok, name);
                r
            }
            "remove" => {
                let id = data.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let r = action_remove(id);
                let ok = r.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                crate::audit::log(user, "host.remove", id, ok, "");
                r
            }
            _ => json!({ "action": action, "error": "unknown action" }),
        };

        vec![Message::Data {
            channel: channel.to_string(),
            data: result,
        }]
    }
}

// ── Action implementations ──────────────────────────────────────

fn parse_transport(s: &str) -> Transport {
    match s {
        "agent" => Transport::Agent,
        _ => Transport::Ssh,
    }
}

fn action_list() -> Value {
    let config = load_config();
    json!({ "action": "list", "hosts": config.hosts })
}

fn action_add(name: &str, address: &str, user: &str, ssh_port: u16, transport: Transport, agent_port: u16, api_key: &str, agent_tls: bool) -> Value {
    if name.is_empty() || address.is_empty() {
        return json!({ "action": "add", "ok": false, "error": "name and address are required" });
    }

    let mut config = load_config();
    let entry = HostEntry {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        address: address.to_string(),
        user: user.to_string(),
        ssh_port,
        added_at: chrono::Utc::now().to_rfc3339(),
        transport,
        agent_port,
        api_key: api_key.to_string(),
        agent_tls,
        port: None,
    };
    let id = entry.id.clone();
    config.hosts.push(entry);

    match save_config(&config) {
        Ok(()) => json!({ "action": "add", "ok": true, "id": id }),
        Err(e) => json!({ "action": "add", "ok": false, "error": e }),
    }
}

fn action_edit(id: &str, name: &str, address: &str, user: &str, ssh_port: u16, transport: Transport, agent_port: u16, api_key: &str, agent_tls: bool) -> Value {
    if id.is_empty() || name.is_empty() || address.is_empty() {
        return json!({ "action": "edit", "ok": false, "error": "id, name and address are required" });
    }

    let mut config = load_config();
    let Some(entry) = config.hosts.iter_mut().find(|h| h.id == id) else {
        return json!({ "action": "edit", "ok": false, "error": "host not found" });
    };

    entry.name = name.to_string();
    entry.address = address.to_string();
    entry.user = user.to_string();
    entry.ssh_port = ssh_port;
    entry.transport = transport;
    entry.agent_port = agent_port;
    entry.api_key = api_key.to_string();
    entry.agent_tls = agent_tls;

    match save_config(&config) {
        Ok(()) => json!({ "action": "edit", "ok": true }),
        Err(e) => json!({ "action": "edit", "ok": false, "error": e }),
    }
}

fn action_remove(id: &str) -> Value {
    let mut config = load_config();
    let before = config.hosts.len();
    config.hosts.retain(|h| h.id != id);

    if config.hosts.len() == before {
        return json!({ "action": "remove", "ok": false, "error": "host not found" });
    }

    match save_config(&config) {
        Ok(()) => json!({ "action": "remove", "ok": true }),
        Err(e) => json!({ "action": "remove", "ok": false, "error": e }),
    }
}
