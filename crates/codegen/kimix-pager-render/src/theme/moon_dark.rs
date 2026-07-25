//! Moon Dark theme — deep space black with cold blue accents.
//!
//! Represents the dark side of the moon: mysterious, deep, cold.
//! Animation: moon slowly rotating with dark side facing viewer.
use ratatui::style::{Color, Modifier};

use super::tokyonight::Theme;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

#[allow(dead_code)]
mod palette {
    use super::*;

    // Deep space backgrounds
    pub const BG: Color = rgb(5, 5, 10); // #05050A — deep space
    pub const BG_DARK: Color = rgb(8, 8, 14); // #08080E
    pub const BG_STORM: Color = rgb(12, 12, 20); // #0C0C14
    pub const BG_HIGHLIGHT: Color = rgb(25, 25, 40); // #191928
    pub const BG_HOVER: Color = rgb(35, 35, 55); // #232337

    // Cold blue accents
    pub const BLUE: Color = rgb(100, 150, 255); // #6496FF — cold blue
    pub const BLUE_DARK: Color = rgb(60, 90, 180); // #3C5AB4
    pub const CYAN: Color = rgb(80, 200, 220); // #50C8DC — ice cyan
    pub const PURPLE: Color = rgb(130, 120, 220); // #8278DC

    // Text
    pub const FG: Color = rgb(200, 210, 230); // #C8D2E6 — silver white
    pub const FG_DARK: Color = rgb(160, 170, 190); // #A0AABE
    pub const GRAY: Color = rgb(80, 85, 100); // #505564
    pub const GRAY_DIM: Color = rgb(50, 55, 70); // #323746

    // Accents
    pub const GREEN: Color = rgb(100, 220, 150); // #64DC96
    pub const RED: Color = rgb(220, 100, 120); // #DC6478
    pub const YELLOW: Color = rgb(200, 180, 100); // #C8B464
    pub const ORANGE: Color = rgb(220, 150, 80); // #DC9650
    pub const MAGENTA: Color = rgb(180, 130, 220); // #B482DC
}

use palette::*;

impl Theme {
    /// Moon Dark — deep space black with cold blue accents.
    pub const fn moon_dark() -> Self {
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
            gray_bright: rgb(100, 105, 120),

            command: YELLOW,
            path: ORANGE,
            running: CYAN,
            warning: YELLOW,

            fuzzy_accent: BLUE,

            accent_plan: rgb(200, 200, 100), // #C8C864 — cold gold

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

            diff_delete_bg: rgb(60, 20, 25),
            diff_delete_fg: RED,
            diff_insert_bg: rgb(15, 50, 30),
            diff_insert_fg: GREEN,
            diff_equal_fg: GRAY,
            diff_gutter_fg: GRAY_DIM,

            bg_visual: rgb(30, 30, 50),

            paste_bg: rgb(25, 25, 45),
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
            animation: super::tokyonight::MoonAnimation::MoonDark,
        }
    }
}
