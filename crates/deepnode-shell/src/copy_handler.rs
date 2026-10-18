use windows::core::{Interface, GUID, PCWSTR};
use windows::Win32::Foundation::{BOOL, CLASS_E_NOAGGREGATION, HWND};
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_SCROLL};
use windows::Win32::UI::Shell::{ICopyHookW, ICopyHookW_Impl};
use windows_core::implement;
use crate::util::get_exe_path;

pub const CLSID_DEEPNODE_COPYHOOK: GUID = GUID::from_u128(0x8f3a2c71_9b4e_4d26_a5f0_6e1d2b7c9a83);

#[implement(ICopyHookW)]
pub struct DeepNodeCopyHook;

impl DeepNodeCopyHook {
    pub fn new() -> Self {
        crate::dll_add_ref();
        Self
    }
}

impl Default for DeepNodeCopyHook {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DeepNodeCopyHook {
    fn drop(&mut self) {
        crate::dll_release();
    }
}

impl ICopyHookW_Impl for DeepNodeCopyHook_Impl {
    fn CopyCallback(
        &self,
        _hwnd: HWND,
        wfunc: u32,
        _wflags: u32,
        pszsrcfile: &PCWSTR,
        _dwsrcattribs: u32,
        pszdestfile: &PCWSTR,
        _dwdestsattribs: u32,
    ) -> u32 {
        const IDYES: u32 = 1;
        const IDCANCEL: u32 = 2;

        // wfunc: FO_MOVE = 1, FO_COPY = 2
        if wfunc != 1 && wfunc != 2 {
            return IDYES;
        }

        // If Scroll Lock is active, bypass DeepNode Copy
        if !CopyHookHandler::should_intercept_operation() {
            return IDYES;
        }

        let src_str = unsafe {
            if pszsrcfile.is_null() {
                String::new()
            } else {
                pszsrcfile.to_string().unwrap_or_default()
            }
        };

        let dest_str = unsafe {
            if pszdestfile.is_null() {
                String::new()
            } else {
                pszdestfile.to_string().unwrap_or_default()
            }
        };

        if src_str.is_empty() || dest_str.is_empty() {
            return IDYES;
        }

        let exe = get_exe_path();
        let mut cmd = std::process::Command::new(&exe);
        if let Some(parent) = exe.parent() {
            cmd.current_dir(parent);
        }

        let mode = if wfunc == 1 { "move" } else { "copy" };
        crate::util::log_shell(&format!("ICopyHookW::CopyCallback: wfunc={} ({}), src={}, dest={}", wfunc, mode, src_str, dest_str));

        cmd.arg("--popup")
            .arg("--mode")
            .arg(mode)
            .arg("--source")
            .arg(&src_str)
            .arg("--target")
            .arg(&dest_str);

        if cmd.spawn().is_ok() {
            crate::util::log_shell("ICopyHookW::CopyCallback spawned popup successfully, returning IDCANCEL to abort native copy");
            // Cancel native Explorer operation so DeepNode Copy takes over
            IDCANCEL
        } else {
            IDYES
        }
    }
}

#[implement(IClassFactory)]
pub struct DeepNodeCopyHookClassFactory;

impl DeepNodeCopyHookClassFactory {
    pub fn new() -> Self {
        crate::dll_add_ref();
        Self
    }
}

impl Default for DeepNodeCopyHookClassFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DeepNodeCopyHookClassFactory {
    fn drop(&mut self) {
        crate::dll_release();
    }
}

impl IClassFactory_Impl for DeepNodeCopyHookClassFactory_Impl {
    fn CreateInstance(
        &self,
        outer: Option<&windows::core::IUnknown>,
        iid: *const GUID,
        ppv: *mut *mut std::ffi::c_void,
    ) -> windows_core::Result<()> {
        if outer.is_some() {
            return Err(windows_core::Error::from(CLASS_E_NOAGGREGATION));
        }
        unsafe {
            let hook: ICopyHookW = DeepNodeCopyHook::new().into();
            hook.query(iid, ppv).ok()
        }
    }

    fn LockServer(&self, lock: BOOL) -> windows_core::Result<()> {
        if lock.as_bool() {
            crate::dll_add_ref();
        } else {
            crate::dll_release();
        }
        Ok(())
    }
}

pub struct CopyHookHandler;

impl CopyHookHandler {
    /// Checks whether Scroll Lock is active. If active, bypass DeepNode Copy and fallback to Windows Explorer copy.
    pub fn is_scroll_lock_active() -> bool {
        unsafe {
            let state = GetKeyState(VK_SCROLL.0 as i32);
            (state & 1) != 0
        }
    }

    /// Handles file drag and drop / clipboard paste request.
    /// Returns true if intercepted and handed over to DeepNode Copy.
    pub fn should_intercept_operation() -> bool {
        if Self::is_scroll_lock_active() {
            tracing::info!("Scroll Lock is active: Bypassing DeepNode Copy, using standard Explorer handler.");
            return false;
        }
        true
    }
}
