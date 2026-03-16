use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use wasmtime::*;

use crate::engine::ModuleLoader;
use crate::navigation::NavigationStack;
use crate::url as oxide_url;

#[derive(Clone)]
pub struct HostState {
    pub console: Arc<Mutex<Vec<ConsoleEntry>>>,
    pub canvas: Arc<Mutex<CanvasState>>,
    pub storage: Arc<Mutex<HashMap<String, String>>>,
    pub timers: Arc<Mutex<Vec<TimerEntry>>>,
    pub clipboard: Arc<Mutex<String>>,
    pub kv_db: Option<Arc<sled::Db>>,
    pub memory: Option<Memory>,
    pub module_loader: Option<Arc<ModuleLoader>>,
    pub navigation: Arc<Mutex<NavigationStack>>,
    pub hyperlinks: Arc<Mutex<Vec<Hyperlink>>>,
    /// Set by guest `api_navigate` — consumed by the UI after module returns.
    pub pending_navigation: Arc<Mutex<Option<String>>>,
    /// The URL of the currently loaded module (set by the host before execution).
    pub current_url: Arc<Mutex<String>>,
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

#[derive(Clone, Debug)]
pub struct CanvasState {
    pub commands: Vec<DrawCommand>,
    pub width: u32,
    pub height: u32,
    pub images: Vec<DecodedImage>,
    pub generation: u64,
}

#[derive(Clone, Debug)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Debug)]
pub enum DrawCommand {
    Clear {
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    },
    Circle {
        cx: f32,
        cy: f32,
        radius: f32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    },
    Text {
        x: f32,
        y: f32,
        size: f32,
        r: u8,
        g: u8,
        b: u8,
        text: String,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        r: u8,
        g: u8,
        b: u8,
        thickness: f32,
    },
    Image {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        image_id: usize,
    },
}

#[derive(Clone, Debug)]
pub struct TimerEntry {
    pub id: u32,
    pub fire_at: Instant,
    pub interval: Option<Duration>,
    pub callback_id: u32,
}

/// A clickable rectangular region on the canvas that acts as a hyperlink.
#[derive(Clone, Debug)]
pub struct Hyperlink {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub url: String,
}

impl Default for HostState {
    fn default() -> Self {
        Self {
            console: Arc::new(Mutex::new(Vec::new())),
            canvas: Arc::new(Mutex::new(CanvasState {
                commands: Vec::new(),
                width: 800,
                height: 600,
                images: Vec::new(),
                generation: 0,
            })),
            storage: Arc::new(Mutex::new(HashMap::new())),
            timers: Arc::new(Mutex::new(Vec::new())),
            clipboard: Arc::new(Mutex::new(String::new())),
            kv_db: None,
            memory: None,
            module_loader: None,
            navigation: Arc::new(Mutex::new(NavigationStack::new())),
            hyperlinks: Arc::new(Mutex::new(Vec::new())),
            pending_navigation: Arc::new(Mutex::new(None)),
            current_url: Arc::new(Mutex::new(String::new())),
        }
    }
}

fn read_guest_string(
    memory: &Memory,
    store: &impl AsContext,
    ptr: u32,
    len: u32,
) -> Result<String> {
    let data = memory
        .data(store)
        .get(ptr as usize..(ptr + len) as usize)
        .context("guest string out of bounds")?;
    String::from_utf8(data.to_vec()).context("guest string is not valid utf-8")
}

fn read_guest_bytes(
    memory: &Memory,
    store: &impl AsContext,
    ptr: u32,
    len: u32,
) -> Result<Vec<u8>> {
    let data = memory
        .data(store)
        .get(ptr as usize..(ptr + len) as usize)
        .context("guest buffer out of bounds")?;
    Ok(data.to_vec())
}

fn write_guest_bytes(
    memory: &Memory,
    store: &mut impl AsContextMut,
    ptr: u32,
    bytes: &[u8],
) -> Result<()> {
    memory
        .data_mut(store)
        .get_mut(ptr as usize..ptr as usize + bytes.len())
        .context("guest buffer out of bounds")?
        .copy_from_slice(bytes);
    Ok(())
}

fn console_log(console: &Arc<Mutex<Vec<ConsoleEntry>>>, level: ConsoleLevel, message: String) {
    console.lock().unwrap().push(ConsoleEntry {
        timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
        level,
        message,
    });
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
            console_log(&caller.data().console, ConsoleLevel::Log, msg);
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_warn",
        |caller: Caller<'_, HostState>, ptr: u32, len: u32| {
            let mem = caller.data().memory.expect("memory not set");
            let msg = read_guest_string(&mem, &caller, ptr, len).unwrap_or_default();
            console_log(&caller.data().console, ConsoleLevel::Warn, msg);
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_error",
        |caller: Caller<'_, HostState>, ptr: u32, len: u32| {
            let mem = caller.data().memory.expect("memory not set");
            let msg = read_guest_string(&mem, &caller, ptr, len).unwrap_or_default();
            console_log(&caller.data().console, ConsoleLevel::Error, msg);
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
        |mut caller: Caller<'_, HostState>,
         name_ptr: u32,
         name_cap: u32,
         data_ptr: u32,
         data_cap: u32|
         -> u64 {
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
                    write_guest_bytes(&mem, &mut caller, name_ptr, &name_bytes[..name_written])
                        .ok();

                    let data_written = file_data.len().min(data_cap as usize);
                    write_guest_bytes(&mem, &mut caller, data_ptr, &file_data[..data_written]).ok();

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
            canvas.images.clear();
            canvas.generation += 1;
            canvas.commands.push(DrawCommand::Clear {
                r: r as u8,
                g: g as u8,
                b: b as u8,
                a: a as u8,
            });
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_canvas_rect",
        |caller: Caller<'_, HostState>,
         x: f32,
         y: f32,
         w: f32,
         h: f32,
         r: u32,
         g: u32,
         b: u32,
         a: u32| {
            caller
                .data()
                .canvas
                .lock()
                .unwrap()
                .commands
                .push(DrawCommand::Rect {
                    x,
                    y,
                    w,
                    h,
                    r: r as u8,
                    g: g as u8,
                    b: b as u8,
                    a: a as u8,
                });
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_canvas_circle",
        |caller: Caller<'_, HostState>,
         cx: f32,
         cy: f32,
         radius: f32,
         r: u32,
         g: u32,
         b: u32,
         a: u32| {
            caller
                .data()
                .canvas
                .lock()
                .unwrap()
                .commands
                .push(DrawCommand::Circle {
                    cx,
                    cy,
                    radius,
                    r: r as u8,
                    g: g as u8,
                    b: b as u8,
                    a: a as u8,
                });
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_canvas_text",
        |caller: Caller<'_, HostState>,
         x: f32,
         y: f32,
         size: f32,
         r: u32,
         g: u32,
         b: u32,
         txt_ptr: u32,
         txt_len: u32| {
            let mem = caller.data().memory.expect("memory not set");
            let text = read_guest_string(&mem, &caller, txt_ptr, txt_len).unwrap_or_default();
            caller
                .data()
                .canvas
                .lock()
                .unwrap()
                .commands
                .push(DrawCommand::Text {
                    x,
                    y,
                    size,
                    r: r as u8,
                    g: g as u8,
                    b: b as u8,
                    text,
                });
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_canvas_line",
        |caller: Caller<'_, HostState>,
         x1: f32,
         y1: f32,
         x2: f32,
         y2: f32,
         r: u32,
         g: u32,
         b: u32,
         thickness: f32| {
            caller
                .data()
                .canvas
                .lock()
                .unwrap()
                .commands
                .push(DrawCommand::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    r: r as u8,
                    g: g as u8,
                    b: b as u8,
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

    // ── Canvas Image ─────────────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_canvas_image",
        |caller: Caller<'_, HostState>,
         x: f32,
         y: f32,
         w: f32,
         h: f32,
         data_ptr: u32,
         data_len: u32| {
            let mem = caller.data().memory.expect("memory not set");
            let raw = read_guest_bytes(&mem, &caller, data_ptr, data_len).unwrap_or_default();
            match image::load_from_memory(&raw) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let (iw, ih) = (rgba.width(), rgba.height());
                    let decoded = DecodedImage {
                        width: iw,
                        height: ih,
                        pixels: rgba.into_raw(),
                    };
                    let mut canvas = caller.data().canvas.lock().unwrap();
                    let image_id = canvas.images.len();
                    canvas.images.push(decoded);
                    canvas.commands.push(DrawCommand::Image {
                        x,
                        y,
                        w,
                        h,
                        image_id,
                    });
                }
                Err(e) => {
                    console_log(
                        &caller.data().console,
                        ConsoleLevel::Error,
                        format!("[IMAGE] Failed to decode: {e}"),
                    );
                }
            }
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
        |mut caller: Caller<'_, HostState>,
         key_ptr: u32,
         key_len: u32,
         out_ptr: u32,
         out_cap: u32|
         -> u32 {
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

    linker.func_wrap(
        "oxide",
        "api_time_now_ms",
        |_caller: Caller<'_, HostState>| -> u64 {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64
        },
    )?;

    // ── Random ───────────────────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_random",
        |_caller: Caller<'_, HostState>| -> u64 {
            let mut buf = [0u8; 8];
            getrandom(&mut buf);
            u64::from_le_bytes(buf)
        },
    )?;

    // ── Notification (writes to console as a "notification") ─────────

    linker.func_wrap(
        "oxide",
        "api_notify",
        |caller: Caller<'_, HostState>,
         title_ptr: u32,
         title_len: u32,
         body_ptr: u32,
         body_len: u32| {
            let mem = caller.data().memory.expect("memory not set");
            let title = read_guest_string(&mem, &caller, title_ptr, title_len).unwrap_or_default();
            let body = read_guest_string(&mem, &caller, body_ptr, body_len).unwrap_or_default();
            console_log(
                &caller.data().console,
                ConsoleLevel::Log,
                format!("[NOTIFICATION] {title}: {body}"),
            );
        },
    )?;

    // ── HTTP Fetch ───────────────────────────────────────────────────
    // Synchronous HTTP client exposed to guest wasm. The actual network
    // call runs on a dedicated OS thread to avoid blocking the tokio
    // runtime that the browser host lives on.

    linker.func_wrap(
        "oxide",
        "api_fetch",
        |mut caller: Caller<'_, HostState>,
         method_ptr: u32,
         method_len: u32,
         url_ptr: u32,
         url_len: u32,
         ct_ptr: u32,
         ct_len: u32,
         body_ptr: u32,
         body_len: u32,
         out_ptr: u32,
         out_cap: u32|
         -> i64 {
            let mem = caller.data().memory.expect("memory not set");
            let method =
                read_guest_string(&mem, &caller, method_ptr, method_len).unwrap_or_default();
            let url = read_guest_string(&mem, &caller, url_ptr, url_len).unwrap_or_default();
            let content_type = read_guest_string(&mem, &caller, ct_ptr, ct_len).unwrap_or_default();
            let body = if body_len > 0 {
                read_guest_bytes(&mem, &caller, body_ptr, body_len).unwrap_or_default()
            } else {
                Vec::new()
            };

            console_log(
                &caller.data().console,
                ConsoleLevel::Log,
                format!("[FETCH] {method} {url}"),
            );

            let (resp_tx, resp_rx) =
                std::sync::mpsc::sync_channel::<Result<(u16, Vec<u8>), String>>(1);

            std::thread::spawn(move || {
                let result = (|| -> Result<(u16, Vec<u8>), String> {
                    let client = reqwest::blocking::Client::builder()
                        .timeout(Duration::from_secs(30))
                        .build()
                        .map_err(|e| e.to_string())?;
                    let parsed: reqwest::Method = method.parse().unwrap_or(reqwest::Method::GET);
                    let mut req = client.request(parsed, &url);
                    if !content_type.is_empty() {
                        req = req.header("Content-Type", &content_type);
                    }
                    if !body.is_empty() {
                        req = req.body(body);
                    }
                    let resp = req.send().map_err(|e| e.to_string())?;
                    let status = resp.status().as_u16();
                    let bytes = resp.bytes().map_err(|e| e.to_string())?.to_vec();
                    Ok((status, bytes))
                })();
                let _ = resp_tx.send(result);
            });

            match resp_rx.recv() {
                Ok(Ok((status, response_body))) => {
                    let write_len = response_body.len().min(out_cap as usize);
                    write_guest_bytes(&mem, &mut caller, out_ptr, &response_body[..write_len]).ok();
                    ((status as i64) << 32) | (write_len as i64)
                }
                Ok(Err(e)) => {
                    console_log(
                        &caller.data().console,
                        ConsoleLevel::Error,
                        format!("[FETCH ERROR] {e}"),
                    );
                    -1
                }
                Err(_) => -1,
            }
        },
    )?;

    // ── Dynamic Module Loading ───────────────────────────────────────
    // Allows a running wasm guest to fetch and execute another .wasm
    // module. The child module shares the same canvas, console, and
    // storage — similar to how a <script> tag loads code into the same
    // page context.

    linker.func_wrap(
        "oxide",
        "api_load_module",
        |caller: Caller<'_, HostState>, url_ptr: u32, url_len: u32| -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let url = read_guest_string(&mem, &caller, url_ptr, url_len).unwrap_or_default();
            let loader = match &caller.data().module_loader {
                Some(l) => l.clone(),
                None => return -1,
            };
            let mut child_state = caller.data().clone();
            child_state.memory = None;
            let console = caller.data().console.clone();

            console_log(
                &console,
                ConsoleLevel::Log,
                format!("[LOAD] Fetching module: {url}"),
            );

            let (tx, rx) = std::sync::mpsc::sync_channel::<Result<Vec<u8>, String>>(1);
            let fetch_url = url.clone();
            std::thread::spawn(move || {
                let result = (|| -> Result<Vec<u8>, String> {
                    let client = reqwest::blocking::Client::builder()
                        .timeout(Duration::from_secs(30))
                        .build()
                        .map_err(|e| e.to_string())?;
                    let resp = client
                        .get(&fetch_url)
                        .header("Accept", "application/wasm")
                        .send()
                        .map_err(|e| e.to_string())?;
                    if !resp.status().is_success() {
                        return Err(format!("HTTP {}", resp.status()));
                    }
                    resp.bytes().map(|b| b.to_vec()).map_err(|e| e.to_string())
                })();
                let _ = tx.send(result);
            });

            let wasm_bytes = match rx.recv() {
                Ok(Ok(bytes)) => bytes,
                Ok(Err(e)) => {
                    console_log(&console, ConsoleLevel::Error, format!("[LOAD ERROR] {e}"));
                    return -1;
                }
                Err(_) => return -1,
            };

            let module = match Module::new(&loader.engine, &wasm_bytes) {
                Ok(m) => m,
                Err(e) => {
                    console_log(
                        &console,
                        ConsoleLevel::Error,
                        format!("[LOAD ERROR] Compile: {e}"),
                    );
                    return -2;
                }
            };

            let mut store = Store::new(&loader.engine, child_state);
            if store.set_fuel(loader.fuel_limit).is_err() {
                return -3;
            }

            let mut child_linker = Linker::new(&loader.engine);
            if register_host_functions(&mut child_linker).is_err() {
                return -3;
            }

            let mem_type = MemoryType::new(1, Some(loader.max_memory_pages));
            let memory = match Memory::new(&mut store, mem_type) {
                Ok(m) => m,
                Err(_) => return -4,
            };

            if child_linker
                .define(&store, "oxide", "memory", memory)
                .is_err()
            {
                return -5;
            }
            store.data_mut().memory = Some(memory);

            let instance = match child_linker.instantiate(&mut store, &module) {
                Ok(i) => i,
                Err(e) => {
                    console_log(
                        &console,
                        ConsoleLevel::Error,
                        format!("[LOAD ERROR] Instantiate: {e}"),
                    );
                    return -6;
                }
            };

            // Use the child module's own exported memory for string I/O
            if let Some(guest_mem) = instance.get_memory(&mut store, "memory") {
                store.data_mut().memory = Some(guest_mem);
            }

            let start_fn = match instance.get_typed_func::<(), ()>(&mut store, "start_app") {
                Ok(f) => f,
                Err(_) => {
                    console_log(
                        &console,
                        ConsoleLevel::Error,
                        "[LOAD ERROR] Module missing start_app".into(),
                    );
                    return -7;
                }
            };

            match start_fn.call(&mut store, ()) {
                Ok(()) => {
                    console_log(
                        &console,
                        ConsoleLevel::Log,
                        format!("[LOAD] Module {url} executed successfully"),
                    );
                    0
                }
                Err(e) => {
                    let msg = if e.to_string().contains("fuel") {
                        "[LOAD ERROR] Child module fuel limit exceeded".to_string()
                    } else {
                        format!("[LOAD ERROR] Runtime: {e}")
                    };
                    console_log(&console, ConsoleLevel::Error, msg);
                    -8
                }
            }
        },
    )?;

    // ── SHA-256 Hashing ──────────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_hash_sha256",
        |mut caller: Caller<'_, HostState>, data_ptr: u32, data_len: u32, out_ptr: u32| -> u32 {
            use sha2::{Digest, Sha256};
            let mem = caller.data().memory.expect("memory not set");
            let data = read_guest_bytes(&mem, &caller, data_ptr, data_len).unwrap_or_default();
            let hash = Sha256::digest(&data);
            write_guest_bytes(&mem, &mut caller, out_ptr, &hash).ok();
            hash.len() as u32
        },
    )?;

    // ── Base64 Encoding / Decoding ───────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_base64_encode",
        |mut caller: Caller<'_, HostState>,
         data_ptr: u32,
         data_len: u32,
         out_ptr: u32,
         out_cap: u32|
         -> u32 {
            use base64::Engine;
            let mem = caller.data().memory.expect("memory not set");
            let data = read_guest_bytes(&mem, &caller, data_ptr, data_len).unwrap_or_default();
            let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
            let bytes = encoded.as_bytes();
            let write_len = bytes.len().min(out_cap as usize);
            write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len]).ok();
            write_len as u32
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_base64_decode",
        |mut caller: Caller<'_, HostState>,
         data_ptr: u32,
         data_len: u32,
         out_ptr: u32,
         out_cap: u32|
         -> u32 {
            use base64::Engine;
            let mem = caller.data().memory.expect("memory not set");
            let encoded = read_guest_string(&mem, &caller, data_ptr, data_len).unwrap_or_default();
            match base64::engine::general_purpose::STANDARD.decode(&encoded) {
                Ok(decoded) => {
                    let write_len = decoded.len().min(out_cap as usize);
                    write_guest_bytes(&mem, &mut caller, out_ptr, &decoded[..write_len]).ok();
                    write_len as u32
                }
                Err(_) => 0,
            }
        },
    )?;

    // ── Persistent Key-Value Store ───────────────────────────────────
    // Backed by a sled embedded database on the host's filesystem.
    // The guest has no direct access to the .db files.

    linker.func_wrap(
        "oxide",
        "api_kv_store_set",
        |caller: Caller<'_, HostState>,
         key_ptr: u32,
         key_len: u32,
         val_ptr: u32,
         val_len: u32|
         -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let key = read_guest_string(&mem, &caller, key_ptr, key_len).unwrap_or_default();
            let val = read_guest_bytes(&mem, &caller, val_ptr, val_len).unwrap_or_default();
            match &caller.data().kv_db {
                Some(db) => match db.insert(key.as_bytes(), val) {
                    Ok(_) => {
                        let _ = db.flush();
                        0
                    }
                    Err(e) => {
                        console_log(
                            &caller.data().console,
                            ConsoleLevel::Error,
                            format!("[KV] set failed: {e}"),
                        );
                        -1
                    }
                },
                None => {
                    console_log(
                        &caller.data().console,
                        ConsoleLevel::Error,
                        "[KV] store not initialised".into(),
                    );
                    -1
                }
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_kv_store_get",
        |mut caller: Caller<'_, HostState>,
         key_ptr: u32,
         key_len: u32,
         out_ptr: u32,
         out_cap: u32|
         -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let key = read_guest_string(&mem, &caller, key_ptr, key_len).unwrap_or_default();
            match &caller.data().kv_db {
                Some(db) => match db.get(key.as_bytes()) {
                    Ok(Some(val)) => {
                        let bytes = val.as_ref();
                        let write_len = bytes.len().min(out_cap as usize);
                        write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len]).ok();
                        write_len as i32
                    }
                    Ok(None) => -1,
                    Err(e) => {
                        console_log(
                            &caller.data().console,
                            ConsoleLevel::Error,
                            format!("[KV] get failed: {e}"),
                        );
                        -2
                    }
                },
                None => -2,
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_kv_store_delete",
        |caller: Caller<'_, HostState>, key_ptr: u32, key_len: u32| -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let key = read_guest_string(&mem, &caller, key_ptr, key_len).unwrap_or_default();
            match &caller.data().kv_db {
                Some(db) => match db.remove(key.as_bytes()) {
                    Ok(_) => {
                        let _ = db.flush();
                        0
                    }
                    Err(e) => {
                        console_log(
                            &caller.data().console,
                            ConsoleLevel::Error,
                            format!("[KV] delete failed: {e}"),
                        );
                        -1
                    }
                },
                None => -1,
            }
        },
    )?;

    // ── Navigation ──────────────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_navigate",
        |caller: Caller<'_, HostState>, url_ptr: u32, url_len: u32| -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let raw_url = read_guest_string(&mem, &caller, url_ptr, url_len).unwrap_or_default();

            let resolved = {
                let cur = caller.data().current_url.lock().unwrap();
                if cur.is_empty() {
                    raw_url.clone()
                } else if let Ok(base) = oxide_url::OxideUrl::parse(&cur) {
                    base.join(&raw_url)
                        .map(|u| u.as_str().to_string())
                        .unwrap_or(raw_url.clone())
                } else {
                    raw_url.clone()
                }
            };

            if oxide_url::OxideUrl::parse(&resolved).is_err() {
                console_log(
                    &caller.data().console,
                    ConsoleLevel::Error,
                    format!("[NAV] invalid URL: {resolved}"),
                );
                return -1;
            }

            console_log(
                &caller.data().console,
                ConsoleLevel::Log,
                format!("[NAV] navigate → {resolved}"),
            );
            *caller.data().pending_navigation.lock().unwrap() = Some(resolved);
            0
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_push_state",
        |caller: Caller<'_, HostState>,
         state_ptr: u32,
         state_len: u32,
         title_ptr: u32,
         title_len: u32,
         url_ptr: u32,
         url_len: u32| {
            let mem = caller.data().memory.expect("memory not set");
            let state = read_guest_bytes(&mem, &caller, state_ptr, state_len).unwrap_or_default();
            let title = read_guest_string(&mem, &caller, title_ptr, title_len).unwrap_or_default();
            let url_arg = read_guest_string(&mem, &caller, url_ptr, url_len).unwrap_or_default();

            let resolved_url = if url_arg.is_empty() {
                caller.data().current_url.lock().unwrap().clone()
            } else {
                let cur = caller.data().current_url.lock().unwrap();
                if cur.is_empty() {
                    url_arg
                } else if let Ok(base) = oxide_url::OxideUrl::parse(&cur) {
                    base.join(&url_arg)
                        .map(|u| u.as_str().to_string())
                        .unwrap_or(url_arg)
                } else {
                    url_arg
                }
            };

            let entry = crate::navigation::HistoryEntry::new(&resolved_url)
                .with_title(title)
                .with_state(state);
            caller.data().navigation.lock().unwrap().push(entry);
            *caller.data().current_url.lock().unwrap() = resolved_url;
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_replace_state",
        |caller: Caller<'_, HostState>,
         state_ptr: u32,
         state_len: u32,
         title_ptr: u32,
         title_len: u32,
         url_ptr: u32,
         url_len: u32| {
            let mem = caller.data().memory.expect("memory not set");
            let state = read_guest_bytes(&mem, &caller, state_ptr, state_len).unwrap_or_default();
            let title = read_guest_string(&mem, &caller, title_ptr, title_len).unwrap_or_default();
            let url_arg = read_guest_string(&mem, &caller, url_ptr, url_len).unwrap_or_default();

            let resolved_url = if url_arg.is_empty() {
                caller.data().current_url.lock().unwrap().clone()
            } else {
                let cur = caller.data().current_url.lock().unwrap();
                if cur.is_empty() {
                    url_arg
                } else if let Ok(base) = oxide_url::OxideUrl::parse(&cur) {
                    base.join(&url_arg)
                        .map(|u| u.as_str().to_string())
                        .unwrap_or(url_arg)
                } else {
                    url_arg
                }
            };

            let entry = crate::navigation::HistoryEntry::new(&resolved_url)
                .with_title(title)
                .with_state(state);
            caller
                .data()
                .navigation
                .lock()
                .unwrap()
                .replace_current(entry);
            *caller.data().current_url.lock().unwrap() = resolved_url;
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_get_url",
        |mut caller: Caller<'_, HostState>, out_ptr: u32, out_cap: u32| -> u32 {
            let url = caller.data().current_url.lock().unwrap().clone();
            let bytes = url.as_bytes();
            let write_len = bytes.len().min(out_cap as usize);
            let mem = caller.data().memory.expect("memory not set");
            write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len]).ok();
            write_len as u32
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_get_state",
        |mut caller: Caller<'_, HostState>, out_ptr: u32, out_cap: u32| -> i32 {
            let state_bytes = {
                let nav = caller.data().navigation.lock().unwrap();
                match nav.current() {
                    Some(entry) if !entry.state.is_empty() => Some(entry.state.clone()),
                    _ => None,
                }
            };
            match state_bytes {
                Some(bytes) => {
                    let write_len = bytes.len().min(out_cap as usize);
                    let mem = caller.data().memory.expect("memory not set");
                    write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len]).ok();
                    write_len as i32
                }
                None => -1,
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_history_length",
        |caller: Caller<'_, HostState>| -> u32 {
            caller.data().navigation.lock().unwrap().len() as u32
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_history_back",
        |caller: Caller<'_, HostState>| -> i32 {
            let mut nav = caller.data().navigation.lock().unwrap();
            match nav.go_back() {
                Some(entry) => {
                    let url = entry.url.clone();
                    *caller.data().current_url.lock().unwrap() = url.clone();
                    *caller.data().pending_navigation.lock().unwrap() = Some(url);
                    1
                }
                None => 0,
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_history_forward",
        |caller: Caller<'_, HostState>| -> i32 {
            let mut nav = caller.data().navigation.lock().unwrap();
            match nav.go_forward() {
                Some(entry) => {
                    let url = entry.url.clone();
                    *caller.data().current_url.lock().unwrap() = url.clone();
                    *caller.data().pending_navigation.lock().unwrap() = Some(url);
                    1
                }
                None => 0,
            }
        },
    )?;

    // ── Hyperlinks ──────────────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_register_hyperlink",
        |caller: Caller<'_, HostState>,
         x: f32,
         y: f32,
         w: f32,
         h: f32,
         url_ptr: u32,
         url_len: u32|
         -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let raw_url = read_guest_string(&mem, &caller, url_ptr, url_len).unwrap_or_default();

            let resolved = {
                let cur = caller.data().current_url.lock().unwrap();
                if cur.is_empty() {
                    raw_url.clone()
                } else if let Ok(base) = oxide_url::OxideUrl::parse(&cur) {
                    base.join(&raw_url)
                        .map(|u| u.as_str().to_string())
                        .unwrap_or(raw_url.clone())
                } else {
                    raw_url.clone()
                }
            };

            caller.data().hyperlinks.lock().unwrap().push(Hyperlink {
                x,
                y,
                w,
                h,
                url: resolved,
            });
            0
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_clear_hyperlinks",
        |caller: Caller<'_, HostState>| {
            caller.data().hyperlinks.lock().unwrap().clear();
        },
    )?;

    // ── URL Utilities ───────────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_url_resolve",
        |mut caller: Caller<'_, HostState>,
         base_ptr: u32,
         base_len: u32,
         rel_ptr: u32,
         rel_len: u32,
         out_ptr: u32,
         out_cap: u32|
         -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let base_str = read_guest_string(&mem, &caller, base_ptr, base_len).unwrap_or_default();
            let rel_str = read_guest_string(&mem, &caller, rel_ptr, rel_len).unwrap_or_default();

            let base = match oxide_url::OxideUrl::parse(&base_str) {
                Ok(u) => u,
                Err(_) => return -1,
            };
            let resolved = match base.join(&rel_str) {
                Ok(u) => u,
                Err(_) => return -2,
            };

            let bytes = resolved.as_str().as_bytes();
            let write_len = bytes.len().min(out_cap as usize);
            write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len]).ok();
            write_len as i32
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_url_encode",
        |mut caller: Caller<'_, HostState>,
         input_ptr: u32,
         input_len: u32,
         out_ptr: u32,
         out_cap: u32|
         -> u32 {
            let mem = caller.data().memory.expect("memory not set");
            let input = read_guest_string(&mem, &caller, input_ptr, input_len).unwrap_or_default();
            let encoded = oxide_url::percent_encode(&input);
            let bytes = encoded.as_bytes();
            let write_len = bytes.len().min(out_cap as usize);
            write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len]).ok();
            write_len as u32
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_url_decode",
        |mut caller: Caller<'_, HostState>,
         input_ptr: u32,
         input_len: u32,
         out_ptr: u32,
         out_cap: u32|
         -> u32 {
            let mem = caller.data().memory.expect("memory not set");
            let input = read_guest_string(&mem, &caller, input_ptr, input_len).unwrap_or_default();
            let decoded = oxide_url::percent_decode(&input);
            let bytes = decoded.as_bytes();
            let write_len = bytes.len().min(out_cap as usize);
            write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len]).ok();
            write_len as u32
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
