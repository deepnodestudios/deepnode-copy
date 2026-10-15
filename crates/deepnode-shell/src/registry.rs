use windows::core::{HRESULT, PCWSTR};
use windows::Win32::Foundation::{E_FAIL, S_OK};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteKeyW, RegDeleteTreeW, RegSetValueExW,
    HKEY, HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use crate::util::get_dll_path;

pub const CLSID_COMMAND_STR: &str = "{5E8F3C72-A2D9-4E61-8C9B-93E48F1A7C20}";
pub const CLSID_DRAGDROP_STR: &str = "{4F8E9C32-B1D8-4C50-9B8A-82E37E096B19}";

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

    // Target roots: try HKEY_CLASSES_ROOT (elevated), fallback to HKCU\Software\Classes (user)
    let roots = [
        (HKEY_CLASSES_ROOT, ""),
        (HKEY_CURRENT_USER, r"Software\Classes\"),
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

            // 3. Register DragDropHandlers
            for target in &["*", "Directory", "Drive"] {
                let dd_key = format!(r"{}{}\shellex\DragDropHandlers\DeepNodeCopy", prefix, target);
                set_key_default_value(root, &dd_key, CLSID_DRAGDROP_STR);
            }

            // 4. Register ContextMenuHandlers (classic Explorer fallback)
            for target in &["*", "Directory", "Directory\\Background", "Drive"] {
                let cm_key = format!(r"{}{}\shellex\ContextMenuHandlers\DeepNodeCopy", prefix, target);
                set_key_default_value(root, &cm_key, CLSID_COMMAND_STR);
            }

            registered = true;
            break;
        }
    }

    if registered {
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
        for target in &["*", "Directory", "Drive"] {
            let dd_key = format!(r"{}{}\shellex\DragDropHandlers\DeepNodeCopy", prefix, target);
            delete_key_tree(root, &dd_key);
        }

        // 2. Delete ContextMenuHandlers
        for target in &["*", "Directory", "Directory\\Background", "Drive"] {
            let cm_key = format!(r"{}{}\shellex\ContextMenuHandlers\DeepNodeCopy", prefix, target);
            delete_key_tree(root, &cm_key);
        }

        // 3. Delete CLSIDs
        let clsid_cmd_key = format!("{}CLSID\\{}", prefix, CLSID_COMMAND_STR);
        delete_key_tree(root, &clsid_cmd_key);

        let clsid_dd_key = format!("{}CLSID\\{}", prefix, CLSID_DRAGDROP_STR);
        delete_key_tree(root, &clsid_dd_key);
    }

    S_OK
}
