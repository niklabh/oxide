# Oxide Forge — Idiomatic Patterns

Battle-tested patterns extracted from the existing Oxide examples. Every
generated app should use these by default; deviations need a reason.

---

## 1. State lives in a single `static mut`

One optional state struct, one helper. Panics at startup only (never in
`on_frame`).

```rust
struct App { count: u32, ws: u32 }
static mut STATE: Option<App> = None;
fn state() -> &'static mut App { unsafe { STATE.as_mut().unwrap() } }

#[no_mangle] pub extern "C" fn start_app() {
    unsafe { STATE = Some(App { count: 0, ws: 0 }); }
}
```

**Why not `once_cell::sync::Lazy`?** Extra dependency, slower first
access, no benefit in single-threaded wasm.

**Why not a `thread_local!`?** No threads in guest wasm; same reason.

---

## 2. Immediate-mode UI

Widgets are called *every frame* with a stable ID; the host remembers
interaction state. Don't store widget state yourself — just read the
return value.

```rust
pub extern "C" fn on_frame(_dt_ms: u32) {
    canvas_clear(18, 18, 26, 255);

    if ui_button(1, 20.0, 20.0, 120.0, 32.0, "Reset") {
        state().count = 0;
    }
    let volume = ui_slider(2, 20.0, 70.0, 200.0, 0.0, 1.0, 0.5);
    let on = ui_checkbox(3, 20.0, 110.0, "Mute", false);
    let name = ui_text_input(4, 20.0, 140.0, 240.0, "");
}
```

IDs must be unique within the app and **stable** across frames. Hard-code
them as integers or use an enum-to-u32 mapping.

---

## 3. Game loop with fixed accumulator (if you care about determinism)

```rust
const DT: f32 = 1.0 / 60.0;

struct App { accum: f32, world: World }

pub extern "C" fn on_frame(dt_ms: u32) {
    let s = state();
    s.accum += dt_ms as f32 / 1000.0;
    while s.accum >= DT {
        s.world.step(DT);
        s.accum -= DT;
    }
    draw(&s.world);
}
```

Cap `dt_ms` at ~100ms to prevent spiral-of-death after tab switches:

```rust
let dt = (dt_ms as f32 / 1000.0).min(0.1);
```

---

## 4. Streaming fetch (LLM / SSE / progressive)

Dispatch once, poll each frame. Always `fetch_remove` on completion.

```rust
struct App { handle: u32, buffer: Vec<u8>, done: bool }

fn start_stream(url: &str) {
    let s = state();
    s.handle = fetch_begin_get(url);
    s.buffer.clear();
    s.done = false;
}

pub extern "C" fn on_frame(_dt_ms: u32) {
    let s = state();
    if s.handle != 0 && !s.done {
        loop {
            match fetch_recv(s.handle) {
                FetchChunk::Data(bytes) => s.buffer.extend(bytes),
                FetchChunk::Pending => break,
                FetchChunk::End | FetchChunk::Error => {
                    s.done = true;
                    fetch_remove(s.handle);
                    s.handle = 0;
                    break;
                }
            }
        }
    }
    // draw s.buffer
}
```

---

## 5. WebSocket chat

```rust
struct App { ws: u32, connected: bool, log: Vec<String> }

pub extern "C" fn start_app() {
    unsafe { STATE = Some(App { ws: ws_connect("ws://localhost:9001"),
                                  connected: false, log: vec![] }); }
}

pub extern "C" fn on_frame(_dt: u32) {
    let s = state();
    match ws_ready_state(s.ws) {
        WS_OPEN => {
            s.connected = true;
            while let Some(msg) = ws_recv(s.ws) {
                s.log.push(msg.text());
            }
        }
        WS_CLOSED => s.connected = false,
        _ => {}
    }
    // draw s.log
}
```

Guard every `ws_send_*` with `ws_ready_state == WS_OPEN`.

---

## 6. WebRTC peer-to-peer

1. `rtc_signal_connect(signaling_url)` → `rtc_signal_join_room(name)`.
2. `rtc_create_peer("")` → handle.
3. Poll `rtc_signal_recv` for offers/answers/ICE.
4. `rtc_create_offer(peer)` → send via signal.
5. `rtc_create_data_channel(peer, "main", true)` → channel.
6. Poll `rtc_recv(peer, 0)` for messages, `rtc_poll_ice_candidate(peer)`
   for locally gathered ICE, and `rtc_connection_state(peer)` for status.

```rust
match rtc_connection_state(s.peer) {
    RTC_STATE_CONNECTED => { /* send and receive */ }
    RTC_STATE_FAILED | RTC_STATE_CLOSED => { s.peer = 0; }
    _ => {}
}
```

---

## 7. Timer callbacks

```rust
static mut TIMER_ID: u32 = 0;

pub extern "C" fn start_app() {
    unsafe { TIMER_ID = set_interval(42, 1000); }
}

#[no_mangle]
pub extern "C" fn on_timer(cb: u32) {
    if cb == 42 {
        log("tick");
    }
}
```

Use timers for low-frequency work (polling a slow API, autosaves).
Don't use them for animation — use `on_frame` for that.

---

## 8. Event hooks

```rust
pub extern "C" fn start_app() {
    on_event("resize", 100);
    on_event("drop_files", 101);
}

#[no_mangle]
pub extern "C" fn on_event(cb: u32) {
    match cb {
        100 => log(&format!("resized: {}", event_type())),
        101 => {
            let data = event_data_into();
            log(&format!("{} bytes dropped", data.len()));
        }
        _ => {}
    }
}
```

---

## 9. Buffer-reuse for zero-alloc streaming

Prefer `fetch_recv_into(&mut buf)` and `microphone_read_samples(&mut buf)`
in hot paths where GC would cost too much.

```rust
const BUF: usize = 8192;
static mut BUF_MEM: [u8; BUF] = [0; BUF];

pub extern "C" fn on_frame(_dt: u32) {
    let s = state();
    let buf = unsafe { &mut BUF_MEM[..] };
    match fetch_recv_into(s.handle, buf) {
        n if n >= 0 => append(&buf[..n as usize]),
        -1 => {} // pending
        _  => finalize(),
    }
}
```

---

## 10. Layout & sizing

Never hard-code canvas dimensions. Read them every frame:

```rust
let (w, h) = canvas_dimensions();
let cx = w as f32 * 0.5;
let cy = h as f32 * 0.5;
```

Pre-compute panel rects once per frame, pass them around as `Rect`
values from `draw::`.

---

## 11. Palette (house colors)

```rust
const BG:        (u8, u8, u8, u8) = (18, 18, 26, 255);
const PANEL:     (u8, u8, u8, u8) = (30, 30, 46, 255);
const ACCENT:    (u8, u8, u8, u8) = (180, 120, 255, 255);
const OK:        (u8, u8, u8, u8) = (160, 220, 160, 255);
const WARN:      (u8, u8, u8, u8) = (240, 200, 120, 255);
const ERR:       (u8, u8, u8, u8) = (240, 120, 120, 255);
const MUTED:     (u8, u8, u8, u8) = (120, 120, 140, 255);
```

---

## 12. Numeric formatting

Use `format!` for display — it works. Avoid pulling `num_format` or
`chrono`; ad-hoc helpers are fine:

```rust
fn pct(x: f32) -> String { format!("{:.0}%", x * 100.0) }
fn mmss(ms: u64) -> String {
    let s = ms / 1000; format!("{:02}:{:02}", s / 60, s % 60)
}
```

---

## 13. Capability proposal marker

When a feature needs a new host capability, emit a top-of-file comment:

```rust
// FORGE_PROPOSAL: api_clipboard_image_read() to read images from clipboard.
// Current capability only supports text via clipboard_read().
```

The Forge UI surfaces these to the user for human approval and host
implementation.
