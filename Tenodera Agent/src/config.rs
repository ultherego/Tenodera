use std::net::SocketAddr;
use std::path::PathBuf;

use serde::Deserialize;

/// Agent configuration — loaded from TOML config file and/or environment variables.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub bind_addr: SocketAddr,
    pub api_key: String,
    pub tls_cert: Option<String>,
    pub tls_key: Option<String>,
    pub allow_unencrypted: bool,
}

/// TOML config file structure.
#[derive(Deserialize, Default)]
struct ConfigFile {
    #[serde(default = "default_bind")]
    bind: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    tls_cert: String,
    #[serde(default)]
    tls_key: String,
    #[serde(default)]
    allow_unencrypted: Option<bool>,
}

fn default_bind() -> String {
    "127.0.0.1:9091".to_string()
}

impl AgentConfig {
    /// Load config with priority: env vars > config file > defaults.
    pub fn load() -> anyhow::Result<Self> {
        let file_config = load_config_file();

        let bind_str = std::env::var("TENODERA_AGENT_BIND")
            .unwrap_or_else(|_| {
                if file_config.bind.is_empty() {
                    "127.0.0.1:9091".to_string()
                } else {
                    file_config.bind.clone()
                }
            });

        let bind_addr: SocketAddr = bind_str.parse()
            .map_err(|e| anyhow::anyhow!("invalid bind address '{bind_str}': {e}"))?;

        let api_key = std::env::var("TENODERA_AGENT_API_KEY")
            .unwrap_or_else(|_| file_config.api_key.clone());

        let tls_cert = std::env::var("TENODERA_AGENT_TLS_CERT").ok()
            .or_else(|| if file_config.tls_cert.is_empty() { None } else { Some(file_config.tls_cert.clone()) });

        let tls_key = std::env::var("TENODERA_AGENT_TLS_KEY").ok()
            .or_else(|| if file_config.tls_key.is_empty() { None } else { Some(file_config.tls_key.clone()) });

        let allow_unencrypted = std::env::var("TENODERA_AGENT_ALLOW_UNENCRYPTED")
            .map(|v| v == "1" || v == "true")
            .unwrap_or_else(|_| file_config.allow_unencrypted.unwrap_or(true));

        Ok(Self {
            bind_addr,
            api_key,
            tls_cert,
            tls_key,
            allow_unencrypted,
        })
    }
}

fn load_config_file() -> ConfigFile {
    let paths = [
        std::env::var("TENODERA_AGENT_CONFIG").ok().map(PathBuf::from),
        Some(PathBuf::from("/etc/tenodera/agent.toml")),
        dirs_config().map(|d| d.join("tenodera/agent.toml")),
    ];

    for path in paths.into_iter().flatten() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            match toml::from_str::<ConfigFile>(&content) {
                Ok(cfg) => {
                    tracing::info!(path = %path.display(), "loaded config file");
                    return cfg;
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "failed to parse config file");
                }
            }
        }
    }

    ConfigFile::default()
}

fn dirs_config() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config"))
}
