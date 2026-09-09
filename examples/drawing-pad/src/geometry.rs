//! # Geometry — core types, palette, and layout constants
//!
//! Fundamental data types and constants shared across the entire application:
//! the `Point`/`Color` type aliases, the Euclidean `dist()` helper, the 12-color
//! `PALETTE`, and every layout constant that sizes and positions the dock UI.

/// A 2D point in canvas coordinates (x, y).
pub(crate) type Point = (f32, f32);

/// An RGB color triplet with 8-bit channels.
pub(crate) type Color = (u8, u8, u8);

/// Compute the Euclidean distance between two points using the Pythagorean theorem:
///   dist = √((x₂ - x₁)² + (y₂ - y₁)²)
///
/// Used throughout for hit testing (swatch clicks, circle closing distance), shape
/// recognition (radius consistency checks), and geometry construction (circle radius
/// from center to cursor).
pub(crate) fn dist(point_a: Point, point_b: Point) -> f32 {
    let (delta_x, delta_y) = (point_a.0 - point_b.0, point_a.1 - point_b.1);
    (delta_x * delta_x + delta_y * delta_y).sqrt()
}

/// The 12-color palette available to the user, arranged in a 4×3 grid in the dock.
/// Colors are chosen for high contrast against the dark canvas background (RGB 30,30,46).
///
/// Layout in the grid (left-to-right, top-to-bottom):
///   Row 0: Slate(30,41,59)   Red(239,68,68)     Blue(59,130,246)    Green(52,211,153)
///   Row 1: Amber(251,191,36) Pink(244,114,182)  Purple(139,92,246)  Cyan(34,211,238)
///   Row 2: White(241,245,249) Gray(100,116,139) Brown(180,83,9)     Emerald(4,120,87)
pub(crate) const PALETTE: [Color; 12] = [
    (30, 41, 59),    // Slate — near-black, subtle on dark bg
    (239, 68, 68),   // Red — vivid primary
    (59, 130, 246),  // Blue — vivid primary
    (52, 211, 153),  // Green — teal/emerald tone
    (251, 191, 36),  // Amber — warm yellow
    (244, 114, 182), // Pink — soft magenta
    (139, 92, 246),  // Purple — violet
    (34, 211, 238),  // Cyan — bright aqua
    (241, 245, 249), // White — near-white, for highlights
    (100, 116, 139), // Gray — muted neutral
    (180, 83, 9),    // Brown — earthy dark orange
    (4, 120, 87),    // Emerald — deep green
];

pub(crate) const SWATCH_COLUMNS: u32 = 4;
pub(crate) const SWATCH_SIZE: f32 = 28.0;
pub(crate) const SWATCH_SPACING: f32 = 32.0;
pub(crate) const SWATCH_HIT_PADDING: f32 = 4.0;
pub(crate) const BUTTON_WIDTH: f32 = 64.0;
pub(crate) const BUTTON_HEIGHT: f32 = 22.0;
pub(crate) const BUTTON_GAP: f32 = 4.0;
pub(crate) const DOCK_HEIGHT: f32 = 120.0;
pub(crate) const DOCK_BACKGROUND: Color = (24, 24, 34);
