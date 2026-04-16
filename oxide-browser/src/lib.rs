//! # Oxide Browser — Host Runtime
//!
//! `oxide-browser` is the native desktop host application for the
//! [Oxide browser](https://github.com/niklabh/oxide), a **binary-first browser**
//! that fetches and executes `.wasm` (WebAssembly) modules instead of
//! HTML/JavaScript.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────┐
//! │                   Oxide Browser                  │
//! │  ┌──────────┐  ┌────────────┐  ┌──────────────┐  │
//! │  │  URL Bar │  │   Canvas   │  │   Console    │  │
//! │  └────┬─────┘  └──────┬─────┘  └──────┬───────┘  │
//! │       │               │               │          │
//! │  ┌────▼───────────────▼───────────────▼───────┐  │
//! │  │              Host Runtime                  │  │
//! │  │  wasmtime engine + sandbox policy          │  │
//! │  │  fuel limit: 500M  │  memory: 256MB max    │  │
//! │  └────────────────────┬───────────────────────┘  │
//! │                       │                          │
//! │  ┌────────────────────▼───────────────────────┐  │
//! │  │          Capability Provider               │  │
//! │  │  "oxide" import module                     │  │
//! │  │  canvas, console, storage, clipboard,      │  │
//! │  │  fetch, images, crypto, base64, protobuf,  │  │
//! │  │  dynamic module loading, audio, timers,    │  │
//! │  │  navigation, widgets, input, hyperlinks,   │  │
//! │  │  GPU/WebGPU-style resource management      │  │
//! │  └────────────────────┬───────────────────────┘  │
//! │                       │                          │
//! │  ┌────────────────────▼───────────────────────┐  │
//! │  │           Guest .wasm Module               │  │
//! │  │  exports: start_app(), on_frame(dt_ms)     │  │
//! │  │  imports: oxide::*                         │  │
//! │  └────────────────────────────────────────────┘  │
//! └──────────────────────────────────────────────────┘
//! ```
//!
//! ## Modules
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`engine`] | Wasmtime engine configuration, sandbox policy, memory bounds |
//! | [`runtime`] | Module fetching, compilation, execution lifecycle |
//! | [`capabilities`] | All host-imported functions exposed to guest wasm modules |
//! | [`gpu`] | WebGPU-style GPU resource management (buffers, textures, shaders, pipelines) |
//! | [`media_capture`] | Camera, microphone, and screen capture with permission prompts |
//! | [`rtc`] | WebRTC peer connections, data channels, media tracks, and signaling |
//! | [`navigation`] | Browser history stack with back/forward traversal |
//! | [`bookmarks`] | Persistent bookmark storage backed by sled |
//! | [`url`] | WHATWG-compliant URL parsing with Oxide-specific schemes |
//! | [`ui`] | GPUI desktop shell (toolbar, canvas, console, tabs) |
//!
//! ## Which API do I need?
//!
//! | You are building… | Use this crate | Notes |
//! |---|---|---|
//! | A **guest** `.wasm` app (canvas, fetch, widgets) | **`oxide-sdk` only** | Import `oxide::*` via the SDK; you never link `oxide-browser`. |
//! | The **stock desktop browser** binary | **`cargo run -p oxide-browser`** | The `oxide` binary wires [`runtime::BrowserHost`] to [`ui::run_browser`]. |
//! | A **custom native shell** (alternate windowing, tests, automation) | [`ui::run_browser`] + [`runtime::BrowserHost`] | Same [`capabilities::HostState`] pipeline; swap or wrap the GPUI window if needed. |
//! | **GPU/UI work** next to Oxide (panels, overlays, devtools) | [`gpui`] | Re-export of [GPUI](https://www.gpui.rs/); version matches this crate’s dependency. |
//!
//! ### Relationship between GPUI and the SDK
//!
//! Guest `.wasm` modules cannot link GPUI directly (it requires native GPU
//! access). Instead, the `oxide-sdk` crate provides drawing functions that
//! the host translates into GPUI primitives each frame:
//!
//! - `canvas_rect` → `Window::paint_quad` with `gpui::fill`
//! - `canvas_circle` → `Window::paint_path` with polygon approximation
//! - `canvas_text` → GPU text shaping via `Window::text_system().shape_line`
//! - `canvas_line` → `Window::paint_path` with `PathBuilder::stroke`
//! - `canvas_image` → `Window::paint_image` with `RenderImage` texture cache
//!
//! The `oxide_sdk::draw` module provides higher-level types (`Canvas`,
//! `Color`, `Rect`, `Point2D`) modelled after GPUI conventions.
//!
//! ### Host entrypoints (Rust)
//!
//! - **[`ui::run_browser`]** — Blocks on the GPUI event loop and opens the main Oxide window. Pass the shared [`capabilities::HostState`] and [`runtime::PageStatus`] from a [`runtime::BrowserHost`].
//! - **`gpui`** — Full GPUI API for native code that ships beside the browser (not available inside guest wasm).
//!
//! ## Security Model
//!
//! Every guest `.wasm` module runs in a strict sandbox:
//!
//! - **No filesystem access** — guests cannot read or write host files
//! - **No environment variables** — guests cannot inspect the host environment
//! - **No raw sockets** — all network access is mediated through `fetch`
//! - **Bounded memory** — 16 MB (256 pages) hard limit
//! - **Fuel metering** — 500M instruction budget prevents infinite loops
//! - **Capability-based I/O** — only explicitly provided `oxide::*` functions
//!   are available to the guest

pub mod audio_format;
pub mod bookmarks;
pub mod capabilities;
pub mod download;
pub mod engine;
pub mod gpu;
pub mod history;
pub mod media_capture;
pub mod midi;
pub mod navigation;
pub mod rtc;
pub mod runtime;
pub mod subtitle;
pub mod ui;
pub mod url;
pub mod video;
pub mod video_format;
pub mod websocket;

/// GPU-accelerated UI framework used by the desktop shell (see [GPUI](https://www.gpui.rs/)).
///
/// Depend on `oxide-browser` and `use oxide_browser::gpui` so your native tooling stays on the same
/// GPUI version as the browser. Guest WebAssembly modules cannot use this crate; they use the
/// [`oxide-sdk`](https://docs.rs/oxide-sdk) crate instead.
pub use gpui;
