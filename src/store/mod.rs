use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Simple in-memory session store. Will be backed by SurrealDB
/// once the core loop is proven.
///
/// Stores conversation history per session key.
pub struct SessionStore {
    sessions: HashMap<String, Session>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub key: String,
    pub agent_id: String,
    pub messages: Vec<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub message_count: u64,
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub fn get_or_create(&mut self, key: &str, agent_id: &str) -> &mut Session {
        self.sessions.entry(key.into()).or_insert_with(|| Session {
            key: key.into(),
            agent_id: agent_id.into(),
            messages: Vec::new(),
            created_at: chrono::Utc::now(),
            message_count: 0,
        })
    }

    pub fn append_message(&mut self, key: &str, message: serde_json::Value) {
        if let Some(session) = self.sessions.get_mut(key) {
            session.messages.push(message);
            session.message_count += 1;
        }
    }

    pub fn get(&self, key: &str) -> Option<&Session> {
        self.sessions.get(key)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Session> {
        self.sessions.get_mut(key)
    }

    pub fn count(&self) -> usize {
        self.sessions.len()
    }

    pub fn sessions_mut(&mut self) -> &mut HashMap<String, Session> {
        &mut self.sessions
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}
