//! Streaming fetch demo.
//!
//! Demonstrates the non-blocking fetch API: dispatch a request with
//! `fetch_begin`, keep rendering, and drain body chunks with `fetch_recv` as
//! they arrive. The default URL uses `httpbin.org/drip`, which trickles a
//! configurable number of bytes over several seconds — perfect for watching
//! the byte counter tick up while the frame loop keeps running.
//!
//! # Building
//!
//! ```bash
//! cargo build --target wasm32-unknown-unknown --release -p stream-fetch-demo
//! ```

use oxide_sdk::*;

const DEFAULT_URL: &str = "https://httpbin.org/drip?duration=5&numbytes=400&delay=0";
const BODY_BUF: usize = 8 * 1024;
const URL_BUF: usize = 512;

static mut STATE: AppState = AppState::new();

struct AppState {
    handle: u32,
    bytes: usize,
    chunks: u32,
    body: [u8; BODY_BUF],
    body_len: usize,
    body_truncated: bool,
    status: u32,
    url_buf: [u8; URL_BUF],
    url_len: usize,
    last_error: [u8; 256],
    last_error_len: usize,
}

impl AppState {
    const fn new() -> Self {
        let mut url_buf = [0u8; URL_BUF];
        let src = DEFAULT_URL.as_bytes();
        let mut i = 0;
        while i < src.len() {
            url_buf[i] = src[i];
            i += 1;
        }
        Self {
            handle: 0,
            bytes: 0,
            chunks: 0,
            body: [0u8; BODY_BUF],
            body_len: 0,
            body_truncated: false,
            status: 0,
            url_buf,
            url_len: DEFAULT_URL.len(),
            last_error: [0u8; 256],
            last_error_len: 0,
        }
    }

    fn url(&self) -> &str {
        core::str::from_utf8(&self.url_buf[..self.url_len]).unwrap_or("")
    }

    fn reset(&mut self) {
        self.handle = 0;
        self.bytes = 0;
        self.chunks = 0;
        self.body_len = 0;
        self.body_truncated = false;
        self.status = 0;
        self.last_error_len = 0;
    }

    fn append_body(&mut self, data: &[u8]) {
        let room = BODY_BUF - self.body_len;
        if room == 0 {
            self.body_truncated = true;
            return;
        }
        let take = data.len().min(room);
        self.body[self.body_len..self.body_len + take].copy_from_slice(&data[..take]);
        self.body_len += take;
        if take < data.len() {
            self.body_truncated = true;
        }
    }

    fn set_error(&mut self, msg: &str) {
        let bytes = msg.as_bytes();
        let n = bytes.len().min(self.last_error.len());
        self.last_error[..n].copy_from_slice(&bytes[..n]);
        self.last_error_len = n;
    }
}

#[no_mangle]
pub extern "C" fn start_app() {
    log("Streaming fetch demo loaded");
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let s = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    let (w, h) = canvas_dimensions();
    let w = w as f32;
    let h = h as f32;

    canvas_clear(20, 24, 36, 255);

    // ── Header ────────────────────────────────────────────────────────────
    canvas_rect(0.0, 0.0, w, 48.0, 28, 34, 54, 255);
    canvas_text(
        14.0,
        14.0,
        20.0,
        160,
        200,
        255,
        255,
        "Oxide Streaming Fetch",
    );

    // ── State badge ───────────────────────────────────────────────────────
    let (br, bg, bb, label) = if s.handle == 0 {
        (100, 100, 120, "IDLE")
    } else {
        match fetch_state(s.handle) {
            FETCH_PENDING => (200, 160, 0, "PENDING"),
            FETCH_STREAMING => (40, 200, 100, "STREAMING"),
            FETCH_DONE => (80, 140, 255, "DONE"),
            FETCH_ERROR => (220, 80, 80, "ERROR"),
            FETCH_ABORTED => (200, 120, 0, "ABORTED"),
            _ => (140, 140, 160, "?"),
        }
    };
    canvas_rounded_rect(w - 140.0, 12.0, 128.0, 24.0, 12.0, br, bg, bb, 60);
    canvas_text(w - 132.0, 17.0, 13.0, br, bg, bb, 255, label);

    // ── URL row ───────────────────────────────────────────────────────────
    let row_y = 60.0;
    canvas_text(14.0, row_y + 4.0, 13.0, 140, 150, 180, 255, "URL:");
    let url_val = ui_text_input(1, 60.0, row_y, w - 290.0, s.url());
    {
        let bytes = url_val.as_bytes();
        let n = bytes.len().min(URL_BUF - 1);
        s.url_buf[..n].copy_from_slice(&bytes[..n]);
        s.url_len = n;
    }

    let btn_x = w - 220.0;
    let active = s.handle != 0 && matches!(fetch_state(s.handle), FETCH_PENDING | FETCH_STREAMING,);

    if ui_button(
        2,
        btn_x,
        row_y,
        96.0,
        28.0,
        if active { "Restart" } else { "Fetch" },
    ) {
        if s.handle != 0 {
            fetch_abort(s.handle);
            fetch_remove(s.handle);
        }
        s.reset();
        let url_owned = {
            let mut tmp = [0u8; URL_BUF];
            tmp[..s.url_len].copy_from_slice(&s.url_buf[..s.url_len]);
            (tmp, s.url_len)
        };
        let url_str = core::str::from_utf8(&url_owned.0[..url_owned.1]).unwrap_or("");
        let id = fetch_begin_get(url_str);
        if id > 0 {
            s.handle = id;
        } else {
            s.set_error("failed to init fetch subsystem");
        }
    }

    let abort_x = w - 114.0;
    if active && ui_button(3, abort_x, row_y, 96.0, 28.0, "Abort") {
        fetch_abort(s.handle);
    }

    // ── Drain incoming chunks ─────────────────────────────────────────────
    if s.handle != 0 {
        s.status = fetch_status(s.handle);
        // Drain everything the host has queued this frame; stop on pending/end/error.
        for _ in 0..64 {
            match fetch_recv(s.handle) {
                FetchChunk::Data(bytes) => {
                    s.chunks += 1;
                    s.bytes += bytes.len();
                    s.append_body(&bytes);
                }
                FetchChunk::Pending => break,
                FetchChunk::End => {
                    log("[stream-fetch] EOF");
                    break;
                }
                FetchChunk::Error => {
                    if let Some(msg) = fetch_error(s.handle) {
                        s.set_error(&msg);
                    } else {
                        s.set_error("unknown error");
                    }
                    break;
                }
            }
        }
    }

    // ── Stats block ───────────────────────────────────────────────────────
    let stats_y = 110.0;
    canvas_rounded_rect(10.0, stats_y, w - 20.0, 84.0, 6.0, 14, 20, 35, 255);

    let mut line = [0u8; 128];
    let status_text = format_u32(&mut line, s.status);
    canvas_text(
        24.0,
        stats_y + 12.0,
        14.0,
        140,
        150,
        180,
        255,
        "HTTP status:",
    );
    canvas_text(130.0, stats_y + 12.0, 14.0, 230, 230, 240, 255, status_text);

    let bytes_text = format_usize(&mut line, s.bytes);
    canvas_text(
        24.0,
        stats_y + 34.0,
        14.0,
        140,
        150,
        180,
        255,
        "Bytes received:",
    );
    canvas_text(150.0, stats_y + 34.0, 14.0, 230, 230, 240, 255, bytes_text);

    let chunks_text = format_u32(&mut line, s.chunks);
    canvas_text(24.0, stats_y + 56.0, 14.0, 140, 150, 180, 255, "Chunks:");
    canvas_text(90.0, stats_y + 56.0, 14.0, 230, 230, 240, 255, chunks_text);

    // ── Body preview ──────────────────────────────────────────────────────
    let body_top = stats_y + 100.0;
    let body_h = h - body_top - 20.0;
    canvas_rounded_rect(10.0, body_top, w - 20.0, body_h, 6.0, 12, 16, 28, 255);
    canvas_text(
        24.0,
        body_top + 8.0,
        12.0,
        120,
        130,
        160,
        255,
        if s.body_truncated {
            "Response body (truncated to 8 KiB):"
        } else {
            "Response body:"
        },
    );

    // Render the body as wrapped lines of up to ~100 chars. Keep it simple —
    // the host text shaper handles UTF-8.
    let text = core::str::from_utf8(&s.body[..s.body_len]).unwrap_or("(non-utf8 body)");
    let line_h = 16.0;
    let start_y = body_top + 28.0;
    let max_lines = ((body_h - 28.0) / line_h) as usize;
    let mut y = start_y;
    for (drawn, part) in text.lines().enumerate() {
        if drawn >= max_lines {
            break;
        }
        canvas_text(24.0, y, 12.0, 210, 220, 230, 255, part);
        y += line_h;
    }

    // ── Error line ────────────────────────────────────────────────────────
    if s.last_error_len > 0 {
        let err = core::str::from_utf8(&s.last_error[..s.last_error_len]).unwrap_or("?");
        canvas_text(14.0, h - 16.0, 12.0, 230, 120, 120, 255, err);
    }
}

// ── Tiny integer formatters (no heap, writes into caller buffer) ──────────

fn format_u32(buf: &mut [u8; 128], n: u32) -> &str {
    format_usize(buf, n as usize)
}

fn format_usize(buf: &mut [u8; 128], mut n: usize) -> &str {
    if n == 0 {
        buf[0] = b'0';
        return core::str::from_utf8(&buf[..1]).unwrap_or("0");
    }
    let mut tmp = [0u8; 24];
    let mut i = 0;
    while n > 0 {
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    let mut j = 0;
    while j < i {
        buf[j] = tmp[i - 1 - j];
        j += 1;
    }
    core::str::from_utf8(&buf[..j]).unwrap_or("")
}

extern crate alloc;
