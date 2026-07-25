//! DeepSeek Blue theme — inspired by DeepSeek's blue whale design.
//!
//! Deep ocean blue with whale-inspired accents, deep and mysterious.
use ratatui::style::{Color, Modifier};

use super::tokyonight::Theme;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

#[allow(dead_code)]
mod palette {
    use super::*;

    // Deep ocean backgrounds
    pub const BG: Color = rgb(8, 12, 20); // #080C14 — deep ocean
    pub const BG_DARK: Color = rgb(10, 15, 25); // #0A0F19
    pub const BG_STORM: Color = rgb(15, 20, 32); // #0F1420
    pub const BG_HIGHLIGHT: Color = rgb(25, 35, 55); // #192337
    pub const BG_HOVER: Color = rgb(35, 48, 70); // #233046

    // Ocean blue accents
    pub const BLUE: Color = rgb(60, 140, 255); // #3C8CFF — whale blue
    pub const BLUE_DARK: Color = rgb(40, 100, 200); // #2864C8
    pub const CYAN: Color = rgb(60, 200, 220); // #3CC8DC — ocean cyan
    pub const TEAL: Color = rgb(40, 180, 180); // #28B4B4

    // Text
    pub const FG: Color = rgb(210, 220, 235); // #D2DCEB — pale blue white
    pub const FG_DARK: Color = rgb(160, 175, 195); // #A0AFC3
    pub const GRAY: Color = rgb(80, 95, 115); // #505F73
    pub const GRAY_DIM: Color = rgb(50, 60, 75); // #323C4B

    // Accents
    pub const GREEN: Color = rgb(80, 200, 140); // #50C88C
    pub const RED: Color = rgb(220, 80, 90); // #DC505A
    pub const YELLOW: Color = rgb(200, 180, 80); // #C8B450
    pub const ORANGE: Color = rgb(220, 140, 60); // #DC8C3C
    pub const PURPLE: Color = rgb(120, 100, 200); // #7864C8
    pub const MAGENTA: Color = rgb(160, 100, 180); // #A064B4
}

use palette::*;

impl Theme {
    /// DeepSeek Blue — deep ocean blue with whale-inspired accents.
    pub const fn deepseek_blue() -> Self {
        Self {
            bg_base: BG_STORM,
            bg_light: BG_HIGHLIGHT,
            bg_dark: BG_DARK,
            bg_highlight: BG_HIGHLIGHT,
            bg_hover: BG_HOVER,
            bg_terminal: BG,

            accent_user: FG_DARK,
            accent_assistant: MAGENTA,
            accent_thinking: PURPLE,
            accent_tool: GRAY,
            accent_system: BLUE,
            accent_error: RED,
            accent_success: GREEN,
            accent_running: CYAN,
            accent_skill: BLUE,

            text_primary: FG,
            text_secondary: FG_DARK,

            gray_dim: GRAY_DIM,
            gray: GRAY,
            gray_bright: rgb(110, 125, 145),

            command: YELLOW,
            path: ORANGE,
            running: CYAN,
            warning: YELLOW,

            fuzzy_accent: BLUE,

            accent_plan: rgb(180, 180, 100), // #B4B464 — sea gold

            accent_verify: GREEN,

            accent_feedback: CYAN,

            accent_remember: PURPLE,

            selection_border: BLUE,
            hover_border: CYAN,
            prompt_border: GRAY,
            prompt_border_active: BLUE,

            accent_model: BLUE,

            scrollbar_bg: BG_HIGHLIGHT,
            scrollbar_fg: GRAY,

            diff_delete_bg: rgb(50, 20, 25),
            diff_delete_fg: RED,
            diff_insert_bg: rgb(15, 45, 30),
            diff_insert_fg: GREEN,
            diff_equal_fg: GRAY,
            diff_gutter_fg: GRAY_DIM,

            bg_visual: rgb(25, 35, 55),

            paste_bg: rgb(20, 30, 50),
            paste_fg: FG,
            paste_dim: GRAY,

            md_heading_h1: BLUE,
            md_heading_h1_mod: Modifier::BOLD,
            md_heading_h2: CYAN,
            md_heading_h2_mod: Modifier::BOLD,
            md_heading_h3: TEAL,
            md_heading_h3_mod: Modifier::empty(),
            md_heading_h4: FG_DARK,
            md_heading_h4_mod: Modifier::BOLD,
            md_heading_h5: FG_DARK,
            md_heading_h5_mod: Modifier::empty(),
            md_heading_h6: GRAY,
            md_heading_h6_mod: Modifier::empty(),
            md_code: CYAN,
            md_task_checked: GREEN,
            md_task_unchecked: GRAY,
            md_muted: GRAY,
            md_code_bg: BG_HIGHLIGHT,
            md_text: FG,
            link_fg: BLUE,
            animation: super::tokyonight::MoonAnimation::OceanWhale,
        }
    }
}
