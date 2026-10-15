use std::path::PathBuf;
use windows_core::Interface;
use windows::core::{implement, GUID, PWSTR};
use windows::Win32::Foundation::{BOOL, E_NOTIMPL, E_OUTOFMEMORY, CLASS_E_NOAGGREGATION};
use windows::Win32::System::Com::{CoTaskMemAlloc, IClassFactory, IClassFactory_Impl, IBindCtx};
use windows::Win32::UI::Shell::{
    IEnumExplorerCommand, IExplorerCommand, IExplorerCommand_Impl, IShellItemArray, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
use crate::util::get_exe_path;
use crate::copy_handler::CopyHookHandler;

pub const CLSID_DEEPNODE_COMMAND: GUID = GUID::from_u128(0x5e8f3c72_a2d9_4e61_8c9b_93e48f1a7c20);

fn to_cotask_pwstr(s: &str) -> Result<PWSTR, windows_core::Error> {
    let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = wide.len() * 2;
    unsafe {
        let ptr = CoTaskMemAlloc(bytes) as *mut u16;
        if ptr.is_null() {
            return Err(windows_core::Error::from(E_OUTOFMEMORY));
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        Ok(PWSTR(ptr))
    }
}

#[implement(IExplorerCommand)]
pub struct DeepNodeExplorerCommand;

impl DeepNodeExplorerCommand {
    pub fn new() -> Self {
        crate::dll_add_ref();
        Self
    }
}

impl Default for DeepNodeExplorerCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DeepNodeExplorerCommand {
    fn drop(&mut self) {
        crate::dll_release();
    }
}

impl IExplorerCommand_Impl for DeepNodeExplorerCommand_Impl {
    fn GetTitle(&self, _items: Option<&IShellItemArray>) -> Result<PWSTR, windows_core::Error> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            to_cotask_pwstr("DeepNode Copy...")
        }))
        .unwrap_or_else(|_| Err(windows_core::Error::from(windows::Win32::Foundation::E_FAIL)))
    }

    fn GetIcon(&self, _items: Option<&IShellItemArray>) -> Result<PWSTR, windows_core::Error> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let exe = get_exe_path();
            to_cotask_pwstr(&format!("{},0", exe.display()))
        }))
        .unwrap_or_else(|_| Err(windows_core::Error::from(windows::Win32::Foundation::E_FAIL)))
    }

    fn GetToolTip(&self, _items: Option<&IShellItemArray>) -> Result<PWSTR, windows_core::Error> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            to_cotask_pwstr("DeepNode Copy - High performance file transfer & verify")
        }))
        .unwrap_or_else(|_| Err(windows_core::Error::from(windows::Win32::Foundation::E_FAIL)))
    }

    fn GetCanonicalName(&self) -> Result<GUID, windows_core::Error> {
        Ok(CLSID_DEEPNODE_COMMAND)
    }

    fn GetState(&self, _items: Option<&IShellItemArray>, _ok_to_be_slow: BOOL) -> Result<u32, windows_core::Error> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // If Scroll Lock is active, disable the command to allow Explorer bypass
            if !CopyHookHandler::should_intercept_operation() {
                return Ok(1); // 1 = ECS_DISABLED
            }
            // 0 = ECS_ENABLED
            Ok(0)
        }))
        .unwrap_or_else(|_| Ok(0))
    }

    fn Invoke(&self, items: Option<&IShellItemArray>, _bind_ctx: Option<&IBindCtx>) -> Result<(), windows_core::Error> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Check Scroll Lock bypass before launching
            if !CopyHookHandler::should_intercept_operation() {
                return Ok(());
            }

            let mut paths: Vec<String> = Vec::new();
            if let Some(item_array) = items {
                unsafe {
                    if let Ok(count) = item_array.GetCount() {
                        for i in 0..count {
                            if let Ok(item) = item_array.GetItemAt(i) {
                                if let Ok(display_name) = item.GetDisplayName(SIGDN_FILESYSPATH) {
                                    if let Ok(s) = display_name.to_string() {
                                        paths.push(s);
                                    }
                                    windows::Win32::System::Com::CoTaskMemFree(Some(display_name.as_ptr() as _));
                                }
                            }
                        }
                    }
                }
            }

            let exe = get_exe_path();
            let mut cmd = std::process::Command::new(&exe);
            if let Some(parent) = exe.parent() {
                cmd.current_dir(parent);
            }

            let mut temp_list_to_clean: Option<PathBuf> = None;

            if !paths.is_empty() {
                let total_len: usize = paths.iter().map(|p| p.len() + 3).sum();
                // If multiple files or long arguments approaching Win32 32,767 char limit,
                // write to a temporary list file with unique UUID instead of individual args
                if paths.len() > 1 || total_len > 2000 {
                    let temp_dir = std::env::temp_dir();
                    let unique_id = uuid::Uuid::new_v4();
                    let list_file = temp_dir.join(format!("deepnode_list_{}.tmp", unique_id));
                    let mut list_content = String::new();
                    for p in &paths {
                        list_content.push_str(p);
                        list_content.push('\n');
                    }
                    if let Err(e) = std::fs::write(&list_file, list_content) {
                        tracing::error!("Failed to write context menu temporary list file {:?}: {}", list_file, e);
                        let msg: Vec<u16> = format!(
                            "Geçici aktarım listesi oluşturulamadı:\n{}\n\n(Failed to write context menu temporary file list)",
                            e
                        )
                        .encode_utf16()
                        .chain(std::iter::once(0))
                        .collect();
                        let title: Vec<u16> = "DeepNode Copy\0".encode_utf16().collect();
                        unsafe {
                            let _ = MessageBoxW(
                                windows::Win32::Foundation::HWND::default(),
                                windows::core::PCWSTR(msg.as_ptr()),
                                windows::core::PCWSTR(title.as_ptr()),
                                MB_ICONERROR | MB_OK,
                            );
                        }
                        return Err(windows_core::Error::from(windows::Win32::Foundation::E_FAIL));
                    }
                    cmd.arg(format!("*{}", list_file.display()));
                    temp_list_to_clean = Some(list_file);
                } else {
                    cmd.arg("--source");
                    for p in paths {
                        cmd.arg(p);
                    }
                }
            }

            if let Err(e) = cmd.spawn() {
                tracing::error!("Failed to launch DeepNode Copy from context menu: {}", e);
                if let Some(ref list) = temp_list_to_clean {
                    let _ = std::fs::remove_file(list);
                }
                let msg: Vec<u16> = format!(
                    "DeepNode Copy başlatılamadı:\n{}\nYürütülebilir yol: {}\n\n(Failed to launch DeepNode Copy)",
                    e, exe.display()
                )
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
                let title: Vec<u16> = "DeepNode Copy\0".encode_utf16().collect();
                unsafe {
                    let _ = MessageBoxW(
                        windows::Win32::Foundation::HWND::default(),
                        windows::core::PCWSTR(msg.as_ptr()),
                        windows::core::PCWSTR(title.as_ptr()),
                        MB_ICONERROR | MB_OK,
                    );
                }
                return Err(windows_core::Error::from(windows::Win32::Foundation::E_FAIL));
            }
            Ok(())
        }))
        .unwrap_or_else(|_| Err(windows_core::Error::from(windows::Win32::Foundation::E_FAIL)))
    }

    fn GetFlags(&self) -> Result<u32, windows_core::Error> {
        // 0 = ECF_DEFAULT
        Ok(0)
    }

    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand, windows_core::Error> {
        Err(windows_core::Error::from(E_NOTIMPL))
    }
}

#[implement(IClassFactory)]
pub struct DeepNodeClassFactory;

impl DeepNodeClassFactory {
    pub fn new() -> Self {
        crate::dll_add_ref();
        Self
    }
}

impl Default for DeepNodeClassFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DeepNodeClassFactory {
    fn drop(&mut self) {
        crate::dll_release();
    }
}

impl IClassFactory_Impl for DeepNodeClassFactory_Impl {
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
            let command: IExplorerCommand = DeepNodeExplorerCommand::new().into();
            command.query(iid, ppv).ok()
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
