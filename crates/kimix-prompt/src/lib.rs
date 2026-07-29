//! Cache-friendly prompt construction.
//!
//! Implements the 4-layer prompt architecture from KimiX's kimi-cli SDK:
//!
//! 1. **Stable system prompt** — 4000 token cap, never modified mid-session
//! 2. **Independent message pipeline** — injections as separate messages, not prepended
//! 3. **Stale injection stripping** — remove old `<system-reminder>` messages before new turns
//! 4. **Stable prefix protection** — first N messages never pruned (KV cache anchor)
//!
//! This structure ensures maximum KV cache reuse across turns.
//! The system prompt and previous turns form a stable prefix;
//! only the current turn's injection and user message change each request.
pub mod template;

#[cfg(test)]
mod prompt_tests;

use serde::{Deserialize, Serialize};

pub use template::{PromptTemplate, TemplateRegistry};
pub mod stats;
pub use stats::UsageStats;

/// Role of a message in the conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    System,
    User,
    Assistant,
    /// System reminder — injected dynamically, stripped before new turns.
    SystemReminder,
}

impl Role {
    pub fn as_str(&self) -> &str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::SystemReminder => "system-reminder",
        }
    }

    pub fn is_system_reminder(&self) -> bool {
        matches!(self, Role::SystemReminder)
    }
}

/// A single message in the conversation.
#[derive(Debug)]
pub struct Message {
    pub role: Role,
    pub content: String,
    /// Whether this message is ephemeral — can be pruned after consumption.
    pub ephemeral: bool,
    /// Cached token estimate (computed lazily).
    #[allow(dead_code)]
    cached_tokens: std::cell::OnceCell<usize>,
}

// Manual Serialize/Deserialize to skip cached_tokens
impl Serialize for Message {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Message", 3)?;
        st.serialize_field("role", &self.role)?;
        st.serialize_field("content", &self.content)?;
        st.serialize_field("ephemeral", &self.ephemeral)?;
        st.end()
    }
}

impl<'de> Deserialize<'de> for Message {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Helper {
            role: Role,
            content: String,
            #[serde(default)]
            ephemeral: bool,
        }
        let h = Helper::deserialize(d)?;
        Ok(Message {
            role: h.role,
            content: h.content,
            ephemeral: h.ephemeral,
            cached_tokens: std::cell::OnceCell::new(),
        })
    }
}

impl Clone for Message {
    fn clone(&self) -> Self {
        Self {
            role: self.role.clone(),
            content: self.content.clone(),
            ephemeral: self.ephemeral,
            cached_tokens: std::cell::OnceCell::new(),
        }
    }
}

impl Message {
    fn new_empty() -> std::cell::OnceCell<usize> {
        std::cell::OnceCell::new()
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            ephemeral: false,
            cached_tokens: Self::new_empty(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            ephemeral: false,
            cached_tokens: Self::new_empty(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            ephemeral: false,
            cached_tokens: Self::new_empty(),
        }
    }

    pub fn system_reminder(content: impl Into<String>) -> Self {
        Self {
            role: Role::SystemReminder,
            content: content.into(),
            ephemeral: false,
            cached_tokens: Self::new_empty(),
        }
    }

    /// Create an ephemeral tool result message (can be pruned after consumption).
    pub fn tool_result(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            ephemeral: true,
            cached_tokens: Self::new_empty(),
        }
    }

    /// Estimated token count (rough: chars/4 for English, chars/2 for CJK).
    /// Result is cached after first computation.
    pub fn estimated_tokens(&self) -> usize {
        *self.cached_tokens.get_or_init(|| {
            let cjk_count = self.content.chars().filter(|c| {
                matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3040}'..='\u{30FF}' | '\u{AC00}'..='\u{D7AF}')
            }).count();
            let ascii_count = self.content.len().saturating_sub(cjk_count);
            cjk_count + ascii_count / 4
        })
    }
}

/// Configuration for prompt construction and cache management.
#[derive(Debug, Clone)]
pub struct PromptConfig {
    /// Maximum tokens for the system prompt.
    pub max_system_prompt_tokens: usize,
    /// Number of initial messages to protect as stable KV cache anchor.
    pub stable_prefix_messages: usize,
    /// Maximum auto-retrieved injections per turn.
    pub max_injections_per_turn: usize,
    /// Maximum tokens for all injections in one turn.
    pub max_injection_tokens_per_turn: usize,
    /// Enable context-budget tool-result pruning.
    /// When enabled, ephemeral tool outputs are pruned after consumption.
    /// This is Maka's key optimization: -41.7% token cost, +2.48pp performance.
    pub context_budget_prune: bool,
    /// Maximum number of ephemeral messages to keep (protects recent tool results).
    pub max_ephemeral_kept: usize,
    /// Maximum tokens for a single tool output before truncation.
    /// Tool outputs exceeding this limit are truncated (head + tail preserved).
    /// Default: 10_000 (matches Codex's truncation policy).
    /// Set to 0 to disable truncation.
    pub max_tool_output_tokens: usize,
    /// Optional context window size for auto-compact observability.
    /// When set, `begin_turn` will log a warning if total estimated tokens
    /// exceed 80% of this window. Does NOT trigger compaction itself.
    /// Default: None (observability disabled).
    pub context_window: Option<usize>,

    /// Optional maximum effective context window for the 80% observation log.
    /// When set and `context_window` is larger, the effective window used for
    /// the 80% ratio is this cap instead of the full context window. Aligns
    /// with `CompactionPolicy::max_effective_context_tokens`.
    /// Default: None (use full `context_window`).
    pub max_effective_context_tokens: Option<usize>,
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            max_system_prompt_tokens: 4000,
            stable_prefix_messages: 4,
            max_injections_per_turn: 3,
            max_injection_tokens_per_turn: 2000,
            context_budget_prune: true,
            max_ephemeral_kept: 3,
            max_tool_output_tokens: 10_000,
            context_window: None,
            max_effective_context_tokens: None,
        }
    }
}

/// The agent prompt builder.
///
/// Manages the conversation message pipeline with cache-friendly injection
/// and context-budget tool-result pruning (Maka's key optimization).
pub struct AgentPrompt {
    config: PromptConfig,
    /// All messages in the conversation (including stripped ones in history).
    pub messages: Vec<Message>,
    /// Current turn number (1-indexed).
    turn: usize,
    /// Total tokens saved by pruning (cumulative).
    pub tokens_saved: usize,
    /// Total prune passes executed.
    pub prune_count: usize,
    /// Usage statistics tracker.
    pub stats: UsageStats,
}

impl AgentPrompt {
    pub fn new(config: PromptConfig) -> Self {
        Self {
            config,
            messages: Vec::new(),
            turn: 0,
            tokens_saved: 0,
            prune_count: 0,
            stats: UsageStats::default(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(PromptConfig::default())
    }

    /// Set the system prompt. Replaces any existing system message.
    /// Must be called once at session start.
    pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
        let prompt = prompt.into();

        // Enforce token cap
        let truncated = if Self::estimate_tokens(&prompt) > self.config.max_system_prompt_tokens {
            truncate_to_tokens(&prompt, self.config.max_system_prompt_tokens)
        } else {
            prompt
        };

        // Remove existing system message
        self.messages.retain(|m| m.role != Role::System);
        self.messages.insert(0, Message::system(truncated));
    }

    /// Begin a new turn.
    ///
    /// 1. Strip stale system reminders from previous turns
    /// 2. Check auto_compact threshold (80% — observability only)
    /// 3. Prune consumed ephemeral messages (context-budget optimization)
    /// 4. Inject fresh recall context as system reminders
    /// 5. Append the user's message
    ///
    /// Returns the messages that should be sent to the LLM for this turn.
    pub fn begin_turn(
        &mut self,
        user_input: &str,
        recall_injections: &[RecallInjection],
    ) -> &[Message] {
        self.turn += 1;

        // Step 1: Strip stale system reminders from ALL previous turns
        self.strip_stale_reminders();

        // Step 1.5: Auto-compact observability — log when estimated tokens
        // exceed 80% of the context window. This does NOT trigger compaction
        // (compaction is handled at the kimix-shell level via
        // `check_auto_compact_needed`). This is purely observability.
        // TODO: wire context_window from SamplingConfig into PromptConfig
        //       so this check fires for all sessions.
        if let Some(context_window) = self.config.context_window
            && context_window > 0
        {
            let current_tokens = self.total_estimated_tokens();
            // Apply effective context cap (aligns with CompactionPolicy)
            let effective_window = match self.config.max_effective_context_tokens {
                Some(cap) if cap > 0 => context_window.min(cap),
                _ => context_window,
            };
            let token_usage_ratio = current_tokens as f64 / effective_window as f64;
            if token_usage_ratio > 0.8 {
                tracing::info!(
                    ratio = token_usage_ratio,
                    current_tokens,
                    context_window,
                    effective_window,
                    turn = self.turn,
                    "auto_compact: token usage at {:.1}% ({} / {}), effective_window={}, threshold 80% crossed",
                    token_usage_ratio * 100.0,
                    current_tokens,
                    context_window,
                    effective_window,
                );
            }
        }

        // Step 2: Context-budget prune — remove consumed ephemera
        if self.config.context_budget_prune {
            self.prune_consumed_ephemera();
        }

        // Step 3: Inject fresh recall context (capped by config)
        let capped = self.cap_injections(recall_injections);
        for inj in &capped {
            let content = inj.format_as_reminder();
            self.messages.push(Message::system_reminder(content));
        }

        // Step 4: Append user message
        self.messages.push(Message::user(user_input.to_string()));

        // Return visible messages (exclude stripped ones)
        self.visible_messages()
    }

    /// Estimate total tokens across all messages in the conversation.
    /// Uses the per-message `estimated_tokens()` method and sums them.
    pub fn total_estimated_tokens(&self) -> usize {
        self.messages.iter().map(|m| m.estimated_tokens()).sum()
    }

    /// Record the assistant's response for this turn.
    pub fn record_response(&mut self, response: &str) {
        self.messages.push(Message::assistant(response.to_string()));
    }

    /// Get messages visible to the LLM (all non-stripped messages).
    /// Returns a reference for zero-copy internal use.
    pub fn visible_messages(&self) -> &[Message] {
        &self.messages
    }

    /// Get visible messages as owned Vec (for external API compatibility).
    pub fn visible_messages_owned(&self) -> Vec<Message> {
        self.messages.clone()
    }

    /// Get the stable prefix messages (used as KV cache anchor).
    /// These are the first N messages after the system prompt.
    pub fn stable_prefix(&self) -> &[Message] {
        let start = if self
            .messages
            .first()
            .map(|m| m.role == Role::System)
            .unwrap_or(false)
        {
            1 // skip system prompt
        } else {
            0
        };
        let end = (start + self.config.stable_prefix_messages).min(self.messages.len());
        &self.messages[start..end]
    }

    /// Strip all system reminder messages (called at turn start).
    fn strip_stale_reminders(&mut self) {
        self.messages.retain(|m| !m.role.is_system_reminder());
    }

    /// Context-budget prune: remove consumed ephemeral messages.
    ///
    /// This is Maka's key optimization: tool outputs are intermediate artifacts.
    /// After the model reads them and produces the next action, they're "consumed"
    /// and can be pruned from subsequent turns. This recovers token budget without
    /// losing essential context.
    ///
    /// Strategy:
    /// - Keep the most recent `max_ephemeral_kept` ephemeral messages
    /// - Remove older ephemeral messages that have been consumed
    /// - Never remove non-ephemeral messages (user queries, assistant responses)
    /// - Track token savings for diagnostics
    fn prune_consumed_ephemera(&mut self) {
        let max_kept = self.config.max_ephemeral_kept;
        if max_kept == 0 {
            // Prune all ephemera
            let before = self.messages.len();
            self.messages.retain(|m| !m.ephemeral);
            let removed = before - self.messages.len();
            self.tokens_saved += removed * 50; // rough estimate per ephemeral msg
            self.stats.record_prune_savings(removed * 50);
            self.prune_count += 1;
            return;
        }

        // Find ephemeral messages beyond the keep threshold
        let ephemeral_indices: Vec<usize> = self
            .messages
            .iter()
            .enumerate()
            .filter(|(_, m)| m.ephemeral)
            .map(|(i, _)| i)
            .collect();

        if ephemeral_indices.len() <= max_kept {
            return; // Nothing to prune
        }

        // Keep the last max_kept ephemeral messages, prune the rest
        let keep_from = ephemeral_indices[ephemeral_indices.len() - max_kept];

        // Collect indices to remove (ephemeral messages before keep_from)
        let to_remove: Vec<usize> = ephemeral_indices
            .iter()
            .filter(|&&i| i < keep_from)
            .copied()
            .collect();

        // Count tokens saved
        let saved: usize = to_remove
            .iter()
            .map(|&i| self.messages[i].estimated_tokens())
            .sum();

        // Remove in reverse order to keep indices valid
        for &i in to_remove.iter().rev() {
            self.messages.remove(i);
        }

        self.tokens_saved += saved;
        self.stats.record_prune_savings(saved);
        self.prune_count += 1;
    }

    /// Cap injections to config limits (max count + max tokens).
    fn cap_injections(&self, injections: &[RecallInjection]) -> Vec<RecallInjection> {
        let max_count = self.config.max_injections_per_turn;
        let max_tokens = self.config.max_injection_tokens_per_turn;

        let mut capped = Vec::new();
        let mut token_budget = max_tokens;

        for inj in injections.iter().take(max_count) {
            let reminder = inj.format_as_reminder();
            let tokens = Self::estimate_tokens(&reminder);
            if tokens <= token_budget {
                token_budget = token_budget.saturating_sub(tokens);
                capped.push(inj.clone());
            }
        }

        capped
    }

    /// Rough token estimation (fast, no tokenizer needed for budget tracking).
    fn estimate_tokens(text: &str) -> usize {
        let cjk = text.chars().filter(|c| {
            matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3040}'..='\u{30FF}' | '\u{AC00}'..='\u{D7AF}')
        }).count();
        let ascii = text.len().saturating_sub(cjk);
        cjk + ascii / 4
    }

    pub fn turn_count(&self) -> usize {
        self.turn
    }

    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Whether context-budget pruning is enabled.
    pub fn is_prune_enabled(&self) -> bool {
        self.config.context_budget_prune
    }

    /// Set the context window for auto-compact observability (80% usage logging).
    pub fn set_context_window(&mut self, window: usize) {
        self.config.context_window = Some(window);
    }

    /// Set the effective context cap used for the 80% observability ratio.
    /// `0` clears the cap (full `context_window` is used).
    pub fn set_max_effective_context_tokens(&mut self, cap: usize) {
        self.config.max_effective_context_tokens = if cap == 0 { None } else { Some(cap) };
    }
}

/// A single recall injection with relevance metadata.
#[derive(Debug, Clone)]
pub struct RecallInjection {
    /// Source: "history", "working", or "recency".
    pub tier: String,
    /// The role of the recalled turn.
    pub role: String,
    /// The content snippet.
    pub content: String,
    /// BM25 relevance score.
    pub score: f64,
    /// Whether this turn was compacted.
    pub is_compacted: bool,
}

impl RecallInjection {
    /// Format as a `<system-reminder>` block matching KimiX's format.
    pub fn format_as_reminder(&self) -> String {
        let label = match self.tier.as_str() {
            "history" => format!(
                "[Auto-retrieved from past conversation — relevance: {:.2}]",
                self.score
            ),
            "working" => "[Relevant context from our current conversation]".to_string(),
            "recency" => format!("[Recently discussed — relevance: {:.2}]", self.score),
            _ => format!("[Retrieved — relevance: {:.2}]", self.score),
        };

        let compacted = if self.is_compacted {
            " [compacted]"
        } else {
            ""
        };

        format!(
            "{}\n> **{}{}**\n> {}",
            label,
            self.role,
            compacted,
            self.content.replace('\n', "\n> ")
        )
    }
}

/// Truncate text to approximately `max_tokens` tokens.
/// Uses character-based heuristics: CJK ≈ 1 char/token, ASCII ≈ 4 chars/token.
/// Uses sub-token accumulation for accurate budget tracking.
fn truncate_to_tokens(text: &str, max_tokens: usize) -> String {
    let mut accumulated_chars: usize = 0;
    let mut token_budget_used: usize = 0;
    let mut chars_out: Vec<char> = Vec::new();

    for c in text.chars() {
        accumulated_chars += 1;
        if is_cjk(c) || accumulated_chars.is_multiple_of(4) {
            token_budget_used += 1;
        }
        if token_budget_used > max_tokens {
            chars_out.push('…');
            break;
        }
        chars_out.push(c);
    }

    chars_out.into_iter().collect()
}

/// Check if a character is CJK (approximate token cost = 1 per char).
fn is_cjk(c: char) -> bool {
    matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{3040}'..='\u{30FF}' | '\u{AC00}'..='\u{D7AF}')
}

/// Truncate tool output to fit within a token budget, preserving head and tail.
///
/// Strategy:
/// - If output is within `max_tokens`, return unchanged.
/// - Otherwise, keep 2/3 of budget as head, 1/3 as tail.
/// - Insert a `...(truncated N tokens)...` marker between head and tail.
///
/// This is a safety net that runs BEFORE the output becomes a message,
/// complementing context-budget prune which runs AFTER consumption.
pub fn truncate_tool_output(output: &str, max_tokens: usize) -> String {
    if max_tokens == 0 {
        return output.to_string();
    }
    // Use token estimation (CJK ≈ 1 char/token, ASCII ≈ 4 chars/token)
    // instead of raw char count to avoid over-truncating ASCII content.
    let total_tokens = estimate_tokens_simple(output);
    if total_tokens <= max_tokens {
        return output.to_string();
    }
    let head_tokens = max_tokens * 2 / 3;
    let tail_tokens = max_tokens - head_tokens;
    let head = truncate_to_tokens(output, head_tokens);
    let tail: String = output.chars().rev().collect();
    let tail = truncate_to_tokens(&tail, tail_tokens);
    let tail: String = tail.chars().rev().collect();
    let saved = total_tokens.saturating_sub(max_tokens);
    format!("{head}\n...(truncated {saved} tokens)...\n{tail}")
}

/// Simple token estimation for truncation (CJK: 1 char/token, ASCII: ~4 chars/token).
fn estimate_tokens_simple(text: &str) -> usize {
    let cjk = text.chars().filter(|c| is_cjk(*c)).count();
    let ascii_len = text.len().saturating_sub(cjk);
    cjk + ascii_len / 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_prompt_stability() {
        let mut prompt = AgentPrompt::with_defaults();
        prompt.set_system_prompt("You are a helpful coding agent. Follow instructions carefully.");

        // System prompt is at position 0 and never changes across turns
        let sys = &prompt.messages[0];
        assert_eq!(sys.role, Role::System);

        // First turn
        let injections = vec![RecallInjection {
            tier: "history".into(),
            role: "user".into(),
            content: "Previous discussion about HTTP client".into(),
            score: 8.5,
            is_compacted: true,
        }];
        let msgs = prompt.begin_turn("Help me with async Python", &injections);
        assert_eq!(msgs[0].role, Role::System);
        assert_eq!(msgs[1].role, Role::SystemReminder);
        assert_eq!(msgs[2].role, Role::User);

        // Second turn: stale reminder from turn 1 is stripped
        let new_injections = vec![RecallInjection {
            tier: "working".into(),
            role: "assistant".into(),
            content: "I created the async client".into(),
            score: 7.2,
            is_compacted: false,
        }];
        let msgs2 = prompt.begin_turn("Add retry logic", &new_injections);

        // Check: only ONE system reminder (fresh), not two
        let reminders: Vec<_> = msgs2
            .iter()
            .filter(|m| m.role.is_system_reminder())
            .collect();
        assert_eq!(reminders.len(), 1, "Stale reminders should be stripped");
        assert_eq!(
            msgs2[0].role,
            Role::System,
            "System prompt should remain first"
        );
    }

    #[test]
    fn test_injection_capping() {
        let mut prompt = AgentPrompt::with_defaults();
        prompt.set_system_prompt("Test");

        // Create 5 injections exceeding the default cap of 3
        let injections: Vec<_> = (0..5)
            .map(|i| RecallInjection {
                tier: "history".into(),
                role: "user".into(),
                content: format!("Turn {}", i),
                score: 10.0 - i as f64,
                is_compacted: false,
            })
            .collect();

        let msgs = prompt.begin_turn("Query", &injections);
        let reminders: Vec<_> = msgs
            .iter()
            .filter(|m| m.role.is_system_reminder())
            .collect();
        assert_eq!(
            reminders.len(),
            3,
            "Should cap at max_injections_per_turn (3)"
        );
    }

    #[test]
    fn test_token_budget_capping() {
        let mut prompt = AgentPrompt::with_defaults();
        prompt.set_system_prompt("Test");

        // First injection fits within budget (500 bytes of ascii ≈ 125 estimated tokens)
        let small = vec![RecallInjection {
            tier: "history".into(),
            role: "user".into(),
            content: "Short recall".into(),
            score: 9.0,
            is_compacted: false,
        }];
        let msgs = prompt.begin_turn("Query1", &small);
        assert_eq!(
            msgs.iter().filter(|m| m.role.is_system_reminder()).count(),
            1,
            "Small injection should fit in budget"
        );

        // Very long injection exceeds token budget (10000 bytes ≈ 2500 estimated tokens > 2000 limit)
        // After stripping stale reminders, we try to inject the long one
        let long = vec![RecallInjection {
            tier: "history".into(),
            role: "user".into(),
            content: "x".repeat(10000),
            score: 9.0,
            is_compacted: false,
        }];
        let msgs2 = prompt.begin_turn("Query2", &long);
        assert_eq!(
            msgs2.iter().filter(|m| m.role.is_system_reminder()).count(),
            0,
            "Long injection should be excluded by token budget"
        );
    }

    #[test]
    fn test_empty_injections() {
        let mut prompt = AgentPrompt::with_defaults();
        prompt.set_system_prompt("Test");

        let msgs = prompt.begin_turn("Simple query", &[]);
        let reminders: Vec<_> = msgs
            .iter()
            .filter(|m| m.role.is_system_reminder())
            .collect();
        assert_eq!(reminders.len(), 0);
        assert_eq!(msgs[1].role, Role::User);
    }

    #[test]
    fn test_record_response() {
        let mut prompt = AgentPrompt::with_defaults();
        prompt.set_system_prompt("Test");

        prompt.begin_turn("Query", &[]);
        prompt.record_response("Answer");

        let msgs = prompt.visible_messages();
        assert_eq!(msgs.last().unwrap().role, Role::Assistant);
        assert_eq!(msgs.last().unwrap().content, "Answer");
    }

    #[test]
    fn test_stable_prefix() {
        let mut prompt = AgentPrompt::with_defaults();
        prompt.set_system_prompt("System");

        for i in 0..6 {
            prompt.begin_turn(&format!("Turn {}", i), &[]);
            prompt.record_response(&format!("Response {}", i));
        }

        let prefix = prompt.stable_prefix();
        // stable_prefix_messages = 4, but we have all messages (system + 6 turns * 2)
        // The stable prefix starts after system prompt, so messages[1..5]
        assert_eq!(prefix.len(), 4);
        assert_eq!(prefix[0].role, Role::User);
        assert_eq!(prefix[0].content, "Turn 0");
    }

    #[test]
    fn test_context_budget_prune() {
        let mut prompt = AgentPrompt::with_defaults();
        prompt.set_system_prompt("Test");

        // Simulate: turn 1 has tool results (ephemeral), turn 2 has tool results
        // After turn 2's results are consumed, turn 1's tool results should be pruned

        // Turn 1: user query → tool result → assistant response
        prompt.begin_turn("Run the tests", &[]);
        // Simulate a tool result (ephemeral)
        prompt
            .messages
            .push(Message::tool_result("test output: 5 passed, 0 failed"));
        prompt.messages.push(Message::tool_result("coverage: 85%"));
        prompt.record_response("Tests passed with 85% coverage.");

        // Turn 2: user query → more tool results
        prompt.begin_turn("Add more tests", &[]);
        prompt
            .messages
            .push(Message::tool_result("added 3 new tests"));
        prompt
            .messages
            .push(Message::tool_result("now 8 tests total"));
        prompt.record_response("Added 3 tests, now 8 total.");

        // Turn 3: the begin_turn should have pruned turn 1's ephemera
        prompt.begin_turn("Check coverage", &[]);
        prompt.record_response("Coverage is now 90%.");

        // Verify: ephemeral messages should be limited
        let ephemeral_count = prompt.messages.iter().filter(|m| m.ephemeral).count();
        assert!(
            ephemeral_count <= prompt.config.max_ephemeral_kept + 2, // +2 for the current turn's
            "Ephemeral messages should be pruned: found {}, max_kept={}",
            ephemeral_count,
            prompt.config.max_ephemeral_kept
        );

        // Verify: token savings tracked
        assert!(
            prompt.tokens_saved > 0,
            "Should have saved tokens from pruning"
        );
        assert!(
            prompt.prune_count > 0,
            "Should have executed at least one prune"
        );
    }

    #[test]
    fn test_context_budget_disabled() {
        let config = PromptConfig {
            context_budget_prune: false,
            ..Default::default()
        };

        let mut prompt = AgentPrompt::new(config);
        prompt.set_system_prompt("Test");

        // Add ephemeral messages
        prompt.begin_turn("Query", &[]);
        prompt.messages.push(Message::tool_result("tool output"));
        prompt.record_response("Response");

        let count_before = prompt.messages.iter().filter(|m| m.ephemeral).count();

        // Next turn — should NOT prune
        prompt.begin_turn("Query 2", &[]);
        prompt.record_response("Response 2");

        let count_after = prompt.messages.iter().filter(|m| m.ephemeral).count();
        assert_eq!(
            count_after, count_before,
            "Should NOT prune when context_budget_prune is disabled"
        );
    }

    #[test]
    fn test_prune_protects_stable_prefix() {
        let mut prompt = AgentPrompt::with_defaults();
        prompt.set_system_prompt("System");

        // Turn 1: non-ephemeral user + assistant
        prompt.begin_turn("Turn 1", &[]);
        prompt.record_response("Response 1");

        // Add ephemeral tool results
        prompt.messages.push(Message::tool_result("build output"));
        prompt.messages.push(Message::tool_result("lint output"));
        prompt.messages.push(Message::tool_result("test output"));
        prompt.messages.push(Message::tool_result("deploy output"));

        // Turn 2: should prune older ephemera
        prompt.begin_turn("Turn 2", &[]);
        prompt.record_response("Response 2");

        // Verify: system prompt + user/assistant messages are preserved
        assert!(
            prompt.messages[0].role == Role::System,
            "System prompt must be preserved"
        );
        assert!(
            prompt.messages.iter().any(|m| m.content == "Turn 1"),
            "Turn 1 user message preserved"
        );
        assert!(
            prompt.messages.iter().any(|m| m.content == "Response 1"),
            "Turn 1 response preserved"
        );
    }

    #[test]
    fn test_truncate_tool_output() {
        let short = "hello world";
        assert_eq!(truncate_tool_output(short, 100), short);

        let long = "x".repeat(200);
        let result = truncate_tool_output(&long, 100);
        assert!(result.len() < long.len());
        assert!(result.contains("truncated"));
        assert!(result.starts_with('x'));
        assert!(result.ends_with('x'));

        assert_eq!(truncate_tool_output(short, 0), short);
    }
}
