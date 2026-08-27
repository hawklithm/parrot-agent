//! Official provider price mapping for token-based cost estimation.
//!
//! When a provider response includes explicit `total_cost_usd`, that value is
//! used directly. This registry fills the gap for responses that return token
//! counts but no cost — most commonly via direct HTTP API calls where the
//! raw provider response has usage metrics but the adapter hasn't calculated
//! a cost field.
//!
//! Prices are sourced from official published rates:
//! - Anthropic: https://docs.anthropic.com/en/docs/about-claude/pricing
//!   (Claude 3.5 Sonnet, Claude 3 Opus, Claude 3 Haiku, Claude 3.5 Haiku)
//! - OpenAI:   https://openai.com/api/pricing/
//!   (GPT-4o, GPT-4o-mini, GPT-4-turbo, GPT-3.5-turbo, o1, o3-mini)
//!
//! Prices are in USD per 1,000,000 tokens. Unknown models return None (caller decides).

use std::collections::HashMap;
use std::sync::LazyLock;

static PRICE_MAP: LazyLock<HashMap<&'static str, ModelPrice>> = LazyLock::new(build_price_map);
#[derive(Debug, Clone, Copy)]
struct ModelPrice {
    input_per_1m: f64,
    cached_input_per_1m: f64,
    output_per_1m: f64,
}

impl ModelPrice {
    /// Calculate cost in USD for a given token breakdown.
    fn estimate_usd(&self, input_tokens: i64, cached_input_tokens: i64, output_tokens: i64) -> f64 {
        let input_cost = (input_tokens as f64 / 1_000_000.0) * self.input_per_1m;
        let cached_cost = (cached_input_tokens as f64 / 1_000_000.0) * self.cached_input_per_1m;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * self.output_per_1m;
        input_cost + cached_cost + output_cost
    }
}

fn build_price_map() -> HashMap<&'static str, ModelPrice> {
    let mut m = HashMap::new();

    // ── Anthropic / Claude ──────────────────────────────────────────

    // Claude 3.5 Sonnet (Oct 2024)
    m.insert("claude-3-5-sonnet", ModelPrice { input_per_1m: 3.0, cached_input_per_1m: 0.30, output_per_1m: 15.0 });
    m.insert("claude-3-5-sonnet-20241022", ModelPrice { input_per_1m: 3.0, cached_input_per_1m: 0.30, output_per_1m: 15.0 });
    // Claude Sonnet 4
    m.insert("claude-sonnet-4-20250514", ModelPrice { input_per_1m: 3.0, cached_input_per_1m: 0.30, output_per_1m: 15.0 });
    m.insert("claude-sonnet-4", ModelPrice { input_per_1m: 3.0, cached_input_per_1m: 0.30, output_per_1m: 15.0 });

    // Claude 3.5 Haiku
    m.insert("claude-3-5-haiku", ModelPrice { input_per_1m: 0.80, cached_input_per_1m: 0.08, output_per_1m: 4.0 });
    m.insert("claude-3-5-haiku-20241022", ModelPrice { input_per_1m: 0.80, cached_input_per_1m: 0.08, output_per_1m: 4.0 });

    // Claude 3 Opus
    m.insert("claude-3-opus", ModelPrice { input_per_1m: 15.0, cached_input_per_1m: 1.50, output_per_1m: 75.0 });
    m.insert("claude-3-opus-20240229", ModelPrice { input_per_1m: 15.0, cached_input_per_1m: 1.50, output_per_1m: 75.0 });

    // Claude 3 Haiku
    m.insert("claude-3-haiku", ModelPrice { input_per_1m: 0.25, cached_input_per_1m: 0.025, output_per_1m: 1.25 });
    m.insert("claude-3-haiku-20240307", ModelPrice { input_per_1m: 0.25, cached_input_per_1m: 0.025, output_per_1m: 1.25 });

    // Claude 2 / Instant
    m.insert("claude-2", ModelPrice { input_per_1m: 8.0, cached_input_per_1m: 8.0, output_per_1m: 24.0 });
    m.insert("claude-instant-1", ModelPrice { input_per_1m: 1.63, cached_input_per_1m: 1.63, output_per_1m: 5.51 });

    // ── OpenAI / GPT ────────────────────────────────────────────────

    // GPT-4o
    m.insert("gpt-4o", ModelPrice { input_per_1m: 2.50, cached_input_per_1m: 1.25, output_per_1m: 10.0 });
    m.insert("gpt-4o-2024-08-06", ModelPrice { input_per_1m: 2.50, cached_input_per_1m: 1.25, output_per_1m: 10.0 });
    m.insert("gpt-4o-2024-11-20", ModelPrice { input_per_1m: 2.50, cached_input_per_1m: 1.25, output_per_1m: 10.0 });

    // GPT-4o mini
    m.insert("gpt-4o-mini", ModelPrice { input_per_1m: 0.15, cached_input_per_1m: 0.075, output_per_1m: 0.60 });
    m.insert("gpt-4o-mini-2024-07-18", ModelPrice { input_per_1m: 0.15, cached_input_per_1m: 0.075, output_per_1m: 0.60 });

    // GPT-4 Turbo
    m.insert("gpt-4-turbo", ModelPrice { input_per_1m: 10.0, cached_input_per_1m: 10.0, output_per_1m: 30.0 });
    m.insert("gpt-4-turbo-2024-04-09", ModelPrice { input_per_1m: 10.0, cached_input_per_1m: 10.0, output_per_1m: 30.0 });

    // GPT-4 / GPT-4-32k
    m.insert("gpt-4", ModelPrice { input_per_1m: 30.0, cached_input_per_1m: 30.0, output_per_1m: 60.0 });
    m.insert("gpt-4-32k", ModelPrice { input_per_1m: 60.0, cached_input_per_1m: 60.0, output_per_1m: 120.0 });

    // GPT-3.5 Turbo
    m.insert("gpt-3.5-turbo", ModelPrice { input_per_1m: 0.50, cached_input_per_1m: 0.50, output_per_1m: 1.50 });
    m.insert("gpt-3.5-turbo-0125", ModelPrice { input_per_1m: 0.50, cached_input_per_1m: 0.50, output_per_1m: 1.50 });

    // o1 reasoning
    m.insert("o1", ModelPrice { input_per_1m: 15.0, cached_input_per_1m: 7.50, output_per_1m: 60.0 });
    m.insert("o1-2024-12-17", ModelPrice { input_per_1m: 15.0, cached_input_per_1m: 7.50, output_per_1m: 60.0 });
    m.insert("o1-mini", ModelPrice { input_per_1m: 1.10, cached_input_per_1m: 0.55, output_per_1m: 4.40 });
    m.insert("o1-mini-2024-09-12", ModelPrice { input_per_1m: 1.10, cached_input_per_1m: 0.55, output_per_1m: 4.40 });
    m.insert("o3-mini", ModelPrice { input_per_1m: 1.10, cached_input_per_1m: 0.55, output_per_1m: 4.40 });
    m.insert("o3-mini-2025-01-31", ModelPrice { input_per_1m: 1.10, cached_input_per_1m: 0.55, output_per_1m: 4.40 });

    m
}

fn lookup(model: &str) -> Option<&'static ModelPrice> {
    let normalized = model.trim().to_ascii_lowercase();

    // Direct match first
    if let Some(price) = PRICE_MAP.get(normalized.as_str()) {
        return Some(price);
    }

    // Prefix match: try progressively shorter known prefixes
    let prefixes = [
        "claude-sonnet-4-20250514",
        "claude-sonnet-4",
        "claude-3-5-sonnet-20241022",
        "claude-3-5-sonnet",
        "claude-3-5-haiku-20241022",
        "claude-3-5-haiku",
        "claude-3-opus-20240229",
        "claude-3-opus",
        "claude-3-haiku-20240307",
        "claude-3-haiku",
        "claude-2",
        "claude-instant-1",
        "claude",
        "gpt-4o-mini-2024-07-18",
        "gpt-4o-mini",
        "gpt-4o-2024-11-20",
        "gpt-4o-2024-08-06",
        "gpt-4o",
        "gpt-4-turbo-2024-04-09",
        "gpt-4-turbo",
        "gpt-4-32k",
        "gpt-4",
        "gpt-3.5-turbo-0125",
        "gpt-3.5-turbo",
        "gpt-3.5",
        "gpt",
        "o1-2024-12-17",
        "o1",
        "o1-mini-2024-09-12",
        "o1-mini",
        "o3-mini-2025-01-31",
        "o3-mini",
        "o3",
    ];
    for prefix in &prefixes {
        if normalized.starts_with(prefix) {
            if let Some(price) = PRICE_MAP.get(*prefix) {
                return Some(price);
            }
        }
    }
    None
}

/// Estimate cost in USD for a model and token breakdown.
/// Returns `None` when the model is unknown and no price can be estimated.
pub fn estimate_cost_usd(
    model: &str,
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
) -> Option<f64> {
    let price = lookup(model)?;
    let cost = price.estimate_usd(input_tokens, cached_input_tokens, output_tokens);
    // Round to reasonable precision, avoid negative-zero artifacts
    Some((cost * 100_000.0).round() / 100_000.0)
}

/// Fill in `cost_usd` from the price registry when the provider did not
/// return an explicit cost but token counts are available.
pub fn fill_missing_cost(
    outcome_cost_usd: Option<f64>,
    model: Option<&str>,
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
) -> Option<f64> {
    if outcome_cost_usd.is_some() {
        return outcome_cost_usd;
    }
    let model = model?;
    if model.is_empty() || model == "unknown" {
        return None;
    }
    estimate_cost_usd(model, input_tokens, cached_input_tokens, output_tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_claude_sonnet_pricing() {
        let cost = estimate_cost_usd("claude-3-5-sonnet-20241022", 1000, 200, 500).unwrap();
        // (1000/1M)*3.0 + (200/1M)*0.30 + (500/1M)*15.0 = 0.003 + 0.00006 + 0.0075 = 0.01056
        assert!((cost - 0.01056).abs() < 0.0001, "sonnet cost={cost}");
    }

    #[test]
    fn anthropic_claude_haiku_pricing() {
        let cost = estimate_cost_usd("claude-3-5-haiku-latest", 2000, 0, 1000).unwrap();
        // (2000/1M)*0.80 + 0 + (1000/1M)*4.0 = 0.0016 + 0.004 = 0.0056
        assert!((cost - 0.0056).abs() < 0.0001, "haiku cost={cost}");
    }

    #[test]
    fn anthropic_claude_3_opus_pricing() {
        let cost = estimate_cost_usd("claude-3-opus-latest", 500, 100, 300).unwrap();
        // (500/1M)*15.0 + (100/1M)*1.50 + (300/1M)*75.0 = 0.0075 + 0.00015 + 0.0225 = 0.03015
        assert!((cost - 0.03015).abs() < 0.0001, "opus cost={cost}");
    }

    #[test]
    fn openai_gpt4o_pricing() {
        let cost = estimate_cost_usd("gpt-4o-2024-11-20", 1500, 500, 800).unwrap();
        // (1500/1M)*2.5 + (500/1M)*1.25 + (800/1M)*10.0 = 0.00375 + 0.000625 + 0.008 = 0.012375
        assert!((cost - 0.012375).abs() < 0.0001, "gpt4o cost={cost}");
    }

    #[test]
    fn openai_gpt4o_mini_pricing() {
        let cost = estimate_cost_usd("gpt-4o-mini", 5000, 0, 2000).unwrap();
        // (5000/1M)*0.15 + 0 + (2000/1M)*0.60 = 0.00075 + 0.0012 = 0.00195
        assert!((cost - 0.00195).abs() < 0.0001, "gpt4o-mini cost={cost}");
    }

    #[test]
    fn unknown_model_returns_none() {
        assert!(estimate_cost_usd("nonexistent-model-v42", 1000, 0, 500).is_none());
    }

    #[test]
    fn fill_missing_uses_explicit_cost_when_present() {
        let result = fill_missing_cost(Some(0.05), Some("claude-3-5-sonnet"), 1000, 0, 500);
        assert_eq!(result, Some(0.05), "explicit cost should be preserved");
    }

    #[test]
    fn fill_missing_estimates_when_no_explicit_cost() {
        let result = fill_missing_cost(None, Some("claude-3-5-sonnet"), 1000, 0, 500);
        assert!(result.is_some(), "should estimate from model+tokens");
        assert!(result.unwrap() > 0.0, "estimated cost positive");
    }

    #[test]
    fn fill_missing_returns_none_for_unknown_model() {
        let result = fill_missing_cost(None, Some("unknown"), 1000, 0, 500);
        assert!(result.is_none(), "unknown model should return None");
    }

    #[test]
    fn fill_missing_returns_none_for_empty_model() {
        let result = fill_missing_cost(None, Some(""), 1000, 0, 500);
        assert!(result.is_none(), "empty model should return None");
    }

    #[test]
    fn fill_missing_zero_tokens_no_model() {
        let result = fill_missing_cost(None, None, 0, 0, 0);
        assert!(result.is_none(), "no model and no tokens should return None");
    }

    #[test]
    fn prefix_matches_gpt4o_variants() {
        assert!(estimate_cost_usd("gpt-4o-2025-01-20", 100, 0, 100).is_some());
        assert!(estimate_cost_usd("gpt-4o-custom-fork-xyz", 100, 0, 100).is_some());
    }

    #[test]
    fn prefix_matches_claude_sonnet_4() {
        let cost = estimate_cost_usd("claude-sonnet-4-20250514", 1000, 0, 500).unwrap();
        assert!(cost > 0.0, "sonnet-4 cost should be positive: {cost}");
    }

    #[test]
    fn o1_mini_price_is_reasonable() {
        let cost = estimate_cost_usd("o1-mini-2024-09-12", 1000, 500, 400).unwrap();
        // (1000/1M)*1.10 + (500/1M)*0.55 + (400/1M)*4.40 = 0.0011 + 0.000275 + 0.00176 = 0.003135
        assert!((cost - 0.003135).abs() < 0.0001, "o1-mini cost={cost}");
    }

    #[test]
    fn o3_mini_price_is_reasonable() {
        let cost = estimate_cost_usd("o3-mini", 2000, 0, 1000).unwrap();
        // (2000/1M)*1.10 + 0 + (1000/1M)*4.40 = 0.0022 + 0.0044 = 0.0066
        assert!((cost - 0.0066).abs() < 0.0001, "o3-mini cost={cost}");
    }
}
