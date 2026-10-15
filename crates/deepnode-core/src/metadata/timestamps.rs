use std::path::Path;
use windows::Win32::Foundation::{FILETIME, GetLastError};
use windows::Win32::Storage::FileSystem::{
    GetFileTime, SetFileTime,
};
use crate::error::{Result, TransferError};
use crate::io::win32_file::Win32FileHandle;

pub struct FileTimestamps {
    pub creation_time: FILETIME,
    pub last_access_time: FILETIME,
    pub last_write_time: FILETIME,
}

impl FileTimestamps {
    /// Converts a SystemTime to a Windows FILETIME.
    pub fn system_time_to_filetime(st: std::time::SystemTime) -> FILETIME {
        let dur = st.duration_since(std::time::SystemTime::UNIX_EPOCH).unwrap_or_default();
        let intervals = (dur.as_secs() + 11644473600) * 10_000_000 + dur.subsec_nanos() as u64 / 100;
        FILETIME {
            dwLowDateTime: intervals as u32,
            dwHighDateTime: (intervals >> 32) as u32,
        }
    }

    /// Reads timestamps from a file handle.
    pub fn read(handle: &Win32FileHandle) -> Result<Self> {
        let mut creation = FILETIME::default();
        let mut access = FILETIME::default();
        let mut write = FILETIME::default();

        unsafe {
            GetFileTime(
                handle.raw(),
                Some(&mut creation),
                Some(&mut access),
                Some(&mut write),
            ).map_err(|e| TransferError::Win32 {
                code: GetLastError().0,
                message: e.to_string(),
            })?;
        }

        Ok(Self {
            creation_time: creation,
            last_access_time: access,
            last_write_time: write,
        })
    }

    /// Applies timestamps to destination file handle.
    pub fn apply(&self, handle: &Win32FileHandle) -> Result<()> {
        unsafe {
            SetFileTime(
                handle.raw(),
                Some(&self.creation_time),
                Some(&self.last_access_time),
                Some(&self.last_write_time),
            ).map_err(|e| TransferError::Win32 {
                code: GetLastError().0,
                message: e.to_string(),
            })?;
        }
        Ok(())
    }

    /// Copies timestamps directly from source path to destination path.
    pub fn copy_between_paths(src: &Path, dst: &Path) -> Result<()> {
        let src_handle = Win32FileHandle::open_read(src, false, false)?;
        let timestamps = Self::read(&src_handle)?;

        // Open destination specifically for timestamps (handles read-only files)
        let dst_handle = Win32FileHandle::open_for_timestamps(dst)?;
        timestamps.apply(&dst_handle)
    }
}
