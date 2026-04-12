use oxide_sdk::draw::*;
use oxide_sdk::*;

static mut TIME: f32 = 0.0;

#[no_mangle]
pub extern "C" fn start_app() {
    log("GPU & Graphics demo loaded!");
}

#[no_mangle]
pub extern "C" fn on_frame(dt_ms: u32) {
    let t = unsafe {
        TIME += dt_ms as f32 / 1000.0;
        TIME
    };

    let c = Canvas::new();
    let (w, _h) = c.dimensions();
    let w = w as f32;

    // ── Solid background ─────────────────────────────────────────────
    c.clear(Color::hex(0x1a1a2e));

    // ── Title ────────────────────────────────────────────────────────
    c.fill_rounded_rect(Rect::new(0.0, 0.0, w, 52.0), 0.0, Color::hex(0x16213e));
    c.text("Phase 2 — GPU & Graphics", Point2D::new(20.0, 14.0), 22.0, Color::WHITE);

    // ── Rounded Rectangles ───────────────────────────────────────────
    let sy = 66.0;
    c.text("Rounded Rects", Point2D::new(20.0, sy), 14.0, Color::hex(0xf9a825));

    let colors = [0xe91e63, 0x2196f3, 0x4caf50, 0xff9800];
    for i in 0..4u32 {
        let x = 20.0 + i as f32 * 105.0;
        let radius = 4.0 + i as f32 * 8.0;
        c.fill_rounded_rect(
            Rect::new(x, sy + 20.0, 90.0, 48.0),
            radius,
            Color::hex(colors[i as usize]),
        );
        c.text(
            &format!("r={radius:.0}"),
            Point2D::new(x + 26.0, sy + 36.0),
            12.0,
            Color::WHITE,
        );
    }

    // ── Arcs ─────────────────────────────────────────────────────────
    let sy = 150.0;
    c.text("Arcs", Point2D::new(20.0, sy), 14.0, Color::hex(0xf9a825));

    let arc_colors = [0x00bcd4, 0xff5722, 0x8bc34a];
    for i in 0..3 {
        let cx = 70.0 + i as f32 * 130.0;
        let cy = sy + 56.0;
        let sweep = core::f32::consts::PI * (0.6 + i as f32 * 0.4);
        let start = t * (1.0 + i as f32 * 0.4);
        // Ghost ring
        c.save();
        c.set_opacity(0.15);
        c.arc(Point2D::new(cx, cy), 26.0, 0.0, core::f32::consts::TAU, 1.0, Color::hex(arc_colors[i]));
        c.restore();
        // Animated arc
        c.arc(Point2D::new(cx, cy), 26.0, start, start + sweep, 3.0, Color::hex(arc_colors[i]));
    }

    // ── Bézier Curves ────────────────────────────────────────────────
    let sy = 240.0;
    c.text("Bézier Curves", Point2D::new(20.0, sy), 14.0, Color::hex(0xf9a825));

    let wave_y = sy + 46.0;
    let amp = 30.0 * (t * 0.8).sin();
    for i in 0..3 {
        let x0 = 20.0 + i as f32 * 150.0;
        let dy = amp * (t * 2.0 + i as f32 * 0.8).sin();
        c.bezier(
            Point2D::new(x0, wave_y),
            Point2D::new(x0 + 40.0, wave_y - 35.0 + dy),
            Point2D::new(x0 + 90.0, wave_y + 35.0 - dy),
            Point2D::new(x0 + 130.0, wave_y),
            2.5,
            Color::rgba(130, 180, 255, 220),
        );
    }

    // ── Gradients ────────────────────────────────────────────────────
    let sy = 320.0;
    c.text("Gradients", Point2D::new(20.0, sy), 14.0, Color::hex(0xf9a825));

    c.text("Linear", Point2D::new(20.0, sy + 18.0), 11.0, Color::rgba(180, 180, 180, 180));
    c.linear_gradient(
        Rect::new(20.0, sy + 32.0, 220.0, 32.0),
        &[
            GradientStop::new(0.0, Color::hex(0xff6b6b)),
            GradientStop::new(0.5, Color::hex(0xfeca57)),
            GradientStop::new(1.0, Color::hex(0x48dbfb)),
        ],
    );

    c.text("Radial", Point2D::new(260.0, sy + 18.0), 11.0, Color::rgba(180, 180, 180, 180));
    c.radial_gradient(
        Rect::new(260.0, sy + 24.0, 60.0, 44.0),
        &[
            GradientStop::new(0.0, Color::WHITE),
            GradientStop::new(1.0, Color::hex(0x6c5ce7)),
        ],
    );

    // ── Transforms ───────────────────────────────────────────────────
    let sy = 400.0;
    c.text("Transforms", Point2D::new(20.0, sy), 14.0, Color::hex(0xf9a825));

    // Orbiting dots
    let ocx = 80.0;
    let ocy = sy + 52.0;
    for i in 0..6 {
        let angle = t * 1.5 + i as f32 * core::f32::consts::TAU / 6.0;
        let ox = ocx + 32.0 * angle.cos();
        let oy = ocy + 32.0 * angle.sin();
        c.fill_circle(Point2D::new(ox, oy), 5.0, Color::rgba(100, 200, 255, 200));
    }
    c.fill_circle(Point2D::new(ocx, ocy), 4.0, Color::WHITE);

    // Translated squares
    for i in 0..3 {
        c.save();
        let dx = 200.0 + i as f32 * 60.0;
        let dy = sy + 28.0 + 10.0 * (t * 2.0 + i as f32).sin();
        c.translate(dx, dy);
        c.fill_rounded_rect(Rect::new(0.0, 0.0, 40.0, 40.0), 6.0, Color::rgba(255, 140, 100, 200));
        c.restore();
    }

    // ── Clipping ─────────────────────────────────────────────────────
    let sy = 490.0;
    c.text("Clipping", Point2D::new(20.0, sy), 14.0, Color::hex(0xf9a825));

    let cx = 20.0;
    let cy = sy + 20.0;
    c.save();
    c.clip(Rect::new(cx, cy, 180.0, 44.0));
    // Wide rect that gets clipped
    c.fill_rect(Rect::new(cx - 10.0, cy - 5.0, 200.0, 54.0), Color::hex(0x00897b));
    // Bouncing ball inside clip
    let bx = cx + 90.0 + 70.0 * (t * 2.0).sin();
    c.fill_circle(Point2D::new(bx, cy + 22.0), 14.0, Color::WHITE);
    c.restore();

    // Clip outline
    c.line(Point2D::new(cx, cy), Point2D::new(cx + 180.0, cy), 1.0, Color::rgba(255, 255, 255, 60));
    c.line(Point2D::new(cx, cy + 44.0), Point2D::new(cx + 180.0, cy + 44.0), 1.0, Color::rgba(255, 255, 255, 60));
    c.line(Point2D::new(cx, cy), Point2D::new(cx, cy + 44.0), 1.0, Color::rgba(255, 255, 255, 60));
    c.line(Point2D::new(cx + 180.0, cy), Point2D::new(cx + 180.0, cy + 44.0), 1.0, Color::rgba(255, 255, 255, 60));
    c.text("clipped", Point2D::new(cx + 190.0, cy + 14.0), 11.0, Color::rgba(140, 140, 140, 180));

    // ── Opacity ──────────────────────────────────────────────────────
    let sy = 570.0;
    c.text("Opacity", Point2D::new(20.0, sy), 14.0, Color::hex(0xf9a825));

    for i in 0..5 {
        let alpha = 1.0 - i as f32 * 0.2;
        let x = 20.0 + i as f32 * 85.0;
        c.save();
        c.set_opacity(alpha);
        c.fill_rounded_rect(Rect::new(x, sy + 20.0, 72.0, 36.0), 8.0, Color::hex(0xe91e63));
        c.text(
            &format!("{:.0}%", alpha * 100.0),
            Point2D::new(x + 18.0, sy + 30.0),
            13.0,
            Color::WHITE,
        );
        c.restore();
    }

    // ── FPS ──────────────────────────────────────────────────────────
    if dt_ms > 0 {
        let fps = 1000.0 / dt_ms as f32;
        c.text(
            &format!("{fps:.0} fps"),
            Point2D::new(w - 66.0, 18.0),
            12.0,
            Color::rgba(120, 255, 120, 200),
        );
    }
}
