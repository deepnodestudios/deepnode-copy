//! Drag & drop / paste interception, modelled on TeraCopy's `TeraCopy::DropHandler`.
//!
//! Explorer builds the same "drop menu" (Copy here / Move here / Create shortcuts here / Cancel)
//! for every drop onto a folder — left-drag, right-drag and Ctrl+V paste — and asks every
//! `Folder|Directory|Drive|lnkfile\shellex\DragDropHandlers` entry to extend it. For a right-drag the
//! menu is shown; otherwise Explorer silently runs whichever item is the menu default. So the
//! handler takes over a transfer by making its own item the default, exactly like TeraCopy:
//!   * `CMF_DEFAULTONLY` queries are ignored.
//!   * Explorer's own default (`DDIDM_COPY` = 1 / `DDIDM_MOVE` = 2) tells whether this is a copy or
//!     a move; anything else (shortcut, Alt-drag...) is left to Explorer.
//!   * A move inside one volume stays with Explorer (instant rename, nothing to copy).
//!   * Explorer then calls `InvokeCommand` on our default item instead of copying natively, and we
//!     open DeepNode Copy's choice window (Explorer / DeepNode Copy).

use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::Mutex;
use windows_core::{implement, Interface, GUID, PSTR, PWSTR};
use windows::Win32::Foundation::BOOL;
use windows::Win32::System::Com::{IDataObject, FORMATETC, TYMED_HGLOBAL, IPersist, IPersist_Impl};
use windows::Win32::System::Ole::ReleaseStgMedium;
use windows::Win32::System::Registry::HKEY;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    IShellExtInit, IShellExtInit_Impl, IContextMenu, IContextMenu_Impl,
    CMINVOKECOMMANDINFO, DragQueryFileW, HDROP, SHGetPathFromIDListW, SHGetPathFromIDListEx,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMenuDefaultItem, GET_MENU_DEFAULT_ITEM_FLAGS, HMENU, InsertMenuItemW, MENUITEMINFOW, MIIM_STRING, MIIM_ID, MIIM_STATE,
    MFS_ENABLED, MessageBoxW, MB_ICONWARNING, MB_ICONERROR, MB_OK, SetMenuDefaultItem,
};
use crate::copy_handler::CopyHookHandler;
use crate::util::get_exe_path;

pub const CLSID_DEEPNODE_DRAGDROP: GUID = GUID::from_u128(0x6e8f3c72_a2d9_4e61_8c9b_93e48f1a7c21);

/// Opens the DeepNode Copy choice window. Default item for left-drag drops and pastes.
const CMD_POPUP: u32 = 0;
/// Starts a DeepNode copy directly (right-drag menu).
const CMD_COPY: u32 = 1;
/// Starts a DeepNode move directly (right-drag menu).
const CMD_MOVE: u32 = 2;
const CMD_COUNT: u32 = 3;

/// Command IDs of Explorer's own drop menu items (shell32 DDIDM_*).
const DDIDM_COPY: u32 = 1;
const DDIDM_MOVE: u32 = 2;

const CMF_DEFAULTONLY: u32 = 0x1;

#[implement(IShellExtInit, IContextMenu, IPersist)]
pub struct DeepNodeDragDropHandler {
    target_folder: Mutex<Option<PathBuf>>,
    source_files: Mutex<Vec<PathBuf>>,
    /// "move" or "copy", decided from Explorer's default drop effect in QueryContextMenu.
    explorer_mode: Mutex<&'static str>,
}

impl DeepNodeDragDropHandler {
    pub fn new() -> Self {
        crate::dll_add_ref();
        Self {
            target_folder: Mutex::new(None),
            source_files: Mutex::new(Vec::new()),
            explorer_mode: Mutex::new("copy"),
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
            *self.explorer_mode.lock().unwrap_or_else(|e| e.into_inner()) = "copy";

            // If Scroll Lock is active, bypass DeepNode Copy immediately
            if !CopyHookHandler::should_intercept_operation() {
                return Ok(());
            }

            if !pidl.is_null() {
                *self.target_folder.lock().unwrap_or_else(|e| e.into_inner()) = path_from_pidl(pidl);
            }

            if let Some(data_obj) = pdtobj {
                *self.source_files.lock().unwrap_or_else(|e| e.into_inner()) = extract_files_from_data_object(data_obj);
            }

            let tgt = self.target_folder.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let srcs = self.source_files.lock().unwrap_or_else(|e| e.into_inner()).clone();
            crate::util::log_shell(&format!("IShellExtInit::Initialize: target={:?}, sources={:?}", tgt, srcs));

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
        u_flags: u32,
    ) -> Result<(), windows_core::Error> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if u_flags & CMF_DEFAULTONLY != 0 || !CopyHookHandler::should_intercept_operation() {
                return Ok(());
            }

            let tgt = self.target_folder.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let srcs = self.source_files.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let target = match tgt {
                Some(t) if !t.as_os_str().is_empty() && !srcs.is_empty() => t,
                _ => return Ok(()),
            };

            // Explorer has already set its own default (copy or move) before handlers are queried.
            let explorer_default = unsafe { GetMenuDefaultItem(hmenu, 0, GET_MENU_DEFAULT_ITEM_FLAGS(0)) };

            let items = [
                (CMD_POPUP, "DeepNode Copy..."),
                (CMD_COPY, "DeepNode Copy here"),
                (CMD_MOVE, "DeepNode Move here"),
            ];
            for (idx, (cmd_id, title)) in items.into_iter().enumerate() {
                let mut title_wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
                let mii = MENUITEMINFOW {
                    cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
                    fMask: MIIM_STRING | MIIM_ID | MIIM_STATE,
                    fState: MFS_ENABLED,
                    wID: id_cmd_first + cmd_id,
                    dwTypeData: PWSTR(title_wide.as_mut_ptr()),
                    ..Default::default()
                };
                unsafe {
                    if let Err(e) = InsertMenuItemW(hmenu, index_menu + idx as u32, true, &mii) {
                        crate::util::log_shell(&format!("QueryContextMenu: InsertMenuItemW failed for item {}: {}", idx, e));
                        return Ok(());
                    }
                }
            }

            let (mode, take_over_eligible) = match explorer_default {
                DDIDM_COPY => (Some("copy"), true),
                // Same-volume moves stay with Explorer (instant rename); still remember the
                // intended mode in case the user explicitly picks our popup item.
                DDIDM_MOVE => (Some("move"), !srcs.iter().all(|s| same_volume(s, &target))),
                _ => (None, false),
            };

            let mut take_over = false;
            if let Some(mode) = mode {
                *self.explorer_mode.lock().unwrap_or_else(|e| e.into_inner()) = mode;
                if take_over_eligible {
                    take_over = unsafe { SetMenuDefaultItem(hmenu, id_cmd_first + CMD_POPUP, 0) }.is_ok();
                }
            }

            crate::util::log_shell(&format!(
                "QueryContextMenu: u_flags=0x{:X}, explorer_default={}, mode={:?}, take_over={}, now_default={}, target={:?}, sources={}",
                u_flags,
                explorer_default as i32,
                mode,
                take_over,
                unsafe { GetMenuDefaultItem(hmenu, 0, GET_MENU_DEFAULT_ITEM_FLAGS(0)) } as i32,
                target,
                srcs.len()
            ));

            // MAKE_HRESULT(SEVERITY_SUCCESS, 0, number of command IDs used)
            Err(windows_core::Error::from_hresult(windows_core::HRESULT(CMD_COUNT as i32)))
        }))
        .unwrap_or_else(|_| Err(windows_core::Error::from(windows::Win32::Foundation::E_FAIL)))
    }

    fn InvokeCommand(&self, pici: *const CMINVOKECOMMANDINFO) -> Result<(), windows_core::Error> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if pici.is_null() {
                return Ok(());
            }

            // A string verb (HIWORD != 0) is not one of ours.
            let verb = unsafe { (*pici).lpVerb.0 as usize };
            crate::util::log_shell(&format!("IContextMenu::InvokeCommand: verb={}", verb));
            if verb >= CMD_COUNT as usize {
                return Err(windows_core::Error::from(windows::Win32::Foundation::E_INVALIDARG));
            }

            let hwnd = unsafe { (*pici).hwnd };
            let sources = self.source_files.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if sources.is_empty() {
                return Ok(());
            }

            let target_path = match self.target_folder.lock().unwrap_or_else(|e| e.into_inner()).clone() {
                Some(t) if !t.as_os_str().is_empty() => t,
                _ => {
                    show_message(
                        hwnd,
                        "Hedef klasör belirlenemedi veya desteklenmeyen bir konuma bırakıldı.\nDosyalarınızın güvenliği için aktarım iptal edildi.\n\n(Target folder could not be resolved. Transfer cancelled for safety.)",
                        MB_ICONWARNING | MB_OK,
                    );
                    return Ok(());
                }
            };

            let list_file = match write_list_file(&sources) {
                Ok(f) => f,
                Err(e) => {
                    show_message(
                        hwnd,
                        &format!("Geçici aktarım listesi oluşturulamadı:\n{}\n\n(Failed to create temporary file list)", e),
                        MB_ICONERROR | MB_OK,
                    );
                    return Err(windows_core::Error::from(windows::Win32::Foundation::E_FAIL));
                }
            };

            let exe = get_exe_path();
            let mut cmd = std::process::Command::new(&exe);
            if let Some(parent) = exe.parent() {
                cmd.current_dir(parent);
            }
            let list_arg = format!("*{}", list_file.display());
            match verb as u32 {
                CMD_POPUP => {
                    let mode = *self.explorer_mode.lock().unwrap_or_else(|e| e.into_inner());
                    cmd.arg("--popup");
                    // Like TeraCopy, the chooser opens where the mouse is at drop/paste time.
                    // Explorer is per-monitor DPI aware, so this is a physical screen point.
                    let mut pt = windows::Win32::Foundation::POINT::default();
                    if unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt) }.is_ok() {
                        cmd.arg("--pos").arg(format!("{},{}", pt.x, pt.y));
                    }
                    cmd.arg("--mode").arg(mode).arg(&list_arg).arg(&target_path);
                }
                CMD_COPY => {
                    cmd.arg("copy").arg(&list_arg).arg(&target_path);
                }
                _ => {
                    cmd.arg("move").arg(&list_arg).arg(&target_path);
                }
            }

            crate::util::log_shell(&format!("InvokeCommand: launching {:?} {:?}", exe, cmd.get_args().collect::<Vec<_>>()));

            let spawned = cmd.spawn().map(|child| {
                // Explorer owns the foreground; let the popup take it (instead of the app forcing it).
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(child.id());
                }
            });
            if let Err(e) = spawned {
                crate::util::log_shell(&format!("InvokeCommand: failed to spawn {:?}: {}", exe, e));
                let _ = std::fs::remove_file(&list_file);
                show_message(
                    hwnd,
                    &format!(
                        "DeepNode Copy uygulaması başlatılamadı:\n{}\nYürütülebilir yol: {}\n\n(Failed to launch DeepNode Copy)",
                        e,
                        exe.display()
                    ),
                    MB_ICONERROR | MB_OK,
                );
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
        Err(windows_core::Error::from(windows::Win32::Foundation::E_NOTIMPL))
    }
}

impl IPersist_Impl for DeepNodeDragDropHandler_Impl {
    fn GetClassID(&self) -> windows_core::Result<windows_core::GUID> {
        Ok(CLSID_DEEPNODE_DRAGDROP)
    }
}

/// Resolves a folder PIDL to a filesystem path, supporting long paths (> 260 characters).
fn path_from_pidl(pidl: *const ITEMIDLIST) -> Option<PathBuf> {
    let mut buf = vec![0u16; 32768];
    let resolved = unsafe {
        SHGetPathFromIDListEx(pidl, &mut buf, windows::Win32::UI::Shell::GPFIDL_DEFAULT).as_bool()
            || SHGetPathFromIDListW(pidl, (&mut buf[..260]).try_into().unwrap()).as_bool()
    };
    if !resolved {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
    let path = String::from_utf16_lossy(&buf[..len]);
    let trimmed = path.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

fn extract_files_from_data_object(pdtobj: &IDataObject) -> Vec<PathBuf> {
    let formatetc = FORMATETC {
        cfFormat: 15, // CF_HDROP
        ptd: std::ptr::null_mut(),
        dwAspect: 1,  // DVASPECT_CONTENT
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };

    unsafe {
        if let Ok(mut medium) = pdtobj.GetData(&formatetc) {
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

            ReleaseStgMedium(&mut medium);
            files
        } else {
            Vec::new()
        }
    }
}

/// Volume identity of a path: drive letter or UNC server\share, lowercased.
fn volume_key(path: &Path) -> Option<String> {
    match path.components().next()? {
        Component::Prefix(p) => Some(match p.kind() {
            Prefix::Disk(d) | Prefix::VerbatimDisk(d) => (d as char).to_ascii_lowercase().to_string(),
            Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => format!(
                r"\\{}\{}",
                server.to_string_lossy().to_lowercase(),
                share.to_string_lossy().to_lowercase()
            ),
            _ => p.as_os_str().to_string_lossy().to_lowercase(),
        }),
        _ => None,
    }
}

pub(crate) fn same_volume(a: &Path, b: &Path) -> bool {
    matches!((volume_key(a), volume_key(b)), (Some(x), Some(y)) if x == y)
}

fn write_list_file(sources: &[PathBuf]) -> std::io::Result<PathBuf> {
    let list_file = std::env::temp_dir().join(format!("deepnode_list_{}.tmp", uuid::Uuid::new_v4()));
    let mut content = String::new();
    for src in sources {
        content.push_str(&src.to_string_lossy());
        content.push('\n');
    }
    std::fs::write(&list_file, content)?;
    Ok(list_file)
}

fn show_message(hwnd: windows::Win32::Foundation::HWND, text: &str, style: windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE) {
    let msg: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let title: Vec<u16> = "DeepNode Copy\0".encode_utf16().collect();
    unsafe {
        let _ = MessageBoxW(
            hwnd,
            windows::core::PCWSTR(msg.as_ptr()),
            windows::core::PCWSTR(title.as_ptr()),
            style,
        );
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
