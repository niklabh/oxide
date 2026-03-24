use oxide_sdk::*;

const BG: (u8, u8, u8) = (25, 25, 40);
const ACCENT: (u8, u8, u8) = (80, 160, 220);
const DIM: (u8, u8, u8) = (140, 130, 160);
const BRIGHT: (u8, u8, u8) = (230, 220, 255);
const GREEN: (u8, u8, u8) = (80, 220, 120);
const ORANGE: (u8, u8, u8) = (240, 180, 60);
const RED: (u8, u8, u8) = (220, 80, 80);
const CYAN: (u8, u8, u8) = (80, 220, 220);

// Callback IDs for on_timer
const CB_COUNTDOWN: u32 = 1;
const CB_DELAYED_MSG: u32 = 2;
const CB_BLINK: u32 = 3;
const CB_STOPWATCH: u32 = 4;

// Widget / button IDs
const BTN_START_COUNTDOWN: u32 = 100;
const BTN_FIRE_DELAY: u32 = 101;
const BTN_TOGGLE_BLINK: u32 = 102;
const BTN_STOPWATCH_START: u32 = 103;
const BTN_STOPWATCH_STOP: u32 = 104;
const BTN_STOPWATCH_RESET: u32 = 105;

static mut COUNTDOWN: i32 = 0;
static mut COUNTDOWN_TIMER: u32 = 0;
static mut DELAYED_MSG: &str = "";
static mut BLINK_ON: bool = false;
static mut BLINK_TIMER: u32 = 0;
static mut STOPWATCH_MS: u64 = 0;
static mut STOPWATCH_TIMER: u32 = 0;

#[no_mangle]
pub extern "C" fn start_app() {
    log("Timer Demo loaded!");
}

#[no_mangle]
pub extern "C" fn on_timer(callback_id: u32) {
    match callback_id {
        CB_COUNTDOWN => unsafe {
            COUNTDOWN -= 1;
            if COUNTDOWN <= 0 {
                COUNTDOWN = 0;
                clear_timer(COUNTDOWN_TIMER);
                COUNTDOWN_TIMER = 0;
            }
        },
        CB_DELAYED_MSG => unsafe {
            DELAYED_MSG = "Timer fired! This appeared after 3 seconds.";
        },
        CB_BLINK => unsafe {
            BLINK_ON = !BLINK_ON;
        },
        CB_STOPWATCH => unsafe {
            STOPWATCH_MS += 100;
        },
        _ => {}
    }
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let (width, _height) = canvas_dimensions();
    let w = width as f32;

    canvas_clear(BG.0, BG.1, BG.2, 255);

    // ── Header ──────────────────────────────────────────────────────
    canvas_rect(0.0, 0.0, w, 52.0, ACCENT.0, ACCENT.1, ACCENT.2, 255);
    canvas_text(20.0, 14.0, 22.0, 255, 255, 255, "Oxide Timer Demo");
    canvas_text(
        20.0,
        36.0,
        11.0,
        200,
        220,
        255,
        "set_timeout / set_interval / clear_timer",
    );

    // ── Countdown (set_interval) ────────────────────────────────────
    canvas_text(
        20.0,
        72.0,
        14.0,
        DIM.0,
        DIM.1,
        DIM.2,
        "COUNTDOWN (set_interval)",
    );

    let countdown = unsafe { COUNTDOWN };
    let running = unsafe { COUNTDOWN_TIMER } != 0;

    if ui_button(
        BTN_START_COUNTDOWN,
        20.0,
        95.0,
        140.0,
        30.0,
        "Start from 10",
    ) && !running
    {
        unsafe {
            COUNTDOWN = 10;
            COUNTDOWN_TIMER = set_interval(CB_COUNTDOWN, 1000);
        }
    }

    let (count_text, count_color) = if countdown > 3 {
        (format!("{countdown}"), GREEN)
    } else if countdown > 0 {
        (format!("{countdown}"), ORANGE)
    } else if running {
        ("0".into(), RED)
    } else {
        ("--".into(), DIM)
    };

    canvas_text(
        200.0,
        100.0,
        28.0,
        count_color.0,
        count_color.1,
        count_color.2,
        &count_text,
    );

    if countdown == 0 && !running {
        canvas_text(260.0, 105.0, 14.0, DIM.0, DIM.1, DIM.2, "Done!");
    }

    // ── Delayed Message (set_timeout) ───────────────────────────────
    canvas_line(20.0, 145.0, w - 20.0, 145.0, 40, 35, 60, 1.0);
    canvas_text(
        20.0,
        160.0,
        14.0,
        DIM.0,
        DIM.1,
        DIM.2,
        "DELAYED MESSAGE (set_timeout)",
    );

    if ui_button(BTN_FIRE_DELAY, 20.0, 183.0, 180.0, 30.0, "Fire after 3 sec") {
        unsafe { DELAYED_MSG = "Waiting..." };
        set_timeout(CB_DELAYED_MSG, 3000);
    }

    let msg = unsafe { DELAYED_MSG };
    if !msg.is_empty() {
        let color = if msg.starts_with("Waiting") {
            ORANGE
        } else {
            GREEN
        };
        canvas_text(220.0, 190.0, 14.0, color.0, color.1, color.2, msg);
    }

    // ── Blink (set_interval + clear_timer) ──────────────────────────
    canvas_line(20.0, 230.0, w - 20.0, 230.0, 40, 35, 60, 1.0);
    canvas_text(
        20.0,
        245.0,
        14.0,
        DIM.0,
        DIM.1,
        DIM.2,
        "BLINK (interval + clear)",
    );

    let blinking = unsafe { BLINK_TIMER } != 0;
    let label = if blinking {
        "Stop Blink"
    } else {
        "Start Blink"
    };
    if ui_button(BTN_TOGGLE_BLINK, 20.0, 268.0, 120.0, 30.0, label) {
        unsafe {
            if blinking {
                clear_timer(BLINK_TIMER);
                BLINK_TIMER = 0;
                BLINK_ON = false;
            } else {
                BLINK_ON = true;
                BLINK_TIMER = set_interval(CB_BLINK, 500);
            }
        }
    }

    let blink_on = unsafe { BLINK_ON };
    if blink_on {
        canvas_circle(200.0, 283.0, 14.0, CYAN.0, CYAN.1, CYAN.2, 255);
    } else {
        canvas_circle(200.0, 283.0, 14.0, 50, 50, 60, 255);
    }

    canvas_text(
        230.0,
        276.0,
        13.0,
        DIM.0,
        DIM.1,
        DIM.2,
        if blinking {
            "Toggling every 500ms"
        } else {
            "Idle"
        },
    );

    // ── Stopwatch (set_interval + clear_timer) ──────────────────────
    canvas_line(20.0, 315.0, w - 20.0, 315.0, 40, 35, 60, 1.0);
    canvas_text(
        20.0,
        330.0,
        14.0,
        DIM.0,
        DIM.1,
        DIM.2,
        "STOPWATCH (100ms interval)",
    );

    let sw_running = unsafe { STOPWATCH_TIMER } != 0;
    let sw_ms = unsafe { STOPWATCH_MS };

    if ui_button(BTN_STOPWATCH_START, 20.0, 353.0, 80.0, 30.0, "Start") && !sw_running {
        unsafe {
            STOPWATCH_TIMER = set_interval(CB_STOPWATCH, 100);
        }
    }
    if ui_button(BTN_STOPWATCH_STOP, 110.0, 353.0, 80.0, 30.0, "Stop") && sw_running {
        unsafe {
            clear_timer(STOPWATCH_TIMER);
            STOPWATCH_TIMER = 0;
        }
    }
    if ui_button(BTN_STOPWATCH_RESET, 200.0, 353.0, 80.0, 30.0, "Reset") && !sw_running {
        unsafe { STOPWATCH_MS = 0 };
    }

    let secs = sw_ms / 1000;
    let tenths = (sw_ms % 1000) / 100;
    canvas_text(
        310.0,
        355.0,
        28.0,
        BRIGHT.0,
        BRIGHT.1,
        BRIGHT.2,
        &format!("{secs}.{tenths}s"),
    );

    // ── Info ─────────────────────────────────────────────────────────
    canvas_line(20.0, 405.0, w - 20.0, 405.0, 40, 35, 60, 1.0);
    canvas_text(
        20.0,
        420.0,
        12.0,
        DIM.0,
        DIM.1,
        DIM.2,
        "Timers fire via exported on_timer(callback_id). Intervals repeat until cleared.",
    );
    canvas_text(
        20.0,
        440.0,
        12.0,
        DIM.0,
        DIM.1,
        DIM.2,
        "Resolution is tied to the frame rate (~16ms at 60fps).",
    );
}
