//! Non-moon logo shapes (Grok X, DeepSeek whale).
use super::raster::{MoonCell, MoonSize};

/// Rasterize a blue whale silhouette for the DeepSeek theme.
pub(super) fn whale_cells(size: MoonSize, secs: f32) -> Vec<Vec<Option<MoonCell>>> {
    let dots_w = (size.cols * 2) as f32;
    let dots_h = (size.rows * 4) as f32;
    let cx = dots_w / 2.0;
    let cy = dots_h / 2.0;
    let breathe = (std::f32::consts::TAU * secs / 3.5).sin() * 0.5;
    let scale = 1.0 + breathe * 0.04;

    const DOT_BITS: [[u32; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

    (0..size.rows as i32)
        .map(|cell_row| {
            (0..size.cols as i32)
                .map(|cell_col| {
                    let mut lit_mask = 0u32;
                    let mut dark_mask = 0u32;
                    for (dot_col, col_bits) in DOT_BITS.iter().enumerate() {
                        for (dot_row, bit) in col_bits.iter().enumerate() {
                            let x = cell_col as f32 * 2.0 + dot_col as f32;
                            let y = cell_row as f32 * 4.0 + dot_row as f32;
                            let dx = (x - cx) / scale;
                            let dy = (y - cy) / scale;
                            let dx_norm = dx / (dots_w * 0.45);
                            let dy_norm = dy / (dots_h * 0.45);

                            let body = dx_norm * dx_norm * 1.3 + dy_norm * dy_norm < 0.85;
                            let tail = dx_norm < -0.85
                                && dx_norm > -1.2
                                && dy_norm.abs() < 0.8 - (dx_norm + 0.85).abs() * 1.5;
                            let head = dx_norm > 0.75 && dx_norm < 1.05 && dy_norm.abs() < 0.3;
                            let eye = (dx_norm - 0.55) * (dx_norm - 0.55) * 25.0
                                + (dy_norm + 0.08) * (dy_norm + 0.08) * 25.0
                                < 0.25;
                            let blowhole = (dx_norm - 0.15) * (dx_norm - 0.15) * 30.0
                                + (dy_norm + 0.35) * (dy_norm + 0.35) * 30.0
                                < 0.2;
                            let dorsal = dx_norm > 0.1
                                && dx_norm < 0.35
                                && dy_norm < -0.55
                                && dy_norm > -0.9
                                && dy_norm.abs() > 0.55 + (dx_norm - 0.22).abs() * 2.0;

                            if body || tail || head {
                                lit_mask |= bit;
                            } else if dorsal || blowhole {
                                dark_mask |= bit;
                            } else if eye {
                                dark_mask |= bit;
                            }
                        }
                    }
                    let (mask, lit) = if lit_mask != 0 {
                        (lit_mask, true)
                    } else {
                        (dark_mask, false)
                    };
                    (mask != 0).then(|| MoonCell {
                        ch: char::from_u32(0x2800 + mask).expect("braille block"),
                        lit,
                    })
                })
                .collect()
        })
        .collect()
}

/// Rasterize a geometric "X" logo for the Grok theme.
pub(super) fn grok_x_cells(size: MoonSize, secs: f32) -> Vec<Vec<Option<MoonCell>>> {
    let dots_w = (size.cols * 2) as i32;
    let dots_h = (size.rows * 4) as i32;
    let cx = (dots_w - 1) as f32 / 2.0;
    let cy = (dots_h - 1) as f32 / 2.0;
    let half_w = dots_w as f32 * 0.42;
    let half_h = dots_h as f32 * 0.42;
    let band = 1.0 + 0.5 * (std::f32::consts::TAU * secs / 1.3).sin().abs();

    const DOT_BITS: [[u32; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

    (0..size.rows as i32)
        .map(|cell_row| {
            (0..size.cols as i32)
                .map(|cell_col| {
                    let mut lit_mask = 0u32;
                    let mut dark_mask = 0u32;
                    for (dot_col, col_bits) in DOT_BITS.iter().enumerate() {
                        for (dot_row, bit) in col_bits.iter().enumerate() {
                            let x = cell_col * 2 + dot_col as i32;
                            let y = cell_row * 4 + dot_row as i32;
                            let dx = (x as f32 - cx) / half_w;
                            let dy = (y as f32 - cy) / half_h;

                            let d1 = (dx - dy).abs();
                            let d2 = (dx + dy).abs();
                            let dist_to_x = d1.min(d2);

                            let centre_dist = (dx * dx + dy * dy).sqrt();
                            let is_centre = centre_dist < 0.35;
                            let is_x = dist_to_x < band && centre_dist < 1.1;

                            if is_centre || is_x {
                                let pulse =
                                    0.6 + 0.4 * (std::f32::consts::TAU * secs / 0.8).sin().abs();
                                if (is_centre && pulse > 0.7) || is_x {
                                    lit_mask |= bit;
                                } else {
                                    dark_mask |= bit;
                                }
                            } else if centre_dist < 1.05 {
                                dark_mask |= bit;
                            }
                        }
                    }
                    let (mask, lit) = if lit_mask != 0 {
                        (lit_mask, true)
                    } else {
                        (dark_mask, false)
                    };
                    (mask != 0).then(|| MoonCell {
                        ch: char::from_u32(0x2800 + mask).expect("braille block"),
                        lit,
                    })
                })
                .collect()
        })
        .collect()
}
