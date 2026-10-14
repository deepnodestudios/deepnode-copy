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

            if get_res.is_err() {
                return Ok(()); // Non-critical or permission denied, continue
            }

            let _ = SetNamedSecurityInfoW(
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
        }

        Ok(())
    }
}
