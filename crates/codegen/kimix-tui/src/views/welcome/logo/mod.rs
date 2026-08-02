//! Logo component — a procedurally generated braille moon that waxes and
//! wanes through a full lunation, echoing the Kimi CLI's moon-phase spinner.
//!
//! Disc raster is phase-cached behind an `Arc` so hit frames clone the
//! pointer only. 太极 dots are paint-time overlays that never mutate the
//! shared grid. Theme colours/particles live in [`anim`].
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::theme::Theme;
use crate::theme::tokyonight::MoonAnimation;

mod anim;
mod raster;
mod shapes;

use anim::anim_frame;
use raster::{
    FULL, FULL_LOGO_MIN_HEIGHT, LUNATION_SECS, MOON_FPS, MoonSize, SMALL, SMALL_LOGO_MIN_HEIGHT,
    apply_taiji, moon_cells_cached,
};
use shapes::{grok_x_cells, whale_cells};

fn pick_logo(window_height: u16) -> Option<MoonSize> {
    pick_logo_for(window_height, logo_hidden())
}

/// Pure tier selection so tests can drive the legacy-console flag directly.
fn pick_logo_for(window_height: u16, hidden: bool) -> Option<MoonSize> {
    if hidden || window_height < SMALL_LOGO_MIN_HEIGHT {
        None
    } else if window_height < FULL_LOGO_MIN_HEIGHT {
        Some(SMALL)
    } else {
        Some(FULL)
    }
}

/// The braille moon has no ASCII stand-in; see the module doc.
fn logo_hidden() -> bool {
    crate::glyphs::is_legacy_windows_console()
}

/// Animation phase in seconds since the first render. Wall-clock based so the
/// lunation speed is independent of the frame rate.
fn anim_phase_secs() -> f32 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f32()
}

/// Quantized animation frame for the current wall-clock phase.
pub fn shimmer_frame() -> u64 {
    if logo_hidden() {
        return 0;
    }
    (anim_phase_secs() * MOON_FPS) as u64
}

/// Lunation phase in `[0, 1)`: 0 = new moon, 0.5 = full moon.
fn phase_now() -> f32 {
    (anim_phase_secs() / LUNATION_SECS).fract()
}

fn render_into(area: Rect, buf: &mut Buffer, theme: &Theme, size: MoonSize) {
    let secs = anim_phase_secs();
    let base = theme.gray;
    let frame = anim_frame(theme.animation, base, size, secs);
    let lit_color = frame.lit;
    let dark_color = frame.dark;
    let particles = frame.particles;

    // Shared Arc disc for moon themes (hit path clones the Arc only);
    // custom shapes own a fresh grid each frame.
    let logo_lines: Vec<Line> = match theme.animation {
        MoonAnimation::GrokX => grok_x_cells(size, secs)
            .iter()
            .map(|row| row_to_line(row.iter().copied(), lit_color, dark_color, base))
            .collect(),
        MoonAnimation::OceanWhale => whale_cells(size, secs)
            .iter()
            .map(|row| row_to_line(row.iter().copied(), lit_color, dark_color, base))
            .collect(),
        _ => {
            let cells = moon_cells_cached(size, phase_now());
            cells
                .iter()
                .enumerate()
                .map(|(row_i, row)| {
                    row_to_line(
                        row.iter().enumerate().map(|(col_i, cell)| {
                            apply_taiji(*cell, col_i as u16, row_i as u16, size, secs)
                        }),
                        lit_color,
                        dark_color,
                        base,
                    )
                })
                .collect()
        }
    };

    Paragraph::new(logo_lines).render(area, buf);

    if !particles.is_empty() {
        let logo_visual_w = size.cols;
        let left_pad = if area.width > logo_visual_w {
            (area.width - logo_visual_w) / 2
        } else {
            0
        };
        let top_pad = if area.height > size.rows {
            (area.height - size.rows) / 2
        } else {
            0
        };
        for (px, py, color) in &particles {
            let sx = area.x + left_pad + px;
            let sy = area.y + top_pad + py;
            if sx < area.x + area.width
                && sy < area.y + area.height
                && let Some(cell) = buf.cell_mut((sx, sy))
            {
                if cell.symbol() == "\u{2800}" || cell.symbol() == " " {
                    cell.set_symbol("\u{2591}");
                    cell.set_fg(*color);
                }
            }
        }
        if theme.animation == MoonAnimation::GrokX {
            for (px, _py, color) in &particles {
                let sx = area.x + left_pad + px;
                for row in 0..size.rows {
                    let sy = area.y + top_pad + row;
                    if sx < area.x + area.width
                        && sy < area.y + area.height
                        && let Some(cell) = buf.cell_mut((sx, sy))
                    {
                        cell.set_bg(*color);
                    }
                }
            }
        }
    }
}

fn row_to_line(
    cells: impl Iterator<Item = Option<raster::MoonCell>>,
    lit_color: Color,
    dark_color: Color,
    base: Color,
) -> Line<'static> {
    let mut spans: Vec<Span> = Vec::new();
    let mut run = String::new();
    let mut run_color: Option<Color> = None;
    for cell in cells {
        let (ch, color) = match cell {
            Some(raster::MoonCell { ch, lit: true }) => (ch, lit_color),
            Some(raster::MoonCell { ch, lit: false }) => (ch, dark_color),
            None => ('\u{2800}', base),
        };
        if run_color != Some(color) {
            if let Some(prev) = run_color {
                spans.push(Span::styled(
                    std::mem::take(&mut run),
                    Style::default().fg(prev),
                ));
            }
            run_color = Some(color);
        }
        run.push(ch);
    }
    if let Some(prev) = run_color {
        spans.push(Span::styled(run, Style::default().fg(prev)));
    }
    Line::from(spans).alignment(Alignment::Center)
}

pub fn logo_line_count(window_height: u16) -> u16 {
    pick_logo(window_height).map_or(0, |s| s.rows)
}

pub fn logo_visual_width(window_height: u16) -> u16 {
    pick_logo(window_height).map_or(24, |s| s.cols)
}

pub fn render_logo(area: Rect, buf: &mut Buffer, theme: &Theme, window_height: u16) {
    if let Some(size) = pick_logo(window_height) {
        render_into(area, buf, theme, size);
    }
}

pub fn full_logo_line_count() -> u16 {
    full_logo_line_count_for(logo_hidden())
}

fn full_logo_line_count_for(hidden: bool) -> u16 {
    if hidden { 0 } else { FULL.rows }
}

pub fn full_logo_visual_width() -> u16 {
    full_logo_visual_width_for(logo_hidden())
}

fn full_logo_visual_width_for(hidden: bool) -> u16 {
    if hidden { 0 } else { FULL.cols }
}

pub fn render_full_logo(area: Rect, buf: &mut Buffer, theme: &Theme) {
    if !logo_hidden() {
        render_into(area, buf, theme, FULL);
    }
}

pub fn compact_logo_line_count() -> u16 {
    if logo_hidden() { 0 } else { SMALL.rows }
}

pub fn render_compact_logo(area: Rect, buf: &mut Buffer, theme: &Theme) {
    if !logo_hidden() {
        render_into(area, buf, theme, SMALL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raster::{FULL, SMALL, FULL_LOGO_MIN_HEIGHT, SMALL_LOGO_MIN_HEIGHT, dot_lit, in_maria_or_crater, moon_cells};

    fn lit_dots(p: f32) -> usize {
        let dots_w = (FULL.cols * 2) as i32;
        let dots_h = (FULL.rows * 4) as i32;
        let cx = (dots_w as f32 - 1.0) / 2.0;
        let cy = (dots_h as f32 - 1.0) / 2.0;
        let r = (dots_w.min(dots_h) as f32) / 2.0 - 0.5;
        let mut count = 0;
        for y in 0..dots_h {
            for x in 0..dots_w {
                let dx = (x as f32 - cx) / r;
                let dy = (y as f32 - cy) / r;
                if dx * dx + dy * dy <= 1.0 && dot_lit(dx, dy, p) {
                    count += 1;
                }
            }
        }
        count
    }

    #[test]
    fn logo_sizes_by_height() {
        assert!(pick_logo_for(SMALL_LOGO_MIN_HEIGHT - 1, false).is_none());
        assert_eq!(pick_logo_for(SMALL_LOGO_MIN_HEIGHT, false), Some(SMALL));
        assert_eq!(pick_logo_for(FULL_LOGO_MIN_HEIGHT - 1, false), Some(SMALL));
        assert_eq!(pick_logo_for(FULL_LOGO_MIN_HEIGHT, false), Some(FULL));
    }

    #[test]
    fn logo_hidden_on_legacy_console_at_every_height() {
        for h in [0, SMALL_LOGO_MIN_HEIGHT, FULL_LOGO_MIN_HEIGHT, u16::MAX] {
            assert!(pick_logo_for(h, true).is_none(), "height {h}");
        }
    }

    #[test]
    fn hero_box_always_uses_full_logo() {
        assert_eq!(full_logo_line_count_for(false), FULL.rows);
        assert_eq!(full_logo_visual_width_for(false), FULL.cols);
        assert!(full_logo_line_count_for(false) > SMALL.rows);
        assert!(full_logo_visual_width_for(false) > SMALL.cols);
    }

    #[test]
    fn full_logo_helpers_collapse_when_hidden() {
        assert_eq!(full_logo_line_count_for(true), 0);
        assert_eq!(full_logo_visual_width_for(true), 0);
    }

    #[test]
    fn compact_logo_is_the_small_moon() {
        if !logo_hidden() {
            assert_eq!(compact_logo_line_count(), SMALL.rows);
            assert!(compact_logo_line_count() < FULL.rows);
        } else {
            assert_eq!(compact_logo_line_count(), 0);
        }
    }

    #[test]
    fn full_moon_lights_the_whole_disc_and_new_moon_none() {
        let full = lit_dots(0.5);
        assert!(full > 0, "full moon must light the disc");
        assert_eq!(lit_dots(0.0), 0, "new moon must be fully dark");
        let quarter = lit_dots(0.25);
        assert!(
            (full / 3..=2 * full / 3).contains(&quarter),
            "first quarter lit {quarter} should be near half of full {full}"
        );
    }

    #[test]
    fn illumination_waxes_then_wanes() {
        let phases = [0.05, 0.15, 0.25, 0.35, 0.45];
        for w in phases.windows(2) {
            assert!(
                lit_dots(w[0]) < lit_dots(w[1]),
                "waxing must grow: p={} vs p={}",
                w[0],
                w[1]
            );
        }
        let phases = [0.55, 0.65, 0.75, 0.85, 0.95];
        for w in phases.windows(2) {
            assert!(
                lit_dots(w[0]) > lit_dots(w[1]),
                "waning must shrink: p={} vs p={}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn waxing_lights_the_right_limb_first() {
        assert!(dot_lit(0.95, 0.0, 0.1), "right limb lit while waxing");
        assert!(!dot_lit(-0.95, 0.0, 0.1), "left limb dark while waxing");
        assert!(dot_lit(-0.95, 0.0, 0.9), "left limb lit while waning");
        assert!(!dot_lit(0.95, 0.0, 0.9), "right limb dark while waning");
    }

    #[test]
    fn new_moon_keeps_a_silhouette_ring() {
        let cells = moon_cells(FULL, 0.0);
        let drawn = cells.iter().flatten().flatten().count();
        assert!(drawn > 0, "new moon must keep an outline ring");
        assert!(
            cells.iter().flatten().flatten().all(|c| !c.lit),
            "no cell may be lit at new moon"
        );
    }

    #[test]
    fn maria_texture_the_disc_in_both_extremes() {
        let full = moon_cells(FULL, 0.5);
        let drawn_dots: u32 = full
            .iter()
            .flatten()
            .flatten()
            .map(|c| (c.ch as u32 - 0x2800).count_ones())
            .sum();
        assert!(
            (drawn_dots as usize) < lit_dots(0.5),
            "full moon must keep dark maria holes"
        );
        let new = moon_cells(FULL, 0.0);
        assert!(
            new[5][4].is_some(),
            "new moon must show maria inside the ring"
        );
        assert!(
            in_maria_or_crater(-0.58, 0.05),
            "Procellarum anchors the maria map"
        );
        assert!(!in_maria_or_crater(0.0, 0.85), "south pole stays mare-free");
    }

    #[test]
    fn moon_raster_is_round_and_fills_the_grid() {
        let cells = moon_cells(FULL, 0.5);
        assert_eq!(cells.len(), FULL.rows as usize);
        assert!(cells.iter().all(|r| r.len() == FULL.cols as usize));
        let first_row_drawn = cells.first().unwrap().iter().flatten().count();
        let mid_row_drawn = cells[FULL.rows as usize / 2].iter().flatten().count();
        assert!(first_row_drawn > 0, "top of the disc must be drawn");
        assert!(
            mid_row_drawn > first_row_drawn,
            "equator must be wider than the pole (round disc)"
        );
        assert_eq!(mid_row_drawn, FULL.cols as usize, "equator spans the grid");
    }

    #[test]
    fn cached_raster_shares_arc() {
        let a = moon_cells_cached(FULL, 0.25);
        let b = moon_cells_cached(FULL, 0.25);
        assert!(std::sync::Arc::ptr_eq(&a, &b), "same phase must share Arc");
    }
}
