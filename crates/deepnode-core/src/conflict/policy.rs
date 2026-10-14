use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    /// Overwrite the target file
    Overwrite,
    /// Overwrite all conflicting files unconditionally
    OverwriteAll,
    /// Skip copying this file
    Skip,
    /// Skip all conflicting files unconditionally
    SkipAll,
    /// Keep both by auto-renaming the destination (e.g., "file (1).txt")
    AutoRename,
    /// Overwrite only if source file is newer (LastWriteTime is more recent)
    OverwriteOlder,
    /// Overwrite if file sizes differ
    ReplaceIfSizeDifferent,
    /// Overwrite only if destination file is smaller than source
    ReplaceIfSmaller,
    /// Ask user via UI modal
    AskUser,
}

impl Default for ConflictPolicy {
    fn default() -> Self {
        ConflictPolicy::AskUser
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictResolutionAction {
    ProceedOverwrite,
    ProceedRenamed(std::path::PathBuf),
    SkipFile,
    PromptNeeded,
}
