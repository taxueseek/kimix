//! Forest theme — 暗夜森林：墨绿底 + 青玉/苔绿 accent。
//!
//! 与 KimixNight 同原则：近黑微绿背景在 256 色终端降级后依然成立，
//! accent 使用自然绿系，量化到 xterm-256 色板（72/114/151 等）不失真。
use ratatui::style::{Color, Modifier};

use super::tokyonight::Theme;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

// Forest palette — 冷调近黑微绿底 + 自然绿 accent。
//
// 背景锚点：#10160f（墨绿黑），文字 #e6efe4（霜白）。
#[allow(dead_code)]
mod palette {
    use super::*;

    // ── Backgrounds ─────────────────────────────────────────────────────
    pub const BG: Color = rgb(9, 14, 9); //          #090e09 — darkest (terminal bg)
    pub const BG_DARK: Color = rgb(12, 18, 12); //    #0c120c — darker
    pub const BG_STORM_DARK: Color = rgb(14, 21, 14); // #0e150e — dark bg
    pub const BG_STORM: Color = rgb(16, 22, 15); //    #10160f — main bg
    pub const BG_HIGHLIGHT: Color = rgb(30, 40, 28); // #1e281c — highlight bg

    // ── Text / grays（微绿灰）────────────────────────────────────────────
    pub const FG: Color = rgb(230, 239, 228); //      #e6efe4 — primary text
    pub const FG_DARK: Color = rgb(198, 212, 196); //  #c6d4c4 — secondary text
    pub const FG_GUTTER: Color = rgb(62, 76, 60); //   #3e4c3c — dim
    pub const COMMENT: Color = rgb(104, 122, 102); //  #687a66 — muted
    pub const DARK3: Color = rgb(86, 104, 84); //      #566854 — medium gray
    pub const DARK5: Color = rgb(122, 142, 118); //    #7a8e76 — bright gray

    // ── Accent colors（自然绿系）─────────────────────────────────────────
    pub const JADE: Color = rgb(127, 176, 105); //    #7fb069 — 青玉绿（主 accent）
    pub const MOSS: Color = rgb(163, 197, 133); //    #a3c585 — 苔绿
    pub const PINE: Color = rgb(88, 156, 132); //     #589c84 — 松青
    pub const FERN: Color = rgb(110, 190, 150); //    #6ebe96 — 蕨绿
    pub const EARTH: Color = rgb(206, 168, 110); //   #cea86e — 土金
    pub const CLAY: Color = rgb(214, 130, 96); //     #d68260 — 陶土
    pub const BERRY: Color = rgb(224, 120, 120); //   #e07878 — 浆果红
    pub const SKY: Color = rgb(136, 184, 214); //     #88b8d6 — 天青

    pub const RED_DARK: Color = rgb(66, 18, 18); //   #421212 — quantizes to red, not gray
    pub const GREEN_DARK: Color = rgb(10, 48, 24); // #0a3018 — quantizes to green, not gray
}
use palette::*;

impl Theme {
    /// Forest theme — 暗夜森林。
    pub const fn forest() -> Self {
        Self {
            bg_base: BG_STORM,
            bg_light: BG_HIGHLIGHT,
            bg_dark: rgb(24, 33, 22), // #182116 — visible code blocks
            bg_highlight: BG_HIGHLIGHT,
            bg_hover: rgb(40, 53, 37), // #283525
            bg_terminal: BG,

            accent_user: FG_DARK,
            accent_assistant: JADE,
            accent_thinking: PINE,
            accent_tool: DARK5,
            accent_system: SKY,
            accent_error: BERRY,
            accent_success: FERN,
            accent_running: JADE,
            accent_skill: PINE,

            text_primary: FG,
            text_secondary: FG_DARK,

            gray_dim: rgb(84, 100, 82), // #546452
            gray: COMMENT,
            gray_bright: DARK5,

            command: EARTH,
            path: CLAY,
            running: FERN,
            warning: EARTH,

            fuzzy_accent: JADE,

            accent_plan: rgb(226, 205, 135), // #e2cd87 — golden

            accent_verify: SKY,

            accent_feedback: FERN,

            accent_remember: rgb(150, 205, 120), // #96cd78 — fresh green

            selection_border: rgb(50, 66, 47),       //      #32422f
            prompt_border: rgb(42, 56, 40),          //         #2a3828
            prompt_border_active: rgb(110, 150, 96), // #6e9660 — jade-tinted when focused
            hover_border: rgb(28, 38, 26),           //          #1c261a

            accent_model: JADE,

            scrollbar_bg: BG_STORM_DARK,
            scrollbar_fg: BG_HIGHLIGHT,

            diff_delete_bg: RED_DARK,
            diff_delete_fg: BERRY,
            diff_insert_bg: GREEN_DARK,
            diff_insert_fg: FERN,
            diff_equal_fg: COMMENT,
            diff_gutter_fg: COMMENT,

            bg_visual: rgb(44, 60, 41), // #2c3c29

            paste_bg: BG_STORM_DARK,
            paste_fg: FG_DARK,
            paste_dim: FG_GUTTER,

            md_heading_h1: JADE,
            md_heading_h1_mod: Modifier::BOLD,
            md_heading_h2: MOSS,
            md_heading_h2_mod: Modifier::BOLD,
            md_heading_h3: PINE,
            md_heading_h3_mod: Modifier::BOLD,
            md_heading_h4: DARK5,
            md_heading_h4_mod: Modifier::BOLD,
            md_heading_h5: COMMENT,
            md_heading_h5_mod: Modifier::BOLD,
            md_heading_h6: DARK3,
            md_heading_h6_mod: Modifier::empty(),
            md_code: EARTH,
            md_task_checked: FERN,
            md_task_unchecked: FG_DARK,
            md_muted: COMMENT,
            md_code_bg: rgb(24, 33, 22),
            md_text: FG_DARK,
            link_fg: rgb(150, 200, 160), // #96c8a0 — soft jade for dark bg
            animation: super::tokyonight::MoonAnimation::ForestGlow,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forest_theme_builds() {
        let theme = Theme::forest();
        assert!(matches!(theme.accent_model, Color::Rgb(127, 176, 105)));
        assert!(matches!(theme.bg_base, Color::Rgb(16, 22, 15)));
    }
}
