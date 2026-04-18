//! # Oxide — The Binary-First WebAssembly Browser
//!
//! <https://docs.oxide.foundation>
//!
//! Oxide is a **binary-first browser** that fetches and executes `.wasm`
//! (WebAssembly) modules instead of HTML/JavaScript. Guest applications run
//! in a secure sandbox with zero access to the host filesystem, environment
//! variables, or raw network sockets. The browser exposes a rich set of
//! **capability APIs** that guest modules import to interact with the host.
//!
//! The desktop shell is built on [GPUI](https://www.gpui.rs/) (Zed's
//! GPU-accelerated UI framework). Guest draw commands map directly onto GPUI
//! primitives — filled quads, GPU-shaped text, vector paths, and image
//! textures — giving your canvas output full hardware acceleration.
//!
//! ## Crate Map
//!
//! | Crate | Purpose | Audience |
//! |-------|---------|----------|
//! | [`oxide_sdk`] | Guest SDK — safe Rust wrappers for the `"oxide"` host imports | App developers |
//! | [`oxide_browser`] | Host runtime — Wasmtime engine, sandbox, GPUI shell | Browser contributors |
//!
//! ---
//!
//! # Quick Start — Building a Guest App
//!
//! ```toml
//! # Cargo.toml
//! [package]
//! name = "my-oxide-app"
//! version = "0.1.0"
//! edition = "2021"
//!
//! [lib]
//! crate-type = ["cdylib"]
//!
//! [dependencies]
//! oxide-sdk = "0.6"
//! ```
//!
//! ### Static app (one-shot render)
//!
//! ```rust,ignore
//! use oxide_sdk::*;
//!
//! #[no_mangle]
//! pub extern "C" fn start_app() {
//!     log("Hello from Oxide!");
//!     canvas_clear(30, 30, 46, 255);
//!     canvas_text(20.0, 40.0, 28.0, 255, 255, 255, 255, "Welcome to Oxide");
//! }
//! ```
//!
//! ### Interactive app (frame loop with widgets)
//!
//! ```rust,ignore
//! use oxide_sdk::*;
//!
//! #[no_mangle]
//! pub extern "C" fn start_app() { log("Ready"); }
//!
//! #[no_mangle]
//! pub extern "C" fn on_frame(_dt_ms: u32) {
//!     canvas_clear(30, 30, 46, 255);
//!     let (mx, my) = mouse_position();
//!     canvas_circle(mx, my, 20.0, 255, 100, 100, 255);
//!     if ui_button(1, 20.0, 20.0, 100.0, 30.0, "Click me!") {
//!         log("Clicked!");
//!     }
//! }
//! ```
//!
//! ### High-level drawing API
//!
//! ```rust,ignore
//! use oxide_sdk::draw::*;
//!
//! #[no_mangle]
//! pub extern "C" fn start_app() {
//!     let c = Canvas::new();
//!     c.clear(Color::hex(0x1e1e2e));
//!     c.fill_rect(Rect::new(10.0, 10.0, 200.0, 100.0), Color::rgb(80, 120, 200));
//!     c.fill_circle(Point2D::new(300.0, 200.0), 50.0, Color::RED);
//!     c.text("Hello!", Point2D::new(20.0, 30.0), 24.0, Color::WHITE);
//! }
//! ```
//!
//! Build and run:
//!
//! ```bash
//! cargo build --target wasm32-unknown-unknown --release
//! # Open Oxide browser → navigate to your .wasm file
//! ```
//!
//! ---
//!
//! # Architecture Overview
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │                    Oxide Browser                    │
//! │                                                     │
//! │  ┌──────────┐  ┌────────────┐  ┌──────────────────┐ │
//! │  │  URL Bar │  │   Canvas   │  │     Console      │ │
//! │  └────┬─────┘  └──────┬─────┘  └──────┬───────────┘ │
//! │       │               │               │             │
//! │  ┌────▼───────────────▼───────────────▼──────────┐  │
//! │  │              Host Runtime (oxide-browser)      │  │
//! │  │  Wasmtime engine · sandbox policy             │  │
//! │  │  fuel: 500M instructions · memory: 256 MB     │  │
//! │  └────────────────────┬──────────────────────────┘  │
//! │                       │                             │
//! │  ┌────────────────────▼──────────────────────────┐  │
//! │  │          Capability Provider                  │  │
//! │  │  "oxide" wasm import module                   │  │
//! │  │  canvas · gpu · audio · video · capture       │  │
//! │  │  fetch · streaming · websocket · webrtc · midi │  │
//! │  │  timers · animation frames · navigation       │  │
//! │  │  console · storage · clipboard · widgets      │  │
//! │  │  input · hyperlinks · crypto · protobuf       │  │
//! │  └────────────────────┬──────────────────────────┘  │
//! │                       │                             │
//! │  ┌────────────────────▼──────────────────────────┐  │
//! │  │           Guest .wasm Module (oxide-sdk)       │  │
//! │  │  exports: start_app(), on_frame(dt_ms)        │  │
//! │  │  imports: oxide::*                            │  │
//! │  └───────────────────────────────────────────────┘  │
//! └─────────────────────────────────────────────────────┘
//! ```
//!
//! ---
//!
//! # Guest SDK API Reference
//!
//! The [`oxide_sdk`] crate provides the full guest-side API. All functions
//! are available via `use oxide_sdk::*;`.
//!
//! ## High-Level Drawing API (`oxide_sdk::draw`)
//!
//! The [`oxide_sdk::draw`] module provides GPUI-inspired ergonomic types
//! that wrap the low-level canvas functions with less boilerplate:
//!
//! | Type | Description |
//! |------|-------------|
//! | [`oxide_sdk::draw::Color`] | sRGB + alpha with named constants and `hex()` constructor |
//! | [`oxide_sdk::draw::Point2D`] | 2D point in canvas coordinates |
//! | [`oxide_sdk::draw::Rect`] | Axis-aligned rectangle with hit-testing |
//! | [`oxide_sdk::draw::Canvas`] | Zero-cost drawing facade |
//!
//! ```rust,ignore
//! use oxide_sdk::draw::*;
//!
//! let c = Canvas::new();
//! c.clear(Color::hex(0x1e1e2e));
//! c.fill_rect(Rect::new(10.0, 10.0, 200.0, 100.0), Color::rgb(80, 120, 200));
//! c.fill_circle(Point2D::new(300.0, 200.0), 50.0, Color::RED);
//! c.text("Hello!", Point2D::new(20.0, 30.0), 24.0, Color::WHITE);
//! c.line(Point2D::ZERO, Point2D::new(400.0, 300.0), 2.0, Color::YELLOW);
//! let (w, h) = c.dimensions();
//! ```
//!
//! ## Low-Level Canvas Drawing
//!
//! The canvas is the main rendering surface. Coordinates start at `(0, 0)`
//! in the top-left corner. Each draw command maps to a GPUI GPU primitive.
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::canvas_clear`] | Clear canvas with a solid RGBA color |
//! | [`oxide_sdk::canvas_rect`] | Draw a filled rectangle |
//! | [`oxide_sdk::canvas_circle`] | Draw a filled circle |
//! | [`oxide_sdk::canvas_text`] | Draw text at a position |
//! | [`oxide_sdk::canvas_line`] | Draw a line between two points |
//! | [`oxide_sdk::canvas_dimensions`] | Get canvas `(width, height)` in pixels |
//! | [`oxide_sdk::canvas_image`] | Draw an encoded image (PNG, JPEG, GIF, WebP) |
//!
//! ```rust,ignore
//! use oxide_sdk::*;
//!
//! canvas_clear(30, 30, 46, 255);
//! canvas_rect(10.0, 10.0, 200.0, 100.0, 80, 120, 200, 255);
//! canvas_circle(300.0, 200.0, 50.0, 200, 100, 150, 255);
//! canvas_text(20.0, 30.0, 24.0, 255, 255, 255, 255, "Hello!");
//! canvas_line(0.0, 0.0, 400.0, 300.0, 255, 200, 0, 255, 2.0);
//!
//! let (w, h) = canvas_dimensions();
//! log(&format!("Canvas: {}x{}", w, h));
//! ```
//!
//! ## Console Logging
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::log`] | Print an informational message |
//! | [`oxide_sdk::warn`] | Print a warning (yellow) |
//! | [`oxide_sdk::error`] | Print an error (red) |
//!
//! ## HTTP Networking
//!
//! All network access is mediated by the host — the guest never opens raw
//! sockets. **Protocol Buffers** is the native wire format.
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::fetch`] | Full HTTP request with method, headers, body |
//! | [`oxide_sdk::fetch_get`] | HTTP GET shorthand |
//! | [`oxide_sdk::fetch_post`] | HTTP POST with content-type and body |
//! | [`oxide_sdk::fetch_post_proto`] | HTTP POST with protobuf body |
//! | [`oxide_sdk::fetch_put`] | HTTP PUT |
//! | [`oxide_sdk::fetch_delete`] | HTTP DELETE |
//!
//! ```rust,ignore
//! use oxide_sdk::*;
//!
//! let resp = fetch_get("https://api.example.com/data").unwrap();
//! log(&format!("Status: {}, Body: {}", resp.status, resp.text()));
//! ```
//!
//! ## Protobuf — Native Data Format
//!
//! The [`oxide_sdk::proto`] module provides a zero-dependency protobuf
//! encoder/decoder compatible with the Protocol Buffers wire format.
//!
//! ```rust,ignore
//! use oxide_sdk::proto::{ProtoEncoder, ProtoDecoder};
//!
//! let msg = ProtoEncoder::new()
//!     .string(1, "alice")
//!     .uint64(2, 42)
//!     .bool(3, true)
//!     .finish();
//!
//! let mut decoder = ProtoDecoder::new(&msg);
//! while let Some(field) = decoder.next() {
//!     match field.number {
//!         1 => log(&format!("name = {}", field.as_str())),
//!         2 => log(&format!("age  = {}", field.as_u64())),
//!         _ => {}
//!     }
//! }
//! ```
//!
//! ## Storage
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::storage_set`] | Store a key-value pair (session-scoped) |
//! | [`oxide_sdk::storage_get`] | Retrieve a value by key |
//! | [`oxide_sdk::storage_remove`] | Delete a key |
//! | [`oxide_sdk::kv_store_set`] | Persistent on-disk KV store |
//! | [`oxide_sdk::kv_store_get`] | Read from persistent KV store |
//! | [`oxide_sdk::kv_store_delete`] | Delete from persistent KV store |
//!
//! ## Audio
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::audio_play`] | Play audio from encoded bytes (WAV, MP3, OGG, FLAC) |
//! | [`oxide_sdk::audio_play_url`] | Fetch audio from a URL and play it |
//! | [`oxide_sdk::audio_play_with_format`] | Play with a format hint |
//! | [`oxide_sdk::audio_detect_format`] | Sniff container from magic bytes |
//! | [`oxide_sdk::audio_pause`] / [`oxide_sdk::audio_resume`] / [`oxide_sdk::audio_stop`] | Playback control |
//! | [`oxide_sdk::audio_set_volume`] / [`oxide_sdk::audio_get_volume`] | Volume control (0.0 – 2.0) |
//! | [`oxide_sdk::audio_is_playing`] | Check playback state |
//! | [`oxide_sdk::audio_position`] / [`oxide_sdk::audio_seek`] / [`oxide_sdk::audio_duration`] | Seek and position |
//! | [`oxide_sdk::audio_set_loop`] | Enable/disable looping |
//! | [`oxide_sdk::audio_channel_play`] | Multi-channel simultaneous playback |
//!
//! ## Video
//!
//! Video decoding uses FFmpeg on the host. Frames are rendered as GPUI
//! textures for GPU-accelerated compositing.
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::video_load`] / [`oxide_sdk::video_load_url`] | Load video from bytes or URL |
//! | [`oxide_sdk::video_play`] / [`oxide_sdk::video_pause`] / [`oxide_sdk::video_stop`] | Playback control |
//! | [`oxide_sdk::video_render`] | Draw current frame to canvas rectangle |
//! | [`oxide_sdk::video_seek`] / [`oxide_sdk::video_position`] / [`oxide_sdk::video_duration`] | Seek and timing |
//! | [`oxide_sdk::video_set_volume`] / [`oxide_sdk::video_set_loop`] | Volume and looping |
//! | [`oxide_sdk::video_set_pip`] | Picture-in-picture floating preview |
//! | [`oxide_sdk::video_hls_variant_count`] / [`oxide_sdk::video_hls_open_variant`] | HLS adaptive streaming |
//! | [`oxide_sdk::subtitle_load_srt`] / [`oxide_sdk::subtitle_load_vtt`] | Load subtitles |
//!
//! ## Media Capture
//!
//! The host shows permission dialogs before granting access to hardware.
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::camera_open`] / [`oxide_sdk::camera_close`] | Camera stream |
//! | [`oxide_sdk::camera_capture_frame`] / [`oxide_sdk::camera_frame_dimensions`] | Capture RGBA8 frames |
//! | [`oxide_sdk::microphone_open`] / [`oxide_sdk::microphone_close`] | Microphone stream |
//! | [`oxide_sdk::microphone_read_samples`] / [`oxide_sdk::microphone_sample_rate`] | Read mono f32 samples |
//! | [`oxide_sdk::screen_capture`] / [`oxide_sdk::screen_capture_dimensions`] | Screenshot |
//!
//! ## Timers
//!
//! Timer callbacks fire via the guest-exported `on_timer(callback_id)` function.
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::set_timeout`] | One-shot timer after a delay |
//! | [`oxide_sdk::set_interval`] | Repeating timer at an interval |
//! | [`oxide_sdk::clear_timer`] | Cancel a timer |
//! | [`oxide_sdk::time_now_ms`] | Current time (ms since UNIX epoch) |
//!
//! ## Animation Frames
//!
//! Vsync-aligned callbacks for smooth rendering. Callbacks fire via the
//! guest-exported `on_timer(callback_id)` function (the same export used by
//! [`oxide_sdk::set_timeout`]), one-shot per request.
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::request_animation_frame`] | Schedule a one-shot frame callback |
//! | [`oxide_sdk::cancel_animation_frame`] | Cancel a pending request |
//!
//! ## Streaming HTTP
//!
//! Non-blocking variant of [`oxide_sdk::fetch`] that streams the response body
//! back to the guest in chunks. Returns immediately with a handle; poll
//! [`oxide_sdk::fetch_state`] and [`oxide_sdk::fetch_recv`] each frame.
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::fetch_begin`] | Dispatch a streaming request, returns a handle |
//! | [`oxide_sdk::fetch_begin_get`] | GET shorthand |
//! | [`oxide_sdk::fetch_state`] | Poll lifecycle state (`FETCH_*` constants) |
//! | [`oxide_sdk::fetch_status`] | HTTP status code, or `0` until headers arrive |
//! | [`oxide_sdk::fetch_recv`] | Poll the next body chunk as a [`oxide_sdk::FetchChunk`] |
//! | [`oxide_sdk::fetch_recv_into`] | Poll into a caller-provided buffer (no allocation) |
//! | [`oxide_sdk::fetch_error`] | Retrieve the error message for a failed request |
//! | [`oxide_sdk::fetch_abort`] | Cancel an in-flight request |
//!
//! ## WebSocket
//!
//! Long-lived bidirectional connections. Messages are queued on the host and
//! drained by the guest each frame via [`oxide_sdk::ws_recv`].
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::ws_connect`] | Open a WebSocket, returns a handle |
//! | [`oxide_sdk::ws_ready_state`] | Poll state ([`oxide_sdk::WS_OPEN`], etc.) |
//! | [`oxide_sdk::ws_send_text`] | Send a UTF-8 text frame |
//! | [`oxide_sdk::ws_send_binary`] | Send a binary frame |
//! | [`oxide_sdk::ws_recv`] | Pop the next [`oxide_sdk::WsMessage`] from the queue |
//! | [`oxide_sdk::ws_close`] | Initiate the close handshake |
//! | [`oxide_sdk::ws_remove`] | Free host resources after close completes |
//!
//! ## MIDI Devices
//!
//! Read and write MIDI messages on hardware controllers and synthesisers.
//! Each input port maintains a bounded queue; long SysEx packets are split.
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::midi_input_count`] / [`oxide_sdk::midi_output_count`] | Enumerate ports |
//! | [`oxide_sdk::midi_input_name`] / [`oxide_sdk::midi_output_name`] | Look up port names |
//! | [`oxide_sdk::midi_open_input`] / [`oxide_sdk::midi_open_output`] | Open a port, returns a handle |
//! | [`oxide_sdk::midi_send`] | Send raw MIDI bytes to an output |
//! | [`oxide_sdk::midi_recv`] | Pop the next packet from an input queue |
//! | [`oxide_sdk::midi_close`] | Close a port |
//!
//! ## GPU
//!
//! WebGPU-style API for GPU-backed buffers, textures, shaders, and
//! pipelines. Shader source is WGSL.
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::gpu_create_buffer`] | Allocate a GPU buffer |
//! | [`oxide_sdk::gpu_create_texture`] | Allocate a 2D texture |
//! | [`oxide_sdk::gpu_create_shader`] | Compile a WGSL shader module |
//! | [`oxide_sdk::gpu_create_pipeline`] | Build a render pipeline |
//! | [`oxide_sdk::gpu_create_compute_pipeline`] | Build a compute pipeline |
//! | [`oxide_sdk::gpu_write_buffer`] | Upload bytes into a buffer |
//! | [`oxide_sdk::gpu_draw`] | Issue a draw call into a target texture |
//! | [`oxide_sdk::gpu_dispatch_compute`] | Dispatch a compute workgroup grid |
//! | [`oxide_sdk::gpu_destroy_buffer`] / [`oxide_sdk::gpu_destroy_texture`] | Release GPU resources |
//!
//! ## Navigation & History
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::navigate`] | Navigate to a new URL |
//! | [`oxide_sdk::push_state`] | Push history entry (like `pushState()`) |
//! | [`oxide_sdk::replace_state`] | Replace current history entry |
//! | [`oxide_sdk::get_url`] | Get current page URL |
//! | [`oxide_sdk::history_back`] / [`oxide_sdk::history_forward`] | Navigate history |
//!
//! ## Input Polling
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::mouse_position`] | Mouse `(x, y)` in canvas coordinates |
//! | [`oxide_sdk::mouse_button_down`] / [`oxide_sdk::mouse_button_clicked`] | Mouse button state |
//! | [`oxide_sdk::key_down`] / [`oxide_sdk::key_pressed`] | Keyboard state |
//! | [`oxide_sdk::scroll_delta`] | Scroll wheel delta |
//! | [`oxide_sdk::shift_held`] / [`oxide_sdk::ctrl_held`] / [`oxide_sdk::alt_held`] | Modifier keys |
//!
//! ## Interactive Widgets
//!
//! Widgets are rendered during the `on_frame()` loop:
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::ui_button`] | Clickable button, returns `true` when clicked |
//! | [`oxide_sdk::ui_checkbox`] | Checkbox, returns current checked state |
//! | [`oxide_sdk::ui_slider`] | Slider, returns current value |
//! | [`oxide_sdk::ui_text_input`] | Text input field, returns current text |
//!
//! ## Crypto & Encoding
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::hash_sha256`] | SHA-256 hash (32-byte array) |
//! | [`oxide_sdk::hash_sha256_hex`] | SHA-256 hash (hex string) |
//! | [`oxide_sdk::base64_encode`] / [`oxide_sdk::base64_decode`] | Base64 encoding/decoding |
//!
//! ## Other APIs
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::clipboard_write`] / [`oxide_sdk::clipboard_read`] | System clipboard access |
//! | [`oxide_sdk::random_u64`] / [`oxide_sdk::random_f64`] | Cryptographic random numbers |
//! | [`oxide_sdk::notify`] | Send a notification |
//! | [`oxide_sdk::upload_file`] | Open native file picker |
//! | [`oxide_sdk::get_location`] | Mock geolocation |
//! | [`oxide_sdk::load_module`] | Dynamically load another `.wasm` module |
//! | [`oxide_sdk::register_hyperlink`] / [`oxide_sdk::clear_hyperlinks`] | Canvas hyperlinks |
//! | [`oxide_sdk::url_resolve`] / [`oxide_sdk::url_encode`] / [`oxide_sdk::url_decode`] | URL utilities |
//!
//! ---
//!
//! # Browser Internals
//!
//! The [`oxide_browser`] crate contains the host-side implementation.
//! Key modules for contributors:
//!
//! - **[`oxide_browser::engine`]** — Wasmtime engine setup, [`oxide_browser::engine::SandboxPolicy`],
//!   fuel metering, bounded linear memory
//! - **[`oxide_browser::runtime`]** — [`oxide_browser::runtime::BrowserHost`] orchestrates module
//!   fetching, compilation, and execution. [`oxide_browser::runtime::LiveModule`] keeps interactive
//!   apps alive across frames.
//! - **[`oxide_browser::capabilities`]** — The `"oxide"` import module: every host function the
//!   guest can call is registered here via `register_host_functions()`. Also contains shared state
//!   types ([`oxide_browser::capabilities::HostState`], [`oxide_browser::capabilities::CanvasState`],
//!   [`oxide_browser::capabilities::InputState`], etc.).
//! - **[`oxide_browser::navigation`]** — [`oxide_browser::navigation::NavigationStack`] implements
//!   browser-style back/forward history with opaque state.
//! - **[`oxide_browser::bookmarks`]** — [`oxide_browser::bookmarks::BookmarkStore`] provides
//!   persistent bookmark storage backed by sled.
//! - **[`oxide_browser::url`]** — [`oxide_browser::url::OxideUrl`] wraps WHATWG URL parsing with
//!   support for `http`, `https`, `file`, and `oxide://` schemes.
//! - **[`oxide_browser::ui`]** — [`oxide_browser::ui::OxideBrowserView`] and [`oxide_browser::ui::run_browser`]
//!   implement tabbed browsing, toolbar, canvas rendering, console panel, and bookmarks sidebar.
//! - **[`oxide_browser::video`]**, **[`oxide_browser::audio_format`]**, **[`oxide_browser::media_capture`]** —
//!   FFmpeg video pipeline, audio format sniffing, and camera/microphone/screen capture host state.
//! - **[`oxide_browser::gpu`]** — WebGPU-style host state: buffers, textures, shaders, and render/compute pipelines.
//! - **[`oxide_browser::rtc`]** — WebRTC peer connections, data channels, media tracks, and the built-in signalling client.
//! - **[`oxide_browser::websocket`]** — WebSocket host state: connection registry, send/recv queues, and ready-state tracking.
//! - **[`oxide_browser::midi`]** — MIDI input/output port enumeration, bounded receive queues, and packet splitting.
//! - **[`oxide_browser::fetch`]** — Streaming fetch host state: in-flight handles, body chunk queue, and abort tracking.
//! - **[`oxide_browser::download`]** — Background downloader for non-WASM URLs surfaced as files in the host UI.
//!
//! ---
//!
//! # Guest Module Contract
//!
//! Every `.wasm` module loaded by Oxide must:
//!
//! 1. **Export `start_app`** — `extern "C" fn()` entry point called on load
//! 2. **Optionally export `on_frame`** — `extern "C" fn(dt_ms: u32)` for
//!    interactive apps with a render loop
//! 3. **Optionally export `on_timer`** — `extern "C" fn(callback_id: u32)`
//!    to receive timer callbacks
//! 4. **Import from `"oxide"`** — all host APIs live under this namespace
//! 5. **Compile as `cdylib`** — `crate-type = ["cdylib"]` in `Cargo.toml`
//! 6. **Target `wasm32-unknown-unknown`** — no WASI, pure capability-based
//!
//! ---
//!
//! # Security Model
//!
//! | Constraint | Value | Purpose |
//! |-----------|-------|---------|
//! | Filesystem access | None | Guest cannot read/write host files |
//! | Environment variables | None | Guest cannot inspect host env |
//! | Raw network sockets | None | All networking is mediated via `fetch` |
//! | Memory limit | 256 MB (4096 pages) | Prevents memory exhaustion |
//! | Fuel limit | 500M instructions | Prevents infinite loops / DoS |
//! | No WASI | — | Zero implicit system access |

pub use oxide_browser;
pub use oxide_sdk;
