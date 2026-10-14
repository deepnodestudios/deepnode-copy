use std::path::{Path, PathBuf};
use crate::engine::item::FileItem;
use crate::error::Result;
use crate::util::{clean_path_str, read_file_list, sanitize_path};

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

            // Check if this is a file list pointer (*list.txt from TC, Opus, or TeraCopy)
            if let Some(stripped) = clean_src.strip_prefix('*') {
                let list_path = sanitize_path(stripped);
                if list_path.exists() {
                    let files_in_list = read_file_list(&list_path);
                    for item_path in files_in_list {
                        let path = sanitize_path(&item_path);
                        Self::collect_path(&path, &clean_dest, &mut items, &mut id_counter)?;
                    }
                    continue;
                }
            }

            let path = sanitize_path(&clean_src);
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
            return Ok(());
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
            let base_dst = destination.join(dir_name);

            for entry in walkdir::WalkDir::new(&clean_src).into_iter().flatten() {
                let entry_path = entry.path();
                if entry_path.is_file() {
                    if let Ok(rel) = entry_path.strip_prefix(&clean_src) {
                        let dst_file = base_dst.join(rel);
                        let size = entry_path.metadata().map(|m| m.len()).unwrap_or(0);

                        items.push(FileItem::new(
                            *id_counter,
                            entry_path.to_path_buf(),
                            dst_file,
                            rel.to_path_buf(),
                            size,
                        ));
                        *id_counter += 1;
                    }
                }
            }
        }

        Ok(())
    }
}
