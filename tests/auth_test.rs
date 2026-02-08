use exoclaw::gateway::auth::verify_connect;

#[test]
fn valid_token_authenticates() {
    let expected = Some("my-secret-token".to_string());
    let msg = r#"{"token": "my-secret-token"}"#;
    assert!(verify_connect(msg, &expected));
}

#[test]
fn invalid_token_rejected() {
    let expected = Some("my-secret-token".to_string());
    let msg = r#"{"token": "wrong-token"}"#;
    assert!(!verify_connect(msg, &expected));
}

#[test]
fn no_token_configured_allows_all() {
    // Loopback mode: no token required
    let expected = None;
    let msg = r#"{"anything": "here"}"#;
    assert!(verify_connect(msg, &expected));
}

#[test]
fn malformed_json_rejected() {
    let expected = Some("secret".to_string());
    let msg = "this is not json";
    assert!(!verify_connect(msg, &expected));
}

#[test]
fn empty_token_string_rejected() {
    let expected = Some("my-secret".to_string());
    let msg = r#"{"token": ""}"#;
    assert!(!verify_connect(msg, &expected));
}

#[test]
fn missing_token_field_rejected() {
    let expected = Some("secret".to_string());
    let msg = r#"{"not_token": "secret"}"#;
    assert!(!verify_connect(msg, &expected));
}

#[test]
fn json_with_extra_fields_accepted() {
    let expected = Some("correct".to_string());
    let msg = r#"{"token": "correct", "extra": true}"#;
    assert!(verify_connect(msg, &expected));
}
