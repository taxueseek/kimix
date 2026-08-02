//! Real-time streaming token throughput (`tok/s`) for the bottom-left chrome.
//!
//! Estimates tokens from live agent/thinking text chunks with
//! [`kimix_token_estimation::estimate_tokens`], then reports a short-window
//! rate. Smoothed with EMA so the readout does not flicker on sparse SSE
//! frames. Pure observation — zero cost when idle.
use std::time::{Duration, Instant};

use kimix_token_estimation::estimate_tokens;

/// Sliding window used for the raw rate (before EMA).
const WINDOW: Duration = Duration::from_millis(1500);
/// EMA coefficient for display smoothing (`0` = freeze, `1` = no smoothing).
const EMA_ALPHA: f32 = 0.35;
/// Hide the readout this long after the last streamed chunk.
const HOLD_AFTER_STREAM: Duration = Duration::from_secs(4);
/// Minimum elapsed before reporting a rate (avoids 1-token → ∞ spikes).
const MIN_ELAPSED: Duration = Duration::from_millis(80);

/// Rolling stream throughput meter.
#[derive(Debug, Default)]
pub struct TokenRateMeter {
    /// Wall clock when the current LLM stream attempt started.
    stream_started: Option<Instant>,
    /// Tokens accumulated in the current stream (estimate).
    stream_tokens: u64,
    /// Ring of `(instant, cumulative_tokens_in_stream)` for the window rate.
    samples: Vec<(Instant, u64)>,
    /// Last time a non-replay chunk was recorded.
    last_chunk_at: Option<Instant>,
    /// Display-smoothed rate (tok/s).
    ema_rate: f32,
    /// Cached label rewritten only when the formatted value changes.
    cached_label: Option<String>,
    /// Rate that produced `cached_label` (avoids reformat every frame).
    cached_rate_int: u32,
}

impl TokenRateMeter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin a new stream attempt (or no-op when already tracking the same one).
    pub fn on_stream_boundary(&mut self) {
        self.stream_started = Some(Instant::now());
        self.stream_tokens = 0;
        self.samples.clear();
        self.samples.push((Instant::now(), 0));
    }

    /// Record a live text chunk. Replay chunks must not call this.
    pub fn record_chunk(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let now = Instant::now();
        if self.stream_started.is_none() {
            self.on_stream_boundary();
        }
        self.stream_tokens = self.stream_tokens.saturating_add(estimate_tokens(text));
        self.last_chunk_at = Some(now);
        self.samples.push((now, self.stream_tokens));
        self.prune(now);
        self.refresh_ema(now);
    }

    /// Turn finished — keep the last rate visible briefly, then fade out.
    pub fn on_turn_end(&mut self) {
        // Keep samples / ema for HOLD_AFTER_STREAM; just stop growing.
        self.stream_started = None;
    }

    /// Current smoothed rate in tokens per second, or `None` when idle/hidden.
    pub fn rate_tok_s(&self) -> Option<f32> {
        let last = self.last_chunk_at?;
        if last.elapsed() > HOLD_AFTER_STREAM {
            return None;
        }
        if self.ema_rate < 0.5 {
            return None;
        }
        Some(self.ema_rate)
    }

    /// Bottom-bar label, e.g. `"42 tok/s"`. Cached across frames.
    pub fn label(&mut self) -> Option<&str> {
        let rate = self.rate_tok_s()?;
        let rounded = rate.round().clamp(0.0, 999_999.0) as u32;
        if self.cached_rate_int != rounded || self.cached_label.is_none() {
            self.cached_rate_int = rounded;
            self.cached_label = Some(format_rate(rounded));
        }
        self.cached_label.as_deref()
    }

    fn prune(&mut self, now: Instant) {
        let cutoff = now.checked_sub(WINDOW).unwrap_or(now);
        // Keep one sample at or before the window edge so the slope is defined.
        let first_keep = self
            .samples
            .iter()
            .rposition(|(t, _)| *t <= cutoff)
            .unwrap_or(0);
        if first_keep > 0 {
            self.samples.drain(..first_keep);
        }
        // Cap growth under pathological flood.
        if self.samples.len() > 256 {
            let drop_n = self.samples.len() - 128;
            self.samples.drain(..drop_n);
        }
    }

    fn refresh_ema(&mut self, now: Instant) {
        let Some(raw) = self.raw_rate(now) else {
            return;
        };
        if self.ema_rate < 0.5 {
            self.ema_rate = raw;
        } else {
            self.ema_rate = EMA_ALPHA * raw + (1.0 - EMA_ALPHA) * self.ema_rate;
        }
    }

    fn raw_rate(&self, now: Instant) -> Option<f32> {
        if self.samples.len() < 2 {
            // Fall back to whole-stream rate once enough time has passed.
            let start = self.stream_started?;
            let elapsed = now.saturating_duration_since(start);
            if elapsed < MIN_ELAPSED || self.stream_tokens == 0 {
                return None;
            }
            return Some(self.stream_tokens as f32 / elapsed.as_secs_f32());
        }
        let (t0, tok0) = self.samples[0];
        let (t1, tok1) = *self.samples.last()?;
        let elapsed = t1.saturating_duration_since(t0).max(now.saturating_duration_since(t0));
        if elapsed < MIN_ELAPSED {
            return None;
        }
        let delta = tok1.saturating_sub(tok0);
        if delta == 0 {
            return None;
        }
        Some(delta as f32 / elapsed.as_secs_f32())
    }
}

fn format_rate(rounded: u32) -> String {
    if rounded >= 10_000 {
        format!("{:.1}k tok/s", rounded as f32 / 1000.0)
    } else {
        format!("{rounded} tok/s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn empty_meter_has_no_label() {
        let mut m = TokenRateMeter::new();
        assert!(m.label().is_none());
    }

    #[test]
    fn chunks_produce_positive_rate() {
        let mut m = TokenRateMeter::new();
        m.on_stream_boundary();
        // Enough ASCII for a few tokens; wait past MIN_ELAPSED.
        let chunk = "abcdefghijklmnop".repeat(8); // 128 chars ≈ 32 tokens
        m.record_chunk(&chunk);
        thread::sleep(Duration::from_millis(100));
        m.record_chunk(&chunk);
        let rate = m.rate_tok_s().expect("rate after two chunks");
        assert!(rate > 0.0, "rate={rate}");
        let label = m.label().expect("label");
        assert!(label.contains("tok/s"), "label={label}");
    }

    #[test]
    fn format_rate_compacts_large() {
        assert_eq!(format_rate(42), "42 tok/s");
        assert_eq!(format_rate(12_500), "12.5k tok/s");
    }

    #[test]
    fn stream_boundary_resets_counts() {
        let mut m = TokenRateMeter::new();
        m.record_chunk(&"x".repeat(400)); // ~100 tokens
        assert!(m.stream_tokens > 0);
        m.on_stream_boundary();
        assert_eq!(m.stream_tokens, 0);
    }
}
