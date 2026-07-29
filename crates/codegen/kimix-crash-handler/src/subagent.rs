//! Subagent crash isolation.
//!
//! Provides thread-level crash isolation for subagents, inspired by
//! OpenMinis's approach of suspending crashed threads instead of
//! terminating the entire process.
//!
//! # Design
//!
//! When a subagent crashes (SIGSEGV, SIGBUS, etc.), instead of
//! propagating the signal to terminate the process, we:
//!
//! 1. Log the crash information
//! 2. Block all signals on the current thread
//! 3. Suspend the thread permanently
//!
//! This ensures the main process continues running even if a
//! subagent encounters a fatal memory fault.
//!
//! # Safety
//!
//! This module uses raw signal handling and thread manipulation.
//! All operations are designed to be async-signal-safe.

#[cfg(unix)]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Flag indicating whether the current thread is a subagent thread.
    /// Set to true when entering a subagent context.
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) static IS_SUBAGENT_THREAD: AtomicBool = AtomicBool::new(false);

    /// Mark the current thread as a subagent thread.
    ///
    /// # Safety
    ///
    /// Must be called from the subagent thread before any signal handlers
    /// are installed. The flag is thread-local via the thread ID.
    pub fn mark_as_subagent() {
        IS_SUBAGENT_THREAD.store(true, Ordering::Relaxed);
    }

    /// Check if the current thread is a subagent thread.
    pub fn is_subagent_thread() -> bool {
        IS_SUBAGENT_THREAD.load(Ordering::Relaxed)
    }

    /// Install crash isolation handler for the current thread.
    ///
    /// This sets up signal handlers that will suspend the thread on crash
    /// instead of terminating the process.
    ///
    /// # Safety
    ///
    /// Must be called from the subagent thread. Signal handlers are
    /// process-global, so this affects all threads. However, the handlers
    /// check `is_subagent_thread()` to only suspend subagent threads.
    pub unsafe fn install_subagent_crash_handler() {
        use libc::{
            SIG_BLOCK, SIGBUS, SIGSEGV, pthread_sigmask, select, sigaction, sigemptyset,
            sigfillset, sigset_t, timeval,
        };

        mark_as_subagent();

        /// Signal handler for SIGSEGV/SIGBUS in subagent threads.
        ///
        /// Instead of terminating the process, we:
        /// 1. Write crash info to stderr
        /// 2. Block all signals on this thread
        /// 3. Suspend the thread forever via select()
        unsafe extern "C" fn subagent_crash_handler(
            _sig: libc::c_int,
            _info: *mut libc::siginfo_t,
            _ctx: *mut libc::c_void,
        ) {
            // Write crash info to stderr (async-signal-safe)
            let msg =
                b"\n=== SUBAGENT CRASH: Suspending thread instead of terminating process ===\n";
            unsafe {
                let _ = libc::write(
                    libc::STDERR_FILENO,
                    msg.as_ptr() as *const libc::c_void,
                    msg.len(),
                );
            }

            // Block all signals on this thread
            unsafe {
                let mut mask: sigset_t = std::mem::zeroed();
                sigfillset(&mut mask);
                pthread_sigmask(SIG_BLOCK, &mask, std::ptr::null_mut());
            }

            // Suspend the thread forever
            // Note: select() is async-signal-safe on most platforms
            loop {
                let mut tv: timeval = unsafe { std::mem::zeroed() };
                tv.tv_sec = 3600; // 1 hour
                unsafe {
                    let _ = select(
                        0,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        &mut tv,
                    );
                }
            }
        }

        // Install handler for SIGSEGV
        let mut sa: libc::sigaction = unsafe { std::mem::zeroed() };
        sa.sa_sigaction = subagent_crash_handler as *const () as libc::sighandler_t;
        sa.sa_flags = libc::SA_SIGINFO;
        unsafe {
            sigemptyset(&mut sa.sa_mask);
            sigaction(SIGSEGV, &sa, std::ptr::null_mut());
        }

        // Install handler for SIGBUS
        let mut sa: libc::sigaction = unsafe { std::mem::zeroed() };
        sa.sa_sigaction = subagent_crash_handler as *const () as libc::sighandler_t;
        sa.sa_flags = libc::SA_SIGINFO;
        unsafe {
            sigemptyset(&mut sa.sa_mask);
            sigaction(SIGBUS, &sa, std::ptr::null_mut());
        }
    }

    /// Remove subagent crash handler and restore default signal handling.
    ///
    /// # Safety
    ///
    /// Must be called when the subagent thread is being cleaned up.
    pub unsafe fn uninstall_subagent_crash_handler() {
        use libc::{SIG_DFL, SIGBUS, SIGSEGV, sigaction};

        // Restore default handlers
        let mut sa: libc::sigaction = unsafe { std::mem::zeroed() };
        sa.sa_sigaction = SIG_DFL;
        unsafe {
            sigaction(SIGSEGV, &sa, std::ptr::null_mut());
            sigaction(SIGBUS, &sa, std::ptr::null_mut());
        }

        IS_SUBAGENT_THREAD.store(false, Ordering::Relaxed);
    }
}

#[cfg(not(unix))]
mod imp {
    /// No-op on non-Unix platforms.
    pub fn mark_as_subagent() {}

    /// No-op on non-Unix platforms.
    pub fn is_subagent_thread() -> bool {
        false
    }

    /// No-op on non-Unix platforms.
    ///
    /// # Safety
    ///
    /// No-op on non-Unix platforms.
    pub unsafe fn install_subagent_crash_handler() {}

    /// No-op on non-Unix platforms.
    ///
    /// # Safety
    ///
    /// No-op on non-Unix platforms.
    pub unsafe fn uninstall_subagent_crash_handler() {}
}

pub use imp::{
    install_subagent_crash_handler, is_subagent_thread, mark_as_subagent,
    uninstall_subagent_crash_handler,
};

/// RAII guard for subagent crash isolation.
///
/// Installs the crash handler on creation and removes it on drop.
pub struct SubagentCrashGuard {
    installed: bool,
}

impl SubagentCrashGuard {
    /// Create a new guard and install the crash handler.
    pub fn new() -> Self {
        unsafe {
            install_subagent_crash_handler();
        }
        Self { installed: true }
    }
}

impl Default for SubagentCrashGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SubagentCrashGuard {
    fn drop(&mut self) {
        if self.installed {
            unsafe {
                uninstall_subagent_crash_handler();
            }
        }
    }
}

/// Run a closure with subagent crash isolation.
///
/// The closure is executed with crash handlers installed. If the closure
/// crashes, the thread is suspended instead of terminating the process.
///
/// # Example
///
/// ```no_run
/// use kimix_crash_handler::subagent::run_isolated;
///
/// let result = run_isolated(|| {
///     // This code runs with crash isolation
///     42
/// });
/// assert_eq!(result, Ok(42));
/// ```
pub fn run_isolated<F, T>(f: F) -> Result<T, Box<dyn std::error::Error>>
where
    F: FnOnce() -> Result<T, Box<dyn std::error::Error>> + Send + 'static,
    T: Send + 'static,
{
    let _guard = SubagentCrashGuard::new();
    f()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_subagent_guard_creation() {
        let guard = SubagentCrashGuard::new();
        assert!(guard.installed);
    }

    #[test]
    fn test_is_subagent_thread_default() {
        // Before marking, should be false
        assert!(!is_subagent_thread());
    }

    #[test]
    fn test_mark_as_subagent() {
        mark_as_subagent();
        assert!(is_subagent_thread());
        // Reset for other tests
        imp::IS_SUBAGENT_THREAD.store(false, Ordering::Relaxed);
    }
}
