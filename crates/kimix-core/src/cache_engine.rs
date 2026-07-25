//! 缓存引擎：管理四分区上下文，确保高缓存命中率。
//!
//! # 架构
//!
//! ```text
//! Context
//! ├── Tier 1 (Immutable)  — 系统提示 + 工具定义
//! ├── Tier 2 (Stable)     — AGENTS.md + 偏好
//! ├── Tier 3 (Volatile)   — 最近 1 轮工具结果
//! └── Tier 4 (Ephemeral)  — 用户消息
//! ```
//!
//! # 缓存策略
//!
//! - Tier 1：永久缓存（字节级稳定前缀）
//! - Tier 2：会话级缓存
//! - Tier 3：消费后立即清除（仅保留最近 1 轮）
//! - Tier 4：不缓存，每轮重建

use std::collections::VecDeque;

use unicode_normalization::UnicodeNormalization;

/// Cache key 版本前缀，与 kimix 发布版本对齐。
const CACHE_KEY_VERSION: &str = "kimix:v0.1.11";

/// Tier 3 保留的最大条目数（最近 N 轮工具结果）。
const MAX_VOLATILE_ENTRIES: usize = 1;

/// Tier 3 预分配容量（预留未来扩展多轮）。
const VOLATILE_CAPACITY: usize = 4;

/// 组装时分隔符额外容量估计。
const ASSEMBLE_SEPARATOR_OVERHEAD: usize = 16;

/// 四分区上下文结构。
///
/// 稳定前缀（Tier1 + Tier2）在跨轮次中保持字节级一致，
/// 以最大化 prefix cache 命中率。
#[derive(Debug, Clone, PartialEq)]
pub struct Context {
    /// Tier 1: 永不变动的内容。
    ///
    /// 包含：系统提示、工具定义列表、i18n 常量。
    /// 缓存策略：永久缓存。
    pub tier1_immutable: String,

    /// Tier 2: 会话内不变的内容。
    ///
    /// 包含：AGENTS.md 指令、用户偏好、项目配置。
    /// 缓存策略：会话级缓存。
    pub tier2_stable: String,

    /// Tier 3: 每轮变动的内容。
    ///
    /// 包含：最近 1 轮工具执行结果。
    /// 缓存策略：消费后立即清除。
    pub tier3_volatile: VecDeque<String>,

    /// Tier 4: 每轮重建的内容。
    ///
    /// 包含：当前用户消息。
    /// 缓存策略：不缓存。
    pub tier4_ephemeral: String,
}

impl Context {
    /// 创建新的四分区上下文。
    ///
    /// # Arguments
    ///
    /// * `system_prompt` - 系统提示（Tier 1）
    /// * `tool_definitions` - 工具定义列表（Tier 1）
    /// * `agents_md` - AGENTS.md 内容（Tier 2）
    /// * `preferences` - 用户偏好（Tier 2）
    ///
    /// # Returns
    ///
    /// 组装好的 [`Context`] 实例。
    ///
    /// # Examples
    ///
    /// ```
    /// use kimix_core::cache_engine::Context;
    ///
    /// let ctx = Context::new(
    ///     "You are a helpful assistant.",
    ///     "read_file, edit_file, bash",
    ///     "# Project Rules\n...",
    ///     "language: zh",
    /// );
    /// assert!(!ctx.tier1_immutable.is_empty());
    /// assert!(ctx.tier3_volatile.is_empty());
    /// ```
    pub fn new(
        system_prompt: &str,
        tool_definitions: &str,
        agents_md: &str,
        preferences: &str,
    ) -> Self {
        tracing::debug!(
            system_prompt_len = system_prompt.len(),
            tool_definitions_len = tool_definitions.len(),
            agents_md_len = agents_md.len(),
            preferences_len = preferences.len(),
            "creating four-tier context"
        );

        Self {
            tier1_immutable: format!("{system_prompt}\n{tool_definitions}"),
            tier2_stable: format!("{agents_md}\n{preferences}"),
            tier3_volatile: VecDeque::with_capacity(VOLATILE_CAPACITY),
            tier4_ephemeral: String::new(),
        }
    }

    /// 计算当前缓存命中率。
    ///
    /// 命中率 = (Tier1 + Tier2 tokens) / 总 tokens。
    ///
    /// # Returns
    ///
    /// `0.0` ~ `1.0` 之间的浮点数；总 token 为 0 时返回 `0.0`。
    ///
    /// # Examples
    ///
    /// ```
    /// use kimix_core::cache_engine::Context;
    ///
    /// let ctx = Context::new("sys", "tools", "agents", "prefs");
    /// let rate = ctx.cache_hit_rate();
    /// assert!(rate > 0.0 && rate <= 1.0);
    /// ```
    pub fn cache_hit_rate(&self) -> f64 {
        let stable_tokens = self.stable_tokens();
        let total_tokens = self.total_tokens();
        if total_tokens == 0 {
            0.0
        } else {
            stable_tokens as f64 / total_tokens as f64
        }
    }

    /// 稳定部分 token 数（Tier1 + Tier2）。
    ///
    /// # Examples
    ///
    /// ```
    /// use kimix_core::cache_engine::Context;
    ///
    /// let ctx = Context::new("hello world", "tools", "", "");
    /// assert!(ctx.stable_tokens() >= 2);
    /// ```
    pub fn stable_tokens(&self) -> usize {
        tokenize(&self.tier1_immutable).len() + tokenize(&self.tier2_stable).len()
    }

    /// 总 token 数（四个分区之和）。
    ///
    /// # Examples
    ///
    /// ```
    /// use kimix_core::cache_engine::Context;
    ///
    /// let mut ctx = Context::new("sys", "tools", "", "");
    /// ctx.tier4_ephemeral = "user says hi".to_string();
    /// assert!(ctx.total_tokens() > ctx.stable_tokens());
    /// ```
    pub fn total_tokens(&self) -> usize {
        self.stable_tokens()
            + self
                .tier3_volatile
                .iter()
                .map(|s| tokenize(s).len())
                .sum::<usize>()
            + tokenize(&self.tier4_ephemeral).len()
    }

    /// 添加 Tier 3 内容（消费后即清策略）。
    ///
    /// 仅保留最近 [`MAX_VOLATILE_ENTRIES`] 条；超出时从队首弹出。
    ///
    /// # Arguments
    ///
    /// * `content` - 工具执行结果
    ///
    /// # Examples
    ///
    /// ```
    /// use kimix_core::cache_engine::Context;
    ///
    /// let mut ctx = Context::new("sys", "tools", "", "");
    /// ctx.push_volatile("result1".to_string());
    /// ctx.push_volatile("result2".to_string());
    /// assert_eq!(ctx.tier3_volatile.len(), 1);
    /// assert_eq!(ctx.tier3_volatile[0], "result2");
    /// ```
    pub fn push_volatile(&mut self, content: String) {
        tracing::debug!(
            content_len = content.len(),
            queue_len_before = self.tier3_volatile.len(),
            "pushing volatile tier3 content"
        );

        self.tier3_volatile.push_back(content);
        while self.tier3_volatile.len() > MAX_VOLATILE_ENTRIES {
            self.tier3_volatile.pop_front();
        }
    }

    /// 清除已消费的 Tier 3 内容。
    ///
    /// # Examples
    ///
    /// ```
    /// use kimix_core::cache_engine::Context;
    ///
    /// let mut ctx = Context::new("sys", "tools", "", "");
    /// ctx.push_volatile("result".to_string());
    /// ctx.clear_consumed_volatile();
    /// assert!(ctx.tier3_volatile.is_empty());
    /// ```
    pub fn clear_consumed_volatile(&mut self) {
        tracing::debug!(
            cleared = self.tier3_volatile.len(),
            "clearing consumed tier3 volatile content"
        );
        self.tier3_volatile.clear();
    }

    /// 组装完整上下文（用于 API 调用）。
    ///
    /// 分区之间以 `\n---\n` 分隔；Tier3 为空时省略该段。
    ///
    /// # Returns
    ///
    /// 按 Tier1 → Tier2 → Tier3 → Tier4 顺序拼接的完整上下文字符串。
    ///
    /// # Examples
    ///
    /// ```
    /// use kimix_core::cache_engine::Context;
    ///
    /// let mut ctx = Context::new("sys", "tools", "agents", "prefs");
    /// ctx.tier4_ephemeral = "hello".to_string();
    /// let assembled = ctx.assemble();
    /// assert!(assembled.contains("sys\ntools"));
    /// assert!(assembled.contains("---"));
    /// assert!(assembled.ends_with("hello"));
    /// ```
    pub fn assemble(&self) -> String {
        let mut result = String::with_capacity(
            self.tier1_immutable.len()
                + self.tier2_stable.len()
                + self.tier3_volatile.iter().map(|s| s.len()).sum::<usize>()
                + self.tier4_ephemeral.len()
                + ASSEMBLE_SEPARATOR_OVERHEAD,
        );

        result.push_str(&self.tier1_immutable);
        result.push_str("\n---\n");
        result.push_str(&self.tier2_stable);

        if !self.tier3_volatile.is_empty() {
            result.push_str("\n---\n");
            for item in &self.tier3_volatile {
                result.push_str(item);
                result.push('\n');
            }
        }

        result.push_str("\n---\n");
        result.push_str(&self.tier4_ephemeral);

        result
    }
}

/// 确定性序列化：确保相同语义内容产生相同的字节序列。
///
/// # 算法
///
/// 1. NFC Unicode 归一化
/// 2. 去除行尾空白
/// 3. 以 `\n` 重新拼接行
///
/// # Arguments
///
/// * `content` - 原始内容
///
/// # Returns
///
/// 归一化后的内容。
///
/// # Examples
///
/// ```
/// use kimix_core::cache_engine::canonicalize;
///
/// // NFD "e" + combining acute → NFC "é"
/// let nfd = "cafe\u{0301}";
/// let nfc = canonicalize(nfd);
/// assert_eq!(nfc, "caf\u{00e9}");
///
/// // 行尾空白被剥离
/// assert_eq!(canonicalize("hello  \nworld\t"), "hello\nworld");
/// ```
pub fn canonicalize(content: &str) -> String {
    content
        .nfc()
        .collect::<String>()
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

/// 计算 Cache Key（blake3 hash）。
///
/// 先对内容做 [`canonicalize`]，再计算 blake3，前缀带版本号。
///
/// # Arguments
///
/// * `content` - 原始内容
///
/// # Returns
///
/// 形如 `kimix:v0.1.11:<hex>` 的缓存键。
///
/// # Examples
///
/// ```
/// use kimix_core::cache_engine::cache_key;
///
/// let key = cache_key("hello world");
/// assert!(key.starts_with("kimix:v0.1.11:"));
/// // 相同内容 → 相同 key
/// assert_eq!(cache_key("hello world"), key);
/// // 行尾空白差异在归一化后消失
/// assert_eq!(cache_key("hello world  "), key);
/// ```
pub fn cache_key(content: &str) -> String {
    let normalized = canonicalize(content);
    let hash = blake3::hash(normalized.as_bytes());
    format!("{CACHE_KEY_VERSION}:{}", hash.to_hex())
}

/// 简单 tokenizer（基于空白分割）。
///
/// 生产环境应使用模型特定的 tokenizer；此处仅用于命中率估算。
fn tokenize(text: &str) -> Vec<&str> {
    text.split_whitespace().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_new() {
        let ctx = Context::new("system", "tools", "agents", "prefs");
        assert_eq!(ctx.tier1_immutable, "system\ntools");
        assert_eq!(ctx.tier2_stable, "agents\nprefs");
        assert!(ctx.tier3_volatile.is_empty());
        assert_eq!(ctx.tier4_ephemeral, "");
    }

    #[test]
    fn test_cache_hit_rate_calculation() {
        // ~4000 + ~3760 stable tokens，无 volatile/ephemeral → 命中率 ≈ 1.0
        let ctx = Context::new(
            &"a ".repeat(4000),
            &"b ".repeat(3760),
            "",
            "",
        );
        let rate = ctx.cache_hit_rate();
        assert!(
            (0.96..=1.0).contains(&rate),
            "expected high hit rate, got {rate}"
        );
        assert_eq!(ctx.stable_tokens(), 7760);
        assert_eq!(ctx.total_tokens(), 7760);
    }

    #[test]
    fn test_cache_hit_rate_with_ephemeral_lowers_rate() {
        let mut ctx = Context::new("sys tools", "defs", "agents", "prefs");
        let stable = ctx.stable_tokens();
        assert!(stable > 0);

        // 注入大量 ephemeral，拉低命中率
        ctx.tier4_ephemeral = "x ".repeat(stable * 4);
        let rate = ctx.cache_hit_rate();
        assert!(rate < 0.5, "expected lowered hit rate, got {rate}");
        assert!(rate > 0.0);
    }

    #[test]
    fn test_cache_hit_rate_empty_is_zero() {
        // 全部空白 → tokenize 结果为空
        let ctx = Context::new("", "", "", "");
        // "\n" split_whitespace → empty
        assert_eq!(ctx.total_tokens(), 0);
        assert_eq!(ctx.cache_hit_rate(), 0.0);
    }

    #[test]
    fn test_canonicalize_nfc() {
        // NFD: e + combining acute accent
        let input = "cafe\u{0301}";
        let expected = "caf\u{00e9}";
        assert_eq!(canonicalize(input), expected);
    }

    #[test]
    fn test_canonicalize_trailing_whitespace() {
        let input = "hello  \nworld\t\n  ";
        // lines() 会丢弃末尾空行语义：最后的 "  " 成为一行，trim_end 后为空串仍保留
        // "hello  \nworld\t\n  " → lines: ["hello  ", "world\t", "  "] → trim → ["hello", "world", ""]
        assert_eq!(canonicalize(input), "hello\nworld\n");
    }

    #[test]
    fn test_volatile_pruning() {
        let mut ctx = Context::new("sys", "tools", "", "");
        ctx.push_volatile("result1".to_string());
        ctx.push_volatile("result2".to_string());
        assert_eq!(ctx.tier3_volatile.len(), 1);
        assert_eq!(ctx.tier3_volatile[0], "result2");
    }

    #[test]
    fn test_clear_consumed_volatile() {
        let mut ctx = Context::new("sys", "tools", "", "");
        ctx.push_volatile("result".to_string());
        assert_eq!(ctx.tier3_volatile.len(), 1);
        ctx.clear_consumed_volatile();
        assert!(ctx.tier3_volatile.is_empty());
    }

    #[test]
    fn test_assemble_order_and_separators() {
        let mut ctx = Context::new("sys", "tools", "agents", "prefs");
        ctx.push_volatile("tool_out".to_string());
        ctx.tier4_ephemeral = "user_msg".to_string();

        let assembled = ctx.assemble();
        assert!(assembled.starts_with("sys\ntools\n---\nagents\nprefs"));
        assert!(assembled.contains("\n---\ntool_out\n"));
        assert!(assembled.ends_with("\n---\nuser_msg"));
    }

    #[test]
    fn test_assemble_skips_empty_volatile() {
        let mut ctx = Context::new("sys", "tools", "agents", "prefs");
        ctx.tier4_ephemeral = "hi".to_string();
        let assembled = ctx.assemble();
        // 无 Tier3 时只有 2 个分隔符（Tier1|2 与 Tier2|4）
        assert_eq!(assembled.matches("---").count(), 2);
        assert!(!assembled.contains("tool_out"));
        assert_eq!(
            assembled,
            "sys\ntools\n---\nagents\nprefs\n---\nhi"
        );
    }

    #[test]
    fn test_cache_key_deterministic() {
        let a = cache_key("hello world");
        let b = cache_key("hello world");
        let c = cache_key("hello world  "); // 行尾空白归一化后相同
        assert_eq!(a, b);
        assert_eq!(a, c);
        assert!(a.starts_with("kimix:v0.1.11:"));
        assert_ne!(cache_key("hello world"), cache_key("hello other"));
    }

    #[test]
    fn test_total_tokens_includes_all_tiers() {
        let mut ctx = Context::new("one two", "three", "four", "five");
        let base = ctx.total_tokens();
        ctx.push_volatile("six seven".to_string());
        ctx.tier4_ephemeral = "eight".to_string();
        assert_eq!(ctx.total_tokens(), base + 2 + 1);
    }
}
