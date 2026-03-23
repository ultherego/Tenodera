use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};

/// Middleware: validate the `Authorization: Bearer <api_key>` header
/// or `?api_key=…` query parameter against the configured key.
pub async fn auth_middleware(request: Request, next: Next) -> Result<Response, StatusCode> {
    let expected_key = request
        .extensions()
        .get::<ApiKey>()
        .map(|k| k.0.clone())
        .unwrap_or_default();

    // If no API key is configured, allow all connections (dev mode).
    if expected_key.is_empty() {
        return Ok(next.run(request).await);
    }

    // Check Authorization header first
    if let Some(auth) = request.headers().get("authorization") {
        if let Ok(val) = auth.to_str() {
            if let Some(token) = val.strip_prefix("Bearer ") {
                if constant_time_eq(token.as_bytes(), expected_key.as_bytes()) {
                    return Ok(next.run(request).await);
                }
            }
        }
    }

    // Check query parameter as fallback (for WebSocket upgrades)
    if let Some(query) = request.uri().query() {
        for pair in query.split('&') {
            if let Some(val) = pair.strip_prefix("api_key=") {
                if constant_time_eq(val.as_bytes(), expected_key.as_bytes()) {
                    return Ok(next.run(request).await);
                }
            }
        }
    }

    Err(StatusCode::UNAUTHORIZED)
}

/// Constant-time comparison to prevent timing attacks on API key.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Extension type to carry the API key into middleware.
#[derive(Clone)]
pub struct ApiKey(pub String);
