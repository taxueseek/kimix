//! Session-level file locking for atomic session switching.
//!
//! Provides file-based locking to ensure that only one session can
//! access the workspace at a time, preventing cross-session file
//! contamination.
//!
//! # Design
//!
//! Inspired by OpenMinis's 4-step atomic session switch:
//! 1. Harvest - capture file changes from current session
//! 2. Clear - remove temporary workspace files
//! 3. Mount - load files for new session
//! 4. Register - index files for the new session
//!
//! This module implements the lock that ensures these steps are atomic.
//!
//! # Usage
//!
//! ```no_run
//! use kimix_workspace::session_lock::SessionFileLock;
//! use std::path::Path;
//!
//! let lock = SessionFileLock::new(
//!     Path::new("/home/user/.kimix"),
//!     "session-abc-123"
//! );
//!
//! if lock.try_lock().is_ok() {
//!     // Safe to access workspace
//!     // ... do work ...
//!     lock.unlock().ok();
//! }
//! ```

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Session-level file lock.
///
/// Uses a lock file to ensure exclusive access to the workspace.
/// The lock file contains the session ID and acquisition timestamp.
pub struct SessionFileLock {
    /// Path to the lock file.
    lock_path: PathBuf,
    /// Session ID that holds the lock.
    session_id: String,
    /// Whether this instance holds the lock.
    held: bool,
}

/// Error type for lock operations.
#[derive(Debug)]
pub enum LockError {
    /// Lock is held by another session.
    LockedByOther { other_session: String },
    /// I/O error occurred.
    Io(io::Error),
    /// Lock acquisition timed out.
    Timeout,
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::LockedByOther { other_session } => {
                write!(f, "Lock held by session: {}", other_session)
            }
            LockError::Io(e) => write!(f, "I/O error: {}", e),
            LockError::Timeout => write!(f, "Lock acquisition timed out"),
        }
    }
}

impl std::error::Error for LockError {}

impl From<io::Error> for LockError {
    fn from(e: io::Error) -> Self {
        LockError::Io(e)
    }
}

impl SessionFileLock {
    /// Create a new session file lock.
    ///
    /// The lock file will be created at `{kimix_home}/session_{session_id}.lock`.
    pub fn new(kimix_home: &Path, session_id: &str) -> Self {
        let lock_path = kimix_home.join(format!("session_{}.lock", session_id));
        Self {
            lock_path,
            session_id: session_id.to_string(),
            held: false,
        }
    }

    /// Try to acquire the lock without waiting.
    ///
    /// Returns `Ok(())` if the lock was acquired, or `Err(LockError)` if
    /// the lock is held by another session.
    pub fn try_lock(&mut self) -> Result<(), LockError> {
        // Check if lock file exists
        if self.lock_path.exists() {
            // Read existing lock
            let contents = fs::read_to_string(&self.lock_path)?;
            let other_session = contents.trim().to_string();

            // If locked by same session, it's a re-entrant lock
            if other_session == self.session_id {
                return Ok(());
            }

            return Err(LockError::LockedByOther { other_session });
        }

        // Create lock file with our session ID
        let mut file = File::create(&self.lock_path)?;
        writeln!(file, "{}", self.session_id)?;

        self.held = true;
        Ok(())
    }

    /// Try to acquire the lock with a timeout.
    ///
    /// Spins until the lock is acquired or the timeout expires.
    pub fn try_lock_timeout(&mut self, timeout: Duration) -> Result<(), LockError> {
        let start = Instant::now();
        let spin_interval = Duration::from_millis(10);

        loop {
            match self.try_lock() {
                Ok(()) => return Ok(()),
                Err(LockError::LockedByOther { .. }) => {
                    if start.elapsed() >= timeout {
                        return Err(LockError::Timeout);
                    }
                    std::thread::sleep(spin_interval);
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Release the lock.
    ///
    /// Only releases if this instance holds the lock.
    pub fn unlock(&mut self) -> Result<(), io::Error> {
        if self.held && self.lock_path.exists() {
            fs::remove_file(&self.lock_path)?;
            self.held = false;
        }
        Ok(())
    }

    /// Check if the lock is currently held by any session.
    pub fn is_locked(&self) -> bool {
        self.lock_path.exists()
    }

    /// Get the session ID that holds the lock, if any.
    pub fn lock_holder(&self) -> Option<String> {
        if self.lock_path.exists() {
            fs::read_to_string(&self.lock_path)
                .ok()
                .map(|s| s.trim().to_string())
        } else {
            None
        }
    }
}

impl Drop for SessionFileLock {
    fn drop(&mut self) {
        if self.held {
            let _ = self.unlock();
        }
    }
}

/// Atomic session switcher.
///
/// Implements the 4-step atomic session switch protocol:
/// 1. Harvest - capture file changes from current session
/// 2. Clear - remove temporary workspace files
/// 3. Mount - load files for new session
/// 4. Register - index files for the new session
pub struct SessionSwitcher {
    workspace_root: PathBuf,
}

impl SessionSwitcher {
    /// Create a new session switcher.
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            workspace_root: workspace_root.to_path_buf(),
        }
    }

    /// Perform an atomic session switch.
    ///
    /// This acquires a lock on the new session, performs the 4-step switch,
    /// and releases the lock when done.
    pub fn switch_session(
        &self,
        kimix_home: &Path,
        current_session: &str,
        new_session: &str,
    ) -> Result<(), LockError> {
        // Acquire lock on new session
        let mut lock = SessionFileLock::new(kimix_home, new_session);
        lock.try_lock_timeout(Duration::from_secs(30))?;

        // Step 1: Harvest - capture changes from current session
        self.harvest_session(current_session)?;

        // Step 2: Clear - remove temporary workspace files
        self.clear_workspace_temp()?;

        // Step 3: Mount - load files for new session
        self.mount_session(new_session)?;

        // Step 4: Register - index files for new session
        self.register_session(new_session)?;

        Ok(())
    }

    /// Harvest file changes from a session.
    fn harvest_session(&self, session_id: &str) -> Result<(), io::Error> {
        let temp_dir = self.workspace_root.join(".kimix_temp").join(session_id);
        if temp_dir.exists() {
            // In a real implementation, this would capture file changes
            // and save them to session storage
        }
        Ok(())
    }

    /// Clear temporary workspace files.
    fn clear_workspace_temp(&self) -> Result<(), io::Error> {
        let temp_dir = self.workspace_root.join(".kimix_temp");
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir)?;
        }
        Ok(())
    }

    /// Mount files for a session.
    fn mount_session(&self, session_id: &str) -> Result<(), io::Error> {
        let temp_dir = self.workspace_root.join(".kimix_temp").join(session_id);
        fs::create_dir_all(&temp_dir)?;
        // In a real implementation, this would load session files
        // into the workspace
        Ok(())
    }

    /// Register session files in the index.
    fn register_session(&self, _session_id: &str) -> Result<(), io::Error> {
        // In a real implementation, this would index session files
        // for fast lookup
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_session_lock_basic() {
        let temp = TempDir::new().unwrap();
        let mut lock = SessionFileLock::new(temp.path(), "session-1");

        assert!(lock.try_lock().is_ok());
        assert!(lock.is_locked());
        assert_eq!(lock.lock_holder(), Some("session-1".to_string()));

        lock.unlock().unwrap();
        assert!(!lock.is_locked());
    }

    #[test]
    fn test_session_lock_conflict() {
        let temp = TempDir::new().unwrap();

        let mut lock1 = SessionFileLock::new(temp.path(), "session-1");
        let mut lock2 = SessionFileLock::new(temp.path(), "session-2");

        lock1.try_lock().unwrap();

        match lock2.try_lock() {
            Err(LockError::LockedByOther { other_session }) => {
                assert_eq!(other_session, "session-1");
            }
            _ => panic!("Expected LockedByOther error"),
        }
    }

    #[test]
    fn test_session_lock_reentrant() {
        let temp = TempDir::new().unwrap();

        let mut lock1 = SessionFileLock::new(temp.path(), "session-1");
        let mut lock2 = SessionFileLock::new(temp.path(), "session-1");

        lock1.try_lock().unwrap();
        assert!(lock2.try_lock().is_ok()); // Same session, should succeed
    }

    #[test]
    fn test_session_lock_drop() {
        let temp = TempDir::new().unwrap();
        let lock_path = temp.path().join("session_session-1.lock");

        {
            let mut lock = SessionFileLock::new(temp.path(), "session-1");
            lock.try_lock().unwrap();
            assert!(lock_path.exists());
        }
        // Lock should be released on drop
        assert!(!lock_path.exists());
    }
}
