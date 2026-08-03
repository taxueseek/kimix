//! `kimix inspect` memtrace summary.
//!
//! The TUI's memory sampler (`kimix_tui::memory_trace`) appends one JSONL
//! file per process under `<kimix-home>/memtrace/`:
//! `<start-ts>-<pid>.jsonl` (plus a `.1` sibling after a 4 MiB rotation).
//! Each line is an event: `start` (pid + version), `sample` (footprint/RSS
//! + jemalloc gauges), `purge` (memory-cliff release with attribution), or
//! `threshold` (jemalloc dump). This module reduces those files to a compact
//! per-file summary without depending on the TUI crate.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// Aggregate summary for one trace file.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemtraceFileSummary {
    /// File name (`<start-ts>-<pid>.jsonl`, or `.jsonl.1` after rotation).
    pub file: String,
    /// PID from the `start` event; falls back to the file name.
    pub pid: Option<u32>,
    /// `start` event timestamp (unix millis).
    pub started_ms: Option<u64>,
    /// Timestamp of the most recent event (unix millis).
    pub last_event_ms: Option<u64>,
    /// Number of `sample` events.
    pub samples: u64,
    /// Number of `purge` events (memory cliffs with attribution).
    pub purges: u64,
    /// Number of `threshold` crossings (jemalloc dumps).
    pub thresholds: u64,
    /// Peak physical footprint observed (bytes; macOS only, `None` on Linux).
    pub peak_footprint_bytes: Option<u64>,
    /// Peak RSS observed (bytes).
    pub peak_rss_bytes: Option<u64>,
    /// Latest footprint observed (bytes).
    pub latest_footprint_bytes: Option<u64>,
    /// Latest RSS observed (bytes).
    pub latest_rss_bytes: Option<u64>,
    /// On-disk size of the trace file (bytes).
    pub file_size_bytes: u64,
}

/// All trace files under the memtrace directory.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemtraceSummary {
    /// Absolute path of the memtrace directory.
    pub directory: String,
    /// One entry per trace file, most recently started first.
    pub files: Vec<MemtraceFileSummary>,
}

/// Summarize every trace file under `dir`. `None` when the directory is
/// absent or contains nothing parseable (so `kimix inspect` just omits the
/// section).
pub fn summarize(dir: &Path) -> Option<MemtraceSummary> {
    if !dir.is_dir() {
        return None;
    }
    let mut files: Vec<MemtraceFileSummary> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| summarize_file(&e.path()))
        .collect();
    if files.is_empty() {
        return None;
    }
    files.sort_by(|a, b| {
        b.started_ms
            .unwrap_or(0)
            .cmp(&a.started_ms.unwrap_or(0))
            .then_with(|| a.file.cmp(&b.file))
    });
    Some(MemtraceSummary {
        directory: dir.display().to_string(),
        files,
    })
}

fn summarize_file(path: &Path) -> Option<MemtraceFileSummary> {
    let file_name = path.file_name()?.to_string_lossy().into_owned();
    // Only memtrace trace files: `<start-ts>-<pid>.jsonl[.1]`.
    if !file_name.ends_with(".jsonl") && !file_name.ends_with(".jsonl.1") {
        return None;
    }
    let data = std::fs::read_to_string(path).ok()?;
    let mut summary = MemtraceFileSummary {
        file: file_name.clone(),
        pid: pid_from_name(summary_name(&file_name)),
        started_ms: None,
        last_event_ms: None,
        samples: 0,
        purges: 0,
        thresholds: 0,
        peak_footprint_bytes: None,
        peak_rss_bytes: None,
        latest_footprint_bytes: None,
        latest_rss_bytes: None,
        file_size_bytes: data.len() as u64,
    };
    for line in data.lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue; // Malformed trailing line (partial write); skip.
        };
        let ts_ms = event.get("ts_ms").and_then(|t| t.as_u64());
        if let Some(ts) = ts_ms {
            summary.last_event_ms = Some(ts.max(summary.last_event_ms.unwrap_or(0)));
        }
        match event.get("kind").and_then(|k| k.as_str()) {
            Some("start") => {
                summary.started_ms = ts_ms;
                if let Some(pid) = event.get("pid").and_then(|p| p.as_u64()) {
                    summary.pid = Some(pid as u32);
                }
            }
            Some("sample") => summary.samples += 1,
            Some("purge") => summary.purges += 1,
            Some("threshold") => summary.thresholds += 1,
            _ => {}
        }
        if let Some(footprint) = event.get("footprint_bytes").and_then(|b| b.as_u64()) {
            summary.peak_footprint_bytes = Some(
                summary
                    .peak_footprint_bytes
                    .map_or(footprint, |p| p.max(footprint)),
            );
            summary.latest_footprint_bytes = Some(footprint);
        }
        if let Some(rss) = event.get("rss_bytes").and_then(|b| b.as_u64()) {
            summary.peak_rss_bytes = Some(summary.peak_rss_bytes.map_or(rss, |p| p.max(rss)));
            summary.latest_rss_bytes = Some(rss);
        }
    }
    Some(summary)
}

/// `<start-ts>-<pid>.jsonl[.1]` → the bare stem without the `.1` suffix.
fn summary_name(file_name: &str) -> &str {
    file_name
        .strip_suffix(".jsonl.1")
        .unwrap_or_else(|| file_name.strip_suffix(".jsonl").unwrap_or(file_name))
}

/// Parse the pid from a `<start-ts>-<pid>` stem (the last numeric segment).
fn pid_from_name(stem: &str) -> Option<u32> {
    stem.rsplit('-').next()?.parse::<u32>().ok()
}

/// Canonical memtrace directory for this install.
pub fn default_memtrace_dir() -> PathBuf {
    crate::util::kimix_home::kimix_home().join("memtrace")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_trace(dir: &Path, name: &str, lines: &[&str]) {
        let path = dir.join(name);
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
    }

    #[test]
    fn pid_from_name_parses_last_segment() {
        assert_eq!(pid_from_name("1767024000-32852"), Some(32852));
        assert_eq!(pid_from_name("1767024000-abc"), None);
    }

    #[test]
    fn summarize_aggregates_events() {
        let dir = std::env::temp_dir().join(format!(
            "kimix-inspect-memtrace-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        write_trace(
            &dir,
            "1767024000-100.jsonl",
            &[
                r#"{"ts_ms":100,"kind":"start","pid":100,"version":"0.1.17"}"#,
                r#"{"ts_ms":200,"kind":"sample","footprint_bytes":1000,"rss_bytes":2000,"alloc":{"allocated":900,"active":950,"resident":1800,"mapped":4000,"retained":5000,"metadata":64}}"#,
                r#"{"ts_ms":300,"kind":"purge","footprint_bytes":600,"rss_bytes":1500,"reason":"session-load-replay","hook_installed":true,"gauge_before_bytes":1000,"purge_us":12}"#,
                r#"{"ts_ms":400,"kind":"threshold","footprint_bytes":5000,"threshold_bytes":4096,"dump_file":"1767024000-100-jemalloc-0.txt"}"#,
            ],
        );
        let summary = summarize(&dir).expect("summary present");
        assert_eq!(summary.files.len(), 1);
        let f = &summary.files[0];
        assert_eq!(f.pid, Some(100));
        assert_eq!(f.started_ms, Some(100));
        assert_eq!(f.last_event_ms, Some(400));
        assert_eq!(f.samples, 1);
        assert_eq!(f.purges, 1);
        assert_eq!(f.thresholds, 1);
        assert_eq!(f.peak_footprint_bytes, Some(5000));
        assert_eq!(f.latest_footprint_bytes, Some(5000));
        assert_eq!(f.peak_rss_bytes, Some(2000));
        assert_eq!(f.latest_rss_bytes, Some(1500));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn summarize_returns_none_for_empty_dir() {
        let dir = std::env::temp_dir().join(format!(
            "kimix-inspect-memtrace-empty-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(summarize(&dir).is_none());
        assert!(summarize(&dir.join("missing")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn summarize_skips_non_trace_files() {
        let dir = std::env::temp_dir().join(format!(
            "kimix-inspect-memtrace-skip-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        write_trace(&dir, "notes.txt", &["not a trace"]);
        assert!(summarize(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotation_sibling_is_included() {
        let dir =
            std::env::temp_dir().join(format!("kimix-inspect-memtrace-rot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_trace(
            &dir,
            "1767024000-200.jsonl",
            &[r#"{"ts_ms":1,"kind":"start","pid":200}"#],
        );
        write_trace(
            &dir,
            "1767024000-200.jsonl.1",
            &[r#"{"ts_ms":2,"kind":"sample","rss_bytes":42}"#],
        );
        let summary = summarize(&dir).expect("summary present");
        assert_eq!(summary.files.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
