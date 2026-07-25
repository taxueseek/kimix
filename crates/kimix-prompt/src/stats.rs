//! Usage statistics: token counting and cost estimation.
use serde::{Deserialize, Serialize};

/// Tracks cumulative usage across a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageStats {
    /// Cumulative input tokens.
    pub total_input_tokens: usize,
    /// Cumulative output tokens.
    pub total_output_tokens: usize,
    /// Number of turns completed.
    pub turn_count: usize,
    /// Tokens saved by context-budget pruning.
    pub tokens_saved_by_prune: usize,
    /// Estimated cost in USD (DeepSeek V3 pricing: $0.14/M in, $0.28/M out).
    pub estimated_cost_usd: f64,
    /// Tokens saved by cache hits (KV cache reuse).
    pub tokens_saved_by_cache: usize,
}

impl UsageStats {
    /// Record a completed turn.
    pub fn record_turn(&mut self, input_tokens: usize, output_tokens: usize) {
        self.total_input_tokens += input_tokens;
        self.total_output_tokens += output_tokens;
        self.turn_count += 1;
        self.recalculate_cost();
    }

    /// Record tokens saved by context-budget pruning.
    pub fn record_prune_savings(&mut self, tokens: usize) {
        self.tokens_saved_by_prune += tokens;
    }

    /// Record cache-hit savings.
    pub fn record_cache_hit(&mut self, tokens: usize) {
        self.tokens_saved_by_cache += tokens;
    }

    fn recalculate_cost(&mut self) {
        self.estimated_cost_usd =
            self.total_input_tokens as f64 * 0.14 / 1_000_000.0
            + self.total_output_tokens as f64 * 0.28 / 1_000_000.0;
    }

    /// Format a human-readable summary.
    pub fn summary(&self) -> String {
        format!(
            "{} turns | {:.1}K in + {:.1}K out | ~${:.4} | prune saved {:.1}K | cache saved {:.1}K",
            self.turn_count,
            self.total_input_tokens as f64 / 1000.0,
            self.total_output_tokens as f64 / 1000.0,
            self.estimated_cost_usd,
            self.tokens_saved_by_prune as f64 / 1000.0,
            self.tokens_saved_by_cache as f64 / 1000.0,
        )
    }

    /// Tokens per turn average.
    pub fn avg_tokens_per_turn(&self) -> f64 {
        if self.turn_count == 0 {
            0.0
        } else {
            (self.total_input_tokens + self.total_output_tokens) as f64 / self.turn_count as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_cost() {
        let mut stats = UsageStats::default();
        stats.record_turn(5000, 1000);
        stats.record_turn(3000, 500);
        assert_eq!(stats.turn_count, 2);
        assert_eq!(stats.total_input_tokens, 8000);
        assert_eq!(stats.total_output_tokens, 1500);
        assert!(stats.estimated_cost_usd > 0.0);
    }

    #[test]
    fn test_summary_non_empty() {
        let mut stats = UsageStats::default();
        stats.record_turn(10000, 2000);
        let s = stats.summary();
        assert!(s.contains("turns"));
        assert!(s.contains("$"));
    }

    #[test]
    fn test_avg_zero_turns() {
        let stats = UsageStats::default();
        assert_eq!(stats.avg_tokens_per_turn(), 0.0);
    }

    #[test]
    fn test_prune_savings() {
        let mut stats = UsageStats::default();
        stats.record_prune_savings(500);
        assert_eq!(stats.tokens_saved_by_prune, 500);
        stats.record_cache_hit(1000);
        assert_eq!(stats.tokens_saved_by_cache, 1000);
    }
}
