use chrono::{DateTime, Datelike, Utc};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, OnceLock};
use tracing::info;

use crate::config::BudgetConfig;

// --- T034: Global token counter, lazily initialized from config ---

static GLOBAL_COUNTER: OnceLock<Mutex<TokenCounter>> = OnceLock::new();

/// Initialize the global token counter from budget config.
/// Safe to call multiple times - only the first call takes effect.
pub fn init_global(budget: &BudgetConfig) {
    let _ = GLOBAL_COUNTER.set(Mutex::new(TokenCounter::new(budget)));
}

/// Get or initialize the global token counter.
/// Initializes with the provided budget config on first call.
pub fn get_or_init_global(budget: &BudgetConfig) -> &'static Mutex<TokenCounter> {
    GLOBAL_COUNTER.get_or_init(|| Mutex::new(TokenCounter::new(budget)))
}

// --- T029: Core data structures ---

/// Scope for a token budget.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum BudgetScope {
    Session(String),
    Daily,
    Monthly,
}

impl fmt::Display for BudgetScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BudgetScope::Session(key) => write!(f, "session:{key}"),
            BudgetScope::Daily => write!(f, "daily"),
            BudgetScope::Monthly => write!(f, "monthly"),
        }
    }
}

/// A token budget with limit, usage, and period tracking.
#[derive(Debug, Clone)]
pub struct TokenBudget {
    pub scope: BudgetScope,
    pub limit: u64,
    pub used: u64,
    pub period_start: DateTime<Utc>,
}

/// An audit log entry for a single LLM API call.
#[derive(Debug, Clone)]
pub struct TokenRecord {
    pub timestamp: DateTime<Utc>,
    pub session_key: String,
    pub agent_id: String,
    pub provider: String,
    pub model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cost_estimate_usd: f64,
}

/// Error returned when a budget would be exceeded.
#[derive(Debug, Clone)]
pub struct BudgetExceeded {
    pub scope: BudgetScope,
    pub used: u64,
    pub limit: u64,
}

impl fmt::Display for BudgetExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "token budget exceeded ({}: {}/{})",
            self.scope, self.used, self.limit
        )
    }
}

impl std::error::Error for BudgetExceeded {}

/// Token usage summary.
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost_estimate_usd: f64,
}

// --- T032: Cost estimation ---

/// Per-token prices (USD per million tokens).
struct PricingEntry {
    input_per_mtok: f64,
    output_per_mtok: f64,
}

/// Get pricing for a provider+model combination.
fn get_pricing(provider: &str, model: &str) -> PricingEntry {
    match (provider, model) {
        ("anthropic", m) if m.contains("sonnet") => PricingEntry {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
        },
        ("anthropic", m) if m.contains("haiku") => PricingEntry {
            input_per_mtok: 0.25,
            output_per_mtok: 1.25,
        },
        ("anthropic", m) if m.contains("opus") => PricingEntry {
            input_per_mtok: 15.0,
            output_per_mtok: 75.0,
        },
        ("openai", m) if m.contains("gpt-4o") => PricingEntry {
            input_per_mtok: 2.50,
            output_per_mtok: 10.0,
        },
        ("openai", m) if m.contains("gpt-4") => PricingEntry {
            input_per_mtok: 30.0,
            output_per_mtok: 60.0,
        },
        ("openai", m) if m.contains("gpt-3.5") => PricingEntry {
            input_per_mtok: 0.50,
            output_per_mtok: 1.50,
        },
        // Default fallback pricing
        _ => PricingEntry {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
        },
    }
}

/// Calculate cost estimate in USD.
pub fn estimate_cost(provider: &str, model: &str, input_tokens: u32, output_tokens: u32) -> f64 {
    let pricing = get_pricing(provider, model);
    let input_cost = (input_tokens as f64 / 1_000_000.0) * pricing.input_per_mtok;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * pricing.output_per_mtok;
    input_cost + output_cost
}

// --- T029/T030/T031: TokenCounter ---

/// Tracks cumulative token usage and enforces budgets.
pub struct TokenCounter {
    /// Budget limits from config (None = unlimited).
    session_limit: Option<u64>,
    daily_limit: Option<u64>,
    monthly_limit: Option<u64>,

    /// Per-session usage tracking.
    session_usage: HashMap<String, u64>,

    /// Daily usage tracking.
    daily_used: u64,
    daily_start: DateTime<Utc>,

    /// Monthly usage tracking.
    monthly_used: u64,
    monthly_start: DateTime<Utc>,

    /// Audit log of all LLM calls.
    records: Vec<TokenRecord>,
}

impl TokenCounter {
    /// Create a new TokenCounter from budget config.
    pub fn new(budget: &BudgetConfig) -> Self {
        let now = Utc::now();
        Self {
            session_limit: budget.session,
            daily_limit: budget.daily,
            monthly_limit: budget.monthly,
            session_usage: HashMap::new(),
            daily_used: 0,
            daily_start: start_of_day(now),
            monthly_used: 0,
            monthly_start: start_of_month(now),
            records: Vec::new(),
        }
    }

    // --- T030: Pre-call budget checking ---

    /// Check if the budget allows an LLM call for the given session.
    /// Returns Ok(()) if within budget, or Err(BudgetExceeded) if any budget would be exceeded.
    ///
    /// `estimated_tokens` is a rough estimate of how many tokens the call will consume.
    pub fn check_budget(
        &mut self,
        session_key: &str,
        estimated_tokens: u64,
    ) -> Result<(), BudgetExceeded> {
        // Reset daily/monthly counters if periods have rolled over
        self.maybe_reset_periods();

        // Check session budget
        if let Some(limit) = self.session_limit {
            let used = self.session_usage.get(session_key).copied().unwrap_or(0);
            if used + estimated_tokens > limit {
                return Err(BudgetExceeded {
                    scope: BudgetScope::Session(session_key.to_string()),
                    used,
                    limit,
                });
            }
        }

        // Check daily budget
        if let Some(limit) = self.daily_limit {
            if self.daily_used + estimated_tokens > limit {
                return Err(BudgetExceeded {
                    scope: BudgetScope::Daily,
                    used: self.daily_used,
                    limit,
                });
            }
        }

        // Check monthly budget
        if let Some(limit) = self.monthly_limit {
            if self.monthly_used + estimated_tokens > limit {
                return Err(BudgetExceeded {
                    scope: BudgetScope::Monthly,
                    used: self.monthly_used,
                    limit,
                });
            }
        }

        Ok(())
    }

    // --- T031: Post-call usage recording ---

    /// Record token usage after an LLM call.
    pub fn record_usage(
        &mut self,
        session_key: &str,
        agent_id: &str,
        provider: &str,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
    ) {
        let total = (input_tokens + output_tokens) as u64;
        let cost = estimate_cost(provider, model, input_tokens, output_tokens);

        // Update session usage
        *self
            .session_usage
            .entry(session_key.to_string())
            .or_insert(0) += total;

        // Update daily/monthly counters (reset if needed)
        self.maybe_reset_periods();
        self.daily_used += total;
        self.monthly_used += total;

        // Create audit record
        let record = TokenRecord {
            timestamp: Utc::now(),
            session_key: session_key.to_string(),
            agent_id: agent_id.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            input_tokens,
            output_tokens,
            cost_estimate_usd: cost,
        };

        info!(
            session = %session_key,
            agent = %agent_id,
            provider = %provider,
            model = %model,
            input_tokens = input_tokens,
            output_tokens = output_tokens,
            cost_usd = format!("{:.6}", cost),
            "token usage recorded"
        );

        self.records.push(record);
    }

    /// Get usage for a given scope.
    pub fn get_usage(&self, scope: &BudgetScope) -> TokenUsage {
        match scope {
            BudgetScope::Session(key) => {
                let total = self.session_usage.get(key).copied().unwrap_or(0);
                let (input, output, cost) = self.sum_records_for_session(key);
                TokenUsage {
                    input_tokens: input,
                    output_tokens: output,
                    total_tokens: total,
                    cost_estimate_usd: cost,
                }
            }
            BudgetScope::Daily => {
                let (input, output, cost) = self.sum_records_since(self.daily_start);
                TokenUsage {
                    input_tokens: input,
                    output_tokens: output,
                    total_tokens: self.daily_used,
                    cost_estimate_usd: cost,
                }
            }
            BudgetScope::Monthly => {
                let (input, output, cost) = self.sum_records_since(self.monthly_start);
                TokenUsage {
                    input_tokens: input,
                    output_tokens: output,
                    total_tokens: self.monthly_used,
                    cost_estimate_usd: cost,
                }
            }
        }
    }

    /// Get all token records (audit log).
    pub fn records(&self) -> &[TokenRecord] {
        &self.records
    }

    /// Reset daily/monthly counters if the period has rolled over.
    fn maybe_reset_periods(&mut self) {
        let now = Utc::now();

        // Reset daily at midnight UTC
        let today_start = start_of_day(now);
        if today_start > self.daily_start {
            self.daily_used = 0;
            self.daily_start = today_start;
        }

        // Reset monthly on the 1st
        let month_start = start_of_month(now);
        if month_start > self.monthly_start {
            self.monthly_used = 0;
            self.monthly_start = month_start;
        }
    }

    fn sum_records_for_session(&self, session_key: &str) -> (u64, u64, f64) {
        let mut input = 0u64;
        let mut output = 0u64;
        let mut cost = 0.0;
        for r in &self.records {
            if r.session_key == session_key {
                input += r.input_tokens as u64;
                output += r.output_tokens as u64;
                cost += r.cost_estimate_usd;
            }
        }
        (input, output, cost)
    }

    fn sum_records_since(&self, since: DateTime<Utc>) -> (u64, u64, f64) {
        let mut input = 0u64;
        let mut output = 0u64;
        let mut cost = 0.0;
        for r in &self.records {
            if r.timestamp >= since {
                input += r.input_tokens as u64;
                output += r.output_tokens as u64;
                cost += r.cost_estimate_usd;
            }
        }
        (input, output, cost)
    }
}

/// Rough estimate of input tokens from message content.
/// Uses character count / 4 heuristic (approximate BPE for English text).
pub fn estimate_input_tokens(messages: &[serde_json::Value]) -> u64 {
    let mut chars: u64 = 0;
    for msg in messages {
        if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
            chars += content.len() as u64;
        }
    }
    // ~4 chars per token for English text (rough BPE heuristic)
    chars / 4 + 1
}

// --- Helper functions ---

fn start_of_day(dt: DateTime<Utc>) -> DateTime<Utc> {
    dt.date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("valid midnight")
        .and_utc()
}

fn start_of_month(dt: DateTime<Utc>) -> DateTime<Utc> {
    dt.date_naive()
        .with_day(1)
        .expect("day 1 is always valid")
        .and_hms_opt(0, 0, 0)
        .expect("valid midnight")
        .and_utc()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_cost_anthropic_sonnet() {
        // 1000 input tokens, 500 output tokens with Anthropic Sonnet
        // Input: 1000/1M * $3 = $0.003
        // Output: 500/1M * $15 = $0.0075
        // Total: $0.0105
        let cost = estimate_cost("anthropic", "claude-sonnet-4-5-20250929", 1000, 500);
        assert!((cost - 0.0105).abs() < 1e-9);
    }

    #[test]
    fn test_estimate_cost_openai_gpt4o() {
        // 1000 input, 500 output with GPT-4o
        // Input: 1000/1M * $2.50 = $0.0025
        // Output: 500/1M * $10 = $0.005
        // Total: $0.0075
        let cost = estimate_cost("openai", "gpt-4o", 1000, 500);
        assert!((cost - 0.0075).abs() < 1e-9);
    }

    #[test]
    fn test_estimate_input_tokens() {
        let messages = vec![
            serde_json::json!({"role": "user", "content": "Hello, how are you?"}),
            serde_json::json!({"role": "assistant", "content": "I'm doing well, thank you!"}),
        ];
        let estimate = estimate_input_tokens(&messages);
        // "Hello, how are you?" = 19 chars + "I'm doing well, thank you!" = 26 chars = 45 chars
        // 45 / 4 + 1 = 12
        assert_eq!(estimate, 12);
    }
}
