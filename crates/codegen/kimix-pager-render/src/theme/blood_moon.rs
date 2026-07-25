//! Blood Moon theme — deep crimson black with blood red accents.
//!
//! Represents the blood moon: mysterious, powerful, ominous.
//! Animation: red-tinted moon with blood glow halo effect.
use ratatui::style::{Color, Modifier};

use super::tokyonight::Theme;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

#[allow(dead_code)]
mod palette {
    use super::*;

    // Deep crimson backgrounds
    pub const BG: Color = rgb(10, 5, 8); // #0A0508 — blood dark
    pub const BG_DARK: Color = rgb(15, 8, 12); // #0F080C
    pub const BG_STORM: Color = rgb(20, 12, 16); // #140C10
    pub const BG_HIGHLIGHT: Color = rgb(40, 25, 32); // #281920
    pub const BG_HOVER: Color = rgb(55, 35, 42); // #37232A

    // Blood red accents
    pub const RED: Color = rgb(220, 60, 60); // #DC3C3C — blood red
    pub const RED_DARK: Color = rgb(150, 40, 40); // #962828
    pub const CRIMSON: Color = rgb(180, 50, 70); // #B43246
    pub const BURGUNDY: Color = rgb(120, 40, 60); // #78283C

    // Text
    pub const FG: Color = rgb(220, 200, 195); // #DCC8C3 — warm white
    pub const FG_DARK: Color = rgb(180, 160, 155); // #B4A09B
    pub const GRAY: Color = rgb(100, 80, 85); // #645055
    pub const GRAY_DIM: Color = rgb(65, 50, 55); // #413237

    // Accents
    pub const ORANGE: Color = rgb(200, 120, 60); // #C8783C
    pub const YELLOW: Color = rgb(200, 170, 80); // #C8AA50
    pub const PURPLE: Color = rgb(160, 80, 120); // #A05078
    pub const MAGENTA: Color = rgb(180, 100, 140); // #B4648C
    pub const BLUE: Color = rgb(100, 120, 180); // #6478B4
    pub const CYAN: Color = rgb(120, 160, 180); // #78A0B4
    pub const GREEN: Color = rgb(100, 160, 100); // #64A064
}

use palette::*;

impl Theme {
    /// Blood Moon — deep crimson black with blood red accents.
    pub const fn blood_moon() -> Self {
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
            accent_system: CRIMSON,
            accent_error: RED,
            accent_success: GREEN,
            accent_running: ORANGE,
            accent_skill: CRIMSON,

            text_primary: FG,
            text_secondary: FG_DARK,

            gray_dim: GRAY_DIM,
            gray: GRAY,
            gray_bright: rgb(120, 100, 105),

            command: YELLOW,
            path: ORANGE,
            running: ORANGE,
            warning: YELLOW,

            fuzzy_accent: CRIMSON,

            accent_plan: rgb(200, 150, 80), // #C89650 — amber

            accent_verify: GREEN,

            accent_feedback: ORANGE,

            accent_remember: PURPLE,

            selection_border: RED,
            hover_border: CRIMSON,
            prompt_border: GRAY,
            prompt_border_active: RED,

            accent_model: CRIMSON,

            scrollbar_bg: BG_HIGHLIGHT,
            scrollbar_fg: GRAY,

            diff_delete_bg: rgb(60, 15, 20),
            diff_delete_fg: RED,
            diff_insert_bg: rgb(15, 40, 20),
            diff_insert_fg: GREEN,
            diff_equal_fg: GRAY,
            diff_gutter_fg: GRAY_DIM,

            bg_visual: rgb(45, 25, 35),

            paste_bg: rgb(40, 25, 32),
            paste_fg: FG,
            paste_dim: GRAY,

            md_heading_h1: RED,
            md_heading_h1_mod: Modifier::BOLD,
            md_heading_h2: CRIMSON,
            md_heading_h2_mod: Modifier::BOLD,
            md_heading_h3: PURPLE,
            md_heading_h3_mod: Modifier::empty(),
            md_heading_h4: FG_DARK,
            md_heading_h4_mod: Modifier::BOLD,
            md_heading_h5: FG_DARK,
            md_heading_h5_mod: Modifier::empty(),
            md_heading_h6: GRAY,
            md_heading_h6_mod: Modifier::empty(),
            md_code: ORANGE,
            md_task_checked: GREEN,
            md_task_unchecked: GRAY,
            md_muted: GRAY,
            md_code_bg: BG_HIGHLIGHT,
            md_text: FG,
            link_fg: CRIMSON,
            animation: super::tokyonight::MoonAnimation::BloodMoon,
        }
    }
}
