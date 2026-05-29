//! Background worker demo — main app.
//!
//! Offloads a CPU-heavy prime count to a separate `.wasm` worker module so the
//! UI frame loop never blocks. The worker (`worker-demo-bg`) runs on its own
//! thread with isolated fuel and memory; the two communicate only by passing
//! byte messages.
//!
//! # Building
//!
//! ```bash
//! cargo build --target wasm32-unknown-unknown --release -p worker-demo
//! cargo build --target wasm32-unknown-unknown --release -p worker-demo-bg
//! ```
//!
//! Both `.wasm` files land in the same directory, so the worker URL is resolved
//! relative to this module's URL at runtime. Open `worker_demo.wasm` in the
//! browser (toolbar **Open** or a `file://` address).

use oxide_sdk::*;

/// Upper bound for the prime count handed to the worker.
const LIMIT: u32 = 80_000;

static mut STATE: State = State::new();

struct State {
    /// Handle of the spawned worker (0 = none).
    handle: u32,
    /// 0 = idle, 1 = computing, 2 = done, 3 = error.
    phase: u8,
    /// Result returned by the worker.
    result: u64,
    /// Spinner angle, accumulated from frame deltas.
    angle: f32,
}

impl State {
    const fn new() -> Self {
        Self {
            handle: 0,
            phase: 0,
            result: 0,
            angle: 0.0,
        }
    }
}

#[no_mangle]
pub extern "C" fn start_app() {
    log("Background worker demo loaded.");
}

#[no_mangle]
pub extern "C" fn on_frame(dt_ms: u32) {
    let s = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    let (w, _h) = canvas_dimensions();

    canvas_clear(18, 18, 28, 255);
    canvas_text(
        20.0,
        20.0,
        24.0,
        220,
        200,
        255,
        255,
        "Background Workers Demo",
    );
    canvas_text(
        20.0,
        56.0,
        14.0,
        150,
        150,
        170,
        255,
        &format!("Count primes below {LIMIT} on a separate worker thread."),
    );
    canvas_text(
        20.0,
        76.0,
        14.0,
        150,
        150,
        170,
        255,
        "The spinner keeps moving, proving the UI thread never blocks.",
    );

    if ui_button(1, 20.0, 110.0, 240.0, 32.0, "Run in background worker") {
        s.phase = 1;
        s.result = 0;
        let base = get_url();
        match url_resolve(&base, "worker_demo_bg.wasm") {
            Some(url) => {
                let handle = spawn_worker(&url);
                if handle > 0 {
                    s.handle = handle as u32;
                    worker_post_message(s.handle, &LIMIT.to_le_bytes());
                } else {
                    log("Failed to spawn worker.");
                    s.phase = 3;
                }
            }
            None => {
                log(&format!("Could not resolve worker URL from base '{base}'."));
                s.phase = 3;
            }
        }
    }

    // Non-blocking poll for the worker's reply.
    if s.phase == 1 {
        if let Some(bytes) = worker_recv(s.handle) {
            if bytes.len() >= 8 {
                let mut arr = [0u8; 8];
                arr.copy_from_slice(&bytes[..8]);
                s.result = u64::from_le_bytes(arr);
                s.phase = 2;
                worker_terminate(s.handle);
            }
        }
    }

    let (status, r, g, b) = match s.phase {
        0 => (
            "Idle — click the button to start.".to_string(),
            160,
            220,
            160,
        ),
        1 => ("Computing on worker…".to_string(), 220, 200, 120),
        2 => (
            format!("Done: {} primes below {LIMIT}.", s.result),
            160,
            220,
            160,
        ),
        _ => (
            "Failed to start worker (see console).".to_string(),
            240,
            120,
            120,
        ),
    };
    canvas_text(20.0, 165.0, 16.0, r, g, b, 255, &status);

    // Spinner animated from accumulated frame deltas (independent of the worker).
    s.angle += dt_ms as f32 * 0.004;
    let cx = w as f32 - 60.0;
    let cy = 126.0;
    canvas_circle(
        cx + s.angle.cos() * 18.0,
        cy + s.angle.sin() * 18.0,
        7.0,
        120,
        180,
        255,
        255,
    );
}
