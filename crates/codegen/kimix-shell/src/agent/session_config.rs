use agent_client_protocol as acp;
use kimix_sampling_types::{ReasoningEffort, ReasoningEffortOption};
use serde::Serialize;

use crate::session::unified_list::SessionKind;

pub(crate) const SELECTABLE_REASONING_EFFORTS: [ReasoningEffort; 5] = [
    ReasoningEffort::Minimal,
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
    ReasoningEffort::Xhigh,
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionConfigOption {
    pub id: String,
    pub category: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KimixSessionDetail {
    pub session_id: String,
    pub kind: String,
    pub cwd: String,
    pub current_model_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl KimixSessionDetail {
    pub fn build(
        session_id: String,
        cwd: String,
        current_model_id: String,
        title: Option<String>,
    ) -> Self {
        Self {
            session_id,
            kind: SessionKind::Build.as_str().to_string(),
            cwd,
            current_model_id,
            title,
        }
    }
}

fn effort_label(effort: ReasoningEffort) -> String {
    match effort {
        ReasoningEffort::None => "None",
        ReasoningEffort::Minimal => "Minimal",
        ReasoningEffort::Low => "Low",
        ReasoningEffort::Medium => "Medium",
        ReasoningEffort::High => "High",
        ReasoningEffort::Xhigh => "X-High",
    }
    .to_string()
}

/// The built-in session-picker modes used when the model has no server list.
/// Reproduces the historical five rows and their labels.
pub(crate) fn legacy_session_effort_options() -> Vec<ReasoningEffortOption> {
    SELECTABLE_REASONING_EFFORTS
        .iter()
        .map(|&effort| ReasoningEffortOption {
            id: effort.as_str().to_string(),
            value: effort,
            label: effort_label(effort),
            description: None,
            default: false,
        })
        .collect()
}

pub(crate) fn build_session_config_options(
    available_models: &[acp::ModelInfo],
    current_model_id: &acp::ModelId,
    effort_options: &[ReasoningEffortOption],
    current_effort: Option<ReasoningEffort>,
) -> Vec<SessionConfigOption> {
    let mut options = Vec::with_capacity(available_models.len() + effort_options.len());

    for model in available_models {
        let label = if model.name.is_empty() {
            model.model_id.0.to_string()
        } else {
            model.name.clone()
        };
        options.push(SessionConfigOption {
            id: model.model_id.0.to_string(),
            category: "model".to_string(),
            label,
            description: None,
            selected: model.model_id == *current_model_id,
        });
    }

    for effort in effort_options {
        options.push(SessionConfigOption {
            id: effort.id.clone(),
            category: "mode".to_string(),
            label: effort.label.clone(),
            description: effort.description.clone(),
            selected: Some(effort.value) == current_effort,
        });
    }

    options
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &'static str, name: &str) -> acp::ModelInfo {
        acp::ModelInfo::new(acp::ModelId::new(id), name.to_string())
    }

    #[test]
    fn options_have_one_selected_model_and_a_mode_per_effort() {
        let models = [model("kimix", "Kimix"), model("kimix-4.5", "Kimix 4.5")];
        let current = acp::ModelId::from("kimix");
        let opts = build_session_config_options(
            &models,
            &current,
            &legacy_session_effort_options(),
            Some(ReasoningEffort::High),
        );

        let model_opts: Vec<_> = opts.iter().filter(|o| o.category == "model").collect();
        assert_eq!(model_opts.len(), 2);
        let selected_models: Vec<_> = model_opts.iter().filter(|o| o.selected).collect();
        assert_eq!(selected_models.len(), 1);
        assert_eq!(selected_models[0].id, "kimix");

        let mode_opts: Vec<_> = opts.iter().filter(|o| o.category == "mode").collect();
        assert_eq!(mode_opts.len(), SELECTABLE_REASONING_EFFORTS.len());
        let selected_modes: Vec<_> = mode_opts.iter().filter(|o| o.selected).collect();
        assert_eq!(selected_modes.len(), 1);
        assert_eq!(selected_modes[0].id, "high");
        assert_eq!(selected_modes[0].label, "High");
    }

    #[test]
    fn none_effort_is_not_a_user_selectable_mode() {
        assert!(!SELECTABLE_REASONING_EFFORTS.contains(&ReasoningEffort::None));
        let models = [model("kimix", "Kimix")];
        let current = acp::ModelId::from("kimix");
        let opts = build_session_config_options(
            &models,
            &current,
            &legacy_session_effort_options(),
            Some(ReasoningEffort::None),
        );
        let modes: Vec<_> = opts.iter().filter(|o| o.category == "mode").collect();
        assert!(modes.iter().all(|o| o.id != "none"));
        assert!(modes.iter().all(|o| !o.selected));
    }

    #[test]
    fn no_mode_options_when_model_lacks_effort_support() {
        let models = [model("kimix", "Kimix")];
        let current = acp::ModelId::from("kimix");
        let opts = build_session_config_options(&models, &current, &[], None);
        assert_eq!(opts.len(), 1);
        assert!(opts.iter().all(|o| o.category == "model"));
    }

    #[test]
    fn model_label_falls_back_to_id_when_name_empty() {
        let models = [model("kimix", "")];
        let current = acp::ModelId::from("kimix");
        let opts = build_session_config_options(&models, &current, &[], None);
        assert_eq!(opts[0].label, "kimix");
    }

    #[test]
    fn session_config_option_serializes_camel_case() {
        let opt = SessionConfigOption {
            id: "kimix".to_string(),
            category: "model".to_string(),
            label: "Kimix".to_string(),
            description: None,
            selected: true,
        };
        let v = serde_json::to_value(&opt).expect("serialize");
        assert_eq!(v["id"], "kimix");
        assert_eq!(v["category"], "model");
        assert_eq!(v["label"], "Kimix");
        assert_eq!(v["selected"], true);
        assert!(v.get("description").is_none());
    }

    #[test]
    fn kimix_session_detail_serializes_camel_case() {
        let detail = KimixSessionDetail::build(
            "sess-1".to_string(),
            "/Users/me/xai".to_string(),
            "kimix".to_string(),
            None,
        );
        let v = serde_json::to_value(&detail).expect("serialize");
        assert_eq!(v["sessionId"], "sess-1");
        assert_eq!(v["kind"], "build");
        assert_eq!(v["cwd"], "/Users/me/xai");
        assert_eq!(v["currentModelId"], "kimix");
        assert!(v.get("title").is_none());
    }
}
