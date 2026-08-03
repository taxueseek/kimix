//! 工具输入修复的本地遥测（JSONL 按天落盘）。
//!
//! 记录每次工具参数修复事件，格式与 `kimix-sampler` 的 `cache_hit-<date>.jsonl`
//! 一致：按天分文件、进程内窗口累计、退出时输出摘要。数据**只写本地**
//! `~/.kimix/metrics/`，零网络出口，符合 kimix 的 zero-egress 契约。
//!
//! 用途：按 (model, tool) 观察修复率，发现某个模型在特定工具契约上退化时
//! 能比用户先察觉（推文里的免费副产品）。
//!
//! 控制：
//! - `KIMIX_REPAIR_METRICS=0` 关闭
//! - `KIMIX_METRICS_DIR` 覆盖指标目录（默认 `<kimix-home>/metrics`，与缓存指标共用）

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::input_repair::RepairKind;

/// 单次修复事件（追加到当天文件）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairRecord {
    #[serde(rename = "type")]
    pub kind: String,
    pub ts_ms: u64,
    /// 工具名（`kimix:read_file` 形态）
    pub tool: String,
    /// 模型 id（`deepseek-v4-flash` 等，来自路由/会话）
    pub model: String,
    /// `repaired` 或 `invalid`
    pub outcome: String,
    /// 命中的修复类型（逗号分隔），`invalid` 时为空
    #[serde(default)]
    pub repairs: String,
}

/// 进程内窗口累计（每条工具调用一次，Mutex 足够）。
#[derive(Default)]
struct WindowStats {
    /// 总工具调用数
    calls: u64,
    /// 修复成功数
    repaired: u64,
    /// 修复失败数
    invalid: u64,
    /// 无需修复数
    clean: u64,
}

pub struct RepairMetrics {
    dir: PathBuf,
    window: Mutex<WindowStats>,
    enabled: bool,
}

static METRICS: OnceLock<RepairMetrics> = OnceLock::new();

/// 进程级「当前模型 id」缓存，由会话层（`prepare_tool_call`）在每次解析
/// 工具参数前设置，供遥测记录时带上模型维度。Mutex 足够（低频写、高频读）。
static CURRENT_MODEL: OnceLock<Mutex<String>> = OnceLock::new();

/// 设置当前模型 id（会话层每次工具调用前调用）。
pub fn set_current_model(model: &str) {
    let m = CURRENT_MODEL.get_or_init(|| Mutex::new(String::new()));
    let mut guard = m.lock().expect("current model lock");
    *guard = model.to_string();
}

/// 读取当前模型 id；未设置时返回 `"unknown"`。
fn current_model() -> String {
    CURRENT_MODEL
        .get()
        .and_then(|m| m.lock().ok())
        .map(|g| g.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn today_stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// 全局遥测单例（惰性初始化，与 `cache_metrics.rs` 同款）。
pub fn metrics() -> &'static RepairMetrics {
    METRICS.get_or_init(|| {
        let enabled = std::env::var("KIMIX_REPAIR_METRICS")
            .map(|v| v != "0")
            .unwrap_or(true);
        let dir = std::env::var("KIMIX_METRICS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = kimix_config::kimix_home();
                home.join("metrics")
            });
        let _ = fs::create_dir_all(&dir);
        RepairMetrics {
            dir,
            window: Mutex::new(WindowStats::default()),
            enabled,
        }
    })
}

impl RepairMetrics {
    /// 记录一次工具输入修复事件。
    ///
    /// # Arguments
    ///
    /// * `tool` - 工具名（`kimix:read_file`）
    /// * `model` - 模型 id
    /// * `outcome` - `"repaired"` / `"invalid"` / `"clean"`
    /// * `repairs` - 命中的修复类型
    pub fn record(
        &self,
        tool: &str,
        model: &str,
        outcome: &str,
        repairs: &[RepairKind],
    ) {
        if !self.enabled {
            return;
        }
        // 调用方未显式传模型名时，回退到会话层设置的全局当前模型。
        let resolved_model = if model.is_empty() {
            current_model()
        } else {
            model.to_string()
        };
        {
            let mut w = self.window.lock().expect("repair metrics lock");
            w.calls += 1;
            match outcome {
                "repaired" => w.repaired += 1,
                "invalid" => w.invalid += 1,
                _ => w.clean += 1,
            }
        }
        let repair_names: Vec<&str> = repairs.iter().map(repair_kind_name).collect();
        let record = RepairRecord {
            kind: "tool_input".to_string(),
            ts_ms: now_ms(),
            tool: tool.to_string(),
            model: resolved_model,
            outcome: outcome.to_string(),
            repairs: repair_names.join(","),
        };
        let _ = fs::create_dir_all(&self.dir);
        let path = self.dir.join(format!("tool_repair-{}.jsonl", today_stamp()));
        if let (Ok(mut f), Ok(line)) = (
            OpenOptions::new().create(true).append(true).open(&path),
            serde_json::to_string(&record),
        ) {
            let _ = writeln!(f, "{line}");
        }
    }

    /// 进程退出时输出窗口摘要（追加到当天文件，启动时读取最近一条输出）。
    pub fn flush_summary(&self) {
        if !self.enabled {
            return;
        }
        let w = self.window.lock().expect("repair metrics lock");
        if w.calls == 0 {
            return;
        }
        #[derive(Serialize)]
        struct Summary {
            #[serde(rename = "type")]
            kind: String,
            ts_ms: u64,
            calls: u64,
            repaired: u64,
            invalid: u64,
            clean: u64,
            repair_rate_percent: f64,
        }
        // 分母 = 解析失败次数（repaired + invalid）；clean 不计。
        // repaired/(repaired+invalid) 才是「失败中被 harness 救回」的比例。
        let failed = w.repaired + w.invalid;
        let repair_rate = if failed > 0 {
            (w.repaired as f64 / failed as f64) * 100.0
        } else {
            0.0
        };
        let summary = Summary {
            kind: "tool_repair_summary".to_string(),
            ts_ms: now_ms(),
            calls: w.calls,
            repaired: w.repaired,
            invalid: w.invalid,
            clean: w.clean,
            repair_rate_percent: repair_rate,
        };
        let _ = fs::create_dir_all(&self.dir);
        let path = self.dir.join(format!("tool_repair-{}.jsonl", today_stamp()));
        if let (Ok(mut f), Ok(line)) = (
            OpenOptions::new().create(true).append(true).open(&path),
            serde_json::to_string(&summary),
        ) {
            let _ = writeln!(f, "{line}");
        }
    }
}

/// 顶层封装：进程退出时输出窗口摘要（仿 `cache_metrics::print_summary`）。
/// 由 `kimix-bin` 在退出路径调用；无调用时不落盘。
pub fn flush_summary() {
    let m = METRICS.get();
    if let Some(metrics) = m {
        metrics.flush_summary();
    }
}

fn repair_kind_name(kind: &RepairKind) -> &'static str {
    match kind {
        RepairKind::NullRemoved => "null_removed",
        RepairKind::StringifiedArrayParsed => "stringified_array_parsed",
        RepairKind::ObjectPlaceholderUnwrapped => "object_placeholder_unwrapped",
        RepairKind::BareStringWrapped => "bare_string_wrapped",
        RepairKind::MarkdownLinkUnwrapped => "markdown_link_unwrapped",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_kind_names_are_snake_case() {
        assert_eq!(repair_kind_name(&RepairKind::NullRemoved), "null_removed");
        assert_eq!(
            repair_kind_name(&RepairKind::StringifiedArrayParsed),
            "stringified_array_parsed"
        );
        assert_eq!(
            repair_kind_name(&RepairKind::BareStringWrapped),
            "bare_string_wrapped"
        );
    }

    #[test]
    fn record_with_disabled_metrics_is_noop() {
        // 不初始化全局（enabled 默认取决于 env），直接构造实例测 disabled 路径
        let m = RepairMetrics {
            dir: std::env::temp_dir(),
            window: Mutex::new(WindowStats::default()),
            enabled: false,
        };
        m.record("kimix:read_file", "deepseek-v4-flash", "repaired", &[]);
        // 不 panic 即通过
        let w = m.window.lock().expect("lock");
        assert_eq!(w.calls, 0, "disabled metrics must not record");
    }

    #[test]
    fn set_current_model_updates_global_and_record_falls_back() {
        // 测全局模型注入 + record 的空串回退逻辑。
        set_current_model("deepseek-v4-flash");
        assert_eq!(current_model(), "deepseek-v4-flash");
        // 用临时 metrics 目录避免污染真实目录
        let m = RepairMetrics {
            dir: std::env::temp_dir().join(format!(
                "kimix-repair-metrics-test-{}",
                std::process::id()
            )),
            window: Mutex::new(WindowStats::default()),
            enabled: true,
        };
        m.record("kimix:read_file", "", "repaired", &[]);
        m.record("kimix:bash", "mimo-v2.5-pro", "repaired", &[]);
        // 落盘记录应带正确的模型名
        let path = m
            .dir
            .join(format!("tool_repair-{}.jsonl", today_stamp()));
        let content = std::fs::read_to_string(&path).expect("record file");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("deepseek-v4-flash"), "falls back to global model");
        assert!(lines[1].contains("mimo-v2.5-pro"), "explicit model wins");
        // 清理
        let _ = std::fs::remove_file(&path);
    }
}
