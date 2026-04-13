//! WebSocket chat demo.
//!
//! Demonstrates the Oxide WebSocket API: connect to a server, send text
//! frames, and poll for incoming messages — all from a guest `.wasm` module.
//!
//! By default the app connects to a local echo server at `ws://127.0.0.1:9001`.
//! Enter any `ws://` or `wss://` URL in the address bar and press Connect.
//!
//! # Building
//!
//! ```bash
//! cargo build --target wasm32-unknown-unknown --release -p ws-chat
//! ```
//!
//! Then open the resulting `.wasm` in the Oxide browser.

use oxide_sdk::*;

const MAX_MESSAGES: usize = 30;
const MSG_BUF_SIZE: usize = 512;

static mut STATE: AppState = AppState::new();

/// Persistent app state stored as a static (no heap allocation needed).
struct AppState {
    // ── Connection ────────────────────────────────────────────────────────
    ws_id: u32,
    connected: bool,

    // ── URL input field ───────────────────────────────────────────────────
    url_buf: [u8; 256],
    url_len: usize,

    // ── Message input field ───────────────────────────────────────────────
    msg_buf: [u8; MSG_BUF_SIZE],
    msg_len: usize,

    // ── Received/sent message log ─────────────────────────────────────────
    // Each entry: (direction: 0=recv 1=sent 2=system, text bytes, length)
    log: [(u8, [u8; MSG_BUF_SIZE], usize); MAX_MESSAGES],
    log_count: usize,

    // ── Scroll offset for the message list ────────────────────────────────
    scroll_offset: f32,
}

impl AppState {
    const fn new() -> Self {
        Self {
            ws_id: 0,
            connected: false,
            url_buf: {
                let mut b = [0u8; 256];
                let src = b"ws://127.0.0.1:9001";
                let mut i = 0;
                while i < src.len() {
                    b[i] = src[i];
                    i += 1;
                }
                b
            },
            url_len: 19, // length of "ws://127.0.0.1:9001"
            msg_buf: [0u8; MSG_BUF_SIZE],
            msg_len: 0,
            log: [(0, [0u8; MSG_BUF_SIZE], 0); MAX_MESSAGES],
            log_count: 0,
            scroll_offset: 0.0,
        }
    }

    fn url(&self) -> &str {
        core::str::from_utf8(&self.url_buf[..self.url_len]).unwrap_or("")
    }

    fn message_text(&self) -> &str {
        core::str::from_utf8(&self.msg_buf[..self.msg_len]).unwrap_or("")
    }

    fn push_log(&mut self, direction: u8, text: &str) {
        if self.log_count >= MAX_MESSAGES {
            // Scroll the ring buffer.
            for i in 0..MAX_MESSAGES - 1 {
                self.log[i] = self.log[i + 1];
            }
            self.log_count = MAX_MESSAGES - 1;
        }
        let idx = self.log_count;
        self.log[idx].0 = direction;
        let bytes = text.as_bytes();
        let len = bytes.len().min(MSG_BUF_SIZE);
        self.log[idx].1[..len].copy_from_slice(&bytes[..len]);
        self.log[idx].2 = len;
        self.log_count += 1;
    }
}

#[no_mangle]
pub extern "C" fn start_app() {
    log("WS Chat demo loaded");
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let s = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    let (width, height) = canvas_dimensions();
    let w = width as f32;
    let h = height as f32;

    // ── Background ────────────────────────────────────────────────────────
    canvas_clear(22, 27, 42, 255);

    // ── Header bar ───────────────────────────────────────────────────────
    canvas_rect(0.0, 0.0, w, 48.0, 30, 38, 68, 255);
    canvas_text(14.0, 14.0, 20.0, 160, 200, 255, 255, "Oxide WebSocket Chat");

    // Connection state badge
    let (badge_r, badge_g, badge_b, badge_text) = match ws_ready_state(s.ws_id) {
        WS_CONNECTING => (200, 160, 0, "CONNECTING"),
        WS_OPEN => (40, 200, 100, "CONNECTED"),
        WS_CLOSING => (200, 120, 0, "CLOSING"),
        _ => {
            if s.ws_id > 0 {
                s.connected = false;
                (180, 60, 60, "CLOSED")
            } else {
                (100, 100, 120, "DISCONNECTED")
            }
        }
    };
    canvas_rounded_rect(
        w - 130.0,
        12.0,
        118.0,
        24.0,
        12.0,
        badge_r,
        badge_g,
        badge_b,
        50,
    );
    canvas_text(
        w - 122.0,
        17.0,
        13.0,
        badge_r,
        badge_g,
        badge_b,
        255,
        badge_text,
    );

    // ── URL / Connect row ─────────────────────────────────────────────────
    let row_y = 60.0;
    canvas_text(14.0, row_y + 4.0, 13.0, 140, 150, 180, 255, "Server:");
    let url_val = ui_text_input(1, 70.0, row_y, w - 190.0, s.url());
    // Persist whatever the user typed.
    {
        let bytes = url_val.as_bytes();
        let len = bytes.len().min(255);
        s.url_buf[..len].copy_from_slice(&bytes[..len]);
        s.url_len = len;
    }

    let btn_x = w - 110.0;
    let btn_label = if s.connected || ws_ready_state(s.ws_id) == WS_OPEN {
        "Disconnect"
    } else {
        "Connect"
    };

    if ui_button(2, btn_x, row_y, 96.0, 28.0, btn_label) {
        if s.connected || ws_ready_state(s.ws_id) == WS_OPEN {
            ws_close(s.ws_id);
            s.connected = false;
            s.push_log(2, "Disconnecting…");
        } else {
            let url = {
                // Copy URL into a stack buffer so we can pass a &str without
                // holding a borrow on `s`.
                let mut tmp = [0u8; 256];
                let len = s.url_len;
                tmp[..len].copy_from_slice(&s.url_buf[..len]);
                (tmp, len)
            };
            let url_str = core::str::from_utf8(&url.0[..url.1]).unwrap_or("");
            let id = ws_connect(url_str);
            if id > 0 {
                s.ws_id = id;
                s.connected = true;
                s.push_log(2, "Connecting…");
            } else {
                s.push_log(2, "Failed to initiate connection.");
            }
        }
    }

    // ── Drain incoming messages ───────────────────────────────────────────
    if s.ws_id > 0 {
        while let Some(msg) = ws_recv(s.ws_id) {
            let text = if msg.is_binary {
                format_bytes_preview(&msg.data)
            } else {
                msg.text()
            };
            s.push_log(0, &text);
        }
        // Detect remote close.
        if ws_ready_state(s.ws_id) == WS_CLOSED && s.connected {
            s.connected = false;
            s.push_log(2, "Connection closed by server.");
            ws_remove(s.ws_id);
        }
    }

    // ── Message log area ─────────────────────────────────────────────────
    let log_top = row_y + 40.0;
    let log_bottom = h - 50.0;
    let log_h = log_bottom - log_top;
    canvas_rounded_rect(10.0, log_top, w - 20.0, log_h, 6.0, 14, 20, 35, 255);

    // Scroll on mouse wheel.
    let (_, dy) = scroll_delta();
    s.scroll_offset = (s.scroll_offset + dy * 20.0).max(0.0);

    let line_h = 22.0;
    let total_h = s.log_count as f32 * line_h;
    let max_scroll = (total_h - log_h + 8.0).max(0.0);
    if s.scroll_offset > max_scroll {
        s.scroll_offset = max_scroll;
    }

    let visible_lines = ((log_h / line_h) as usize).min(s.log_count);
    let first_idx = if s.log_count > visible_lines {
        let auto_scroll = (total_h - s.scroll_offset - log_h) < line_h;
        if auto_scroll {
            s.log_count - visible_lines
        } else {
            let scroll_idx = (s.scroll_offset / line_h) as usize;
            scroll_idx.min(s.log_count - visible_lines)
        }
    } else {
        0
    };

    for i in 0..visible_lines {
        let idx = first_idx + i;
        if idx >= s.log_count {
            break;
        }
        let dir = s.log[idx].0;
        let len = s.log[idx].2;
        let text = core::str::from_utf8(&s.log[idx].1[..len]).unwrap_or("(invalid utf-8)");
        let y = log_top + 8.0 + i as f32 * line_h;

        // Colour coding: received=green, sent=blue, system=grey
        let (prefix, pr, pg, pb) = match dir {
            0 => ("▼ ", 80u8, 220u8, 140u8),
            1 => ("▲ ", 100u8, 160u8, 255u8),
            _ => ("• ", 140u8, 150u8, 170u8),
        };

        let mut line = [0u8; 520];
        let plen = prefix.len();
        line[..plen].copy_from_slice(prefix.as_bytes());
        let tlen = text.len().min(512);
        line[plen..plen + tlen].copy_from_slice(&text.as_bytes()[..tlen]);
        let full_len = plen + tlen;
        let display = core::str::from_utf8(&line[..full_len]).unwrap_or(text);
        canvas_text(18.0, y, 13.0, pr, pg, pb, 255, display);
    }

    // ── Message compose row ───────────────────────────────────────────────
    let compose_y = h - 42.0;
    canvas_rect(0.0, compose_y - 4.0, w, 46.0, 26, 34, 54, 255);

    let input_val = ui_text_input(3, 14.0, compose_y, w - 110.0, s.message_text());
    {
        let bytes = input_val.as_bytes();
        let len = bytes.len().min(MSG_BUF_SIZE - 1);
        s.msg_buf[..len].copy_from_slice(&bytes[..len]);
        s.msg_len = len;
    }

    let can_send = s.ws_id > 0 && ws_ready_state(s.ws_id) == WS_OPEN && s.msg_len > 0;
    if ui_button(4, w - 90.0, compose_y, 76.0, 28.0, "Send") && can_send {
        let text = {
            let mut tmp = [0u8; MSG_BUF_SIZE];
            tmp[..s.msg_len].copy_from_slice(&s.msg_buf[..s.msg_len]);
            (tmp, s.msg_len)
        };
        let text_str = core::str::from_utf8(&text.0[..text.1]).unwrap_or("");
        ws_send_text(s.ws_id, text_str);
        s.push_log(1, text_str);
        // Clear input after send.
        s.msg_len = 0;
        s.msg_buf = [0u8; MSG_BUF_SIZE];
    }
}

/// Format the first few bytes of a binary frame for display.
fn format_bytes_preview(data: &[u8]) -> alloc::string::String {
    let preview_len = data.len().min(16);
    let mut s = alloc::string::String::from("[binary] ");
    for byte in &data[..preview_len] {
        let hi = byte >> 4;
        let lo = byte & 0xF;
        s.push(hex_char(hi));
        s.push(hex_char(lo));
        s.push(' ');
    }
    if data.len() > 16 {
        s.push('…');
    }
    s
}

fn hex_char(n: u8) -> char {
    if n < 10 {
        (b'0' + n) as char
    } else {
        (b'a' + n - 10) as char
    }
}

extern crate alloc;
