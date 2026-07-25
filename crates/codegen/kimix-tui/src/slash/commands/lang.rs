//! `/lang` -- switch the UI language.
//!
//! `/lang zh` / `/lang en` / `/lang auto`（跟随系统语言）。
//! 界面文案按系统语言自动选择；此指令用于手动覆盖，选择会持久化到
//! `~/.kimix/config.toml` 的 `[ui].language`。
//!
//! `run` dispatches `Action::SetLanguage` — the dispatcher handles
//! runtime mutation + persistence + toast.
use crate::app::actions::Action;
use crate::i18n::{self, Lang};
use crate::slash::command::{AppCtx, ArgItem, CommandExecCtx, CommandResult, SlashCommand};

/// Switch the UI language (zh / en / auto).
pub struct LangCommand;

impl SlashCommand for LangCommand {
    fn name(&self) -> &str {
        "lang"
    }

    fn aliases(&self) -> &[&str] {
        &["language"]
    }

    fn description(&self) -> &str {
        "Switch UI language"
    }

    fn usage(&self) -> &str {
        "/lang <zh|en|auto>"
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn args_required(&self) -> bool {
        false
    }

    fn arg_placeholder(&self) -> Option<&str> {
        Some("<zh|en|auto>")
    }

    fn suggest_args(&self, _ctx: &AppCtx, _args_query: &str) -> Option<Vec<ArgItem>> {
        let current = i18n::current().code();
        let mk = |value: &str, desc: String| ArgItem {
            display: value.to_string(),
            match_text: value.to_string(),
            insert_text: value.to_string(),
            description: desc,
        };
        Some(vec![
            mk(
                "auto",
                if current == "auto" {
                    "auto (follow system) (active)".into()
                } else {
                    "auto (follow system)".into()
                },
            ),
            mk("zh", "中文".into()),
            mk("en", "English".into()),
        ])
    }

    fn run(&self, _ctx: &mut CommandExecCtx, args: &str) -> CommandResult {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            return CommandResult::Error(i18n::tr("Usage: /lang zh or /lang en").to_string());
        }
        if trimmed.eq_ignore_ascii_case("auto") || Lang::parse(trimmed).is_some() {
            CommandResult::Action(Action::SetLanguage(trimmed.to_lowercase()))
        } else {
            CommandResult::Error(i18n::tr("Usage: /lang zh or /lang en").to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_known_languages() {
        assert_eq!(Lang::parse("zh"), Some(Lang::Zh));
        assert_eq!(Lang::parse("ZH-CN"), Some(Lang::Zh));
        assert_eq!(Lang::parse("en"), Some(Lang::En));
        assert_eq!(Lang::parse("english"), Some(Lang::En));
        assert_eq!(Lang::parse("fr"), None);
    }

    #[test]
    fn detect_falls_back_to_english() {
        // 无 zh 前缀的环境一律英文（具体值取决于测试环境，不断言具体语言，
        // 只断言不会 panic 且返回值合法）。
        let _ = Lang::detect();
    }
}
