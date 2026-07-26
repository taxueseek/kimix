//! Kimix 界面多语言（i18n）。
//!
//! 规则：
//! - 根据系统语言自动选择界面语言，支持简体中文与英文
//! - 系统语言为中文时显示中文，其他语言一律显示英文
//! - 指令、快捷键名（/help、Ctrl+N、Enter 等）在两种语言下均保持英文
//!
//! 语言解析优先级：KIMIXI_LANG / KIMIX_LANG 环境变量
//!   > ~/.kimix/config.toml `[ui].language`（"auto" 或未设置时跳过）
//!   > LC_ALL > LC_MESSAGES > LANG 系统探测。
//!
//! 对存量英文文案采用 gettext 风格：渲染处调用 [`tr`]，中文查表，
//! 英文原样透传。新文案直接走 [`Strings`] 字段。
use std::sync::atomic::{AtomicU8, Ordering};

/// 界面语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En = 0,
    Zh = 1,
}

static CURRENT: AtomicU8 = AtomicU8::new(0);
static LOADED: AtomicU8 = AtomicU8::new(0);

impl Lang {
    fn from_u8(v: u8) -> Self {
        if v == 1 { Lang::Zh } else { Lang::En }
    }

    /// 系统环境探测：仅识别 zh 前缀，其余一律英文。
    pub fn detect() -> Self {
        for var in ["KIMIXI_LANG", "KIMIX_LANG", "LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(val) = std::env::var(var) {
                let val = val.trim();
                if val.is_empty() || val == "C" || val == "POSIX" {
                    continue;
                }
                return if val.to_lowercase().starts_with("zh") {
                    Lang::Zh
                } else {
                    Lang::En
                };
            }
        }
        Lang::En
    }

    /// 解析配置 / 指令中的语言名。
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "zh" | "zh-cn" | "zh-hans" | "chinese" | "中文" => Some(Lang::Zh),
            "en" | "en-us" | "en-gb" | "english" | "英文" => Some(Lang::En),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::Zh => "zh",
            Lang::En => "en",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Lang::Zh => "中文",
            Lang::En => "English",
        }
    }
}

/// 从配置文件读取 `[ui].language`（失败静默返回 None）。
fn config_language() -> Option<Lang> {
    let root = kimix_shell::config::load_effective_config().ok()?;
    let lang = root.get("ui")?.get("language")?.as_str()?;
    if lang.trim().eq_ignore_ascii_case("auto") {
        return None;
    }
    Lang::parse(lang)
}

/// 当前界面语言（惰性初始化：环境变量 > 配置文件 > 系统探测）。
pub fn current() -> Lang {
    if LOADED.swap(1, Ordering::AcqRel) == 0 {
        let lang = if std::env::var("KIMIXI_LANG").is_ok() || std::env::var("KIMIX_LANG").is_ok() {
            Lang::detect()
        } else {
            config_language().unwrap_or_else(Lang::detect)
        };
        CURRENT.store(lang as u8, Ordering::Release);
    }
    Lang::from_u8(CURRENT.load(Ordering::Acquire))
}

/// 运行时切换语言（/lang 指令调用）。
pub fn set_current(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Release);
    LOADED.store(1, Ordering::Release);
}

/// 测试辅助：重置惰性初始化状态。
#[cfg(any(test, feature = "test-support"))]
pub fn reset_for_test() {
    CURRENT.store(0, Ordering::Release);
    LOADED.store(0, Ordering::Release);
}

/// gettext 风格翻译：英文原样返回；中文查表，未命中也返回英文。
///
/// 表内只收录主界面高频文案；未收录的英文直接显示（可接受的渐进覆盖）。
pub fn tr(en: &'static str) -> &'static str {
    match current() {
        Lang::En => en,
        Lang::Zh => zh_lookup(en).unwrap_or(en),
    }
}

/// [`tr`] 的 owned 版本：用于运行期才持有的 String（如面板条目）。
pub fn tr_owned(en: &str) -> String {
    match current() {
        Lang::En => en.to_string(),
        Lang::Zh => zh_lookup(en).unwrap_or(en).to_string(),
    }
}

/// [`tr`] 的零分配版本：返回借用（命中静态中文表或原英文）。
pub fn tr_cow(en: &str) -> std::borrow::Cow<'_, str> {
    match current() {
        Lang::En => std::borrow::Cow::Borrowed(en),
        Lang::Zh => std::borrow::Cow::Borrowed(zh_lookup(en).unwrap_or(en)),
    }
}

/// 带占位符的翻译：模板键 `{name}` 按序替换。
///
/// ```
/// # use kimix_tui::i18n::tr_fmt;
/// let s = tr_fmt("{count} queued", &[("count", "3")]);
/// ```
pub fn tr_fmt(en: &'static str, args: &[(&str, &str)]) -> String {
    let mut s = tr(en).to_string();
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

/// 中文文案表（键为英文原文）。
fn zh_lookup(en: &str) -> Option<&'static str> {
    Some(match en {
        // ── 欢迎屏 ──────────────────────────────────────────────
        "New worktree" => "新建工作树",
        "Resume session" => "恢复会话",
        "Quit" => "退出",
        "Import Claude settings" => "导入 Claude 设置",
        "Found official kimi-cli settings in ~/.kimi." => {
            "发现官方 kimi-cli 设置（位于 ~/.kimi）。"
        }
        "Run `kimix import-kimi` to import them (one-time)." => {
            "运行 `kimix import-kimi` 即可一次性导入。"
        }
        "Thanks for trying Kimix, give feedback with /feedback!" => {
            "感谢试用 Kimix，欢迎用 /feedback 反馈！"
        }

        // ── 命令面板分区与条目 ──────────────────────────────────
        "Commands" => "指令",
        "Session" => "会话",
        "New Session" => "新建会话",
        "New Session in Worktree" => "在工作树中新建会话",
        "Agent Dashboard" => "Agent 仪表盘",
        "Back to Home" => "返回主页",
        "Resume Session" => "恢复会话",
        "Rename Session" => "重命名会话",
        "Session Info" => "会话信息",
        "Send Feedback" => "发送反馈",
        "Context" => "上下文",
        "Compact History" => "压缩历史",
        "Context Usage" => "上下文用量",
        "View Plan" => "查看计划",
        "Memory" => "记忆",
        "Model & Input" => "模型与输入",
        "Switch Model" => "切换模型",
        "Always Approve Mode" => "自动批准模式",
        "Multiline Input" => "多行输入",
        "Tools" => "工具",
        "Hooks" => "Hooks",
        "Plugins" => "插件",
        "Skills" => "技能",
        "MCP Servers" => "MCP 服务器",
        "Manage Agents" => "管理 Agent",
        "Other" => "其他",
        "Switch Theme" => "切换主题",
        "Switch Language" => "切换语言",
        "How-to Guides" => "使用指南",
        "Theme" => "主题",
        "Language" => "语言",
        "Pick model" => "选择模型",
        "Pick theme" => "选择主题",
        "Pick language" => "选择语言",
        "Pick option" => "选择选项",
        "Settings" => "设置",
        "Keyboard Shortcuts" => "键盘快捷键",
        "Exit" => "退出",

        // ── 面板通用件 ─────────────────────────────────────────
        " search: " => " 搜索: ",
        "No matches" => "无匹配",
        "press again to " => "再按一次以",
        "nav" => "移动",
        "select" => "选择",
        "close" => "关闭",
        "search" => "搜索",
        "expand" => "展开",
        "copy" => "复制",
        "toggle" => "切换",
        "reload" => "重载",
        "add" => "添加",
        "delete" => "删除",
        "cancel" => "取消",
        "confirm" => "确认",
        "send" => "发送",
        "quit" => "退出",

        // ── 快捷键标签 ─────────────────────────────────────────
        "back" => "返回",
        "approve" => "批准",
        "plan" => "计划",
        "comment" => "评论",
        "fullscreen" => "全屏",
        "accept" => "接受",
        "accept suggestion" => "接受建议",
        "answer" => "回答",
        "apply" => "应用",
        "clear" => "清除",
        "clear search" => "清除搜索",
        "copy cmd" => "复制命令",
        "copy output" => "复制输出",
        "copy path" => "复制路径",
        "copy pattern" => "复制模式",
        "copy query" => "复制查询",
        "copy url" => "复制链接",
        "create" => "创建",
        "dashboard" => "仪表盘",
        "delete row" => "删除行",
        "drill" => "深入",
        "dismiss" => "关闭",
        "edit" => "编辑",
        "filename" => "文件名",
        "filter" => "筛选",
        "fire" => "触发",
        "fwd" => "前进",
        "go" => "前往",
        "goto" => "跳转",
        "input" => "输入",
        "keep filter" => "保持筛选",
        "keep running" => "继续运行",
        "kill" => "终止",
        "lines" => "行模式",
        "list" => "列表",
        "mode" => "模式",
        "New Agent" => "新建 Agent",
        "newline" => "换行",
        "open" => "打开",
        "paste" => "粘贴",
        "prompt" => "提示词",
        "quit plan" => "退出计划",
        "raw" => "原始",
        "reorder" => "重排",
        "save" => "保存",
        "scope" => "范围",
        "scrollback" => "回滚",
        "send now" => "立即发送",
        "send to bg" => "后台发送",
        "send+open" => "发送并打开",
        "shortcuts" => "快捷键",
        "submit" => "提交",
        "switch tab" => "切换标签",
        "top/btm" => "首/末",
        "turn" => "轮次",
        "unselect" => "取消选择",
        "view" => "查看",
        "wrap" => "换行",
        "save comment" => "保存评论",
        "request changes" => "请求修改",
        "always-approve" => "自动批准",

        // ── 状态栏 ─────────────────────────────────────────────
        "Turn" => "轮次",
        "Plan" => "计划",
        "Waiting" => "等待中",
        "Thinking" => "思考中",
        "Streaming" => "生成中",

        // ── 欢迎屏补充 ─────────────────────────────────────────
        "Yes, proceed" => "是，继续",
        "No, quit" => "否，退出",
        "Run Kimix in a project directory?" => "在项目目录中运行 Kimix？",
        "This gives Kimix full context of your codebase for better results." => {
            "这将让 Kimix 获得完整的代码库上下文，以获得更好的结果。"
        }
        "Type a message..." => "输入消息…",
        "Build anything" => "想构建什么？",
        "detached" => "游离",
        "worktree " => "工作树 ",
        "{display} (worktree of {main_repo})" => "{display}（{main_repo} 的工作树）",
        "new in worktree" => "在工作树中新建",
        "close this session" => "关闭此会话",

        // ── 信任对话框 ─────────────────────────────────────────
        "Do you trust the contents of this directory?" => "信任此目录中的内容吗？",
        "Kimix may run or modify contents in this directory," => {
            "Kimix 可能运行或修改此目录中的内容，"
        }
        "posing security risks." => "存在安全风险。",

        // ── 登录 / 认证流程 ────────────────────────────────────
        "A browser window will open for authentication." => "浏览器窗口将打开，用于登录认证。",
        "Approve in your browser to finish signing in." => "在浏览器中批准以完成登录。",
        "Make sure your browser shows this code." => "确认浏览器中显示的验证码与此一致。",
        "If it doesn't open, click " => "如果没有打开，点击",
        "here" => "这里",
        " to copy." => "复制。",
        "Copying not working? Click here to show full URL." => "无法复制？点击这里显示完整 URL。",
        "copied!" => "已复制！",
        "Select the URL below with your mouse and copy manually." => {
            "用鼠标选中下方的 URL 并手动复制。"
        }
        "Waiting for login to complete..." => "等待登录完成…",
        "Waiting for approval..." => "等待批准…",
        "Waiting for auth URL..." => "等待认证链接…",
        "Paste your token here..." => "在此粘贴令牌…",
        "Paste your API key here..." => "在此粘贴 API key…",
        "Paste your Moonshot API key (from {host})" => "粘贴你的 Moonshot API key（来自 {host}）",
        "Login with {label}" => "登录 {label}",
        "Connecting..." => "连接中…",
        "go back" => "返回",

        // ── 模态进行中状态 ─────────────────────────────────────
        "Updating..." => "更新中…",
        "Processing..." => "处理中…",
        "authenticating..." => "认证中…",
        "enabling..." => "启用中…",
        "disabling..." => "禁用中…",

        // ── 上下文用量 ─────────────────────────────────────────
        "System prompt" => "系统提示词",
        "Messages" => "消息",

        // ── 设置选项 ─────────────────────────────────────────
        "(no override)" => "（不覆盖）",
        "Inherit the default model (no per-user override)." => "继承默认模型（不覆盖用户设置）。",
        "Follow system dark/light appearance." => "跟随系统深色/浅色外观。",
        "Neutral dark with magenta accent." => "中性深色配洋红色。",
        "Light theme for bright environments." => "适用于明亮环境的浅色主题。",
        "Dark + blue-tinted; needs truecolor." => "深色带蓝色调，需要 truecolor。",
        "Muted dark with mauve accents; needs truecolor." => "柔和深色配紫红色，需要 truecolor。",
        "Deep dark with warm accents; needs truecolor." => "深色配暖色，需要 truecolor。",
        "Dark plum with sakura-pink accents. 暗夜樱花。" => "深梅色配樱花粉。",
        "Dark green with jade accents. 暗夜森林。" => "深绿色配翡翠色。",
        "Deep space black with cold blue. 月之暗面。" => "深空黑配冷蓝色。",
        "Moonlit white with warm silver. 月之亮面。" => "月光白配暖银色。",
        "Deep crimson with blood red glow. 红月。" => "深红色配血红光晕。",
        "Grok inspired dark theme." => "Grok 风格深色主题。",
        "Deep ocean blue whale theme. 蓝鲸。" => "深海蓝鲸主题。",
        "Use the agent's default permission behavior (currently equivalent to Ask)." => "使用 Agent 默认权限行为（当前等同于询问）。",
        "Prompt for permission before tool actions." => "工具操作前提示权限。",
        "LLM classifier approves safe tools; dangerous actions may still prompt or deny." => "LLM 分类器批准安全工具；危险操作可能仍需提示或拒绝。",
        "Auto-approve every tool action. Skips ALL permission prompts." => "自动批准所有工具操作，跳过所有权限提示。",
        "Follow the system locale. 跟随系统语言。" => "跟随系统语言。",
        "Simplified Chinese interface. 简体中文界面。" => "简体中文界面。",
        "English interface. 英文界面。" => "英文界面。",
        "Agent runs tools and edits files directly (default)." => "Agent 直接运行工具和编辑文件（默认）。",
        "Agent summarises a plan and asks for approval before running tools." => "Agent 汇总计划并请求批准后再运行工具。",
        "Show diagrams with a clickable row to open/copy the rendered image." => "显示图表，提供可点击行以打开/复制渲染图像。",
        "Same as auto: always show the clickable affordance row." => "同自动：始终显示可点击行。",
        "Always show the raw Mermaid source as a code block." => "始终以代码块显示原始 Mermaid 源码。",
        "Detect wheel vs trackpad per gesture from event timing. Default." => "根据事件时间检测滚轮或触控板。默认。",

        // ── 文档标题 ─────────────────────────────────────────
        "Getting Started" => "快速入门",
        "Installation, first launch, and basic interaction" => "安装、首次启动和基本交互",
        "Authentication" => "认证",
        "Browser login, API keys, OIDC, external auth providers" => "浏览器登录、API 密钥、OIDC、外部认证",
        "Keyboard Shortcuts" => "键盘快捷键",
        "Complete reference for all TUI key bindings" => "所有 TUI 快捷键的完整参考",
        "Slash Commands" => "斜杠命令",
        "All / commands for session management, models, memory, hooks" => "所有 / 命令：会话管理、模型、记忆、钩子",
        "Configuration" => "配置",
        "config.toml, pager.toml, environment variables, file locations" => "config.toml、pager.toml、环境变量、文件位置",
        "Theming and Appearance" => "主题与外观",
        "Themes, color support, pager.toml customization" => "主题、颜色支持、pager.toml 自定义",
        "MCP Servers" => "MCP 服务器",
        "Setting up external tool integrations via MCP" => "通过 MCP 设置外部工具集成",
        "Skills" => "技能",
        "Creating and using reusable prompt packages" => "创建和使用可复用的提示词包",
        "Plugins" => "插件",
        "Installing, managing, and creating plugin packages" => "安装、管理和创建插件包",
        "Hooks" => "钩子",
        "Project lifecycle scripts for pre/post tool-use events" => "工具使用前后的项目生命周期脚本",
        "Custom Models" => "自定义模型",
        "BYOK, Ollama, OpenAI-compatible endpoints" => "自带密钥、Ollama、OpenAI 兼容端点",
        "Project Rules (AGENTS.md)" => "项目规则 (AGENTS.md)",
        "Per-directory instructions and precedence rules" => "目录级指令和优先级规则",
        "Cross-session knowledge persistence and search" => "跨会话知识持久化和搜索",
        "Headless Mode and Scripting" => "无头模式和脚本",
        "Non-interactive CLI for automation and CI/CD" => "用于自动化和 CI/CD 的非交互式 CLI",
        "Agent Mode and IDE Integration" => "Agent 模式和 IDE 集成",
        "Subagents and Personas" => "子代理和角色",
        "Spawning parallel child agents with specialized roles" => "生成具有专门角色的并行子代理",
        "Session Management" => "会话管理",
        "Save, load, resume, rewind, and compact sessions" => "保存、加载、恢复、回退和压缩会话",
        "Sandbox Mode" => "沙盒模式",
        "Structured planning with approval dialogs" => "带审批对话框的结构化计划",
        "Plan Mode" => "计划模式",
        "Background Tasks and Monitoring" => "后台任务和监控",
        "Background commands, /loop, monitor, scheduler" => "后台命令、/loop、监控、调度器",
        "Terminal Support and Troubleshooting" => "终端支持和故障排除",
        "Permissions and Safety" => "权限和安全",
        "Tool approval, sandbox, security" => "工具审批、沙盒、安全",
        "Hooks & Plugins Guide" => "钩子和插件指南",
        "Using hooks and plugins" => "使用钩子和插件",
        "Sandbox & Plan Mode" => "沙盒和计划模式",
        "Isolating and approving tool use" => "隔离和审批工具使用",
        "Agent & IDE Integration" => "Agent 和 IDE 集成",
        "Editor integration, subagents, personas" => "编辑器集成、子代理、角色",
        "Sessions & Memory" => "会话和记忆",
        "Persistence, search, context budget" => "持久化、搜索、上下文预算",
        "Background Tasks & Scheduler" => "后台任务和调度器",
        "Loop, monitor, scheduler" => "循环、监控、调度器",
        "Advanced Troubleshooting" => "高级故障排除",
        "Terminal, permissions, diagnostics" => "终端、权限、诊断",
        "Session Digger" => "会话挖掘器",
        "Analyze and compare sessions" => "分析和比较会话",
        "Hooks & Plugins Quick Reference" => "钩子和插件快速参考",
        "Quick reference for hooks and plugins" => "钩子和插件快速参考",
        "Sandbox & Plan Mode Quick Reference" => "沙盒和计划模式快速参考",
        "Quick reference for sandbox and plan mode" => "沙盒和计划模式快速参考",
        "Agent & IDE Quick Reference" => "Agent 和 IDE 快速参考",
        "Quick reference for agent and IDE integration" => "Agent 和 IDE 集成快速参考",
        "Sessions & Memory Quick Reference" => "会话和记忆快速参考",
        "Quick reference for sessions and memory" => "会话和记忆快速参考",
        "Background Tasks Quick Reference" => "后台任务快速参考",
        "Quick reference for background tasks" => "后台任务快速参考",
        "Troubleshooting Quick Reference" => "故障排除快速参考",
        "Quick reference for troubleshooting" => "故障排除快速参考",
        "CLI Reference" => "CLI 参考",
        "Command-line flags and options" => "命令行标志和选项",
        "Config Reference" => "配置参考",
        "All config.toml keys" => "所有 config.toml 键",
        "Hooks Reference" => "钩子参考",
        "Hook events and payload format" => "钩子事件和载荷格式",
        "Plugins Reference" => "插件参考",
        "Plugin manifest and capabilities" => "插件清单和能力",
        "Skills Reference" => "技能参考",
        "SKILL.md format and conventions" => "SKILL.md 格式和规范",
        "MCP Reference" => "MCP 参考",
        "MCP protocol and server setup" => "MCP 协议和服务器设置",
        "Reasoning/overhead" => "推理/开销",
        "Free" => "空闲",
        "Tool definitions" => "工具定义",
        "Cache hit rate" => "缓存命中率",
        "缓存{pct}%" => "缓存{pct}%",

        // ── /lang 指令 ─────────────────────────────────────────
        "Switch UI language" => "切换界面语言",
        "Usage: /lang zh or /lang en" => "用法：/lang zh 或 /lang en",

        // ── 权限管理 ─────────────────────────────────────────────
        "Allow" => "允许",
        "Reject" => "拒绝",
        "Always" => "总是",
        "Once" => "仅一次",
        "Always allow" => "总是允许",
        "Always reject" => "总是拒绝",
        "plan approval" => "计划审批",
        "commenting" => "评论中",
        "Decision" => "决策",
        "Auto" => "自动",
        "Allow once" => "允许一次",
        "Reject once" => "拒绝一次",
        "permission" => "权限",
        "Permission" => "权限",
        "to choose permission scope" => "选择权限范围",
        "Allow Edit?" => "允许编辑？",
        "Allow Bash?" => "允许执行命令？",
        "Allow MCP?" => "允许 MCP 工具？",
        "All tools from" => "所有工具来自",
        "this tool" => "此工具",
        "this server" => "此服务器",
        "all servers" => "所有服务器",

        // ── 主题名称 ─────────────────────────────────────────────
        "Moon Dark" => "月之暗面",
        "Moon Light" => "月之亮面",
        "Blood Moon" => "红月",
        "Grok Dark" => "Grok 暗黑",
        "DeepSeek Blue" => "DeepSeek 蓝鲸",
        "Kimix Night" => "Kimix 暗夜",
        "Kimix Day" => "Kimix 白昼",
        "Tokyo Night" => "东京之夜",
        "Rose Pine Moon" => "玫瑰松月",
        "Sakura" => "樱花",
        "Forest" => "森林",

        // ── 环境诊断 ─────────────────────────────────────────────
        "Clipboard may be unreachable." => "剪贴板可能无法访问。",
        "See /terminal-setup for potential fixes." => "查看 /terminal-setup 获取修复方案。",
        "Copies need this terminal to stay focused." => "复制操作需要终端保持聚焦。",
        "See /terminal-setup for details." => "查看 /terminal-setup 获取详情。",
        "Shift+Enter newlines need a WezTerm config change." => {
            "Shift+Enter 换行需要修改 WezTerm 配置。"
        }
        "See /terminal-setup for the fix." => "查看 /terminal-setup 获取修复方法。",
        "Unset NO_COLOR and restart Kimix." => "取消 NO_COLOR 设置并重启 Kimix。",
        "Persist in ~/.zshrc / ~/.bashrc and restart Kimix." => {
            "写入 ~/.zshrc / ~/.bashrc 并重启 Kimix。"
        }

        // ── 设置面板：分类 ─────────────────────────────────────
        "Appearance" => "外观",
        "Mouse" => "鼠标",
        "Editor & Input" => "编辑与输入",
        "Agent & Approval" => "Agent 与审批",
        "Models" => "模型",
        "Advanced" => "高级",

        // ── 设置面板：设置项标签 ───────────────────────────────
        "Compact mode" => "紧凑模式",
        "Default screen mode" => "默认屏幕模式",
        "Show timestamps" => "显示时间戳",
        "Timeline sidebar" => "时间线侧栏",
        "Disable vim input mode" => "禁用 vim 输入",
        "Vim scrollback navigation" => "Vim 回滚导航",
        "Auto dark theme" => "自动深色主题",
        "Auto light theme" => "自动浅色主题",
        "Render Mermaid diagrams" => "渲染 Mermaid 图表",
        "Permission mode" => "权限模式",
        "Remember tool approvals" => "记住工具批准",
        "Multiline" => "多行输入",
        "Default model" => "默认模型",
        "Max thoughts width" => "思考面板宽度",
        "Show thinking blocks" => "显示思考块",
        "Prompt suggestions" => "提示词建议",
        "Respect manual folds" => "保留手动折叠",
        "Group tool calls" => "分组工具调用",
        "Collapsed edit blocks" => "折叠编辑块",
        "Match display refresh rate" => "匹配屏幕刷新率",
        "Scroll speed" => "滚动速度",
        "Scroll input" => "滚动输入方式",
        "Scroll lines" => "滚动行数",
        "Invert scroll" => "反转滚动方向",
        "Text selection" => "文本选择",
        "Default selected permission" => "默认选中权限",
        "Ask-Question timeout" => "提问超时",
        "Plan mode" => "计划模式",
        "Show tips" => "显示提示",
        "Show contextual hints" => "显示情景提示",
        "Auto-update" => "自动更新",
        "Hunk tracker" => "变更追踪",
        "Undo" => "撤销提示",
        "Image input" => "图片输入提示",
        "Send now" => "立即发送提示",
        "Small screen" => "小屏提示",
        "Word select" => "划词选择提示",
        "Fork secondary model" => "分支副模型",

        // ── 设置面板：设置项描述 ───────────────────────────────
        "Reduce padding around messages for more content density. Auto-enabled while the terminal is 20 rows or shorter." => {
            "减少消息周围的留白，提高内容密度。终端不足 20 行时自动启用。"
        }
        "How plain kimix opens next time: Fullscreen (default when unset) or Minimal. Writes [ui] screen_mode in config.toml. Restart required. Switch this session only with /minimal or /fullscreen." => {
            "下次启动时的打开方式：全屏（未设置时默认）或精简模式。写入 config.toml 的 [ui] screen_mode，需重启生效。本次会话可用 /minimal 或 /fullscreen 切换。"
        }
        "Show clock time next to user messages and agent responses." => {
            "在用户消息和 Agent 回复旁显示时间。"
        }
        "Per-turn tick rail in place of the scrollbar: hover previews a turn, click jumps to it." => {
            "用逐轮刻度轨替代滚动条：悬停预览轮次，点击跳转。"
        }
        "Use plain readline-style input instead of vim keys in the prompt. Experimental." => {
            "输入框使用 readline 风格输入，不用 vim 按键。实验性功能。"
        }
        "Enable vim keys (h/j/k/l, gg/G, /) for navigating the scrollback. Does not affect the input prompt." => {
            "启用 vim 按键（h/j/k/l、gg/G、/）浏览回滚区，不影响输入框。"
        }
        "Color theme for the pager UI." => "界面配色主题。",
        "Interface language. Auto follows the system locale (Chinese systems show 中文, others show English)." => {
            "界面语言。自动跟随系统语言（中文系统显示中文，其他显示英文）。"
        }
        "Theme to use when the system is in dark mode (only with theme=auto)." => {
            "系统为深色模式时使用的主题（仅当 theme=auto 时生效）。"
        }
        "Theme to use when the system is in light mode (only with theme=auto)." => {
            "系统为浅色模式时使用的主题（仅当 theme=auto 时生效）。"
        }
        "How ```mermaid code blocks are shown: auto/on add a clickable row to open the rendered diagram; off shows the raw source." => {
            "mermaid 代码块的显示方式：auto/on 会附加一行可点击入口以打开渲染图表，off 显示原始源码。"
        }
        "Default uses the agent's built-in behavior; Ask prompts for each tool action; Auto uses an LLM classifier for risky tools; Always approve grants all permissions automatically." => {
            "Default 使用 Agent 内置行为；Ask 对每个工具操作征求确认；Auto 用 LLM 分类器判断高风险工具；Always approve 自动授予所有权限。"
        }
        "Show \"Always allow\" options in permission prompts so you can stop being re-asked about a specific command or tool. Applies in ask and auto; Always-approve still skips all prompts. Restart required." => {
            "在权限提示中显示「总是允许」选项，避免对特定命令或工具重复询问。对 ask 和 auto 生效；Always-approve 本就跳过所有提示。需重启生效。"
        }
        "When on, Enter inserts a newline and Shift+Enter sends. Resets each session." => {
            "开启后 Enter 插入换行、Shift+Enter 发送。每次会话重置。"
        }
        "Model used for new sessions. Changing this also switches the active session. Pick `(no override)` to clear." => {
            "新会话使用的模型。修改后同时切换当前会话。选择 (no override) 可清除。"
        }
        "Column width budget for the agent's thoughts panel (40-500, default 120)." => {
            "Agent 思考面板的列宽上限（40-500，默认 120）。"
        }
        "Show agent thinking/reasoning blocks in the scrollback while streaming." => {
            "流式输出时在回滚区显示 Agent 的思考/推理块。"
        }
        "After each turn, predict your likely next prompt and show it as ghost text in the input (Tab to accept). Uses a small model call per turn." => {
            "每轮结束后预测你可能的下一条提示词，以幽灵文本显示在输入框（按 Tab 接受）。每轮消耗一次小模型调用。"
        }
        "Keep manually folded blocks as-is while streaming and stop auto-scroll when expanding a block. Experimental." => {
            "流式输出时保留手动折叠的块，展开块时停止自动滚动。实验性功能。"
        }
        "Fold consecutive read/search/list tool calls and subagent rows into one summary row; finished thoughts fold into the group too." => {
            "把连续的 read/search/list 工具调用和子 Agent 行折叠成一行摘要；完成的思考也折叠进组内。"
        }
        "Show edits as one-line +N/-M diffstat summaries and merge back-to-back edits to the same file into one block; expand a row to see the diffs." => {
            "把编辑显示为单行 +N/-M 增删统计，并合并对同一文件的连续编辑；展开可查看具体差异。"
        }
        "On high-refresh displays, the TUI will stream/scroll faster to match the display. Off keeps the classic ~60 Hz cadence. Restart required." => {
            "在高刷新率显示器上加快流式输出与滚动以匹配屏幕。关闭则保持约 60 Hz 的经典节奏。需重启生效。"
        }
        "Mouse-wheel and trackpad scroll speed multiplier (1-100). Higher = faster." => {
            "滚轮与触控板的滚动速度倍率（1-100），数值越大越快。"
        }
        "Force wheel or trackpad scroll behavior when auto-detection misreads your device." => {
            "自动检测误判设备时，强制使用滚轮或触控板的滚动行为。"
        }
        "Lines per scroll tick for both wheel and trackpad (1-10). Until set, each terminal's own profile applies." => {
            "滚轮与触控板每次滚动的行数（1-10）。未设置时使用各终端自身的配置。"
        }
        "Reverse vertical scroll direction (natural scrolling)." => {
            "反转垂直滚动方向（自然滚动）。"
        }
        "How long in-app selection stays on screen and what double-click does (fold vs. select & copy a word). For your terminal or multiplexer's own selection, hold Shift while dragging (native copy)." => {
            "应用内选区在屏幕上保留的时长，以及双击的行为（折叠或选词复制）。如需使用终端或多路复用器自身的选区，按住 Shift 拖动即可（原生复制）。"
        }
        "Which row the cursor preselects on permission prompts." => "权限提示中光标预先选中的行。",
        "When on, the ask_user_question tool will time out after a set period of time instead of infinitely blocking." => {
            "开启后，ask_user_question 工具会在超时后自动结束，而不是无限阻塞。"
        }
        "When on, the agent summarises a plan before running tools or making edits." => {
            "开启后，Agent 会先给出计划摘要，再运行工具或修改文件。"
        }
        "Show the tip-of-the-day banner on startup. Restart required." => {
            "启动时显示每日提示横幅。需重启生效。"
        }
        "Show brief, in-context keyboard hints as you work; toggle each one individually." => {
            "工作时显示简短的情景快捷键提示；可逐条开关。"
        }
        "Automatically download and install pager updates on startup. Restart required." => {
            "启动时自动下载并安装更新。需重启生效。"
        }
        "Which file changes the agent tracks as hunks. Off disables tracking (and LOC stats) entirely. Restart required." => {
            "Agent 追踪哪些文件变更（hunk）。Off 完全关闭追踪（及代码行数统计）。需重启生效。"
        }
        "Remind you that Ctrl+Z restores the prompt after you clear it." => {
            "清空输入框后，提示可用 Ctrl+Z 恢复。"
        }
        "Suggest plan mode (Tab) when your prompt looks like a planning request." => {
            "当提示词像是规划需求时，建议使用计划模式（Tab）。"
        }
        "Offer to paste an image when one is on the clipboard and the model accepts images." => {
            "剪贴板中有图片且模型支持图片时，提示粘贴图片。"
        }
        "After you queue a follow-up mid-turn, remind you that Enter on an empty prompt sends the top queued item now." => {
            "轮次进行中排队了后续消息后，提示在空输入框按 Enter 可立即发送队首消息。"
        }
        "Suggest /compact-mode once per run when the terminal is short on rows." => {
            "终端行数不足时，每次运行提示一次 /compact-mode。"
        }
        "After double-clicking conversation text while Text selection is fold/nav, remind you that Word select lives in Settings." => {
            "当文本选择为折叠/导航时双击对话文本后，提示划词选择可在设置中开启。"
        }
        "Model used for the secondary agent when forking. Pick `(no override)` to clear." => {
            "分支时副 Agent 使用的模型。选择 (no override) 可清除。"
        }

        // ── 设置面板：选项显示名 ───────────────────────────────
        "Normal" => "普通",
        "Always-Approve" => "始终批准",
        "Code" => "代码",
        "Default" => "默认",
        "Ask" => "询问",
        "Always approve" => "总是批准",
        "On" => "开",
        "Off" => "关",
        "Fullscreen" => "全屏",
        "Minimal" => "精简",
        "Auto-detect" => "自动检测",
        "Mouse wheel" => "滚轮",
        "Trackpad" => "触控板",
        "Flash after copy" => "复制后闪烁",
        "Hold until dismissed" => "保留直到取消",
        "Word select (terminal-like)" => "划词选择（终端风格）",
        "Agent only" => "仅 Agent",
        "All dirty" => "所有改动",
        "Oscura Midnight" => "幽暗午夜",

        // ── 设置面板：选项描述 ─────────────────────────────────
        "Follow system dark/light appearance." => "跟随系统深浅色外观。",
        "Neutral dark with magenta accent." => "中性深色，品红点缀。",
        "Light theme for bright environments." => "适合明亮环境的浅色主题。",
        "Dark + blue-tinted; needs truecolor." => "深色偏蓝；需要 truecolor 支持。",
        "Muted dark with mauve accents; needs truecolor." => {
            "柔和深色，紫红点缀；需要 truecolor 支持。"
        }
        "Deep dark with warm accents; needs truecolor." => {
            "深邃深色，暖色点缀；需要 truecolor 支持。"
        }
        "Dark plum with sakura-pink accents. 暗夜樱花。" => "深梅底色，樱粉点缀。",
        "Dark green with jade accents. 暗夜森林。" => "深绿底色，翠玉点缀。",
        "Deep space black with cold blue. 月之暗面。" => "深空黑，冷蓝点缀。",
        "Moonlit white with warm silver. 月之亮面。" => "月光白，暖银点缀。",
        "Deep crimson with blood red glow. 红月。" => "深红底色，血红光晕。",
        "Grok inspired dark theme." => "受 Grok 启发的深色主题。",
        "Deep ocean blue whale theme. 蓝鲸。" => "深海蓝鲸主题。",
        "Use the agent's default permission behavior (currently equivalent to Ask)." => {
            "使用 Agent 的默认权限行为（目前等同于 Ask）。"
        }
        "Prompt for permission before tool actions." => "工具操作前征求确认。",
        "LLM classifier approves safe tools; dangerous actions may still prompt or deny." => {
            "LLM 分类器自动批准安全工具；危险操作仍可能询问或拒绝。"
        }
        "Auto-approve every tool action. Skips ALL permission prompts." => {
            "自动批准所有工具操作，跳过全部权限提示。"
        }
        "Follow the system locale. 跟随系统语言。" => "跟随系统语言。",
        "Simplified Chinese interface. 简体中文界面。" => "简体中文界面。",
        "English interface. 英文界面。" => "英文界面。",
        "Agent runs tools and edits files directly (default)." => {
            "Agent 直接运行工具、修改文件（默认）。"
        }
        "Agent summarises a plan and asks for approval before running tools." => {
            "Agent 先给出计划摘要，经确认后再运行工具。"
        }
        "Show diagrams with a clickable row to open/copy the rendered image." => {
            "显示图表，并附一行可点击入口用于打开/复制渲染图。"
        }
        "Same as auto: always show the clickable affordance row." => {
            "同 auto：始终显示可点击的操作入口。"
        }
        "Always show the raw Mermaid source as a code block." => {
            "始终以代码块显示 Mermaid 原始源码。"
        }
        "Detect wheel vs trackpad per gesture from event timing. Default." => {
            "根据事件时序逐次判断滚轮或触控板。默认。"
        }
        "Always treat scrolling as wheel notches (fixed lines per tick)." => {
            "始终把滚动当作滚轮刻度（每次固定行数）。"
        }
        "Always treat scrolling as a trackpad (fractional accumulation)." => {
            "始终把滚动当作触控板（小数累积）。"
        }
        "Brief highlight on mouse-up, then clear. Double-click toggles fold. Default." => {
            "松开鼠标后短暂高亮随即清除。双击切换折叠。默认。"
        }
        "Keep the selection visible until Esc, click, or scroll. Double-click toggles fold." => {
            "选区保留至 Esc、点击或滚动。双击切换折叠。"
        }
        "Double-click selects & copies a word, triple-click a line; selection stays until dismissed." => {
            "双击选中并复制单词，三击选中整行；选区保留直到取消。"
        }
        "Track only files the agent edits (default)." => "仅追踪 Agent 编辑的文件（默认）。",
        "Track every git-dirty file, including external edits." => {
            "追踪所有 git 改动文件，包括外部编辑。"
        }
        "Disable hunk tracking entirely. Also disables LOC tracking." => {
            "完全关闭 hunk 追踪，同时关闭代码行数统计。"
        }
        "Open Kimix in the standard fullscreen TUI. Default when unset." => {
            "以标准全屏 TUI 打开 Kimix。未设置时默认。"
        }
        "Open Kimix in scrollback-native (minimal) mode." => "以回滚原生（精简）模式打开 Kimix。",

        // ── 快捷键帮助：分类标签 ───────────────────────────────
        "Essentials" => "基础",
        "Input" => "输入",
        "Conversation Navigation" => "对话导航",
        "Conversation Actions" => "对话操作",
        "Panels" => "面板",
        "Dashboard" => "仪表盘",

        // ── 快捷键帮助：底部栏 ─────────────────────────────────
        "f filter" => "f 过滤",
        "f show all" => "f 显示全部",
        "e/Space/\u{2192} expand" => "e/Space/\u{2192} 展开",
        "\u{2190} collapse" => "\u{2190} 折叠",
        "Enter details" => "Enter 详情",
        "/ search" => "/ 搜索",
        "Esc close" => "Esc 关闭",
        "Esc back" => "Esc 返回",
        "\u{2191}/\u{2193} nav" => "\u{2191}/\u{2193} 移动",
        "\u{2191}/\u{2193} scroll" => "\u{2191}/\u{2193} 滚动",
        "Ctrl+./X close" => "Ctrl+./X 关闭",
        "(not active in current context)" => "（当前上下文中不可用）",
        "Search scrollback" => "搜索回滚区",
        "Paste images (and text) from the clipboard" => "从剪贴板粘贴图片和文本",

        // ── PASTE_LONG_HELP ─────────────────────────────────────
        "Pastes clipboard images into the prompt as chips, and plain text as typed.\n\
Prefer Ctrl+V. Use Alt+V as a fallback when Ctrl+V fails (some terminals or \
configs drop image clipboards; older Windows Terminal versions only pasted \
text).\n\
You can also drag an image file from Explorer into the prompt." => {
            "将剪贴板中的图片作为切片粘贴到输入框，文本则直接键入。\n\
优先使用 Ctrl+V，若 Ctrl+V 无效可用 Alt+V 作为备用（部分终端或配置会丢弃图片剪贴板；\
旧版 Windows Terminal 仅粘贴文本）。\n\
也可将图片文件从资源管理器拖入输入框。"
        }
        "Pastes clipboard images into the prompt as chips, and plain text as typed.\n\
Use Ctrl+V for screenshots, browser \"Copy Image\", and file-manager image \
copies (many terminals swallow Cmd+V and never deliver it to the TUI).\n\
You can also drag an image file into the prompt." => {
            "将剪贴板中的图片作为切片粘贴到输入框，文本则直接键入。\n\
使用 Ctrl+V 粘贴截图、浏览器「复制图片」和文件管理器中的图片（\
多数终端会拦截 Cmd+V 而不会传递给 TUI）。\n\
也可将图片文件拖入输入框。"
        }
        "Pastes clipboard images into the prompt as chips, and plain text as typed.\n\
Use Ctrl+V for screenshots, browser \"Copy Image\", and file-manager image \
copies.\n\
You can also drag an image file into the prompt." => {
            "将剪贴板中的图片作为切片粘贴到输入框，文本则直接键入。\n\
使用 Ctrl+V 粘贴截图、浏览器「复制图片」和文件管理器中的图片。\n\
也可将图片文件拖入输入框。"
        }

        // ── Agent 视图渲染 ─────────────────────────────────────
        " navigate" => " 移动",
        " question" => " 查看问题",
        " copy" => " 复制",

        // ── 轮次状态 ───────────────────────────────────────────
        "Starting session\u{2026}" => "启动会话中\u{2026}",

        // ── 思考块 / 工具块 / 状态标签 ──────────────────────────
        "Thinking\u{2026}" => "思考中\u{2026}",
        "Thought" => "思考",
        "Recap" => "摘要",
        "Answering\u{2026}" => "回复中\u{2026}",
        "No bundled items." => "暂无捆绑项。",
        "No running tasks. Press " => "无运行中的任务。按 ",
        "Status: " => "状态：",
        "Active Subagent: " => "活跃子 Agent：",
        "Coming from " => "从 ",

        // ── 提示条 (tips) ──────────────────────────────────────
        "Want double-click to select? " => "想双击选中文字？",
        "Tight on space? Try " => "空间不够？试试 ",
        "Planning? Check out plan mode via " => "在规划？试试计划模式 ",
        "Image in clipboard \u{b7} " => "剪贴板中有图片 \u{b7} ",
        "Input cleared \u{b7} " => "输入已清空 \u{b7} ",
        "Queued \u{b7} " => "已排队 \u{b7} ",

        // ── 工具块标签 ─────────────────────────────────────────
        "Search " => "搜索 ",
        "Run " => "运行 ",
        "Skill " => "技能 ",
        "Task " => "任务 ",
        "Subagent " => "子 Agent ",
        "Use " => "使用 ",

        // ── 操作标签（actions/defaults.rs） ───────────────────
        "all" => "全部",
        "bottom" => "底部",
        "close overlay" => "关闭浮层",
        "commands" => "指令",
        "exit" => "退出",
        "expand/collapse thinking" => "展开/折叠思考",
        "extensions" => "扩展",
        "fold" => "折叠",
        "group" => "分组",
        "half page down" => "半页下",
        "half page up" => "半页上",
        "link" => "链接",
        "location" => "位置",
        "model" => "模型",
        "mouse reporting" => "鼠标报告",
        "multiline" => "多行",
        "new" => "新建",
        "next" => "下一个",
        "next session" => "下一会话",
        "page down" => "下页",
        "page up" => "上页",
        "pin" => "固定",
        "prev" => "上一个",
        "prev session" => "上一会话",
        "queue" => "队列",
        "rename" => "重命名",
        "reorder down" => "下移",
        "reorder up" => "上移",
        "response" => "回复",
        "rewind" => "回退",
        "scroll down" => "下滚",
        "scroll up" => "上滚",
        "sessions" => "会话列表",
        "settings" => "设置",
        "stop" => "停止",
        "tasks" => "任务",
        "todos" => "待办",
        "worktree" => "工作树",
        "yolo" => "始终批准",
        "shell" => "命令",

        // ── 操作描述（actions/defaults.rs） ────────────────────
        "Select next entry" => "选择下一个条目",
        "Select previous entry" => "选择上一个条目",
        "Next turn" => "下一轮次",
        "Previous turn" => "上一轮次",
        "Next response" => "下一回复",
        "Previous response" => "上一回复",
        "Go to top" => "跳到顶部",
        "Go to bottom" => "跳到底部",
        "Scroll up one line" => "向上滚动一行",
        "Scroll down one line" => "向下滚动一行",
        "Scroll up half page" => "向上滚动半页",
        "Scroll down half page" => "向下滚动半页",
        "Scroll up one page" => "向上滚动一页",
        "Scroll down one page" => "向下滚动一页",
        "Collapse selected entry" => "折叠选中条目",
        "Expand selected entry" => "展开选中条目",
        "Expand / collapse" => "展开 / 折叠",
        "Expand all / collapse all" => "全部展开 / 全部折叠",
        "Toggle all thinking blocks" => "切换全部思考块",
        "Toggle raw markdown" => "切换原始 Markdown",
        "Copy content" => "复制内容",
        "Copy command / path" => "复制命令 / 路径",
        "Open in viewer" => "在查看器中打开",
        "Next link" => "下一个链接",
        "Previous link" => "上一个链接",
        "Rewind to selected turn" => "回退到选中的轮次",
        "Kill background task" => "终止后台任务",
        "Send" => "发送",
        "Focus prompt" => "聚焦输入框",
        "Focus scrollback" => "聚焦回滚区",
        "Command palette" => "命令面板",
        "Back to dashboard" => "返回仪表盘",
        "Cancel turn" => "取消轮次",
        "Change working directory for new agents" => "为新 Agent 切换工作目录",
        "Close dashboard" => "关闭仪表盘",
        "Cycle dispatch mode" => "切换调度模式",
        "Cycle mode (Normal / Plan / Decision)" => "切换模式（普通 / 计划 / 决策）",
        "Keyboard shortcuts" => "键盘快捷键",
        "New session" => "新建会话",
        "Next session" => "下一会话",
        "Open extensions" => "打开扩展",
        "Open sessions" => "打开会话列表",
        "Open the Agent Dashboard" => "打开 Agent 仪表盘",
        "Open the settings modal" => "打开设置面板",
        "Pin / unpin agent" => "固定 / 取消固定 Agent",
        "Previous session" => "上一会话",
        "Rename agent" => "重命名 Agent",
        "Reorder agent down" => "下移 Agent",
        "Reorder agent up" => "上移 Agent",
        "Select next row" => "选择下一行",
        "Select previous row" => "选择上一行",
        "Send now while running (cancels the current turn)" => "立即发送（取消当前轮次）",
        "Send running task to background" => "将运行中的任务发送到后台",
        "Shell mode (type ! on empty prompt)" => "命令模式（在空输入框输入 !）",
        "Show shortcuts overlay" => "显示快捷键浮层",
        "Stop / Close agent" => "停止 / 关闭 Agent",
        "Stop agent, close session (back to dashboard)" => "停止 Agent，关闭会话（返回仪表盘）",
        "Toggle always-approve" => "切换始终批准",
        "Toggle mouse reporting (native copy/paste)" => "切换鼠标报告（原生复制/粘贴）",

        // ── 任务面板 ───────────────────────────────────────────
        "Subagents" => "子 Agent",
        "Tasks" => "任务",
        "Watchers" => "监视器",
        "Monitor" => "监控",
        "No tasks or agents." => "暂无任务或 Agent。",
        " to show all." => " 查看全部。",
        "No agents yet, type a prompt to start one." => "暂无 Agent，输入提示词即可启动。",
        "No agents match `a:{n}` — press Esc to clear the filter." => {
            "没有匹配 `a:{n}` 的 Agent — 按 Esc 清除筛选。"
        }
        "No agents in state `{}` — press Esc to clear the filter." => {
            "没有处于 `{}` 状态的 Agent — 按 Esc 清除筛选。"
        }

        // ── 导入 Claude 设置弹窗 ───────────────────────────────
        "Permissions" => "权限",
        "Env vars" => "环境变量",
        "Paths" => "路径",
        "Esc cancel" => "Esc 取消",
        "Switch model" => "切换模型",

        // ── 跳转轮次 ───────────────────────────────────────────
        "Jump to which turn?" => "跳转到哪一轮？",

        // ── 设置面板底部栏 ─────────────────────────────────────
        "Backspace edit" => "Backspace 编辑",
        "Enter commit" => "Enter 提交",
        "Esc clear" => "Esc 清除",

        // ── 子 Agent 目录 ──────────────────────────────────────
        "Personas" => "人设",
        "Roles" => "角色",
        "Agents" => "Agent",

        // ── 会话选择器 ─────────────────────────────────────────
        "All" => "全部",
        "Local" => "本地",
        "Remote" => "远程",
        "External" => "外部",

        // ── 计划审批 ───────────────────────────────────────────
        "Waiting on plan approval" => "等待计划审批",
        "No plan written — approve or request changes" => "未编写计划 — 批准或请求修改",

        // ── 新建工作树 ─────────────────────────────────────────
        "New Worktree" => "新建工作树",

        // ── 权限视图（pager-render） ───────────────────────────
        "Always allow on all sessions" => "所有会话始终允许",
        "Always allow this command" => "始终允许此命令",

        // ── 编辑/搜索块标签 ───────────────────────────────────
        "Edit " => "编辑 ",

        // ── 链接/文件操作 ──────────────────────────────────────
        "Visit " => "访问 ",
        "Saved to " => "已保存到 ",

        // ── 图片预览 ───────────────────────────────────────────
        "Preview unavailable" => "预览不可用",
        "Preview pending" => "预览加载中",

        // ── 检查面板（inspect） ────────────────────────────────
        "Telemetry" => "遥测",
        "Feedback" => "反馈",
        "Project Instructions" => "项目指令",
        "Marketplaces" => "市场",
        "System Managed" => "系统管理",
        "Managed" => "已管理",
        "System Requirements" => "系统要求",
        "Requirements" => "要求",
        "Project" => "项目",

        // ── 运行状态标签 ───────────────────────────────────────
        "Compacting" => "压缩中",
        "Responding" => "回复中",
        "Running tool" => "运行工具中",
        "Running: " => "运行中：",
        "Waiting for response…" => "等待回复…",
        "Waiting on subagent…" => "等待子 Agent…",
        "Waiting on task output…" => "等待任务输出…",
        "Waiting on tasks…" => "等待任务…",
        "Sleeping…" => "休眠中…",
        "Running…" => "运行中…",
        "Waiting…" => "等待中…",
        "Verifying…" => "验证中…",
        "Cancelling…" => "取消中…",
        "Retrying (attempt {attempt})…" => "重试中（第 {attempt} 次）…",

        // ── 终端设置 ───────────────────────────────────────────
        "Environment\n" => "环境\n",
        "\nClipboard routes\n" => "\n剪贴板路由\n",
        "\nNo issues found.\n" => "\n未发现问题。\n",

        // ── 导出 ───────────────────────────────────────────────
        "## User\n\n" => "## 用户\n\n",
        "## Assistant\n\n" => "## 助手\n\n",
        "## Tools\n\n" => "## 工具\n\n",

        // ── 提示条补充 ─────────────────────────────────────────
        "Tip" => "提示",

        _ => return None,
    })
}
