//! `/login` -- log in or re-authenticate with your account.
//!
//! Opens the multi-provider picker (Kimi Code / xAI Session / Moonshot /
//! optional Grok CLI bridge). Does **not** auto-start Kimi-only login.
use crate::app::actions::Action;
use crate::slash::command::{CommandExecCtx, CommandResult, SlashCommand};

pub struct LoginCommand;

impl SlashCommand for LoginCommand {
    fn name(&self) -> &str {
        "login"
    }

    fn description(&self) -> &str {
        "Log in (Kimi Code / xAI Session / …)"
    }

    fn usage(&self) -> &str {
        "/login"
    }

    fn run(&self, _ctx: &mut CommandExecCtx, _args: &str) -> CommandResult {
        // None → show picker so Grok users are not forced into Kimi OAuth.
        CommandResult::Action(Action::Login { method_id: None })
    }
}
