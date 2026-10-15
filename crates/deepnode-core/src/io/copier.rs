use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use crate::error::{Result, TransferError};
use crate::hash::hasher::MultiHasher;
use crate::io::buffer::AlignedBufferPool;
use crate::io::disk::{DiskDetector, DiskTopology};
use crate::io::win32_file::Win32FileHandle;

pub struct CopyProgress {
    pub bytes_copied: u64,
    pub total_bytes: u64,
}

pub type ProgressCallback = Box<dyn Fn(u64) + Send + Sync>;

pub struct FileCopier {
    buffer_pool: AlignedBufferPool,
    buffer_size: usize,
    direct_io: bool,
}

impl FileCopier {
    pub fn new(buffer_size: usize, direct_io: bool) -> Self {
        let size = if buffer_size == 0 { 2 * 1024 * 1024 } else { buffer_size };
        Self {
            buffer_pool: AlignedBufferPool::new(size, 4096, 4),
            buffer_size: size,
            direct_io,
        }
    }

    /// High performance copy of a single file with on-the-fly hash calculation and pause/cancel support.
    pub fn copy_file<F>(
        &self,
        src_path: &Path,
        dst_path: &Path,
        hasher: Option<&mut MultiHasher>,
        pause_flag: Arc<AtomicBool>,
        cancel_flag: Arc<AtomicBool>,
        progress_cb: F,
    ) -> Result<u64>
    where
        F: Fn(u64) + Send + Sync + 'static,
    {
        self.copy_file_with_skip(src_path, dst_path, hasher, pause_flag, cancel_flag, None, progress_cb)
    }

    /// High performance copy of a single file with on-the-fly hash calculation, pause/cancel support, and individual file skipping.
    #[allow(clippy::too_many_arguments, clippy::permissions_set_readonly_false)]
    pub fn copy_file_with_skip<F>(
        &self,
        src_path: &Path,
        dst_path: &Path,
        mut hasher: Option<&mut MultiHasher>,
        pause_flag: Arc<AtomicBool>,
        cancel_flag: Arc<AtomicBool>,
        skip_flag: Option<Arc<AtomicBool>>,
        progress_cb: F,
    ) -> Result<u64>
    where
        F: Fn(u64) + Send + Sync + 'static,
    {
        // 1. Initial cancel / skip check before touching any file or metadata
        if cancel_flag.load(Ordering::SeqCst) {
            return Err(TransferError::Canceled);
        }
        if let Some(ref skip) = skip_flag {
            if skip.load(Ordering::SeqCst) {
                skip.store(false, Ordering::SeqCst);
                return Err(TransferError::SkippedByUser);
            }
        }

        // 1. Initial cancel & skip check BEFORE touching or truncating destination!
        if cancel_flag.load(Ordering::SeqCst) {
            return Err(TransferError::Canceled);
        }
        if let Some(ref skip) = skip_flag {
            if skip.load(Ordering::SeqCst) {
                skip.store(false, Ordering::SeqCst);
                return Err(TransferError::SkippedByUser);
            }
        }

        // 2. Pre-open pause check: wait here BEFORE touching or truncating destination!
        while pause_flag.load(Ordering::Relaxed) {
            if cancel_flag.load(Ordering::SeqCst) {
                return Err(TransferError::Canceled);
            }
            if let Some(ref skip) = skip_flag {
                if skip.load(Ordering::SeqCst) {
                    skip.store(false, Ordering::SeqCst);
                    return Err(TransferError::SkippedByUser);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        let dst_existed_before = dst_path.exists();
        let topology = DiskDetector::analyze(src_path, dst_path);

        // Pre-query source size to decide on Direct I/O qualification
        let probe_size = std::fs::metadata(src_path).map(|m| m.len()).unwrap_or(0);
        const SECTOR_SIZE: usize = 4096;
        let use_direct_io = self.direct_io && probe_size >= 64 * 1024;

        let (src_file, src_is_direct) = if use_direct_io {
            match Win32FileHandle::open_read(src_path, true, false) {
                Ok(h) => (h, true),
                Err(_) => (Win32FileHandle::open_read(src_path, false, false)?, false),
            }
        } else {
            (Win32FileHandle::open_read(src_path, false, false)?, false)
        };
        let total_size = src_file.get_size()?;

        // Ensure parent directory exists
        if let Some(parent) = dst_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // If destination exists, clear any read-only attribute so CREATE_ALWAYS succeeds
        if dst_existed_before {
            if let Ok(metadata) = std::fs::metadata(dst_path) {
                let mut perms = metadata.permissions();
                if perms.readonly() {
                    perms.set_readonly(false);
                    let _ = std::fs::set_permissions(dst_path, perms);
                }
            }
        }

        let (dst_file, dst_is_direct) = if src_is_direct {
            match Win32FileHandle::open_write(dst_path, true, true, false) {
                Ok(h) => (h, true),
                Err(_) => (Win32FileHandle::open_write(dst_path, true, false, false)?, false),
            }
        } else {
            (Win32FileHandle::open_write(dst_path, true, false, false)?, false)
        };

        if total_size > 0 {
            if let Err(e) = dst_file.preallocate_size(total_size) {
                tracing::warn!("Preallocate size failed for {:?}: {}. Continuing with dynamic allocation.", dst_path, e);
            }
        }

        // Adjust buffer size depending on topology
        let effective_buf_size = match topology {
            DiskTopology::SamePhysicalDrive(_) => 4 * 1024 * 1024, // 4MB to prevent seek chatter
            _ => self.buffer_size,
        };

        let mut buffer = self.buffer_pool.acquire();
        let mut total_copied: u64 = 0;

        while total_copied < total_size {
            // Check cancellation
            if cancel_flag.load(Ordering::SeqCst) {
                self.buffer_pool.release(buffer);
                drop(dst_file);
                drop(src_file);
                if !dst_existed_before || total_copied > 0 {
                    cleanup_cancelled_file(dst_path);
                }
                return Err(TransferError::Canceled);
            }

            // Check file skip
            if let Some(ref skip) = skip_flag {
                if skip.load(Ordering::SeqCst) {
                    skip.store(false, Ordering::SeqCst);
                    self.buffer_pool.release(buffer);
                    drop(dst_file);
                    drop(src_file);
                    if !dst_existed_before || total_copied > 0 {
                        cleanup_cancelled_file(dst_path);
                    }
                    return Err(TransferError::SkippedByUser);
                }
            }

            // Handle pause state
            while pause_flag.load(Ordering::Relaxed) {
                if cancel_flag.load(Ordering::SeqCst) {
                    self.buffer_pool.release(buffer);
                    drop(dst_file);
                    drop(src_file);
                    if !dst_existed_before || total_copied > 0 {
                        cleanup_cancelled_file(dst_path);
                    }
                    return Err(TransferError::Canceled);
                }
                if let Some(ref skip) = skip_flag {
                    if skip.load(Ordering::SeqCst) {
                        skip.store(false, Ordering::SeqCst);
                        self.buffer_pool.release(buffer);
                        drop(dst_file);
                        drop(src_file);
                        if !dst_existed_before || total_copied > 0 {
                            cleanup_cancelled_file(dst_path);
                        }
                        return Err(TransferError::SkippedByUser);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }

            let slice = buffer.as_mut_slice();
            let max_chunk = std::cmp::min(slice.len(), effective_buf_size);
            let remaining = (total_size - total_copied) as usize;
            let raw_to_read = std::cmp::min(max_chunk, remaining);

            let (_read_bytes, valid_bytes) = if src_is_direct {
                let aligned_to_read = raw_to_read.div_ceil(SECTOR_SIZE) * SECTOR_SIZE;
                let actual_read = src_file.read_sync(&mut slice[..aligned_to_read])?;
                let valid = std::cmp::min(actual_read, raw_to_read);
                (actual_read, valid)
            } else {
                let actual_read = src_file.read_exact_sync(&mut slice[..raw_to_read])?;
                (actual_read, actual_read)
            };

            if valid_bytes == 0 {
                break;
            }

            // Hash on-the-fly with the true logical payload bytes
            if let Some(ref mut h) = hasher {
                h.update(&slice[..valid_bytes]);
            }

            // Write to destination
            if dst_is_direct {
                if valid_bytes % SECTOR_SIZE == 0 {
                    dst_file.write_all_sync(&slice[..valid_bytes])?;
                } else {
                    // Final slack sector: zero-pad buffer up to sector boundary, write full sector, then truncate
                    let aligned_write = valid_bytes.div_ceil(SECTOR_SIZE) * SECTOR_SIZE;
                    slice[valid_bytes..aligned_write].fill(0);
                    dst_file.write_all_sync(&slice[..aligned_write])?;
                    let _ = dst_file.set_length(total_size);
                }
            } else {
                dst_file.write_all_sync(&slice[..valid_bytes])?;
            }

            total_copied += valid_bytes as u64;
            progress_cb(total_copied);
        }

        self.buffer_pool.release(buffer);
        if dst_is_direct && total_size % SECTOR_SIZE as u64 != 0 {
            let _ = dst_file.set_length(total_size);
        }
        drop(dst_file);
        drop(src_file);

        if cancel_flag.load(Ordering::SeqCst) {
            if !dst_existed_before || total_copied > 0 {
                cleanup_cancelled_file(dst_path);
            }
            return Err(TransferError::Canceled);
        }

        Ok(total_copied)
    }
}

/// Reliably deletes a cancelled file (with backoff & Win32 DeleteFileW).
pub fn cleanup_cancelled_file(dst_path: &Path) -> bool {
    let clean_path = crate::util::sanitize_path(dst_path);
    if !clean_path.exists() {
        return true;
    }

    let mut deleted = false;
    for attempt in 1..=15 {
        if std::fs::remove_file(&clean_path).is_ok() {
            deleted = true;
            break;
        }
        let wide = Win32FileHandle::to_wide_path(&clean_path);
        unsafe {
            if windows::Win32::Storage::FileSystem::DeleteFileW(windows::core::PCWSTR(wide.as_ptr())).is_ok() {
                deleted = true;
                break;
            }
        }
        if !clean_path.exists() {
            deleted = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(30 * attempt as u64));
    }

    if !deleted && clean_path.exists() {
        let time_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
        let temp_name = clean_path.with_extension(format!("deepnode_del_{}", time_ms));
        if std::fs::rename(&clean_path, &temp_name).is_ok() {
            let _ = std::fs::remove_file(&temp_name);
            deleted = true;
        }
    }

    deleted
}

/// Reliably deletes a cancelled file and any resulting empty parent directories up to root/desktop or specified stop_dir.
pub fn cleanup_cancelled_file_and_empty_parents(dst_path: &Path, stop_dir: Option<&Path>) -> bool {
    cleanup_cancelled_file(dst_path);

    let clean_path = crate::util::sanitize_path(dst_path);
    let clean_stop = stop_dir.map(crate::util::sanitize_path);

    // Now clean up any empty parent directories created for this path
    let mut current = clean_path.parent();
    while let Some(dir) = current {
        // If a specific destination stop directory was specified, stop at it
        if let Some(ref stop) = clean_stop {
            if dir == stop || dir.to_string_lossy().eq_ignore_ascii_case(&stop.to_string_lossy()) {
                break;
            }
        }

        let dir_str = dir.to_string_lossy().to_lowercase();
        // Stop condition: root drive, Desktop, Downloads, Documents, etc.
        if dir_str.ends_with(r"\desktop")
            || dir_str.ends_with(r"\downloads")
            || dir_str.ends_with(r"\documents")
            || dir_str.ends_with(r"\pictures")
            || dir_str.ends_with(r"\videos")
            || dir_str.ends_with(r"\music")
            || dir.parent().is_none() // e.g. "C:\"
            || dir.parent().unwrap().parent().is_none() // e.g. "C:\Users"
        {
            break;
        }

        let mut dir_removed = false;
        for attempt in 1..=10 {
            if let Ok(mut entries) = std::fs::read_dir(dir) {
                if entries.next().is_none() {
                    // Empty directory
                    if std::fs::remove_dir(dir).is_ok() {
                        dir_removed = true;
                        break;
                    }
                    let wide = Win32FileHandle::to_wide_path(dir);
                    unsafe {
                        if windows::Win32::Storage::FileSystem::RemoveDirectoryW(windows::core::PCWSTR(wide.as_ptr())).is_ok() {
                            dir_removed = true;
                            break;
                        }
                    }
                } else {
                    // Directory is not empty, stop ascending
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(30 * attempt as u64));
        }

        if !dir_removed {
            break;
        }

        current = dir.parent();
    }

    true
}
