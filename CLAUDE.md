# Oxide — CLAUDE.md

This is a Rust workspace. `oxide-browser` is the native host (wasmtime + GPUI desktop app). `oxide-sdk` is the guest-side crate compiled to `wasm32-unknown-unknown`. They never share Rust types at runtime — the boundary is pure FFI through linear memory.

---

## Build commands

```bash
# Build the host browser
cargo build -p oxide-browser

# Run the browser
cargo run -p oxide-browser

# Build an example guest app (always release for WASM)
cargo build --target wasm32-unknown-unknown --release -p hello-oxide

# Full check suite (must pass before any commit)
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Build the SDK for WASM (sanity check)
cargo build -p oxide-sdk --target wasm32-unknown-unknown
```

FFmpeg dev libraries are required on the host for video decode (`brew install ffmpeg` on macOS).

---

## Project structure

```
oxide/
├── oxide-browser/src/
│   ├── capabilities.rs   # All host functions registered into the wasmtime Linker
│   ├── engine.rs         # WasmEngine, SandboxPolicy, fuel/memory bounds
│   ├── runtime.rs        # BrowserHost — fetch, compile, instantiate, frame loop
│   ├── ui.rs             # GPUI shell — toolbar, canvas, console, widgets
│   ├── navigation.rs     # History stack
│   ├── url.rs            # URL parsing (http, https, file, oxide schemes)
│   ├── rtc.rs            # WebRTC (register_rtc_functions)
│   ├── websocket.rs      # WebSocket (register_ws_functions)
│   └── gpu.rs, audio_format.rs, video.rs, media_capture.rs, …
├── oxide-sdk/src/
│   ├── lib.rs            # FFI declarations + safe public wrappers
│   └── proto.rs          # Zero-dependency protobuf codec
└── examples/
    ├── hello-oxide/      # Minimal guest app
    ├── ws-chat/          # WebSocket demo
    ├── rtc-chat/         # WebRTC demo
    └── …
```

`HostState` in `capabilities.rs` is the central shared state struct. All host modules (rtc, websocket, gpu, …) add an `Arc<Mutex<Option<TheirState>>>` field to it and register their functions via a `register_*_functions(linker)` call at the bottom of `register_host_functions`.

---

## Adding a host function (the most common task)

Every new capability follows this four-step pattern:

**1. If the feature needs significant state, create a new module** (e.g. `websocket.rs`) with a `TheirState` struct and a `register_*_functions(linker: &mut Linker<HostState>) -> Result<()>` function. Add `pub mod their_module;` to `lib.rs`.

**2. Add state to `HostState`** in `capabilities.rs`:
```rust
pub ws: Arc<Mutex<Option<crate::websocket::WsState>>>,
```
And initialise it in the `Default` impl:
```rust
ws: Arc::new(Mutex::new(None)),
```

**3. Register the host function** inside `register_host_functions` (or in the module's own `register_*_functions`):
```rust
linker.func_wrap("oxide", "api_my_feature",
    |mut caller: Caller<'_, HostState>, ptr: u32, len: u32| -> u32 {
        let mem = caller.data().memory.expect("memory not set");
        let s = read_guest_string(&mem, &caller, ptr, len).unwrap_or_default();
        // … host logic …
        0
    },
)?;
```

**4. Expose it in the SDK** (`oxide-sdk/src/lib.rs`):
```rust
// In the extern "C" block:
#[link_name = "api_my_feature"]
fn _api_my_feature(ptr: u32, len: u32) -> u32;

// Public wrapper:
/// Brief doc comment.
pub fn my_feature(s: &str) -> u32 {
    unsafe { _api_my_feature(s.as_ptr() as u32, s.len() as u32) }
}
```

### Memory helpers (already in `capabilities.rs`)

| Helper | Use |
|--------|-----|
| `read_guest_string(&mem, &caller, ptr, len)` | Read UTF-8 from guest |
| `read_guest_bytes(&mem, &caller, ptr, len)` | Read raw bytes from guest |
| `write_guest_bytes(&mem, &mut caller, ptr, data)` | Write bytes into guest |

Data always crosses the boundary as `(ptr: u32, len: u32)` pairs. Never pass Rust references across the boundary.

---

## Naming conventions

| Entity | Convention | Example |
|--------|------------|---------|
| Host function name (linker) | `api_<category>_<action>` | `api_ws_connect`, `api_canvas_rect` |
| SDK FFI import | `_api_<name>` | `_api_ws_connect` |
| SDK public wrapper | `<category>_<action>` | `ws_connect`, `canvas_rect` |
| State struct | `<Feature>State` | `WsState`, `RtcState`, `GpuState` |
| Register function | `register_<feature>_functions` | `register_ws_functions` |
| Constants | `UPPER_SNAKE_CASE` | `WS_OPEN`, `KEY_ENTER` |

---

## Security rules — never break these

- **Never link WASI** — the sandbox is airtight by construction. No filesystem, no env vars, no raw sockets.
- **All host access is additive** — if you didn't explicitly register it in the linker, the guest cannot call it.
- **Validate all lengths** before reading guest memory. A malicious guest can pass any ptr/len.
- **New capabilities must be opt-in** — add them to the linker explicitly, not via a wildcard.

---

## Guest app contract

Every `.wasm` module must:
1. Export `start_app()` — called once on load.
2. Optionally export `on_frame(dt_ms: u32)` — called every frame (fuel replenished each call).
3. Optionally export `on_timer(callback_id: u32)` — called when a `set_timeout`/`set_interval` fires.
4. Compile as `[lib] crate-type = ["cdylib"]` targeting `wasm32-unknown-unknown`.
5. Import everything from the `"oxide"` wasm import module — never WASI.

---

## LLM coding guidelines

**Think before coding.**
- State assumptions explicitly. If multiple interpretations exist, present them — don't pick silently.
- If something is unclear, stop and ask before writing code.
- If a simpler approach exists, say so. Push back when warranted.

**Simplicity first.**
- Minimum code that solves the problem. Nothing speculative.
- No abstractions for single-use code. No "flexibility" that wasn't asked for.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

**Surgical changes.**
- Touch only what you must. Don't improve adjacent code, comments, or formatting.
- Match the existing style (e.g. `block_on` for async in RTC, unbounded mpsc + background task in websocket).
- When your changes make something unused (import, variable, function), remove it. Don't remove pre-existing dead code unless asked.
- Every changed line should trace directly to the request.

**Verify your work.**
- After any host-side change: `cargo build -p oxide-browser` must pass.
- After any SDK change: `cargo build -p oxide-sdk --target wasm32-unknown-unknown` must pass.
- After any example change: `cargo build --target wasm32-unknown-unknown --release -p <example>` must pass.
- For host functions: confirm the function is registered in `register_host_functions` (or called from there) and the SDK wrapper is present with a doc comment.

---

## Common pitfalls

- **`memory` is set after instantiation** — host functions called during `start_app` may need to use `caller.data().memory.expect("memory not set")`. The `memory` field is `Option<Memory>` for this reason.
- **Guest state is no_std** — example apps run on `wasm32-unknown-unknown` with no std allocator by default unless `alloc` is explicitly linked. Keep guest code allocation-minimal.
- **`block_on` vs spawn** — RTC uses `runtime.block_on` for synchronous host calls. WebSocket uses `runtime.spawn` for background tasks. Match the pattern of the subsystem you're working in.
- **`Arc<Mutex<Option<T>>>` lazy init** — all major subsystems (audio, gpu, rtc, ws) are `None` until first use. Always use an `ensure_*(state)` helper before locking and calling methods.
- **Frame fuel** — each `on_frame` call gets 50M instructions (`FRAME_FUEL_LIMIT` in `runtime.rs`). Host functions themselves don't count against fuel, but they can't call back into the guest.
