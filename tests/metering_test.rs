use exoclaw::agent::metering::{BudgetScope, TokenCounter, estimate_cost, estimate_input_tokens};
use exoclaw::config::BudgetConfig;

// --- T028: Token metering unit tests ---

#[test]
fn token_counter_allows_under_session_budget() {
    let budget = BudgetConfig {
        session: Some(10000),
        daily: None,
        monthly: None,
    };
    let mut counter = TokenCounter::new(&budget);
    let session = "agent:ws:user:peer";

    // Record some usage
    counter.record_usage(
        session,
        "default",
        "anthropic",
        "claude-sonnet-4-5-20250929",
        500,
        200,
    );

    // Should be under budget
    let result = counter.check_budget(session, 100);
    assert!(result.is_ok());
}

#[test]
fn token_counter_refuses_over_session_budget() {
    let budget = BudgetConfig {
        session: Some(1000),
        daily: None,
        monthly: None,
    };
    let mut counter = TokenCounter::new(&budget);
    let session = "agent:ws:user:peer";

    // Record usage near the limit
    counter.record_usage(
        session,
        "default",
        "anthropic",
        "claude-sonnet-4-5-20250929",
        500,
        400,
    );

    // 900 used + 200 estimated = 1100 > 1000 limit
    let result = counter.check_budget(session, 200);
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert_eq!(err.used, 900);
    assert_eq!(err.limit, 1000);
    match &err.scope {
        BudgetScope::Session(key) => assert_eq!(key, session),
        _ => panic!("expected session scope"),
    }
}

#[test]
fn token_counter_refuses_over_daily_budget() {
    let budget = BudgetConfig {
        session: None,
        daily: Some(5000),
        monthly: None,
    };
    let mut counter = TokenCounter::new(&budget);

    // Record usage across different sessions
    counter.record_usage(
        "s1",
        "default",
        "anthropic",
        "claude-sonnet-4-5-20250929",
        1000,
        1000,
    );
    counter.record_usage(
        "s2",
        "default",
        "anthropic",
        "claude-sonnet-4-5-20250929",
        1000,
        1000,
    );

    // 4000 used + 1500 estimated = 5500 > 5000 limit
    let result = counter.check_budget("s3", 1500);
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert_eq!(err.used, 4000);
    assert_eq!(err.limit, 5000);
    assert!(matches!(err.scope, BudgetScope::Daily));
}

#[test]
fn token_counter_refuses_over_monthly_budget() {
    let budget = BudgetConfig {
        session: None,
        daily: None,
        monthly: Some(10000),
    };
    let mut counter = TokenCounter::new(&budget);

    counter.record_usage("s1", "default", "openai", "gpt-4o", 3000, 2000);
    counter.record_usage("s2", "default", "openai", "gpt-4o", 3000, 2000);

    // 10000 used + 1 estimated = 10001 > 10000 limit
    let result = counter.check_budget("s3", 1);
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert_eq!(err.used, 10000);
    assert_eq!(err.limit, 10000);
    assert!(matches!(err.scope, BudgetScope::Monthly));
}

#[test]
fn no_budget_allows_unlimited() {
    let budget = BudgetConfig {
        session: None,
        daily: None,
        monthly: None,
    };
    let mut counter = TokenCounter::new(&budget);

    // Record large usage
    counter.record_usage(
        "s1",
        "default",
        "anthropic",
        "claude-sonnet-4-5-20250929",
        50000,
        50000,
    );

    // Should always pass with no limits
    let result = counter.check_budget("s1", 100000);
    assert!(result.is_ok());
}

#[test]
fn usage_accumulates_per_session() {
    let budget = BudgetConfig {
        session: Some(10000),
        daily: None,
        monthly: None,
    };
    let mut counter = TokenCounter::new(&budget);
    let session = "agent:ws:user:peer";

    counter.record_usage(
        session,
        "default",
        "anthropic",
        "claude-sonnet-4-5-20250929",
        100,
        100,
    );
    counter.record_usage(
        session,
        "default",
        "anthropic",
        "claude-sonnet-4-5-20250929",
        200,
        200,
    );
    counter.record_usage(
        session,
        "default",
        "anthropic",
        "claude-sonnet-4-5-20250929",
        300,
        300,
    );

    let usage = counter.get_usage(&BudgetScope::Session(session.to_string()));
    // 100+100 + 200+200 + 300+300 = 1200
    assert_eq!(usage.total_tokens, 1200);
    assert_eq!(usage.input_tokens, 600);
    assert_eq!(usage.output_tokens, 600);
}

#[test]
fn token_record_logged_per_call() {
    let budget = BudgetConfig::default();
    let mut counter = TokenCounter::new(&budget);

    counter.record_usage(
        "s1",
        "agent1",
        "anthropic",
        "claude-sonnet-4-5-20250929",
        500,
        200,
    );
    counter.record_usage("s2", "agent2", "openai", "gpt-4o", 1000, 500);

    let records = counter.records();
    assert_eq!(records.len(), 2);

    assert_eq!(records[0].session_key, "s1");
    assert_eq!(records[0].agent_id, "agent1");
    assert_eq!(records[0].provider, "anthropic");
    assert_eq!(records[0].model, "claude-sonnet-4-5-20250929");
    assert_eq!(records[0].input_tokens, 500);
    assert_eq!(records[0].output_tokens, 200);

    assert_eq!(records[1].session_key, "s2");
    assert_eq!(records[1].agent_id, "agent2");
    assert_eq!(records[1].provider, "openai");
    assert_eq!(records[1].model, "gpt-4o");
    assert_eq!(records[1].input_tokens, 1000);
    assert_eq!(records[1].output_tokens, 500);
}

#[test]
fn cost_estimation_anthropic_sonnet() {
    // Anthropic Sonnet: input=$3/MTok, output=$15/MTok
    // 1000 input: 1000/1M * $3 = $0.003
    // 500 output: 500/1M * $15 = $0.0075
    // Total: $0.0105
    let cost = estimate_cost("anthropic", "claude-sonnet-4-5-20250929", 1000, 500);
    assert!((cost - 0.0105).abs() < 1e-9);
}

#[test]
fn cost_estimation_openai_gpt4o() {
    // OpenAI GPT-4o: input=$2.50/MTok, output=$10/MTok
    // 1000 input: 1000/1M * $2.50 = $0.0025
    // 500 output: 500/1M * $10 = $0.005
    // Total: $0.0075
    let cost = estimate_cost("openai", "gpt-4o", 1000, 500);
    assert!((cost - 0.0075).abs() < 1e-9);
}

#[test]
fn cost_recorded_in_token_record() {
    let budget = BudgetConfig::default();
    let mut counter = TokenCounter::new(&budget);

    counter.record_usage(
        "s1",
        "default",
        "anthropic",
        "claude-sonnet-4-5-20250929",
        1000,
        500,
    );

    let records = counter.records();
    assert_eq!(records.len(), 1);
    assert!((records[0].cost_estimate_usd - 0.0105).abs() < 1e-9);
}

#[test]
fn input_token_estimation_heuristic() {
    let messages = vec![serde_json::json!({"role": "user", "content": "Hello world"})];
    let estimate = estimate_input_tokens(&messages);
    // "Hello world" = 11 chars, 11/4 + 1 = 3
    assert_eq!(estimate, 3);
}

#[test]
fn budget_exceeded_display_format() {
    let budget = BudgetConfig {
        session: Some(100),
        daily: None,
        monthly: None,
    };
    let mut counter = TokenCounter::new(&budget);
    counter.record_usage(
        "s1",
        "default",
        "anthropic",
        "claude-sonnet-4-5-20250929",
        50,
        50,
    );

    let err = counter.check_budget("s1", 50).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("token budget exceeded"));
    assert!(msg.contains("session:s1"));
    assert!(msg.contains("100/100"));
}

#[test]
fn session_budget_independent_between_sessions() {
    let budget = BudgetConfig {
        session: Some(1000),
        daily: None,
        monthly: None,
    };
    let mut counter = TokenCounter::new(&budget);

    // Fill session 1 near limit
    counter.record_usage(
        "s1",
        "default",
        "anthropic",
        "claude-sonnet-4-5-20250929",
        400,
        500,
    );

    // Session 2 should still have full budget
    let result = counter.check_budget("s2", 500);
    assert!(result.is_ok());

    // Session 1 should be near limit
    let result = counter.check_budget("s1", 200);
    assert!(result.is_err());
}

#[test]
fn multiple_budget_scopes_checked() {
    // Session limit high, daily limit low
    let budget = BudgetConfig {
        session: Some(100000),
        daily: Some(500),
        monthly: None,
    };
    let mut counter = TokenCounter::new(&budget);

    counter.record_usage(
        "s1",
        "default",
        "anthropic",
        "claude-sonnet-4-5-20250929",
        200,
        200,
    );

    // Within session budget but over daily budget
    let result = counter.check_budget("s1", 200);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err.scope, BudgetScope::Daily));
}

#[test]
fn zero_token_call_allowed() {
    let budget = BudgetConfig {
        session: Some(100),
        daily: None,
        monthly: None,
    };
    let mut counter = TokenCounter::new(&budget);
    counter.record_usage(
        "s1",
        "default",
        "anthropic",
        "claude-sonnet-4-5-20250929",
        50,
        49,
    );

    // 99 used + 0 estimated = within budget
    let result = counter.check_budget("s1", 0);
    assert!(result.is_ok());
}
