use std::path::{Path, PathBuf};
use tracing::{info, warn};
use crate::error::{Result, TransferError};

pub struct VssManager;

impl VssManager {
    /// Attempts to query shadow copies specifically belonging to the given drive letter.
    pub fn get_shadow_devices_for_drive(drive_letter: char) -> Vec<String> {
        let drive_arg = format!("/for={}:", drive_letter);
        let mut devices = Vec::new();
        if let Ok(output) = std::process::Command::new("vssadmin")
            .args(["list", "shadows", &drive_arg])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                if let Some(pos) = line.find(r"\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy") {
                    let dev = line[pos..].trim();
                    devices.push(dev.to_string());
                }
            }
        }
        devices
    }

    /// Attempts to create a new shadow copy for the specified drive letter.
    pub fn create_shadow_for_drive(drive_letter: char) -> Result<String> {
        let drive_arg = format!("/for={}:", drive_letter);
        let output = std::process::Command::new("vssadmin")
            .args(["create", "shadow", &drive_arg])
            .output()
            .map_err(|e| TransferError::Vss(format!("Failed to invoke vssadmin: {}", e)))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(TransferError::Vss(format!("vssadmin create shadow failed: {}", err.trim())));
        }

        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if let Some(pos) = line.find(r"\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy") {
                let dev = line[pos..].trim();
                return Ok(dev.to_string());
            }
        }

        Err(TransferError::Vss("Failed to parse created shadow device name".to_string()))
    }

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

                // 1. Check existing shadow devices verified to belong to THIS volume
                let existing_devices = Self::get_shadow_devices_for_drive(drive_letter);
                for shadow_dev in existing_devices {
                    let shadow_file = format!("{}{}", shadow_dev, relative_part);
                    let candidate = PathBuf::from(&shadow_file);
                    if candidate.exists() {
                        info!("Found existing verified VSS shadow copy for locked file: {:?}", candidate);
                        return Ok(candidate);
                    }
                }

                // 2. If none exist, attempt to create a new volume snapshot
                if let Ok(new_shadow_dev) = Self::create_shadow_for_drive(drive_letter) {
                    let shadow_file = format!("{}{}", new_shadow_dev, relative_part);
                    let candidate = PathBuf::from(&shadow_file);
                    if candidate.exists() {
                        info!("Created and verified new VSS shadow snapshot: {:?}", candidate);
                        return Ok(candidate);
                    }
                }
            }
        }

        warn!("No active or creatable VSS snapshot found for: {:?}", original_path);
        Err(TransferError::Vss(
            "VSS shadow snapshot not available or elevation required".to_string(),
        ))
    }
}
