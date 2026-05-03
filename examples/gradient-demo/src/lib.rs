//! Gradient showcase — a static, fully responsive grid of hand-picked
//! gradient palettes. Every rectangle fits strictly inside the canvas
//! bounds regardless of width or height.

use oxide_sdk::draw::*;
use oxide_sdk::*;

const MARGIN: f32 = 20.0;
const GAP: f32 = 12.0;
const HERO_H: f32 = 72.0;
const FOOTER_H: f32 = 32.0;
const MIN_CARD_W: f32 = 90.0;
const MIN_CARD_H: f32 = 60.0;

const PALETTES: &[(&str, [u32; 3])] = &[
    ("Sunset", [0xff7e5f, 0xfeb47b, 0xffcf7d]),
    ("Aurora", [0x43cea2, 0x185a9d, 0x6a11cb]),
    ("Ocean", [0x2bc0e4, 0x5564eb, 0x1a2980]),
    ("Peach", [0xff9a9e, 0xfad0c4, 0xffdde1]),
    ("Grape", [0x8e2de2, 0x4a00e0, 0xc471f5]),
    ("Emerald", [0x00b09b, 0x96c93d, 0xf5fd60]),
];

#[no_mangle]
pub extern "C" fn start_app() {
    log("Gradient demo loaded!");
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let c = Canvas::new();
    let (cw, ch) = c.dimensions();
    let w = cw as f32;
    let h = ch as f32;
    if w < 2.0 || h < 2.0 {
        return;
    }

    draw_background(w, h);
    draw_hero(&c, w);

    let body_top = HERO_H + GAP;
    let body_bottom = (h - FOOTER_H).max(body_top);
    let body_h = body_bottom - body_top;
    if body_h >= MIN_CARD_H + 20.0 {
        draw_palette_grid(&c, w, body_top, body_h);
    }

    if h >= HERO_H + FOOTER_H {
        draw_footer(&c, w, h);
    }
}

// ── Full-canvas vertical linear gradient backdrop.
fn draw_background(w: f32, h: f32) {
    vertical_gradient(
        Rect::new(0.0, 0.0, w, h),
        &[
            GradientStop::new(0.0, Color::hex(0x1b1033)),
            GradientStop::new(0.5, Color::hex(0x3b2670)),
            GradientStop::new(1.0, Color::hex(0x0a1a3d)),
        ],
    );
}

// ── Hero bar: horizontal gradient + accent underline + title text.
fn draw_hero(c: &Canvas, w: f32) {
    horizontal_gradient(
        Rect::new(0.0, 0.0, w, HERO_H),
        &[
            GradientStop::new(0.0, Color::rgba(20, 10, 40, 230)),
            GradientStop::new(1.0, Color::rgba(60, 20, 80, 150)),
        ],
    );
    horizontal_gradient(
        Rect::new(0.0, HERO_H - 3.0, w, 3.0),
        &[
            GradientStop::new(0.0, Color::hex(0xff6a88)),
            GradientStop::new(0.5, Color::hex(0xffb86c)),
            GradientStop::new(1.0, Color::hex(0x5e60ce)),
        ],
    );

    c.text(
        "Gradient Showcase",
        Point2D::new(MARGIN, 16.0),
        24.0,
        Color::WHITE,
    );
    c.text(
        "Linear & radial fills rendered by the Oxide canvas",
        Point2D::new(MARGIN, 46.0),
        12.0,
        Color::rgba(220, 210, 255, 220),
    );
}

// ── Grid of palette cards. The column count is chosen so every card is
//    at least MIN_CARD_W wide, so the grid can never overflow horizontally.
fn draw_palette_grid(c: &Canvas, w: f32, top: f32, body_h: f32) {
    let n = PALETTES.len();
    let avail_w = (w - MARGIN * 2.0).max(0.0);
    if avail_w < MIN_CARD_W {
        return;
    }

    // Cols that still give cards ≥ MIN_CARD_W (with GAPs between them).
    let max_cols = ((avail_w + GAP) / (MIN_CARD_W + GAP)).floor().max(1.0) as usize;
    let cols = max_cols.min(n).max(1);
    let rows = n.div_ceil(cols);

    let total_gap_x = GAP * (cols as f32 - 1.0);
    let card_w = ((avail_w - total_gap_x) / cols as f32).max(0.0);

    let label_h = 16.0;
    let label_y = top;
    let grid_top = top + label_h + 6.0;
    let grid_h = (body_h - label_h - 6.0).max(0.0);

    let total_gap_y = GAP * (rows as f32 - 1.0);
    let card_h = ((grid_h - total_gap_y) / rows as f32).max(0.0);
    if card_w < MIN_CARD_W || card_h < MIN_CARD_H {
        return;
    }

    c.text(
        "PALETTES",
        Point2D::new(MARGIN, label_y),
        11.0,
        Color::rgba(230, 220, 255, 200),
    );

    for (i, (name, hexes)) in PALETTES.iter().enumerate() {
        let col = (i % cols) as f32;
        let row = (i / cols) as f32;
        let x = MARGIN + col * (card_w + GAP);
        let y = grid_top + row * (card_h + GAP);
        let rect = Rect::new(x, y, card_w, card_h);

        diagonal_gradient(
            rect,
            &[
                GradientStop::new(0.0, Color::hex(hexes[0])),
                GradientStop::new(0.5, Color::hex(hexes[1])),
                GradientStop::new(1.0, Color::hex(hexes[2])),
            ],
        );

        // Top sheen.
        let sheen_w = (card_w - 12.0).max(0.0);
        if sheen_w > 0.0 {
            c.fill_rounded_rect(
                Rect::new(rect.x + 6.0, rect.y + 6.0, sheen_w, 2.0),
                1.0,
                Color::rgba(255, 255, 255, 90),
            );
        }

        c.text(
            name,
            Point2D::new(rect.x + 10.0, rect.y + rect.h - 22.0),
            13.0,
            Color::WHITE,
        );
    }
}

fn draw_footer(c: &Canvas, w: f32, h: f32) {
    horizontal_gradient(
        Rect::new(0.0, h - FOOTER_H, w, FOOTER_H),
        &[
            GradientStop::new(0.0, Color::rgba(10, 5, 25, 200)),
            GradientStop::new(1.0, Color::rgba(40, 15, 60, 140)),
        ],
    );
    c.text(
        "oxide_sdk::draw — canvas_gradient with linear color stops",
        Point2D::new(MARGIN, h - FOOTER_H + 11.0),
        11.0,
        Color::rgba(220, 210, 240, 220),
    );
}

// ───────── Helpers ─────────────────────────────────────────────────────────

fn vertical_gradient(rect: Rect, stops: &[GradientStop]) {
    let raw = stops_to_raw(stops);
    canvas_gradient(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        GRADIENT_LINEAR,
        rect.x,
        rect.y,
        rect.x,
        rect.y + rect.h,
        &raw,
    );
}

fn horizontal_gradient(rect: Rect, stops: &[GradientStop]) {
    let raw = stops_to_raw(stops);
    canvas_gradient(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        GRADIENT_LINEAR,
        rect.x,
        rect.y,
        rect.x + rect.w,
        rect.y,
        &raw,
    );
}

fn diagonal_gradient(rect: Rect, stops: &[GradientStop]) {
    let raw = stops_to_raw(stops);
    canvas_gradient(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        GRADIENT_LINEAR,
        rect.x,
        rect.y,
        rect.x + rect.w,
        rect.y + rect.h,
        &raw,
    );
}

fn stops_to_raw(stops: &[GradientStop]) -> Vec<(f32, u8, u8, u8, u8)> {
    stops
        .iter()
        .map(|s| (s.offset, s.color.r, s.color.g, s.color.b, s.color.a))
        .collect()
}
