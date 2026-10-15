use std::path::PathBuf;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
    KEY_READ,
};

/// Resolves the full path of deepnode_shell.dll.
pub fn get_dll_path() -> Option<PathBuf> {
    unsafe {
        let mut hmodule = HMODULE::default();
        let addr = get_dll_path as *const () as *const u16;
        if GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
            PCWSTR(addr),
            &mut hmodule,
        )
        .is_ok()
        {
            let mut buf = [0u16; 1024];
            let len = GetModuleFileNameW(hmodule, &mut buf) as usize;
            if len > 0 {
                return Some(PathBuf::from(String::from_utf16_lossy(&buf[..len])));
            }
        }
    }
    None
}

/// Resolves the directory where deepnode_shell.dll is located.
pub fn get_dll_dir() -> Option<PathBuf> {
    get_dll_path().and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

fn query_reg_path(root: HKEY, subkey: &str, value_name: &str) -> Option<PathBuf> {
    unsafe {
        let subkey_w: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
        let val_w: Vec<u16> = if value_name.is_empty() {
            vec![0]
        } else {
            value_name.encode_utf16().chain(std::iter::once(0)).collect()
        };

        let mut hkey = HKEY::default();
        if RegOpenKeyExW(root, PCWSTR(subkey_w.as_ptr()), 0, KEY_READ, &mut hkey).is_ok() {
            let mut buf = [0u16; 1024];
            let mut buf_len = (buf.len() * 2) as u32;
            let val_ptr = if value_name.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR(val_w.as_ptr())
            };

            let res = RegQueryValueExW(
                hkey,
                val_ptr,
                None,
                None,
                Some(buf.as_mut_ptr() as *mut u8),
                Some(&mut buf_len),
            );
            let _ = RegCloseKey(hkey);

            if res.is_ok() && buf_len > 0 {
                let len_chars = (buf_len as usize / 2).saturating_sub(1);
                let text = String::from_utf16_lossy(&buf[..len_chars]);
                let trimmed = text.trim().trim_matches('"');
                if !trimmed.is_empty() {
                    return Some(PathBuf::from(trimmed));
                }
            }
        }
    }
    None
}

/// Dynamically discovers the executable path of DeepNodeCopy.exe without any hardcoded developer paths.
pub fn get_exe_path() -> PathBuf {
    // 1. Check relative to deepnode_shell.dll location
    if let Some(dll_dir) = get_dll_dir() {
        for name in &["DeepNodeCopy.exe", "deepnode-gui.exe"] {
            let candidate = dll_dir.join(name);
            if candidate.is_file() {
                return candidate;
            }
            let parent_candidate = dll_dir.join("..").join(name);
            if parent_candidate.is_file() {
                return parent_candidate;
            }
            let bin_candidate = dll_dir.join("bin").join(name);
            if bin_candidate.is_file() {
                return bin_candidate;
            }
        }
    }

    // 2. Windows App Paths registry keys
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        if let Some(p) = query_reg_path(
            root,
            r"Software\Microsoft\Windows\CurrentVersion\App Paths\DeepNodeCopy.exe",
            "",
        ) {
            if p.is_file() {
                return p;
            }
        }
        if let Some(dir) = query_reg_path(root, r"Software\DeepNode\DeepNode Copy", "InstallPath") {
            let candidate = dir.join("DeepNodeCopy.exe");
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    // 3. Standard Program Files installation paths
    if let Ok(pf) = std::env::var("ProgramFiles") {
        let p = PathBuf::from(pf).join("DeepNode Copy").join("DeepNodeCopy.exe");
        if p.is_file() {
            return p;
        }
    }
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let p = PathBuf::from(local_app_data)
            .join("Programs")
            .join("DeepNode Copy")
            .join("DeepNodeCopy.exe");
        if p.is_file() {
            return p;
        }
    }

    // 4. Current exe (ONLY if it is NOT explorer.exe)
    if let Ok(current) = std::env::current_exe() {
        let file_name = current
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        if file_name != "explorer.exe" {
            if let Some(dir) = current.parent() {
                for name in &["DeepNodeCopy.exe", "deepnode-gui.exe"] {
                    let candidate = dir.join(name);
                    if candidate.is_file() {
                        return candidate;
                    }
                }
            }
        }
    }

    PathBuf::from("DeepNodeCopy.exe")
}
