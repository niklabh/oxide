//! Demonstrates the Oxide event system: built-in events (`resize`, `focus`,
//! `blur`, `visibility_change`, `online`/`offline`, `touch_*`, `gamepad_*`,
//! `drop_files`) plus custom events emitted from a button click.

#![allow(static_mut_refs)]

use oxide_sdk::*;

const BG: (u8, u8, u8) = (24, 24, 36);
const ACCENT: (u8, u8, u8) = (110, 180, 220);
const DIM: (u8, u8, u8) = (140, 130, 160);
const BRIGHT: (u8, u8, u8) = (235, 230, 250);
const GREEN: (u8, u8, u8) = (110, 220, 140);
const ORANGE: (u8, u8, u8) = (240, 180, 70);
const PINK: (u8, u8, u8) = (240, 140, 200);

const CB_RESIZE: u32 = 1;
const CB_FOCUS: u32 = 2;
const CB_BLUR: u32 = 3;
const CB_VISIBILITY: u32 = 4;
const CB_ONLINE: u32 = 5;
const CB_OFFLINE: u32 = 6;
const CB_TOUCH_START: u32 = 7;
const CB_TOUCH_MOVE: u32 = 8;
const CB_TOUCH_END: u32 = 9;
const CB_GAMEPAD_BTN: u32 = 10;
const CB_GAMEPAD_AXIS: u32 = 11;
const CB_GAMEPAD_CONNECTED: u32 = 12;
const CB_DROP_FILES: u32 = 13;
const CB_PING: u32 = 100;

const BTN_PING: u32 = 200;
const BTN_CLEAR: u32 = 201;

static mut LAST_EVENT: String = String::new();
static mut LAST_RESIZE: (u32, u32) = (0, 0);
static mut TOUCH_POS: (f32, f32) = (0.0, 0.0);
static mut TOUCHING: bool = false;
static mut FOCUS_STATE: &str = "unknown";
static mut ONLINE_STATE: &str = "unknown";
static mut LAST_DROP: String = String::new();
static mut PING_COUNT: u32 = 0;
static mut GAMEPAD_LINE: String = String::new();

fn read_u32_le(b: &[u8], offset: usize) -> u32 {
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&b[offset..offset + 4]);
    u32::from_le_bytes(buf)
}

fn read_f32_le(b: &[u8], offset: usize) -> f32 {
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&b[offset..offset + 4]);
    f32::from_le_bytes(buf)
}

#[no_mangle]
pub extern "C" fn start_app() {
    log("events-demo: registering listeners");
    // Fully qualify so we don't shadow the `on_event` export below.
    oxide_sdk::on_event("resize", CB_RESIZE);
    oxide_sdk::on_event("focus", CB_FOCUS);
    oxide_sdk::on_event("blur", CB_BLUR);
    oxide_sdk::on_event("visibility_change", CB_VISIBILITY);
    oxide_sdk::on_event("online", CB_ONLINE);
    oxide_sdk::on_event("offline", CB_OFFLINE);
    oxide_sdk::on_event("touch_start", CB_TOUCH_START);
    oxide_sdk::on_event("touch_move", CB_TOUCH_MOVE);
    oxide_sdk::on_event("touch_end", CB_TOUCH_END);
    oxide_sdk::on_event("gamepad_connected", CB_GAMEPAD_CONNECTED);
    oxide_sdk::on_event("gamepad_button", CB_GAMEPAD_BTN);
    oxide_sdk::on_event("gamepad_axis", CB_GAMEPAD_AXIS);
    oxide_sdk::on_event("drop_files", CB_DROP_FILES);
    oxide_sdk::on_event("ping", CB_PING);
}

#[no_mangle]
pub extern "C" fn on_event(callback_id: u32) {
    let etype = event_type();
    let data = event_data_into();
    unsafe {
        LAST_EVENT = format!("{etype} ({} bytes)", data.len());
    }
    match callback_id {
        CB_RESIZE if data.len() == 8 => {
            let w = read_u32_le(&data, 0);
            let h = read_u32_le(&data, 4);
            unsafe {
                LAST_RESIZE = (w, h);
            }
        }
        CB_FOCUS => unsafe {
            FOCUS_STATE = "focused";
        },
        CB_BLUR => unsafe {
            FOCUS_STATE = "blurred";
        },
        CB_VISIBILITY => {
            let s = String::from_utf8_lossy(&data).into_owned();
            log(&format!("visibility_change: {s}"));
        }
        CB_ONLINE => unsafe {
            ONLINE_STATE = "online";
        },
        CB_OFFLINE => unsafe {
            ONLINE_STATE = "offline";
        },
        CB_TOUCH_START if data.len() == 8 => {
            let x = read_f32_le(&data, 0);
            let y = read_f32_le(&data, 4);
            unsafe {
                TOUCHING = true;
                TOUCH_POS = (x, y);
            }
        }
        CB_TOUCH_MOVE if data.len() == 8 => {
            let x = read_f32_le(&data, 0);
            let y = read_f32_le(&data, 4);
            unsafe {
                TOUCH_POS = (x, y);
            }
        }
        CB_TOUCH_END => unsafe {
            TOUCHING = false;
        },
        CB_GAMEPAD_CONNECTED => {
            let name = String::from_utf8_lossy(&data).into_owned();
            unsafe {
                GAMEPAD_LINE = format!("connected: {name}");
            }
        }
        CB_GAMEPAD_BTN if data.len() == 12 => {
            let id = read_u32_le(&data, 0);
            let code = read_u32_le(&data, 4);
            let pressed = read_u32_le(&data, 8) != 0;
            unsafe {
                GAMEPAD_LINE = format!(
                    "gamepad #{id} button code={code} {}",
                    if pressed { "DOWN" } else { "up" }
                );
            }
        }
        CB_GAMEPAD_AXIS if data.len() == 12 => {
            let id = read_u32_le(&data, 0);
            let code = read_u32_le(&data, 4);
            let v = read_f32_le(&data, 8);
            unsafe {
                GAMEPAD_LINE = format!("gamepad #{id} axis code={code} value={v:+.2}");
            }
        }
        CB_DROP_FILES => {
            let s = String::from_utf8_lossy(&data).into_owned();
            unsafe {
                LAST_DROP = s.clone();
            }
            log(&format!("drop_files: {s}"));
        }
        CB_PING => unsafe {
            PING_COUNT += 1;
        },
        _ => {}
    }
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let (w, _h) = canvas_dimensions();
    let w = w as f32;

    canvas_clear(BG.0, BG.1, BG.2, 255);

    canvas_rect(0.0, 0.0, w, 52.0, ACCENT.0, ACCENT.1, ACCENT.2, 255);
    canvas_text(20.0, 14.0, 22.0, 255, 255, 255, 255, "Oxide Event System");
    canvas_text(
        20.0,
        36.0,
        11.0,
        20,
        20,
        40,
        255,
        "on_event / emit_event / built-in events",
    );

    let mut y = 72.0;

    let last = unsafe { LAST_EVENT.clone() };
    canvas_text(
        20.0,
        y,
        14.0,
        DIM.0,
        DIM.1,
        DIM.2,
        255,
        "Last event delivered:",
    );
    canvas_text(
        220.0,
        y,
        14.0,
        BRIGHT.0,
        BRIGHT.1,
        BRIGHT.2,
        255,
        if last.is_empty() { "(none yet)" } else { &last },
    );
    y += 28.0;

    let (rw, rh) = unsafe { LAST_RESIZE };
    canvas_text(20.0, y, 14.0, DIM.0, DIM.1, DIM.2, 255, "Canvas resize:");
    canvas_text(
        220.0,
        y,
        14.0,
        BRIGHT.0,
        BRIGHT.1,
        BRIGHT.2,
        255,
        &if rw == 0 {
            "(no resize yet — try resizing the window)".to_string()
        } else {
            format!("{rw} x {rh}")
        },
    );
    y += 28.0;

    let f = unsafe { FOCUS_STATE };
    canvas_text(20.0, y, 14.0, DIM.0, DIM.1, DIM.2, 255, "Focus state:");
    let fc = if f == "focused" { GREEN } else { ORANGE };
    canvas_text(220.0, y, 14.0, fc.0, fc.1, fc.2, 255, f);
    y += 28.0;

    let o = unsafe { ONLINE_STATE };
    canvas_text(20.0, y, 14.0, DIM.0, DIM.1, DIM.2, 255, "Network:");
    let oc = if o == "online" { GREEN } else { ORANGE };
    canvas_text(220.0, y, 14.0, oc.0, oc.1, oc.2, 255, o);
    y += 28.0;

    let touching = unsafe { TOUCHING };
    let (tx, ty) = unsafe { TOUCH_POS };
    canvas_text(20.0, y, 14.0, DIM.0, DIM.1, DIM.2, 255, "Touch (mouse):");
    if touching {
        canvas_text(
            220.0,
            y,
            14.0,
            PINK.0,
            PINK.1,
            PINK.2,
            255,
            &format!("DOWN at ({tx:.0}, {ty:.0})"),
        );
        canvas_circle(tx, ty, 18.0, PINK.0, PINK.1, PINK.2, 200);
    } else {
        canvas_text(220.0, y, 14.0, DIM.0, DIM.1, DIM.2, 255, "(release)");
    }
    y += 28.0;

    let gp = unsafe { GAMEPAD_LINE.clone() };
    canvas_text(20.0, y, 14.0, DIM.0, DIM.1, DIM.2, 255, "Gamepad:");
    canvas_text(
        220.0,
        y,
        14.0,
        BRIGHT.0,
        BRIGHT.1,
        BRIGHT.2,
        255,
        if gp.is_empty() {
            "(no gamepad input yet)"
        } else {
            &gp
        },
    );
    y += 28.0;

    let drop = unsafe { LAST_DROP.clone() };
    canvas_text(20.0, y, 14.0, DIM.0, DIM.1, DIM.2, 255, "Last drop:");
    canvas_text(
        220.0,
        y,
        14.0,
        BRIGHT.0,
        BRIGHT.1,
        BRIGHT.2,
        255,
        if drop.is_empty() {
            "(drag a file onto this window)"
        } else {
            &drop
        },
    );
    y += 32.0;

    canvas_line(20.0, y, w - 20.0, y, 50, 45, 70, 255, 1.0);
    y += 16.0;

    canvas_text(
        20.0,
        y,
        14.0,
        DIM.0,
        DIM.1,
        DIM.2,
        255,
        "Custom event (emit_event / on_event):",
    );
    y += 22.0;

    if ui_button(BTN_PING, 20.0, y, 140.0, 30.0, "Emit \"ping\"") {
        emit_event("ping", b"hello");
    }
    if ui_button(BTN_CLEAR, 170.0, y, 140.0, 30.0, "Reset count") {
        unsafe {
            PING_COUNT = 0;
        }
    }
    let count = unsafe { PING_COUNT };
    canvas_text(
        330.0,
        y + 8.0,
        16.0,
        BRIGHT.0,
        BRIGHT.1,
        BRIGHT.2,
        255,
        &format!("ping count: {count}"),
    );
}
