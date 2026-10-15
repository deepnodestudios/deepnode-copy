use std::path::PathBuf;
use std::sync::Mutex;
use windows_core::{implement, Interface, GUID, PSTR, PWSTR};
use windows::Win32::Foundation::BOOL;
use windows::Win32::System::Com::{IDataObject, FORMATETC, TYMED_HGLOBAL};
use windows::Win32::System::Ole::ReleaseStgMedium;
use windows::Win32::System::Registry::HKEY;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    IShellExtInit, IShellExtInit_Impl, IContextMenu, IContextMenu_Impl,
    CMINVOKECOMMANDINFO, DragQueryFileW, HDROP, SHGetPathFromIDListW, SHGetPathFromIDListEx,
    SHFileOperationW, SHFILEOPSTRUCTW, FO_COPY, FOF_ALLOWUNDO, FOF_NOCONFIRMMKDIR,
};
use windows::Win32::UI::WindowsAndMessaging::{
    HMENU, InsertMenuItemW, MENUITEMINFOW, MIIM_STRING, MIIM_ID, MIIM_STATE,
    MFS_DEFAULT, MFS_ENABLED, MessageBoxW, MB_ICONWARNING, MB_ICONERROR, MB_OK,
};
use crate::copy_handler::CopyHookHandler;
use crate::util::get_exe_path;

pub const CLSID_DEEPNODE_DRAGDROP: GUID = GUID::from_u128(0x6e8f3c72_a2d9_4e61_8c9b_93e48f1a7c21);

const CMD_COPY: u32 = 0;
const CMD_REPLACE_OLDER: u32 = 1;
const CMD_MOVE: u32 = 2;
const CMD_EXPLORER: u32 = 3;

#[implement(IShellExtInit, IContextMenu)]
pub struct DeepNodeDragDropHandler {
    target_folder: Mutex<Option<PathBuf>>,
    source_files: Mutex<Vec<PathBuf>>,
}

impl DeepNodeDragDropHandler {
    pub fn new() -> Self {
        crate::dll_add_ref();
        Self {
            target_folder: Mutex::new(None),
            source_files: Mutex::new(Vec::new()),
        }
    }
}

impl Default for DeepNodeDragDropHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DeepNodeDragDropHandler {
    fn drop(&mut self) {
        crate::dll_release();
    }
}

impl IShellExtInit_Impl for DeepNodeDragDropHandler_Impl {
    fn Initialize(
        &self,
        pidl: *const ITEMIDLIST,
        pdtobj: Option<&IDataObject>,
        _hkeyprogid: HKEY,
    ) -> Result<(), windows_core::Error> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Always reset previous state to prevent leaking previous drag-and-drop operations
            *self.target_folder.lock().unwrap_or_else(|e| e.into_inner()) = None;
            self.source_files.lock().unwrap_or_else(|e| e.into_inner()).clear();

            // If Scroll Lock is active, bypass DeepNode Copy immediately
            if !CopyHookHandler::should_intercept_operation() {
                return Ok(());
            }

            // Extract Target Folder from PIDL supporting long paths (> 260 characters)
            if !pidl.is_null() {
                let mut target_buf = vec![0u16; 32768];
                unsafe {
                    let resolved = SHGetPathFromIDListEx(
                        pidl,
                        &mut target_buf,
                        windows::Win32::UI::Shell::GPFIDL_DEFAULT,
                    )
                    .as_bool();

                    if resolved {
                        let len = target_buf.iter().position(|&c| c == 0).unwrap_or(0);
                        if len > 0 {
                            let path = String::from_utf16_lossy(&target_buf[..len]);
                            let trimmed = path.trim();
                            if !trimmed.is_empty() {
                                *self.target_folder.lock().unwrap_or_else(|e| e.into_inner()) = Some(PathBuf::from(trimmed));
                            }
                        }
                    } else {
                        let mut fallback_buf = [0u16; 260];
                        if SHGetPathFromIDListW(pidl, &mut fallback_buf).as_bool() {
                            let len = fallback_buf.iter().position(|&c| c == 0).unwrap_or(0);
                            if len > 0 {
                                let path = String::from_utf16_lossy(&fallback_buf[..len]);
                                let trimmed = path.trim();
                                if !trimmed.is_empty() {
                                    *self.target_folder.lock().unwrap_or_else(|e| e.into_inner()) = Some(PathBuf::from(trimmed));
                                }
                            }
                        }
                    }
                }
            }

            // Extract Source Files from IDataObject (CF_HDROP = 15)
            if let Some(data_obj) = pdtobj {
                let formatetc = FORMATETC {
                    cfFormat: 15, // CF_HDROP
                    ptd: std::ptr::null_mut(),
                    dwAspect: 1,  // DVASPECT_CONTENT
                    lindex: -1,
                    tymed: TYMED_HGLOBAL.0 as u32,
                };

                unsafe {
                    if let Ok(mut medium) = data_obj.GetData(&formatetc) {
                        let hdrop = HDROP(medium.u.hGlobal.0);
                        let count = DragQueryFileW(hdrop, 0xFFFFFFFF, None);
                        let mut files = Vec::new();

                        for i in 0..count {
                            let len = DragQueryFileW(hdrop, i, None) as usize;
                            if len > 0 {
                                let mut buf = vec![0u16; len + 1];
                                DragQueryFileW(hdrop, i, Some(&mut buf));
                                let path = String::from_utf16_lossy(&buf[..len]);
                                files.push(PathBuf::from(path));
                            }
                        }

                        *self.source_files.lock().unwrap_or_else(|e| e.into_inner()) = files;
                        ReleaseStgMedium(&mut medium);
                    }
                }
            }

            Ok(())
        }))
        .unwrap_or_else(|_| Err(windows_core::Error::from(windows::Win32::Foundation::E_FAIL)))
    }
}

impl IContextMenu_Impl for DeepNodeDragDropHandler_Impl {
    fn QueryContextMenu(
        &self,
        hmenu: HMENU,
        index_menu: u32,
        id_cmd_first: u32,
        _id_cmd_last: u32,
        _u_flags: u32,
    ) -> Result<(), windows_core::Error> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Scroll Lock Check via CopyHookHandler: If active, don't show custom menu, fallback to standard Windows
            if !CopyHookHandler::should_intercept_operation() {
                return Ok(());
            }

            let items = [
                (CMD_COPY, "DeepNode Copy", true),
                (CMD_REPLACE_OLDER, "DeepNode Copy - replace older files", false),
                (CMD_MOVE, "DeepNode Move", false),
                (CMD_EXPLORER, "Explorer (Standart Gezgin)", false),
            ];

            for (idx, (cmd_id, title, is_default)) in items.into_iter().enumerate() {
                let current_index = index_menu + idx as u32;
                let mut title_wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
                let mii = MENUITEMINFOW {
                    cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
                    fMask: MIIM_STRING | MIIM_ID | MIIM_STATE,
                    fState: if is_default { MFS_DEFAULT | MFS_ENABLED } else { MFS_ENABLED },
                    wID: id_cmd_first + cmd_id,
                    dwTypeData: PWSTR(title_wide.as_mut_ptr()),
                    ..Default::default()
                };

                unsafe {
                    if let Err(e) = InsertMenuItemW(hmenu, current_index, true, &mii) {
                        tracing::error!("InsertMenuItemW failed for item {}: {}", idx, e);
                    }
                }
            }

            Ok(())
        }))
        .unwrap_or_else(|_| Err(windows_core::Error::from(windows::Win32::Foundation::E_FAIL)))
    }

    fn InvokeCommand(&self, pici: *const CMINVOKECOMMANDINFO) -> Result<(), windows_core::Error> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if pici.is_null() {
                return Ok(());
            }

            let cmd_offset = unsafe { (*pici).lpVerb.0 as usize };
            if cmd_offset > 3 {
                return Ok(());
            }

            let sources = self.source_files.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if sources.is_empty() {
                return Ok(());
            }

            let target = self.target_folder.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let target_path = match target {
                Some(t) if !t.as_os_str().is_empty() => t,
                _ => {
                    let hwnd = unsafe { (*pici).hwnd };
                    let msg: Vec<u16> = "Hedef klasör belirlenemedi veya desteklenmeyen bir konuma bırakıldı.\nDosyalarınızın güvenliği için aktarım iptal edildi.\n\n(Target folder could not be resolved. Transfer cancelled for safety.)\0"
                        .encode_utf16()
                        .collect();
                    let title: Vec<u16> = "DeepNode Copy\0".encode_utf16().collect();
                    unsafe {
                        let _ = MessageBoxW(
                            hwnd,
                            windows::core::PCWSTR(msg.as_ptr()),
                            windows::core::PCWSTR(title.as_ptr()),
                            MB_ICONWARNING | MB_OK,
                        );
                    }
                    return Ok(());
                }
            };

            // When "Explorer (Standart Gezgin)" is selected, invoke Windows native SHFileOperationW copy
            if cmd_offset == CMD_EXPLORER as usize {
                let mut from_wide: Vec<u16> = Vec::new();
                for src in &sources {
                    from_wide.extend(src.to_string_lossy().encode_utf16());
                    from_wide.push(0);
                }
                from_wide.push(0); // Double null-terminator

                let mut to_wide: Vec<u16> = target_path.to_string_lossy().encode_utf16().collect();
                to_wide.push(0);
                to_wide.push(0); // Double null-terminator

                unsafe {
                    let hwnd = (*pici).hwnd;
                    let mut op = SHFILEOPSTRUCTW {
                        hwnd,
                        wFunc: FO_COPY,
                        pFrom: windows::core::PCWSTR(from_wide.as_ptr()),
                        pTo: windows::core::PCWSTR(to_wide.as_ptr()),
                        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMMKDIR).0 as u16,
                        fAnyOperationsAborted: BOOL(0),
                        hNameMappings: std::ptr::null_mut(),
                        lpszProgressTitle: windows::core::PCWSTR::null(),
                    };
                    let res = SHFileOperationW(&mut op);
                    if res != 0 {
                        tracing::warn!("SHFileOperationW returned code: {}", res);
                    }
                }
                return Ok(());
            }

            let target_str = target_path.to_string_lossy().to_string();

            // Write sources to a temporary file list with globally unique UUID
            let temp_dir = std::env::temp_dir();
            let unique_id = uuid::Uuid::new_v4();
            let list_file = temp_dir.join(format!("deepnode_list_{}.tmp", unique_id));
            let mut list_content = String::new();
            for src in &sources {
                list_content.push_str(&src.to_string_lossy());
                list_content.push('\n');
            }
            if let Err(e) = std::fs::write(&list_file, list_content) {
                tracing::error!("Failed to write temporary list file {:?}: {}", list_file, e);
                let hwnd = unsafe { (*pici).hwnd };
                let msg: Vec<u16> = format!(
                    "Geçici aktarım listesi oluşturulamadı:\n{}\n\n(Failed to create temporary file list)",
                    e
                )
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
                let title: Vec<u16> = "DeepNode Copy\0".encode_utf16().collect();
                unsafe {
                    let _ = MessageBoxW(
                        hwnd,
                        windows::core::PCWSTR(msg.as_ptr()),
                        windows::core::PCWSTR(title.as_ptr()),
                        MB_ICONERROR | MB_OK,
                    );
                }
                return Err(windows_core::Error::from(windows::Win32::Foundation::E_FAIL));
            }

            let exe = get_exe_path();
            let mut cmd = std::process::Command::new(&exe);
            if let Some(parent) = exe.parent() {
                cmd.current_dir(parent);
            }

            match cmd_offset as u32 {
                CMD_COPY => {
                    cmd.arg("copy")
                        .arg(format!("*{}", list_file.display()))
                        .arg(&target_str);
                }
                CMD_REPLACE_OLDER => {
                    cmd.arg("copy")
                        .arg(format!("*{}", list_file.display()))
                        .arg(&target_str)
                        .arg("/OverwriteOlder");
                }
                CMD_MOVE => {
                    cmd.arg("move")
                        .arg(format!("*{}", list_file.display()))
                        .arg(&target_str);
                }
                _ => {
                    let _ = std::fs::remove_file(&list_file);
                    return Ok(());
                }
            }

            if let Err(e) = cmd.spawn() {
                tracing::error!("Failed to spawn process {:?}: {}", exe, e);
                let _ = std::fs::remove_file(&list_file);
                let hwnd = unsafe { (*pici).hwnd };
                let msg: Vec<u16> = format!(
                    "DeepNode Copy uygulaması başlatılamadı:\n{}\nYürütülebilir yol: {}\n\n(Failed to launch DeepNode Copy)",
                    e, exe.display()
                )
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
                let title: Vec<u16> = "DeepNode Copy\0".encode_utf16().collect();
                unsafe {
                    let _ = MessageBoxW(
                        hwnd,
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

    fn GetCommandString(
        &self,
        _id_cmd: usize,
        _u_type: u32,
        _p_reserved: *const u32,
        _psz_name: PSTR,
        _cch_max: u32,
    ) -> Result<(), windows_core::Error> {
        Ok(())
    }
}

#[implement(windows::Win32::System::Com::IClassFactory)]
pub struct DeepNodeDragDropClassFactory;

impl DeepNodeDragDropClassFactory {
    pub fn new() -> Self {
        crate::dll_add_ref();
        Self
    }
}

impl Default for DeepNodeDragDropClassFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DeepNodeDragDropClassFactory {
    fn drop(&mut self) {
        crate::dll_release();
    }
}

impl windows::Win32::System::Com::IClassFactory_Impl for DeepNodeDragDropClassFactory_Impl {
    fn CreateInstance(
        &self,
        outer: Option<&windows::core::IUnknown>,
        iid: *const GUID,
        ppv: *mut *mut std::ffi::c_void,
    ) -> windows_core::Result<()> {
        if outer.is_some() {
            return Err(windows_core::Error::from(windows::Win32::Foundation::CLASS_E_NOAGGREGATION));
        }
        unsafe {
            let handler: IShellExtInit = DeepNodeDragDropHandler::new().into();
            handler.query(iid, ppv).ok()
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
