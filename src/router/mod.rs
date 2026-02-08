use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Hierarchical session router — same model as OpenClaw but in ~80 lines.
///
/// Binding priority: peer > guild > team > account > channel > default
pub struct SessionRouter {
    bindings: Vec<Binding>,
    sessions: HashMap<String, SessionState>,
    default_agent: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Binding {
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
}

struct SessionState {
    message_count: u64,
}

pub struct RouteResult {
    pub agent_id: String,
    pub session_key: String,
    pub matched_by: &'static str,
}

impl Default for SessionRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionRouter {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
            sessions: HashMap::new(),
            default_agent: "default".into(),
        }
    }

    pub fn add_binding(&mut self, binding: Binding) {
        self.bindings.push(binding);
    }

    pub fn resolve(
        &mut self,
        channel: &str,
        account: &str,
        peer: Option<&str>,
        guild: Option<&str>,
        team: Option<&str>,
    ) -> RouteResult {
        // Priority: peer > guild > team > account > channel > default
        let (agent_id, matched_by) = self
            .bindings
            .iter()
            .find_map(|b| {
                if let (Some(bp), Some(p)) = (&b.peer_id, peer) {
                    if bp == p {
                        return Some((&b.agent_id, "binding.peer"));
                    }
                }
                None
            })
            .or_else(|| {
                self.bindings.iter().find_map(|b| {
                    if let (Some(bg), Some(g)) = (&b.guild_id, guild) {
                        if bg == g {
                            return Some((&b.agent_id, "binding.guild"));
                        }
                    }
                    None
                })
            })
            .or_else(|| {
                self.bindings.iter().find_map(|b| {
                    if let (Some(bt), Some(t)) = (&b.team_id, team) {
                        if bt == t {
                            return Some((&b.agent_id, "binding.team"));
                        }
                    }
                    None
                })
            })
            .or_else(|| {
                self.bindings.iter().find_map(|b| {
                    if b.account_id.as_deref() == Some(account)
                        && b.peer_id.is_none()
                        && b.guild_id.is_none()
                    {
                        return Some((&b.agent_id, "binding.account"));
                    }
                    None
                })
            })
            .or_else(|| {
                self.bindings.iter().find_map(|b| {
                    if b.channel.as_deref() == Some(channel)
                        && b.account_id.is_none()
                        && b.peer_id.is_none()
                    {
                        return Some((&b.agent_id, "binding.channel"));
                    }
                    None
                })
            })
            .unwrap_or((&self.default_agent, "default"));

        let session_key = format!("{agent_id}:{channel}:{account}:{}", peer.unwrap_or("main"));

        self.sessions
            .entry(session_key.clone())
            .and_modify(|s| s.message_count += 1)
            .or_insert(SessionState { message_count: 1 });

        RouteResult {
            agent_id: agent_id.clone(),
            session_key,
            matched_by,
        }
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}
