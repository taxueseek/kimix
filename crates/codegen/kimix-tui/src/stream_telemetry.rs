//! Stream throughput telemetry for the bottom-left `tok/s` chrome.
//!
//! Lives at the crate root so `acp` can own it without depending on `views`.
//! Prefers live `NotificationMeta::total_tokens` deltas when the shell stamps
//! them; falls back to per-chunk `estimate_tokens` otherwise. Display-only —
//! zero cost when idle.
use std::time::{Duration, Instant};

use kimix_token_estimation::estimate_tokens;

use crate::acp::meta::NotificationMeta;

/// EMA coefficient (`0` = freeze, `1` = no smoothing).
const EMA_ALPHA: f32 = 0.35;
/// Hide the readout this long after the last observation.
const HOLD_AFTER: Duration = Duration::from_secs(4);
/// Minimum elapsed before reporting a rate (avoids 1-token → ∞ spikes).
const MIN_ELAPSED: Duration = Duration::from_millis(80);

/// Which signal currently drives the meter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Source {
    #[default]
    None,
    /// Shell-stamped `totalTokens` deltas.
    Meta,
    /// Chunk-text estimate (fallback).
    Estimate,
}

/// Rolling stream throughput meter (presentation-agnostic).
#[derive(Debug, Default)]
pub struct StreamTelemetry {
    stream_started: Option<Instant>,
    /// `totalTokens` snapshot at the current stream boundary (meta path).
    total_at_boundary: Option<u64>,
    /// Last observed session `totalTokens`.
    last_total: Option<u64>,
    /// Cumulative estimated tokens in the current stream (fallback path).
    est_tokens: u64,
    /// Previous observation used for short-window slope: `(instant, cumulative)`.
    prev: Option<(Instant, u64)>,
    last_event_at: Option<Instant>,
    source: Source,
    ema_rate: f32,
    cached_label: Option<String>,
    cached_rate_int: u32,
}

impl StreamTelemetry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin a new stream attempt.
    pub fn on_stream_boundary(&mut self) {
        let now = Instant::now();
        self.stream_started = Some(now);
        self.est_tokens = 0;
        self.prev = None;
        // Keep last_total so the next meta sample can still form a delta;
        // re-baseline total_at_boundary on the next meta observation.
        self.total_at_boundary = self.last_total;
        self.source = Source::None;
        // Soft-reset EMA so a fresh stream does not inherit a stale spike.
        self.ema_rate = 0.0;
        self.cached_label = None;
        self.cached_rate_int = 0;
    }

    /// Turn finished — keep the last rate visible briefly, then fade out.
    pub fn on_turn_end(&mut self) {
        self.stream_started = None;
    }

    /// Observe notification meta. Prefer `total_tokens` when present.
    pub fn observe_meta(&mut self, meta: &NotificationMeta) {
        if meta.is_replay {
            return;
        }
        let Some(total) = meta.total_tokens else {
            return;
        };
        let now = Instant::now();
        if self.stream_started.is_none() {
            self.on_stream_boundary();
        }
        if self.total_at_boundary.is_none() {
            self.total_at_boundary = Some(total);
        }
        self.last_total = Some(total);
        self.last_event_at = Some(now);
        self.source = Source::Meta;

        // Cumulative tokens produced since stream boundary.
        let baseline = self.total_at_boundary.unwrap_or(total);
        let cumulative = total.saturating_sub(baseline);
        self.observe_cumulative(now, cumulative);
    }

    /// Fallback: record a live text chunk when meta tokens are unavailable.
    ///
    /// No-op once the meta path has taken over for this stream, so the two
    /// signals never fight.
    pub fn record_chunk_fallback(&mut self, text: &str) {
        if text.is_empty() || self.source == Source::Meta {
            return;
        }
        let now = Instant::now();
        if self.stream_started.is_none() {
            self.on_stream_boundary();
        }
        self.est_tokens = self.est_tokens.saturating_add(estimate_tokens(text));
        self.last_event_at = Some(now);
        self.source = Source::Estimate;
        self.observe_cumulative(now, self.est_tokens);
    }

    /// Refresh the cached label from the current rate. Call once per frame
    /// before [`label_str`].
    pub fn tick_label(&mut self) {
        let Some(rate) = self.rate_tok_s() else {
            self.cached_label = None;
            self.cached_rate_int = 0;
            return;
        };
        let rounded = rate.round().clamp(0.0, 999_999.0) as u32;
        if self.cached_rate_int != rounded || self.cached_label.is_none() {
            self.cached_rate_int = rounded;
            // Approximate marker when driven by chunk estimate only.
            self.cached_label = Some(match self.source {
                Source::Estimate => format!("~{} tok/s", format_number(rounded)),
                _ => format!("{} tok/s", format_number(rounded)),
            });
        }
    }

    /// Bottom-bar label after [`tick_label`], e.g. `"42 tok/s"` or `"~42 tok/s"`.
    pub fn label_str(&self) -> Option<&str> {
        self.cached_label.as_deref()
    }

    /// Current smoothed rate in tokens per second, or `None` when idle/hidden.
    pub fn rate_tok_s(&self) -> Option<f32> {
        let last = self.last_event_at?;
        if last.elapsed() > HOLD_AFTER {
            return None;
        }
        if self.ema_rate < 0.5 {
            return None;
        }
        Some(self.ema_rate)
    }

    fn observe_cumulative(&mut self, now: Instant, cumulative: u64) {
        if let Some((t0, tok0)) = self.prev {
            let elapsed = now.saturating_duration_since(t0);
            if elapsed >= MIN_ELAPSED {
                let delta = cumulative.saturating_sub(tok0);
                if delta > 0 {
                    let raw = delta as f32 / elapsed.as_secs_f32();
                    self.push_ema(raw);
                }
            }
        } else if let Some(started) = self.stream_started {
            // First sample after boundary: whole-stream rate once enough time passes.
            let elapsed = now.saturating_duration_since(started);
            if elapsed >= MIN_ELAPSED && cumulative > 0 {
                self.push_ema(cumulative as f32 / elapsed.as_secs_f32());
            }
        }
        self.prev = Some((now, cumulative));
    }

    fn push_ema(&mut self, raw: f32) {
        if self.ema_rate < 0.5 {
            self.ema_rate = raw;
        } else {
            self.ema_rate = EMA_ALPHA * raw + (1.0 - EMA_ALPHA) * self.ema_rate;
        }
    }
}

fn format_number(rounded: u32) -> String {
    if rounded >= 10_000 {
        format!("{:.1}k", rounded as f32 / 1000.0)
    } else {
        rounded.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn meta_with_tokens(total: u64) -> NotificationMeta {
        NotificationMeta {
            total_tokens: Some(total),
            ..NotificationMeta::default()
        }
    }

    #[test]
    fn empty_meter_has_no_label() {
        let mut m = StreamTelemetry::new();
        m.tick_label();
        assert!(m.label_str().is_none());
    }

    #[test]
    fn meta_tokens_drive_rate() {
        let mut m = StreamTelemetry::new();
        m.on_stream_boundary();
        m.observe_meta(&meta_with_tokens(1000));
        thread::sleep(Duration::from_millis(100));
        m.observe_meta(&meta_with_tokens(1000 + 80));
        let rate = m.rate_tok_s().expect("rate after meta deltas");
        assert!(rate > 0.0, "rate={rate}");
        m.tick_label();
        let label = m.label_str().expect("label");
        assert!(label.contains("tok/s"), "label={label}");
        assert!(!label.starts_with('~'), "meta path should be exact: {label}");
    }

    #[test]
    fn chunk_fallback_marks_approximate() {
        let mut m = StreamTelemetry::new();
        m.on_stream_boundary();
        let chunk = "abcdefghijklmnop".repeat(8);
        m.record_chunk_fallback(&chunk);
        thread::sleep(Duration::from_millis(100));
        m.record_chunk_fallback(&chunk);
        assert!(m.rate_tok_s().unwrap_or(0.0) > 0.0);
        m.tick_label();
        let label = m.label_str().expect("label");
        assert!(label.starts_with('~'), "estimate path should be approximate: {label}");
    }

    #[test]
    fn meta_takes_over_from_estimate() {
        let mut m = StreamTelemetry::new();
        m.on_stream_boundary();
        m.record_chunk_fallback(&"x".repeat(400));
        assert_eq!(m.source, Source::Estimate);
        m.observe_meta(&meta_with_tokens(500));
        thread::sleep(Duration::from_millis(100));
        m.observe_meta(&meta_with_tokens(600));
        assert_eq!(m.source, Source::Meta);
        // Further chunks must not override meta.
        let before = m.est_tokens;
        m.record_chunk_fallback(&"y".repeat(400));
        assert_eq!(m.est_tokens, before);
    }

    #[test]
    fn stream_boundary_resets_estimate() {
        let mut m = StreamTelemetry::new();
        m.record_chunk_fallback(&"x".repeat(400));
        assert!(m.est_tokens > 0);
        m.on_stream_boundary();
        assert_eq!(m.est_tokens, 0);
    }

    #[test]
    fn format_number_compacts_large() {
        assert_eq!(format_number(42), "42");
        assert_eq!(format_number(12_500), "12.5k");
    }
}
