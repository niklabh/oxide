//! Demo of the crypto, compression, and system info APIs.
//!
//! Exercises the host functions added for guest apps that need hashing,
//! compression, or read-only platform facts:
//!
//! - [`hash_sha256`] / [`hash_sha512`] / [`hmac_sha256`]
//! - [`uuid_v4`] / [`random_bytes`]
//! - [`compress`] / [`decompress`]
//! - [`system_theme`] / [`system_locale`] / [`system_timezone`] / [`battery_level`]
//!
//! # Build
//!
//! ```bash
//! cargo build --target wasm32-unknown-unknown --release -p platform-demo
//! ```
//!
//! Open the resulting `.wasm` in Oxide (toolbar **Open** or a `file://` address).

use oxide_sdk::*;

const BG: (u8, u8, u8) = (24, 26, 34);
const ACCENT: (u8, u8, u8) = (110, 90, 220);
const DIM: (u8, u8, u8) = (140, 140, 165);
const BRIGHT: (u8, u8, u8) = (235, 235, 245);
const GREEN: (u8, u8, u8) = (90, 220, 130);
const ORANGE: (u8, u8, u8) = (240, 180, 60);
const RED: (u8, u8, u8) = (225, 90, 90);

const BTN_NEW_UUID: u32 = 100;
const BTN_NEW_RANDOM: u32 = 101;

const SAMPLE: &[u8] =
    b"Oxide is a binary-first browser: it fetches and runs .wasm modules in a secure sandbox. \
      Repetition compresses well well well well well well well well well well well well.";

struct Demo {
    sha256: String,
    sha512: String,
    hmac: String,
    uuid: String,
    random_hex: String,
    gzip_len: usize,
    zlib_len: usize,
    roundtrip_ok: bool,
    locale: String,
    timezone: String,
    tz_offset_min: i32,
}

static mut DEMO: Option<Demo> = None;

fn refresh_uuid(demo: &mut Demo) {
    demo.uuid = uuid_v4();
}

fn refresh_random(demo: &mut Demo) {
    let mut hex = String::new();
    for b in random_bytes(16) {
        hex.push_str(&format!("{b:02x}"));
    }
    demo.random_hex = hex;
}

#[no_mangle]
pub extern "C" fn start_app() {
    log("Platform Demo loaded!");

    let gzip = compress(CompressionFormat::Gzip, SAMPLE);
    let zlib = compress(CompressionFormat::Zlib, SAMPLE);
    let roundtrip_ok = decompress(CompressionFormat::Gzip, &gzip).as_deref() == Some(SAMPLE);

    let mut demo = Demo {
        sha256: hash_sha256_hex(SAMPLE),
        sha512: hash_sha512_hex(SAMPLE),
        hmac: hmac_sha256_hex(b"secret-key", SAMPLE),
        uuid: String::new(),
        random_hex: String::new(),
        gzip_len: gzip.len(),
        zlib_len: zlib.len(),
        roundtrip_ok,
        locale: system_locale(),
        timezone: system_timezone(),
        tz_offset_min: system_timezone_offset_minutes(),
    };
    refresh_uuid(&mut demo);
    refresh_random(&mut demo);
    unsafe { DEMO = Some(demo) };
}

fn row(y: f32, label: &str, value: &str, color: (u8, u8, u8)) {
    canvas_text(20.0, y, 12.0, DIM.0, DIM.1, DIM.2, 255, label);
    canvas_text(150.0, y, 12.0, color.0, color.1, color.2, 255, value);
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let (width, _) = canvas_dimensions();
    let w = width as f32;

    canvas_clear(BG.0, BG.1, BG.2, 255);

    canvas_rect(0.0, 0.0, w, 52.0, ACCENT.0, ACCENT.1, ACCENT.2, 255);
    canvas_text(20.0, 14.0, 22.0, 255, 255, 255, 255, "Oxide Platform Demo");
    canvas_text(
        20.0,
        36.0,
        11.0,
        215,
        205,
        255,
        255,
        "crypto / compression / system info",
    );

    let demo = unsafe {
        match (*core::ptr::addr_of_mut!(DEMO)).as_mut() {
            Some(d) => d,
            None => return,
        }
    };

    // ── Crypto ───────────────────────────────────────────────────────
    canvas_text(20.0, 70.0, 14.0, DIM.0, DIM.1, DIM.2, 255, "CRYPTO");
    row(95.0, "sha256", &demo.sha256, BRIGHT);
    row(115.0, "sha512", &format!("{}…", &demo.sha512[..64]), BRIGHT);
    row(135.0, "hmac-sha256", &demo.hmac, BRIGHT);
    row(155.0, "uuid_v4", &demo.uuid, GREEN);
    row(175.0, "random (16B)", &demo.random_hex, GREEN);

    ui_button(BTN_NEW_UUID, 20.0, 198.0, 110.0, 28.0, "New UUID", || {
        let demo = unsafe { (*core::ptr::addr_of_mut!(DEMO)).as_mut().unwrap() };
        refresh_uuid(demo);
    });
    ui_button(
        BTN_NEW_RANDOM,
        140.0,
        198.0,
        110.0,
        28.0,
        "New Random",
        || {
            let demo = unsafe { (*core::ptr::addr_of_mut!(DEMO)).as_mut().unwrap() };
            refresh_random(demo);
        },
    );

    // ── Compression ──────────────────────────────────────────────────
    canvas_line(20.0, 245.0, w - 20.0, 245.0, 45, 45, 60, 255, 1.0);
    canvas_text(20.0, 258.0, 14.0, DIM.0, DIM.1, DIM.2, 255, "COMPRESSION");
    row(
        283.0,
        "input",
        &format!("{} bytes of text", SAMPLE.len()),
        BRIGHT,
    );
    row(
        303.0,
        "gzip",
        &format!(
            "{} bytes ({}%)",
            demo.gzip_len,
            demo.gzip_len * 100 / SAMPLE.len()
        ),
        BRIGHT,
    );
    row(
        323.0,
        "zlib",
        &format!(
            "{} bytes ({}%)",
            demo.zlib_len,
            demo.zlib_len * 100 / SAMPLE.len()
        ),
        BRIGHT,
    );
    if demo.roundtrip_ok {
        row(343.0, "roundtrip", "compress -> decompress OK", GREEN);
    } else {
        row(343.0, "roundtrip", "FAILED", RED);
    }

    // ── System info ──────────────────────────────────────────────────
    canvas_line(20.0, 375.0, w - 20.0, 375.0, 45, 45, 60, 255, 1.0);
    canvas_text(20.0, 388.0, 14.0, DIM.0, DIM.1, DIM.2, 255, "SYSTEM INFO");

    let theme = match system_theme() {
        THEME_LIGHT => "light",
        THEME_DARK => "dark",
        _ => "unknown",
    };
    row(413.0, "theme", theme, BRIGHT);
    row(
        433.0,
        "locale",
        if demo.locale.is_empty() {
            "unknown"
        } else {
            &demo.locale
        },
        BRIGHT,
    );
    let sign = if demo.tz_offset_min < 0 { "-" } else { "+" };
    let abs = demo.tz_offset_min.unsigned_abs();
    row(
        453.0,
        "timezone",
        &format!(
            "{} (UTC{}{:02}:{:02})",
            if demo.timezone.is_empty() {
                "unknown"
            } else {
                &demo.timezone
            },
            sign,
            abs / 60,
            abs % 60
        ),
        BRIGHT,
    );

    let level = battery_level();
    if level < 0 {
        row(473.0, "battery", "no battery", DIM);
    } else {
        let charging = match battery_charging() {
            1 => " (charging)",
            0 => " (on battery)",
            _ => "",
        };
        let color = if level <= 20 {
            RED
        } else if level <= 50 {
            ORANGE
        } else {
            GREEN
        };
        row(473.0, "battery", &format!("{level}%{charging}"), color);
    }
}
