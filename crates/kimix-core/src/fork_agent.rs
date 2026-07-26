//! Fork 子代理：通过字节级前缀一致实现缓存优化。
//!
//! # 核心洞察
//!
//! 所有 Fork 子进程共享父 Agent 的完整 system prompt（Tier1 + Tier2），
//! 仅最后一条消息（Tier4 用户指令）不同。这使得 API 请求前缀
//! 完全一致，最大化 prefix cache 命中率。
//!
//! ```text
//! 父 Agent:   [Tier1][Tier2][Tier3][Tier4_parent]
//! Fork A:     [Tier1][Tier2][     ][Tier4_A]   ← 前缀字节级一致
//! Fork B:     [Tier1][Tier2][     ][Tier4_B]
//! ```
//!
//! # 防递归
//!
//! 1. 最大深度限制（[`MAX_FORK_DEPTH`]）
//! 2. 消息扫描（检测 [`FORK_BOILERPLATE_TAG`]）

use std::collections::VecDeque;
use std::path::PathBuf;

use tracing::{debug, info, warn};

use crate::cache_engine::Context;

// ── Constants ────────────────────────────────────────────────────────────────

/// 最大 Fork 嵌套深度（含）。
///
/// `depth >= MAX_FORK_DEPTH` 时 [`check_fork_safety`] 返回
/// [`ForkError::MaxDepthExceeded`]。允许深度为 `0..MAX_FORK_DEPTH`
///（即 0、1、2 三层）。
pub const MAX_FORK_DEPTH: u32 = 3;

/// 嵌套 Fork 标记标签。
///
/// 若会话 `tier4_ephemeral` 中包含此标签，视为已处于 fork boilerplate
/// 上下文，禁止再次 fork，防止无限递归。
pub const FORK_BOILERPLATE_TAG: &str = "<fork-boilerplate>";

// ── Types ────────────────────────────────────────────────────────────────────

/// 会话 ID（UUID v4）。
pub type SessionId = uuid::Uuid;

/// 多 Agent 编排角色（与 [`crate::subagent::AgentRole`] 子任务角色正交）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRole {
    /// 仅编排工具（spawn / send_message / stop）。
    Coordinator,
    /// 仅执行工具（read / edit / bash / search）。
    Worker,
    /// 继承父级完整上下文（缓存优化）。
    Fork,
}

/// 会话生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    /// 已创建，尚未开始执行。
    Pending,
    /// 执行中。
    Running,
    /// 成功完成。
    Completed,
    /// 失败终止。
    Failed,
}

/// Agent 会话：Coordinator / Worker / Fork 的统一载体。
///
/// 字段对齐设计文档 §1 / §16.5，供 Fork 与上层编排共享。
#[derive(Debug, Clone)]
pub struct AgentSession {
    /// 会话唯一标识。
    pub id: SessionId,
    /// 编排角色。
    pub role: SessionRole,
    /// 模型 ID（Fork 必须继承父级，以共享 prefix cache）。
    pub model_id: String,
    /// 四分区上下文。
    pub context: Context,
    /// 子会话 ID 列表。
    pub children: Vec<SessionId>,
    /// 跨 Agent 共享知识库路径（Scratchpad）。
    pub scratchpad: PathBuf,
    /// 当前状态。
    pub status: SessionStatus,
    /// 当前 Fork 嵌套深度（根会话为 0）。
    pub fork_depth: u32,
}

impl AgentSession {
    /// 构造根会话（Coordinator，`fork_depth = 0`）。
    ///
    /// # Arguments
    ///
    /// * `model_id` - 模型标识
    /// * `context` - 四分区上下文
    /// * `scratchpad` - 共享 scratchpad 路径
    ///
    /// # Examples
    ///
    /// ```
    /// use kimix_core::cache_engine::Context;
    /// use kimix_core::fork_agent::{AgentSession, SessionRole, SessionStatus};
    /// use std::path::PathBuf;
    ///
    /// let ctx = Context::new("sys", "tools", "agents", "prefs");
    /// let session = AgentSession::root("model-a", ctx, PathBuf::from("/tmp/sp"));
    /// assert_eq!(session.role, SessionRole::Coordinator);
    /// assert_eq!(session.fork_depth, 0);
    /// assert_eq!(session.status, SessionStatus::Pending);
    /// ```
    pub fn root(model_id: impl Into<String>, context: Context, scratchpad: PathBuf) -> Self {
        Self {
            id: SessionId::new_v4(),
            role: SessionRole::Coordinator,
            model_id: model_id.into(),
            context,
            children: Vec::new(),
            scratchpad,
            status: SessionStatus::Pending,
            fork_depth: 0,
        }
    }
}

/// Fork 操作错误。
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ForkError {
    /// 超过最大 Fork 深度。
    #[error("max fork depth exceeded (limit: {limit})")]
    MaxDepthExceeded {
        /// 当前深度。
        depth: u32,
        /// 允许的最大深度（[`MAX_FORK_DEPTH`]）。
        limit: u32,
    },

    /// 消息中检测到嵌套 Fork 标记。
    #[error("nested fork detected in message")]
    NestedForkDetected,
}

// ── Public API ───────────────────────────────────────────────────────────────

/// 创建 Fork 子代理（缓存优化）。
///
/// 继承父级 `model_id`、`tier1_immutable`、`tier2_stable` 与 `scratchpad`；
/// `tier3_volatile` 置空；`tier4_ephemeral` 设为 `instruction`。
/// 新会话 `role = Fork`，`fork_depth = parent.fork_depth + 1`。
///
/// **调用方应先** [`check_fork_safety`] **再调用本函数**，或使用
/// [`try_fork`] 合并检查与创建。
///
/// # Arguments
///
/// * `parent` - 父 Agent 会话
/// * `instruction` - 子代理专属指令（写入 Tier4）
///
/// # Returns
///
/// 新的 [`AgentSession`]（`SessionRole::Fork`）。
///
/// # Examples
///
/// ```
/// use kimix_core::cache_engine::Context;
/// use kimix_core::fork_agent::{fork, AgentSession, SessionRole};
/// use std::path::PathBuf;
///
/// let ctx = Context::new("sys", "tools", "agents", "prefs");
/// let parent = AgentSession::root("gpt-x", ctx, PathBuf::from("/tmp/sp"));
/// let child = fork(&parent, "analyze auth module");
/// assert_eq!(child.role, SessionRole::Fork);
/// assert_eq!(child.model_id, parent.model_id);
/// assert_eq!(child.context.tier1_immutable, parent.context.tier1_immutable);
/// assert!(child.context.tier3_volatile.is_empty());
/// assert_eq!(child.context.tier4_ephemeral, "analyze auth module");
/// ```
pub fn fork(parent: &AgentSession, instruction: &str) -> AgentSession {
    let child_depth = parent.fork_depth.saturating_add(1);

    info!(
        parent_id = %parent.id,
        parent_depth = parent.fork_depth,
        child_depth,
        model_id = %parent.model_id,
        instruction_len = instruction.len(),
        "creating fork child agent"
    );

    debug!(
        tier1_len = parent.context.tier1_immutable.len(),
        tier2_len = parent.context.tier2_stable.len(),
        parent_tier3_len = parent.context.tier3_volatile.len(),
        "fork inherits tier1/tier2 byte-identically; tier3 cleared"
    );

    AgentSession {
        id: SessionId::new_v4(),
        role: SessionRole::Fork,
        model_id: parent.model_id.clone(),
        context: Context {
            tier1_immutable: parent.context.tier1_immutable.clone(),
            tier2_stable: parent.context.tier2_stable.clone(),
            tier3_volatile: VecDeque::new(),
            tier4_ephemeral: instruction.to_string(),
        },
        children: Vec::new(),
        scratchpad: parent.scratchpad.clone(),
        status: SessionStatus::Pending,
        fork_depth: child_depth,
    }
}

/// 防递归安全检查。
///
/// Fork 子进程可保留 Agent 工具（cache-identical tool defs），
/// 但通过以下机制防止无限递归：
///
/// 1. 最大深度限制：`depth >= `[`MAX_FORK_DEPTH`]
/// 2. 消息扫描：`tier4_ephemeral` 含 [`FORK_BOILERPLATE_TAG`]
///
/// # Arguments
///
/// * `session` - 待检查的会话（通常是即将发起 fork 的父会话）
/// * `depth` - 当前嵌套深度（一般传 `session.fork_depth`）
///
/// # Errors
///
/// * [`ForkError::MaxDepthExceeded`] — 深度超限
/// * [`ForkError::NestedForkDetected`] — 检测到嵌套 fork 标签
///
/// # Examples
///
/// ```
/// use kimix_core::cache_engine::Context;
/// use kimix_core::fork_agent::{
///     check_fork_safety, AgentSession, ForkError, FORK_BOILERPLATE_TAG, MAX_FORK_DEPTH,
/// };
/// use std::path::PathBuf;
///
/// let ctx = Context::new("sys", "tools", "agents", "prefs");
/// let session = AgentSession::root("m", ctx, PathBuf::from("/tmp/sp"));
/// assert!(check_fork_safety(&session, 0).is_ok());
/// assert!(matches!(
///     check_fork_safety(&session, MAX_FORK_DEPTH),
///     Err(ForkError::MaxDepthExceeded { .. })
/// ));
/// ```
pub fn check_fork_safety(session: &AgentSession, depth: u32) -> Result<(), ForkError> {
    if depth >= MAX_FORK_DEPTH {
        warn!(
            session_id = %session.id,
            depth,
            limit = MAX_FORK_DEPTH,
            "fork safety: max depth exceeded"
        );
        return Err(ForkError::MaxDepthExceeded {
            depth,
            limit: MAX_FORK_DEPTH,
        });
    }

    if session
        .context
        .tier4_ephemeral
        .contains(FORK_BOILERPLATE_TAG)
    {
        warn!(
            session_id = %session.id,
            depth,
            "fork safety: nested fork boilerplate tag detected"
        );
        return Err(ForkError::NestedForkDetected);
    }

    debug!(
        session_id = %session.id,
        depth,
        "fork safety check passed"
    );
    Ok(())
}

/// 安全创建 Fork 子代理：先 [`check_fork_safety`]，再 [`fork`]。
///
/// 额外对 `instruction` 本身做 boilerplate 标签扫描，避免把嵌套标记
/// 写入子会话后才被发现。
///
/// # Arguments
///
/// * `parent` - 父 Agent 会话
/// * `instruction` - 子代理专属指令
///
/// # Errors
///
/// 透传 [`check_fork_safety`] 错误；若 `instruction` 含
/// [`FORK_BOILERPLATE_TAG`] 则返回 [`ForkError::NestedForkDetected`]。
///
/// # Examples
///
/// ```
/// use kimix_core::cache_engine::Context;
/// use kimix_core::fork_agent::{try_fork, AgentSession, SessionRole};
/// use std::path::PathBuf;
///
/// let ctx = Context::new("sys", "tools", "agents", "prefs");
/// let parent = AgentSession::root("m", ctx, PathBuf::from("/tmp/sp"));
/// let child = try_fork(&parent, "do work").unwrap();
/// assert_eq!(child.role, SessionRole::Fork);
/// assert_eq!(child.fork_depth, 1);
/// ```
pub fn try_fork(parent: &AgentSession, instruction: &str) -> Result<AgentSession, ForkError> {
    check_fork_safety(parent, parent.fork_depth)?;

    if instruction.contains(FORK_BOILERPLATE_TAG) {
        warn!(
            parent_id = %parent.id,
            "fork refused: instruction contains boilerplate tag"
        );
        return Err(ForkError::NestedForkDetected);
    }

    Ok(fork(parent, instruction))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_context() -> Context {
        Context::new(
            "You are kimix.",
            "read_file, edit_file, bash",
            "# AGENTS.md\nrules",
            "language: zh",
        )
    }

    fn sample_parent() -> AgentSession {
        let mut parent = AgentSession::root(
            "model-shared",
            sample_context(),
            PathBuf::from("/tmp/kimix-scratchpad"),
        );
        // 父级可能已有 volatile 与 ephemeral，fork 后应被清空 / 替换。
        parent.context.push_volatile("tool_result_old".to_string());
        parent.context.tier4_ephemeral = "parent user msg".to_string();
        parent
    }

    /// 正常 fork：字节级前缀一致 + 模型/scratchpad 继承。
    #[test]
    fn fork_inherits_prefix_byte_identically() {
        let parent = sample_parent();
        let instruction = "analyze the auth module";
        let child = fork(&parent, instruction);

        assert_ne!(child.id, parent.id);
        assert_eq!(child.role, SessionRole::Fork);
        assert_eq!(child.model_id, parent.model_id);
        assert_eq!(child.status, SessionStatus::Pending);
        assert_eq!(child.fork_depth, parent.fork_depth + 1);
        assert!(child.children.is_empty());

        // 字节级一致（Tier1 / Tier2）
        assert_eq!(
            child.context.tier1_immutable.as_bytes(),
            parent.context.tier1_immutable.as_bytes()
        );
        assert_eq!(
            child.context.tier2_stable.as_bytes(),
            parent.context.tier2_stable.as_bytes()
        );

        // Tier3 清空；Tier4 仅为子指令
        assert!(child.context.tier3_volatile.is_empty());
        assert_eq!(child.context.tier4_ephemeral, instruction);

        // Scratchpad 路径共享
        assert_eq!(child.scratchpad, parent.scratchpad);

        // 父级自身未被修改
        assert!(!parent.context.tier3_volatile.is_empty());
        assert_eq!(parent.context.tier4_ephemeral, "parent user msg");
    }

    /// 多个 fork 子代理共享同一稳定前缀（缓存共享前提）。
    #[test]
    fn multiple_forks_share_identical_stable_prefix() {
        let parent = sample_parent();
        let a = fork(&parent, "task A");
        let b = fork(&parent, "task B");

        assert_eq!(a.context.tier1_immutable, b.context.tier1_immutable);
        assert_eq!(a.context.tier2_stable, b.context.tier2_stable);
        assert_eq!(a.model_id, b.model_id);
        assert_eq!(a.scratchpad, b.scratchpad);
        assert_ne!(a.context.tier4_ephemeral, b.context.tier4_ephemeral);
        assert_ne!(a.id, b.id);
    }

    /// 深度超限：`depth >= MAX_FORK_DEPTH` 返回 MaxDepthExceeded。
    #[test]
    fn check_fork_safety_rejects_max_depth() {
        let parent = sample_parent();

        assert!(check_fork_safety(&parent, 0).is_ok());
        assert!(check_fork_safety(&parent, 1).is_ok());
        assert!(check_fork_safety(&parent, 2).is_ok());

        let err = check_fork_safety(&parent, MAX_FORK_DEPTH).unwrap_err();
        assert_eq!(
            err,
            ForkError::MaxDepthExceeded {
                depth: MAX_FORK_DEPTH,
                limit: MAX_FORK_DEPTH,
            }
        );

        let err = check_fork_safety(&parent, MAX_FORK_DEPTH + 5).unwrap_err();
        assert!(matches!(err, ForkError::MaxDepthExceeded { .. }));
    }

    /// 标签检测：tier4 含 `<fork-boilerplate>` 时拒绝。
    #[test]
    fn check_fork_safety_rejects_boilerplate_tag() {
        let mut parent = sample_parent();
        parent.context.tier4_ephemeral = format!("prefix {FORK_BOILERPLATE_TAG} suffix");

        let err = check_fork_safety(&parent, 0).unwrap_err();
        assert_eq!(err, ForkError::NestedForkDetected);
    }

    /// `try_fork`：成功路径与深度链。
    #[test]
    fn try_fork_success_and_depth_chain() {
        let root = sample_parent();
        assert_eq!(root.fork_depth, 0);

        let d1 = try_fork(&root, "level-1").expect("depth 0 -> 1");
        assert_eq!(d1.fork_depth, 1);
        assert_eq!(d1.role, SessionRole::Fork);

        let d2 = try_fork(&d1, "level-2").expect("depth 1 -> 2");
        assert_eq!(d2.fork_depth, 2);

        let d3 = try_fork(&d2, "level-3").expect("depth 2 -> 3");
        assert_eq!(d3.fork_depth, 3);

        // 深度 3 不可再 fork
        let err = try_fork(&d3, "level-4").unwrap_err();
        assert_eq!(
            err,
            ForkError::MaxDepthExceeded {
                depth: 3,
                limit: MAX_FORK_DEPTH,
            }
        );
    }

    /// `try_fork`：instruction 自身含 boilerplate 标签时拒绝。
    #[test]
    fn try_fork_rejects_instruction_with_boilerplate() {
        let parent = sample_parent();
        let bad = format!("do work {FORK_BOILERPLATE_TAG}");
        let err = try_fork(&parent, &bad).unwrap_err();
        assert_eq!(err, ForkError::NestedForkDetected);
    }

    /// 空 instruction 仍可 fork（边界：仅清空 Tier4）。
    #[test]
    fn fork_allows_empty_instruction() {
        let parent = sample_parent();
        let child = fork(&parent, "");
        assert_eq!(child.context.tier4_ephemeral, "");
        assert!(child.context.tier3_volatile.is_empty());
        assert_eq!(
            child.context.tier1_immutable,
            parent.context.tier1_immutable
        );
    }
}
