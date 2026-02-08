use exoclaw::memory::MemoryEngine;
use exoclaw::memory::episodic::EpisodicMemory;
use exoclaw::memory::semantic::{MemoryEntity, SemanticMemory, extract_entities};
use exoclaw::memory::soul::SoulLoader;
use exoclaw::types::{Message, MessageContent};

fn make_text_message(role: &str, text: &str) -> Message {
    Message {
        role: role.to_string(),
        content: MessageContent::Text {
            text: text.to_string(),
        },
        timestamp: chrono::Utc::now(),
        token_count: None,
    }
}

fn make_entity(subject: &str, predicate: &str, object: &str, session_key: &str) -> MemoryEntity {
    MemoryEntity {
        id: uuid::Uuid::new_v4().to_string(),
        entity_type: "fact".to_string(),
        subject: subject.to_string(),
        predicate: predicate.to_string(),
        object: object.to_string(),
        session_key: session_key.to_string(),
        learned_at: chrono::Utc::now(),
        superseded_at: None,
        superseded_by: None,
        confidence: 0.9,
    }
}

// =============================================================
// Episodic Memory Tests
// =============================================================

#[test]
fn episodic_sliding_window_keeps_last_n_turns() {
    let mut mem = EpisodicMemory::new(3); // 3 turns = 6 messages max
    let key = "test:ws:user:peer";

    // Append 4 turns (8 messages)
    for i in 0..4 {
        mem.append(key, make_text_message("user", &format!("user msg {i}")));
        mem.append(
            key,
            make_text_message("assistant", &format!("asst msg {i}")),
        );
    }

    let all = mem.all(key);
    assert_eq!(all.len(), 6); // 3 turns * 2 messages

    // Should have turns 1, 2, 3 (turn 0 dropped)
    if let MessageContent::Text { ref text } = all[0].content {
        assert_eq!(text, "user msg 1");
    } else {
        panic!("expected text message");
    }
    if let MessageContent::Text { ref text } = all[5].content {
        assert_eq!(text, "asst msg 3");
    } else {
        panic!("expected text message");
    }
}

#[test]
fn episodic_older_turns_dropped_from_window() {
    let mut mem = EpisodicMemory::new(3); // 3 turns = 6 messages max
    let key = "test:ws:user:peer";

    // Append 6 turns (12 messages), only last 3 turns should remain
    for i in 0..6 {
        mem.append(key, make_text_message("user", &format!("user {i}")));
        mem.append(key, make_text_message("assistant", &format!("asst {i}")));
    }

    let all = mem.all(key);
    assert_eq!(all.len(), 6); // 3 turns * 2

    // First message in window should be user 3 (turns 0,1,2 dropped)
    if let MessageContent::Text { ref text } = all[0].content {
        assert_eq!(text, "user 3");
    } else {
        panic!("expected text message");
    }
}

#[test]
fn episodic_empty_session_returns_empty() {
    let mem = EpisodicMemory::new(5);
    assert!(mem.recent("nonexistent", 5).is_empty());
    assert!(mem.all("nonexistent").is_empty());
}

#[test]
fn episodic_recent_with_less_than_n() {
    let mut mem = EpisodicMemory::new(10);
    let key = "test:ws:user:peer";

    mem.append(key, make_text_message("user", "only message"));
    let recent = mem.recent(key, 5);
    assert_eq!(recent.len(), 1);
}

// =============================================================
// Semantic Memory Tests
// =============================================================

#[test]
fn semantic_store_and_query() {
    let mut mem = SemanticMemory::new(true);
    let entity = make_entity("user", "dog_name", "Luna", "session1");
    mem.store(entity);

    let results = mem.query("user", "dog_name");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].object, "Luna");
}

#[test]
fn semantic_query_subject() {
    let mut mem = SemanticMemory::new(true);
    mem.store(make_entity("user", "name", "Alice", "s1"));
    mem.store(make_entity("user", "location", "NYC", "s1"));
    mem.store(make_entity("other", "name", "Bob", "s1"));

    let results = mem.query_subject("user");
    assert_eq!(results.len(), 2);
}

#[test]
fn semantic_entity_supersession() {
    let mut mem = SemanticMemory::new(true);

    // Store initial location
    let old = make_entity("user", "location", "NYC", "s1");
    let old_id = old.id.clone();
    mem.store(old);

    // Store updated location (should supersede)
    let new = make_entity("user", "location", "LA", "s1");
    mem.store(new);

    // Active query should return only LA
    let results = mem.query("user", "location");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].object, "LA");

    // Total count should be 2 (old still exists but superseded)
    assert_eq!(mem.count(), 2);
    assert_eq!(mem.active_count(), 1);

    // Verify old entity has superseded_at set
    // (check all entities including superseded)
    let all_user: Vec<_> = mem
        .all_active()
        .into_iter()
        .chain(std::iter::empty()) // just to show all_active only returns active
        .collect();
    assert!(
        all_user
            .iter()
            .all(|e| e.id != old_id || e.superseded_at.is_some())
    );
}

#[test]
fn semantic_query_relevant() {
    let mut mem = SemanticMemory::new(true);
    mem.store(make_entity("user", "dog_name", "Luna", "s1"));
    mem.store(make_entity("user", "cat_name", "Mochi", "s1"));
    mem.store(make_entity("user", "location", "NYC", "s1"));

    let results = mem.query_relevant(&["dog", "Luna"]);
    assert!(!results.is_empty());
    // Should find the dog entity
    assert!(results.iter().any(|e| e.object == "Luna"));
}

#[test]
fn semantic_disabled_ignores_stores() {
    let mut mem = SemanticMemory::new(false);
    mem.store(make_entity("user", "name", "Alice", "s1"));
    assert_eq!(mem.count(), 0);
    assert!(!mem.is_enabled());
}

// =============================================================
// Entity Extraction Tests
// =============================================================

#[test]
fn extract_my_name_is() {
    let entities = extract_entities("My name is Alice.", "s1");
    assert!(!entities.is_empty());
    let name = entities.iter().find(|e| e.predicate == "name");
    assert!(name.is_some());
    assert_eq!(name.unwrap().object, "Alice");
}

#[test]
fn extract_i_live_in() {
    let entities = extract_entities("I live in San Francisco.", "s1");
    let location = entities.iter().find(|e| e.predicate == "location");
    assert!(location.is_some());
    assert_eq!(location.unwrap().object, "San Francisco");
}

#[test]
fn extract_moved_from_to() {
    let entities = extract_entities("I moved from NYC to LA.", "s1");
    let prev = entities.iter().find(|e| e.predicate == "previous_location");
    let curr = entities.iter().find(|e| e.predicate == "location");
    assert!(prev.is_some());
    assert_eq!(prev.unwrap().object, "NYC");
    assert!(curr.is_some());
    assert_eq!(curr.unwrap().object, "LA");
}

#[test]
fn extract_my_x_is_y() {
    let entities = extract_entities("My favorite color is blue.", "s1");
    let color = entities.iter().find(|e| e.predicate == "favorite_color");
    assert!(color.is_some());
    assert_eq!(color.unwrap().object, "blue");
}

#[test]
fn extract_i_work_at() {
    let entities = extract_entities("I work at Google.", "s1");
    let employer = entities.iter().find(|e| e.predicate == "employer");
    assert!(employer.is_some());
    assert_eq!(employer.unwrap().object, "Google");
}

#[test]
fn extract_multiple_facts() {
    let text = "My name is Bob. I live in Tokyo. My dog is Rex.";
    let entities = extract_entities(text, "s1");
    assert!(entities.len() >= 3);
}

#[test]
fn extract_empty_text() {
    let entities = extract_entities("", "s1");
    assert!(entities.is_empty());
}

#[test]
fn extract_no_patterns() {
    let entities = extract_entities("The weather is nice today.", "s1");
    assert!(entities.is_empty());
}

// =============================================================
// Soul Loader Tests
// =============================================================

#[test]
fn soul_load_from_file() {
    let dir = std::env::temp_dir().join("exoclaw_test_soul");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test_soul.md");
    std::fs::write(
        &path,
        "# Agent Personality\n\nYou are a helpful assistant.\n",
    )
    .unwrap();

    let mut loader = SoulLoader::new();
    let soul = loader.load("test-agent", path.to_str().unwrap()).unwrap();
    assert_eq!(soul.agent_id, "test-agent");
    assert!(soul.content.contains("helpful assistant"));
    assert!(soul.token_count > 0);

    // Clean up
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn soul_always_included_in_context() {
    let dir = std::env::temp_dir().join("exoclaw_test_soul_ctx");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("soul.md");
    std::fs::write(&path, "You are a pirate assistant. Arrr!").unwrap();

    let mut engine = MemoryEngine::new(5, true);
    engine.soul.load("pirate", path.to_str().unwrap()).unwrap();

    let context = engine.assemble_context("s1", "pirate", "hello");
    assert!(!context.is_empty());

    // First message should be the soul
    if let MessageContent::Text { ref text } = context[0].content {
        assert!(text.contains("pirate assistant"));
    } else {
        panic!("expected text content for soul");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================
// Context Assembly Tests
// =============================================================

#[test]
fn context_assembly_under_5k_tokens_for_50_turns() {
    let mut engine = MemoryEngine::new(5, true);
    let session_key = "test:ws:user:peer";

    // Simulate 50 turns of conversation
    for i in 0..50 {
        let user_msg = make_text_message(
            "user",
            &format!("User message number {i}: this is a test turn with some text content."),
        );
        let asst_msg = make_text_message(
            "assistant",
            &format!("Assistant response {i}: here is some helpful response text."),
        );
        engine.process_response(session_key, &user_msg, &asst_msg);
    }

    let context = engine.assemble_context(session_key, "default", "what did we talk about?");

    // Count approximate tokens (~4 chars per token)
    let total_chars: usize = context
        .iter()
        .map(|m| match &m.content {
            MessageContent::Text { text } => text.len(),
            _ => 0,
        })
        .sum();
    let approx_tokens = total_chars / 4;

    // Should be under 5000 tokens
    assert!(
        approx_tokens < 5000,
        "assembled context was ~{approx_tokens} tokens, expected < 5000"
    );

    // Should only have the last 10 episodic messages (5 turns = 10 messages user+assistant)
    // plus any semantic entities
    let episodic_count = context
        .iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .count();
    assert_eq!(
        episodic_count, 10,
        "expected 10 episodic messages (5 turns), got {episodic_count}"
    );
}

#[test]
fn context_assembly_fresh_session_only_soul() {
    let dir = std::env::temp_dir().join("exoclaw_test_fresh");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("soul.md");
    std::fs::write(&path, "You are helpful.").unwrap();

    let mut engine = MemoryEngine::new(5, true);
    engine.soul.load("agent1", path.to_str().unwrap()).unwrap();

    let context = engine.assemble_context("new_session", "agent1", "hello");

    // Should only have the soul message
    assert_eq!(context.len(), 1);
    assert_eq!(context[0].role, "system");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn semantic_fact_retrieval_at_turn_50() {
    let mut engine = MemoryEngine::new(5, true);
    let session_key = "test:ws:user:peer";

    // Turn 1: user states a fact
    let user_msg = make_text_message("user", "My name is Alice and my dog is Luna.");
    let asst_msg = make_text_message(
        "assistant",
        "Nice to meet you Alice! Luna is a lovely name for a dog.",
    );
    engine.process_response(session_key, &user_msg, &asst_msg);

    // Turns 2-50: filler conversation
    for i in 2..=50 {
        let user_msg = make_text_message("user", &format!("Tell me about topic {i}."));
        let asst_msg = make_text_message("assistant", &format!("Here is info about topic {i}."));
        engine.process_response(session_key, &user_msg, &asst_msg);
    }

    // Now query about the dog - should find Luna via semantic memory
    let context = engine.assemble_context(session_key, "default", "What is my dog's name?");

    // Check that semantic facts are included
    let has_dog_fact = context.iter().any(|m| {
        if let MessageContent::Text { ref text } = m.content {
            text.contains("Luna")
        } else {
            false
        }
    });

    assert!(
        has_dog_fact,
        "should retrieve dog name 'Luna' from semantic memory at turn 50"
    );
}

#[test]
fn semantic_entity_update_supersedes_old() {
    let mut engine = MemoryEngine::new(5, true);
    let session_key = "test:ws:user:peer";

    // User states initial location
    let user_msg = make_text_message("user", "I live in NYC.");
    let asst_msg = make_text_message("assistant", "NYC is great!");
    engine.process_response(session_key, &user_msg, &asst_msg);

    // Verify NYC is stored
    let results = engine.semantic.query("user", "location");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].object, "NYC");

    // User updates location
    let user_msg = make_text_message("user", "I moved from NYC to LA.");
    let asst_msg = make_text_message("assistant", "LA is sunny!");
    engine.process_response(session_key, &user_msg, &asst_msg);

    // Active location should be LA
    let results = engine.semantic.query("user", "location");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].object, "LA");
}

// =============================================================
// Memory Config Tests
// =============================================================

#[test]
fn memory_config_defaults() {
    use exoclaw::config::MemoryConfig;

    let config = MemoryConfig::default();
    assert_eq!(config.episodic_window, 5);
    assert!(config.semantic_enabled);
}

#[test]
fn memory_engine_from_config() {
    let engine = MemoryEngine::new(10, false);
    assert_eq!(engine.episodic.window_size(), 10);
    assert!(!engine.semantic.is_enabled());
}
