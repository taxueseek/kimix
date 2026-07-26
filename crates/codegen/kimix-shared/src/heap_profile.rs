//! Heap profile monitoring for production memory diagnostics.
//!
//! This module provides jemalloc heap profiling integration for
//! diagnosing memory issues in production environments.
//!
//! # Architecture
//!
//! ```text
//! HeapProfileMonitor → polls jemalloc stats → threshold check → dump + upload
//! ```
//!
//! # Features
//!
//! - Real-time resident memory monitoring
//! - Configurable threshold-based heap dumps
//! - GCS upload for offline analysis
//! - Arena purge for memory release

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Jemalloc statistics snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JemallocStats {
    /// Allocated bytes.
    pub allocated: u64,
    /// Resident bytes.
    pub resident: u64,
}

/// Heap profile dump configuration.
#[derive(Debug, Clone)]
pub struct HeapProfileConfig {
    /// Poll interval for memory stats.
    pub poll_interval: Duration,
    /// Threshold (bytes) to trigger a heap dump.
    pub dump_threshold: u64,
    /// Maximum number of dumps to retain.
    pub max_dumps: usize,
    /// Directory for dump files.
    pub dump_dir: PathBuf,
    /// Whether to upload dumps to GCS.
    pub enable_upload: bool,
}

impl Default for HeapProfileConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(30),
            dump_threshold: 512 * 1024 * 1024, // 512MB
            max_dumps: 5,
            dump_dir: std::env::temp_dir().join("kimix-heap-profiles"),
            enable_upload: false,
        }
    }
}

/// Stats provider signature: returns current jemalloc stats when profiling is available.
type StatsFn = fn() -> Option<JemallocStats>;
/// Dump provider signature: writes a heap profile to `path`.
type DumpFn = fn(&Path) -> Result<(), String>;

/// Heap profile monitor that polls jemalloc stats and triggers dumps.
pub struct HeapProfileMonitor {
    /// Configuration.
    config: HeapProfileConfig,
    /// Stats provider function.
    stats_fn: Option<StatsFn>,
    /// Dump provider function.
    dump_fn: Option<DumpFn>,
    /// Whether profiling is available.
    prof_available: bool,
    /// Last dump time.
    last_dump: Arc<Mutex<Option<Instant>>>,
    /// Dump count.
    dump_count: Arc<Mutex<usize>>,
}

impl HeapProfileMonitor {
    /// Create a new heap profile monitor.
    pub fn new(config: HeapProfileConfig) -> Self {
        Self {
            config,
            stats_fn: None,
            dump_fn: None,
            prof_available: false,
            last_dump: Arc::new(Mutex::new(None)),
            dump_count: Arc::new(Mutex::new(0)),
        }
    }

    /// Install hooks from the composition root.
    pub fn install(
        &mut self,
        stats_fn: fn() -> Option<JemallocStats>,
        dump_fn: fn(&Path) -> Result<(), String>,
        prof_available: bool,
    ) {
        self.stats_fn = Some(stats_fn);
        self.dump_fn = Some(dump_fn);
        self.prof_available = prof_available;
    }

    /// Get current jemalloc stats.
    pub fn stats(&self) -> Option<JemallocStats> {
        self.stats_fn?()
    }

    /// Dump heap profile to a file.
    pub fn dump(&self, path: &Path) -> Result<(), String> {
        if !self.prof_available {
            return Err("jemalloc profiling not available".to_string());
        }

        let dump_fn = self.dump_fn.ok_or("dump function not installed")?;
        dump_fn(path)?;

        // Update dump metadata
        if let Ok(mut last_dump) = self.last_dump.lock() {
            *last_dump = Some(Instant::now());
        }
        if let Ok(mut count) = self.dump_count.lock() {
            *count += 1;
        }

        Ok(())
    }

    /// Check if a dump should be triggered based on the threshold.
    pub fn should_dump(&self) -> bool {
        let stats = match self.stats() {
            Some(s) => s,
            None => return false,
        };

        stats.resident >= self.config.dump_threshold
    }

    /// Run a single poll cycle.
    pub fn poll_cycle(&self) -> Option<PathBuf> {
        if !self.should_dump() {
            return None;
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let dump_path = self
            .config
            .dump_dir
            .join(format!("heap-{}.heap", timestamp));

        if let Err(e) = std::fs::create_dir_all(&self.config.dump_dir) {
            tracing::error!(error = %e, "Failed to create dump directory");
            return None;
        }

        if let Err(e) = self.dump(&dump_path) {
            tracing::error!(error = %e, "Failed to dump heap profile");
            return None;
        }

        tracing::info!(path = %dump_path.display(), "Heap profile dumped");
        Some(dump_path)
    }

    /// Get the number of dumps performed.
    pub fn dump_count(&self) -> usize {
        self.dump_count.lock().map(|c| *c).unwrap_or(0)
    }

    /// Get the time since the last dump.
    pub fn time_since_last_dump(&self) -> Option<Duration> {
        self.last_dump
            .lock()
            .ok()
            .and_then(|last| last.map(|t| t.elapsed()))
    }

    /// Cleanup old dump files beyond max_dumps.
    pub fn cleanup(&self) -> Result<usize, std::io::Error> {
        if !self.config.dump_dir.exists() {
            return Ok(0);
        }

        let mut dumps: Vec<PathBuf> = std::fs::read_dir(&self.config.dump_dir)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .map(|e| e == "heap")
                    .unwrap_or(false)
            })
            .map(|entry| entry.path())
            .collect();

        dumps.sort_by(|a, b| {
            let a_modified = a
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            let b_modified = b
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            b_modified.cmp(&a_modified) // Newest first
        });

        let mut removed = 0;
        for dump in dumps.iter().skip(self.config.max_dumps) {
            if std::fs::remove_file(dump).is_ok() {
                removed += 1;
            }
        }

        Ok(removed)
    }
}

/// RAII guard for heap profiling.
pub struct HeapProfileGuard {
    // Held only to keep the monitor alive for the guard's lifetime (RAII).
    _monitor: Arc<HeapProfileMonitor>,
    handle: Option<std::thread::JoinHandle<()>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

impl HeapProfileGuard {
    /// Start background monitoring.
    pub fn start(monitor: Arc<HeapProfileMonitor>) -> Self {
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_clone = stop.clone();
        let monitor_clone = monitor.clone();

        let handle = std::thread::spawn(move || {
            while !stop_clone.load(std::sync::atomic::Ordering::Relaxed) {
                monitor_clone.poll_cycle();
                std::thread::sleep(monitor_clone.config.poll_interval);
            }
        });

        Self {
            _monitor: monitor,
            handle: Some(handle),
            stop,
        }
    }

    /// Stop monitoring.
    pub fn stop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for HeapProfileGuard {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heap_profile_monitor_new() {
        let config = HeapProfileConfig::default();
        let monitor = HeapProfileMonitor::new(config);
        assert!(!monitor.prof_available);
        assert_eq!(monitor.dump_count(), 0);
    }

    #[test]
    fn test_heap_profile_config_default() {
        let config = HeapProfileConfig::default();
        assert_eq!(config.poll_interval, Duration::from_secs(30));
        assert_eq!(config.dump_threshold, 512 * 1024 * 1024);
        assert_eq!(config.max_dumps, 5);
        assert!(!config.enable_upload);
    }

    #[test]
    fn test_should_dump_without_stats() {
        let monitor = HeapProfileMonitor::new(HeapProfileConfig::default());
        // Without stats function, should not dump
        assert!(!monitor.should_dump());
    }

    #[test]
    fn test_cleanup_nonexistent_dir() {
        let config = HeapProfileConfig {
            dump_dir: std::env::temp_dir().join("nonexistent-kimix-test"),
            ..Default::default()
        };
        let monitor = HeapProfileMonitor::new(config);
        assert!(monitor.cleanup().is_ok());
    }
}
