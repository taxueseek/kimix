#[cfg(test)]
mod tests {
    use crate::{AgentPrompt, Message, PromptConfig, RecallInjection, Role, truncate_tool_output};

    const FIXED_SYSTEM_PROMPT: &str = "You are a coding agent. (fixed content, no timestamps)";

    /// Run the same multi-turn workflow (system + 8 user/assistant/tool rounds)
    /// so the context-budget prune replaces old tool results with placeholders.
    /// Two independent runs MUST serialize identically — this is the KV-cache
    /// prefix stability contract: any nondeterminism (timestamps, ordering,
    /// hash seeds, iteration order) here would break prompt-cache hits.
    fn run_same_workflow() -> AgentPrompt {
        let mut p = AgentPrompt::new(default_config());
        p.set_system_prompt(FIXED_SYSTEM_PROMPT);
        for turn in 0..8 {
            p.begin_turn(&format!("user question {turn}"), &[]);
            p.record_response(&format!("assistant answer {turn}"));
            p.record_tool_result(&format!("tool output {turn} {}", "x".repeat(300)));
        }
        p
    }

    fn default_config() -> PromptConfig {
        PromptConfig {
            max_system_prompt_tokens: 4000,
            stable_prefix_messages: 2,
            max_injections_per_turn: 10,
            max_injection_tokens_per_turn: 2000,
            context_budget_prune: true,
            max_ephemeral_kept: 5,
            max_tool_output_tokens: 10000,
            context_window: None,
            max_effective_context_tokens: None,
            soft_nudge_ratio: 0.55,
            content_hash_dedup: true,
            ephemeral_preview_chars: 120,
        }
    }

    #[test]
    fn role_as_str() {
        assert_eq!(Role::System.as_str(), "system");
        assert_eq!(Role::User.as_str(), "user");
        assert_eq!(Role::Assistant.as_str(), "assistant");
        assert_eq!(Role::SystemReminder.as_str(), "system-reminder");
    }
    #[test]
    fn role_is_reminder() {
        assert!(Role::SystemReminder.is_system_reminder());
        assert!(!Role::User.is_system_reminder());
    }
    #[test]
    fn message_constructors() {
        assert!(!Message::system("s").ephemeral);
        assert_eq!(Message::user("u").role, Role::User);
        assert_eq!(Message::assistant("a").role, Role::Assistant);
        // system_reminder is NOT ephemeral by default
        assert!(Message::tool_result("t").ephemeral);
    }

    #[test]
    fn prefix_serialization_byte_stable_across_rebuilds() {
        let a = run_same_workflow();
        let b = run_same_workflow();
        let sa = serde_json::to_vec(a.visible_messages()).unwrap();
        let sb = serde_json::to_vec(b.visible_messages()).unwrap();
        assert_eq!(sa, sb, "identical workflow must serialize byte-identically");

        // 8 tool results with max_ephemeral_kept=5 must have produced prune
        // placeholders (deterministic text), proving the prune path is stable.
        let placeholders = a
            .visible_messages()
            .iter()
            .filter(|m| m.content.contains("[tool result omitted (context budget)"))
            .count();
        assert!(
            placeholders >= 2,
            "expected pruned placeholders, got {placeholders}"
        );
    }

    #[test]
    fn stable_prefix_is_fixed_after_rebuild() {
        let mut a = AgentPrompt::new(default_config());
        let mut b = AgentPrompt::new(default_config());
        a.set_system_prompt(FIXED_SYSTEM_PROMPT);
        b.set_system_prompt(FIXED_SYSTEM_PROMPT);
        a.begin_turn("q1", &[]);
        b.begin_turn("q1", &[]);
        a.record_response("r1");
        b.record_response("r1");

        let prefix_a = serde_json::to_vec(a.stable_prefix()).unwrap();
        let prefix_b = serde_json::to_vec(b.stable_prefix()).unwrap();
        assert_eq!(prefix_a, prefix_b);
        assert!(
            !prefix_a.is_empty(),
            "stable prefix should anchor the cache after a turn"
        );
    }
    #[test]
    fn message_serde_roundtrip() {
        let m = Message::user("test");
        let j = serde_json::to_string(&m).unwrap();
        let d: Message = serde_json::from_str(&j).unwrap();
        assert_eq!(d.content, "test");
    }
    #[test]
    fn message_serde_no_cached_tokens() {
        assert!(
            !serde_json::to_string(&Message::system("s"))
                .unwrap()
                .contains("cached_tokens")
        );
    }
    #[test]
    fn message_clone() {
        let c = Message::assistant("r").clone();
        assert_eq!(c.content, "r");
    }
    #[test]
    fn message_estimated_tokens() {
        let m = Message::user("hello world");
        let t = m.estimated_tokens();
        assert!(t > 0);
        assert_eq!(t, m.estimated_tokens());
    }
    #[test]
    fn agent_empty() {
        let p = AgentPrompt::with_defaults();
        assert_eq!(p.turn_count(), 0);
        assert!(p.visible_messages().is_empty());
    }
    #[test]
    fn agent_system_prompt() {
        let mut p = AgentPrompt::with_defaults();
        p.set_system_prompt("expert");
        assert_eq!(p.visible_messages()[0].content, "expert");
    }
    #[test]
    fn agent_begin_turn() {
        let mut p = AgentPrompt::new(default_config());
        p.begin_turn("q", &[]);
        assert!(p.visible_messages().iter().any(|m| m.role == Role::User));
    }
    #[test]
    fn agent_record_response() {
        let mut p = AgentPrompt::new(default_config());
        p.begin_turn("q", &[]);
        p.record_response("a");
        assert!(p.visible_messages().iter().any(|m| m.content == "a"));
    }
    #[test]
    fn agent_turn_count() {
        let mut p = AgentPrompt::new(default_config());
        p.begin_turn("1", &[]);
        p.begin_turn("2", &[]);
        assert_eq!(p.turn_count(), 2);
    }
    #[test]
    fn agent_stable_prefix() {
        let mut p = AgentPrompt::new(default_config());
        p.set_system_prompt("sys");
        p.begin_turn("u", &[]);
        assert!(!p.stable_prefix().is_empty());
    }
    #[test]
    fn agent_prune_flag() {
        assert!(AgentPrompt::new(default_config()).is_prune_enabled());
    }
    #[test]
    fn agent_visible_owned() {
        let mut p = AgentPrompt::new(default_config());
        p.begin_turn("q", &[]);
        assert_eq!(p.visible_messages_owned().len(), p.visible_messages().len());
    }
    #[test]
    fn agent_many_turns() {
        let mut p = AgentPrompt::new(default_config());
        for i in 0..100 {
            p.begin_turn(&format!("m{i}"), &[]);
        }
        assert_eq!(p.turn_count(), 100);
    }
    #[test]
    fn recall_format() {
        let inj = RecallInjection {
            tier: "h".into(),
            role: "u".into(),
            content: "auth flow".into(),
            score: 0.95,
            is_compacted: false,
        };
        assert!(inj.format_as_reminder().contains("auth flow"));
    }
    #[test]
    fn recall_compacted() {
        let inj = RecallInjection {
            tier: "w".into(),
            role: "a".into(),
            content: "sum".into(),
            score: 0.5,
            is_compacted: true,
        };
        assert!(inj.format_as_reminder().contains("sum"));
    }
    #[test]
    fn truncate_ok() {
        assert_eq!(truncate_tool_output("s", 100), "s");
    }
    #[test]
    fn truncate_over() {
        let s = "x".repeat(1000);
        let t = truncate_tool_output(&s, 10);
        assert!(t.len() < s.len());
    }
    #[test]
    fn truncate_empty() {
        assert_eq!(truncate_tool_output("", 5), "");
    }
}
