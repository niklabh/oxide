# Oxide — A Binary-First Browser

Oxide is a decentralised browser that fetches and executes `.wasm` (WebAssembly) modules instead of HTML/JavaScript. Guest applications run in a secure, sandboxed environment with capability-based access to host APIs.

![oxide](screens/oxide.png)

![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/niklabh/oxide/ci.yml?branch=main)


## Quick Start

```bash
# Install the wasm target
rustup target add wasm32-unknown-unknown

# Build and run the browser
cargo run -p oxide-browser

# Build the example guest app
cargo build --target wasm32-unknown-unknown --release -p hello-oxide

# In the browser, click "Open File" and select:
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
│  │  "oxide" import module — ~50 host functions                │  │
│  │  canvas · console · storage · clipboard · fetch · crypto   │  │
│  │  input · widgets · navigation · dynamic module loading     │  │
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
│       ├── capabilities.rs  # ~50 host functions registered into the wasmtime Linker
│       ├── navigation.rs    # History stack, back/forward, push/replace state
│       ├── url.rs           # WHATWG-style URL parser (http, https, file, oxide schemes)
│       └── ui.rs            # GPUI shell — toolbar, canvas paint, console, widgets
├── oxide-sdk/               # Guest-side SDK (no dependencies, pure FFI wrappers)
│   └── src/
│       ├── lib.rs           # Safe Rust wrappers over host imports
│       └── proto.rs         # Zero-dependency protobuf wire-format codec
└── examples/
    ├── hello-oxide/         # Minimal guest app
    └── fullstack-notes/     # Full-stack example (Rust frontend + backend)
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
3. **Link** — A `Linker` registers all `oxide::*` host functions from `capabilities.rs` and creates a bounded linear memory (256 pages / 16 MB max).
4. **Instantiate** — wasmtime creates an `Instance`; the `Store` carries a `HostState` holding canvas commands, console lines, input state, storage maps, and widget state.
5. **start_app()** — the guest's exported entry point runs once.
6. **on_frame(dt_ms)** — if the guest exports this function, the browser calls it every frame for interactive/immediate-mode apps. Fuel is replenished each frame.

Load and execution happen on a background thread with its own tokio runtime. The UI communicates via `RunRequest`/`RunResult` channels and keeps a `LiveModule` handle for the frame loop.

### Host–Guest Boundary

Guest modules start with **zero capabilities**. Every interaction with the outside world goes through explicit host functions registered in the wasmtime `Linker` under the `"oxide"` namespace:

| Category | Host Functions |
|----------|---------------|
| **Canvas** | `clear`, `rect`, `circle`, `text`, `line`, `image`, `dimensions` |
| **UI widgets** | `button`, `checkbox`, `slider`, `text_input` (immediate-mode) |
| **Console** | `log`, `warn`, `error` |
| **Input** | `mouse_position`, `mouse_button_down/clicked`, `key_down/pressed`, `scroll_delta`, `modifiers` |
| **Storage** | `storage_set/get/remove` (session), `kv_store_set/get/delete` (persistent, sled-backed) |
| **Networking** | `fetch` (full HTTP), convenience wrappers for GET/POST/PUT/DELETE |
| **Navigation** | `navigate`, `push_state`, `replace_state`, `get_url`, `history_back/forward` |
| **Crypto / encoding** | `hash_sha256`, `base64_encode/decode` |
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
| Memory ceiling | 16 MB (256 pages) | Prevents memory exhaustion |
| Fuel budget | 500M instructions/call | Prevents infinite loops and DoS |

Security is **additive, not subtractive**: there is nothing to claw back because nothing is granted by default. File uploads go through a host-side native file picker; HTTP goes through a host-side reqwest client; child modules get their own isolated memory and fuel. No WASI is linked — the sandbox is airtight by construction.

## Core Stack

| Component   | Crate      | Purpose |
|-------------|------------|---------|
| Runtime     | `wasmtime` | WASM execution with fuel metering and memory limits |
| Networking  | `reqwest`  | Fetch `.wasm` binaries from URLs |
| Async       | `tokio`    | Async runtime for network operations |
| UI          | [GPUI](https://www.gpui.rs/) | GPU-accelerated window; URL bar, canvas, console |
| Storage     | `sled`     | Persistent key-value store (per-origin) |
| File Picker | `rfd`      | Native OS file dialogs |
| Clipboard   | `arboard`  | System clipboard access |
| Imaging     | `image`    | PNG/JPEG/GIF/WebP decoding for canvas images |
| Crypto      | `sha2`     | SHA-256 hashing for guest modules |

## Documentation

See [DOCS.md](DOCS.md) for the full developer guide, API reference, and instructions for building WASM websites.
