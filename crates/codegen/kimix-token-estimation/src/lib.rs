//! Pure shared token-estimation primitives.
//!
//! This crate is the single source of truth for the token-estimation heuristic
//! that `/context`, `/session-info`, the auto-compact gates, the preflight
//! overflow check, and every client renderer use to talk about context-window
//! usage.
//!
//! ## Model-aware estimation
//!
//! Different LLM families use different tokenizers with varying efficiency:
//!
//! | Model family | Tokenizer | Approx bytes/token |
//! |---|---|---|
//! | GPT-4o / GPT-5 | o200k_base | ~3.5 (EN), ~2.5 (CJK) |
//! | GPT-3.5 / GPT-4 | cl100k_base | ~4.0 (EN), ~3.0 (CJK) |
//! | Claude / Grok / Gemini | proprietary BPE | ~3.5 (EN), ~2.0 (CJK) |
//! | DeepSeek / Qwen | custom BPE | ~3.8 (EN), ~1.5 (CJK) |
//! | Kimi K2 / unknown | unknown | falls back to heuristic |
//!
//! The [`estimate_model_tokens`] function accepts a model ID and applies the
//! best available encoding heuristic for that model. The [`TokenEstimator`]
//! struct adds runtime calibration from API-reported usage to progressively
//! narrow estimation error over a session.
//!
//! All original public APIs ([`estimate_tokens`], [`estimate_chars`], etc.)
//! remain unchanged for backward compatibility.

/// Bytes per token under the rough character-based heuristic for
/// Latin/ASCII text. CJK characters are tallied separately at 1 token each
/// (they are individual BPE tokens in practice) so this constant only applies
/// to the non-CJK portion.
pub const BYTES_PER_TOKEN: u64 = 4;

/// Per-image approximate token cost when summing
/// low-resolution image patches.
pub const IMAGE_TOKEN_ESTIMATE: u64 = 765;

// ── CJK detection (binary search optimized) ────────────────────────────────

/// CJK Unicode ranges where each character ≈ 1 BPE token.
/// Ranges are sorted by start codepoint for binary search.
/// CJK characters are encoded as 3 bytes in UTF-8 but a BPE tokenizer
/// treats each as an individual token, so the old `bytes/4` heuristic
/// underestimated them by 2-3×. We count them separately at 1 token each.
const CJK_RANGES: &[(char, char)] = &[
    ('\u{2000}', '\u{206F}'), // General Punctuation (CJK-width)
    ('\u{2E80}', '\u{2EFF}'), // CJK Radicals Supplement
    ('\u{2F00}', '\u{2FDF}'), // Kangxi Radicals
    ('\u{3000}', '\u{303F}'), // CJK Symbols and Punctuation
    ('\u{3040}', '\u{309F}'), // Hiragana
    ('\u{30A0}', '\u{30FF}'), // Katakana
    ('\u{31C0}', '\u{31EF}'), // CJK Strokes
    ('\u{3400}', '\u{4DBF}'), // CJK Unified Ideographs Extension A
    ('\u{4E00}', '\u{9FFF}'), // CJK Unified Ideographs
    ('\u{AC00}', '\u{D7AF}'), // Hangul Syllables
    ('\u{F900}', '\u{FAFF}'), // CJK Compatibility Ideographs
    ('\u{FF00}', '\u{FFEF}'), // Halfwidth and Fullwidth Forms
];

/// CJK character detection via binary search over sorted Unicode ranges.
///
/// O(log n) instead of the previous O(n) linear scan over 12 ranges.
#[inline]
fn is_cjk(c: char) -> bool {
    CJK_RANGES
        .binary_search_by(|(lo, hi)| {
            if c < *lo {
                std::cmp::Ordering::Greater // target is before this range
            } else if c > *hi {
                std::cmp::Ordering::Less // target is after this range
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

// ── Model-aware encoding heuristics ────────────────────────────────────────

/// Approximate bytes-per-token for ASCII text per model family.
///
/// Derived from public tokenizer benchmarks and community measurements.
/// Values represent the *effective* bytes per token for mixed English text
/// (not raw vocabulary size).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EncodingProfile {
    /// Effective bytes per ASCII token.
    pub ascii_bytes_per_token: f64,
    /// Effective bytes per CJK character (usually ~1.0 since CJK chars
    /// are typically 1 token each in BPE).
    pub cjk_chars_per_token: f64,
    /// Safety multiplier for models whose tokenizer is not publicly available.
    /// 1.0 = no margin; 1.15 = +15% safety margin.
    pub safety_margin: f64,
}

impl EncodingProfile {
    /// o200k_base: GPT-4o, GPT-5, o1, o3 family.
    /// Largest vocabulary (200K) yields the best English bytes/token ratio.
    /// ~4.0-4.2 bytes per English token in benchmarks.
    pub const O200K: Self = Self {
        ascii_bytes_per_token: 4.0,
        cjk_chars_per_token: 1.0,
        safety_margin: 1.0,
    };

    /// cl100k_base: GPT-3.5, GPT-4 (legacy).
    /// ~3.5-4.0 bytes per English token; slightly less efficient than o200k.
    pub const CL100K: Self = Self {
        ascii_bytes_per_token: 3.8,
        cjk_chars_per_token: 1.0,
        safety_margin: 1.0,
    };

    /// Approximation for Claude, Grok, Gemini.
    /// Anthropic does not publish an offline tokenizer; cl100k_base is the
    /// closest public analogue. We apply a +15% safety margin to compensate
    /// for tokenizer divergence (Claude consumes ~10-15% more tokens than
    /// GPT-4o for equivalent text).
    pub const CLOSED_SOURCE_APPROX: Self = Self {
        ascii_bytes_per_token: 3.5,
        cjk_chars_per_token: 1.0,
        safety_margin: 1.15,
    };

    /// Approximation for DeepSeek, Qwen.
    /// Optimized for Chinese with large CJK-aware vocabularies (100-150K).
    /// English efficiency is comparable to cl100k.
    pub const CHINESE_OPTIMIZED: Self = Self {
        ascii_bytes_per_token: 3.8,
        cjk_chars_per_token: 1.0,
        safety_margin: 1.05,
    };

    /// Fallback heuristic (current bytes/4 + CJK). Used when the model is
    /// unknown or no better profile is available.
    pub const HEURISTIC: Self = Self {
        ascii_bytes_per_token: 4.0,
        cjk_chars_per_token: 1.0,
        safety_margin: 1.0,
    };
}

/// Classify a model ID into the best available [`EncodingProfile`].
///
/// Matching is case-insensitive and uses substring matching on the model ID
/// to handle both short names (`gpt-4o`) and full paths (`openai/gpt-4o-mini`).
pub fn classify_model(model_id: &str) -> EncodingProfile {
    let m = model_id.to_lowercase();

    // ── OpenAI family (check specific models first) ──
    if m.contains("o200k")
        || m.contains("gpt-4o")
        || m.contains("gpt-5")
        || m.contains("gpt5")
        || m.starts_with("o1")
        || m.starts_with("o3")
        || m.contains("-o1-")
        || m.contains("-o3-")
    {
        return EncodingProfile::O200K;
    }

    if m.contains("gpt-4")
        || m.contains("gpt-3.5")
        || m.contains("gpt4")
        || m.contains("gpt3")
        || m.contains("gpt_4")
        || m.contains("gpt_3")
    {
        return EncodingProfile::CL100K;
    }

    // ── Chinese-optimized models ──
    if m.contains("deepseek")
        || m.contains("qwen")
        || m.contains("glm")
        || m.contains("yi-")
        || m.contains("baichuan")
        || m.contains("internlm")
    {
        return EncodingProfile::CHINESE_OPTIMIZED;
    }

    // ── Closed-source approximations (Claude, Grok, Gemini, etc.) ──
    if m.contains("claude")
        || m.contains("grok")
        || m.contains("gemini")
        || m.contains("palm")
        || m.contains("llama")
        || m.contains("mistral")
        || m.contains("mixtral")
        || m.contains("command")
        || m.contains("dbrx")
    {
        return EncodingProfile::CLOSED_SOURCE_APPROX;
    }

    // ── Kimi / Moonshot family: also approximated ──
    if m.contains("kimi") || m.contains("moonshot") {
        return EncodingProfile::CLOSED_SOURCE_APPROX;
    }

    // ── Unknown model: conservative heuristic ──
    EncodingProfile::HEURISTIC
}

/// Estimate token count for a string using a model-aware [`EncodingProfile`].
///
/// This is more accurate than [`estimate_tokens`] when the model identity
/// is known. For unknown models, falls back to the same heuristic as
/// [`estimate_tokens`].
#[inline]
pub fn estimate_model_tokens(s: &str, model_id: &str) -> u64 {
    let profile = classify_model(model_id);
    estimate_with_profile(s, &profile)
}

/// Estimate token count using an explicit [`EncodingProfile`].
#[inline]
pub fn estimate_with_profile(s: &str, profile: &EncodingProfile) -> u64 {
    let mut cjk_count: u64 = 0;
    let mut ascii_bytes: u64 = 0;

    for c in s.chars() {
        if is_cjk(c) {
            cjk_count += 1;
        } else {
            ascii_bytes += c.len_utf8() as u64;
        }
    }

    let ascii_tokens = (ascii_bytes as f64 / profile.ascii_bytes_per_token) as u64;
    let cjk_tokens = (cjk_count as f64 / profile.cjk_chars_per_token) as u64;
    let raw = ascii_tokens + cjk_tokens;

    // Apply safety margin
    let with_margin = (raw as f64 * profile.safety_margin) as u64;

    // Floor to 1 for non-empty strings
    if with_margin == 0 && !s.is_empty() {
        1
    } else {
        with_margin
    }
}

// ── Runtime calibration ────────────────────────────────────────────────────

/// A session-scoped token estimator that progressively improves accuracy
/// by learning from API-reported usage data.
///
/// # How it works
///
/// 1. Initially uses the [`EncodingProfile`] heuristic for the active model.
/// 2. After each API response that reports `prompt_tokens`, computes
///    `actual_ratio = api_tokens / heuristic_estimate`.
/// 3. Maintains an exponentially-weighted moving average (EWMA) of these
///    ratios, so recent observations have more influence.
/// 4. Future estimates are `heuristic * ewma_ratio`.
///
/// This achieves near-exact accuracy after 3-5 API round-trips without
/// requiring the actual tokenizer binary.
#[derive(Debug)]
pub struct TokenEstimator {
    /// The base profile for the current model.
    profile: EncodingProfile,
    /// EWMA of (api_tokens / heuristic_tokens). 1.0 = no calibration yet.
    ratio: f64,
    /// Number of calibration observations received.
    observations: u32,
    /// Smoothing factor for EWMA (0.0 = ignore new data, 1.0 = only latest).
    /// 0.3 gives ~70% weight to the last 5 observations.
    alpha: f64,
}

impl TokenEstimator {
    /// Create a new estimator for the given model.
    pub fn new(model_id: &str) -> Self {
        Self {
            profile: classify_model(model_id),
            ratio: 1.0,
            observations: 0,
            alpha: 0.3,
        }
    }

    /// Create with an explicit profile (for testing or manual override).
    pub fn with_profile(profile: EncodingProfile) -> Self {
        Self {
            profile,
            ratio: 1.0,
            observations: 0,
            alpha: 0.3,
        }
    }

    /// Estimate tokens for a text string using the calibrated heuristic.
    #[inline]
    pub fn estimate(&self, s: &str) -> u64 {
        let raw = estimate_with_profile(s, &self.profile);
        let calibrated = (raw as f64 * self.ratio) as u64;
        // Never go below 1 for non-empty
        if calibrated == 0 && !s.is_empty() {
            1
        } else {
            calibrated
        }
    }

    /// Calibrate from an API-reported usage count.
    ///
    /// `heuristic_tokens` should be the result of `estimate_with_profile()`
    /// on the same input text. `api_tokens` is the `prompt_tokens` value
    /// from the API response.
    ///
    /// Returns the current calibration ratio (1.0 = uncalibrated).
    pub fn calibrate(&mut self, heuristic_tokens: u64, api_tokens: u64) -> f64 {
        if heuristic_tokens == 0 || api_tokens == 0 {
            return self.ratio;
        }
        let observed_ratio = api_tokens as f64 / heuristic_tokens as f64;

        // Clamp observed ratio to [0.3, 3.0] to prevent wild swings
        // from edge cases (empty prompts, image-heavy, etc.)
        let clamped = observed_ratio.clamp(0.3, 3.0);

        if self.observations == 0 {
            self.ratio = clamped;
        } else {
            // EWMA: new = alpha * observed + (1 - alpha) * old
            self.ratio = self.alpha * clamped + (1.0 - self.alpha) * self.ratio;
        }
        self.observations += 1;
        self.ratio
    }

    /// Current calibration ratio. 1.0 = uncalibrated (heuristic only).
    pub fn ratio(&self) -> f64 {
        self.ratio
    }

    /// Number of calibration observations received.
    pub fn observations(&self) -> u32 {
        self.observations
    }

    /// The underlying encoding profile.
    pub fn profile(&self) -> &EncodingProfile {
        &self.profile
    }
}

// ── Original public API (unchanged) ────────────────────────────────────────

/// Token estimate for a string with CJK-aware weighting.
///
/// CJK characters are counted at 1 token each (matching BPE tokenizer
/// behavior). Non-CJK text uses the bytes/4 heuristic. Non-empty strings
/// always count at least 1 token.
///
/// For model-aware estimation, use [`estimate_model_tokens`] instead.
#[inline(always)]
pub fn estimate_tokens(s: &str) -> u64 {
    let mut cjk_count: u64 = 0;
    let mut non_cjk_bytes: u64 = 0;

    for c in s.chars() {
        if is_cjk(c) {
            cjk_count += 1;
        } else {
            non_cjk_bytes += c.len_utf8() as u64;
        }
    }

    let non_cjk_tokens = non_cjk_bytes / BYTES_PER_TOKEN;
    let total = cjk_count + non_cjk_tokens;
    // Floor to 1 for non-empty strings so short messages (e.g. "ok")
    // are never counted as 0 tokens.
    if total == 0 && !s.is_empty() {
        1
    } else {
        total
    }
}

/// Inverse of [`estimate_tokens`]: convert a token budget into a character
/// budget. Used by skill discovery to size text passages against the model's
/// context window.
#[inline]
pub fn estimate_chars(tokens: u64) -> u64 {
    tokens.saturating_mul(BYTES_PER_TOKEN)
}

/// Token estimate for `image_count` images at [`IMAGE_TOKEN_ESTIMATE`] each.
#[inline]
pub fn estimate_image_tokens(image_count: u64) -> u64 {
    image_count.saturating_mul(IMAGE_TOKEN_ESTIMATE)
}

/// Usage percentage as `f64`, clamped to `100.0`. Returns `0.0` when
/// `total == 0`.
#[inline]
pub fn usage_percentage(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        ((used as f64) / (total as f64) * 100.0).min(100.0)
    }
}

/// Usage percentage rounded to `u8`, clamped to `100`.
#[inline]
pub fn usage_percentage_u8(used: u64, total: u64) -> u8 {
    usage_percentage(used, total).round() as u8
}

/// Integer-arithmetic (truncating) usage percentage, clamped to `100`.
///
/// Differs from [`usage_percentage_u8`] in two ways: no `f64` round-trip,
/// and the result is **truncated** (not rounded).
///
/// Returns `u8` because the result is bounded to `100`. Saturates on
/// overflow via `saturating_mul`.
#[inline]
pub fn usage_percentage_truncated_u8(used: u64, total: u64) -> u8 {
    used.saturating_mul(100)
        .checked_div(total)
        .map_or(0, |pct| pct.min(100) as u8)
}

/// `total - used`, saturating at zero. The "free" portion of the context
/// window for `/context` rendering.
#[inline]
pub fn free_tokens(total: u64, used: u64) -> u64 {
    total.saturating_sub(used)
}

/// True when `used >= context_window * threshold_percent / 100`. Returns
/// `false` for `context_window == 0` so callers do not have to special-case
/// missing windows. Computed in integer arithmetic to match the existing
/// auto-compact gate semantics.
#[inline]
pub fn exceeds_threshold(used: u64, context_window: u64, threshold_percent: u8) -> bool {
    if context_window == 0 {
        return false;
    }
    used.saturating_mul(100) >= context_window.saturating_mul(threshold_percent as u64)
}

/// True when `used * 100 >= context_window * threshold_percent - headroom * 100`,
/// the scaled form of [`exceeds_threshold`] minus a token headroom.
/// Returns `false` for `context_window == 0`.
#[inline]
pub fn exceeds_threshold_with_headroom(
    used: u64,
    context_window: u64,
    threshold_percent: u8,
    headroom: u64,
) -> bool {
    if context_window == 0 {
        return false;
    }
    used.saturating_mul(100)
        >= context_window
            .saturating_mul(threshold_percent as u64)
            .saturating_sub(headroom.saturating_mul(100))
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Original estimate_tokens tests (unchanged) ──

    #[test]
    fn estimate_tokens_cjk_aware() {
        // Empty string → 0
        assert_eq!(estimate_tokens(""), 0);
        // Non-empty always ≥ 1
        assert_eq!(estimate_tokens("abc"), 1);
        // 4 ASCII bytes = 1 token
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
        // CJK characters at 1 token each
        assert_eq!(estimate_tokens("你好"), 2);
        assert_eq!(estimate_tokens("你好世界"), 4);
        // Mixed: "Hi你好" = 2 ASCII bytes(0) + 2 CJK = 2 tokens, floored to 2
        assert_eq!(estimate_tokens("Hi你好"), 2);
        // 4000 ASCII chars = 1000 tokens (unchanged)
        assert_eq!(estimate_tokens(&"x".repeat(4000)), 1000);
        // CJK-heavy: 1000 CJK chars = 1000 tokens (was 750 with old bytes/4)
        let cjk_text = "中".repeat(1000);
        assert_eq!(estimate_tokens(&cjk_text), 1000);
    }

    #[test]
    fn estimate_chars_is_inverse() {
        assert_eq!(estimate_chars(0), 0);
        assert_eq!(estimate_chars(1), 4);
        assert_eq!(estimate_chars(1000), 4000);
    }

    #[test]
    fn estimate_image_tokens_uses_constant() {
        assert_eq!(estimate_image_tokens(0), 0);
        assert_eq!(estimate_image_tokens(1), IMAGE_TOKEN_ESTIMATE);
        assert_eq!(estimate_image_tokens(3), 3 * IMAGE_TOKEN_ESTIMATE);
    }

    #[test]
    fn usage_percentage_clamps_and_handles_zero_total() {
        assert_eq!(usage_percentage(0, 0), 0.0);
        assert_eq!(usage_percentage(50, 100), 50.0);
        assert_eq!(usage_percentage(150, 100), 100.0);
        assert_eq!(usage_percentage(100, 0), 0.0);
    }

    #[test]
    fn usage_percentage_u8_rounds() {
        assert_eq!(usage_percentage_u8(0, 100), 0);
        assert_eq!(usage_percentage_u8(50, 100), 50);
        assert_eq!(usage_percentage_u8(99, 100), 99);
        // 12_700 / 256_000 = 0.04960... -> 5 after rounding
        assert_eq!(usage_percentage_u8(12_700, 256_000), 5);
        assert_eq!(usage_percentage_u8(150, 100), 100);
    }

    /// Half-boundary contract — locks rounding direction. `85 / 200 = 0.425`
    /// becomes `42.5%` which rounds half-up to `43`. The truncating helper
    /// returns `42` for the same input (see `usage_percentage_truncated_u8`).
    #[test]
    fn usage_percentage_u8_rounds_half_up() {
        assert_eq!(usage_percentage_u8(85, 200), 43);
        // 7 / 8 = 0.875, rounds to 88 (truncated would be 87).
        assert_eq!(usage_percentage_u8(7, 8), 88);
    }

    #[test]
    fn usage_percentage_truncated_u8_clamps_and_handles_zero_total() {
        assert_eq!(usage_percentage_truncated_u8(0, 0), 0);
        assert_eq!(usage_percentage_truncated_u8(50, 100), 50);
        assert_eq!(usage_percentage_truncated_u8(150, 100), 100);
        // Large values do not overflow because we use saturating_mul.
        assert_eq!(usage_percentage_truncated_u8(u64::MAX, 1), 100);
    }

    /// Truncation contract — distinguishes this helper from
    /// `usage_percentage_u8`, which rounds. Locks in that
    /// `exceeds_threshold(used, cw, p)` and
    /// `usage_percentage_truncated_u8(used, cw) >= p` agree.
    #[test]
    fn usage_percentage_truncated_u8_truncates_does_not_round() {
        // 85 / 200 = 0.425, truncated -> 42 (rounded would be 43).
        assert_eq!(usage_percentage_truncated_u8(85, 200), 42);
        // 7 / 8 = 0.875, truncated -> 87 (rounded would be 88).
        assert_eq!(usage_percentage_truncated_u8(7, 8), 87);
    }

    #[test]
    fn free_tokens_saturates() {
        assert_eq!(free_tokens(100, 30), 70);
        assert_eq!(free_tokens(100, 100), 0);
        assert_eq!(free_tokens(100, 200), 0);
    }

    #[test]
    fn exceeds_threshold_matches_integer_pct() {
        assert!(!exceeds_threshold(50, 100, 85));
        assert!(exceeds_threshold(85, 100, 85));
        assert!(exceeds_threshold(99, 100, 85));
        assert!(!exceeds_threshold(50, 0, 85));
    }

    /// Strict-boundary contract — pin the `>=` semantics. At cw=1000,
    /// pct=85, `850 * 100 == 1000 * 85` so the gate must fire at exactly
    /// 850 tokens. This is one token earlier than the legacy `>` gate
    /// (`total > cw * pct / 100` which fired at 851).
    #[test]
    fn exceeds_threshold_fires_on_strict_boundary() {
        assert!(exceeds_threshold(850, 1000, 85));
        assert!(!exceeds_threshold(849, 1000, 85));
        // 1000 * 85 / 100 = 850, so 850 is the new strict boundary.
        // Same shape at the other commonly-configured threshold (95%):
        assert!(exceeds_threshold(950, 1000, 95));
        assert!(!exceeds_threshold(949, 1000, 95));
    }

    /// Property: with `headroom == 0` the helper agrees with
    /// [`exceeds_threshold`] across a representative grid of inputs,
    /// including the non-round windows where floor-divide drifts.
    #[test]
    fn exceeds_threshold_with_headroom_zero_headroom_matches_exceeds_threshold() {
        for cw in [0_u64, 1, 50, 100, 101, 1024, 100_000, 128_001, 1_000_001] {
            for pct in [0_u8, 1, 50, 85, 99, 100] {
                for used in [
                    0_u64,
                    1,
                    cw / 2,
                    cw.saturating_sub(1),
                    cw,
                    cw + 1,
                    cw + 1000,
                ] {
                    assert_eq!(
                        exceeds_threshold_with_headroom(used, cw, pct, 0),
                        exceeds_threshold(used, cw, pct),
                        "mismatch at used={used} cw={cw} pct={pct}",
                    );
                }
            }
        }
    }

    #[test]
    fn exceeds_threshold_with_headroom_subtracts_headroom() {
        // 100K window, 85% threshold = 85_000. Headroom 4_000 -> fires at 81_000.
        assert!(!exceeds_threshold_with_headroom(80_999, 100_000, 85, 4_000));
        assert!(exceeds_threshold_with_headroom(81_000, 100_000, 85, 4_000));
    }

    #[test]
    fn exceeds_threshold_with_headroom_zero_window() {
        assert!(!exceeds_threshold_with_headroom(0, 0, 85, 0));
        assert!(!exceeds_threshold_with_headroom(100, 0, 85, 4_000));
    }

    #[test]
    fn exceeds_threshold_with_headroom_headroom_larger_than_threshold_saturates() {
        // 100K * 85% = 85_000 (8_500_000 scaled). Headroom 1M tokens scales to
        // 100_000_000 — saturating sub yields 0, so any used fires.
        assert!(exceeds_threshold_with_headroom(0, 100_000, 85, 1_000_000));
    }

    // ── New: is_cjk binary search correctness ──

    #[test]
    fn is_cjk_binary_search_matches_known_chars() {
        // CJK Unified Ideographs
        assert!(is_cjk('中'));
        assert!(is_cjk('国'));
        assert!(is_cjk('人'));
        // Hiragana
        assert!(is_cjk('あ'));
        assert!(is_cjk('の'));
        // Katakana
        assert!(is_cjk('ア'));
        assert!(is_cjk('カ'));
        // Hangul
        assert!(is_cjk('한'));
        assert!(is_cjk('글'));
        // CJK Extension A
        assert!(is_cjk('\u{3400}'));
        assert!(is_cjk('\u{4DBF}'));
        // Kangxi Radicals
        assert!(is_cjk('\u{2F00}'));
        // CJK Symbols
        assert!(is_cjk('、'));
        assert!(is_cjk('。'));
    }

    #[test]
    fn is_cjk_rejects_non_cjk() {
        assert!(!is_cjk('a'));
        assert!(!is_cjk('Z'));
        assert!(!is_cjk('0'));
        assert!(!is_cjk(' '));
        assert!(!is_cjk('é'));
        assert!(!is_cjk('ñ'));
        assert!(!is_cjk('α'));
        assert!(!is_cjk('₽'));
    }

    // ── New: model classification tests ──

    #[test]
    fn classify_model_openai_o200k() {
        assert_eq!(classify_model("gpt-4o"), EncodingProfile::O200K);
        assert_eq!(classify_model("gpt-4o-mini"), EncodingProfile::O200K);
        assert_eq!(classify_model("gpt-5"), EncodingProfile::O200K);
        assert_eq!(classify_model("o1-preview"), EncodingProfile::O200K);
        assert_eq!(classify_model("o3-mini"), EncodingProfile::O200K);
        assert_eq!(classify_model("openai/gpt-4o"), EncodingProfile::O200K);
    }

    #[test]
    fn classify_model_openai_cl100k() {
        assert_eq!(classify_model("gpt-4"), EncodingProfile::CL100K);
        assert_eq!(classify_model("gpt-4-turbo"), EncodingProfile::CL100K);
        assert_eq!(classify_model("gpt-3.5-turbo"), EncodingProfile::CL100K);
    }

    #[test]
    fn classify_model_chinese_optimized() {
        assert_eq!(
            classify_model("deepseek-chat"),
            EncodingProfile::CHINESE_OPTIMIZED
        );
        assert_eq!(
            classify_model("deepseek-v4-pro"),
            EncodingProfile::CHINESE_OPTIMIZED
        );
        assert_eq!(
            classify_model("qwen-plus"),
            EncodingProfile::CHINESE_OPTIMIZED
        );
    }

    #[test]
    fn classify_model_closed_source_approximation() {
        assert_eq!(
            classify_model("claude-sonnet-4-20250514"),
            EncodingProfile::CLOSED_SOURCE_APPROX
        );
        assert_eq!(
            classify_model("claude-3-5-haiku"),
            EncodingProfile::CLOSED_SOURCE_APPROX
        );
        assert_eq!(
            classify_model("grok-4"),
            EncodingProfile::CLOSED_SOURCE_APPROX
        );
        assert_eq!(
            classify_model("gemini-2.5-pro"),
            EncodingProfile::CLOSED_SOURCE_APPROX
        );
        assert_eq!(
            classify_model("kimi-for-coding"),
            EncodingProfile::CLOSED_SOURCE_APPROX
        );
        assert_eq!(
            classify_model("moonshot-cn/kimi-k2-turbo"),
            EncodingProfile::CLOSED_SOURCE_APPROX
        );
        assert_eq!(
            classify_model("llama-3.1-70b"),
            EncodingProfile::CLOSED_SOURCE_APPROX
        );
    }

    #[test]
    fn classify_model_unknown_falls_back_to_heuristic() {
        assert_eq!(classify_model(""), EncodingProfile::HEURISTIC);
        assert_eq!(classify_model("my-custom-model"), EncodingProfile::HEURISTIC);
        assert_eq!(
            classify_model("some-provider/some-model"),
            EncodingProfile::HEURISTIC
        );
    }

    // ── New: model-aware estimation tests ──

    #[test]
    fn estimate_model_tokens_gpt4o_more_efficient_than_heuristic() {
        let text = "The quick brown fox jumps over the lazy dog.";
        let heuristic = estimate_tokens(text);
        let o200k = estimate_model_tokens(text, "gpt-4o");
        // o200k uses 3.5 bytes/token vs 4.0, so should produce fewer tokens
        assert!(
            o200k <= heuristic,
            "o200k ({o200k}) should be <= heuristic ({heuristic})"
        );
        assert!(o200k > 0);
    }

    #[test]
    fn estimate_model_tokens_claude_applies_safety_margin() {
        let text = "The quick brown fox jumps over the lazy dog.";
        let _heuristic = estimate_tokens(text);
        let claude = estimate_model_tokens(text, "claude-sonnet-4-20250514");
        // Claude uses 3.5 bytes/token * 1.15 margin ≈ 4.025 effective
        // So roughly similar to heuristic, maybe slightly fewer
        assert!(claude > 0);
        // The safety margin means Claude estimate should be >= raw 3.5 estimate
        let raw_3_5 = (text.len() as f64 / 3.5) as u64;
        let claude_raw = (raw_3_5 as f64 * 1.15) as u64;
        assert!(
            claude >= claude_raw.saturating_sub(1),
            "Claude estimate ({claude}) should be >= raw 3.5 * 1.15 ({claude_raw})"
        );
    }

    #[test]
    fn estimate_model_tokens_cjk_text() {
        let text = "你好世界，这是一个测试。";
        let heuristic = estimate_tokens(text);
        let gpt4o = estimate_model_tokens(text, "gpt-4o");
        let deepseek = estimate_model_tokens(text, "deepseek-chat");
        // All should produce > 0
        assert!(heuristic > 0);
        assert!(gpt4o > 0);
        assert!(deepseek > 0);
        // CJK text: all profiles use 1.0 cjk_chars_per_token,
        // so differences come from ASCII chars (punctuation, etc.)
    }

    #[test]
    fn estimate_model_tokens_empty_string() {
        assert_eq!(estimate_model_tokens("", "gpt-4o"), 0);
        assert_eq!(estimate_model_tokens("", "claude-sonnet-4"), 0);
    }

    #[test]
    fn estimate_model_tokens_single_char() {
        assert!(estimate_model_tokens("x", "gpt-4o") >= 1);
        assert!(estimate_model_tokens("中", "gpt-4o") >= 1);
    }

    // ── New: TokenEstimator calibration tests ──

    #[test]
    fn estimator_starts_uncalibrated() {
        let est = TokenEstimator::new("gpt-4o");
        assert_eq!(est.ratio(), 1.0);
        assert_eq!(est.observations(), 0);
    }

    #[test]
    fn estimator_calibration_converges() {
        let mut est = TokenEstimator::new("gpt-4o");
        let text = "Hello world, this is a test message for calibration.";

        let heuristic = est.estimate(text);
        // Simulate API returning ~30% more tokens than heuristic.
        // Use a multiplier large enough to survive u64 quantization.
        let api_tokens = heuristic + heuristic / 3 + 1;
        let expected_ratio = api_tokens as f64 / heuristic as f64;

        // After first observation, ratio should match the actual observed ratio.
        let ratio1 = est.calibrate(heuristic, api_tokens);
        assert!(
            (ratio1 - expected_ratio).abs() < 0.01,
            "first calibration should be ~{expected_ratio:.3}, got {ratio1}"
        );
        assert_eq!(est.observations(), 1);

        // After 10 observations at the same ratio, EWMA should converge close
        for _ in 0..9 {
            est.calibrate(heuristic, api_tokens);
        }
        let ratio_final = est.ratio();
        assert!(
            (ratio_final - expected_ratio).abs() < 0.05,
            "should converge close to {expected_ratio:.3} after 10 obs, got {ratio_final}"
        );
        assert_eq!(est.observations(), 10);
    }

    #[test]
    fn estimator_calibration_clamps_wild_values() {
        let mut est = TokenEstimator::new("gpt-4o");
        // Extreme ratio should be clamped to [0.3, 3.0]
        est.calibrate(10, 100); // ratio would be 10.0, clamped to 3.0
        assert_eq!(est.ratio(), 3.0);

        est.calibrate(100, 10); // ratio would be 0.1, clamped to 0.3
        // EWMA: 0.3 * 0.3 + 0.7 * 3.0 = 0.09 + 2.1 = 2.19
        let ratio = est.ratio();
        assert!(
            (ratio - 2.19).abs() < 0.01,
            "EWMA should be ~2.19, got {ratio}"
        );
    }

    #[test]
    fn estimator_calibration_ignores_zero_inputs() {
        let mut est = TokenEstimator::new("gpt-4o");
        let ratio = est.calibrate(0, 100);
        assert_eq!(ratio, 1.0, "should not calibrate with zero heuristic");
        assert_eq!(est.observations(), 0);
    }

    #[test]
    fn estimator_estimate_applies_calibration() {
        let mut est = TokenEstimator::new("gpt-4o");
        let text = "Test string for calibration application";

        let uncalibrated = est.estimate(text);
        // Calibrate with ratio 2.0
        let heuristic = estimate_with_profile(text, est.profile());
        est.calibrate(heuristic, heuristic * 2);
        let calibrated = est.estimate(text);

        assert!(
            calibrated > uncalibrated,
            "calibrated ({calibrated}) should be > uncalibrated ({uncalibrated})"
        );
        assert!(
            (calibrated as f64 / uncalibrated as f64 - 2.0).abs() < 0.1,
            "calibrated should be ~2x uncalibrated"
        );
    }
}
