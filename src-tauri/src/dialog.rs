use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CoTaskMemFree, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, FOS_ALLOWMULTISELECT, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};

pub fn pick_files() -> Result<Vec<String>, String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| format!("Failed to create FileOpenDialog: {}", e))?;

        let options = dialog.GetOptions().map_err(|e| e.to_string())?;
        dialog
            .SetOptions(options | FOS_ALLOWMULTISELECT)
            .map_err(|e| e.to_string())?;

        if dialog.Show(HWND(0 as _)).is_err() {
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

        CoUninitialize();
        Ok(paths)
    }
}

pub fn pick_folder() -> Result<Option<String>, String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| format!("Failed to create FileOpenDialog: {}", e))?;

        let options = dialog.GetOptions().map_err(|e| e.to_string())?;
        dialog
            .SetOptions(options | FOS_PICKFOLDERS)
            .map_err(|e| e.to_string())?;

        if dialog.Show(HWND(0 as _)).is_err() {
            // User cancelled
            return Ok(None);
        }

        let result = dialog.GetResult().map_err(|e| e.to_string())?;
        let display_name = result
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|e| e.to_string())?;

        let path = display_name.to_string().map_err(|e| e.to_string())?;
        CoTaskMemFree(Some(display_name.as_ptr() as _));

        CoUninitialize();
        Ok(Some(path))
    }
}
