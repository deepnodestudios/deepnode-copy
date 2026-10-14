use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum TransferError {
    #[error("I/O error: {0}")]
    Io(String),

    #[error("Win32 error (code {code}): {message}")]
    Win32 { code: u32, message: String },

    #[error("File is locked by another process (sharing violation)")]
    FileLocked,

    #[error("Integrity verification failed for '{path}': expected {expected}, actual {actual}")]
    HashMismatch {
        path: String,
        expected: String,
        actual: String,
    },

    #[error("File skipped by conflict resolution policy")]
    SkippedByPolicy,

    #[error("Transfer operation timed out after {0} seconds")]
    Timeout(u64),

    #[error("Transfer was canceled by user")]
    Canceled,

    #[error("Volume Shadow Copy (VSS) error: {0}")]
    Vss(String),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Target disk is full: required {required} bytes, available {available} bytes")]
    DiskFull { required: u64, available: u64 },
}

pub type Result<T> = std::result::Result<T, TransferError>;
