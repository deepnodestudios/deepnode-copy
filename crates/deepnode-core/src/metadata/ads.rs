use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    FindFirstStreamW, FindNextStreamW, FindStreamInfoStandard,
    WIN32_FIND_STREAM_DATA,
};
use crate::error::Result;
use crate::io::win32_file::Win32FileHandle;

pub struct StreamInfo {
    pub name: String,
    pub size: i64,
}

pub struct AdsManager;

impl AdsManager {
    /// Enumerates all alternate data streams on an NTFS file.
    pub fn list_streams(path: &Path) -> Vec<StreamInfo> {
        let wide = Win32FileHandle::to_wide_path(path);
        let mut streams = Vec::new();

        unsafe {
            let mut find_data = WIN32_FIND_STREAM_DATA::default();
            let handle = match FindFirstStreamW(
                PCWSTR(wide.as_ptr()),
                FindStreamInfoStandard,
                &mut find_data as *mut _ as *mut _,
                0,
            ) {
                Ok(h) if h != INVALID_HANDLE_VALUE => h,
                _ => return streams,
            };

            loop {
                let stream_name_len = find_data.cStreamName
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(find_data.cStreamName.len());

                let stream_name = String::from_utf16_lossy(&find_data.cStreamName[..stream_name_len]);

                // Skip the primary unnamed data stream (::$DATA)
                if stream_name != "::$DATA" && !stream_name.is_empty() {
                    streams.push(StreamInfo {
                        name: stream_name,
                        size: find_data.StreamSize,
                    });
                }

                if FindNextStreamW(handle, &mut find_data as *mut _ as *mut _).is_err() {
                    break;
                }
            }

            let _ = CloseHandle(handle);
        }

        streams
    }

    /// Copies all alternate data streams from source file to destination file.
    pub fn copy_alternate_streams(src: &Path, dst: &Path) -> Result<()> {
        let streams = Self::list_streams(src);
        for stream in streams {
            // Stream names typically come formatted as ":StreamName:$DATA"
            // To access stream: "C:\path\file.txt:StreamName"
            let stream_clean = stream.name.trim_end_matches(":$DATA");
            let src_stream_path = format!("{}{}", src.to_string_lossy(), stream_clean);
            let dst_stream_path = format!("{}{}", dst.to_string_lossy(), stream_clean);

            if let (Ok(mut reader), Ok(mut writer)) = (
                std::fs::File::open(Path::new(&src_stream_path)),
                std::fs::File::create(Path::new(&dst_stream_path)),
            ) {
                let _ = std::io::copy(&mut reader, &mut writer);
            }
        }
        Ok(())
    }
}
