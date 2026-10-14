// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State, Window};
use tokio::sync::{mpsc, Mutex};

mod dialog;

use deepnode_core::conflict::policy::ConflictPolicy;
use deepnode_core::engine::config::{TransferConfig, TransferMode, VerifyMode};
use deepnode_core::engine::coordinator::TransferCoordinator;
use deepnode_core::engine::events::{ConflictResolutionChoice, EngineEvent};
use deepnode_core::engine::item::{FileItem, FileItemStatus};
use deepnode_core::engine::on_finish::{OnFinishAction, OnFinishExecutor};
use deepnode_core::engine::scanner::FileScanner;
use deepnode_core::engine::stats::TransferStats;
use deepnode_core::hash::hasher::HashAlgorithm;
use deepnode_core::hash::verifier::HashVerifier;
use deepnode_core::{clean_path_str, read_file_list, sanitize_path};
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
}

pub struct AppState {
    pause_flag: Arc<AtomicBool>,
    cancel_flag: Arc<AtomicBool>,
    last_failed_items: Arc<Mutex<Vec<FileItem>>>,
    last_config: Arc<Mutex<Option<TransferConfig>>>,
    db: Arc<Mutex<Option<Database>>>,
    conflict_sender: Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<ConflictResolutionChoice>>>>,
    transfer_state: Arc<tokio::sync::Mutex<CurrentTransferState>>,
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
                        positional.push(cleaned);
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
                if path.exists() {
                    let items_from_list = read_file_list(&path);
                    sources.extend(items_from_list);
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
async fn toggle_compact_mode(window: Window, compact: bool) -> Result<(), String> {
    let size = if compact {
        tauri::LogicalSize::new(620.0, 310.0)
    } else {
        tauri::LogicalSize::new(720.0, 700.0)
    };
    window.set_size(size).map_err(|e| e.to_string())
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

#[tauri::command]
async fn cancel_transfer(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    log_engine("cancel_transfer command invoked");
    state.cancel_flag.store(true, Ordering::SeqCst);
    {
        let mut cur = state.transfer_state.lock().await;
        cur.status = "canceled".to_string();
        for file in cur.files.iter_mut() {
            if file.status == FileItemStatus::Copying || file.status == FileItemStatus::Pending {
                file.status = FileItemStatus::Canceled;
            }
        }
    }
    let mut lock = state.conflict_sender.lock().await;
    if let Some(tx) = lock.take() {
        let _ = tx.send(ConflictResolutionChoice {
            action: "skip".to_string(),
            apply_to_all: false,
        });
    }
    let _ = app.emit("engine_event", &EngineEvent::TransferCanceled);
    Ok(())
}

#[tauri::command]
async fn close_app(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    log_engine("close_app command invoked: setting cancel_flag, destroying windows and exiting");
    state.cancel_flag.store(true, Ordering::SeqCst);
    for (_, window) in app.webview_windows() {
        let _ = window.destroy();
    }
    std::process::exit(0);
}

#[tauri::command]
async fn pick_source_files() -> Result<Vec<String>, String> {
    dialog::pick_files()
}

#[tauri::command]
async fn pick_source_folder() -> Result<Option<String>, String> {
    dialog::pick_folder()
}

#[tauri::command]
async fn pick_destination_folder() -> Result<Option<String>, String> {
    dialog::pick_folder()
}

#[derive(serde::Serialize)]
pub struct ScanPreviewResult {
    pub total_files: usize,
    pub total_bytes: u64,
    pub items: Vec<ScanItemPreview>,
}

#[derive(serde::Serialize)]
pub struct ScanItemPreview {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

#[tauri::command]
async fn scan_items_preview(sources: Vec<String>) -> Result<ScanPreviewResult, String> {
    let dummy_dest = std::env::temp_dir();
    let scanned = FileScanner::scan(&sources, &dummy_dest).map_err(|e| e.to_string())?;
    let total_files = scanned.len();
    let total_bytes = scanned.iter().map(|i| i.size).sum();
    let mut items = Vec::new();

    for item in scanned {
        let path = item.src_path.to_string_lossy().to_string();
        let name = item.src_path.file_name().unwrap_or_default().to_string_lossy().to_string();
        items.push(ScanItemPreview {
            path,
            name,
            size: item.size,
            is_dir: false,
        });
    }

    Ok(ScanPreviewResult {
        total_files,
        total_bytes,
        items,
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

#[tauri::command]
async fn register_shell_context_menu(app: AppHandle) -> Result<(), String> {
    let app_dir = app.path().app_data_dir().unwrap_or_default();
    let dll_path = app_dir.join("deepnode_shell.dll");
    let script = format!(
        "regsvr32 /s \"{}\"",
        dll_path.to_str().unwrap_or_default()
    );
    let output = std::process::Command::new("cmd")
        .args(["/C", &script])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[derive(serde::Serialize)]
pub struct VerifyResult {
    pub success: bool,
    pub src_hash: String,
    pub dst_hash: String,
    pub algorithm: String,
}

#[tauri::command]
async fn verify_file(src: String, dst: String, algorithm: Option<String>) -> Result<VerifyResult, String> {
    let algo = match algorithm.as_deref() {
        Some("xxhash128") => HashAlgorithm::XxHash3_128,
        Some("blake3") => HashAlgorithm::Blake3,
        Some("md5") => HashAlgorithm::Md5,
        Some("sha1") => HashAlgorithm::Sha1,
        Some("sha256") => HashAlgorithm::Sha256,
        _ => HashAlgorithm::XxHash3_64,
    };

    let src_clean = clean_path_str(&src);
    let dst_clean = clean_path_str(&dst);
    let src_path = sanitize_path(&src_clean);
    let dst_path = sanitize_path(&dst_clean);

    let src_hash = HashVerifier::compute_file_hash(&src_path, algo, false).map_err(|e| e.to_string())?;
    let dst_hash = HashVerifier::compute_file_hash(&dst_path, algo, true).map_err(|e| e.to_string())?;
    let success = src_hash.eq_ignore_ascii_case(&dst_hash);

    Ok(VerifyResult {
        success,
        src_hash,
        dst_hash,
        algorithm: format!("{:?}", algo),
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
    let total_bytes: u64 = items.iter().map(|i| i.size).sum();
    let total_files = items.len();

    state.pause_flag.store(false, Ordering::Relaxed);
    state.cancel_flag.store(false, Ordering::Relaxed);

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
                    EngineEvent::ProgressUpdated(stats) => {
                        cur.stats = stats.clone();
                    }
                    EngineEvent::FileStarted(item) => {
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
                        if let Some(pos) = cur.files.iter_mut().find(|f| f.id == item.id) {
                            pos.status = FileItemStatus::Completed;
                            pos.bytes_transferred = item.size;
                        }
                    }
                    EngineEvent::FileSkipped { item, .. } => {
                        if let Some(pos) = cur.files.iter_mut().find(|f| f.id == item.id) {
                            pos.status = FileItemStatus::Skipped;
                        }
                    }
                    EngineEvent::FileFailed { item, .. } => {
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
                        cur.status = "canceled".to_string();
                        for file in cur.files.iter_mut() {
                            if file.status == FileItemStatus::Copying || file.status == FileItemStatus::Pending {
                                file.status = FileItemStatus::Canceled;
                            }
                        }
                    }
                    EngineEvent::TransferFailed(_) => {
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
    let cancel_for_worker = Arc::clone(&cancel_flag);
    std::thread::Builder::new()
        .name("deepnode-transfer-worker".to_string())
        .spawn(move || {
            log_engine("Transfer worker thread spawned");
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            if let Ok(runtime) = rt {
                runtime.block_on(async move {
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
                                    let has_errors = !failed_items.is_empty();
                                    OnFinishExecutor::play_completion_sound(has_errors);
                                    OnFinishExecutor::execute(on_finish_action);
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
                                OnFinishExecutor::play_completion_sound(true);
                            }
                        }
                    }
                });
            } else {
                log_engine("Failed to create worker thread runtime!");
            }
        })
        .map_err(|e| e.to_string())?;

    Ok(StartTransferResult {
        total_files,
        total_bytes,
        items,
    })
}

#[tauri::command]
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
    let clean_dest = clean_path_str(&destination);
    if clean_dest.is_empty() {
        return Err("Destination path is empty".to_string());
    }
    let dest_buf = sanitize_path(&clean_dest);

    let items = FileScanner::scan(&sources, &dest_buf).map_err(|e| e.to_string())?;
    if items.is_empty() {
        return Err("No valid files found in source paths".to_string());
    }

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
    }

    let state_arc = Arc::new(AppState {
        pause_flag: Arc::clone(&state.pause_flag),
        cancel_flag: Arc::clone(&state.cancel_flag),
        last_failed_items: Arc::clone(&state.last_failed_items),
        last_config: Arc::clone(&state.last_config),
        db: Arc::clone(&state.db),
        conflict_sender: Arc::clone(&state.conflict_sender),
        transfer_state: Arc::clone(&state.transfer_state),
    });

    spawn_transfer_worker(
        app,
        state_arc,
        items,
        config,
        on_finish.unwrap_or_else(|| "do_nothing".to_string()),
        Some(dest_buf),
    )
}

#[tauri::command]
async fn retry_failed_transfer(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let failed = state.last_failed_items.lock().await.clone();
    let config_opt = state.last_config.lock().await.clone();

    if failed.is_empty() {
        return Err("No failed items to retry".to_string());
    }

    let config = config_opt.unwrap_or_default();
    let state_arc = Arc::new(AppState {
        pause_flag: Arc::clone(&state.pause_flag),
        cancel_flag: Arc::clone(&state.cancel_flag),
        last_failed_items: Arc::clone(&state.last_failed_items),
        last_config: Arc::clone(&state.last_config),
        db: Arc::clone(&state.db),
        conflict_sender: Arc::clone(&state.conflict_sender),
        transfer_state: Arc::clone(&state.transfer_state),
    });

    spawn_transfer_worker(app, state_arc, failed, config, "do_nothing".to_string(), None)?;
    Ok(())
}

#[tauri::command]
async fn resolve_conflict(
    state: State<'_, AppState>,
    action: String,
    apply_to_all: bool,
) -> Result<(), String> {
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
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .args(["/c", "start", "", &url])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()
            .map_err(|e| e.to_string())?;
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

static UPDATE_INSTALLER_PATH: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

#[tauri::command]
async fn start_update_download(app: AppHandle, url: String) -> Result<(), String> {
    if !url.starts_with("https://github.com/deepnodestudios/deepnode-copy/releases/")
        && !url.contains("github-releases.githubusercontent.com")
        && !url.starts_with("https://deepnodestudios.com/")
    {
        return Err("Update source is not allowed".to_string());
    }

    let temp_dir = std::env::temp_dir().join("DeepNodeUpdate");
    let _ = std::fs::create_dir_all(&temp_dir);
    let target_file = temp_dir.join("DeepNodeCopy_Setup.exe");
    let part_file = temp_dir.join("DeepNodeCopy_Setup.exe.part");

    let client = reqwest::Client::builder()
        .user_agent("DeepNodeCopy/0.1.0")
        .build()
        .map_err(|e| e.to_string())?;

    tauri::async_runtime::spawn(async move {
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
            std::env::temp_dir().join("DeepNodeUpdate").join("DeepNodeCopy_Setup.exe")
        })
    };

    if !path.exists() {
        return Err("Installer file not found".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // Launch installer detached (silent NSIS upgrade)
        std::process::Command::new(&path)
            .args(["/S"])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()
            .map_err(|e| format!("Failed to launch installer: {}", e))?;

        // Quit current application so installer can overwrite binary
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(800));
            std::process::exit(0);
        });

        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(())
    }
}

fn main() {
    let app_dir = std::env::var("APPDATA")
        .map(|a| PathBuf::from(a).join("DeepNodeCopy"))
        .unwrap_or_else(|_| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&app_dir);

    // Initialize Sentry with official project DSN or SENTRY_DSN environment variable
    let sentry_dsn = std::env::var("SENTRY_DSN").unwrap_or_else(|_| {
        "https://b475ca7076c2499baac1d734301acaff@o4512196697128960.ingest.de.sentry.io/4512196703420496".to_string()
    });

    let mut sentry_opts = sentry::ClientOptions::default();
    sentry_opts.release = Some("deepnode-copy@0.1.0".into());
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
        .write(true)
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
    let db_instance = Database::open(db_path).ok();

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
    };

    let shared_state = Arc::new(AppState {
        pause_flag: Arc::new(AtomicBool::new(false)),
        cancel_flag: Arc::new(AtomicBool::new(false)),
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
            .manage(AppState {
                pause_flag: Arc::clone(&shared_state.pause_flag),
                cancel_flag: Arc::clone(&shared_state.cancel_flag),
                last_failed_items: Arc::clone(&shared_state.last_failed_items),
                last_config: Arc::clone(&shared_state.last_config),
                db: Arc::clone(&shared_state.db),
                conflict_sender: Arc::clone(&shared_state.conflict_sender),
                transfer_state: Arc::clone(&shared_state.transfer_state),
            })
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
                    let dest = launch_args_for_setup.destination.clone().unwrap();
                    let sources = launch_args_for_setup.sources.clone();
                    let mode = launch_args_for_setup.mode.clone();
                    let conflict = launch_args_for_setup.conflict_policy.clone();
                    let on_fin = launch_args_for_setup.on_finish.clone().unwrap_or_else(|| "do_nothing".to_string());

                    log_engine(&format!(
                        "NATIVE AUTO-START: Starting transfer immediately from setup hook! Sources: {:?} -> Dest: {}",
                        sources, dest
                    ));

                    let dest_buf = sanitize_path(&dest);
                    let items = FileScanner::scan(&sources, &dest_buf).unwrap_or_default();
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
                            }
                            if let Err(e) = spawn_transfer_worker(
                                app_handle,
                                state_clone,
                                items_clone,
                                config,
                                on_fin,
                                Some(dest_buf),
                            ) {
                                log_engine(&format!("Native auto-start error: {}", e));
                            }
                        });
                    }
                }

                Ok(())
            })
            .on_window_event(|window, event| {
                if let tauri::WindowEvent::CloseRequested { .. } = event {
                    let state: State<AppState> = window.state();
                    state.cancel_flag.store(true, Ordering::SeqCst);
                    log_engine("Native CloseRequested received: destroying window and exiting");
                    let _ = window.destroy();
                    std::process::exit(0);
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
                toggle_compact_mode,
                pick_source_files,
                pick_source_folder,
                pick_destination_folder,
                scan_items_preview,
                get_launch_args,
                get_current_transfer_state,
                log_js,
                capture_sentry_message,
                register_shell_context_menu,
                open_external_url,
                start_update_download,
                install_app_update
            ])
            .run(tauri::generate_context!())
    }));

    match res {
        Ok(Ok(_)) => log_engine("Tauri application exited cleanly"),
        Ok(Err(e)) => log_engine(&format!("Tauri application run error: {:?}", e)),
        Err(panic) => log_engine(&format!("Tauri application panicked in run: {:?}", panic)),
    }
}
