use std::path::PathBuf;
use windows_core::Interface;
use windows::core::{implement, GUID, PWSTR};
use windows::Win32::Foundation::{BOOL, E_NOTIMPL, E_OUTOFMEMORY, CLASS_E_NOAGGREGATION};
use windows::Win32::System::Com::{CoTaskMemAlloc, IClassFactory, IClassFactory_Impl, IBindCtx};
use windows::Win32::UI::Shell::{
    IEnumExplorerCommand, IExplorerCommand, IExplorerCommand_Impl, IShellItemArray, SIGDN_FILESYSPATH,
};

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

fn get_dll_dir() -> Option<PathBuf> {
    use windows::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS};
    use windows::Win32::Foundation::HMODULE;
    unsafe {
        let mut hmodule = HMODULE::default();
        let addr = get_dll_dir as *const () as *const u16;
        if GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
            windows::core::PCWSTR(addr),
            &mut hmodule,
        ).is_ok() {
            let mut buf = [0u16; 1024];
            let len = GetModuleFileNameW(hmodule, &mut buf) as usize;
            if len > 0 {
                let dll_path = PathBuf::from(String::from_utf16_lossy(&buf[..len]));
                return dll_path.parent().map(|p| p.to_path_buf());
            }
        }
    }
    None
}

fn get_exe_path() -> PathBuf {
    if let Some(dll_dir) = get_dll_dir() {
        let candidate = dll_dir.join("DeepNodeCopy.exe");
        if candidate.exists() {
            return candidate;
        }
        let parent_candidate = dll_dir.join("..").join("DeepNodeCopy.exe");
        if parent_candidate.exists() {
            return parent_candidate;
        }
    }
    if let Ok(current) = std::env::current_exe() {
        if let Some(dir) = current.parent() {
            let candidate = dir.join("DeepNodeCopy.exe");
            if candidate.exists() {
                return candidate;
            }
        }
    }
    PathBuf::from(r"E:\Projeler\DeepNode Copy\bin\DeepNodeCopy.exe")
}

#[implement(IExplorerCommand)]
pub struct DeepNodeExplorerCommand;

impl IExplorerCommand_Impl for DeepNodeExplorerCommand_Impl {
    fn GetTitle(&self, _items: Option<&IShellItemArray>) -> Result<PWSTR, windows_core::Error> {
        to_cotask_pwstr("DeepNode Copy...")
    }

    fn GetIcon(&self, _items: Option<&IShellItemArray>) -> Result<PWSTR, windows_core::Error> {
        let exe = get_exe_path();
        to_cotask_pwstr(&format!("{},0", exe.display()))
    }

    fn GetToolTip(&self, _items: Option<&IShellItemArray>) -> Result<PWSTR, windows_core::Error> {
        to_cotask_pwstr("DeepNode Copy - High performance file transfer & verify")
    }

    fn GetCanonicalName(&self) -> Result<GUID, windows_core::Error> {
        Ok(CLSID_DEEPNODE_COMMAND)
    }

    fn GetState(&self, _items: Option<&IShellItemArray>, _ok_to_be_slow: BOOL) -> Result<u32, windows_core::Error> {
        // 0 = ECS_ENABLED
        Ok(0)
    }

    fn Invoke(&self, items: Option<&IShellItemArray>, _bind_ctx: Option<&IBindCtx>) -> Result<(), windows_core::Error> {
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
        if !paths.is_empty() {
            cmd.arg("--source");
            for p in paths {
                cmd.arg(p);
            }
        }
        let _ = cmd.spawn();
        Ok(())
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
            let command: IExplorerCommand = DeepNodeExplorerCommand.into();
            command.query(iid, ppv).ok()
        }
    }

    fn LockServer(&self, _lock: BOOL) -> windows_core::Result<()> {
        Ok(())
    }
}
