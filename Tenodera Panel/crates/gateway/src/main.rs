mod audit;
mod auth;
mod bridge_transport;
mod config;
mod hosts_config;
mod pam;
mod session;
mod tls;
mod ws;

use std::sync::Arc;

use axum::{
    Router,
    routing::get,
};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::config::GatewayConfig;
use crate::session::SessionStore;

/// Shared application state passed to all handlers.
pub struct AppState {
    pub config: GatewayConfig,
    pub sessions: SessionStore,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("tenodera_gateway=debug".parse()?))
        .init();

    let config = GatewayConfig::default();
    let bind_addr = config.bind_addr;

    let state = Arc::new(AppState {
        config,
        sessions: SessionStore::new(),
    });

    // Build TLS acceptor before moving state into router
    let allow_unencrypted = state.config.allow_unencrypted;
    let tls_acceptor = tls::build_acceptor(&state.config)?;

    let app = Router::new()
        .route("/api/auth/login", axum::routing::post(auth::login))
        .route("/api/auth/logout", axum::routing::post(auth::logout))
        .route("/api/ws", get(ws::ws_upgrade))
        .route("/api/health", get(health))
        // Serve built frontend from ui/dist (production)
        .fallback_service(
            tower_http::services::ServeDir::new(
                std::env::var("TENODERA_UI_DIR").unwrap_or_else(|_| "ui/dist".to_string()),
            )
            .fallback(tower_http::services::ServeFile::new(
                format!(
                    "{}/index.html",
                    std::env::var("TENODERA_UI_DIR").unwrap_or_else(|_| "ui/dist".to_string())
                ),
            )),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    // Check if TLS is configured
    match tls_acceptor {
        Some(acceptor) => {
            tracing::info!("tenodera-gateway listening on {} (TLS)", bind_addr);
            tls::serve_tls(listener, acceptor, app).await?;
        }
        None => {
            if !allow_unencrypted {
                anyhow::bail!("TLS not configured and TENODERA_ALLOW_UNENCRYPTED is not set. \
                    Set TENODERA_TLS_CERT and TENODERA_TLS_KEY, or set TENODERA_ALLOW_UNENCRYPTED=1 for dev.");
            }
            tracing::info!("tenodera-gateway listening on {} (plaintext)", bind_addr);
            axum::serve(listener, app).await?;
        }
    }

    Ok(())
}

async fn health() -> &'static str {
    "ok"
}
