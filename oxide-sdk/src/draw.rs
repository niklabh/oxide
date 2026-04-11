//! Higher-level drawing API inspired by GPUI's rendering model.
//!
//! This module provides ergonomic types and a builder-style [`Canvas`] facade
//! that wraps the low-level `canvas_*` functions. Guest apps can choose between
//! the raw functions (maximum control) and these helpers (less boilerplate).
//!
//! # Example
//!
//! ```rust,ignore
//! use oxide_sdk::draw::*;
//!
//! let mut c = Canvas::new();
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

    /// True if the point `(px, py)` is inside this rectangle.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && py >= self.y && px <= self.x + self.w && py <= self.y + self.h
    }

    pub fn origin(&self) -> Point2D {
        Point2D::new(self.x, self.y)
    }

    pub fn center(&self) -> Point2D {
        Point2D::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// Builder-style canvas facade that wraps the low-level drawing functions.
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

    /// Draw a filled circle.
    pub fn fill_circle(&self, center: Point2D, radius: f32, color: Color) {
        crate::canvas_circle(
            center.x, center.y, radius, color.r, color.g, color.b, color.a,
        );
    }

    /// Draw text at a position.
    pub fn text(&self, text: &str, pos: Point2D, size: f32, color: Color) {
        crate::canvas_text(pos.x, pos.y, size, color.r, color.g, color.b, text);
    }

    /// Draw a line between two points.
    pub fn line(&self, from: Point2D, to: Point2D, thickness: f32, color: Color) {
        crate::canvas_line(
            from.x, from.y, to.x, to.y, color.r, color.g, color.b, thickness,
        );
    }

    /// Draw an image from encoded bytes (PNG, JPEG, GIF, WebP).
    pub fn image(&self, rect: Rect, data: &[u8]) {
        crate::canvas_image(rect.x, rect.y, rect.w, rect.h, data);
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
