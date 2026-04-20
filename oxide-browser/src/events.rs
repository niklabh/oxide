//! Host-side event system for Oxide guest modules.
//!
//! Guests register listeners with `api_on_event(type, callback_id)` and receive
//! events through the optional exported `on_event(callback_id: u32)` function.
//! While the callback runs, the guest may read the current event type and
//! payload via `api_event_type_*` and `api_event_data_*`.
//!
//! Two sources push events onto the per-guest queue:
//!
//! 1. **Custom events** emitted by the guest itself via `api_emit_event(type, data)`.
//! 2. **Built-in events** produced by the host each tick:
//!    - `resize` — payload is `(width: u32, height: u32)` little-endian.
//!    - `focus` / `blur` — empty payload; fires when the canvas gains/loses focus.
//!    - `visibility_change` — payload is `"visible"` or `"hidden"` UTF-8.
//!    - `online` / `offline` — empty payload; backed by a 30 s reachability check.
//!    - `touch_start` / `touch_move` / `touch_end` — payload is `(x: f32, y: f32)`
//!      little-endian. Synthesised from primary mouse button + mouse position;
//!      `touch_cancel` is reserved for real touch input on platforms that surface it.
//!    - `gamepad_connected` — payload is the device name (UTF-8).
//!    - `gamepad_button` — payload is `(gamepad_id: u32, button_code: u32, pressed: u32)`
//!      little-endian (`pressed` = 1 for press, 0 for release).
//!    - `gamepad_axis` — payload is `(gamepad_id: u32, axis_code: u32, value: f32)`
//!      little-endian (`value` in [-1.0, 1.0]).
//!    - `drop_files` — payload is a JSON array of dropped file paths (UTF-8).
//!
//! Built-in producers run inside [`drain_pending_events`], which the runtime
//! calls once per frame before invoking the guest `on_event` exports.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use wasmtime::{Caller, Linker};

use crate::capabilities::{read_guest_bytes, read_guest_string, write_guest_bytes, HostState};

/// Maximum events kept in a guest's pending queue. Older events are dropped
/// on overflow to prevent unbounded growth from a slow / non-listening guest.
const MAX_QUEUED_EVENTS: usize = 4096;

/// `online_state` sentinel values.
const ONLINE_UNKNOWN: u8 = 0;
const ONLINE_YES: u8 = 1;
const ONLINE_NO: u8 = 2;

/// One queued event: type name and arbitrary opaque payload bytes.
#[derive(Clone, Debug)]
pub struct PendingEvent {
    pub event_type: String,
    pub data: Vec<u8>,
}

/// Event type and payload currently being delivered to the guest callback.
#[derive(Clone, Default, Debug)]
struct CurrentEvent {
    event_type: String,
    data: Vec<u8>,
}

/// Per-guest event system state: listener table, pending queue, current event,
/// and built-in event detector state (last canvas size, focus, mouse, online).
pub struct EventState {
    /// `event_type` → ordered list of `(listener_id, callback_id)`.
    /// Vec preserves insertion order so listeners fire in registration order.
    listeners: HashMap<String, Vec<(u32, u32)>>,
    /// Reverse index: `listener_id` → `event_type` (for `api_off_event`).
    listener_owner: HashMap<u32, String>,
    next_listener_id: u32,
    queue: VecDeque<PendingEvent>,
    current: CurrentEvent,

    // Built-in detector state — updated inside `drain_pending_events`.
    last_canvas_size: Option<(u32, u32)>,
    last_focused: Option<bool>,
    last_mouse_down: bool,
    last_mouse_pos: (f32, f32),

    // Online/offline: atomic shared with checker thread (ONLINE_UNKNOWN/YES/NO).
    // Dropping `online_shutdown` signals the thread to exit via channel disconnect.
    online_state: Arc<AtomicU8>,
    last_online: Option<bool>,
    online_thread_started: bool,
    online_shutdown: Option<Sender<()>>,

    // Gamepad: events arrive from poll thread via channel.
    // Dropping `gamepad_shutdown` signals the thread to exit via channel disconnect.
    gamepad_rx: Option<Receiver<PendingEvent>>,
    gamepad_thread_started: bool,
    gamepad_shutdown: Option<Sender<()>>,
}

impl Default for EventState {
    fn default() -> Self {
        Self {
            listeners: HashMap::new(),
            listener_owner: HashMap::new(),
            next_listener_id: 1,
            queue: VecDeque::new(),
            current: CurrentEvent::default(),
            last_canvas_size: None,
            last_focused: None,
            last_mouse_down: false,
            last_mouse_pos: (0.0, 0.0),
            online_state: Arc::new(AtomicU8::new(ONLINE_UNKNOWN)),
            last_online: None,
            online_thread_started: false,
            online_shutdown: None,
            gamepad_rx: None,
            gamepad_thread_started: false,
            gamepad_shutdown: None,
        }
    }
}

impl EventState {
    fn alloc_listener_id(&mut self) -> u32 {
        let id = self.next_listener_id;
        self.next_listener_id = self.next_listener_id.wrapping_add(1).max(1);
        id
    }

    fn add_listener(&mut self, event_type: String, callback_id: u32) -> u32 {
        // Lazily kick off background producers when something is actually listening.
        if (event_type == "online" || event_type == "offline") && !self.online_thread_started {
            self.start_online_checker();
        }
        if event_type.starts_with("gamepad_") && !self.gamepad_thread_started {
            self.start_gamepad_poll();
        }
        let listener_id = self.alloc_listener_id();
        self.listeners
            .entry(event_type.clone())
            .or_default()
            .push((listener_id, callback_id));
        self.listener_owner.insert(listener_id, event_type);
        listener_id
    }

    fn remove_listener(&mut self, listener_id: u32) -> bool {
        let Some(event_type) = self.listener_owner.remove(&listener_id) else {
            return false;
        };
        if let Some(vec) = self.listeners.get_mut(&event_type) {
            vec.retain(|(lid, _)| *lid != listener_id);
            if vec.is_empty() {
                self.listeners.remove(&event_type);
            }
        }
        true
    }

    fn enqueue(&mut self, event: PendingEvent) {
        if self.listeners.contains_key(&event.event_type) {
            if self.queue.len() >= MAX_QUEUED_EVENTS {
                self.queue.pop_front();
            }
            self.queue.push_back(event);
        }
    }

    fn start_online_checker(&mut self) {
        self.online_thread_started = true;
        let state = self.online_state.clone();
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
        self.online_shutdown = Some(shutdown_tx);
        thread::Builder::new()
            .name("oxide-online-checker".into())
            .spawn(move || {
                let client = match reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(5))
                    .build()
                {
                    Ok(c) => c,
                    Err(_) => return,
                };
                loop {
                    let ok = client
                        .head("https://www.cloudflare.com/cdn-cgi/trace")
                        .send()
                        .map(|r| r.status().is_success())
                        .unwrap_or(false);
                    state.store(if ok { ONLINE_YES } else { ONLINE_NO }, Ordering::Relaxed);
                    // Sleep 30 s but wake immediately on shutdown (sender dropped).
                    match shutdown_rx.recv_timeout(Duration::from_secs(30)) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
            })
            .ok();
    }

    fn start_gamepad_poll(&mut self) {
        self.gamepad_thread_started = true;
        let (tx, rx) = mpsc::channel::<PendingEvent>();
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
        self.gamepad_rx = Some(rx);
        self.gamepad_shutdown = Some(shutdown_tx);
        thread::Builder::new()
            .name("oxide-gamepad-poll".into())
            .spawn(move || gamepad_poll_loop(tx, shutdown_rx))
            .ok();
    }
}

/// Drain built-in event sources + the pending custom-event queue and return
/// `(callback_id, type, data)` tuples to dispatch via `on_event`.
///
/// `state` is mutably accessed once. The runtime then calls `on_event` for
/// each returned tuple, after writing the type/data into [`EventState::current`]
/// via [`set_current_event`].
pub fn drain_pending_events(
    events: &Arc<std::sync::Mutex<EventState>>,
    canvas_size: (u32, u32),
    focused: bool,
    mouse_down: bool,
    mouse_pos: (f32, f32),
) -> Vec<(u32, String, Vec<u8>)> {
    let mut g = events.lock().unwrap();

    // ── Built-in: resize ──────────────────────────────────────────────
    match g.last_canvas_size {
        Some(prev) if prev == canvas_size => {}
        _ => {
            if g.last_canvas_size.is_some() {
                let mut data = Vec::with_capacity(8);
                data.extend_from_slice(&canvas_size.0.to_le_bytes());
                data.extend_from_slice(&canvas_size.1.to_le_bytes());
                g.enqueue(PendingEvent {
                    event_type: "resize".into(),
                    data,
                });
            }
            g.last_canvas_size = Some(canvas_size);
        }
    }

    // ── Built-in: focus / blur / visibility_change ────────────────────
    match g.last_focused {
        Some(prev) if prev == focused => {}
        _ => {
            if g.last_focused.is_some() {
                let (focus_evt, vis_payload) = if focused {
                    ("focus", b"visible".to_vec())
                } else {
                    ("blur", b"hidden".to_vec())
                };
                g.enqueue(PendingEvent {
                    event_type: focus_evt.into(),
                    data: Vec::new(),
                });
                g.enqueue(PendingEvent {
                    event_type: "visibility_change".into(),
                    data: vis_payload,
                });
            }
            g.last_focused = Some(focused);
        }
    }

    // ── Built-in: touch_start / touch_move / touch_end (mouse-synthesised) ─
    let prev_down = g.last_mouse_down;
    let prev_pos = g.last_mouse_pos;
    if mouse_down && !prev_down {
        g.enqueue(PendingEvent {
            event_type: "touch_start".into(),
            data: encode_xy(mouse_pos),
        });
    } else if mouse_down && prev_down && mouse_pos != prev_pos {
        g.enqueue(PendingEvent {
            event_type: "touch_move".into(),
            data: encode_xy(mouse_pos),
        });
    } else if !mouse_down && prev_down {
        g.enqueue(PendingEvent {
            event_type: "touch_end".into(),
            data: encode_xy(prev_pos),
        });
    }
    g.last_mouse_down = mouse_down;
    g.last_mouse_pos = mouse_pos;

    // ── Built-in: online / offline ────────────────────────────────────
    let online_raw = g.online_state.load(Ordering::Relaxed);
    if online_raw != ONLINE_UNKNOWN {
        let now = online_raw == ONLINE_YES;
        match g.last_online {
            Some(prev) if prev == now => {}
            _ => {
                if g.last_online.is_some() {
                    g.enqueue(PendingEvent {
                        event_type: if now { "online" } else { "offline" }.into(),
                        data: Vec::new(),
                    });
                }
                g.last_online = Some(now);
            }
        }
    }

    // ── Built-in: gamepad_* ───────────────────────────────────────────
    if let Some(rx) = g.gamepad_rx.as_ref() {
        let mut drained = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            drained.push(ev);
        }
        for ev in drained {
            g.enqueue(ev);
        }
    }

    // ── Drain queue → dispatch tuples ─────────────────────────────────
    let mut out = Vec::new();
    while let Some(ev) = g.queue.pop_front() {
        if let Some(vec) = g.listeners.get(&ev.event_type) {
            for &(_, cb) in vec {
                out.push((cb, ev.event_type.clone(), ev.data.clone()));
            }
        }
    }
    out
}

/// Push a freshly-dropped file batch onto the queue as a `drop_files` event.
///
/// Called from the UI layer when the GPUI canvas receives an external file drop.
/// `paths` are encoded as a UTF-8 JSON array (e.g. `["/tmp/a.png","/tmp/b.png"]`).
pub fn enqueue_drop_files(
    events: &Arc<std::sync::Mutex<EventState>>,
    paths: &[std::path::PathBuf],
) {
    let mut g = events.lock().unwrap();
    let mut json = String::from("[");
    for (i, p) in paths.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push('"');
        for c in p.to_string_lossy().chars() {
            match c {
                '"' => json.push_str("\\\""),
                '\\' => json.push_str("\\\\"),
                '\n' => json.push_str("\\n"),
                '\r' => json.push_str("\\r"),
                '\t' => json.push_str("\\t"),
                c if (c as u32) < 0x20 => json.push_str(&format!("\\u{:04x}", c as u32)),
                c => json.push(c),
            }
        }
        json.push('"');
    }
    json.push(']');
    g.enqueue(PendingEvent {
        event_type: "drop_files".into(),
        data: json.into_bytes(),
    });
}

/// Populate `current` so the guest can read the event type/data inside its
/// `on_event` callback. Called by the runtime immediately before each
/// `on_event(callback_id)` invocation.
pub fn set_current_event(
    events: &Arc<std::sync::Mutex<EventState>>,
    event_type: String,
    data: Vec<u8>,
) {
    let mut g = events.lock().unwrap();
    g.current.event_type = event_type;
    g.current.data = data;
}

fn encode_xy((x, y): (f32, f32)) -> Vec<u8> {
    let mut data = Vec::with_capacity(8);
    data.extend_from_slice(&x.to_le_bytes());
    data.extend_from_slice(&y.to_le_bytes());
    data
}

// ── Gamepad polling ──────────────────────────────────────────────────────────

fn gamepad_poll_loop(tx: Sender<PendingEvent>, shutdown: Receiver<()>) {
    use gilrs::{EventType, Gilrs};
    let mut gilrs = match Gilrs::new() {
        Ok(g) => g,
        Err(_) => return,
    };
    loop {
        while let Some(gilrs::Event { id, event, .. }) = gilrs.next_event() {
            let id_u: u32 = Into::<usize>::into(id) as u32;
            let pending = match event {
                EventType::Connected => {
                    let name = gilrs.gamepad(id).name().to_string();
                    Some(PendingEvent {
                        event_type: "gamepad_connected".into(),
                        data: name.into_bytes(),
                    })
                }
                EventType::ButtonPressed(btn, _) => Some(PendingEvent {
                    event_type: "gamepad_button".into(),
                    data: encode_button(id_u, btn as u32, true),
                }),
                EventType::ButtonReleased(btn, _) => Some(PendingEvent {
                    event_type: "gamepad_button".into(),
                    data: encode_button(id_u, btn as u32, false),
                }),
                EventType::AxisChanged(axis, value, _) => Some(PendingEvent {
                    event_type: "gamepad_axis".into(),
                    data: encode_axis(id_u, axis as u32, value),
                }),
                _ => None,
            };
            if let Some(ev) = pending {
                if tx.send(ev).is_err() {
                    return;
                }
            }
        }
        // Sleep 16 ms between polls but wake immediately on shutdown (sender dropped).
        match shutdown.recv_timeout(Duration::from_millis(16)) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

fn encode_button(id: u32, code: u32, pressed: bool) -> Vec<u8> {
    let mut data = Vec::with_capacity(12);
    data.extend_from_slice(&id.to_le_bytes());
    data.extend_from_slice(&code.to_le_bytes());
    data.extend_from_slice(&(pressed as u32).to_le_bytes());
    data
}

fn encode_axis(id: u32, code: u32, value: f32) -> Vec<u8> {
    let mut data = Vec::with_capacity(12);
    data.extend_from_slice(&id.to_le_bytes());
    data.extend_from_slice(&code.to_le_bytes());
    data.extend_from_slice(&value.to_le_bytes());
    data
}

// ── Host function registration ───────────────────────────────────────────────

/// Register `api_on_event`, `api_off_event`, `api_emit_event`, and the
/// in-callback type/data read functions on the linker.
pub fn register_event_functions(linker: &mut Linker<HostState>) -> Result<()> {
    linker.func_wrap(
        "oxide",
        "api_on_event",
        |caller: Caller<'_, HostState>, type_ptr: u32, type_len: u32, callback_id: u32| -> u32 {
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return 0,
            };
            let event_type = match read_guest_string(&mem, &caller, type_ptr, type_len) {
                Ok(s) if !s.is_empty() => s,
                _ => return 0,
            };
            let mut g = caller.data().events.lock().unwrap();
            g.add_listener(event_type, callback_id)
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_off_event",
        |caller: Caller<'_, HostState>, listener_id: u32| -> u32 {
            let mut g = caller.data().events.lock().unwrap();
            u32::from(g.remove_listener(listener_id))
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_emit_event",
        |caller: Caller<'_, HostState>,
         type_ptr: u32,
         type_len: u32,
         data_ptr: u32,
         data_len: u32| {
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return,
            };
            let event_type = match read_guest_string(&mem, &caller, type_ptr, type_len) {
                Ok(s) if !s.is_empty() => s,
                _ => return,
            };
            let data = if data_len == 0 {
                Vec::new()
            } else {
                read_guest_bytes(&mem, &caller, data_ptr, data_len).unwrap_or_default()
            };
            let mut g = caller.data().events.lock().unwrap();
            g.enqueue(PendingEvent { event_type, data });
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_event_type_len",
        |caller: Caller<'_, HostState>| -> u32 {
            let g = caller.data().events.lock().unwrap();
            g.current.event_type.len() as u32
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_event_type_read",
        |mut caller: Caller<'_, HostState>, out_ptr: u32, out_cap: u32| -> u32 {
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return 0,
            };
            // Clone before write_guest_bytes borrows caller mutably.
            let bytes = caller
                .data()
                .events
                .lock()
                .unwrap()
                .current
                .event_type
                .clone()
                .into_bytes();
            let len = bytes.len().min(out_cap as usize);
            if write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..len]).is_err() {
                return 0;
            }
            len as u32
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_event_data_len",
        |caller: Caller<'_, HostState>| -> u32 {
            let g = caller.data().events.lock().unwrap();
            g.current.data.len() as u32
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_event_data_read",
        |mut caller: Caller<'_, HostState>, out_ptr: u32, out_cap: u32| -> u32 {
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return 0,
            };
            // Clone before write_guest_bytes borrows caller mutably.
            let bytes = caller.data().events.lock().unwrap().current.data.clone();
            let len = bytes.len().min(out_cap as usize);
            if write_guest_bytes(&mem, &mut caller, out_ptr, &bytes[..len]).is_err() {
                return 0;
            }
            len as u32
        },
    )?;

    Ok(())
}
