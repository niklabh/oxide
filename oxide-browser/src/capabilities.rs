use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use wasmtime::*;

// Shared state between host functions and the UI layer.
#[derive(Clone)]
pub struct HostState {
    pub console: Arc<Mutex<Vec<ConsoleEntry>>>,
    pub canvas: Arc<Mutex<CanvasState>>,
    pub storage: Arc<Mutex<HashMap<String, String>>>,
    pub timers: Arc<Mutex<Vec<TimerEntry>>>,
    pub clipboard: Arc<Mutex<String>>,
    pub memory: Option<Memory>,
}

#[derive(Clone, Debug)]
pub struct ConsoleEntry {
    pub timestamp: String,
    pub level: ConsoleLevel,
    pub message: String,
}

#[derive(Clone, Debug)]
pub enum ConsoleLevel {
    Log,
    Warn,
    Error,
}

#[derive(Clone, Debug, Default)]
pub struct CanvasState {
    pub commands: Vec<DrawCommand>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug)]
pub enum DrawCommand {
    Clear { r: u8, g: u8, b: u8, a: u8 },
    Rect { x: f32, y: f32, w: f32, h: f32, r: u8, g: u8, b: u8, a: u8 },
    Circle { cx: f32, cy: f32, radius: f32, r: u8, g: u8, b: u8, a: u8 },
    Text { x: f32, y: f32, size: f32, r: u8, g: u8, b: u8, text: String },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, r: u8, g: u8, b: u8, thickness: f32 },
}

#[derive(Clone, Debug)]
pub struct TimerEntry {
    pub id: u32,
    pub fire_at: Instant,
    pub interval: Option<Duration>,
    pub callback_id: u32,
}

impl Default for HostState {
    fn default() -> Self {
        Self {
            console: Arc::new(Mutex::new(Vec::new())),
            canvas: Arc::new(Mutex::new(CanvasState {
                commands: Vec::new(),
                width: 800,
                height: 600,
            })),
            storage: Arc::new(Mutex::new(HashMap::new())),
            timers: Arc::new(Mutex::new(Vec::new())),
            clipboard: Arc::new(Mutex::new(String::new())),
            memory: None,
        }
    }
}

fn read_guest_string(memory: &Memory, store: &impl AsContext, ptr: u32, len: u32) -> Result<String> {
    let data = memory
        .data(store)
        .get(ptr as usize..(ptr + len) as usize)
        .context("guest string out of bounds")?;
    String::from_utf8(data.to_vec()).context("guest string is not valid utf-8")
}

fn write_guest_bytes(memory: &Memory, store: &mut impl AsContextMut, ptr: u32, bytes: &[u8]) -> Result<()> {
    memory
        .data_mut(store)
        .get_mut(ptr as usize..ptr as usize + bytes.len())
        .context("guest buffer out of bounds")?
        .copy_from_slice(bytes);
    Ok(())
}

/// Register all host-provided capabilities onto the linker.
pub fn register_host_functions(linker: &mut Linker<HostState>) -> Result<()> {
    // ── Console ──────────────────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_log",
        |caller: Caller<'_, HostState>, ptr: u32, len: u32| {
            let mem = caller.data().memory.expect("memory not set");
            let msg = read_guest_string(&mem, &caller, ptr, len).unwrap_or_default();
            let entry = ConsoleEntry {
                timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
                level: ConsoleLevel::Log,
                message: msg,
            };
            caller.data().console.lock().unwrap().push(entry);
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_warn",
        |caller: Caller<'_, HostState>, ptr: u32, len: u32| {
            let mem = caller.data().memory.expect("memory not set");
            let msg = read_guest_string(&mem, &caller, ptr, len).unwrap_or_default();
            let entry = ConsoleEntry {
                timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
                level: ConsoleLevel::Warn,
                message: msg,
            };
            caller.data().console.lock().unwrap().push(entry);
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_error",
        |caller: Caller<'_, HostState>, ptr: u32, len: u32| {
            let mem = caller.data().memory.expect("memory not set");
            let msg = read_guest_string(&mem, &caller, ptr, len).unwrap_or_default();
            let entry = ConsoleEntry {
                timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
                level: ConsoleLevel::Error,
                message: msg,
            };
            caller.data().console.lock().unwrap().push(entry);
        },
    )?;

    // ── Geolocation ──────────────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_get_location",
        |mut caller: Caller<'_, HostState>, out_ptr: u32, out_cap: u32| -> u32 {
            let location = "37.7749,-122.4194"; // mock: San Francisco
            let bytes = location.as_bytes();
            let write_len = bytes.len().min(out_cap as usize);
            let mem = caller.data().memory.expect("memory not set");
            write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len]).ok();
            write_len as u32
        },
    )?;

    // ── File Picker ──────────────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_upload_file",
        |mut caller: Caller<'_, HostState>, name_ptr: u32, name_cap: u32, data_ptr: u32, data_cap: u32| -> u64 {
            let dialog = rfd::FileDialog::new()
                .set_title("Oxide: Select a file to upload")
                .pick_file();

            match dialog {
                Some(path) => {
                    let file_name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let file_data = std::fs::read(&path).unwrap_or_default();

                    let mem = caller.data().memory.expect("memory not set");

                    let name_bytes = file_name.as_bytes();
                    let name_written = name_bytes.len().min(name_cap as usize);
                    write_guest_bytes(&mem, &mut caller, name_ptr, &name_bytes[..name_written]).ok();

                    let data_written = file_data.len().min(data_cap as usize);
                    write_guest_bytes(&mem, &mut caller, data_ptr, &file_data[..data_written]).ok();

                    // pack two u32s: (name_len << 32) | data_len
                    ((name_written as u64) << 32) | (data_written as u64)
                }
                None => 0,
            }
        },
    )?;

    // ── Canvas Drawing ───────────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_canvas_clear",
        |caller: Caller<'_, HostState>, r: u32, g: u32, b: u32, a: u32| {
            let mut canvas = caller.data().canvas.lock().unwrap();
            canvas.commands.clear();
            canvas.commands.push(DrawCommand::Clear {
                r: r as u8, g: g as u8, b: b as u8, a: a as u8,
            });
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_canvas_rect",
        |caller: Caller<'_, HostState>, x: f32, y: f32, w: f32, h: f32, r: u32, g: u32, b: u32, a: u32| {
            caller.data().canvas.lock().unwrap().commands.push(DrawCommand::Rect {
                x, y, w, h,
                r: r as u8, g: g as u8, b: b as u8, a: a as u8,
            });
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_canvas_circle",
        |caller: Caller<'_, HostState>, cx: f32, cy: f32, radius: f32, r: u32, g: u32, b: u32, a: u32| {
            caller.data().canvas.lock().unwrap().commands.push(DrawCommand::Circle {
                cx, cy, radius,
                r: r as u8, g: g as u8, b: b as u8, a: a as u8,
            });
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_canvas_text",
        |caller: Caller<'_, HostState>, x: f32, y: f32, size: f32, r: u32, g: u32, b: u32, txt_ptr: u32, txt_len: u32| {
            let mem = caller.data().memory.expect("memory not set");
            let text = read_guest_string(&mem, &caller, txt_ptr, txt_len).unwrap_or_default();
            caller.data().canvas.lock().unwrap().commands.push(DrawCommand::Text {
                x, y, size,
                r: r as u8, g: g as u8, b: b as u8,
                text,
            });
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_canvas_line",
        |caller: Caller<'_, HostState>, x1: f32, y1: f32, x2: f32, y2: f32, r: u32, g: u32, b: u32, thickness: f32| {
            caller.data().canvas.lock().unwrap().commands.push(DrawCommand::Line {
                x1, y1, x2, y2,
                r: r as u8, g: g as u8, b: b as u8,
                thickness,
            });
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_canvas_dimensions",
        |caller: Caller<'_, HostState>| -> u64 {
            let canvas = caller.data().canvas.lock().unwrap();
            ((canvas.width as u64) << 32) | (canvas.height as u64)
        },
    )?;

    // ── Local Storage ────────────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_storage_set",
        |caller: Caller<'_, HostState>, key_ptr: u32, key_len: u32, val_ptr: u32, val_len: u32| {
            let mem = caller.data().memory.expect("memory not set");
            let key = read_guest_string(&mem, &caller, key_ptr, key_len).unwrap_or_default();
            let val = read_guest_string(&mem, &caller, val_ptr, val_len).unwrap_or_default();
            caller.data().storage.lock().unwrap().insert(key, val);
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_storage_get",
        |mut caller: Caller<'_, HostState>, key_ptr: u32, key_len: u32, out_ptr: u32, out_cap: u32| -> u32 {
            let mem = caller.data().memory.expect("memory not set");
            let key = read_guest_string(&mem, &caller, key_ptr, key_len).unwrap_or_default();
            let val = caller
                .data()
                .storage
                .lock()
                .unwrap()
                .get(&key)
                .cloned()
                .unwrap_or_default();
            let bytes = val.as_bytes();
            let write_len = bytes.len().min(out_cap as usize);
            write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len]).ok();
            write_len as u32
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_storage_remove",
        |caller: Caller<'_, HostState>, key_ptr: u32, key_len: u32| {
            let mem = caller.data().memory.expect("memory not set");
            let key = read_guest_string(&mem, &caller, key_ptr, key_len).unwrap_or_default();
            caller.data().storage.lock().unwrap().remove(&key);
        },
    )?;

    // ── Clipboard ────────────────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_clipboard_write",
        |caller: Caller<'_, HostState>, ptr: u32, len: u32| {
            let mem = caller.data().memory.expect("memory not set");
            let text = read_guest_string(&mem, &caller, ptr, len).unwrap_or_default();
            *caller.data().clipboard.lock().unwrap() = text.clone();
            if let Ok(mut ctx) = arboard::Clipboard::new() {
                let _ = ctx.set_text(text);
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_clipboard_read",
        |mut caller: Caller<'_, HostState>, out_ptr: u32, out_cap: u32| -> u32 {
            let text = arboard::Clipboard::new()
                .and_then(|mut ctx| ctx.get_text())
                .unwrap_or_default();
            let bytes = text.as_bytes();
            let write_len = bytes.len().min(out_cap as usize);
            let mem = caller.data().memory.expect("memory not set");
            write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len]).ok();
            write_len as u32
        },
    )?;

    // ── Timers (simplified: returns epoch millis) ────────────────────

    linker.func_wrap("oxide", "api_time_now_ms", |_caller: Caller<'_, HostState>| -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    })?;

    // ── Random ───────────────────────────────────────────────────────

    linker.func_wrap("oxide", "api_random", |_caller: Caller<'_, HostState>| -> u64 {
        let mut buf = [0u8; 8];
        getrandom(&mut buf);
        u64::from_le_bytes(buf)
    })?;

    // ── Notification (writes to console as a "notification") ─────────

    linker.func_wrap(
        "oxide",
        "api_notify",
        |caller: Caller<'_, HostState>, title_ptr: u32, title_len: u32, body_ptr: u32, body_len: u32| {
            let mem = caller.data().memory.expect("memory not set");
            let title = read_guest_string(&mem, &caller, title_ptr, title_len).unwrap_or_default();
            let body = read_guest_string(&mem, &caller, body_ptr, body_len).unwrap_or_default();
            let entry = ConsoleEntry {
                timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
                level: ConsoleLevel::Log,
                message: format!("[NOTIFICATION] {title}: {body}"),
            };
            caller.data().console.lock().unwrap().push(entry);
        },
    )?;

    Ok(())
}

fn getrandom(buf: &mut [u8]) {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    for chunk in buf.chunks_mut(8) {
        let val = RandomState::new().build_hasher().finish().to_le_bytes();
        for (dst, src) in chunk.iter_mut().zip(val.iter()) {
            *dst = *src;
        }
    }
}
