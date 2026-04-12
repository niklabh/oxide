//! Higher-level drawing API inspired by GPUI's rendering model.
//!
//! This module provides ergonomic types and an immediate-mode [`Canvas`] facade
//! that wraps the low-level `canvas_*` functions. Guest apps can choose between
//! the raw functions (maximum control) and these helpers (less boilerplate).
//!
//! # Example
//!
//! ```rust,ignore
//! use oxide_sdk::draw::*;
//!
//! let c = Canvas::new();
//! c.clear(Color::rgb(30, 30, 46));
//! c.fill_rect(Rect::new(10.0, 10.0, 200.0, 100.0), Color::rgb(80, 120, 200));
//! c.fill_circle(Point2D::new(300.0, 200.0), 50.0, Color::rgba(200, 100, 150, 200));
//! c.text("Hello!", Point2D::new(20.0, 30.0), 24.0, Color::WHITE);
//! c.line(Point2D::new(0.0, 0.0), Point2D::new(400.0, 300.0), 2.0, Color::YELLOW);
//! ```

/// sRGB color with alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// Opaque color from RGB channels.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Color with explicit alpha.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parse a 24-bit hex value (e.g. `0xFF8040`) into an opaque color.
    pub const fn hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xFF) as u8,
            g: ((hex >> 8) & 0xFF) as u8,
            b: (hex & 0xFF) as u8,
            a: 255,
        }
    }

    /// Return this color with a different alpha value.
    #[must_use]
    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    pub const WHITE: Self = Self::rgb(255, 255, 255);
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const RED: Self = Self::rgb(255, 0, 0);
    pub const GREEN: Self = Self::rgb(0, 255, 0);
    pub const BLUE: Self = Self::rgb(0, 0, 255);
    pub const YELLOW: Self = Self::rgb(255, 255, 0);
    pub const CYAN: Self = Self::rgb(0, 255, 255);
    pub const MAGENTA: Self = Self::rgb(255, 0, 255);
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);
}

impl Default for Color {
    fn default() -> Self {
        Self::WHITE
    }
}

/// A 2D point in canvas coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point2D {
    pub x: f32,
    pub y: f32,
}

impl Point2D {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub const ZERO: Self = Self::new(0.0, 0.0);
}

/// An axis-aligned rectangle in canvas coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Create from origin point and size.
    pub const fn from_point_size(origin: Point2D, w: f32, h: f32) -> Self {
        Self {
            x: origin.x,
            y: origin.y,
            w,
            h,
        }
    }

    /// True if the point `(px, py)` is inside this rectangle (half-open: inclusive on
    /// the near edge, exclusive on the far edge).
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }

    pub fn origin(&self) -> Point2D {
        Point2D::new(self.x, self.y)
    }

    pub fn center(&self) -> Point2D {
        Point2D::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// A gradient color stop at a position along the gradient axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    pub offset: f32,
    pub color: Color,
}

impl GradientStop {
    pub const fn new(offset: f32, color: Color) -> Self {
        Self { offset, color }
    }
}

/// Immediate-mode canvas facade that wraps the low-level drawing functions.
///
/// All methods paint immediately (no retained scene graph). Create one per
/// frame or per `start_app` call and issue draw commands through it.
pub struct Canvas;

impl Canvas {
    /// Create a new canvas handle. This is zero-cost — no allocation occurs.
    pub fn new() -> Self {
        Self
    }

    /// Clear the canvas with a solid color.
    pub fn clear(&self, color: Color) {
        crate::canvas_clear(color.r, color.g, color.b, color.a);
    }

    /// Draw a filled rectangle.
    pub fn fill_rect(&self, rect: Rect, color: Color) {
        crate::canvas_rect(
            rect.x, rect.y, rect.w, rect.h, color.r, color.g, color.b, color.a,
        );
    }

    /// Draw a filled rounded rectangle with uniform corner radius.
    pub fn fill_rounded_rect(&self, rect: Rect, radius: f32, color: Color) {
        crate::canvas_rounded_rect(
            rect.x, rect.y, rect.w, rect.h, radius, color.r, color.g, color.b, color.a,
        );
    }

    /// Draw a filled circle.
    pub fn fill_circle(&self, center: Point2D, radius: f32, color: Color) {
        crate::canvas_circle(
            center.x, center.y, radius, color.r, color.g, color.b, color.a,
        );
    }

    /// Draw a circular arc stroke from `start_angle` to `end_angle` (radians).
    pub fn arc(
        &self,
        center: Point2D,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        thickness: f32,
        color: Color,
    ) {
        crate::canvas_arc(
            center.x,
            center.y,
            radius,
            start_angle,
            end_angle,
            color.r,
            color.g,
            color.b,
            color.a,
            thickness,
        );
    }

    /// Draw a cubic Bézier curve stroke.
    pub fn bezier(
        &self,
        from: Point2D,
        ctrl1: Point2D,
        ctrl2: Point2D,
        to: Point2D,
        thickness: f32,
        color: Color,
    ) {
        crate::canvas_bezier(
            from.x, from.y, ctrl1.x, ctrl1.y, ctrl2.x, ctrl2.y, to.x, to.y, color.r, color.g,
            color.b, color.a, thickness,
        );
    }

    /// Draw text at a position.
    pub fn text(&self, text: &str, pos: Point2D, size: f32, color: Color) {
        crate::canvas_text(pos.x, pos.y, size, color.r, color.g, color.b, color.a, text);
    }

    /// Draw a line between two points.
    pub fn line(&self, from: Point2D, to: Point2D, thickness: f32, color: Color) {
        crate::canvas_line(
            from.x, from.y, to.x, to.y, color.r, color.g, color.b, color.a, thickness,
        );
    }

    /// Draw an image from encoded bytes (PNG, JPEG, GIF, WebP).
    pub fn image(&self, rect: Rect, data: &[u8]) {
        crate::canvas_image(rect.x, rect.y, rect.w, rect.h, data);
    }

    /// Fill a rectangle with a linear gradient.
    pub fn linear_gradient(&self, rect: Rect, stops: &[GradientStop]) {
        let raw: Vec<(f32, u8, u8, u8, u8)> = stops
            .iter()
            .map(|s| (s.offset, s.color.r, s.color.g, s.color.b, s.color.a))
            .collect();
        crate::canvas_gradient(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            crate::GRADIENT_LINEAR,
            rect.x,
            rect.y,
            rect.x + rect.w,
            rect.y + rect.h,
            &raw,
        );
    }

    /// Fill a rectangle with a radial gradient.
    pub fn radial_gradient(&self, rect: Rect, stops: &[GradientStop]) {
        let raw: Vec<(f32, u8, u8, u8, u8)> = stops
            .iter()
            .map(|s| (s.offset, s.color.r, s.color.g, s.color.b, s.color.a))
            .collect();
        let cx = rect.x + rect.w / 2.0;
        let cy = rect.y + rect.h / 2.0;
        let r = rect.w.max(rect.h) / 2.0;
        crate::canvas_gradient(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            crate::GRADIENT_RADIAL,
            cx,
            cy,
            0.0,
            r,
            &raw,
        );
    }

    /// Push the current transform/clip/opacity state.
    pub fn save(&self) {
        crate::canvas_save();
    }

    /// Restore the most recently saved state.
    pub fn restore(&self) {
        crate::canvas_restore();
    }

    /// Apply a 2D translation to subsequent draw commands.
    pub fn translate(&self, tx: f32, ty: f32) {
        crate::canvas_transform(1.0, 0.0, 0.0, 1.0, tx, ty);
    }

    /// Apply a 2D rotation (radians) to subsequent draw commands.
    pub fn rotate(&self, angle: f32) {
        let (s, c) = (angle.sin(), angle.cos());
        crate::canvas_transform(c, s, -s, c, 0.0, 0.0);
    }

    /// Apply a uniform scale to subsequent draw commands.
    pub fn scale(&self, sx: f32, sy: f32) {
        crate::canvas_transform(sx, 0.0, 0.0, sy, 0.0, 0.0);
    }

    /// Apply a full 2D affine transform.
    pub fn transform(&self, a: f32, b: f32, c: f32, d: f32, tx: f32, ty: f32) {
        crate::canvas_transform(a, b, c, d, tx, ty);
    }

    /// Intersect the current clip with a rectangle.
    pub fn clip(&self, rect: Rect) {
        crate::canvas_clip(rect.x, rect.y, rect.w, rect.h);
    }

    /// Set layer opacity (0.0–1.0) for subsequent draw commands.
    pub fn set_opacity(&self, alpha: f32) {
        crate::canvas_opacity(alpha);
    }

    /// Get the canvas dimensions in pixels.
    pub fn dimensions(&self) -> (u32, u32) {
        crate::canvas_dimensions()
    }

    /// Get the canvas width in pixels.
    pub fn width(&self) -> u32 {
        self.dimensions().0
    }

    /// Get the canvas height in pixels.
    pub fn height(&self) -> u32 {
        self.dimensions().1
    }
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}
