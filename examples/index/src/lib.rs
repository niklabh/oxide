use oxide_sdk::*;

const BG: (u8, u8, u8) = (18, 18, 30);
const HEADER_BG: (u8, u8, u8) = (28, 28, 48);
const ACCENT: (u8, u8, u8) = (120, 90, 255);
const ACCENT_GLOW: (u8, u8, u8) = (160, 130, 255);
const TEXT_BRIGHT: (u8, u8, u8) = (240, 235, 255);
const TEXT_DIM: (u8, u8, u8) = (130, 125, 150);
const CARD_BG: (u8, u8, u8) = (32, 32, 52);
const CARD_HOVER: (u8, u8, u8) = (42, 38, 68);
const GREEN: (u8, u8, u8) = (80, 220, 140);
const BLUE: (u8, u8, u8) = (80, 160, 240);
const ORANGE: (u8, u8, u8) = (240, 170, 60);
const PURPLE: (u8, u8, u8) = (160, 90, 220);
const PINK: (u8, u8, u8) = (240, 140, 200);
const DIVIDER: (u8, u8, u8) = (50, 45, 70);

struct Card {
    title: &'static str,
    subtitle: &'static str,
    description: &'static str,
    url: &'static str,
    color: (u8, u8, u8),
    icon_char: &'static str,
}

const CARDS: &[Card] = &[
    Card {
        title: "Interactive Widgets",
        subtitle: "hello_oxide.wasm",
        description: "Buttons, checkboxes, sliders, text inputs, and mouse tracking.",
        url: "https://oxide.foundation/hello_oxide.wasm",
        color: GREEN,
        icon_char: "W",
    },
    Card {
        title: "Audio Player",
        subtitle: "audio_player.wasm",
        description: "Tone pads, custom frequencies, URL streaming, SFX channels.",
        url: "https://oxide.foundation/audio_player.wasm",
        color: BLUE,
        icon_char: "A",
    },
    Card {
        title: "Timer Demo",
        subtitle: "timer_demo.wasm",
        description: "Countdown, delayed messages, blink intervals, and stopwatch.",
        url: "https://oxide.foundation/timer_demo.wasm",
        color: ORANGE,
        icon_char: "T",
    },
    Card {
        title: "Media Capture",
        subtitle: "media_capture.wasm",
        description: "Camera preview, microphone level, and screen screenshot.",
        url: "https://oxide.foundation/media_capture.wasm",
        color: PURPLE,
        icon_char: "M",
    },
    Card {
        title: "Event System",
        subtitle: "events_demo.wasm",
        description: "Resize, focus, online/offline, touch, gamepad, drag-drop, and custom events.",
        url: "https://oxide.foundation/events_demo.wasm",
        color: PINK,
        icon_char: "E",
    },
];

#[no_mangle]
pub extern "C" fn start_app() {
    log("Oxide Landing Page loaded!");
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let (width, _height) = canvas_dimensions();
    let w = width as f32;
    let t = time_now_ms() as f32 / 1000.0;

    canvas_clear(BG.0, BG.1, BG.2, 255);
    clear_hyperlinks();

    // ── Animated background particles ────────────────────────────────
    for i in 0..12 {
        let fi = i as f32;
        let px = ((t * 0.15 + fi * 1.7).sin() * 0.5 + 0.5) * w;
        let py = ((t * 0.1 + fi * 2.3).cos() * 0.5 + 0.5) * 500.0;
        let r = 2.0 + (t * 0.3 + fi).sin().abs() * 4.0;
        canvas_circle(px, py, r, ACCENT.0, ACCENT.1, ACCENT.2, 25);
    }

    // ── Header region ────────────────────────────────────────────────
    canvas_rect(
        0.0,
        0.0,
        w,
        120.0,
        HEADER_BG.0,
        HEADER_BG.1,
        HEADER_BG.2,
        255,
    );

    let glow_alpha = ((t * 2.0).sin() * 0.3 + 0.7) * 255.0;
    canvas_rect(
        0.0,
        118.0,
        w,
        2.0,
        ACCENT.0,
        ACCENT.1,
        ACCENT.2,
        glow_alpha as u8,
    );

    // Title with subtle glow effect
    canvas_text(
        32.0,
        28.0,
        32.0,
        ACCENT_GLOW.0,
        ACCENT_GLOW.1,
        ACCENT_GLOW.2,
        255,
        "Oxide",
    );
    canvas_text(
        32.0,
        66.0,
        14.0,
        TEXT_DIM.0,
        TEXT_DIM.1,
        TEXT_DIM.2,
        255,
        "A WebAssembly-native application platform",
    );
    canvas_text(
        32.0,
        88.0,
        12.0,
        TEXT_DIM.0,
        TEXT_DIM.1,
        TEXT_DIM.2,
        255,
        "oxide.foundation",
    );

    // Version badge
    let badge_x = w - 120.0;
    canvas_rect(badge_x, 48.0, 88.0, 24.0, ACCENT.0, ACCENT.1, ACCENT.2, 60);
    canvas_text(
        badge_x + 12.0,
        52.0,
        12.0,
        TEXT_BRIGHT.0,
        TEXT_BRIGHT.1,
        TEXT_BRIGHT.2,
        255,
        "v0.1.0",
    );

    // ── Section title ────────────────────────────────────────────────
    canvas_text(
        32.0,
        142.0,
        18.0,
        TEXT_BRIGHT.0,
        TEXT_BRIGHT.1,
        TEXT_BRIGHT.2,
        255,
        "Demo Applications",
    );
    canvas_line(
        32.0,
        168.0,
        w - 32.0,
        168.0,
        DIVIDER.0,
        DIVIDER.1,
        DIVIDER.2,
        255,
        1.0,
    );

    // ── App cards ────────────────────────────────────────────────────
    let card_start_y = 185.0;
    let card_h = 90.0;
    let card_gap = 12.0;
    let card_margin = 32.0;
    let card_w = w - card_margin * 2.0;

    let (mx, my) = mouse_position();

    for (i, card) in CARDS.iter().enumerate() {
        let cy = card_start_y + (i as f32) * (card_h + card_gap);
        let hovered =
            mx >= card_margin && mx <= card_margin + card_w && my >= cy && my <= cy + card_h;

        let bg = if hovered { CARD_HOVER } else { CARD_BG };
        canvas_rect(card_margin, cy, card_w, card_h, bg.0, bg.1, bg.2, 255);

        // Left color accent bar
        canvas_rect(
            card_margin,
            cy,
            4.0,
            card_h,
            card.color.0,
            card.color.1,
            card.color.2,
            255,
        );

        // Icon circle
        let icon_cx = card_margin + 36.0;
        let icon_cy = cy + card_h / 2.0;
        canvas_circle(
            icon_cx,
            icon_cy,
            18.0,
            card.color.0,
            card.color.1,
            card.color.2,
            40,
        );
        canvas_text(
            icon_cx - 7.0,
            icon_cy - 10.0,
            18.0,
            card.color.0,
            card.color.1,
            card.color.2,
            255,
            card.icon_char,
        );

        // Title
        canvas_text(
            card_margin + 68.0,
            cy + 14.0,
            16.0,
            TEXT_BRIGHT.0,
            TEXT_BRIGHT.1,
            TEXT_BRIGHT.2,
            255,
            card.title,
        );

        // Subtitle (filename)
        canvas_text(
            card_margin + 68.0,
            cy + 36.0,
            11.0,
            card.color.0,
            card.color.1,
            card.color.2,
            255,
            card.subtitle,
        );

        // Description
        canvas_text(
            card_margin + 68.0,
            cy + 56.0,
            12.0,
            TEXT_DIM.0,
            TEXT_DIM.1,
            TEXT_DIM.2,
            255,
            card.description,
        );

        // Arrow indicator on hover
        if hovered {
            canvas_text(
                card_margin + card_w - 36.0,
                cy + card_h / 2.0 - 10.0,
                18.0,
                card.color.0,
                card.color.1,
                card.color.2,
                255,
                ">",
            );
        }

        register_hyperlink(card_margin, cy, card_w, card_h, card.url);
    }

    // ── Footer ───────────────────────────────────────────────────────
    let footer_y = card_start_y + (CARDS.len() as f32) * (card_h + card_gap) + 20.0;
    canvas_line(
        32.0,
        footer_y,
        w - 32.0,
        footer_y,
        DIVIDER.0,
        DIVIDER.1,
        DIVIDER.2,
        255,
        1.0,
    );

    canvas_text(
        32.0,
        footer_y + 16.0,
        11.0,
        TEXT_DIM.0,
        TEXT_DIM.1,
        TEXT_DIM.2,
        255,
        "Built with Oxide SDK  |  Rust + WebAssembly  |  oxide.foundation",
    );
    canvas_text(
        32.0,
        footer_y + 36.0,
        11.0,
        TEXT_DIM.0,
        TEXT_DIM.1,
        TEXT_DIM.2,
        255,
        "Click any card to launch the demo in this browser.",
    );
}
