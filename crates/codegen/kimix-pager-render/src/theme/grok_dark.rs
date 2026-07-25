//! Grok Dark theme — inspired by xAI Grok's dark interface.
//!
//! Deep black with vibrant accent colors, modern and bold.
use ratatui::style::{Color, Modifier};

use super::tokyonight::Theme;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

mod palette {
    use super::*;

    pub const BG: Color = rgb(10, 10, 10); // #0A0A0A
    pub const BG_DARK: Color = rgb(15, 15, 15); // #0F0F0F
    pub const BG_STORM: Color = rgb(18, 18, 18); // #121212
    pub const BG_HIGHLIGHT: Color = rgb(30, 30, 30); // #1E1E1E
    pub const BG_HOVER: Color = rgb(40, 40, 40); // #282828

    pub const BLUE: Color = rgb(80, 140, 255); // #508CFF
    pub const CYAN: Color = rgb(0, 200, 200); // #00C8C8
    pub const GREEN: Color = rgb(0, 220, 120); // #00DC78
    pub const RED: Color = rgb(255, 80, 80); // #FF5050
    pub const YELLOW: Color = rgb(255, 200, 0); // #FFC800
    pub const ORANGE: Color = rgb(255, 150, 50); // #FF9632
    pub const PURPLE: Color = rgb(160, 100, 255); // #A064FF
    pub const MAGENTA: Color = rgb(200, 100, 200); // #C864C8

    pub const FG: Color = rgb(230, 230, 230); // #E6E6E6
    pub const FG_DARK: Color = rgb(180, 180, 180); // #B4B4B4
    pub const GRAY: Color = rgb(100, 100, 100); // #646464
    pub const GRAY_DIM: Color = rgb(60, 60, 60); // #3C3C3C
}

use palette::*;

impl Theme {
    pub const fn grok_dark() -> Self {
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
            gray_bright: rgb(130, 130, 130),

            command: YELLOW,
            path: ORANGE,
            running: CYAN,
            warning: YELLOW,

            fuzzy_accent: BLUE,

            accent_plan: rgb(255, 200, 100), // #FFC864

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

            diff_delete_bg: rgb(60, 20, 20),
            diff_delete_fg: RED,
            diff_insert_bg: rgb(20, 50, 30),
            diff_insert_fg: GREEN,
            diff_equal_fg: GRAY,
            diff_gutter_fg: GRAY_DIM,

            bg_visual: rgb(30, 30, 50),

            paste_bg: rgb(25, 25, 40),
            paste_fg: FG,
            paste_dim: GRAY,

            md_heading_h1: BLUE,
            md_heading_h1_mod: Modifier::BOLD,
            md_heading_h2: CYAN,
            md_heading_h2_mod: Modifier::BOLD,
            md_heading_h3: PURPLE,
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
            animation: super::tokyonight::MoonAnimation::GrokX,
        }
    }
}
