//! Host capabilities and shared state for WebAssembly guests.
//!
//! This module defines [`HostState`] and the data structures the host and guest share
//! (console, canvas, timers, input, widgets, navigation, and more).
//! [`register_host_functions`] attaches the **`oxide`** Wasm import module to a Wasmtime
//! [`Linker`]: every host function that guest modules may call—`api_log`, `api_canvas_*`,
//! `api_storage_*`, `api_navigate`, audio and UI APIs, etc.—is registered there under the
//! import module name `oxide`.
//!
//! Guest code imports these symbols from `oxide`; implementations run on the host and
//! read or mutate the [`HostState`] held in the Wasmtime store attached to the linker.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use image::GenericImageView;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use wasmtime::*;

use crate::audio_format;
use crate::bookmarks::SharedBookmarkStore;
use crate::engine::ModuleLoader;
use crate::navigation::NavigationStack;
use crate::subtitle;
use crate::url as oxide_url;
use crate::video::{self, VideoPlaybackState};
use crate::video_format;

/// Per-channel audio state: a rodio Player plus metadata.
struct AudioChannel {
    player: rodio::Player,
    duration_ms: u64,
    looping: bool,
}

/// Multi-channel audio playback engine backed by [rodio](https://crates.io/crates/rodio).
///
/// Each logical channel has its own [`rodio::Player`] so guests can play overlapping
/// sounds (for example music on one channel and effects on another). The default channel
/// used by the single-channel `api_audio_*` imports is `0`.
pub struct AudioEngine {
    _device_sink: rodio::stream::MixerDeviceSink,
    channels: HashMap<u32, AudioChannel>,
}

impl AudioEngine {
    fn try_new() -> Option<Self> {
        let mut device_sink = rodio::DeviceSinkBuilder::open_default_sink().ok()?;
        device_sink.log_on_drop(false);
        Some(Self {
            _device_sink: device_sink,
            channels: HashMap::new(),
        })
    }

    fn ensure_channel(&mut self, id: u32) -> &mut AudioChannel {
        if !self.channels.contains_key(&id) {
            let player = rodio::Player::connect_new(self._device_sink.mixer());
            self.channels.insert(
                id,
                AudioChannel {
                    player,
                    duration_ms: 0,
                    looping: false,
                },
            );
        }
        self.channels.get_mut(&id).unwrap()
    }

    fn play_bytes_on(&mut self, channel_id: u32, data: Vec<u8>) -> bool {
        use rodio::Source;

        let cursor = std::io::Cursor::new(data);
        let reader = std::io::BufReader::new(cursor);
        let source = match rodio::Decoder::try_from(reader) {
            Ok(s) => s,
            Err(_) => return false,
        };

        let duration_ms = source
            .total_duration()
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let ch = self.ensure_channel(channel_id);
        ch.player.clear();
        ch.duration_ms = duration_ms;

        if ch.looping {
            ch.player.append(source.repeat_infinite());
        } else {
            ch.player.append(source);
        }
        ch.player.play();
        true
    }
}

/// All shared state between the browser host and a guest Wasm module (and dynamically loaded children).
///
/// Most fields are behind [`Arc`] and [`Mutex`] so the same state can be shared across
/// threads and nested module loads. Host code sets fields like [`HostState::memory`] and
/// [`HostState::current_url`] before or during execution; guest imports mutate the rest
/// through the registered `oxide` functions.
#[derive(Clone)]
pub struct HostState {
    /// Console log lines shown in the host UI, appended by [`console_log`] and `api_*` helpers.
    pub console: Arc<Mutex<Vec<ConsoleEntry>>>,
    /// Raster canvas: queued draw commands and decoded images for the current frame.
    pub canvas: Arc<Mutex<CanvasState>>,
    /// In-memory key/value session storage (string keys and values), similar to `localStorage` in scope.
    pub storage: Arc<Mutex<HashMap<String, String>>>,
    /// Pending one-shot and interval timers; the host drains these and invokes `on_timer` on the guest.
    pub timers: Arc<Mutex<Vec<TimerEntry>>>,
    /// Monotonic counter used to assign unique [`TimerEntry::id`] values for `api_set_timeout` / `api_set_interval`.
    pub timer_next_id: Arc<Mutex<u32>>,
    /// Last text written to or read from the clipboard via the guest API (when permitted).
    pub clipboard: Arc<Mutex<String>>,
    /// When `false`, `api_clipboard_read` / `api_clipboard_write` are blocked and log a warning.
    pub clipboard_allowed: Arc<Mutex<bool>>,
    /// Optional embedded [`sled`] database for persistent per-origin key/value bytes (`api_kv_store_*`).
    pub kv_db: Option<Arc<sled::Db>>,
    /// The guest’s exported linear memory, used to read/write pointers passed to host imports.
    pub memory: Option<Memory>,
    /// Engine and limits used by `api_load_module` to fetch and instantiate child Wasm modules.
    pub module_loader: Option<Arc<ModuleLoader>>,
    /// Session history stack for `api_push_state`, `api_replace_state`, and back/forward navigation.
    pub navigation: Arc<Mutex<NavigationStack>>,
    /// Hit-test regions registered by the guest for link clicks in the canvas area.
    pub hyperlinks: Arc<Mutex<Vec<Hyperlink>>>,
    /// Set by guest `api_navigate` — consumed by the UI after module returns.
    pub pending_navigation: Arc<Mutex<Option<String>>>,
    /// The URL of the currently loaded module (set by the host before execution).
    pub current_url: Arc<Mutex<String>>,
    /// Input state polled by the guest each frame.
    pub input_state: Arc<Mutex<InputState>>,
    /// Widget commands issued by the guest during `on_frame`.
    pub widget_commands: Arc<Mutex<Vec<WidgetCommand>>>,
    /// Persistent widget values (checkbox, slider, text input state).
    pub widget_states: Arc<Mutex<HashMap<u32, WidgetValue>>>,
    /// Button IDs that were clicked during the last render pass.
    pub widget_clicked: Arc<Mutex<HashSet<u32>>>,
    /// Top-left corner of the canvas panel in egui screen coords.
    pub canvas_offset: Arc<Mutex<(f32, f32)>>,
    /// Persistent bookmark storage shared across tabs.
    pub bookmark_store: SharedBookmarkStore,
    /// Audio playback engine (lazily initialised on first audio API call).
    pub audio: Arc<Mutex<Option<AudioEngine>>>,
    /// `Content-Type` from the last `api_audio_play_url` response (UTF-8), for codec negotiation introspection.
    pub last_audio_url_content_type: Arc<Mutex<String>>,
    /// Video playback, decode, subtitles, and HLS variant metadata (FFmpeg).
    pub video: Arc<Mutex<VideoPlaybackState>>,
    /// Last decoded video frame for picture-in-picture (RGBA, copied when PiP is enabled).
    pub video_pip_frame: Arc<Mutex<Option<DecodedImage>>>,
    /// Bumped when the PiP buffer is updated so the UI can refresh the floating texture.
    pub video_pip_serial: Arc<Mutex<u64>>,
    /// Camera, microphone, and screen capture (permission prompts + native APIs).
    pub media_capture: Arc<Mutex<crate::media_capture::MediaCaptureState>>,
}

/// A single console log line: local time, severity, and message text.
#[derive(Clone, Debug)]
pub struct ConsoleEntry {
    /// Time of day when the entry was recorded (`chrono` local format, e.g. `14:03:22.123`).
    pub timestamp: String,
    /// Severity bucket for styling in the host console.
    pub level: ConsoleLevel,
    /// UTF-8 message body.
    pub message: String,
}

/// Severity level for [`ConsoleEntry`] and [`console_log`].
#[derive(Clone, Debug)]
pub enum ConsoleLevel {
    /// Informational message (maps to `api_log`).
    Log,
    /// Warning (maps to `api_warn`).
    Warn,
    /// Error (maps to `api_error`).
    Error,
}

/// Current canvas snapshot for one frame: command list, dimensions, image atlas, and invalidation generation.
#[derive(Clone, Debug)]
pub struct CanvasState {
    /// Ordered draw operations accumulated since the last clear (or start of frame).
    pub commands: Vec<DrawCommand>,
    /// Canvas width in pixels.
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
    /// Decoded images indexed by position in this vector; [`DrawCommand::Image`] references them by `image_id`.
    pub images: Vec<DecodedImage>,
    /// Bumped when the canvas is cleared so the host can detect a full redraw.
    pub generation: u64,
}

/// An image decoded to RGBA8 pixels for compositing in the host canvas renderer.
#[derive(Clone, Debug)]
pub struct DecodedImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Raw RGBA bytes, row-major (`width * height * 4` elements when full frame).
    pub pixels: Vec<u8>,
}

/// One canvas drawing operation produced by guest `api_canvas_*` imports and consumed by the host renderer.
#[derive(Clone, Debug)]
pub enum DrawCommand {
    /// Fill the entire canvas with a solid RGBA color and reset the command list (see `api_canvas_clear`).
    Clear { r: u8, g: u8, b: u8, a: u8 },
    /// Axis-aligned filled rectangle in canvas coordinates with RGBA fill.
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
    /// Filled circle centered at `(cx, cy)` with the given radius and RGBA fill.
    Circle {
        cx: f32,
        cy: f32,
        radius: f32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    },
    /// Text baseline position `(x, y)`, font size in pixels, RGB color, and string payload.
    Text {
        x: f32,
        y: f32,
        size: f32,
        r: u8,
        g: u8,
        b: u8,
        text: String,
    },
    /// Line from `(x1, y1)` to `(x2, y2)` with RGB stroke color and stroke width in pixels.
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
    /// Draw [`DecodedImage`] `image_id` from `images` into the axis-aligned rectangle `(x, y, w, h)`.
    Image {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        image_id: usize,
    },
}

/// A scheduled timer: either a one-shot `setTimeout` or repeating `setInterval`.
#[derive(Clone, Debug)]
pub struct TimerEntry {
    /// Host-assigned id returned by `api_set_timeout` / `api_set_interval` for `api_clear_timer`.
    pub id: u32,
    /// Absolute time when this entry should fire next.
    pub fire_at: Instant,
    /// `None` for a one-shot timer; `Some(duration)` for an interval (rescheduled after each fire).
    pub interval: Option<Duration>,
    /// Guest-defined id passed to the exported `on_timer` callback when this timer fires.
    pub callback_id: u32,
}

/// Remove due timers from `timers`, collect each fired entry’s [`TimerEntry::callback_id`], and return them.
///
/// Compares each [`TimerEntry::fire_at`] against `Instant::now()`. **One-shot** entries
/// (`interval` is `None`) are removed from the vector after firing. **Interval** entries
/// are kept and their `fire_at` is advanced by `interval` so they fire again later. The
/// host typically calls the guest’s `on_timer` once per id in the returned vector.
pub fn drain_expired_timers(timers: &Arc<Mutex<Vec<TimerEntry>>>) -> Vec<u32> {
    let now = Instant::now();
    let mut guard = timers.lock().unwrap();
    let mut fired = Vec::new();
    let mut i = 0;
    while i < guard.len() {
        if guard[i].fire_at <= now {
            fired.push(guard[i].callback_id);
            if let Some(interval) = guard[i].interval {
                guard[i].fire_at = now + interval;
                i += 1;
            } else {
                guard.swap_remove(i);
            }
        } else {
            i += 1;
        }
    }
    fired
}

/// A clickable axis-aligned rectangle on the canvas that navigates to a URL when hit-tested.
///
/// Populated by `api_register_hyperlink` and cleared with `api_clear_hyperlinks`. Coordinates
/// are in the same space as canvas drawing (the host maps pointer position into this space).
#[derive(Clone, Debug)]
pub struct Hyperlink {
    /// Left edge of the hit region in canvas coordinates.
    pub x: f32,
    /// Top edge of the hit region in canvas coordinates.
    pub y: f32,
    /// Width of the hit region.
    pub w: f32,
    /// Height of the hit region.
    pub h: f32,
    /// Target URL (already resolved relative to the current page URL when registered).
    pub url: String,
}

/// Per-frame input snapshot from the host (egui) for guest polling via `api_mouse_*`, `api_key_*`, etc.
#[derive(Clone, Debug, Default)]
pub struct InputState {
    /// Pointer horizontal position in window/content coordinates before canvas offset subtraction in APIs.
    pub mouse_x: f32,
    /// Pointer vertical position in window/content coordinates before canvas offset subtraction in APIs.
    pub mouse_y: f32,
    /// Mouse buttons currently held: index 0 = primary, 1 = secondary, 2 = middle.
    pub mouse_buttons_down: [bool; 3],
    /// Mouse buttons that transitioned to pressed this frame (same indexing as `mouse_buttons_down`).
    pub mouse_buttons_clicked: [bool; 3],
    /// Key codes currently held (host-defined `u32` values, polled by `api_key_down`).
    pub keys_down: Vec<u32>,
    /// Key codes that registered a press this frame (`api_key_pressed`).
    pub keys_pressed: Vec<u32>,
    /// Shift modifier held this frame.
    pub modifiers_shift: bool,
    /// Control modifier held this frame.
    pub modifiers_ctrl: bool,
    /// Alt modifier held this frame.
    pub modifiers_alt: bool,
    /// Horizontal scroll delta for this frame.
    pub scroll_x: f32,
    /// Vertical scroll delta for this frame.
    pub scroll_y: f32,
}

/// UI control the guest requested for the current frame; the host egui layer renders these after canvas content.
///
/// Commands are queued during `on_frame`; stable `id` values tie widgets to [`WidgetValue`] state and click tracking.
#[derive(Clone, Debug)]
pub enum WidgetCommand {
    /// Clickable button with label; `api_ui_button` returns whether this `id` was clicked this pass.
    Button {
        id: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        label: String,
    },
    /// Toggle with label; checked state lives in [`WidgetValue::Bool`] for this `id`.
    Checkbox {
        id: u32,
        x: f32,
        y: f32,
        label: String,
    },
    /// Horizontal slider between `min` and `max`; value stored in [`WidgetValue::Float`].
    Slider {
        id: u32,
        x: f32,
        y: f32,
        w: f32,
        min: f32,
        max: f32,
    },
    /// Single-line text field; current text stored in [`WidgetValue::Text`].
    TextInput { id: u32, x: f32, y: f32, w: f32 },
}

/// Persistent control state for interactive widgets, keyed by widget `id` across frames.
#[derive(Clone, Debug)]
pub enum WidgetValue {
    /// Checkbox on/off.
    Bool(bool),
    /// Slider current value.
    Float(f32),
    /// Text field contents.
    Text(String),
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
            timer_next_id: Arc::new(Mutex::new(1)),
            clipboard: Arc::new(Mutex::new(String::new())),
            clipboard_allowed: Arc::new(Mutex::new(false)),
            kv_db: None,
            memory: None,
            module_loader: None,
            navigation: Arc::new(Mutex::new(NavigationStack::new())),
            hyperlinks: Arc::new(Mutex::new(Vec::new())),
            pending_navigation: Arc::new(Mutex::new(None)),
            current_url: Arc::new(Mutex::new(String::new())),
            input_state: Arc::new(Mutex::new(InputState::default())),
            widget_commands: Arc::new(Mutex::new(Vec::new())),
            widget_states: Arc::new(Mutex::new(HashMap::new())),
            widget_clicked: Arc::new(Mutex::new(HashSet::new())),
            canvas_offset: Arc::new(Mutex::new((0.0, 0.0))),
            bookmark_store: crate::bookmarks::new_shared(),
            audio: Arc::new(Mutex::new(None)),
            last_audio_url_content_type: Arc::new(Mutex::new(String::new())),
            video: Arc::new(Mutex::new(VideoPlaybackState::default())),
            video_pip_frame: Arc::new(Mutex::new(None)),
            video_pip_serial: Arc::new(Mutex::new(0)),
            media_capture: Arc::new(Mutex::new(
                crate::media_capture::MediaCaptureState::default(),
            )),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn video_render_at(
    video: &Arc<Mutex<VideoPlaybackState>>,
    pip_frame: &Arc<Mutex<Option<DecodedImage>>>,
    pip_serial: &Arc<Mutex<u64>>,
    canvas: &Arc<Mutex<CanvasState>>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> Result<(), String> {
    let t = {
        let g = video.lock().unwrap();
        g.current_position_ms()
    };
    let mut g = video.lock().unwrap();
    let player = g
        .player
        .as_mut()
        .ok_or_else(|| "no video loaded".to_string())?;
    let (pixels, pw, ph) = player.decode_frame_at(t)?;
    let pip_on = g.pip;
    let subtitle_text = subtitle::cue_text_at(&g.subtitles, t).map(|s| s.to_string());
    drop(g);

    let decoded = DecodedImage {
        width: pw,
        height: ph,
        pixels,
    };
    if pip_on {
        *pip_frame.lock().unwrap() = Some(decoded.clone());
        if let Ok(mut s) = pip_serial.lock() {
            *s = s.saturating_add(1);
        }
    }
    let mut canvas = canvas.lock().unwrap();
    let image_id = canvas.images.len();
    canvas.images.push(decoded);
    canvas.commands.push(DrawCommand::Image {
        x,
        y,
        w,
        h,
        image_id,
    });
    if let Some(text) = subtitle_text {
        let ty = (y + h - 24.0).max(y + 12.0);
        canvas.commands.push(DrawCommand::Text {
            x: x + 8.0,
            y: ty,
            size: 16.0,
            r: 255,
            g: 255,
            b: 255,
            text,
        });
    }
    Ok(())
}

fn read_guest_string(
    memory: &Memory,
    store: &impl AsContext,
    ptr: u32,
    len: u32,
) -> Result<String> {
    let start = ptr as usize;
    let end = start
        .checked_add(len as usize)
        .context("guest string pointer arithmetic overflow")?;
    let data = memory
        .data(store)
        .get(start..end)
        .context("guest string out of bounds")?;
    String::from_utf8(data.to_vec()).context("guest string is not valid utf-8")
}

fn read_guest_bytes(
    memory: &Memory,
    store: &impl AsContext,
    ptr: u32,
    len: u32,
) -> Result<Vec<u8>> {
    let start = ptr as usize;
    let end = start
        .checked_add(len as usize)
        .context("guest buffer pointer arithmetic overflow")?;
    let data = memory
        .data(store)
        .get(start..end)
        .context("guest buffer out of bounds")?;
    Ok(data.to_vec())
}

pub(crate) fn write_guest_bytes(
    memory: &Memory,
    store: &mut impl AsContextMut,
    ptr: u32,
    bytes: &[u8],
) -> Result<()> {
    let start = ptr as usize;
    let end = start
        .checked_add(bytes.len())
        .context("guest write pointer arithmetic overflow")?;
    memory
        .data_mut(store)
        .get_mut(start..end)
        .context("guest buffer out of bounds")?
        .copy_from_slice(bytes);
    Ok(())
}

/// Append a [`ConsoleEntry`] with the current local timestamp to the shared console buffer.
///
/// Used by `api_log` / `api_warn` / `api_error` and by other host helpers that surface messages to the UI.
pub fn console_log(console: &Arc<Mutex<Vec<ConsoleEntry>>>, level: ConsoleLevel, message: String) {
    console.lock().unwrap().push(ConsoleEntry {
        timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
        level,
        message,
    });
}

fn audio_try_play(
    engine: &mut AudioEngine,
    channel: u32,
    data: Vec<u8>,
    format_hint: u32,
    console: &Arc<Mutex<Vec<ConsoleEntry>>>,
) -> bool {
    let sniffed = audio_format::sniff_audio_format(&data);
    if format_hint != 0
        && format_hint != audio_format::AUDIO_FORMAT_UNKNOWN
        && sniffed != audio_format::AUDIO_FORMAT_UNKNOWN
        && sniffed != format_hint
    {
        console_log(
            console,
            ConsoleLevel::Warn,
            format!("[AUDIO] Format hint {format_hint} does not match sniffed container {sniffed}"),
        );
    }
    engine.play_bytes_on(channel, data)
}

/// Register every `oxide` import on `linker` so guest modules can link against them.
///
/// This wires dozens of functions (console, canvas, storage, clipboard, timers, HTTP,
/// dynamic module loading, crypto helpers, navigation, hyperlinks, input, audio, UI
/// widgets, etc.) under the Wasm import module name **`oxide`**. Each closure captures
/// [`Caller`] to read [`HostState`] from the store: guest pointers are resolved through
/// [`HostState::memory`], and shared handles (`Arc<Mutex<…>>`) are updated in place.
///
/// Call this once when building the linker for a main or child instance; the dynamic loader
/// path also invokes it when instantiating a child module (see the `api_load_module` import).
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
                    let (iw, ih) = img.dimensions();
                    const MAX_IMAGE_PIXELS: u32 = 4096 * 4096; // ~16M pixels
                    if iw.saturating_mul(ih) > MAX_IMAGE_PIXELS {
                        console_log(
                            &caller.data().console,
                            ConsoleLevel::Error,
                            format!(
                                "[IMAGE] Rejected: {iw}x{ih} exceeds maximum of {MAX_IMAGE_PIXELS} pixels"
                            ),
                        );
                        return;
                    }
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
            let allowed = *caller.data().clipboard_allowed.lock().unwrap();
            if !allowed {
                console_log(
                    &caller.data().console,
                    ConsoleLevel::Warn,
                    "[CLIPBOARD] Write blocked — clipboard access not permitted".into(),
                );
                return;
            }
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
            let allowed = *caller.data().clipboard_allowed.lock().unwrap();
            if !allowed {
                console_log(
                    &caller.data().console,
                    ConsoleLevel::Warn,
                    "[CLIPBOARD] Read blocked — clipboard access not permitted".into(),
                );
                return 0;
            }
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

    // ── Timers ────────────────────────────────────────────────────────
    // Timers fire via the guest-exported `on_timer(callback_id)` function,
    // which the host calls from the frame loop for each expired timer.

    linker.func_wrap(
        "oxide",
        "api_set_timeout",
        |caller: Caller<'_, HostState>, callback_id: u32, delay_ms: u32| -> u32 {
            let mut next = caller.data().timer_next_id.lock().unwrap();
            let id = *next;
            *next = next.wrapping_add(1).max(1);
            drop(next);

            let entry = TimerEntry {
                id,
                fire_at: Instant::now() + Duration::from_millis(delay_ms as u64),
                interval: None,
                callback_id,
            };
            caller.data().timers.lock().unwrap().push(entry);
            id
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_set_interval",
        |caller: Caller<'_, HostState>, callback_id: u32, interval_ms: u32| -> u32 {
            let mut next = caller.data().timer_next_id.lock().unwrap();
            let id = *next;
            *next = next.wrapping_add(1).max(1);
            drop(next);

            let interval = Duration::from_millis(interval_ms as u64);
            let entry = TimerEntry {
                id,
                fire_at: Instant::now() + interval,
                interval: Some(interval),
                callback_id,
            };
            caller.data().timers.lock().unwrap().push(entry);
            id
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_clear_timer",
        |caller: Caller<'_, HostState>, timer_id: u32| {
            caller
                .data()
                .timers
                .lock()
                .unwrap()
                .retain(|t| t.id != timer_id);
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
            let origin = caller.data().current_url.lock().unwrap().clone();
            let prefixed_key = format!("{origin}::{key}");
            match &caller.data().kv_db {
                Some(db) => match db.insert(prefixed_key.as_bytes(), val) {
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
            let origin = caller.data().current_url.lock().unwrap().clone();
            let prefixed_key = format!("{origin}::{key}");
            match &caller.data().kv_db {
                Some(db) => match db.get(prefixed_key.as_bytes()) {
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
            let origin = caller.data().current_url.lock().unwrap().clone();
            let prefixed_key = format!("{origin}::{key}");
            match &caller.data().kv_db {
                Some(db) => match db.remove(prefixed_key.as_bytes()) {
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

    // ── Input Polling ────────────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_mouse_position",
        |caller: Caller<'_, HostState>| -> u64 {
            let input = caller.data().input_state.lock().unwrap();
            let offset = caller.data().canvas_offset.lock().unwrap();
            let x = input.mouse_x - offset.0;
            let y = input.mouse_y - offset.1;
            ((x.to_bits() as u64) << 32) | (y.to_bits() as u64)
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_mouse_button_down",
        |caller: Caller<'_, HostState>, button: u32| -> u32 {
            let input = caller.data().input_state.lock().unwrap();
            if (button as usize) < 3 && input.mouse_buttons_down[button as usize] {
                1
            } else {
                0
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_mouse_button_clicked",
        |caller: Caller<'_, HostState>, button: u32| -> u32 {
            let input = caller.data().input_state.lock().unwrap();
            if (button as usize) < 3 && input.mouse_buttons_clicked[button as usize] {
                1
            } else {
                0
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_key_down",
        |caller: Caller<'_, HostState>, key: u32| -> u32 {
            let input = caller.data().input_state.lock().unwrap();
            if input.keys_down.contains(&key) {
                1
            } else {
                0
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_key_pressed",
        |caller: Caller<'_, HostState>, key: u32| -> u32 {
            let input = caller.data().input_state.lock().unwrap();
            if input.keys_pressed.contains(&key) {
                1
            } else {
                0
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_scroll_delta",
        |caller: Caller<'_, HostState>| -> u64 {
            let input = caller.data().input_state.lock().unwrap();
            ((input.scroll_x.to_bits() as u64) << 32) | (input.scroll_y.to_bits() as u64)
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_modifiers",
        |caller: Caller<'_, HostState>| -> u32 {
            let input = caller.data().input_state.lock().unwrap();
            let mut flags = 0u32;
            if input.modifiers_shift {
                flags |= 1;
            }
            if input.modifiers_ctrl {
                flags |= 2;
            }
            if input.modifiers_alt {
                flags |= 4;
            }
            flags
        },
    )?;

    // ── Audio Playback ────────────────────────────────────────────
    // All single-argument functions operate on the default channel (0).
    // Channel-specific variants allow simultaneous playback on separate
    // channels (e.g. background music on 0, SFX on 1+).

    linker.func_wrap(
        "oxide",
        "api_audio_play",
        |caller: Caller<'_, HostState>, data_ptr: u32, data_len: u32| -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let data = read_guest_bytes(&mem, &caller, data_ptr, data_len).unwrap_or_default();
            if data.is_empty() {
                return -1;
            }

            let audio = caller.data().audio.clone();
            let mut guard = audio.lock().unwrap();
            if guard.is_none() {
                *guard = AudioEngine::try_new();
            }
            match guard.as_mut() {
                Some(engine) => {
                    if engine.play_bytes_on(0, data) {
                        console_log(
                            &caller.data().console,
                            ConsoleLevel::Log,
                            "[AUDIO] Playing from bytes".into(),
                        );
                        0
                    } else {
                        console_log(
                            &caller.data().console,
                            ConsoleLevel::Error,
                            "[AUDIO] Failed to decode audio data".into(),
                        );
                        -2
                    }
                }
                None => {
                    console_log(
                        &caller.data().console,
                        ConsoleLevel::Error,
                        "[AUDIO] No audio device available".into(),
                    );
                    -3
                }
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_audio_detect_format",
        |caller: Caller<'_, HostState>, data_ptr: u32, data_len: u32| -> u32 {
            let mem = caller.data().memory.expect("memory not set");
            let data = read_guest_bytes(&mem, &caller, data_ptr, data_len).unwrap_or_default();
            audio_format::sniff_audio_format(&data)
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_audio_play_with_format",
        |caller: Caller<'_, HostState>, data_ptr: u32, data_len: u32, format_hint: u32| -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let data = read_guest_bytes(&mem, &caller, data_ptr, data_len).unwrap_or_default();
            if data.is_empty() {
                return -1;
            }

            let audio = caller.data().audio.clone();
            let mut guard = audio.lock().unwrap();
            if guard.is_none() {
                *guard = AudioEngine::try_new();
            }
            match guard.as_mut() {
                Some(engine) => {
                    if audio_try_play(engine, 0, data, format_hint, &caller.data().console) {
                        console_log(
                            &caller.data().console,
                            ConsoleLevel::Log,
                            "[AUDIO] Playing from bytes (with format hint)".into(),
                        );
                        0
                    } else {
                        console_log(
                            &caller.data().console,
                            ConsoleLevel::Error,
                            "[AUDIO] Failed to decode audio data".into(),
                        );
                        -2
                    }
                }
                None => {
                    console_log(
                        &caller.data().console,
                        ConsoleLevel::Error,
                        "[AUDIO] No audio device available".into(),
                    );
                    -3
                }
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_audio_play_url",
        |caller: Caller<'_, HostState>, url_ptr: u32, url_len: u32| -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let url = read_guest_string(&mem, &caller, url_ptr, url_len).unwrap_or_default();

            console_log(
                &caller.data().console,
                ConsoleLevel::Log,
                format!("[AUDIO] Fetching {url}"),
            );

            let (tx, rx) =
                std::sync::mpsc::sync_channel::<Result<(Vec<u8>, Option<String>), String>>(1);
            let fetch_url = url.clone();
            std::thread::spawn(move || {
                let result = (|| -> Result<(Vec<u8>, Option<String>), String> {
                    let client = reqwest::blocking::Client::builder()
                        .timeout(Duration::from_secs(30))
                        .build()
                        .map_err(|e| e.to_string())?;
                    let resp = client
                        .get(&fetch_url)
                        .header(ACCEPT, audio_format::AUDIO_HTTP_ACCEPT)
                        .send()
                        .map_err(|e| e.to_string())?;
                    if !resp.status().is_success() {
                        return Err(format!("HTTP {}", resp.status()));
                    }
                    let ct = resp
                        .headers()
                        .get(CONTENT_TYPE)
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    let bytes = resp.bytes().map(|b| b.to_vec()).map_err(|e| e.to_string())?;
                    Ok((bytes, ct))
                })();
                let _ = tx.send(result);
            });

            let (data, content_type) = match rx.recv() {
                Ok(Ok(pair)) => pair,
                Ok(Err(e)) => {
                    console_log(
                        &caller.data().console,
                        ConsoleLevel::Error,
                        format!("[AUDIO] Fetch error: {e}"),
                    );
                    return -1;
                }
                Err(_) => return -1,
            };

            *caller.data().last_audio_url_content_type.lock().unwrap() =
                content_type.clone().unwrap_or_default();

            let sniffed = audio_format::sniff_audio_format(&data);
            if let Some(ref ct) = content_type {
                if audio_format::is_likely_non_audio_document(ct)
                    && sniffed == audio_format::AUDIO_FORMAT_UNKNOWN
                {
                    console_log(
                        &caller.data().console,
                        ConsoleLevel::Error,
                        "[AUDIO] Response is not a supported audio resource (document MIME, no audio signature)"
                            .into(),
                    );
                    return -4;
                }
                let mime_fmt = audio_format::mime_to_audio_format(ct);
                if mime_fmt != audio_format::AUDIO_FORMAT_UNKNOWN
                    && sniffed != audio_format::AUDIO_FORMAT_UNKNOWN
                    && mime_fmt != sniffed
                {
                    console_log(
                        &caller.data().console,
                        ConsoleLevel::Warn,
                        format!(
                            "[AUDIO] Content-Type disagrees with sniffed container (MIME -> {mime_fmt}, sniff -> {sniffed})"
                        ),
                    );
                }
            }

            let audio = caller.data().audio.clone();
            let mut guard = audio.lock().unwrap();
            if guard.is_none() {
                *guard = AudioEngine::try_new();
            }
            match guard.as_mut() {
                Some(engine) => {
                    if engine.play_bytes_on(0, data) {
                        let ct = content_type.as_deref().unwrap_or("(none)");
                        console_log(
                            &caller.data().console,
                            ConsoleLevel::Log,
                            format!("[AUDIO] Playing from URL: {url} (Content-Type: {ct})"),
                        );
                        0
                    } else {
                        console_log(
                            &caller.data().console,
                            ConsoleLevel::Error,
                            "[AUDIO] Failed to decode fetched audio".into(),
                        );
                        -2
                    }
                }
                None => {
                    console_log(
                        &caller.data().console,
                        ConsoleLevel::Error,
                        "[AUDIO] No audio device available".into(),
                    );
                    -3
                }
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_audio_last_url_content_type",
        |mut caller: Caller<'_, HostState>, out_ptr: u32, out_cap: u32| -> u32 {
            let s = caller
                .data()
                .last_audio_url_content_type
                .lock()
                .unwrap()
                .clone();
            let bytes = s.as_bytes();
            let write_len = bytes.len().min(out_cap as usize);
            let mem = caller.data().memory.expect("memory not set");
            write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len]).ok();
            write_len as u32
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_audio_pause",
        |caller: Caller<'_, HostState>| {
            let audio = caller.data().audio.clone();
            let guard = audio.lock().unwrap();
            if let Some(engine) = guard.as_ref() {
                if let Some(ch) = engine.channels.get(&0) {
                    ch.player.pause();
                }
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_audio_resume",
        |caller: Caller<'_, HostState>| {
            let audio = caller.data().audio.clone();
            let guard = audio.lock().unwrap();
            if let Some(engine) = guard.as_ref() {
                if let Some(ch) = engine.channels.get(&0) {
                    ch.player.play();
                }
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_audio_stop",
        |caller: Caller<'_, HostState>| {
            let audio = caller.data().audio.clone();
            let guard = audio.lock().unwrap();
            if let Some(engine) = guard.as_ref() {
                if let Some(ch) = engine.channels.get(&0) {
                    ch.player.stop();
                }
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_audio_set_volume",
        |caller: Caller<'_, HostState>, level: f32| {
            let audio = caller.data().audio.clone();
            let guard = audio.lock().unwrap();
            if let Some(engine) = guard.as_ref() {
                if let Some(ch) = engine.channels.get(&0) {
                    ch.player.set_volume(level.clamp(0.0, 2.0));
                }
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_audio_get_volume",
        |caller: Caller<'_, HostState>| -> f32 {
            let audio = caller.data().audio.clone();
            let guard = audio.lock().unwrap();
            guard
                .as_ref()
                .and_then(|e| e.channels.get(&0))
                .map(|ch| ch.player.volume())
                .unwrap_or(1.0)
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_audio_is_playing",
        |caller: Caller<'_, HostState>| -> u32 {
            let audio = caller.data().audio.clone();
            let guard = audio.lock().unwrap();
            match guard.as_ref().and_then(|e| e.channels.get(&0)) {
                Some(ch) if !ch.player.is_paused() && !ch.player.empty() => 1,
                _ => 0,
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_audio_position",
        |caller: Caller<'_, HostState>| -> u64 {
            let audio = caller.data().audio.clone();
            let guard = audio.lock().unwrap();
            guard
                .as_ref()
                .and_then(|e| e.channels.get(&0))
                .map(|ch| ch.player.get_pos().as_millis() as u64)
                .unwrap_or(0)
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_audio_seek",
        |caller: Caller<'_, HostState>, position_ms: u64| -> i32 {
            let audio = caller.data().audio.clone();
            let guard = audio.lock().unwrap();
            match guard.as_ref().and_then(|e| e.channels.get(&0)) {
                Some(ch) => {
                    let pos = Duration::from_millis(position_ms);
                    match ch.player.try_seek(pos) {
                        Ok(_) => 0,
                        Err(e) => {
                            console_log(
                                &caller.data().console,
                                ConsoleLevel::Warn,
                                format!("[AUDIO] Seek failed: {e}"),
                            );
                            -1
                        }
                    }
                }
                None => -1,
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_audio_duration",
        |caller: Caller<'_, HostState>| -> u64 {
            let audio = caller.data().audio.clone();
            let guard = audio.lock().unwrap();
            guard
                .as_ref()
                .and_then(|e| e.channels.get(&0))
                .map(|ch| ch.duration_ms)
                .unwrap_or(0)
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_audio_set_loop",
        |caller: Caller<'_, HostState>, enabled: u32| {
            let audio = caller.data().audio.clone();
            let mut guard = audio.lock().unwrap();
            if guard.is_none() {
                *guard = AudioEngine::try_new();
            }
            if let Some(engine) = guard.as_mut() {
                engine.ensure_channel(0).looping = enabled != 0;
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_audio_channel_play",
        |caller: Caller<'_, HostState>, channel: u32, data_ptr: u32, data_len: u32| -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let data = read_guest_bytes(&mem, &caller, data_ptr, data_len).unwrap_or_default();
            if data.is_empty() {
                return -1;
            }

            let audio = caller.data().audio.clone();
            let mut guard = audio.lock().unwrap();
            if guard.is_none() {
                *guard = AudioEngine::try_new();
            }
            match guard.as_mut() {
                Some(engine) => {
                    if engine.play_bytes_on(channel, data) {
                        console_log(
                            &caller.data().console,
                            ConsoleLevel::Log,
                            format!("[AUDIO] Playing on channel {channel}"),
                        );
                        0
                    } else {
                        console_log(
                            &caller.data().console,
                            ConsoleLevel::Error,
                            format!("[AUDIO] Failed to decode audio for channel {channel}"),
                        );
                        -2
                    }
                }
                None => -3,
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_audio_channel_play_with_format",
        |caller: Caller<'_, HostState>,
         channel: u32,
         data_ptr: u32,
         data_len: u32,
         format_hint: u32|
         -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let data = read_guest_bytes(&mem, &caller, data_ptr, data_len).unwrap_or_default();
            if data.is_empty() {
                return -1;
            }

            let audio = caller.data().audio.clone();
            let mut guard = audio.lock().unwrap();
            if guard.is_none() {
                *guard = AudioEngine::try_new();
            }
            match guard.as_mut() {
                Some(engine) => {
                    if audio_try_play(engine, channel, data, format_hint, &caller.data().console) {
                        console_log(
                            &caller.data().console,
                            ConsoleLevel::Log,
                            format!("[AUDIO] Playing on channel {channel} (with format hint)"),
                        );
                        0
                    } else {
                        console_log(
                            &caller.data().console,
                            ConsoleLevel::Error,
                            format!("[AUDIO] Failed to decode audio for channel {channel}"),
                        );
                        -2
                    }
                }
                None => -3,
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_audio_channel_stop",
        |caller: Caller<'_, HostState>, channel: u32| {
            let audio = caller.data().audio.clone();
            let guard = audio.lock().unwrap();
            if let Some(engine) = guard.as_ref() {
                if let Some(ch) = engine.channels.get(&channel) {
                    ch.player.stop();
                }
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_audio_channel_set_volume",
        |caller: Caller<'_, HostState>, channel: u32, level: f32| {
            let audio = caller.data().audio.clone();
            let guard = audio.lock().unwrap();
            if let Some(engine) = guard.as_ref() {
                if let Some(ch) = engine.channels.get(&channel) {
                    ch.player.set_volume(level.clamp(0.0, 2.0));
                }
            }
        },
    )?;

    // ── Video (FFmpeg) ─────────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_video_detect_format",
        |caller: Caller<'_, HostState>, data_ptr: u32, data_len: u32| -> u32 {
            let mem = caller.data().memory.expect("memory not set");
            let data = read_guest_bytes(&mem, &caller, data_ptr, data_len).unwrap_or_default();
            video_format::sniff_video_format(&data)
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_video_load",
        |caller: Caller<'_, HostState>, data_ptr: u32, data_len: u32, format_hint: u32| -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let data = read_guest_bytes(&mem, &caller, data_ptr, data_len).unwrap_or_default();
            if data.is_empty() {
                return -1;
            }
            let mut guard = caller.data().video.lock().unwrap();
            match guard.open_bytes(&data, format_hint) {
                Ok(()) => {
                    console_log(
                        &caller.data().console,
                        ConsoleLevel::Log,
                        "[VIDEO] Loaded from bytes".into(),
                    );
                    0
                }
                Err(e) => {
                    console_log(
                        &caller.data().console,
                        ConsoleLevel::Error,
                        format!("[VIDEO] Load failed: {e}"),
                    );
                    -2
                }
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_video_load_url",
        |caller: Caller<'_, HostState>, url_ptr: u32, url_len: u32| -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let url = read_guest_string(&mem, &caller, url_ptr, url_len).unwrap_or_default();
            if url.is_empty() {
                return -1;
            }
            console_log(
                &caller.data().console,
                ConsoleLevel::Log,
                format!("[VIDEO] Opening {url}"),
            );

            let client = match reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(90))
                .build()
            {
                Ok(c) => c,
                Err(_) => return -3,
            };

            let mut ct = String::new();
            if let Ok(resp) = client.head(&url).send() {
                if let Some(h) = resp.headers().get(CONTENT_TYPE) {
                    if let Ok(s) = h.to_str() {
                        ct = s.to_string();
                    }
                }
            }

            let mut master_body: Option<String> = None;
            let fetch_master = url.to_ascii_lowercase().contains("m3u8")
                || ct.to_ascii_lowercase().contains("mpegurl")
                || ct.to_ascii_lowercase().contains("m3u8");
            if fetch_master {
                if let Ok(resp) = client
                    .get(&url)
                    .header(ACCEPT, video_format::VIDEO_HTTP_ACCEPT)
                    .timeout(Duration::from_secs(60))
                    .send()
                {
                    if resp.status().is_success() {
                        if let Ok(t) = resp.text() {
                            master_body = Some(t);
                        }
                    }
                }
            }

            let mut guard = caller.data().video.lock().unwrap();
            guard.stop();
            guard.last_url_content_type = ct.clone();
            guard.hls_base_url = url.clone();
            if let Some(ref body) = master_body {
                guard.hls_variants = video::parse_hls_master_variants(body);
            } else {
                guard.hls_variants.clear();
            }

            match video::VideoPlayer::open_url(&url) {
                Ok(p) => {
                    guard.player = Some(p);
                    let ctd = ct.as_str();
                    console_log(
                        &caller.data().console,
                        ConsoleLevel::Log,
                        format!("[VIDEO] Opened URL (Content-Type: {ctd})"),
                    );
                    0
                }
                Err(e) => {
                    console_log(
                        &caller.data().console,
                        ConsoleLevel::Error,
                        format!("[VIDEO] Open failed: {e}"),
                    );
                    -2
                }
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_video_last_url_content_type",
        |mut caller: Caller<'_, HostState>, out_ptr: u32, out_cap: u32| -> u32 {
            let s = caller
                .data()
                .video
                .lock()
                .unwrap()
                .last_url_content_type
                .clone();
            let bytes = s.as_bytes();
            let write_len = bytes.len().min(out_cap as usize);
            let mem = caller.data().memory.expect("memory not set");
            write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len]).ok();
            write_len as u32
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_video_hls_variant_count",
        |caller: Caller<'_, HostState>| -> u32 {
            caller.data().video.lock().unwrap().hls_variants.len() as u32
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_video_hls_variant_url",
        |mut caller: Caller<'_, HostState>, index: u32, out_ptr: u32, out_cap: u32| -> u32 {
            let resolved = {
                let g = caller.data().video.lock().unwrap();
                g.hls_variants
                    .get(index as usize)
                    .and_then(|rel| video::resolve_against_base(&g.hls_base_url, rel))
                    .or_else(|| g.hls_variants.get(index as usize).cloned())
                    .unwrap_or_default()
            };
            let bytes = resolved.as_bytes();
            let write_len = bytes.len().min(out_cap as usize);
            let mem = caller.data().memory.expect("memory not set");
            write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len]).ok();
            write_len as u32
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_video_hls_open_variant",
        |caller: Caller<'_, HostState>, index: u32| -> i32 {
            let url_opt = {
                let g = caller.data().video.lock().unwrap();
                g.hls_variants.get(index as usize).map(|rel| {
                    video::resolve_against_base(&g.hls_base_url, rel).unwrap_or_else(|| rel.clone())
                })
            };
            let Some(url) = url_opt else {
                return -1;
            };
            let mut guard = caller.data().video.lock().unwrap();
            guard.hls_base_url = url.clone();
            guard.hls_variants.clear();
            match video::VideoPlayer::open_url(&url) {
                Ok(p) => {
                    guard.player = Some(p);
                    guard.reset_playback_clock();
                    console_log(
                        &caller.data().console,
                        ConsoleLevel::Log,
                        format!("[VIDEO] Opened HLS variant {index}"),
                    );
                    0
                }
                Err(e) => {
                    console_log(
                        &caller.data().console,
                        ConsoleLevel::Error,
                        format!("[VIDEO] Variant open failed: {e}"),
                    );
                    -2
                }
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_video_play",
        |caller: Caller<'_, HostState>| {
            caller.data().video.lock().unwrap().play();
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_video_pause",
        |caller: Caller<'_, HostState>| {
            caller.data().video.lock().unwrap().pause();
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_video_stop",
        |caller: Caller<'_, HostState>| {
            caller.data().video.lock().unwrap().stop();
            *caller.data().video_pip_frame.lock().unwrap() = None;
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_video_seek",
        |caller: Caller<'_, HostState>, position_ms: u64| -> i32 {
            caller.data().video.lock().unwrap().seek(position_ms);
            0
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_video_position",
        |caller: Caller<'_, HostState>| -> u64 {
            caller.data().video.lock().unwrap().current_position_ms()
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_video_duration",
        |caller: Caller<'_, HostState>| -> u64 {
            caller.data().video.lock().unwrap().duration_ms()
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_video_render",
        |caller: Caller<'_, HostState>, x: f32, y: f32, w: f32, h: f32| -> i32 {
            match video_render_at(
                &caller.data().video,
                &caller.data().video_pip_frame,
                &caller.data().video_pip_serial,
                &caller.data().canvas,
                x,
                y,
                w,
                h,
            ) {
                Ok(()) => 0,
                Err(e) => {
                    console_log(
                        &caller.data().console,
                        ConsoleLevel::Error,
                        format!("[VIDEO] Render: {e}"),
                    );
                    -1
                }
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_video_set_volume",
        |caller: Caller<'_, HostState>, level: f32| {
            caller.data().video.lock().unwrap().volume = level.clamp(0.0, 2.0);
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_video_get_volume",
        |caller: Caller<'_, HostState>| -> f32 { caller.data().video.lock().unwrap().volume },
    )?;

    linker.func_wrap(
        "oxide",
        "api_video_set_loop",
        |caller: Caller<'_, HostState>, enabled: u32| {
            caller.data().video.lock().unwrap().looping = enabled != 0;
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_video_set_pip",
        |caller: Caller<'_, HostState>, enabled: u32| {
            caller.data().video.lock().unwrap().pip = enabled != 0;
            if enabled == 0 {
                *caller.data().video_pip_frame.lock().unwrap() = None;
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_subtitle_load_srt",
        |caller: Caller<'_, HostState>, ptr: u32, len: u32| -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let s = read_guest_string(&mem, &caller, ptr, len).unwrap_or_default();
            caller.data().video.lock().unwrap().subtitles = subtitle::parse_srt(&s);
            0
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_subtitle_load_vtt",
        |caller: Caller<'_, HostState>, ptr: u32, len: u32| -> i32 {
            let mem = caller.data().memory.expect("memory not set");
            let s = read_guest_string(&mem, &caller, ptr, len).unwrap_or_default();
            caller.data().video.lock().unwrap().subtitles = subtitle::parse_vtt(&s);
            0
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_subtitle_clear",
        |caller: Caller<'_, HostState>| {
            caller.data().video.lock().unwrap().subtitles.clear();
        },
    )?;

    // ── Interactive Widgets ─────────────────────────────────────────

    linker.func_wrap(
        "oxide",
        "api_ui_button",
        |caller: Caller<'_, HostState>,
         id: u32,
         x: f32,
         y: f32,
         w: f32,
         h: f32,
         label_ptr: u32,
         label_len: u32|
         -> u32 {
            let mem = caller.data().memory.expect("memory not set");
            let label = read_guest_string(&mem, &caller, label_ptr, label_len).unwrap_or_default();
            caller
                .data()
                .widget_commands
                .lock()
                .unwrap()
                .push(WidgetCommand::Button {
                    id,
                    x,
                    y,
                    w,
                    h,
                    label,
                });
            if caller.data().widget_clicked.lock().unwrap().contains(&id) {
                1
            } else {
                0
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_ui_checkbox",
        |caller: Caller<'_, HostState>,
         id: u32,
         x: f32,
         y: f32,
         label_ptr: u32,
         label_len: u32,
         initial: u32|
         -> u32 {
            let mem = caller.data().memory.expect("memory not set");
            let label = read_guest_string(&mem, &caller, label_ptr, label_len).unwrap_or_default();
            let mut states = caller.data().widget_states.lock().unwrap();
            let entry = states
                .entry(id)
                .or_insert_with(|| WidgetValue::Bool(initial != 0));
            let checked = match entry {
                WidgetValue::Bool(b) => *b,
                _ => initial != 0,
            };
            drop(states);
            caller
                .data()
                .widget_commands
                .lock()
                .unwrap()
                .push(WidgetCommand::Checkbox { id, x, y, label });
            if checked {
                1
            } else {
                0
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_ui_slider",
        |caller: Caller<'_, HostState>,
         id: u32,
         x: f32,
         y: f32,
         w: f32,
         min: f32,
         max: f32,
         initial: f32|
         -> f32 {
            let mut states = caller.data().widget_states.lock().unwrap();
            let entry = states
                .entry(id)
                .or_insert_with(|| WidgetValue::Float(initial));
            let value = match entry {
                WidgetValue::Float(v) => *v,
                _ => initial,
            };
            drop(states);
            caller
                .data()
                .widget_commands
                .lock()
                .unwrap()
                .push(WidgetCommand::Slider {
                    id,
                    x,
                    y,
                    w,
                    min,
                    max,
                });
            value
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_ui_text_input",
        |mut caller: Caller<'_, HostState>,
         id: u32,
         x: f32,
         y: f32,
         w: f32,
         init_ptr: u32,
         init_len: u32,
         out_ptr: u32,
         out_cap: u32|
         -> u32 {
            let mem = caller.data().memory.expect("memory not set");
            let text = {
                let mut states = caller.data().widget_states.lock().unwrap();
                let entry = states.entry(id).or_insert_with(|| {
                    let init =
                        read_guest_string(&mem, &caller, init_ptr, init_len).unwrap_or_default();
                    WidgetValue::Text(init)
                });
                match entry {
                    WidgetValue::Text(t) => t.clone(),
                    _ => String::new(),
                }
            };
            caller
                .data()
                .widget_commands
                .lock()
                .unwrap()
                .push(WidgetCommand::TextInput { id, x, y, w });
            let bytes = text.as_bytes();
            let write_len = bytes.len().min(out_cap as usize);
            write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..write_len]).ok();
            write_len as u32
        },
    )?;

    crate::media_capture::register_media_capture_functions(linker)?;

    Ok(())
}

fn getrandom(buf: &mut [u8]) {
    ::getrandom::getrandom(buf).expect("OS random number generator unavailable");
}
