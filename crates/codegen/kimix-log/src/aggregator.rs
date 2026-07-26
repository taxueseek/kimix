//! Log aggregator for summary mode.
//!
//! Reduces log noise by batching repeated log messages and flushing
//! them as a single summary line at configurable intervals.
//!
//! # Example
//!
//! ```rust
//! use kimix_log::aggregator::LogAggregator;
//! use std::time::Duration;
//!
//! let mut agg = LogAggregator::new(Duration::from_secs(60));
//!
//! // Record repeated events
//! agg.record("dns_refresh");
//! agg.record("dns_refresh");
//! agg.record("tool_call");
//!
//! // Check if we should flush
//! if agg.should_flush() {
//!     let summary = agg.flush_summary();
//!     // summary contains: "dns_refresh x5, tool_call x3"
//! }
//! ```

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Log aggregator that batches repeated messages into summary lines.
pub struct LogAggregator {
    /// Count of each message pattern since last flush.
    counts: HashMap<String, usize>,
    /// Time of last flush.
    last_flush: Instant,
    /// Interval between flushes.
    interval: Duration,
}

impl LogAggregator {
    /// Create a new aggregator with the specified flush interval.
    pub fn new(interval: Duration) -> Self {
        Self {
            counts: HashMap::new(),
            last_flush: Instant::now(),
            interval,
        }
    }

    /// Create a new aggregator with default 60-second interval.
    pub fn with_default_interval() -> Self {
        Self::new(Duration::from_secs(60))
    }

    /// Record a log message pattern.
    ///
    /// The message is deduplicated by its string content. If the same
    /// message is recorded multiple times, only the count is incremented.
    pub fn record(&mut self, message: &str) {
        *self.counts.entry(message.to_string()).or_insert(0) += 1;
    }

    /// Check if enough time has passed to flush the summary.
    pub fn should_flush(&self) -> bool {
        self.last_flush.elapsed() >= self.interval
    }

    /// Flush the summary and reset counters.
    ///
    /// Returns a formatted summary string like "dns_refresh x5, tool_call x3".
    /// If no messages were recorded, returns None.
    pub fn flush_summary(&mut self) -> Option<String> {
        if self.counts.is_empty() {
            return None;
        }

        let summary = self.format_summary();
        self.counts.clear();
        self.last_flush = Instant::now();
        Some(summary)
    }

    /// Get the current count for a message pattern.
    pub fn count(&self, message: &str) -> usize {
        self.counts.get(message).copied().unwrap_or(0)
    }

    /// Get the total number of recorded messages.
    pub fn total_count(&self) -> usize {
        self.counts.values().sum()
    }

    /// Format the current counts into a summary string.
    fn format_summary(&self) -> String {
        let mut entries: Vec<_> = self.counts.iter().collect();
        entries.sort_by(|a, b| b.1.cmp(a.1)); // Sort by count descending

        entries
            .iter()
            .map(|(msg, count)| {
                if **count == 1 {
                    msg.to_string()
                } else {
                    format!("{} x{}", msg, count)
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl Default for LogAggregator {
    fn default() -> Self {
        Self::with_default_interval()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregator_basic() {
        let mut agg = LogAggregator::new(Duration::from_secs(60));

        agg.record("dns_refresh");
        agg.record("dns_refresh");
        agg.record("tool_call");

        assert_eq!(agg.count("dns_refresh"), 2);
        assert_eq!(agg.count("tool_call"), 1);
        assert_eq!(agg.total_count(), 3);
    }

    #[test]
    fn test_aggregator_flush() {
        let mut agg = LogAggregator::new(Duration::from_millis(0)); // Immediate flush

        agg.record("dns_refresh");
        agg.record("dns_refresh");
        agg.record("tool_call");

        assert!(agg.should_flush());
        let summary = agg.flush_summary().unwrap();
        assert!(summary.contains("dns_refresh x2"));
        assert!(summary.contains("tool_call"));

        // After flush, counters are reset
        assert_eq!(agg.total_count(), 0);
    }

    #[test]
    fn test_aggregator_empty_flush() {
        let mut agg = LogAggregator::new(Duration::from_millis(0));
        assert!(agg.flush_summary().is_none());
    }

    #[test]
    fn test_aggregator_sorted_by_count() {
        let mut agg = LogAggregator::new(Duration::from_millis(0));

        agg.record("rare_event");
        agg.record("common_event");
        agg.record("common_event");
        agg.record("common_event");

        let summary = agg.flush_summary().unwrap();
        // "common_event x3" should come before "rare_event"
        let common_pos = summary.find("common_event").unwrap();
        let rare_pos = summary.find("rare_event").unwrap();
        assert!(common_pos < rare_pos);
    }
}
