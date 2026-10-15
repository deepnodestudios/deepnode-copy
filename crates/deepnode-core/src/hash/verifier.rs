use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use crate::error::{Result, TransferError};
use crate::hash::hasher::{HashAlgorithm, MultiHasher};
use crate::io::buffer::AlignedBuffer;
use crate::io::win32_file::Win32FileHandle;

pub struct HashVerifier;

impl HashVerifier {
    /// Computes hash of a file using unbuffered direct I/O where possible to bypass OS file cache.
    pub fn compute_file_hash(path: &Path, algo: HashAlgorithm, bypass_cache: bool) -> Result<String> {
        let file = Win32FileHandle::open_read(path, bypass_cache, false)?;
        let file_size = file.get_size()?;
        let mut hasher = MultiHasher::new(algo);
        let mut buffer = AlignedBuffer::new(1024 * 1024, 4096);
        let mut remaining = file_size;

        loop {
            if remaining == 0 {
                break;
            }
            let read_bytes = file.read_sync(buffer.as_mut_slice())?;
            if read_bytes == 0 {
                break;
            }
            let valid_bytes = std::cmp::min(read_bytes as u64, remaining) as usize;
            hasher.update(&buffer.as_mut_slice()[..valid_bytes]);
            remaining = remaining.saturating_sub(valid_bytes as u64);
        }

        Ok(hasher.finalize_hex())
    }

    /// Verifies source and destination files match.
    pub fn verify_files(src_path: &Path, dst_path: &Path, algo: HashAlgorithm) -> Result<bool> {
        // Read target with cache bypass to ensure data is physically written on disk platter/NAND
        let src_hash = Self::compute_file_hash(src_path, algo, false)?;
        let dst_hash = Self::compute_file_hash(dst_path, algo, true)?;

        if src_hash.eq_ignore_ascii_case(&dst_hash) {
            Ok(true)
        } else {
            Err(TransferError::HashMismatch {
                path: dst_path.to_string_lossy().to_string(),
                expected: src_hash,
                actual: dst_hash,
            })
        }
    }

    /// Validates a file against an expected clipboard hash string.
    pub fn verify_against_expected(path: &Path, expected_hash: &str) -> Result<Option<HashAlgorithm>> {
        let clean_expected = expected_hash.trim().to_lowercase();
        let algo = match clean_expected.len() {
            16 => HashAlgorithm::XxHash3_64,
            32 => {
                // Could be MD5 or XxHash3_128, default to MD5 if unsure or test both
                HashAlgorithm::Md5
            }
            40 => HashAlgorithm::Sha1,
            64 => {
                // Could be SHA-256 or BLAKE3
                HashAlgorithm::Sha256
            }
            _ => return Err(TransferError::Io("Unsupported hash length".to_string())),
        };

        let calculated = Self::compute_file_hash(path, algo, false)?;
        if calculated.eq_ignore_ascii_case(&clean_expected) {
            return Ok(Some(algo));
        }

        // Fallback for 32/64 length ambiguity
        if clean_expected.len() == 32 {
            let xxh128 = Self::compute_file_hash(path, HashAlgorithm::XxHash3_128, false)?;
            if xxh128.eq_ignore_ascii_case(&clean_expected) {
                return Ok(Some(HashAlgorithm::XxHash3_128));
            }
        } else if clean_expected.len() == 64 {
            let blake = Self::compute_file_hash(path, HashAlgorithm::Blake3, false)?;
            if blake.eq_ignore_ascii_case(&clean_expected) {
                return Ok(Some(HashAlgorithm::Blake3));
            }
        }

        Err(TransferError::HashMismatch {
            path: path.to_string_lossy().to_string(),
            expected: clean_expected,
            actual: calculated,
        })
    }

    /// Writes checksum to NTFS Alternate Data Stream (ADS): `<filename>:DeepNodeHash`
    pub fn write_hash_to_ads(path: &Path, _algo: HashAlgorithm, hash_val: &str) -> std::io::Result<()> {
        let ads_path_str = format!("{}:DeepNodeHash", path.to_string_lossy());
        let mut f = File::create(Path::new(&ads_path_str))?;
        writeln!(f, "{}", hash_val.trim())?;
        Ok(())
    }

    /// Reads checksum from NTFS Alternate Data Stream (ADS): `<filename>:DeepNodeHash`.
    /// Handles both raw checksums and legacy `Algorithm:hash` formats, returning the clean hash string.
    pub fn read_hash_from_ads(path: &Path) -> std::io::Result<Option<String>> {
        let ads_path_str = format!("{}:DeepNodeHash", path.to_string_lossy());
        let p = Path::new(&ads_path_str);
        if !p.exists() {
            return Ok(None);
        }
        let f = File::open(p)?;
        let mut reader = BufReader::new(f);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        if let Some((_, hash_part)) = trimmed.split_once(':') {
            Ok(Some(hash_part.trim().to_string()))
        } else {
            Ok(Some(trimmed.to_string()))
        }
    }

    /// Exports a `.hash`, `.md5`, or `.sha256` checksum manifest file.
    /// Preserves relative paths for subdirectories to prevent file name collisions.
    pub fn export_checksum_manifest(
        output_file: &Path,
        records: &[(&Path, &str)],
    ) -> std::io::Result<()> {
        let base_dir = output_file.parent().unwrap_or(Path::new("."));
        let mut f = File::create(output_file)?;
        for (file_path, hash) in records {
            let rel_str = if let Ok(rel) = file_path.strip_prefix(base_dir) {
                rel.to_string_lossy().replace('\\', "/")
            } else if let Some(fname) = file_path.file_name() {
                fname.to_string_lossy().to_string()
            } else {
                file_path.to_string_lossy().replace('\\', "/")
            };
            writeln!(f, "{} *{}", hash, rel_str)?;
        }
        Ok(())
    }
}
