//! Typography demo — shows `canvas_text_ex` and `canvas_measure_text`.
//!
//! Renders the same sample text in a range of weights and styles, then uses
//! `canvas_measure_text` to draw a tight underline under one line and to
//! right-align another.

use oxide_sdk::*;

const SAMPLE: &str = "The quick brown fox jumps over the lazy dog";

#[no_mangle]
pub extern "C" fn start_app() {
    log("typography-demo loaded");
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let (w, _h) = canvas_dimensions();
    let w = w as f32;

    canvas_clear(24, 24, 32, 255);

    canvas_text_ex(
        w / 2.0,
        20.0,
        28.0,
        230,
        230,
        255,
        255,
        "",
        700,
        FONT_STYLE_NORMAL,
        TEXT_ALIGN_CENTER,
        "Oxide Typography",
    );

    let left_col_x = 24.0;
    let label_x = 24.0;
    let sample_x = 170.0;
    let mut y = 80.0;

    for (label, weight, style) in [
        ("Thin", 100, FONT_STYLE_NORMAL),
        ("Light", 300, FONT_STYLE_NORMAL),
        ("Normal", 400, FONT_STYLE_NORMAL),
        ("Medium", 500, FONT_STYLE_NORMAL),
        ("Bold", 700, FONT_STYLE_NORMAL),
        ("Black", 900, FONT_STYLE_NORMAL),
        ("Italic", 400, FONT_STYLE_ITALIC),
        ("Bold italic", 700, FONT_STYLE_ITALIC),
    ] {
        canvas_text_ex(
            label_x,
            y,
            14.0,
            140,
            150,
            180,
            255,
            "",
            400,
            FONT_STYLE_NORMAL,
            TEXT_ALIGN_LEFT,
            label,
        );
        canvas_text_ex(
            sample_x,
            y,
            18.0,
            235,
            235,
            245,
            255,
            "",
            weight,
            style,
            TEXT_ALIGN_LEFT,
            SAMPLE,
        );
        y += 32.0;
    }

    // Measurement demo: draw a tight underline under a sample line.
    y += 16.0;
    let underline_size = 22.0;
    let underline_text = "Measured — underlined exactly to its width.";
    canvas_text_ex(
        left_col_x,
        y,
        underline_size,
        180,
        220,
        255,
        255,
        "",
        500,
        FONT_STYLE_NORMAL,
        TEXT_ALIGN_LEFT,
        underline_text,
    );
    let m = canvas_measure_text(underline_size, "", 500, FONT_STYLE_NORMAL, underline_text);
    let underline_y = y + m.ascent + 4.0;
    canvas_line(
        left_col_x,
        underline_y,
        left_col_x + m.width,
        underline_y,
        180,
        220,
        255,
        255,
        1.5,
    );
    canvas_text_ex(
        left_col_x,
        underline_y + 8.0,
        12.0,
        110,
        130,
        160,
        255,
        "",
        400,
        FONT_STYLE_NORMAL,
        TEXT_ALIGN_LEFT,
        &format!(
            "width {:.1}px  ascent {:.1}px  descent {:.1}px",
            m.width, m.ascent, m.descent,
        ),
    );

    // Alignment demo: right-aligned label.
    canvas_text_ex(
        w - 24.0,
        y,
        underline_size,
        255,
        210,
        180,
        255,
        "",
        700,
        FONT_STYLE_ITALIC,
        TEXT_ALIGN_RIGHT,
        "right-aligned",
    );
}
