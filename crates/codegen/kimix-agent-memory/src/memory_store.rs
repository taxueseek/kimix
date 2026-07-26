//! SQLite-backed durable session store.
//!
//! Persists [`crate::session::MemorySession`] turns, branches, and checkpoints
//! to a local SQLite database. The in-memory `MemorySession` remains the primary
//! working state; the store is write-through on mutation and read-once on load.

use rusqlite::{Connection, Result as SqlResult, params};
use std::path::Path;

use crate::memory_session::{Checkpoint, MemorySession, Turn};

/// Persistence backend for session memory.
pub struct SessionStore {
    pub(crate) conn: Connection,
}

impl SessionStore {
    /// Open (or create) the session database at `db_path`.
    pub fn open(db_path: &Path) -> SqlResult<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    /// Open an in-memory database (for testing).
    #[cfg(test)]
    pub fn open_in_memory() -> SqlResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> SqlResult<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                session_id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                next_turn_number INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE IF NOT EXISTS turns (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                parent_turn_id TEXT,
                turn_number INTEGER NOT NULL,
                branch_name TEXT NOT NULL DEFAULT 'main',
                ephemeral INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (session_id) REFERENCES sessions(session_id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS checkpoints (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                turn_id TEXT NOT NULL,
                label TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                file_snapshot_hash TEXT,
                FOREIGN KEY (session_id) REFERENCES sessions(session_id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_turns_session
                ON turns(session_id, branch_name, turn_number);
            CREATE INDEX IF NOT EXISTS idx_turns_parent ON turns(parent_turn_id);
            CREATE INDEX IF NOT EXISTS idx_checkpoints_session ON checkpoints(session_id);",
        )
    }

    // ── Write operations ─────────────────────────────────────────────────

    /// Persist a full session snapshot (upsert). Call after each mutation.
    pub fn save_session(&self, session: &MemorySession) -> SqlResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO sessions (session_id, created_at, updated_at, next_turn_number)
             VALUES (?1, ?2, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET
                updated_at = excluded.updated_at,
                next_turn_number = excluded.next_turn_number",
            params![session.session_id, now, session.next_turn_number()],
        )?;

        // Delete existing turns for this session and re-insert (simple full-sync).
        self.conn.execute(
            "DELETE FROM turns WHERE session_id = ?1 AND branch_name = 'main'",
            params![session.session_id],
        )?;
        for turn in session.turns() {
            self.insert_turn(&session.session_id, turn, "main")?;
        }

        // Branches
        for (branch_name, turns) in session.branches() {
            self.conn.execute(
                "DELETE FROM turns WHERE session_id = ?1 AND branch_name = ?2",
                params![session.session_id, branch_name],
            )?;
            for turn in turns {
                self.insert_turn(&session.session_id, turn, branch_name)?;
            }
        }

        // Checkpoints
        self.conn.execute(
            "DELETE FROM checkpoints WHERE session_id = ?1",
            params![session.session_id],
        )?;
        for cp in session.checkpoints() {
            self.conn.execute(
                "INSERT INTO checkpoints (id, session_id, turn_id, label, timestamp, file_snapshot_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    cp.id,
                    session.session_id,
                    cp.turn_id,
                    cp.label,
                    cp.timestamp.to_rfc3339(),
                    cp.file_snapshot_hash,
                ],
            )?;
        }

        Ok(())
    }

    fn insert_turn(&self, session_id: &str, turn: &Turn, branch: &str) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO turns (id, session_id, role, content, timestamp, parent_turn_id, turn_number, branch_name, ephemeral)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                turn.id,
                session_id,
                turn.role,
                turn.content,
                turn.timestamp.to_rfc3339(),
                turn.parent_turn_id,
                turn.turn_number,
                branch,
                turn.ephemeral as i32,
            ],
        )?;
        Ok(())
    }

    // ── Read operations ──────────────────────────────────────────────────

    /// Load a full session from the store. Returns `None` if not found.
    pub fn load_session(&self, session_id: &str) -> SqlResult<Option<MemorySession>> {
        let meta: Option<(String, usize)> = self
            .conn
            .query_row(
                "SELECT created_at, next_turn_number FROM sessions WHERE session_id = ?1",
                params![session_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let (_created_at, next_turn) = match meta {
            Some(m) => m,
            None => return Ok(None),
        };

        let mut session = MemorySession::new(session_id.to_string());

        // Load main-branch turns
        let mut stmt = self.conn.prepare(
            "SELECT id, role, content, timestamp, parent_turn_id, turn_number, ephemeral
             FROM turns WHERE session_id = ?1 AND branch_name = 'main'
             ORDER BY turn_number ASC",
        )?;
        let main_turns: Vec<Turn> = stmt
            .query_map(params![session_id], |row| {
                Ok(Turn {
                    id: row.get(0)?,
                    role: row.get(1)?,
                    content: row.get(2)?,
                    timestamp: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    parent_turn_id: row.get(4)?,
                    turn_number: row.get(5)?,
                    ephemeral: row.get::<_, i32>(6)? != 0,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        for turn in main_turns {
            session.add_turn_raw(turn);
        }

        // Load branches
        let mut branch_stmt = self.conn.prepare(
            "SELECT DISTINCT branch_name FROM turns WHERE session_id = ?1 AND branch_name != 'main'",
        )?;
        let branch_names: Vec<String> = branch_stmt
            .query_map(params![session_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        for branch_name in branch_names {
            let mut turns_stmt = self.conn.prepare(
                "SELECT id, role, content, timestamp, parent_turn_id, turn_number, ephemeral
                 FROM turns WHERE session_id = ?1 AND branch_name = ?2
                 ORDER BY turn_number ASC",
            )?;
            let branch_turns: Vec<Turn> = turns_stmt
                .query_map(params![session_id, branch_name], |row| {
                    Ok(Turn {
                        id: row.get(0)?,
                        role: row.get(1)?,
                        content: row.get(2)?,
                        timestamp: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .unwrap_or_else(|_| chrono::Utc::now()),
                        parent_turn_id: row.get(4)?,
                        turn_number: row.get(5)?,
                        ephemeral: row.get::<_, i32>(6)? != 0,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();

            session.add_branch(&branch_name, branch_turns);
        }

        // Load checkpoints
        let mut cp_stmt = self.conn.prepare(
            "SELECT id, turn_id, label, timestamp, file_snapshot_hash
             FROM checkpoints WHERE session_id = ?1
             ORDER BY timestamp ASC",
        )?;
        let checkpoints: Vec<Checkpoint> = cp_stmt
            .query_map(params![session_id], |row| {
                Ok(Checkpoint {
                    id: row.get(0)?,
                    turn_id: row.get(1)?,
                    label: row.get(2)?,
                    timestamp: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    file_snapshot_hash: row.get(4)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        for cp in checkpoints {
            session.add_checkpoint(cp);
        }

        // Restore next_turn_number
        session.set_next_turn_number(next_turn);

        Ok(Some(session))
    }

    /// List all saved session IDs with their last-updated timestamps.
    pub fn list_sessions(&self) -> SqlResult<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT session_id, updated_at FROM sessions ORDER BY updated_at DESC")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Delete a session and all its turns/checkpoints.
    pub fn delete_session(&self, session_id: &str) -> SqlResult<()> {
        self.conn.execute(
            "DELETE FROM sessions WHERE session_id = ?1",
            params![session_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_session::{Checkpoint, Turn};

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
    fn round_trip_session() {
        let store = SessionStore::open_in_memory().unwrap();
        let mut session = MemorySession::new("sess-1".into());

        session.add_turn_raw(make_turn("t1", "user", "hello", 1));
        session.add_turn_raw(make_turn("t2", "assistant", "hi there", 2));
        session.set_next_turn_number(3);
        session.add_checkpoint(Checkpoint {
            id: "cp1".into(),
            turn_id: "t2".into(),
            label: "After greeting".into(),
            timestamp: chrono::Utc::now(),
            file_snapshot_hash: None,
        });

        store.save_session(&session).unwrap();

        let loaded = store.load_session("sess-1").unwrap().unwrap();
        assert_eq!(loaded.session_id, "sess-1");
        assert_eq!(loaded.turns().len(), 2);
        assert_eq!(loaded.checkpoints().len(), 1);
        assert_eq!(loaded.next_turn_number(), 3);
    }

    #[test]
    fn load_nonexistent() {
        let store = SessionStore::open_in_memory().unwrap();
        assert!(store.load_session("no-such").unwrap().is_none());
    }

    #[test]
    fn list_and_delete() {
        let store = SessionStore::open_in_memory().unwrap();
        let s = MemorySession::new("s1".into());
        store.save_session(&s).unwrap();
        let s = MemorySession::new("s2".into());
        store.save_session(&s).unwrap();

        let list = store.list_sessions().unwrap();
        assert_eq!(list.len(), 2);

        store.delete_session("s1").unwrap();
        let list = store.list_sessions().unwrap();
        assert_eq!(list.len(), 1);
        assert!(store.load_session("s1").unwrap().is_none());
    }

    #[test]
    fn branches_persist() {
        let store = SessionStore::open_in_memory().unwrap();
        let mut session = MemorySession::new("branch-test".into());
        session.add_turn_raw(make_turn("t1", "user", "main turn", 1));

        let branch_turns = vec![
            make_turn("b1", "user", "branch turn 1", 1),
            make_turn("b2", "assistant", "branch turn 2", 2),
        ];
        session.add_branch("alt", branch_turns);

        store.save_session(&session).unwrap();

        let loaded = store.load_session("branch-test").unwrap().unwrap();
        assert_eq!(loaded.turns().len(), 1);
        let branches = loaded.branches();
        assert_eq!(branches.len(), 1);
        assert_eq!(branches.get("alt").unwrap().len(), 2);
    }
}
