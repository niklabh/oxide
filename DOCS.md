# Oxide Browser — Developer Documentation

## What is Oxide?

Oxide is a **binary-first browser** that fetches and executes `.wasm` (WebAssembly) modules instead of HTML/JavaScript. Guest applications run in a secure, sandboxed environment with zero access to the host filesystem, environment variables, or arbitrary network sockets.

The browser provides a set of **capability APIs** that guest modules can import to interact with the host — drawing on a GPU-accelerated canvas, reading/writing storage, making HTTP requests, playing audio/video, capturing media, WebRTC peer-to-peer communication, GPU compute, and more.

The desktop shell is built on [GPUI](https://www.gpui.rs/) (Zed's GPU-accelerated UI framework). Guest draw commands map directly onto GPUI primitives — filled quads, GPU-shaped text, vector paths, and image textures — so your canvas output gets full hardware acceleration.

---

## Architecture

```
┌──────────────────────────────────────────────────────┐
│                   Oxide Browser                      │
│  ┌──────────┐  ┌────────────┐  ┌──────────────┐      │
│  │  URL Bar │  │   Canvas   │  │   Console    │      │
│  └────┬─────┘  └──────┬─────┘  └──────┬───────┘      │
│       │               │               │              │
│  ┌────▼───────────────▼───────────────▼───────┐      │
│  │              Host Runtime                  │      │
│  │  wasmtime engine + sandbox policy          │      │
│  │  fuel limit: 500M  │  memory: 16MB max     │      │
│  └────────────────────┬───────────────────────┘      │
│                       │                              │
│  ┌────────────────────▼───────────────────────┐      │
│  │          Capability Provider               │      │
│  │  "oxide" import module                     │      │
│  │  canvas, console, storage, clipboard,      │      │
│  │  fetch, audio, video, media capture,       │      │
│  │  WebRTC, crypto, timers, navigation,       │      │
│  │  widgets, input, hyperlinks, GPU           │      │
│  └────────────────────┬───────────────────────┘      │
│                       │                              │
│  ┌────────────────────▼───────────────────────┐      │
│  │           Guest .wasm Module               │      │
│  │  exports: start_app(), on_frame(dt_ms)     │      │
│  │  imports: oxide::*                         │      │
│  └────────────────────────────────────────────┘      │
└──────────────────────────────────────────────────────┘
```

### Rendering Model (GPUI)

Oxide uses an **immediate-mode rendering** approach backed by GPUI:

1. Each frame, the guest issues draw commands (`canvas_clear`, `canvas_rect`, `canvas_text`, …) which push `DrawCommand` variants into the host state.
2. Widget calls (`ui_button`, `ui_slider`, …) push `WidgetCommand` entries.
3. The GPUI shell paints both queues each frame using GPU-accelerated primitives: `paint_quad` for rectangles, `paint_path` for circles and lines, `paint_image` for images, and GPU text shaping for text.
4. Images are decoded once and cached as GPUI `RenderImage` textures (same path for video frames).
5. Widget interactions (clicks, drags, text edits) flow back through shared state, which the guest reads on the next frame call.

No layout engine. No style cascade. No DOM. The guest decides exactly where every pixel goes.

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

## Creating a Guest Application

Guest applications are Rust libraries compiled to `wasm32-unknown-unknown`.

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
```

The `crate-type = ["cdylib"]` is essential — it tells Cargo to produce a `.wasm` file suitable for dynamic loading.

### Step 3: Write your app

#### Static app (one-shot render)

```rust
use oxide_sdk::*;

#[no_mangle]
pub extern "C" fn start_app() {
    log("App started!");
    canvas_clear(30, 30, 46, 255);
    canvas_text(20.0, 30.0, 28.0, 255, 255, 255, 255, "My Oxide App");
    canvas_rect(20.0, 80.0, 200.0, 100.0, 80, 120, 200, 255);
    canvas_circle(400.0, 300.0, 60.0, 200, 100, 150, 255);
    canvas_line(20.0, 200.0, 600.0, 200.0, 100, 100, 100, 255, 2.0);
}
```

#### Interactive app (frame loop with widgets)

```rust
use oxide_sdk::*;

static mut COUNTER: i32 = 0;

#[no_mangle]
pub extern "C" fn start_app() {
    log("Interactive app started");
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    canvas_clear(30, 30, 46, 255);
    canvas_text(20.0, 30.0, 24.0, 255, 255, 255, 255, "Counter App");

    if ui_button(1, 20.0, 70.0, 120.0, 30.0, "Increment") {
        unsafe { COUNTER += 1; }
    }
    if ui_button(2, 150.0, 70.0, 80.0, 30.0, "Reset") {
        unsafe { COUNTER = 0; }
    }

    let count = unsafe { COUNTER };
    canvas_text(20.0, 120.0, 18.0, 160, 220, 160, 255, &format!("Count: {count}"));

    let (mx, my) = mouse_position();
    canvas_circle(mx, my, 10.0, 255, 100, 100, 180);
}
```

#### Using the high-level drawing API

```rust
use oxide_sdk::draw::*;

#[no_mangle]
pub extern "C" fn start_app() {
    let c = Canvas::new();
    c.clear(Color::hex(0x1e1e2e));
    c.fill_rect(Rect::new(10.0, 10.0, 200.0, 100.0), Color::rgb(80, 120, 200));
    c.fill_circle(Point2D::new(300.0, 200.0), 50.0, Color::RED);
    c.text("Hello!", Point2D::new(20.0, 30.0), 24.0, Color::WHITE);
    c.line(Point2D::ZERO, Point2D::new(400.0, 300.0), 2.0, Color::YELLOW);
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
- Click **"Open"** and select your `.wasm` file, OR
- Serve it over HTTP and enter the URL in the address bar

---

## API Reference

All APIs are available through the `oxide-sdk` crate. Import them with `use oxide_sdk::*;`.

### High-Level Drawing API (`oxide_sdk::draw`)

The `draw` module provides GPUI-inspired ergonomic types that wrap the low-level canvas functions.

#### Types

| Type | Description |
|------|-------------|
| `Color` | sRGB + alpha color with named constants and constructors |
| `Point2D` | 2D point in canvas coordinates |
| `Rect` | Axis-aligned rectangle (x, y, w, h) with hit-testing |
| `Canvas` | Zero-cost facade for drawing operations |

#### Color

```rust
use oxide_sdk::draw::Color;

let c = Color::rgb(255, 128, 0);        // opaque orange
let c = Color::rgba(255, 128, 0, 128);  // semi-transparent
let c = Color::hex(0xFF8000);           // from hex
let c = Color::RED.with_alpha(128);     // modify alpha

// Named constants: WHITE, BLACK, RED, GREEN, BLUE, YELLOW, CYAN, MAGENTA, TRANSPARENT
```

#### Canvas

```rust
use oxide_sdk::draw::*;

let c = Canvas::new();
c.clear(Color::hex(0x1e1e2e));
c.fill_rect(Rect::new(10.0, 10.0, 200.0, 100.0), Color::BLUE);
c.fill_circle(Point2D::new(300.0, 200.0), 50.0, Color::RED);
c.text("Hello!", Point2D::new(20.0, 30.0), 24.0, Color::WHITE);
c.line(Point2D::ZERO, Point2D::new(100.0, 100.0), 2.0, Color::YELLOW);
c.image(Rect::new(0.0, 0.0, 400.0, 300.0), &image_bytes);

let (w, h) = c.dimensions();
```

### Low-Level Canvas API

| Function | Signature | Description |
|----------|-----------|-------------|
| `canvas_clear` | `fn(r, g, b, a: u8)` | Clear canvas with solid RGBA color |
| `canvas_rect` | `fn(x, y, w, h: f32, r, g, b, a: u8)` | Draw filled rectangle |
| `canvas_circle` | `fn(cx, cy, radius: f32, r, g, b, a: u8)` | Draw filled circle |
| `canvas_text` | `fn(x, y, size: f32, r, g, b, a: u8, text: &str)` | Draw text |
| `canvas_line` | `fn(x1, y1, x2, y2: f32, r, g, b, a: u8, thickness: f32)` | Draw a line |
| `canvas_image` | `fn(x, y, w, h: f32, data: &[u8])` | Draw encoded image (PNG/JPEG/GIF/WebP) |
| `canvas_dimensions` | `fn() -> (u32, u32)` | Get canvas `(width, height)` in pixels |

```rust
canvas_clear(30, 30, 46, 255);
canvas_rect(10.0, 10.0, 100.0, 50.0, 255, 0, 0, 255);
canvas_circle(200.0, 200.0, 30.0, 0, 255, 0, 255);
canvas_text(10.0, 80.0, 16.0, 255, 255, 255, 255, "Hello!");
canvas_line(0.0, 0.0, 100.0, 100.0, 255, 255, 0, 255, 2.0);

let (w, h) = canvas_dimensions();
```

### Console

| Function | Signature | Description |
|----------|-----------|-------------|
| `log` | `fn(msg: &str)` | Informational message |
| `warn` | `fn(msg: &str)` | Warning (yellow) |
| `error` | `fn(msg: &str)` | Error (red) |

### Input Polling

All input functions return per-frame data. Call them from `on_frame()`.

| Function | Signature | Description |
|----------|-----------|-------------|
| `mouse_position` | `fn() -> (f32, f32)` | Mouse `(x, y)` in canvas coordinates |
| `mouse_button_down` | `fn(button: u32) -> bool` | Button currently held (0=left, 1=right, 2=middle) |
| `mouse_button_clicked` | `fn(button: u32) -> bool` | Button clicked this frame |
| `key_down` | `fn(key: u32) -> bool` | Key currently held (see `KEY_*` constants) |
| `key_pressed` | `fn(key: u32) -> bool` | Key pressed this frame |
| `scroll_delta` | `fn() -> (f32, f32)` | Scroll wheel `(dx, dy)` this frame |
| `modifiers` | `fn() -> u32` | Bitmask: bit 0=Shift, 1=Ctrl, 2=Alt |
| `shift_held` | `fn() -> bool` | Shift modifier |
| `ctrl_held` | `fn() -> bool` | Ctrl/Cmd modifier |
| `alt_held` | `fn() -> bool` | Alt modifier |

#### Key Constants

```rust
// Letters: KEY_A (0) through KEY_Z (25)
// Digits: KEY_0 (26) through KEY_9 (35)
// Special: KEY_ENTER (36), KEY_ESCAPE (37), KEY_TAB (38), KEY_BACKSPACE (39),
//          KEY_DELETE (40), KEY_SPACE (41)
// Arrows: KEY_UP (42), KEY_DOWN (43), KEY_LEFT (44), KEY_RIGHT (45)
// Navigation: KEY_HOME (46), KEY_END (47), KEY_PAGE_UP (48), KEY_PAGE_DOWN (49)
```

### Interactive Widgets

Widgets are **immediate-mode**: call them every frame from `on_frame()`. They return the current value. The host renders them as native GPUI elements overlaid on the canvas.

| Function | Signature | Description |
|----------|-----------|-------------|
| `ui_button` | `fn(id, x, y, w, h: f32, label: &str) -> bool` | Button; returns `true` when clicked |
| `ui_checkbox` | `fn(id, x, y: f32, label: &str, initial: bool) -> bool` | Checkbox; returns checked state |
| `ui_slider` | `fn(id, x, y, w, min, max, initial: f32) -> f32` | Slider; returns current value |
| `ui_text_input` | `fn(id, x, y, w: f32, initial: &str) -> String` | Text field; returns current text |

```rust
if ui_button(1, 20.0, 100.0, 120.0, 28.0, "Click Me!") {
    log("Button was clicked!");
}

let dark_mode = ui_checkbox(10, 20.0, 140.0, "Dark mode", false);
let volume = ui_slider(20, 20.0, 180.0, 300.0, 0.0, 100.0, 50.0);
let name = ui_text_input(30, 20.0, 220.0, 300.0, "");
```

### HTTP Fetch

All network access is mediated by the host — the guest never opens raw sockets.

| Function | Signature | Description |
|----------|-----------|-------------|
| `fetch` | `fn(method, url, content_type, body) -> Result<FetchResponse, i64>` | Full HTTP request |
| `fetch_get` | `fn(url: &str) -> Result<FetchResponse, i64>` | GET shorthand |
| `fetch_post` | `fn(url, content_type, body) -> Result<FetchResponse, i64>` | POST |
| `fetch_post_proto` | `fn(url, msg: &ProtoEncoder) -> Result<FetchResponse, i64>` | POST with protobuf |
| `fetch_put` | `fn(url, content_type, body) -> Result<FetchResponse, i64>` | PUT |
| `fetch_delete` | `fn(url: &str) -> Result<FetchResponse, i64>` | DELETE |

```rust
let resp = fetch_get("https://api.example.com/data").unwrap();
log(&format!("Status: {}, Body: {}", resp.status, resp.text()));
```

### Protobuf (Native Data Format)

The `oxide_sdk::proto` module provides a zero-dependency encoder/decoder compatible with the Protocol Buffers wire format.

```rust
use oxide_sdk::proto::{ProtoEncoder, ProtoDecoder};

let msg = ProtoEncoder::new()
    .string(1, "alice")
    .uint64(2, 42)
    .bool(3, true);
let data = msg.finish();

let mut decoder = ProtoDecoder::new(&data);
while let Some(field) = decoder.next() {
    match field.number {
        1 => log(&format!("name = {}", field.as_str())),
        2 => log(&format!("age  = {}", field.as_u64())),
        _ => {}
    }
}
```

### Storage

#### Session storage (in-memory, cleared on restart)

| Function | Signature | Description |
|----------|-----------|-------------|
| `storage_set` | `fn(key: &str, value: &str)` | Store a value |
| `storage_get` | `fn(key: &str) -> String` | Retrieve (empty if missing) |
| `storage_remove` | `fn(key: &str)` | Delete a key |

#### Persistent KV store (on-disk, survives restarts)

| Function | Signature | Description |
|----------|-----------|-------------|
| `kv_store_set` | `fn(key: &str, value: &[u8]) -> bool` | Store bytes |
| `kv_store_set_str` | `fn(key: &str, value: &str) -> bool` | Store string |
| `kv_store_get` | `fn(key: &str) -> Option<Vec<u8>>` | Read bytes |
| `kv_store_get_str` | `fn(key: &str) -> Option<String>` | Read string |
| `kv_store_delete` | `fn(key: &str) -> bool` | Delete key |

### Audio Playback

| Function | Signature | Description |
|----------|-----------|-------------|
| `audio_play` | `fn(data: &[u8]) -> i32` | Play from encoded bytes (WAV/MP3/OGG/FLAC) |
| `audio_play_url` | `fn(url: &str) -> i32` | Fetch and play from URL |
| `audio_play_with_format` | `fn(data: &[u8], format: AudioFormat) -> i32` | Play with format hint |
| `audio_detect_format` | `fn(data: &[u8]) -> AudioFormat` | Sniff container from magic bytes |
| `audio_pause` / `audio_resume` / `audio_stop` | `fn()` | Playback control |
| `audio_set_volume` / `audio_get_volume` | `fn(f32)` / `fn() -> f32` | Volume (0.0–2.0) |
| `audio_is_playing` | `fn() -> bool` | Check playback state |
| `audio_position` / `audio_seek` / `audio_duration` | | Seek and position (ms) |
| `audio_set_loop` | `fn(enabled: bool)` | Enable/disable looping |
| `audio_channel_play` | `fn(channel: u32, data: &[u8]) -> i32` | Multi-channel playback |
| `audio_channel_stop` | `fn(channel: u32)` | Stop a channel |
| `audio_channel_set_volume` | `fn(channel: u32, level: f32)` | Per-channel volume |

```rust
// Play from URL
audio_play_url("https://example.com/song.mp3");

// Multi-channel (music + effects)
audio_channel_play(0, &music_bytes);   // background music
audio_channel_play(1, &sfx_bytes);     // sound effect
audio_channel_set_volume(0, 0.5);      // lower music volume
```

### Video Playback

Requires FFmpeg on the host for decoding.

| Function | Signature | Description |
|----------|-----------|-------------|
| `video_load` | `fn(data: &[u8]) -> i32` | Load from encoded bytes |
| `video_load_url` | `fn(url: &str) -> i32` | Load from URL (progressive or HLS) |
| `video_play` / `video_pause` / `video_stop` | `fn()` | Playback control |
| `video_render` | `fn(x, y, w, h: f32) -> i32` | Draw current frame to canvas |
| `video_seek` | `fn(position_ms: u64) -> i32` | Seek to position |
| `video_position` / `video_duration` | `fn() -> u64` | Position and duration (ms) |
| `video_set_volume` / `video_get_volume` | `fn(f32)` / `fn() -> f32` | Volume control |
| `video_set_loop` | `fn(enabled: bool)` | Enable looping |
| `video_set_pip` | `fn(enabled: bool)` | Picture-in-picture overlay |
| `video_hls_variant_count` / `video_hls_open_variant` | | HLS adaptive streaming |
| `subtitle_load_srt` / `subtitle_load_vtt` | `fn(text: &str) -> i32` | Load subtitles |
| `subtitle_clear` | `fn()` | Remove subtitles |

```rust
video_load_url("https://example.com/video.mp4");
video_play();

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let (w, h) = canvas_dimensions();
    video_render(0.0, 0.0, w as f32, h as f32);
}
```

### Media Capture

Permission dialogs are shown by the host before granting access.

| Function | Signature | Description |
|----------|-----------|-------------|
| `camera_open` | `fn() -> i32` | Open default camera (0=success) |
| `camera_close` | `fn()` | Stop camera |
| `camera_capture_frame` | `fn(out: &mut [u8]) -> u32` | Capture RGBA8 frame |
| `camera_frame_dimensions` | `fn() -> (u32, u32)` | Frame size in pixels |
| `microphone_open` | `fn() -> i32` | Start mic capture (0=success) |
| `microphone_close` | `fn()` | Stop mic |
| `microphone_sample_rate` | `fn() -> u32` | Sample rate in Hz |
| `microphone_read_samples` | `fn(out: &mut [f32]) -> u32` | Read mono f32 samples |
| `screen_capture` | `fn(out: &mut [u8]) -> Result<usize, i32>` | Screenshot as RGBA8 |
| `screen_capture_dimensions` | `fn() -> (u32, u32)` | Screenshot dimensions |
| `media_pipeline_stats` | `fn() -> (u64, u32)` | Camera frames / mic depth |

```rust
if camera_open() == 0 {
    let mut buf = vec![0u8; 1920 * 1080 * 4];
    let bytes = camera_capture_frame(&mut buf);
    let (w, h) = camera_frame_dimensions();
    log(&format!("Captured {}x{} frame ({} bytes)", w, h, bytes));
}
```

### WebRTC / Real-Time Communication

The RTC API enables peer-to-peer communication for video calls, multiplayer games, collaborative tools, and decentralized messaging. Connections use the [webrtc-rs](https://github.com/webrtc-rs/webrtc) stack on the host; guests interact through handle-based APIs. Events (incoming messages, ICE candidates, state changes) are polled each frame rather than using callbacks.

#### Peer Connection

| Function | Signature | Description |
|----------|-----------|-------------|
| `rtc_create_peer` | `fn(stun_servers: &str) -> u32` | Create a peer connection (comma-separated STUN/TURN URLs, `""` for default) |
| `rtc_close_peer` | `fn(peer_id: u32) -> bool` | Close and release a peer |
| `rtc_connection_state` | `fn(peer_id: u32) -> u32` | Poll connection state (`RTC_STATE_*` constants) |

Connection state constants: `RTC_STATE_NEW` (0), `RTC_STATE_CONNECTING` (1), `RTC_STATE_CONNECTED` (2), `RTC_STATE_DISCONNECTED` (3), `RTC_STATE_FAILED` (4), `RTC_STATE_CLOSED` (5).

#### SDP Offer/Answer Exchange

| Function | Signature | Description |
|----------|-----------|-------------|
| `rtc_create_offer` | `fn(peer_id: u32) -> Result<String, i32>` | Generate SDP offer and set as local description |
| `rtc_create_answer` | `fn(peer_id: u32) -> Result<String, i32>` | Generate SDP answer (after setting remote offer) |
| `rtc_set_local_description` | `fn(peer_id: u32, sdp: &str, is_offer: bool) -> i32` | Set local SDP explicitly |
| `rtc_set_remote_description` | `fn(peer_id: u32, sdp: &str, is_offer: bool) -> i32` | Set remote SDP from the other peer |

#### ICE Candidates

| Function | Signature | Description |
|----------|-----------|-------------|
| `rtc_add_ice_candidate` | `fn(peer_id: u32, candidate_json: &str) -> i32` | Add a trickled ICE candidate (JSON) |
| `rtc_poll_ice_candidate` | `fn(peer_id: u32) -> Option<String>` | Poll for locally gathered ICE candidates |

#### Data Channels

| Function | Signature | Description |
|----------|-----------|-------------|
| `rtc_create_data_channel` | `fn(peer_id: u32, label: &str, ordered: bool) -> u32` | Create a data channel (ordered=TCP-like, unordered=UDP-like) |
| `rtc_send_text` | `fn(peer_id: u32, channel_id: u32, text: &str) -> i32` | Send UTF-8 text |
| `rtc_send_binary` | `fn(peer_id: u32, channel_id: u32, data: &[u8]) -> i32` | Send binary data |
| `rtc_send` | `fn(peer_id: u32, channel_id: u32, data: &[u8], is_binary: bool) -> i32` | Send with explicit mode |
| `rtc_recv` | `fn(peer_id: u32, channel_id: u32) -> Option<RtcMessage>` | Poll for incoming messages (channel_id=0 for any) |
| `rtc_poll_data_channel` | `fn(peer_id: u32) -> Option<RtcDataChannelInfo>` | Poll for remotely-created data channels |

`RtcMessage` has fields: `channel_id: u32`, `is_binary: bool`, `data: Vec<u8>`, and a `.text()` helper.

#### Media Tracks

| Function | Signature | Description |
|----------|-----------|-------------|
| `rtc_add_track` | `fn(peer_id: u32, kind: u32) -> u32` | Attach a media track (`RTC_TRACK_AUDIO`=0, `RTC_TRACK_VIDEO`=1) |
| `rtc_poll_track` | `fn(peer_id: u32) -> Option<RtcTrackInfo>` | Poll for remote media tracks added by the peer |

`RtcTrackInfo` has fields: `kind: u32`, `id: String`, `stream_id: String`.

#### Signaling

The built-in signaling client bootstraps connections via HTTP. Guests can also use the `fetch` API with any custom signaling server.

| Function | Signature | Description |
|----------|-----------|-------------|
| `rtc_signal_connect` | `fn(url: &str) -> bool` | Connect to a signaling server |
| `rtc_signal_join_room` | `fn(room: &str) -> i32` | Join a signaling room |
| `rtc_signal_send` | `fn(data: &[u8]) -> i32` | Send a signaling message |
| `rtc_signal_recv` | `fn() -> Option<Vec<u8>>` | Poll for incoming signaling messages |

#### Example: P2P Chat

```rust
use oxide_sdk::*;

static mut PEER: u32 = 0;
static mut CHANNEL: u32 = 0;
static mut GREETED: bool = false;

#[no_mangle]
pub extern "C" fn start_app() {
    let peer = rtc_create_peer("");
    unsafe { PEER = peer; }

    let ch = rtc_create_data_channel(peer, "chat", true);
    unsafe { CHANNEL = ch; }

    match rtc_create_offer(peer) {
        Ok(sdp) => log(&format!("Share this offer:\n{sdp}")),
        Err(e) => error(&format!("Offer failed: {e}")),
    }
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let (peer, ch) = unsafe { (PEER, CHANNEL) };

    if rtc_connection_state(peer) == RTC_STATE_CONNECTED {
        // Send a greeting once on connect
        if !unsafe { GREETED } {
            rtc_send_text(peer, ch, "Hello from Oxide!");
            unsafe { GREETED = true; }
        }

        while let Some(msg) = rtc_recv(peer, 0) {
            log(&format!("Received: {}", msg.text()));
        }
    }

    while let Some(candidate) = rtc_poll_ice_candidate(peer) {
        log(&format!("ICE: {candidate}"));
    }
}
```

### Timers

Timer callbacks fire via the guest-exported `on_timer(callback_id)` function.

| Function | Signature | Description |
|----------|-----------|-------------|
| `set_timeout` | `fn(callback_id: u32, delay_ms: u32) -> u32` | One-shot timer |
| `set_interval` | `fn(callback_id: u32, interval_ms: u32) -> u32` | Repeating timer |
| `clear_timer` | `fn(timer_id: u32)` | Cancel a timer |
| `time_now_ms` | `fn() -> u64` | Current UNIX timestamp in ms |

```rust
let timer = set_timeout(42, 1000); // fires on_timer(42) after 1 second

#[no_mangle]
pub extern "C" fn on_timer(callback_id: u32) {
    if callback_id == 42 {
        log("Timer fired!");
    }
}
```

### Navigation & History

| Function | Signature | Description |
|----------|-----------|-------------|
| `navigate` | `fn(url: &str) -> i32` | Navigate to a URL |
| `push_state` | `fn(state: &[u8], title: &str, url: &str)` | Push history entry |
| `replace_state` | `fn(state: &[u8], title: &str, url: &str)` | Replace current entry |
| `get_url` | `fn() -> String` | Current page URL |
| `get_state` | `fn() -> Option<Vec<u8>>` | Current history state bytes |
| `history_length` | `fn() -> u32` | Number of history entries |
| `history_back` / `history_forward` | `fn() -> bool` | Navigate history |

### Hyperlinks

| Function | Signature | Description |
|----------|-----------|-------------|
| `register_hyperlink` | `fn(x, y, w, h: f32, url: &str) -> i32` | Register clickable region |
| `clear_hyperlinks` | `fn()` | Remove all hyperlinks |

```rust
canvas_text(20.0, 100.0, 16.0, 100, 150, 255, 255, "Visit Example");
register_hyperlink(20.0, 100.0, 200.0, 20.0, "https://example.com/app.wasm");
```

### Crypto & Encoding

| Function | Signature | Description |
|----------|-----------|-------------|
| `hash_sha256` | `fn(data: &[u8]) -> [u8; 32]` | SHA-256 hash (raw) |
| `hash_sha256_hex` | `fn(data: &[u8]) -> String` | SHA-256 hash (hex) |
| `base64_encode` | `fn(data: &[u8]) -> String` | Encode to base64 |
| `base64_decode` | `fn(encoded: &str) -> Vec<u8>` | Decode from base64 |

### Clipboard

| Function | Signature | Description |
|----------|-----------|-------------|
| `clipboard_write` | `fn(text: &str)` | Copy to clipboard |
| `clipboard_read` | `fn() -> String` | Read from clipboard |

### Random

| Function | Signature | Description |
|----------|-----------|-------------|
| `random_u64` | `fn() -> u64` | Cryptographic random u64 |
| `random_f64` | `fn() -> f64` | Random float in [0.0, 1.0) |

### Other

| Function | Signature | Description |
|----------|-----------|-------------|
| `notify` | `fn(title: &str, body: &str)` | Show a notification |
| `upload_file` | `fn() -> Option<UploadedFile>` | Open native file picker |
| `get_location` | `fn() -> String` | Mock geolocation as `"lat,lon"` |
| `load_module` | `fn(url: &str) -> i32` | Dynamically load another `.wasm` |
| `url_resolve` | `fn(base: &str, rel: &str) -> Option<String>` | Resolve relative URL |
| `url_encode` / `url_decode` | `fn(input: &str) -> String` | Percent-encoding |

---

## Security Model

### Sandbox Guarantees

| Constraint | Value | Purpose |
|------------|-------|---------|
| **Filesystem access** | None | Guest cannot read/write host files |
| **Environment variables** | None | Guest cannot read host env vars |
| **Network sockets** | None | Guest cannot open arbitrary connections |
| **Memory limit** | 16 MB (256 pages) | Prevents memory exhaustion |
| **Fuel limit** | 500M instructions | Prevents infinite loops / DoS |

### How it works

1. **No WASI**: The runtime does *not* link WASI modules. The guest has zero implicit access to system resources.
2. **Bounded memory**: Linear memory is created with a hard upper bound. Any allocation beyond this causes a trap.
3. **Fuel metering**: Each WebAssembly instruction consumes fuel. When fuel runs out, execution halts with a clear error message.
4. **Capability-based access**: The only way a guest can interact with the outside world is through the explicitly provided `oxide::*` host functions.

### Mediated I/O

- **File upload**: `upload_file()` opens a native OS file picker. The guest never gets filesystem access; the host passes the selected file's name and content bytes.
- **HTTP fetch**: `fetch()` proxies through the host. The guest cannot open raw sockets.
- **Dynamic loading**: `load_module()` fetches and runs a child `.wasm` with isolated memory and fuel, preventing sandbox escape.

---

## Guest Module Contract

Every `.wasm` module loaded by Oxide must:

1. **Export `start_app`** — `extern "C" fn()` entry point, called once on load.
2. **Optionally export `on_frame`** — `extern "C" fn(dt_ms: u32)` for interactive apps with a render loop.
3. **Optionally export `on_timer`** — `extern "C" fn(callback_id: u32)` to receive timer callbacks.
4. **Import from `"oxide"` module** — All host capabilities are under this namespace.
5. **Compile as `cdylib`** — `crate-type = ["cdylib"]` in `Cargo.toml`.
6. **Target `wasm32-unknown-unknown`** — no WASI, pure capability-based I/O.

### Minimal valid guest (without the SDK)

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

---

## Project Structure

```
oxide/
├── Cargo.toml                    # Workspace root
├── DOCS.md                       # This file
├── oxide-browser/                # Host browser application
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs               # Entry point (BrowserHost + run_browser)
│       ├── engine.rs             # WasmEngine, SandboxPolicy, fuel/memory
│       ├── runtime.rs            # BrowserHost, fetch_and_run, module lifecycle
│       ├── capabilities.rs       # Host functions ("oxide" import module)
│       ├── ui.rs                 # GPUI shell (toolbar, canvas, console, tabs)
│       ├── navigation.rs         # History stack with back/forward
│       ├── bookmarks.rs          # Persistent bookmarks (sled)
│       ├── url.rs                # WHATWG URL parsing
│       ├── audio_format.rs       # Audio magic-byte sniffing
│       ├── video.rs              # FFmpeg video pipeline
│       ├── video_format.rs       # Video format sniffing
│       ├── subtitle.rs           # SRT/VTT subtitle parsing
│       ├── media_capture.rs      # Camera, mic, screen capture
│       ├── rtc.rs                # WebRTC peer connections, data channels, signaling
│       └── gpu.rs                # WebGPU-style GPU resource management
├── oxide-sdk/                    # Guest SDK (no dependencies)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                # Safe wrappers over host FFI imports
│       ├── draw.rs               # High-level drawing API (Color, Rect, Canvas)
│       └── proto.rs              # Zero-dependency protobuf codec
├── oxide-docs/                   # Rustdoc hub crate
└── examples/
    ├── hello-oxide/              # Interactive widget demo
    ├── audio-player/             # Audio playback example
    ├── video-player/             # Video playback example
    ├── timer-demo/               # Timer callbacks
    ├── media-capture/            # Camera/mic/screen capture
    ├── gpu-graphics-demo/        # GPU/WebGPU rendering demo
    ├── rtc-chat/                 # WebRTC peer-to-peer chat demo
    ├── index/                    # Demo hub (links to other examples)
    └── fullstack-notes/          # Full-stack example (Rust frontend + backend)
```

---

## Extending the Browser

### Adding a new host function

1. **Define it in `capabilities.rs`**: Add a `linker.func_wrap(...)` call in `register_host_functions`.
2. **Add the FFI declaration in `oxide-sdk/src/lib.rs`**: Add the raw `extern "C"` import and a safe wrapper function.
3. **Document it**: Update this file and the rustdoc comments.

### Example: Adding `api_set_title`

In `capabilities.rs`:
```rust
linker.func_wrap(
    "oxide",
    "api_set_title",
    |caller: Caller<'_, HostState>, ptr: u32, len: u32| {
        let mem = caller.data().memory.expect("memory not set");
        let title = read_guest_string(&mem, &caller, ptr, len).unwrap_or_default();
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

## Serving WASM Files Over HTTP

```bash
cd target/wasm32-unknown-unknown/release/
python3 -m http.server 8080
# Navigate to: http://localhost:8080/my_oxide_app.wasm
```

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| "module must export `start_app`" | Missing entry point | Add `#[no_mangle] pub extern "C" fn start_app()` |
| "fuel limit exceeded" | Infinite loop or very long computation | Optimize code or reduce work per frame |
| "failed to compile wasm module" | Invalid `.wasm` binary | Ensure `--target wasm32-unknown-unknown` |
| "guest string out of bounds" | Buffer too small | Increase buffer sizes in guest code |
| "network request failed" | URL unreachable | Ensure the server is running and accessible |
| Blank canvas | No draw commands | Call `canvas_clear()` + draw something |
| `canvas_dimensions()` returns (0,0) | Called before first frame | Call from `on_frame()` after the canvas is laid out |
| `shift_held()` always false | Modifier not synced | Ensure you're on oxide-browser ≥ 0.4.0 |
