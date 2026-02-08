use crate::types::Message;
use std::collections::HashMap;

/// Sliding-window episodic memory. Keeps the last N turns per session.
/// A "turn" is a user+assistant message pair (2 messages).
///
/// Older turns roll off the window but remain in the session store
/// for semantic extraction.
pub struct EpisodicMemory {
    /// Number of turns to keep (1 turn = 2 messages: user + assistant).
    window_turns: usize,
    sessions: HashMap<String, Vec<Message>>,
}

impl EpisodicMemory {
    /// Create a new episodic memory with the given window size in turns.
    /// Default window is 5 turns (~1-2K tokens, 10 messages).
    pub fn new(window_turns: usize) -> Self {
        Self {
            window_turns,
            sessions: HashMap::new(),
        }
    }

    /// Append a message to the session's episodic window.
    pub fn append(&mut self, session_key: &str, message: Message) {
        let max_messages = self.window_turns * 2;
        let messages = self.sessions.entry(session_key.to_string()).or_default();
        messages.push(message);
        // Trim to window size (keep most recent)
        if messages.len() > max_messages {
            let drain_count = messages.len() - max_messages;
            messages.drain(..drain_count);
        }
    }

    /// Get the most recent N messages for a session.
    pub fn recent(&self, session_key: &str, n: usize) -> Vec<Message> {
        match self.sessions.get(session_key) {
            Some(messages) => {
                let count = n.min(messages.len());
                messages[messages.len() - count..].to_vec()
            }
            None => Vec::new(),
        }
    }

    /// Get all messages currently in the window for a session.
    pub fn all(&self, session_key: &str) -> Vec<Message> {
        self.sessions.get(session_key).cloned().unwrap_or_default()
    }

    /// Get the configured window size in turns.
    pub fn window_size(&self) -> usize {
        self.window_turns
    }
}
