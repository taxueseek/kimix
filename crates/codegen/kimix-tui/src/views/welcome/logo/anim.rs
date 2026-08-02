//! Per-theme welcome animation: colours + particle overlays.
use ratatui::style::Color;

use crate::render::color::blend_color;
use crate::theme::tokyonight::MoonAnimation;

use super::raster::MoonSize;

/// One animation frame: disc colours + sparse particle overlays.
pub(super) struct AnimFrame {
    pub lit: Color,
    pub dark: Color,
    pub particles: Vec<(u16, u16, Color)>,
}

/// Build the themed colour/particle frame for the current clock.
pub(super) fn anim_frame(
    animation: MoonAnimation,
    base: Color,
    size: MoonSize,
    secs: f32,
) -> AnimFrame {
    match animation {
        MoonAnimation::BloodMoon => blood_moon(base, size, secs),
        MoonAnimation::MoonLight => moon_light(base, size, secs),
        MoonAnimation::MoonDark | MoonAnimation::Standard => moon_dark(base, size, secs),
        MoonAnimation::GrokX => grok_x(base, size, secs),
        MoonAnimation::OceanWhale => ocean_whale(base, size, secs),
        MoonAnimation::SakuraPetals => sakura(base, size, secs),
        MoonAnimation::ForestGlow => forest(base, size, secs),
    }
}

fn blood_moon(base: Color, size: MoonSize, secs: f32) -> AnimFrame {
    let pulse = 0.5 + 0.35 * (std::f32::consts::TAU * secs / 2.8).sin();
    let wobble = 0.12 * (std::f32::consts::TAU * secs / 1.5).sin();
    let lit = blend_color(
        base,
        Color::Rgb(230, 40, 40),
        (pulse + wobble).clamp(0.0, 1.0),
    )
    .unwrap_or(base);
    let dark = blend_color(base, Color::Rgb(60, 10, 15), 0.55).unwrap_or(base);
    let mut drops = Vec::with_capacity(8);
    let seed = (secs * 7.3) as u64;
    for i in 0..8u64 {
        let drop_phase = (secs * 0.7 + i as f32 * 0.8).fract();
        let x = (size.cols as f32 * 0.15
            + size.cols as f32 * 0.7 * ((seed.wrapping_mul(17 + i) as f32 * 0.01).fract()))
            as u16;
        let y = size.rows.saturating_sub(1) + (drop_phase * 3.0) as u16;
        let alpha = (1.0 - drop_phase).max(0.0);
        if y < size.rows.saturating_add(3) {
            drops.push((
                x.min(size.cols.saturating_sub(1)),
                y,
                blend_color(Color::Rgb(0, 0, 0), Color::Rgb(180, 30, 30), alpha)
                    .unwrap_or(Color::Rgb(0, 0, 0)),
            ));
        }
    }
    AnimFrame {
        lit,
        dark,
        particles: drops,
    }
}

fn moon_light(base: Color, size: MoonSize, secs: f32) -> AnimFrame {
    let glow = 0.82 + 0.12 * (std::f32::consts::TAU * secs / 5.5).sin();
    let lit = blend_color(base, Color::Rgb(255, 245, 225), glow).unwrap_or(base);
    let dark = blend_color(base, Color::Rgb(180, 170, 140), 0.45).unwrap_or(base);
    let mut corona = Vec::with_capacity(18);
    let ring_count = 18;
    for i in 0..ring_count {
        let angle = std::f32::consts::TAU * i as f32 / ring_count as f32 + secs * 0.3;
        let radius = size.cols as f32 * 0.65;
        let cx = size.cols as f32 / 2.0;
        let cy = size.rows as f32 / 2.0;
        let x = (cx + radius * angle.cos()) as u16;
        let y = (cy + radius * angle.sin() * 0.5) as u16;
        let sparkle = 0.3 + 0.4 * ((secs * 3.0 + i as f32).sin() * 0.5 + 0.5);
        if x < size.cols && y < size.rows {
            corona.push((
                x,
                y,
                blend_color(Color::Rgb(0, 0, 0), Color::Rgb(255, 220, 150), sparkle)
                    .unwrap_or(Color::Rgb(0, 0, 0)),
            ));
        }
    }
    AnimFrame {
        lit,
        dark,
        particles: corona,
    }
}

fn moon_dark(base: Color, size: MoonSize, secs: f32) -> AnimFrame {
    let breathe = 0.6 + 0.15 * (std::f32::consts::TAU * secs / 7.0).sin();
    let lit = blend_color(base, Color::Rgb(180, 200, 230), breathe).unwrap_or(base);
    let dark = blend_color(base, Color::Rgb(30, 35, 50), 0.35).unwrap_or(base);
    let mut stars = Vec::with_capacity(12);
    let star_seed = (secs * 13.7) as u64;
    for i in 0..12u64 {
        let sx = ((star_seed.wrapping_mul(31 + i * 7) as f32 * 0.001).fract() * size.cols as f32)
            as u16;
        let sy = ((star_seed.wrapping_mul(53 + i * 11) as f32 * 0.001).fract() * size.rows as f32)
            as u16;
        let twinkle = ((secs * 2.5 + i as f32 * 1.7).sin() * 0.5 + 0.5).powi(4);
        if sx < size.cols && sy < size.rows && twinkle > 0.05 {
            stars.push((
                sx,
                sy,
                blend_color(Color::Rgb(0, 0, 0), Color::Rgb(150, 180, 220), twinkle)
                    .unwrap_or(Color::Rgb(0, 0, 0)),
            ));
        }
    }
    AnimFrame {
        lit,
        dark,
        particles: stars,
    }
}

fn grok_x(base: Color, size: MoonSize, secs: f32) -> AnimFrame {
    let pulse = 0.75 + 0.2 * (std::f32::consts::TAU * secs / 1.8).sin();
    let lit = blend_color(base, Color::Rgb(80, 200, 255), pulse).unwrap_or(base);
    let scan_line = ((secs * 1.5).fract() * size.rows as f32) as u16;
    let dark = blend_color(base, Color::Rgb(20, 30, 50), 0.45).unwrap_or(base);
    let mut scans = Vec::new();
    for col in 0..size.cols {
        if col == scan_line || col == (scan_line + size.cols / 3) % size.cols {
            scans.push((
                col,
                0,
                blend_color(Color::Rgb(0, 0, 0), Color::Rgb(100, 220, 255), 0.15)
                    .unwrap_or(Color::Rgb(0, 0, 0)),
            ));
        }
    }
    AnimFrame {
        lit,
        dark,
        particles: scans,
    }
}

fn ocean_whale(base: Color, size: MoonSize, secs: f32) -> AnimFrame {
    let wave = 0.65 + 0.2 * (std::f32::consts::TAU * secs / 4.5).sin();
    let shimmer = 0.06 * (std::f32::consts::TAU * secs / 1.2).sin();
    let lit = blend_color(base, Color::Rgb(50, 140, 210), wave + shimmer).unwrap_or(base);
    let dark = blend_color(base, Color::Rgb(15, 30, 50), 0.4).unwrap_or(base);
    let mut ripples = Vec::with_capacity(size.rows as usize);
    for row in 0..size.rows {
        let offset = ((secs * 0.8 + row as f32 * 0.5).sin() * 0.5 + 0.5) as u16;
        let ripple_alpha = 0.08 + 0.06 * (secs * 2.0 + row as f32).sin();
        ripples.push((
            offset % size.cols,
            row,
            blend_color(Color::Rgb(0, 0, 0), Color::Rgb(80, 180, 240), ripple_alpha)
                .unwrap_or(Color::Rgb(0, 0, 0)),
        ));
    }
    AnimFrame {
        lit,
        dark,
        particles: ripples,
    }
}

fn sakura(base: Color, size: MoonSize, secs: f32) -> AnimFrame {
    let bloom = 0.72 + 0.18 * (std::f32::consts::TAU * secs / 4.0).sin();
    let lit = blend_color(base, Color::Rgb(242, 160, 192), bloom).unwrap_or(base);
    let dark = blend_color(base, Color::Rgb(80, 40, 55), 0.4).unwrap_or(base);
    let mut petals = Vec::with_capacity(10);
    for i in 0..10u32 {
        let fall = (secs * 0.35 + i as f32 * 0.37).fract();
        let sway = (secs * 1.1 + i as f32 * 1.7).sin() * 0.15;
        let x = ((0.1 + (i as f32 * 0.09) + sway).rem_euclid(1.0) * size.cols as f32) as u16;
        let y = (fall * (size.rows as f32 + 1.0)) as u16;
        let alpha = (1.0 - fall).max(0.15) * 0.7;
        if y < size.rows {
            petals.push((
                x.min(size.cols.saturating_sub(1)),
                y,
                blend_color(Color::Rgb(0, 0, 0), Color::Rgb(255, 180, 210), alpha)
                    .unwrap_or(Color::Rgb(0, 0, 0)),
            ));
        }
    }
    AnimFrame {
        lit,
        dark,
        particles: petals,
    }
}

fn forest(base: Color, size: MoonSize, secs: f32) -> AnimFrame {
    let breathe = 0.62 + 0.2 * (std::f32::consts::TAU * secs / 5.5).sin();
    let lit = blend_color(base, Color::Rgb(127, 176, 105), breathe).unwrap_or(base);
    let dark = blend_color(base, Color::Rgb(20, 35, 18), 0.4).unwrap_or(base);
    let mut fireflies = Vec::with_capacity(12);
    for i in 0..12u32 {
        let orbit = secs * (0.4 + (i % 3) as f32 * 0.12) + i as f32;
        let x = ((0.5 + 0.42 * orbit.cos()) * size.cols as f32) as u16;
        let y = ((0.5 + 0.38 * (orbit * 1.3).sin()) * size.rows as f32) as u16;
        let twinkle = ((secs * 3.2 + i as f32 * 2.1).sin() * 0.5 + 0.5).powi(2);
        if twinkle > 0.12 && x < size.cols && y < size.rows {
            fireflies.push((
                x,
                y,
                blend_color(Color::Rgb(0, 0, 0), Color::Rgb(180, 255, 140), twinkle)
                    .unwrap_or(Color::Rgb(0, 0, 0)),
            ));
        }
    }
    AnimFrame {
        lit,
        dark,
        particles: fireflies,
    }
}
