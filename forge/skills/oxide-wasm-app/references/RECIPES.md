# Oxide Forge — Recipes

Copy-pasteable snippets for common tasks. Each recipe is a full
`on_frame`-compatible fragment; assume the preamble from `PATTERNS.md §1`.

---

## Recipe 1 — Minimal counter

```rust
use oxide_sdk::*;

struct App { count: i32 }
static mut STATE: Option<App> = None;
fn state() -> &'static mut App { unsafe { STATE.as_mut().unwrap() } }

#[no_mangle] pub extern "C" fn start_app() {
    unsafe { STATE = Some(App { count: 0 }); }
}

#[no_mangle] pub extern "C" fn on_frame(_dt: u32) {
    canvas_clear(18, 18, 26, 255);
    canvas_text(20.0, 20.0, 24.0, 220, 220, 255, 255, "Counter");
    if ui_button(1, 20.0, 70.0, 80.0, 32.0, "-") { state().count -= 1; }
    if ui_button(2, 110.0, 70.0, 80.0, 32.0, "+") { state().count += 1; }
    canvas_text(200.0, 78.0, 20.0, 160, 220, 160, 255,
        &format!("{}", state().count));
}
```

---

## Recipe 2 — Drag a circle with the mouse

```rust
struct App { cx: f32, cy: f32, dragging: bool }
static mut STATE: Option<App> = None;
fn state() -> &'static mut App { unsafe { STATE.as_mut().unwrap() } }

#[no_mangle] pub extern "C" fn start_app() {
    unsafe { STATE = Some(App { cx: 200.0, cy: 200.0, dragging: false }); }
}

#[no_mangle] pub extern "C" fn on_frame(_dt: u32) {
    canvas_clear(18, 18, 26, 255);
    let (mx, my) = mouse_position();
    let s = state();
    let r = 30.0;
    let over = (mx - s.cx).hypot(my - s.cy) < r;
    if over && mouse_button_clicked(0) { s.dragging = true; }
    if !mouse_button_down(0) { s.dragging = false; }
    if s.dragging { s.cx = mx; s.cy = my; }
    canvas_circle(s.cx, s.cy, r, 180, 120, 255, 255);
}
```

---

## Recipe 3 — Stream tokens from an LLM (OpenAI-compatible SSE)

Assumes the endpoint emits `data: {...}` SSE lines.

```rust
struct App { handle: u32, out: String, prompt: String }
static mut STATE: Option<App> = None;
fn state() -> &'static mut App { unsafe { STATE.as_mut().unwrap() } }

#[no_mangle] pub extern "C" fn start_app() {
    unsafe { STATE = Some(App {
        handle: 0, out: String::new(), prompt: String::new(),
    }); }
}

fn kick(prompt: &str) {
    let s = state();
    s.out.clear();
    let body = format!(
        r#"{{"model":"gpt-4o-mini","stream":true,"messages":[{{"role":"user","content":{}}}]}}"#,
        serde_json_ish::escape(prompt)
    );
    s.handle = fetch_begin("POST",
        "https://api.openai.com/v1/chat/completions",
        "application/json",
        body.as_bytes());
}

#[no_mangle] pub extern "C" fn on_frame(_dt: u32) {
    canvas_clear(18, 18, 26, 255);
    let s = state();

    s.prompt = ui_text_input(1, 20.0, 20.0, 500.0, &s.prompt);
    if ui_button(2, 530.0, 20.0, 80.0, 32.0, "Send") && !s.prompt.is_empty() {
        kick(&s.prompt.clone());
    }

    if s.handle != 0 {
        loop {
            match fetch_recv(s.handle) {
                FetchChunk::Data(bytes) => {
                    // Very small SSE parser — accumulate and look for
                    // "data: { ... \"content\":\"X\" ... }" lines.
                    s.out.push_str(&String::from_utf8_lossy(&bytes));
                }
                FetchChunk::Pending => break,
                FetchChunk::End | FetchChunk::Error => {
                    fetch_remove(s.handle); s.handle = 0; break;
                }
            }
        }
    }

    let body = &s.out;
    canvas_text(20.0, 80.0, 14.0, 220, 220, 255, 255, body);
}

// Inline escape helper — keep guest zero-dep.
mod serde_json_ish {
    pub fn escape(s: &str) -> String {
        let mut out = String::from("\"");
        for c in s.chars() {
            match c {
                '"'  => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }
}
```

(For production use, wire in a proper SSE parser that splits on `\n\n`
and strips the `data:` prefix before rendering.)

---

## Recipe 4 — WebSocket echo

```rust
struct App { ws: u32, input: String, log: Vec<String> }
static mut STATE: Option<App> = None;
fn state() -> &'static mut App { unsafe { STATE.as_mut().unwrap() } }

#[no_mangle] pub extern "C" fn start_app() {
    let ws = ws_connect("ws://localhost:9001");
    unsafe { STATE = Some(App { ws, input: String::new(), log: vec![] }); }
}

#[no_mangle] pub extern "C" fn on_frame(_dt: u32) {
    canvas_clear(18, 18, 26, 255);
    let s = state();

    while let Some(m) = ws_recv(s.ws) { s.log.push(m.text()); }

    let connected = ws_ready_state(s.ws) == WS_OPEN;
    let color = if connected { (160, 220, 160) } else { (240, 120, 120) };
    canvas_text(20.0, 20.0, 14.0, color.0, color.1, color.2, 255,
        if connected { "connected" } else { "…" });

    s.input = ui_text_input(1, 20.0, 50.0, 400.0, &s.input);
    if ui_button(2, 430.0, 50.0, 80.0, 32.0, "Send") && connected {
        let _ = ws_send_text(s.ws, &s.input);
        s.input.clear();
    }

    let mut y = 100.0;
    for line in s.log.iter().rev().take(20) {
        canvas_text(20.0, y, 13.0, 220, 220, 255, 255, line);
        y += 18.0;
    }
}
```

---

## Recipe 5 — WebRTC DataChannel echo (bootstrap)

```rust
const STUN: &str = "stun:stun.l.google.com:19302";

struct App { peer: u32, channel: u32, log: Vec<String> }
static mut STATE: Option<App> = None;
fn state() -> &'static mut App { unsafe { STATE.as_mut().unwrap() } }

#[no_mangle] pub extern "C" fn start_app() {
    let _ = rtc_signal_connect("wss://signaling.example.com");
    let _ = rtc_signal_join_room("demo-room");
    let peer = rtc_create_peer(STUN);
    let channel = rtc_create_data_channel(peer, "main", true);
    unsafe { STATE = Some(App { peer, channel, log: vec![] }); }
}

#[no_mangle] pub extern "C" fn on_frame(_dt: u32) {
    canvas_clear(18, 18, 26, 255);
    let s = state();

    while let Some(m) = rtc_recv(s.peer, 0) {
        s.log.push(String::from_utf8_lossy(&m.data).into_owned());
    }
    if let Some(ice) = rtc_poll_ice_candidate(s.peer) {
        let _ = rtc_signal_send(ice.as_bytes());
    }
    if let Some(sig) = rtc_signal_recv() {
        // parse JSON {type, sdp|candidate}; set remote or add ICE
        log(&format!("signal: {} bytes", sig.len()));
    }

    if ui_button(1, 20.0, 20.0, 120.0, 32.0, "ping") {
        let _ = rtc_send_text(s.peer, s.channel, "ping");
    }
    let mut y = 80.0;
    for line in s.log.iter().rev().take(20) {
        canvas_text(20.0, y, 13.0, 220, 220, 255, 255, line);
        y += 18.0;
    }
}
```

---

## Recipe 6 — Draw-on-canvas (whiteboard brush)

```rust
struct App { strokes: Vec<Vec<(f32, f32)>>, current: Option<Vec<(f32, f32)>> }
static mut STATE: Option<App> = None;
fn state() -> &'static mut App { unsafe { STATE.as_mut().unwrap() } }

#[no_mangle] pub extern "C" fn start_app() {
    unsafe { STATE = Some(App { strokes: vec![], current: None }); }
}

#[no_mangle] pub extern "C" fn on_frame(_dt: u32) {
    canvas_clear(18, 18, 26, 255);
    let (mx, my) = mouse_position();
    let s = state();

    if mouse_button_down(0) {
        let stroke = s.current.get_or_insert_with(Vec::new);
        if stroke.last().map_or(true, |&(lx, ly)| (mx - lx).hypot(my - ly) > 1.5) {
            stroke.push((mx, my));
        }
    } else if let Some(done) = s.current.take() {
        s.strokes.push(done);
    }

    for stroke in s.strokes.iter().chain(s.current.iter()) {
        for pair in stroke.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            canvas_line(a.0, a.1, b.0, b.1, 220, 220, 255, 255, 2.5);
        }
    }
}
```

---

## Recipe 7 — Persistent note (kv_store)

```rust
struct App { text: String }
static mut STATE: Option<App> = None;
fn state() -> &'static mut App { unsafe { STATE.as_mut().unwrap() } }

const KEY: &str = "oxide.forge.demo.note";

#[no_mangle] pub extern "C" fn start_app() {
    let text = kv_store_get_str(KEY).unwrap_or_default();
    unsafe { STATE = Some(App { text }); }
}

#[no_mangle] pub extern "C" fn on_frame(_dt: u32) {
    canvas_clear(18, 18, 26, 255);
    let s = state();
    let new_text = ui_text_input(1, 20.0, 20.0, 600.0, &s.text);
    if new_text != s.text {
        s.text = new_text;
        kv_store_set_str(KEY, &s.text);
    }
    canvas_text(20.0, 70.0, 13.0, 140, 140, 160, 255,
        &format!("{} chars (autosaved)", s.text.len()));
}
```

---

## Recipe 8 — GPU compute (sum of squares)

```rust
const WGSL: &str = r#"
@group(0) @binding(0) var<storage, read_write> data: array<f32>;

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i < arrayLength(&data)) {
        data[i] = data[i] * data[i];
    }
}
"#;

#[no_mangle] pub extern "C" fn start_app() {
    let shader = gpu_create_shader(WGSL);
    let pipeline = gpu_create_compute_pipeline(shader, "cs_main");
    let buf = gpu_create_buffer(1024 * 4, gpu_usage::STORAGE);
    let data: Vec<f32> = (0..1024).map(|i| i as f32).collect();
    let bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 4)
    };
    gpu_write_buffer(buf, 0, bytes);
    gpu_dispatch_compute(pipeline, 16, 1, 1);
    log("dispatched");
}
```

---

## Recipe 9 — Audio tone (from bytes)

(Audio playback takes encoded bytes. For a live tone generator use
MIDI + an external soft-synth, or preload short WAV fragments.)

```rust
const BEEP: &[u8] = include_bytes!("beep.wav"); // short WAV committed beside lib.rs

#[no_mangle] pub extern "C" fn start_app() { /* … */ }

#[no_mangle] pub extern "C" fn on_frame(_dt: u32) {
    canvas_clear(18, 18, 26, 255);
    if ui_button(1, 20.0, 20.0, 80.0, 32.0, "Beep") {
        audio_play(BEEP);
    }
}
```

---

## Recipe 10 — Multi-pane layout

```rust
#[no_mangle] pub extern "C" fn on_frame(_dt: u32) {
    let (w, h) = canvas_dimensions();
    let (wf, hf) = (w as f32, h as f32);
    canvas_clear(18, 18, 26, 255);

    let sidebar_w = 200.0;
    canvas_rect(0.0, 0.0, sidebar_w, hf, 24, 24, 36, 255);
    canvas_text(20.0, 20.0, 16.0, 220, 220, 255, 255, "Sidebar");

    let main_x = sidebar_w + 1.0;
    let main_w = wf - main_x;
    canvas_rect(main_x, 0.0, main_w, hf, 18, 18, 26, 255);
    canvas_text(main_x + 20.0, 20.0, 16.0, 220, 220, 255, 255, "Main");
}
```

---

## Recipe 11 — Confirmation dialog (modal-style)

Immediate-mode modals: draw the overlay last so it sits on top.

```rust
pub fn confirm(show: &mut bool, title: &str) -> Option<bool> {
    if !*show { return None; }
    let (w, h) = canvas_dimensions();
    canvas_rect(0.0, 0.0, w as f32, h as f32, 0, 0, 0, 150);
    let (mx, my) = ((w as f32 - 320.0) * 0.5, (h as f32 - 140.0) * 0.5);
    canvas_rounded_rect(mx, my, 320.0, 140.0, 8.0, 30, 30, 46, 255);
    canvas_text(mx + 20.0, my + 20.0, 16.0, 220, 220, 255, 255, title);
    if ui_button(900, mx + 20.0,  my + 90.0, 120.0, 32.0, "Cancel") {
        *show = false; return Some(false);
    }
    if ui_button(901, mx + 180.0, my + 90.0, 120.0, 32.0, "Confirm") {
        *show = false; return Some(true);
    }
    None
}
```

---

## Recipe 12 — Tiny JSON helpers (no serde)

When serde is forbidden, hand-roll small helpers. Works for known-shape
payloads:

```rust
fn json_str(payload: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\":", key);
    let start = payload.find(&needle)?;
    let rest = &payload[start + needle.len()..];
    let rest = rest.trim_start();
    if !rest.starts_with('"') { return None; }
    let rest = &rest[1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}
```

For bigger payloads, parse with `ProtoDecoder` if you control the
schema, or keep the data in a list-of-lines plain-text format.
