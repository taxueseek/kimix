//! Moon disc rasterization and phase-keyed shared cache.
use std::sync::Arc;

/// Full-size moon (hero box and tall stacked layouts), in braille cells.
pub(super) const FULL_COLS: u16 = 20;
pub(super) const FULL_ROWS: u16 = 10;
/// Small moon for short windows, in braille cells.
pub(super) const SMALL_COLS: u16 = 10;
pub(super) const SMALL_ROWS: u16 = 5;

/// Height at or above which the small moon is shown (below it, no logo).
pub(super) const SMALL_LOGO_MIN_HEIGHT: u16 = 22;
/// Height at or above which the full moon is shown.
pub(super) const FULL_LOGO_MIN_HEIGHT: u16 = 26;

/// Seconds for one full lunation (new → full → new).
pub(super) const LUNATION_SECS: f32 = 8.0;
/// Redraw cadence in frames per second.
pub(super) const MOON_FPS: f32 = 12.0;
/// Squared inner radius (normalized) of the dark-limb outline ring.
const RING_INNER_SQ: f32 = 0.82;

/// Lunar maria as `(cx, cy, radius²)` in normalized disc coordinates.
const MARIA: &[(f32, f32, f32)] = &[
    (-0.40, -0.42, 0.018), // Imbrium
    (0.12, -0.50, 0.008),  // Serenitatis
    (0.40, -0.22, 0.012),  // Tranquillitatis
    (0.55, 0.20, 0.005),   // Fecunditatis
    (0.62, -0.42, 0.004),  // Crisium
    (-0.58, 0.05, 0.010),  // Procellarum
    (-0.28, 0.40, 0.006),  // Nubium
    (0.08, 0.12, 0.004),   // Vaporum
];

const CRATERS: &[(f32, f32, f32)] = &[
    (-0.15, -0.35, 0.003),
    (0.22, -0.15, 0.002),
    (-0.48, 0.20, 0.004),
    (0.30, 0.30, 0.003),
    (-0.65, -0.10, 0.003),
    (0.05, -0.55, 0.002),
    (0.50, -0.10, 0.003),
    (-0.20, 0.20, 0.002),
    (-0.35, -0.15, 0.002),
    (0.60, 0.05, 0.003),
];

pub(super) fn in_maria_or_crater(dx: f32, dy: f32) -> bool {
    MARIA.iter().chain(CRATERS.iter()).any(|&(mx, my, r_sq)| {
        let ex = dx - mx;
        let ey = dy - my;
        ex * ex + ey * ey <= r_sq
    })
}

/// One logo size tier, in braille cells.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct MoonSize {
    pub cols: u16,
    pub rows: u16,
}

pub(super) const FULL: MoonSize = MoonSize {
    cols: FULL_COLS,
    rows: FULL_ROWS,
};
pub(super) const SMALL: MoonSize = MoonSize {
    cols: SMALL_COLS,
    rows: SMALL_ROWS,
};

/// One rendered braille cell of the moon.
#[derive(Clone, Copy, Debug)]
pub(super) struct MoonCell {
    pub ch: char,
    pub lit: bool,
}

pub(super) type MoonGrid = Arc<Vec<Vec<Option<MoonCell>>>>;

/// Whether the normalized disc point `(dx, dy)` is sunlit at phase `p`.
pub(super) fn dot_lit(dx: f32, dy: f32, p: f32) -> bool {
    let k = (std::f32::consts::TAU * p).cos();
    let x_edge = (1.0 - dy * dy).max(0.0).sqrt();
    if p < 0.5 {
        dx >= k * x_edge
    } else {
        dx <= -k * x_edge
    }
}

/// Quantized lunation frames for the moon raster cache (~96 keys per size).
fn phase_frame_index(p: f32) -> u32 {
    let n = (LUNATION_SECS * MOON_FPS).round().max(1.0) as u32;
    ((p.fract() + 1.0).fract() * n as f32).floor() as u32 % n
}

/// Rasterize the moon at `size` and phase `p` into rows of braille cells.
pub(super) fn moon_cells(size: MoonSize, p: f32) -> Vec<Vec<Option<MoonCell>>> {
    const DOT_BITS: [[u32; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

    let dots_w = (size.cols * 2) as i32;
    let dots_h = (size.rows * 4) as i32;
    let cx = (dots_w as f32 - 1.0) / 2.0;
    let cy = (dots_h as f32 - 1.0) / 2.0;
    let r = (dots_w.min(dots_h) as f32) / 2.0 - 0.5;

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
                            let dx = (x as f32 - cx) / r;
                            let dy = (y as f32 - cy) / r;
                            let d_sq = dx * dx + dy * dy;
                            if d_sq > 1.0 {
                                continue;
                            }
                            let crater = in_maria_or_crater(dx, dy);
                            if dot_lit(dx, dy, p) {
                                if crater {
                                    dark_mask |= bit;
                                } else {
                                    lit_mask |= bit;
                                }
                            } else if crater || d_sq >= RING_INNER_SQ {
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

/// Shared moon disc raster. Hit path is `Arc::clone` only — no grid copy.
pub(super) fn moon_cells_cached(size: MoonSize, p: f32) -> MoonGrid {
    use std::cell::RefCell;
    struct Entry {
        cols: u16,
        rows: u16,
        phase_q: u32,
        cells: MoonGrid,
    }
    thread_local! {
        static CACHE: RefCell<Option<Entry>> = const { RefCell::new(None) };
    }
    let phase_q = phase_frame_index(p);
    CACHE.with(|slot| {
        let mut guard = slot.borrow_mut();
        if let Some(entry) = guard.as_ref()
            && entry.cols == size.cols
            && entry.rows == size.rows
            && entry.phase_q == phase_q
        {
            return Arc::clone(&entry.cells);
        }
        let cells = Arc::new(moon_cells(size, p));
        *guard = Some(Entry {
            cols: size.cols,
            rows: size.rows,
            phase_q,
            cells: Arc::clone(&cells),
        });
        cells
    })
}

/// Apply 太极 bit-clear to a single cell if `(col, row)` matches an orbiting
/// yin/yang patch. Copies only the touched cell — shared disc raster stays
/// immutable.
pub(super) fn apply_taiji(
    cell: Option<MoonCell>,
    col: u16,
    row: u16,
    size: MoonSize,
    secs: f32,
) -> Option<MoonCell> {
    let Some(mut cell) = cell else {
        return None;
    };
    let dots_w = (size.cols * 2) as f32;
    let dots_h = (size.rows * 4) as f32;
    let cx = (dots_w - 1.0) / 2.0;
    let cy = (dots_h - 1.0) / 2.0;
    let r = (dots_w.min(dots_h)) / 2.0 - 0.5;
    let orbit = std::f32::consts::TAU * secs / 12.0;

    let pairs = [
        (0.35 * orbit.cos(), 0.28 * orbit.sin(), true),
        (-0.35 * orbit.cos(), -0.28 * orbit.sin(), false),
    ];
    const DOT_BITS: [[u32; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

    for (dx, dy, force_dark) in pairs {
        let px = cx + dx * r;
        let py = cy + dy * r;
        let pcol = (px / 2.0) as i32;
        let prow = (py / 4.0) as i32;
        if pcol != col as i32 || prow != row as i32 || pcol < 0 || prow < 0 {
            continue;
        }
        let dot_col = (px as i32 % 2).max(0) as usize;
        let dot_row = (py as i32 % 4).max(0) as usize;
        let bit = DOT_BITS[dot_col.min(1)][dot_row.min(3)];
        let current = cell.ch as u32 - 0x2800;
        if force_dark && (current & bit) != 0 {
            cell.ch = char::from_u32(0x2800 + (current & !bit)).unwrap_or(cell.ch);
        }
    }
    Some(cell)
}
