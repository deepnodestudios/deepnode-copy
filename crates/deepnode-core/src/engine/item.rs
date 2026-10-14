use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileItemStatus {
    Pending,
    Scanning,
    Copying,
    Verifying,
    Completed,
    Skipped,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileItem {
    pub id: u64,
    pub src_path: PathBuf,
    pub dst_path: PathBuf,
    pub relative_path: PathBuf,
    pub size: u64,
    pub bytes_transferred: u64,
    pub status: FileItemStatus,
    pub src_hash: Option<String>,
    pub dst_hash: Option<String>,
    pub error_message: Option<String>,
    pub retry_attempts: u32,
}

impl FileItem {
    pub fn new(id: u64, src_path: PathBuf, dst_path: PathBuf, relative_path: PathBuf, size: u64) -> Self {
        Self {
            id,
            src_path,
            dst_path,
            relative_path,
            size,
            bytes_transferred: 0,
            status: FileItemStatus::Pending,
            src_hash: None,
            dst_hash: None,
            error_message: None,
            retry_attempts: 0,
        }
    }
}
