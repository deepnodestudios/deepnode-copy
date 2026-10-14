use std::path::{Path, PathBuf};
use crate::conflict::policy::{ConflictPolicy, ConflictResolutionAction};

pub struct ConflictResolver;

impl ConflictResolver {
    /// Resolves target file conflict based on the given policy.
    pub fn resolve(
        src_path: &Path,
        dst_path: &Path,
        policy: ConflictPolicy,
    ) -> ConflictResolutionAction {
        if !dst_path.exists() {
            return ConflictResolutionAction::ProceedOverwrite;
        }

        // If source and destination point to the exact same file, never overwrite in place!
        // Always auto-rename to avoid truncating source file to 0 bytes.
        let is_same_file = if src_path == dst_path {
            true
        } else if let (Ok(s), Ok(d)) = (std::fs::canonicalize(src_path), std::fs::canonicalize(dst_path)) {
            s == d
        } else {
            false
        };

        if is_same_file {
            let unique_path = Self::generate_unique_path(dst_path);
            return ConflictResolutionAction::ProceedRenamed(unique_path);
        }

        match policy {
            ConflictPolicy::Overwrite | ConflictPolicy::OverwriteAll => {
                ConflictResolutionAction::ProceedOverwrite
            }
            ConflictPolicy::Skip | ConflictPolicy::SkipAll => {
                ConflictResolutionAction::SkipFile
            }
            ConflictPolicy::AutoRename => {
                let unique_path = Self::generate_unique_path(dst_path);
                ConflictResolutionAction::ProceedRenamed(unique_path)
            }
            ConflictPolicy::OverwriteOlder => {
                let src_meta = src_path.metadata().ok();
                let dst_meta = dst_path.metadata().ok();

                if let (Some(s), Some(d)) = (src_meta, dst_meta) {
                    let s_time = s.modified().ok();
                    let d_time = d.modified().ok();

                    if let (Some(st), Some(dt)) = (s_time, d_time) {
                        if st > dt {
                            return ConflictResolutionAction::ProceedOverwrite;
                        } else {
                            return ConflictResolutionAction::SkipFile;
                        }
                    }
                }
                ConflictResolutionAction::PromptNeeded
            }
            ConflictPolicy::ReplaceIfSizeDifferent => {
                let src_len = src_path.metadata().map(|m| m.len()).unwrap_or(0);
                let dst_len = dst_path.metadata().map(|m| m.len()).unwrap_or(0);

                if src_len != dst_len {
                    ConflictResolutionAction::ProceedOverwrite
                } else {
                    ConflictResolutionAction::SkipFile
                }
            }
            ConflictPolicy::ReplaceIfSmaller => {
                let src_len = src_path.metadata().map(|m| m.len()).unwrap_or(0);
                let dst_len = dst_path.metadata().map(|m| m.len()).unwrap_or(0);

                if dst_len < src_len {
                    ConflictResolutionAction::ProceedOverwrite
                } else {
                    ConflictResolutionAction::SkipFile
                }
            }
            ConflictPolicy::AskUser => ConflictResolutionAction::PromptNeeded,
        }
    }

    /// Generates "file (1).ext", "file (2).ext" until a non-existing filename is found.
    pub fn generate_unique_path(path: &Path) -> PathBuf {
        let parent = path.parent().unwrap_or_else(|| Path::new(""));
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
        let ext = path.extension().and_then(|e| e.to_str());

        let mut counter = 1;
        loop {
            let new_filename = match ext {
                Some(e) if !e.is_empty() => format!("{} ({}).{}", stem, counter, e),
                _ => format!("{} ({})", stem, counter),
            };
            let candidate = parent.join(new_filename);
            if !candidate.exists() {
                return candidate;
            }
            counter += 1;
        }
    }
}
