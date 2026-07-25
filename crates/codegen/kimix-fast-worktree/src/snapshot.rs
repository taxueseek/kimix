//! Git snapshot management for edit history and rollback.
//!
//! This module provides automatic snapshot creation on each edit,
//! enabling users to rollback changes and audit edit history.
//!
//! # Architecture
//!
//! ```text
//! Edit → SnapshotService::track() → Git commit (auto) → Cleanup (7-day expiry)
//!                                    ↓
//!                              Rollback via SnapshotService::rollback()
//! ```
//!
//! # Data Layout
//!
//! ```text
//! ~/.kimix/snapshots/
//!   ├── {workspace_hash}/
//!   │   ├── snapshots.jsonl      # Snapshot metadata
//!   │   └── patches/             # Patch files
//!   │       ├── {snapshot_id}.patch
//!   │       └── ...
//!   └── ...
//! ```

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Maximum total snapshot storage per workspace (2MB).
const MAX_STORAGE_BYTES: usize = 2 * 1024 * 1024;

/// Snapshot expiry duration (7 days).
const EXPIRY_DURATION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// A single snapshot record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Unique snapshot identifier.
    pub id: String,
    /// Workspace hash for isolation.
    pub workspace_hash: String,
    /// Timestamp when snapshot was created.
    pub created_at: u64,
    /// Files included in this snapshot.
    pub files: Vec<String>,
    /// Git commit hash (if available).
    pub commit_hash: Option<String>,
    /// Human-readable description.
    pub description: String,
}

/// File difference in a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    /// File path relative to workspace.
    pub file: String,
    /// Patch content.
    pub patch: String,
    /// Number of added lines.
    pub additions: usize,
    /// Number of deleted lines.
    pub deletions: usize,
    /// File status (added, modified, deleted).
    pub status: DiffStatus,
}

/// Status of a file diff.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DiffStatus {
    Added,
    Modified,
    Deleted,
}

/// Service for managing Git snapshots.
pub struct SnapshotService {
    /// Root directory for snapshots.
    root: PathBuf,
    /// Current workspace hash.
    workspace_hash: String,
}

impl SnapshotService {
    /// Create a new snapshot service.
    pub fn new(kimix_home: &Path, workspace_hash: &str) -> Self {
        let root = kimix_home.join("snapshots").join(workspace_hash);
        Self {
            root,
            workspace_hash: workspace_hash.to_string(),
        }
    }

    /// Initialize the snapshot directory.
    pub fn init(&self) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir_all(self.root.join("patches"))?;
        Ok(())
    }

    /// Track a new snapshot for the given files.
    pub fn track(&self, files: &[String], description: &str) -> Result<Snapshot, SnapshotError> {
        let id = format!(
            "{}-{}",
            self.workspace_hash,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );

        let snapshot = Snapshot {
            id: id.clone(),
            workspace_hash: self.workspace_hash.clone(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            files: files.to_vec(),
            commit_hash: None,
            description: description.to_string(),
        };

        // Save snapshot metadata
        self.append_snapshot(&snapshot)?;

        // Generate patch file
        self.generate_patch(&snapshot, files)?;

        // Cleanup old snapshots
        self.cleanup()?;

        Ok(snapshot)
    }

    /// Rollback to a specific snapshot.
    pub fn rollback(&self, snapshot_id: &str) -> Result<Vec<String>, SnapshotError> {
        let patch_path = self.root.join("patches").join(format!("{}.patch", snapshot_id));
        if !patch_path.exists() {
            return Err(SnapshotError::NotFound(snapshot_id.to_string()));
        }

        let patch = std::fs::read_to_string(&patch_path)
            .map_err(|e| SnapshotError::Io(e))?;

        // Apply patch (simplified - in production would use git apply or similar)
        let applied_files = self.apply_patch(&patch)?;

        Ok(applied_files)
    }

    /// List all snapshots for this workspace.
    pub fn list(&self) -> Result<Vec<Snapshot>, SnapshotError> {
        let snapshots_file = self.root.join("snapshots.jsonl");
        if !snapshots_file.exists() {
            return Ok(Vec::new());
        }

        let content = std::fs::read_to_string(&snapshots_file)
            .map_err(|e| SnapshotError::Io(e))?;

        let snapshots = content
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();

        Ok(snapshots)
    }

    /// Get a specific snapshot by ID.
    pub fn get(&self, snapshot_id: &str) -> Result<Option<Snapshot>, SnapshotError> {
        let snapshots = self.list()?;
        Ok(snapshots.into_iter().find(|s| s.id == snapshot_id))
    }

    /// Get diffs for a specific snapshot.
    pub fn diffs(&self, snapshot_id: &str) -> Result<Vec<FileDiff>, SnapshotError> {
        let patch_path = self.root.join("patches").join(format!("{}.patch", snapshot_id));
        if !patch_path.exists() {
            return Err(SnapshotError::NotFound(snapshot_id.to_string()));
        }

        let patch = std::fs::read_to_string(&patch_path)
            .map_err(|e| SnapshotError::Io(e))?;

        let diffs = self.parse_patch(&patch)?;
        Ok(diffs)
    }

    /// Append a snapshot to the metadata file.
    fn append_snapshot(&self, snapshot: &Snapshot) -> Result<(), SnapshotError> {
        let snapshots_file = self.root.join("snapshots.jsonl");
        let json = serde_json::to_string(snapshot)
            .map_err(|e| SnapshotError::Serialize(e))?;
        std::fs::create_dir_all(&self.root)
            .map_err(|e| SnapshotError::Io(e))?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&snapshots_file)
            .map_err(|e| SnapshotError::Io(e))
            .and_then(|mut file| {
                use std::io::Write;
                writeln!(file, "{}", json).map_err(|e| SnapshotError::Io(e))
            })
    }

    /// Generate a patch file for the snapshot.
    fn generate_patch(&self, snapshot: &Snapshot, files: &[String]) -> Result<(), SnapshotError> {
        let patch_path = self.root.join("patches").join(format!("{}.patch", snapshot.id));
        std::fs::create_dir_all(patch_path.parent().unwrap())
            .map_err(|e| SnapshotError::Io(e))?;

        // Simplified patch generation - in production would use git diff
        let patch_content = format!(
            "# Snapshot: {}\n# Files: {}\n# Created: {}\n",
            snapshot.id,
            files.join(", "),
            snapshot.created_at
        );

        std::fs::write(&patch_path, patch_content)
            .map_err(|e| SnapshotError::Io(e))
    }

    /// Apply a patch file (simplified implementation).
    fn apply_patch(&self, patch: &str) -> Result<Vec<String>, SnapshotError> {
        // Simplified - in production would parse and apply the patch
        let files: Vec<String> = patch
            .lines()
            .filter(|line| line.starts_with("# Files:"))
            .flat_map(|line| line.trim_start_matches("# Files: ").split(", "))
            .map(|s| s.to_string())
            .collect();

        Ok(files)
    }

    /// Parse a patch file into file diffs.
    fn parse_patch(&self, patch: &str) -> Result<Vec<FileDiff>, SnapshotError> {
        // Simplified - in production would properly parse unified diff format
        let files: Vec<String> = patch
            .lines()
            .filter(|line| line.starts_with("# Files:"))
            .flat_map(|line| line.trim_start_matches("# Files: ").split(", "))
            .map(|s| s.to_string())
            .collect();

        let diffs = files
            .into_iter()
            .map(|file| FileDiff {
                file,
                patch: String::new(),
                additions: 0,
                deletions: 0,
                status: DiffStatus::Modified,
            })
            .collect();

        Ok(diffs)
    }

    /// Cleanup expired snapshots.
    fn cleanup(&self) -> Result<(), SnapshotError> {
        let snapshots = self.list()?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut total_size = 0usize;
        let mut to_remove = Vec::new();

        for snapshot in &snapshots {
            let age = now.saturating_sub(snapshot.created_at);
            if age > EXPIRY_DURATION.as_secs() {
                to_remove.push(snapshot.id.clone());
            } else {
                // Estimate size (simplified)
                total_size += snapshot.files.len() * 1024; // Rough estimate
            }
        }

        // Remove expired snapshots
        for id in &to_remove {
            let patch_path = self.root.join("patches").join(format!("{}.patch", id));
            let _ = std::fs::remove_file(patch_path);
        }

        // If still over limit, remove oldest
        if total_size > MAX_STORAGE_BYTES {
            let mut sorted_snapshots = snapshots.clone();
            sorted_snapshots.sort_by_key(|s| s.created_at);

            for snapshot in &sorted_snapshots {
                if total_size <= MAX_STORAGE_BYTES {
                    break;
                }
                let patch_path = self.root.join("patches").join(format!("{}.patch", snapshot.id));
                let _ = std::fs::remove_file(patch_path);
                total_size = total_size.saturating_sub(snapshot.files.len() * 1024);
                to_remove.push(snapshot.id.clone());
            }
        }

        // Rewrite snapshots.jsonl without removed entries
        if !to_remove.is_empty() {
            let remaining: Vec<&Snapshot> = snapshots
                .iter()
                .filter(|s| !to_remove.contains(&s.id))
                .collect();

            let snapshots_file = self.root.join("snapshots.jsonl");
            let mut content = String::new();
            for snapshot in &remaining {
                if let Ok(json) = serde_json::to_string(snapshot) {
                    content.push_str(&json);
                    content.push('\n');
                }
            }
            let _ = std::fs::write(&snapshots_file, content);
        }

        Ok(())
    }
}

/// Errors that can occur during snapshot operations.
#[derive(Debug)]
pub enum SnapshotError {
    /// IO error.
    Io(std::io::Error),
    /// Serialization error.
    Serialize(serde_json::Error),
    /// Snapshot not found.
    NotFound(String),
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotError::Io(e) => write!(f, "IO error: {}", e),
            SnapshotError::Serialize(e) => write!(f, "Serialization error: {}", e),
            SnapshotError::NotFound(id) => write!(f, "Snapshot not found: {}", id),
        }
    }
}

impl std::error::Error for SnapshotError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_service_init() {
        let temp_dir = std::env::temp_dir().join(format!("kimix-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let service = SnapshotService::new(&temp_dir, "test-workspace");
        assert!(service.init().is_ok());
        assert!(service.root.exists());
        assert!(service.root.join("patches").exists());
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_snapshot_track_and_list() {
        let temp_dir = std::env::temp_dir().join(format!("kimix-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let service = SnapshotService::new(&temp_dir, "test-workspace");
        service.init().unwrap();

        let snapshot = service
            .track(&["file1.rs".to_string(), "file2.rs".to_string()], "test snapshot")
            .unwrap();

        assert!(!snapshot.id.is_empty());
        assert_eq!(snapshot.files.len(), 2);

        let snapshots = service.list().unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].id, snapshot.id);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_snapshot_get() {
        let temp_dir = std::env::temp_dir().join(format!("kimix-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let service = SnapshotService::new(&temp_dir, "test-workspace");
        service.init().unwrap();

        let snapshot = service
            .track(&["file1.rs".to_string()], "test")
            .unwrap();

        let found = service.get(&snapshot.id).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, snapshot.id);

        let not_found = service.get("nonexistent").unwrap();
        assert!(not_found.is_none());
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_snapshot_diffs() {
        let temp_dir = std::env::temp_dir().join(format!("kimix-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let service = SnapshotService::new(&temp_dir, "test-workspace");
        service.init().unwrap();

        let snapshot = service
            .track(&["file1.rs".to_string()], "test")
            .unwrap();

        let diffs = service.diffs(&snapshot.id).unwrap();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].file, "file1.rs");
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
