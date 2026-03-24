use std::sync::Arc;

use axum::{Json, extract::State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::pam;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub user: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub session_id: String,
    pub user: String,
}

#[derive(Serialize)]
pub struct LoginError {
    pub error: String,
}

#[derive(Deserialize)]
pub struct LogoutRequest {
    pub session_id: String,
}

/// POST /api/auth/logout
///
/// Destroys the server-side session so credentials are no longer held in memory.
pub async fn logout(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LogoutRequest>,
) -> StatusCode {
    let user = state
        .sessions
        .get(&req.session_id)
        .await
        .map(|s| s.user.clone())
        .unwrap_or_default();
    state.sessions.remove(&req.session_id).await;
    crate::audit::log(&user, "logout", "", true, "");
    tracing::info!(session_id = %req.session_id, "session destroyed via logout");
    StatusCode::OK
}

/// POST /api/auth/login
///
/// Authenticates via PAM (unix_chkpwd). Creates session on success.
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<LoginError>)> {
    if req.user.is_empty() || req.password.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(LoginError {
                error: "user and password required".into(),
            }),
        ));
    }

    tracing::info!(user = %req.user, "login attempt");

    let result = pam::authenticate(&req.user, &req.password).await;

    if !result.success {
        tracing::warn!(user = %req.user, "authentication failed");
        crate::audit::log(&req.user, "login", "", false, "authentication failed");
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(LoginError {
                error: result.error.unwrap_or_else(|| "authentication failed".into()),
            }),
        ));
    }

    let session = state.sessions.create(req.user.clone(), req.password).await;
    crate::audit::log(&session.user, "login", "", true, "");

    Ok(Json(LoginResponse {
        session_id: session.id.clone(),
        user: session.user.clone(),
    }))
}
