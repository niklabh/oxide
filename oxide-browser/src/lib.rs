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
//! │  │  fuel limit: 500M  │  memory: 16MB max     │  │
//! │  └────────────────────┬───────────────────────┘  │
//! │                       │                          │
//! │  ┌────────────────────▼───────────────────────┐  │
//! │  │          Capability Provider               │  │
//! │  │  "oxide" import module                     │  │
//! │  │  canvas, console, storage, clipboard,      │  │
//! │  │  fetch, images, crypto, base64, protobuf,  │  │
//! │  │  dynamic module loading, audio, timers,    │  │
//! │  │  navigation, widgets, input, hyperlinks    │  │
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
//! | [`navigation`] | Browser history stack with back/forward traversal |
//! | [`bookmarks`] | Persistent bookmark storage backed by sled |
//! | [`url`] | WHATWG-compliant URL parsing with Oxide-specific schemes |
//! | [`ui`] | egui/eframe desktop UI (toolbar, canvas, console, tabs) |
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
pub mod engine;
pub mod navigation;
pub mod runtime;
pub mod subtitle;
pub mod ui;
pub mod url;
pub mod video;
pub mod video_format;
