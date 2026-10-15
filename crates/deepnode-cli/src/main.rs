use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use clap::{Parser, Subcommand, ValueEnum};
use tokio::sync::mpsc;
use deepnode_core::conflict::policy::ConflictPolicy;
use deepnode_core::engine::config::{TransferConfig, TransferMode, VerifyMode};
use deepnode_core::engine::coordinator::TransferCoordinator;
use deepnode_core::engine::events::EngineEvent;
use deepnode_core::engine::scanner::FileScanner;
use deepnode_core::hash::hasher::HashAlgorithm;
use deepnode_core::hash::verifier::HashVerifier;

#[derive(Parser)]
#[command(name = "deepnode-copy", version, about = "High-performance Windows file copy & verification manager")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Copy files or directories to destination
    Copy {
        /// Source paths and destination directory (last path is target destination)
        #[arg(required = true)]
        paths: Vec<String>,

        /// Bypass Windows file cache (Direct I/O)
        #[arg(long, default_value_t = false)]
        direct_io: bool,

        /// Verification mode (always, never)
        #[arg(long, value_enum, default_value_t = CliVerifyMode::Always)]
        verify: CliVerifyMode,

        /// Checksum algorithm (xxhash64, xxhash128, blake3, md5, sha1, sha256)
        #[arg(long, value_enum, default_value_t = CliHashAlgorithm::Xxhash64)]
        hash: CliHashAlgorithm,

        /// Conflict resolution policy
        #[arg(long, value_enum, default_value_t = CliConflictPolicy::AutoRename)]
        conflict: CliConflictPolicy,
    },

    /// Move files or directories to destination
    Move {
        /// Source paths and destination directory (last path is target destination)
        #[arg(required = true)]
        paths: Vec<String>,

        /// Bypass Windows file cache (Direct I/O)
        #[arg(long, default_value_t = false)]
        direct_io: bool,

        /// Verification mode
        #[arg(long, value_enum, default_value_t = CliVerifyMode::Always)]
        verify: CliVerifyMode,

        /// Checksum algorithm
        #[arg(long, value_enum, default_value_t = CliHashAlgorithm::Xxhash64)]
        hash: CliHashAlgorithm,

        /// Conflict resolution policy
        #[arg(long, value_enum, default_value_t = CliConflictPolicy::AutoRename)]
        conflict: CliConflictPolicy,
    },

    /// Verify file integrity or match against expected checksum
    Verify {
        /// Path to target file
        file: PathBuf,

        /// Expected hash string from clipboard
        #[arg(long)]
        expected: Option<String>,

        /// Hash algorithm to use
        #[arg(long, value_enum, default_value_t = CliHashAlgorithm::Xxhash64)]
        hash: CliHashAlgorithm,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum CliVerifyMode {
    Always,
    Never,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum CliHashAlgorithm {
    Xxhash64,
    Xxhash128,
    Blake3,
    Md5,
    Sha1,
    Sha256,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum CliConflictPolicy {
    Overwrite,
    Skip,
    AutoRename,
    OverwriteOlder,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Copy {
            mut paths,
            direct_io,
            verify,
            hash,
            conflict,
        } => {
            if paths.len() < 2 {
                eprintln!("Error: Please provide at least one source and a destination directory.");
                std::process::exit(2);
            }
            let destination = PathBuf::from(paths.pop().unwrap());
            run_transfer(
                TransferMode::Copy,
                paths,
                destination,
                direct_io,
                verify,
                hash,
                conflict,
            ).await
        }
        Commands::Move {
            mut paths,
            direct_io,
            verify,
            hash,
            conflict,
        } => {
            if paths.len() < 2 {
                eprintln!("Error: Please provide at least one source and a destination directory.");
                std::process::exit(2);
            }
            let destination = PathBuf::from(paths.pop().unwrap());
            run_transfer(
                TransferMode::Move,
                paths,
                destination,
                direct_io,
                verify,
                hash,
                conflict,
            ).await
        }
        Commands::Verify { file, expected, hash } => {
            run_verify(file, expected, hash)
        }
    };

    if let Err(err) = result {
        eprintln!("\nError: {}", err);
        std::process::exit(1);
    }
}

async fn run_transfer(
    mode: TransferMode,
    sources: Vec<String>,
    destination: PathBuf,
    direct_io: bool,
    verify: CliVerifyMode,
    hash: CliHashAlgorithm,
    conflict: CliConflictPolicy,
) -> anyhow::Result<()> {
    println!("DeepNode Copy v{}", env!("CARGO_PKG_VERSION"));
    println!("--------------------------------------------------");
    println!("Scanning files...");

    let items = FileScanner::scan(&sources, &destination)?;
    if items.is_empty() {
        anyhow::bail!("No valid files found to transfer.");
    }

    let total_bytes: u64 = items.iter().map(|i| i.size).sum();
    println!("Found {} files (Total: {:.2} MB)", items.len(), total_bytes as f64 / 1_048_576.0);

    let config = TransferConfig {
        mode,
        buffer_size: 2 * 1024 * 1024,
        direct_io,
        verify_mode: match verify {
            CliVerifyMode::Always => VerifyMode::Always,
            CliVerifyMode::Never => VerifyMode::Never,
        },
        hash_algorithm: match hash {
            CliHashAlgorithm::Xxhash64 => HashAlgorithm::XxHash3_64,
            CliHashAlgorithm::Xxhash128 => HashAlgorithm::XxHash3_128,
            CliHashAlgorithm::Blake3 => HashAlgorithm::Blake3,
            CliHashAlgorithm::Md5 => HashAlgorithm::Md5,
            CliHashAlgorithm::Sha1 => HashAlgorithm::Sha1,
            CliHashAlgorithm::Sha256 => HashAlgorithm::Sha256,
        },
        conflict_policy: match conflict {
            CliConflictPolicy::Overwrite => ConflictPolicy::Overwrite,
            CliConflictPolicy::Skip => ConflictPolicy::Skip,
            CliConflictPolicy::AutoRename => ConflictPolicy::AutoRename,
            CliConflictPolicy::OverwriteOlder => ConflictPolicy::OverwriteOlder,
        },
        ..Default::default()
    };

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let pause_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_flag_ctrlc = Arc::clone(&cancel_flag);
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            eprintln!("\nCanceling transfer (Ctrl+C)...");
            cancel_flag_ctrlc.store(true, Ordering::SeqCst);
        }
    });

    let mut coordinator = TransferCoordinator::new(
        config,
        items,
        event_tx,
        pause_flag,
        cancel_flag,
    );
    coordinator.set_destination_dir(destination.clone());

    // Event printing task
    let printer_handle = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                EngineEvent::FileStarted(item) => {
                    println!("[Transferring] {}", item.relative_path.display());
                }
                EngineEvent::ProgressUpdated(stats) => {
                    let speed_mb = stats.current_speed_bps as f64 / 1_048_576.0;
                    print!(
                        "\rProgress: {:.1}% | Speed: {:.1} MB/s | ETA: {}s | Files: {}/{}",
                        stats.overall_percentage,
                        speed_mb,
                        stats.eta_seconds,
                        stats.completed_files,
                        stats.total_files
                    );
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                }
                EngineEvent::FileCompleted(_) => {}
                EngineEvent::FileSkipped { item, reason } => {
                    println!("\n[Skipped] {} ({})", item.relative_path.display(), reason);
                }
                EngineEvent::FileFailed { item, error } => {
                    println!("\n[Failed] {} ({})", item.relative_path.display(), error);
                }
                EngineEvent::VerificationCompleted { item, matched } => {
                    if !matched {
                        println!("\n[VERIFY ERROR] Mismatch on {}", item.dst_path.display());
                    }
                }
                EngineEvent::TransferCompleted(stats) => {
                    println!("\n--------------------------------------------------");
                    println!(
                        "Transfer complete: {} succeeded, {} skipped, {} failed.",
                        stats.completed_files, stats.skipped_files, stats.failed_files
                    );
                    break;
                }
                EngineEvent::TransferCanceled => {
                    println!("\nTransfer canceled.");
                    break;
                }
                _ => {}
            }
        }
    });

    let run_res = coordinator.run().await;
    drop(coordinator);
    let _ = printer_handle.await;
    let stats = run_res?;

    if stats.failed_files > 0 {
        anyhow::bail!(
            "Transfer completed with {} failed file(s) ({} succeeded, {} skipped).",
            stats.failed_files,
            stats.completed_files,
            stats.skipped_files
        );
    }

    Ok(())
}

fn run_verify(
    file: PathBuf,
    expected: Option<String>,
    hash: CliHashAlgorithm,
) -> anyhow::Result<()> {
    if !file.exists() {
        anyhow::bail!("File not found: {:?}", file);
    }

    let algo = match hash {
        CliHashAlgorithm::Xxhash64 => HashAlgorithm::XxHash3_64,
        CliHashAlgorithm::Xxhash128 => HashAlgorithm::XxHash3_128,
        CliHashAlgorithm::Blake3 => HashAlgorithm::Blake3,
        CliHashAlgorithm::Md5 => HashAlgorithm::Md5,
        CliHashAlgorithm::Sha1 => HashAlgorithm::Sha1,
        CliHashAlgorithm::Sha256 => HashAlgorithm::Sha256,
    };

    println!("Calculating {:?} hash for: {:?}", algo, file);
    let calculated = HashVerifier::compute_file_hash(&file, algo, false)?;
    println!("Calculated Hash: {}", calculated);

    if let Some(exp) = expected {
        if calculated.eq_ignore_ascii_case(exp.trim()) {
            println!("RESULT: SUCCESS (Hash matches!)");
        } else {
            anyhow::bail!("RESULT: FAILED (Expected: {}, Actual: {})", exp.trim(), calculated);
        }
    }

    Ok(())
}
