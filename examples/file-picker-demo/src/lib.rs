//! Native file and folder picker demo for the Oxide browser.
//!
//! Exercises every host function exposed by `oxide-sdk`'s file picker API:
//!
//! - [`file_pick`] — multi-select with an extension filter.
//! - [`folder_pick`] + [`folder_entries`] — browse a folder's children.
//! - [`file_metadata`] — display name, size, MIME, and last-modified.
//! - [`file_read`] — preview images with `canvas_image`.
//! - [`file_read_range`] — read the first 2 KiB of text files for a preview.
//!
//! # Build
//!
//! ```bash
//! cargo build --target wasm32-unknown-unknown --release -p file-picker-demo
//! ```
//!
//! Open the resulting `.wasm` in Oxide and click "Pick Files…" or
//! "Pick Folder…" to invoke the native OS dialog.

use oxide_sdk::*;

// ── Colors ────────────────────────────────────────────────────────────────────

const BG: (u8, u8, u8) = (24, 24, 36);
const HEADER_BG: (u8, u8, u8) = (40, 32, 68);
const PANEL_BG: (u8, u8, u8) = (32, 32, 50);
const ROW_BG: (u8, u8, u8) = (42, 42, 62);
const ROW_HOVER: (u8, u8, u8) = (58, 50, 88);
const ROW_SEL: (u8, u8, u8) = (80, 70, 140);
const DIVIDER: (u8, u8, u8) = (60, 55, 90);
const TEXT_BRIGHT: (u8, u8, u8) = (235, 230, 255);
const TEXT_DIM: (u8, u8, u8) = (150, 145, 170);
const TEXT_MUTED: (u8, u8, u8) = (110, 105, 130);
const ACCENT: (u8, u8, u8) = (140, 110, 255);
const OK: (u8, u8, u8) = (120, 220, 150);
const ERR: (u8, u8, u8) = (240, 120, 130);

// ── Layout ────────────────────────────────────────────────────────────────────

const HEADER_H: f32 = 56.0;
const TOOLBAR_H: f32 = 56.0;
const ROW_H: f32 = 30.0;
const LIST_W: f32 = 380.0;

// ── App state ─────────────────────────────────────────────────────────────────

struct Row {
    name: String,
    size: u64,
    is_dir: bool,
    handle: u32,
}

#[derive(Default)]
struct Preview {
    handle: u32,
    name: String,
    size: u64,
    mime: String,
    modified_ms: u64,
    is_dir: bool,
    /// Encoded image bytes ready for `canvas_image` (empty if not an image).
    image_bytes: Vec<u8>,
    /// First 2 KiB decoded as text (empty if not a text-ish file).
    text_preview: String,
    /// `true` once we've attempted to load the body (success or failure).
    body_loaded: bool,
    /// Human-readable status for the preview pane.
    status: String,
}

struct AppState {
    rows: Vec<Row>,
    /// `Some((handle, name))` when we're currently browsing a folder.
    current_folder: Option<(u32, String)>,
    selected_handle: u32,
    preview: Option<Preview>,
    last_error: String,
}

impl AppState {
    const fn new() -> Self {
        Self {
            rows: Vec::new(),
            current_folder: None,
            selected_handle: 0,
            preview: None,
            last_error: String::new(),
        }
    }
}

static mut STATE: AppState = AppState::new();

fn state() -> &'static mut AppState {
    // Single-threaded guest — standard pattern used across Oxide examples.
    unsafe { &mut *core::ptr::addr_of_mut!(STATE) }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn format_modified(ms: u64) -> String {
    if ms == 0 {
        return String::from("—");
    }
    let secs = ms / 1000;
    let days = secs / 86_400;
    // Proleptic Gregorian date from UNIX epoch (1970-01-01).
    let (year, month, day) = civil_from_days(days as i64);
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{year:04}-{month:02}-{day:02} {h:02}:{m:02}:{s:02} UTC")
}

/// Howard Hinnant's civil-from-days algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}

fn is_image_mime(mime: &str) -> bool {
    mime.starts_with("image/") && mime != "image/svg+xml"
}

fn is_text_mime(mime: &str) -> bool {
    mime.starts_with("text/")
        || matches!(
            mime,
            "application/json" | "application/xml" | "application/javascript" | "image/svg+xml"
        )
}

// ── UI: toolbar, rows, preview ────────────────────────────────────────────────

fn draw_header(w: f32) {
    canvas_rect(
        0.0,
        0.0,
        w,
        HEADER_H,
        HEADER_BG.0,
        HEADER_BG.1,
        HEADER_BG.2,
        255,
    );
    canvas_text(
        20.0,
        14.0,
        22.0,
        TEXT_BRIGHT.0,
        TEXT_BRIGHT.1,
        TEXT_BRIGHT.2,
        255,
        "Oxide File Picker Demo",
    );
    canvas_text(
        20.0,
        38.0,
        12.0,
        TEXT_DIM.0,
        TEXT_DIM.1,
        TEXT_DIM.2,
        255,
        "file_pick · folder_pick · file_metadata · file_read · file_read_range",
    );
}

fn draw_toolbar(y: f32, w: f32) {
    canvas_rect(
        0.0, y, w, TOOLBAR_H, PANEL_BG.0, PANEL_BG.1, PANEL_BG.2, 255,
    );
    canvas_rect(
        0.0,
        y + TOOLBAR_H - 1.0,
        w,
        1.0,
        DIVIDER.0,
        DIVIDER.1,
        DIVIDER.2,
        255,
    );

    let s = state();

    if ui_button(1, 20.0, y + 14.0, 150.0, 30.0, "Pick Image…") {
        let handles = file_pick("Pick an image", "png,jpg,jpeg,gif,webp,bmp", false);
        if handles.is_empty() {
            s.last_error = String::from("Picker cancelled");
        } else {
            s.last_error.clear();
            load_picked_files(&handles);
        }
    }
    if ui_button(2, 180.0, y + 14.0, 150.0, 30.0, "Pick Files…") {
        let handles = file_pick("Pick one or more files", "", true);
        if handles.is_empty() {
            s.last_error = String::from("Picker cancelled");
        } else {
            s.last_error.clear();
            load_picked_files(&handles);
        }
    }
    if ui_button(3, 340.0, y + 14.0, 150.0, 30.0, "Pick Folder…") {
        match folder_pick("Pick a folder") {
            Some(h) => {
                s.last_error.clear();
                open_folder(h, format!("handle #{h}"));
            }
            None => s.last_error = String::from("Folder picker cancelled"),
        }
    }
    if ui_button(4, 500.0, y + 14.0, 100.0, 30.0, "Clear") {
        s.rows.clear();
        s.current_folder = None;
        s.selected_handle = 0;
        s.preview = None;
        s.last_error.clear();
    }

    if !s.last_error.is_empty() {
        canvas_text(
            620.0,
            y + 22.0,
            13.0,
            ERR.0,
            ERR.1,
            ERR.2,
            255,
            &s.last_error,
        );
    }
}

fn draw_breadcrumb(y: f32, w: f32) -> f32 {
    let s = state();
    let h = 24.0;
    canvas_rect(0.0, y, w, h, BG.0, BG.1, BG.2, 255);
    let label = match &s.current_folder {
        Some((_, name)) => format!("📁 {}   ({} entries)", name, s.rows.len()),
        None => format!("📄 Picked files   ({})", s.rows.len()),
    };
    canvas_text(
        20.0,
        y + 6.0,
        13.0,
        ACCENT.0,
        ACCENT.1,
        ACCENT.2,
        255,
        &label,
    );
    y + h
}

fn draw_row_list(y_top: f32, h_avail: f32) {
    let s = state();
    canvas_rect(
        0.0, y_top, LIST_W, h_avail, PANEL_BG.0, PANEL_BG.1, PANEL_BG.2, 255,
    );
    canvas_rect(
        LIST_W, y_top, 1.0, h_avail, DIVIDER.0, DIVIDER.1, DIVIDER.2, 255,
    );

    if s.rows.is_empty() {
        canvas_text(
            20.0,
            y_top + 20.0,
            13.0,
            TEXT_MUTED.0,
            TEXT_MUTED.1,
            TEXT_MUTED.2,
            255,
            "No selection. Use the buttons above.",
        );
        return;
    }

    let (mx, my) = mouse_position();
    let click = mouse_button_clicked(0);
    let max_rows = (h_avail / ROW_H) as usize;

    // Collect click target first to avoid iterator-vs-borrow conflicts.
    let mut clicked_idx: Option<usize> = None;

    for (i, row) in s.rows.iter().enumerate().take(max_rows) {
        let ry = y_top + (i as f32) * ROW_H;
        let hovered = (0.0..=LIST_W).contains(&mx) && my >= ry && my < ry + ROW_H;
        let selected = row.handle == s.selected_handle;
        let bg = if selected {
            ROW_SEL
        } else if hovered {
            ROW_HOVER
        } else {
            ROW_BG
        };
        canvas_rect(
            4.0,
            ry + 2.0,
            LIST_W - 8.0,
            ROW_H - 4.0,
            bg.0,
            bg.1,
            bg.2,
            255,
        );

        let icon = if row.is_dir { "📁" } else { "📄" };
        canvas_text(
            14.0,
            ry + 7.0,
            14.0,
            TEXT_BRIGHT.0,
            TEXT_BRIGHT.1,
            TEXT_BRIGHT.2,
            255,
            icon,
        );
        let name = trim_for_width(&row.name, 26);
        canvas_text(
            40.0,
            ry + 7.0,
            14.0,
            TEXT_BRIGHT.0,
            TEXT_BRIGHT.1,
            TEXT_BRIGHT.2,
            255,
            &name,
        );
        let size_str = if row.is_dir {
            String::from("dir")
        } else {
            format_size(row.size)
        };
        canvas_text(
            LIST_W - 90.0,
            ry + 7.0,
            12.0,
            TEXT_DIM.0,
            TEXT_DIM.1,
            TEXT_DIM.2,
            255,
            &size_str,
        );

        if hovered && click {
            clicked_idx = Some(i);
        }
    }

    if let Some(i) = clicked_idx {
        let handle = s.rows[i].handle;
        let name = s.rows[i].name.clone();
        let is_dir = s.rows[i].is_dir;
        if is_dir {
            open_folder(handle, name);
        } else {
            select_file(handle);
        }
    }
}

fn trim_for_width(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars - 1).collect();
        out.push('…');
        out
    }
}

fn draw_preview(x: f32, y_top: f32, w: f32, h: f32) {
    canvas_rect(x, y_top, w, h, BG.0, BG.1, BG.2, 255);

    let s = state();
    let Some(p) = &s.preview else {
        canvas_text(
            x + 20.0,
            y_top + 20.0,
            13.0,
            TEXT_MUTED.0,
            TEXT_MUTED.1,
            TEXT_MUTED.2,
            255,
            "Select a file on the left to preview its contents.",
        );
        return;
    };

    // Metadata block.
    let mut row_y = y_top + 16.0;
    draw_kv(x + 20.0, row_y, "name", &p.name);
    row_y += 22.0;
    draw_kv(x + 20.0, row_y, "size", &format_size(p.size));
    row_y += 22.0;
    draw_kv(x + 20.0, row_y, "mime", &p.mime);
    row_y += 22.0;
    draw_kv(x + 20.0, row_y, "modified", &format_modified(p.modified_ms));
    row_y += 22.0;
    draw_kv(
        x + 20.0,
        row_y,
        "handle",
        &format!(
            "{} ({})",
            p.handle,
            if p.is_dir { "folder" } else { "file" }
        ),
    );
    row_y += 26.0;

    // Status line (success or error from the read attempt).
    if !p.status.is_empty() {
        let color = if p.image_bytes.is_empty() && p.text_preview.is_empty() {
            ERR
        } else {
            OK
        };
        canvas_text(
            x + 20.0,
            row_y,
            12.0,
            color.0,
            color.1,
            color.2,
            255,
            &p.status,
        );
        row_y += 20.0;
    }

    // Preview body.
    let body_y = row_y + 8.0;
    let body_h = (y_top + h) - body_y - 12.0;
    let body_w = w - 40.0;
    if body_h < 40.0 {
        return;
    }
    canvas_rect(
        x + 20.0,
        body_y,
        body_w,
        body_h,
        PANEL_BG.0,
        PANEL_BG.1,
        PANEL_BG.2,
        255,
    );

    if !p.image_bytes.is_empty() {
        // Fit image into the body box while keeping centered.
        let pad = 8.0;
        canvas_image(
            x + 20.0 + pad,
            body_y + pad,
            body_w - 2.0 * pad,
            body_h - 2.0 * pad,
            &p.image_bytes,
        );
    } else if !p.text_preview.is_empty() {
        draw_text_block(
            x + 30.0,
            body_y + 10.0,
            body_w - 20.0,
            body_h - 20.0,
            &p.text_preview,
        );
    } else if p.is_dir {
        canvas_text(
            x + 30.0,
            body_y + 14.0,
            13.0,
            TEXT_DIM.0,
            TEXT_DIM.1,
            TEXT_DIM.2,
            255,
            "Directory — use the list on the left to browse its children.",
        );
    } else {
        canvas_text(
            x + 30.0,
            body_y + 14.0,
            13.0,
            TEXT_DIM.0,
            TEXT_DIM.1,
            TEXT_DIM.2,
            255,
            "No inline preview available for this MIME type.",
        );
    }
}

fn draw_kv(x: f32, y: f32, key: &str, value: &str) {
    canvas_text(x, y, 12.0, TEXT_DIM.0, TEXT_DIM.1, TEXT_DIM.2, 255, key);
    canvas_text(
        x + 80.0,
        y,
        13.0,
        TEXT_BRIGHT.0,
        TEXT_BRIGHT.1,
        TEXT_BRIGHT.2,
        255,
        value,
    );
}

fn draw_text_block(x: f32, y: f32, w: f32, h: f32, text: &str) {
    let font_size = 12.0;
    let line_h = 16.0;
    let max_cols = (w / (font_size * 0.58)) as usize;
    let max_lines = (h / line_h) as usize;
    let mut drawn_lines = 0usize;

    for raw_line in text.lines() {
        if drawn_lines >= max_lines {
            break;
        }
        // Wrap long lines.
        if raw_line.is_empty() {
            drawn_lines += 1;
            continue;
        }
        let mut remaining = raw_line;
        while !remaining.is_empty() && drawn_lines < max_lines {
            let take = remaining
                .char_indices()
                .nth(max_cols)
                .map(|(i, _)| i)
                .unwrap_or(remaining.len());
            let (head, tail) = remaining.split_at(take);
            canvas_text(
                x,
                y + drawn_lines as f32 * line_h,
                font_size,
                TEXT_BRIGHT.0,
                TEXT_BRIGHT.1,
                TEXT_BRIGHT.2,
                255,
                head,
            );
            drawn_lines += 1;
            remaining = tail;
        }
    }

    if drawn_lines >= max_lines {
        canvas_text(
            x,
            y + (max_lines.saturating_sub(1)) as f32 * line_h + line_h,
            11.0,
            TEXT_MUTED.0,
            TEXT_MUTED.1,
            TEXT_MUTED.2,
            255,
            "…preview truncated",
        );
    }
}

// ── Actions ───────────────────────────────────────────────────────────────────

fn load_picked_files(handles: &[u32]) {
    let s = state();
    s.rows.clear();
    s.current_folder = None;
    for &h in handles {
        if let Some(meta) = file_metadata(h) {
            s.rows.push(Row {
                name: meta.name,
                size: meta.size,
                is_dir: meta.is_dir,
                handle: h,
            });
        } else {
            s.rows.push(Row {
                name: format!("handle #{h}"),
                size: 0,
                is_dir: false,
                handle: h,
            });
        }
    }
    if let Some(first) = handles.first() {
        select_file(*first);
    }
}

fn open_folder(handle: u32, label: String) {
    let s = state();
    let entries = folder_entries(handle);
    s.rows = entries
        .into_iter()
        .map(|e| Row {
            name: e.name,
            size: e.size,
            is_dir: e.is_dir,
            handle: e.handle,
        })
        .collect();
    // Folders first, then files, both alphabetically.
    s.rows.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => core::cmp::Ordering::Less,
        (false, true) => core::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    s.current_folder = Some((handle, label));
    s.selected_handle = 0;
    s.preview = file_metadata(handle).map(|m| Preview {
        handle,
        name: m.name,
        size: m.size,
        mime: m.mime,
        modified_ms: m.modified_ms,
        is_dir: m.is_dir,
        image_bytes: Vec::new(),
        text_preview: String::new(),
        body_loaded: true,
        status: String::new(),
    });
}

fn select_file(handle: u32) {
    let s = state();
    s.selected_handle = handle;
    let Some(meta) = file_metadata(handle) else {
        s.preview = None;
        s.last_error = String::from("file_metadata failed");
        return;
    };

    let mut preview = Preview {
        handle,
        name: meta.name,
        size: meta.size,
        mime: meta.mime.clone(),
        modified_ms: meta.modified_ms,
        is_dir: meta.is_dir,
        ..Default::default()
    };

    if meta.is_dir {
        preview.status = String::from("Directory metadata loaded.");
        preview.body_loaded = true;
        s.preview = Some(preview);
        return;
    }

    if is_image_mime(&meta.mime) {
        // Full read for images so canvas_image can decode.
        match file_read(handle) {
            Some(bytes) => {
                preview.status = format!("file_read: {} bytes", bytes.len());
                preview.image_bytes = bytes;
            }
            None => {
                preview.status = String::from("file_read failed (image too large or I/O error)")
            }
        }
    } else if is_text_mime(&meta.mime) {
        // Range read for text so huge files don't blow the preview buffer.
        let want: u32 = 2048;
        match file_read_range(handle, 0, want) {
            Some(bytes) => {
                let n = bytes.len();
                let text = String::from_utf8_lossy(&bytes).into_owned();
                preview.text_preview = text;
                preview.status = format!("file_read_range(0, {want}): {n} bytes");
            }
            None => preview.status = String::from("file_read_range failed"),
        }
    } else {
        preview.status = format!("{} — no inline preview", meta.mime);
    }

    preview.body_loaded = true;
    s.preview = Some(preview);
}

// ── Lifecycle ─────────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn start_app() {
    log("file-picker-demo: click a button to invoke the native picker.");
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let (cw, ch) = canvas_dimensions();
    let w = cw as f32;
    let h = ch as f32;

    canvas_clear(BG.0, BG.1, BG.2, 255);

    draw_header(w);
    draw_toolbar(HEADER_H, w);
    let list_top = draw_breadcrumb(HEADER_H + TOOLBAR_H, w);

    let list_h = h - list_top;
    draw_row_list(list_top, list_h);
    draw_preview(LIST_W + 1.0, list_top, w - LIST_W - 1.0, list_h);
}
