//! Global memory manager: multi-session recall and cross-session search.
use crate::memory_session::MemorySession;
use std::collections::HashMap;
use std::path::PathBuf;

/// Manages memory across multiple sessions.
///
/// Each session has its own BM25 index. The manager supports:
/// - Creating/loading sessions
/// - Cross-session search
/// - Auto-recall on new prompts
pub struct MemoryManager {
    /// Active sessions by ID.
    sessions: HashMap<String, MemorySession>,
    /// Base directory for persistent storage.
    storage_dir: PathBuf,
    /// Whether to persist sessions to disk.
    persist: bool,
}

impl MemoryManager {
    /// Create a new memory manager.
    pub fn new(storage_dir: PathBuf, persist: bool) -> Self {
        if persist {
            std::fs::create_dir_all(&storage_dir).ok();
        }
        Self {
            sessions: HashMap::new(),
            storage_dir,
            persist,
        }
    }

    /// Get or create a session (mutable access).
    pub fn get_session(&mut self, session_id: &str) -> &mut MemorySession {
        if !self.sessions.contains_key(session_id) {
            // Try to load from disk
            let path = self.session_path(session_id);
            let session = if self.persist && path.exists() {
                MemorySession::load_from_file(&path)
                    .unwrap_or_else(|_| MemorySession::new(session_id.to_string()))
            } else {
                MemorySession::new(session_id.to_string())
            };
            self.sessions.insert(session_id.to_string(), session);
        }
        self.sessions
            .get_mut(session_id)
            .expect("session just inserted; must exist after contains_key check")
    }

    /// Read-only access to a session (returns None if not loaded).
    pub fn get_session_readonly(&self, session_id: &str) -> Option<&MemorySession> {
        self.sessions.get(session_id)
    }

    /// Load all sessions from disk (for search). Requires &mut self for loading.
    pub fn load_all_sessions(&mut self) {
        if !self.persist {
            return;
        }
        if let Ok(entries) = std::fs::read_dir(&self.storage_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "json")
                    && let Some(stem) = path.file_stem()
                {
                    let sid = stem.to_string_lossy().to_string();
                    if let std::collections::hash_map::Entry::Vacant(e) = self.sessions.entry(sid)
                        && let Ok(session) = MemorySession::load_from_file(&path)
                    {
                        e.insert(session);
                    }
                }
            }
        }
    }

    /// Search across all sessions.
    ///
    /// Returns (session_id, turn_index, score) tuples.
    pub fn cross_session_search(&self, query: &str, top_k: usize) -> Vec<(String, usize, f64)> {
        let mut all_results = Vec::new();

        for (sid, session) in &self.sessions {
            for (turn_idx, score) in session.search(query, top_k) {
                all_results.push((sid.clone(), turn_idx, score));
            }
        }

        // Sort by score descending
        all_results.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        all_results.truncate(top_k);
        all_results
    }

    /// Save a specific session to disk.
    pub fn save_session(&self, session_id: &str) -> std::io::Result<()> {
        if let Some(session) = self.sessions.get(session_id) {
            let path = self.session_path(session_id);
            session.save_to_file(&path)?;
        }
        Ok(())
    }

    /// Save all sessions to disk.
    pub fn save_all(&self) -> std::io::Result<()> {
        if !self.persist {
            return Ok(());
        }
        for sid in self.sessions.keys() {
            self.save_session(sid)?;
        }
        Ok(())
    }

    /// Remove a session from memory and disk.
    pub fn remove_session(&mut self, session_id: &str) {
        self.sessions.remove(session_id);
        if self.persist {
            let path = self.session_path(session_id);
            std::fs::remove_file(&path).ok();
        }
    }

    /// Number of active sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    fn session_path(&self, session_id: &str) -> PathBuf {
        self.storage_dir.join(format!("{}.json", session_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cross_session_search() {
        let tmp = std::env::temp_dir().join("kimix_test_memory");
        let mut mgr = MemoryManager::new(tmp.clone(), false);

        {
            let s1 = mgr.get_session("session-1");
            s1.add_turn("user", "Python异步HTTP客户端开发");
            s1.add_turn("assistant", "创建了aiohttp客户端");
        }
        {
            let s2 = mgr.get_session("session-2");
            s2.add_turn("user", "Rust TUI应用状态栏实现");
            s2.add_turn("assistant", "使用ratatui的Paragraph组件");
        }

        let results = mgr.cross_session_search("HTTP客户端", 3);
        assert!(!results.is_empty());
        assert_eq!(results[0].0, "session-1");

        let results = mgr.cross_session_search("TUI状态栏", 3);
        assert!(!results.is_empty());
        assert_eq!(results[0].0, "session-2");
    }

    #[test]
    fn test_persist_and_load() {
        let tmp = std::env::temp_dir().join("kimix_test_persist");
        let _ = std::fs::remove_dir_all(&tmp);

        let sid = "persist-test";

        // Create and save
        {
            let mut mgr = MemoryManager::new(tmp.clone(), true);
            let session = mgr.get_session(sid);
            session.add_turn("user", "Hello world");
            session.add_turn("assistant", "Hi there");
            mgr.save_all().unwrap();
        }

        // Load in new manager
        {
            let mut mgr = MemoryManager::new(tmp.clone(), true);
            let session = mgr.get_session(sid);
            assert_eq!(session.len(), 2);

            let results = session.search("Hello", 3);
            assert!(!results.is_empty());
        }

        std::fs::remove_dir_all(&tmp).ok();
    }
}
