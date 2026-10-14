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
    pub total_files: usize,
    pub completed_files: usize,
    pub skipped_files: usize,
    pub failed_files: usize,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
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
    pub fn open<P: AsRef<Path>>(path: P) -> DbResult<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    pub fn open_in_memory() -> DbResult<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> DbResult<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS transfer_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_uuid TEXT NOT NULL UNIQUE,
                start_time TEXT NOT NULL,
                end_time TEXT,
                mode TEXT NOT NULL,
                total_files INTEGER NOT NULL,
                completed_files INTEGER NOT NULL,
                skipped_files INTEGER NOT NULL,
                failed_files INTEGER NOT NULL,
                total_bytes INTEGER NOT NULL,
                transferred_bytes INTEGER NOT NULL,
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
            "#,
        )?;
        Ok(())
    }

    pub fn insert_session(&self, session: &SessionRecord) -> DbResult<i64> {
        self.conn.execute(
            r#"
            INSERT INTO transfer_sessions (
                session_uuid, start_time, end_time, mode,
                total_files, completed_files, skipped_files, failed_files,
                total_bytes, transferred_bytes, status
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                session.session_uuid,
                session.start_time.to_rfc3339(),
                session.end_time.map(|t| t.to_rfc3339()),
                session.mode,
                session.total_files as i64,
                session.completed_files as i64,
                session.skipped_files as i64,
                session.failed_files as i64,
                session.total_bytes as i64,
                session.transferred_bytes as i64,
                session.status
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_session_status(
        &self,
        session_id: i64,
        status: &str,
        completed: usize,
        skipped: usize,
        failed: usize,
        transferred_bytes: u64,
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
                transferred_bytes = ?6
            WHERE id = ?7
            "#,
            params![now, status, completed as i64, skipped as i64, failed as i64, transferred_bytes as i64, session_id],
        )?;
        Ok(())
    }

    pub fn insert_file(&self, file: &FileRecord) -> DbResult<i64> {
        self.conn.execute(
            r#"
            INSERT INTO transfer_files (
                session_id, src_path, dst_path, size, status,
                src_hash, dst_hash, error_message, retry_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                file.session_id,
                file.src_path,
                file.dst_path,
                file.size as i64,
                file.status,
                file.src_hash,
                file.dst_hash,
                file.error_message,
                file.retry_count
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_recent_sessions(&self, limit: usize) -> DbResult<Vec<SessionRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, session_uuid, start_time, end_time, mode,
                   total_files, completed_files, skipped_files, failed_files,
                   total_bytes, transferred_bytes, status
            FROM transfer_sessions
            ORDER BY id DESC
            LIMIT ?1
            "#,
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
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
                total_files: row.get::<_, i64>(5)? as usize,
                completed_files: row.get::<_, i64>(6)? as usize,
                skipped_files: row.get::<_, i64>(7)? as usize,
                failed_files: row.get::<_, i64>(8)? as usize,
                total_bytes: row.get::<_, i64>(9)? as u64,
                transferred_bytes: row.get::<_, i64>(10)? as u64,
                status: row.get(11)?,
            })
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }
}
