//! Shared utilities used by both `Kimix-shell` and its downstream clients
//! (e.g. `Kimix-pager-render`). This crate sits upstream of `Kimix-shell`
//! so it must never depend on it.
pub mod clipboard;
pub mod placeholder_images;
pub mod session;
pub mod stderr;
pub mod ui_config;
