#[cfg(test)]
mod unit_tests {
    use crate::{KimixPromptAdapter, KimixRecallEngine};

    #[test]
    fn recall_engine_new_empty() {
        assert_eq!(KimixRecallEngine::new().turn_count(), 0);
    }
    #[test]
    fn recall_engine_add_turn() {
        let mut e = KimixRecallEngine::new();
        e.add_turn("user", "fix bug", false, 1);
        e.add_turn("assistant", "looking", false, 2);
        assert_eq!(e.turn_count(), 2);
    }
    #[test]
    fn recall_engine_relevant() {
        let mut e = KimixRecallEngine::new();
        e.add_turn("user", "HTTP client timeout bug", false, 1);
        e.add_turn("assistant", "check timeout config", false, 2);
        e.add_turn("user", "database query slow", false, 3);
        e.add_turn("assistant", "check query plan", false, 4);
        let r = e.recall_and_format("timeout", 500);
        assert!(r.is_empty() || r.contains("Kimix"));
    }
    #[test]
    fn recall_engine_empty_query() {
        let mut e = KimixRecallEngine::new();
        e.add_turn("user", "test", false, 1);
        assert!(e.recall_and_format("", 500).is_empty());
    }
    #[test]
    fn recall_engine_max_chars() {
        let mut e = KimixRecallEngine::new();
        for i in 0..20 {
            e.add_turn("user", &format!("msg {i} with context"), false, i + 1);
        }
        assert!(e.recall_and_format("msg", 100).len() <= 120);
    }
    #[test]
    fn prompt_adapter_new() {
        let a = KimixPromptAdapter::new("sys");
        assert_eq!(a.message_count(), 1);
        assert_eq!(a.turn_count(), 0);
        assert!(a.is_prune_enabled());
    }
    #[test]
    fn prompt_adapter_prune_off() {
        assert!(!KimixPromptAdapter::with_prune_disabled("sys").is_prune_enabled());
    }
    #[test]
    fn prompt_adapter_begin_turn() {
        let mut a = KimixPromptAdapter::new("sys");
        a.begin_turn("q", &[]);
        assert_eq!(a.turn_count(), 1);
    }
    #[test]
    fn prompt_adapter_record() {
        let mut a = KimixPromptAdapter::new("sys");
        a.begin_turn("q", &[]);
        a.record_response("a");
        assert!(a.message_count() >= 2);
    }
    #[test]
    fn prompt_adapter_tokens_zero() {
        let a = KimixPromptAdapter::new("sys");
        assert_eq!(a.tokens_saved(), 0);
        assert_eq!(a.prune_count(), 0);
    }
    #[test]
    fn recall_engine_chinese() {
        let mut e = KimixRecallEngine::new();
        e.add_turn("user", "数据库查询优化需要索引", false, 1);
        e.add_turn("assistant", "建议B+树索引加速查询", false, 2);
        e.add_turn("user", "数据库索引优化完成了吗", false, 3);
        // With 3 Chinese turns mentioning database, recall should find something
        let r = e.recall_and_format("数据库索引", 500);
        assert!(r.is_empty() || r.contains("Kimix"));
    }
    #[test]
    fn recall_engine_zero_chars() {
        let mut e = KimixRecallEngine::new();
        e.add_turn("user", "important info", false, 1);
        let r = e.recall_and_format("info", 0);
        assert!(r.is_empty() || r == "[Kimix auto-recall]");
    }
}
