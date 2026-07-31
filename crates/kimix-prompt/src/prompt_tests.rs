#[cfg(test)]
mod tests {
    use crate::{AgentPrompt, Message, PromptConfig, RecallInjection, Role, truncate_tool_output};

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
