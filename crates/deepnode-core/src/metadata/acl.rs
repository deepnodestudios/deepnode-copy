use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HLOCAL;
use windows::Win32::Security::{
    DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
};
use windows::Win32::Security::Authorization::{
    GetNamedSecurityInfoW, SetNamedSecurityInfoW, SE_FILE_OBJECT,
};
use crate::error::Result;
use crate::io::win32_file::Win32FileHandle;

pub struct SecurityAcl;

impl SecurityAcl {
    /// Copies discretionary access control lists (DACL) from source file to destination.
    pub fn copy_dacl(src: &Path, dst: &Path) -> Result<()> {
        let src_wide = Win32FileHandle::to_wide_path(src);
        let dst_wide = Win32FileHandle::to_wide_path(dst);

        unsafe {
            let mut sec_desc = PSECURITY_DESCRIPTOR::default();
            let mut dacl = std::ptr::null_mut();

            let get_res = GetNamedSecurityInfoW(
                PCWSTR(src_wide.as_ptr()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                Some(&mut dacl),
                None,
                &mut sec_desc,
            );

            if get_res.0 != 0 {
                tracing::warn!("GetNamedSecurityInfoW failed for {:?}: code {}", src, get_res.0);
                return Err(crate::error::TransferError::Win32 {
                    code: get_res.0,
                    message: format!("Failed to read security DACL for {:?}: code {}", src, get_res.0),
                });
            }

            let set_res = SetNamedSecurityInfoW(
                PCWSTR(dst_wide.as_ptr()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                Some(dacl),
                None,
            );

            if !sec_desc.0.is_null() {
                let _ = windows::Win32::Foundation::LocalFree(HLOCAL(sec_desc.0));
            }

            if set_res.0 != 0 {
                tracing::warn!("SetNamedSecurityInfoW failed for {:?}: code {}", dst, set_res.0);
                return Err(crate::error::TransferError::Win32 {
                    code: set_res.0,
                    message: format!("Failed to set security DACL for {:?}: code {}", dst, set_res.0),
                });
            }
        }

        Ok(())
    }
}
