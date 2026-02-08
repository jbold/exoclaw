pub mod episodic;
pub mod semantic;
pub mod soul;

use crate::types::{Message, MessageContent};
use episodic::EpisodicMemory;
use semantic::{SemanticMemory, extract_entities};
use soul::SoulLoader;

/// Coordinates all three memory layers: soul, semantic, and episodic.
///
/// Context assembly order:
/// 1. Soul document (always first, ~500 tokens)
/// 2. Relevant semantic entities matching the query
/// 3. Recent episodic turns (sliding window)
///
/// Target assembled context: 3-5K tokens total.
pub struct MemoryEngine {
    pub episodic: EpisodicMemory,
    pub semantic: SemanticMemory,
    pub soul: SoulLoader,
}

impl MemoryEngine {
    /// Create a new memory engine.
    ///
    /// - `episodic_window`: number of recent turns to keep (default 5)
    /// - `semantic_enabled`: whether to extract and store entities
    pub fn new(episodic_window: usize, semantic_enabled: bool) -> Self {
        Self {
            episodic: EpisodicMemory::new(episodic_window),
            semantic: SemanticMemory::new(semantic_enabled),
            soul: SoulLoader::new(),
        }
    }

    /// Assemble context for an LLM call.
    ///
    /// Returns a Vec<Message> containing:
    /// 1. Soul document as a system message (if loaded)
    /// 2. Semantic entities relevant to the query as a system message
    /// 3. Recent episodic turns
    pub fn assemble_context(
        &mut self,
        session_key: &str,
        agent_id: &str,
        query: &str,
    ) -> Vec<Message> {
        let mut context = Vec::new();

        // 1. Soul document (always first)
        if let Some(soul_content) = self.soul.get_content(agent_id) {
            context.push(Message {
                role: "system".to_string(),
                content: MessageContent::Text { text: soul_content },
                timestamp: chrono::Utc::now(),
                token_count: None,
            });
        }

        // 2. Semantic entities relevant to the query
        if self.semantic.is_enabled() {
            let cleaned: Vec<String> = query
                .split_whitespace()
                .map(|w| {
                    w.chars()
                        .filter(|c| c.is_alphanumeric())
                        .collect::<String>()
                        .to_lowercase()
                })
                .filter(|w| w.len() > 2) // Skip short words
                .collect();
            let keywords: Vec<&str> = cleaned.iter().map(|s| s.as_str()).collect();

            if !keywords.is_empty() {
                let relevant = self.semantic.query_relevant(&keywords);
                if !relevant.is_empty() {
                    let facts: Vec<String> = relevant
                        .iter()
                        .take(10) // Limit to 10 most relevant facts
                        .map(|e| format!("{}'s {}: {}", e.subject, e.predicate, e.object))
                        .collect();

                    let facts_text = format!("Known facts:\n{}", facts.join("\n"));

                    context.push(Message {
                        role: "system".to_string(),
                        content: MessageContent::Text { text: facts_text },
                        timestamp: chrono::Utc::now(),
                        token_count: None,
                    });
                }
            }
        }

        // 3. Recent episodic turns
        let recent = self.episodic.all(session_key);
        context.extend(recent);

        context
    }

    /// Process a response: extract entities and append messages to episodic memory.
    pub fn process_response(
        &mut self,
        session_key: &str,
        user_message: &Message,
        assistant_message: &Message,
    ) {
        // Append both messages to episodic memory
        self.episodic.append(session_key, user_message.clone());
        self.episodic.append(session_key, assistant_message.clone());

        // Extract entities from the user message (user states facts about themselves)
        if let MessageContent::Text { ref text } = user_message.content {
            let entities = extract_entities(text, session_key);
            for entity in entities {
                self.semantic.store(entity);
            }
        }

        // Also extract from assistant message (assistant may restate/confirm facts)
        if let MessageContent::Text { ref text } = assistant_message.content {
            let entities = extract_entities(text, session_key);
            for entity in entities {
                self.semantic.store(entity);
            }
        }
    }

    /// Append a single message to episodic memory without entity extraction.
    pub fn append_to_episodic(&mut self, session_key: &str, message: Message) {
        self.episodic.append(session_key, message);
    }
}
