use std::path::Path;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
    ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, GetFileSizeEx, SetEndOfFile, SetFilePointerEx,
    FILE_ATTRIBUTE_NORMAL, FILE_FLAGS_AND_ATTRIBUTES,
    FILE_FLAG_NO_BUFFERING, FILE_FLAG_OVERLAPPED, FILE_FLAG_SEQUENTIAL_SCAN,
    FILE_FLAG_WRITE_THROUGH, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    CREATE_ALWAYS, OPEN_EXISTING, OPEN_ALWAYS, FILE_BEGIN,
    ReadFile, WriteFile,
};
use windows::Win32::System::IO::{GetOverlappedResult, OVERLAPPED};
use crate::error::{Result, TransferError};

pub struct Win32FileHandle {
    handle: HANDLE,
}

unsafe impl Send for Win32FileHandle {}
unsafe impl Sync for Win32FileHandle {}

impl Win32FileHandle {
    pub fn raw(&self) -> HANDLE {
        self.handle
    }

    /// Converts a std::path::Path to null-terminated UTF-16 wide string with long path (\\?\) support.
    pub fn to_wide_path(path: &Path) -> Vec<u16> {
        let clean = crate::util::sanitize_path(path);
        let abs_path = if clean.is_absolute() {
            clean
        } else {
            match std::env::current_dir() {
                Ok(cur) => cur.join(clean),
                Err(_) => clean,
            }
        };

        let path_str = abs_path.to_string_lossy();
        let extended_path = if !path_str.starts_with(r"\\?\") && !path_str.starts_with(r"\\.\") {
            format!(r"\\?\{}", path_str)
        } else {
            path_str.to_string()
        };

        OsStr::new(&extended_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// Opens a file for reading with options for direct I/O and overlapped flags.
    pub fn open_read(path: &Path, direct_io: bool, overlapped: bool) -> Result<Self> {
        let wide = Self::to_wide_path(path);
        let mut flags = FILE_ATTRIBUTE_NORMAL.0 | FILE_FLAG_SEQUENTIAL_SCAN.0;

        if direct_io {
            flags |= FILE_FLAG_NO_BUFFERING.0;
        }
        if overlapped {
            flags |= FILE_FLAG_OVERLAPPED.0;
        }

        unsafe {
            let handle = CreateFileW(
                PCWSTR(wide.as_ptr()),
                FILE_GENERIC_READ.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(flags),
                HANDLE(std::ptr::null_mut()),
            ).map_err(|e| {
                let code = GetLastError().0;
                if code == ERROR_SHARING_VIOLATION.0 || code == ERROR_LOCK_VIOLATION.0 {
                    TransferError::FileLocked
                } else {
                    TransferError::Win32 {
                        code,
                        message: e.to_string(),
                    }
                }
            })?;

            if handle == INVALID_HANDLE_VALUE {
                let code = GetLastError().0;
                if code == ERROR_SHARING_VIOLATION.0 || code == ERROR_LOCK_VIOLATION.0 {
                    return Err(TransferError::FileLocked);
                }
                return Err(TransferError::Win32 {
                    code,
                    message: "Failed to open file for reading".to_string(),
                });
            }

            Ok(Self { handle })
        }
    }

    /// Opens or creates a file for writing with pre-allocation support.
    pub fn open_write(
        path: &Path,
        overwrite: bool,
        direct_io: bool,
        overlapped: bool,
    ) -> Result<Self> {
        let wide = Self::to_wide_path(path);
        let mut flags = FILE_ATTRIBUTE_NORMAL.0 | FILE_FLAG_SEQUENTIAL_SCAN.0;

        if direct_io {
            flags |= FILE_FLAG_NO_BUFFERING.0 | FILE_FLAG_WRITE_THROUGH.0;
        }
        if overlapped {
            flags |= FILE_FLAG_OVERLAPPED.0;
        }

        let disposition = if overwrite { CREATE_ALWAYS } else { OPEN_ALWAYS };

        unsafe {
            let handle = CreateFileW(
                PCWSTR(wide.as_ptr()),
                FILE_GENERIC_WRITE.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                disposition,
                FILE_FLAGS_AND_ATTRIBUTES(flags),
                HANDLE(std::ptr::null_mut()),
            ).map_err(|e| {
                let code = GetLastError().0;
                TransferError::Win32 {
                    code,
                    message: e.to_string(),
                }
            })?;

            if handle == INVALID_HANDLE_VALUE {
                let code = GetLastError().0;
                return Err(TransferError::Win32 {
                    code,
                    message: "Failed to open/create file for writing".to_string(),
                });
            }

            Ok(Self { handle })
        }
    }

    /// Gets exact file size in bytes using GetFileSizeEx.
    pub fn get_size(&self) -> Result<u64> {
        unsafe {
            let mut size: i64 = 0;
            GetFileSizeEx(self.handle, &mut size).map_err(|e| TransferError::Win32 {
                code: GetLastError().0,
                message: e.to_string(),
            })?;
            Ok(size as u64)
        }
    }

    /// Pre-allocates file size to minimize disk fragmentation.
    pub fn preallocate_size(&self, size: u64) -> Result<()> {
        unsafe {
            // First try fast FileAllocationInfo (instantaneous on NTFS without zeroing clusters)
            let alloc_info = windows::Win32::Storage::FileSystem::FILE_ALLOCATION_INFO {
                AllocationSize: size as i64,
            };
            let res = windows::Win32::Storage::FileSystem::SetFileInformationByHandle(
                self.handle,
                windows::Win32::Storage::FileSystem::FileAllocationInfo,
                &alloc_info as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<windows::Win32::Storage::FileSystem::FILE_ALLOCATION_INFO>() as u32,
            );
            if res.is_ok() {
                return Ok(());
            }

            // Fallback to SetFilePointerEx + SetEndOfFile
            SetFilePointerEx(self.handle, size as i64, None, FILE_BEGIN).map_err(|e| {
                TransferError::Win32 {
                    code: GetLastError().0,
                    message: e.to_string(),
                }
            })?;

            SetEndOfFile(self.handle).map_err(|e| TransferError::Win32 {
                code: GetLastError().0,
                message: e.to_string(),
            })?;

            // Reset file pointer back to start
            SetFilePointerEx(self.handle, 0, None, FILE_BEGIN).map_err(|e| {
                TransferError::Win32 {
                    code: GetLastError().0,
                    message: e.to_string(),
                }
            })?;

            Ok(())
        }
    }

    /// Reads up to buffer.len() bytes synchronously.
    pub fn read_sync(&self, buffer: &mut [u8]) -> Result<usize> {
        unsafe {
            let mut bytes_read: u32 = 0;
            let success = ReadFile(
                self.handle,
                Some(buffer),
                Some(&mut bytes_read),
                None,
            );

            if let Err(e) = success {
                let code = GetLastError().0;
                return Err(TransferError::Win32 {
                    code,
                    message: e.to_string(),
                });
            }

            Ok(bytes_read as usize)
        }
    }

    /// Writes buffer.len() bytes synchronously.
    pub fn write_sync(&self, buffer: &[u8]) -> Result<usize> {
        unsafe {
            let mut bytes_written: u32 = 0;
            let success = WriteFile(
                self.handle,
                Some(buffer),
                Some(&mut bytes_written),
                None,
            );

            if let Err(e) = success {
                let code = GetLastError().0;
                return Err(TransferError::Win32 {
                    code,
                    message: e.to_string(),
                });
            }

            Ok(bytes_written as usize)
        }
    }

    /// Asynchronous Overlapped Read at given offset.
    pub fn read_overlapped(
        &self,
        buffer: &mut [u8],
        offset: u64,
        overlapped: &mut OVERLAPPED,
    ) -> Result<()> {
        overlapped.Anonymous.Anonymous.Offset = (offset & 0xFFFFFFFF) as u32;
        overlapped.Anonymous.Anonymous.OffsetHigh = ((offset >> 32) & 0xFFFFFFFF) as u32;

        unsafe {
            let res = ReadFile(
                self.handle,
                Some(buffer),
                None,
                Some(overlapped),
            );

            if let Err(e) = res {
                let code = GetLastError();
                if code.0 != 997 { // ERROR_IO_PENDING = 997
                    return Err(TransferError::Win32 {
                        code: code.0,
                        message: e.to_string(),
                    });
                }
            }
            Ok(())
        }
    }

    /// Asynchronous Overlapped Write at given offset.
    pub fn write_overlapped(
        &self,
        buffer: &[u8],
        offset: u64,
        overlapped: &mut OVERLAPPED,
    ) -> Result<()> {
        overlapped.Anonymous.Anonymous.Offset = (offset & 0xFFFFFFFF) as u32;
        overlapped.Anonymous.Anonymous.OffsetHigh = ((offset >> 32) & 0xFFFFFFFF) as u32;

        unsafe {
            let res = WriteFile(
                self.handle,
                Some(buffer),
                None,
                Some(overlapped),
            );

            if let Err(e) = res {
                let code = GetLastError();
                if code.0 != 997 { // ERROR_IO_PENDING = 997
                    return Err(TransferError::Win32 {
                        code: code.0,
                        message: e.to_string(),
                    });
                }
            }
            Ok(())
        }
    }

    /// Waits for overlapped I/O operation to complete and returns transferred bytes.
    pub fn wait_overlapped(&self, overlapped: &mut OVERLAPPED) -> Result<usize> {
        unsafe {
            let mut bytes_transferred: u32 = 0;
            let success = GetOverlappedResult(self.handle, overlapped, &mut bytes_transferred, true);

            if let Err(e) = success {
                let code = GetLastError().0;
                return Err(TransferError::Win32 {
                    code,
                    message: e.to_string(),
                });
            }

            Ok(bytes_transferred as usize)
        }
    }
}

impl Drop for Win32FileHandle {
    fn drop(&mut self) {
        if self.handle != INVALID_HANDLE_VALUE && !self.handle.0.is_null() {
            unsafe {
                let _ = CloseHandle(self.handle);
            }
        }
    }
}
