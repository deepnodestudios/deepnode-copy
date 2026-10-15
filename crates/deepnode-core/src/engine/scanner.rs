use std::path::{Path, PathBuf};
use crate::engine::item::FileItem;
use crate::error::Result;
use crate::util::{
    clean_path_str, is_deepnode_temp_list, resolve_star_path, sanitize_path, StarPathResolution,
};

pub struct FileScanner;

impl FileScanner {
    /// Scans source items (which can be files, directories, or a "*listfile.txt" file)
    /// and constructs the list of FileItems relative to destination directory.
    pub fn scan(sources: &[String], destination: &Path) -> Result<Vec<FileItem>> {
        let clean_dest = sanitize_path(destination);
        let mut items = Vec::new();
        let mut id_counter: u64 = 1;

        for src_str in sources {
            let clean_src = clean_path_str(src_str);
            if clean_src.is_empty() {
                continue;
            }

            // Check if this is a file list pointer or an XYplorer *"<curitem>" source
            if let Some(stripped) = clean_src.strip_prefix('*') {
                let list_path = sanitize_path(stripped);
                match resolve_star_path(&list_path) {
                    StarPathResolution::FileList(files_in_list) => {
                        for item_path in files_in_list {
                            let path = sanitize_path(&item_path);
                            if !path.exists() {
                                return Err(crate::error::TransferError::Io(format!(
                                    "Source file in list not found: {:?}", path
                                )));
                            }
                            Self::collect_path(&path, &clean_dest, &mut items, &mut id_counter)?;
                        }
                        if is_deepnode_temp_list(&list_path) {
                            let _ = std::fs::remove_file(&list_path);
                        }
                        continue;
                    }
                    StarPathResolution::SingleSource(path) => {
                        Self::collect_path(&path, &clean_dest, &mut items, &mut id_counter)?;
                        continue;
                    }
                    StarPathResolution::NotFound(path) => {
                        return Err(crate::error::TransferError::Io(format!(
                            "Source path or file list not found: {:?}", path
                        )));
                    }
                }
            }

            let path = sanitize_path(&clean_src);
            if !path.exists() {
                return Err(crate::error::TransferError::Io(format!(
                    "Source path does not exist: {:?}", path
                )));
            }
            Self::collect_path(&path, &clean_dest, &mut items, &mut id_counter)?;
        }

        Ok(items)
    }

    fn collect_path(
        src: &Path,
        destination: &Path,
        items: &mut Vec<FileItem>,
        id_counter: &mut u64,
    ) -> Result<()> {
        let clean_src = sanitize_path(src);
        if !clean_src.exists() {
            return Err(crate::error::TransferError::Io(format!(
                "Source path does not exist: {:?}", clean_src
            )));
        }

        if clean_src.is_file() {
            let filename = clean_src.file_name().unwrap_or_default();
            let dst_file = destination.join(filename);
            let size = clean_src.metadata().map(|m| m.len()).unwrap_or(0);

            items.push(FileItem::new(
                *id_counter,
                clean_src.clone(),
                dst_file,
                PathBuf::from(filename),
                size,
            ));
            *id_counter += 1;
        } else if clean_src.is_dir() {
            let dir_name = clean_src.file_name().unwrap_or_default();
            let mut found_any_file = false;

            for entry in walkdir::WalkDir::new(&clean_src).into_iter().flatten() {
                let entry_path = entry.path();
                if entry_path.is_file() {
                    found_any_file = true;
                    if let Ok(rel) = entry_path.strip_prefix(&clean_src) {
                        let rel_with_dir = std::path::Path::new(dir_name).join(rel);
                        let dst_file = destination.join(&rel_with_dir);
                        let size = entry_path.metadata().map(|m| m.len()).unwrap_or(0);

                        items.push(FileItem::new(
                            *id_counter,
                            entry_path.to_path_buf(),
                            dst_file,
                            rel_with_dir,
                            size,
                        ));
                        *id_counter += 1;
                    }
                } else if entry_path.is_dir() && entry_path != clean_src {
                    let is_empty = std::fs::read_dir(entry_path).map(|mut d| d.next().is_none()).unwrap_or(false);
                    if is_empty {
                        if let Ok(rel) = entry_path.strip_prefix(&clean_src) {
                            let rel_with_dir = std::path::Path::new(dir_name).join(rel);
                            let dst_dir = destination.join(&rel_with_dir);
                            let _ = std::fs::create_dir_all(&dst_dir);
                        }
                    }
                }
            }

            if !found_any_file {
                let dst_dir = destination.join(dir_name);
                let _ = std::fs::create_dir_all(&dst_dir);
            }
        }

        Ok(())
    }
}
