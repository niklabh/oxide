# Oxide SDK — Capabilities Catalog

Exhaustive list of every public symbol in `oxide-sdk` (v0.6). Every generated
guest app imports these from `oxide_sdk::*`. Nothing outside this list is
available to guest code — if a capability is not listed, it does not exist
and must be proposed as a host-side addition (see `PATTERNS.md §Capability
proposals`).

The host side registers these under the `"oxide"` wasm import module. Guest
code never links WASI and never receives filesystem, env-var, or raw-socket
access.

---

## Quick Start

### 1. `Cargo.toml`

```toml
[package]
name = "my-oxide-app"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
oxide-sdk = "0.6"
```

Build:

```bash
cargo build --target wasm32-unknown-unknown --release
```

### 2. Minimum `start_app` + `on_frame` shell

```rust
use oxide_sdk::*;

#[no_mangle]
pub extern "C" fn start_app() {
    log("app started");
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    canvas_clear(30, 30, 46, 255);
    canvas_text(20.0, 40.0, 28.0, 255, 255, 255, 255, "Hello Oxide");
}
```

### 3. Optional entry points

- `on_timer(callback_id: u32)` — fired for `set_timeout` / `set_interval` /
  `request_animation_frame`.
- `on_event(callback_id: u32)` — fired for built-in events (`resize`,
  `focus`, `touch_*`, `gamepad_*`, `drop_files`, `visibility_change`,
  `online`, `offline`) and for custom events registered via
  `on_event()` / `emit_event()`.

### 4. The `static mut STATE` pattern

Guest apps are single-threaded. State lives in a module-level `static mut`:

```rust
use oxide_sdk::*;

struct App { counter: u32 }
static mut STATE: Option<App> = None;
fn state() -> &'static mut App { unsafe { STATE.as_mut().unwrap() } }

#[no_mangle]
pub extern "C" fn start_app() {
    unsafe { STATE = Some(App { counter: 0 }); }
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    state().counter += 1;
    canvas_clear(20, 20, 30, 255);
    canvas_text(10.0, 20.0, 16.0, 255, 255, 255, 255,
        &format!("tick {}", state().counter));
}
```

---

## Canvas — low-level drawing primitives

- `canvas_clear(r: u8, g: u8, b: u8, a: u8)` — clear with solid RGBA.
- `canvas_rect(x: f32, y: f32, w: f32, h: f32, r: u8, g: u8, b: u8, a: u8)` — filled rectangle.
- `canvas_circle(cx: f32, cy: f32, radius: f32, r: u8, g: u8, b: u8, a: u8)` — filled circle.
- `canvas_text(x: f32, y: f32, size: f32, r: u8, g: u8, b: u8, a: u8, text: &str)` — text with RGBA.
- `canvas_line(x1: f32, y1: f32, x2: f32, y2: f32, r: u8, g: u8, b: u8, a: u8, thickness: f32)` — stroked line.
- `canvas_dimensions() -> (u32, u32)` — `(width, height)`.
- `canvas_image(x: f32, y: f32, w: f32, h: f32, data: &[u8])` — draw encoded image bytes (PNG, JPEG, GIF, WebP).

## Canvas — extended shapes

- `canvas_rounded_rect(x, y, w, h, radius, r, g, b, a)` — uniform corner radius.
- `canvas_arc(cx, cy, radius, start_angle, end_angle, r, g, b, a, thickness)` — circular arc (radians, clockwise from +X).
- `canvas_bezier(x1, y1, cp1x, cp1y, cp2x, cp2y, x2, y2, r, g, b, a, thickness)` — cubic Bézier stroke.
- `canvas_gradient(x, y, w, h, kind, ax, ay, bx, by, stops)` — gradient fill; `stops: &[(f32, u8, u8, u8, u8)]`.
- `GRADIENT_LINEAR: u32 = 0`, `GRADIENT_RADIAL: u32 = 1`.

## Canvas — state stack

- `canvas_save()`, `canvas_restore()` — push / pop transform + clip + opacity.
- `canvas_transform(a, b, c, d, tx, ty)` — 2D affine (column-major 3×2).
- `canvas_clip(x, y, w, h)` — intersect clip with AABB.
- `canvas_opacity(alpha: f32)` — layer alpha (0.0–1.0, multiplicative).

## Drawing (high-level, `draw::` module)

Structs: `Color { r, g, b, a: u8 }`, `Point2D { x, y: f32 }`,
`Rect { x, y, w, h: f32 }`, `GradientStop { offset: f32, color: Color }`,
`Canvas` (zero-cost facade).

`Color`:

- `Color::rgb(r, g, b)`, `Color::rgba(r, g, b, a)`, `Color::hex(0xRRGGBB)`,
  `Color::with_alpha(self, a)`.
- Constants: `WHITE`, `BLACK`, `RED`, `GREEN`, `BLUE`, `YELLOW`, `CYAN`,
  `MAGENTA`, `TRANSPARENT`.

`Point2D`: `Point2D::new(x, y)`, `Point2D::ZERO`.

`Rect`: `Rect::new(x, y, w, h)`, `Rect::from_point_size(p, w, h)`,
`rect.contains(px, py)`, `rect.origin()`, `rect.center()`.

`Canvas` methods (all `&self`): `clear`, `fill_rect`, `fill_rounded_rect`,
`fill_circle`, `arc`, `bezier`, `text`, `line`, `image`, `linear_gradient`,
`radial_gradient`, `save`, `restore`, `translate`, `rotate`, `scale`,
`transform`, `clip`, `set_opacity`, `dimensions`, `width`, `height`.

## Widgets — immediate-mode UI (call from `on_frame` only)

- `ui_button(id, x, y, w, h, label) -> bool` — returns true on click.
- `ui_checkbox(id, x, y, label, initial) -> bool` — current state.
- `ui_slider(id, x, y, w, min, max, initial) -> f32` — current value.
- `ui_text_input(id, x, y, w, initial) -> String` — current text.

Widget `id` must be stable across frames and unique within the app.

## GPU — WebGPU-style

- `gpu_create_buffer(size: u64, usage: u32) -> u32` — `usage` = `gpu_usage::*` bitmask.
- `gpu_create_texture(width, height) -> u32` — RGBA8.
- `gpu_create_shader(source: &str) -> u32` — WGSL.
- `gpu_create_pipeline(shader, vertex_entry, fragment_entry) -> u32`.
- `gpu_create_compute_pipeline(shader, entry_point) -> u32`.
- `gpu_write_buffer(handle, offset, data) -> bool`.
- `gpu_draw(pipeline, target_texture, vertex_count, instance_count) -> bool`.
- `gpu_dispatch_compute(pipeline, x, y, z) -> bool`.
- `gpu_destroy_buffer(handle) -> bool`, `gpu_destroy_texture(handle) -> bool`.

Usage flags (module `gpu_usage`): `VERTEX = 0x0020`, `INDEX = 0x0010`,
`UNIFORM = 0x0040`, `STORAGE = 0x0080`.

## Console

- `log(msg)`, `warn(msg)`, `error(msg)`.

## HTTP — blocking fetch

```rust
pub struct FetchResponse { pub status: u32, pub body: Vec<u8> }
impl FetchResponse { pub fn text(&self) -> String; }
```

- `fetch(method, url, content_type, body) -> Result<FetchResponse, i64>`.
- `fetch_get(url)`, `fetch_post(url, ct, body)`, `fetch_post_proto(url, enc)`,
  `fetch_put(url, ct, body)`, `fetch_delete(url)` — same return.

## HTTP — streaming (non-blocking)

States: `FETCH_PENDING=0`, `FETCH_STREAMING=1`, `FETCH_DONE=2`,
`FETCH_ERROR=3`, `FETCH_ABORTED=4`.

```rust
pub enum FetchChunk { Data(Vec<u8>), Pending, End, Error }
```

- `fetch_begin(method, url, ct, body) -> u32` — handle > 0 or 0 on fail.
- `fetch_begin_get(url) -> u32`.
- `fetch_state(handle) -> u32`, `fetch_status(handle) -> u32`.
- `fetch_recv(handle) -> FetchChunk` — poll next chunk.
- `fetch_recv_into(handle, buf) -> i64` — zero-alloc; `-1` pending, `-2` EOF,
  `-3` error, `-4` unknown.
- `fetch_error(handle) -> Option<String>`.
- `fetch_abort(handle) -> bool`, `fetch_remove(handle)`.

## Protobuf (module `proto`)

- `proto::ProtoEncoder` — builder; chainable `.uint64/int32/sint32/bool/
  bytes/string/message/fixed64/…(field, value)` → `.finish() -> Vec<u8>`.
- `proto::ProtoDecoder<'a>` — iterator; `.next() -> Option<ProtoField<'a>>`.
- `proto::ProtoField<'a>` — `.as_u64/i64/u32/i32/sint64/sint32/bool/f64/
  f32/bytes/str/message()`.

## Storage — ephemeral (sandboxed local storage)

- `storage_set(key, value: &str)`, `storage_get(key) -> String`,
  `storage_remove(key)`.

## Storage — persistent (on-disk KV)

- `kv_store_set(key, value: &[u8]) -> bool`, `kv_store_set_str(key, value)`.
- `kv_store_get(key) -> Option<Vec<u8>>`, `kv_store_get_str(key) -> Option<String>`.
- `kv_store_delete(key) -> bool`.

## Audio

```rust
pub enum AudioFormat { Unknown=0, Wav=1, Mp3=2, Ogg=3, Flac=4 }
```

- `audio_play(data)`, `audio_detect_format(data) -> AudioFormat`,
  `audio_play_with_format(data, format)`, `audio_play_url(url)`.
- `audio_pause()`, `audio_resume()`, `audio_stop()`.
- `audio_set_volume(level: f32)` (0.0–2.0), `audio_get_volume() -> f32`.
- `audio_is_playing() -> bool`, `audio_position() -> u64`,
  `audio_seek(ms) -> i32`, `audio_duration() -> u64`.
- `audio_set_loop(enabled)`, `audio_last_url_content_type() -> String`.
- Channels: `audio_channel_play(channel, data)`,
  `audio_channel_play_with_format(channel, data, format)`,
  `audio_channel_stop(channel)`, `audio_channel_set_volume(channel, level)`.

## Video (FFmpeg-backed)

```rust
pub enum VideoFormat { Unknown=0, Mp4=1, Webm=2, Av1=3 }
```

- `video_detect_format(data) -> VideoFormat`, `video_load(data)`,
  `video_load_with_format(data, format)`, `video_load_url(url)`.
- `video_last_url_content_type() -> String`.
- HLS: `video_hls_variant_count()`, `video_hls_variant_url(i)`,
  `video_hls_open_variant(i)`.
- `video_play()`, `video_pause()`, `video_stop()`.
- `video_seek(ms)`, `video_position() -> u64`, `video_duration() -> u64`.
- `video_render(x, y, w, h)` — blit current frame into canvas rect.
- `video_set_volume(level)` (0.0–2.0), `video_get_volume() -> f32`.
- `video_set_loop(enabled)`, `video_set_pip(enabled)`.
- Subtitles: `subtitle_load_srt(text)`, `subtitle_load_vtt(text)`,
  `subtitle_clear()`.

## Media capture

- `camera_open() -> i32` (0 ok; `-1` denied, `-2` none, `-3` open failed),
  `camera_close()`, `camera_capture_frame(out: &mut [u8]) -> u32`,
  `camera_frame_dimensions() -> (u32, u32)`.
- `microphone_open() -> i32`, `microphone_close()`,
  `microphone_sample_rate() -> u32`,
  `microphone_read_samples(out: &mut [f32]) -> u32`.
- `screen_capture(out: &mut [u8]) -> Result<usize, i32>`,
  `screen_capture_dimensions() -> (u32, u32)`.
- `media_pipeline_stats() -> (u64, u32)` — `(camera_frames, mic_ring_depth)`.

## WebRTC

State: `RTC_STATE_NEW=0`, `_CONNECTING=1`, `_CONNECTED=2`, `_DISCONNECTED=3`,
`_FAILED=4`, `_CLOSED=5`. Tracks: `RTC_TRACK_AUDIO=0`, `RTC_TRACK_VIDEO=1`.

```rust
pub struct RtcMessage { pub channel_id: u32, pub is_binary: bool, pub data: Vec<u8> }
pub struct RtcDataChannelInfo { pub channel_id: u32, pub label: String }
pub struct RtcTrackInfo { pub kind: u32, pub id: String, pub stream_id: String }
```

- `rtc_create_peer(stun_servers) -> u32`, `rtc_close_peer(peer)`.
- `rtc_create_offer(peer) -> Result<String, i32>`, `rtc_create_answer(peer)`.
- `rtc_set_local_description(peer, sdp, is_offer) -> i32`,
  `rtc_set_remote_description(…)`,
  `rtc_add_ice_candidate(peer, json) -> i32`.
- `rtc_connection_state(peer) -> u32`,
  `rtc_poll_ice_candidate(peer) -> Option<String>`.
- `rtc_create_data_channel(peer, label, ordered) -> u32`,
  `rtc_send_text/send_binary/send`, `rtc_recv(peer, channel) -> Option<RtcMessage>`
  (channel `0` = any).
- `rtc_poll_data_channel(peer) -> Option<RtcDataChannelInfo>`.
- Tracks: `rtc_add_track(peer, kind) -> u32`,
  `rtc_poll_track(peer) -> Option<RtcTrackInfo>`.
- Signaling: `rtc_signal_connect(url) -> bool`,
  `rtc_signal_join_room(room) -> i32`,
  `rtc_signal_send(data) -> i32`, `rtc_signal_recv() -> Option<Vec<u8>>`.

## WebSocket

State: `WS_CONNECTING=0`, `WS_OPEN=1`, `WS_CLOSING=2`, `WS_CLOSED=3`.

```rust
pub struct WsMessage { pub is_binary: bool, pub data: Vec<u8> }
impl WsMessage { pub fn text(&self) -> String; }
```

- `ws_connect(url) -> u32` (0 = fail), `ws_ready_state(id) -> u32`.
- `ws_send_text(id, text) -> i32`, `ws_send_binary(id, data) -> i32`.
- `ws_recv(id) -> Option<WsMessage>`, `ws_close(id) -> i32`, `ws_remove(id)`.

## MIDI

- `midi_input_count() -> u32`, `midi_output_count() -> u32`.
- `midi_input_name(i) -> String`, `midi_output_name(i) -> String`.
- `midi_open_input(i) -> u32`, `midi_open_output(i) -> u32`.
- `midi_send(handle, data) -> i32`, `midi_recv(handle) -> Option<Vec<u8>>`,
  `midi_close(handle)`.

## Input — polling

- `mouse_position() -> (f32, f32)`, `mouse_button_down(b) -> bool`,
  `mouse_button_clicked(b) -> bool`. Buttons: 0 left, 1 right, 2 middle.
- `key_down(k) -> bool`, `key_pressed(k) -> bool`.
- `scroll_delta() -> (f32, f32)`.
- `modifiers() -> u32` (bit 0 Shift, 1 Ctrl/Cmd, 2 Alt),
  `shift_held() -> bool`, `ctrl_held() -> bool`, `alt_held() -> bool`.

Key codes (`KEY_*: u32`): `A=0 … Z=25`, `0=26 … 9=35`, `ENTER=36`,
`ESCAPE=37`, `TAB=38`, `BACKSPACE=39`, `DELETE=40`, `SPACE=41`, `UP=42`,
`DOWN=43`, `LEFT=44`, `RIGHT=45`, `HOME=46`, `END=47`, `PAGE_UP=48`,
`PAGE_DOWN=49`.

## Clipboard

- `clipboard_write(text)`, `clipboard_read() -> String`.

## Time & timers

- `time_now_ms() -> u64`.
- `set_timeout(cb_id, delay_ms) -> u32`, `set_interval(cb_id, ms) -> u32`,
  `clear_timer(id)`.
- `request_animation_frame(cb_id) -> u32`, `cancel_animation_frame(id)`.

Callbacks fire via exported `on_timer(callback_id: u32)`.

## Events

Fires exported `on_event(callback_id: u32)`.

- `on_event(event_type, callback_id) -> u32`, `off_event(listener_id) -> bool`.
- `emit_event(event_type, data)`.
- Inside the callback: `event_type() -> String`,
  `event_data(out: &mut [u8]) -> usize`,
  `event_data_into() -> Vec<u8>`.

Built-in types: `resize`, `focus`, `blur`, `visibility_change`, `online`,
`offline`, `touch_start/move/end`, `gamepad_connected/button/axis`,
`drop_files`.

## Random

- `random_u64() -> u64`, `random_f64() -> f64` in `[0.0, 1.0)`.

## Crypto / hash

- `hash_sha256(data) -> [u8; 32]`, `hash_sha256_hex(data) -> String`.

## Base64

- `base64_encode(data) -> String`, `base64_decode(s) -> Vec<u8>`.

## Navigation

- `navigate(url) -> i32`, `get_url() -> String`.
- `push_state(state, title, url)`, `replace_state(state, title, url)`.
- `get_state() -> Option<Vec<u8>>`, `history_length() -> u32`,
  `history_back() -> bool`, `history_forward() -> bool`.

## Hyperlinks

- `register_hyperlink(x, y, w, h, url) -> i32`, `clear_hyperlinks()`.

## URL utilities

- `url_resolve(base, relative) -> Option<String>`,
  `url_encode(s) -> String`, `url_decode(s) -> String`.

## Files — handle-based

```rust
pub struct UploadedFile { pub name: String, pub data: Vec<u8> }
pub struct FileMetadata { pub name: String, pub size: u64, pub mime: String,
                          pub modified_ms: u64, pub is_dir: bool }
pub struct FolderEntry { pub name: String, pub size: u64, pub is_dir: bool,
                         pub handle: u32 }
```

- `upload_file() -> Option<UploadedFile>` — 1 MiB native picker.
- `file_pick(title, filters, multiple) -> Vec<u32>` — filters
  `"png,jpg,gif"`; empty = cancelled.
- `folder_pick(title) -> Option<u32>`.
- `folder_entries(handle) -> Vec<FolderEntry>`.
- `file_read(handle) -> Option<Vec<u8>>` — up to 64 MiB.
- `file_read_range(handle, offset, len) -> Option<Vec<u8>>`.
- `file_metadata(handle) -> Option<FileMetadata>`.

## Geolocation

- `get_location() -> String` — `"lat,lon"` (host-configurable mock).

## Notifications

- `notify(title, body)` — rendered in browser console.

## Dynamic module loading

- `load_module(url) -> i32` — fetch and run another `.wasm`; shares canvas,
  console, and storage with the current app.
