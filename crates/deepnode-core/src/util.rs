use std::path::{Path, PathBuf};

/// Cleans path string by stripping UTF-8/UTF-16 BOM and surrounding outer quotes,
/// while strictly preserving legal NTFS filename characters (such as apostrophes
/// and leading/trailing spaces inside quotes).
pub fn clean_path_str(s: &str) -> String {
    let mut trimmed = s.trim();
    // Strip UTF-8 / UTF-16 BOM if present
    trimmed = trimmed.trim_start_matches('\u{feff}').trim_start_matches('\u{fffe}');

    // If enclosed in a matching pair of quotes, strip only the outer pair
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        trimmed = &trimmed[1..trimmed.len() - 1];
    }

    trimmed.to_string()
}

/// Sanitizes a path reference into a clean PathBuf without BOM or enclosing quotes.
pub fn sanitize_path<P: AsRef<Path>>(p: P) -> PathBuf {
    let s = p.as_ref().to_string_lossy();
    let cleaned = clean_path_str(&s);
    PathBuf::from(cleaned)
}

/// Returns true if the file path represents a temporary list file created by DeepNode
/// (e.g. named deepnode_list_*.tmp).
pub fn is_deepnode_temp_list(path: &Path) -> bool {
    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
        file_name.starts_with("deepnode_list_") && file_name.ends_with(".tmp")
    } else {
        false
    }
}

/// Reads a list file supporting UTF-8 (with or without BOM), UTF-16 LE, and UTF-16 BE.
/// Returns a list of sanitized, non-empty paths.
pub fn read_file_list(path: &Path) -> Vec<String> {
    let clean_path = sanitize_path(path);
    let bytes = match std::fs::read(&clean_path) {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };

    let content = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        // UTF-8 with BOM
        String::from_utf8_lossy(&bytes[3..]).to_string()
    } else if bytes.starts_with(&[0xff, 0xfe]) {
        // UTF-16 LE with BOM
        let u16_slice: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&chunk| u16::from_le_bytes(chunk))
            .collect();
        String::from_utf16_lossy(&u16_slice)
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        // UTF-16 BE with BOM
        let u16_slice: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&chunk| u16::from_be_bytes(chunk))
            .collect();
        String::from_utf16_lossy(&u16_slice)
    } else {
        // Default to UTF-8 lossy
        String::from_utf8_lossy(&bytes).to_string()
    };

    content
        .lines()
        .map(clean_path_str)
        .filter(|p| !p.is_empty())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StarPathResolution {
    /// It's a genuine file list containing valid paths (e.g. created by Total Commander, Directory Opus, DeepNode, or a text file of paths)
    FileList(Vec<String>),
    /// It's a normal file or directory (e.g. selected in XYplorer using `*"<curitem>"` syntax) that should itself be copied/moved
    SingleSource(PathBuf),
    /// The path does not exist on disk
    NotFound(PathBuf),
}

/// Resolves an argument prefixed with '*' (commonly used by Total Commander, Directory Opus, and XYplorer).
/// Correctly distinguishes between genuine file lists (UTF-8/UTF-16 files with paths) and single selected files
/// passed via XYplorer's `*"<curitem>"` syntax (e.g. video files, audio files, documents, or text files whose contents are not paths).
pub fn resolve_star_path(path: &Path) -> StarPathResolution {
    let clean = sanitize_path(path);
    if !clean.exists() {
        return StarPathResolution::NotFound(clean);
    }

    if clean.is_dir() {
        return StarPathResolution::SingleSource(clean);
    }

    if is_deepnode_temp_list(&clean) {
        return StarPathResolution::FileList(read_file_list(&clean));
    }

    // Check file extension: known binary, media, archive, document extensions are never list files
    if let Some(ext) = clean.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_lowercase();
        match ext_lower.as_str() {
            "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "3gp" |
            "mp3" | "flac" | "wav" | "aac" | "ogg" | "m4a" | "wma" |
            "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "iso" | "img" | "vhd" | "vhdx" |
            "exe" | "dll" | "sys" | "msi" | "msix" | "appx" | "bin" |
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" |
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "ico" | "webp" | "tiff" | "psd" |
            "db" | "sqlite" | "sqlite3" | "mdb" | "accdb" => {
                return StarPathResolution::SingleSource(clean);
            }
            _ => {}
        }
    }

    // Check if file contains null bytes (binary file without extension or unrecognized extension)
    if let Ok(bytes) = std::fs::read(&clean) {
        if bytes.is_empty() {
            return StarPathResolution::SingleSource(clean);
        }
        let is_utf16 = bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]);
        if !is_utf16 && bytes.iter().take(4096).any(|&b| b == 0) {
            return StarPathResolution::SingleSource(clean);
        }
    }

    // Read candidate lines
    let lines = read_file_list(&clean);
    if lines.is_empty() {
        return StarPathResolution::SingleSource(clean);
    }

    // Check if the lines in the file correspond to real, existing paths on disk
    let mut existing_count = 0;
    let check_limit = lines.len().min(10);
    for line in &lines[..check_limit] {
        let candidate = sanitize_path(line);
        if candidate.exists() {
            existing_count += 1;
        }
    }

    if existing_count > 0 {
        // At least one line exists on disk -> It's a valid path list!
        StarPathResolution::FileList(lines)
    } else {
        // None of the lines exist on disk -> It's a regular file selected in XYplorer (e.g. notes.txt)!
        StarPathResolution::SingleSource(clean)
    }
}

/// Checks if a path string contains reserved Windows device names (CON, PRN, AUX, NUL, COM1..9, LPT1..9)
/// or dangerous device namespaces (\\.\, \\?\GLOBALROOT).
pub fn is_reserved_or_device_path(path_str: &str) -> bool {
    let clean = clean_path_str(path_str);
    if clean.is_empty() {
        return false;
    }

    let lower = clean.to_lowercase();
    if lower.starts_with(r"\\.\") || lower.starts_with(r"\\?\globalroot") {
        return true;
    }

    let path = Path::new(&clean);
    for component in path.components() {
        if let std::path::Component::Normal(os_str) = component {
            let name = os_str.to_string_lossy().to_ascii_uppercase();
            let stem = name.split('.').next().unwrap_or("");
            match stem {
                "CON" | "PRN" | "AUX" | "NUL" |
                "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9" |
                "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9" => {
                    return true;
                }
                _ => {}
            }
        }
    }
    false
}

/// Validates that a path string is safe for filesystem operations:
/// - Not empty
/// - No null bytes or ASCII control characters (< 32)
/// - Not a reserved Windows device or raw physical device namespace
pub fn validate_filesystem_path(path_str: &str) -> std::result::Result<PathBuf, String> {
    let clean = clean_path_str(path_str);
    if clean.is_empty() {
        return Err("Dosya veya klasör yolu boş olamaz".to_string());
    }

    if clean.chars().any(|c| (c as u32) < 32) {
        return Err("Yol geçersiz kontrol karakterleri içeriyor".to_string());
    }

    if is_reserved_or_device_path(&clean) {
        return Err(format!("Aygıt veya rezerve edilmiş yol kullanılamaz: {}", clean));
    }

    Ok(sanitize_path(&clean))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_reserved_or_device_path() {
        assert!(is_reserved_or_device_path("CON"));
        assert!(is_reserved_or_device_path("con"));
        assert!(is_reserved_or_device_path("C:\\folder\\CON.txt"));
        assert!(is_reserved_or_device_path("NUL"));
        assert!(is_reserved_or_device_path("COM1"));
        assert!(is_reserved_or_device_path("LPT2"));
        assert!(is_reserved_or_device_path(r"\\.\PhysicalDrive0"));
        assert!(is_reserved_or_device_path(r"\\?\GLOBALROOT\Device"));

        assert!(!is_reserved_or_device_path(r"C:\Users\User\Documents\file.txt"));
        assert!(!is_reserved_or_device_path(r"D:\Data\constant.json"));
        assert!(!is_reserved_or_device_path(r"\\server\share\folder\item.pdf"));
    }

    #[test]
    fn test_validate_filesystem_path() {
        assert!(validate_filesystem_path("").is_err());
        assert!(validate_filesystem_path("   ").is_err());
        assert!(validate_filesystem_path("C:\\test\0file.txt").is_err());
        assert!(validate_filesystem_path("C:\\test\nfile.txt").is_err());
        assert!(validate_filesystem_path("CON").is_err());
        assert!(validate_filesystem_path(r"\\.\PhysicalDrive0").is_err());

        let valid = validate_filesystem_path(r"C:\Users\Test\File.txt").expect("should be valid");
        assert_eq!(valid, PathBuf::from(r"C:\Users\Test\File.txt"));

        let valid_unc = validate_filesystem_path(r"\\server\share\file.txt").expect("should be valid");
        assert_eq!(valid_unc, PathBuf::from(r"\\server\share\file.txt"));
    }

    #[test]
    fn test_clean_path_str_preserves_spaces_and_quotes() {
        // Enclosed in quotes with leading/trailing spaces inside: should preserve internal spaces
        assert_eq!(clean_path_str(r#""C:\folder\ file with spaces.txt ""#), r"C:\folder\ file with spaces.txt ");
        assert_eq!(clean_path_str(r#"  "D:\data\ leading.txt"  "#), r"D:\data\ leading.txt");
        // Filenames with apostrophes: must NOT strip apostrophes
        assert_eq!(clean_path_str(r"C:\docs\O'Connor.docx"), r"C:\docs\O'Connor.docx");
        assert_eq!(clean_path_str(r#"C:\music\'Greatest Hits'.mp3"#), r#"C:\music\'Greatest Hits'.mp3"#);
        // Strips UTF-8 and UTF-16 BOM
        assert_eq!(clean_path_str("\u{feff}C:\\file.txt"), r"C:\file.txt");
        assert_eq!(clean_path_str("\u{fffe}C:\\file.txt"), r"C:\file.txt");
    }
}
