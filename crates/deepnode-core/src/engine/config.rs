use serde::{Deserialize, Serialize};
use crate::conflict::policy::ConflictPolicy;
use crate::hash::hasher::HashAlgorithm;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferMode {
    Copy,
    Move,
}

impl Default for TransferMode {
    fn default() -> Self {
        TransferMode::Copy
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifyMode {
    Never,
    Always,
    OnDifferenceOnly,
}

impl Default for VerifyMode {
    fn default() -> Self {
        VerifyMode::Always
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferConfig {
    pub mode: TransferMode,
    pub buffer_size: usize,           // e.g. 2 * 1024 * 1024 (2MB)
    pub direct_io: bool,              // FILE_FLAG_NO_BUFFERING
    pub conflict_policy: ConflictPolicy,
    pub verify_mode: VerifyMode,
    pub hash_algorithm: HashAlgorithm,
    pub preserve_timestamps: bool,
    pub preserve_attributes: bool,
    pub preserve_ads: bool,
    pub preserve_acl: bool,
    pub use_vss_for_locked: bool,
    pub retry_count: u32,             // e.g. 3 retries
    pub retry_timeout_secs: u64,      // e.g. 30 seconds
    pub export_checksum_file: bool,   // generate .hash / .md5 at target
    pub write_checksum_to_ads: bool,  // store hash into NTFS ADS stream
}

impl Default for TransferConfig {
    fn default() -> Self {
        Self {
            mode: TransferMode::Copy,
            buffer_size: 2 * 1024 * 1024, // 2MB
            direct_io: false,
            conflict_policy: ConflictPolicy::AskUser,
            verify_mode: VerifyMode::Always,
            hash_algorithm: HashAlgorithm::XxHash3_64,
            preserve_timestamps: true,
            preserve_attributes: true,
            preserve_ads: true,
            preserve_acl: false,
            use_vss_for_locked: true,
            retry_count: 3,
            retry_timeout_secs: 30,
            export_checksum_file: false,
            write_checksum_to_ads: false,
        }
    }
}
