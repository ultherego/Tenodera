use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

/// A live user session.
///
/// Stores the user's password for the lifetime of the session so that the
/// gateway can open SSH tunnels to remote hosts on behalf of the user
/// (same model as Cockpit).
#[derive(Clone)]
pub struct Session {
    pub id: String,
    pub user: String,
    pub password: String,
    pub created_at: std::time::Instant,
    pub last_activity: std::time::Instant,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("id", &self.id)
            .field("user", &self.user)
            .field("password", &"***")
            .field("created_at", &self.created_at)
            .field("last_activity", &self.last_activity)
            .finish()
    }
}

/// Thread-safe in-memory session store.
#[derive(Debug, Clone)]
pub struct SessionStore {
    inner: Arc<RwLock<HashMap<String, Session>>>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create(&self, user: String, password: String) -> Session {
        let now = std::time::Instant::now();
        let session = Session {
            id: Uuid::new_v4().to_string(),
            user,
            password,
            created_at: now,
            last_activity: now,
        };
        self.inner.write().await.insert(session.id.clone(), session.clone());
        session
    }

    pub async fn get(&self, id: &str) -> Option<Session> {
        self.inner.read().await.get(id).cloned()
    }

    /// Update last_activity timestamp for the given session.
    pub async fn touch(&self, id: &str) {
        if let Some(session) = self.inner.write().await.get_mut(id) {
            session.last_activity = std::time::Instant::now();
        }
    }

    pub async fn remove(&self, id: &str) {
        self.inner.write().await.remove(id);
    }
}
