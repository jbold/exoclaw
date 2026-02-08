use exoclaw::store::SessionStore;
use serde_json::json;

#[test]
fn new_and_default_start_empty() {
    let store = SessionStore::new();
    assert_eq!(store.count(), 0);

    let default_store = SessionStore::default();
    assert_eq!(default_store.count(), 0);
}

#[test]
fn get_or_create_reuses_existing_session() {
    let mut store = SessionStore::new();
    let created_at = {
        let session = store.get_or_create("web:acct:peer", "agent-a");
        assert_eq!(session.key, "web:acct:peer");
        assert_eq!(session.agent_id, "agent-a");
        assert_eq!(session.message_count, 0);
        session.created_at
    };

    let session = store.get_or_create("web:acct:peer", "agent-b");
    assert_eq!(session.agent_id, "agent-a");
    assert_eq!(session.created_at, created_at);
    assert_eq!(store.count(), 1);
}

#[test]
fn append_message_tracks_count_and_ignores_missing_session() {
    let mut store = SessionStore::new();
    store.append_message("missing", json!({"role":"user","content":"ignored"}));
    assert!(store.get("missing").is_none());

    store.get_or_create("web:acct:peer", "agent-a");
    store.append_message("web:acct:peer", json!({"role":"user","content":"hello"}));
    store.append_message(
        "web:acct:peer",
        json!({"role":"assistant","content":"world"}),
    );

    let session = store.get("web:acct:peer").expect("session should exist");
    assert_eq!(session.message_count, 2);
    assert_eq!(session.messages.len(), 2);
}

#[test]
fn get_mut_and_sessions_mut_allow_updates() {
    let mut store = SessionStore::new();
    store.get_or_create("web:acct:peer", "agent-a");

    if let Some(session) = store.get_mut("web:acct:peer") {
        session.agent_id = "agent-b".to_string();
    }

    let clone_for_other_key = store.get("web:acct:peer").expect("session").clone();
    store
        .sessions_mut()
        .insert("other:key".to_string(), clone_for_other_key);

    assert_eq!(store.count(), 2);
    assert_eq!(
        store.get("web:acct:peer").expect("session").agent_id,
        "agent-b"
    );
}
