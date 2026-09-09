//! # Rendering
//!
//! Low-level drawing functions that convert shapes and strokes into canvas
//! primitive calls. These handle the visual details that the geometry types
//! don't encode directly (e.g., rectangle outlines as 4 lines, freehand
//! strokes as thick polylines with round joints).

use oxide_sdk::*;

use crate::geometry::{dist, Color, Point};
use crate::shapes::DrawTool;

/// Render a rectangle outline: a semi-transparent filled interior plus four
/// solid edge lines.
///
/// The interior is drawn with α=60 (about 24% opacity) using `canvas_rect`,
/// giving a subtle fill that doesn't overpower the stroke. The four edges are
/// drawn as individual `canvas_line` calls with full opacity (α=255) and the
/// specified thickness.
///
/// The corners are formed by iterating over the 5 corner points in order:
///   (left,top) → (right,top) → (right,bottom) → (left,bottom) → (left,top)
/// and drawing lines between consecutive pairs via `windows(2)`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_rect_outline(
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    red: u8,
    green: u8,
    blue: u8,
    thickness: f32,
) {
    canvas_rect(left, top, right - left, bottom - top, red, green, blue, 60);
    for window in [
        (left, top),
        (right, top),
        (right, bottom),
        (left, bottom),
        (left, top),
    ]
    .windows(2)
    {
        canvas_line(
            window[0].0,
            window[0].1,
            window[1].0,
            window[1].1,
            red,
            green,
            blue,
            255,
            thickness,
        );
    }
}

/// Render a freehand stroke as a "tube" — a series of connected thick line segments
/// with filled circles at each joint for smooth, rounded connections.
///
/// This technique prevents visible gaps or sharp angles at the joints between
/// consecutive line segments. Without the joint circles, two thick lines meeting
/// at an angle would leave a triangular gap at the corner.
///
/// Rendering steps:
///   1. Draw thick `canvas_line` segments between each consecutive pair of points.
///   2. Draw a filled `canvas_circle` at each point with radius = thickness × 0.5.
///
/// Special cases:
///   - Empty point list: no-op.
///   - Single point: renders as just a circle (a dot).
pub(crate) fn render_stroke_tube(points: &[Point], color: Color, thickness: f32) {
    if points.is_empty() {
        return;
    }
    let (red, green, blue) = color;
    let joint_radius = thickness * 0.5;
    if points.len() == 1 {
        canvas_circle(
            points[0].0,
            points[0].1,
            joint_radius,
            red,
            green,
            blue,
            255,
        );
        return;
    }
    for segment in points.windows(2) {
        canvas_line(
            segment[0].0,
            segment[0].1,
            segment[1].0,
            segment[1].1,
            red,
            green,
            blue,
            255,
            thickness,
        );
    }
    for &point in points {
        canvas_circle(point.0, point.1, joint_radius, red, green, blue, 255);
    }
}

/// Render a live preview of an in-progress stroke. This is called every frame
/// while the user is actively drawing, providing immediate visual feedback.
///
/// Mirrors the rendering logic of [`crate::shapes::Shape::render`] but uses the
/// current mouse position (`end`) as the endpoint rather than the committed end
/// position.
///
/// For geometric tools (Line, Rect, Circle), the endpoint follows the mouse in
/// real time, so the user sees the shape "rubber-band" as they drag. For Freehand,
/// the accumulated `points` array is rendered directly using the tube renderer.
pub(crate) fn render_preview(
    tool: DrawTool,
    start: Point,
    end: Point,
    points: &[Point],
    color: Color,
    thickness: f32,
) {
    let (red, green, blue) = color;
    match tool {
        DrawTool::Line => {
            canvas_line(
                start.0, start.1, end.0, end.1, red, green, blue, 255, thickness,
            );
        }
        DrawTool::Circle => {
            let radius = dist(start, end);
            canvas_arc(
                start.0,
                start.1,
                radius,
                0.0,
                core::f32::consts::TAU,
                red,
                green,
                blue,
                255,
                thickness,
            );
        }
        DrawTool::Rect => {
            let left = start.0.min(end.0);
            let right = start.0.max(end.0);
            let top = start.1.min(end.1);
            let bottom = start.1.max(end.1);
            draw_rect_outline(left, top, right, bottom, red, green, blue, thickness);
        }
        DrawTool::Freehand => render_stroke_tube(points, color, thickness),
    }
}
