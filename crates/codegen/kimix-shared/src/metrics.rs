//! Metrics event tracking for model calls, tool calls, and agent requests.
//!
//! This module provides structured event tracking for quantitative analysis
//! of system performance and usage patterns.
//!
//! # Architecture
//!
//! ```text
//! Event → Bus → Subscriber → Handler (logging, aggregation, upload)
//! ```
//!
//! # Events
//!
//! - `ModelCall`: LLM API call metrics (TTFT, latency, tokens)
//! - `ToolCall`: Tool execution metrics (name, size, status)
//! - `AgentRequest`: Agent workflow metrics (phase, files changed)

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Model call event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCall {
    /// Session identifier.
    pub session_id: String,
    /// Model identifier.
    pub model_id: String,
    /// Provider identifier.
    pub provider: String,
    /// Time to first token (ms).
    pub ttft_ms: Option<u64>,
    /// Total latency (ms).
    pub latency_ms: u64,
    /// Cached read tokens.
    pub cached_read_tokens: u64,
    /// Total input tokens.
    pub total_tokens_in: u64,
    /// Total output tokens.
    pub total_tokens_out: u64,
    /// Finish reason.
    pub finish_reason: String,
    /// Timestamp.
    pub timestamp: u64,
}

/// Tool call event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Session identifier.
    pub session_id: String,
    /// Tool name.
    pub tool_name: String,
    /// Input size (bytes).
    pub input_bytes: usize,
    /// Output size (bytes).
    pub output_bytes: usize,
    /// Tool call identifier.
    pub tool_call_id: String,
    /// Status (success, error, cancelled).
    pub status: ToolCallStatus,
    /// Timestamp.
    pub timestamp: u64,
}

/// Tool call status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolCallStatus {
    Success,
    Error,
    Cancelled,
}

/// Agent request event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    /// Session identifier.
    pub session_id: String,
    /// Request phase.
    pub phase: String,
    /// Task type.
    pub task_type: String,
    /// Surface (cli, tui, headless).
    pub surface: String,
    /// Total input tokens.
    pub total_tokens_in: u64,
    /// Total output tokens.
    pub total_tokens_out: u64,
    /// Number of files changed.
    pub files_changed: usize,
    /// Validation status.
    pub validation_status: String,
    /// Timestamp.
    pub timestamp: u64,
}

/// A boxed event subscriber callback.
type Subscriber<T> = Box<dyn Fn(&T) + Send + Sync>;

/// Metrics event bus for publishing events.
pub struct MetricsBus {
    /// Subscribers for model call events.
    model_call_subscribers: Vec<Subscriber<ModelCall>>,
    /// Subscribers for tool call events.
    tool_call_subscribers: Vec<Subscriber<ToolCall>>,
    /// Subscribers for agent request events.
    agent_request_subscribers: Vec<Subscriber<AgentRequest>>,
    /// Event history for aggregation.
    history: Arc<Mutex<EventHistory>>,
}

/// Event history for aggregation.
#[derive(Debug, Default)]
struct EventHistory {
    /// Model calls by session.
    model_calls: HashMap<String, Vec<ModelCall>>,
    /// Tool calls by session.
    tool_calls: HashMap<String, Vec<ToolCall>>,
    /// Agent requests by session.
    agent_requests: HashMap<String, Vec<AgentRequest>>,
    /// Total events.
    total_events: usize,
}

impl MetricsBus {
    /// Create a new metrics bus.
    pub fn new() -> Self {
        Self {
            model_call_subscribers: Vec::new(),
            tool_call_subscribers: Vec::new(),
            agent_request_subscribers: Vec::new(),
            history: Arc::new(Mutex::new(EventHistory::default())),
        }
    }

    /// Subscribe to model call events.
    pub fn subscribe_model_call(&mut self, handler: impl Fn(&ModelCall) + Send + Sync + 'static) {
        self.model_call_subscribers.push(Box::new(handler));
    }

    /// Subscribe to tool call events.
    pub fn subscribe_tool_call(&mut self, handler: impl Fn(&ToolCall) + Send + Sync + 'static) {
        self.tool_call_subscribers.push(Box::new(handler));
    }

    /// Subscribe to agent request events.
    pub fn subscribe_agent_request(
        &mut self,
        handler: impl Fn(&AgentRequest) + Send + Sync + 'static,
    ) {
        self.agent_request_subscribers.push(Box::new(handler));
    }

    /// Publish a model call event.
    pub fn publish_model_call(&self, event: &ModelCall) {
        // Notify subscribers
        for subscriber in &self.model_call_subscribers {
            subscriber(event);
        }

        // Store in history
        if let Ok(mut history) = self.history.lock() {
            history
                .model_calls
                .entry(event.session_id.clone())
                .or_default()
                .push(event.clone());
            history.total_events += 1;
        }
    }

    /// Publish a tool call event.
    pub fn publish_tool_call(&self, event: &ToolCall) {
        // Notify subscribers
        for subscriber in &self.tool_call_subscribers {
            subscriber(event);
        }

        // Store in history
        if let Ok(mut history) = self.history.lock() {
            history
                .tool_calls
                .entry(event.session_id.clone())
                .or_default()
                .push(event.clone());
            history.total_events += 1;
        }
    }

    /// Publish an agent request event.
    pub fn publish_agent_request(&self, event: &AgentRequest) {
        // Notify subscribers
        for subscriber in &self.agent_request_subscribers {
            subscriber(event);
        }

        // Store in history
        if let Ok(mut history) = self.history.lock() {
            history
                .agent_requests
                .entry(event.session_id.clone())
                .or_default()
                .push(event.clone());
            history.total_events += 1;
        }
    }

    /// Get aggregated metrics for a session.
    pub fn get_session_metrics(&self, session_id: &str) -> Option<SessionMetrics> {
        let history = self.history.lock().ok()?;

        let model_calls = history.model_calls.get(session_id)?;
        let tool_calls = history
            .tool_calls
            .get(session_id)
            .cloned()
            .unwrap_or_default();
        let agent_requests = history
            .agent_requests
            .get(session_id)
            .cloned()
            .unwrap_or_default();

        // Calculate aggregated metrics
        let total_model_calls = model_calls.len();
        let total_tool_calls = tool_calls.len();
        let total_agent_requests = agent_requests.len();

        let avg_ttft_ms = if total_model_calls > 0 {
            let sum: u64 = model_calls.iter().filter_map(|c| c.ttft_ms).sum();
            Some(sum / total_model_calls as u64)
        } else {
            None
        };

        let avg_latency_ms = if total_model_calls > 0 {
            let sum: u64 = model_calls.iter().map(|c| c.latency_ms).sum();
            sum / total_model_calls as u64
        } else {
            0
        };

        let total_tokens_in: u64 = model_calls.iter().map(|c| c.total_tokens_in).sum();
        let total_tokens_out: u64 = model_calls.iter().map(|c| c.total_tokens_out).sum();

        let total_files_changed: usize = agent_requests.iter().map(|r| r.files_changed).sum();

        Some(SessionMetrics {
            session_id: session_id.to_string(),
            total_model_calls,
            total_tool_calls,
            total_agent_requests,
            avg_ttft_ms,
            avg_latency_ms,
            total_tokens_in,
            total_tokens_out,
            total_files_changed,
        })
    }

    /// Get total event count.
    pub fn total_events(&self) -> usize {
        self.history.lock().map(|h| h.total_events).unwrap_or(0)
    }

    /// Clear history for a session.
    pub fn clear_session(&self, session_id: &str) {
        if let Ok(mut history) = self.history.lock() {
            history.model_calls.remove(session_id);
            history.tool_calls.remove(session_id);
            history.agent_requests.remove(session_id);
        }
    }
}

impl Default for MetricsBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Aggregated metrics for a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetrics {
    /// Session identifier.
    pub session_id: String,
    /// Total model calls.
    pub total_model_calls: usize,
    /// Total tool calls.
    pub total_tool_calls: usize,
    /// Total agent requests.
    pub total_agent_requests: usize,
    /// Average time to first token (ms).
    pub avg_ttft_ms: Option<u64>,
    /// Average latency (ms).
    pub avg_latency_ms: u64,
    /// Total input tokens.
    pub total_tokens_in: u64,
    /// Total output tokens.
    pub total_tokens_out: u64,
    /// Total files changed.
    pub total_files_changed: usize,
}

/// Helper struct for timing operations.
pub struct Timer {
    /// Start time.
    start: Instant,
    /// Label for the timer.
    label: String,
}

impl Timer {
    /// Create a new timer.
    pub fn new(label: &str) -> Self {
        Self {
            start: Instant::now(),
            label: label.to_string(),
        }
    }

    /// Elapsed time in milliseconds.
    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    /// Elapsed time as Duration.
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    /// Complete the timer and return elapsed milliseconds.
    pub fn finish(self) -> u64 {
        self.elapsed_ms()
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        if std::thread::panicking() {
            tracing::warn!(label = %self.label, elapsed_ms = self.elapsed_ms(), "timer dropped during panic");
        }
    }
}

/// Get current timestamp in milliseconds.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_metrics_bus_publish_model_call() {
        let mut bus = MetricsBus::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        bus.subscribe_model_call(move |_| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        let event = ModelCall {
            session_id: "test".to_string(),
            model_id: "gpt-4".to_string(),
            provider: "openai".to_string(),
            ttft_ms: Some(100),
            latency_ms: 500,
            cached_read_tokens: 0,
            total_tokens_in: 1000,
            total_tokens_out: 500,
            finish_reason: "stop".to_string(),
            timestamp: now_ms(),
        };

        bus.publish_model_call(&event);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert_eq!(bus.total_events(), 1);
    }

    #[test]
    fn test_metrics_bus_publish_tool_call() {
        let mut bus = MetricsBus::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        bus.subscribe_tool_call(move |_| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        let event = ToolCall {
            session_id: "test".to_string(),
            tool_name: "read_file".to_string(),
            input_bytes: 100,
            output_bytes: 500,
            tool_call_id: "tc-1".to_string(),
            status: ToolCallStatus::Success,
            timestamp: now_ms(),
        };

        bus.publish_tool_call(&event);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_timer() {
        let timer = Timer::new("test");
        std::thread::sleep(Duration::from_millis(10));
        let elapsed = timer.finish();
        assert!(elapsed >= 10);
    }

    #[test]
    fn test_session_metrics() {
        let bus = MetricsBus::new();

        // Publish some events
        for i in 0..5 {
            let event = ModelCall {
                session_id: "test".to_string(),
                model_id: "gpt-4".to_string(),
                provider: "openai".to_string(),
                ttft_ms: Some(100 + i * 10),
                latency_ms: 500 + i * 50,
                cached_read_tokens: 0,
                total_tokens_in: 1000,
                total_tokens_out: 500,
                finish_reason: "stop".to_string(),
                timestamp: now_ms(),
            };
            bus.publish_model_call(&event);
        }

        let metrics = bus.get_session_metrics("test").unwrap();
        assert_eq!(metrics.total_model_calls, 5);
        assert!(metrics.avg_ttft_ms.is_some());
        assert!(metrics.avg_latency_ms > 0);
    }
}
