//! Canonical session-mode enum shared between the agent and pager.
//!
//! ACP carries the mode as an opaque [`acp::SessionModeId`] (`Arc<str>`).
//! This enum is the typed counterpart both crates parse into / serialize
//! out of, so plan-mode state is driven by the closed set of variants
//! instead of by ad-hoc string matching at each boundary.

/// Wire representation is the snake-cased variant name (`default`, `plan`,
/// `decision`, `ask`) via [`strum`]. Unknown ids parse back to [`SessionMode::Default`]
/// so newer modes added on the agent side don't brick older pagers.
#[derive(Debug, Clone, PartialEq, Eq, strum::EnumString, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum SessionMode {
    Default,
    Plan,
    Decision,
    Ask,
}

impl SessionMode {
    /// Parse from the wire id. Unknown ids fall back to [`SessionMode::Default`].
    pub fn from_id(id: &str) -> Self {
        id.parse().unwrap_or(Self::Default)
    }

    /// The canonical wire id for this mode (snake_case).
    pub fn as_id(&self) -> &'static str {
        self.into()
    }

    pub fn is_plan(&self) -> bool {
        matches!(self, Self::Plan)
    }

    pub fn is_decision(&self) -> bool {
        matches!(self, Self::Decision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_known_ids() {
        for &id in &["default", "plan", "decision", "ask"] {
            let mode = SessionMode::from_id(id);
            assert_eq!(mode.as_id(), id, "round-trip failed for {id}");
        }
    }

    #[test]
    fn unknown_id_falls_back_to_default() {
        assert_eq!(SessionMode::from_id("browser_use"), SessionMode::Default);
        assert_eq!(SessionMode::from_id(""), SessionMode::Default);
        assert_eq!(SessionMode::from_id("PLAN"), SessionMode::Default); // case-sensitive
    }

    #[test]
    fn is_plan_only_for_plan_variant() {
        assert!(SessionMode::Plan.is_plan());
        assert!(!SessionMode::Default.is_plan());
        assert!(!SessionMode::Ask.is_plan());
    }

    #[test]
    fn is_decision_only_for_decision_variant() {
        assert!(SessionMode::Decision.is_decision());
        assert!(!SessionMode::Default.is_decision());
        assert!(!SessionMode::Plan.is_decision());
        assert!(!SessionMode::Ask.is_decision());
    }
}
