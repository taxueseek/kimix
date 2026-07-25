//! Moon Light theme — moonlit white with warm silver accents.
//!
//! Represents the bright side of the moon: luminous, gentle, warm.
//! Animation: moon slowly rotating with bright side facing viewer.
use ratatui::style::{Color, Modifier};

use super::tokyonight::Theme;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

#[allow(dead_code)]
mod palette {
    use super::*;

    // Moonlit backgrounds
    pub const BG: Color = rgb(245, 242, 235); // #F5F2EB — moonlit white
    pub const BG_DARK: Color = rgb(235, 232, 225); // #EBE8E1
    pub const BG_STORM: Color = rgb(240, 237, 230); // #F0EDE6
    pub const BG_HIGHLIGHT: Color = rgb(225, 222, 215); // #E1DED7
    pub const BG_HOVER: Color = rgb(215, 212, 205); // #D7D4CD

    // Warm silver accents
    pub const BLUE: Color = rgb(80, 120, 180); // #5078B4 — steel blue
    pub const BLUE_DARK: Color = rgb(60, 90, 150); // #3C5A96
    pub const CYAN: Color = rgb(70, 150, 160); // #4696A0
    pub const PURPLE: Color = rgb(120, 100, 160); // #7864A0

    // Text (dark on light)
    pub const FG: Color = rgb(30, 30, 40); // #1E1E28
    pub const FG_DARK: Color = rgb(60, 60, 70); // #3C3C46
    pub const GRAY: Color = rgb(140, 140, 150); // #8C8C96
    pub const GRAY_DIM: Color = rgb(170, 170, 180); // #AAAA B4

    // Accents
    pub const GREEN: Color = rgb(60, 140, 80); // #3C8C50
    pub const RED: Color = rgb(180, 60, 70); // #B43C46
    pub const YELLOW: Color = rgb(160, 140, 60); // #A08C3C
    pub const ORANGE: Color = rgb(180, 120, 60); // #B4783C
    pub const MAGENTA: Color = rgb(140, 90, 140); // #8C5A8C
}

use palette::*;

impl Theme {
    /// Moon Light — moonlit white with warm silver accents.
    pub const fn moon_light() -> Self {
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
            gray_bright: rgb(120, 120, 130),

            command: YELLOW,
            path: ORANGE,
            running: CYAN,
            warning: YELLOW,

            fuzzy_accent: BLUE,

            accent_plan: rgb(140, 130, 60), // #8C823C — warm gold

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

            diff_delete_bg: rgb(255, 220, 220),
            diff_delete_fg: RED,
            diff_insert_bg: rgb(220, 255, 225),
            diff_insert_fg: GREEN,
            diff_equal_fg: GRAY,
            diff_gutter_fg: GRAY_DIM,

            bg_visual: rgb(210, 210, 225),

            paste_bg: rgb(220, 220, 235),
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
            animation: super::tokyonight::MoonAnimation::MoonLight,
        }
    }
}
