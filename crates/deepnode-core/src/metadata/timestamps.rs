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

        // Open destination with GENERIC_WRITE to set timestamps
        let dst_handle = Win32FileHandle::open_write(dst, false, false, false)?;
        timestamps.apply(&dst_handle)
    }
}
