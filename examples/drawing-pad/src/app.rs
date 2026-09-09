//! # Application state, entry points, and dock UI
//!
//! The global application singleton and the WASM-exported entry points that
//! the Oxide host calls. The frame loop in `on_frame` orchestrates input,
//! drawing, and UI each frame, and renders the bottom dock toolbar.

use alloc::vec::Vec;

use oxide_sdk::*;

use crate::geometry::{
    BUTTON_GAP, BUTTON_HEIGHT, BUTTON_WIDTH, DOCK_BACKGROUND, DOCK_HEIGHT, PALETTE, SWATCH_COLUMNS,
    SWATCH_HIT_PADDING, SWATCH_SIZE, SWATCH_SPACING,
};
use crate::render::render_preview;
use crate::session::Session;
use crate::shapes::{DrawTool, Shape};

/// The top-level application state. Holds everything needed to render a frame
/// and respond to user input.
struct App {
    /// Index into `PALETTE` for the currently selected drawing color.
    selected_color_index: usize,
    /// Current brush/stroke thickness in pixels. Adjustable via the dock slider.
    brush_size: f32,
    /// The currently active drawing tool (Line, Rect, Circle, or Freehand).
    active_tool: DrawTool,
    /// The current drawing session state machine. `Idle` when no stroke is active,
    /// `Drawing` while the user is actively drawing.
    session: Session,
    /// Tracks the previous frame's mouse-button state for edge detection.
    /// `just_pressed` is true only on the frame when the button transitions
    /// from up to down (not while it's held).
    was_mouse_down: bool,
    /// All shapes that have been committed (finished drawing strokes). These are
    /// re-rendered every frame in order, forming the drawing's history.
    committed_shapes: Vec<Shape>,
}

/// Global application singleton stored as a `static mut`. This is safe because:
///
/// - The WASM guest is single-threaded (no concurrent access).
/// - `on_frame` is the only entry point that touches `APP`, and it never holds
///   two borrows simultaneously (the scoped borrow block ensures this).
/// - `start_app` only calls `log()`, which doesn't touch `APP`.
static mut APP: Option<App> = None;

/// Get mutable access to the global app state. Initializes `APP` on first call
/// with default values (green color, 6px brush, Line tool, empty canvas).
///
/// # Safety
///
/// The WASM guest is single-threaded and `on_frame` is the only entry point
/// that touches `APP`. Callers must not hold two borrows simultaneously —
/// the scoped borrow block in [`on_frame`] (the Input phase) ensures this by
/// dropping the borrow before any button callbacks can re-borrow.
fn app() -> &'static mut App {
    unsafe {
        APP.get_or_insert(App {
            selected_color_index: 3, // Default: Green (index 3 in PALETTE)
            brush_size: 6.0,
            active_tool: DrawTool::Line,
            session: Session::Idle,
            was_mouse_down: false,
            committed_shapes: Vec::new(),
        })
    }
}

/// WASM-exported entry point called once when the module is loaded by the host.
/// Logs a startup message to the browser console.
#[no_mangle]
pub extern "C" fn start_app() {
    log("Drawing pad started");
}

/// WASM-exported frame loop, called by the Oxide host every frame (~60 Hz).
/// `delta_ms` is the time in milliseconds since the last frame (currently unused
/// but available for animation timing).
///
/// Each frame executes three phases:
///
/// ## 1. Input
///
/// A scoped borrow of `app()` handles all input processing. The scope is critical:
/// it drops the mutable borrow before any `ui_button_variant` callbacks execute,
/// preventing double-mutable-borrow panics (buttons re-borrow `app()` in their
/// closures).
///
/// Input handling:
/// - **Mouse press on swatch** → Update selected color.
/// - **Mouse press on canvas** → Start a new `Session::Drawing`.
/// - **Mouse held** → Push mouse position into the active session.
/// - **Mouse release** → Finish the session, commit the resulting Shape.
///
/// ## 2. Draw
///
/// Clear the canvas to the dark background color, render all committed shapes,
/// and render the live preview of the in-progress stroke (if any).
///
/// ## 3. Dock
///
/// Render the bottom toolbar:
/// - Dark background rect.
/// - Clear button (✕) — clears all committed shapes.
/// - Tool mode buttons — stacked vertically on the left; active tool is highlighted.
/// - Brush size slider — horizontal bar between tools and palette.
/// - Color swatches — 4×3 grid centered in the dock; selected color has a glow.
#[no_mangle]
pub extern "C" fn on_frame(_delta_ms: u32) {
    let (canvas_width, canvas_height) = canvas_dimensions();
    let (canvas_width, canvas_height) = (canvas_width as f32, canvas_height as f32);
    let dock_top = canvas_height - DOCK_HEIGHT;
    let (mouse_x, mouse_y) = mouse_position();
    let mouse_is_down = mouse_button_down(0);

    // ── Input ──────────────────────────────────────────────────────────
    // Scoped borrow: release `app` before button callbacks run so they
    // can re-borrow freely without a double-mutable-borrow conflict.
    //
    // This scope is necessary because `ui_button_variant` closures capture
    // `app()` mutably, and the outer `app()` borrow above would conflict.
    // By confining the borrow to this block, it's dropped before any
    // callback executes.
    {
        let app = app();
        // Detect the leading edge of a mouse press (down this frame, was up last frame).
        // This prevents repeated actions from held buttons (e.g., starting a new stroke
        // every frame while the mouse is held).
        let just_pressed = mouse_is_down && !app.was_mouse_down;
        let (swatch_origin_x, swatch_origin_y) = compute_swatch_origin(canvas_width, dock_top);
        if just_pressed {
            // Check if the click landed on a color swatch.
            if let Some(color_index) =
                hit_test_swatches(mouse_x, mouse_y, swatch_origin_x, swatch_origin_y)
            {
                app.selected_color_index = color_index;
            }
        }
        // Only start a new stroke if:
        //   - This is a leading-edge press (just pressed, not held)
        //   - The click is on the canvas area (above the dock, with a 4px margin)
        //   - No stroke is currently active (session is Idle)
        let in_canvas_area = mouse_y > 0.0 && mouse_y < dock_top - 4.0;
        if just_pressed && in_canvas_area && matches!(app.session, Session::Idle) {
            app.session = Session::begin(
                app.active_tool,
                PALETTE[app.selected_color_index],
                app.brush_size,
                (mouse_x, mouse_y),
            );
        }
        // While the mouse is held, feed the current position into the active session.
        if mouse_is_down {
            app.session.push((mouse_x, mouse_y));
        }
        // On mouse release: finish the session and commit the resulting shape.
        if !mouse_is_down && app.was_mouse_down {
            // `mem::replace` moves the session out so we can call `finish()` (which
            // consumes self) while simultaneously setting the session back to Idle.
            let previous_session = core::mem::replace(&mut app.session, Session::Idle);
            if let Some(shape) = previous_session.finish() {
                app.committed_shapes.push(shape);
            }
        }
        // Record mouse state for edge detection in the next frame.
        app.was_mouse_down = mouse_is_down;
    }

    // ── Draw ───────────────────────────────────────────────────────────
    // Clear the entire canvas to a dark blue-gray background.
    canvas_clear(30, 30, 46, 255);
    // Re-render every committed shape from oldest to newest (painter's algorithm).
    for shape in &app().committed_shapes {
        shape.render();
    }
    // Render the live preview of the in-progress stroke (rubber-banding for
    // geometric tools, accumulating polyline for freehand).
    if let Session::Drawing {
        tool,
        color,
        thickness,
        start,
        ref points,
        ..
    } = app().session
    {
        let end = *points.last().unwrap_or(&start);
        render_preview(tool, start, end, points, color, thickness);
    }

    // ── Dock ───────────────────────────────────────────────────────────
    // Draw the dock background as a filled rectangle spanning the full width.
    canvas_rect(
        0.0,
        dock_top,
        canvas_width,
        DOCK_HEIGHT,
        DOCK_BACKGROUND.0,
        DOCK_BACKGROUND.1,
        DOCK_BACKGROUND.2,
        255,
    );

    let (grid_x, grid_y) = compute_swatch_origin(canvas_width, dock_top);
    // Position the clear button to the right of the swatch grid.
    let clear_x = grid_x + SWATCH_COLUMNS as f32 * SWATCH_SPACING + 20.0;
    let clear_y = dock_top + (DOCK_HEIGHT - 32.0) / 2.0;

    // Clear button — Ghost variant (transparent until hover), ✕ symbol.
    ui_button_variant(
        2,
        clear_x,
        clear_y,
        32.0,
        32.0,
        "✕",
        UiVariant::Ghost,
        || {
            app().committed_shapes.clear();
        },
    );

    // Tool mode buttons — stacked vertically on the left side of the dock.
    // Vertically centered within the dock height based on the number of tools.
    let mode_button_top_y = dock_top
        + (DOCK_HEIGHT - DrawTool::ALL.len() as f32 * (BUTTON_HEIGHT + BUTTON_GAP) - BUTTON_GAP)
            / 2.0;
    for (tool_index, &tool) in DrawTool::ALL.iter().enumerate() {
        // The active tool gets the Default (filled) variant; others are Ghost.
        let variant = if app().active_tool == tool {
            UiVariant::Default
        } else {
            UiVariant::Ghost
        };
        let button_y = mode_button_top_y + tool_index as f32 * (BUTTON_HEIGHT + BUTTON_GAP);
        ui_button_variant(
            10 + tool_index as u32,
            16.0,
            button_y,
            BUTTON_WIDTH,
            BUTTON_HEIGHT,
            tool.label(),
            variant,
            move || {
                app().active_tool = tool;
            },
        );
    }

    // Brush size slider — horizontal bar positioned between the tool buttons
    // and the color swatch grid. Returns the current slider value, which is
    // immediately applied to `app().brush_size`.
    let brush_x = grid_x - 20.0 - 108.0;
    let brush_y = dock_top + (DOCK_HEIGHT - 22.0) / 2.0;
    let brush = ui_slider(5, brush_x, brush_y, 88.0, 1.0, 40.0, 6.0);
    app().brush_size = brush;
    // Display the current brush size as text below the slider.
    canvas_text(
        brush_x,
        brush_y + 26.0,
        12.0,
        220,
        220,
        230,
        255,
        &format!("{:.0}px", brush),
    );

    // Color swatches — render the 4×3 grid of palette colors. The selected
    // swatch gets an additional semi-transparent glow (rounded rect behind it)
    // at 60% opacity to indicate selection.
    let (sel_red, sel_green, sel_blue) = PALETTE[app().selected_color_index];
    for (palette_idx, &(swatch_red, swatch_green, swatch_blue)) in PALETTE.iter().enumerate() {
        let (swatch_x, swatch_y) = swatch_cell(palette_idx, grid_x, grid_y);
        // Selection indicator: a slightly larger, semi-transparent rounded rect
        // behind the selected swatch, creating a subtle glow effect.
        if palette_idx == app().selected_color_index {
            canvas_rounded_rect(
                swatch_x - 2.0,
                swatch_y - 2.0,
                SWATCH_SIZE + 4.0,
                SWATCH_SIZE + 4.0,
                6.0,
                sel_red,
                sel_green,
                sel_blue,
                60,
            );
        }
        // The swatch itself — a fully opaque rounded rectangle.
        canvas_rounded_rect(
            swatch_x,
            swatch_y,
            SWATCH_SIZE,
            SWATCH_SIZE,
            5.0,
            swatch_red,
            swatch_green,
            swatch_blue,
            255,
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Layout helpers
//
//  Utility functions for computing positions within the dock's swatch grid.
//  The palette is arranged as a 4-column × 3-row grid, centered horizontally
//  in the dock and positioned near the bottom with a 12px margin.
// ═══════════════════════════════════════════════════════════════════════════════

/// Compute the (x, y) position of a swatch cell given its palette index and the
/// grid's origin point.
///
/// The grid is laid out in row-major order:
///   - Column = index % SWATCH_COLUMNS (wraps every 4)
///   - Row = index / SWATCH_COLUMNS (integer division)
///
/// Position = origin + (column × spacing, row × spacing)
fn swatch_cell(index: usize, grid_origin_x: f32, grid_origin_y: f32) -> (f32, f32) {
    (
        grid_origin_x + (index as u32 % SWATCH_COLUMNS) as f32 * SWATCH_SPACING,
        grid_origin_y + (index as u32 / SWATCH_COLUMNS) as f32 * SWATCH_SPACING,
    )
}

/// Compute the top-left origin of the swatch grid, centering it horizontally
/// in the canvas and positioning it near the bottom of the dock.
///
/// Horizontal: `(canvas_width - grid_width) / 2.0` centers the grid.
/// Vertical: `dock_top + DOCK_HEIGHT - grid_height - 12.0` places the grid
///   near the bottom of the dock with a 12px bottom margin.
fn compute_swatch_origin(canvas_width: f32, dock_top: f32) -> (f32, f32) {
    let grid_width = SWATCH_COLUMNS as f32 * SWATCH_SPACING;
    let grid_height = (PALETTE.len() as u32 / SWATCH_COLUMNS) as f32 * SWATCH_SPACING;
    (
        (canvas_width - grid_width) / 2.0,
        dock_top + DOCK_HEIGHT - grid_height - 12.0,
    )
}

/// Hit-test the mouse position against all swatches in the palette grid.
/// Returns the index of the first swatch whose bounding box (expanded by
/// `SWATCH_HIT_PADDING` on all sides) contains the mouse position.
///
/// The padding makes swatches easier to click by extending the clickable area
/// beyond the visible swatch square. Uses `find_map` to short-circuit on the
/// first match (swatches don't overlap, so at most one can match).
fn hit_test_swatches(
    mouse_x: f32,
    mouse_y: f32,
    grid_origin_x: f32,
    grid_origin_y: f32,
) -> Option<usize> {
    PALETTE.iter().enumerate().find_map(|(palette_idx, _)| {
        let (swatch_x, swatch_y) = swatch_cell(palette_idx, grid_origin_x, grid_origin_y);
        // AABB (axis-aligned bounding box) test with padding.
        if mouse_x >= swatch_x - SWATCH_HIT_PADDING
            && mouse_x <= swatch_x + SWATCH_SIZE + SWATCH_HIT_PADDING
            && mouse_y >= swatch_y - SWATCH_HIT_PADDING
            && mouse_y <= swatch_y + SWATCH_SIZE + SWATCH_HIT_PADDING
        {
            Some(palette_idx)
        } else {
            None
        }
    })
}
