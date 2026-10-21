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
    let sample = b"The quick brown fox jumps over the lazy dog";

    // 1. Authoritative Golden Vector Assertions (RFC / Standard reference vectors)
    let mut md5 = MultiHasher::new(HashAlgorithm::Md5);
    md5.update(sample);
    assert_eq!(md5.finalize_hex(), "9e107d9d372bb6826bd81d3542a419d6", "MD5 golden vector mismatch");

    let mut sha1 = MultiHasher::new(HashAlgorithm::Sha1);
    sha1.update(sample);
    assert_eq!(sha1.finalize_hex(), "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12", "SHA1 golden vector mismatch");

    let mut sha256 = MultiHasher::new(HashAlgorithm::Sha256);
    sha256.update(sample);
    assert_eq!(sha256.finalize_hex(), "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592", "SHA256 golden vector mismatch");

    let mut blake = MultiHasher::new(HashAlgorithm::Blake3);
    blake.update(sample);
    assert_eq!(blake.finalize_hex(), "2f1514181aadccd913abd94cfa592701a5686ab23f8df1dff1b74710febc6d4a", "Blake3 golden vector mismatch");

    let mut xxh64 = MultiHasher::new(HashAlgorithm::XxHash3_64);
    xxh64.update(sample);
    let expected_xxh64 = format!("{:016x}", xxhash_rust::xxh3::xxh3_64(sample));
    assert_eq!(xxh64.finalize_hex(), expected_xxh64, "xxHash3-64 vector mismatch against reference implementation");

    // 2. Incremental Streaming / Chunking Equivalence:
    // Hashing in irregular chunk sizes (1, 3, 7 bytes) must yield identical digest to single-block update
    let mut streaming_xxh64 = MultiHasher::new(HashAlgorithm::XxHash3_64);
    for chunk in sample.chunks(7) {
        streaming_xxh64.update(chunk);
    }
    assert_eq!(streaming_xxh64.finalize_hex(), expected_xxh64, "Streaming chunked xxHash3-64 must match full pass");

    let mut streaming_sha256 = MultiHasher::new(HashAlgorithm::Sha256);
    for chunk in sample.chunks(5) {
        streaming_sha256.update(chunk);
    }
    assert_eq!(streaming_sha256.finalize_hex(), "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592", "Streaming chunked SHA256 must match full pass");
}

#[test]
fn test_file_copy_and_verification() {
    use deepnode_core::error::TransferError;

    let dir = tempdir().unwrap();
    let src_path = dir.path().join("source_unaligned.dat");
    let dst_path = dir.path().join("destination_unaligned.dat");

    // Sector-unaligned, non-trivial prime file size: 123,457 bytes (123457 % 4096 = 577, 123457 % 512 = 65)
    // Non-repeating pseudorandom patterned payload to ensure byte-level integrity
    let mut data = Vec::with_capacity(123_457);
    for i in 0usize..123_457 {
        data.push((i.wrapping_mul(167).wrapping_add(13)) as u8);
    }
    std::fs::write(&src_path, &data).unwrap();

    let golden_hash = format!("{:016x}", xxhash_rust::xxh3::xxh3_64(&data));

    let copier = FileCopier::new(64 * 1024, false);
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

    assert_eq!(transferred, 123_457, "Copied bytes must match exact source size");
    assert_eq!(hasher.finalize_hex(), golden_hash, "Stream-computed hash during copy must match authoritative golden hash");
    assert_eq!(std::fs::metadata(&dst_path).unwrap().len(), 123_457, "Destination file size must be exact on disk");

    // 1. Positive Verification: Target hash matches golden hash directly
    let computed_dst_hash = HashVerifier::compute_file_hash(&dst_path, HashAlgorithm::XxHash3_64, false).unwrap();
    assert_eq!(computed_dst_hash, golden_hash, "Destination file on disk must independently hash to golden hash");

    let matched = HashVerifier::verify_files(&src_path, &dst_path, HashAlgorithm::XxHash3_64).unwrap();
    assert!(matched, "Source and destination must verify as matching");

    // 2. Negative Verification: Single-byte corruption detection
    let mut corrupted = data.clone();
    corrupted[65_432] ^= 0xFF; // Invert all bits of byte 65432
    std::fs::write(&dst_path, &corrupted).unwrap();

    let corrupted_res = HashVerifier::verify_files(&src_path, &dst_path, HashAlgorithm::XxHash3_64);
    assert!(
        matches!(corrupted_res, Err(TransferError::HashMismatch { .. })),
        "Single bit-flipped byte MUST return TransferError::HashMismatch error! Got: {:?}",
        corrupted_res
    );

    // 3. Negative Verification: Truncation corruption detection
    let truncated = &data[..data.len() - 1];
    std::fs::write(&dst_path, truncated).unwrap();

    let truncated_res = HashVerifier::verify_files(&src_path, &dst_path, HashAlgorithm::XxHash3_64);
    assert!(
        matches!(truncated_res, Err(TransferError::HashMismatch { .. })),
        "File truncated by 1 byte MUST return TransferError::HashMismatch error! Got: {:?}",
        truncated_res
    );
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

    let stats = coordinator_handle.await.unwrap().unwrap();
    assert!(file_skipped_event);
    assert_eq!(stats.skipped_files, 1);
    assert_eq!(stats.total_files, 0, "A skipped file must not remain in the progress total");
    assert_eq!(stats.total_bytes, 0);
    assert!((stats.overall_percentage - 100.0).abs() < 0.01);

    let content = std::fs::read_to_string(&dst_file).unwrap();
    assert_eq!(content, "OLD DEST CONTENT");
}

#[tokio::test]
async fn test_removed_queued_file_is_skipped_and_excluded_from_totals() {
    use deepnode_core::engine::config::{TransferConfig, VerifyMode};
    use deepnode_core::engine::coordinator::TransferCoordinator;
    use deepnode_core::engine::events::EngineEvent;
    use deepnode_core::engine::item::FileItem;
    use std::collections::HashSet;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    let dir = tempdir().unwrap();
    let removed_src = dir.path().join("removed.bin");
    let kept_src = dir.path().join("kept.bin");
    std::fs::write(&removed_src, vec![0x11; 100]).unwrap();
    std::fs::write(&kept_src, vec![0x22; 500]).unwrap();

    let removed_dst = dir.path().join("removed_dst.bin");
    let kept_dst = dir.path().join("kept_dst.bin");

    let items = vec![
        FileItem::new(1, removed_src, removed_dst.clone(), std::path::PathBuf::from("removed_dst.bin"), 100),
        FileItem::new(2, kept_src, kept_dst.clone(), std::path::PathBuf::from("kept_dst.bin"), 500),
    ];

    let mut skipped_ids = HashSet::new();
    skipped_ids.insert(1);

    let mut config = TransferConfig::default();
    config.verify_mode = VerifyMode::Never;

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut coordinator = TransferCoordinator::new(
        config,
        items,
        event_tx,
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    coordinator.set_skip_controls(
        Arc::new(AtomicBool::new(false)),
        Some(Arc::new(std::sync::Mutex::new(skipped_ids))),
    );

    let handle = tokio::spawn(async move { coordinator.run().await });

    let mut completed_stats = None;
    while let Some(event) = event_rx.recv().await {
        if let EngineEvent::TransferCompleted(stats) = event {
            completed_stats = Some(stats);
        }
    }
    let stats = handle.await.unwrap().unwrap();
    let completed_stats = completed_stats.expect("TransferCompleted must be emitted");

    assert!(!removed_dst.exists(), "Removed queued file must not be copied");
    assert!(kept_dst.exists(), "Remaining queued file must still be copied");

    assert_eq!(stats.skipped_files, 1);
    assert_eq!(stats.completed_files, 1);
    assert_eq!(stats.total_files, 1, "Removed file must be dropped from the total");
    assert_eq!(stats.total_bytes, 500, "Removed file bytes must be dropped from the total");
    assert!((stats.overall_percentage - 100.0).abs() < 0.01, "Progress must reach 100% after removal");
    assert_eq!(completed_stats.total_files, 1);
    assert_eq!(completed_stats.overall_percentage, 100.0);
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

    let config = TransferConfig {
        buffer_size: 256 * 1024,
        ..Default::default()
    };
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

    let config = TransferConfig {
        buffer_size: 512 * 1024, // smaller buffer so multiple chunks occur
        ..Default::default()
    };
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

#[tokio::test]
async fn test_coordinator_conflict_cancellation_preserves_existing_target_file() {
    use deepnode_core::engine::config::TransferConfig;
    use deepnode_core::engine::coordinator::TransferCoordinator;
    use deepnode_core::engine::events::EngineEvent;
    use deepnode_core::engine::item::FileItem;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc;

    let dir = tempdir().unwrap();
    let src_file = dir.path().join("src_new.txt");
    let dst_file = dir.path().join("precious_existing_user_file.txt");

    std::fs::write(&src_file, b"INCOMING NEW CONTENT").unwrap();
    std::fs::write(&dst_file, b"PRECIOUS USER DATA THAT MUST NEVER BE DELETED").unwrap();

    let items = vec![FileItem::new(
        1,
        src_file.clone(),
        dst_file.clone(),
        std::path::PathBuf::from("precious_existing_user_file.txt"),
        20,
    )];

    let mut config = TransferConfig::default();
    config.conflict_policy = ConflictPolicy::AskUser;
    config.verify_mode = deepnode_core::engine::config::VerifyMode::Never;

    let pause_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let conflict_hook = Arc::new(tokio::sync::Mutex::new(None));

    let cancel_flag_clone = Arc::clone(&cancel_flag);
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
                // User sees conflict dialog and clicks "Cancel" (or closes window)
                cancel_flag_clone.store(true, Ordering::SeqCst);
                // Also drop the hook sender
                let mut hook_lock = conflict_hook.lock().await;
                let _ = hook_lock.take();
            }
            EngineEvent::TransferCanceled => break,
            _ => {}
        }
    }

    let stats_res = coordinator_handle.await.unwrap();
    assert!(stats_res.is_err(), "Coordinator should return Err(Canceled)");

    // CRITICAL ASSERTION: The destination file MUST STILL EXIST and possess original content!
    assert!(dst_file.exists(), "Target file was deleted on conflict cancellation! Data loss bug!");
    let preserved_content = std::fs::read_to_string(&dst_file).unwrap();
    assert_eq!(
        preserved_content,
        "PRECIOUS USER DATA THAT MUST NEVER BE DELETED",
        "Target file content was corrupted or overwritten on conflict cancellation!"
    );
}

#[test]
fn test_copier_pause_then_cancel_preserves_existing_target_file() {
    use deepnode_core::io::copier::FileCopier;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let dir = tempdir().unwrap();
    let src_file = dir.path().join("source.bin");
    let dst_file = dir.path().join("preexisting_dest.bin");

    std::fs::write(&src_file, b"NEW INCOMING DATA TO COPY").unwrap();
    std::fs::write(&dst_file, b"ORIGINAL PRE-EXISTING DATA TO PRESERVE").unwrap();

    let pause_flag = Arc::new(AtomicBool::new(true)); // Start in PAUSED state
    let cancel_flag = Arc::new(AtomicBool::new(false));

    let cancel_flag_clone = Arc::clone(&cancel_flag);

    // Spawn thread that simulates user hitting Cancel while paused
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(80));
        cancel_flag_clone.store(true, Ordering::SeqCst);
    });

    let copier = FileCopier::new(1024 * 1024, false);
    let res = copier.copy_file_with_skip(
        &src_file,
        &dst_file,
        None,
        pause_flag,
        cancel_flag,
        None,
        |_| {},
    );

    assert!(res.is_err(), "Copying should have been canceled");

    // CRITICAL ASSERTION: The destination file MUST STILL EXIST and possess original content!
    assert!(dst_file.exists(), "Pre-existing target file was deleted on Pause->Cancel! Critical data loss bug!");
    let preserved = std::fs::read_to_string(&dst_file).unwrap();
    assert_eq!(
        preserved,
        "ORIGINAL PRE-EXISTING DATA TO PRESERVE",
        "Pre-existing target file was truncated or modified on Pause->Cancel!"
    );
}

#[test]
fn test_to_wide_path_unc_and_local() {
    use deepnode_core::io::win32_file::Win32FileHandle;
    use std::path::Path;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    // Test 1: Standard local drive path
    let local = Path::new(r"C:\Folder\file.txt");
    let wide = Win32FileHandle::to_wide_path(local);
    let s = OsString::from_wide(&wide[..wide.len() - 1]).to_string_lossy().into_owned();
    assert_eq!(s, r"\\?\C:\Folder\file.txt");

    // Test 2: UNC network path
    let unc = Path::new(r"\\server\share\file.txt");
    let wide_unc = Win32FileHandle::to_wide_path(unc);
    let s_unc = OsString::from_wide(&wide_unc[..wide_unc.len() - 1]).to_string_lossy().into_owned();
    assert_eq!(s_unc, r"\\?\UNC\server\share\file.txt");

    // Test 3: Already prefixed with \\?\
    let prefixed = Path::new(r"\\?\D:\Data\file.bin");
    let wide_pre = Win32FileHandle::to_wide_path(prefixed);
    let s_pre = OsString::from_wide(&wide_pre[..wide_pre.len() - 1]).to_string_lossy().into_owned();
    assert_eq!(s_pre, r"\\?\D:\Data\file.bin");

    // Test 4: Already prefixed UNC \\?\UNC\
    let prefixed_unc = Path::new(r"\\?\UNC\server\share\file.txt");
    let wide_pre_unc = Win32FileHandle::to_wide_path(prefixed_unc);
    let s_pre_unc = OsString::from_wide(&wide_pre_unc[..wide_pre_unc.len() - 1]).to_string_lossy().into_owned();
    assert_eq!(s_pre_unc, r"\\?\UNC\server\share\file.txt");
}

#[test]
fn test_copier_direct_io_unaligned_file_size() {
    use deepnode_core::io::copier::FileCopier;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    let dir = tempdir().unwrap();
    let src = dir.path().join("direct_src.bin");
    let dst = dir.path().join("direct_dst.bin");

    // 123,456 bytes: > 64KB and not a multiple of 4096 (123456 % 4096 = 576)
    // If Direct I/O slack space / sector alignment was unhandled, this would fail with Win32 Error 87
    let payload = vec![0xABu8; 123_456];
    std::fs::write(&src, &payload).unwrap();

    let pause_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::new(AtomicBool::new(false));

    let copier = FileCopier::new(1024 * 1024, true);
    let copied = copier.copy_file(
        &src,
        &dst,
        None,
        pause_flag,
        cancel_flag,
        |_| {},
    ).expect("Direct I/O copy should succeed without Error 87");

    assert_eq!(copied, 123_456);
    assert_eq!(std::fs::metadata(&dst).unwrap().len(), 123_456);
    let dst_data = std::fs::read(&dst).unwrap();
    assert_eq!(dst_data, payload, "Destination content must match source byte-for-byte");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_verify_mode_on_difference_only() {
    use deepnode_core::engine::config::{TransferConfig, VerifyMode};
    use deepnode_core::engine::coordinator::TransferCoordinator;
    use deepnode_core::engine::events::EngineEvent;
    use deepnode_core::engine::item::FileItem;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    let dir = tempdir().unwrap();
    let src_file = dir.path().join("source_diff.txt");
    let dst_file = dir.path().join("dest_diff.txt");

    // Write source file and an existing destination file with different content
    std::fs::write(&src_file, b"NEW CONTENT DIFFERENT FROM DEST").unwrap();
    std::fs::write(&dst_file, b"OLD PRE-EXISTING CONTENT").unwrap();

    let config = TransferConfig {
        verify_mode: VerifyMode::OnDifferenceOnly,
        conflict_policy: deepnode_core::conflict::policy::ConflictPolicy::Overwrite,
        ..Default::default()
    };

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let pause_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::new(AtomicBool::new(false));

    let item = FileItem::new(
        1,
        src_file.clone(),
        dst_file.clone(),
        std::path::PathBuf::from("diff.txt"),
        std::fs::metadata(&src_file).unwrap().len(),
    );

    let mut coordinator = TransferCoordinator::new(
        config,
        vec![item],
        event_tx,
        pause_flag,
        cancel_flag,
    );

    let coordinator_handle = tokio::spawn(async move {
        coordinator.run().await
    });

    let mut verification_started = false;
    let mut verification_matched = false;

    while let Some(event) = event_rx.recv().await {
        match event {
            EngineEvent::VerificationStarted(_) => {
                verification_started = true;
            }
            EngineEvent::VerificationCompleted { matched, .. } if matched => {
                verification_matched = true;
            }
            _ => {}
        }
    }

    coordinator_handle.await.unwrap().unwrap();

    assert!(verification_started, "VerifyMode::OnDifferenceOnly must trigger verification on differing files!");
    assert!(verification_matched, "Verification must match source and destination hashes!");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_overwrite_readonly_target_file() {
    use deepnode_core::engine::config::TransferConfig;
    use deepnode_core::engine::coordinator::TransferCoordinator;
    use deepnode_core::engine::item::FileItem;
    use deepnode_core::conflict::policy::ConflictPolicy;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    let dir = tempdir().unwrap();
    let src_file = dir.path().join("readonly_src.txt");
    let dst_file = dir.path().join("readonly_dst.txt");

    // Write source file and an initial destination file
    std::fs::write(&src_file, b"INITIAL CONTENT").unwrap();
    std::fs::write(&dst_file, b"INITIAL CONTENT").unwrap();

    // Mark the destination file as READ-ONLY on disk
    let mut perms = std::fs::metadata(&dst_file).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&dst_file, perms).unwrap();
    assert!(std::fs::metadata(&dst_file).unwrap().permissions().readonly());

    // Update source file with new content that needs to overwrite the read-only destination
    std::fs::write(&src_file, b"UPDATED SECOND PASS OVERWRITE DATA").unwrap();

    let config = TransferConfig {
        conflict_policy: ConflictPolicy::Overwrite,
        preserve_attributes: true,
        preserve_timestamps: true,
        ..Default::default()
    };

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let pause_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::new(AtomicBool::new(false));

    let item = FileItem::new(
        1,
        src_file.clone(),
        dst_file.clone(),
        std::path::PathBuf::from("readonly_dst.txt"),
        std::fs::metadata(&src_file).unwrap().len(),
    );

    let mut coordinator = TransferCoordinator::new(
        config,
        vec![item],
        event_tx,
        pause_flag,
        cancel_flag,
    );

    let coordinator_handle = tokio::spawn(async move {
        coordinator.run().await
    });

    while event_rx.recv().await.is_some() {}

    let stats = coordinator_handle.await.unwrap().expect("Transfer should succeed overwriting read-only target without Error 5");
    assert_eq!(stats.completed_files, 1, "Completed files should be 1");
    assert_eq!(stats.failed_files, 0, "Failed files should be 0");

    let final_content = std::fs::read_to_string(&dst_file).unwrap();
    assert_eq!(final_content, "UPDATED SECOND PASS OVERWRITE DATA");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_timestamps_preserved_when_ads_written() {
    use deepnode_core::engine::config::TransferConfig;
    use deepnode_core::engine::coordinator::TransferCoordinator;
    use deepnode_core::engine::item::FileItem;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use std::time::{Duration, SystemTime};
    use tokio::sync::mpsc;

    let dir = tempdir().unwrap();
    let src_file = dir.path().join("source_with_timestamp.txt");
    let dst_file = dir.path().join("dest_with_timestamp.txt");

    std::fs::write(&src_file, b"DATA TO PRESERVE TIMESTAMPS WITH ADS").unwrap();

    // Set source file's last modified time to 10 days in the past
    let ten_days_ago = SystemTime::now() - Duration::from_secs(10 * 86400);
    let src_handle = deepnode_core::io::win32_file::Win32FileHandle::open_for_timestamps(&src_file).unwrap();
    let ft = deepnode_core::metadata::timestamps::FileTimestamps {
        creation_time: deepnode_core::metadata::timestamps::FileTimestamps::system_time_to_filetime(ten_days_ago),
        last_access_time: deepnode_core::metadata::timestamps::FileTimestamps::system_time_to_filetime(ten_days_ago),
        last_write_time: deepnode_core::metadata::timestamps::FileTimestamps::system_time_to_filetime(ten_days_ago),
    };
    ft.apply(&src_handle).unwrap();
    drop(src_handle);

    let src_mod_before = std::fs::metadata(&src_file).unwrap().modified().unwrap();

    let config = TransferConfig {
        preserve_timestamps: true,
        preserve_ads: true,
        write_checksum_to_ads: true,
        ..Default::default()
    };

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let pause_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::new(AtomicBool::new(false));

    let item = FileItem::new(
        1,
        src_file.clone(),
        dst_file.clone(),
        std::path::PathBuf::from("dest_with_timestamp.txt"),
        std::fs::metadata(&src_file).unwrap().len(),
    );

    let mut coordinator = TransferCoordinator::new(
        config,
        vec![item],
        event_tx,
        pause_flag,
        cancel_flag,
    );

    let coordinator_handle = tokio::spawn(async move {
        coordinator.run().await
    });

    while event_rx.recv().await.is_some() {}

    let stats = coordinator_handle.await.unwrap().expect("Transfer should succeed");
    assert_eq!(stats.completed_files, 1);

    // Assert that dst_file's modified time preserved the 10-days-ago timestamp
    // and was NOT overwritten to `now()` by ADS or checksum stream writing!
    let dst_mod = std::fs::metadata(&dst_file).unwrap().modified().unwrap();
    let diff = if dst_mod > src_mod_before {
        dst_mod.duration_since(src_mod_before).unwrap()
    } else {
        src_mod_before.duration_since(dst_mod).unwrap()
    };
    assert!(diff < Duration::from_secs(2), "Destination modified time was overwritten by ADS write! Expected ~10 days ago, diff: {:?}", diff);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_single_file_cancellation_preserves_destination_directory_without_set_destination_dir() {
    use deepnode_core::engine::config::TransferConfig;
    use deepnode_core::engine::coordinator::TransferCoordinator;
    use deepnode_core::engine::events::EngineEvent;
    use deepnode_core::engine::item::FileItem;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc;

    let temp = tempdir().unwrap();
    let target_dir = temp.path().join("TargetFolder");
    std::fs::create_dir_all(&target_dir).unwrap();
    assert!(target_dir.exists(), "Target folder must exist initially");

    let src_file = temp.path().join("single_source.bin");
    // Write 25 MB file so transfer runs long enough to trigger cancellation mid-flight
    let data = vec![0xEE; 25 * 1024 * 1024];
    std::fs::write(&src_file, data).unwrap();

    let dst_file = target_dir.join("single_source.bin");

    let mut config = TransferConfig::default();
    config.buffer_size = 512 * 1024;
    config.verify_mode = deepnode_core::engine::config::VerifyMode::Never;

    let pause_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag_clone = Arc::clone(&cancel_flag);
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    let item = FileItem::new(
        1,
        src_file.clone(),
        dst_file.clone(),
        std::path::PathBuf::from("single_source.bin"),
        std::fs::metadata(&src_file).unwrap().len(),
    );

    let mut coordinator = TransferCoordinator::new(
        config,
        vec![item],
        event_tx,
        pause_flag,
        cancel_flag,
    );
    // CRITICAL: DO NOT CALL set_destination_dir! This tests the coordinator's
    // automatic destination inference for single file sources.

    let coordinator_handle = tokio::spawn(async move {
        coordinator.run().await
    });

    while let Some(event) = event_rx.recv().await {
        match event {
            EngineEvent::FileProgress { transferred, .. } if transferred > 1024 * 1024 => {
                cancel_flag_clone.store(true, Ordering::SeqCst);
            }
            EngineEvent::TransferCanceled => break,
            EngineEvent::TransferCompleted(_) => break,
            _ => {}
        }
    }

    let res = coordinator_handle.await.unwrap();
    assert!(res.is_err(), "Coordinator must return Err on cancellation");

    // Wait for file handles to close
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Partial file should be cleaned up
    assert!(!dst_file.exists(), "Partially copied file must be deleted on cancellation");

    // CRITICAL BUG VERIFICATION (H7):
    // TargetFolder MUST NOT be deleted even though it is now empty!
    // The old off-by-one bug caused stop_dir to be TargetFolder's parent, wiping TargetFolder!
    assert!(
        target_dir.exists(),
        "CRITICAL: TargetFolder was incorrectly deleted on single file cancellation! stop_dir was off-by-one."
    );
}

#[test]
fn test_deepnode_temp_list_cleanup_and_identification() {
    use deepnode_core::is_deepnode_temp_list;
    use deepnode_core::engine::scanner::FileScanner;
    use tempfile::tempdir;
    use std::io::Write;

    let dir = tempdir().unwrap();

    let normal_list = dir.path().join("files.txt");
    let deepnode_list = dir.path().join(format!("deepnode_list_{}.tmp", uuid::Uuid::new_v4()));
    let other_tmp = dir.path().join("something_else.tmp");

    assert!(!is_deepnode_temp_list(&normal_list));
    assert!(is_deepnode_temp_list(&deepnode_list));
    assert!(!is_deepnode_temp_list(&other_tmp));

    let sample_file = dir.path().join("item.txt");
    std::fs::write(&sample_file, b"content").unwrap();

    // Test with deepnode_list: FileScanner should scan and remove it
    {
        let mut f = std::fs::File::create(&deepnode_list).unwrap();
        writeln!(f, "{}", sample_file.display()).unwrap();
    }
    assert!(deepnode_list.exists());

    let dst_dir = dir.path().join("dest");
    let sources = vec![format!("*{}", deepnode_list.display())];
    let items = FileScanner::scan(&sources, &dst_dir).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].src_path, sample_file);

    // DeepNode temp list MUST be automatically deleted after consumption
    assert!(!deepnode_list.exists(), "Temporary list file must be removed after scanning");

    // Normal list MUST NOT be deleted
    {
        let mut f = std::fs::File::create(&normal_list).unwrap();
        writeln!(f, "{}", sample_file.display()).unwrap();
    }
    assert!(normal_list.exists());
    let sources_normal = vec![format!("*{}", normal_list.display())];
    let items_normal = FileScanner::scan(&sources_normal, &dst_dir).unwrap();
    assert_eq!(items_normal.len(), 1);
    assert!(normal_list.exists(), "User list file must not be removed");
}

#[tokio::test]
async fn test_move_source_deletion_safety_and_readonly_source() {
    use deepnode_core::engine::config::{TransferConfig, TransferMode, VerifyMode};
    use deepnode_core::engine::coordinator::TransferCoordinator;
    use deepnode_core::engine::events::EngineEvent;
    use deepnode_core::engine::item::FileItem;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    let dir = tempdir().unwrap();
    let src_file = dir.path().join("move_src.txt");
    let dst_file = dir.path().join("move_dst.txt");

    std::fs::write(&src_file, b"MOVE SECURE CONTENT").unwrap();

    // Mark source file as read-only on Windows
    let mut perms = std::fs::metadata(&src_file).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&src_file, perms).unwrap();

    let items = vec![FileItem::new(
        1,
        src_file.clone(),
        dst_file.clone(),
        std::path::PathBuf::from("move_dst.txt"),
        19,
    )];

    let config = TransferConfig {
        mode: TransferMode::Move,
        verify_mode: VerifyMode::Always,
        ..Default::default()
    };

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

    while let Some(event) = event_rx.recv().await {
        if let EngineEvent::TransferCompleted(_) = event {
            break;
        }
    }

    let stats_res = coordinator_handle.await.unwrap();
    assert!(stats_res.is_ok());

    // Target must exist and have the correct content
    assert!(dst_file.exists());
    let content = std::fs::read_to_string(&dst_file).unwrap();
    assert_eq!(content, "MOVE SECURE CONTENT");

    // Source must be cleanly deleted (read-only attribute stripped prior to deletion)
    assert!(!src_file.exists(), "Source file must be deleted upon successful move");
}

#[test]
fn test_scanner_reports_error_on_missing_source() {
    use deepnode_core::engine::scanner::FileScanner;

    let dir = tempdir().unwrap();
    let non_existent = dir.path().join("missing_file.txt");
    let dst_dir = dir.path().join("dest");

    let sources = vec![non_existent.to_string_lossy().to_string()];
    let res = FileScanner::scan(&sources, &dst_dir);

    assert!(res.is_err(), "Scanner must return an error for non-existent sources");
}

#[test]
fn test_scanner_replicates_empty_directory() {
    use deepnode_core::engine::scanner::FileScanner;

    let dir = tempdir().unwrap();
    let src_dir = dir.path().join("src_folder");
    let empty_subfolder = src_dir.join("empty_nested");
    std::fs::create_dir_all(&empty_subfolder).unwrap();

    let dst_dir = dir.path().join("dest_folder");

    let sources = vec![src_dir.to_string_lossy().to_string()];
    let items = FileScanner::scan(&sources, &dst_dir).unwrap();

    // No files to transfer, but destination empty directory should have been created
    assert_eq!(items.len(), 0);
    assert!(dst_dir.join("src_folder").join("empty_nested").exists(), "Empty directory structure must be preserved");
}

#[tokio::test]
async fn test_retry_failed_only_excludes_skipped_items() {
    use deepnode_core::engine::config::TransferConfig;
    use deepnode_core::engine::coordinator::TransferCoordinator;
    use deepnode_core::engine::item::{FileItem, FileItemStatus};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    let dir = tempdir().unwrap();
    let mut item_failed = FileItem::new(
        1,
        dir.path().join("f1.txt"),
        dir.path().join("dst1.txt"),
        std::path::PathBuf::from("dst1.txt"),
        10,
    );
    item_failed.status = FileItemStatus::Failed;
    item_failed.error_message = Some("I/O error".to_string());

    let mut item_skipped = FileItem::new(
        2,
        dir.path().join("f2.txt"),
        dir.path().join("dst2.txt"),
        std::path::PathBuf::from("dst2.txt"),
        20,
    );
    item_skipped.status = FileItemStatus::Skipped;

    let mut item_success = FileItem::new(
        3,
        dir.path().join("f3.txt"),
        dir.path().join("dst3.txt"),
        std::path::PathBuf::from("dst3.txt"),
        30,
    );
    item_success.status = FileItemStatus::Completed;

    let coordinator = TransferCoordinator::new(
        TransferConfig::default(),
        vec![item_failed, item_skipped, item_success],
        mpsc::unbounded_channel().0,
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );

    let failed_items = coordinator.get_failed_items();
    let skipped_items = coordinator.get_skipped_items();

    assert_eq!(failed_items.len(), 1);
    assert_eq!(failed_items[0].id, 1);

    assert_eq!(skipped_items.len(), 1);
    assert_eq!(skipped_items[0].id, 2);

    // Retry must include both failed and skipped items, never completed ones.
    let retryable = coordinator.get_retryable_items();
    assert_eq!(retryable.len(), 2);
    assert_eq!(retryable[0].id, 1);
    assert_eq!(retryable[1].id, 2);
}

#[tokio::test]
async fn test_queue_manager_execution() {
    use deepnode_core::queue::{QueueManager, QueueTask, TaskState};
    use deepnode_core::engine::config::TransferConfig;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let manager = QueueManager::new(2);
    let task = QueueTask::new(
        "task-1".to_string(),
        "Test Task".to_string(),
        TransferConfig::default(),
        vec![],
    );

    manager.add_task(task).await;
    let tasks = manager.get_tasks().await;
    assert_eq!(tasks.len(), 1);

    let executed = Arc::new(AtomicBool::new(false));
    let executed_clone = Arc::clone(&executed);

    manager.process_queue(move |t| {
        let flag = Arc::clone(&executed_clone);
        async move {
            *t.state.lock().await = TaskState::Completed;
            flag.store(true, Ordering::SeqCst);
        }
    }).await;

    assert!(executed.load(Ordering::SeqCst), "Queue task should execute via process_queue");
    let tasks_after = manager.get_tasks().await;
    let state = *tasks_after[0].state.lock().await;
    assert_eq!(state, TaskState::Completed);
}

#[test]
fn test_xyplorer_curitem_star_resolution() {
    use deepnode_core::engine::scanner::FileScanner;
    use deepnode_core::util::{resolve_star_path, StarPathResolution};

    let dir = tempdir().unwrap();
    let dest_dir = dir.path().join("dest");
    std::fs::create_dir_all(&dest_dir).unwrap();

    // 1. Simulate XYplorer selecting a binary file (e.g. video.mp4) with *"path\video.mp4"
    let video_file = dir.path().join("vacation_clip.mp4");
    std::fs::write(&video_file, b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00isommp42").unwrap();

    let res = resolve_star_path(&video_file);
    assert!(matches!(res, StarPathResolution::SingleSource(_)));

    let scan_sources = vec![format!("*{}", video_file.display())];
    let scanned = FileScanner::scan(&scan_sources, &dest_dir).expect("scan video file");
    assert_eq!(scanned.len(), 1);
    assert_eq!(scanned[0].src_path, video_file);
    assert_eq!(scanned[0].dst_path, dest_dir.join("vacation_clip.mp4"));

    // 2. Simulate XYplorer selecting a text file with notes (lines don't exist as files)
    let notes_file = dir.path().join("notes.txt");
    std::fs::write(&notes_file, "First line of notes\nSecond line of notes\n").unwrap();

    let res2 = resolve_star_path(&notes_file);
    assert!(matches!(res2, StarPathResolution::SingleSource(_)));

    let scan_notes = vec![format!("*{}", notes_file.display())];
    let scanned_notes = FileScanner::scan(&scan_notes, &dest_dir).expect("scan notes file");
    assert_eq!(scanned_notes.len(), 1);
    assert_eq!(scanned_notes[0].src_path, notes_file);

    // 3. Genuine file list pointing to actual existing files
    let target1 = dir.path().join("target1.dat");
    let target2 = dir.path().join("target2.dat");
    std::fs::write(&target1, b"data1").unwrap();
    std::fs::write(&target2, b"data2").unwrap();

    let list_file = dir.path().join("FileList.lst");
    std::fs::write(&list_file, format!("{}\n{}\n", target1.display(), target2.display())).unwrap();

    let res3 = resolve_star_path(&list_file);
    assert!(matches!(res3, StarPathResolution::FileList(_)));

    let scan_list = vec![format!("*{}", list_file.display())];
    let scanned_list = FileScanner::scan(&scan_list, &dest_dir).expect("scan list file");
    assert_eq!(scanned_list.len(), 2);
    assert_eq!(scanned_list[0].src_path, target1);
    assert_eq!(scanned_list[1].src_path, target2);
}

#[test]
fn test_direct_io_sector_unaligned_boundaries_matrix() {
    let dir = tempdir().unwrap();

    // Critical Win32 Direct I/O boundary sizes:
    // Tests 1 byte, sector boundaries, cluster boundaries, 64KB Direct I/O threshold boundaries, and prime unaligned sizes
    let test_sizes = [
        1,          // Tiny file
        7,          // Arbitrary small
        511,        // Sector (512) - 1
        512,        // Exact sector
        513,        // Sector + 1
        4095,       // Cluster (4096) - 1
        4096,       // Exact cluster
        4097,       // Cluster + 1
        65_535,     // 64KB Direct I/O threshold - 1
        65_536,     // Exact 64KB threshold
        65_537,     // 64KB threshold + 1
        131_071,    // Prime irregular size
    ];

    let copier = FileCopier::new(1024 * 1024, true); // Direct I/O forced

    for (idx, &size) in test_sizes.iter().enumerate() {
        let src = dir.path().join(format!("direct_boundary_{}_src.bin", idx));
        let dst = dir.path().join(format!("direct_boundary_{}_dst.bin", idx));

        // Generate non-repeating pattern
        let mut payload = Vec::with_capacity(size);
        for i in 0..size {
            payload.push((i.wrapping_mul(31).wrapping_add(idx * 7)) as u8);
        }
        std::fs::write(&src, &payload).unwrap();

        let golden_hash = format!("{:016x}", xxhash_rust::xxh3::xxh3_64(&payload));

        let pause = Arc::new(AtomicBool::new(false));
        let cancel = Arc::new(AtomicBool::new(false));
        let mut hasher = MultiHasher::new(HashAlgorithm::XxHash3_64);

        let copied = copier
            .copy_file(&src, &dst, Some(&mut hasher), pause, cancel, |_| {})
            .unwrap_or_else(|e| panic!("Direct I/O copy failed for size {}: {}", size, e));

        assert_eq!(copied, size as u64, "Copied bytes mismatch for size {}", size);
        assert_eq!(hasher.finalize_hex(), golden_hash, "Direct I/O hash mismatch for size {}", size);

        // Verify on disk metadata size (no padding slack allowed)
        let dst_len = std::fs::metadata(&dst).unwrap().len();
        assert_eq!(dst_len, size as u64, "Disk file length had slack/padding for size {}", size);

        // Verify exact byte equality
        let read_back = std::fs::read(&dst).unwrap();
        assert_eq!(read_back, payload, "Disk payload corrupted for size {}", size);
    }
}

#[test]
fn test_direct_io_destination_truncation_no_slack_bytes() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("small_src.bin");
    let dst = dir.path().join("large_dst.bin");

    // Pre-create destination with 100,000 bytes filled with 0xFF
    let large_data = vec![0xFFu8; 100_000];
    std::fs::write(&dst, &large_data).unwrap();
    assert_eq!(std::fs::metadata(&dst).unwrap().len(), 100_000);

    // Overwrite with a smaller 13,337 byte source file using Direct I/O
    let small_payload: Vec<u8> = (0..13_337).map(|i| (i % 251) as u8).collect();
    std::fs::write(&src, &small_payload).unwrap();

    let copier = FileCopier::new(1024 * 1024, true);
    let pause = Arc::new(AtomicBool::new(false));
    let cancel = Arc::new(AtomicBool::new(false));

    let copied = copier
        .copy_file(&src, &dst, None, pause, cancel, |_| {})
        .expect("Direct I/O overwrite of larger file must succeed");

    assert_eq!(copied, 13_337);
    let final_len = std::fs::metadata(&dst).unwrap().len();
    assert_eq!(final_len, 13_337, "Destination MUST be truncated to source size, no old bytes or sector slack allowed");

    let dst_data = std::fs::read(&dst).unwrap();
    assert_eq!(dst_data, small_payload);
}

#[test]
fn test_conflict_policies_with_unaligned_files() {
    let dir = tempdir().unwrap();

    // Source: 7,777 bytes
    let src = dir.path().join("conflict_src.bin");
    let src_data: Vec<u8> = (0..7_777).map(|i| (i % 239) as u8).collect();
    std::fs::write(&src, &src_data).unwrap();

    // Existing: 3,333 bytes
    let existing = dir.path().join("conflict_existing.bin");
    let existing_data: Vec<u8> = (0..3_333).map(|i| (i % 197) as u8).collect();
    std::fs::write(&existing, &existing_data).unwrap();

    // 1. AutoRename
    let rename_res = ConflictResolver::resolve(&src, &existing, ConflictPolicy::AutoRename);
    match rename_res {
        ConflictResolutionAction::ProceedRenamed(renamed_path) => {
            assert_eq!(renamed_path.file_name().unwrap().to_str().unwrap(), "conflict_existing (1).bin");
        }
        _ => panic!("Expected ProceedRenamed action"),
    }

    // 2. Skip
    let skip_res = ConflictResolver::resolve(&src, &existing, ConflictPolicy::Skip);
    assert!(matches!(skip_res, ConflictResolutionAction::SkipFile));

    // 3. Overwrite
    let overwrite_res = ConflictResolver::resolve(&src, &existing, ConflictPolicy::Overwrite);
    assert!(matches!(overwrite_res, ConflictResolutionAction::ProceedOverwrite));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_move_mode_unaligned_file_with_metadata_verification() {
    use deepnode_core::engine::config::{TransferConfig, TransferMode, VerifyMode};
    use deepnode_core::engine::coordinator::TransferCoordinator;
    use deepnode_core::engine::item::FileItem;
    use std::time::{Duration, SystemTime};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    let dir = tempdir().unwrap();
    let src = dir.path().join("move_unaligned_src.dat");
    let dst = dir.path().join("move_unaligned_dst.dat");

    // Prime unaligned size: 73,193 bytes
    let data: Vec<u8> = (0usize..73_193).map(|i| (i.wrapping_mul(71).wrapping_add(19)) as u8).collect();
    std::fs::write(&src, &data).unwrap();

    // Set timestamps 5 days ago
    let five_days_ago = SystemTime::now() - Duration::from_secs(5 * 86400);
    let src_handle = deepnode_core::io::win32_file::Win32FileHandle::open_for_timestamps(&src).unwrap();
    let ft = deepnode_core::metadata::timestamps::FileTimestamps {
        creation_time: deepnode_core::metadata::timestamps::FileTimestamps::system_time_to_filetime(five_days_ago),
        last_access_time: deepnode_core::metadata::timestamps::FileTimestamps::system_time_to_filetime(five_days_ago),
        last_write_time: deepnode_core::metadata::timestamps::FileTimestamps::system_time_to_filetime(five_days_ago),
    };
    ft.apply(&src_handle).unwrap();
    drop(src_handle);

    let config = TransferConfig {
        mode: TransferMode::Move,
        verify_mode: VerifyMode::Always,
        preserve_timestamps: true,
        preserve_attributes: true,
        ..Default::default()
    };

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let pause_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::new(AtomicBool::new(false));

    let item = FileItem::new(1, src.clone(), dst.clone(), std::path::PathBuf::from("move_unaligned_dst.dat"), 73_193);

    let mut coordinator = TransferCoordinator::new(config, vec![item], event_tx, pause_flag, cancel_flag);
    let handle = tokio::spawn(async move { coordinator.run().await });

    while event_rx.recv().await.is_some() {}

    let stats = handle.await.unwrap().expect("Move must succeed");
    assert_eq!(stats.completed_files, 1);
    assert_eq!(stats.failed_files, 0);

    // Source must be deleted after move
    assert!(!src.exists(), "Source file MUST be deleted after successful Move");
    // Destination must exist with exact length and contents
    assert!(dst.exists(), "Destination file must exist");
    assert_eq!(std::fs::metadata(&dst).unwrap().len(), 73_193);
    assert_eq!(std::fs::read(&dst).unwrap(), data);

    // Timestamps must be preserved within 2s tolerance
    let dst_mod = std::fs::metadata(&dst).unwrap().modified().unwrap();
    let diff = if dst_mod > five_days_ago {
        dst_mod.duration_since(five_days_ago).unwrap()
    } else {
        five_days_ago.duration_since(dst_mod).unwrap()
    };
    assert!(diff < Duration::from_secs(2), "Modified timestamp was not preserved in Move mode!");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_ads_alternate_data_streams_multi_payload_unaligned() {
    use deepnode_core::engine::config::TransferConfig;
    use deepnode_core::engine::coordinator::TransferCoordinator;
    use deepnode_core::engine::item::FileItem;
    use deepnode_core::metadata::ads::AdsManager;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    let dir = tempdir().unwrap();
    let src = dir.path().join("ads_multi_src.txt");
    let dst = dir.path().join("ads_multi_dst.txt");

    // Main file: 2,137 bytes unaligned
    let main_data: Vec<u8> = (0..2_137).map(|i| (i % 255) as u8).collect();
    std::fs::write(&src, &main_data).unwrap();

    // Stream 1: Zone.Identifier (67 bytes)
    let zone_content = b"[ZoneTransfer]\r\nZoneId=3\r\nReferrerUrl=https://deepnode.local\r\n";
    let zone_path = format!("{}:Zone.Identifier", src.display());
    std::fs::write(&zone_path, zone_content).unwrap();

    // Stream 2: CustomMeta (37 bytes binary)
    let meta_content: Vec<u8> = (0..37).map(|i| (i * 3 + 7) as u8).collect();
    let meta_path = format!("{}:CustomMeta", src.display());
    std::fs::write(&meta_path, &meta_content).unwrap();

    // Stream 3: ExtraData (1,234 bytes unaligned)
    let extra_content: Vec<u8> = (0..1_234).map(|i| (i * 13 + 5) as u8).collect();
    let extra_path = format!("{}:ExtraData", src.display());
    std::fs::write(&extra_path, &extra_content).unwrap();

    // Verify list_streams sees all 3 streams on source
    let src_streams = AdsManager::list_streams(&src);
    assert_eq!(src_streams.len(), 3, "Source file must have 3 alternate data streams");

    let config = TransferConfig {
        preserve_ads: true,
        ..Default::default()
    };

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let pause_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::new(AtomicBool::new(false));

    let item = FileItem::new(1, src.clone(), dst.clone(), std::path::PathBuf::from("ads_multi_dst.txt"), 2_137);
    let mut coordinator = TransferCoordinator::new(config, vec![item], event_tx, pause_flag, cancel_flag);
    let handle = tokio::spawn(async move { coordinator.run().await });

    while event_rx.recv().await.is_some() {}

    let stats = handle.await.unwrap().expect("Transfer with ADS must succeed");
    assert_eq!(stats.completed_files, 1);

    // Verify main file on destination
    assert_eq!(std::fs::metadata(&dst).unwrap().len(), 2_137);
    assert_eq!(std::fs::read(&dst).unwrap(), main_data);

    // Verify destination streams
    let dst_streams = AdsManager::list_streams(&dst);
    assert_eq!(dst_streams.len(), 3, "Destination file must have all 3 alternate data streams preserved");

    let dst_zone_path = format!("{}:Zone.Identifier", dst.display());
    assert_eq!(std::fs::read(&dst_zone_path).unwrap(), zone_content);

    let dst_meta_path = format!("{}:CustomMeta", dst.display());
    assert_eq!(std::fs::read(&dst_meta_path).unwrap(), meta_content);

    let dst_extra_path = format!("{}:ExtraData", dst.display());
    assert_eq!(std::fs::read(&dst_extra_path).unwrap(), extra_content);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_vss_locked_file_clean_error_reporting_and_isolation() {
    use deepnode_core::engine::config::TransferConfig;
    use deepnode_core::engine::coordinator::TransferCoordinator;
    use deepnode_core::engine::events::EngineEvent;
    use deepnode_core::engine::item::FileItem;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_GENERIC_READ, FILE_SHARE_NONE, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL,
    };
    use windows::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows::core::PCWSTR;

    let dir = tempdir().unwrap();
    let locked_src = dir.path().join("locked_exclusive.dat");
    let normal_src = dir.path().join("normal_unlocked.dat");
    let locked_dst = dir.path().join("locked_exclusive_dst.dat");
    let normal_dst = dir.path().join("normal_unlocked_dst.dat");

    std::fs::write(&locked_src, b"EXCLUSIVE LOCKED CONTENT").unwrap();
    std::fs::write(&normal_src, b"NORMAL ACCESSIBLE CONTENT").unwrap();

    // Lock locked_src with FILE_SHARE_NONE
    let wide = deepnode_core::io::win32_file::Win32FileHandle::to_wide_path(&locked_src);
    let lock_handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_GENERIC_READ.0,
            FILE_SHARE_NONE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    };
    assert!(lock_handle.is_ok(), "Failed to acquire exclusive lock on test file");
    let raw_handle = lock_handle.unwrap();
    assert_ne!(raw_handle, INVALID_HANDLE_VALUE);

    let items = vec![
        FileItem::new(1, locked_src.clone(), locked_dst.clone(), std::path::PathBuf::from("locked_dst.dat"), 24),
        FileItem::new(2, normal_src.clone(), normal_dst.clone(), std::path::PathBuf::from("normal_dst.dat"), 25),
    ];

    let config = TransferConfig::default(); // VSS disabled by default

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let pause_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::new(AtomicBool::new(false));

    let mut coordinator = TransferCoordinator::new(config, items, event_tx, pause_flag, cancel_flag);
    let handle = tokio::spawn(async move { coordinator.run().await });

    let mut failed_event_received = false;
    let mut completed_event_received = false;

    while let Some(event) = event_rx.recv().await {
        match event {
            EngineEvent::FileFailed { item, error } => {
                if item.id == 1 {
                    failed_event_received = true;
                    assert!(
                        error.contains("sharing violation") || error.contains("32") || error.contains("locked"),
                        "Error message must specify sharing violation, got: {}", error
                    );
                }
            }
            EngineEvent::FileCompleted(item) if item.id == 2 => {
                completed_event_received = true;
            }
            _ => {}
        }
    }

    let stats = handle.await.unwrap().expect("Coordinator must complete batch without crashing");

    // Release the lock
    unsafe { let _ = CloseHandle(raw_handle); }

    assert_eq!(stats.failed_files, 1, "Locked file must be counted as failed");
    assert_eq!(stats.completed_files, 1, "Normal file must succeed despite locked sibling");
    assert!(failed_event_received, "EngineEvent::FileFailed must be emitted for locked file");
    assert!(completed_event_received, "EngineEvent::FileCompleted must be emitted for unlocked file");
    assert!(!locked_dst.exists(), "Locked file destination must not exist or be left partially written");
    assert!(normal_dst.exists(), "Normal file destination must exist");
}







