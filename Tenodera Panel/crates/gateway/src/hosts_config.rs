use std::path::PathBuf;

use serde::Deserialize;

/// How to connect to a host's management service.
#[derive(Debug, Clone, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// Connect via SSH tunnel to localhost tenodera-agent.
    #[default]
    Ssh,
    /// Direct WebSocket to tenodera-agent (requires API key).
    Agent,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HostEntry {
    pub id: String,
    pub address: String,
    /// SSH user override. Empty or missing → use the logged-in session user.
    #[serde(default)]
    pub user: String,
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    /// Transport method to use for this host.
    #[serde(default)]
    pub transport: Transport,
    /// Agent WebSocket port (default: 9091).
    #[serde(default = "default_agent_port")]
    pub agent_port: u16,
    /// API key for authenticating with the agent.
    #[serde(default)]
    pub api_key: String,
    /// Use TLS when connecting to the agent (default: true).
    #[serde(default = "default_agent_tls")]
    pub agent_tls: bool,
}

impl HostEntry {
    /// Effective SSH user: host-level override if set, otherwise the session user.
    pub fn effective_user<'a>(&'a self, session_user: &'a str) -> &'a str {
        if self.user.is_empty() { session_user } else { &self.user }
    }
}

fn default_ssh_port() -> u16 {
    22
}

fn default_agent_port() -> u16 {
    9091
}

fn default_agent_tls() -> bool {
    false
}

#[derive(Debug, Deserialize, Default)]
pub struct HostsConfig {
    pub hosts: Vec<HostEntry>,
}

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    PathBuf::from(home).join(".config/tenodera/hosts.json")
}

pub fn find_host(host_id: &str) -> Option<HostEntry> {
    let config: HostsConfig = std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    config.hosts.into_iter().find(|h| h.id == host_id)
}
