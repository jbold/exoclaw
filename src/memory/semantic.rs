use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

/// A fact, relationship, or attribute extracted from conversation.
/// Stored in the semantic memory layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntity {
    pub id: String,
    pub entity_type: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub session_key: String,
    pub learned_at: DateTime<Utc>,
    pub superseded_at: Option<DateTime<Utc>>,
    pub superseded_by: Option<String>,
    pub confidence: f32,
}

/// In-memory semantic memory store. Stores entities indexed by subject.
pub struct SemanticMemory {
    entities: HashMap<String, Vec<MemoryEntity>>,
    enabled: bool,
}

impl SemanticMemory {
    pub fn new(enabled: bool) -> Self {
        Self {
            entities: HashMap::new(),
            enabled,
        }
    }

    /// Whether semantic memory is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Store a new entity. If an entity with the same subject+predicate
    /// already exists and is active (not superseded), supersede it.
    pub fn store(&mut self, entity: MemoryEntity) {
        if !self.enabled {
            return;
        }

        // Check for existing active entity with same subject+predicate
        let existing_id = self.find_active(&entity.subject, &entity.predicate);

        if let Some(old_id) = existing_id {
            self.supersede(&old_id, &entity.id);
        }

        let subject = entity.subject.clone();
        self.entities.entry(subject).or_default().push(entity);
    }

    /// Query entities by subject and predicate. Returns only active (not superseded) entities.
    pub fn query(&self, subject: &str, predicate: &str) -> Vec<&MemoryEntity> {
        self.entities
            .get(subject)
            .map(|entities| {
                entities
                    .iter()
                    .filter(|e| e.predicate == predicate && e.superseded_at.is_none())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Query all active entities for a subject.
    pub fn query_subject(&self, subject: &str) -> Vec<&MemoryEntity> {
        self.entities
            .get(subject)
            .map(|entities| {
                entities
                    .iter()
                    .filter(|e| e.superseded_at.is_none())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Query entities matching any of the given keywords in subject, predicate, or object.
    /// Returns only active entities, sorted by relevance (number of keyword matches).
    pub fn query_relevant(&self, keywords: &[&str]) -> Vec<&MemoryEntity> {
        let mut results: Vec<(&MemoryEntity, usize)> = Vec::new();

        for entities in self.entities.values() {
            for entity in entities {
                if entity.superseded_at.is_some() {
                    continue;
                }
                let score = keywords
                    .iter()
                    .filter(|kw| {
                        let kw_lower = kw.to_lowercase();
                        let subj = entity.subject.to_lowercase();
                        let pred = entity.predicate.to_lowercase();
                        let obj = entity.object.to_lowercase();
                        // Bidirectional substring: keyword in field OR field in keyword
                        subj.contains(&kw_lower)
                            || kw_lower.contains(&subj)
                            || pred.contains(&kw_lower)
                            || kw_lower.contains(&pred)
                            || obj.contains(&kw_lower)
                            || kw_lower.contains(&obj)
                    })
                    .count();

                if score > 0 {
                    results.push((entity, score));
                }
            }
        }

        // Sort by score descending
        results.sort_by(|a, b| b.1.cmp(&a.1));
        results.into_iter().map(|(e, _)| e).collect()
    }

    /// Mark an entity as superseded by another.
    fn supersede(&mut self, old_id: &str, new_id: &str) {
        let now = Utc::now();
        for entities in self.entities.values_mut() {
            for entity in entities.iter_mut() {
                if entity.id == old_id {
                    entity.superseded_at = Some(now);
                    entity.superseded_by = Some(new_id.to_string());
                    debug!(old_id, new_id, "superseded entity");
                    return;
                }
            }
        }
    }

    /// Find the ID of an active entity with the given subject+predicate.
    fn find_active(&self, subject: &str, predicate: &str) -> Option<String> {
        self.entities.get(subject).and_then(|entities| {
            entities
                .iter()
                .find(|e| e.predicate == predicate && e.superseded_at.is_none())
                .map(|e| e.id.clone())
        })
    }

    /// Get all active entities across all subjects.
    pub fn all_active(&self) -> Vec<&MemoryEntity> {
        self.entities
            .values()
            .flat_map(|entities| entities.iter().filter(|e| e.superseded_at.is_none()))
            .collect()
    }

    /// Get total entity count (including superseded).
    pub fn count(&self) -> usize {
        self.entities.values().map(|v| v.len()).sum()
    }

    /// Get active entity count.
    pub fn active_count(&self) -> usize {
        self.all_active().len()
    }
}

/// Extract entities from a text response using simple pattern matching.
///
/// Patterns recognized:
/// - "my name is X" / "I'm X" / "I am X"
/// - "I live in X" / "I'm from X" / "I am from X"
/// - "my X is Y" (e.g., "my dog is Luna", "my favorite color is blue")
/// - "I moved from X to Y" / "I moved to X"
/// - "I work at X" / "I work for X"
pub fn extract_entities(text: &str, session_key: &str) -> Vec<MemoryEntity> {
    let mut entities = Vec::new();
    let now = Utc::now();

    // Normalize: work on each sentence
    for sentence in text.split(['.', '!', '?']) {
        let sentence = sentence.trim();
        if sentence.is_empty() {
            continue;
        }
        let lower = sentence.to_lowercase();

        // "my name is X" / "I'm X" / "I am X" (as introduction)
        if let Some(name) = extract_after_pattern(&lower, sentence, "my name is ") {
            entities.push(make_entity("user", "name", &name, session_key, now, 0.9));
        }

        // "I live in X"
        if let Some(place) = extract_after_pattern(&lower, sentence, "i live in ") {
            entities.push(make_entity(
                "user",
                "location",
                &place,
                session_key,
                now,
                0.85,
            ));
        }

        // "I'm from X" / "I am from X"
        if let Some(place) = extract_after_pattern(&lower, sentence, "i'm from ")
            .or_else(|| extract_after_pattern(&lower, sentence, "i am from "))
        {
            entities.push(make_entity("user", "from", &place, session_key, now, 0.85));
        }

        // "I moved to X" / "I moved from X to Y"
        if lower.contains("i moved") {
            if let Some(caps) = extract_moved_pattern(&lower, sentence) {
                if let Some(ref from) = caps.0 {
                    entities.push(make_entity(
                        "user",
                        "previous_location",
                        from,
                        session_key,
                        now,
                        0.8,
                    ));
                }
                entities.push(make_entity(
                    "user",
                    "location",
                    &caps.1,
                    session_key,
                    now,
                    0.85,
                ));
            }
        }

        // "I work at X" / "I work for X"
        if let Some(company) = extract_after_pattern(&lower, sentence, "i work at ")
            .or_else(|| extract_after_pattern(&lower, sentence, "i work for "))
        {
            entities.push(make_entity(
                "user",
                "employer",
                &company,
                session_key,
                now,
                0.85,
            ));
        }

        // "my X is Y" (generic possessive pattern) - find all occurrences
        for (predicate, object) in extract_all_my_x_is_y(&lower, sentence) {
            // Skip if already handled by a more specific pattern
            if predicate != "name" {
                entities.push(make_entity(
                    "user",
                    &predicate,
                    &object,
                    session_key,
                    now,
                    0.75,
                ));
            }
        }
    }

    entities
}

/// Extract text after a pattern, using the original case from the sentence.
fn extract_after_pattern(lower: &str, original: &str, pattern: &str) -> Option<String> {
    if let Some(pos) = lower.find(pattern) {
        let start = pos + pattern.len();
        let value = original[start..].trim();
        // Take until end of clause or common delimiters
        let value = value
            .split([',', ';', '(', ')'])
            .next()
            .unwrap_or(value)
            .trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

/// Extract "I moved from X to Y" or "I moved to X" patterns.
fn extract_moved_pattern(lower: &str, original: &str) -> Option<(Option<String>, String)> {
    // "I moved from X to Y"
    if let Some(from_pos) = lower.find("i moved from ") {
        let after_from = from_pos + "i moved from ".len();
        let rest = &original[after_from..];
        let rest_lower = &lower[after_from..];
        if let Some(to_pos) = rest_lower.find(" to ") {
            let from = rest[..to_pos].trim().to_string();
            let to = rest[to_pos + 4..].trim();
            let to = to
                .split([',', ';', '(', ')'])
                .next()
                .unwrap_or(to)
                .trim()
                .to_string();
            if !to.is_empty() {
                return Some((Some(from), to));
            }
        }
    }

    // "I moved to X"
    if let Some(to_pos) = lower.find("i moved to ") {
        let start = to_pos + "i moved to ".len();
        let value = original[start..].trim();
        let value = value
            .split([',', ';', '(', ')'])
            .next()
            .unwrap_or(value)
            .trim()
            .to_string();
        if !value.is_empty() {
            return Some((None, value));
        }
    }

    None
}

/// Extract all "my X is Y" patterns from a sentence.
fn extract_all_my_x_is_y(lower: &str, original: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let mut search_start = 0;

    while search_start < lower.len() {
        if let Some(my_pos) = lower[search_start..].find("my ") {
            let abs_pos = search_start + my_pos;
            let after_my = abs_pos + 3;
            let rest = &original[after_my..];
            let rest_lower = &lower[after_my..];

            if let Some(is_pos) = rest_lower.find(" is ") {
                let predicate = rest[..is_pos].trim();
                let object_start = is_pos + 4;
                let remaining = rest[object_start..].trim();
                // Take until "and", comma, semicolon, or clause boundary
                let object = remaining
                    .split([',', ';', '(', ')'])
                    .next()
                    .unwrap_or(remaining)
                    .trim();
                // Also split on " and " to handle "my X is Y and my Z is W"
                let object = object.split(" and ").next().unwrap_or(object).trim();

                if !predicate.is_empty() && !object.is_empty() {
                    let predicate = predicate.to_lowercase().replace(' ', "_");
                    results.push((predicate, object.to_string()));
                }

                search_start = after_my + object_start;
            } else {
                search_start = after_my;
            }
        } else {
            break;
        }
    }

    results
}

fn make_entity(
    subject: &str,
    predicate: &str,
    object: &str,
    session_key: &str,
    learned_at: DateTime<Utc>,
    confidence: f32,
) -> MemoryEntity {
    MemoryEntity {
        id: uuid::Uuid::new_v4().to_string(),
        entity_type: "fact".to_string(),
        subject: subject.to_string(),
        predicate: predicate.to_string(),
        object: object.to_string(),
        session_key: session_key.to_string(),
        learned_at,
        superseded_at: None,
        superseded_by: None,
        confidence,
    }
}
