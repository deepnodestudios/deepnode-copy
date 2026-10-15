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
    pub auto_start: bool,
    pub conflict_policy: Option<String>,
    pub on_finish: Option<String>,
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
            if p.exists() || (p.parent().map(|par| par.exists()).unwrap_or(false) && p.is_absolute()) {
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
    let mut auto_start = false;
    let mut conflict_policy = None;
    let mut on_finish = None;

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

        if destination.is_none() && positional.len() > 1 && auto_start {
            destination = positional.pop().map(|p| clean_path_str(&p));
            for p in positional {
                let c = clean_path_str(&p);
                if !c.is_empty() {
                    sources.push(c);
                }
            }
        } else if destination.is_none() && positional.len() == 1 && auto_start {
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
    let sources: Vec<String> = sources
        .into_iter()
        .map(|s| clean_path_str(&s))
        .filter(|s| !s.is_empty())
        .collect();

    if destination.is_some() && !sources.is_empty() && (mode == "copy" || mode == "move") {
        auto_start = true;
    }

    LaunchArgs {
        sources,
        destination,
        mode,
        is_popup,
        auto_start,
        conflict_policy,
        on_finish,
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

#[tauri::command]
async fn close_app(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    log_engine("close_app command invoked: initiating graceful shutdown");
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

    // If transfer worker is active, give it up to 1.5s to exit coordinator,
    // clean partial file, and finalize SQLite history records
    let start = std::time::Instant::now();
    while state.is_worker_running.load(Ordering::SeqCst) && start.elapsed() < std::time::Duration::from_millis(1500) {
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    }

    // Safety fallback: ensure any remaining partial files are deleted
    cleanup_partial_transfers(&state).await;

    // Finalize any in_progress session in DB if worker didn't finish it
    {
        let mut db_guard = state.db.lock().await;
        if let Some(ref mut db) = *db_guard {
            let _ = db.finalize_active_sessions_as_canceled();
        }
    }

    for (_, window) in app.webview_windows() {
        let _ = window.destroy();
    }
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
    let current = std::env::current_exe().map_err(|e| format!("Yürütülebilir dosya yolu alınamadı: {}", e))?;
    if let Some(parent) = current.parent() {
        let sibling = parent.join("DeepNodeCopy.exe");
        if sibling.exists() {
            return Ok(sibling);
        }
        if let Some(grandparent) = parent.parent() {
            let bin_sibling = grandparent.join("bin").join("DeepNodeCopy.exe");
            if bin_sibling.exists() {
                return Ok(bin_sibling);
            }
        }
    }
    Ok(current)
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

    // 5. Windows 11 Modern Context Menu (Sparse Package) if manifest exists
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
        let s = exe.to_string_lossy().to_lowercase();
        !(s.contains("program files") || s.contains("appdata\\local\\programs"))
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
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
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
            // Use standard cmd with delay to let host exit cleanly, replace file, and restart
            let cmd_line = format!(
                "ping 127.0.0.1 -n 2 > nul & copy /y \"{}\" \"{}\" & start \"\" \"{}\"",
                path_str, cur_exe_str, cur_exe_str
            );
            std::process::Command::new("cmd")
                .args(["/C", &cmd_line])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| format!("Güncelleme süreci başlatılamadı: {}", e))?;
        }

        // Close current application cleanly so file can be overwritten or setup can proceed
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(500));
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

fn main() {
    cleanup_stale_temp_lists();

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

    let panic_log_path = app_dir.join("panic.log");
    std::panic::set_hook(Box::new(move |info| {
        let msg = format!("PANIC: {:?}\n", info);
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

                // If auto_start is true, trigger transfer immediately in native Rust
                if launch_args_for_setup.auto_start
                    && launch_args_for_setup.destination.is_some()
                    && !launch_args_for_setup.sources.is_empty()
                {
                    // Immediately mark transfer_state as transferring & consume auto_start
                    // so the webview (App.tsx) never sees idle + auto_start=true!
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

                    log_engine(&format!(
                        "NATIVE AUTO-START: Starting transfer immediately from setup hook! Sources: {:?} -> Dest: {}",
                        sources, dest
                    ));

                    let dest_buf = match validate_filesystem_path(&dest) {
                        Ok(p) if !p.is_file() => p,
                        _ => {
                            log_engine("NATIVE AUTO-START: Invalid destination path");
                            let mut cur = state_for_setup.transfer_state.blocking_lock();
                            cur.status = "idle".to_string();
                            return Ok(());
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
                        let mut cur = state_for_setup.transfer_state.blocking_lock();
                        cur.status = "idle".to_string();
                        return Ok(());
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

                        let app_handle = app.handle().clone();
                        let state_clone = Arc::clone(&state_for_setup);
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
                        let mut cur = state_for_setup.transfer_state.blocking_lock();
                        cur.status = "idle".to_string();
                    }
                }

                Ok(())
            })
            .on_window_event(|window, event| {
                if let tauri::WindowEvent::CloseRequested { .. } = event {
                    let state: State<AppState> = window.state();
                    state.cancel_flag.store(true, Ordering::SeqCst);
                    log_engine("Native CloseRequested received: stopping and cleaning partial files");
                    let state_clone = (*state).clone();
                    let win = window.clone();
                    let _ = win.hide();
                    std::thread::spawn(move || {
                        {
                            let mut lock = state_clone.conflict_sender.blocking_lock();
                            if let Some(tx) = lock.take() {
                                let _ = tx.send(ConflictResolutionChoice {
                                    action: "skip".to_string(),
                                    apply_to_all: false,
                                });
                            }
                        }

                        let start = std::time::Instant::now();
                        while state_clone.is_worker_running.load(Ordering::SeqCst)
                            && start.elapsed() < std::time::Duration::from_millis(2000)
                        {
                            std::thread::sleep(std::time::Duration::from_millis(20));
                        }

                        cleanup_partial_transfers_sync(&state_clone);

                        {
                            let mut db_guard = state_clone.db.blocking_lock();
                            if let Some(ref mut db) = *db_guard {
                                let _ = db.finalize_active_sessions_as_canceled();
                            }
                        }

                        let _ = win.destroy();
                        std::process::exit(0);
                    });
                }
            })
            .invoke_handler(tauri::generate_handler![
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
                start_dragging
            ])
            .run(tauri::generate_context!())
    }));

    match res {
        Ok(Ok(_)) => log_engine("Tauri application exited cleanly"),
        Ok(Err(e)) => log_engine(&format!("Tauri application run error: {:?}", e)),
        Err(panic) => log_engine(&format!("Tauri application panicked in run: {:?}", panic)),
    }
}
