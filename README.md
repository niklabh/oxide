# Oxide — A Binary-First Browser

[![Release](https://img.shields.io/github/v/release/niklabh/oxide?style=flat-square)](https://github.com/niklabh/oxide/releases)
[![Crates.io](https://img.shields.io/crates/v/oxide-sdk?style=flat-square)](https://crates.io/crates/oxide-sdk)
[![License](https://img.shields.io/crates/l/oxide-sdk?style=flat-square)](https://github.com/niklabh/oxide/blob/main/LICENSE)
[![docs.rs](https://img.shields.io/docsrs/oxide-sdk?style=flat-square)](https://docs.rs/oxide-sdk)
[![CI](https://img.shields.io/github/actions/workflow/status/niklabh/oxide/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/niklabh/oxide/actions/workflows/ci.yml)

Oxide is a decentralised browser that fetches and executes `.wasm` (WebAssembly) modules instead of HTML/JavaScript. Guest applications run in a secure, sandboxed environment with capability-based access to host APIs.

![oxide](screens/oxide.png)


## Quick Start

```bash
# Install the wasm target
rustup target add wasm32-unknown-unknown

# Build and run the browser
cargo run -p oxide-browser

# Build the example guest app
cargo build --target wasm32-unknown-unknown --release -p hello-oxide

# In the browser, click "Open" and select:
# target/wasm32-unknown-unknown/release/hello_oxide.wasm
```

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                        Oxide Browser                             │
│                                                                  │
│  ┌──────────┐  ┌────────────────────────┐  ┌─────────────────┐   │
│  │  URL Bar │  │        Canvas          │  │     Console     │   │
│  └────┬─────┘  └───────────┬────────────┘  └────────┬────────┘   │
│       │                    │                        │            │
│  ┌────▼────────────────────▼────────────────────────▼─-───────┐  │
│  │                    Host Runtime                            │  │
│  │  wasmtime engine  ·  fuel metering  ·  bounded memory      │  │
│  └────────────────────────────┬───────────────────────────────┘  │
│                               │                                  │
│  ┌────────────────────────────▼───────────────────────────────┐  │
│  │                  Capability Layer                          │  │
│  │  "oxide" import module — ~150 host functions                │  │
│  │  canvas · gpu · audio · video · capture · fetch · streaming │  │
│  │  websocket · webrtc · midi · timers · animation frames     │  │
│  │  console · storage · clipboard · widgets · crypto · ...    │  │
│  └────────────────────────────┬───────────────────────────────┘  │
│                               │                                  │
│  ┌────────────────────────────▼───────────────────────────────┐  │
│  │                  Guest .wasm Module                        │  │
│  │  exports: start_app(), on_frame(dt)                        │  │
│  │  imports: oxide::*  (via oxide-sdk)                        │  │
│  └────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

### Project Structure

```
oxide/
├── oxide-browser/           # Host browser application
│   └── src/
│       ├── main.rs          # GPUI entry — BrowserHost + run_browser
│       ├── engine.rs        # WasmEngine, SandboxPolicy, compile & memory bounds
│       ├── runtime.rs       # BrowserHost, fetch/load/instantiate pipeline
│       ├── capabilities.rs  # ~100 host functions registered into the wasmtime Linker
│       ├── navigation.rs    # History stack, back/forward, push/replace state
│       ├── url.rs           # WHATWG-style URL parser (http, https, file, oxide schemes)
│       └── ui.rs            # GPUI shell — toolbar, canvas paint, console, widgets
├── oxide-sdk/               # Guest-side SDK (no dependencies, pure FFI wrappers)
│   └── src/
│       ├── lib.rs           # Safe Rust wrappers over host imports
│       └── proto.rs         # Zero-dependency protobuf wire-format codec
└── examples/
    ├── hello-oxide/         # Minimal guest app
    ├── audio-player/        # Audio playback
    ├── video-player/        # FFmpeg video, subtitles, PiP, HLS
    ├── media-capture/       # Camera, microphone, screen capture
    ├── gpu-graphics-demo/   # WebGPU-style rendering and compute
    ├── rtc-chat/            # WebRTC peer-to-peer chat
    ├── ws-chat/             # WebSocket chat
    ├── stream-fetch-demo/   # Streaming HTTP fetch
    ├── timer-demo/          # set_timeout / set_interval
    ├── raf-demo/            # request_animation_frame
    ├── midi-demo/           # MIDI input visualizer
    ├── index/               # Demo hub linking the others
    └── fullstack-notes/     # Full-stack Rust frontend + backend
```

### Module Loading Pipeline

When a user enters a URL or opens a `.wasm` file, the browser walks through a short, deterministic pipeline — no parsing, no tree-building, no style resolution:

```
 ┌───────────────┐     ┌──────────────┐     ┌──────────────────┐
 │  Fetch bytes  │────▶│   Compile    │────▶│  Link host fns   │
 │  (HTTP/file)  │     │  (wasmtime)  │     │  + bounded mem   │
 └───────────────┘     └──────────────┘     └────────┬─────────┘
                                                     │
 ┌───────────────┐     ┌──────────────┐     ┌────────▼─────────┐
 │  Frame loop   │◀────│  start_app() │◀────│   Instantiate    │
 │  (on_frame)   │     │  entry call  │     │   wasm module    │
 └───────────────┘     └──────────────┘     └──────────────────┘
```

1. **Fetch** — download raw `.wasm` bytes via HTTP or read from a local file (max 50 MB).
2. **Compile** — `WasmEngine` compiles the module through wasmtime with a `SandboxPolicy` that sets fuel and memory bounds.
3. **Link** — A `Linker` registers all `oxide::*` host functions from `capabilities.rs` and creates a bounded linear memory (4096 pages / 256 MB max).
4. **Instantiate** — wasmtime creates an `Instance`; the `Store` carries a `HostState` holding canvas commands, console lines, input state, storage maps, and widget state.
5. **start_app()** — the guest's exported entry point runs once.
6. **on_frame(dt_ms)** — if the guest exports this function, the browser calls it every frame for interactive/immediate-mode apps. Fuel is replenished each frame.

Load and execution happen on a background thread with its own tokio runtime. The UI communicates via `RunRequest`/`RunResult` channels and keeps a `LiveModule` handle for the frame loop.

### Host–Guest Boundary

Guest modules start with **zero capabilities**. Every interaction with the outside world goes through explicit host functions registered in the wasmtime `Linker` under the `"oxide"` namespace:

| Category | Host Functions |
|----------|---------------|
| **Canvas** | `clear`, `rect`, `circle`, `text`, `line`, `image`, `dimensions`, plus the `oxide_sdk::draw` high-level API |
| **GPU** | `gpu_create_buffer/texture/shader`, `gpu_create_pipeline`, `gpu_create_compute_pipeline`, `gpu_write_buffer`, `gpu_draw`, `gpu_dispatch_compute` (WGSL) |
| **UI widgets** | `button`, `checkbox`, `slider`, `text_input` (immediate-mode) |
| **Console** | `log`, `warn`, `error` |
| **Input** | `mouse_position`, `mouse_button_down/clicked`, `key_down/pressed`, `scroll_delta`, `modifiers` |
| **Storage** | `storage_set/get/remove` (session), `kv_store_set/get/delete` (persistent, sled-backed) |
| **HTTP** | `fetch` (full HTTP), `fetch_get/post/put/delete`, `fetch_post_proto` |
| **Streaming HTTP** | `fetch_begin`, `fetch_state`, `fetch_status`, `fetch_recv`, `fetch_error`, `fetch_abort` |
| **WebSocket** | `ws_connect`, `ws_send_text/binary`, `ws_recv`, `ws_ready_state`, `ws_close`, `ws_remove` |
| **WebRTC** | `rtc_create_peer`, `rtc_create_offer/answer`, `rtc_set_local/remote_description`, `rtc_add_ice_candidate`, `rtc_create_data_channel`, `rtc_send_text/binary`, `rtc_recv`, `rtc_add_track`, `rtc_poll_track`, `rtc_signal_*` |
| **Audio** | `audio_play`, `audio_play_url`, `audio_pause/resume/stop`, `audio_set_volume`, `audio_seek`, `audio_set_loop`, `audio_channel_play` (multi-channel), `audio_detect_format` |
| **Video** | `video_load`, `video_load_url`, `video_play/pause/stop`, `video_render`, `video_seek`, `video_set_pip`, `video_hls_*`, `subtitle_load_srt/vtt` (FFmpeg-backed) |
| **Media capture** | `camera_open`, `camera_capture_frame`, `microphone_open`, `microphone_read_samples`, `screen_capture` |
| **MIDI** | `midi_input_count/output_count`, `midi_input_name/output_name`, `midi_open_input/output`, `midi_send`, `midi_recv`, `midi_close` |
| **Timers** | `set_timeout`, `set_interval`, `clear_timer`, `request_animation_frame`, `cancel_animation_frame` |
| **Navigation** | `navigate`, `push_state`, `replace_state`, `get_url`, `history_back/forward` |
| **Hyperlinks** | `register_hyperlink`, `clear_hyperlinks` (clickable canvas regions) |
| **Crypto / encoding** | `hash_sha256`, `hash_sha256_hex`, `base64_encode/decode` |
| **Clipboard** | `read`, `write` |
| **Time / random** | `time_now_ms`, `random_u64`, `random_f64` |
| **Dynamic loading** | `load_module` — fetch and run a child `.wasm` with isolated memory and fuel |

Data crosses the boundary through the guest's linear memory: the guest passes `(ptr, len)` pairs; the host reads/writes with `read_guest_string` / `write_guest_bytes`. Return values pack status and length into a single integer where needed.

### Rendering Model

Oxide uses an **immediate-mode rendering** approach — there is no retained scene graph or DOM:

1. Each frame, the guest issues draw commands (`canvas_clear`, `canvas_rect`, `canvas_text`, …) which push `DrawCommand` variants into `HostState.canvas.commands`.
2. Widget calls (`ui_button`, `ui_slider`, …) push `WidgetCommand` entries.
3. `ui.rs` drains both queues each frame and paints them with GPUI (`paint_quad`, `paint_path`, `paint_image`, text shaping) inside the canvas region.
4. Images are decoded once and cached as GPUI [`RenderImage`](https://docs.rs/gpui) textures (same path for video frames).
5. Widget interactions (clicks, drags, text edits) flow back through `widget_states` and `widget_clicked`, which the guest reads on the next frame call.

No layout engine. No style cascade. The guest decides exactly where every pixel goes.

### Security Model

| Constraint | Value | Purpose |
|---|---|---|
| Filesystem access | **None** | Guest cannot touch host files |
| Environment variables | **None** | Guest cannot read host env |
| Network sockets | **None** | All HTTP is mediated by the host |
| Memory ceiling | 256 MB (4096 pages) | Prevents memory exhaustion |
| Fuel budget | 500M instructions/call | Prevents infinite loops and DoS |

Security is **additive, not subtractive**: there is nothing to claw back because nothing is granted by default. File uploads go through a host-side native file picker; HTTP goes through a host-side reqwest client; child modules get their own isolated memory and fuel. No WASI is linked — the sandbox is airtight by construction.

## Core Stack

| Component   | Crate      | Purpose |
|-------------|------------|---------|
| Runtime     | `wasmtime` | WASM execution with fuel metering and memory limits |
| Networking  | `reqwest` + `tokio-tungstenite` | HTTP fetch (`.wasm`, streaming) and WebSocket frames |
| Async       | `tokio`    | Async runtime for network and background work |
| UI          | [GPUI](https://www.gpui.rs/) | GPU-accelerated shell; URL bar, canvas, console, tabs |
| Storage     | `sled`     | Persistent key-value store (per-origin) |
| File Picker | `rfd`      | Native OS file dialogs |
| Clipboard   | `arboard`  | System clipboard access |
| Imaging     | `image`    | PNG/JPEG/GIF/WebP decoding for canvas images |
| Video       | `ffmpeg-next` | H.264/H.265/AV1/VP9 decode, HLS, subtitles, PiP |
| Audio       | `rodio`    | Multi-channel playback and decode |
| Capture     | `nokhwa`, `cpal`, `screenshots` | Camera, microphone, screen capture |
| GPU         | `wgpu`     | WebGPU-style host backend for guest GPU APIs |
| WebRTC      | `webrtc`   | Peer connections, data channels, media tracks |
| MIDI        | `coremidi` (macOS) | Cross-platform MIDI I/O |
| Crypto      | `sha2`     | SHA-256 hashing for guest modules |

## Documentation

See [DOCS.md](DOCS.md) for the full developer guide, API reference, and instructions for building WASM websites.
