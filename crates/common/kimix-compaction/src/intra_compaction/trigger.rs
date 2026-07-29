//! Trigger decision and result types for intra-compaction.
use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;

use super::config::{IntraCompactionConfig, IntraCompactionMode};

/// Information about why intra-compaction was triggered.
///
/// Constructed by [`should_compact`] and threaded through to
/// [`crate::compact`] and the agent's event stream.
#[derive(Debug, Clone)]
pub struct IntraCompactionTrigger {
    /// Token count of the prompt most recently sent to the model.
    pub last_prompt_tokens: u32,
    /// Context window of the agent's current sampler (`max_len`).
    pub context_window: u32,
    /// `last_prompt_tokens / context_window` as an integer percentage,
    /// clamped to [0, 100].
    pub percent: u8,
    /// Step index (0-based) at which the trigger fired.
    pub step: u32,
}

/// Result of a successful compaction.
#[derive(Debug, Clone)]
pub struct IntraCompactionResult {
    /// Sum of tokens in the turns that were compacted.
    pub tokens_before: u32,
    /// Tokens in the resulting compaction turn (the LLM summary).
    pub tokens_after: u32,
    /// Number of accumulated turns that were replaced.
    pub turns_compacted: u32,
    /// End-to-end elapsed time (decision → apply).
    pub elapsed: Duration,
    /// The summary text the LLM produced — the developer-turn content that
    /// replaced the compacted turns (for `HistoryThenSteps`, both passes'
    /// summaries joined). Carried so callers can record the actual result
    /// (e.g. as a developer turn in the thinking trace). `Arc<str>` because the
    /// summary can be large and is cloned along with the event downstream.
    pub summary: Arc<str>,
}

/// Errors that can occur during intra-compaction.
///
/// All errors are non-fatal — the caller should log and continue without
/// compaction. Worst case the next sampling call may fail with 400, which
/// is the same as today (no compaction support at all).
#[derive(Debug, Error)]
pub enum IntraCompactionError {
    /// The accumulated turn list has nothing meaningful to compact.
    /// Triggered when:
    /// - `get_accumulated_turns_for_compaction()` returns empty
    /// - `select_turns_to_compact()` finds nothing reducible (below
    ///   `min_compactable_tokens` or no safe split point)
    #[error("nothing to compact")]
    NothingToCompact,

    /// The compaction LLM call timed out with no usable output.
    #[error("compaction LLM call timed out")]
    Timeout,

    /// The compaction LLM returned an empty response.
    #[error("compaction LLM returned empty response")]
    EmptyResponse,

    /// Compaction result was not smaller than the original by the configured
    /// minimum (`max_reduction_ratio`).
    #[error("insufficient reduction: {tokens_after} > {tokens_before} * ratio")]
    InsufficientReduction {
        tokens_before: u32,
        tokens_after: u32,
    },

    /// `apply_steps_compaction` received an invalid `n_turns_to_remove`
    /// (greater than the current accumulated-turn count). Parser state is
    /// left unchanged.
    #[error("invalid split: requested {requested}, only {available} available")]
    InvalidSplit { requested: usize, available: usize },

    /// The parser variant does not support intra-compaction.
    #[error("intra-compaction not supported by this parser variant")]
    Unsupported,

    /// LLM sampler construction failed.
    #[error("compaction sampler build failed: {0}")]
    SamplerBuild(String),

    /// LLM sampler call could not be started.
    #[error("compaction sampler start failed: {0}")]
    SamplerStart(String),

    /// LLM sampler emitted an error mid-stream.
    #[error("compaction sampler error: {0}")]
    SamplerStream(String),

    /// `apply_steps_compaction` failed for a parser-specific reason
    /// (e.g. SglangEngine rebuild error).
    #[error("apply failed: {0}")]
    Apply(String),
}

/// Pure decision function: should intra-compaction trigger now?
///
/// Returns `Some(trigger)` if all gating conditions are met; `None` otherwise.
/// Caller must additionally check the feature flag and/or any global kill
/// switch — this function deals only with the policy + step state.
///
/// `min_steps_before_compact` remains on [`IntraCompactionConfig`] for every
/// mode, but is **not** enforced when
/// [`mode`](IntraCompactionConfig::mode) is
/// [`IntraCompactionMode::FullReplace`] — that path matches Kimix's
/// full-replace trigger (token threshold alone) so a large first-step prompt
/// can still compact. Partial modes still gate on min steps.
pub fn should_compact(
    policy: &IntraCompactionConfig,
    last_prompt_tokens: u32,
    context_window: u32,
    current_step: u32,
) -> Option<IntraCompactionTrigger> {
    if !policy.enabled {
        return None;
    }
    if context_window == 0 {
        return None;
    }
    // FullReplace: token threshold only (field still present on config).
    // Partial modes: skip early steps with little content to reduce.
    if policy.mode != IntraCompactionMode::FullReplace
        && current_step < policy.min_steps_before_compact
    {
        return None;
    }

    // Cap the effective context window for threshold computation.
    // Large-context models (1M tokens) hit a quality cliff well below
    // their nominal window; this cap keeps compaction triggers in the
    // reliable working range (research shows degradation starts at ~32K,
    // becomes severe beyond ~200K).
    let effective_window = if policy.max_effective_context_tokens > 0 {
        context_window.min(policy.max_effective_context_tokens)
    } else {
        context_window
    };

    let threshold =
        (effective_window as u64 * policy.trigger_threshold_percent as u64 / 100) as u32;
    if last_prompt_tokens <= threshold {
        return None;
    }

    // Report percent against actual context_window for observability
    let percent = (last_prompt_tokens as u64 * 100 / context_window as u64).min(100) as u8;
    Some(IntraCompactionTrigger {
        last_prompt_tokens,
        context_window,
        percent,
        step: current_step,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_policy() -> IntraCompactionConfig {
        IntraCompactionConfig {
            enabled: true,
            // Default mode is FullReplace — min_steps stored but not enforced.
            trigger_threshold_percent: 75,
            target_threshold_percent: 50,
            min_steps_before_compact: 3,
            ..Default::default()
        }
    }

    fn enabled_partial_policy(mode: IntraCompactionMode) -> IntraCompactionConfig {
        IntraCompactionConfig {
            mode,
            ..enabled_policy()
        }
    }

    #[test]
    fn returns_none_when_disabled() {
        let mut p = enabled_policy();
        p.enabled = false;
        assert!(should_compact(&p, 90_000, 100_000, 10).is_none());
    }

    #[test]
    fn returns_none_when_below_threshold() {
        let p = enabled_policy();
        // 74% of 100K = 74_000, threshold 75% = 75_000.
        assert!(should_compact(&p, 74_000, 100_000, 10).is_none());
    }

    #[test]
    fn returns_some_when_above_threshold() {
        let p = enabled_policy();
        let t = should_compact(&p, 90_000, 100_000, 10).expect("should trigger");
        assert_eq!(t.last_prompt_tokens, 90_000);
        assert_eq!(t.context_window, 100_000);
        assert_eq!(t.percent, 90);
        assert_eq!(t.step, 10);
    }

    #[test]
    fn full_replace_keeps_field_but_ignores_min_steps() {
        let p = enabled_policy();
        assert_eq!(p.mode, IntraCompactionMode::FullReplace);
        assert_eq!(p.min_steps_before_compact, 3);
        // Field is present; FullReplace only uses the token threshold
        // (parity with Kimix auto-compact).
        let t = should_compact(&p, 90_000, 100_000, 0).expect("should trigger");
        assert_eq!(t.step, 0);
        assert!(should_compact(&p, 90_000, 100_000, 2).is_some());
    }

    #[test]
    fn partial_modes_enforce_min_steps() {
        for mode in [
            IntraCompactionMode::StepsOnly,
            IntraCompactionMode::HistoryOnly,
            IntraCompactionMode::HistoryThenSteps,
        ] {
            let p = enabled_partial_policy(mode);
            assert!(
                should_compact(&p, 90_000, 100_000, 2).is_none(),
                "{mode:?} should gate on min_steps"
            );
            let t = should_compact(&p, 90_000, 100_000, 3).expect("should trigger at min steps");
            assert_eq!(t.step, 3);
        }
    }

    #[test]
    fn returns_none_when_context_window_zero() {
        let p = enabled_policy();
        assert!(should_compact(&p, 1_000, 0, 10).is_none());
    }

    #[test]
    fn percent_caps_at_100() {
        let p = enabled_policy();
        let t = should_compact(&p, 200_000, 100_000, 10).expect("should trigger");
        assert_eq!(t.percent, 100);
    }

    #[test]
    fn boundary_exact_threshold_does_not_trigger() {
        let p = enabled_policy();
        // last_prompt_tokens == threshold: not strictly greater than.
        assert!(should_compact(&p, 75_000, 100_000, 10).is_none());
        // One above triggers.
        assert!(should_compact(&p, 75_001, 100_000, 10).is_some());
    }

    #[test]
    fn effective_cap_triggers_earlier_for_large_context() {
        let p = enabled_policy();
        // 1M context, cap at 200K, threshold 75% of 200K = 150K
        // 180K tokens should trigger (180K > 150K)
        assert!(should_compact(&p, 180_000, 1_000_000, 10).is_some());
        // 120K tokens should NOT trigger (120K < 150K)
        assert!(should_compact(&p, 120_000, 1_000_000, 10).is_none());
    }

    #[test]
    fn effective_cap_does_not_affect_small_context() {
        let p = enabled_policy();
        // 64K context, cap at 200K — cap doesn't apply
        // 75% of 64K = 48K
        assert!(should_compact(&p, 50_000, 64_000, 10).is_some());
        assert!(should_compact(&p, 40_000, 64_000, 10).is_none());
    }

    #[test]
    fn effective_cap_disabled_when_zero() {
        let mut p = enabled_policy();
        p.max_effective_context_tokens = 0;
        // 1M context, cap disabled, threshold 75% of 1M = 750K
        assert!(should_compact(&p, 800_000, 1_000_000, 10).is_some());
        assert!(should_compact(&p, 700_000, 1_000_000, 10).is_none());
    }

    #[test]
    fn effective_cap_at_boundary_256k() {
        let p = enabled_policy();
        // 256K context, cap at 200K, threshold 75% of 200K = 150K
        // 160K tokens should trigger (160K > 150K)
        assert!(should_compact(&p, 160_000, 256_000, 10).is_some());
        // 140K tokens should NOT trigger (140K < 150K)
        assert!(should_compact(&p, 140_000, 256_000, 10).is_none());
    }
}
