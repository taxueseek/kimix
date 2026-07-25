//! Session memory: stores conversation turns and manages memory lifecycle.
//!
//! Supports tree-branching sessions (inspired by Pi's parentId-based session tree)
//! and checkpoint snapshots (inspired by Kimi Code's checkpoint mechanism).
use chrono::{DateTime, Utc};
use kimix_core::{InvertedIndex, Tokenizer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// A single conversation turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    /// Unique turn ID.
    pub id: String,
    /// Role: "user" or "assistant".
    pub role: String,
    /// Message content.
    pub content: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Parent turn ID for tree branching (None = root of current branch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_turn_id: Option<String>,
    /// Turn number in the chain (1-indexed within current branch).
    #[serde(default)]
    pub turn_number: usize,
    /// Whether this turn is ephemeral (can be pruned).
    #[serde(default)]
    pub ephemeral: bool,
}

/// A checkpoint: snapshot of session state at a specific turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Checkpoint ID.
    pub id: String,
    /// Turn ID this checkpoint was created at.
    pub turn_id: String,
    /// Human-readable label.
    pub label: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Snapshot hash of workspace files (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_snapshot_hash: Option<String>,
}

/// A memory session tied to a conversation.
pub struct MemorySession {
    /// Session identifier.
    pub session_id: String,
    /// All turns in this session (main branch).
    turns: Vec<Turn>,
    /// Branched turns keyed by branch name.
    branches: HashMap<String, Vec<Turn>>,
    /// Checkpoints created during this session.
    checkpoints: Vec<Checkpoint>,
    /// BM25 index over turn contents.
    index: InvertedIndex,
    /// Tokenizer for indexing and search.
    tokenizer: Tokenizer,
    /// Whether the index needs a full rebuild (after compaction).
    dirty: bool,
    /// Next turn number for auto-increment.
    next_turn_number: usize,
}

impl MemorySession {
    /// Create a new session with the given ID.
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            turns: Vec::new(),
            branches: HashMap::new(),
            checkpoints: Vec::new(),
            index: InvertedIndex::new(),
            tokenizer: Tokenizer::new(2),
            dirty: false,
            next_turn_number: 1,
        }
    }

    /// Number of turns in the main branch.
    pub fn len(&self) -> usize {
        self.turns.len()
    }

    /// Whether the session is empty.
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }

    /// Get a turn by index in the main branch.
    pub fn get_turn(&self, index: usize) -> Option<&Turn> {
        self.turns.get(index)
    }

    /// Get a turn by its ID (searches main branch and all branches).
    pub fn get_turn_by_id(&self, turn_id: &str) -> Option<&Turn> {
        if let Some(t) = self.turns.iter().find(|t| t.id == turn_id) {
            return Some(t);
        }
        for turns in self.branches.values() {
            if let Some(t) = turns.iter().find(|t| t.id == turn_id) {
                return Some(t);
            }
        }
        None
    }

    /// Get the turn lineage from root to a given turn (for context injection).
    pub fn turn_lineage(&self, turn_id: &str) -> Vec<&Turn> {
        // Collect turn IDs by walking the parent chain
        let mut ids = Vec::new();
        let mut current: Option<String> = Some(turn_id.to_string());
        while let Some(ref id) = current {
            ids.push(id.clone());
            match self.get_turn_by_id(id) {
                Some(turn) => current = turn.parent_turn_id.clone(),
                None => break,
            }
        }
        // Resolve IDs to turn references in reverse (root first)
        ids.reverse();
        ids.iter()
            .filter_map(|id| self.get_turn_by_id(id))
            .collect()
    }

    /// Add a turn to the session and index its content.
    pub fn add_turn(&mut self, role: &str, content: &str) {
        let turn = Turn {
            id: uuid_v7(),
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            parent_turn_id: self.turns.last().map(|t| t.id.clone()),
            turn_number: self.next_turn_number,
            ephemeral: false,
        };
        self.next_turn_number += 1;

        let doc_id = self.turns.len();
        let tokens = self.tokenizer.tokenize(&turn.content);
        self.index.add_document(doc_id, &tokens);
        self.turns.push(turn);
    }

    /// Add a turn as a child of a specific parent turn (tree branching).
    pub fn add_turn_under(&mut self, role: &str, content: &str, parent_turn_id: &str) {
        let turn = Turn {
            id: uuid_v7(),
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            parent_turn_id: Some(parent_turn_id.to_string()),
            turn_number: self.next_turn_number,
            ephemeral: false,
        };
        self.next_turn_number += 1;

        let doc_id = self.turns.len();
        let tokens = self.tokenizer.tokenize(&turn.content);
        self.index.add_document(doc_id, &tokens);
        self.turns.push(turn);
    }

    /// Fork the session at a given turn, creating a named branch.
    /// Returns a new session ID for the fork.
    pub fn fork(&mut self, from_turn_id: &str, branch_name: &str) -> String {
        let fork_session_id = format!("{}-{}", self.session_id, branch_name);

        if let Some(idx) = self.turns.iter().position(|t| t.id == from_turn_id) {
            // Collect turns up to and including the fork point
            let branch_turns: Vec<Turn> = (0..=idx)
                .map(|i| Turn {
                    id: uuid_v7(),
                    role: self.turns[i].role.clone(),
                    content: self.turns[i].content.clone(),
                    timestamp: self.turns[i].timestamp,
                    parent_turn_id: if i == 0 {
                        None
                    } else {
                        self.turns[i].parent_turn_id.clone()
                    },
                    turn_number: i + 1,
                    ephemeral: false,
                })
                .collect();
            self.branches.insert(branch_name.to_string(), branch_turns);
        }

        fork_session_id
    }

    /// Get a branch's turns by name.
    pub fn get_branch(&self, branch_name: &str) -> Option<&Vec<Turn>> {
        self.branches.get(branch_name)
    }

    /// List all branch names.
    pub fn branch_names(&self) -> Vec<&String> {
        self.branches.keys().collect()
    }

    /// Number of branches.
    pub fn branch_count(&self) -> usize {
        self.branches.len()
    }

    // ── Checkpoint methods ──

    /// Create a checkpoint at the current turn.
    pub fn create_checkpoint(&mut self, label: &str) {
        if let Some(last_turn) = self.turns.last() {
            let cp = Checkpoint {
                id: uuid_v7(),
                turn_id: last_turn.id.clone(),
                label: label.to_string(),
                timestamp: Utc::now(),
                file_snapshot_hash: None,
            };
            self.checkpoints.push(cp);
        }
    }

    /// Create a checkpoint with a file snapshot hash.
    pub fn create_checkpoint_with_hash(&mut self, label: &str, file_hash: &str) {
        if let Some(last_turn) = self.turns.last() {
            let cp = Checkpoint {
                id: uuid_v7(),
                turn_id: last_turn.id.clone(),
                label: label.to_string(),
                timestamp: Utc::now(),
                file_snapshot_hash: Some(file_hash.to_string()),
            };
            self.checkpoints.push(cp);
        }
    }

    /// List all checkpoints.
    pub fn list_checkpoints(&self) -> &[Checkpoint] {
        &self.checkpoints
    }

    /// Get checkpoint count.
    pub fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }

    // ── Persistence helpers (for SessionStore) ──

    /// Immutable reference to main-branch turns.
    pub fn turns(&self) -> &[Turn] {
        &self.turns
    }

    /// Immutable reference to all branches.
    pub fn branches(&self) -> &std::collections::HashMap<String, Vec<Turn>> {
        &self.branches
    }

    /// Immutable reference to checkpoints.
    pub fn checkpoints(&self) -> &[Checkpoint] {
        &self.checkpoints
    }

    /// Current next turn number.
    pub fn next_turn_number(&self) -> usize {
        self.next_turn_number
    }

    /// Restore next turn number (after loading from store).
    pub fn set_next_turn_number(&mut self, n: usize) {
        self.next_turn_number = n;
    }

    /// Add a fully-constructed turn (for store loading).
    pub fn add_turn_raw(&mut self, turn: Turn) {
        let doc_id = self.turns.len();
        let tokens = self.tokenizer.tokenize(&turn.content);
        self.index.add_document(doc_id, &tokens);
        self.turns.push(turn);
    }

    /// Add a named branch of turns (for store loading).
    pub fn add_branch(&mut self, name: &str, turns: Vec<Turn>) {
        self.branches.insert(name.to_string(), turns);
    }

    /// Add a pre-constructed checkpoint (for store loading).
    pub fn add_checkpoint(&mut self, cp: Checkpoint) {
        self.checkpoints.push(cp);
    }

    /// Retrieve the turn ID of a checkpoint, for rollback.
    pub fn get_checkpoint_turn_id(&self, checkpoint_id: &str) -> Option<&str> {
        self.checkpoints
            .iter()
            .find(|cp| cp.id == checkpoint_id)
            .map(|cp| cp.turn_id.as_str())
    }

    // ── Search and context methods ──

    /// Search for turns relevant to a query.
    ///
    /// Returns (turn_index, bm25_score) pairs sorted by relevance descending.
    pub fn search(&self, query: &str, top_k: usize) -> Vec<(usize, f64)> {
        let searcher =
            kimix_core::Searcher::new(self.tokenizer.clone(), kimix_core::BM25Scorer::default());

        let results = searcher.search(query, &self.index, top_k);
        results
            .into_iter()
            .filter(|r| r.doc_id < self.turns.len())
            .map(|r| (r.doc_id, r.score))
            .collect()
    }

    /// Generate recall context for injection into a prompt.
    ///
    /// Searches for turns relevant to `query` and formats them as a
    /// context block suitable for prepending to the system prompt.
    pub fn recall_context(&self, query: &str, top_k: usize, max_chars: usize) -> String {
        let results = self.search(query, top_k);
        if results.is_empty() {
            return String::new();
        }

        let mut lines = vec!["[Auto-retrieved from past conversation]".to_string()];
        let mut total = lines[0].len();

        for (idx, score) in results {
            if idx >= self.turns.len() {
                continue;
            }
            let turn = &self.turns[idx];
            let snippet = format!(
                "[{}] (relevance: {:.2}): {}",
                turn.role, score, turn.content
            );

            if total + snippet.len() > max_chars {
                let remaining = max_chars.saturating_sub(total).saturating_sub(4);
                if remaining > 0 {
                    lines.push(format!("{}...", &snippet[..remaining.min(snippet.len())]));
                }
                break;
            }
            lines.push(snippet);
            total += lines.last().map(|s| s.len() + 1).unwrap_or(0);
        }

        lines.join("\n")
    }

    /// Generate a compaction prompt for LLM-based context summarization.
    ///
    /// Returns a prompt that can be sent to the LLM to produce a structured
    /// summary of the conversation history. After compaction, call
    /// `replace_with_summary()` with the LLM's output.
    pub fn compaction_prompt(&self, max_turns: usize) -> Option<String> {
        if self.turns.len() <= max_turns {
            return None;
        }

        let turns_to_compact = &self.turns[..self.turns.len() - max_turns];
        let context_text: String = turns_to_compact
            .iter()
            .map(|t| format!("[{}]: {}", t.role, t.content))
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompt = format!(
            "Compact the above agent conversation context according to the following priorities and rules.\n\
             \n\
             **Priorities:**\n\
             - **Current Task State** — what is being worked on right now\n\
             - **Errors & Solutions** — all errors encountered and how they were resolved\n\
             - **Code Evolution** — final working versions only (drop intermediate attempts)\n\
             - **System Context** — project structure, dependencies, environment setup\n\
             - **Design Decisions** — architectural choices and rationale\n\
             - **TODO Items** — unfinished tasks and known issues\n\
             \n\
             Rules:\n\
             - Keep the summary under 1500 tokens\n\
             - Use bullet points for clarity\n\
             - Discard thinking blocks and intermediate tool outputs\n\
             \n\
             ---\n\n\
             Context to compact:\n\n{}",
            context_text
        );

        Some(prompt)
    }

    /// Replace old turns with a summary after LLM compaction.
    ///
    /// `summary`: the LLM's structured summary.
    /// `keep_recent`: number of recent turns to keep uncompacted.
    pub fn replace_with_summary(&mut self, summary: &str, keep_recent: usize) {
        if keep_recent >= self.turns.len() {
            return;
        }

        // Keep recent turns
        let recent: Vec<Turn> = self.turns[self.turns.len() - keep_recent..].to_vec();

        // Create summary turn
        let summary_turn = Turn {
            id: uuid_v7(),
            role: "system".to_string(),
            content: format!("[Compacted history]\n{}", summary),
            timestamp: Utc::now(),
            parent_turn_id: None,
            turn_number: 0,
            ephemeral: false,
        };

        // Rebuild: summary first, then recent turns
        self.turns.clear();
        self.index.clear();
        self.turns.push(summary_turn);
        self.turns.extend(recent);

        // Re-index
        for (i, turn) in self.turns.iter().enumerate() {
            let tokens = self.tokenizer.tokenize(&turn.content);
            self.index.add_document(i, &tokens);
        }

        self.dirty = true;
    }

    /// Save session to a JSON file (including tree structure and checkpoints).
    pub fn save_to_file(&self, path: &PathBuf) -> std::io::Result<()> {
        let data = SessionData {
            session_id: self.session_id.clone(),
            turns: self.turns.clone(),
            branches: self.branches.clone(),
            checkpoints: self.checkpoints.clone(),
            next_turn_number: self.next_turn_number,
        };
        let json = serde_json::to_string_pretty(&data)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load session from a JSON file and rebuild the tree + index.
    pub fn load_from_file(path: &PathBuf) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let data: SessionData = serde_json::from_str(&json)?;

        let mut session = Self::new(data.session_id);
        session.branches = data.branches;
        session.checkpoints = data.checkpoints;
        session.next_turn_number = data.next_turn_number;

        for turn in &data.turns {
            let doc_id = session.turns.len();
            let tokens = session.tokenizer.tokenize(&turn.content);
            session.index.add_document(doc_id, &tokens);
            session.turns.push(turn.clone());
        }

        Ok(session)
    }
}

/// Serializable session data (tree-aware).
#[derive(Debug, Serialize, Deserialize)]
struct SessionData {
    session_id: String,
    turns: Vec<Turn>,
    #[serde(default)]
    branches: HashMap<String, Vec<Turn>>,
    #[serde(default)]
    checkpoints: Vec<Checkpoint>,
    #[serde(default)]
    next_turn_number: usize,
}

/// Generate a unique v7-like UUID (timestamp-based + random suffix).
fn uuid_v7() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    // Combine timestamp with an atomic counter for uniqueness within the same millisecond
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    // Mix timestamp and counter using splitmix64
    let mut x = ts.wrapping_add(counter);
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x = x ^ (x >> 31);
    format!("{:016x}-{:04x}", ts.wrapping_add(counter), (x & 0xFFFF) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_search() {
        let mut session = MemorySession::new("test-session".into());

        session.add_turn("user", "帮我写一个Python的异步HTTP客户端");
        session.add_turn("assistant", "已创建aiohttp_client.py，使用连接池和重试");
        session.add_turn("user", "加上请求超时和响应缓存");
        session.add_turn("assistant", "好的，已添加timeout=30和缓存层");

        assert_eq!(session.len(), 4);

        let results = session.search("HTTP客户端", 3);
        assert!(!results.is_empty());
        // First turn about HTTP client should be most relevant
        assert!(results[0].0 <= 2);
    }

    #[test]
    fn test_recall_context() {
        let mut session = MemorySession::new("test-recall".into());

        session.add_turn("user", "Python异步HTTP客户端的连接池配置");
        session.add_turn(
            "assistant",
            "使用aiohttp.ClientSession，max_connections=100",
        );

        let context = session.recall_context("连接池", 3, 500);
        assert!(context.contains("Auto-retrieved"));
        assert!(context.contains("连接池"));
    }

    #[test]
    fn test_compaction_prompt() {
        let mut session = MemorySession::new("test-compact".into());

        for i in 0..20 {
            session.add_turn(
                if i % 2 == 0 { "user" } else { "assistant" },
                &format!("Turn {} content with some details about the project", i),
            );
        }

        // With max_turns=10, compaction should trigger
        let prompt = session.compaction_prompt(10);
        assert!(prompt.is_some());
        assert!(prompt.unwrap().contains("Compact the above"));

        // With max_turns=30, no compaction needed
        let prompt = session.compaction_prompt(30);
        assert!(prompt.is_none());
    }

    #[test]
    fn test_save_load() {
        let mut session = MemorySession::new("test-persist".into());
        session.add_turn("user", "Hello world");
        session.add_turn("assistant", "Hi there!");

        let tmp = std::env::temp_dir().join("kimix_test_session.json");
        session.save_to_file(&tmp).unwrap();

        let loaded = MemorySession::load_from_file(&tmp).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.turns[0].content, "Hello world");

        // Search should work on loaded session
        let results = loaded.search("Hello", 3);
        assert!(!results.is_empty());

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_replace_with_summary() {
        let mut session = MemorySession::new("test-summary".into());

        for i in 0..10 {
            session.add_turn(
                if i % 2 == 0 { "user" } else { "assistant" },
                &format!("Turn {}", i),
            );
        }

        session.replace_with_summary("Project discussed HTTP client implementation.", 3);

        // Should have: 1 summary turn + 3 recent turns = 4 turns
        assert_eq!(session.len(), 4);
        assert!(session.turns[0].role == "system");
        assert!(session.turns[0].content.contains("Compacted"));

        // Search should still work
        let results = session.search("HTTP client", 3);
        assert!(!results.is_empty());
    }

    // ── Tree branching tests ──

    #[test]
    fn test_tree_parent_turn_id() {
        let mut session = MemorySession::new("test-tree".into());

        session.add_turn("user", "Root question");
        session.add_turn("assistant", "Root answer");

        // Turns should have parent_turn_id linking them
        let t0 = &session.turns[0];
        let t1 = &session.turns[1];
        assert!(t0.parent_turn_id.is_none(), "First turn has no parent");
        assert_eq!(
            t1.parent_turn_id.as_ref().unwrap(),
            &t0.id,
            "Second turn's parent is the first turn"
        );
    }

    #[test]
    fn test_fork_creates_branch() {
        let mut session = MemorySession::new("test-fork".into());

        session.add_turn("user", "How to write async code?");
        session.add_turn("assistant", "Use asyncio with event loop");
        assert_eq!(session.turns.len(), 2, "should have 2 turns before fork");
        let fork_point = session.turns[1].id.clone();

        // Fork at the assistant response
        let fork_id = session.fork(&fork_point, "alternative-approach");
        assert!(fork_id.contains("alternative-approach"));
        assert_eq!(session.branch_count(), 1, "should have 1 branch");

        let branch = session.get_branch("alternative-approach").unwrap();
        // The branch should contain turns up to the fork point (2 turns)
        // If it has only 1, there's a bug in fork()
        if branch.len() != 2 {
            panic!(
                "branch has {} turns (expected 2). Main turns: {:?}. Fork point id: {}",
                branch.len(),
                session.turns.iter().map(|t| (&t.role, &t.id)).collect::<Vec<_>>(),
                fork_point,
            );
        }
    }

    #[test]
    fn test_turn_lineage() {
        let mut session = MemorySession::new("test-lineage".into());

        session.add_turn("user", "Question 1");
        session.add_turn("assistant", "Answer 1");
        session.add_turn("user", "Question 2");

        // Verify parent chain
        let t0_id = session.turns[0].id.clone();
        let t1_id = session.turns[1].id.clone();
        let t2_id = session.turns[2].id.clone();

        assert!(session.turns[0].parent_turn_id.is_none());
        assert_eq!(session.turns[1].parent_turn_id.as_ref().unwrap(), &t0_id);
        assert_eq!(session.turns[2].parent_turn_id.as_ref().unwrap(), &t1_id);

        // Verify get_turn_by_id works
        assert!(session.get_turn_by_id(&t0_id).is_some());
        assert!(session.get_turn_by_id(&t1_id).is_some());
        assert!(session.get_turn_by_id(&t2_id).is_some());

        let lineage = session.turn_lineage(&t2_id);
        assert_eq!(lineage.len(), 3, "lineage should trace back through parent chain");
    }

    // ── Checkpoint tests ──

    #[test]
    fn test_create_checkpoint() {
        let mut session = MemorySession::new("test-checkpoint".into());

        session.add_turn("user", "Initial setup");
        session.create_checkpoint("after-setup");
        session.add_turn("assistant", "Setup complete");

        assert_eq!(session.checkpoint_count(), 1);
        let cp = &session.list_checkpoints()[0];
        assert_eq!(cp.label, "after-setup");
        assert_eq!(cp.turn_id, session.turns[0].id);
    }

    #[test]
    fn test_checkpoint_with_file_hash() {
        let mut session = MemorySession::new("test-cp-hash".into());

        session.add_turn("user", "Edit config");
        session.create_checkpoint_with_hash("config-edit", "abc123def");

        let cp = &session.list_checkpoints()[0];
        assert_eq!(cp.file_snapshot_hash.as_ref().unwrap(), "abc123def");
    }

    #[test]
    fn test_multiple_checkpoints() {
        let mut session = MemorySession::new("test-multi-cp".into());

        session.add_turn("user", "Step 1");
        session.create_checkpoint("cp-1");
        session.add_turn("assistant", "Done 1");
        session.create_checkpoint("cp-2");
        session.add_turn("user", "Step 2");

        assert_eq!(session.checkpoint_count(), 2);
        assert_eq!(session.list_checkpoints()[0].label, "cp-1");
        assert_eq!(session.list_checkpoints()[1].label, "cp-2");
    }

    #[test]
    fn test_fork_preserves_in_save_load() {
        let mut session = MemorySession::new("test-fork-persist".into());

        session.add_turn("user", "Main question");
        let fork_turn_id = session.turns[0].id.clone();
        session.add_turn("assistant", "Main answer");
        session.fork(&fork_turn_id, "explore");
        session.create_checkpoint("after-fork");

        let tmp = std::env::temp_dir().join("kimix_test_tree.json");
        session.save_to_file(&tmp).unwrap();

        let loaded = MemorySession::load_from_file(&tmp).unwrap();
        assert_eq!(loaded.branch_count(), 1);
        assert!(loaded.get_branch("explore").is_some());
        assert_eq!(loaded.checkpoint_count(), 1);

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_add_turn_under_branching() {
        let mut session = MemorySession::new("test-under".into());

        session.add_turn("user", "Root");
        let root_id = session.turns[0].id.clone();
        session.add_turn_under("user", "Branch child", &root_id);

        assert_eq!(session.len(), 2);
        let child = &session.turns[1];
        assert_eq!(child.parent_turn_id.as_ref().unwrap(), &root_id);
        assert_eq!(child.turn_number, 2);
    }
}
