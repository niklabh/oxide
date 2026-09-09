//! # Session state machine
//!
//! Tracks the lifecycle of a single drawing stroke from mouse-down to mouse-up.
//!
//! States:
//!   Idle → (mouse press on canvas) → Drawing → (mouse release) → Idle
//!
//! During the Drawing state, incoming mouse positions are filtered by adaptive
//! sampling and accumulated into a point list. On finish, the points are passed
//! through the smoothing pipeline and committed as a Shape.

use alloc::vec;
use alloc::vec::Vec;

use crate::geometry::{dist, Color, Point};
use crate::shapes::{DrawTool, Geom, Shape};
use crate::smoothing::{smooth_pipeline, RecognizedShape};

/// The drawing session state machine.
///
/// - **Idle** — No active stroke. Waiting for a mouse press on the canvas.
/// - **Drawing** — An active stroke in progress. Contains:
///   - `tool` — Which drawing tool is active (Line, Rect, Circle, Freehand).
///   - `color` — The selected palette color at stroke start.
///   - `thickness` — The brush size at stroke start.
///   - `start` — The mouse position where the stroke began (anchor point).
///   - `points` — Accumulated sample points (for Freehand; geometric tools only use `start`).
///   - `average_step_distance` — Exponentially-weighted moving average of inter-point
///     distances, used for adaptive sampling (see `push`).
pub(crate) enum Session {
    Idle,
    Drawing {
        tool: DrawTool,
        color: Color,
        thickness: f32,
        start: Point,
        points: Vec<Point>,
        /// Adaptive sampling threshold: the exponential moving average of the distance
        /// between consecutive accepted points. Updated each frame with α=0.3.
        /// New points are only accepted if they exceed 80% of this average (clamped
        /// to [2.0, 10.0] pixels), reducing noise from slow/stationary mouse input
        /// while preserving detail in fast strokes.
        average_step_distance: f32,
    },
}

impl Session {
    /// Begin a new drawing session at the given position.
    pub(crate) fn begin(tool: DrawTool, color: Color, thickness: f32, start: Point) -> Self {
        Self::Drawing {
            tool,
            color,
            thickness,
            start,
            points: vec![start],
            average_step_distance: 4.0,
        }
    }

    /// Accept a new mouse position into the active stroke. Uses adaptive sampling
    /// to filter out redundant points:
    ///
    /// 1. Compute the distance from the new point to the last accepted point.
    /// 2. Update the running average with exponential moving average (α=0.3):
    ///    `new_avg = 0.7 × old_avg + 0.3 × distance`
    ///    This gives recent distances more weight while maintaining stability.
    /// 3. Accept the point only if distance ≥ 80% of the average (clamped to 2–10px).
    ///    This threshold adapts to drawing speed:
    ///    - Fast strokes: average is large → accepts points far apart (captures speed)
    ///    - Slow strokes: average is small → filters jitter (rejects noise)
    ///
    /// The clamp range [2.0, 10.0] prevents the threshold from becoming too tight
    /// (which would lose detail) or too loose (which would accept jitter).
    pub(crate) fn push(&mut self, point: Point) {
        if let Self::Drawing {
            points,
            average_step_distance,
            ..
        } = self
        {
            let distance = points
                .last()
                .map_or(f32::INFINITY, |&last| dist(point, last));
            *average_step_distance = *average_step_distance * 0.7 + distance * 0.3;
            if distance >= (*average_step_distance * 0.8).clamp(2.0, 10.0) {
                points.push(point);
            }
        }
    }

    /// Finish the current stroke and produce a committed Shape.
    ///
    /// For each tool:
    /// - **Freehand (≥3 points)**: Runs the smoothing pipeline; may snap to circle.
    /// - **Freehand (<3 points)**: Commits as raw freehand.
    /// - **Line/Rect/Circle**: Commits the corresponding geometric shape.
    ///
    /// Returns `Some(Shape)` on success, or `None` if the session was already Idle.
    pub(crate) fn finish(self) -> Option<Shape> {
        match self {
            Self::Drawing {
                tool,
                color,
                thickness,
                start,
                points,
                ..
            } => {
                let end = *points.last().unwrap_or(&start);
                let geom = match tool {
                    DrawTool::Freehand if points.len() >= 3 => {
                        let (smoothed, recognized) = smooth_pipeline(&points);
                        match recognized {
                            RecognizedShape::Circle { center, radius } => {
                                Geom::Circle { center, radius }
                            }
                            RecognizedShape::Freehand => Geom::Freehand { points: smoothed },
                        }
                    }
                    DrawTool::Freehand => Geom::Freehand { points },
                    DrawTool::Line => Geom::Line { start, end },
                    DrawTool::Rect => Geom::Rect { start, end },
                    DrawTool::Circle => Geom::Circle {
                        center: start,
                        radius: dist(start, end),
                    },
                };
                Some(Shape {
                    color,
                    thickness,
                    geom,
                })
            }
            Self::Idle => None,
        }
    }
}
