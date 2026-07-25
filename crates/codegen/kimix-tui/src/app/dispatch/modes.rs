//! Plan, yolo, auto, and permission mode transitions and toasts.
use super::ctx::with_active_agent;
use super::queue::maybe_drain_queue;
use super::session::lifecycle::skip_picker_and_create_session;
use super::settings::ui::{refresh_open_settings_modals, save_success_toast};
use crate::app::actions::Effect;
use crate::app::app_view::{ActiveView, AppView};
use agent_client_protocol as acp;

/// Show the current plan: if a plan file exists, open it in the preview
/// overlay popover. If no plan has been written yet, show a toast.
///
/// Delegates to `AgentView::show_plan_preview()` which reads the plan file
/// from `~/.kimix/sessions/<urlencoded_cwd>/<session_id>/plan.md`.
pub(super) fn dispatch_show_plan(app: &mut AppView) -> Vec<Effect> {
    with_active_agent(app, |agent| {
        if agent.plan_approval_view.is_some() {
            agent.reopen_plan_approval();
        } else {
            agent.show_plan_preview();
        }
    });
    vec![]
}

/// Enter plan mode via `/plan`.
///
/// When not in plan mode: emits `SetSessionMode` (or `SetModeThenPrompt`
/// if a description is provided). When already in plan mode: no-op with toast.
/// Use `/view-plan` to open the current saved plan preview.
///
/// When a description is present, the mode switch and prompt send must be
/// ordered: the mode switch ACP call must complete before the prompt is
/// dispatched. `SetModeThenPrompt` bundles both into a single spawned task
/// to guarantee this ordering.
pub(super) fn dispatch_enter_plan_mode(
    app: &mut AppView,
    description: Option<String>,
) -> Vec<Effect> {
    let ActiveView::Agent(id) = app.active_view else {
        return vec![];
    };
    let Some(agent) = app.agents.get_mut(&id) else {
        return vec![];
    };

    let in_plan = agent.plan_mode_pending.unwrap_or(agent.plan_mode_active);
    if in_plan {
        app.show_toast("Already in plan mode. Use /view-plan to view the current plan.");
        return vec![];
    }

    let agent = app.agents.get_mut(&id).unwrap();
    let Some(session_id) = agent.session.session_id.clone() else {
        agent.show_toast("No active session");
        return vec![];
    };

    // Set optimistic pending state (same pattern as dispatch_cycle_mode).
    agent.plan_mode_pending = Some(true);
    tracing::info!("Plan mode entered via /plan slash command");

    let mode_id = acp::SessionModeId::new("plan");

    if let Some(desc) = description {
        // Enqueue and drain: maybe_drain_queue does all synchronous turn
        // setup (scrollback, start_turn, prompt_id) and returns a SendPrompt.
        // We combine it with the mode switch into a single sequential effect
        // so the mode switch completes before the prompt is sent.
        // The description is a plain prompt: capture composer-recognized
        // tokens like the normal submit path (offsets recomputed against
        // `desc` since the leading `/plan ` was stripped).
        let skill_token_ranges = agent
            .prompt
            .slash_controller
            .recognized_token_ranges(&desc, &agent.session.models);
        agent
            .session
            .enqueue_prompt_with_skill_tokens(desc, skill_token_ranges);
        let drain = maybe_drain_queue(agent);
        let mut effects = Vec::with_capacity(1);
        for eff in drain {
            match eff {
                Effect::SendPrompt {
                    agent_id,
                    text,
                    prompt_id,
                    skill_token_ranges,
                    ..
                } => {
                    effects.push(Effect::SetModeThenPrompt {
                        session_id: session_id.clone(),
                        mode_id: mode_id.clone(),
                        agent_id,
                        text,
                        prompt_id,
                        skill_token_ranges,
                    });
                }
                other => effects.push(other),
            }
        }
        // If drain was empty (not idle), just emit the mode switch — the
        // prompt stays queued and will drain naturally when the agent idles.
        if effects.is_empty() {
            effects.push(Effect::SetSessionMode {
                session_id,
                mode_id,
            });
        }
        effects
    } else {
        vec![Effect::SetSessionMode {
            session_id,
            mode_id,
        }]
    }
}

/// Set plan mode (on / off). PAGER-owned + ACP-mediated, per-session.
///
/// Optimistic flow: captures effective state (`pending.or(active)`),
/// sets `plan_mode_pending`, refreshes modals, toasts, then emits
/// `Effect::SetSessionMode`. Shell confirms via `CurrentModeUpdate`.
///
/// No explicit rollback — `SetSessionMode` has no failure surface.
/// If the ACP transport drops, `plan_mode_pending` stays set until
/// the next `CurrentModeUpdate` or session restart.
///
/// Idempotent: same value toasts but skips the ACP round-trip.
pub(super) fn set_plan_mode(
    app: &mut AppView,
    kind: crate::app::actions::PlanModeKind,
) -> Vec<Effect> {
    let ActiveView::Agent(id) = app.active_view else {
        return vec![];
    };
    let Some(agent) = app.agents.get_mut(&id) else {
        return vec![];
    };

    let Some(session_id) = agent.session.session_id.clone() else {
        agent.show_toast("No active session");
        return vec![];
    };

    // Effective state: prefer optimistic pending over confirmed
    // active. Mirrors `dispatch_cycle_mode`'s `in_plan` read so
    // rapid toggles don't double-send.
    let prev = agent.plan_mode_pending.unwrap_or(agent.plan_mode_active);
    let new = kind.to_bool();

    // Idempotent: toast but skip the ACP round-trip.
    if prev == new {
        app.show_toast(&plan_mode_toast(kind));
        return vec![];
    }

    // Optimistic mutation: pager-side pending flag, then UI feedback,
    // then effect. The shell's `CurrentModeUpdate` broadcast will
    // confirm + clear `plan_mode_pending` via `detect_plan_mode_change`.
    agent.plan_mode_pending = Some(new);
    refresh_open_settings_modals(app);
    app.show_toast(&plan_mode_toast(kind));

    tracing::info!(
        target: "settings",
        key = "plan_mode",
        value = new,
        "setting changed",
    );

    // OFF targets `SessionMode::Default`, not the user's prior mode.
    // If the user was in `Ask` (shell-injection only), that preference
    // is silently dropped. See `PLAN_MODE_CHOICES` in `settings/defs.rs`.
    let mode_id = acp::SessionModeId::new(if new {
        kimix_tools::types::SessionMode::Plan.as_id()
    } else {
        kimix_tools::types::SessionMode::Default.as_id()
    });

    vec![Effect::SetSessionMode {
        session_id,
        mode_id,
    }]
}

/// Format the `Plan mode` toast. Non-destructive in both directions
/// (unlike YOLO), so both ON and OFF use the uniform ✓ glyph.
/// Uses lowercase "on"/"off" via `save_success_toast`.
fn plan_mode_toast(kind: crate::app::actions::PlanModeKind) -> String {
    save_success_toast("Plan mode", kind.to_bool())
}

/// The single gate for client paths that ENABLE always-approve: `Some(reason)`
/// iff `enabling` and the pin (`app.yolo_policy_block`) is set. Every enabling
/// path routes through here (or [`refuse_if_yolo_locked`]) so new paths stay
/// gated by default; callers must NOT persist on a refusal.
pub(super) fn yolo_enable_blocked(app: &AppView, enabling: bool) -> Option<&'static str> {
    if enabling {
        app.yolo_policy_block
    } else {
        None
    }
}

/// `Vec<Effect>` wrapper for the persisting setters: on a refusal, toast and
/// return `Some(vec![])` (no persist); `None` means proceed.
fn refuse_if_yolo_locked(app: &mut AppView, enabling: bool) -> Option<Vec<Effect>> {
    let warning = yolo_enable_blocked(app, enabling)?;
    app.show_toast(warning);
    Some(vec![])
}

/// Canonical "auto wins only when yolo is off" precedence — the single source
/// of truth for the yolo-over-auto rule applied at every reconnect / seed / meta
/// site. Callers pass the already-resolved auto signal (a per-session flag or a
/// `permission_mode == Some("auto")` test).
pub(crate) fn effective_auto(yolo: bool, auto: bool) -> bool {
    !yolo && auto
}

/// When the auto gate is off, force the displayed permission mode off Auto and
/// clear every agent's per-session auto flag, so the UI / Shift+Tab cycle /
/// settings snapshot and each tab's badge never show Auto while the feature is
/// disabled. Shared by the startup reconcile and the mid-session kill-switch.
/// Clearing every agent (not just when the global mirror still reads "auto")
/// matters because `switch_to_agent` re-anchors the mirror to the active tab.
pub(crate) fn downgrade_displayed_auto_if_gated(app: &mut AppView) {
    if app.auto_mode_gate {
        return;
    }
    for agent in app.agents.values_mut() {
        agent.session.auto_mode = false;
    }
    if app.current_ui.permission_mode.as_deref() == Some("auto") {
        app.current_ui.permission_mode = Some("ask".into());
    }
}

/// Whether a newly created session should start with the Auto display flag set:
/// the gate is on, the current UI mode is Auto, and yolo is not winning. Mirrors
/// the canonical `auto && !yolo` precedence used on the wire (`ClientCapabilities`
/// / `SessionFlags`). The `auto_mode_gate` check is defense-in-depth so a stale
/// `current_ui == "auto"` can never seed a new session into Auto when gated off.
pub(super) fn inherit_auto_mode(app: &AppView) -> bool {
    app.auto_mode_gate
        && effective_auto(
            app.default_yolo,
            app.current_ui.permission_mode.as_deref() == Some("auto"),
        )
}

/// Keep the active session's `auto_mode` display flag in lockstep with the
/// applied canonical permission mode. The canonical (`app.current_ui
/// .permission_mode`) is the single value every mode-change path finalizes —
/// the cycle, the settings setter, and the rollback all write it — so deriving
/// the flag from it (and clearing it under yolo, which wins) keeps the prompt
/// "auto" indicator correct regardless of which seam applied the mode.
pub(super) fn sync_active_auto_flag(app: &mut AppView) {
    let is_auto = app.current_ui.permission_mode.as_deref() == Some("auto");
    if let ActiveView::Agent(id) = app.active_view
        && let Some(agent) = app.agents.get_mut(&id)
    {
        agent.session.auto_mode = effective_auto(agent.session.is_yolo(), is_auto);
    }
    // Keep `/auto` feature-gate visibility in lockstep across slash surfaces.
    app.sync_permission_mode_slash_gate();
}

/// State-only `permission_mode` (YOLO) mutation; also called from rollback.
/// Flips to ON are refused while the pin is set.
pub(super) fn set_yolo_mode_inner(app: &mut AppView, new: bool) {
    if yolo_enable_blocked(app, new).is_some() {
        tracing::warn!("always-approve enable blocked by managed policy");
        return;
    }
    // Global mirrors update unconditionally (even if the user navigated
    // away from the agent mid-rollback). Per-agent state is gated below.
    app.default_yolo = new;
    app.permission_mode_from_soft_default = false;
    // Write-only mirror — see fn doc-comment.
    app.current_ui.permission_mode = Some(if new { "always-approve" } else { "ask" }.to_string());

    let ActiveView::Agent(id) = app.active_view else {
        return;
    };
    let Some(agent) = app.agents.get_mut(&id) else {
        return;
    };

    let previous_state = agent.session.is_yolo();

    // Drain ordering invariant: flag flip BEFORE the drain (see fn
    // doc-comment). Do NOT reorder these without re-reading the
    // contract.
    agent.session.yolo_mode = new;

    if new {
        // YOLO ON: auto-approve all queued permissions. Drain runs
        // even on idempotent re-dispatch. Prefers `AllowOnce`; falls
        // back to `Cancelled` (never `AllowAlways`).
        agent.last_permission_click = None;
        for perm in agent.permission_queue.drain(..) {
            if let Some(allow) = perm
                .options
                .iter()
                .find(|o| o.kind == acp::PermissionOptionKind::AllowOnce)
            {
                perm.request
                    .response_tx
                    .send(Ok(acp::RequestPermissionResponse::new(
                        acp::RequestPermissionOutcome::Selected(
                            acp::SelectedPermissionOutcome::new(allow.option_id.clone()),
                        ),
                    )))
                    .ok();
            } else {
                perm.request
                    .response_tx
                    .send(Ok(acp::RequestPermissionResponse::new(
                        acp::RequestPermissionOutcome::Cancelled,
                    )))
                    .ok();
            }
        }
        // Restore stashed prompt since queue is now empty.
        if let Some(stashed) = agent.permission_stashed_prompt.take() {
            agent.prompt.restore(stashed);
        }
    }

    // Telemetry + tracing guarded on real state change only.
    if previous_state != new {
        tracing::info!(target: "settings", key = "permission_mode", value = new, "setting changed");
    }
}

/// Set YOLO (`permission_mode`). SHELL-owned, emits
/// `Effect::PersistPermissionMode` with rollback. The drain runs
/// unconditionally on YOLO=ON (even duplicate dispatches) because
/// a permission could arrive between dispatches.
fn capture_prev_permission_canonical(app: &AppView, prev_yolo: bool) -> &'static str {
    if prev_yolo {
        "always-approve"
    } else {
        match app.current_ui.permission_mode.as_deref() {
            Some("default") => "default",
            Some("auto") => "auto",
            _ => "ask",
        }
    }
}

pub(super) fn set_yolo_mode(app: &mut AppView, new: bool) -> Vec<Effect> {
    // Managed policy pins always-approve off — no state change, no persist.
    if let Some(blocked) = refuse_if_yolo_locked(app, new) {
        return blocked;
    }
    let ActiveView::Agent(id) = app.active_view else {
        return vec![];
    };
    // Capture LIVE yolo + plan state and session_id atomically for rollback.
    let (prev_yolo, session_id, effective_plan) = app
        .agents
        .get(&id)
        .map(|a| {
            (
                a.session.is_yolo(),
                a.session.session_id.clone(),
                a.plan_mode_pending.unwrap_or(a.plan_mode_active),
            )
        })
        .unwrap_or((false, None, false));
    let prev_canonical = capture_prev_permission_canonical(app, prev_yolo);

    set_yolo_mode_inner(app, new);

    // Refresh modal snapshots so the indicator reflects the new value.
    refresh_open_settings_modals(app);
    // Toggling yolo always lands on ask/always-approve (never auto); keep the
    // per-session auto display flag in sync (clears it).
    sync_active_auto_flag(app);

    // Toast on every save. YOLO ON gets a weightier visual; under an active
    // plan mode, say the plan edit gate stays binding — "all tool actions
    // auto-run" would overpromise while the shell rejects non-plan-file edits.
    if new && effective_plan {
        app.show_toast(YOLO_ON_UNDER_PLAN_TOAST);
    } else {
        app.show_toast(&yolo_toast(new));
    }

    // Forward write is always "ask" or "always-approve" (bool entry
    // point). Rollback uses `prev_canonical` with LIVE precedence.
    let canonical: &'static str = if new { "always-approve" } else { "ask" };
    vec![Effect::PersistPermissionMode {
        canonical,
        session_id,
        persist: crate::app::actions::PermissionModePersist::WithRollback(prev_canonical),
    }]
}

/// Set permission mode by typed kind. Entry point from the settings
/// modal. Mirrors `set_yolo_mode` but preserves the canonical string
/// (the inner collapses "default" onto "ask"; this setter restores
/// the distinction by overriding `app.current_ui.permission_mode`
/// after the inner call). Rollback uses LIVE-precedence canonical.
pub(super) fn set_permission_mode(
    app: &mut AppView,
    kind: crate::app::actions::PermissionModeKind,
) -> Vec<Effect> {
    // Feature gate: a commit to Auto is inert when the auto permission-mode
    // feature is disabled. Reading `app.auto_mode_gate` here (the same source
    // the Shift+Tab cycle uses) keeps the settings modal and the cycle in
    // lockstep — both degrade Auto → Ask when the gate is off.
    let kind =
        if matches!(kind, crate::app::actions::PermissionModeKind::Auto) && !app.auto_mode_gate {
            crate::app::actions::PermissionModeKind::Ask
        } else {
            kind
        };
    // Managed policy pins always-approve off — keep the modal on live state.
    if let Some(blocked) = refuse_if_yolo_locked(app, kind.is_always_approve()) {
        refresh_open_settings_modals(app);
        return blocked;
    }
    let ActiveView::Agent(id) = app.active_view else {
        return vec![];
    };
    // Capture LIVE yolo + plan state and session_id atomically for rollback.
    let (prev_yolo, session_id, effective_plan) = app
        .agents
        .get(&id)
        .map(|a| {
            (
                a.session.is_yolo(),
                a.session.session_id.clone(),
                a.plan_mode_pending.unwrap_or(a.plan_mode_active),
            )
        })
        .unwrap_or((false, None, false));
    let prev_canonical = capture_prev_permission_canonical(app, prev_yolo);

    // State mutation via shared inner. We overwrite the canonical
    // below for the Default case. Inner clears the soft-default latch.
    set_yolo_mode_inner(app, kind.is_always_approve());

    // Restore the "default" distinction the inner's bool-projection
    // collapses. No-op for `AlwaysApprove` and `Ask`.
    app.current_ui.permission_mode = Some(kind.as_canonical().to_string());

    // Refresh modal so its snapshot reflects the overridden canonical.
    refresh_open_settings_modals(app);
    // Keep the per-session auto display flag in sync with the applied canonical
    // (`kind` was already degraded to Ask when the gate is off, so a remaining
    // Auto here means the gate passed).
    sync_active_auto_flag(app);

    // Toast on every save (plan-aware for AlwaysApprove, mirroring
    // `set_yolo_mode` — the plan edit gate stays binding under yolo).
    if kind.is_always_approve() && effective_plan {
        app.show_toast(YOLO_ON_UNDER_PLAN_TOAST);
    } else {
        app.show_toast(&permission_mode_toast(kind));
    }

    vec![Effect::PersistPermissionMode {
        canonical: kind.as_canonical(),
        session_id,
        persist: crate::app::actions::PermissionModePersist::WithRollback(prev_canonical),
    }]
}

/// Build the toast for a `permission_mode` commit. `AlwaysApprove`
/// reuses `yolo_toast(true)` (destructive). `Ask` and `Default` get
/// dedicated "Permission mode: ..." toasts matching the picker brand.
pub(super) fn permission_mode_toast(kind: crate::app::actions::PermissionModeKind) -> String {
    use crate::app::actions::PermissionModeKind;
    match kind {
        PermissionModeKind::AlwaysApprove => yolo_toast(true),
        PermissionModeKind::Auto => "\u{2713} Permission mode: Auto (classifier)".to_string(),
        PermissionModeKind::Ask => "\u{2713} Permission mode: Ask".to_string(),
        PermissionModeKind::Default => "\u{2713} Permission mode: Default".to_string(),
    }
}

/// YOLO-ON toast when plan mode is active: always-approve arms the permission
/// fast path, but the shell's plan-mode gate still rejects non-plan-file
/// edits, so the standard "all tool actions auto-run" would overpromise.
pub(super) const YOLO_ON_UNDER_PLAN_TOAST: &str =
    "\u{26A0} Always-approve ON: plan mode still blocks file edits until you exit plan mode";

/// Build the YOLO toast — ⚠ on ON (destructive), ✓ on OFF (safe default).
fn yolo_toast(new: bool) -> String {
    if new {
        // Warning glyph + consequence — only post-commit feedback.
        "\u{26A0} Always-approve ON: all tool actions auto-run".to_string()
    } else {
        // OFF restores safe default — uniform ✓ glyph.
        save_success_toast("Always-approve", false)
    }
}

/// Toggle YOLO mode (Ctrl+O keybinding path). Delegates to the
/// registry-driven `set_yolo_mode` so permission-queue draining,
/// telemetry, and persistence all flow through a single code path.
pub(super) fn dispatch_toggle_yolo(app: &mut AppView) -> Vec<Effect> {
    let ActiveView::Agent(id) = app.active_view else {
        return vec![];
    };
    let Some(agent) = app.agents.get(&id) else {
        return vec![];
    };
    let new = !agent.session.yolo_mode;
    set_yolo_mode(app, new)
}

/// Tab toggles Plan mode on/off (bidirectional Normal ↔ Plan).
pub(super) fn dispatch_toggle_plan_mode(app: &mut AppView) -> Vec<Effect> {
    let ActiveView::Agent(id) = app.active_view else {
        return vec![];
    };
    let Some(agent) = app.agents.get(&id) else {
        return vec![];
    };
    let in_plan = agent.plan_mode_pending.unwrap_or(agent.plan_mode_active);
    set_plan_mode(
        app,
        if in_plan {
            crate::app::actions::PlanModeKind::Off
        } else {
            crate::app::actions::PlanModeKind::On
        },
    )
}

/// Shift+Tab mode cycle from the agent chat view: the shared cycle body plus
/// plan-nudge acceptance telemetry (the nudge advertises this chord). The
/// dashboard peek calls [`dispatch_cycle_mode_and_sync`] instead, so a peeked
/// agent — whose prompt the user is not looking at — never attributes an accept.
pub(super) fn dispatch_cycle_mode(app: &mut AppView) -> Vec<Effect> {
    let (nudge_showing, in_plan_before) = active_agent_plan_nudge_state(app);
    let effects = dispatch_cycle_mode_and_sync(app);
    if nudge_showing
        && !in_plan_before
        && let ActiveView::Agent(id) = app.active_view
        && let Some(agent) = app.agents.get_mut(&id)
        && agent.plan_mode_pending.unwrap_or(agent.plan_mode_active)
    {
        agent
            .ephemeral_tip
            .clear(crate::tips::plan_nudge::PLAN_NUDGE_KEY);
    }
    effects
}

/// The Shift+Tab cycle body shared by the agent view and the dashboard peek:
/// apply the mode, then keep the per-session `auto_mode` display flag in sync
/// with the freshly written canonical mode — covering every arm (including the
/// pre-session and policy-pin early returns) without per-arm edits. Deliberately
/// telemetry-free: the dashboard peek reuses it so it can't attribute a
/// plan-nudge acceptance for an agent the user isn't viewing.
pub(super) fn dispatch_cycle_mode_and_sync(app: &mut AppView) -> Vec<Effect> {
    app.permission_mode_from_soft_default = false;
    let effects = dispatch_cycle_mode_inner(app);
    sync_active_auto_flag(app);
    effects
}

/// The active agent's `(plan nudge visible, optimistically in plan mode)`, or
/// `(false, false)` with no active agent. Lets [`dispatch_cycle_mode`] attribute
/// a shift+tab that turns plan mode on while the nudge shows as an acceptance.
pub(super) fn active_agent_plan_nudge_state(app: &AppView) -> (bool, bool) {
    let ActiveView::Agent(id) = app.active_view else {
        return (false, false);
    };
    match app.agents.get(&id) {
        Some(agent) => (
            agent.ephemeral_tip.current_key() == Some(crate::tips::plan_nudge::PLAN_NUDGE_KEY),
            agent.plan_mode_pending.unwrap_or(agent.plan_mode_active),
        ),
        None => (false, false),
    }
}

/// Cycle session mode: Normal → Plan → Decision → Normal.
///
/// Uses `plan_mode_pending` (optimistic) when available, falling back to
/// `plan_mode_active` (confirmed by ACP). This prevents double-sends when
/// the user presses Shift+Tab faster than the ACP round-trip.
/// `decision_mode_active` tracks whether the session is in Decision mode.
/// Permission modes (auto/always-approve) are independent of the session mode cycle.
fn dispatch_cycle_mode_inner(app: &mut AppView) -> Vec<Effect> {
    let ActiveView::Agent(id) = app.active_view else {
        return vec![];
    };
    let Some(agent) = app.agents.get_mut(&id) else {
        return vec![];
    };
    let Some(session_id) = agent.session.session_id.clone() else {
        // No session yet: cycle locally and stash in `deferred_session_mode`.
        let in_plan = agent.plan_mode_pending.unwrap_or(agent.plan_mode_active);
        let in_decision = agent.decision_mode_active;
        match (in_plan, in_decision) {
            // Normal → Plan
            (false, false) => {
                agent.plan_mode_pending = Some(true);
                agent.deferred_session_mode = Some(kimix_tools::types::SessionMode::Plan);
                agent.show_mode_switch_banner("计划");
                tracing::info!("Mode cycle (pre-session): Normal → Plan");
            }
            // Plan → Decision
            (true, false) => {
                agent.plan_mode_pending = Some(false);
                agent.decision_mode_active = true;
                agent.deferred_session_mode = Some(kimix_tools::types::SessionMode::Decision);
                agent.show_mode_switch_banner("决策");
                tracing::info!("Mode cycle (pre-session): Plan → Decision");
            }
            // Decision → Normal
            (false, true) => {
                agent.decision_mode_active = false;
                agent.deferred_session_mode = None;
                agent.show_mode_switch_banner("普通");
                tracing::info!("Mode cycle (pre-session): Decision → Normal");
            }
            // Any mixed state → Normal
            _ => {
                agent.plan_mode_pending = Some(false);
                agent.decision_mode_active = false;
                agent.deferred_session_mode = None;
                agent.show_mode_switch_banner("普通");
                tracing::info!("Mode cycle (pre-session): mixed → Normal");
            }
        };
        refresh_open_settings_modals(app);
        let mut effects = Vec::new();
        effects.extend(skip_picker_and_create_session(app, id));
        return effects;
    };

    // Effective plan/decision state: prefer optimistic pending over confirmed active.
    let in_plan = agent.plan_mode_pending.unwrap_or(agent.plan_mode_active);
    let in_decision = agent.decision_mode_active;

    match (in_plan, in_decision) {
        // Normal → Plan
        (false, false) => {
            agent.plan_mode_pending = Some(true);
            agent.show_mode_switch_banner("计划");
            refresh_open_settings_modals(app);
            tracing::info!("Mode cycle: Normal → Plan");
            vec![Effect::SetSessionMode {
                session_id,
                mode_id: acp::SessionModeId::new(kimix_tools::types::SessionMode::Plan.as_id()),
            }]
        }
        // Plan → Decision
        (true, false) => {
            agent.plan_mode_pending = Some(false);
            agent.decision_mode_active = true;
            agent.show_mode_switch_banner("决策");
            refresh_open_settings_modals(app);
            tracing::info!("Mode cycle: Plan → Decision");
            vec![Effect::SetSessionMode {
                session_id,
                mode_id: acp::SessionModeId::new(kimix_tools::types::SessionMode::Decision.as_id()),
            }]
        }
        // Decision → Normal
        (false, true) => {
            agent.decision_mode_active = false;
            agent.show_mode_switch_banner("普通");
            refresh_open_settings_modals(app);
            tracing::info!("Mode cycle: Decision → Normal");
            vec![Effect::SetSessionMode {
                session_id,
                mode_id: acp::SessionModeId::new(kimix_tools::types::SessionMode::Default.as_id()),
            }]
        }
        // Any other combination → reset to Normal.
        _ => {
            agent.plan_mode_pending = Some(false);
            agent.decision_mode_active = false;
            agent.show_mode_switch_banner("普通");
            refresh_open_settings_modals(app);
            tracing::info!("Mode cycle: mixed state → Normal");
            vec![Effect::SetSessionMode {
                session_id,
                mode_id: acp::SessionModeId::new(kimix_tools::types::SessionMode::Default.as_id()),
            }]
        }
    }
}
