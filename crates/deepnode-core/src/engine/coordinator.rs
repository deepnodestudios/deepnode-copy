use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::conflict::policy::{ConflictPolicy, ConflictResolutionAction};
use crate::conflict::resolver::ConflictResolver;
use crate::engine::config::{TransferConfig, TransferMode, VerifyMode};
use crate::engine::events::{ConflictInfo, ConflictResolutionChoice, EngineEvent};
use crate::engine::item::{FileItem, FileItemStatus};
use crate::engine::stats::{SpeedCalculator, TransferStats};
use crate::error::{Result, TransferError};
use crate::hash::hasher::MultiHasher;
use crate::hash::verifier::HashVerifier;
use crate::io::copier::FileCopier;
use crate::metadata::acl::SecurityAcl;
use crate::metadata::ads::AdsManager;
use crate::metadata::attributes::FileAttributes;
use crate::metadata::timestamps::FileTimestamps;
use crate::vss::shadow_copy::{ResolvedShadow, VssManager};
use crate::io::win32_file::Win32FileHandle;
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{DeleteFileW, SetFileAttributesW, FILE_ATTRIBUTE_NORMAL};

fn available_space_for_path(path: &Path) -> Option<u64> {
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut probe = path.to_path_buf();
    let existing = loop {
        if probe.exists() {
            break probe;
        }
        match probe.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => probe = parent.to_path_buf(),
            _ => return None,
        }
    };

    let wide = Win32FileHandle::to_wide_path(&existing);
    let mut free_to_caller: u64 = 0;
    unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(wide.as_ptr()),
            Some(&mut free_to_caller as *mut u64),
            None,
            None,
        )
        .ok()?;
    }
    Some(free_to_caller)
}

fn delete_source_after_move(src_path: &Path) -> std::io::Result<()> {
    if !src_path.exists() {
        return Ok(());
    }
    if let Ok(metadata) = std::fs::metadata(src_path) {
        let mut perms = metadata.permissions();
        if perms.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            let _ = std::fs::set_permissions(src_path, perms);
        }
    }
    std::fs::remove_file(src_path).or_else(|_| {
        let wide = Win32FileHandle::to_wide_path(src_path);
        unsafe {
            let _ = SetFileAttributesW(PCWSTR(wide.as_ptr()), FILE_ATTRIBUTE_NORMAL);
            DeleteFileW(PCWSTR(wide.as_ptr())).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::PermissionDenied, e.to_string())
            })
        }
    })
}

pub struct TransferCoordinator {
    config: TransferConfig,
    items: Vec<FileItem>,
    event_tx: mpsc::UnboundedSender<EngineEvent>,
    pause_flag: Arc<AtomicBool>,
    cancel_flag: Arc<AtomicBool>,
    skip_flag: Arc<AtomicBool>,
    skipped_ids: Option<Arc<std::sync::Mutex<std::collections::HashSet<u64>>>>,
    conflict_hook: Option<Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<ConflictResolutionChoice>>>>>,
    destination_dir: Option<std::path::PathBuf>,
}

impl TransferCoordinator {
    pub fn new(
        config: TransferConfig,
        items: Vec<FileItem>,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
        pause_flag: Arc<AtomicBool>,
        cancel_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            config,
            items,
            event_tx,
            pause_flag,
            cancel_flag,
            skip_flag: Arc::new(AtomicBool::new(false)),
            skipped_ids: None,
            conflict_hook: None,
            destination_dir: None,
        }
    }

    pub fn set_skip_controls(
        &mut self,
        skip_flag: Arc<AtomicBool>,
        skipped_ids: Option<Arc<std::sync::Mutex<std::collections::HashSet<u64>>>>,
    ) {
        self.skip_flag = skip_flag;
        self.skipped_ids = skipped_ids;
    }

    pub fn set_conflict_hook(
        &mut self,
        hook: Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<ConflictResolutionChoice>>>>,
    ) {
        self.conflict_hook = Some(hook);
    }

    pub fn set_destination_dir(&mut self, dest: std::path::PathBuf) {
        self.destination_dir = Some(dest);
    }

    pub fn cleanup_cancelled_item(&self, dst_path: &Path) -> bool {
        crate::io::copier::cleanup_cancelled_empty_parents(
            dst_path,
            self.destination_dir.as_deref(),
        )
    }

    fn mark_remaining_canceled(&mut self, from_idx: usize) {
        for i in from_idx..self.items.len() {
            if self.items[i].status == FileItemStatus::Copying {
                self.items[i].status = FileItemStatus::Canceled;
            } else if self.items[i].status == FileItemStatus::Pending {
                self.items[i].status = FileItemStatus::Canceled;
                // Never delete Pending files - they were never written to by this transfer!
            }
        }
    }

    /// Executes the full batch transfer with smart skip & retry error recovery.
    pub async fn run(&mut self) -> Result<TransferStats> {
        let total_files = self.items.len();
        let total_bytes: u64 = self.items.iter().map(|i| i.size).sum();

        if self.destination_dir.is_none() {
            if let Some(first) = self.items.first() {
                let rel_components = first.relative_path.components().count();
                let mut inferred = first.dst_path.as_path();
                if rel_components > 0 {
                    for _ in 0..rel_components {
                        if let Some(p) = inferred.parent() {
                            inferred = p;
                        }
                    }
                } else if let Some(p) = inferred.parent() {
                    inferred = p;
                }
                self.destination_dir = Some(inferred.to_path_buf());
            }
        }

        if let Some(dest) = self.destination_dir.as_deref() {
            if let Some(available) = available_space_for_path(dest) {
                if total_bytes > available {
                    error!(
                        "Not enough free space on destination {:?}: required {}, available {}",
                        dest, total_bytes, available
                    );
                    let err = TransferError::DiskFull {
                        required: total_bytes,
                        available,
                    };
                    let _ = self.event_tx.send(EngineEvent::TransferFailed(err.to_string()));
                    return Err(err);
                }
            }
        }

        info!("TransferCoordinator::run starting: {} files, {} bytes", total_files, total_bytes);

        let _ = self.event_tx.send(EngineEvent::TransferStarted {
            total_files,
            total_bytes,
            items: self.items.clone(),
        });

        let mut stats = TransferStats {
            total_files,
            total_bytes,
            ..Default::default()
        };

        let mut speed_calc = SpeedCalculator::new(3.0);
        let copier = FileCopier::new(self.config.buffer_size, self.config.direct_io);

        let mut checksum_records: Vec<(std::path::PathBuf, String)> = Vec::new();

        for idx in 0..self.items.len() {
            if self.cancel_flag.load(Ordering::SeqCst) {
                info!("TransferCoordinator canceled before processing file idx {}", idx);
                self.cleanup_cancelled_item(&self.items[idx].dst_path);
                self.mark_remaining_canceled(idx);
                let _ = self.event_tx.send(EngineEvent::TransferCanceled);
                return Err(TransferError::Canceled);
            }

            // Handle pause state between files before touching the next file
            while self.pause_flag.load(Ordering::Relaxed) {
                if self.cancel_flag.load(Ordering::SeqCst) {
                    info!("TransferCoordinator canceled while paused before processing file idx {}", idx);
                    self.cleanup_cancelled_item(&self.items[idx].dst_path);
                    self.mark_remaining_canceled(idx);
                    let _ = self.event_tx.send(EngineEvent::TransferCanceled);
                    return Err(TransferError::Canceled);
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }

            let is_skipped = {
                let id = self.items[idx].id;
                let in_set = if let Some(ref s_ids) = self.skipped_ids {
                    s_ids.lock().unwrap_or_else(|e| e.into_inner()).contains(&id)
                } else {
                    false
                };
                in_set || self.items[idx].status == FileItemStatus::Skipped
            };

            if is_skipped {
                let mut item = self.items[idx].clone();
                item.status = FileItemStatus::Skipped;
                self.items[idx] = item.clone();
                account_pre_copy_skip(&mut stats, item.size);
                let _ = self.event_tx.send(EngineEvent::FileSkipped {
                    item: item.clone(),
                    reason: "Skipped by user".to_string(),
                });
                let _ = self.event_tx.send(EngineEvent::ProgressUpdated(stats.clone()));
                continue;
            }

            let mut item = self.items[idx].clone();

            stats.current_file_name = item
                .src_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            stats.current_file_size = item.size;
            stats.current_file_transferred = 0;

            // Conflict resolution
            let resolution = ConflictResolver::resolve(
                &item.src_path,
                &item.dst_path,
                self.config.conflict_policy,
            );

            match resolution {
                ConflictResolutionAction::SkipFile => {
                    item.status = FileItemStatus::Skipped;
                    account_pre_copy_skip(&mut stats, item.size);
                    let _ = self.event_tx.send(EngineEvent::FileSkipped {
                        item: item.clone(),
                        reason: "Skipped by conflict resolution policy".to_string(),
                    });
                    self.items[idx] = item;
                    continue;
                }
                ConflictResolutionAction::ProceedRenamed(new_path) => {
                    item.dst_path = new_path;
                }
                ConflictResolutionAction::PromptNeeded => {
                    if let Some(ref conflict_hook) = self.conflict_hook {
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        *conflict_hook.lock().await = Some(tx);

                        let src_meta = std::fs::metadata(&item.src_path).ok();
                        let dst_meta = std::fs::metadata(&item.dst_path).ok();
                        let src_size = src_meta.as_ref().map(|m| m.len()).unwrap_or(item.size);
                        let dst_size = dst_meta.as_ref().map(|m| m.len()).unwrap_or(0);
                        let src_modified = src_meta
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        let dst_modified = dst_meta
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0);

                        let filename = item
                            .src_path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();

                        let conflict_info = ConflictInfo {
                            file_id: item.id,
                            filename,
                            src_path: item.src_path.to_string_lossy().to_string(),
                            dst_path: item.dst_path.to_string_lossy().to_string(),
                            src_size,
                            dst_size,
                            src_modified,
                            dst_modified,
                        };

                        let _ = self.event_tx.send(EngineEvent::ConflictDetected(conflict_info));

                        // Wait for response from frontend, cancellation, or timeout
                        let conflict_timeout = Duration::from_secs(self.config.retry_timeout_secs.max(30));
                        let choice_res = tokio::select! {
                            res = rx => res.ok(),
                            _ = tokio::time::sleep(conflict_timeout) => {
                                warn!("Conflict resolution timed out after {:?} for file {:?}", conflict_timeout, item.dst_path);
                                Some(ConflictResolutionChoice {
                                    action: "skip".to_string(),
                                    apply_to_all: false,
                                })
                            }
                            _ = async {
                                loop {
                                    if self.cancel_flag.load(Ordering::SeqCst) {
                                        break;
                                    }
                                    tokio::time::sleep(Duration::from_millis(50)).await;
                                }
                            } => None,
                        };

                        if self.cancel_flag.load(Ordering::SeqCst) || choice_res.is_none() {
                            self.cleanup_cancelled_item(&item.dst_path);
                            item.status = FileItemStatus::Canceled;
                            self.items[idx] = item;
                            self.mark_remaining_canceled(idx + 1);
                            let _ = self.event_tx.send(EngineEvent::TransferCanceled);
                            return Err(TransferError::Canceled);
                        }

                        let choice = choice_res.unwrap();
                        if choice.apply_to_all {
                            match choice.action.as_str() {
                                "overwrite" => self.config.conflict_policy = ConflictPolicy::OverwriteAll,
                                "skip" => self.config.conflict_policy = ConflictPolicy::SkipAll,
                                "rename" => self.config.conflict_policy = ConflictPolicy::AutoRename,
                                "older" => self.config.conflict_policy = ConflictPolicy::OverwriteOlder,
                                _ => {}
                            }
                        }

                        match choice.action.as_str() {
                            "overwrite" => {
                                // Proceed overwrite
                            }
                            "skip" => {
                                item.status = FileItemStatus::Skipped;
                                account_pre_copy_skip(&mut stats, item.size);
                                let _ = self.event_tx.send(EngineEvent::FileSkipped {
                                    item: item.clone(),
                                    reason: "User skipped conflicting file".to_string(),
                                });
                                self.items[idx] = item;
                                continue;
                            }
                            "rename" => {
                                item.dst_path = ConflictResolver::generate_unique_path(&item.dst_path);
                            }
                            "older" => {
                                if src_modified > dst_modified {
                                    // Source is newer -> overwrite
                                } else {
                                    item.status = FileItemStatus::Skipped;
                                    stats.skipped_files += 1;
                                    let _ = self.event_tx.send(EngineEvent::FileSkipped {
                                        item: item.clone(),
                                        reason: "Skipped: Target file is newer or same age".to_string(),
                                    });
                                    self.items[idx] = item;
                                    continue;
                                }
                            }
                            _ => {
                                item.dst_path = ConflictResolver::generate_unique_path(&item.dst_path);
                            }
                        }
                    } else {
                        item.dst_path = ConflictResolver::generate_unique_path(&item.dst_path);
                    }
                }
                ConflictResolutionAction::ProceedOverwrite => {}
            }

            // Probe the final path: conflict resolution may have renamed the destination.
            let had_difference = match std::fs::metadata(&item.dst_path).ok() {
                Some(ref dm) => {
                    let size_diff = dm.len() != item.size;
                    let time_diff = match (
                        std::fs::metadata(&item.src_path).ok().and_then(|m| m.modified().ok()),
                        dm.modified().ok(),
                    ) {
                        (Some(st), Some(dt)) => st != dt,
                        _ => true,
                    };
                    size_diff || time_diff
                }
                None => true,
            };

            item.status = FileItemStatus::Copying;
            self.items[idx].status = FileItemStatus::Copying;

            info!("Starting file transfer [{}/{}]: {} ({} bytes)", idx + 1, total_files, stats.current_file_name, item.size);

            let _ = self.event_tx.send(EngineEvent::FileStarted(item.clone()));
            let _ = self.event_tx.send(EngineEvent::FileProgress {
                id: item.id,
                transferred: 0,
                total: item.size,
            });
            let _ = self.event_tx.send(EngineEvent::ProgressUpdated(stats.clone()));

            // Transfer single file with smart retry loop
            let copy_result = self.copy_with_retry(&mut item, &copier, &mut stats, &mut speed_calc).await;

            match copy_result {
                Err(TransferError::Canceled) => {
                    // The copier already removed its temporary file; the original destination
                    // (if any) was never touched, so only empty directories are reclaimed.
                    self.cleanup_cancelled_item(&item.dst_path);
                    item.status = FileItemStatus::Canceled;
                    self.items[idx] = item;
                    self.mark_remaining_canceled(idx + 1);
                    let _ = self.event_tx.send(EngineEvent::TransferCanceled);
                    return Err(TransferError::Canceled);
                }
                Err(TransferError::SkippedByUser) => {
                    // Same as cancellation: only the copier's temporary file is removed.
                    item.status = FileItemStatus::Skipped;
                    self.items[idx] = item.clone();
                    stats.skipped_files += 1;
                    let _ = self.event_tx.send(EngineEvent::FileSkipped {
                        item: item.clone(),
                        reason: "Skipped by user".to_string(),
                    });
                    let _ = self.event_tx.send(EngineEvent::ProgressUpdated(stats.clone()));
                    continue;
                }
                Ok((bytes_transferred, effective_src_path, shadow_lease)) => {
                    // Keep any temporary VSS snapshot alive until metadata has been
                    // read from the shadow path below; it is deleted on drop.
                    let _shadow_lease = shadow_lease;

                    // Post-copy verification
                    let should_verify = match self.config.verify_mode {
                        VerifyMode::Always => item.size > 0,
                        VerifyMode::Never => false,
                        VerifyMode::OnDifferenceOnly => item.size > 0 && had_difference,
                    };

                    if should_verify {
                        if self.cancel_flag.load(Ordering::SeqCst) {
                            // The file was already fully copied and atomically finalized.
                            // Keep it; deleting a completed file on cancel would lose data.
                            item.status = FileItemStatus::Canceled;
                            self.items[idx] = item;
                            self.mark_remaining_canceled(idx + 1);
                            let _ = self.event_tx.send(EngineEvent::TransferCanceled);
                            return Err(TransferError::Canceled);
                        }
                        let _ = self.event_tx.send(EngineEvent::VerificationStarted(item.clone()));
                        let verify_res = if let Some(ref src_h) = item.src_hash {
                            match HashVerifier::compute_file_hash(&item.dst_path, self.config.hash_algorithm, true) {
                                Ok(dst_h) => {
                                    item.dst_hash = Some(dst_h.clone());
                                    if src_h.eq_ignore_ascii_case(&dst_h) {
                                        Ok(true)
                                    } else {
                                        Err(TransferError::HashMismatch {
                                            path: item.dst_path.to_string_lossy().to_string(),
                                            expected: src_h.clone(),
                                            actual: dst_h,
                                        })
                                    }
                                }
                                Err(e) => Err(e),
                            }
                        } else {
                            HashVerifier::verify_files(
                                &effective_src_path,
                                &item.dst_path,
                                self.config.hash_algorithm,
                            )
                        };

                        match verify_res {
                            Ok(_) => {
                                let _ = self.event_tx.send(EngineEvent::VerificationCompleted {
                                     item: item.clone(),
                                     matched: true,
                                 });
                            }
                            Err(e) => {
                                error!("Verification failed for {:?}: {}", item.dst_path, e);
                                item.status = FileItemStatus::Failed;
                                item.error_message = Some(e.to_string());
                                stats.failed_files += 1;
                                let _ = self.event_tx.send(EngineEvent::VerificationCompleted {
                                    item: item.clone(),
                                    matched: false,
                                });
                                let _ = self.event_tx.send(EngineEvent::FileFailed {
                                    item: item.clone(),
                                    error: e.to_string(),
                                });
                                self.items[idx] = item;
                                continue;
                            }
                        }
                    }

                    // Check if skip was requested during copy or verification
                    if self.skip_flag.load(Ordering::SeqCst) {
                        self.skip_flag.store(false, Ordering::SeqCst);
                        // The destination is a fully finalized copy; keep it instead of deleting data.
                        item.status = FileItemStatus::Skipped;
                        stats.skipped_files += 1;
                        let _ = self.event_tx.send(EngineEvent::FileSkipped {
                            item: item.clone(),
                            reason: "Skipped by user during verification".to_string(),
                        });
                        self.items[idx] = item;
                        continue;
                    }

                    // Check cancellation immediately after copy/verify
                    if self.cancel_flag.load(Ordering::SeqCst) {
                        // Keep the already-finalized destination file.
                        item.status = FileItemStatus::Canceled;
                        self.items[idx] = item;
                        self.mark_remaining_canceled(idx + 1);
                        let _ = self.event_tx.send(EngineEvent::TransferCanceled);
                        return Err(TransferError::Canceled);
                    }

                    // Metadata preservation (read from shadow copy if VSS was used to bypass locks)
                    let meta_src = if effective_src_path != item.src_path {
                        &effective_src_path
                    } else {
                        &item.src_path
                    };

                    let mut meta_errors: Vec<String> = Vec::new();

                    // 1. Alternate Data Streams (ADS) - must be written BEFORE timestamps
                    // because writing/modifying any ADS stream automatically updates LastWriteTime on NTFS!
                    if self.config.preserve_ads {
                        if let Err(e) = AdsManager::copy_alternate_streams(meta_src, &item.dst_path) {
                            warn!("Failed to copy ADS streams for {:?}: {}", item.dst_path, e);
                            meta_errors.push(format!("ADS kopyalanamadı: {}", e));
                        }
                    }

                    // 2. NTFS ADS checksum writing - must also occur BEFORE timestamps
                    if self.config.write_checksum_to_ads {
                        if let Some(ref h) = item.src_hash {
                            if let Err(e) = HashVerifier::write_hash_to_ads(
                                &item.dst_path,
                                self.config.hash_algorithm,
                                h,
                            ) {
                                warn!("Failed to write checksum to ADS for {:?}: {}", item.dst_path, e);
                                meta_errors.push(format!("ADS sağlama yazılamadı: {}", e));
                            }
                        }
                    }

                    // 3. Security ACLs
                    if self.config.preserve_acl {
                        if let Err(e) = SecurityAcl::copy_dacl(meta_src, &item.dst_path) {
                            warn!("Failed to copy security ACL for {:?}: {}", item.dst_path, e);
                            meta_errors.push(format!("ACL kopyalanamadı: {}", e));
                        }
                    }

                    // 4. Timestamps - applied AFTER all file and ADS stream writes have finished
                    if self.config.preserve_timestamps {
                        if let Err(e) = FileTimestamps::copy_between_paths(meta_src, &item.dst_path) {
                            warn!("Failed to copy timestamps for {:?}: {}", item.dst_path, e);
                            meta_errors.push(format!("Zaman damgası kopyalanamadı: {}", e));
                        }
                    }

                    // 5. Attributes (READONLY, HIDDEN, SYSTEM) - MUST BE APPLIED LAST
                    // so that setting READONLY does not block any previous stream/timestamp write operations
                    if self.config.preserve_attributes {
                        if let Err(e) = FileAttributes::copy_attributes(meta_src, &item.dst_path) {
                            warn!("Failed to copy file attributes for {:?}: {}", item.dst_path, e);
                            meta_errors.push(format!("Öznitelikler kopyalanamadı: {}", e));
                        }
                    }

                    if !meta_errors.is_empty() && item.error_message.is_none() {
                        item.error_message = Some(meta_errors.join("; "));
                    }

                    // Collect checksum manifest records
                    if self.config.export_checksum_file {
                        if let Some(ref h) = item.src_hash {
                            checksum_records.push((item.dst_path.clone(), h.clone()));
                        }
                    }

                    // If Move mode, verify destination integrity and delete source file
                    if self.config.mode == TransferMode::Move {
                        // Never remove the source when metadata could not be preserved: doing so
                        // would permanently lose ACLs, streams, timestamps or attributes.
                        if !meta_errors.is_empty() {
                            error!("Move aborted source deletion because metadata copy failed for {:?}", item.dst_path);
                            item.status = FileItemStatus::Failed;
                            item.error_message = Some(format!(
                                "Kopyalandı ancak meta veriler korunamadığı için kaynak silinmedi: {}",
                                meta_errors.join("; ")
                            ));
                            stats.failed_files += 1;
                            let _ = self.event_tx.send(EngineEvent::FileFailed {
                                item: item.clone(),
                                error: item.error_message.clone().unwrap(),
                            });
                            self.items[idx] = item;
                            continue;
                        }

                        let dst_exists = item.dst_path.exists();
                        let dst_size = std::fs::metadata(&item.dst_path).map(|m| m.len()).unwrap_or(0);
                        if !dst_exists || (item.size != 0 && dst_size != item.size) {
                            error!("Move failed: destination file missing or truncated ({:?})", item.dst_path);
                            item.status = FileItemStatus::Failed;
                            item.error_message = Some("Destination file missing or size mismatch after move".to_string());
                            stats.failed_files += 1;
                            let _ = self.event_tx.send(EngineEvent::FileFailed {
                                item: item.clone(),
                                error: item.error_message.clone().unwrap(),
                            });
                            self.items[idx] = item;
                            continue;
                        }

                        if let Err(e) = delete_source_after_move(&item.src_path) {
                            warn!("Failed to delete source file after move: {:?}: {}", item.src_path, e);
                            item.status = FileItemStatus::Failed;
                            item.error_message = Some(format!(
                                "Copied to destination but failed to delete source: {}",
                                e
                            ));
                            stats.failed_files += 1;
                            let _ = self.event_tx.send(EngineEvent::FileFailed {
                                item: item.clone(),
                                error: item.error_message.clone().unwrap(),
                            });
                            self.items[idx] = item;
                            continue;
                        }
                    }

                    item.status = FileItemStatus::Completed;
                    stats.completed_files += 1;
                    info!("File transfer completed: {} (transferred {} bytes)", item.src_path.display(), bytes_transferred);
                    let _ = self.event_tx.send(EngineEvent::FileCompleted(item.clone()));
                }
                Err(err) => {
                    warn!("File transfer failed for {:?}: {}", item.src_path, err);
                    item.status = FileItemStatus::Failed;
                    item.error_message = Some(err.to_string());
                    stats.failed_files += 1;
                    let _ = self.event_tx.send(EngineEvent::FileFailed {
                        item: item.clone(),
                        error: err.to_string(),
                    });
                }
            }

            self.items[idx] = item;

            // Update overall progress percentage (total_bytes shrinks when queued files are removed)
            if stats.total_bytes > 0 {
                stats.overall_percentage = (stats.transferred_bytes as f32 / stats.total_bytes as f32) * 100.0;
            }
            let _ = self.event_tx.send(EngineEvent::ProgressUpdated(stats.clone()));
        }

        // Export checksum manifest file if requested
        if self.config.export_checksum_file && !checksum_records.is_empty() {
            if let Some(first_dst) = checksum_records.first().map(|r| &r.0) {
                if let Some(target_dir) = first_dst.parent() {
                    let ext = match self.config.hash_algorithm {
                        crate::hash::hasher::HashAlgorithm::Md5 => "md5",
                        crate::hash::hasher::HashAlgorithm::Sha1 => "sha1",
                        crate::hash::hasher::HashAlgorithm::Sha256 => "sha256",
                        _ => "hash",
                    };
                    let manifest_file = target_dir.join(format!("checksums.{}", ext));
                    let refs: Vec<(&Path, &str)> = checksum_records
                        .iter()
                        .map(|(p, h)| (p.as_path(), h.as_str()))
                        .collect();
                    if let Err(e) = HashVerifier::export_checksum_manifest(&manifest_file, &refs) {
                        error!("Failed to export checksum manifest to {:?}: {}", manifest_file, e);
                    }
                }
            }
        }

        if self.cancel_flag.load(Ordering::SeqCst) {
            info!("TransferCoordinator canceled at final check");
            self.mark_remaining_canceled(0);
            let _ = self.event_tx.send(EngineEvent::TransferCanceled);
            return Err(TransferError::Canceled);
        }

        info!("TransferCoordinator completed: completed={}, skipped={}, failed={}, total_transferred={}", stats.completed_files, stats.skipped_files, stats.failed_files, stats.transferred_bytes);
        let _ = self.event_tx.send(EngineEvent::TransferCompleted(stats.clone()));
        Ok(stats)
    }

    /// Attempts copying a file with configurable retries, timeout, and VSS fallback for locked files.
    async fn copy_with_retry(
        &self,
        item: &mut FileItem,
        copier: &FileCopier,
        stats: &mut TransferStats,
        speed_calc: &mut SpeedCalculator,
    ) -> Result<(u64, std::path::PathBuf, Option<ResolvedShadow>)> {
        let max_retries = self.config.retry_count;
        let timeout_duration = Duration::from_secs(self.config.retry_timeout_secs);
        let start_time = Instant::now();

        let mut attempt = 0;
        let mut actual_src_path = item.src_path.clone();
        let mut shadow_lease: Option<ResolvedShadow> = None;

        loop {
            attempt += 1;
            item.retry_attempts = attempt;

            let pause = Arc::clone(&self.pause_flag);
            let cancel = Arc::clone(&self.cancel_flag);
            let event_tx_clone = self.event_tx.clone();
            let file_id = item.id;
            let file_name = item.src_path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let file_size = item.size;

            let base_transferred = stats.transferred_bytes;
            let total_bytes = stats.total_bytes;
            let total_files = stats.total_files;
            let completed_files = stats.completed_files;
            let skipped_files = stats.skipped_files;
            let failed_files = stats.failed_files;

            struct SharedProgress {
                last_emit: Instant,
                speed_calc: SpeedCalculator,
            }

            let shared_progress = Arc::new(parking_lot::Mutex::new(SharedProgress {
                last_emit: Instant::now(),
                speed_calc: speed_calc.clone(),
            }));

            let progress_state_clone = Arc::clone(&shared_progress);
            let file_name_clone = file_name.clone();

            let mut hasher = MultiHasher::new(self.config.hash_algorithm);

            let res = copier.copy_file_with_skip(
                &actual_src_path,
                &item.dst_path,
                Some(&mut hasher),
                pause,
                cancel,
                Some(Arc::clone(&self.skip_flag)),
                move |file_copied| {
                    let mut sp = progress_state_clone.lock();
                    let current_total = base_transferred + file_copied;
                    sp.speed_calc.record_progress(current_total);

                    let now = Instant::now();
                    if now.duration_since(sp.last_emit) >= Duration::from_millis(100) {
                        sp.last_emit = now;
                        let cur_speed = sp.speed_calc.current_speed();
                        let avg_speed = sp.speed_calc.average_speed(current_total);
                        let remaining = total_bytes.saturating_sub(current_total);
                        let eta = sp.speed_calc.estimate_eta(remaining);
                        let pct = if total_bytes > 0 {
                            (current_total as f32 / total_bytes as f32) * 100.0
                        } else {
                            0.0
                        };

                        let live_stats = TransferStats {
                            total_files,
                            completed_files,
                            skipped_files,
                            failed_files,
                            total_bytes,
                            transferred_bytes: current_total,
                            current_speed_bps: cur_speed,
                            average_speed_bps: avg_speed,
                            eta_seconds: eta,
                            overall_percentage: pct,
                            current_file_name: file_name_clone.clone(),
                            current_file_transferred: file_copied,
                            current_file_size: file_size,
                        };

                        let _ = event_tx_clone.send(EngineEvent::ProgressUpdated(live_stats));
                        let _ = event_tx_clone.send(EngineEvent::FileProgress {
                            id: file_id,
                            transferred: file_copied,
                            total: file_size,
                        });
                    }
                },
            );

            match res {
                Ok(bytes) => {
                    item.src_hash = Some(hasher.finalize_hex());
                    let final_sp = shared_progress.lock();
                    *speed_calc = final_sp.speed_calc.clone();
                    stats.transferred_bytes = base_transferred + bytes;
                    stats.current_speed_bps = speed_calc.current_speed();
                    stats.average_speed_bps = speed_calc.average_speed(stats.transferred_bytes);
                    let remaining = stats.total_bytes.saturating_sub(stats.transferred_bytes);
                    stats.eta_seconds = speed_calc.estimate_eta(remaining);
                    stats.current_file_transferred = bytes;
                    if total_bytes > 0 {
                        stats.overall_percentage = (stats.transferred_bytes as f32 / total_bytes as f32) * 100.0;
                    }
                    let _ = self.event_tx.send(EngineEvent::ProgressUpdated(stats.clone()));
                    let _ = self.event_tx.send(EngineEvent::FileProgress {
                        id: file_id,
                        transferred: bytes,
                        total: file_size,
                    });
                    return Ok((bytes, actual_src_path, shadow_lease));
                }
                Err(TransferError::Canceled) => {
                    // The copier removed its partial temporary file; never touch the destination.
                    return Err(TransferError::Canceled);
                }
                Err(TransferError::SkippedByUser) => {
                    return Err(TransferError::SkippedByUser);
                }
                Err(TransferError::FileLocked) if self.config.use_vss_for_locked && actual_src_path == item.src_path => {
                    warn!("File is locked: {:?}. Attempting VSS shadow copy...", item.src_path);
                    match VssManager::resolve_shadow_path(&item.src_path) {
                        Ok(shadow) => {
                            actual_src_path = shadow.path().to_path_buf();
                            shadow_lease = Some(shadow);
                            continue;
                        }
                        Err(e) => warn!("VSS shadow copy unavailable: {}", e),
                    }
                    if attempt >= max_retries || start_time.elapsed() >= timeout_duration {
                        return Err(TransferError::FileLocked);
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
                Err(err) => {
                    if attempt >= max_retries || start_time.elapsed() >= timeout_duration {
                        return Err(err);
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    }

    /// Returns list of failed items for "Retry Failed Only" action.
    pub fn get_failed_items(&self) -> Vec<FileItem> {
        self.items
            .iter()
            .filter(|i| i.status == FileItemStatus::Failed)
            .cloned()
            .collect()
    }

    /// Failed and user-skipped items can both be retried by the Retry action.
    pub fn get_retryable_items(&self) -> Vec<FileItem> {
        self.items
            .iter()
            .filter(|i| i.status == FileItemStatus::Failed || i.status == FileItemStatus::Skipped)
            .cloned()
            .collect()
    }

    /// Returns list of skipped items.
    pub fn get_skipped_items(&self) -> Vec<FileItem> {
        self.items
            .iter()
            .filter(|i| i.status == FileItemStatus::Skipped)
            .cloned()
            .collect()
    }
}

/// A file skipped before any bytes were copied no longer counts toward progress totals.
fn account_pre_copy_skip(stats: &mut TransferStats, size: u64) {
    stats.skipped_files += 1;
    stats.total_files = stats.total_files.saturating_sub(1);
    stats.total_bytes = stats.total_bytes.saturating_sub(size);
    stats.overall_percentage = if stats.total_bytes == 0 {
        100.0
    } else {
        ((stats.transferred_bytes as f32 / stats.total_bytes as f32) * 100.0).min(100.0)
    };
}
