#![allow(clippy::too_many_arguments)]

//! Markdown split editor: multi-line **`ui_text_area`** with a scrolling live preview.
//!
//! ```bash
//! cargo build --target wasm32-unknown-unknown --release -p markdown-editor
//! ```

use oxide_sdk::*;

const PAD: f32 = 10.0;
const HEADER_H: f32 = 42.0;

const DEFAULT_MD: &str = "\
# Markdown editor\n\nSide-by-side **source** and preview.\nFocus the left pane and type (`inline code`), or try lists:\n\n- Alpha\n- Beta\n\n---\n\n```rust\nfn main() {}\n```\n\nMore at https://oxide.foundation\n";

static mut PREVIEW_SCROLL: f32 = 0.0;

#[no_mangle]
pub extern "C" fn start_app() {
    unsafe { PREVIEW_SCROLL = 0.0 };
    log("markdown-editor loaded");
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let (cw, ch) = canvas_dimensions();
    let w = cw as f32;
    let h = ch as f32;

    canvas_clear(22, 22, 28, 255);

    canvas_rect(0.0, 0.0, w, HEADER_H, 36, 36, 48, 255);
    canvas_text_ex(
        PAD,
        26.0,
        22.0,
        235,
        235,
        250,
        255,
        "",
        650,
        FONT_STYLE_NORMAL,
        TEXT_ALIGN_LEFT,
        "Markdown editor",
    );

    let body_top = HEADER_H + 6.0;
    let body_h = (h - body_top - PAD).max(80.0);
    let gutter = 10.0;
    let inner_w = w - PAD * 2.0 - gutter;
    let col_w = (inner_w / 2.0).max(120.0);
    let editor_x = PAD;
    let preview_x = PAD + col_w + gutter;

    canvas_rect(editor_x, body_top, col_w, body_h, 28, 28, 34, 255);
    canvas_rect(preview_x, body_top, col_w, body_h, 18, 18, 24, 255);
    canvas_line(
        preview_x - gutter * 0.5,
        body_top,
        preview_x - gutter * 0.5,
        body_top + body_h,
        60,
        60,
        72,
        255,
        1.5,
    );

    canvas_text_ex(
        editor_x + 6.0,
        body_top + 4.0,
        13.0,
        140,
        140,
        170,
        255,
        "",
        500,
        FONT_STYLE_ITALIC,
        TEXT_ALIGN_LEFT,
        "Source",
    );
    canvas_text_ex(
        preview_x + 6.0,
        body_top + 4.0,
        13.0,
        140,
        140,
        170,
        255,
        "",
        500,
        FONT_STYLE_ITALIC,
        TEXT_ALIGN_LEFT,
        "Preview",
    );

    let area_top = body_top + 22.0;
    let area_h = (body_h - 26.0).max(40.0);

    let md = ui_text_area(7, editor_x + 4.0, area_top, col_w - 8.0, area_h, DEFAULT_MD);

    let preview_inner_x = preview_x + 8.0;
    let preview_w = col_w - 16.0;
    let clip_top = area_top + 6.0;
    let clip_bot = body_top + body_h - 6.0;
    let view_h = clip_bot - clip_top;

    let (mx, my) = mouse_position();
    let over_preview =
        mx >= preview_x && mx <= preview_x + col_w && my >= clip_top && my <= clip_bot;

    unsafe {
        if over_preview {
            let (_sx, dsy) = scroll_delta();
            PREVIEW_SCROLL += dsy;
            if PREVIEW_SCROLL < 0.0 {
                PREVIEW_SCROLL = 0.0;
            }
        }
        clear_hyperlinks();
        let content_h = {
            let mut probe = Probe { y: 0.0 };
            preview_walk(
                &md,
                preview_inner_x,
                preview_w,
                0.0,
                f32::MAX,
                &mut probe,
                false,
            );
            probe.y + 16.0
        };
        let max_scroll = (content_h - view_h).max(0.0);
        PREVIEW_SCROLL = PREVIEW_SCROLL.min(max_scroll);
        let mut drawer = Probe {
            y: clip_top - PREVIEW_SCROLL,
        };
        preview_walk(
            &md,
            preview_inner_x,
            preview_w,
            clip_top,
            clip_bot,
            &mut drawer,
            true,
        );
    }
}

fn simplify_paragraph(raw: &str) -> String {
    raw.replace('`', "").replace("**", "")
}

struct Probe {
    y: f32,
}

fn preview_walk(
    md: &str,
    x: f32,
    max_w: f32,
    clip_top: f32,
    clip_bot: f32,
    st: &mut Probe,
    draw: bool,
) {
    let lines: Vec<&str> = md.lines().collect();
    let mut i = 0usize;
    let text_r = 220u8;
    let text_g = 220u8;
    let text_b = 232u8;

    while i < lines.len() {
        let raw = lines[i];
        let line = raw.trim_end();

        if line.is_empty() {
            st.y += 8.0;
            i += 1;
            continue;
        }

        if line.starts_with("```") {
            i += 1;
            let mut code = String::new();
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                if !code.is_empty() {
                    code.push('\n');
                }
                code.push_str(lines[i]);
                i += 1;
            }
            if i < lines.len() {
                i += 1;
            }

            let box_top = st.y + 8.0;
            let inner_x = x + 6.0;
            let inner_w = max_w - 12.0;
            let text_top = box_top + 10.0;
            let zz = code.split('\n').fold(text_top, |yy, ln| {
                if ln.is_empty() {
                    yy + 12.6 * 1.28
                } else {
                    wrap_plain(
                        false, inner_x, yy, inner_w, ln, 12.6, 400, 200, 206, 222, clip_top,
                        clip_bot,
                    )
                }
            });
            let h_box = (zz - box_top + 14.0).max(34.0);
            if draw && overlaps(box_top - 8.0, box_top + h_box, clip_top, clip_bot) {
                canvas_rect(x - 4.0, box_top - 4.0, max_w + 8.0, h_box, 42, 44, 58, 255);
                let _ = code.split('\n').fold(text_top, |yy, ln| {
                    if ln.is_empty() {
                        yy + 12.6 * 1.28
                    } else {
                        wrap_plain(
                            true, inner_x, yy, inner_w, ln, 12.6, 400, 200, 206, 222, clip_top,
                            clip_bot,
                        )
                    }
                });
            }
            st.y += h_box + 12.0;
            continue;
        }

        if is_rule(line) {
            let hy = st.y + 6.0;
            if draw && hy >= clip_top && hy <= clip_bot {
                canvas_line(x - 4.0, hy, x + max_w + 4.0, hy, 72, 72, 94, 255, 1.0);
            }
            st.y += 16.0;
            i += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("### ") {
            st.y = heading_height(
                st.y, draw, x, rest, max_w, clip_top, clip_bot, 17.6, 600, 205, 210, 248,
            );
            i += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix("## ") {
            st.y = heading_height(
                st.y, draw, x, rest, max_w, clip_top, clip_bot, 19.4, 650, 210, 220, 248,
            );
            i += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix("# ") {
            st.y = heading_height(
                st.y, draw, x, rest, max_w, clip_top, clip_bot, 24.8, 700, 230, 228, 255,
            );
            i += 1;
            continue;
        }

        if let Some(item) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            let pill = st.y + 18.0;
            if draw && pill >= clip_top && pill <= clip_bot {
                canvas_text_ex(
                    x + 4.0,
                    pill + 13.8,
                    14.8,
                    160,
                    178,
                    210,
                    255,
                    "",
                    700,
                    FONT_STYLE_NORMAL,
                    TEXT_ALIGN_LEFT,
                    "•",
                );
            }
            st.y = wrapped_body(
                st.y,
                draw,
                x + 24.0,
                item,
                max_w - 26.0,
                clip_top,
                clip_bot,
                text_r,
                text_g,
                text_b,
            );
            st.y += 6.0;
            i += 1;
            continue;
        }

        let mut para = String::from(line);
        i += 1;
        while i < lines.len() {
            let n = lines[i].trim_end();
            if n.is_empty() {
                break;
            }
            if n.starts_with('#')
                || n.starts_with("```")
                || n.starts_with("- ")
                || n.starts_with("* ")
                || is_rule(n)
            {
                break;
            }
            para.push(' ');
            para.push_str(n.trim_start());
            i += 1;
        }
        let simplified = simplify_paragraph(&para);
        st.y = wrapped_body(
            st.y,
            draw,
            x,
            &simplified,
            max_w,
            clip_top,
            clip_bot,
            text_r,
            text_g,
            text_b,
        );
        st.y += 10.0;
    }
}

fn overlaps(y0: f32, y1: f32, top: f32, bot: f32) -> bool {
    y1 >= top && y0 <= bot
}

fn heading_height(
    mut y: f32,
    draw: bool,
    x: f32,
    text: &str,
    max_w: f32,
    clip_top: f32,
    clip_bot: f32,
    size: f32,
    weight: u32,
    r: u8,
    g: u8,
    b: u8,
) -> f32 {
    y += size + 4.0;
    y = wrap_plain(
        draw, x, y, max_w, text, size, weight, r, g, b, clip_top, clip_bot,
    );
    y + 10.0
}

fn wrapped_body(
    y: f32,
    draw: bool,
    x: f32,
    text: &str,
    max_w: f32,
    clip_top: f32,
    clip_bot: f32,
    r: u8,
    g: u8,
    b: u8,
) -> f32 {
    wrap_plain(
        draw,
        x,
        y + 17.8,
        max_w,
        text,
        14.8,
        400,
        r,
        g,
        b,
        clip_top,
        clip_bot,
    )
}

fn wrap_plain(
    draw: bool,
    x: f32,
    mut y: f32,
    max_w: f32,
    text: &str,
    size: f32,
    weight: u32,
    r: u8,
    g: u8,
    b: u8,
    clip_top: f32,
    clip_bot: f32,
) -> f32 {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return y + size * 0.6;
    }
    let mut line = String::new();
    let line_gap = size * 1.28;

    for w in words {
        let trial = if line.is_empty() {
            w.to_string()
        } else {
            format!("{line} {w}")
        };
        let m = canvas_measure_text(size, "", weight, FONT_STYLE_NORMAL, &trial);
        if m.width > max_w && !line.is_empty() {
            emit_line(
                draw,
                x,
                &line,
                size,
                weight,
                r,
                g,
                b,
                clip_top,
                clip_bot,
                y + size,
            );
            y += line_gap;
            line = w.to_string();
        } else {
            line = trial;
        }
    }
    if !line.is_empty() {
        emit_line(
            draw,
            x,
            &line,
            size,
            weight,
            r,
            g,
            b,
            clip_top,
            clip_bot,
            y + size,
        );
        y += line_gap;
    }
    y
}

fn emit_line(
    draw: bool,
    x: f32,
    line: &str,
    size: f32,
    weight: u32,
    r: u8,
    g: u8,
    b: u8,
    clip_top: f32,
    clip_bot: f32,
    baseline_y: f32,
) {
    if draw {
        let m = canvas_measure_text(size, "", weight, FONT_STYLE_NORMAL, line);
        let top_g = baseline_y - m.ascent;
        let bot_g = baseline_y + m.descent;
        if bot_g >= clip_top && top_g <= clip_bot {
            canvas_text_ex(
                x,
                baseline_y,
                size,
                r,
                g,
                b,
                255,
                "",
                weight,
                FONT_STYLE_NORMAL,
                TEXT_ALIGN_LEFT,
                line,
            );
        }
    }
}

fn is_rule(s: &str) -> bool {
    let t = s.trim();
    t.len() >= 3 && t.chars().all(|c| c == '-' || c == '*' || c == '_')
}
