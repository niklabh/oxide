# Oxide Browser — Developer Documentation

## What is Oxide?

Oxide is a **binary-first browser** that fetches and executes `.wasm` (WebAssembly) modules instead of HTML/JavaScript. Guest applications run in a secure, sandboxed environment with zero access to the host filesystem, environment variables, or arbitrary network sockets.

The browser provides a set of **capability APIs** that guest modules can import to interact with the host — drawing on a canvas, reading/writing local storage, accessing the clipboard, and more.

---

## Architecture

```
┌──────────────────────────────────────────────────┐
│                   Oxide Browser                  │
│  ┌──────────┐  ┌────────────┐  ┌──────────────┐  │
│  │  URL Bar │  │   Canvas   │  │   Console    │  │
│  └────┬─────┘  └──────┬─────┘  └──────┬───────┘  │
│       │               │               │          │
│  ┌────▼───────────────▼───────────────▼───────┐  │
│  │              Host Runtime                  │  │
│  │  wasmtime engine + sandbox policy          │  │
│  │  fuel limit: 500M  │  memory: 16MB max     │  │
│  └────────────────────┬───────────────────────┘  │
│                       │                          │
│  ┌────────────────────▼───────────────────────┐  │
│  │          Capability Provider               │  │
│  │  "oxide" import module                     │  │
│  │  canvas, console, storage, clipboard,      │  │
│  │  fetch, images, crypto, base64, protobuf,  │  │
│  │  dynamic module loading                    │  │
│  └────────────────────┬───────────────────────┘  │
│                       │                          │
│  ┌────────────────────▼───────────────────────┐  │
│  │           Guest .wasm Module               │  │
│  │  exports: start_app()                      │  │
│  │  imports: oxide::*                         │  │
│  └────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────┘
```

---

## Building & Running the Browser

### Prerequisites

- Rust toolchain (1.75+): https://rustup.rs
- The `wasm32-unknown-unknown` target (for building guest apps):

```bash
rustup target add wasm32-unknown-unknown
```

### Build the browser

```bash
cd oxide
cargo build --release -p oxide-browser
```

### Run the browser

```bash
cargo run -p oxide-browser
```

This opens the Oxide browser window with a URL bar, canvas area, and console panel.

---

## Creating a Guest Application (WASM Website)

Guest applications are Rust libraries compiled to `wasm32-unknown-unknown` that export a single entry point: `start_app()`.

### Step 1: Create a new project

```bash
cargo new --lib my-oxide-app
cd my-oxide-app
```

### Step 2: Configure Cargo.toml

```toml
[package]
name = "my-oxide-app"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
oxide-sdk = { path = "../oxide/oxide-sdk" }
# Or, if published:
# oxide-sdk = "0.1"
```

The `crate-type = ["cdylib"]` is essential — it tells Cargo to produce a `.wasm` file suitable for dynamic loading.

### Step 3: Write your app

```rust
// src/lib.rs
use oxide_sdk::*;

#[no_mangle]
pub extern "C" fn start_app() {
    log("App started!");

    // Clear the canvas with a dark background
    canvas_clear(30, 30, 46, 255);

    // Draw a title
    canvas_text(20.0, 30.0, 28.0, 255, 255, 255, "My Oxide App");

    // Draw some shapes
    canvas_rect(20.0, 80.0, 200.0, 100.0, 80, 120, 200, 255);
    canvas_circle(400.0, 300.0, 60.0, 200, 100, 150, 255);
    canvas_line(20.0, 200.0, 600.0, 200.0, 100, 100, 100, 2.0);

    log("Rendering complete.");
}
```

### Step 4: Build

```bash
cargo build --target wasm32-unknown-unknown --release
```

The output `.wasm` file will be at:
```
target/wasm32-unknown-unknown/release/my_oxide_app.wasm
```

### Step 5: Run in Oxide

- Open the Oxide browser
- Click **"Open File"** and select your `.wasm` file, OR
- Serve it over HTTP and enter the URL in the address bar

---

## Browser API Reference

All APIs are available through the `oxide-sdk` crate. Import them with `use oxide_sdk::*;`.

### Console

| Function | Signature | Description |
|---|---|---|
| `log` | `fn log(msg: &str)` | Print a message to the browser console |
| `warn` | `fn warn(msg: &str)` | Print a warning (yellow) to the console |
| `error` | `fn error(msg: &str)` | Print an error (red) to the console |

```rust
log("This is informational");
warn("This is a warning");
error("Something went wrong!");
```

### Canvas

The canvas is the main rendering surface. Coordinates start at (0, 0) in the top-left.

| Function | Signature | Description |
|---|---|---|
| `canvas_clear` | `fn canvas_clear(r: u8, g: u8, b: u8, a: u8)` | Clear canvas with solid color |
| `canvas_rect` | `fn canvas_rect(x, y, w, h: f32, r, g, b, a: u8)` | Draw filled rectangle |
| `canvas_circle` | `fn canvas_circle(cx, cy, radius: f32, r, g, b, a: u8)` | Draw filled circle |
| `canvas_text` | `fn canvas_text(x, y, size: f32, r, g, b: u8, text: &str)` | Draw text |
| `canvas_line` | `fn canvas_line(x1, y1, x2, y2: f32, r, g, b: u8, thickness: f32)` | Draw a line |
| `canvas_dimensions` | `fn canvas_dimensions() -> (u32, u32)` | Get canvas (width, height) |

```rust
canvas_clear(0, 0, 0, 255);                                // black background
canvas_rect(10.0, 10.0, 100.0, 50.0, 255, 0, 0, 255);     // red rectangle
canvas_circle(200.0, 200.0, 30.0, 0, 255, 0, 255);         // green circle
canvas_text(10.0, 80.0, 16.0, 255, 255, 255, "Hello!");    // white text
canvas_line(0.0, 0.0, 100.0, 100.0, 255, 255, 0, 2.0);    // yellow diagonal line

let (w, h) = canvas_dimensions();
log(&format!("Canvas is {}x{}", w, h));
```

### Geolocation

| Function | Signature | Description |
|---|---|---|
| `get_location` | `fn get_location() -> String` | Returns mock GPS coordinates as `"lat,lon"` |

```rust
let coords = get_location();  // "37.7749,-122.4194"
let parts: Vec<&str> = coords.split(',').collect();
let lat = parts[0];
let lon = parts[1];
```

> **Note:** This returns mock data in the PoC. A production version would request real GPS permission.

### File Upload

| Function | Signature | Description |
|---|---|---|
| `upload_file` | `fn upload_file() -> Option<UploadedFile>` | Opens native file picker, returns file content |

The `UploadedFile` struct:
```rust
pub struct UploadedFile {
    pub name: String,      // filename
    pub data: Vec<u8>,     // raw file bytes (max 1MB)
}
```

```rust
if let Some(file) = upload_file() {
    log(&format!("Selected: {} ({} bytes)", file.name, file.data.len()));
}
```

### Local Storage

Key-value storage scoped to the current session. Data does not persist across browser restarts in the PoC.

| Function | Signature | Description |
|---|---|---|
| `storage_set` | `fn storage_set(key: &str, value: &str)` | Store a value |
| `storage_get` | `fn storage_get(key: &str) -> String` | Retrieve a value (empty if missing) |
| `storage_remove` | `fn storage_remove(key: &str)` | Delete a key |

```rust
storage_set("username", "alice");
let name = storage_get("username");  // "alice"
storage_remove("username");
let name = storage_get("username");  // ""
```

### Clipboard

| Function | Signature | Description |
|---|---|---|
| `clipboard_write` | `fn clipboard_write(text: &str)` | Copy text to system clipboard |
| `clipboard_read` | `fn clipboard_read() -> String` | Read text from system clipboard |

```rust
clipboard_write("Copied from Oxide!");
let text = clipboard_read();
```

### Time

| Function | Signature | Description |
|---|---|---|
| `time_now_ms` | `fn time_now_ms() -> u64` | Current time in ms since UNIX epoch |

```rust
let start = time_now_ms();
// ... do work ...
let elapsed = time_now_ms() - start;
log(&format!("Took {}ms", elapsed));
```

### Random

| Function | Signature | Description |
|---|---|---|
| `random_u64` | `fn random_u64() -> u64` | Random 64-bit unsigned integer |
| `random_f64` | `fn random_f64() -> f64` | Random float in [0.0, 1.0) |

```rust
let dice = (random_u64() % 6) + 1;
let probability = random_f64();
```

### Notifications

| Function | Signature | Description |
|---|---|---|
| `notify` | `fn notify(title: &str, body: &str)` | Display a notification in the console |

```rust
notify("Download Complete", "Your file has been saved.");
```

### HTTP Fetch

The fetch API lets guest modules make HTTP requests. The host mediates all network access — the guest never opens raw sockets. **Protocol Buffers is the native data format** — use `fetch_post_proto` for the idiomatic path.

| Function | Signature | Description |
|---|---|---|
| `fetch` | `fn fetch(method, url, content_type, body) -> Result<FetchResponse, i64>` | Full HTTP request |
| `fetch_get` | `fn fetch_get(url: &str) -> Result<FetchResponse, i64>` | HTTP GET |
| `fetch_post` | `fn fetch_post(url, content_type, body) -> Result<FetchResponse, i64>` | HTTP POST |
| `fetch_post_proto` | `fn fetch_post_proto(url, msg: &ProtoEncoder) -> Result<FetchResponse, i64>` | POST with protobuf body |
| `fetch_put` | `fn fetch_put(url, content_type, body) -> Result<FetchResponse, i64>` | HTTP PUT |
| `fetch_delete` | `fn fetch_delete(url: &str) -> Result<FetchResponse, i64>` | HTTP DELETE |

The `FetchResponse` struct:
```rust
pub struct FetchResponse {
    pub status: u32,        // HTTP status code
    pub body: Vec<u8>,      // response body bytes
}
impl FetchResponse {
    pub fn text(&self) -> String;  // body as UTF-8
}
```

```rust
// Simple GET
let resp = fetch_get("https://api.example.com/data").unwrap();
log(&format!("Status: {}, Body: {}", resp.status, resp.text()));

// POST with protobuf (native format)
use oxide_sdk::proto::ProtoEncoder;
let msg = ProtoEncoder::new()
    .string(1, "alice")
    .uint64(2, 42);
let resp = fetch_post_proto("https://api.example.com/users", &msg).unwrap();

// POST with JSON
let json = r#"{"name":"alice"}"#;
let resp = fetch_post("https://api.example.com/users", "application/json", json.as_bytes()).unwrap();
```

### Dynamic Module Loading

Guest modules can fetch and execute other `.wasm` modules at runtime — analogous to a `<script src="...">` tag in traditional browsers. The child module shares the same canvas, console, and storage context.

| Function | Signature | Description |
|---|---|---|
| `load_module` | `fn load_module(url: &str) -> i32` | Fetch and run another .wasm module (0 = success) |

```rust
let result = load_module("https://cdn.example.com/widget.wasm");
if result != 0 {
    error(&format!("Failed to load module: error code {}", result));
}
```

### Canvas Image

Display decoded images (PNG, JPEG, GIF, WebP) on the canvas. Image bytes can come from `fetch`, `upload_file`, or be embedded in the binary.

| Function | Signature | Description |
|---|---|---|
| `canvas_image` | `fn canvas_image(x, y, w, h: f32, data: &[u8])` | Draw a decoded image at position/size |

```rust
// Fetch an image from a URL and display it
let resp = fetch_get("https://example.com/logo.png").unwrap();
canvas_image(20.0, 100.0, 200.0, 150.0, &resp.body);

// Display an uploaded file as an image
if let Some(file) = upload_file() {
    canvas_image(0.0, 0.0, 400.0, 300.0, &file.data);
}
```

### Protobuf (Native Data Format)

Oxide uses Protocol Buffers wire format as its native serialisation layer. The `oxide_sdk::proto` module provides a zero-dependency encoder/decoder that produces bytes compatible with any protobuf implementation.

```rust
use oxide_sdk::proto::{ProtoEncoder, ProtoDecoder};

// Encode a message
let msg = ProtoEncoder::new()
    .string(1, "alice")           // field 1: string
    .uint64(2, 42)                // field 2: varint
    .bool(3, true)                // field 3: bool
    .double(4, 3.14)              // field 4: double
    .bytes(5, &[0xCA, 0xFE]);    // field 5: raw bytes
let data = msg.finish();

// Decode a message
let mut decoder = ProtoDecoder::new(&data);
while let Some(field) = decoder.next() {
    match field.number {
        1 => log(&format!("name  = {}", field.as_str())),
        2 => log(&format!("age   = {}", field.as_u64())),
        3 => log(&format!("admin = {}", field.as_bool())),
        4 => log(&format!("score = {}", field.as_f64())),
        5 => log(&format!("blob  = {:?}", field.as_bytes())),
        _ => {}
    }
}

// Nested messages
let address = ProtoEncoder::new()
    .string(1, "123 Main St")
    .string(2, "Springfield");
let user = ProtoEncoder::new()
    .string(1, "alice")
    .message(2, &address);   // embed sub-message
```

The encoder supports all protobuf wire types: `uint32`, `uint64`, `int32`, `int64`, `sint32`, `sint64`, `bool`, `string`, `bytes`, `float`, `double`, `fixed32`, `fixed64`, `sfixed32`, `sfixed64`, and nested `message`.

### SHA-256 Hashing

| Function | Signature | Description |
|---|---|---|
| `hash_sha256` | `fn hash_sha256(data: &[u8]) -> [u8; 32]` | SHA-256 hash (raw bytes) |
| `hash_sha256_hex` | `fn hash_sha256_hex(data: &[u8]) -> String` | SHA-256 hash (hex string) |

```rust
let hash = hash_sha256(b"hello world");
let hex = hash_sha256_hex(b"hello world");
log(&format!("SHA-256: {}", hex));
```

### Base64

| Function | Signature | Description |
|---|---|---|
| `base64_encode` | `fn base64_encode(data: &[u8]) -> String` | Encode bytes to base64 |
| `base64_decode` | `fn base64_decode(encoded: &str) -> Vec<u8>` | Decode base64 to bytes |

```rust
let encoded = base64_encode(b"Hello, Oxide!");
let decoded = base64_decode(&encoded);
assert_eq!(decoded, b"Hello, Oxide!");
```

---

## Security Model

### Sandbox Guarantees

Every guest `.wasm` module runs under strict constraints:

| Constraint | Value | Purpose |
|---|---|---|
| **Filesystem access** | None | Guest cannot read/write host files |
| **Environment variables** | None | Guest cannot read host env vars |
| **Network sockets** | None | Guest cannot open arbitrary connections |
| **Memory limit** | 16 MB (256 pages) | Prevents memory exhaustion attacks |
| **Fuel limit** | 500M instructions | Prevents infinite loops / DoS |

### How it works

1. **No WASI**: The runtime does *not* link WASI modules. The guest has zero implicit access to system resources.
2. **Bounded memory**: Linear memory is created with a hard upper bound. Any allocation beyond this causes a trap.
3. **Fuel metering**: Each WebAssembly instruction consumes fuel. When fuel runs out, execution halts with a clear error message.
4. **Capability-based access**: The only way a guest can interact with the outside world is through the explicitly provided `oxide::*` host functions.

### Mediated I/O

Several APIs grant controlled access to external resources while maintaining the sandbox:

- **File upload**: `upload_file()` opens a native OS file picker. The guest never gets filesystem access; the *host* mediates the interaction and only passes the selected file's name and content bytes.
- **HTTP fetch**: `fetch()` lets the guest make HTTP requests, but all network access is proxied through the host. The guest cannot open raw sockets. The host can enforce allowlists, rate limits, or other policies.
- **Dynamic loading**: `load_module()` fetches and runs another `.wasm` module. The child inherits the same canvas/console/storage but gets its own memory and fuel budget, preventing escape from the sandbox.

---

## Project Structure

```
oxide/
├── Cargo.toml                    # Workspace root
├── DOCS.md                       # This file
├── oxide-browser/                # The host browser application
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs               # Entry point, sets up GUI
│       ├── engine.rs             # WasmEngine, SandboxPolicy, fuel/memory
│       ├── runtime.rs            # BrowserHost, fetch_and_run, module execution
│       ├── capabilities.rs       # Host functions (the "oxide" import module)
│       └── ui.rs                 # egui-based UI (URL bar, canvas, console)
├── oxide-sdk/                    # Guest SDK
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                # Safe wrappers around host FFI imports
│       └── proto.rs              # Protobuf wire-format encoder/decoder
└── examples/
    └── hello-oxide/              # Example guest application
        ├── Cargo.toml
        └── src/
            └── lib.rs
```

---

## Building the Example Guest App

```bash
# From the workspace root
cargo build --target wasm32-unknown-unknown --release -p hello-oxide

# The output .wasm file:
ls target/wasm32-unknown-unknown/release/hello_oxide.wasm
```

Then open it in the browser:
```bash
cargo run -p oxide-browser
# Click "Open File" → select hello_oxide.wasm
```

---

## Serving WASM Files Over HTTP

To test URL-based loading, serve your `.wasm` file over HTTP:

```bash
# Using Python's built-in server
cd target/wasm32-unknown-unknown/release/
python3 -m http.server 8080

# Then in Oxide, navigate to:
# http://localhost:8080/hello_oxide.wasm
```

Or use any static file server (nginx, caddy, etc.).

---

## Extending the Browser

### Adding a new host function

1. **Define it in `capabilities.rs`**: Add a `linker.func_wrap(...)` call in `register_host_functions`.
2. **Add the FFI declaration in `oxide-sdk/src/lib.rs`**: Add the raw `extern "C"` import and a safe wrapper function.
3. **Document it**: Update this file with the new API.

### Example: Adding a `api_set_title` capability

In `capabilities.rs`:
```rust
linker.func_wrap(
    "oxide",
    "api_set_title",
    |caller: Caller<'_, HostState>, ptr: u32, len: u32| {
        let mem = caller.data().memory.expect("memory not set");
        let title = read_guest_string(&mem, &caller, ptr, len)
            .unwrap_or_default();
        // Store the title somewhere accessible to the UI...
    },
)?;
```

In `oxide-sdk/src/lib.rs`:
```rust
#[link(wasm_import_module = "oxide")]
extern "C" {
    #[link_name = "api_set_title"]
    fn _api_set_title(ptr: u32, len: u32);
}

pub fn set_title(title: &str) {
    unsafe { _api_set_title(title.as_ptr() as u32, title.len() as u32) }
}
```

---

## Guest Module Contract

Every `.wasm` module loaded by Oxide must satisfy:

1. **Export `start_app`**: A function with signature `extern "C" fn()`. This is the entry point called by the browser.
2. **Import from `"oxide"` module**: All host capabilities are provided under the `"oxide"` import namespace.
3. **Use host-provided memory**: The browser provides a `Memory` export under `oxide::memory`. The SDK handles this transparently.

### Minimal valid guest (without the SDK)

If you don't want to use the SDK, you can write raw imports:

```rust
#[link(wasm_import_module = "oxide")]
extern "C" {
    fn api_log(ptr: u32, len: u32);
    fn api_canvas_clear(r: u32, g: u32, b: u32, a: u32);
}

#[no_mangle]
pub extern "C" fn start_app() {
    let msg = "Hello from raw WASM!";
    unsafe {
        api_log(msg.as_ptr() as u32, msg.len() as u32);
        api_canvas_clear(20, 20, 40, 255);
    }
}
```

Compile with:
```bash
cargo build --target wasm32-unknown-unknown --release
```

---

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| "module must export `start_app`" | Missing entry point | Add `#[no_mangle] pub extern "C" fn start_app()` |
| "fuel limit exceeded" | Infinite loop or very long computation | Optimize your code or reduce work per frame |
| "failed to compile wasm module" | Invalid `.wasm` binary | Ensure you compiled with `--target wasm32-unknown-unknown` |
| "guest string out of bounds" | Buffer too small for host data | Increase buffer sizes in your guest code |
| "network request failed" | URL unreachable or CORS | Ensure the server is running and accessible |
| Blank canvas | No draw commands issued | Call `canvas_clear()` and draw something in `start_app()` |
