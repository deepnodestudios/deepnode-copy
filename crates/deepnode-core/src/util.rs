use std::path::{Path, PathBuf};

/// Cleans path string by stripping UTF-8/UTF-16 BOM, surrounding quotes, and leading/trailing whitespace.
pub fn clean_path_str(s: &str) -> String {
    s.trim()
        .trim_start_matches('\u{feff}') // UTF-8 BOM
        .trim_start_matches('\u{fffe}') // UTF-16 BOM
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

/// Sanitizes a path reference into a clean PathBuf without BOM or enclosing quotes.
pub fn sanitize_path<P: AsRef<Path>>(p: P) -> PathBuf {
    let s = p.as_ref().to_string_lossy();
    let cleaned = clean_path_str(&s);
    PathBuf::from(cleaned)
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
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        String::from_utf16_lossy(&u16_slice)
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        // UTF-16 BE with BOM
        let u16_slice: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
            .collect();
        String::from_utf16_lossy(&u16_slice)
    } else {
        // Default to UTF-8 lossy
        String::from_utf8_lossy(&bytes).to_string()
    };

    content
        .lines()
        .map(|line| clean_path_str(line))
        .filter(|p| !p.is_empty())
        .collect()
}
