use std::path::Path;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type DbResult<T> = std::result::Result<T, DbError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: Option<i64>,
    pub session_uuid: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub mode: String,
    pub source_summary: String,
    pub destination: String,
    pub total_files: usize,
    pub completed_files: usize,
    pub skipped_files: usize,
    pub failed_files: usize,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub average_speed_bps: f64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub id: Option<i64>,
    pub session_id: i64,
    pub src_path: String,
    pub dst_path: String,
    pub size: u64,
    pub status: String,
    pub src_hash: Option<String>,
    pub dst_hash: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: u32,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    fn configure_connection(conn: &Connection) -> DbResult<()> {
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
            "#,
        )?;
        Ok(())
    }

    pub fn open<P: AsRef<Path>>(path: P) -> DbResult<Self> {
        let conn = Connection::open(path)?;
        Self::configure_connection(&conn)?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    pub fn open_in_memory() -> DbResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::configure_connection(&conn)?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> DbResult<()> {
        let user_version: u32 = self
            .conn
            .query_row("PRAGMA user_version;", [], |r| r.get(0))?;

        if user_version == 0 {
            self.conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS transfer_sessions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    session_uuid TEXT NOT NULL UNIQUE,
                    start_time TEXT NOT NULL,
                    end_time TEXT,
                    mode TEXT NOT NULL,
                    source_summary TEXT NOT NULL DEFAULT '',
                    destination TEXT NOT NULL DEFAULT '',
                    total_files INTEGER NOT NULL,
                    completed_files INTEGER NOT NULL,
                    skipped_files INTEGER NOT NULL,
                    failed_files INTEGER NOT NULL,
                    total_bytes INTEGER NOT NULL,
                    transferred_bytes INTEGER NOT NULL,
                    average_speed_bps REAL NOT NULL DEFAULT 0,
                    status TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS transfer_files (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    session_id INTEGER NOT NULL,
                    src_path TEXT NOT NULL,
                    dst_path TEXT NOT NULL,
                    size INTEGER NOT NULL,
                    status TEXT NOT NULL,
                    src_hash TEXT,
                    dst_hash TEXT,
                    error_message TEXT,
                    retry_count INTEGER NOT NULL DEFAULT 0,
                    FOREIGN KEY (session_id) REFERENCES transfer_sessions(id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_transfer_files_session ON transfer_files(session_id);
                CREATE INDEX IF NOT EXISTS idx_transfer_sessions_start ON transfer_sessions(start_time DESC);

                PRAGMA user_version = 2;
                "#,
            )?;
        } else if user_version < 2 {
            // Check table info before adding missing columns
            let mut stmt = self.conn.prepare("PRAGMA table_info(transfer_sessions)")?;
            let existing_cols: Vec<String> = stmt
                .query_map([], |row| row.get::<_, String>(1))?
                .filter_map(|r| r.ok())
                .collect();

            if !existing_cols.contains(&"source_summary".to_string()) {
                self.conn.execute(
                    "ALTER TABLE transfer_sessions ADD COLUMN source_summary TEXT NOT NULL DEFAULT ''",
                    [],
                )?;
            }
            if !existing_cols.contains(&"destination".to_string()) {
                self.conn.execute(
                    "ALTER TABLE transfer_sessions ADD COLUMN destination TEXT NOT NULL DEFAULT ''",
                    [],
                )?;
            }
            if !existing_cols.contains(&"average_speed_bps".to_string()) {
                self.conn.execute(
                    "ALTER TABLE transfer_sessions ADD COLUMN average_speed_bps REAL NOT NULL DEFAULT 0",
                    [],
                )?;
            }

            self.conn.execute("PRAGMA user_version = 2;", [])?;
        }

        // Heal any orphaned in_progress sessions from previous abnormal process termination
        self.conn.execute(
            "UPDATE transfer_sessions SET status = 'interrupted' WHERE status = 'in_progress'",
            [],
        )?;

        Ok(())
    }

    pub fn insert_session(&self, session: &SessionRecord) -> DbResult<i64> {
        self.conn.execute(
            r#"
            INSERT INTO transfer_sessions (
                session_uuid, start_time, end_time, mode,
                source_summary, destination,
                total_files, completed_files, skipped_files, failed_files,
                total_bytes, transferred_bytes, average_speed_bps, status
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            "#,
            params![
                session.session_uuid,
                session.start_time.to_rfc3339(),
                session.end_time.map(|t| t.to_rfc3339()),
                session.mode,
                session.source_summary,
                session.destination,
                session.total_files as i64,
                session.completed_files as i64,
                session.skipped_files as i64,
                session.failed_files as i64,
                session.total_bytes as i64,
                session.transferred_bytes as i64,
                session.average_speed_bps,
                session.status
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_session_finish(
        &self,
        session_id: i64,
        status: &str,
        completed: usize,
        skipped: usize,
        failed: usize,
        transferred_bytes: u64,
        average_speed_bps: f64,
    ) -> DbResult<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            r#"
            UPDATE transfer_sessions SET
                end_time = ?1,
                status = ?2,
                completed_files = ?3,
                skipped_files = ?4,
                failed_files = ?5,
                transferred_bytes = ?6,
                average_speed_bps = ?7
            WHERE id = ?8
            "#,
            params![
                now,
                status,
                completed as i64,
                skipped as i64,
                failed as i64,
                transferred_bytes as i64,
                average_speed_bps,
                session_id
            ],
        )?;
        let _ = self.enforce_retention_policy(1000, Some(90));
        Ok(())
    }

    /// Enforces database retention policy by keeping up to `max_sessions` recent sessions
    /// and optionally removing sessions older than `max_age_days`.
    /// Associated records in `transfer_files` are atomically removed.
    pub fn enforce_retention_policy(&self, max_sessions: usize, max_age_days: Option<u32>) -> DbResult<usize> {
        let tx = self.conn.unchecked_transaction()?;
        let mut total_deleted = 0;

        // 1. Delete sessions older than max_age_days if specified
        if let Some(days) = max_age_days {
            let cutoff = (Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();
            tx.execute(
                "DELETE FROM transfer_files WHERE session_id IN (
                    SELECT id FROM transfer_sessions WHERE start_time < ?1 AND status != 'in_progress'
                )",
                params![cutoff],
            )?;
            let deleted_by_age = tx.execute(
                "DELETE FROM transfer_sessions WHERE start_time < ?1 AND status != 'in_progress'",
                params![cutoff],
            )?;
            total_deleted += deleted_by_age;
        }

        // 2. Keep at most max_sessions recent sessions
        if max_sessions > 0 {
            tx.execute(
                "DELETE FROM transfer_files WHERE session_id IN (
                    SELECT id FROM transfer_sessions 
                    ORDER BY id DESC 
                    LIMIT -1 OFFSET ?1
                )",
                params![max_sessions as i64],
            )?;
            let deleted_by_count = tx.execute(
                "DELETE FROM transfer_sessions WHERE id IN (
                    SELECT id FROM transfer_sessions 
                    ORDER BY id DESC 
                    LIMIT -1 OFFSET ?1
                )",
                params![max_sessions as i64],
            )?;
            total_deleted += deleted_by_count;
        }

        tx.commit()?;
        Ok(total_deleted)
    }

    pub fn finalize_active_sessions_as_canceled(&self) -> DbResult<usize> {
        let now = Utc::now().to_rfc3339();
        let count = self.conn.execute(
            r#"
            UPDATE transfer_sessions
            SET status = 'canceled', end_time = ?1
            WHERE status = 'in_progress'
            "#,
            params![now],
        )?;
        Ok(count)
    }

    pub fn insert_files_batch(&mut self, session_id: i64, files: &[FileRecord]) -> DbResult<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                r#"
                INSERT INTO transfer_files (
                    session_id, src_path, dst_path, size, status,
                    src_hash, dst_hash, error_message, retry_count
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
            )?;
            for file in files {
                stmt.execute(params![
                    session_id,
                    file.src_path,
                    file.dst_path,
                    file.size as i64,
                    file.status,
                    file.src_hash,
                    file.dst_hash,
                    file.error_message,
                    file.retry_count
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_recent_sessions(
        &self,
        limit: usize,
        offset: usize,
        filter: Option<&str>,
    ) -> DbResult<Vec<SessionRecord>> {
        let safe_limit = if limit > 10_000 {
            10_000
        } else {
            limit.max(1) as i64
        };
        let safe_offset = if offset > i64::MAX as usize {
            i64::MAX
        } else {
            offset as i64
        };

        if let Some(q) = filter {
            let pattern = format!("%{}%", q.trim());
            let mut stmt = self.conn.prepare(
                r#"
                SELECT id, session_uuid, start_time, end_time, mode,
                       source_summary, destination,
                       total_files, completed_files, skipped_files, failed_files,
                       total_bytes, transferred_bytes, average_speed_bps, status
                FROM transfer_sessions
                WHERE source_summary LIKE ?1 OR destination LIKE ?2 OR status LIKE ?3
                ORDER BY id DESC
                LIMIT ?4 OFFSET ?5
                "#,
            )?;
            let rows = stmt.query_map(
                params![pattern, pattern, pattern, safe_limit, safe_offset],
                Self::map_session_row,
            )?;
            let mut results = Vec::new();
            for r in rows {
                results.push(r?);
            }
            Ok(results)
        } else {
            let mut stmt = self.conn.prepare(
                r#"
                SELECT id, session_uuid, start_time, end_time, mode,
                       source_summary, destination,
                       total_files, completed_files, skipped_files, failed_files,
                       total_bytes, transferred_bytes, average_speed_bps, status
                FROM transfer_sessions
                ORDER BY id DESC
                LIMIT ?1 OFFSET ?2
                "#,
            )?;
            let rows = stmt.query_map(params![safe_limit, safe_offset], Self::map_session_row)?;
            let mut results = Vec::new();
            for r in rows {
                results.push(r?);
            }
            Ok(results)
        }
    }

    pub fn get_session_by_id(&self, session_id: i64) -> DbResult<Option<SessionRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, session_uuid, start_time, end_time, mode,
                    source_summary, destination,
                    total_files, completed_files, skipped_files, failed_files,
                    total_bytes, transferred_bytes, average_speed_bps, status
            FROM transfer_sessions
            WHERE id = ?1
            "#,
        )?;
        let mut rows = stmt.query_map(params![session_id], Self::map_session_row)?;
        if let Some(r) = rows.next() {
            Ok(Some(r?))
        } else {
            Ok(None)
        }
    }

    fn map_session_row(row: &rusqlite::Row) -> rusqlite::Result<SessionRecord> {
        let start_str: String = row.get(2)?;
        let end_str: Option<String> = row.get(3)?;

        let start_time = DateTime::parse_from_rfc3339(&start_str)
            .map(|t| t.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let end_time = end_str.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|t| t.with_timezone(&Utc))
                .ok()
        });

        Ok(SessionRecord {
            id: Some(row.get(0)?),
            session_uuid: row.get(1)?,
            start_time,
            end_time,
            mode: row.get(4)?,
            source_summary: row.get(5).unwrap_or_default(),
            destination: row.get(6).unwrap_or_default(),
            total_files: row.get::<_, i64>(7)? as usize,
            completed_files: row.get::<_, i64>(8)? as usize,
            skipped_files: row.get::<_, i64>(9)? as usize,
            failed_files: row.get::<_, i64>(10)? as usize,
            total_bytes: row.get::<_, i64>(11)? as u64,
            transferred_bytes: row.get::<_, i64>(12)? as u64,
            average_speed_bps: row.get(13).unwrap_or(0.0),
            status: row.get(14)?,
        })
    }

    pub fn get_files_by_session(&self, session_id: i64) -> DbResult<Vec<FileRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, session_id, src_path, dst_path, size, status,
                   src_hash, dst_hash, error_message, retry_count
            FROM transfer_files
            WHERE session_id = ?1
            ORDER BY id ASC
            "#,
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok(FileRecord {
                id: Some(row.get(0)?),
                session_id: row.get(1)?,
                src_path: row.get(2)?,
                dst_path: row.get(3)?,
                size: row.get::<_, i64>(4)? as u64,
                status: row.get(5)?,
                src_hash: row.get(6)?,
                dst_hash: row.get(7)?,
                error_message: row.get(8)?,
                retry_count: row.get::<_, i64>(9)? as u32,
            })
        })?;
        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }

    pub fn delete_session(&self, session_id: i64) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM transfer_files WHERE session_id = ?1", params![session_id])?;
        tx.execute("DELETE FROM transfer_sessions WHERE id = ?1", params![session_id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn clear_all_history(&self) -> DbResult<()> {
        self.conn.execute_batch(
            r#"
            DELETE FROM transfer_files;
            DELETE FROM transfer_sessions;
            VACUUM;
            "#,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_finalize_active_sessions_as_canceled() {
        let db = Database::open_in_memory().expect("failed to open in-memory db");
        let session = SessionRecord {
            id: None,
            session_uuid: "test-uuid-1".to_string(),
            start_time: Utc::now(),
            end_time: None,
            mode: "copy".to_string(),
            source_summary: "C:\\src".to_string(),
            destination: "D:\\dst".to_string(),
            total_files: 5,
            completed_files: 2,
            skipped_files: 0,
            failed_files: 0,
            total_bytes: 1000,
            transferred_bytes: 400,
            average_speed_bps: 1000.0,
            status: "in_progress".to_string(),
        };
        let session_id = db.insert_session(&session).expect("failed to insert session");

        let updated_count = db.finalize_active_sessions_as_canceled().expect("finalize failed");
        assert_eq!(updated_count, 1);

        let sessions = db.get_recent_sessions(10, 0, None).expect("failed to get sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, Some(session_id));
        assert_eq!(sessions[0].status, "canceled");
        assert!(sessions[0].end_time.is_some());
    }

    #[test]
    fn test_init_schema_heals_in_progress_sessions() {
        let db = Database::open_in_memory().expect("failed to open in-memory db");
        let session = SessionRecord {
            id: None,
            session_uuid: "test-uuid-2".to_string(),
            start_time: Utc::now(),
            end_time: None,
            mode: "copy".to_string(),
            source_summary: "C:\\src".to_string(),
            destination: "D:\\dst".to_string(),
            total_files: 10,
            completed_files: 0,
            skipped_files: 0,
            failed_files: 0,
            total_bytes: 5000,
            transferred_bytes: 0,
            average_speed_bps: 0.0,
            status: "in_progress".to_string(),
        };
        db.insert_session(&session).expect("failed to insert session");

        // Re-run init_schema as would happen on next app boot
        db.init_schema().expect("init_schema failed");

        let sessions = db.get_recent_sessions(10, 0, None).expect("failed to get sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].status, "interrupted");
    }

    #[test]
    fn test_schema_version_and_foreign_keys_and_delete_transaction() {
        let mut db = Database::open_in_memory().expect("failed to open in-memory db");

        // Verify user_version is 2
        let version: u32 = db.conn.query_row("PRAGMA user_version;", [], |r| r.get(0)).expect("query user_version");
        assert_eq!(version, 2);

        // Verify foreign_keys is ON (1)
        let fk: i32 = db.conn.query_row("PRAGMA foreign_keys;", [], |r| r.get(0)).expect("query foreign_keys");
        assert_eq!(fk, 1);

        let session = SessionRecord {
            id: None,
            session_uuid: "test-uuid-3".to_string(),
            start_time: Utc::now(),
            end_time: None,
            mode: "copy".to_string(),
            source_summary: "C:\\src".to_string(),
            destination: "D:\\dst".to_string(),
            total_files: 1,
            completed_files: 1,
            skipped_files: 0,
            failed_files: 0,
            total_bytes: 500,
            transferred_bytes: 500,
            average_speed_bps: 500.0,
            status: "completed".to_string(),
        };
        let sid = db.insert_session(&session).expect("insert session");

        let files = vec![FileRecord {
            id: None,
            session_id: sid,
            src_path: "C:\\src\\test.dat".to_string(),
            dst_path: "D:\\dst\\test.dat".to_string(),
            size: 500,
            status: "completed".to_string(),
            src_hash: None,
            dst_hash: None,
            error_message: None,
            retry_count: 0,
        }];
        db.insert_files_batch(sid, &files).expect("insert files");

        let session_files = db.get_files_by_session(sid).expect("get files");
        assert_eq!(session_files.len(), 1);

        db.delete_session(sid).expect("delete session transaction");

        let after_sessions = db.get_recent_sessions(10, 0, None).expect("get sessions");
        assert!(after_sessions.is_empty());
        let after_files = db.get_files_by_session(sid).expect("get files");
        assert!(after_files.is_empty());
    }

    #[test]
    fn test_retention_policy_and_limit_overflow() {
        let db = Database::open_in_memory().expect("failed to open in-memory db");

        // Insert 5 sessions
        for i in 1..=5 {
            let session = SessionRecord {
                id: None,
                session_uuid: format!("session-{}", i),
                start_time: Utc::now(),
                end_time: Some(Utc::now()),
                mode: "copy".to_string(),
                source_summary: format!("C:\\src_{}", i),
                destination: "D:\\dst".to_string(),
                total_files: 1,
                completed_files: 1,
                skipped_files: 0,
                failed_files: 0,
                total_bytes: 100,
                transferred_bytes: 100,
                average_speed_bps: 100.0,
                status: "completed".to_string(),
            };
            db.insert_session(&session).expect("insert session");
        }

        let all = db.get_recent_sessions(10, 0, None).expect("get all");
        assert_eq!(all.len(), 5);

        // Test safe limit clamp (very large limit like usize::MAX won't overflow to negative)
        let large_limit = db.get_recent_sessions(usize::MAX, 0, None).expect("overflow limit");
        assert_eq!(large_limit.len(), 5);

        // Enforce retention: keep only 2 sessions
        let pruned = db.enforce_retention_policy(2, None).expect("retention policy");
        assert_eq!(pruned, 3);

        let remaining = db.get_recent_sessions(10, 0, None).expect("remaining");
        assert_eq!(remaining.len(), 2);
    }
}
