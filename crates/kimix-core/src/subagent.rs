//! 子 Agent 委托系统
//!
//! 精简自 grok-build 的 15 角色系统，保留 6 个核心角色，
//! 支持全工具 / 只读 / 只读+执行 / 只读+写 四种能力模式。

/// 子 Agent 角色（精简自 grok-build 的 15 角色）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRole {
    /// 编码实现（全工具）
    Coder,
    /// 审查分析（只读）
    Reviewer,
    /// 调研分析（只读）
    Researcher,
    /// 架构设计（只读）
    Architect,
    /// 通用（全工具）
    GeneralPurpose,
    /// 探索（只读 4 工具）
    Explore,
}

/// 能力模式（参考 grok-build）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityMode {
    /// 全工具
    All,
    /// 只读（read / grep / glob / list）
    ReadOnly,
    /// 读 + 执行（read + execute）
    Execute,
    /// 读 + 写（read + write + edit）
    ReadWrite,
}

/// 子 Agent 配置
#[derive(Debug, Clone)]
pub struct SubagentConfig {
    pub role: AgentRole,
    pub model: String,
    pub max_turns: usize,
    pub capability_mode: CapabilityMode,
}

/// 子 Agent 任务委托结果
#[derive(Debug, Clone)]
pub struct SubagentResult {
    pub role: AgentRole,
    pub summary: String,
    pub success: bool,
    pub turns_used: usize,
}

impl AgentRole {
    /// 返回该角色的默认配置（推荐模型 + 能力模式）
    pub fn default_config(&self) -> SubagentConfig {
        match self {
            AgentRole::Coder => SubagentConfig {
                role: *self,
                model: "longcat".to_string(),
                max_turns: 30,
                capability_mode: CapabilityMode::All,
            },
            AgentRole::Reviewer => SubagentConfig {
                role: *self,
                model: "deepseek-pro".to_string(),
                max_turns: 15,
                capability_mode: CapabilityMode::ReadOnly,
            },
            AgentRole::Researcher => SubagentConfig {
                role: *self,
                model: "deepseek-flash".to_string(),
                max_turns: 20,
                capability_mode: CapabilityMode::ReadOnly,
            },
            AgentRole::Architect => SubagentConfig {
                role: *self,
                model: "mimo-pro".to_string(),
                max_turns: 15,
                capability_mode: CapabilityMode::ReadOnly,
            },
            AgentRole::GeneralPurpose => SubagentConfig {
                role: *self,
                model: "longcat".to_string(),
                max_turns: 25,
                capability_mode: CapabilityMode::All,
            },
            AgentRole::Explore => SubagentConfig {
                role: *self,
                model: "deepseek-flash".to_string(),
                max_turns: 12,
                capability_mode: CapabilityMode::ReadOnly,
            },
        }
    }

    /// 生成对应的中文系统提示词
    pub fn system_prompt(&self) -> &str {
        match self {
            AgentRole::Coder => "\
你是软件开发者。编写健壮、可测试的代码，遵循项目现有风格。
输出改动的最小 diff，不做无关重构。先读后写，编译后再提交。",
            AgentRole::Reviewer => "\
你是严格审查员。使用 CoVe 验证链审查代码质量与事实准确性。
逐条指出具体问题，不泛泛而谈。只读工具，不修改代码。",
            AgentRole::Researcher => "\
你是调研分析师。结构化输出调研结果，结论前置。
先广泛搜索再深挖关键来源，标注每项结论的信息来源。只读工具。",
            AgentRole::Architect => "\
你是软件架构师。以约束驱动设计，每个决策标注权衡理由。
输出架构方案时附带接口定义和模块边界。只读工具。",
            AgentRole::GeneralPurpose => "\
完成分配的任务，不多不少。先理解目标再动手，
遇到模糊指令主动澄清。可用全部工具。",
            AgentRole::Explore => "\
你是快速代码库探索器。先广搜定位关键文件，再深挖具体实现。
输出文件路径 + 关键代码片段，不做修改。只读工具。",
        }
    }

    /// 返回该角色能力模式对应的可用工具列表
    pub fn capability_tools(&self) -> Vec<&str> {
        let config = self.default_config();
        match config.capability_mode {
            CapabilityMode::All => vec![
                "read", "write", "edit", "exec", "grep", "glob", "list",
            ],
            CapabilityMode::ReadOnly => vec!["read", "grep", "glob", "list"],
            CapabilityMode::Execute => vec![
                "read", "grep", "glob", "list", "exec",
            ],
            CapabilityMode::ReadWrite => vec![
                "read", "write", "edit", "grep", "glob", "list",
            ],
        }
    }
}
