//! Canonical session-selection CLI intent — headless subset.
//!
//! Shared by headless mode for resume / new-with-id / fork logic.
use std::path::{Path, PathBuf};

use anyhow::Context;

// ── Fork helpers ──────────────────────────────────────────────────────────

/// Build `Kimix/session/fork` params shared by TUI effects and headless.
pub fn fork_session_params(
    parent_session_id: &str,
    parent_cwd: &Path,
    new_session_id: Option<&str>,
    parent_is_worktree: bool,
) -> serde_json::Value {
    let parent_cwd_str = parent_cwd.to_string_lossy().into_owned();
    let source_cwd = kimix_shell::session::resolve_local_session_any_cwd(parent_session_id)
        .unwrap_or_else(|| parent_cwd_str.clone());
    let mut payload = serde_json::json!(
        { "sourceSessionId" : parent_session_id, "sourceCwd" : source_cwd, "newCwd" :
        parent_cwd_str.clone(), "sessionKind" : "fork", }
    );
    if let Some(nid) = new_session_id {
        payload["newSessionId"] = serde_json::Value::String(nid.to_string());
    }
    if parent_is_worktree {
        payload["sourceWorkspaceDir"] = serde_json::Value::String(parent_cwd_str);
    }
    payload
}

/// Whether a persisted session (or its cwd) is worktree-backed.
pub fn parent_session_is_worktree(session_id: &str, cwd: &Path) -> bool {
    let cwd_str = cwd.to_string_lossy();
    let sessions_root = kimix_shell::util::kimix_home::kimix_home().join("sessions");
    let encoded = kimix_shell::util::kimix_home::encode_cwd_dirname(&cwd_str);
    let summary_path = sessions_root
        .join(encoded)
        .join(session_id)
        .join("summary.json");
    if let Ok(bytes) = std::fs::read(&summary_path)
        && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes)
    {
        if v.get("session_kind").and_then(|k| k.as_str()) == Some("worktree") {
            return true;
        }
        if v.get("source_workspace_dir")
            .and_then(|k| k.as_str())
            .is_some_and(|s| !s.is_empty())
        {
            return true;
        }
    }
    let mut cur = Some(cwd);
    while let Some(dir) = cur {
        let git = dir.join(".git");
        if git.is_file() {
            return true;
        }
        if git.is_dir() {
            return false;
        }
        cur = dir.parent();
    }
    false
}

/// Parse `newSessionId` from a `Kimix/session/fork` ACP response body.
pub fn fork_response_new_session_id(resp_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(resp_json).unwrap_or_default();
    if v.get("error").is_some_and(|e| !e.is_null()) {
        return None;
    }
    v.get("newSessionId")
        .and_then(|x| x.as_str())
        .or_else(|| {
            v.get("result")
                .and_then(|r| r.get("newSessionId"))
                .and_then(|x| x.as_str())
        })
        .map(|s| s.to_string())
}

/// Error string from a fork response, if present.
pub fn fork_response_error(resp_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(resp_json).ok()?;
    v.get("error")
        .filter(|e| !e.is_null())
        .map(|e| e.to_string())
}

// ── Session startup intent ────────────────────────────────────────────────

/// Pure interpretation of session-selection CLI flags (no I/O).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStartupIntent {
    /// Fresh session; agent picks the ID.
    NewAuto,
    /// Fresh session with a client-chosen ID (must not exist under cwd).
    NewWithId { session_id: String },
    /// Load an existing session (strict — never create).
    Resume {
        /// `None` means resolve most-recent for cwd at materialize time.
        session_id: Option<String>,
        most_recent_for_cwd: bool,
    },
    /// Resolve source like resume, then fork; optional forced ID for the child.
    ForkFrom {
        source_session_id: Option<String>,
        most_recent_for_cwd: bool,
        new_session_id: Option<String>,
    },
}

/// Flag combinations that clap allows but we reject at runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupFlagError {
    /// `--session-id` with resume/continue/load without `--fork-session`.
    SessionIdRequiresFork,
    /// `--fork-session` without resume/continue/load.
    ForkRequiresResumeOrContinue,
    /// `--fork-session` with `--worktree` (not supported yet).
    ForkWithWorktree,
}

impl std::fmt::Display for StartupFlagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionIdRequiresFork => {
                write!(
                    f,
                    "Error: --session-id can only be used with --continue or --resume if --fork-session is also specified."
                )
            }
            Self::ForkRequiresResumeOrContinue => {
                write!(f, "Error: --fork-session requires --resume or --continue.")
            }
            Self::ForkWithWorktree => {
                write!(
                    f,
                    "Error: --fork-session cannot be combined with --worktree."
                )
            }
        }
    }
}

impl std::error::Error for StartupFlagError {}

/// Inputs shared by interactive CLI and headless (no clap dependency).
#[derive(Debug, Clone, Copy)]
pub struct SessionStartupFlags<'a> {
    pub session_id: Option<&'a str>,
    /// Explicit resume id from `-r` / `--resume` (not the empty most-recent sentinel).
    pub resume_session_id: Option<&'a str>,
    /// `--resume` with no value (most recent for cwd).
    pub resume_most_recent: bool,
    pub continue_last_session: bool,
    pub fork_session: bool,
    /// True when `--worktree` is set (any label, including empty default).
    pub has_worktree: bool,
}

/// Classify session-selection flags into a single intent (no I/O).
pub fn session_startup_intent_from_flags(
    f: SessionStartupFlags<'_>,
) -> Result<SessionStartupIntent, StartupFlagError> {
    let has_resume_id = f.resume_session_id.is_some();
    let most_recent = f.resume_most_recent || f.continue_last_session;
    let has_resume_or_continue = has_resume_id || most_recent;
    if f.fork_session && f.has_worktree {
        return Err(StartupFlagError::ForkWithWorktree);
    }
    if f.fork_session && !has_resume_or_continue {
        return Err(StartupFlagError::ForkRequiresResumeOrContinue);
    }
    if let Some(sid) = f.session_id {
        if has_resume_or_continue && !f.fork_session {
            return Err(StartupFlagError::SessionIdRequiresFork);
        }
        if f.fork_session {
            return Ok(SessionStartupIntent::ForkFrom {
                source_session_id: f.resume_session_id.map(|s| s.to_owned()),
                most_recent_for_cwd: most_recent && !has_resume_id,
                new_session_id: Some(sid.to_owned()),
            });
        }
        return Ok(SessionStartupIntent::NewWithId {
            session_id: sid.to_owned(),
        });
    }
    if f.fork_session {
        return Ok(SessionStartupIntent::ForkFrom {
            source_session_id: f.resume_session_id.map(|s| s.to_owned()),
            most_recent_for_cwd: most_recent && !has_resume_id,
            new_session_id: None,
        });
    }
    if let Some(id) = f.resume_session_id {
        return Ok(SessionStartupIntent::Resume {
            session_id: Some(id.to_owned()),
            most_recent_for_cwd: false,
        });
    }
    if most_recent {
        return Ok(SessionStartupIntent::Resume {
            session_id: None,
            most_recent_for_cwd: true,
        });
    }
    Ok(SessionStartupIntent::NewAuto)
}

// ── Materialization ───────────────────────────────────────────────────────

/// Outcome of async materialization (local resolve / remote restore / preflight).
#[derive(Debug, Clone)]
pub enum MaterializedStartup {
    /// Create a new session with an agent-chosen ID (or defer to welcome).
    NewAuto,
    /// Create a new session with this ID (`session/new` meta.sessionId).
    NewWithId { session_id: String },
    /// Strict load of an existing session.
    Resume {
        session_id: String,
        original_cwd: Option<PathBuf>,
        title: Option<String>,
    },
    /// Fork from a resolved parent, then load the child.
    Fork {
        parent_session_id: String,
        parent_cwd: Option<PathBuf>,
        parent_title: Option<String>,
        new_session_id: Option<String>,
    },
}

/// Context for [`materialize_startup`] (interactive vs headless share this).
#[derive(Debug, Clone, Copy)]
pub struct MaterializeCtx {
    /// When true, skip process-cwd preflight for `NewWithId`.
    pub has_worktree: bool,
    /// When true, attempt remote restore if the session is not on disk.
    pub allow_remote_restore: bool,
    /// Process-wide flag: resume targets are kimi.com conversations.
    pub chat_mode: bool,
}

// ── Chat-mode guards (shared with TUI) ────────────────────────────────────

/// User-facing refusal when process-wide `--chat` would open a local Build disk row.
pub const CHAT_MODE_LOCAL_BUILD_REFUSAL: &str = "cannot open a local Build session while --chat is active; \
resume a conversation or start a new chat (/chat)";

/// User-facing refusal for `--fork-session` + `--chat`.
pub const CHAT_MODE_FORK_CONFLICT: &str = "--fork-session is not supported with --chat";

/// Conservative shape check for a chat-mode `--resume <id>` passthrough.
pub fn valid_conversation_id_shape(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

/// True when `session_id` resolves under the cwd-scoped local Build sessions tree.
pub fn local_build_session_on_disk(session_id: &str, cwd: &Path) -> bool {
    let cwd_str = cwd.to_string_lossy();
    kimix_shell::session::resolve_local_session(session_id, &cwd_str).is_some()
}

/// Pure policy: process-wide `--chat` refuses a local Build disk row.
pub fn chat_mode_refuses_local_build(
    chat_mode: bool,
    conversation_entry: bool,
    is_local_build_on_disk: bool,
) -> bool {
    chat_mode && !conversation_entry && is_local_build_on_disk
}

/// Process-wide `--chat` must not load (or coerce) local Build disk rows.
pub fn chat_mode_refuses_local_build_load(
    chat_mode: bool,
    conversation_entry: bool,
    session_id: &str,
    cwd: &Path,
) -> bool {
    if !chat_mode || conversation_entry {
        return false;
    }
    local_build_session_on_disk(session_id, cwd)
}

// ── CWD helpers ───────────────────────────────────────────────────────────

/// Cwd where a forked child session is written (interactive + headless SSOT).
pub fn effective_fork_new_cwd(process_cwd: &str, parent_cwd: Option<&Path>) -> String {
    parent_cwd
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| process_cwd.to_string())
}

/// Preflight: preferred id must be a UUID and not a persisted session under `cwd`.
pub fn ensure_session_id_available(session_id: &str, cwd: &str) -> anyhow::Result<()> {
    if uuid::Uuid::try_parse(session_id).is_err() {
        anyhow::bail!("Error: --session-id must be a valid UUID (got '{session_id}').");
    }
    if kimix_shell::session::persistence::session_exists_for_cwd(session_id, cwd) {
        anyhow::bail!("Error: Session ID {session_id} is already in use.");
    }
    Ok(())
}

// ── Private helpers ───────────────────────────────────────────────────────

/// Resolve most-recent session id for cwd, or error.
async fn most_recent_session_id(cwd: &str) -> anyhow::Result<(String, Option<String>)> {
    let summaries = kimix_shell::session::persistence::list_summaries(Some(cwd)).await?;
    let first = summaries.first().ok_or_else(|| {
        anyhow::anyhow!(
            "No session found for current directory. \
             Use 'kimix' to start a new session."
        )
    })?;
    Ok((first.info.id.to_string(), first.display_title_opt()))
}

struct ResolvedExisting {
    id: String,
    original_cwd: Option<PathBuf>,
    title: Option<String>,
}

/// Resolve an existing session for strict resume (local / any-cwd / remote / worktree defer).
async fn resolve_existing_session(
    ctx: MaterializeCtx,
    session_id: &str,
    cwd: &str,
) -> anyhow::Result<ResolvedExisting> {
    if let Some(local_id) = kimix_shell::session::resolve_local_session(session_id, cwd) {
        tracing::info!(
            session_id = % session_id, local_id = % local_id, "Session found locally"
        );
        return Ok(ResolvedExisting {
            id: local_id,
            original_cwd: None,
            title: None,
        });
    }
    if let Some(original_cwd) = kimix_shell::session::resolve_local_session_any_cwd(session_id) {
        tracing::info!(
            session_id = % session_id, original_cwd = % original_cwd,
            "Session found locally under different CWD"
        );
        eprintln!(
            "Session {} found locally (originally in {})",
            session_id, original_cwd
        );
        return Ok(ResolvedExisting {
            id: session_id.to_string(),
            original_cwd: Some(PathBuf::from(original_cwd)),
            title: None,
        });
    }
    if ctx.has_worktree {
        tracing::info!(
            session_id = % session_id,
            "Session not found locally; deferring restore to worktree resume handler"
        );
        eprintln!(
            "Session {} not found locally; it will be restored into the new worktree.",
            session_id
        );
        return Ok(ResolvedExisting {
            id: session_id.to_string(),
            original_cwd: None,
            title: None,
        });
    }
    if !ctx.allow_remote_restore {
        anyhow::bail!("Session does not exist");
    }
    eprintln!(
        "Session {} not found locally, restoring from remote...",
        session_id
    );
    let raw_config = kimix_shell::config::load_effective_config()
        .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;
    let agent_config = kimix_shell::agent::config::Config::new_from_toml_cfg(&raw_config)
        .map_err(|e| anyhow::anyhow!("Failed to create agent config: {}", e))?;
    use kimix_shell::agent::session_registry_client::SessionRegistryClient;
    use kimix_shell::auth::{AuthManager, ensure_authenticated_or_noninteractive};
    use kimix_shell::session::restore::restore_session_with_progress;
    use kimix_shell::util::kimix_home::kimix_home;
    let deployment_key = agent_config.endpoints.deployment_key.clone();
    ensure_authenticated_or_noninteractive(
        &agent_config.kimi_code_config,
        deployment_key.is_some(),
        None,
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to authenticate for session restore: {}", e))?;
    let auth_manager = std::sync::Arc::new(AuthManager::new(
        &kimix_home(),
        agent_config.kimi_code_config.clone(),
    ));
    let registry_client =
        SessionRegistryClient::new(agent_config.endpoints.proxy_url(), String::new())
            .with_deployment_key(deployment_key.clone())
            .with_alpha_test_key(agent_config.endpoints.alpha_test_key.clone())
            .with_auth(auth_manager.clone());
    let progress: kimix_shell::session::restore::ProgressCallback =
        Box::new(|event| eprintln!("  {}", event.display_line()));
    let result =
        restore_session_with_progress(&registry_client, session_id, cwd, None, Some(progress))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to restore session from remote: {:#}", e))?;
    let effective_id = if result.local_session_id.is_empty() {
        session_id.to_string()
    } else {
        result.local_session_id
    };
    eprintln!("  Restored as local session {}", effective_id);
    Ok(ResolvedExisting {
        id: effective_id,
        original_cwd: None,
        title: None,
    })
}

// ── Public materialize entry ──────────────────────────────────────────────

/// Materialize CLI intent into a concrete startup plan (I/O + remote restore).
pub async fn materialize_startup(
    ctx: MaterializeCtx,
    intent: SessionStartupIntent,
) -> anyhow::Result<MaterializedStartup> {
    let cwd = std::env::current_dir()
        .context("Failed to get cwd")?
        .to_string_lossy()
        .to_string();
    materialize_startup_for_cwd(ctx, intent, &cwd).await
}

/// Same as [`materialize_startup`] but with an explicit process cwd (tests / headless).
pub async fn materialize_startup_for_cwd(
    ctx: MaterializeCtx,
    intent: SessionStartupIntent,
    cwd: &str,
) -> anyhow::Result<MaterializedStartup> {
    if ctx.chat_mode && matches!(intent, SessionStartupIntent::ForkFrom { .. }) {
        anyhow::bail!("{CHAT_MODE_FORK_CONFLICT}");
    }
    match intent {
        SessionStartupIntent::NewAuto => Ok(MaterializedStartup::NewAuto),
        SessionStartupIntent::NewWithId { session_id } => {
            if !ctx.has_worktree {
                ensure_session_id_available(&session_id, cwd)?;
            } else if uuid::Uuid::try_parse(&session_id).is_err() {
                anyhow::bail!("Error: --session-id must be a valid UUID (got '{session_id}').");
            }
            Ok(MaterializedStartup::NewWithId { session_id })
        }
        SessionStartupIntent::Resume {
            session_id: None,
            most_recent_for_cwd: true,
        } => {
            if ctx.chat_mode {
                anyhow::bail!("chat-mode resume requires a build with the `chat` cargo feature");
            }
            let started = std::time::Instant::now();
            let (id, title) = most_recent_session_id(cwd).await?;
            tracing::info!(
                source = "local",
                elapsed_ms = started.elapsed().as_millis() as u64,
                "startup.continue.resolve"
            );
            Ok(MaterializedStartup::Resume {
                session_id: id,
                original_cwd: None,
                title,
            })
        }
        SessionStartupIntent::ForkFrom {
            source_session_id: None,
            most_recent_for_cwd: true,
            new_session_id,
        } => {
            if let Some(ref nid) = new_session_id {
                ensure_session_id_available(nid, cwd)?;
            }
            let (id, title) = most_recent_session_id(cwd).await?;
            Ok(MaterializedStartup::Fork {
                parent_session_id: id,
                parent_cwd: None,
                parent_title: title,
                new_session_id,
            })
        }
        SessionStartupIntent::Resume {
            session_id: Some(session_id),
            ..
        } => {
            if ctx.chat_mode {
                if !valid_conversation_id_shape(&session_id) {
                    anyhow::bail!("invalid conversation id {session_id:?}");
                }
                return Ok(MaterializedStartup::Resume {
                    session_id,
                    original_cwd: None,
                    title: None,
                });
            }
            let r = resolve_existing_session(ctx, &session_id, cwd).await?;
            Ok(MaterializedStartup::Resume {
                session_id: r.id,
                original_cwd: r.original_cwd,
                title: r.title,
            })
        }
        SessionStartupIntent::ForkFrom {
            source_session_id: Some(session_id),
            new_session_id,
            ..
        } => {
            let r = resolve_existing_session(ctx, &session_id, cwd).await?;
            if let Some(ref nid) = new_session_id {
                let new_cwd = effective_fork_new_cwd(cwd, r.original_cwd.as_deref());
                ensure_session_id_available(nid, &new_cwd)?;
            }
            Ok(MaterializedStartup::Fork {
                parent_session_id: r.id,
                parent_cwd: r.original_cwd,
                parent_title: r.title,
                new_session_id,
            })
        }
        SessionStartupIntent::Resume {
            session_id: None,
            most_recent_for_cwd: false,
        }
        | SessionStartupIntent::ForkFrom {
            source_session_id: None,
            most_recent_for_cwd: false,
            ..
        } => {
            anyhow::bail!("internal: invalid session startup intent (unreachable from CLI flags)")
        }
    }
}
