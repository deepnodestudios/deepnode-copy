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
use crate::vss::shadow_copy::VssManager;

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
        crate::io::copier::cleanup_cancelled_file_and_empty_parents(
            dst_path,
            self.destination_dir.as_deref(),
        )
    }

    /// Reliably removes a partially transferred file on cancellation and cleans up any resulting empty directories.
    pub fn remove_partial_file(path: &Path) -> bool {
        crate::io::copier::cleanup_cancelled_file_and_empty_parents(path, None)
    }

    fn mark_remaining_canceled(&mut self, from_idx: usize) {
        for i in from_idx..self.items.len() {
            if self.items[i].status == FileItemStatus::Copying {
                self.items[i].status = FileItemStatus::Canceled;
                self.cleanup_cancelled_item(&self.items[i].dst_path);
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
                for _ in 0..rel_components {
                    if let Some(p) = inferred.parent() {
                        inferred = p;
                    }
                }
                if let Some(p) = inferred.parent() {
                    self.destination_dir = Some(p.to_path_buf());
                } else {
                    self.destination_dir = Some(inferred.to_path_buf());
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
                self.mark_remaining_canceled(idx);
                let _ = self.event_tx.send(EngineEvent::TransferCanceled);
                return Err(TransferError::Canceled);
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
                stats.skipped_files += 1;
                let _ = self.event_tx.send(EngineEvent::FileSkipped {
                    item: item.clone(),
                    reason: "Skipped by user".to_string(),
                });
                let _ = self.event_tx.send(EngineEvent::ProgressUpdated(stats.clone()));
                continue;
            }

            let mut item = self.items[idx].clone();
            item.status = FileItemStatus::Copying;
            self.items[idx].status = FileItemStatus::Copying;

            stats.current_file_name = item
                .src_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            stats.current_file_size = item.size;
            stats.current_file_transferred = 0;

            info!("Starting file transfer [{}/{}]: {} ({} bytes)", idx + 1, total_files, stats.current_file_name, item.size);

            let _ = self.event_tx.send(EngineEvent::FileStarted(item.clone()));
            let _ = self.event_tx.send(EngineEvent::FileProgress {
                id: item.id,
                transferred: 0,
                total: item.size,
            });
            let _ = self.event_tx.send(EngineEvent::ProgressUpdated(stats.clone()));

            // Conflict resolution
            let resolution = ConflictResolver::resolve(
                &item.src_path,
                &item.dst_path,
                self.config.conflict_policy,
            );

            match resolution {
                ConflictResolutionAction::SkipFile => {
                    item.status = FileItemStatus::Skipped;
                    stats.skipped_files += 1;
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

                        // Wait for response from frontend or cancellation
                        let choice_res = tokio::select! {
                            res = rx => res.ok(),
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
                            self.mark_remaining_canceled(idx);
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
                                stats.skipped_files += 1;
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

            // Transfer single file with smart retry loop
            let copy_result = self.copy_with_retry(&mut item, &copier, &mut stats, &mut speed_calc).await;

            match copy_result {
                Err(TransferError::Canceled) => {
                    self.cleanup_cancelled_item(&item.dst_path);
                    item.status = FileItemStatus::Canceled;
                    self.items[idx] = item;
                    self.mark_remaining_canceled(idx + 1);
                    let _ = self.event_tx.send(EngineEvent::TransferCanceled);
                    return Err(TransferError::Canceled);
                }
                Err(TransferError::SkippedByUser) => {
                    self.cleanup_cancelled_item(&item.dst_path);
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
                Ok(bytes_transferred) => {
                    // Post-copy verification
                    if self.config.verify_mode == VerifyMode::Always && item.size > 0 {
                        if self.cancel_flag.load(Ordering::SeqCst) {
                            self.cleanup_cancelled_item(&item.dst_path);
                            item.status = FileItemStatus::Canceled;
                            self.items[idx] = item;
                            self.mark_remaining_canceled(idx + 1);
                            let _ = self.event_tx.send(EngineEvent::TransferCanceled);
                            return Err(TransferError::Canceled);
                        }
                        let _ = self.event_tx.send(EngineEvent::VerificationStarted(item.clone()));
                        let verify_res = HashVerifier::verify_files(
                            &item.src_path,
                            &item.dst_path,
                            self.config.hash_algorithm,
                        );

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

                    // Check cancellation immediately after copy/verify
                    if self.cancel_flag.load(Ordering::SeqCst) {
                        self.cleanup_cancelled_item(&item.dst_path);
                        item.status = FileItemStatus::Canceled;
                        self.items[idx] = item;
                        self.mark_remaining_canceled(idx + 1);
                        let _ = self.event_tx.send(EngineEvent::TransferCanceled);
                        return Err(TransferError::Canceled);
                    }

                    // Metadata preservation
                    if self.config.preserve_timestamps {
                        let _ = FileTimestamps::copy_between_paths(&item.src_path, &item.dst_path);
                    }
                    if self.config.preserve_attributes {
                        let _ = FileAttributes::copy_attributes(&item.src_path, &item.dst_path);
                    }
                    if self.config.preserve_ads {
                        let _ = AdsManager::copy_alternate_streams(&item.src_path, &item.dst_path);
                    }
                    if self.config.preserve_acl {
                        let _ = SecurityAcl::copy_dacl(&item.src_path, &item.dst_path);
                    }

                    // NTFS ADS checksum writing
                    if self.config.write_checksum_to_ads {
                        if let Some(ref h) = item.src_hash {
                            let _ = HashVerifier::write_hash_to_ads(
                                &item.dst_path,
                                self.config.hash_algorithm,
                                h,
                            );
                        }
                    }

                    // Collect checksum manifest records
                    if self.config.export_checksum_file {
                        if let Some(ref h) = item.src_hash {
                            checksum_records.push((item.dst_path.clone(), h.clone()));
                        }
                    }

                    // If Move mode, delete source upon verified transfer
                    if self.config.mode == TransferMode::Move {
                        let _ = std::fs::remove_file(&item.src_path);
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

            // Update overall progress percentage
            if total_bytes > 0 {
                stats.overall_percentage = (stats.transferred_bytes as f32 / total_bytes as f32) * 100.0;
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
                    let _ = HashVerifier::export_checksum_manifest(&manifest_file, &refs);
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
    ) -> Result<u64> {
        let max_retries = self.config.retry_count;
        let timeout_duration = Duration::from_secs(self.config.retry_timeout_secs);
        let start_time = Instant::now();

        let mut attempt = 0;
        let mut actual_src_path = item.src_path.clone();

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
                    return Ok(bytes);
                }
                Err(TransferError::Canceled) => {
                    self.cleanup_cancelled_item(&item.dst_path);
                    return Err(TransferError::Canceled);
                }
                Err(TransferError::SkippedByUser) => {
                    self.cleanup_cancelled_item(&item.dst_path);
                    return Err(TransferError::SkippedByUser);
                }
                Err(TransferError::FileLocked) if self.config.use_vss_for_locked && actual_src_path == item.src_path => {
                    warn!("File is locked: {:?}. Attempting VSS shadow copy...", item.src_path);
                    if let Ok(shadow_path) = VssManager::resolve_shadow_path(&item.src_path) {
                        actual_src_path = shadow_path;
                        continue;
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

    /// Returns list of failed or skipped items for "Retry Failed Only" action.
    pub fn get_failed_items(&self) -> Vec<FileItem> {
        self.items
            .iter()
            .filter(|i| i.status == FileItemStatus::Failed || i.status == FileItemStatus::Skipped)
            .cloned()
            .collect()
    }
}
