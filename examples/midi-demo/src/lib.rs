//! MIDI monitor and piano keyboard visualizer for the Oxide browser.
//!
//! Demonstrates the Oxide MIDI API: enumerate input/output devices, open a
//! connection, receive note-on/off and CC messages in real time, and render
//! an interactive piano keyboard that lights up as notes arrive.
//!
//! # Building
//!
//! ```bash
//! cargo build --target wasm32-unknown-unknown --release -p midi-demo
//! ```
//!
//! Then open `target/wasm32-unknown-unknown/release/midi_demo.wasm` in Oxide.

use oxide_sdk::*;

// ── Layout constants ──────────────────────────────────────────────────────────

const HEADER_H: f32 = 48.0;
const DEVICE_PANEL_H: f32 = 110.0;
const LOG_H: f32 = 200.0;
const PIANO_H: f32 = 110.0;

// Piano: show C2–B5 (MIDI notes 36–83), 4 octaves = 28 white keys
const PIANO_START_NOTE: u8 = 36; // C2
const PIANO_END_NOTE: u8 = 83; // B5
const WHITE_KEY_W: f32 = 22.0;
const WHITE_KEY_H: f32 = 80.0;
const BLACK_KEY_W: f32 = 13.0;
const BLACK_KEY_H: f32 = 50.0;

// Message log ring buffer
const LOG_SIZE: usize = 16;
const MSG_LEN: usize = 64;

// Max MIDI devices shown
const MAX_DEVICES: usize = 8;
const DEV_NAME_LEN: usize = 64;
// Max MIDI message size for stack buffers
const MIDI_MSG_MAX: usize = 16;

// ── State ─────────────────────────────────────────────────────────────────────

struct AppState {
    // Device list (scanned once in start_app)
    input_count: u32,
    input_names: [[u8; DEV_NAME_LEN]; MAX_DEVICES],
    input_name_lens: [usize; MAX_DEVICES],

    // Active connection
    port_handle: u32,
    connected_idx: i32, // -1 = none

    // Note state: true = held
    active_notes: [bool; 128],

    // Message log ring buffer: (text bytes, len)
    log: [[u8; MSG_LEN]; LOG_SIZE],
    log_lens: [usize; LOG_SIZE],
    log_head: usize, // index of oldest entry
    log_count: usize,
}

impl AppState {
    const fn new() -> Self {
        Self {
            input_count: 0,
            input_names: [[0u8; DEV_NAME_LEN]; MAX_DEVICES],
            input_name_lens: [0usize; MAX_DEVICES],
            port_handle: 0,
            connected_idx: -1,
            active_notes: [false; 128],
            log: [[0u8; MSG_LEN]; LOG_SIZE],
            log_lens: [0usize; LOG_SIZE],
            log_head: 0,
            log_count: 0,
        }
    }

    fn push_log(&mut self, text: &str) {
        let slot = (self.log_head + self.log_count) % LOG_SIZE;
        let bytes = text.as_bytes();
        let len = bytes.len().min(MSG_LEN);
        self.log[slot][..len].copy_from_slice(&bytes[..len]);
        self.log_lens[slot] = len;
        if self.log_count < LOG_SIZE {
            self.log_count += 1;
        } else {
            // Ring is full — advance head to drop oldest
            self.log_head = (self.log_head + 1) % LOG_SIZE;
        }
    }

    fn log_entry(&self, age: usize) -> &str {
        // age=0 is newest
        let count = self.log_count;
        if age >= count {
            return "";
        }
        let idx = (self.log_head + count - 1 - age) % LOG_SIZE;
        let len = self.log_lens[idx];
        core::str::from_utf8(&self.log[idx][..len]).unwrap_or("")
    }

    fn device_name(&self, idx: usize) -> &str {
        if idx >= MAX_DEVICES {
            return "";
        }
        let len = self.input_name_lens[idx];
        core::str::from_utf8(&self.input_names[idx][..len]).unwrap_or("")
    }
}

static mut STATE: AppState = AppState::new();

// ── Entry points ──────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn start_app() {
    let s = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };

    let count = midi_input_count().min(MAX_DEVICES as u32);
    s.input_count = count;
    for i in 0..count as usize {
        let name = midi_input_name(i as u32);
        let bytes = name.as_bytes();
        let len = bytes.len().min(DEV_NAME_LEN);
        s.input_names[i][..len].copy_from_slice(&bytes[..len]);
        s.input_name_lens[i] = len;
    }

    if count == 0 {
        log("MIDI Demo: no input devices found");
    } else {
        log(&format!("MIDI Demo: {} input device(s) found", count));
    }
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let s = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    let (w_u, h_u) = canvas_dimensions();
    let w = w_u as f32;
    let h = h_u as f32;

    // Drain incoming MIDI messages (up to 32 per frame to avoid stalling).
    if s.port_handle > 0 {
        for _ in 0..32 {
            match midi_recv(s.port_handle) {
                Some(data) => {
                    // Copy into a fixed-size stack array to release the Vec
                    // before mutably borrowing `s` in handle_midi_message.
                    let mut buf = [0u8; MIDI_MSG_MAX];
                    let len = data.len().min(MIDI_MSG_MAX);
                    buf[..len].copy_from_slice(&data[..len]);
                    handle_midi_message(s, &buf[..len]);
                }
                None => break,
            }
        }
    }

    // ── Background ────────────────────────────────────────────────────────
    canvas_clear(18, 22, 36, 255);

    // ── Header ────────────────────────────────────────────────────────────
    canvas_rect(0.0, 0.0, w, HEADER_H, 28, 36, 60, 255);
    canvas_text(16.0, 13.0, 22.0, 160, 200, 255, 255, "Oxide MIDI Monitor");
    let status_text = if s.port_handle > 0 {
        "CONNECTED"
    } else {
        "DISCONNECTED"
    };
    let (sr, sg, sb) = if s.port_handle > 0 {
        (40u8, 200u8, 100u8)
    } else {
        (120u8, 120u8, 140u8)
    };
    canvas_rounded_rect(w - 140.0, 12.0, 126.0, 24.0, 12.0, sr, sg, sb, 40);
    canvas_text(w - 132.0, 17.0, 13.0, sr, sg, sb, 255, status_text);

    // ── Device panel ──────────────────────────────────────────────────────
    let dev_y = HEADER_H + 4.0;
    canvas_text(
        16.0,
        dev_y + 4.0,
        13.0,
        140,
        150,
        180,
        255,
        "Input Devices:",
    );

    if s.input_count == 0 {
        canvas_text(
            16.0,
            dev_y + 22.0,
            13.0,
            100,
            110,
            130,
            255,
            "No MIDI input devices found. Connect a device and reload.",
        );
    } else {
        let btn_y = dev_y + 20.0;
        for i in 0..s.input_count as usize {
            let btn_x = 16.0 + i as f32 * 190.0;
            let is_active = s.connected_idx == i as i32;

            // Copy name into a local stack buffer so we don't hold a borrow
            // on `s` while calling SDK functions that need `&mut s`.
            let mut name_buf = [0u8; DEV_NAME_LEN];
            let name_len = {
                let name_bytes = s.device_name(i).as_bytes();
                let len = name_bytes.len().min(DEV_NAME_LEN);
                name_buf[..len].copy_from_slice(&name_bytes[..len]);
                len
            };
            let name = core::str::from_utf8(&name_buf[..name_len]).unwrap_or("");

            let (br, bg, bb) = if is_active {
                (50u8, 140u8, 220u8)
            } else {
                (40u8, 50u8, 80u8)
            };
            canvas_rounded_rect(btn_x, btn_y, 178.0, 32.0, 6.0, br, bg, bb, 255);
            let display = if name.len() > 22 { &name[..22] } else { name };
            canvas_text(btn_x + 8.0, btn_y + 8.0, 13.0, 230, 235, 255, 255, display);

            if ui_button((10 + i) as u32, btn_x, btn_y, 178.0, 32.0, "") {
                if is_active {
                    midi_close(s.port_handle);
                    s.port_handle = 0;
                    s.connected_idx = -1;
                    s.active_notes = [false; 128];
                    s.push_log("Disconnected");
                } else {
                    if s.port_handle > 0 {
                        midi_close(s.port_handle);
                        s.active_notes = [false; 128];
                    }
                    let h = midi_open_input(i as u32);
                    s.port_handle = h;
                    if h > 0 {
                        s.connected_idx = i as i32;
                        let mut msg = [0u8; 80];
                        let prefix = b"Connected to: ";
                        let plen = prefix.len();
                        let nlen = name_len.min(80 - plen);
                        msg[..plen].copy_from_slice(prefix);
                        msg[plen..plen + nlen].copy_from_slice(&name_buf[..nlen]);
                        let full = core::str::from_utf8(&msg[..plen + nlen]).unwrap_or("");
                        s.push_log(full);
                    } else {
                        s.connected_idx = -1;
                        s.push_log("Failed to open MIDI device");
                    }
                }
            }
        }
    }

    // Hint text below device buttons
    if s.input_count > 0 {
        canvas_text(
            16.0,
            dev_y + 60.0,
            12.0,
            100,
            110,
            130,
            255,
            "Click a device button to connect / disconnect.",
        );
    }

    // ── Message log ───────────────────────────────────────────────────────
    let log_y = HEADER_H + DEVICE_PANEL_H;
    canvas_rect(0.0, log_y, w, LOG_H, 14, 18, 30, 255);
    canvas_text(16.0, log_y + 6.0, 13.0, 140, 150, 180, 255, "Messages:");

    let line_h = 18.0;
    let visible = ((LOG_H - 26.0) / line_h) as usize;
    for i in 0..visible.min(s.log_count) {
        let text = s.log_entry(i);
        let y = log_y + 24.0 + i as f32 * line_h;
        let (r, g, b) = log_color(text);
        canvas_text(16.0, y, 13.0, r, g, b, 255, text);
    }
    if s.log_count == 0 {
        canvas_text(
            16.0,
            log_y + 28.0,
            13.0,
            70,
            80,
            100,
            255,
            "Waiting for MIDI messages…",
        );
    }

    // ── Piano keyboard ────────────────────────────────────────────────────
    let piano_y = h - PIANO_H - 8.0;
    draw_piano(s, 16.0, piano_y, w);
}

// ── MIDI message handler ──────────────────────────────────────────────────────

fn handle_midi_message(s: &mut AppState, msg: &[u8]) {
    if msg.is_empty() {
        return;
    }
    let status = msg[0];
    let kind = status & 0xF0;
    let ch = (status & 0x0F) + 1;

    match kind {
        0x90 if msg.len() >= 3 => {
            let note = msg[1];
            let vel = msg[2];
            if vel > 0 {
                s.active_notes[note as usize] = true;
                let text = format_note_on(ch, note, vel);
                s.push_log(&text);
            } else {
                // Note On with vel=0 is a Note Off
                s.active_notes[note as usize] = false;
                let text = format_note_off(ch, note);
                s.push_log(&text);
            }
        }
        0x80 if msg.len() >= 3 => {
            let note = msg[1];
            s.active_notes[note as usize] = false;
            let text = format_note_off(ch, msg[1]);
            s.push_log(&text);
        }
        0xB0 if msg.len() >= 3 => {
            let cc = msg[1];
            let val = msg[2];
            let mut buf = [0u8; 48];
            let text = format_cc(&mut buf, ch, cc, val);
            s.push_log(text);
        }
        0xC0 if msg.len() >= 2 => {
            let mut buf = [0u8; 32];
            let text = format_prog(&mut buf, ch, msg[1]);
            s.push_log(text);
        }
        0xE0 if msg.len() >= 3 => {
            let lsb = msg[1] as i16;
            let msb = msg[2] as i16;
            let bend = ((msb << 7) | lsb) - 8192;
            let mut buf = [0u8; 32];
            let text = format_pitchbend(&mut buf, ch, bend);
            s.push_log(text);
        }
        _ => {
            // Unknown / SysEx — show raw hex
            let mut buf = [0u8; 48];
            let n = format_hex(&mut buf, msg);
            let text = core::str::from_utf8(&buf[..n]).unwrap_or("?");
            s.push_log(text);
        }
    }
}

// ── Piano drawing ─────────────────────────────────────────────────────────────

fn draw_piano(s: &AppState, x: f32, y: f32, canvas_w: f32) {
    // Center the keyboard
    let white_count = count_white_keys(PIANO_START_NOTE, PIANO_END_NOTE);
    let total_w = white_count as f32 * WHITE_KEY_W;
    let offset_x = x + ((canvas_w - 32.0 - total_w) * 0.5).max(0.0);

    // White keys first
    let mut wx = offset_x;
    let mut note = PIANO_START_NOTE;
    while note <= PIANO_END_NOTE {
        if is_white(note) {
            let active = s.active_notes[note as usize];
            let (r, g, b) = if active {
                (100u8, 180u8, 255u8)
            } else {
                (230u8, 230u8, 230u8)
            };
            canvas_rounded_rect(wx, y, WHITE_KEY_W - 1.0, WHITE_KEY_H, 3.0, r, g, b, 255);
            wx += WHITE_KEY_W;
        }
        note += 1;
    }

    // Black keys on top
    wx = offset_x;
    note = PIANO_START_NOTE;
    while note <= PIANO_END_NOTE {
        if is_white(note) {
            if note < PIANO_END_NOTE && !is_white(note + 1) {
                // There's a black key after this white key
                let bx = wx + WHITE_KEY_W - BLACK_KEY_W * 0.5 - 1.0;
                let black_note = note + 1;
                let active = s.active_notes[black_note as usize];
                let (r, g, b) = if active {
                    (60u8, 140u8, 230u8)
                } else {
                    (30u8, 35u8, 50u8)
                };
                canvas_rounded_rect(bx, y, BLACK_KEY_W, BLACK_KEY_H, 2.0, r, g, b, 255);
            }
            wx += WHITE_KEY_W;
        }
        note += 1;
    }

    // Label: octave markers (C notes)
    wx = offset_x;
    note = PIANO_START_NOTE;
    while note <= PIANO_END_NOTE {
        if is_white(note) {
            if note.is_multiple_of(12) {
                // C note
                let octave = note / 12 - 1; // MIDI C4 = note 60 = octave 4
                let label = match octave {
                    1 => "C1",
                    2 => "C2",
                    3 => "C3",
                    4 => "C4",
                    5 => "C5",
                    6 => "C6",
                    7 => "C7",
                    _ => "C",
                };
                canvas_text(
                    wx + 2.0,
                    y + WHITE_KEY_H - 14.0,
                    10.0,
                    80,
                    90,
                    110,
                    255,
                    label,
                );
            }
            wx += WHITE_KEY_W;
        }
        note += 1;
    }
}

fn is_white(note: u8) -> bool {
    matches!(note % 12, 0 | 2 | 4 | 5 | 7 | 9 | 11)
}

fn count_white_keys(start: u8, end: u8) -> u32 {
    let mut count = 0u32;
    let mut n = start;
    while n <= end {
        if is_white(n) {
            count += 1;
        }
        n += 1;
    }
    count
}

// ── Message formatting (no heap alloc) ───────────────────────────────────────

fn note_name(note: u8) -> &'static str {
    match note % 12 {
        0 => "C",
        1 => "C#",
        2 => "D",
        3 => "D#",
        4 => "E",
        5 => "F",
        6 => "F#",
        7 => "G",
        8 => "G#",
        9 => "A",
        10 => "A#",
        11 => "B",
        _ => "?",
    }
}

fn format_note_on(ch: u8, note: u8, vel: u8) -> alloc::string::String {
    let octave = (note / 12) as i32 - 1;
    alloc::format!(
        "▶ Note On   ch{:2}  {:3}{:2}  vel={:3}",
        ch,
        note_name(note),
        octave,
        vel
    )
}

fn format_note_off(ch: u8, note: u8) -> alloc::string::String {
    let octave = (note / 12) as i32 - 1;
    alloc::format!("■ Note Off  ch{:2}  {:3}{:2}", ch, note_name(note), octave)
}

fn format_cc(buf: &mut [u8; 48], ch: u8, cc: u8, val: u8) -> &str {
    let text = alloc::format!("~ CC        ch{:2}  cc={:3}  val={:3}", ch, cc, val);
    let bytes = text.as_bytes();
    let n = bytes.len().min(buf.len());
    buf[..n].copy_from_slice(&bytes[..n]);
    core::str::from_utf8(&buf[..n]).unwrap_or("")
}

fn format_prog(buf: &mut [u8; 32], ch: u8, prog: u8) -> &str {
    let text = alloc::format!("♪ Prog Chg  ch{:2}  prog={:3}", ch, prog);
    let bytes = text.as_bytes();
    let n = bytes.len().min(buf.len());
    buf[..n].copy_from_slice(&bytes[..n]);
    core::str::from_utf8(&buf[..n]).unwrap_or("")
}

fn format_pitchbend(buf: &mut [u8; 32], ch: u8, bend: i16) -> &str {
    let text = alloc::format!("↕ Pitch Bend ch{:2} {:+5}", ch, bend);
    let bytes = text.as_bytes();
    let n = bytes.len().min(buf.len());
    buf[..n].copy_from_slice(&bytes[..n]);
    core::str::from_utf8(&buf[..n]).unwrap_or("")
}

fn format_hex(buf: &mut [u8; 48], data: &[u8]) -> usize {
    let mut pos = 0usize;
    let prefix = b"? Hex: ";
    buf[..prefix.len()].copy_from_slice(prefix);
    pos += prefix.len();
    for byte in data.iter().take(8) {
        if pos + 3 > buf.len() {
            break;
        }
        let hi = byte >> 4;
        let lo = byte & 0xF;
        buf[pos] = hex_char(hi);
        buf[pos + 1] = hex_char(lo);
        buf[pos + 2] = b' ';
        pos += 3;
    }
    pos
}

fn hex_char(n: u8) -> u8 {
    if n < 10 {
        b'0' + n
    } else {
        b'a' + n - 10
    }
}

fn log_color(text: &str) -> (u8, u8, u8) {
    if text.starts_with('▶') {
        (80, 220, 140) // Note On — green
    } else if text.starts_with('■') {
        (140, 150, 180) // Note Off — grey
    } else if text.starts_with('~') {
        (200, 160, 80) // CC — amber
    } else if text.starts_with('♪') {
        (160, 120, 220) // Prog Change — purple
    } else if text.starts_with('↕') {
        (100, 180, 220) // Pitch Bend — blue
    } else if text.starts_with("Connected") {
        (60, 200, 140) // system
    } else {
        (140, 150, 170)
    }
}

extern crate alloc;
