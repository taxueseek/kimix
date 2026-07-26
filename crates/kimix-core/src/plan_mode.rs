//! Plan Mode：先看后做的安全机制（双重审批 + `allowedPrompts` 预授权）。
//!
//! # 生命周期
//!
//! ```text
//! 用户请求 → EnterPlan（权限收窄为只读）
//!   → Explore Agent 搜索代码库
//!   → Plan Agent 生成方案
//!   → ExitPlan（提交计划 + allowedPrompts）
//!   → 用户审批（可编辑 plan.md）
//!   → ApprovePlan（恢复权限 + 注入预授权）
//!   → 按计划执行
//! ```
//!
//! # 双重审批
//!
//! 1. **进入审批**：进入 Plan Mode 前由 UI/编排层确认（本模块负责权限收窄）。
//! 2. **退出审批**：`exit_plan` 写出可编辑的 `plan.md` 与预授权清单；用户批准后
//!    调用 `approve_plan` 恢复权限并注入 `allowedPrompts`。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ── Permission & session stubs (kimix-core local surface) ─────────────────────

/// Agent 权限模式。
///
/// Plan Mode 进入时切换为 [`PermissionMode::Plan`]（只读工具集）；
/// 用户批准计划后恢复为进入前的模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    /// 默认：敏感操作需确认。
    #[default]
    Default,
    /// 不询问，按策略自动放行。
    DontAsk,
    /// 绕过权限检查（危险，仅显式启用）。
    BypassPermissions,
    /// 计划模式：仅允许只读探索工具。
    Plan,
}

/// Plan Mode 使用的最小会话句柄。
///
/// 仅承载权限切换与预授权注入所需字段；完整会话状态由上层编排持有。
#[derive(Debug, Clone)]
pub struct AgentSession {
    /// 会话标识（用于生成计划文件路径）。
    pub id: String,
    /// 当前权限模式。
    pub permission_mode: PermissionMode,
    /// 已注入的预授权命令（`approve_plan` 写入）。
    pub granted_prompts: Vec<AllowedPrompt>,
    /// 计划文件目录覆盖（测试 / 自定义布局）；`None` 时使用默认 `~/.kimix/plans`。
    plans_dir: Option<PathBuf>,
}

impl AgentSession {
    /// 创建默认会话（`Default` 权限，无预授权）。
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            permission_mode: PermissionMode::Default,
            granted_prompts: Vec::new(),
            plans_dir: None,
        }
    }

    /// 覆盖计划文件存储目录（主要用于单元测试）。
    pub fn with_plans_dir(mut self, dir: PathBuf) -> Self {
        self.plans_dir = Some(dir);
        self
    }

    /// 注入一条预授权命令（tool + prompt 文本匹配）。
    pub fn grant_prompt(&mut self, tool: &str, prompt: &str) {
        self.granted_prompts.push(AllowedPrompt {
            tool: tool.to_string(),
            prompt: prompt.to_string(),
        });
    }

    /// 当前会话的计划存储目录。
    fn plans_dir(&self) -> PathBuf {
        self.plans_dir.clone().unwrap_or_else(default_plans_dir)
    }
}

// ── Public types ─────────────────────────────────────────────────────────────

/// 预授权命令：计划阶段声明、批准后自动放行的 tool+prompt 对。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowedPrompt {
    /// 工具名（如 `"Bash"`、`"run_terminal_command"`）。
    pub tool: String,
    /// 允许的 prompt / 命令类别描述（如 `"run tests"`）。
    pub prompt: String,
}

/// 进入 Plan Mode 后返回的上下文，供退出与批准阶段使用。
#[derive(Debug, Clone)]
pub struct PlanContext {
    /// 进入 Plan Mode 前的权限模式（批准后恢复）。
    pub previous_mode: PermissionMode,
    /// Plan Mode 下允许的只读工具集。
    pub allowed_tools: Vec<String>,
    /// 计划文件路径（`*.md`；旁路 `*.prompts.json` 存预授权）。
    pub plan_file: PathBuf,
}

/// Plan Mode 操作错误。
#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    /// 当前会话未处于 Plan Mode。
    #[error("plan mode not active")]
    NotInPlanMode,

    /// 会话已处于 Plan Mode，禁止重复进入。
    #[error("already in plan mode")]
    AlreadyInPlanMode,

    /// 计划文件不存在。
    #[error("plan file not found: {0}")]
    PlanFileNotFound(PathBuf),

    /// 文件系统错误。
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// JSON 序列化 / 反序列化错误。
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

// ── Constants ────────────────────────────────────────────────────────────────

/// Plan Mode 只读工具集（与设计文档 §16.6 对齐）。
const PLAN_READONLY_TOOLS: &[&str] = &["read_file", "grep", "glob"];

// ── Public API ───────────────────────────────────────────────────────────────

/// 进入计划模式。
///
/// # 效果
///
/// - 保存当前权限模式以便恢复
/// - 将权限切换为 [`PermissionMode::Plan`]（只读）
/// - 返回只读工具集与计划文件路径
///
/// # Errors
///
/// - [`PlanError::AlreadyInPlanMode`]：已在 Plan Mode 中
pub fn enter_plan(session: &mut AgentSession) -> Result<PlanContext, PlanError> {
    if session.permission_mode == PermissionMode::Plan {
        return Err(PlanError::AlreadyInPlanMode);
    }

    let previous_mode = session.permission_mode;
    session.permission_mode = PermissionMode::Plan;

    let plan_file = generate_plan_path(&session.plans_dir(), &session.id);

    tracing::info!(
        session_id = %session.id,
        previous = ?previous_mode,
        plan_file = %plan_file.display(),
        "entered plan mode"
    );

    Ok(PlanContext {
        previous_mode,
        allowed_tools: PLAN_READONLY_TOOLS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        plan_file,
    })
}

/// 退出计划模式：写出计划文件与预授权清单，供用户审批。
///
/// # Arguments
///
/// * `session` — 当前会话（须处于 Plan Mode）
/// * `plan` — 生成的计划 Markdown 内容
/// * `allowed_prompts` — 预授权命令类别
///
/// # Returns
///
/// 计划文件路径（用户可在批准前编辑）。
///
/// # Errors
///
/// - [`PlanError::NotInPlanMode`]：未处于 Plan Mode
/// - [`PlanError::Io`]：写文件失败
/// - [`PlanError::Serde`]：预授权序列化失败
///
/// # Note
///
/// 本函数**不**恢复权限；须在用户批准后调用 [`approve_plan`]。
pub fn exit_plan(
    session: &mut AgentSession,
    plan: &str,
    allowed_prompts: &[AllowedPrompt],
) -> Result<PathBuf, PlanError> {
    if session.permission_mode != PermissionMode::Plan {
        return Err(PlanError::NotInPlanMode);
    }

    let plan_path = generate_plan_path(&session.plans_dir(), &session.id);

    if let Some(parent) = plan_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // 写入计划文件（用户可编辑）
    std::fs::write(&plan_path, plan)?;

    // 保存 allowedPrompts 供后续注入
    let prompts_path = prompts_path_for(&plan_path);
    let prompts_json = serde_json::to_string_pretty(allowed_prompts)?;
    std::fs::write(&prompts_path, prompts_json)?;

    tracing::info!(
        session_id = %session.id,
        plan_file = %plan_path.display(),
        prompt_count = allowed_prompts.len(),
        "exited plan mode; plan pending user approval"
    );

    Ok(plan_path)
}

/// 用户批准后恢复执行。
///
/// # 效果
///
/// - 恢复进入前的权限模式
/// - 读取并注入 `allowedPrompts` 预授权
///
/// # Errors
///
/// - [`PlanError::NotInPlanMode`]：未处于 Plan Mode
/// - [`PlanError::PlanFileNotFound`]：计划文件缺失
/// - [`PlanError::Io`] / [`PlanError::Serde`]：读取或解析预授权失败
pub fn approve_plan(session: &mut AgentSession, ctx: &PlanContext) -> Result<(), PlanError> {
    if session.permission_mode != PermissionMode::Plan {
        return Err(PlanError::NotInPlanMode);
    }

    if !ctx.plan_file.exists() {
        return Err(PlanError::PlanFileNotFound(ctx.plan_file.clone()));
    }

    // 恢复权限
    session.permission_mode = ctx.previous_mode;

    // 读取 allowedPrompts 并注入
    let prompts_path = prompts_path_for(&ctx.plan_file);
    if prompts_path.exists() {
        let prompts_json = std::fs::read_to_string(&prompts_path)?;
        let allowed_prompts: Vec<AllowedPrompt> = serde_json::from_str(&prompts_json)?;

        for prompt in &allowed_prompts {
            session.grant_prompt(&prompt.tool, &prompt.prompt);
        }

        tracing::info!(
            session_id = %session.id,
            restored = ?ctx.previous_mode,
            prompt_count = allowed_prompts.len(),
            "plan approved; permissions restored and prompts granted"
        );
    } else {
        tracing::info!(
            session_id = %session.id,
            restored = ?ctx.previous_mode,
            "plan approved; no allowedPrompts file present"
        );
    }

    Ok(())
}

/// 生成计划文件路径：`{plans_dir}/{session_id}.md`。
///
/// 默认 `plans_dir` 为 `~/.kimix/plans`（见 [`default_plans_dir`]）。
pub fn generate_plan_path(plans_dir: &Path, session_id: &str) -> PathBuf {
    plans_dir.join(format!("{session_id}.md"))
}

/// 默认计划目录：`$HOME/.kimix/plans`（无 HOME 时回退为 `./.kimix/plans`）。
pub fn default_plans_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".kimix").join("plans")
}

/// 预授权旁路文件路径：将 `.md` 替换为 `.prompts.json`。
fn prompts_path_for(plan_path: &Path) -> PathBuf {
    // Prefer stem-based naming so `foo.md` → `foo.prompts.json` rather than
    // `foo.md.prompts.json` via a naive extension swap.
    match plan_path.file_stem().and_then(|s| s.to_str()) {
        Some(stem) => plan_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!("{stem}.prompts.json")),
        None => plan_path.with_extension("prompts.json"),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_session(dir: &TempDir) -> AgentSession {
        AgentSession::new("sess-test-1").with_plans_dir(dir.path().to_path_buf())
    }

    /// 正常流程：enter → exit → approve，权限恢复且预授权注入。
    #[test]
    fn full_lifecycle_enter_exit_approve() {
        let dir = TempDir::new().expect("tempdir");
        let mut session = test_session(&dir);
        session.permission_mode = PermissionMode::DontAsk;

        let ctx = enter_plan(&mut session).expect("enter");
        assert_eq!(session.permission_mode, PermissionMode::Plan);
        assert_eq!(ctx.previous_mode, PermissionMode::DontAsk);
        assert_eq!(
            ctx.allowed_tools,
            vec![
                "read_file".to_string(),
                "grep".to_string(),
                "glob".to_string()
            ]
        );
        assert_eq!(ctx.plan_file, dir.path().join("sess-test-1.md"));

        let prompts = vec![
            AllowedPrompt {
                tool: "Bash".into(),
                prompt: "run tests".into(),
            },
            AllowedPrompt {
                tool: "Bash".into(),
                prompt: "install deps".into(),
            },
        ];
        let plan_body = "# Plan\n\n1. Refactor auth\n";
        let plan_path = exit_plan(&mut session, plan_body, &prompts).expect("exit");
        assert_eq!(plan_path, ctx.plan_file);
        assert!(plan_path.exists());
        // 退出后仍保持 Plan Mode，等待用户批准
        assert_eq!(session.permission_mode, PermissionMode::Plan);

        let written = std::fs::read_to_string(&plan_path).expect("read plan");
        assert_eq!(written, plan_body);

        let prompts_path = prompts_path_for(&plan_path);
        assert!(prompts_path.exists());

        approve_plan(&mut session, &ctx).expect("approve");
        assert_eq!(session.permission_mode, PermissionMode::DontAsk);
        assert_eq!(session.granted_prompts, prompts);
    }

    /// 未进入 Plan Mode 时 exit / approve 均返回 `NotInPlanMode`。
    #[test]
    fn not_in_plan_mode_errors() {
        let dir = TempDir::new().expect("tempdir");
        let mut session = test_session(&dir);

        let exit_err = exit_plan(&mut session, "plan", &[]).unwrap_err();
        assert!(matches!(exit_err, PlanError::NotInPlanMode));

        let ctx = PlanContext {
            previous_mode: PermissionMode::Default,
            allowed_tools: vec![],
            plan_file: dir.path().join("missing.md"),
        };
        let approve_err = approve_plan(&mut session, &ctx).unwrap_err();
        assert!(matches!(approve_err, PlanError::NotInPlanMode));
    }

    /// 计划文件缺失时 `approve_plan` 返回 `PlanFileNotFound`。
    #[test]
    fn approve_plan_file_not_found() {
        let dir = TempDir::new().expect("tempdir");
        let mut session = test_session(&dir);
        let ctx = enter_plan(&mut session).expect("enter");

        // 未调用 exit_plan，计划文件不存在
        assert!(!ctx.plan_file.exists());
        let err = approve_plan(&mut session, &ctx).unwrap_err();
        match err {
            PlanError::PlanFileNotFound(p) => assert_eq!(p, ctx.plan_file),
            other => panic!("expected PlanFileNotFound, got {other:?}"),
        }
        // 失败后仍应保持 Plan Mode（未恢复权限）
        assert_eq!(session.permission_mode, PermissionMode::Plan);
    }

    /// 重复进入 Plan Mode 被拒绝。
    #[test]
    fn already_in_plan_mode() {
        let dir = TempDir::new().expect("tempdir");
        let mut session = test_session(&dir);
        enter_plan(&mut session).expect("first enter");
        let err = enter_plan(&mut session).unwrap_err();
        assert!(matches!(err, PlanError::AlreadyInPlanMode));
    }

    /// `generate_plan_path` 路径形状与默认目录。
    #[test]
    fn generate_plan_path_shape() {
        let base = PathBuf::from("/tmp/kimix-plans");
        let path = generate_plan_path(&base, "abc-123");
        assert_eq!(path, PathBuf::from("/tmp/kimix-plans/abc-123.md"));

        let default = default_plans_dir();
        assert!(
            default.ends_with(Path::new(".kimix/plans")),
            "default plans dir should end with .kimix/plans, got {}",
            default.display()
        );
    }

    /// 无预授权清单时仍可批准（仅恢复权限）。
    #[test]
    fn approve_without_prompts_file() {
        let dir = TempDir::new().expect("tempdir");
        let mut session = test_session(&dir);
        session.permission_mode = PermissionMode::BypassPermissions;

        let ctx = enter_plan(&mut session).expect("enter");
        // 只写 plan.md，不写 prompts 旁路文件
        if let Some(parent) = ctx.plan_file.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(&ctx.plan_file, "# empty plan\n").expect("write plan");

        approve_plan(&mut session, &ctx).expect("approve");
        assert_eq!(session.permission_mode, PermissionMode::BypassPermissions);
        assert!(session.granted_prompts.is_empty());
    }
}
