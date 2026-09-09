//! # Drawing tools and committed shapes
//!
//! The four available drawing modes (`DrawTool`) and the shape types that hold
//! committed geometry (`Geom`, `Shape`). A `Shape` is a finalized drawing element
//! created by `Session::finish()` in [`crate::session`], stored in the app's
//! `committed_shapes` vector, and re-rendered every frame via [`Shape::render`].

use alloc::vec::Vec;

use oxide_sdk::*;

use crate::geometry::{Color, Point};
use crate::render::{draw_rect_outline, render_stroke_tube};

/// Available drawing tools.
///
/// - **Line** — Click-and-drag from point A to B; commits a straight line segment.
/// - **Rect** — Click-and-drag defines two opposite corners; commits a rectangle outline.
/// - **Circle** — Click sets the center; drag sets the radius; commits a circle.
/// - **Freehand** — Draw freely; the smoothing pipeline processes the stroke, and
///   circle recognition may promote it to an ideal circle.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DrawTool {
    Line,
    Rect,
    Circle,
    Freehand,
}

impl DrawTool {
    /// All tools in display order (top to bottom in the dock).
    pub(crate) const ALL: [DrawTool; 4] = [
        DrawTool::Line,
        DrawTool::Rect,
        DrawTool::Circle,
        DrawTool::Freehand,
    ];

    /// Human-readable label for the tool, used as the button text in the dock.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Line => "Line",
            Self::Rect => "Rect",
            Self::Circle => "Circle",
            Self::Freehand => "Freehand",
        }
    }
}

/// The geometric definition of a shape. Each variant stores only the minimal data
/// needed to render that geometry type.
pub(crate) enum Geom {
    /// A straight line segment from `start` to `end`.
    Line { start: Point, end: Point },
    /// A rectangle defined by two opposite corners (`start` and `end`).
    /// The actual left/right/top/bottom are computed during rendering by taking
    /// min/max of the corners, so the user can drag in any direction.
    Rect { start: Point, end: Point },
    /// A circle defined by its center point and radius in pixels.
    Circle { center: Point, radius: f32 },
    /// A freehand polyline — an ordered list of points that have been smoothed
    /// and simplified by the smoothing pipeline.
    Freehand { points: Vec<Point> },
}

/// A committed drawing element: a geometry definition paired with visual properties.
/// Shapes are created by `Session::finish()` and stored in `App::committed_shapes`.
pub(crate) struct Shape {
    /// The RGB color of this shape.
    pub(crate) color: Color,
    /// Stroke thickness in pixels. Used for line width, circle arc thickness, and
    /// the tube radius for freehand strokes.
    pub(crate) thickness: f32,
    /// The geometric definition — determines which rendering path is used.
    pub(crate) geom: Geom,
}

impl Shape {
    /// Render this shape to the canvas by dispatching to the appropriate SDK drawing
    /// call based on the geometry type.
    ///
    /// - **Line** → `canvas_line` with the two endpoints.
    /// - **Circle** → `canvas_arc` with a full 0→2π arc (full circle).
    /// - **Rect** → `draw_rect_outline` which renders a filled interior + 4 edge lines.
    /// - **Freehand** → `render_stroke_tube` which renders connected thick lines with
    ///   round joints.
    pub(crate) fn render(&self) {
        let (red, green, blue) = self.color;
        let thickness = self.thickness;
        match &self.geom {
            Geom::Line { start, end } => canvas_line(
                start.0, start.1, end.0, end.1, red, green, blue, 255, thickness,
            ),
            Geom::Circle { center, radius } => canvas_arc(
                center.0,
                center.1,
                *radius,
                0.0,
                core::f32::consts::TAU, // Full circle: 0 to 2π radians
                red,
                green,
                blue,
                255,
                thickness,
            ),
            Geom::Rect { start, end } => {
                // Normalize corners so left < right and top < bottom,
                // regardless of which direction the user dragged.
                let left = start.0.min(end.0);
                let right = start.0.max(end.0);
                let top = start.1.min(end.1);
                let bottom = start.1.max(end.1);
                draw_rect_outline(left, top, right, bottom, red, green, blue, thickness);
            }
            Geom::Freehand { points } => render_stroke_tube(points, self.color, thickness),
        }
    }
}
