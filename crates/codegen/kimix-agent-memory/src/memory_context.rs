//! Context management: usage tracking, compaction triggers, pruning.
//!
//! Mirrors KimiX's `kimi_cli/config.py` loop control configuration.
use serde::{Deserialize, Serialize};

/// Configuration for context window management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    /// Maximum context size in tokens (model-dependent).
    pub max_context_size: usize,
    /// Reserved tokens for LLM response generation.
    pub reserved_context_size: usize,
    /// Context usage ratio at which auto-compaction triggers.
    /// Triggers when: context_tokens >= max_context_size * compaction_trigger_ratio
    /// OR: context_tokens + reserved_context_size >= max_context_size
    pub compaction_trigger_ratio: f64,
    /// Context usage ratio at which the compact reminder is injected.
    /// Should be lower than compaction_trigger_ratio.
    pub compact_reminder_threshold: f64,
    /// Number of initial messages protected as stable KV cache prefix.
    pub stable_prefix_messages: usize,
    /// Number of recent turns protected from pruning.
    pub recent_turns_protected: usize,
    /// Minimum tokens to free for a prune pass to execute.
    pub prune_min_free_tokens: usize,
    /// Minimum steps between consecutive prune passes.
    pub prune_cooldown_steps: usize,
    /// Maximum preserved messages during compaction.
    pub max_preserved_messages: usize,
    /// Minimum preserved messages during compaction.
    pub min_preserved_messages: usize,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_context_size: 128_000,
            reserved_context_size: 75_000,
            compaction_trigger_ratio: 0.75,
            compact_reminder_threshold: 0.70,
            stable_prefix_messages: 4,
            recent_turns_protected: 6,
            prune_min_free_tokens: 2_000,
            prune_cooldown_steps: 4,
            max_preserved_messages: 2,
            min_preserved_messages: 1,
        }
    }
}

/// Current context usage state.
#[derive(Debug, Clone, Default)]
pub struct ContextState {
    /// Estimated current token count in the context window.
    pub context_tokens: usize,
    /// Number of turns in the conversation.
    pub turn_count: usize,
    /// Steps since last prune.
    pub steps_since_prune: usize,
    /// Whether context was recently compacted.
    pub recently_compacted: bool,
}

impl ContextState {
    /// Whether auto-compaction should trigger.
    /// Dual condition (matching KimiX's logic):
    /// 1. context_tokens >= max_context_size * compaction_trigger_ratio
    /// 2. context_tokens + reserved_context_size >= max_context_size
    pub fn should_compact(&self, config: &ContextConfig) -> bool {
        let ratio_trigger = self.context_tokens as f64
            >= config.max_context_size as f64 * config.compaction_trigger_ratio;
        let reserved_trigger =
            self.context_tokens + config.reserved_context_size >= config.max_context_size;
        ratio_trigger || reserved_trigger
    }

    /// Whether to inject a compact reminder (before auto-compaction triggers).
    pub fn should_remind_compact(&self, config: &ContextConfig) -> bool {
        self.context_tokens as f64
            >= config.max_context_size as f64 * config.compact_reminder_threshold
    }

    /// Generate a compact reminder message for injection.
    pub fn compact_reminder(&self, config: &ContextConfig) -> String {
        format!(
            "[system-reminder] Context usage is high ({:.0}% of {} tokens). \
             Consider calling /compact to free space before the next step.",
            self.usage_ratio(config) * 100.0,
            config.max_context_size
        )
    }

    /// Context usage as a ratio of max_context_size.
    pub fn usage_ratio(&self, config: &ContextConfig) -> f64 {
        if self.context_tokens == 0 || config.max_context_size == 0 {
            0.0
        } else {
            self.context_tokens as f64 / config.max_context_size as f64
        }
    }
}

/// Context manager: tracks usage, triggers compaction, manages pruning.
pub struct ContextManager {
    pub config: ContextConfig,
    pub state: ContextState,
}

impl ContextManager {
    pub fn new(config: ContextConfig) -> Self {
        Self {
            config,
            state: ContextState::default(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(ContextConfig::default())
    }

    /// Update context state after a turn.
    pub fn update(&mut self, tokens_added: usize) {
        self.state.context_tokens += tokens_added;
        self.state.turn_count += 1;
        self.state.steps_since_prune += 1;
    }

    /// Check and return an action to take.
    pub fn check(&self) -> ContextAction {
        if self.state.should_compact(&self.config) && !self.state.recently_compacted {
            ContextAction::Compact
        } else if self.state.should_remind_compact(&self.config) {
            ContextAction::RemindCompact
        } else {
            ContextAction::None
        }
    }

    /// Mark compaction as completed, reset context tokens to estimated post-compaction value.
    pub fn mark_compacted(&mut self, new_token_estimate: usize) {
        self.state.context_tokens = new_token_estimate;
        self.state.recently_compacted = true;
    }

    /// Reset the "recently compacted" flag (e.g., after a few turns).
    pub fn reset_compaction_flag(&mut self) {
        self.state.recently_compacted = false;
    }
}

/// Action to take based on context state.
#[derive(Debug, PartialEq, Eq)]
pub enum ContextAction {
    /// Trigger LLM-driven compaction.
    Compact,
    /// Inject a reminder to the user/agent about high context usage.
    RemindCompact,
    /// No action needed.
    None,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compaction_trigger_by_ratio() {
        let config = ContextConfig {
            max_context_size: 100_000,
            compaction_trigger_ratio: 0.75,
            ..Default::default()
        };

        let state = ContextState {
            context_tokens: 80_000, // 80% — above 75%
            ..Default::default()
        };
        assert!(state.should_compact(&config));
    }

    #[test]
    fn test_compaction_trigger_by_reserved() {
        let config = ContextConfig {
            max_context_size: 200_000,
            reserved_context_size: 75_000,
            compaction_trigger_ratio: 0.90,
            ..Default::default()
        };

        let state = ContextState {
            context_tokens: 130_000, // 130K + 75K = 205K >= 200K
            ..Default::default()
        };
        assert!(state.should_compact(&config));
    }

    #[test]
    fn test_no_compaction_below_threshold() {
        let config = ContextConfig::default();
        let state = ContextState {
            context_tokens: 50_000, // Below both 128K * 0.75 = 96K and 50K + 75K < 128K
            ..Default::default()
        };
        assert!(!state.should_compact(&config));
    }

    #[test]
    fn test_reminder_below_compact() {
        // 145K / 200K = 72.5% — above 70% reminder, below 75% compact
        // 145K + 75K = 220K >= 200K → also triggers compact by reserved.
        // Use a config where reserved doesn't interfere.
        let config = ContextConfig {
            max_context_size: 200_000,
            reserved_context_size: 40_000,    // Lower reserved
            compaction_trigger_ratio: 0.75,   // 150K
            compact_reminder_threshold: 0.70, // 140K
            ..Default::default()
        };
        let state = ContextState {
            context_tokens: 145_000,
            ..Default::default()
        };
        // ratio: 145K < 150K → False
        // reserved: 145K + 40K = 185K < 200K → False
        assert!(!state.should_compact(&config));
        assert!(state.should_remind_compact(&config));
    }
}
