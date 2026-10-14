use std::path::{Path, PathBuf};
use tracing::{info, warn};
use crate::error::{Result, TransferError};

pub struct VssManager;

impl VssManager {
    /// Attempts to locate or create a shadow copy path for a locked file.
    /// In Windows, Volume Shadow Copies allow reading open/exclusive-locked files (e.g. Outlook PST, SQL DBs).
    pub fn resolve_shadow_path(original_path: &Path) -> Result<PathBuf> {
        let abs = std::fs::canonicalize(original_path)
            .unwrap_or_else(|_| original_path.to_path_buf());
        let str_path = abs.to_string_lossy();
        let clean = str_path.strip_prefix(r"\\?\").unwrap_or(&str_path);

        if let Some(drive_letter) = clean.chars().next() {
            if drive_letter.is_alphabetic() && clean.chars().nth(1) == Some(':') {
                let relative_part = &clean[2..]; // e.g. "\folder\locked.db"
                // Check if any shadow copy device already exists for this volume
                for idx in 1..=10 {
                    let shadow_dev = format!(r"\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy{}", idx);
                    let shadow_file = format!("{}{}", shadow_dev, relative_part);
                    let candidate = PathBuf::from(&shadow_file);
                    if candidate.exists() {
                        info!("Found existing VSS shadow copy for locked file: {:?}", candidate);
                        return Ok(candidate);
                    }
                }
            }
        }

        warn!("No active VSS snapshot found for: {:?}", original_path);
        Err(TransferError::Vss(
            "VSS shadow snapshot not available or elevation required".to_string(),
        ))
    }
}
