use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Foundation::GetLastError;
use windows::Win32::Storage::FileSystem::{
    GetFileAttributesW, SetFileAttributesW, FILE_FLAGS_AND_ATTRIBUTES, INVALID_FILE_ATTRIBUTES,
};
use crate::error::{Result, TransferError};
use crate::io::win32_file::Win32FileHandle;

pub struct FileAttributes;

impl FileAttributes {
    /// Copies standard Windows file attributes (Read-only, Hidden, System, Archive).
    pub fn copy_attributes(src: &Path, dst: &Path) -> Result<()> {
        let src_wide = Win32FileHandle::to_wide_path(src);
        let dst_wide = Win32FileHandle::to_wide_path(dst);

        unsafe {
            let attrs = GetFileAttributesW(PCWSTR(src_wide.as_ptr()));
            if attrs == INVALID_FILE_ATTRIBUTES {
                return Err(TransferError::Win32 {
                    code: GetLastError().0,
                    message: "Failed to read file attributes".to_string(),
                });
            }

            // Exclude temporary or sparse attributes if needed, keep standard
            let mask = 0x00000001 /* READONLY */
                | 0x00000002     /* HIDDEN */
                | 0x00000004     /* SYSTEM */
                | 0x00000020;    /* ARCHIVE */

            let preserved_attrs = FILE_FLAGS_AND_ATTRIBUTES(attrs & mask);

            SetFileAttributesW(PCWSTR(dst_wide.as_ptr()), preserved_attrs)
                .map_err(|e| TransferError::Win32 {
                    code: GetLastError().0,
                    message: e.to_string(),
                })?;
        }

        Ok(())
    }
}
