# Oxide SDK

Guest-side SDK for building WebAssembly applications that run inside the
[Oxide browser](https://github.com/niklabh/oxide) — a binary-first browser
that fetches and executes `.wasm` modules instead of HTML/JavaScript.

This crate provides safe Rust wrappers around the raw host-imported functions
exposed by the `"oxide"` import module. It has **zero dependencies**.

## Quick Start

Add `oxide-sdk` to your guest crate:

```toml
[package]
name = "my-oxide-app"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
oxide-sdk = "0.1"
```

Write your app:

```rust
use oxide_sdk::*;

#[no_mangle]
pub extern "C" fn start_app() {
    log("Hello from Oxide!");
    canvas_clear(30, 30, 46, 255);
    canvas_text(20.0, 40.0, 28.0, 255, 255, 255, "Welcome to Oxide");
}
```

Build for wasm and open in the Oxide browser:

```bash
rustup target add wasm32-unknown-unknown
cargo build --target wasm32-unknown-unknown --release
```

## Available APIs

| Category | Functions |
|----------|-----------|
| **Canvas** | `canvas_clear`, `canvas_rect`, `canvas_circle`, `canvas_text`, `canvas_line`, `canvas_image`, `canvas_dimensions` |
| **UI Widgets** | `ui_button`, `ui_checkbox`, `ui_slider`, `ui_text_input` (immediate-mode) |
| **Console** | `log`, `warn`, `error` |
| **Input** | `mouse_position`, `mouse_button_down/clicked`, `key_down/pressed`, `scroll_delta`, `modifiers` |
| **Storage** | `storage_set/get/remove` (session), `kv_store_set/get/delete` (persistent) |
| **Networking** | `fetch`, `http_get`, `http_post`, `http_put`, `http_delete` |
| **Navigation** | `navigate`, `push_state`, `replace_state`, `get_url`, `history_back/forward` |
| **Crypto** | `hash_sha256`, `base64_encode/decode` |
| **Clipboard** | `clipboard_read`, `clipboard_write` |
| **Time / Random** | `time_now_ms`, `random_u64`, `random_f64` |
| **Dynamic loading** | `load_module` |

## License

Apache-2.0
