//! Turn-continuation provider: detect three classes of weak model output and
//! inject a synthetic user message so the loop keeps working.
//!
//! Weak open-source models end turns prematurely in three reproducible ways:
//!
//! 1. **Length** — the output was cut by the provider's `max_tokens` limit.
//!    The response contains visible text but no tool calls and stopped with a
//!    `Length` stop reason. We ask the model to continue exactly where it
//!    stopped.
//! 2. **Dangling intent** — the model says "I'll now update the file" or
//!    "Let me run the tests" and then ends the turn without doing it. The
//!    turn is not empty, but the last sentence promises action that never
//!    happened. We ask the model to actually perform the promised action.
//! 3. **Empty** — the response has no visible content and no tool calls.
//!    (The sampler already resamples empties internally; this arm is the
//!    backstop for the rare case that reaches the turn layer.)
//!
//! The provider is a **pure state machine** — no session dependencies — so
//! it is trivially unit-testable. The session layer (`turn.rs`) owns the
//! counters, calls [`ContinuationProvider::provide`] after each completed
//! model response, and injects the returned prompt via
//! `ConversationItem::auto_continue`.
//!
//! # Guardrails
//!
//! - Per-class retry caps (`length_limit` / `intent_limit` / `empty_limit`)
//!   prevent infinite continuation loops.
//! - Tool-call turns reset the `empty` and `intent` counters (the model was
//!   demonstrably productive); the `length` counter is deliberately **not**
//!   reset because a truncation can recur across tool rounds.
//! - Intent detection uses conservative phrase matching with a false-positive
//!   exclusion list (`let me know`, `if you ...`, ...) so a genuine handoff
//!   back to the user is never hijacked.

use std::fmt;

/// Why a continuation was (or was not) produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinuationKind {
    /// Model hit the output token limit mid-sentence.
    LengthCut,
    /// Model promised an action but ended the turn without performing it.
    DanglingIntent,
    /// Model produced no visible content and no tool calls.
    Empty,
    /// Nothing worth continuing — the turn should end.
    None,
}

impl ContinuationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LengthCut => "length",
            Self::DanglingIntent => "intent",
            Self::Empty => "empty",
            Self::None => "none",
        }
    }
}

/// Inputs describing the just-completed model response.
#[derive(Debug, Clone)]
pub struct TurnEndSignal {
    /// Whether the assistant produced any visible text this round.
    pub had_visible_content: bool,
    /// Provider stop reason (normalized Kimix vocabulary).
    pub stop_reason: Option<kimix_sampling_types::conversation::StopReason>,
    /// Trailing assistant text (untrimmed).
    pub last_assistant_text: String,
    /// Whether the round contained any tool calls.
    pub had_tool_calls: bool,
}

/// Per-class continuation budgets. Sensible defaults for weak open-source
/// models; the session can override per instance.
#[derive(Debug, Clone, Copy)]
pub struct ContinuationLimits {
    pub length_limit: u32,
    pub intent_limit: u32,
    pub empty_limit: u32,
}

impl Default for ContinuationLimits {
    fn default() -> Self {
        Self {
            length_limit: 3,
            intent_limit: 2,
            empty_limit: 2,
        }
    }
}

/// Outcome of a single [`ContinuationProvider::provide`] call.
#[derive(Debug, Clone)]
pub enum ContinuationOutcome {
    /// Inject `prompt` as a synthetic user message and continue the loop.
    Continue {
        kind: ContinuationKind,
        prompt: String,
    },
    /// Do not inject; the turn ends normally.
    EndTurn,
    /// The `empty` budget was exhausted — the session should surface a
    /// notice rather than silently ending (the model produced nothing at all).
    EmptyBudgetExhausted { attempts: u32 },
}

impl ContinuationOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Continue { .. } => "continue",
            Self::EndTurn => "end_turn",
            Self::EmptyBudgetExhausted { .. } => "empty_exhausted",
        }
    }
}

/// Stateful continuation policy. Holds only counters; `Default` is the
/// canonical starting point.
#[derive(Debug, Clone)]
pub struct ContinuationProvider {
    empty_counter: u32,
    length_counter: u32,
    intent_counter: u32,
    limits: ContinuationLimits,
}

impl Default for ContinuationProvider {
    fn default() -> Self {
        Self {
            empty_counter: 0,
            length_counter: 0,
            intent_counter: 0,
            limits: ContinuationLimits::default(),
        }
    }
}

impl ContinuationProvider {
    pub fn new(limits: ContinuationLimits) -> Self {
        Self {
            empty_counter: 0,
            length_counter: 0,
            intent_counter: 0,
            limits,
        }
    }

    /// Evaluate the turn-end signal and decide whether to continue.
    pub fn provide(&mut self, signal: &TurnEndSignal) -> ContinuationOutcome {
        if signal.had_visible_content {
            self.provide_visible(signal)
        } else {
            self.provide_empty(signal)
        }
    }

    /// Signal that a round performed tool calls — resets the counters that
    /// reflect "unproductive" turns. `length` is intentionally preserved.
    pub fn on_tool_turn(&mut self) {
        self.empty_counter = 0;
        self.intent_counter = 0;
    }

    fn provide_visible(&mut self, signal: &TurnEndSignal) -> ContinuationOutcome {
        // Truncation wins: the model literally ran out of output budget.
        if signal.stop_reason == Some(kimix_sampling_types::conversation::StopReason::Length)
            && self.length_counter < self.limits.length_limit
        {
            self.length_counter += 1;
            return ContinuationOutcome::Continue {
                kind: ContinuationKind::LengthCut,
                prompt: length_cut_prompt().into(),
            };
        }
        if self.intent_counter < self.limits.intent_limit
            && ends_with_dangling_intent(&signal.last_assistant_text)
        {
            self.intent_counter += 1;
            return ContinuationOutcome::Continue {
                kind: ContinuationKind::DanglingIntent,
                prompt: dangling_intent_prompt().into(),
            };
        }
        ContinuationOutcome::EndTurn
    }

    fn provide_empty(&mut self, signal: &TurnEndSignal) -> ContinuationOutcome {
        if signal.had_tool_calls {
            // Tool calls + no visible text is a legitimate "did the work,
            // only called tools" round — nothing to continue.
            return ContinuationOutcome::EndTurn;
        }
        if self.empty_counter < self.limits.empty_limit {
            self.empty_counter += 1;
            return ContinuationOutcome::Continue {
                kind: ContinuationKind::Empty,
                prompt: empty_prompt().into(),
            };
        }
        ContinuationOutcome::EmptyBudgetExhausted {
            attempts: self.empty_counter,
        }
    }
}

/// Expose counters for tests / observability.
impl ContinuationProvider {
    pub fn counters(&self) -> (u32, u32, u32) {
        (self.empty_counter, self.length_counter, self.intent_counter)
    }
}

// ─── Prompts (original, license-clean) ─────────────────────────────────────
//
// These are purpose-written for Kimix; they describe the failure mode and the
// expected fix explicitly so a weak model can comply without extra reasoning.

/// Model ended a turn that was cut off by the output limit.
pub fn length_cut_prompt() -> &'static str {
    "Your previous response was cut off by the output token limit before you \
     finished. Continue exactly where you left off — do not repeat content \
     you already produced. If you were about to call a tool, call it now."
}

/// Model ended its turn right after announcing an action it never performed.
pub fn dangling_intent_prompt() -> &'static str {
    "You ended your turn right after saying you were about to do something, \
     without actually doing it. Do it now — make the tool call or produce the \
     output you described. Do not restate the plan. If the task is genuinely \
     complete, state the final result instead."
}

/// Model produced nothing at all (no visible text, no tool calls).
pub fn empty_prompt() -> &'static str {
    "You produced an empty response. Tell the user what is happening or what \
     you plan to do next, and continue the task."
}

// ─── Dangling-intent detection ─────────────────────────────────────────────

/// Markers of a deferred action (lowercased, substring match). Open-source
/// models phrase these anywhere in the final sentence ("I'll update the file
/// now", "Next step: run the tests"), so we match anywhere — the
/// false-positive list below keeps genuine handoffs out.
const INTENT_PHRASES: &[&str] = &[
    "let me ",
    "i'll ",
    "ill ",
    "i will ",
    "i'm going to ",
    "im going to ",
    "next step",
    "next, ",
    "next: ",
];

/// Markers that mean the turn is *waiting* on the user rather than deferring
/// an action — never treat these as dangling intent.
const FALSE_POSITIVE_PREFIXES: &[&str] = &[
    "let me know",
    "let me think",
    "let me check if you",
    "if you ",
    "when you ",
    "shall i ",
    "should i ",
    "would you like",
    "tell me if",
    "let me know if",
];

/// True when `text` ends a turn with a deferred-action phrase that the model
/// announced but did not perform.
pub fn ends_with_dangling_intent(text: &str) -> bool {
    let text = strip_markdown_edges(text);
    let text = text.trim();
    if text.is_empty() {
        return false;
    }
    let lower = text.to_ascii_lowercase();
    // Genuine handoffs to the user win over every other signal.
    for marker in FALSE_POSITIVE_PREFIXES {
        if lower.contains(marker) {
            return false;
        }
    }
    // A trailing colon ("Next step:", "Now do this:") is a strong promise
    // marker regardless of phrasing.
    if text.ends_with(':') {
        return true;
    }
    INTENT_PHRASES.iter().any(|phrase| lower.contains(phrase))
}

/// Strip a single trailing code fence / emphasis edge so intent detection
/// sees the natural sentence end.
fn strip_markdown_edges(text: &str) -> &str {
    let text = text.trim();
    let text = text.strip_suffix("```").unwrap_or(text);
    let text = text.strip_suffix('`').unwrap_or(text);
    text.trim_end()
}

impl fmt::Display for ContinuationProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ContinuationProvider(empty={}/{}, length={}/{}, intent={}/{})",
            self.empty_counter,
            self.limits.empty_limit,
            self.length_counter,
            self.limits.length_limit,
            self.intent_counter,
            self.limits.intent_limit,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kimix_sampling_types::conversation::StopReason;

    fn sig(content: bool, stop: Option<StopReason>, text: &str, tools: bool) -> TurnEndSignal {
        TurnEndSignal {
            had_visible_content: content,
            stop_reason: stop,
            last_assistant_text: text.to_string(),
            had_tool_calls: tools,
        }
    }

    #[test]
    fn length_cut_triggers_continuation() {
        let mut p = ContinuationProvider::default();
        let out = p.provide(&sig(
            true,
            Some(StopReason::Length),
            "We need to update",
            false,
        ));
        match out {
            ContinuationOutcome::Continue { kind, prompt } => {
                assert_eq!(kind, ContinuationKind::LengthCut);
                assert!(prompt.contains("cut off"));
            }
            other => panic!("expected continue, got {other:?}"),
        }
        assert_eq!(p.counters(), (0, 1, 0));
    }

    #[test]
    fn length_cut_respects_limit() {
        let mut p = ContinuationProvider::default();
        for _ in 0..3 {
            let out = p.provide(&sig(true, Some(StopReason::Length), "more", false));
            assert!(matches!(out, ContinuationOutcome::Continue { .. }));
        }
        let out = p.provide(&sig(true, Some(StopReason::Length), "more", false));
        assert!(matches!(out, ContinuationOutcome::EndTurn));
        assert_eq!(p.counters().1, 3);
    }

    #[test]
    fn dangling_intent_triggers_continuation() {
        let mut p = ContinuationProvider::default();
        let out = p.provide(&sig(
            true,
            Some(StopReason::Stop),
            "I'll update the file now",
            false,
        ));
        match out {
            ContinuationOutcome::Continue { kind, .. } => {
                assert_eq!(kind, ContinuationKind::DanglingIntent);
            }
            other => panic!("expected continue, got {other:?}"),
        }
        assert_eq!(p.counters().2, 1);
    }

    #[test]
    fn dangling_intent_with_colon_triggers() {
        let mut p = ContinuationProvider::default();
        let out = p.provide(&sig(
            true,
            Some(StopReason::Stop),
            "Next step: run the tests",
            false,
        ));
        assert!(matches!(out, ContinuationOutcome::Continue { .. }));
    }

    #[test]
    fn natural_completion_does_not_trigger() {
        let mut p = ContinuationProvider::default();
        let out = p.provide(&sig(
            true,
            Some(StopReason::Stop),
            "The tests pass. Task complete.",
            false,
        ));
        assert!(matches!(out, ContinuationOutcome::EndTurn));
    }

    #[test]
    fn handoff_to_user_not_hijacked() {
        // "let me know" / "if you" are genuine handoffs, never dangling.
        for text in [
            "Let me know if you want me to continue",
            "Tell me if this looks right",
            "Shall I proceed with the changes?",
        ] {
            assert!(
                !ends_with_dangling_intent(text),
                "must not treat as dangling: {text:?}"
            );
        }
    }

    #[test]
    fn empty_triggers_then_exhausts() {
        let mut p = ContinuationProvider::default();
        for _ in 0..2 {
            let out = p.provide(&sig(false, Some(StopReason::Stop), "", false));
            assert!(matches!(out, ContinuationOutcome::Continue { .. }));
        }
        let out = p.provide(&sig(false, Some(StopReason::Stop), "", false));
        assert!(matches!(
            out,
            ContinuationOutcome::EmptyBudgetExhausted { attempts: 2 }
        ));
    }

    #[test]
    fn empty_with_tool_calls_is_fine() {
        let mut p = ContinuationProvider::default();
        let out = p.provide(&sig(false, Some(StopReason::Stop), "", true));
        assert!(matches!(out, ContinuationOutcome::EndTurn));
    }

    #[test]
    fn tool_turn_resets_intent_but_not_length() {
        let mut p = ContinuationProvider::default();
        let _ = p.provide(&sig(true, Some(StopReason::Length), "cut", false));
        let _ = p.provide(&sig(true, Some(StopReason::Stop), "I'll now do it", false));
        assert_eq!(p.counters(), (0, 1, 1));
        p.on_tool_turn();
        assert_eq!(p.counters(), (0, 1, 0), "intent reset, length preserved");
    }

    #[test]
    fn code_fence_edge_does_not_break_detection() {
        assert!(ends_with_dangling_intent("I'm going to fix the bug:\n```"));
        assert!(ends_with_dangling_intent("Let me check the config file:"));
        assert!(!ends_with_dangling_intent("Done. Everything works."));
    }

    #[test]
    fn empty_prompt_is_directive() {
        assert!(empty_prompt().contains("empty response"));
    }
}
