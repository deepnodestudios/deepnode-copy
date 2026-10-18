use windows::core::{HRESULT, PCWSTR};
use windows::Win32::Foundation::{E_FAIL, S_OK};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteKeyW, RegDeleteTreeW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use crate::util::get_dll_path;

pub const CLSID_COMMAND_STR: &str = "{5E8F3C72-A2D9-4E61-8C9B-93E48F1A7C20}";
pub const CLSID_DRAGDROP_STR: &str = "{6E8F3C72-A2D9-4E61-8C9B-93E48F1A7C21}";
pub const CLSID_COPYHOOK_STR: &str = "{8F3A2C71-9B4E-4D26-A5F0-6E1D2B7C9A83}";

fn set_reg_string(hkey: HKEY, value_name: &str, data: &str) -> bool {
    unsafe {
        let val_w: Vec<u16> = if value_name.is_empty() {
            vec![0]
        } else {
            value_name.encode_utf16().chain(std::iter::once(0)).collect()
        };
        let val_ptr = if value_name.is_empty() {
            PCWSTR::null()
        } else {
            PCWSTR(val_w.as_ptr())
        };

        let data_w: Vec<u16> = data.encode_utf16().chain(std::iter::once(0)).collect();
        let slice = std::slice::from_raw_parts(data_w.as_ptr() as *const u8, data_w.len() * 2);

        RegSetValueExW(
            hkey,
            val_ptr,
            0,
            REG_SZ,
            Some(slice),
        )
        .is_ok()
    }
}

fn create_or_open_key(root: HKEY, subkey: &str) -> Option<HKEY> {
    unsafe {
        let subkey_w: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
        let mut out_hkey = HKEY::default();
        let res = RegCreateKeyExW(
            root,
            PCWSTR(subkey_w.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut out_hkey,
            None,
        );
        if res.is_ok() {
            Some(out_hkey)
        } else {
            None
        }
    }
}

fn set_key_default_value(root: HKEY, subkey: &str, value: &str) -> bool {
    if let Some(hkey) = create_or_open_key(root, subkey) {
        let ok = set_reg_string(hkey, "", value);
        unsafe { let _ = RegCloseKey(hkey); }
        ok
    } else {
        false
    }
}

fn set_key_named_value(root: HKEY, subkey: &str, name: &str, value: &str) -> bool {
    if let Some(hkey) = create_or_open_key(root, subkey) {
        let ok = set_reg_string(hkey, name, value);
        unsafe { let _ = RegCloseKey(hkey); }
        ok
    } else {
        false
    }
}

/// Keys whose `shellex\DragDropHandlers` list our handler (the same set TeraCopy registers).
/// Explorer consults these for every drop and paste onto a folder, drive or folder shortcut.
const DRAGDROP_TARGETS: &[&str] = &["Folder", "Directory", "Drive", "lnkfile"];
/// Registered by older builds; removed on (re)registration.
const LEGACY_DRAGDROP_TARGETS: &[&str] = &["*", r"Directory\Background", r"Drive\Background", "DesktopBackground"];
/// Older builds replaced Explorer's own `shellex\DropHandler` on these keys with our CLSID.
const LEGACY_DROPHANDLER_TARGETS: &[&str] = &["Directory", "Folder", r"Directory\Background", "Drive"];

fn query_key_default_value(root: HKEY, subkey: &str) -> Option<String> {
    unsafe {
        let subkey_w: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(root, PCWSTR(subkey_w.as_ptr()), 0, KEY_READ, &mut hkey).is_err() {
            return None;
        }
        let mut buf = [0u16; 256];
        let mut len = (buf.len() * 2) as u32;
        let res = RegQueryValueExW(hkey, PCWSTR::null(), None, None, Some(buf.as_mut_ptr() as *mut u8), Some(&mut len));
        let _ = RegCloseKey(hkey);
        if res.is_err() {
            return None;
        }
        let chars = (len as usize / 2).min(buf.len());
        Some(String::from_utf16_lossy(&buf[..chars]).trim_end_matches('\0').to_string())
    }
}

/// Removes `shellex\DropHandler` entries that point at our CLSID, restoring Explorer's own drop
/// target (which is what runs the DragDropHandlers chain). Values owned by others are kept.
fn remove_legacy_drop_handlers(root: HKEY, prefix: &str) {
    for target in LEGACY_DROPHANDLER_TARGETS {
        let dh_key = format!(r"{}{}\shellex\DropHandler", prefix, target);
        delete_key_tree(root, &format!(r"{}\DeepNodeCopy", dh_key));
        let ours = query_key_default_value(root, &dh_key)
            .map(|v| v.eq_ignore_ascii_case(CLSID_DRAGDROP_STR))
            .unwrap_or(false);
        if ours {
            delete_key_tree(root, &dh_key);
        }
    }
}

fn delete_key_tree(root: HKEY, subkey: &str) {
    unsafe {
        let subkey_w: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
        // Try RegDeleteTreeW first, if not delete simple key
        if RegDeleteTreeW(root, PCWSTR(subkey_w.as_ptr())).is_err() {
            let _ = RegDeleteKeyW(root, PCWSTR(subkey_w.as_ptr()));
        }
    }
}

pub fn register_server() -> HRESULT {
    let dll_path = match get_dll_path() {
        Some(p) => p.to_string_lossy().to_string(),
        None => return E_FAIL,
    };

    // Target roots: HKCU\Software\Classes (user, always works) and HKEY_CLASSES_ROOT (system-wide if elevated)
    let roots = [
        (HKEY_CURRENT_USER, r"Software\Classes\"),
        (HKEY_CLASSES_ROOT, ""),
    ];

    let mut registered = false;
    for (root, prefix) in roots {
        // 1. Context Menu CLSID
        let clsid_cmd_key = format!("{}CLSID\\{}", prefix, CLSID_COMMAND_STR);
        let inproc_cmd_key = format!("{}\\InprocServer32", clsid_cmd_key);
        if set_key_default_value(root, &clsid_cmd_key, "DeepNode Copy Context Menu")
            && set_key_default_value(root, &inproc_cmd_key, &dll_path)
            && set_key_named_value(root, &inproc_cmd_key, "ThreadingModel", "Apartment")
        {
            // 2. Drag & Drop CLSID
            let clsid_dd_key = format!("{}CLSID\\{}", prefix, CLSID_DRAGDROP_STR);
            let inproc_dd_key = format!("{}\\InprocServer32", clsid_dd_key);
            set_key_default_value(root, &clsid_dd_key, "DeepNode Copy Drag Drop Handler");
            set_key_default_value(root, &inproc_dd_key, &dll_path);
            set_key_named_value(root, &inproc_dd_key, "ThreadingModel", "Apartment");

            // 3. Copy Hook CLSID
            let clsid_ch_key = format!("{}CLSID\\{}", prefix, CLSID_COPYHOOK_STR);
            let inproc_ch_key = format!("{}\\InprocServer32", clsid_ch_key);
            set_key_default_value(root, &clsid_ch_key, "DeepNode Copy Hook Handler");
            set_key_default_value(root, &inproc_ch_key, &dll_path);
            set_key_named_value(root, &inproc_ch_key, "ThreadingModel", "Apartment");

            // 4. Register DragDropHandlers
            for target in DRAGDROP_TARGETS {
                let dd_key = format!(r"{}{}\shellex\DragDropHandlers\DeepNodeCopy", prefix, target);
                set_key_default_value(root, &dd_key, CLSID_DRAGDROP_STR);
            }
            for target in LEGACY_DRAGDROP_TARGETS {
                let dd_key = format!(r"{}{}\shellex\DragDropHandlers\DeepNodeCopy", prefix, target);
                delete_key_tree(root, &dd_key);
            }

            // 5. Register CopyHookHandlers
            for target in &["Directory", "Folder"] {
                let ch_key = format!(r"{}{}\shellex\CopyHookHandlers\DeepNodeCopy", prefix, target);
                set_key_default_value(root, &ch_key, CLSID_COPYHOOK_STR);
            }

            // 6. Register ContextMenuHandlers (classic Explorer fallback)
            for target in &["*", "Directory", "Folder", "Directory\\Background", "Drive", "Drive\\Background", "DesktopBackground"] {
                let cm_key = format!(r"{}{}\shellex\ContextMenuHandlers\DeepNodeCopy", prefix, target);
                set_key_default_value(root, &cm_key, CLSID_COMMAND_STR);
            }

            // 7. Undo the DropHandler override of older builds: it replaced Explorer's folder drop
            //    target, so the DragDropHandlers chain (and our default item) never ran.
            remove_legacy_drop_handlers(root, prefix);

            // 8. Register in Shell Extensions Approved
            let approved_path = r"Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Approved";
            for app_root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
                set_key_named_value(app_root, approved_path, CLSID_COMMAND_STR, "DeepNode Copy Context Menu Extension");
                set_key_named_value(app_root, approved_path, CLSID_DRAGDROP_STR, "DeepNode Copy Drag Drop Extension");
                set_key_named_value(app_root, approved_path, CLSID_COPYHOOK_STR, "DeepNode Copy Hook Extension");
            }

            registered = true;
        }
    }

    if registered {
        unsafe {
            windows::Win32::UI::Shell::SHChangeNotify(
                windows::Win32::UI::Shell::SHCNE_ASSOCCHANGED,
                windows::Win32::UI::Shell::SHCNF_IDLIST,
                None,
                None,
            );
        }
        S_OK
    } else {
        E_FAIL
    }
}

pub fn unregister_server() -> HRESULT {
    let roots = [
        (HKEY_CLASSES_ROOT, ""),
        (HKEY_CURRENT_USER, r"Software\Classes\"),
    ];

    for (root, prefix) in roots {
        // 1. Delete DragDropHandlers
        for target in DRAGDROP_TARGETS.iter().chain(LEGACY_DRAGDROP_TARGETS) {
            let dd_key = format!(r"{}{}\shellex\DragDropHandlers\DeepNodeCopy", prefix, target);
            delete_key_tree(root, &dd_key);
        }

        // 2. Delete CopyHookHandlers
        for target in &["Directory", "Folder"] {
            let ch_key = format!(r"{}{}\shellex\CopyHookHandlers\DeepNodeCopy", prefix, target);
            delete_key_tree(root, &ch_key);
        }

        // 3. Delete ContextMenuHandlers
        for target in &["*", "Directory", "Folder", "Directory\\Background", "Drive", "Drive\\Background", "DesktopBackground"] {
            let cm_key = format!(r"{}{}\shellex\ContextMenuHandlers\DeepNodeCopy", prefix, target);
            delete_key_tree(root, &cm_key);
        }

        // 4. Delete DropHandlers left by older builds
        remove_legacy_drop_handlers(root, prefix);

        // 4. Delete CLSIDs
        let clsid_cmd_key = format!("{}CLSID\\{}", prefix, CLSID_COMMAND_STR);
        delete_key_tree(root, &clsid_cmd_key);

        let clsid_dd_key = format!("{}CLSID\\{}", prefix, CLSID_DRAGDROP_STR);
        delete_key_tree(root, &clsid_dd_key);

        let clsid_ch_key = format!("{}CLSID\\{}", prefix, CLSID_COPYHOOK_STR);
        delete_key_tree(root, &clsid_ch_key);
    }

    S_OK
}
