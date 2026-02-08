use exoclaw::router::{Binding, SessionRouter};

fn make_binding(
    agent_id: &str,
    channel: Option<&str>,
    account_id: Option<&str>,
    peer_id: Option<&str>,
    guild_id: Option<&str>,
    team_id: Option<&str>,
) -> Binding {
    Binding {
        agent_id: agent_id.to_string(),
        channel: channel.map(String::from),
        account_id: account_id.map(String::from),
        peer_id: peer_id.map(String::from),
        guild_id: guild_id.map(String::from),
        team_id: team_id.map(String::from),
    }
}

#[test]
fn peer_binding_highest_priority() {
    let mut router = SessionRouter::new();
    router.add_binding(make_binding(
        "channel-agent",
        Some("telegram"),
        None,
        None,
        None,
        None,
    ));
    router.add_binding(make_binding(
        "peer-agent",
        None,
        None,
        Some("user-42"),
        None,
        None,
    ));

    let result = router.resolve("telegram", "acct", Some("user-42"), None, None);
    assert_eq!(result.agent_id, "peer-agent");
    assert_eq!(result.matched_by, "binding.peer");
}

#[test]
fn guild_binding_before_channel() {
    let mut router = SessionRouter::new();
    router.add_binding(make_binding(
        "channel-agent",
        Some("discord"),
        None,
        None,
        None,
        None,
    ));
    router.add_binding(make_binding(
        "guild-agent",
        None,
        None,
        None,
        Some("server-1"),
        None,
    ));

    let result = router.resolve("discord", "acct", None, Some("server-1"), None);
    assert_eq!(result.agent_id, "guild-agent");
    assert_eq!(result.matched_by, "binding.guild");
}

#[test]
fn team_binding_before_account() {
    let mut router = SessionRouter::new();
    router.add_binding(make_binding(
        "account-agent",
        None,
        Some("acct1"),
        None,
        None,
        None,
    ));
    router.add_binding(make_binding(
        "team-agent",
        None,
        None,
        None,
        None,
        Some("team-a"),
    ));

    let result = router.resolve("slack", "acct1", None, None, Some("team-a"));
    assert_eq!(result.agent_id, "team-agent");
    assert_eq!(result.matched_by, "binding.team");
}

#[test]
fn account_binding_before_channel() {
    let mut router = SessionRouter::new();
    router.add_binding(make_binding(
        "channel-agent",
        Some("telegram"),
        None,
        None,
        None,
        None,
    ));
    router.add_binding(make_binding(
        "account-agent",
        None,
        Some("user-1"),
        None,
        None,
        None,
    ));

    let result = router.resolve("telegram", "user-1", None, None, None);
    assert_eq!(result.agent_id, "account-agent");
    assert_eq!(result.matched_by, "binding.account");
}

#[test]
fn channel_binding_before_default() {
    let mut router = SessionRouter::new();
    router.add_binding(make_binding(
        "ws-agent",
        Some("websocket"),
        None,
        None,
        None,
        None,
    ));

    let result = router.resolve("websocket", "me", None, None, None);
    assert_eq!(result.agent_id, "ws-agent");
    assert_eq!(result.matched_by, "binding.channel");
}

#[test]
fn default_agent_fallback() {
    let router = &mut SessionRouter::new();
    let result = router.resolve("unknown", "anon", None, None, None);
    assert_eq!(result.agent_id, "default");
    assert_eq!(result.matched_by, "default");
}

#[test]
fn session_key_format() {
    let mut router = SessionRouter::new();
    let result = router.resolve("telegram", "user1", Some("peer1"), None, None);
    assert_eq!(result.session_key, "default:telegram:user1:peer1");
}

#[test]
fn session_key_default_peer() {
    let mut router = SessionRouter::new();
    let result = router.resolve("websocket", "me", None, None, None);
    assert_eq!(result.session_key, "default:websocket:me:main");
}

#[test]
fn session_creation_on_first_message() {
    let mut router = SessionRouter::new();
    assert_eq!(router.session_count(), 0);

    router.resolve("ws", "me", None, None, None);
    assert_eq!(router.session_count(), 1);
}

#[test]
fn session_reuse_on_subsequent_messages() {
    let mut router = SessionRouter::new();
    router.resolve("ws", "me", None, None, None);
    router.resolve("ws", "me", None, None, None);
    assert_eq!(router.session_count(), 1);
}

#[test]
fn different_peers_create_different_sessions() {
    let mut router = SessionRouter::new();
    router.resolve("ws", "me", Some("peer1"), None, None);
    router.resolve("ws", "me", Some("peer2"), None, None);
    assert_eq!(router.session_count(), 2);
}
