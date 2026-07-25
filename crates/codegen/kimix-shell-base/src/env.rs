//! Environment helpers for the shell crate family.
//!
//! Kimix has exactly one environment; endpoint defaults live in the
//! [`kimix_env`] leaf crate so sibling crates can share them without
//! depending on this crate. This module re-exports the shared test
//! helper.
pub use kimix_env::EnvVarGuard;
