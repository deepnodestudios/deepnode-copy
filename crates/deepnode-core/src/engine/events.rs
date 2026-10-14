use serde::{Deserialize, Serialize};
use crate::engine::item::FileItem;
use crate::engine::stats::TransferStats;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum EngineEvent {
    TransferStarted {
        total_files: usize,
        total_bytes: u64,
        items: Vec<FileItem>,
    },
    ProgressUpdated(TransferStats),
    FileStarted(FileItem),
    FileProgress {
        id: u64,
        transferred: u64,
        total: u64,
    },
    FileCompleted(FileItem),
    FileSkipped {
        item: FileItem,
        reason: String,
    },
    FileFailed {
        item: FileItem,
        error: String,
    },
    VerificationStarted(FileItem),
    VerificationCompleted {
        item: FileItem,
        matched: bool,
    },
    TransferPaused,
    TransferResumed,
    TransferCompleted(TransferStats),
    TransferCanceled,
    TransferFailed(String),
    ConflictDetected(ConflictInfo),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictInfo {
    pub file_id: u64,
    pub filename: String,
    pub src_path: String,
    pub dst_path: String,
    pub src_size: u64,
    pub dst_size: u64,
    pub src_modified: u64,
    pub dst_modified: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictResolutionChoice {
    pub action: String, // "overwrite", "skip", "rename", "older"
    pub apply_to_all: bool,
}
