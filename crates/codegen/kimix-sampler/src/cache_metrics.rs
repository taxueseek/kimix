//! KV-cache 命中率持续指标。
//!
//! 每次采样响应把 `(prompt_tokens, cached_tokens)` 追加到
//! `<kimix-home>/metrics/cache_hit-<YYYY-MM-DD>.jsonl`（按天分文件，单文件
//! 天然有界），进程内维护窗口累计，进程退出时输出窗口摘要并追加一行
//! `process_summary`。下次启动时读取最近一条 summary 并输出，形成
//! 「上一进程命中率」的持续记录，供回归盯防。
//!
//! 控制：
//! - `KIMIX_CACHE_METRICS=0` 关闭
//! - `KIMIX_METRICS_DIR` 覆盖指标目录（默认 `<kimix-home>/metrics`）
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::types::RequestId;

/// 进程内窗口累计（`record` 每轮一次，Mutex 足够）。
#[derive(Default)]
struct WindowStats {
    requests: u64,
    prompt_tokens: u64,
    cached_tokens: u64,
}

/// 单次响应的命中率记录（追加到当天文件）。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheHitRecord {
    #[serde(rename = "type")]
    kind: String,
    ts_ms: u64,
    request_id: String,
    prompt_tokens: u64,
    cached_tokens: u64,
    cache_hit_percent: f64,
}

/// 进程退出时的窗口汇总（启动时读取最近一条输出）。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProcessSummary {
    #[serde(rename = "type")]
    kind: String,
    ts_ms: u64,
    pid: u32,
    requests: u64,
    prompt_tokens: u64,
    cached_tokens: u64,
    cache_hit_percent: f64,
}

pub struct CacheMetrics {
    dir: PathBuf,
    window: Mutex<WindowStats>,
}

static METRICS: OnceLock<CacheMetrics> = OnceLock::new();

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn today_stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn hit_percent(prompt_tokens: u64, cached_tokens: u64) -> f64 {
    if prompt_tokens > 0 {
        cached_tokens as f64 / prompt_tokens as f64 * 100.0
    } else {
        0.0
    }
}

fn metrics_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("KIMIX_METRICS_DIR")
        && !dir.is_empty()
    {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var("KIMIX_SHARE_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".kimix"))
        })?;
    Some(home.join("metrics"))
}

impl CacheMetrics {
    fn new() -> Option<Self> {
        let dir = metrics_dir()?;
        let _ = fs::create_dir_all(&dir);
        Some(Self {
            dir,
            window: Mutex::new(WindowStats::default()),
        })
    }

    fn today_file(&self) -> PathBuf {
        self.dir.join(format!("cache_hit-{}.jsonl", today_stamp()))
    }

    fn append_line(&self, line: &str) {
        let _ = fs::create_dir_all(&self.dir);
        let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.today_file())
        else {
            return;
        };
        let _ = writeln!(file, "{line}");
        let _ = file.flush();
    }

    fn record(&self, request_id: &RequestId, prompt_tokens: u32, cached_tokens: u32) {
        {
            let mut w = self.window.lock().unwrap();
            w.requests += 1;
            w.prompt_tokens = w.prompt_tokens.saturating_add(prompt_tokens as u64);
            w.cached_tokens = w.cached_tokens.saturating_add(cached_tokens as u64);
        }
        let record = CacheHitRecord {
            kind: "cache_hit".to_string(),
            ts_ms: now_ms(),
            request_id: request_id.to_string(),
            prompt_tokens: prompt_tokens as u64,
            cached_tokens: cached_tokens as u64,
            cache_hit_percent: hit_percent(prompt_tokens as u64, cached_tokens as u64),
        };
        if let Ok(line) = serde_json::to_string(&record) {
            self.append_line(&line);
        }
    }

    fn print_summary(&self) {
        let (requests, prompt_tokens, cached_tokens) = {
            let w = self.window.lock().unwrap();
            (w.requests, w.prompt_tokens, w.cached_tokens)
        };
        if requests == 0 {
            return;
        }
        let percent = hit_percent(prompt_tokens, cached_tokens);
        tracing::info!(
            target: "kimix_sampler::prompt_cache",
            requests,
            prompt_tokens,
            cached_tokens,
            cache_hit_percent = format!("{percent:.1}%"),
            "cache metrics: process summary"
        );
        let summary = ProcessSummary {
            kind: "process_summary".to_string(),
            ts_ms: now_ms(),
            pid: std::process::id(),
            requests,
            prompt_tokens,
            cached_tokens,
            cache_hit_percent: percent,
        };
        if let Ok(line) = serde_json::to_string(&summary) {
            self.append_line(&line);
        }
    }
}

/// 启动时初始化指标（幂等）。`KIMIX_CACHE_METRICS=0` 时静默关闭。
pub fn init() {
    if std::env::var("KIMIX_CACHE_METRICS").ok().as_deref() == Some("0") {
        return;
    }
    let metrics = METRICS.get_or_init(|| {
        CacheMetrics::new().unwrap_or_else(|| {
            // 无法解析 home 时退化为内存窗口（不落盘，仍可打印摘要）。
            CacheMetrics {
                dir: PathBuf::from("."),
                window: Mutex::new(WindowStats::default()),
            }
        })
    });
    // 启动摘要：输出上一进程的窗口命中率。
    if let Some(last) = read_last_process_summary(&metrics.dir) {
        tracing::info!(
            target: "kimix_sampler::prompt_cache",
            previous_requests = last.requests,
            previous_prompt_tokens = last.prompt_tokens,
            previous_cached_tokens = last.cached_tokens,
            previous_cache_hit_percent = format!("{:.1}%", last.cache_hit_percent),
            "cache metrics: previous process summary"
        );
    }
}

/// 每轮响应记录一次（三个流式后端共用）。
pub fn record(request_id: &RequestId, prompt_tokens: u32, cached_tokens: u32) {
    if let Some(metrics) = METRICS.get() {
        metrics.record(request_id, prompt_tokens, cached_tokens);
    }
}

/// 进程退出时输出窗口摘要（main 退出路径调用）。
pub fn print_summary() {
    if let Some(metrics) = METRICS.get() {
        metrics.print_summary();
    }
}

fn read_last_process_summary(dir: &std::path::Path) -> Option<ProcessSummary> {
    // 只查今天的文件（昨天的摘要已由昨天的启动输出过，不重复）。
    let path = dir.join(format!("cache_hit-{}.jsonl", today_stamp()));
    let contents = fs::read_to_string(path).ok()?;
    contents
        .lines()
        .filter_map(|line| serde_json::from_str::<ProcessSummary>(line).ok())
        .next_back()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_percent_matches_expected() {
        assert_eq!(hit_percent(100, 90), 90.0);
        assert_eq!(hit_percent(0, 0), 0.0);
        assert_eq!(hit_percent(200, 0), 0.0);
    }

    #[test]
    fn record_and_summary_roundtrip() {
        let dir = std::env::temp_dir().join(format!("kimix-cache-metrics-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let metrics = CacheMetrics {
            dir,
            window: Mutex::new(WindowStats::default()),
        };
        metrics.record(&RequestId::random(), 100, 90);
        metrics.record(&RequestId::random(), 200, 150);
        metrics.print_summary();
        let file = metrics.today_file();
        let contents = fs::read_to_string(&file).expect("file written");
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 3, "two records + one summary");
        let summary: ProcessSummary =
            serde_json::from_str(lines.last().unwrap()).expect("summary parses");
        assert_eq!(summary.requests, 2);
        assert_eq!(summary.prompt_tokens, 300);
        assert_eq!(summary.cached_tokens, 240);
        assert!((summary.cache_hit_percent - 80.0).abs() < 1e-9);
        let _ = fs::remove_dir_all(&file.parent().unwrap());
    }
}
