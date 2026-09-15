//! Micro-benchmark harness for the reusable scrollback scratch buffer.
//!
//! Run manually with:
//! `cargo test -p kimix-tui --test scratch_buffer_perf -- --ignored --nocapture`
//!
//! This intentionally reports timings without making timing-dependent test
//! assertions. The purpose is to establish a comparable local measurement
//! before changing `ScratchBuffer::prepare()` semantics.

use std::hint::black_box;
use std::time::{Duration, Instant};

use kimix_tui::scrollback::render::ScratchBuffer;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

const WARMUP: usize = 200;
const ITERS: usize = 2_000;

fn per_op<F>(mut f: F) -> Duration
where
    F: FnMut(),
{
    for _ in 0..WARMUP {
        f();
    }

    let start = Instant::now();
    for _ in 0..ITERS {
        f();
    }
    start.elapsed() / ITERS as u32
}

fn measure(width: u16, height: u16) {
    let rect = Rect::new(0, 0, width, height);

    let mut scratch = ScratchBuffer::new();
    let prepare = per_op(|| {
        black_box(scratch.prepared(width, height));
    });

    // Isolate the reset cost from resize/setup. This is the lower-level
    // operation that `ScratchBuffer::prepare()` currently performs every
    // time, including when the backing buffer already has the required size.
    let mut buffer = Buffer::empty(rect);
    let reset = per_op(|| {
        buffer.reset();
        black_box(&buffer);
    });

    println!(
        "scratch_prepare width={width} height={height} prepare={prepare:?} reset={reset:?}"
    );
}

#[test]
#[ignore = "manual performance measurement; timing must not gate CI"]
fn measure_scratch_prepare_cost() {
    for (width, height) in [(80, 24), (120, 50), (200, 60), (240, 100)] {
        measure(width, height);
    }
}
