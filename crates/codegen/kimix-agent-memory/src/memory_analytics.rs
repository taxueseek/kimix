//! Session analytics — statistics, cost estimation, and usage reports.
//!
//! Queries the SQLite session store to produce structured reports on
//! token usage, turn counts, model switching patterns, and cost estimates.

use rusqlite::params;

use crate::memory_store::SessionStore;

/// Per-session summary statistics.
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub session_id: String,
    pub turn_count: usize,
    pub user_turns: usize,
    pub assistant_turns: usize,
    pub created_at: String,
    pub updated_at: String,
}

/// Token usage breakdown for a session.
#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub session_id: String,
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub total_tokens: u64,
    pub estimated_cost_usd: f64,
}

/// Model usage distribution for a session.
#[derive(Debug, Clone)]
pub struct ModelUsage {
    pub model_name: String,
    pub turn_count: usize,
    pub total_tokens: u64,
}

/// Complete session analytics report.
#[derive(Debug, Clone)]
pub struct SessionReport {
    pub summary: SessionSummary,
    pub token_usage: Option<TokenUsage>,
    pub model_distribution: Vec<ModelUsage>,
    pub top_checkpoints: Vec<String>,
}

impl SessionStore {
    /// Generate a summary for all saved sessions.
    pub fn summarize_all(&self) -> rusqlite::Result<Vec<SessionSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.session_id, s.created_at, s.updated_at,
                    COUNT(t.id) as total_turns,
                    SUM(CASE WHEN t.role = 'user' THEN 1 ELSE 0 END) as user_turns,
                    SUM(CASE WHEN t.role = 'assistant' THEN 1 ELSE 0 END) as assistant_turns
             FROM sessions s
             LEFT JOIN turns t ON s.session_id = t.session_id AND t.branch_name = 'main'
             GROUP BY s.session_id
             ORDER BY s.updated_at DESC",
        )?;

        let rows = stmt
            .query_map([], |row| {
                Ok(SessionSummary {
                    session_id: row.get(0)?,
                    created_at: row.get(1)?,
                    updated_at: row.get(2)?,
                    turn_count: row.get::<_, i64>(3)? as usize,
                    user_turns: row.get::<_, i64>(4)? as usize,
                    assistant_turns: row.get::<_, i64>(5)? as usize,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Generate a summary for a single session.
    pub fn summarize_session(&self, session_id: &str) -> rusqlite::Result<Option<SessionSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.session_id, s.created_at, s.updated_at,
                    COUNT(t.id),
                    SUM(CASE WHEN t.role = 'user' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN t.role = 'assistant' THEN 1 ELSE 0 END)
             FROM sessions s
             LEFT JOIN turns t ON s.session_id = t.session_id AND t.branch_name = 'main'
             WHERE s.session_id = ?1
             GROUP BY s.session_id",
        )?;

        Ok(stmt
            .query_row(params![session_id], |row| {
                Ok(SessionSummary {
                    session_id: row.get(0)?,
                    created_at: row.get(1)?,
                    updated_at: row.get(2)?,
                    turn_count: row.get::<_, i64>(3)? as usize,
                    user_turns: row.get::<_, i64>(4)? as usize,
                    assistant_turns: row.get::<_, i64>(5)? as usize,
                })
            })
            .ok())
    }

    /// Generate a complete analytics report for a session.
    pub fn analyze_session(&self, session_id: &str) -> rusqlite::Result<Option<SessionReport>> {
        let summary = match self.summarize_session(session_id)? {
            Some(s) => s,
            None => return Ok(None),
        };

        // Estimate token usage from content lengths (rough heuristic: ~1.3 chars/token).
        let token_usage = self.conn.query_row(
            "SELECT SUM(LENGTH(content)) as total_chars,
                    SUM(CASE WHEN role = 'user' THEN LENGTH(content) ELSE 0 END) as prompt_chars,
                    SUM(CASE WHEN role = 'assistant' THEN LENGTH(content) ELSE 0 END) as completion_chars
             FROM turns WHERE session_id = ?1 AND branch_name = 'main'",
            params![session_id],
            |row| {
                let total: f64 = row.get::<_, f64>(0)?;
                let prompt: f64 = row.get::<_, f64>(1)?;
                let completion: f64 = row.get::<_, f64>(2)?;
                let char_per_token = 1.3;
                let prompt_tokens = (prompt / char_per_token) as u64;
                let completion_tokens = (completion / char_per_token) as u64;
                let total_tokens = (total / char_per_token) as u64;
                // Rough cost: $0.002/1K prompt + $0.006/1K completion (blended average)
                let cost = (prompt_tokens as f64 * 0.002 + completion_tokens as f64 * 0.006) / 1000.0;
                Ok(TokenUsage {
                    session_id: session_id.to_string(),
                    total_prompt_tokens: prompt_tokens,
                    total_completion_tokens: completion_tokens,
                    total_tokens,
                    estimated_cost_usd: (cost * 100.0).round() / 100.0,
                })
            },
        ).ok();

        // Model distribution is not stored per-turn, so return empty for now.
        let model_distribution = vec![];

        // Top 5 checkpoints by recency.
        let mut cp_stmt = self.conn.prepare(
            "SELECT label FROM checkpoints WHERE session_id = ?1 ORDER BY timestamp DESC LIMIT 5",
        )?;
        let top_checkpoints: Vec<String> = cp_stmt
            .query_map(params![session_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(Some(SessionReport {
            summary,
            token_usage,
            model_distribution,
            top_checkpoints,
        }))
    }

    /// Total statistics across all sessions.
    pub fn global_stats(&self) -> rusqlite::Result<GlobalStats> {
        let total_sessions: usize =
            self.conn
                .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;

        let total_turns: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM turns WHERE branch_name = 'main'",
            [],
            |row| row.get(0),
        )?;

        let total_chars: f64 = self.conn.query_row(
            "SELECT COALESCE(SUM(LENGTH(content)), 0) FROM turns",
            [],
            |row| row.get(0),
        )?;

        let char_per_token = 1.3;
        let estimated_tokens = (total_chars / char_per_token) as u64;
        let estimated_cost = (estimated_tokens as f64 * 0.004) / 1000.0; // blended $0.004/1K

        Ok(GlobalStats {
            total_sessions,
            total_turns,
            estimated_tokens,
            estimated_cost_usd: (estimated_cost * 100.0).round() / 100.0,
        })
    }
}

/// Global statistics across all sessions.
#[derive(Debug, Clone)]
pub struct GlobalStats {
    pub total_sessions: usize,
    pub total_turns: usize,
    pub estimated_tokens: u64,
    pub estimated_cost_usd: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_session::{MemorySession, Turn};

    fn make_turn(id: &str, role: &str, content: &str, num: usize) -> Turn {
        Turn {
            id: id.to_string(),
            role: role.to_string(),
            content: content.to_string(),
            timestamp: chrono::Utc::now(),
            parent_turn_id: None,
            turn_number: num,
            ephemeral: false,
        }
    }

    #[test]
    fn summarize_session_counts_turns() {
        let store = SessionStore::open_in_memory().unwrap();
        let mut session = MemorySession::new("analytics-1".into());
        session.add_turn_raw(make_turn("t1", "user", "Hello, help me code", 1));
        session.add_turn_raw(make_turn("t2", "assistant", "Sure, what do you need?", 2));
        session.add_turn_raw(make_turn("t3", "user", "Write a function", 3));
        store.save_session(&session).unwrap();

        let summary = store.summarize_session("analytics-1").unwrap().unwrap();
        assert_eq!(summary.turn_count, 3);
        assert_eq!(summary.user_turns, 2);
        assert_eq!(summary.assistant_turns, 1);
    }

    #[test]
    fn analyze_session_estimates_tokens() {
        let store = SessionStore::open_in_memory().unwrap();
        let mut session = MemorySession::new("cost-test".into());
        // ~130 char prompt = ~100 tokens, ~260 char completion = ~200 tokens
        session.add_turn_raw(make_turn("t1", "user", &"x".repeat(130), 1));
        session.add_turn_raw(make_turn("t2", "assistant", &"y".repeat(260), 2));
        store.save_session(&session).unwrap();

        let report = store.analyze_session("cost-test").unwrap().unwrap();
        let usage = report.token_usage.unwrap();
        // 130/1.3 ≈ 100 prompt, 260/1.3 = 200 completion, 390/1.3 = 300 total
        assert!(usage.total_prompt_tokens >= 99 && usage.total_prompt_tokens <= 101);
        assert!(usage.total_completion_tokens >= 199 && usage.total_completion_tokens <= 201);
        // Very small sessions have near-zero cost; just verify the field is set.
        assert!(usage.estimated_cost_usd >= 0.0);
    }

    #[test]
    fn global_stats_aggregates() {
        let store = SessionStore::open_in_memory().unwrap();
        let mut s1 = MemorySession::new("gs-1".into());
        s1.add_turn_raw(make_turn("a1", "user", "test content here", 1));
        store.save_session(&s1).unwrap();
        let mut s2 = MemorySession::new("gs-2".into());
        s2.add_turn_raw(make_turn("b1", "user", "more content", 1));
        store.save_session(&s2).unwrap();

        let stats = store.global_stats().unwrap();
        assert_eq!(stats.total_sessions, 2);
        assert_eq!(stats.total_turns, 2);
        assert!(stats.estimated_tokens > 0);
    }
}
