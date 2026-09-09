//! A simple drawing pad: freehand strokes, a color palette, brush size, and clear.

#![allow(static_mut_refs)]

use oxide_sdk::*;

type Point = (f32, f32);
type Color = (u8, u8, u8);

const PALETTE: [Color; 6] = [
    (239, 68, 68),
    (59, 130, 246),
    (52, 211, 153),
    (251, 191, 36),
    (244, 114, 182),
    (241, 245, 249),
];

const SWATCH_SIZE: f32 = 28.0;
const SWATCH_SPACING: f32 = 36.0;
const DOCK_HEIGHT: f32 = 64.0;
const DOCK_BACKGROUND: Color = (24, 24, 34);
const MIN_BRUSH: f32 = 1.0;
const MAX_BRUSH: f32 = 24.0;
const DEFAULT_BRUSH: f32 = 5.0;
const SLIDER_WIDTH: f32 = 120.0;

struct Segment {
    start: Point,
    end: Point,
    color: Color,
    thickness: f32,
}

struct App {
    selected_color: usize,
    brush_size: f32,
    last_point: Option<Point>,
    segments: Vec<Segment>,
}

static mut APP: Option<App> = None;

fn app() -> &'static mut App {
    unsafe {
        APP.get_or_insert(App {
            selected_color: 2,
            brush_size: DEFAULT_BRUSH,
            last_point: None,
            segments: Vec::new(),
        })
    }
}

#[no_mangle]
pub extern "C" fn start_app() {
    log("Drawing pad started");
}

#[no_mangle]
pub extern "C" fn on_frame(_delta_ms: u32) {
    let (canvas_width, canvas_height) = canvas_dimensions();
    let (canvas_width, canvas_height) = (canvas_width as f32, canvas_height as f32);
    let dock_top = canvas_height - DOCK_HEIGHT;
    let (mouse_x, mouse_y) = mouse_position();
    let mouse_is_down = mouse_button_down(0);
    let grid_x = (canvas_width - PALETTE.len() as f32 * SWATCH_SPACING) / 2.0;
    let grid_y = dock_top + (DOCK_HEIGHT - SWATCH_SIZE) / 2.0;

    // ── Input ──────────────────────────────────────────────────────────
    let app = app();
    let point = (mouse_x, mouse_y);
    let in_canvas = mouse_is_down && mouse_y < dock_top - 4.0;
    if in_canvas {
        app.segments.push(Segment {
            start: app.last_point.unwrap_or(point),
            end: point,
            color: PALETTE[app.selected_color],
            thickness: app.brush_size,
        });
    }
    app.last_point = in_canvas.then_some(point);

    // ── Draw ───────────────────────────────────────────────────────────
    canvas_clear(30, 30, 46, 255);
    for segment in &app.segments {
        let (red, green, blue) = segment.color;
        let ((start_x, start_y), (end_x, end_y)) = (segment.start, segment.end);
        let thickness = segment.thickness;
        canvas_line(
            start_x, start_y, end_x, end_y, red, green, blue, 255, thickness,
        );
        canvas_circle(end_x, end_y, thickness * 0.5, red, green, blue, 255);
    }

    // ── Dock ───────────────────────────────────────────────────────────
    let (dock_r, dock_g, dock_b) = DOCK_BACKGROUND;
    canvas_rect(
        0.0,
        dock_top,
        canvas_width,
        DOCK_HEIGHT,
        dock_r,
        dock_g,
        dock_b,
        255,
    );

    for (idx, &(red, green, blue)) in PALETTE.iter().enumerate() {
        let swatch_x = grid_x + idx as f32 * SWATCH_SPACING;
        if mouse_is_down
            && mouse_x >= swatch_x - 4.0
            && mouse_x <= swatch_x + SWATCH_SIZE + 4.0
            && mouse_y >= grid_y - 4.0
            && mouse_y <= grid_y + SWATCH_SIZE + 4.0
        {
            app.selected_color = idx;
        }
        let size = SWATCH_SIZE + f32::from(idx == app.selected_color) * 6.0;
        let offset = (SWATCH_SIZE - size) / 2.0;
        let (rect_x, rect_y) = (swatch_x + offset, grid_y + offset);
        canvas_rounded_rect(rect_x, rect_y, size, size, 5.0, red, green, blue, 255);
    }

    let (clear_x, clear_y) = (
        grid_x + PALETTE.len() as f32 * SWATCH_SPACING + 20.0,
        dock_top + (DOCK_HEIGHT - 32.0) / 2.0,
    );
    ui_button_variant(
        1,
        clear_x,
        clear_y,
        32.0,
        32.0,
        "✕",
        UiVariant::Ghost,
        || app.segments.clear(),
    );

    let (slider_x, slider_y) = (clear_x + 56.0, dock_top + (DOCK_HEIGHT - 20.0) / 2.0);
    app.brush_size = ui_slider(
        2,
        slider_x,
        slider_y,
        SLIDER_WIDTH,
        MIN_BRUSH,
        MAX_BRUSH,
        DEFAULT_BRUSH,
    );
}
