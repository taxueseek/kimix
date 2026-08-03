//! 本地遥测桥接：把 `kimix-tools` 的工具输入修复遥测以薄 re-export 暴露给
//! `kimix-bin`（后者只直接依赖 kimix-shell，不依赖 kimix-tools）。
//!
//! 保留模式与 `session::inference_metrics` 一致：调用路径不变，宿主 crate
//! 不必透传整棵依赖。

pub use kimix_tools::input_repair_telemetry::{flush_summary, set_current_model};
