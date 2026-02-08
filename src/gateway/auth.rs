use subtle::ConstantTimeEq;

/// Verify the initial WebSocket connect message contains a valid token.
/// Returns true if no token is required (loopback) or if token matches.
pub fn verify_connect(msg: &str, expected: &Option<String>) -> bool {
    let expected = match expected {
        Some(t) => t,
        None => return true, // No auth required (loopback mode)
    };

    // Parse {"token": "..."} from connect message
    let token = match serde_json::from_str::<serde_json::Value>(msg) {
        Ok(v) => v.get("token").and_then(|t| t.as_str()).map(String::from),
        Err(_) => None,
    };

    match token {
        Some(ref t) => constant_time_eq(t.as_bytes(), expected.as_bytes()),
        None => false,
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}
