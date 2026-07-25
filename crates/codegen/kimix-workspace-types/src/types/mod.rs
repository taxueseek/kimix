//! Supporting structs/enums referenced from events.
//!
//! Each type carries a `// TODO(workspace): align with <canonical type>`
//! comment naming the crate it should eventually be reconciled against.
pub mod git;
pub mod plugins;
pub mod session;
pub mod skills;

pub use git::VcsKind;
pub use plugins::{HookInfo, PluginInfo};
pub use session::{FsEventKind, LspServerStatus, McpServerStatus};
pub use skills::SkillInfo;
