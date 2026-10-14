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
    CMINVOKECOMMANDINFO, DragQueryFileW, HDROP, SHGetPathFromIDListW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    HMENU, InsertMenuItemW, MENUITEMINFOW, MIIM_STRING, MIIM_ID, MIIM_STATE,
    MFS_DEFAULT, MFS_ENABLED,
};

pub const CLSID_DEEPNODE_DRAGDROP: GUID = GUID::from_u128(0x6e8f3c72_a2d9_4e61_8c9b_93e48f1a7c21);

const CMD_COPY: u32 = 0;
const CMD_REPLACE_OLDER: u32 = 1;
const CMD_MOVE: u32 = 2;
const CMD_EXPLORER: u32 = 3;

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

#[implement(IShellExtInit, IContextMenu)]
pub struct DeepNodeDragDropHandler {
    target_folder: Mutex<Option<PathBuf>>,
    source_files: Mutex<Vec<PathBuf>>,
}

impl DeepNodeDragDropHandler {
    pub fn new() -> Self {
        Self {
            target_folder: Mutex::new(None),
            source_files: Mutex::new(Vec::new()),
        }
    }
}

impl IShellExtInit_Impl for DeepNodeDragDropHandler_Impl {
    fn Initialize(
        &self,
        pidl: *const ITEMIDLIST,
        pdtobj: Option<&IDataObject>,
        _hkeyprogid: HKEY,
    ) -> Result<(), windows_core::Error> {
        // Extract Target Folder from PIDL
        if !pidl.is_null() {
            let mut target_buf = [0u16; 260];
            unsafe {
                if SHGetPathFromIDListW(pidl, &mut target_buf).as_bool() {
                    let len = target_buf.iter().position(|&c| c == 0).unwrap_or(target_buf.len());
                    let path = String::from_utf16_lossy(&target_buf[..len]);
                    if !path.is_empty() {
                        *self.target_folder.lock().unwrap() = Some(PathBuf::from(path));
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

                    *self.source_files.lock().unwrap() = files;
                    ReleaseStgMedium(&mut medium);
                }
            }
        }

        Ok(())
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
        // Scroll Lock Check: If active, don't show custom menu, fallback to standard Windows
        unsafe {
            let scroll_state = windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x91); // VK_SCROLL
            if (scroll_state & 1) != 0 {
                return Ok(());
            }
        }

        let items = [
            (CMD_COPY, "DeepNode Copy", true),
            (CMD_REPLACE_OLDER, "DeepNode Copy - replace older files", false),
            (CMD_MOVE, "DeepNode Move", false),
            (CMD_EXPLORER, "Explorer (Standart Gezgin)", false),
        ];

        let mut current_index = index_menu;
        for (cmd_id, title, is_default) in items {
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
                let _ = InsertMenuItemW(hmenu, current_index, true, &mii);
            }
            current_index += 1;
        }

        Ok(())
    }

    fn InvokeCommand(&self, pici: *const CMINVOKECOMMANDINFO) -> Result<(), windows_core::Error> {
        if pici.is_null() {
            return Ok(());
        }

        let cmd_offset = unsafe { (*pici).lpVerb.0 as usize };
        if cmd_offset > 3 {
            return Ok(());
        }

        let target = self.target_folder.lock().unwrap().clone();
        let sources = self.source_files.lock().unwrap().clone();

        if sources.is_empty() {
            return Ok(());
        }

        let target_str = target.map(|t| t.to_string_lossy().to_string()).unwrap_or_else(|| {
            std::env::var("USERPROFILE").map(|u| format!("{}\\Desktop", u)).unwrap_or_else(|_| "C:\\".to_string())
        });

        if cmd_offset == CMD_EXPLORER as usize {
            // Explorer option: don't intercept, let Explorer do it
            return Ok(());
        }

        // Write sources to a temporary file list
        let temp_dir = std::env::temp_dir();
        let list_file = temp_dir.join(format!("deepnode_list_{}.tmp", std::process::id()));
        let mut list_content = String::new();
        for src in &sources {
            list_content.push_str(&src.to_string_lossy());
            list_content.push('\n');
        }
        let _ = std::fs::write(&list_file, list_content);

        let exe = get_exe_path();
        let mut cmd = std::process::Command::new(&exe);

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
            _ => return Ok(()),
        }

        let _ = cmd.spawn();
        Ok(())
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

    fn LockServer(&self, _lock: BOOL) -> windows_core::Result<()> {
        Ok(())
    }
}

