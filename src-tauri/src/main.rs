// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State, Window};
use tokio::sync::{mpsc, Mutex};

mod dialog;

use deepnode_core::conflict::policy::ConflictPolicy;
use deepnode_core::engine::config::{TransferConfig, TransferMode, VerifyMode};
use deepnode_core::engine::coordinator::TransferCoordinator;
use deepnode_core::engine::events::{ConflictInfo, ConflictResolutionChoice, EngineEvent};
use deepnode_core::engine::item::{FileItem, FileItemStatus};
use deepnode_core::engine::on_finish::{OnFinishAction, OnFinishExecutor};
use deepnode_core::engine::scanner::FileScanner;
use deepnode_core::engine::stats::TransferStats;
use deepnode_core::hash::hasher::HashAlgorithm;
use deepnode_core::hash::verifier::HashVerifier;
use deepnode_core::{
    clean_path_str, is_deepnode_temp_list, resolve_star_path, sanitize_path,
    validate_filesystem_path, StarPathResolution,
};
use deepnode_db::Database;

pub fn log_engine(msg: &str) {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let dir = PathBuf::from(&appdata).join("DeepNodeCopy");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("engine.log");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            use std::io::Write;
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let _ = writeln!(f, "[{}] {}", timestamp, msg);
            let _ = f.flush();
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct LaunchArgs {
    pub sources: Vec<String>,
    pub destination: Option<String>,
    pub mode: String,
    pub is_popup: bool,
    pub is_silent: bool,
    pub auto_start: bool,
    pub conflict_policy: Option<String>,
    pub on_finish: Option<String>,
    /// Physical screen point where the popup's top-left corner goes (`--pos x,y`); the shell
    /// extension passes the cursor position captured at drop/paste time, like TeraCopy.
    #[serde(default)]
    pub popup_anchor: Option<(i32, i32)>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CurrentTransferState {
    pub status: String, // "idle", "transferring", "paused", "completed", "canceled", "failed"
    pub stats: TransferStats,
    pub files: Vec<FileItem>,
    pub launch_args: LaunchArgs,
    pub active_conflict: Option<ConflictInfo>,
}

#[derive(Clone)]
pub struct AppState {
    pub pause_flag: Arc<AtomicBool>,
    pub cancel_flag: Arc<AtomicBool>,
    pub skip_flag: Arc<AtomicBool>,
    pub is_worker_running: Arc<AtomicBool>,
    pub skipped_ids: Arc<std::sync::Mutex<std::collections::HashSet<u64>>>,
    pub last_failed_items: Arc<Mutex<Vec<FileItem>>>,
    pub last_config: Arc<Mutex<Option<TransferConfig>>>,
    pub db: Arc<Mutex<Option<Database>>>,
    pub conflict_sender: Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<ConflictResolutionChoice>>>>,
    pub transfer_state: Arc<tokio::sync::Mutex<CurrentTransferState>>,
}

fn recombine_split_paths(tokens: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].starts_with('/') || tokens[i].starts_with('-') {
            result.push(tokens[i].clone());
            i += 1;
            continue;
        }

        let mut best_combined = tokens[i].clone();
        let mut best_j = i;

        let has_star = tokens[i].starts_with('*');
        let base = if has_star { tokens[i].trim_start_matches('*') } else { tokens[i].as_str() };
        let base_exists = sanitize_path(base).exists();
        let mut candidate = tokens[i].clone();
        let mut j = i + 1;
        while j < tokens.len() {
            if tokens[j].starts_with('/') || tokens[j].starts_with('-') {
                break;
            }

            candidate.push(' ');
            candidate.push_str(&tokens[j]);
            let check_path = if has_star {
                candidate.trim_start_matches('*')
            } else {
                &candidate
            };
            let p = sanitize_path(check_path);
            if p.exists() {
                best_combined = candidate.clone();
                best_j = j;
                break;
            }
            // An existing path is already complete; absorbing the next argument (e.g. a
            // `*deepnode_list_*.tmp` followed by the destination) corrupts both.
            if !base_exists && p.parent().map(|par| par.exists()).unwrap_or(false) && p.is_absolute() {
                best_combined = candidate.clone();
                best_j = j;
            }
            j += 1;
        }

        if best_j > i {
            result.push(best_combined);
            i = best_j + 1;
        } else {
            result.push(tokens[i].clone());
            i += 1;
        }
    }
    result
}

fn parse_cli_args(args: &[String]) -> LaunchArgs {
    let mut sources = Vec::new();
    let mut destination = None;
    let mut mode = "copy".to_string();
    let mut is_popup = false;
    let mut is_silent = false;
    let mut auto_start = false;
    let mut conflict_policy = None;
    let mut on_finish = None;
    let mut popup_anchor = None;

    if args.len() > 1 {
        let first = clean_path_str(&args[1]).to_lowercase();
        let mut idx = 1;

        if first == "copy" {
            mode = "copy".to_string();
            auto_start = true;
            idx = 2;
        } else if first == "move" {
            mode = "move".to_string();
            auto_start = true;
            idx = 2;
        }

        let recombined = recombine_split_paths(&args[idx..]);
        let mut positional = Vec::new();
        let mut r_idx = 0;

        while r_idx < recombined.len() {
            let raw_arg = &recombined[r_idx];
            let arg = clean_path_str(raw_arg);
            if arg.is_empty() {
                r_idx += 1;
                continue;
            }

            if arg.eq_ignore_ascii_case("--popup") || arg.eq_ignore_ascii_case("-p") {
                is_popup = true;
            } else if arg.eq_ignore_ascii_case("--silent") || arg.eq_ignore_ascii_case("--background") || arg.eq_ignore_ascii_case("-b") {
                is_silent = true;
            } else if arg.eq_ignore_ascii_case("--pos") && r_idx + 1 < recombined.len() {
                let pos = clean_path_str(&recombined[r_idx + 1]);
                popup_anchor = pos
                    .split_once(',')
                    .and_then(|(x, y)| Some((x.trim().parse().ok()?, y.trim().parse().ok()?)));
                r_idx += 1;
            } else if arg.eq_ignore_ascii_case("--mode") && r_idx + 1 < recombined.len() {
                mode = clean_path_str(&recombined[r_idx + 1]);
                r_idx += 1;
            } else if arg.eq_ignore_ascii_case("--target") && r_idx + 1 < recombined.len() {
                let target_cleaned = clean_path_str(&recombined[r_idx + 1]);
                if !target_cleaned.is_empty() {
                    destination = Some(target_cleaned);
                }
                r_idx += 1;
            } else if arg.eq_ignore_ascii_case("--source") {
                r_idx += 1;
                while r_idx < recombined.len() && !recombined[r_idx].starts_with('-') && !recombined[r_idx].starts_with('/') {
                    let cleaned = clean_path_str(&recombined[r_idx]);
                    if !cleaned.is_empty() {
                        if cleaned.starts_with('*') {
                            let file_path = clean_path_str(cleaned.trim_start_matches('*'));
                            let path = sanitize_path(&file_path);
                            match resolve_star_path(&path) {
                                StarPathResolution::FileList(items_from_list) => {
                                    sources.extend(items_from_list);
                                    if is_deepnode_temp_list(&path) {
                                        let _ = std::fs::remove_file(&path);
                                    }
                                }
                                StarPathResolution::SingleSource(p) => {
                                    sources.push(p.to_string_lossy().to_string());
                                }
                                StarPathResolution::NotFound(p) => {
                                    sources.push(p.to_string_lossy().to_string());
                                }
                            }
                        } else {
                            positional.push(cleaned);
                        }
                    }
                    r_idx += 1;
                }
                continue;
            } else if arg.eq_ignore_ascii_case("/overwriteolder") {
                conflict_policy = Some("older".to_string());
            } else if arg.eq_ignore_ascii_case("/skipall") {
                conflict_policy = Some("skip".to_string());
            } else if arg.eq_ignore_ascii_case("/overwriteall") {
                conflict_policy = Some("overwrite".to_string());
            } else if arg.eq_ignore_ascii_case("/renameall") {
                conflict_policy = Some("auto_rename".to_string());
            } else if arg.eq_ignore_ascii_case("/close") {
                on_finish = Some("close_app".to_string());
            } else if arg.eq_ignore_ascii_case("/noclose") {
                on_finish = Some("do_nothing".to_string());
            } else if arg.eq_ignore_ascii_case("/shutdown") {
                on_finish = Some("shutdown".to_string());
            } else if arg.starts_with('*') {
                let file_path = clean_path_str(arg.trim_start_matches('*'));
                let path = sanitize_path(&file_path);
                match resolve_star_path(&path) {
                    StarPathResolution::FileList(items_from_list) => {
                        sources.extend(items_from_list);
                        if is_deepnode_temp_list(&path) {
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                    StarPathResolution::SingleSource(p) => {
                        positional.push(p.to_string_lossy().to_string());
                    }
                    StarPathResolution::NotFound(p) => {
                        positional.push(p.to_string_lossy().to_string());
                    }
                }
            } else if !arg.starts_with('-') && !arg.starts_with('/') {
                positional.push(arg);
            }
            r_idx += 1;
        }

        if destination.is_none() && positional.len() > 1 && (auto_start || is_popup) {
            destination = positional.pop().map(|p| clean_path_str(&p));
            for p in positional {
                let c = clean_path_str(&p);
                if !c.is_empty() {
                    sources.push(c);
                }
            }
        } else if destination.is_none() && positional.len() == 1 && (auto_start || is_popup) {
            destination = positional.pop().map(|p| clean_path_str(&p));
        } else {
            for p in positional {
                let c = clean_path_str(&p);
                if !c.is_empty() {
                    sources.push(c);
                }
            }
        }
    }

    let destination = destination.map(|d| clean_path_str(&d)).filter(|d| !d.is_empty());
    let mut sources: Vec<String> = sources
        .into_iter()
        .map(|s| clean_path_str(&s))
        .filter(|s| !s.is_empty())
        .collect();

    // If destination is specified (e.g. via --target from Explorer context menu) but sources is empty,
    // automatically read the copied file list from Windows Clipboard!
    if destination.is_some() && sources.is_empty() {
        #[cfg(target_os = "windows")]
        {
            let clip_files = get_clipboard_hdrop_files();
            if !clip_files.is_empty() {
                sources = clip_files;
                is_popup = true;
            }
        }
    }

    if destination.is_some() && !sources.is_empty() && !is_popup && (mode == "copy" || mode == "move") {
        auto_start = true;
    }

    LaunchArgs {
        sources,
        destination,
        mode,
        is_popup,
        is_silent,
        auto_start,
        conflict_policy,
        on_finish,
        popup_anchor,
    }
}

#[tauri::command]
async fn set_always_on_top(window: Window, always_on_top: bool) -> Result<(), String> {
    window.set_always_on_top(always_on_top).map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_conflict_mode(window: Window, active: bool, is_compact: bool) -> Result<(), String> {
    log_engine(&format!("set_conflict_mode: active={}, is_compact={}", active, is_compact));
    if is_compact {
        let size = if active {
            tauri::LogicalSize::new(580.0, 420.0)
        } else {
            tauri::LogicalSize::new(580.0, 190.0)
        };
        window.set_size(size).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

#[tauri::command]
async fn toggle_compact_mode(window: Window, compact: bool) -> Result<(), String> {
    let size = if compact {
        tauri::LogicalSize::new(580.0, 190.0)
    } else {
        tauri::LogicalSize::new(960.0, 640.0)
    };
    window.set_size(size).map_err(|e| e.to_string())
}

#[tauri::command]
async fn minimize_window(window: Window) -> Result<(), String> {
    window.minimize().map_err(|e| e.to_string())
}

#[tauri::command]
async fn toggle_maximize_window(window: Window) -> Result<(), String> {
    if window.is_maximized().map_err(|e| e.to_string())? {
        window.unmaximize().map_err(|e| e.to_string())
    } else {
        window.maximize().map_err(|e| e.to_string())
    }
}

#[tauri::command]
async fn pause_transfer(state: State<'_, AppState>) -> Result<(), String> {
    state.pause_flag.store(true, Ordering::Relaxed);
    let mut cur = state.transfer_state.lock().await;
    cur.status = "paused".to_string();
    Ok(())
}

#[tauri::command]
async fn resume_transfer(state: State<'_, AppState>) -> Result<(), String> {
    state.pause_flag.store(false, Ordering::Relaxed);
    let mut cur = state.transfer_state.lock().await;
    cur.status = "transferring".to_string();
    Ok(())
}

fn cleanup_partial_files_slice(files: &[FileItem], dest_dir: Option<&Path>) {
    for file in files {
        if file.status != FileItemStatus::Completed
            && (file.bytes_transferred > 0 || file.status == FileItemStatus::Copying)
            && file.dst_path.exists()
        {
            log_engine(&format!("Cleaning up incomplete partial file: {:?}", file.dst_path));
            let ok = deepnode_core::io::copier::cleanup_cancelled_file_and_empty_parents(&file.dst_path, dest_dir);
            log_engine(&format!("Incomplete file {:?} deletion result: {}", file.dst_path, ok));
        }
    }
}

async fn cleanup_partial_transfers(state: &AppState) {
    let (files, dest_dir) = {
        let guard = state.transfer_state.lock().await;
        let dest = guard.launch_args.destination.as_ref().map(PathBuf::from);
        (guard.files.clone(), dest)
    };
    cleanup_partial_files_slice(&files, dest_dir.as_deref());
}

#[allow(dead_code)]
fn cleanup_partial_transfers_sync(state: &AppState) {
    let (files, dest_dir) = {
        let guard = state.transfer_state.blocking_lock();
        let dest = guard.launch_args.destination.as_ref().map(PathBuf::from);
        (guard.files.clone(), dest)
    };
    cleanup_partial_files_slice(&files, dest_dir.as_deref());
}

#[tauri::command]
async fn cancel_transfer(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    log_engine("cancel_transfer command invoked: signaling cancel_flag and cleaning up partial files");
    state.cancel_flag.store(true, Ordering::SeqCst);

    // Unblock any conflict sender
    let mut lock = state.conflict_sender.lock().await;
    if let Some(tx) = lock.take() {
        let _ = tx.send(ConflictResolutionChoice {
            action: "skip".to_string(),
            apply_to_all: false,
        });
    }

    // Wait up to 2000ms for worker thread to exit coordinator loop and write DB records
    let start = std::time::Instant::now();
    while state.is_worker_running.load(Ordering::SeqCst) && start.elapsed() < std::time::Duration::from_millis(2000) {
        tokio::time::sleep(tokio::time::Duration::from_millis(25)).await;
    }

    // Ensure status is marked canceled in transfer_state
    {
        let mut cur = state.transfer_state.lock().await;
        cur.status = "canceled".to_string();
        cur.active_conflict = None;
        for file in cur.files.iter_mut() {
            if file.status == FileItemStatus::Copying
                || file.status == FileItemStatus::Pending
                || (file.status != FileItemStatus::Completed && file.bytes_transferred > 0)
            {
                file.status = FileItemStatus::Canceled;
            }
        }
    }

    // Clean up partial files
    cleanup_partial_transfers(&state).await;

    // Finalize any in_progress session in DB as canceled
    {
        let mut db_guard = state.db.lock().await;
        if let Some(ref mut db) = *db_guard {
            let _ = db.finalize_active_sessions_as_canceled();
        }
    }

    let _ = app.emit("engine_event", &EngineEvent::TransferCanceled);
    Ok(())
}

#[tauri::command]
async fn start_dragging(window: Window) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
async fn skip_current_file(state: State<'_, AppState>) -> Result<(), String> {
    log_engine("skip_current_file command invoked");
    state.skip_flag.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
async fn skip_files_by_id(app: AppHandle, state: State<'_, AppState>, ids: Vec<u64>) -> Result<(), String> {
    log_engine(&format!("skip_files_by_id invoked for {:?}", ids));
    {
        let mut lock = state.skipped_ids.lock().unwrap_or_else(|e| e.into_inner());
        for id in &ids {
            lock.insert(*id);
        }
    }
    {
        let mut cur = state.transfer_state.lock().await;
        for file in cur.files.iter_mut() {
            if ids.contains(&file.id) {
                if file.status == FileItemStatus::Copying || file.status == FileItemStatus::Verifying {
                    state.skip_flag.store(true, Ordering::SeqCst);
                } else if file.status == FileItemStatus::Pending {
                    file.status = FileItemStatus::Skipped;
                    let _ = app.emit("engine_event", &EngineEvent::FileSkipped {
                        item: file.clone(),
                        reason: "Skipped by user".to_string(),
                    });
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
async fn remove_queued_files(app: AppHandle, state: State<'_, AppState>, ids: Vec<u64>) -> Result<(), String> {
    log_engine(&format!("remove_queued_files invoked for {:?}", ids));
    {
        let mut lock = state.skipped_ids.lock().unwrap_or_else(|e| e.into_inner());
        for id in &ids {
            lock.insert(*id);
        }
    }

    let mut files_to_clean = Vec::new();
    let dest_dir = {
        let mut cur = state.transfer_state.lock().await;
        for file in cur.files.iter_mut() {
            if ids.contains(&file.id) && (file.status == FileItemStatus::Copying || file.status == FileItemStatus::Verifying) {
                state.skip_flag.store(true, Ordering::SeqCst);
                files_to_clean.push(file.clone());
            }
        }
        cur.files.retain(|f| !ids.contains(&f.id) || f.status == FileItemStatus::Completed);
        cur.stats.total_files = cur.files.len();
        cur.stats.total_bytes = cur.files.iter().map(|f| f.size).sum();
        let _ = app.emit("engine_event", &EngineEvent::ProgressUpdated(cur.stats.clone()));
        cur.launch_args.destination.as_ref().map(PathBuf::from)
    };

    if !files_to_clean.is_empty() {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        cleanup_partial_files_slice(&files_to_clean, dest_dir.as_deref());
    }

    Ok(())
}

#[tauri::command]
fn show_in_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let clean = clean_path_str(&path);
        let target_path = validate_filesystem_path(&clean)?;
        if !target_path.exists() {
            return Err(format!("Dosya veya klasör bulunamadı: {:?}", target_path));
        }

        let p_str = target_path.to_string_lossy().to_string();
        if target_path.is_file() {
            std::process::Command::new("explorer")
                .args(["/select,", &p_str])
                .creation_flags(0x08000000)
                .spawn()
                .map_err(|e| e.to_string())?;
        } else {
            std::process::Command::new("explorer")
                .arg(&p_str)
                .creation_flags(0x08000000)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        Ok(())
    }
}

#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let clean = clean_path_str(&path);
        let target_path = validate_filesystem_path(&clean)?;
        if !target_path.exists() {
            return Err(format!("Klasör bulunamadı: {:?}", target_path));
        }
        if !target_path.is_dir() {
            return Err(format!("Belirtilen yol bir klasör değil: {:?}", target_path));
        }

        let p_str = target_path.to_string_lossy().to_string();
        std::process::Command::new("explorer")
            .arg(&p_str)
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn get_clipboard_hdrop_files() -> Vec<String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

    let mut files = Vec::new();
    unsafe {
        let mut opened = false;
        for _ in 0..6 {
            if OpenClipboard(HWND::default()).is_ok() {
                opened = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        if opened {
            if let Ok(handle) = GetClipboardData(15) { // CF_HDROP = 15
                if !handle.0.is_null() {
                    let hdrop = HDROP(handle.0);
                    let count = DragQueryFileW(hdrop, 0xFFFFFFFF, None);
                    for i in 0..count {
                        let len = DragQueryFileW(hdrop, i, None) as usize;
                        if len > 0 {
                            let mut buf = vec![0u16; len + 1];
                            DragQueryFileW(hdrop, i, Some(&mut buf));
                            let path = String::from_utf16_lossy(&buf[..len]);
                            let trimmed = path.trim();
                            if !trimmed.is_empty() {
                                files.push(trimmed.to_string());
                            }
                        }
                    }
                }
            }
            let _ = CloseClipboard();
        }
    }
    files
}

#[tauri::command]
fn get_clipboard_files() -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        get_clipboard_hdrop_files()
    }
    #[cfg(not(target_os = "windows"))]
    {
        Vec::new()
    }
}

#[cfg(target_os = "windows")]
fn force_window_to_foreground(window: &tauri::WebviewWindow) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetForegroundWindow, ShowWindow, BringWindowToTop, IsIconic, SW_RESTORE, SW_SHOW,
    };
    use windows::Win32::Foundation::HWND;

    // Foreground rights come from whoever launched or messaged us: the shell extension and
    // the second-instance IPC client call AllowSetForegroundWindow for this process. No
    // AttachThreadInput / synthetic Alt key: those hung WebView2 when Explorer's thread was
    // busy and are the kind of input tampering antivirus behaviour monitors flag.
    if let Ok(hwnd_raw) = window.hwnd() {
        let hwnd = HWND(hwnd_raw.0 as *mut _);
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            } else {
                let _ = ShowWindow(hwnd, SW_SHOW);
            }

            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
            // Topmost is not forced here: only the choice popup asks for it (set_always_on_top),
            // so the main and compact windows can go behind other windows normally.
        }
    }
}

/// Logical size of the copy/paste choice popup.
const POPUP_SIZE: (f64, f64) = (480.0, 320.0);

#[cfg(target_os = "windows")]
/// Sizes the choice popup and places it like TeraCopy's "Paste Window": the visible window's
/// top-left corner sits at `anchor` (the cursor position the shell extension captured at
/// drop/paste time) or at the current cursor, on the monitor under that point, shifted — not
/// flipped — to stay inside that monitor's work area (taskbar excluded).
/// Must run on the main thread so the size change is applied before the window is measured.
fn place_popup_window(window: &tauri::WebviewWindow, anchor: Option<(i32, i32)>) {
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClientRect, GetCursorPos, GetWindowRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
    };

    let Ok(hwnd_raw) = window.hwnd() else { return };
    let hwnd = HWND(hwnd_raw.0 as *mut _);

    let pt = match anchor {
        Some((x, y)) => POINT { x, y },
        None => {
            let mut p = POINT::default();
            if unsafe { GetCursorPos(&mut p) }.is_err() {
                return;
            }
            p
        }
    };

    // Resolve the target monitor first: a window that has not been shown yet reports a
    // scale factor of 1.0, so Tauri's Logical size lands as physical pixels and the popup
    // opens smaller on scaled displays. Size in physical pixels for the target DPI instead.
    let (target_dpi, work) = unsafe {
        let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if monitor.is_invalid() || !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return;
        }
        let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        (dpi_x.max(96), info.rcWork)
    };

    let client_w = (POPUP_SIZE.0 * target_dpi as f64 / 96.0).round() as i32;
    let client_h = (POPUP_SIZE.1 * target_dpi as f64 / 96.0).round() as i32;

    let (mut x, mut y) = (pt.x.max(work.left), pt.y.max(work.top));
    if x + client_w > work.right {
        x = (work.right - client_w).max(work.left);
    }
    if y + client_h > work.bottom {
        y = (work.bottom - client_h).max(work.top);
    }

    // Move first so Windows assigns the target monitor's DPI, then set the exact physical
    // size synchronously. The second pass covers a WM_DPICHANGED resize triggered by the move.
    for _ in 0..2 {
        unsafe {
            let mut outer = RECT::default();
            let mut client = RECT::default();
            if GetWindowRect(hwnd, &mut outer).is_err() {
                return;
            }
            let _ = GetClientRect(hwnd, &mut client);
            let frame_w = (outer.right - outer.left) - client.right;
            let frame_h = (outer.bottom - outer.top) - client.bottom;

            // The undecorated window keeps invisible resize borders around its visible frame;
            // align the visible frame with the anchor, not the outer rect.
            let mut visible = outer;
            let _ = DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut visible as *mut RECT as *mut _,
                std::mem::size_of::<RECT>() as u32,
            );
            let (inset_left, inset_top) = (visible.left - outer.left, visible.top - outer.top);

            let _ = SetWindowPos(
                hwnd,
                None,
                x - inset_left,
                y - inset_top,
                client_w + frame_w,
                client_h + frame_h,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

#[cfg(target_os = "windows")]
fn trigger_explorer_popup(app: AppHandle, sources: Vec<String>, destination: String, anchor: Option<(i32, i32)>) {
    if sources.is_empty() {
        return;
    }

    let dest_final = if destination.is_empty() {
        std::env::var("USERPROFILE").ok().map(|p| PathBuf::from(p).join("Desktop").to_string_lossy().to_string()).unwrap_or_default()
    } else {
        destination
    };

    log_engine(&format!(
        "Explorer interceptor triggered! Sources: {:?}, Destination: {}",
        sources, dest_final
    ));

    let app_clone = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = app_clone.get_webview_window("main") {
            let _ = window.unmaximize();
            place_popup_window(&window, anchor);
            let _ = window.set_always_on_top(true);
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
            force_window_to_foreground(&window);

            #[derive(serde::Serialize, Clone)]
            struct PastePopupPayload {
                sources: Vec<String>,
                destination: String,
            }
            let payload = PastePopupPayload {
                sources,
                destination: dest_final,
            };
            let _ = window.emit("open_copy_paste_modal", payload);
        }
    });
}

#[cfg(target_os = "windows")]
fn try_send_args_to_existing_instance(args: &[String]) -> bool {
    use std::io::Write;
    use windows::Win32::System::Pipes::WaitNamedPipeW;
    use windows::core::PCWSTR;

    let pipe_path = r"\\.\pipe\deepnode_copy_ipc";
    let wide_pipe: Vec<u16> = pipe_path.encode_utf16().chain(std::iter::once(0)).collect();

    // Check if pipe exists or wait briefly (300ms)
    let pipe_ready = unsafe { WaitNamedPipeW(PCWSTR(wide_pipe.as_ptr()), 300).as_bool() };
    if !pipe_ready {
        return false;
    }

    if let Ok(mut file) = std::fs::OpenOptions::new().write(true).open(pipe_path) {
        // We were started by Explorer and hold foreground rights; pass them to the primary
        // instance so it can bring its popup to the front.
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(u32::MAX); // ASFW_ANY
        }
        if let Ok(json) = serde_json::to_string(args) {
            if writeln!(file, "{}", json).is_ok() && file.flush().is_ok() {
                return true;
            }
        }
    }
    false
}

fn trigger_native_transfer(
    app: AppHandle,
    state: Arc<AppState>,
    sources: Vec<String>,
    dest: String,
    mode: String,
    conflict: Option<String>,
    on_fin: String,
) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        #[cfg(target_os = "windows")]
        force_window_to_foreground(&win);
    }

    let dest_buf = match validate_filesystem_path(&dest) {
        Ok(p) if !p.is_file() => p,
        _ => {
            log_engine("NATIVE AUTO-START: Invalid destination path");
            let mut cur = state.transfer_state.blocking_lock();
            cur.status = "idle".to_string();
            return;
        }
    };

    let mut validated_sources = Vec::new();
    for s in &sources {
        let clean = clean_path_str(s);
        if clean.is_empty() {
            continue;
        }
        if let Some(stripped) = clean.strip_prefix('*') {
            if let Ok(list_p) = validate_filesystem_path(stripped) {
                if list_p.exists() {
                    validated_sources.push(format!("*{}", list_p.to_string_lossy()));
                }
            }
        } else if let Ok(src_p) = validate_filesystem_path(&clean) {
            if src_p.exists() {
                validated_sources.push(src_p.to_string_lossy().to_string());
            }
        }
    }

    if validated_sources.is_empty() {
        log_engine("NATIVE AUTO-START: No valid source paths found");
        let mut cur = state.transfer_state.blocking_lock();
        cur.status = "idle".to_string();
        return;
    }

    let items = FileScanner::scan(&validated_sources, &dest_buf).unwrap_or_default();
    log_engine(&format!("Scanned {} files for auto-start transfer", items.len()));

    if !items.is_empty() {
        let t_mode = match mode.as_str() {
            "move" => TransferMode::Move,
            _ => TransferMode::Copy,
        };

        let c_policy = match conflict.as_deref().unwrap_or("ask").to_lowercase().as_str() {
            "overwrite" | "overwriteall" => ConflictPolicy::OverwriteAll,
            "skip" | "skipall" => ConflictPolicy::SkipAll,
            "older" | "overwriteolder" => ConflictPolicy::OverwriteOlder,
            "rename" | "autorename" => ConflictPolicy::AutoRename,
            _ => ConflictPolicy::AskUser,
        };

        let config = TransferConfig {
            mode: t_mode,
            buffer_size: 2 * 1024 * 1024,
            direct_io: false,
            conflict_policy: c_policy,
            verify_mode: VerifyMode::Always,
            hash_algorithm: HashAlgorithm::XxHash3_64,
            preserve_timestamps: true,
            preserve_attributes: true,
            preserve_ads: true,
            preserve_acl: false,
            use_vss_for_locked: true,
            retry_count: 3,
            retry_timeout_secs: 30,
            export_checksum_file: false,
            write_checksum_to_ads: false,
        };

        let app_handle = app.clone();
        let state_clone = Arc::clone(&state);
        let items_clone = items.clone();

        tauri::async_runtime::spawn(async move {
            {
                let mut cur = state_clone.transfer_state.lock().await;
                cur.status = "transferring".to_string();
                cur.files = items_clone.clone();
                cur.stats.total_files = items_clone.len();
                cur.stats.total_bytes = items_clone.iter().map(|i| i.size).sum();
                cur.active_conflict = None;
            }
            if let Err(e) = spawn_transfer_worker(
                app_handle,
                Arc::clone(&state_clone),
                items_clone,
                config,
                on_fin,
                Some(dest_buf),
            ) {
                log_engine(&format!("Native auto-start error: {}", e));
                let mut cur = state_clone.transfer_state.lock().await;
                cur.status = "idle".to_string();
            }
        });
    } else {
        let mut cur = state.transfer_state.blocking_lock();
        cur.status = "idle".to_string();
    }
}

#[cfg(target_os = "windows")]
fn handle_forwarded_cli_args(app: &AppHandle, state: &Arc<AppState>, args: &[String]) {
    let launch_args = parse_cli_args(args);

    if launch_args.is_popup && !launch_args.sources.is_empty() {
        let dest = launch_args.destination.unwrap_or_default();
        trigger_explorer_popup(app.clone(), launch_args.sources, dest, launch_args.popup_anchor);
    } else if launch_args.auto_start && launch_args.destination.is_some() && !launch_args.sources.is_empty() {
        let dest = launch_args.destination.unwrap();
        let on_fin = launch_args.on_finish.unwrap_or_else(|| "do_nothing".to_string());
        trigger_native_transfer(
            app.clone(),
            Arc::clone(state),
            launch_args.sources,
            dest,
            launch_args.mode,
            launch_args.conflict_policy,
            on_fin,
        );
    } else if !launch_args.sources.is_empty() {
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.show();
            let _ = win.unminimize();
            force_window_to_foreground(&win);

            #[derive(serde::Serialize, Clone)]
            struct ForwardedPayload {
                sources: Vec<String>,
                destination: Option<String>,
                auto_start: bool,
                mode: String,
            }
            let payload = ForwardedPayload {
                sources: launch_args.sources.clone(),
                destination: launch_args.destination.clone(),
                auto_start: launch_args.auto_start,
                mode: launch_args.mode.clone(),
            };
            let _ = win.emit("cli_args_forwarded", payload);
        }
    } else {
        log_engine("handle_forwarded_cli_args: restoring and focusing main window UI");
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.set_always_on_top(false);
            let _ = win.center();
            let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize {
                width: 960.0,
                height: 640.0,
            }));
            let _ = win.unminimize();
            let _ = win.show();
            let _ = win.set_focus();
            force_window_to_foreground(&win);
            let _ = win.emit("restore_main_ui", ());
        }
    }
}

#[cfg(target_os = "windows")]
fn start_ipc_pipe_server(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::net::windows::named_pipe::ServerOptions;

        let pipe_name = r"\\.\pipe\deepnode_copy_ipc";
        let mut first = true;

        loop {
            let server_res = ServerOptions::new()
                .first_pipe_instance(first)
                .create(pipe_name);

            let server = match server_res {
                Ok(s) => {
                    first = false;
                    s
                }
                Err(e) => {
                    log_engine(&format!("IPC Named Pipe server create error: {:?}", e));
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    continue;
                }
            };

            if server.connect().await.is_ok() {
                let mut reader = BufReader::new(server);
                let mut line = String::new();
                if reader.read_line(&mut line).await.is_ok() {
                    let trimmed = line.trim();
                    if let Ok(forwarded_args) = serde_json::from_str::<Vec<String>>(trimmed) {
                        log_engine(&format!("IPC Named Pipe received arguments from secondary instance: {:?}", forwarded_args));
                        let app_clone = app.clone();
                        let state_clone = Arc::clone(&state);
                        let _ = app.run_on_main_thread(move || {
                            handle_forwarded_cli_args(&app_clone, &state_clone, &forwarded_args);
                        });
                    }
                }
            }
        }
    });
}

#[tauri::command]
fn explorer_copy(sources: Vec<String>, destination: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::{BOOL, HWND};
        use windows::Win32::UI::Shell::{
            SHFileOperationW, SHFILEOPSTRUCTW, FOF_ALLOWUNDO, FOF_NOCONFIRMMKDIR, FO_COPY,
        };
        use windows::core::PCWSTR;

        if sources.is_empty() || destination.is_empty() {
            return Err("Kaynak dosyalar veya hedef klasör belirtilmedi.".to_string());
        }

        let mut from_wide: Vec<u16> = Vec::new();
        for src in &sources {
            from_wide.extend(src.encode_utf16());
            from_wide.push(0);
        }
        from_wide.push(0); // Double null-terminator

        let mut to_wide: Vec<u16> = destination.encode_utf16().collect();
        to_wide.push(0);
        to_wide.push(0); // Double null-terminator

        unsafe {
            let mut op = SHFILEOPSTRUCTW {
                hwnd: HWND::default(),
                wFunc: FO_COPY,
                pFrom: PCWSTR(from_wide.as_ptr()),
                pTo: PCWSTR(to_wide.as_ptr()),
                fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMMKDIR).0 as u16,
                fAnyOperationsAborted: BOOL(0),
                hNameMappings: std::ptr::null_mut(),
                lpszProgressTitle: PCWSTR::null(),
            };
            let res = SHFileOperationW(&mut op);
            if res != 0 {
                return Err(format!("Windows Gezgini kopyalama hatası kodu: {}", res));
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (sources, destination);
        Ok(())
    }
}

#[tauri::command]
async fn app_ready_to_show(window: tauri::WebviewWindow, state: State<'_, AppState>) -> Result<(), String> {
    log_engine("app_ready_to_show invoked from frontend");
    let (is_popup, auto_start, anchor, is_silent) = {
        let guard = state.transfer_state.lock().await;
        let a = &guard.launch_args;
        (a.is_popup, a.auto_start, a.popup_anchor, a.is_silent)
    };

    // A --silent (background) instance stays hidden; forwarded popups/transfers show it later.
    if is_silent {
        log_engine("app_ready_to_show: silent mode, keeping the main window hidden");
        return Ok(());
    }

    // Size, place and show on the main thread so the popup is measured after resizing.
    let win = window.clone();
    window
        .run_on_main_thread(move || {
            if is_popup {
                #[cfg(target_os = "windows")]
                place_popup_window(&win, anchor);
                let _ = win.set_always_on_top(true);
            } else if auto_start {
                let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize {
                    width: 580.0,
                    height: 190.0,
                }));
            }

            let _ = win.show();
            let _ = win.unminimize();
            let _ = win.set_focus();
            #[cfg(target_os = "windows")]
            force_window_to_foreground(&win);
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn close_app(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    log_engine("close_app command invoked: exiting application cleanly");
    state.cancel_flag.store(true, Ordering::SeqCst);

    // Unblock any conflict sender so waiting threads can proceed
    let mut lock = state.conflict_sender.lock().await;
    if let Some(tx) = lock.take() {
        let _ = tx.send(ConflictResolutionChoice {
            action: "skip".to_string(),
            apply_to_all: false,
        });
    }

    // Hide windows immediately for instant UI feedback
    for (_, window) in app.webview_windows() {
        let _ = window.hide();
    }

    // If transfer worker is active, give it up to 1s to exit coordinator,
    // clean partial file, and finalize SQLite history records
    let start = std::time::Instant::now();
    while state.is_worker_running.load(Ordering::SeqCst) && start.elapsed() < std::time::Duration::from_millis(1000) {
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    }

    // Safety fallback: ensure any remaining partial files are deleted
    cleanup_partial_transfers(&state).await;

    // Apply any staged silent update on exit
    #[cfg(target_os = "windows")]
    apply_silent_update_on_exit();

    flush_telemetry();
    std::process::exit(0);
}

#[tauri::command]
async fn pick_source_files(window: Window) -> Result<Vec<String>, String> {
    let hwnd_raw = window.hwnd().ok().map(|h| h.0 as isize);
    tokio::task::spawn_blocking(move || dialog::pick_files(hwnd_raw))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn pick_source_folder(window: Window) -> Result<Option<String>, String> {
    let hwnd_raw = window.hwnd().ok().map(|h| h.0 as isize);
    tokio::task::spawn_blocking(move || dialog::pick_folder(hwnd_raw))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn pick_destination_folder(window: Window) -> Result<Option<String>, String> {
    let hwnd_raw = window.hwnd().ok().map(|h| h.0 as isize);
    tokio::task::spawn_blocking(move || dialog::pick_folder(hwnd_raw))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn resolve_destination_path(path: String) -> Result<String, String> {
    let clean = clean_path_str(&path);
    if clean.is_empty() {
        return Ok(String::new());
    }
    let p = std::path::Path::new(&clean);
    if p.is_file() {
        if let Some(parent) = p.parent() {
            return Ok(parent.to_string_lossy().to_string());
        }
    }
    Ok(clean)
}

#[derive(serde::Serialize)]
pub struct ScanPreviewResult {
    pub total_files: usize,
    pub total_bytes: u64,
    pub items: Vec<FileItem>,
}

#[tauri::command]
async fn scan_items_preview(sources: Vec<String>) -> Result<ScanPreviewResult, String> {
    let mut validated_sources = Vec::new();
    for s in &sources {
        let clean = clean_path_str(s);
        if clean.is_empty() {
            continue;
        }
        if let Some(stripped) = clean.strip_prefix('*') {
            if let Ok(list_path) = validate_filesystem_path(stripped) {
                if list_path.exists() {
                    validated_sources.push(format!("*{}", list_path.to_string_lossy()));
                }
            }
        } else if let Ok(src_path) = validate_filesystem_path(&clean) {
            if src_path.exists() {
                validated_sources.push(src_path.to_string_lossy().to_string());
            }
        }
    }
    if validated_sources.is_empty() {
        return Ok(ScanPreviewResult {
            total_files: 0,
            total_bytes: 0,
            items: Vec::new(),
        });
    }

    let dummy_dest = std::env::temp_dir();
    let scanned = tokio::task::spawn_blocking(move || {
        FileScanner::scan(&validated_sources, &dummy_dest)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let total_files = scanned.len();
    let total_bytes = scanned.iter().map(|i| i.size).sum();

    Ok(ScanPreviewResult {
        total_files,
        total_bytes,
        items: scanned,
    })
}

#[tauri::command]
async fn get_launch_args(state: State<'_, AppState>) -> Result<LaunchArgs, String> {
    let cur = state.transfer_state.lock().await;
    Ok(cur.launch_args.clone())
}

#[tauri::command]
async fn get_current_transfer_state(state: State<'_, AppState>) -> Result<CurrentTransferState, String> {
    let cur = state.transfer_state.lock().await;
    Ok(cur.clone())
}

#[tauri::command]
async fn log_js(msg: String) -> Result<(), String> {
    log_engine(&format!("FRONTEND JS: {}", msg));
    if msg.starts_with("WINDOW ERROR:") || msg.starts_with("UNHANDLED REJECTION:") {
        sentry::capture_message(&msg, sentry::Level::Error);
    }
    Ok(())
}

/// Sends queued Sentry events before the process exits. `std::process::exit` skips the guard's
/// drop in `main`, which would otherwise discard anything captured just before closing.
fn flush_telemetry() {
    if let Some(client) = sentry::Hub::main().client() {
        let flushed = client.flush(Some(std::time::Duration::from_secs(2)));
        log_engine(&format!("flush_telemetry: Sentry queue flushed={}", flushed));
    }
}

#[tauri::command]
async fn capture_sentry_message(msg: String, level: Option<String>) -> Result<(), String> {
    log_engine(&format!("capture_sentry_message: {}", msg));
    let s_level = match level.as_deref() {
        Some("warning") => sentry::Level::Warning,
        Some("info") => sentry::Level::Info,
        Some("fatal") => sentry::Level::Fatal,
        _ => sentry::Level::Error,
    };
    sentry::capture_message(&msg, s_level);
    Ok(())
}

fn run_reg(args: &[&str]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let output = std::process::Command::new("reg")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("reg.exe çalıştırılamadı: {}", e))?;
    if output.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let msg = if !err.is_empty() { err } else { out };
        Err(format!("Kayıt defteri hatası: {}", msg))
    }
}

fn get_context_menu_exe_path() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("Yürütülebilir dosya yolu alınamadı: {}", e))
}

#[tauri::command]
async fn is_shell_context_menu_registered() -> Result<bool, String> {
    tokio::task::spawn_blocking(|| {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let output = std::process::Command::new("reg")
            .args(["query", "HKCU\\Software\\Classes\\*\\shell\\DeepNodeCopy"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        match output {
            Ok(out) => Ok(out.status.success()),
            Err(_) => Ok(false),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn register_shell_context_menu(_app: AppHandle, lang: Option<String>) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let exe = get_context_menu_exe_path()?;
    let exe_str = exe.to_string_lossy().to_string();
    let exe_icon = format!("\"{}\",0", exe_str);

    let l = lang.as_deref().unwrap_or("tr");
    let (copy_title, move_title, paste_title, success_msg) = match l {
        "tr" => (
            "DeepNode Copy ile Kopyala",
            "DeepNode Copy ile Taşı",
            "DeepNode Copy ile Buraya Yapıştır",
            "Windows sağ tık menü entegrasyonu başarıyla kaydedildi!",
        ),
        "de" => (
            "Mit DeepNode Copy kopieren",
            "Mit DeepNode Copy verschieben",
            "Hier mit DeepNode Copy einfügen",
            "Windows-Kontextmenü erfolgreich registriert!",
        ),
        "es" => (
            "Copiar con DeepNode Copy",
            "Mover con DeepNode Copy",
            "Pegar aquí con DeepNode Copy",
            "¡Integración del menú contextual de Windows registrada con éxito!",
        ),
        "fr" => (
            "Copier avec DeepNode Copy",
            "Déplacer avec DeepNode Copy",
            "Coller ici avec DeepNode Copy",
            "Intégration du menu contextuel Windows enregistrée avec succès !",
        ),
        "it" => (
            "Copia con DeepNode Copy",
            "Sposta con DeepNode Copy",
            "Incolla qui con DeepNode Copy",
            "Integrazione del menu contestuale di Windows registrata con successo!",
        ),
        "pt" => (
            "Copiar com DeepNode Copy",
            "Mover com DeepNode Copy",
            "Colar aqui com DeepNode Copy",
            "Integração do menu de contexto do Windows registrada com sucesso!",
        ),
        "ru" => (
            "Копировать с DeepNode Copy",
            "Переместить с DeepNode Copy",
            "Вставить сюда с DeepNode Copy",
            "Интеграция с контекстным меню Windows успешно зарегистрирована!",
        ),
        "ja" => (
            "DeepNode Copy でコピー",
            "DeepNode Copy で移動",
            "DeepNode Copy でここに貼り付け",
            "Windows右クリックメニューの統合が完了しました！",
        ),
        "ko" => (
            "DeepNode Copy로 복사",
            "DeepNode Copy로 이동",
            "DeepNode Copy로 여기에 붙여넣기",
            "Windows 우클릭 메뉴 등록이 완료되었습니다!",
        ),
        "zh-CN" => (
            "使用 DeepNode Copy 复制",
            "使用 DeepNode Copy 移动",
            "使用 DeepNode Copy 粘贴到此处",
            "Windows 右键菜单集成已成功注册！",
        ),
        "zh-TW" => (
            "使用 DeepNode Copy 複製",
            "使用 DeepNode Copy 移動",
            "使用 DeepNode Copy 貼上至此處",
            "Windows 右鍵功能表整合已成功註冊！",
        ),
        _ => (
            "Copy with DeepNode Copy",
            "Move with DeepNode Copy",
            "Paste here with DeepNode Copy",
            "Windows context menu integration successfully registered!",
        ),
    };

    // 1. Files (*)
    run_reg(&["add", "HKCU\\Software\\Classes\\*\\shell\\DeepNodeCopy", "/ve", "/d", copy_title, "/f"])?;
    run_reg(&["add", "HKCU\\Software\\Classes\\*\\shell\\DeepNodeCopy", "/v", "Icon", "/d", &exe_icon, "/f"])?;
    run_reg(&["add", "HKCU\\Software\\Classes\\*\\shell\\DeepNodeCopy\\command", "/ve", "/d", &format!("\"{}\" --source \"%1\"", exe_str), "/f"])?;

    run_reg(&["add", "HKCU\\Software\\Classes\\*\\shell\\DeepNodeMove", "/ve", "/d", move_title, "/f"])?;
    run_reg(&["add", "HKCU\\Software\\Classes\\*\\shell\\DeepNodeMove", "/v", "Icon", "/d", &exe_icon, "/f"])?;
    run_reg(&["add", "HKCU\\Software\\Classes\\*\\shell\\DeepNodeMove\\command", "/ve", "/d", &format!("\"{}\" --mode move --source \"%1\"", exe_str), "/f"])?;

    // 2. Directories / Folders (Directory)
    run_reg(&["add", "HKCU\\Software\\Classes\\Directory\\shell\\DeepNodeCopy", "/ve", "/d", copy_title, "/f"])?;
    run_reg(&["add", "HKCU\\Software\\Classes\\Directory\\shell\\DeepNodeCopy", "/v", "Icon", "/d", &exe_icon, "/f"])?;
    run_reg(&["add", "HKCU\\Software\\Classes\\Directory\\shell\\DeepNodeCopy\\command", "/ve", "/d", &format!("\"{}\" --source \"%1\"", exe_str), "/f"])?;

    run_reg(&["add", "HKCU\\Software\\Classes\\Directory\\shell\\DeepNodeMove", "/ve", "/d", move_title, "/f"])?;
    run_reg(&["add", "HKCU\\Software\\Classes\\Directory\\shell\\DeepNodeMove", "/v", "Icon", "/d", &exe_icon, "/f"])?;
    run_reg(&["add", "HKCU\\Software\\Classes\\Directory\\shell\\DeepNodeMove\\command", "/ve", "/d", &format!("\"{}\" --mode move --source \"%1\"", exe_str), "/f"])?;

    // 3. Drives (Drive)
    run_reg(&["add", "HKCU\\Software\\Classes\\Drive\\shell\\DeepNodeCopy", "/ve", "/d", copy_title, "/f"])?;
    run_reg(&["add", "HKCU\\Software\\Classes\\Drive\\shell\\DeepNodeCopy", "/v", "Icon", "/d", &exe_icon, "/f"])?;
    run_reg(&["add", "HKCU\\Software\\Classes\\Drive\\shell\\DeepNodeCopy\\command", "/ve", "/d", &format!("\"{}\" --source \"%1\"", exe_str), "/f"])?;

    run_reg(&["add", "HKCU\\Software\\Classes\\Drive\\shell\\DeepNodeMove", "/ve", "/d", move_title, "/f"])?;
    run_reg(&["add", "HKCU\\Software\\Classes\\Drive\\shell\\DeepNodeMove", "/v", "Icon", "/d", &exe_icon, "/f"])?;
    run_reg(&["add", "HKCU\\Software\\Classes\\Drive\\shell\\DeepNodeMove\\command", "/ve", "/d", &format!("\"{}\" --mode move --source \"%1\"", exe_str), "/f"])?;

    // 4. Folder Background (Directory\Background)
    run_reg(&["add", "HKCU\\Software\\Classes\\Directory\\Background\\shell\\DeepNodeCopy", "/ve", "/d", paste_title, "/f"])?;
    run_reg(&["add", "HKCU\\Software\\Classes\\Directory\\Background\\shell\\DeepNodeCopy", "/v", "Icon", "/d", &exe_icon, "/f"])?;
    run_reg(&["add", "HKCU\\Software\\Classes\\Directory\\Background\\shell\\DeepNodeCopy\\command", "/ve", "/d", &format!("\"{}\" --target \"%V\"", exe_str), "/f"])?;

    // 5. Default Copy & Move Interception Handlers (CopyHook & DragDrop)
    let clsid_dd = "{6E8F3C72-A2D9-4E61-8C9B-93E48F1A7C21}";
    let clsid_ch = "{8F3A2C71-9B4E-4D26-A5F0-6E1D2B7C9A83}";
    // Explorer runs the DragDropHandlers chain for every drop and paste onto these keys
    // (same set as TeraCopy); the handler makes its item the default to take over the transfer.
    for target in &["Directory", "Folder", "Drive", "lnkfile"] {
        let _ = run_reg(&["add", &format!("HKCU\\Software\\Classes\\{}\\shellex\\DragDropHandlers\\DeepNodeCopy", target), "/ve", "/d", clsid_dd, "/f"]);
    }
    let _ = run_reg(&["delete", "HKCU\\Software\\Classes\\*\\shellex\\DragDropHandlers\\DeepNodeCopy", "/f"]);
    for target in &["Directory", "Folder"] {
        let _ = run_reg(&["add", &format!("HKCU\\Software\\Classes\\{}\\shellex\\CopyHookHandlers\\DeepNodeCopy", target), "/ve", "/d", clsid_ch, "/f"]);
    }

    // Older builds replaced Explorer's own folder DropHandler with ours, which bypasses the
    // DragDropHandlers chain. The DLL's DllRegisterServer (regsvr32 below) resets the
    // DropHandler value where it still points at our CLSID.
    for target in &["Directory", "Folder", "Directory\\Background", "Drive"] {
        let _ = run_reg(&["delete", &format!("HKCU\\Software\\Classes\\{}\\shellex\\DropHandler\\DeepNodeCopy", target), "/f"]);
    }

    // 6. InprocServer32 DLL Registration via regsvr32
    if let Some(exe_dir) = exe.parent() {
        let dll_candidates = [
            exe_dir.join("deepnode_shell.dll"),
            exe_dir.join("bin").join("deepnode_shell.dll"),
            exe_dir.parent().map(|p| p.join("bin").join("deepnode_shell.dll")).unwrap_or_default(),
        ];
        if let Some(dll_path) = dll_candidates.iter().find(|p| p.exists()) {
            let _ = std::process::Command::new("regsvr32")
                .args(["/s", &dll_path.to_string_lossy()])
                .creation_flags(0x08000000)
                .output();
        }
    }

    // 7. Windows 11 Modern Context Menu (Sparse Package) if manifest exists
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    if let Some(exe_dir) = exe.parent() {
        let manifest_candidates = [
            exe_dir.join("AppxManifest.xml"),
            exe_dir.join("resources").join("AppxManifest.xml"),
            exe_dir.join("resources").join("bin").join("AppxManifest.xml"),
            exe_dir.join("bin").join("AppxManifest.xml"),
            exe_dir.parent().map(|p| p.join("bin").join("AppxManifest.xml")).unwrap_or_default(),
        ];
        if let Some(manifest) = manifest_candidates.iter().find(|p| p.exists()) {
            let manifest_dir = manifest.parent().unwrap_or(exe_dir);
            let cer_path = manifest_dir.join("DeepNodePublic.cer");
            if cer_path.exists() {
                let _ = std::process::Command::new("powershell")
                    .args([
                        "-NoProfile",
                        "-Command",
                        &format!("Import-Certificate -FilePath '{}' -CertStoreLocation 'Cert:\\CurrentUser\\TrustedPeople' -ErrorAction SilentlyContinue", cer_path.display()),
                    ])
                    .creation_flags(CREATE_NO_WINDOW)
                    .output();
            }
            let _ = std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    &format!("Add-AppxPackage -Path '{}' -Register -ExternalLocation '{}' -ErrorAction SilentlyContinue", manifest.display(), manifest_dir.display()),
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
        }
    }

        Ok(success_msg.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn unregister_shell_context_menu(lang: Option<String>) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let _ = run_reg(&["delete", "HKCU\\Software\\Classes\\*\\shell\\DeepNodeCopy", "/f"]);
        let _ = run_reg(&["delete", "HKCU\\Software\\Classes\\*\\shell\\DeepNodeMove", "/f"]);
        let _ = run_reg(&["delete", "HKCU\\Software\\Classes\\Directory\\shell\\DeepNodeCopy", "/f"]);
        let _ = run_reg(&["delete", "HKCU\\Software\\Classes\\Directory\\shell\\DeepNodeMove", "/f"]);
        let _ = run_reg(&["delete", "HKCU\\Software\\Classes\\Directory\\Background\\shell\\DeepNodeCopy", "/f"]);
        let _ = run_reg(&["delete", "HKCU\\Software\\Classes\\Drive\\shell\\DeepNodeCopy", "/f"]);
        let _ = run_reg(&["delete", "HKCU\\Software\\Classes\\Drive\\shell\\DeepNodeMove", "/f"]);

        for target in &["*", "Directory", "Folder", "Drive", "lnkfile"] {
            let _ = run_reg(&["delete", &format!("HKCU\\Software\\Classes\\{}\\shellex\\DragDropHandlers\\DeepNodeCopy", target), "/f"]);
        }
        for target in &["Directory", "Folder"] {
            let _ = run_reg(&["delete", &format!("HKCU\\Software\\Classes\\{}\\shellex\\CopyHookHandlers\\DeepNodeCopy", target), "/f"]);
        }
        for target in &["Directory", "Folder", "Directory\\Background", "Drive"] {
            let _ = run_reg(&["delete", &format!("HKCU\\Software\\Classes\\{}\\shellex\\DropHandler\\DeepNodeCopy", target), "/f"]);
        }
        let _ = run_reg(&["delete", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", "/v", "DeepNode Copy", "/f"]);

        let _ = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-AppxPackage -Name '*DeepNode*' | Remove-AppxPackage -ErrorAction SilentlyContinue",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        let l = lang.as_deref().unwrap_or("tr");
        let msg = match l {
            "tr" => "Windows sağ tık menü entegrasyonu başarıyla kaldırıldı.",
            "de" => "Windows-Kontextmenü erfolgreich entfernt.",
            "es" => "Integración del menú contextual de Windows eliminada con éxito.",
            "fr" => "Intégration du menu contextuel Windows supprimée avec succès.",
            _ => "Windows context menu integration successfully removed.",
        };
        Ok(msg.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct VerifyResult {
    pub success: bool,
    pub src_hash: String,
    pub dst_hash: Option<String>,
    pub algorithm: String,
    pub calculated: String,
    pub matches: Option<bool>,
}

#[tauri::command]
async fn verify_file(
    file_path: Option<String>,
    src: Option<String>,
    dst: Option<String>,
    expected: Option<String>,
    algo: Option<String>,
    algorithm: Option<String>,
) -> Result<VerifyResult, String> {
    let target = file_path
        .or(src)
        .ok_or_else(|| "Doğrulanacak dosya yolu belirtilmedi".to_string())?;
    let target_path = validate_filesystem_path(&target)?;
    if !target_path.exists() {
        return Err(format!("Dosya bulunamadı: {:?}", target_path));
    }
    if !target_path.is_file() {
        return Err(format!("Belirtilen yol bir dosya değil: {:?}", target_path));
    }

    let algo_choice = algo.or(algorithm);
    let expected_clean = expected.map(|e| e.trim().to_lowercase()).filter(|e| !e.is_empty());

    let hash_algo = match algo_choice.as_deref() {
        Some("xxhash128") | Some("xxhash3_128") => HashAlgorithm::XxHash3_128,
        Some("blake3") => HashAlgorithm::Blake3,
        Some("md5") => HashAlgorithm::Md5,
        Some("sha1") => HashAlgorithm::Sha1,
        Some("sha256") => HashAlgorithm::Sha256,
        Some("xxhash64") | Some("xxhash3_64") | Some("xxhash") => HashAlgorithm::XxHash3_64,
        _ => {
            if let Some(ref exp) = expected_clean {
                match exp.len() {
                    16 => HashAlgorithm::XxHash3_64,
                    32 => HashAlgorithm::Md5,
                    40 => HashAlgorithm::Sha1,
                    64 => HashAlgorithm::Sha256,
                    _ => HashAlgorithm::XxHash3_64,
                }
            } else {
                HashAlgorithm::XxHash3_64
            }
        }
    };

    let (calculated, dst_hash_res, matches) = tokio::task::spawn_blocking(move || -> Result<(String, Option<String>, Option<bool>), String> {
        let calculated = HashVerifier::compute_file_hash(&target_path, hash_algo, false)
            .map_err(|e| format!("Sağlama hesaplama hatası: {}", e))?;

        let mut matches = None;
        let mut dst_hash_res = None;

        if let Some(ref exp) = expected_clean {
            matches = Some(calculated.eq_ignore_ascii_case(exp));
        } else if let Some(ref dst_val) = dst {
            let dst_path = validate_filesystem_path(dst_val)?;
            if dst_path.exists() && dst_path.is_file() {
                let h = HashVerifier::compute_file_hash(&dst_path, hash_algo, true)
                    .map_err(|e| format!("Hedef sağlama hatası: {}", e))?;
                matches = Some(calculated.eq_ignore_ascii_case(&h));
                dst_hash_res = Some(h);
            }
        }

        Ok((calculated, dst_hash_res, matches))
    })
    .await
    .map_err(|e| e.to_string())??;

    let success = matches.unwrap_or(true);

    Ok(VerifyResult {
        success,
        src_hash: calculated.clone(),
        dst_hash: dst_hash_res,
        algorithm: format!("{:?}", hash_algo),
        calculated,
        matches,
    })
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct StartTransferResult {
    pub total_files: usize,
    pub total_bytes: u64,
    pub items: Vec<FileItem>,
}

fn spawn_transfer_worker(
    app: AppHandle,
    state: Arc<AppState>,
    items: Vec<FileItem>,
    config: TransferConfig,
    on_finish: String,
    destination: Option<PathBuf>,
) -> Result<StartTransferResult, String> {
    if state.is_worker_running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Err("Aktif bir aktarım iş parçacığı zaten çalışıyor".to_string());
    }

    let total_bytes: u64 = items.iter().map(|i| i.size).sum();
    let total_files = items.len();

    // Prepare transfer session record for history database
    let session_uuid = uuid::Uuid::new_v4().to_string();
    let source_summary = if !items.is_empty() {
        let first = items[0].src_path.file_name().and_then(|n| n.to_str()).unwrap_or("Dosya");
        if items.len() > 1 {
            format!("{} (+{} öğe)", first, items.len() - 1)
        } else {
            first.to_string()
        }
    } else {
        "Boş aktarım".to_string()
    };
    let dest_str = destination.as_ref().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
    let mode_str = format!("{:?}", config.mode).to_lowercase();

    let session_rec = deepnode_db::SessionRecord {
        id: None,
        session_uuid,
        start_time: chrono::Utc::now(),
        end_time: None,
        mode: mode_str,
        source_summary,
        destination: dest_str,
        total_files,
        completed_files: 0,
        skipped_files: 0,
        failed_files: 0,
        total_bytes,
        transferred_bytes: 0,
        average_speed_bps: 0.0,
        status: "in_progress".to_string(),
    };

    let db_for_worker = Arc::clone(&state.db);

    state.pause_flag.store(false, Ordering::Relaxed);
    state.cancel_flag.store(false, Ordering::Relaxed);
    state.skip_flag.store(false, Ordering::SeqCst);
    state.skipped_ids.lock().unwrap_or_else(|e| e.into_inner()).clear();

    let pause_flag = Arc::clone(&state.pause_flag);
    let cancel_flag = Arc::clone(&state.cancel_flag);
    let last_failed = Arc::clone(&state.last_failed_items);
    let conflict_sender = Arc::clone(&state.conflict_sender);
    let transfer_state_arc = Arc::clone(&state.transfer_state);

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    let mut coordinator = TransferCoordinator::new(
        config,
        items.clone(),
        event_tx,
        pause_flag,
        Arc::clone(&cancel_flag),
    );
    coordinator.set_skip_controls(Arc::clone(&state.skip_flag), Some(Arc::clone(&state.skipped_ids)));
    if let Some(dest) = destination {
        coordinator.set_destination_dir(dest);
    }
    coordinator.set_conflict_hook(conflict_sender);

    // Initial event emission
    let init_event = EngineEvent::TransferStarted {
        total_files,
        total_bytes,
        items: items.clone(),
    };
    let _ = app.emit("engine_event", &init_event);

    // Event forwarder and state updater
    let app_event = app.clone();
    let ts_for_events = Arc::clone(&transfer_state_arc);
    let cancel_for_events = Arc::clone(&cancel_flag);
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            {
                let mut cur = ts_for_events.lock().await;
                match &event {
                    EngineEvent::ConflictDetected(info) => {
                        log_engine(&format!("Event loop: ConflictDetected for {}", info.filename));
                        cur.active_conflict = Some(info.clone());
                    }
                    EngineEvent::ProgressUpdated(stats) => {
                        cur.stats = stats.clone();
                    }
                    EngineEvent::FileStarted(item) => {
                        cur.active_conflict = None;
                        cur.stats.current_file_name = item.relative_path.to_string_lossy().to_string();
                        cur.stats.current_file_size = item.size;
                        if let Some(pos) = cur.files.iter_mut().find(|f| f.id == item.id) {
                            pos.status = FileItemStatus::Copying;
                        }
                    }
                    EngineEvent::FileProgress { id, transferred, .. } => {
                        cur.stats.current_file_transferred = *transferred;
                        if let Some(pos) = cur.files.iter_mut().find(|f| f.id == *id) {
                            pos.bytes_transferred = *transferred;
                            pos.status = FileItemStatus::Copying;
                        }
                    }
                    EngineEvent::FileCompleted(item) => {
                        cur.active_conflict = None;
                        if let Some(pos) = cur.files.iter_mut().find(|f| f.id == item.id) {
                            pos.status = FileItemStatus::Completed;
                            pos.bytes_transferred = item.size;
                        }
                    }
                    EngineEvent::FileSkipped { item, .. } => {
                        cur.active_conflict = None;
                        if let Some(pos) = cur.files.iter_mut().find(|f| f.id == item.id) {
                            pos.status = FileItemStatus::Skipped;
                        }
                    }
                    EngineEvent::FileFailed { item, .. } => {
                        cur.active_conflict = None;
                        if let Some(pos) = cur.files.iter_mut().find(|f| f.id == item.id) {
                            pos.status = FileItemStatus::Failed;
                        }
                    }
                    EngineEvent::TransferPaused => {
                        cur.status = "paused".to_string();
                    }
                    EngineEvent::TransferResumed => {
                        cur.status = "transferring".to_string();
                    }
                    EngineEvent::TransferCompleted(stats) => {
                        cur.active_conflict = None;
                        let is_canc = cancel_for_events.load(Ordering::SeqCst) || cur.status == "canceled";
                        if !is_canc {
                            cur.status = "completed".to_string();
                            cur.stats = stats.clone();
                            let _ = app_event.emit("engine_event", &event);
                        } else {
                            cur.status = "canceled".to_string();
                            let _ = app_event.emit("engine_event", &EngineEvent::TransferCanceled);
                        }
                        continue;
                    }
                    EngineEvent::TransferCanceled => {
                        cur.active_conflict = None;
                        cur.status = "canceled".to_string();
                        for file in cur.files.iter_mut() {
                            if file.status == FileItemStatus::Copying || file.status == FileItemStatus::Pending {
                                file.status = FileItemStatus::Canceled;
                            }
                        }
                    }
                    EngineEvent::TransferFailed(_) => {
                        cur.active_conflict = None;
                        cur.status = "failed".to_string();
                    }
                    _ => {}
                }
            }
            let _ = app_event.emit("engine_event", &event);
        }
    });

    let on_finish_action = match on_finish.as_str() {
        "shutdown" => OnFinishAction::Shutdown,
        "sleep" => OnFinishAction::Sleep,
        "hibernate" => OnFinishAction::Hibernate,
        "close_app" => OnFinishAction::CloseApp,
        _ => OnFinishAction::DoNothing,
    };

    let app_coord = app.clone();
    let ts_worker = Arc::clone(&transfer_state_arc);
    let worker_running_flag = Arc::clone(&state.is_worker_running);
    let state_spawn_err = Arc::clone(&state);
    let cancel_for_worker = Arc::clone(&cancel_flag);
    let thread_spawn_res = std::thread::Builder::new()
        .name("deepnode-transfer-worker".to_string())
        .spawn(move || {
            struct WorkerGuard(Arc<AtomicBool>);
            impl Drop for WorkerGuard {
                fn drop(&mut self) {
                    self.0.store(false, Ordering::SeqCst);
                    log_engine("Transfer worker thread terminated: is_worker_running reset to false");
                }
            }
            let _worker_guard = WorkerGuard(worker_running_flag);
            log_engine("Transfer worker thread spawned");
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            if let Ok(runtime) = rt {
                runtime.block_on(async move {
                    let session_id_opt = {
                        let db_guard = db_for_worker.lock().await;
                        if let Some(ref db) = *db_guard {
                            match db.insert_session(&session_rec) {
                                Ok(id) => Some(id),
                                Err(err) => {
                                    log_engine(&format!("CRITICAL: Failed to record transfer session in DB: {}", err));
                                    tracing::error!("Failed to record transfer session in DB: {}", err);
                                    None
                                }
                            }
                        } else {
                            log_engine("WARNING: History database is not available for this session");
                            None
                        }
                    };

                    log_engine("worker thread calling coordinator.run()...");
                    let stats_result = coordinator.run().await;
                    let failed_items = coordinator.get_failed_items();
                    *last_failed.lock().await = failed_items.clone();
                    log_engine(&format!(
                        "coordinator.run completed: stats_result={:?}",
                        stats_result.as_ref().map(|s| (s.completed_files, s.total_files, s.transferred_bytes))
                    ));

                    let is_canc = cancel_for_worker.load(Ordering::SeqCst);
                    if is_canc {
                        log_engine("Worker thread: cancel_flag is true, reporting canceled");
                        let mut cur = ts_worker.lock().await;
                        cur.status = "canceled".to_string();
                        for file in cur.files.iter_mut() {
                            if file.status == FileItemStatus::Copying || file.status == FileItemStatus::Pending {
                                file.status = FileItemStatus::Canceled;
                            }
                        }
                        let _ = app_coord.emit("engine_event", &EngineEvent::TransferCanceled);
                    } else {
                        match stats_result {
                            Ok(_stats) => {
                                let mut cur = ts_worker.lock().await;
                                if !cancel_for_worker.load(Ordering::SeqCst) && cur.status != "canceled" {
                                    cur.status = "completed".to_string();
                                } else {
                                    cur.status = "canceled".to_string();
                                    let _ = app_coord.emit("engine_event", &EngineEvent::TransferCanceled);
                                }
                            }
                            Err(deepnode_core::error::TransferError::Canceled) => {
                                log_engine("coordinator returned Canceled");
                                let mut cur = ts_worker.lock().await;
                                cur.status = "canceled".to_string();
                                let _ = app_coord.emit("engine_event", &EngineEvent::TransferCanceled);
                            }
                            Err(e) => {
                                log_engine(&format!("coordinator returned error: {:?}", e));
                                sentry::capture_message(&format!("Transfer failed: {}", e), sentry::Level::Error);
                                tracing::error!("Transfer failed with error: {:?}", e);
                                {
                                    let mut cur = ts_worker.lock().await;
                                    cur.status = "failed".to_string();
                                }
                                let _ = app_coord.emit("engine_event", &EngineEvent::TransferFailed(e.to_string()));
                            }
                        }
                    }

                    // 1. Write SQLite history records FIRST before executing OnFinish action (especially CloseApp/Shutdown)
                    if let Some(session_id) = session_id_opt {
                        let cur = ts_worker.lock().await;
                        let final_status = if is_canc || cur.status == "canceled" {
                            "canceled"
                        } else if !failed_items.is_empty() || cur.status == "failed" {
                            "failed"
                        } else {
                            "completed"
                        };

                        let file_records: Vec<deepnode_db::FileRecord> = cur.files.iter().map(|f| deepnode_db::FileRecord {
                            id: None,
                            session_id,
                            src_path: f.src_path.to_string_lossy().to_string(),
                            dst_path: f.dst_path.to_string_lossy().to_string(),
                            size: f.size,
                            status: match f.status {
                                FileItemStatus::Completed => "completed".to_string(),
                                FileItemStatus::Failed => "failed".to_string(),
                                FileItemStatus::Skipped => "skipped".to_string(),
                                FileItemStatus::Canceled => "canceled".to_string(),
                                _ => "pending".to_string(),
                            },
                            src_hash: f.src_hash.clone(),
                            dst_hash: f.dst_hash.clone(),
                            error_message: f.error_message.clone(),
                            retry_count: f.retry_attempts,
                        }).collect();

                        let mut db_guard = db_for_worker.lock().await;
                        if let Some(ref mut db) = *db_guard {
                            if let Err(e) = db.insert_files_batch(session_id, &file_records) {
                                log_engine(&format!("CRITICAL: insert_files_batch failed: {}", e));
                                tracing::error!("insert_files_batch failed: {}", e);
                            }
                            if let Err(e) = db.update_session_finish(
                                session_id,
                                final_status,
                                cur.stats.completed_files,
                                cur.stats.skipped_files,
                                cur.stats.failed_files,
                                cur.stats.transferred_bytes,
                                cur.stats.average_speed_bps as f64,
                            ) {
                                log_engine(&format!("CRITICAL: update_session_finish failed: {}", e));
                                tracing::error!("update_session_finish failed: {}", e);
                            }
                        }
                    }

                    // 2. NOW execute completion sound and OnFinish action
                    let current_worker_status = {
                        let cur = ts_worker.lock().await;
                        cur.status.clone()
                    };

                    if current_worker_status == "completed" {
                        let has_errors = !failed_items.is_empty();
                        OnFinishExecutor::play_completion_sound(has_errors);
                        if let Err(err_msg) = OnFinishExecutor::execute(on_finish_action) {
                            log_engine(&format!("CRITICAL: OnFinishExecutor error: {}", err_msg));
                            tracing::error!("OnFinishExecutor error: {}", err_msg);
                        }
                    } else if current_worker_status == "failed" {
                        OnFinishExecutor::play_completion_sound(true);
                    }
                });
            } else {
                log_engine("Failed to create worker thread runtime!");
            }
        });

    if let Err(e) = thread_spawn_res {
        state_spawn_err.is_worker_running.store(false, Ordering::SeqCst);
        return Err(e.to_string());
    }

    Ok(StartTransferResult {
        total_files,
        total_bytes,
        items,
    })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn start_transfer(
    app: AppHandle,
    state: State<'_, AppState>,
    sources: Vec<String>,
    destination: String,
    mode: Option<String>,
    verify_mode: Option<String>,
    hash_algorithm: Option<String>,
    conflict_policy: Option<String>,
    direct_io: Option<bool>,
    on_finish: Option<String>,
) -> Result<StartTransferResult, String> {
    {
        let mut cur = state.transfer_state.lock().await;
        if cur.status == "transferring" || cur.status == "paused" || cur.status == "scanning" {
            return Err("Aktif bir aktarım zaten devam ediyor".to_string());
        }
        if state.is_worker_running.load(Ordering::SeqCst) {
            return Err("Aktif bir aktarım zaten devam ediyor".to_string());
        }
        cur.status = "scanning".to_string();
    }

    let dest_buf = match validate_filesystem_path(&destination) {
        Ok(buf) => {
            if buf.is_file() {
                if let Some(parent) = buf.parent() {
                    parent.to_path_buf()
                } else {
                    let mut cur = state.transfer_state.lock().await;
                    cur.status = "idle".to_string();
                    return Err("Hedef yol bir dosya olamaz, geçerli bir klasör olmalıdır".to_string());
                }
            } else {
                buf
            }
        }
        Err(e) => {
            let mut cur = state.transfer_state.lock().await;
            cur.status = "idle".to_string();
            return Err(format!("Geçersiz hedef klasör yolu: {}", e));
        }
    };

    if sources.is_empty() {
        let mut cur = state.transfer_state.lock().await;
        cur.status = "idle".to_string();
        return Err("Kaynak dosya veya klasör belirtilmedi".to_string());
    }

    let mut validated_sources = Vec::new();
    for s in &sources {
        let clean = clean_path_str(s);
        if clean.is_empty() {
            continue;
        }
        if let Some(stripped) = clean.strip_prefix('*') {
            let list_path = match validate_filesystem_path(stripped) {
                Ok(p) => p,
                Err(e) => {
                    let mut cur = state.transfer_state.lock().await;
                    cur.status = "idle".to_string();
                    return Err(format!("Geçersiz liste dosyası yolu: {}", e));
                }
            };
            if list_path.exists() {
                validated_sources.push(format!("*{}", list_path.to_string_lossy()));
            }
        } else {
            let src_path = match validate_filesystem_path(&clean) {
                Ok(p) => p,
                Err(e) => {
                    let mut cur = state.transfer_state.lock().await;
                    cur.status = "idle".to_string();
                    return Err(format!("Geçersiz kaynak yolu: {}", e));
                }
            };
            if src_path.exists() {
                validated_sources.push(src_path.to_string_lossy().to_string());
            }
        }
    }

    if validated_sources.is_empty() {
        let mut cur = state.transfer_state.lock().await;
        cur.status = "idle".to_string();
        return Err("Geçerli hiçbir kaynak dosya veya klasör bulunamadı".to_string());
    }

    let scan_sources = validated_sources.clone();
    let scan_dest = dest_buf.clone();
    let scan_res = tokio::task::spawn_blocking(move || {
        FileScanner::scan(&scan_sources, &scan_dest)
    })
    .await
    .map_err(|e| e.to_string())?;

    let items = match scan_res {
        Ok(items) if !items.is_empty() => items,
        Ok(_) => {
            let mut cur = state.transfer_state.lock().await;
            cur.status = "idle".to_string();
            return Err("No valid files found in source paths".to_string());
        }
        Err(e) => {
            let mut cur = state.transfer_state.lock().await;
            cur.status = "idle".to_string();
            return Err(e.to_string());
        }
    };

    let t_mode = match mode.as_deref().unwrap_or("copy") {
        "move" => TransferMode::Move,
        _ => TransferMode::Copy,
    };

    let v_mode = match verify_mode.as_deref().unwrap_or("always") {
        "never" => VerifyMode::Never,
        "ondifference" | "difference" => VerifyMode::OnDifferenceOnly,
        _ => VerifyMode::Always,
    };

    let h_algo = match hash_algorithm.as_deref().unwrap_or("xxhash3") {
        "xxhash128" => HashAlgorithm::XxHash3_128,
        "blake3" => HashAlgorithm::Blake3,
        "md5" => HashAlgorithm::Md5,
        "sha1" => HashAlgorithm::Sha1,
        "sha256" => HashAlgorithm::Sha256,
        _ => HashAlgorithm::XxHash3_64,
    };

    let c_policy = match conflict_policy.as_deref().unwrap_or("ask").to_lowercase().as_str() {
        "overwrite" | "overwriteall" => ConflictPolicy::OverwriteAll,
        "skip" | "skipall" => ConflictPolicy::SkipAll,
        "older" | "overwriteolder" => ConflictPolicy::OverwriteOlder,
        "rename" | "autorename" => ConflictPolicy::AutoRename,
        _ => ConflictPolicy::AskUser,
    };

    let config = TransferConfig {
        mode: t_mode,
        buffer_size: 2 * 1024 * 1024,
        direct_io: direct_io.unwrap_or(false),
        conflict_policy: c_policy,
        verify_mode: v_mode,
        hash_algorithm: h_algo,
        preserve_timestamps: true,
        preserve_attributes: true,
        preserve_ads: true,
        preserve_acl: false,
        use_vss_for_locked: true,
        retry_count: 3,
        retry_timeout_secs: 30,
        export_checksum_file: false,
        write_checksum_to_ads: false,
    };

    *state.last_config.lock().await = Some(config.clone());
    {
        let mut cur = state.transfer_state.lock().await;
        cur.status = "transferring".to_string();
        cur.files = items.clone();
        cur.stats.total_files = items.len();
        cur.stats.total_bytes = items.iter().map(|i| i.size).sum();
        cur.active_conflict = None;
    }

    let state_arc = Arc::new((*state).clone());

    let spawn_res = spawn_transfer_worker(
        app,
        state_arc,
        items,
        config,
        on_finish.unwrap_or_else(|| "do_nothing".to_string()),
        Some(dest_buf),
    );

    if spawn_res.is_err() {
        let mut cur = state.transfer_state.lock().await;
        cur.status = "idle".to_string();
    }

    spawn_res
}

#[tauri::command]
async fn retry_failed_transfer(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut retry_items = {
        let failed_lock = state.last_failed_items.lock().await;
        failed_lock.clone()
    };

    if retry_items.is_empty() {
        return Err("No failed items to retry".to_string());
    }

    // Reset status of all retry items to Pending and clear transient failure errors
    for item in retry_items.iter_mut() {
        item.status = FileItemStatus::Pending;
        item.bytes_transferred = 0;
        item.error_message = None;
    }

    let config_opt = state.last_config.lock().await.clone();
    let config = config_opt.unwrap_or_default();

    let dest_buf = {
        let cur = state.transfer_state.lock().await;
        cur.launch_args.destination.as_ref().map(PathBuf::from)
            .or_else(|| retry_items.first().and_then(|i| i.dst_path.parent().map(|p| p.to_path_buf())))
    };

    {
        let mut cur = state.transfer_state.lock().await;
        if cur.status == "transferring" || cur.status == "paused" || cur.status == "scanning" {
            return Err("Aktif bir aktarım zaten devam ediyor".to_string());
        }
        if state.is_worker_running.load(Ordering::SeqCst) {
            return Err("Aktif bir aktarım zaten devam ediyor".to_string());
        }
        cur.status = "transferring".to_string();
        cur.files = retry_items.clone();
        cur.stats = deepnode_core::engine::stats::TransferStats::default();
        cur.stats.total_files = retry_items.len();
        cur.stats.total_bytes = retry_items.iter().map(|i| i.size).sum();
        cur.active_conflict = None;
    }

    let state_arc = Arc::new((*state).clone());
    let res = spawn_transfer_worker(
        app,
        state_arc,
        retry_items,
        config,
        "do_nothing".to_string(),
        dest_buf,
    );

    if res.is_err() {
        let mut cur = state.transfer_state.lock().await;
        cur.status = "idle".to_string();
    }
    res?;
    Ok(())
}

#[tauri::command]
async fn resolve_conflict(
    state: State<'_, AppState>,
    action: String,
    apply_to_all: bool,
) -> Result<(), String> {
    log_engine(&format!("resolve_conflict invoked: action={}, apply_to_all={}", action, apply_to_all));
    {
        let mut cur = state.transfer_state.lock().await;
        cur.active_conflict = None;
    }
    let mut lock = state.conflict_sender.lock().await;
    if let Some(tx) = lock.take() {
        let _ = tx.send(ConflictResolutionChoice {
            action,
            apply_to_all,
        });
    }
    Ok(())
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    let lower = trimmed.to_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") && !lower.starts_with("mailto:") {
        return Err("Geçersiz URL şeması: yalnızca http, https veya mailto desteklenir".to_string());
    }

    // Reject control characters, quotes, and command-injection meta-characters
    if trimmed.chars().any(|c| c.is_control() || c == '"' || c == '\'' || c == '`' || c == '<' || c == '>' || c == '^' || c == '&' || c == '|') {
        return Err("Güvenlik ihlali: URL geçersiz veya tehlikeli karakterler içeriyor".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = OsStr::new(trimmed).encode_wide().chain(std::iter::once(0)).collect();
        unsafe {
            let res = windows::Win32::UI::Shell::ShellExecuteW(
                windows::Win32::Foundation::HWND(std::ptr::null_mut()),
                windows::core::w!("open"),
                windows::core::PCWSTR(wide.as_ptr()),
                windows::core::PCWSTR::null(),
                windows::core::PCWSTR::null(),
                windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            );
            if res.0 as usize <= 32 {
                return Err(format!("URL açılamadı: ShellExecuteW hata kodu {}", res.0 as usize));
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct UpdateProgressPayload {
    pub status: String,
    pub progress: u32,
    pub error: Option<String>,
}

#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
fn is_portable_app() -> bool {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            if parent.join("uninstall.exe").exists()
                || parent.join("Uninstall DeepNode Copy.exe").exists()
            {
                return false;
            }
        }
        let s = exe.to_string_lossy().to_lowercase();
        !(s.contains("program files")
            || s.contains("appdata\\local\\programs")
            || s.contains("appdata\\local\\deepnode copy"))
    } else {
        true
    }
}

static UPDATE_INSTALLER_PATH: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);
static IS_DOWNLOADING_UPDATE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn validate_update_url(url_str: &str) -> Result<reqwest::Url, String> {
    let parsed = reqwest::Url::parse(url_str).map_err(|e| format!("Geçersiz URL: {}", e))?;
    if parsed.scheme() != "https" {
        return Err("Güncelleme kaynağı yalnızca güvenli HTTPS protokolü kullanabilir".to_string());
    }

    let host = parsed.host_str().unwrap_or("").to_lowercase();
    let path = parsed.path();

    let is_valid = match host.as_str() {
        "github.com" => path.starts_with("/deepnodestudios/deepnode-copy/releases/download/"),
        "objects.githubusercontent.com" | "github-releases.githubusercontent.com" => true,
        "deepnodestudios.com" | "www.deepnodestudios.com" => true,
        _ => false,
    };

    if !is_valid {
        return Err(format!("Güvenilmeyen güncelleme kaynağı/alan adı: {}", host));
    }

    Ok(parsed)
}

fn verify_installer_authenticode(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let path_str = path.to_string_lossy().to_string();
    let ps_cmd = format!(
        "$sig = Get-AuthenticodeSignature -FilePath '{}'; \
         if ($sig.Status -eq 'Valid' -or ($sig.SignerCertificate -ne $null -and $sig.SignerCertificate.Subject -like '*DeepNode*')) {{ \
             exit 0 \
         }} else {{ \
             exit 1 \
         }}",
        path_str.replace('\'', "''")
    );

    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps_cmd])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("İmza doğrulama süreci çalıştırılamadı: {}", e))?;

    if !output.status.success() {
        log_engine(&format!("CRITICAL: Installer authenticode validation failed for {:?}", path));
        return Err("Güvenlik Hatası: Güncelleme yükleyicisi geçerli bir DeepNode dijital imzasına sahip değil veya bozulmuş!".to_string());
    }

    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ReleaseAssetInfo {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct LatestReleaseInfo {
    pub tag_name: String,
    pub name: String,
    pub body: Option<String>,
    pub html_url: String,
    pub assets: Vec<ReleaseAssetInfo>,
}

#[tauri::command]
async fn check_latest_release() -> Result<LatestReleaseInfo, String> {
    let client = reqwest::Client::builder()
        .user_agent(format!("DeepNodeCopy/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("HTTP client oluşturulamadı: {}", e))?;

    let url = "https://api.github.com/repos/deepnodestudios/deepnode-copy/releases/latest";
    let res = client
        .get(url)
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .map_err(|e| format!("GitHub API bağlantı hatası: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("GitHub API yanıt vermedi: HTTP {}", res.status()));
    }

    let val: serde_json::Value = res.json().await.map_err(|e| format!("JSON ayrıştırma hatası: {}", e))?;

    let tag_name = val["tag_name"].as_str().unwrap_or("").to_string();
    let name = val["name"].as_str().unwrap_or(&tag_name).to_string();
    let body = val["body"].as_str().map(|s| s.to_string());
    let html_url = val["html_url"].as_str().unwrap_or("").to_string();

    let mut assets = Vec::new();
    if let Some(arr) = val["assets"].as_array() {
        for a in arr {
            let aname = a["name"].as_str().unwrap_or("").to_string();
            let durl = a["browser_download_url"].as_str().unwrap_or("").to_string();
            let size = a["size"].as_u64().unwrap_or(0);
            if !aname.is_empty() && !durl.is_empty() {
                assets.push(ReleaseAssetInfo {
                    name: aname,
                    browser_download_url: durl,
                    size,
                });
            }
        }
    }

    Ok(LatestReleaseInfo {
        tag_name,
        name,
        body,
        html_url,
        assets,
    })
}

fn compare_semver(v1: &str, v2: &str) -> i32 {
    let parse = |v: &str| -> Vec<u32> {
        let clean = v.trim_start_matches('v').trim_start_matches('V');
        let base = clean.split('-').next().unwrap_or(clean);
        base.split('.')
            .map(|p| p.parse::<u32>().unwrap_or(0))
            .collect()
    };
    let p1 = parse(v1);
    let p2 = parse(v2);
    for i in 0..3 {
        let n1 = p1.get(i).copied().unwrap_or(0);
        let n2 = p2.get(i).copied().unwrap_or(0);
        if n1 > n2 {
            return 1;
        }
        if n1 < n2 {
            return -1;
        }
    }
    0
}

#[tauri::command]
fn get_staged_update() -> Option<serde_json::Value> {
    let app_dir = std::env::var("APPDATA")
        .map(|a| PathBuf::from(a).join("DeepNodeCopy"))
        .ok()?;
    let update_dir = app_dir.join("updates");
    let json_path = update_dir.join("pending_update.json");
    if !json_path.exists() {
        return None;
    }
    let s = std::fs::read_to_string(&json_path).ok()?;
    let val: serde_json::Value = serde_json::from_str(&s).ok()?;
    let target_ver = val.get("version").and_then(|v| v.as_str())?;
    if compare_semver(target_ver, env!("CARGO_PKG_VERSION")) > 0 {
        let target_name = val.get("file_name").and_then(|v| v.as_str()).unwrap_or("pending_setup.exe");
        if update_dir.join(target_name).exists() {
            return Some(val);
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn apply_silent_update_on_exit() {
    let app_dir = match std::env::var("APPDATA") {
        Ok(a) => PathBuf::from(a).join("DeepNodeCopy"),
        Err(_) => return,
    };
    let update_dir = app_dir.join("updates");
    let json_path = update_dir.join("pending_update.json");

    if !json_path.exists() {
        return;
    }

    let meta_str = match std::fs::read_to_string(&json_path) {
        Ok(s) => s,
        Err(_) => return,
    };

    let meta: serde_json::Value = match serde_json::from_str(&meta_str) {
        Ok(v) => v,
        Err(_) => return,
    };

    let target_version = meta.get("version").and_then(|v| v.as_str()).unwrap_or("");
    let is_setup = meta.get("is_setup").and_then(|v| v.as_bool()).unwrap_or(true);
    let target_file_name = meta.get("file_name").and_then(|v| v.as_str()).unwrap_or("pending_setup.exe");
    let update_file = update_dir.join(target_file_name);

    if !update_file.exists() {
        let _ = std::fs::remove_file(&json_path);
        return;
    }

    let cur_ver = env!("CARGO_PKG_VERSION");
    if compare_semver(cur_ver, target_version) >= 0 {
        let _ = std::fs::remove_file(&update_file);
        let _ = std::fs::remove_file(&json_path);
        return;
    }

    if verify_installer_authenticode(&update_file).is_err() {
        log_engine("apply_silent_update_on_exit: authenticode verification failed, discarding");
        let _ = std::fs::remove_file(&update_file);
        let _ = std::fs::remove_file(&json_path);
        return;
    }

    log_engine(&format!(
        "apply_silent_update_on_exit: applying silent update to v{} in background without displaying any window",
        target_version
    ));

    if is_setup {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let is_msi = target_file_name.to_lowercase().ends_with(".msi");
        if is_msi {
            let _ = std::process::Command::new("msiexec.exe")
                .args(["/i", &update_file.to_string_lossy(), "/qn", "/norestart"])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
        } else {
            let _ = std::process::Command::new(&update_file)
                .arg("/S")
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
        }
    } else {
        if let Ok(cur_exe) = std::env::current_exe() {
            let old_exe = cur_exe.with_extension("exe.old");
            let _ = std::fs::remove_file(&old_exe);
            if std::fs::rename(&cur_exe, &old_exe).is_ok() {
                if std::fs::copy(&update_file, &cur_exe).is_ok() {
                    let _ = std::fs::remove_file(&update_file);
                    let _ = std::fs::remove_file(&json_path);
                    log_engine("apply_silent_update_on_exit: portable in-place swap complete");
                } else {
                    let _ = std::fs::rename(&old_exe, &cur_exe);
                }
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn apply_pending_update_at_startup() {
    let app_dir = match std::env::var("APPDATA") {
        Ok(a) => PathBuf::from(a).join("DeepNodeCopy"),
        Err(_) => return,
    };
    let update_dir = app_dir.join("updates");
    let json_path = update_dir.join("pending_update.json");

    if !json_path.exists() {
        return;
    }

    let meta_str = match std::fs::read_to_string(&json_path) {
        Ok(s) => s,
        Err(_) => return,
    };

    let meta: serde_json::Value = match serde_json::from_str(&meta_str) {
        Ok(v) => v,
        Err(_) => return,
    };

    let target_version = meta.get("version").and_then(|v| v.as_str()).unwrap_or("");
    let is_setup = meta.get("is_setup").and_then(|v| v.as_bool()).unwrap_or(true);
    let target_file_name = meta.get("file_name").and_then(|v| v.as_str()).unwrap_or("pending_setup.exe");
    let update_file = update_dir.join(target_file_name);

    if !update_file.exists() {
        let _ = std::fs::remove_file(&json_path);
        return;
    }

    let cur_ver = env!("CARGO_PKG_VERSION");
    if compare_semver(cur_ver, target_version) >= 0 {
        let _ = std::fs::remove_file(&update_file);
        let _ = std::fs::remove_file(&json_path);
        return;
    }

    if verify_installer_authenticode(&update_file).is_err() {
        let _ = std::fs::remove_file(&update_file);
        let _ = std::fs::remove_file(&json_path);
        return;
    }

    log_engine(&format!(
        "apply_pending_update_at_startup: applying update to v{} (current is v{}) before continuing launch",
        target_version, cur_ver
    ));

    let raw_args: Vec<String> = std::env::args().collect();
    let cur_exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(_) => return,
    };

    if is_setup {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let old_exe = cur_exe.with_extension("exe.old");
        let _ = std::fs::remove_file(&old_exe);
        let _ = std::fs::rename(&cur_exe, &old_exe);

        let status = std::process::Command::new(&update_file)
            .arg("/S")
            .creation_flags(CREATE_NO_WINDOW)
            .status();

        if let Ok(st) = status {
            if st.success() {
                let _ = std::fs::remove_file(&update_file);
                let _ = std::fs::remove_file(&json_path);

                if cur_exe.exists() {
                    let mut cmd = std::process::Command::new(&cur_exe);
                    if raw_args.len() > 1 {
                        cmd.args(&raw_args[1..]);
                    }
                    let _ = cmd.spawn();
                    std::process::exit(0);
                }
            }
        }

        if !cur_exe.exists() && old_exe.exists() {
            let _ = std::fs::rename(&old_exe, &cur_exe);
        }
    } else {
        let old_exe = cur_exe.with_extension("exe.old");
        let _ = std::fs::remove_file(&old_exe);
        if std::fs::rename(&cur_exe, &old_exe).is_ok() {
            if std::fs::copy(&update_file, &cur_exe).is_ok() {
                let _ = std::fs::remove_file(&update_file);
                let _ = std::fs::remove_file(&json_path);

                let mut cmd = std::process::Command::new(&cur_exe);
                if raw_args.len() > 1 {
                    cmd.args(&raw_args[1..]);
                }
                let _ = cmd.spawn();
                std::process::exit(0);
            } else {
                let _ = std::fs::rename(&old_exe, &cur_exe);
            }
        }
    }
}

async fn run_silent_update_check_cycle(app: &AppHandle, state: &Arc<AppState>) {

    let mut wait_count = 0;
    while state.is_worker_running.load(Ordering::SeqCst) && wait_count < 60 {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        wait_count += 1;
    }
    if state.is_worker_running.load(Ordering::SeqCst) {
        log_engine("Silent update check postponed: transfer is actively running");
        return;
    }

    let app_dir = match std::env::var("APPDATA") {
        Ok(a) => PathBuf::from(a).join("DeepNodeCopy"),
        Err(_) => return,
    };
    let update_dir = app_dir.join("updates");
    let json_path = update_dir.join("pending_update.json");

    if json_path.exists() {
        if let Ok(meta_str) = std::fs::read_to_string(&json_path) {
            if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&meta_str) {
                let staged_ver = meta.get("version").and_then(|v| v.as_str()).unwrap_or("");
                let cur_ver = env!("CARGO_PKG_VERSION");
                if compare_semver(staged_ver, cur_ver) > 0 {
                    let target_name = meta.get("file_name").and_then(|v| v.as_str()).unwrap_or("pending_setup.exe");
                    let staged_file = update_dir.join(target_name);
                    if staged_file.exists() && verify_installer_authenticode(&staged_file).is_ok() {
                        log_engine(&format!(
                            "Silent update already staged for v{} (ready to apply on exit or next run)",
                            staged_ver
                        ));
                        return;
                    }
                }
            }
        }
    }

    log_engine("Silent update background check querying GitHub releases...");
    let release = match check_latest_release().await {
        Ok(r) => r,
        Err(e) => {
            log_engine(&format!("Silent update check failed to fetch latest release: {}", e));
            return;
        }
    };

    let latest_tag = release.tag_name;
    let cur_ver = env!("CARGO_PKG_VERSION");
    if compare_semver(&latest_tag, cur_ver) <= 0 {
        log_engine(&format!(
            "Silent update check: application is up-to-date (current: v{}, latest: {})",
            cur_ver, latest_tag
        ));
        return;
    }

    log_engine(&format!(
        "Silent update found newer version: {} (current: v{}). Choosing download asset...",
        latest_tag, cur_ver
    ));

    let is_portable = is_portable_app();
    let is_setup = !is_portable;

    let chosen_asset = if is_setup {
        release.assets.iter().find(|a| {
            let n = a.name.to_lowercase();
            n == "deepnodecopy_setup.exe" || (n.contains("setup") && n.ends_with(".exe"))
        }).cloned()
    } else {
        release.assets.iter().find(|a| {
            let n = a.name.to_lowercase();
            n == "deepnodecopy.exe" || (n.ends_with(".exe") && !n.contains("setup"))
        }).cloned()
    };

    let asset = match chosen_asset {
        Some(a) => a,
        None => {
            log_engine("Silent update asset not found for current environment");
            return;
        }
    };

    let file_name = if is_setup {
        "pending_setup.exe"
    } else {
        "pending_update.exe"
    };

    let _ = std::fs::create_dir_all(&update_dir);
    let part_path = update_dir.join(format!("{}.part", file_name));
    let target_path = update_dir.join(file_name);

    log_engine(&format!(
        "Silently downloading {} ({:.2} MB) in background...",
        asset.name, asset.size as f64 / 1_048_576.0
    ));

    let client = match reqwest::Client::builder()
        .user_agent(format!("DeepNodeCopy/{}", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log_engine(&format!("Silent update HTTP client build failed: {}", e));
            return;
        }
    };

    let res = match client.get(&asset.browser_download_url).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            log_engine(&format!("Silent update download failed: HTTP {}", r.status()));
            return;
        }
        Err(e) => {
            log_engine(&format!("Silent update network error: {}", e));
            return;
        }
    };

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let mut file = match tokio::fs::File::create(&part_path).await {
        Ok(f) => f,
        Err(e) => {
            log_engine(&format!("Silent update could not create part file: {}", e));
            return;
        }
    };

    let mut stream = res.bytes_stream();
    let mut download_ok = true;

    while let Some(chunk_res) = stream.next().await {
        match chunk_res {
            Ok(chunk) => {
                if let Err(e) = file.write_all(&chunk).await {
                    log_engine(&format!("Silent update write failed: {}", e));
                    download_ok = false;
                    break;
                }
            }
            Err(e) => {
                log_engine(&format!("Silent update stream failed: {}", e));
                download_ok = false;
                break;
            }
        }
    }

    let _ = file.flush().await;
    drop(file);

    if !download_ok {
        let _ = tokio::fs::remove_file(&part_path).await;
        return;
    }

    if let Err(err) = verify_installer_authenticode(&part_path) {
        log_engine(&format!("Silent update signature verification failed: {}", err));
        let _ = std::fs::remove_file(&part_path);
        return;
    }

    if let Err(e) = std::fs::rename(&part_path, &target_path) {
        log_engine(&format!("Silent update rename failed: {}", e));
        let _ = std::fs::remove_file(&part_path);
        return;
    }

    let clean_ver = latest_tag.trim_start_matches('v').trim_start_matches('V').to_string();
    let meta = serde_json::json!({
        "version": clean_ver,
        "target_tag": latest_tag,
        "is_setup": is_setup,
        "file_name": file_name,
        "downloaded_at": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        "ready": true
    });

    if let Ok(meta_str) = serde_json::to_string_pretty(&meta) {
        let _ = std::fs::write(&json_path, meta_str);
    }

    log_engine(&format!(
        "Silent update v{} successfully staged! Will be applied silently when application closes or on next run.",
        clean_ver
    ));

    let _ = app.emit("staged_update_ready", serde_json::json!({
        "version": clean_ver,
        "tag": latest_tag,
        "is_setup": is_setup
    }));
}

async fn run_silent_update_background_service(app: AppHandle, state: Arc<AppState>) {
    // Initial wait before first background check
    tokio::time::sleep(std::time::Duration::from_secs(20)).await;

    loop {
        run_silent_update_check_cycle(&app, &state).await;

        // Periodic check every 4 hours matching reference updater lifecycle
        tokio::time::sleep(std::time::Duration::from_secs(4 * 3600)).await;
    }
}

#[tauri::command]
fn apply_staged_update_now() -> Result<(), String> {
    log_engine("apply_staged_update_now invoked by user/frontend: applying update and exiting");
    #[cfg(target_os = "windows")]
    apply_silent_update_on_exit();
    flush_telemetry();
    std::process::exit(0);
}

#[tauri::command]
async fn start_update_download(app: AppHandle, url: String) -> Result<(), String> {
    let parsed_url = validate_update_url(&url)?;

    if IS_DOWNLOADING_UPDATE.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Err("Zaten devam eden bir güncelleme indirme işlemi var".to_string());
    }

    let temp_dir = std::env::temp_dir().join("DeepNodeUpdate");
    let _ = std::fs::create_dir_all(&temp_dir);
    let is_setup = parsed_url.path().to_lowercase().contains("setup");
    let file_name = if is_setup {
        "DeepNodeCopy_Setup.exe"
    } else {
        "DeepNodeCopy_Update.exe"
    };
    let target_file = temp_dir.join(file_name);
    let download_id = uuid::Uuid::new_v4();
    let part_file = temp_dir.join(format!("{}.{}.part", file_name, download_id));

    let client = reqwest::Client::builder()
        .user_agent(format!("DeepNodeCopy/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| {
            IS_DOWNLOADING_UPDATE.store(false, Ordering::SeqCst);
            e.to_string()
        })?;

    tauri::async_runtime::spawn(async move {
        struct DownloadGuard;
        impl Drop for DownloadGuard {
            fn drop(&mut self) {
                IS_DOWNLOADING_UPDATE.store(false, Ordering::SeqCst);
            }
        }
        let _guard = DownloadGuard;

        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;

        let res = client.get(&url).send().await;
        let response = match res {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                let _ = app.emit(
                    "update_progress",
                    UpdateProgressPayload {
                        status: "error".to_string(),
                        progress: 0,
                        error: Some(format!("HTTP {}", r.status())),
                    },
                );
                return;
            }
            Err(e) => {
                let _ = app.emit(
                    "update_progress",
                    UpdateProgressPayload {
                        status: "error".to_string(),
                        progress: 0,
                        error: Some(e.to_string()),
                    },
                );
                return;
            }
        };

        let total_size = response.content_length().unwrap_or(0);
        let mut file = match tokio::fs::File::create(&part_file).await {
            Ok(f) => f,
            Err(e) => {
                let _ = app.emit(
                    "update_progress",
                    UpdateProgressPayload {
                        status: "error".to_string(),
                        progress: 0,
                        error: Some(e.to_string()),
                    },
                );
                return;
            }
        };

        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();
        let mut last_emit = std::time::Instant::now();

        while let Some(chunk_res) = stream.next().await {
            match chunk_res {
                Ok(chunk) => {
                    if let Err(e) = file.write_all(&chunk).await {
                        let _ = app.emit(
                            "update_progress",
                            UpdateProgressPayload {
                                status: "error".to_string(),
                                progress: 0,
                                error: Some(e.to_string()),
                            },
                        );
                        let _ = tokio::fs::remove_file(&part_file).await;
                        return;
                    }
                    downloaded += chunk.len() as u64;
                    let pct = if total_size > 0 {
                        ((downloaded as f64 / total_size as f64) * 100.0) as u32
                    } else {
                        50
                    };

                    if last_emit.elapsed() >= std::time::Duration::from_millis(300) || pct == 100 {
                        let _ = app.emit(
                            "update_progress",
                            UpdateProgressPayload {
                                status: "downloading".to_string(),
                                progress: pct,
                                error: None,
                            },
                        );
                        last_emit = std::time::Instant::now();
                    }
                }
                Err(e) => {
                    let _ = app.emit(
                        "update_progress",
                        UpdateProgressPayload {
                            status: "error".to_string(),
                            progress: 0,
                            error: Some(e.to_string()),
                        },
                    );
                    let _ = tokio::fs::remove_file(&part_file).await;
                    return;
                }
            }
        }

        let _ = file.flush().await;
        drop(file);

        if let Err(e) = std::fs::rename(&part_file, &target_file) {
            let _ = app.emit(
                "update_progress",
                UpdateProgressPayload {
                    status: "error".to_string(),
                    progress: 0,
                    error: Some(e.to_string()),
                },
            );
            let _ = std::fs::remove_file(&part_file);
            return;
        }

        if let Ok(mut lock) = UPDATE_INSTALLER_PATH.lock() {
            *lock = Some(target_file);
        }

        let _ = app.emit(
            "update_progress",
            UpdateProgressPayload {
                status: "ready".to_string(),
                progress: 100,
                error: None,
            },
        );
    });

    Ok(())
}

#[tauri::command]
fn install_app_update() -> Result<(), String> {
    let path = {
        let lock = UPDATE_INSTALLER_PATH.lock().map_err(|e| e.to_string())?;
        lock.clone().unwrap_or_else(|| {
            let temp_dir = std::env::temp_dir().join("DeepNodeUpdate");
            let update_exe = temp_dir.join("DeepNodeCopy_Update.exe");
            if update_exe.exists() {
                update_exe
            } else {
                temp_dir.join("DeepNodeCopy_Setup.exe")
            }
        })
    };

    if !path.exists() {
        return Err("Güncelleme dosyası bulunamadı".to_string());
    }

    // Verify Authenticode digital signature and publisher integrity before execution
    verify_installer_authenticode(&path)?;

    #[cfg(target_os = "windows")]
    {
        let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let path_str = path.to_string_lossy().to_string();
        let cur_exe_str = current_exe.to_string_lossy().to_string();
        let is_setup = path_str.to_lowercase().contains("setup");

        log_engine(&format!(
            "install_app_update: installer='{}', current_exe='{}', is_setup={}",
            path_str, cur_exe_str, is_setup
        ));

        if is_setup {
            let needs_elevation = cur_exe_str.to_lowercase().contains("program files");
            use std::ffi::OsStr;
            use std::os::windows::ffi::OsStrExt;
            let path_wide: Vec<u16> = OsStr::new(&path_str).encode_wide().chain(std::iter::once(0)).collect();
            let verb = if needs_elevation {
                windows::core::w!("runas")
            } else {
                windows::core::w!("open")
            };

            unsafe {
                let res = windows::Win32::UI::Shell::ShellExecuteW(
                    windows::Win32::Foundation::HWND(std::ptr::null_mut()),
                    verb,
                    windows::core::PCWSTR(path_wide.as_ptr()),
                    windows::core::PCWSTR::null(),
                    windows::core::PCWSTR::null(),
                    windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
                );
                if res.0 as usize <= 32 {
                    return Err(format!("Kurulum başlatılamadı: ShellExecuteW hata kodu {}", res.0 as usize));
                }
            }
        } else {
            // Portable standalone executable:
            // On Windows NTFS, a running executable file can be renamed while open.
            // Once renamed to .exe.old, the original path is vacant and can be replaced immediately.
            let cur_clean_str = if cur_exe_str.starts_with(r"\\?\") {
                cur_exe_str.trim_start_matches(r"\\?\").to_string()
            } else {
                cur_exe_str.clone()
            };
            let current_clean_path = PathBuf::from(&cur_clean_str);
            let old_exe = current_clean_path.with_extension("exe.old");

            if old_exe.exists() {
                let _ = std::fs::remove_file(&old_exe);
            }

            let mut swap_done = false;
            if let Ok(()) = std::fs::rename(&current_clean_path, &old_exe) {
                if let Ok(_) = std::fs::copy(&path, &current_clean_path) {
                    swap_done = true;
                    log_engine(&format!(
                        "In-process portable swap succeeded: '{}' replaced with '{}'",
                        cur_clean_str, path_str
                    ));
                } else {
                    // Rollback rename if copy failed
                    let _ = std::fs::rename(&old_exe, &current_clean_path);
                }
            }

            if swap_done {
                // Launch the updated current_exe via ShellExecuteW
                use std::ffi::OsStr;
                use std::os::windows::ffi::OsStrExt;
                let cur_wide: Vec<u16> = OsStr::new(&cur_clean_str)
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();

                unsafe {
                    let res = windows::Win32::UI::Shell::ShellExecuteW(
                        windows::Win32::Foundation::HWND(std::ptr::null_mut()),
                        windows::core::w!("open"),
                        windows::core::PCWSTR(cur_wide.as_ptr()),
                        windows::core::PCWSTR::null(),
                        windows::core::PCWSTR::null(),
                        windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
                    );
                    if res.0 as usize <= 32 {
                        let _ = std::process::Command::new(&current_clean_path).spawn();
                    }
                }
            } else {
                // Fallback if direct rename failed (e.g. read-only folder):
                // Create a temporary batch file to wait, copy, and relaunch without cmd /C quote mangling
                let bat_path = std::env::temp_dir().join("DeepNodeUpdate").join("apply_update.bat");
                let bat_content = format!(
                    "@echo off\r\ntimeout /t 1 /nobreak >nul\r\n:retry\r\ncopy /y \"{}\" \"{}\" >nul 2>&1\r\nif errorlevel 1 (\r\n    timeout /t 1 /nobreak >nul\r\n    goto retry\r\n)\r\nstart \"\" \"{}\"\r\n(goto) 2>nul & del \"%~f0\"\r\n",
                    path_str, cur_clean_str, cur_clean_str
                );
                let _ = std::fs::write(&bat_path, bat_content);

                use std::ffi::OsStr;
                use std::os::windows::ffi::OsStrExt;
                let bat_wide: Vec<u16> = OsStr::new(&bat_path.to_string_lossy().to_string())
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();

                unsafe {
                    windows::Win32::UI::Shell::ShellExecuteW(
                        windows::Win32::Foundation::HWND(std::ptr::null_mut()),
                        windows::core::w!("open"),
                        windows::core::PCWSTR(bat_wide.as_ptr()),
                        windows::core::PCWSTR::null(),
                        windows::core::PCWSTR::null(),
                        windows::Win32::UI::WindowsAndMessaging::SW_HIDE,
                    );
                }
            }
        }

        // Close current application cleanly so file can be overwritten or setup can proceed
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(300));
            flush_telemetry();
            std::process::exit(0);
        });

        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(())
    }
}

#[tauri::command]
async fn get_history_sessions(
    state: State<'_, AppState>,
    limit: Option<usize>,
    offset: Option<usize>,
    filter: Option<String>,
) -> Result<Vec<deepnode_db::SessionRecord>, String> {
    let limit = limit.unwrap_or(50).clamp(1, 1000);
    let offset = offset.unwrap_or(0).min(1_000_000);
    let db_guard = state.db.lock().await;
    if let Some(ref db) = *db_guard {
        db.get_recent_sessions(limit, offset, filter.as_deref())
            .map_err(|e| e.to_string())
    } else {
        Ok(Vec::new())
    }
}

#[tauri::command]
async fn get_history_session_files(
    state: State<'_, AppState>,
    session_id: i64,
) -> Result<Vec<deepnode_db::FileRecord>, String> {
    let db_guard = state.db.lock().await;
    if let Some(ref db) = *db_guard {
        db.get_files_by_session(session_id).map_err(|e| e.to_string())
    } else {
        Ok(Vec::new())
    }
}

#[tauri::command]
async fn delete_history_session(
    state: State<'_, AppState>,
    session_id: i64,
) -> Result<(), String> {
    let db_guard = state.db.lock().await;
    if let Some(ref db) = *db_guard {
        db.delete_session(session_id).map_err(|e| e.to_string())
    } else {
        Err("Veritabanı bağlantısı mevcut değil (Database unavailable)".to_string())
    }
}

#[tauri::command]
async fn clear_history(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db_guard = state.db.lock().await;
    if let Some(ref db) = *db_guard {
        db.clear_all_history().map_err(|e| e.to_string())
    } else {
        Err("Veritabanı bağlantısı mevcut değil (Database unavailable)".to_string())
    }
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn sanitize_csv_cell(input: &str) -> String {
    let trimmed = input.trim();
    let escaped = input.replace('"', "\"\"");
    if trimmed.starts_with('=') || trimmed.starts_with('+') || trimmed.starts_with('-') || trimmed.starts_with('@') {
        format!("'{}", escaped)
    } else {
        escaped
    }
}

#[tauri::command]
async fn export_history_session(
    state: State<'_, AppState>,
    session_id: i64,
    format: String,
) -> Result<String, String> {
    let db_guard = state.db.lock().await;
    let (session, files) = if let Some(ref db) = *db_guard {
        let s = db.get_session_by_id(session_id).map_err(|e| e.to_string())?
            .ok_or_else(|| "Oturum bulunamadı".to_string())?;
        let f = db.get_files_by_session(session_id).map_err(|e| e.to_string())?;
        (s, f)
    } else {
        return Err("Veritabanı hazır değil".to_string());
    };

    tokio::task::spawn_blocking(move || {
        match format.to_lowercase().as_str() {
            "csv" => {
                let mut csv = String::from("Kaynak,Hedef,Boyut,Durum,KaynakHash,HedefHash,Hata\n");
                for f in files {
                    csv.push_str(&format!(
                        "\"{}\",\"{}\",{},\"{}\",\"{}\",\"{}\",\"{}\"\n",
                        sanitize_csv_cell(&f.src_path),
                        sanitize_csv_cell(&f.dst_path),
                        f.size,
                        f.status,
                        f.src_hash.unwrap_or_default(),
                        f.dst_hash.unwrap_or_default(),
                        sanitize_csv_cell(&f.error_message.unwrap_or_default())
                    ));
                }
                Ok(csv)
            }
            "checksum" => {
                let mut out = String::new();
                for f in files {
                    let filename = Path::new(&f.dst_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&f.dst_path);
                    if let Some(hash) = f.dst_hash.or(f.src_hash) {
                        out.push_str(&format!("{} *{}\n", hash, filename));
                    } else if f.status.to_lowercase() == "failed" {
                        out.push_str(&format!("# FAILED: {}\n", filename));
                    }
                }
                Ok(out)
            }
            _ => {
                let duration_secs = session.end_time.map(|e| (e - session.start_time).num_seconds()).unwrap_or(0);
                let mut html = format!(
                    r#"<!DOCTYPE html>
<html lang="tr">
<head>
    <meta charset="UTF-8">
    <title>DeepNode Copy Aktarım Raporu - {start_time}</title>
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #0f172a; color: #f1f5f9; padding: 24px; margin: 0; }}
        .card {{ background: #1e293b; border-radius: 12px; padding: 20px; margin-bottom: 20px; border: 1px solid #334155; }}
        h1 {{ margin: 0 0 16px 0; font-size: 20px; color: #38bdf8; }}
        .grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(160px, 1fr)); gap: 12px; margin-bottom: 16px; }}
        .stat {{ background: #0f172a; padding: 12px; border-radius: 8px; border: 1px solid #1e293b; }}
        .stat-label {{ font-size: 11px; color: #94a3b8; text-transform: uppercase; }}
        .stat-val {{ font-size: 16px; font-weight: 700; color: #f8fafc; margin-top: 4px; }}
        table {{ width: 100%; border-collapse: collapse; font-size: 12px; }}
        th, td {{ padding: 10px 12px; text-align: left; border-bottom: 1px solid #334155; }}
        th {{ background: #0f172a; color: #94a3b8; font-weight: 600; text-transform: uppercase; font-size: 11px; }}
        .completed {{ color: #4ade80; font-weight: 600; }}
        .failed {{ color: #f87171; font-weight: 600; }}
        .skipped {{ color: #fbbf24; font-weight: 600; }}
        .hash {{ font-family: monospace; font-size: 11px; color: #38bdf8; }}
        .footer {{ text-align: center; font-size: 11px; color: #64748b; margin-top: 24px; }}
    </style>
</head>
<body>
    <div class="card">
        <h1>DeepNode Copy Aktarım Raporu</h1>
        <div class="grid">
            <div class="stat"><div class="stat-label">Tarih</div><div class="stat-val">{start_time}</div></div>
            <div class="stat"><div class="stat-label">Durum</div><div class="stat-val">{status}</div></div>
            <div class="stat"><div class="stat-label">Toplam Dosya</div><div class="stat-val">{total_files}</div></div>
            <div class="stat"><div class="stat-label">Aktarılan Boyut</div><div class="stat-val">{size_mb:.2} MB</div></div>
            <div class="stat"><div class="stat-label">Süre</div><div class="stat-val">{duration} sn</div></div>
            <div class="stat"><div class="stat-label">Ortalama Hız</div><div class="stat-val">{avg_speed:.1} MB/s</div></div>
        </div>
        <div style="font-size: 12px; color: #94a3b8;"><strong>Hedef:</strong> {destination}</div>
    </div>
    <div class="card">
        <table>
            <thead>
                <tr>
                    <th>Dosya Yolu</th>
                    <th>Boyut</th>
                    <th>Durum</th>
                    <th>Doğrulama Sağlaması (Hash)</th>
                </tr>
            </thead>
            <tbody>"#,
                    start_time = session.start_time.format("%d.%m.%Y %H:%M:%S"),
                    status = session.status.to_uppercase(),
                    total_files = session.total_files,
                    size_mb = session.transferred_bytes as f64 / 1_048_576.0,
                    duration = duration_secs,
                    avg_speed = session.average_speed_bps / 1_048_576.0,
                    destination = escape_html(&session.destination)
                );

                for f in files {
                    let size_mb = f.size as f64 / 1_048_576.0;
                    let hash_display = f.dst_hash.or(f.src_hash).unwrap_or_else(|| "-".to_string());
                    html.push_str(&format!(
                        r#"<tr>
                            <td>{}</td>
                            <td>{:.2} MB</td>
                            <td class="{}">{}</td>
                            <td class="hash">{}</td>
                        </tr>"#,
                        escape_html(&f.src_path),
                        size_mb,
                        f.status.to_lowercase(),
                        f.status.to_uppercase(),
                        escape_html(&hash_display)
                    ));
                }

                html.push_str(
                    r#"</tbody>
            </table>
        </div>
        <div class="footer">Oluşturuldu: DeepNode Copy | Kriptografik Bütünlük Doğrulandı</div>
    </body>
    </html>"#
                );

                Ok(html)
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

fn cleanup_stale_temp_lists() {
    let temp_dir = std::env::temp_dir();
    if let Ok(entries) = std::fs::read_dir(temp_dir) {
        let now = std::time::SystemTime::now();
        for entry in entries.flatten() {
            let path = entry.path();
            if is_deepnode_temp_list(&path) {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(modified) = meta.modified() {
                        if let Ok(duration) = now.duration_since(modified) {
                            if duration.as_secs() > 300 {
                                let _ = std::fs::remove_file(&path);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn cleanup_stale_update_files() {
    if let Ok(exe) = std::env::current_exe() {
        let old_exe = exe.with_extension("exe.old");
        if old_exe.exists() {
            let _ = std::fs::remove_file(old_exe);
        }
    }
    let temp_dir = std::env::temp_dir().join("DeepNodeUpdate");
    if let Ok(entries) = std::fs::read_dir(&temp_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if ext == "part" || ext == "bat" {
                    let _ = std::fs::remove_file(p);
                }
            }
        }
    }
}

#[cfg(target_os = "windows")]
/// Older builds started the app with Windows (`Run` → `--silent`) to keep the global input hooks
/// alive. The shell extension now launches the app on demand (like TeraCopy), so remove that entry.
fn remove_legacy_autostart() {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegDeleteKeyValueW, HKEY_CURRENT_USER};
    unsafe {
        let _ = RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Run"),
            w!("DeepNode Copy"),
        );
    }
}

fn main() {
    cleanup_stale_temp_lists();
    cleanup_stale_update_files();
    #[cfg(target_os = "windows")]
    apply_pending_update_at_startup();
    #[cfg(target_os = "windows")]
    remove_legacy_autostart();

    let app_dir = std::env::var("APPDATA")
        .map(|a| PathBuf::from(a).join("DeepNodeCopy"))
        .unwrap_or_else(|_| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&app_dir);

    // Initialize Sentry with official project DSN or SENTRY_DSN environment variable
    let sentry_dsn = std::env::var("SENTRY_DSN").unwrap_or_else(|_| {
        "https://b475ca7076c2499baac1d734301acaff@o4512196697128960.ingest.de.sentry.io/4512196703420496".to_string()
    });

    let mut sentry_opts = sentry::ClientOptions::default();
    sentry_opts.release = Some(format!("deepnode-copy@{}", env!("CARGO_PKG_VERSION")).into());
    sentry_opts.environment = Some(if cfg!(debug_assertions) { "development".into() } else { "production".into() });
    sentry_opts.attach_stacktrace = true;
    sentry_opts.send_default_pii = false;

    let _sentry_guard = sentry::init((sentry_dsn, sentry_opts));

    let raw_args: Vec<String> = std::env::args().collect();
    log_engine(&format!(
        "DeepNodeCopy main starting (PID: {}). Command line args: {:?}",
        std::process::id(),
        raw_args
    ));

#[cfg(target_os = "windows")]
static PRIMARY_MUTEX_HANDLE: std::sync::OnceLock<isize> = std::sync::OnceLock::new();

#[cfg(target_os = "windows")]
fn check_single_instance(raw_args: &[String]) -> bool {
    use windows::Win32::System::Threading::{CreateMutexW, OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
    use windows::core::w;

    let mutex_name = w!("Local\\DeepNodeCopy_SingleInstance_Mutex");

    // 1. Try opening existing mutex first
    let already_exists = unsafe {
        OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, mutex_name).is_ok()
    };

    if already_exists {
        log_engine(&format!(
            "Secondary instance detected via OpenMutexW (PID: {}). Forwarding args to primary instance via Named Pipe...",
            std::process::id()
        ));
        unsafe {
            // Explicitly allow primary instance to bring itself to foreground
            let _ = windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(0xFFFFFFFF);
        }
        let mut sent = false;
        for attempt in 1..=15 {
            if try_send_args_to_existing_instance(raw_args) {
                log_engine(&format!(
                    "Arguments successfully forwarded on attempt {}. Exiting secondary process (PID: {}).",
                    attempt,
                    std::process::id()
                ));
                sent = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if !sent {
            log_engine(&format!(
                "WARNING: Failed to forward arguments to primary instance after 15 attempts. Exiting cleanly (PID: {}).",
                std::process::id()
            ));
        }
        return false;
    }

    // 2. This is the primary instance: create the mutex and keep it open permanently in static memory
    let mutex_res = unsafe {
        CreateMutexW(None, true, mutex_name)
    };
    match mutex_res {
        Ok(handle) => {
            log_engine(&format!(
                "Primary instance mutex created successfully (PID: {}, handle={:?})",
                std::process::id(),
                handle
            ));
            let _ = PRIMARY_MUTEX_HANDLE.set(handle.0 as isize);
        }
        Err(e) => {
            log_engine(&format!("Primary instance CreateMutexW warning: {:?}", e));
        }
    }

    true
}

    #[cfg(target_os = "windows")]
    if !check_single_instance(&raw_args) {
        return;
    }

    let panic_log_path = app_dir.join("panic.log");
    std::panic::set_hook(Box::new(move |info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            format!("{:?}", info)
        };
        let loc = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
        let msg = format!("PANIC: {} at {}\n", payload, loc);
        let _ = std::fs::write(&panic_log_path, &msg);
        log_engine(&msg);
        sentry::capture_message(&msg, sentry::Level::Fatal);
    }));

    let log_file_path = app_dir.join("engine.log");
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file_path)
    {
        use tracing_subscriber::prelude::*;
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_writer(file)
            .with_ansi(false);
        let sentry_layer = sentry_tracing::layer();
        let _ = tracing_subscriber::registry()
            .with(fmt_layer)
            .with(sentry_layer)
            .try_init();
    }

    let db_path = app_dir.join("history.db");
    let db_instance = match Database::open(db_path.clone()) {
        Ok(db) => {
            log_engine(&format!("SQLite history database opened successfully at {:?}", db_path));
            Some(db)
        }
        Err(err) => {
            log_engine(&format!("CRITICAL: Failed to open SQLite history database at {:?}: {}", db_path, err));
            eprintln!("Failed to open SQLite history database at {:?}: {}", db_path, err);
            None
        }
    };

    let launch_args = parse_cli_args(&raw_args);
    log_engine(&format!(
        "Parsed CLI launch args: sources={:?}, dest={:?}, mode={}, auto_start={}, is_popup={}",
        launch_args.sources, launch_args.destination, launch_args.mode, launch_args.auto_start, launch_args.is_popup
    ));

    let initial_state = CurrentTransferState {
        status: "idle".to_string(),
        stats: TransferStats::default(),
        files: Vec::new(),
        launch_args: launch_args.clone(),
        active_conflict: None,
    };

    let shared_state = Arc::new(AppState {
        pause_flag: Arc::new(AtomicBool::new(false)),
        cancel_flag: Arc::new(AtomicBool::new(false)),
        skip_flag: Arc::new(AtomicBool::new(false)),
        is_worker_running: Arc::new(AtomicBool::new(false)),
        skipped_ids: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
        last_failed_items: Arc::new(Mutex::new(Vec::new())),
        last_config: Arc::new(Mutex::new(None)),
        db: Arc::new(Mutex::new(db_instance)),
        conflict_sender: Arc::new(tokio::sync::Mutex::new(None)),
        transfer_state: Arc::new(tokio::sync::Mutex::new(initial_state)),
    });

    let state_for_setup = Arc::clone(&shared_state);
    let launch_args_for_setup = launch_args.clone();

    log_engine("Database opened. Initializing tauri::Builder...");

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        tauri::Builder::default()
            .manage((*shared_state).clone())
            .setup(move |app| {
                log_engine("tauri setup hook entered!");
                let windows = app.webview_windows();
                log_engine(&format!("webview_windows count: {}", windows.len()));
                for (label, win) in &windows {
                    log_engine(&format!("window found: label={}, url={:?}", label, win.url()));
                }

                if launch_args_for_setup.is_silent {
                    for (_label, win) in &windows {
                        let _ = win.hide();
                    }
                    log_engine("Application started in silent background mode. Main window hidden.");
                } else {
                    // The window stays hidden (visible: false) until the frontend has rendered and
                    // calls app_ready_to_show. Showing/foregrounding it here, before WebView2 exists,
                    // attached our input queue to Explorer's busy drop/paste thread and left the
                    // webview uncreated (a black window) for popups launched by the shell extension.
                    log_engine("Application started in GUI mode. Main window will be shown when the frontend is ready.");
                }

                // If auto_start is true, trigger transfer immediately in native Rust
                if launch_args_for_setup.auto_start
                    && launch_args_for_setup.destination.is_some()
                    && !launch_args_for_setup.sources.is_empty()
                {
                    {
                        let mut cur = state_for_setup.transfer_state.blocking_lock();
                        cur.status = "transferring".to_string();
                        cur.launch_args.auto_start = false;
                    }

                    let dest = launch_args_for_setup.destination.clone().unwrap();
                    let sources = launch_args_for_setup.sources.clone();
                    let mode = launch_args_for_setup.mode.clone();
                    let conflict = launch_args_for_setup.conflict_policy.clone();
                    let on_fin = launch_args_for_setup.on_finish.clone().unwrap_or_else(|| "do_nothing".to_string());

                    trigger_native_transfer(
                        app.handle().clone(),
                        Arc::clone(&state_for_setup),
                        sources,
                        dest,
                        mode,
                        conflict,
                        on_fin,
                    );
                }

                if let Some(win) = app.get_webview_window("main") {
                    if launch_args_for_setup.is_popup {
                        #[cfg(target_os = "windows")]
                        place_popup_window(&win, launch_args_for_setup.popup_anchor);
                        let _ = win.set_always_on_top(true);
                    } else if launch_args_for_setup.auto_start {
                        let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize {
                            width: 580.0,
                            height: 190.0,
                        }));
                    } else {
                        let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize {
                            width: 960.0,
                            height: 640.0,
                        }));
                        let _ = win.center();
                    }
                }

                let silent_app = app.handle().clone();
                let silent_state = Arc::clone(&state_for_setup);
                tauri::async_runtime::spawn(async move {
                    run_silent_update_background_service(silent_app, silent_state).await;
                });

                #[cfg(target_os = "windows")]
                start_ipc_pipe_server(app.handle().clone(), Arc::clone(&state_for_setup));

                Ok(())
            })
            .on_window_event(|window, event| {
                if let tauri::WindowEvent::CloseRequested { api: _, .. } = event {
                    log_engine("CloseRequested: exiting application cleanly.");
                    let state: State<AppState> = window.state();
                    state.cancel_flag.store(true, Ordering::SeqCst);
                    #[cfg(target_os = "windows")]
                    apply_silent_update_on_exit();
                    flush_telemetry();
                    std::process::exit(0);
                }
            })
            .invoke_handler(tauri::generate_handler![
                app_ready_to_show,
                close_app,
                start_transfer,
                pause_transfer,
                resume_transfer,
                cancel_transfer,
                retry_failed_transfer,
                resolve_conflict,
                verify_file,
                set_always_on_top,
                set_conflict_mode,
                toggle_compact_mode,
                minimize_window,
                toggle_maximize_window,
                pick_source_files,
                pick_source_folder,
                pick_destination_folder,
                resolve_destination_path,
                scan_items_preview,
                get_launch_args,
                get_current_transfer_state,
                log_js,
                capture_sentry_message,
                register_shell_context_menu,
                unregister_shell_context_menu,
                is_shell_context_menu_registered,
                open_external_url,
                check_latest_release,
                start_update_download,
                install_app_update,
                get_app_version,
                get_staged_update,
                is_portable_app,
                get_history_sessions,
                get_history_session_files,
                delete_history_session,
                clear_history,
                export_history_session,
                skip_current_file,
                skip_files_by_id,
                remove_queued_files,
                show_in_folder,
                open_folder,
                start_dragging,
                get_clipboard_files,
                explorer_copy,
                apply_staged_update_now
            ])
            .build(tauri::generate_context!())
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?
            .run(|_app_handle, event| {
                if let tauri::RunEvent::ExitRequested { .. } = event {
                    log_engine("RunEvent::ExitRequested: process exiting");
                }
            });
        Ok::<(), Box<dyn std::error::Error>>(())
    }));

    match res {
        Ok(Ok(_)) => log_engine("Tauri application exited cleanly"),
        Ok(Err(e)) => log_engine(&format!("Tauri application run error: {:?}", e)),
        Err(panic) => log_engine(&format!("Tauri application panicked in run: {:?}", panic)),
    }
}

#[cfg(test)]
mod cli_args_tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn list_file_and_destination_stay_separate() {
        let list = std::env::temp_dir().join(format!("deepnode_list_{}.tmp", uuid::Uuid::new_v4()));
        let source = std::env::temp_dir().join("source file.mp4");
        std::fs::write(&list, format!("{}\n", source.display())).unwrap();

        let star = format!("*{}", list.display());
        let parsed = parse_cli_args(&args(&["deepnode-gui.exe", "--popup", "--pos", "10,20", "--mode", "copy", &star, "D:\\"]));

        assert!(parsed.is_popup);
        assert_eq!(parsed.sources, vec![source.to_string_lossy().to_string()]);
        assert_eq!(parsed.destination.as_deref(), Some("D:\\"));
        assert!(!list.exists(), "the temporary list file must be consumed");
    }

    #[test]
    fn consecutive_existing_sources_stay_separate() {
        let dir = std::env::temp_dir().join(format!("deepnode_src_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = dir.join("a.txt");
        let second = dir.join("b.txt");
        std::fs::write(&first, b"a").unwrap();
        std::fs::write(&second, b"b").unwrap();

        let parsed = parse_cli_args(&args(&[
            "deepnode-gui.exe",
            "--popup",
            "--source",
            &first.to_string_lossy(),
            &second.to_string_lossy(),
            "--target",
            "D:\\",
        ]));

        assert_eq!(
            parsed.sources,
            vec![first.to_string_lossy().to_string(), second.to_string_lossy().to_string()]
        );
        assert_eq!(parsed.destination.as_deref(), Some("D:\\"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn split_path_with_spaces_is_recombined() {
        let dir = std::env::temp_dir().join(format!("deepnode_split_{}", uuid::Uuid::new_v4()));
        let spaced = dir.join("My Folder");
        std::fs::create_dir_all(&spaced).unwrap();
        let file = spaced.join("report.txt");
        std::fs::write(&file, b"x").unwrap();

        let full = file.to_string_lossy().to_string();
        let (head, tail) = full.split_once(' ').unwrap();
        let parsed = parse_cli_args(&args(&["deepnode-gui.exe", "--popup", head, tail, "D:\\"]));

        assert_eq!(parsed.sources, vec![full]);
        assert_eq!(parsed.destination.as_deref(), Some("D:\\"));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
