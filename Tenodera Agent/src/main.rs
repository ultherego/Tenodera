pub mod audit;
mod auth;
mod config;
mod handler;
mod handlers;
mod protocol;
mod router;
mod server;
mod tls;

use std::sync::Arc;

use axum::{Router, middleware};
use tokio::net::TcpListener;

use crate::auth::ApiKey;
use crate::config::AgentConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = AgentConfig::load()?;

    if config.api_key.is_empty() {
        tracing::warn!("no API key configured — all connections will be accepted (dev mode)");
    }

    let state = Arc::new(server::AgentState {});

    let app = Router::new()
        .route("/ws", axum::routing::get(server::ws_upgrade))
        .route("/health", axum::routing::get(server::health))
        .layer(middleware::from_fn(auth::auth_middleware))
        .layer(axum::Extension(ApiKey(config.api_key.clone())))
        .with_state(state);

    let listener = TcpListener::bind(config.bind_addr).await?;
    tracing::info!(addr = %config.bind_addr, "tenodera-agent listening");

    let tls_acceptor = tls::build_acceptor(&config)?;

    match tls_acceptor {
        Some(acceptor) => {
            tracing::info!("TLS enabled");
            tls::serve_tls(listener, acceptor, app).await?;
        }
        None => {
            if !config.allow_unencrypted {
                anyhow::bail!(
                    "TLS not configured and allow_unencrypted=false — refusing to start. \
                     Set tls_cert/tls_key or allow_unencrypted=true."
                );
            }
            tracing::warn!("TLS not configured — running unencrypted (not recommended for production)");
            axum::serve(listener, app).await?;
        }
    }

    Ok(())
}
