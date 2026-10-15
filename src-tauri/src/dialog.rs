use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CoTaskMemFree, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, FOS_ALLOWMULTISELECT, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};

struct ComScope(bool);

impl ComScope {
    fn new() -> Self {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        ComScope(hr.is_ok())
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.0 {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

/// Opens native multi-file picker on a dedicated STA thread, modal to parent window.
pub fn pick_files(parent_hwnd_raw: Option<isize>) -> Result<Vec<String>, String> {
    let handle = std::thread::Builder::new()
        .name("deepnode-file-dialog-picker".to_string())
        .spawn(move || {
            let _com = ComScope::new();
            unsafe {
                let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| format!("Failed to create FileOpenDialog: {}", e))?;

                let options = dialog.GetOptions().map_err(|e| e.to_string())?;
                dialog
                    .SetOptions(options | FOS_ALLOWMULTISELECT)
                    .map_err(|e| e.to_string())?;

                let parent_hwnd = parent_hwnd_raw
                    .map(|h| HWND(h as *mut std::ffi::c_void))
                    .unwrap_or(HWND(std::ptr::null_mut()));

                if dialog.Show(parent_hwnd).is_err() {
                    // User cancelled
                    return Ok(Vec::new());
                }

                let results = dialog.GetResults().map_err(|e| e.to_string())?;
                let count = results.GetCount().map_err(|e| e.to_string())?;
                let mut paths = Vec::new();

                for i in 0..count {
                    if let Ok(item) = results.GetItemAt(i) {
                        if let Ok(display_name) = item.GetDisplayName(SIGDN_FILESYSPATH) {
                            if let Ok(s) = display_name.to_string() {
                                paths.push(s);
                            }
                            CoTaskMemFree(Some(display_name.as_ptr() as _));
                        }
                    }
                }

                Ok(paths)
            }
        })
        .map_err(|e| format!("Failed to spawn file dialog thread: {}", e))?;

    handle
        .join()
        .unwrap_or_else(|_| Err("File dialog thread panicked".to_string()))
}

/// Opens native folder picker on a dedicated STA thread, modal to parent window.
pub fn pick_folder(parent_hwnd_raw: Option<isize>) -> Result<Option<String>, String> {
    let handle = std::thread::Builder::new()
        .name("deepnode-folder-dialog-picker".to_string())
        .spawn(move || {
            let _com = ComScope::new();
            unsafe {
                let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| format!("Failed to create FileOpenDialog: {}", e))?;

                let options = dialog.GetOptions().map_err(|e| e.to_string())?;
                dialog
                    .SetOptions(options | FOS_PICKFOLDERS)
                    .map_err(|e| e.to_string())?;

                let parent_hwnd = parent_hwnd_raw
                    .map(|h| HWND(h as *mut std::ffi::c_void))
                    .unwrap_or(HWND(std::ptr::null_mut()));

                if dialog.Show(parent_hwnd).is_err() {
                    // User cancelled
                    return Ok(None);
                }

                let result = dialog.GetResult().map_err(|e| e.to_string())?;
                let display_name = result
                    .GetDisplayName(SIGDN_FILESYSPATH)
                    .map_err(|e| e.to_string())?;

                let path = display_name.to_string().map_err(|e| e.to_string())?;
                CoTaskMemFree(Some(display_name.as_ptr() as _));

                Ok(Some(path))
            }
        })
        .map_err(|e| format!("Failed to spawn folder dialog thread: {}", e))?;

    handle
        .join()
        .unwrap_or_else(|_| Err("Folder dialog thread panicked".to_string()))
}
