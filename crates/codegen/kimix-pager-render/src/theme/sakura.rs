//! Sakura theme — 暗夜樱花：深李色底 + 樱粉 accent。
//!
//! 设计原则与 KimixNight 一致：中性近黑背景保证 256 色终端降级后依然成立，
//! accent 使用饱和樱粉系，量化到 xterm-256 色板（211/217/175 等）不失真。
use ratatui::style::{Color, Modifier};

use super::tokyonight::Theme;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

// Sakura palette — 暖调近黑底 + 樱粉 accent。
//
// 背景锚点：#1c1418（深李黑），文字 #f0e6ea（暖白）。
#[allow(dead_code)]
mod palette {
    use super::*;

    // ── Backgrounds ─────────────────────────────────────────────────────
    pub const BG: Color = rgb(16, 11, 14); //        #100b0e — darkest (terminal bg)
    pub const BG_DARK: Color = rgb(20, 14, 17); //   #140e11 — darker
    pub const BG_STORM_DARK: Color = rgb(24, 17, 21); // #181115 — dark bg
    pub const BG_STORM: Color = rgb(28, 20, 24); //    #1c1418 — main bg
    pub const BG_HIGHLIGHT: Color = rgb(46, 34, 40); // #2e2228 — highlight bg

    // ── Text / grays（暖灰）──────────────────────────────────────────────
    pub const FG: Color = rgb(240, 230, 234); //      #f0e6ea — primary text
    pub const FG_DARK: Color = rgb(210, 196, 202); //  #d2c4ca — secondary text
    pub const FG_GUTTER: Color = rgb(76, 62, 70); //   #4c3e46 — dim
    pub const COMMENT: Color = rgb(122, 102, 112); //  #7a6670 — muted
    pub const DARK3: Color = rgb(102, 84, 94); //      #66545e — medium gray
    pub const DARK5: Color = rgb(140, 118, 130); //    #8c7682 — bright gray

    // ── Accent colors（樱粉系）───────────────────────────────────────────
    pub const SAKURA: Color = rgb(242, 160, 192); //  #f2a0c0 — 主樱粉
    pub const SAKURA_DEEP: Color = rgb(214, 123, 164); // #d67ba4 — 深樱粉
    pub const MAUVE: Color = rgb(198, 148, 210); //   #c694d2 — 淡紫
    pub const ROSE: Color = rgb(235, 130, 150); //    #eb8296 — 玫瑰红
    pub const PEACH: Color = rgb(240, 175, 150); //   #f0af96 — 桃色
    pub const JADE: Color = rgb(140, 200, 170); //    #8cc8aa — 青玉（对比色）
    pub const GOLD: Color = rgb(230, 195, 130); //    #e6c382 — 暖金
    pub const MIST: Color = rgb(160, 175, 215); //    #a0afd7 — 雾蓝

    pub const RED_DARK: Color = rgb(74, 20, 28); //   #4a141c — quantizes to red, not gray
    pub const GREEN_DARK: Color = rgb(14, 50, 34); // #0e3222 — quantizes to green, not gray
}
use palette::*;

impl Theme {
    /// Sakura theme — 暗夜樱花。
    pub const fn sakura() -> Self {
        Self {
            bg_base: BG_STORM,
            bg_light: BG_HIGHLIGHT,
            bg_dark: rgb(36, 26, 31), // #241a1f — visible code blocks
            bg_highlight: BG_HIGHLIGHT,
            bg_hover: rgb(56, 42, 49), // #382a31
            bg_terminal: BG,

            accent_user: FG_DARK,
            accent_assistant: SAKURA,
            accent_thinking: SAKURA_DEEP,
            accent_tool: DARK5,
            accent_system: MIST,
            accent_error: ROSE,
            accent_success: JADE,
            accent_running: SAKURA,
            accent_skill: MAUVE,

            text_primary: FG,
            text_secondary: FG_DARK,

            gray_dim: rgb(100, 82, 92), // #64525c
            gray: COMMENT,
            gray_bright: DARK5,

            command: GOLD,
            path: PEACH,
            running: JADE,
            warning: GOLD,

            fuzzy_accent: SAKURA,

            accent_plan: rgb(240, 210, 150), // #f0d296 — golden

            accent_verify: MAUVE,

            accent_feedback: JADE,

            accent_remember: rgb(150, 205, 140), // #96cd8c — soft green

            selection_border: rgb(76, 56, 66), //      #4c3842
            prompt_border: rgb(62, 46, 55),    //         #3e2e37
            prompt_border_active: rgb(150, 108, 132), // #966c84 — sakura-tinted when focused
            hover_border: rgb(40, 30, 36),     //          #281e24

            accent_model: SAKURA,

            scrollbar_bg: BG_STORM_DARK,
            scrollbar_fg: BG_HIGHLIGHT,

            diff_delete_bg: RED_DARK,
            diff_delete_fg: ROSE,
            diff_insert_bg: GREEN_DARK,
            diff_insert_fg: JADE,
            diff_equal_fg: COMMENT,
            diff_gutter_fg: COMMENT,

            bg_visual: rgb(66, 48, 58), // #42303a

            paste_bg: BG_STORM_DARK,
            paste_fg: FG_DARK,
            paste_dim: FG_GUTTER,

            md_heading_h1: SAKURA,
            md_heading_h1_mod: Modifier::BOLD,
            md_heading_h2: MAUVE,
            md_heading_h2_mod: Modifier::BOLD,
            md_heading_h3: PEACH,
            md_heading_h3_mod: Modifier::BOLD,
            md_heading_h4: DARK5,
            md_heading_h4_mod: Modifier::BOLD,
            md_heading_h5: COMMENT,
            md_heading_h5_mod: Modifier::BOLD,
            md_heading_h6: DARK3,
            md_heading_h6_mod: Modifier::empty(),
            md_code: PEACH,
            md_task_checked: JADE,
            md_task_unchecked: FG_DARK,
            md_muted: COMMENT,
            md_code_bg: rgb(36, 26, 31),
            md_text: FG_DARK,
            link_fg: rgb(220, 150, 190), // #dc96be — soft sakura for dark bg
            animation: super::tokyonight::MoonAnimation::Standard,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sakura_theme_builds() {
        let theme = Theme::sakura();
        assert!(matches!(theme.accent_model, Color::Rgb(242, 160, 192)));
        assert!(matches!(theme.bg_base, Color::Rgb(28, 20, 24)));
    }
}
