use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::{
    IOCTL_STORAGE_GET_DEVICE_NUMBER, STORAGE_DEVICE_NUMBER,
};
use windows::Win32::System::IO::DeviceIoControl;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskTopology {
    SamePhysicalDrive(u32),
    DifferentPhysicalDrives(u32, u32),
    NetworkOrUnknown,
}

pub struct DiskDetector;

impl DiskDetector {
    /// Extracts the volume root from a path (e.g., "C:\Folder" -> "\\.\C:")
    pub fn get_volume_path(path: &Path) -> Option<String> {
        let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let str_path = abs.to_str()?;
        let clean = str_path.strip_prefix(r"\\?\").unwrap_or(str_path);

        if let Some(drive_letter) = clean.chars().next() {
            if drive_letter.is_alphabetic() && clean.chars().nth(1) == Some(':') {
                return Some(format!(r"\\.\{}:", drive_letter));
            }
        }
        None
    }

    /// Queries the physical disk number of the volume using IOCTL_STORAGE_GET_DEVICE_NUMBER.
    pub fn get_physical_drive_number(path: &Path) -> Option<u32> {
        let vol_path_str = Self::get_volume_path(path)?;
        let wide: Vec<u16> = OsStr::new(&vol_path_str)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let handle: HANDLE = match CreateFileW(
                PCWSTR(wide.as_ptr()),
                0, // No access right needed just for device query
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
                HANDLE(std::ptr::null_mut()),
            ) {
                Ok(h) if h != INVALID_HANDLE_VALUE => h,
                _ => return None,
            };

            let mut dev_num = STORAGE_DEVICE_NUMBER::default();
            let mut bytes_returned: u32 = 0;

            let success = DeviceIoControl(
                handle,
                IOCTL_STORAGE_GET_DEVICE_NUMBER,
                None,
                0,
                Some(&mut dev_num as *mut _ as *mut _),
                std::mem::size_of::<STORAGE_DEVICE_NUMBER>() as u32,
                Some(&mut bytes_returned),
                None,
            );

            let _ = CloseHandle(handle);

            if success.is_ok() {
                Some(dev_num.DeviceNumber)
            } else {
                None
            }
        }
    }

    /// Analyzes the topology between source and destination paths.
    pub fn analyze(source: &Path, destination: &Path) -> DiskTopology {
        let src_disk = Self::get_physical_drive_number(source);
        let dst_disk = Self::get_physical_drive_number(destination);

        match (src_disk, dst_disk) {
            (Some(s), Some(d)) if s == d => DiskTopology::SamePhysicalDrive(s),
            (Some(s), Some(d)) => DiskTopology::DifferentPhysicalDrives(s, d),
            _ => DiskTopology::NetworkOrUnknown,
        }
    }
}
