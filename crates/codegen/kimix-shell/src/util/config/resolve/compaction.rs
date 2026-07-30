/// Default auto-compact threshold (% of context window) when no source sets it.
pub const DEFAULT_AUTO_COMPACT_THRESHOLD_PERCENT: u8 = 75;

/// Default effective context cap (tokens) for compaction trigger math.
/// Aligns with NoLiMa quality cliff; set 0 via config/env to disable.
pub const DEFAULT_MAX_EFFECTIVE_CONTEXT_TOKENS: u32 = 200_000;

/// Env override for `max_effective_context_tokens`. Parsed as `u32`.
/// `0` disables the cap (use full model context window).
pub(crate) const ENV_MAX_EFFECTIVE_CONTEXT_TOKENS: &str = "KIMIX_MAX_EFFECTIVE_CONTEXT_TOKENS";

/// Env-var override for `auto_compact_threshold_percent`. Parsed as `u8`;
/// out-of-range or unparseable values are ignored.
pub(crate) const ENV_AUTO_COMPACT_THRESHOLD_PERCENT: &str = "KIMIX_AUTO_COMPACT_THRESHOLD_PERCENT";

/// Resolve auto-compact threshold percent (0-100) for the given model.
///
/// Two scopes (per-model and global) across two tiers (user TOML and
/// remote settings). User-tier always wins over remote; within a tier, per-model
/// wins over global. Env var sits on top as a per-process override.
///
/// Precedence (highest first):
///   1. env `KIMIX_AUTO_COMPACT_THRESHOLD_PERCENT`
///   2. user TOML `[model.<id>].auto_compact_threshold_percent`
///      (read from `cfg.config_models`; the effective merge of user +
///      managed `[model.<id>]` sections)
///   3. user TOML `[session].auto_compact_threshold_percent`
///      (read from `cfg.session.auto_compact_threshold_percent: Option<u8>`)
///   4. remote settings per-model `ModelInfo.auto_compact_threshold_percent`
///      (populated from `kimix_models[i].auto_compact_threshold_percent`;
///      intentionally NOT collapsed via `ConfigModelOverride::apply` so the
///      user-vs-GB per-model distinction is preserved)
///   5. remote settings global `RemoteSettings.auto_compact_threshold_percent`
///      (populated from `kimix_settings.auto_compact_threshold_percent`)
///   6. default `DEFAULT_AUTO_COMPACT_THRESHOLD_PERCENT` (75)
///
/// Values outside `0..=100` from the env var are ignored with a debug log and
/// the resolver falls through to the next tier. TOML/remote fields are typed
/// `u8` and so naturally constrained.
pub fn resolve_auto_compact_threshold_percent(
    cfg: &crate::agent::config::Config,
    model_id: &str,
    model: Option<&crate::agent::config::ModelInfo>,
) -> u8 {
    resolve_auto_compact_threshold_percent_from_tiers(
        cfg.config_models
            .get(model_id)
            .and_then(|m| m.auto_compact_threshold_percent),
        cfg.session.auto_compact_threshold_percent,
        model.and_then(|m| m.auto_compact_threshold_percent),
        cfg.remote_settings
            .as_ref()
            .and_then(|r| r.auto_compact_threshold_percent),
    )
}

/// Lower-level form of [`resolve_auto_compact_threshold_percent`] that takes
/// the four tiers as plain `Option<u8>` values rather than reaching into a
/// `Config`. Useful from sites that don't hold a `Config` reference (e.g.,
/// subagent spawn paths where the parent's config tiers are plumbed in
/// explicitly and the per-model lookup uses the SUBAGENT's resolved model id,
/// not the parent's).
///
/// Precedence: env > `user_per_model` > `user_global` > `gb_per_model`
/// > `gb_global` > `DEFAULT_AUTO_COMPACT_THRESHOLD_PERCENT`.
pub fn resolve_auto_compact_threshold_percent_from_tiers(
    user_per_model: Option<u8>,
    user_global: Option<u8>,
    gb_per_model: Option<u8>,
    gb_global: Option<u8>,
) -> u8 {
    fn clamp_env(raw: i64) -> Option<u8> {
        if (0..=100).contains(&raw) {
            Some(raw as u8)
        } else {
            tracing::debug!(
                source = "env",
                value = raw,
                "auto_compact_threshold_percent out of range 0..=100; ignoring"
            );
            None
        }
    }
    let from_env = || -> Option<u8> {
        std::env::var(ENV_AUTO_COMPACT_THRESHOLD_PERCENT)
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .and_then(clamp_env)
    };

    from_env()
        .or(user_per_model)
        .or(user_global)
        .or(gb_per_model)
        .or(gb_global)
        .unwrap_or(DEFAULT_AUTO_COMPACT_THRESHOLD_PERCENT)
}

/// Client default per-compaction wall-clock budget (seconds). Fleet p99 of
/// successful compactions is ~181s (≈225s at 400K+ input), so 300s clears the
/// legit tail with margin while cutting a runaway from the ~600s deadline.
pub const DEFAULT_COMPACTION_WALL_CLOCK_BUDGET_SECS: u64 = 300;

/// Below this, a configured budget is almost certainly a misconfig (fleet
/// success p99 ~181s); logged at `warn`, not clamped.
const COMPACTION_WALL_CLOCK_BUDGET_WARN_SECS: u64 = 120;

/// Env override for the compaction wall-clock budget (seconds). Parsed as
/// `u64`; unparseable values fall through.
const ENV_COMPACTION_WALL_CLOCK_BUDGET_SECS: &str = "KIMIX_COMPACTION_WALL_CLOCK_SECS";

/// Resolve the per-compaction wall-clock budget (seconds). Precedence: env
/// `KIMIX_COMPACTION_WALL_CLOCK_SECS` > remote settings global
/// `RemoteSettings.compaction_wall_clock_budget_secs` >
/// [`DEFAULT_COMPACTION_WALL_CLOCK_BUDGET_SECS`] (a per-model `ModelInfo` tier
/// would slot in ahead of the global one).
///
/// `0` **disables** it. Low values are warned, not clamped — any "safe" clamp
/// (e.g. 30s) would itself cut legit compactions, trading one silent failure for
/// another; ops own the value.
pub fn resolve_compaction_wall_clock_budget_secs(gb_global: Option<u64>) -> u64 {
    let from_env = std::env::var(ENV_COMPACTION_WALL_CLOCK_BUDGET_SECS)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok());
    let resolved = from_env
        .or(gb_global)
        .unwrap_or(DEFAULT_COMPACTION_WALL_CLOCK_BUDGET_SECS);
    if resolved > 0 && resolved < COMPACTION_WALL_CLOCK_BUDGET_WARN_SECS {
        tracing::warn!(
            budget_secs = resolved,
            "compaction wall-clock budget {resolved}s is below {COMPACTION_WALL_CLOCK_BUDGET_WARN_SECS}s \
             and may cut legitimate compactions (fleet success p99 ~181s); set 0 to disable"
        );
    }
    resolved
}

/// Resolve the effective-context cap used for compaction triggers and the
/// prompt 80% observability log.
///
/// Precedence (highest first):
/// 1. env `KIMIX_MAX_EFFECTIVE_CONTEXT_TOKENS`
/// 2. user TOML `[session].max_effective_context_tokens`
/// 3. [`DEFAULT_MAX_EFFECTIVE_CONTEXT_TOKENS`] (200_000)
///
/// `0` disables the cap (full model context window is used for thresholds).
pub fn resolve_max_effective_context_tokens(user_session: Option<u32>) -> u32 {
    if let Ok(raw) = std::env::var(ENV_MAX_EFFECTIVE_CONTEXT_TOKENS)
        && let Ok(n) = raw.trim().parse::<u32>()
    {
        return n;
    }
    user_session.unwrap_or(DEFAULT_MAX_EFFECTIVE_CONTEXT_TOKENS)
}

/// Default soft-efficiency nudge lower bound (ratio of effective window).
pub const DEFAULT_SOFT_NUDGE_RATIO: f64 = 0.55;

/// Env override for soft nudge ratio. Parsed as `f64`; `0` disables.
pub(crate) const ENV_SOFT_NUDGE_RATIO: &str = "KIMIX_SOFT_NUDGE_RATIO";

/// Env override for content-hash tool-result dedup (`0`/`false`/`off` / `1`/`true`/`on`).
pub(crate) const ENV_CONTENT_HASH_DEDUP: &str = "KIMIX_CONTENT_HASH_DEDUP";

/// Resolve soft efficiency nudge ratio.
///
/// Precedence: env `KIMIX_SOFT_NUDGE_RATIO` > user TOML
/// `[session].soft_nudge_ratio` > [`DEFAULT_SOFT_NUDGE_RATIO`].
/// Values `<= 0` disable; values `> 1` are clamped to `1.0`.
pub fn resolve_soft_nudge_ratio(user_session: Option<f64>) -> f64 {
    let raw = if let Ok(s) = std::env::var(ENV_SOFT_NUDGE_RATIO)
        && let Ok(n) = s.trim().parse::<f64>()
    {
        n
    } else {
        user_session.unwrap_or(DEFAULT_SOFT_NUDGE_RATIO)
    };
    if raw <= 0.0 {
        0.0
    } else if raw > 1.0 {
        1.0
    } else {
        raw
    }
}

/// Resolve whether ingress content-hash dedup is enabled.
///
/// Precedence: env `KIMIX_CONTENT_HASH_DEDUP` > user TOML
/// `[session].content_hash_dedup` > `true`.
pub fn resolve_content_hash_dedup(user_session: Option<bool>) -> bool {
    if let Ok(raw) = std::env::var(ENV_CONTENT_HASH_DEDUP) {
        match raw.trim().to_ascii_lowercase().as_str() {
            "0" | "false" | "off" | "no" => return false,
            "1" | "true" | "on" | "yes" => return true,
            _ => {}
        }
    }
    user_session.unwrap_or(true)
}

#[cfg(test)]
mod compaction_wall_clock_budget_tests {
    use super::resolve_compaction_wall_clock_budget_secs as resolve;

    // Assumes KIMIX_COMPACTION_WALL_CLOCK_SECS is unset in the test env.
    #[test]
    fn default_global_disable_and_no_clamp() {
        assert_eq!(resolve(None), 300); // client default
        assert_eq!(resolve(Some(450)), 450); // server global wins
        assert_eq!(resolve(Some(0)), 0); // 0 explicitly disables (no clamp)
        assert_eq!(resolve(Some(5)), 5); // low values pass through (warned, not clamped)
    }
}

#[cfg(test)]
mod max_effective_context_tokens_tests {
    use super::{DEFAULT_MAX_EFFECTIVE_CONTEXT_TOKENS, resolve_max_effective_context_tokens};

    #[test]
    fn default_is_200k() {
        assert_eq!(DEFAULT_MAX_EFFECTIVE_CONTEXT_TOKENS, 200_000);
    }

    #[test]
    fn user_session_honored_when_env_unset() {
        if std::env::var(super::ENV_MAX_EFFECTIVE_CONTEXT_TOKENS).is_err() {
            assert_eq!(resolve_max_effective_context_tokens(None), 200_000);
            assert_eq!(resolve_max_effective_context_tokens(Some(0)), 0);
            assert_eq!(resolve_max_effective_context_tokens(Some(150_000)), 150_000);
        }
    }
}

#[cfg(test)]
mod soft_nudge_and_dedup_resolve_tests {
    use super::{
        DEFAULT_SOFT_NUDGE_RATIO, resolve_content_hash_dedup, resolve_soft_nudge_ratio,
    };

    #[test]
    fn soft_nudge_defaults_and_clamps_when_env_unset() {
        if std::env::var(super::ENV_SOFT_NUDGE_RATIO).is_err() {
            assert!((resolve_soft_nudge_ratio(None) - DEFAULT_SOFT_NUDGE_RATIO).abs() < f64::EPSILON);
            assert!((resolve_soft_nudge_ratio(Some(0.6)) - 0.6).abs() < f64::EPSILON);
            assert_eq!(resolve_soft_nudge_ratio(Some(0.0)), 0.0);
            assert_eq!(resolve_soft_nudge_ratio(Some(-1.0)), 0.0);
            assert_eq!(resolve_soft_nudge_ratio(Some(2.0)), 1.0);
        }
    }

    #[test]
    fn content_hash_dedup_defaults_on_when_env_unset() {
        if std::env::var(super::ENV_CONTENT_HASH_DEDUP).is_err() {
            assert!(resolve_content_hash_dedup(None));
            assert!(resolve_content_hash_dedup(Some(true)));
            assert!(!resolve_content_hash_dedup(Some(false)));
        }
    }
}
