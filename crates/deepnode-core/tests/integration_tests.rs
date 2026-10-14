use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tempfile::tempdir;

use deepnode_core::conflict::policy::{ConflictPolicy, ConflictResolutionAction};
use deepnode_core::conflict::resolver::ConflictResolver;
use deepnode_core::hash::hasher::{HashAlgorithm, MultiHasher};
use deepnode_core::hash::verifier::HashVerifier;
use deepnode_core::io::copier::FileCopier;

#[test]
fn test_hash_algorithms() {
    let sample = b"DeepNode Copy - High performance Win32 file manager";

    let mut xxh64 = MultiHasher::new(HashAlgorithm::XxHash3_64);
    xxh64.update(sample);
    let hex64 = xxh64.finalize_hex();
    assert_eq!(hex64.len(), 16);

    let mut blake = MultiHasher::new(HashAlgorithm::Blake3);
    blake.update(sample);
    let hex_blake = blake.finalize_hex();
    assert_eq!(hex_blake.len(), 64);

    let mut sha256 = MultiHasher::new(HashAlgorithm::Sha256);
    sha256.update(sample);
    let hex_sha = sha256.finalize_hex();
    assert_eq!(hex_sha.len(), 64);
}

#[test]
fn test_file_copy_and_verification() {
    let dir = tempdir().unwrap();
    let src_path = dir.path().join("source.dat");
    let dst_path = dir.path().join("destination.dat");

    // Create 5MB dummy test file
    let mut file = std::fs::File::create(&src_path).unwrap();
    let data = vec![0xAA; 5 * 1024 * 1024];
    file.write_all(&data).unwrap();
    drop(file);

    let copier = FileCopier::new(1024 * 1024, false);
    let mut hasher = MultiHasher::new(HashAlgorithm::XxHash3_64);
    let pause = Arc::new(AtomicBool::new(false));
    let cancel = Arc::new(AtomicBool::new(false));

    let transferred = copier
        .copy_file(
            &src_path,
            &dst_path,
            Some(&mut hasher),
            pause,
            cancel,
            |_| {},
        )
        .unwrap();

    assert_eq!(transferred, 5 * 1024 * 1024);
    assert!(dst_path.exists());

    // Verify
    let matched = HashVerifier::verify_files(&src_path, &dst_path, HashAlgorithm::XxHash3_64).unwrap();
    assert!(matched);
}

#[test]
fn test_conflict_autorename() {
    let dir = tempdir().unwrap();
    let existing = dir.path().join("report.txt");
    std::fs::File::create(&existing).unwrap();

    let src = dir.path().join("source.txt");
    std::fs::File::create(&src).unwrap();

    let resolution = ConflictResolver::resolve(&src, &existing, ConflictPolicy::AutoRename);
    match resolution {
        ConflictResolutionAction::ProceedRenamed(new_path) => {
            assert_eq!(new_path.file_name().unwrap().to_str().unwrap(), "report (1).txt");
        }
        _ => panic!("Expected ProceedRenamed action"),
    }
}

#[test]
fn test_bom_and_file_list_parsing() {
    let dir = tempdir().unwrap();
    let sample_file = dir.path().join("sample.txt");
    std::fs::write(&sample_file, b"sample content").unwrap();

    // Create a FileList.dat with UTF-8 BOM
    let list_file = dir.path().join("FileList.dat");
    let mut file = std::fs::File::create(&list_file).unwrap();
    // Write UTF-8 BOM
    file.write_all(&[0xef, 0xbb, 0xbf]).unwrap();
    // Write sample_file path with quotes
    writeln!(file, "\"{}\"", sample_file.display()).unwrap();
    drop(file);

    let parsed = deepnode_core::read_file_list(&list_file);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0], sample_file.to_string_lossy());

    // Test FileScanner::scan with this list file
    let dst_dir = dir.path().join("destination");
    let sources = vec![format!("*{}", list_file.display())];
    let items = deepnode_core::engine::scanner::FileScanner::scan(&sources, &dst_dir).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].src_path, sample_file);
    assert_eq!(items[0].dst_path, dst_dir.join("sample.txt"));
}

#[tokio::test]
async fn test_coordinator_conflict_resolution_interactive() {
    use deepnode_core::engine::config::TransferConfig;
    use deepnode_core::engine::coordinator::TransferCoordinator;
    use deepnode_core::engine::events::{ConflictResolutionChoice, EngineEvent};
    use deepnode_core::engine::item::FileItem;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    let dir = tempdir().unwrap();
    let src_file = dir.path().join("conflict_src.txt");
    let dst_file = dir.path().join("conflict_dst.txt");

    std::fs::write(&src_file, b"NEW SOURCE CONTENT").unwrap();
    std::fs::write(&dst_file, b"OLD DEST CONTENT").unwrap();

    let items = vec![FileItem::new(
        1,
        src_file.clone(),
        dst_file.clone(),
        std::path::PathBuf::from("conflict_dst.txt"),
        18,
    )];

    let mut config = TransferConfig::default();
    config.conflict_policy = ConflictPolicy::AskUser;
    config.verify_mode = deepnode_core::engine::config::VerifyMode::Never;

    let pause_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let conflict_hook = Arc::new(tokio::sync::Mutex::new(None));

    let mut coordinator = TransferCoordinator::new(
        config,
        items,
        event_tx,
        pause_flag,
        cancel_flag,
    );
    coordinator.set_conflict_hook(Arc::clone(&conflict_hook));

    let coordinator_handle = tokio::spawn(async move {
        coordinator.run().await
    });

    // Monitor events and respond to ConflictDetected
    let mut conflict_detected = false;
    while let Some(event) = event_rx.recv().await {
        match event {
            EngineEvent::ConflictDetected(info) => {
                assert_eq!(info.filename, "conflict_src.txt");
                conflict_detected = true;
                // Wait briefly and send overwrite choice
                let mut hook_lock = conflict_hook.lock().await;
                if let Some(sender) = hook_lock.take() {
                    sender.send(ConflictResolutionChoice {
                        action: "overwrite".to_string(),
                        apply_to_all: false,
                    }).unwrap();
                }
            }
            EngineEvent::TransferCompleted(_) => {
                break;
            }
            _ => {}
        }
    }

    let stats_res = coordinator_handle.await.unwrap();
    assert!(stats_res.is_ok());
    assert!(conflict_detected);

    let content = std::fs::read_to_string(&dst_file).unwrap();
    assert_eq!(content, "NEW SOURCE CONTENT");
}

#[tokio::test]
async fn test_coordinator_conflict_resolution_rename() {
    use deepnode_core::engine::config::TransferConfig;
    use deepnode_core::engine::coordinator::TransferCoordinator;
    use deepnode_core::engine::events::{ConflictResolutionChoice, EngineEvent};
    use deepnode_core::engine::item::FileItem;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    let dir = tempdir().unwrap();
    let src_file = dir.path().join("conflict_src.txt");
    let dst_file = dir.path().join("conflict_dst.txt");

    std::fs::write(&src_file, b"NEW SOURCE CONTENT").unwrap();
    std::fs::write(&dst_file, b"OLD DEST CONTENT").unwrap();

    let items = vec![FileItem::new(
        1,
        src_file.clone(),
        dst_file.clone(),
        std::path::PathBuf::from("conflict_dst.txt"),
        18,
    )];

    let mut config = TransferConfig::default();
    config.conflict_policy = ConflictPolicy::AskUser;
    config.verify_mode = deepnode_core::engine::config::VerifyMode::Never;

    let pause_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let conflict_hook = Arc::new(tokio::sync::Mutex::new(None));

    let mut coordinator = TransferCoordinator::new(
        config,
        items,
        event_tx,
        pause_flag,
        cancel_flag,
    );
    coordinator.set_conflict_hook(Arc::clone(&conflict_hook));

    let coordinator_handle = tokio::spawn(async move {
        coordinator.run().await
    });

    while let Some(event) = event_rx.recv().await {
        match event {
            EngineEvent::ConflictDetected(_) => {
                let mut hook_lock = conflict_hook.lock().await;
                if let Some(sender) = hook_lock.take() {
                    sender.send(ConflictResolutionChoice {
                        action: "rename".to_string(),
                        apply_to_all: false,
                    }).unwrap();
                }
            }
            EngineEvent::TransferCompleted(_) => break,
            _ => {}
        }
    }

    let stats_res = coordinator_handle.await.unwrap();
    assert!(stats_res.is_ok());

    let old_content = std::fs::read_to_string(&dst_file).unwrap();
    assert_eq!(old_content, "OLD DEST CONTENT");

    let renamed_file = dir.path().join("conflict_dst (1).txt");
    assert!(renamed_file.exists());
    let new_content = std::fs::read_to_string(&renamed_file).unwrap();
    assert_eq!(new_content, "NEW SOURCE CONTENT");
}

#[tokio::test]
async fn test_coordinator_conflict_resolution_skip() {
    use deepnode_core::engine::config::TransferConfig;
    use deepnode_core::engine::coordinator::TransferCoordinator;
    use deepnode_core::engine::events::{ConflictResolutionChoice, EngineEvent};
    use deepnode_core::engine::item::FileItem;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    let dir = tempdir().unwrap();
    let src_file = dir.path().join("conflict_src.txt");
    let dst_file = dir.path().join("conflict_dst.txt");

    std::fs::write(&src_file, b"NEW SOURCE CONTENT").unwrap();
    std::fs::write(&dst_file, b"OLD DEST CONTENT").unwrap();

    let items = vec![FileItem::new(
        1,
        src_file.clone(),
        dst_file.clone(),
        std::path::PathBuf::from("conflict_dst.txt"),
        18,
    )];

    let mut config = TransferConfig::default();
    config.conflict_policy = ConflictPolicy::AskUser;
    config.verify_mode = deepnode_core::engine::config::VerifyMode::Never;

    let pause_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let conflict_hook = Arc::new(tokio::sync::Mutex::new(None));

    let mut coordinator = TransferCoordinator::new(
        config,
        items,
        event_tx,
        pause_flag,
        cancel_flag,
    );
    coordinator.set_conflict_hook(Arc::clone(&conflict_hook));

    let coordinator_handle = tokio::spawn(async move {
        coordinator.run().await
    });

    let mut file_skipped_event = false;
    while let Some(event) = event_rx.recv().await {
        match event {
            EngineEvent::ConflictDetected(_) => {
                let mut hook_lock = conflict_hook.lock().await;
                if let Some(sender) = hook_lock.take() {
                    sender.send(ConflictResolutionChoice {
                        action: "skip".to_string(),
                        apply_to_all: false,
                    }).unwrap();
                }
            }
            EngineEvent::FileSkipped { .. } => {
                file_skipped_event = true;
            }
            EngineEvent::TransferCompleted(_) => break,
            _ => {}
        }
    }

    let stats_res = coordinator_handle.await.unwrap();
    assert!(stats_res.is_ok());
    assert!(file_skipped_event);

    let content = std::fs::read_to_string(&dst_file).unwrap();
    assert_eq!(content, "OLD DEST CONTENT");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_coordinator_cancellation() {
    use deepnode_core::engine::config::TransferConfig;
    use deepnode_core::engine::coordinator::TransferCoordinator;
    use deepnode_core::engine::events::EngineEvent;
    use deepnode_core::engine::item::FileItem;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    let dir = tempdir().unwrap();
    let src_file = dir.path().join("large_src.bin");
    let dst_file = dir.path().join("large_dst.bin");

    // 10 MB file
    let data = vec![0x55; 10 * 1024 * 1024];
    std::fs::write(&src_file, &data).unwrap();

    let items = vec![FileItem::new(
        1,
        src_file.clone(),
        dst_file.clone(),
        std::path::PathBuf::from("large_dst.bin"),
        10 * 1024 * 1024,
    )];

    let config = TransferConfig::default();
    let pause_flag = Arc::new(AtomicBool::new(true)); // Start paused so it waits in copier
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    let cancel_flag_clone = Arc::clone(&cancel_flag);
    let mut coordinator = TransferCoordinator::new(
        config,
        items,
        event_tx,
        pause_flag,
        cancel_flag,
    );

    let coordinator_handle = tokio::spawn(async move {
        coordinator.run().await
    });

    let mut transfer_canceled_event_received = false;
    let mut completed_event_received = false;

    // Listen to events and cancel as soon as FileStarted is received
    while let Some(event) = event_rx.recv().await {
        match event {
            EngineEvent::FileStarted(_) => {
                cancel_flag_clone.store(true, Ordering::Relaxed);
            }
            EngineEvent::TransferCanceled => {
                transfer_canceled_event_received = true;
                break;
            }
            EngineEvent::TransferCompleted(_) => {
                completed_event_received = true;
                break;
            }
            _ => {}
        }
    }

    let stats_res = coordinator_handle.await.unwrap();
    assert!(stats_res.is_err(), "Expected coordinator to return error on cancellation");
    assert!(transfer_canceled_event_received, "Expected TransferCanceled event");
    assert!(!completed_event_received, "Should not emit TransferCompleted on cancellation");
    assert!(!dst_file.exists(), "Partially copied destination file must be deleted on cancellation!");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cancellation_cleans_partially_transferred_file() {
    use deepnode_core::engine::config::TransferConfig;
    use deepnode_core::engine::coordinator::TransferCoordinator;
    use deepnode_core::engine::events::EngineEvent;
    use deepnode_core::engine::item::FileItem;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc;

    let dir = tempdir().unwrap();
    let src_file = dir.path().join("partial_src.bin");
    let dst_file = dir.path().join("partial_dst.bin");

    // 20 MB file
    let data = vec![0xAB; 20 * 1024 * 1024];
    std::fs::write(&src_file, &data).unwrap();

    let items = vec![FileItem::new(
        1,
        src_file.clone(),
        dst_file.clone(),
        std::path::PathBuf::from("partial_dst.bin"),
        20 * 1024 * 1024,
    )];

    let mut config = TransferConfig::default();
    config.buffer_size = 512 * 1024; // smaller buffer so multiple chunks occur
    let pause_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    let cancel_flag_clone = Arc::clone(&cancel_flag);
    let mut coordinator = TransferCoordinator::new(
        config,
        items,
        event_tx,
        pause_flag,
        cancel_flag,
    );

    let coordinator_handle = tokio::spawn(async move {
        coordinator.run().await
    });

    let mut canceled_received = false;

    // Trigger cancel as soon as progress shows some bytes copied
    while let Some(event) = event_rx.recv().await {
        match event {
            EngineEvent::FileProgress { transferred, .. } if transferred > 1024 * 1024 => {
                cancel_flag_clone.store(true, Ordering::SeqCst);
            }
            EngineEvent::TransferCanceled => {
                canceled_received = true;
                break;
            }
            EngineEvent::TransferCompleted(_) => break,
            _ => {}
        }
    }

    let stats_res = coordinator_handle.await.unwrap();
    assert!(stats_res.is_err(), "Coordinator should return Err on cancel");
    assert!(canceled_received, "TransferCanceled event must be received");

    // Give 100ms for OS thread teardown
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    assert!(
        !dst_file.exists(),
        "Partial 20MB destination file must be completely removed from disk after cancellation!"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cancellation_cleans_empty_parent_folder() {
    use deepnode_core::engine::config::TransferConfig;
    use deepnode_core::engine::coordinator::TransferCoordinator;
    use deepnode_core::engine::events::EngineEvent;
    use deepnode_core::engine::scanner::FileScanner;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc;

    let dir = tempdir().unwrap();
    let src_dir = dir.path().join("source");
    let src_sub = src_dir.join("MovieFolder");
    let dst_dir = dir.path().join("dest");
    std::fs::create_dir_all(&src_sub).unwrap();
    std::fs::create_dir_all(&dst_dir).unwrap();

    let movie_file = src_sub.join("big_movie.mkv");
    // Write 25 MB file
    let big_data = vec![0xAB; 25 * 1024 * 1024];
    std::fs::write(&movie_file, big_data).unwrap();

    let sources = vec![src_sub.to_string_lossy().to_string()];
    let items = FileScanner::scan(&sources, &dst_dir).unwrap();
    assert_eq!(items.len(), 1);

    let actual_dst_file = items[0].dst_path.clone();
    let actual_dst_folder = actual_dst_file.parent().unwrap().to_path_buf();

    let mut config = TransferConfig::default();
    config.verify_mode = deepnode_core::engine::config::VerifyMode::Never;
    config.buffer_size = 512 * 1024;

    let pause_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag_clone = Arc::clone(&cancel_flag);
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    let mut coordinator = TransferCoordinator::new(
        config,
        items,
        event_tx,
        pause_flag,
        cancel_flag,
    );
    coordinator.set_destination_dir(dst_dir.clone());

    let coordinator_handle = tokio::spawn(async move {
        coordinator.run().await
    });

    // Trigger cancel as soon as progress shows some bytes copied
    while let Some(event) = event_rx.recv().await {
        match event {
            EngineEvent::FileProgress { transferred, .. } if transferred > 1024 * 1024 => {
                cancel_flag_clone.store(true, Ordering::SeqCst);
            }
            EngineEvent::TransferCanceled => {
                break;
            }
            EngineEvent::TransferCompleted(_) => break,
            _ => {}
        }
    }

    let stats_res = coordinator_handle.await.unwrap();
    assert!(stats_res.is_err(), "Coordinator should return Err on cancel");

    // Give 100ms for OS thread teardown
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    assert!(
        !actual_dst_file.exists(),
        "Partial file inside MovieFolder must be deleted on cancellation!"
    );
    assert!(
        !actual_dst_folder.exists(),
        "Empty MovieFolder directory created on destination must be completely deleted on cancellation!"
    );
    assert!(
        dst_dir.exists(),
        "Base destination directory must be preserved!"
    );
}

#[tokio::test]
async fn test_coordinator_live_progress_and_events() {
    use deepnode_core::engine::config::TransferConfig;
    use deepnode_core::engine::coordinator::TransferCoordinator;
    use deepnode_core::engine::events::EngineEvent;
    use deepnode_core::engine::scanner::FileScanner;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    let dir = tempdir().unwrap();
    let src_dir = dir.path().join("source");
    let dst_dir = dir.path().join("dest");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&dst_dir).unwrap();

    // Create 2 test files (3 MB and 2 MB)
    let file1 = src_dir.join("movie.mkv");
    let file2 = src_dir.join("subtitles.sup");
    std::fs::write(&file1, vec![0x42; 3 * 1024 * 1024]).unwrap();
    std::fs::write(&file2, vec![0x24; 2 * 1024 * 1024]).unwrap();

    let sources = vec![src_dir.to_string_lossy().to_string()];
    let items = FileScanner::scan(&sources, &dst_dir).unwrap();
    assert_eq!(items.len(), 2);

    let config = TransferConfig::default();
    let pause_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    let mut coordinator = TransferCoordinator::new(
        config,
        items,
        event_tx,
        pause_flag,
        cancel_flag,
    );

    let coordinator_handle = tokio::spawn(async move {
        coordinator.run().await
    });

    let mut events_received = Vec::new();
    let mut progress_count = 0;
    let mut file_progress_count = 0;

    while let Some(event) = event_rx.recv().await {
        match &event {
            EngineEvent::TransferStarted { total_files, total_bytes, .. } => {
                events_received.push(format!("TransferStarted: files={}, bytes={}", total_files, total_bytes));
            }
            EngineEvent::FileStarted(item) => {
                events_received.push(format!("FileStarted: {}", item.relative_path.display()));
            }
            EngineEvent::ProgressUpdated(_) => {
                progress_count += 1;
            }
            EngineEvent::FileProgress { .. } => {
                file_progress_count += 1;
            }
            EngineEvent::FileCompleted(item) => {
                events_received.push(format!("FileCompleted: {}", item.relative_path.display()));
            }
            EngineEvent::TransferCompleted(stats) => {
                events_received.push(format!("TransferCompleted: completed={}", stats.completed_files));
                break;
            }
            _ => {}
        }
    }

    let stats_res = coordinator_handle.await.unwrap();
    assert!(stats_res.is_ok(), "Coordinator failed: {:?}", stats_res.err());
    println!("Events: {:?}", events_received);
    println!("Progress updates: {}, File progress updates: {}", progress_count, file_progress_count);
    assert!(progress_count > 0, "Expected at least one ProgressUpdated event");
    assert!(file_progress_count > 0, "Expected at least one FileProgress event");
}

