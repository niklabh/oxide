//! Server-Sent Events demo.
//!
//! Opens an EventSource-style stream with [`sse_open`], polls [`sse_state`],
//! and drains [`sse_recv`] each frame. The default URL is Wikimedia's public
//! recent-change firehose.
//!
//! # Build
//!
//! ```bash
//! cargo build --target wasm32-unknown-unknown --release -p sse-demo
//! ```

use oxide_sdk::*;

const DEFAULT_URL: &str = "https://stream.wikimedia.org/v2/stream/recentchange";
const MAX_LINES: usize = 12;

const BG: (u8, u8, u8) = (24, 26, 34);
const ACCENT: (u8, u8, u8) = (40, 140, 180);
const DIM: (u8, u8, u8) = (140, 140, 165);
const BRIGHT: (u8, u8, u8) = (235, 235, 245);
const GREEN: (u8, u8, u8) = (90, 220, 130);
const ORANGE: (u8, u8, u8) = (240, 180, 60);
const RED: (u8, u8, u8) = (225, 90, 90);

const BTN_CONNECT: u32 = 1;
const BTN_CLOSE: u32 = 2;

struct Demo {
    handle: u32,
    lines: Vec<String>,
    event_count: u32,
}

static mut DEMO: Option<Demo> = None;

#[no_mangle]
pub extern "C" fn start_app() {
    log("SSE Demo loaded");
    unsafe {
        DEMO = Some(Demo {
            handle: 0,
            lines: Vec::new(),
            event_count: 0,
        });
    }
}

fn push_line(demo: &mut Demo, line: String) {
    demo.lines.push(line);
    if demo.lines.len() > MAX_LINES {
        let extra = demo.lines.len() - MAX_LINES;
        demo.lines.drain(..extra);
    }
}

fn state_label(state: u32) -> (&'static str, (u8, u8, u8)) {
    match state {
        SSE_CONNECTING => ("connecting", ORANGE),
        SSE_OPEN => ("open", GREEN),
        SSE_CLOSED => ("closed", DIM),
        SSE_ERROR => ("error", RED),
        _ => ("unknown", DIM),
    }
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let (width, _) = canvas_dimensions();
    let w = width as f32;

    canvas_clear(BG.0, BG.1, BG.2, 255);
    canvas_rect(0.0, 0.0, w, 52.0, ACCENT.0, ACCENT.1, ACCENT.2, 255);
    canvas_text(20.0, 14.0, 22.0, 255, 255, 255, 255, "Server-Sent Events");
    canvas_text(
        20.0,
        36.0,
        11.0,
        210,
        230,
        240,
        255,
        "EventSource streams with automatic reconnect",
    );

    let demo = unsafe {
        match (*core::ptr::addr_of_mut!(DEMO)).as_mut() {
            Some(d) => d,
            None => return,
        }
    };

    if demo.handle != 0 {
        while let Some(ev) = sse_recv(demo.handle) {
            demo.event_count += 1;
            let preview: String = ev.data.chars().take(90).collect();
            push_line(demo, format!("{} [{}] {}", ev.name, ev.id, preview));
        }
    }

    let state = if demo.handle == 0 {
        SSE_CLOSED
    } else {
        sse_state(demo.handle)
    };
    let (label, color) = state_label(state);

    canvas_text(20.0, 70.0, 12.0, DIM.0, DIM.1, DIM.2, 255, "url");
    canvas_text(
        70.0,
        70.0,
        12.0,
        BRIGHT.0,
        BRIGHT.1,
        BRIGHT.2,
        255,
        DEFAULT_URL,
    );
    canvas_text(20.0, 92.0, 12.0, DIM.0, DIM.1, DIM.2, 255, "state");
    canvas_text(70.0, 92.0, 12.0, color.0, color.1, color.2, 255, label);
    canvas_text(
        160.0,
        92.0,
        12.0,
        DIM.0,
        DIM.1,
        DIM.2,
        255,
        &format!("events: {}", demo.event_count),
    );

    if state == SSE_ERROR {
        let err = sse_error(demo.handle);
        if !err.is_empty() {
            canvas_text(20.0, 114.0, 12.0, RED.0, RED.1, RED.2, 255, &err);
        }
    }

    ui_button(BTN_CONNECT, 20.0, 136.0, 110.0, 28.0, "Connect", || {
        let demo = unsafe { (*core::ptr::addr_of_mut!(DEMO)).as_mut().unwrap() };
        if demo.handle != 0 {
            sse_close(demo.handle);
            sse_remove(demo.handle);
        }
        demo.handle = sse_open(DEFAULT_URL);
        demo.event_count = 0;
        demo.lines.clear();
        if demo.handle == 0 {
            push_line(demo, "sse_open failed".into());
        }
    });
    ui_button(BTN_CLOSE, 140.0, 136.0, 110.0, 28.0, "Close", || {
        let demo = unsafe { (*core::ptr::addr_of_mut!(DEMO)).as_mut().unwrap() };
        if demo.handle != 0 {
            sse_close(demo.handle);
            sse_remove(demo.handle);
            demo.handle = 0;
            push_line(demo, "closed".into());
        }
    });

    canvas_line(20.0, 180.0, w - 20.0, 180.0, 45, 45, 60, 255, 1.0);
    canvas_text(20.0, 192.0, 14.0, DIM.0, DIM.1, DIM.2, 255, "EVENTS");

    let mut y = 218.0;
    for line in &demo.lines {
        canvas_text(20.0, y, 12.0, BRIGHT.0, BRIGHT.1, BRIGHT.2, 255, line);
        y += 18.0;
    }
}
