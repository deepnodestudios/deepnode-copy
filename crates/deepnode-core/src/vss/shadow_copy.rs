use std::path::{Path, PathBuf};
use tracing::{info, warn};
use crate::error::{Result, TransferError};

pub struct VssManager;

/// A shadow-copy path resolved for one file. Snapshots created by DeepNode are
/// deleted by their shadow ID when this lease is dropped; pre-existing snapshots
/// are never deleted.
pub struct ResolvedShadow {
    path: PathBuf,
    cleanup_id: Option<String>,
}

impl ResolvedShadow {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ResolvedShadow {
    fn drop(&mut self) {
        if let Some(id) = self.cleanup_id.take() {
            let output = std::process::Command::new("vssadmin")
                .args(["delete", "shadows", &format!("/shadow={}", id), "/quiet"])
                .output();
            match output {
                Ok(o) if o.status.success() => {
                    info!("Deleted temporary VSS snapshot {}", id);
                }
                Ok(o) => {
                    warn!(
                        "Failed to delete temporary VSS snapshot {}: {}",
                        id,
                        String::from_utf8_lossy(&o.stderr).trim()
                    );
                }
                Err(e) => warn!("Failed to invoke vssadmin delete for {}: {}", id, e),
            }
        }
    }
}

fn parse_shadow_entries(text: &str) -> Vec<(String, Option<String>)> {
    let mut entries = Vec::new();
    let mut current_device: Option<String> = None;
    let mut current_id: Option<String> = None;

    for line in text.lines() {
        if let Some(pos) = line.find(r"\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy") {
            if let Some(device) = current_device.take() {
                entries.push((device, current_id.take()));
            }
            current_device = Some(line[pos..].trim().to_string());
        } else if let Some(pos) = line.find("Shadow Copy ID:") {
            let id = line[pos + "Shadow Copy ID:".len()..].trim().to_string();
            if !id.is_empty() {
                current_id = Some(id);
            }
        }
    }

    if let Some(device) = current_device {
        entries.push((device, current_id));
    }

    entries
}

impl VssManager {
    /// Attempts to query shadow copies specifically belonging to the given drive letter.
    pub fn get_shadow_devices_for_drive(drive_letter: char) -> Vec<String> {
        let drive_arg = format!("/for={}:", drive_letter);
        if let Ok(output) = std::process::Command::new("vssadmin")
            .args(["list", "shadows", &drive_arg])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            return parse_shadow_entries(&text)
                .into_iter()
                .map(|(device, _id)| device)
                .collect();
        }
        Vec::new()
    }

    /// Attempts to create a new shadow copy for the specified drive letter.
    /// Returns the shadow device path and its shadow-copy ID (when parseable).
    pub fn create_shadow_for_drive(drive_letter: char) -> Result<(String, Option<String>)> {
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
        parse_shadow_entries(&text)
            .into_iter()
            .last()
            .ok_or_else(|| TransferError::Vss("Failed to parse created shadow device name".to_string()))
    }

    /// Attempts to locate or create a shadow copy path for a locked file.
    /// In Windows, Volume Shadow Copies allow reading open/exclusive-locked files (e.g. Outlook PST, SQL DBs).
    pub fn resolve_shadow_path(original_path: &Path) -> Result<ResolvedShadow> {
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
                        return Ok(ResolvedShadow {
                            path: candidate,
                            cleanup_id: None,
                        });
                    }
                }

                // 2. If none exist, attempt to create a new volume snapshot.
                // The returned lease deletes it after the transfer finishes.
                match Self::create_shadow_for_drive(drive_letter) {
                    Ok((new_shadow_dev, cleanup_id)) => {
                        let shadow_file = format!("{}{}", new_shadow_dev, relative_part);
                        let candidate = PathBuf::from(&shadow_file);
                        if candidate.exists() {
                            info!("Created and verified new VSS shadow snapshot: {:?}", candidate);
                            return Ok(ResolvedShadow {
                                path: candidate,
                                cleanup_id,
                            });
                        }
                        warn!(
                            "Created VSS snapshot {} but the requested file was not present in it",
                            new_shadow_dev
                        );
                        // Delete the unusable snapshot immediately.
                        drop(ResolvedShadow {
                            path: candidate,
                            cleanup_id,
                        });
                    }
                    Err(e) => warn!("Could not create VSS snapshot for {}: {}", drive_letter, e),
                }
            }
        }

        warn!("No active or creatable VSS snapshot found for: {:?}", original_path);
        Err(TransferError::Vss(
            "VSS shadow snapshot not available or elevation required".to_string(),
        ))
    }
}
