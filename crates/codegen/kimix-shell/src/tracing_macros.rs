//! Tracing-based utility macros (inlined from the former kimix-tracing-macros crate).
//!
//! These macros provide:
//! - Timestamped logging (`tprintln!`, `teprintln!`)
//! - Execution timing with automatic logging (`timed!`)

/// Prints a message via tracing::info with a Unix timestamp prefix.
#[macro_export]
macro_rules! tprintln {
    () => {{
        let ts = ::std::time::SystemTime::now()
            .duration_since(::std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        ::tracing::info!("{}::", ts)
    }};
    ($($arg:tt)*) => {{
        let ts = ::std::time::SystemTime::now()
            .duration_since(::std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        ::tracing::info!("{}::{}", ts, ::std::format_args!($($arg)*))
    }};
}

/// Prints a message via tracing::warn with a Unix timestamp prefix.
#[macro_export]
macro_rules! teprintln {
    () => {{
        let ts = ::std::time::SystemTime::now()
            .duration_since(::std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        ::tracing::warn!("{}::", ts)
    }};
    ($($arg:tt)*) => {{
        let ts = ::std::time::SystemTime::now()
            .duration_since(::std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        ::tracing::warn!("{}::{}", ts, ::std::format_args!($($arg)*))
    }};
}

/// Measure and log execution time of a block.
///
/// Variants:
/// - `timed!({ ... })` — Returns `(value, elapsed_ms)`.
/// - `timed!(log: "name", { ... })` — Logs at debug level, returns value.
/// - `timed!(log: level, "name", { ... })` — Logs at `level`, returns value.
/// - `timed!(try: "name", { ... })` — Sync Result, logs and returns Result.
/// - `timed!(try: level, "name", { ... })` — Sync Result with explicit level.
/// - `timed!(try: "name", async { ... })` — Async Result.
/// - `timed!(try: level, "name", async { ... })` — Async Result with explicit level.
#[macro_export]
macro_rules! timed {
    (@log_ok $lvl:ident, $name:expr, $elapsed_ms:expr) => {{
        ::tracing::$lvl!(elapsed_ms = $elapsed_ms as u64, "{}", $name);
    }};
    (@log_err $lvl:ident, $name:expr, $elapsed_ms:expr, $err:expr) => {{
        ::tracing::$lvl!(elapsed_ms = $elapsed_ms as u64, error = ?$err, "{}", $name);
    }};

    ($block:block) => {{
        let start = ::std::time::Instant::now();
        let value = $block;
        let elapsed_ms = start.elapsed().as_millis();
        (value, elapsed_ms)
    }};

    (log: $name:expr, $block:block) => {{
        let start = ::std::time::Instant::now();
        let value = $block;
        let elapsed_ms = start.elapsed().as_millis();
        $crate::timed!(@log_ok debug, $name, elapsed_ms);
        value
    }};

    (log: $lvl:ident, $name:expr, $block:block) => {{
        let start = ::std::time::Instant::now();
        let value = $block;
        let elapsed_ms = start.elapsed().as_millis();
        $crate::timed!(@log_ok $lvl, $name, elapsed_ms);
        value
    }};

    (try: $name:expr, $block:block) => {{
        let start = ::std::time::Instant::now();
        let result = (|| $block)();
        let elapsed_ms = start.elapsed().as_millis();
        match result {
            Ok(value) => {
                $crate::timed!(@log_ok debug, $name, elapsed_ms);
                Ok(value)
            }
            Err(err) => {
                $crate::timed!(@log_err debug, $name, elapsed_ms, err);
                Err(err)
            }
        }
    }};

    (try: $lvl:ident, $name:expr, $block:block) => {{
        let start = ::std::time::Instant::now();
        let result = (|| $block)();
        let elapsed_ms = start.elapsed().as_millis();
        match result {
            Ok(value) => {
                $crate::timed!(@log_ok $lvl, $name, elapsed_ms);
                Ok(value)
            }
            Err(err) => {
                $crate::timed!(@log_err $lvl, $name, elapsed_ms, err);
                Err(err)
            }
        }
    }};

    (try: $name:expr, async $block:block) => {{
        let start = ::std::time::Instant::now();
        let result = (async $block).await;
        let elapsed_ms = start.elapsed().as_millis();
        match result {
            Ok(value) => {
                $crate::timed!(@log_ok debug, $name, elapsed_ms);
                Ok(value)
            }
            Err(err) => {
                $crate::timed!(@log_err debug, $name, elapsed_ms, err);
                Err(err)
            }
        }
    }};

    (try: $lvl:ident, $name:expr, async $block:block) => {{
        let start = ::std::time::Instant::now();
        let result = (async $block).await;
        let elapsed_ms = start.elapsed().as_millis();
        match result {
            Ok(value) => {
                $crate::timed!(@log_ok $lvl, $name, elapsed_ms);
                Ok(value)
            }
            Err(err) => {
                $crate::timed!(@log_err $lvl, $name, elapsed_ms, err);
                Err(err)
            }
        }
    }};
}
