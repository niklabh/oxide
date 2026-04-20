//! Host-side native file and folder picker for Oxide guest modules.
//!
//! Guests call `api_file_pick` / `api_folder_pick` to invoke the OS picker
//! and receive opaque `u32` handles. Paths never cross the sandbox boundary;
//! the host keeps a `HashMap<handle, PathBuf>` and exposes reads via
//! `api_file_read`, `api_file_read_range`, and `api_file_metadata`.
//!
//! `api_folder_entries` lists a picked directory as JSON, pre-allocating
//! sub-handles for each child so the guest can read files without ever
//! seeing the underlying path.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use anyhow::Result;
use wasmtime::{Caller, Linker};

use crate::capabilities::{
    console_log, read_guest_string, write_guest_bytes, ConsoleLevel, HostState,
};

/// One picked file or folder, keyed by an opaque handle the guest holds.
pub struct PickedEntry {
    pub path: PathBuf,
    pub is_dir: bool,
}

/// All picker state for a tab. Handles are never reused within a session.
pub struct FilePickerState {
    entries: HashMap<u32, PickedEntry>,
    next_id: u32,
}

impl Default for FilePickerState {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            next_id: 1,
        }
    }
}

impl FilePickerState {
    fn alloc(&mut self, path: PathBuf, is_dir: bool) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.entries.insert(id, PickedEntry { path, is_dir });
        id
    }

    fn get(&self, handle: u32) -> Option<&PickedEntry> {
        self.entries.get(&handle)
    }
}

fn mime_for_extension(ext: &str) -> &'static str {
    match ext.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "mov" => "video/quicktime",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" => "text/javascript",
        "json" => "application/json",
        "xml" => "application/xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn modified_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn file_name_of(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn json_escape(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Register all file picker host functions.
pub fn register_file_picker_functions(linker: &mut Linker<HostState>) -> Result<()> {
    // api_file_pick(title, title_len, filters, filters_len, multiple, out_ptr, out_cap) -> i32
    //   filters: comma-separated extensions ("png,jpg,gif"); empty string = all files.
    //   out buffer receives u32 handles (little-endian) up to `out_cap / 4`.
    //   Returns count of handles written, or -1 if the user cancelled.
    linker.func_wrap(
        "oxide",
        "api_file_pick",
        |mut caller: Caller<'_, HostState>,
         title_ptr: u32,
         title_len: u32,
         filters_ptr: u32,
         filters_len: u32,
         multiple: u32,
         out_ptr: u32,
         out_cap: u32|
         -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let title = read_guest_string(&mem, &caller, title_ptr, title_len)
                .unwrap_or_else(|_| "Oxide: Select a file".to_string());
            let filters =
                read_guest_string(&mem, &caller, filters_ptr, filters_len).unwrap_or_default();

            let mut dialog = rfd::FileDialog::new().set_title(&title);
            let exts: Vec<String> = filters
                .split(',')
                .map(|s| s.trim().trim_start_matches('.').to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if !exts.is_empty() {
                let refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
                dialog = dialog.add_filter("Files", &refs);
            }

            let paths: Vec<PathBuf> = if multiple != 0 {
                dialog.pick_files().unwrap_or_default()
            } else {
                match dialog.pick_file() {
                    Some(p) => vec![p],
                    None => Vec::new(),
                }
            };

            if paths.is_empty() {
                return -1;
            }

            let picker = caller.data().file_picker.clone();
            let mut state = picker.lock().unwrap();
            let max = (out_cap / 4) as usize;
            let mut handles: Vec<u8> = Vec::with_capacity(paths.len().min(max) * 4);
            for path in paths.iter().take(max) {
                let id = state.alloc(path.clone(), false);
                handles.extend_from_slice(&id.to_le_bytes());
            }
            drop(state);

            let count = (handles.len() / 4) as i32;
            if write_guest_bytes(&mem, &mut caller, out_ptr, &handles).is_err() {
                return -1;
            }
            count
        },
    )?;

    // api_folder_pick(title_ptr, title_len) -> u32
    //   Returns a folder handle, or 0 on cancel.
    linker.func_wrap(
        "oxide",
        "api_folder_pick",
        |caller: Caller<'_, HostState>, title_ptr: u32, title_len: u32| -> u32 {
            let mem = caller.data().memory.expect("memory not set");
            let title = read_guest_string(&mem, &caller, title_ptr, title_len)
                .unwrap_or_else(|_| "Oxide: Select a folder".to_string());

            let path = match rfd::FileDialog::new().set_title(&title).pick_folder() {
                Some(p) => p,
                None => return 0,
            };
            let picker = caller.data().file_picker.clone();
            let id = picker.lock().unwrap().alloc(path, true);
            id
        },
    )?;

    // api_folder_entries(handle, out_ptr, out_cap) -> i32
    //   Writes JSON array of entries:
    //     [{"name":"a.txt","size":123,"is_dir":false,"handle":42}, ...]
    //   Sub-handles are allocated on the fly so guests can read children
    //   without learning any host path. Returns bytes written, -1 on bad
    //   handle, -2 on io error, or negative of required size if truncated.
    linker.func_wrap(
        "oxide",
        "api_folder_entries",
        |mut caller: Caller<'_, HostState>, handle: u32, out_ptr: u32, out_cap: u32| -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let picker = caller.data().file_picker.clone();
            let dir_path = {
                let state = picker.lock().unwrap();
                match state.get(handle) {
                    Some(e) if e.is_dir => e.path.clone(),
                    _ => return -1,
                }
            };

            let read_dir = match std::fs::read_dir(&dir_path) {
                Ok(it) => it,
                Err(_) => return -2,
            };

            let mut children: Vec<(PathBuf, bool, u64)> = Vec::new();
            for entry in read_dir.flatten() {
                let path = entry.path();
                let meta = match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                children.push((path, meta.is_dir(), meta.len()));
            }

            let mut json = String::from("[");
            let mut state = picker.lock().unwrap();
            for (i, (path, is_dir, size)) in children.iter().enumerate() {
                if i > 0 {
                    json.push(',');
                }
                let id = state.alloc(path.clone(), *is_dir);
                json.push_str("{\"name\":");
                json_escape(&file_name_of(path), &mut json);
                json.push_str(&format!(
                    ",\"size\":{size},\"is_dir\":{is_dir},\"handle\":{id}}}",
                    size = size,
                    is_dir = is_dir,
                    id = id,
                ));
            }
            drop(state);
            json.push(']');

            let bytes = json.as_bytes();
            if bytes.len() > out_cap as usize {
                return -(bytes.len() as i32);
            }
            if write_guest_bytes(&mem, &mut caller, out_ptr, bytes).is_err() {
                return -2;
            }
            bytes.len() as i32
        },
    )?;

    // api_file_read(handle, out_ptr, out_cap) -> i64
    //   Reads the full file. Returns bytes written, -1 invalid handle,
    //   -2 io error, or -(required size) if the buffer is too small.
    linker.func_wrap(
        "oxide",
        "api_file_read",
        |mut caller: Caller<'_, HostState>, handle: u32, out_ptr: u32, out_cap: u32| -> i64 {
            let mem = caller.data().memory.expect("memory not set");
            let picker = caller.data().file_picker.clone();
            let path = {
                let state = picker.lock().unwrap();
                match state.get(handle) {
                    Some(e) if !e.is_dir => e.path.clone(),
                    _ => return -1,
                }
            };
            let data = match std::fs::read(&path) {
                Ok(d) => d,
                Err(_) => return -2,
            };
            if data.len() > out_cap as usize {
                return -(data.len() as i64);
            }
            if write_guest_bytes(&mem, &mut caller, out_ptr, &data).is_err() {
                return -2;
            }
            data.len() as i64
        },
    )?;

    // api_file_read_range(handle, offset_lo, offset_hi, len, out_ptr, out_cap) -> i64
    //   Reads [offset .. offset+len) from the file. Returns bytes written,
    //   -1 invalid handle, -2 io error. Short reads are returned verbatim
    //   (EOF reached before `len`).
    linker.func_wrap(
        "oxide",
        "api_file_read_range",
        |mut caller: Caller<'_, HostState>,
         handle: u32,
         offset_lo: u32,
         offset_hi: u32,
         len: u32,
         out_ptr: u32,
         out_cap: u32|
         -> i64 {
            use std::io::{Read, Seek, SeekFrom};
            let mem = caller.data().memory.expect("memory not set");
            let picker = caller.data().file_picker.clone();
            let path = {
                let state = picker.lock().unwrap();
                match state.get(handle) {
                    Some(e) if !e.is_dir => e.path.clone(),
                    _ => return -1,
                }
            };
            let want = (len as usize).min(out_cap as usize);
            let offset = ((offset_hi as u64) << 32) | (offset_lo as u64);
            let mut file = match std::fs::File::open(&path) {
                Ok(f) => f,
                Err(_) => return -2,
            };
            if file.seek(SeekFrom::Start(offset)).is_err() {
                return -2;
            }
            let mut buf = vec![0u8; want];
            let n = match file.read(&mut buf) {
                Ok(n) => n,
                Err(_) => return -2,
            };
            buf.truncate(n);
            if write_guest_bytes(&mem, &mut caller, out_ptr, &buf).is_err() {
                return -2;
            }
            n as i64
        },
    )?;

    // api_file_metadata(handle, out_ptr, out_cap) -> i32
    //   Writes JSON: {"name":"a.txt","size":123,"mime":"image/png",
    //                 "modified_ms":1712000000000,"is_dir":false}
    //   Returns bytes written, -1 invalid handle, -2 io error, or
    //   -(required size) if the buffer is too small.
    linker.func_wrap(
        "oxide",
        "api_file_metadata",
        |mut caller: Caller<'_, HostState>, handle: u32, out_ptr: u32, out_cap: u32| -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let picker = caller.data().file_picker.clone();
            let (path, is_dir) = {
                let state = picker.lock().unwrap();
                match state.get(handle) {
                    Some(e) => (e.path.clone(), e.is_dir),
                    None => return -1,
                }
            };
            let meta = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => return -2,
            };
            let name = file_name_of(&path);
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();
            let mime = if is_dir {
                "inode/directory"
            } else {
                mime_for_extension(&ext)
            };
            let mut json = String::new();
            json.push_str("{\"name\":");
            json_escape(&name, &mut json);
            json.push_str(",\"size\":");
            json.push_str(&meta.len().to_string());
            json.push_str(",\"mime\":");
            json_escape(mime, &mut json);
            json.push_str(",\"modified_ms\":");
            json.push_str(&modified_ms(&meta).to_string());
            json.push_str(",\"is_dir\":");
            json.push_str(if is_dir { "true" } else { "false" });
            json.push('}');

            let bytes = json.as_bytes();
            if bytes.len() > out_cap as usize {
                return -(bytes.len() as i32);
            }
            if write_guest_bytes(&mem, &mut caller, out_ptr, bytes).is_err() {
                console_log(
                    &caller.data().console,
                    ConsoleLevel::Error,
                    "[file_picker] failed to write metadata".to_string(),
                );
                return -2;
            }
            bytes.len() as i32
        },
    )?;

    Ok(())
}
