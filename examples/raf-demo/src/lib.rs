use oxide_sdk::*;

const BG: (u8, u8, u8) = (25, 25, 40);
const ACCENT: (u8, u8, u8) = (80, 160, 220);
const BALL_COLOR: (u8, u8, u8) = (240, 100, 100);

const CB_ANIMATION: u32 = 1;
const BTN_TOGGLE: u32 = 100;

static mut BALL_X: f32 = 100.0;
static mut BALL_Y: f32 = 100.0;
static mut VX: f32 = 3.0;
static mut VY: f32 = 2.0;
static mut RUNNING: bool = true;
static mut RAF_ID: u32 = 0;

#[no_mangle]
pub extern "C" fn start_app() {
    log("RAF Demo loaded! Using request_animation_frame for smooth physics updates.");
    unsafe {
        RAF_ID = request_animation_frame(CB_ANIMATION);
    }
}

#[no_mangle]
pub extern "C" fn on_timer(callback_id: u32) {
    if callback_id == CB_ANIMATION && unsafe { RUNNING } {
        unsafe {
            // Simple physics update (called ~60x/sec via rAF)
            BALL_X += VX;
            BALL_Y += VY;

            let (w, h) = canvas_dimensions();
            let width = w as f32;
            let height = h as f32;
            let radius = 20.0;

            // Bounce off walls
            if BALL_X - radius < 0.0 || BALL_X + radius > width {
                VX = -VX;
                BALL_X = BALL_X.clamp(radius, width - radius);
            }
            if BALL_Y - radius < 0.0 || BALL_Y + radius > height {
                VY = -VY;
                BALL_Y = BALL_Y.clamp(radius, height - radius);
            }

            // Request next frame to continue animation
            RAF_ID = request_animation_frame(CB_ANIMATION);
        }
    }
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let (width, height) = canvas_dimensions();
    let w = width as f32;
    let h = height as f32;

    canvas_clear(BG.0, BG.1, BG.2, 255);

    // Header
    canvas_rect(0.0, 0.0, w, 60.0, ACCENT.0, ACCENT.1, ACCENT.2, 255);
    canvas_text(20.0, 18.0, 24.0, 255, 255, 255, 255, "Oxide RAF Demo");
    canvas_text(
        20.0,
        42.0,
        13.0,
        200,
        220,
        255,
        255,
        "request_animation_frame + on_timer for vsync-aligned updates",
    );

    unsafe {
        // Draw bouncing ball
        let x = BALL_X;
        let y = BALL_Y;
        canvas_circle(x, y, 20.0, BALL_COLOR.0, BALL_COLOR.1, BALL_COLOR.2, 255);

        // Velocity indicators
        canvas_line(x, y, x + VX * 5.0, y + VY * 5.0, 255, 255, 100, 255, 3.0);

        let status = if RUNNING {
            "RUNNING (rAF driven)"
        } else {
            "PAUSED"
        };
        let status_color = if RUNNING {
            (80, 220, 120)
        } else {
            (200, 100, 100)
        };
        canvas_text(
            20.0,
            80.0,
            16.0,
            status_color.0,
            status_color.1,
            status_color.2,
            255,
            status,
        );

        let vx = VX;
        let vy = VY;
        canvas_text(
            20.0,
            110.0,
            13.0,
            180,
            180,
            200,
            255,
            &format!("pos: ({:.1}, {:.1})  vel: ({:.1}, {:.1})", x, y, vx, vy),
        );

        // Toggle button
        let label = if RUNNING {
            "Pause Animation"
        } else {
            "Resume Animation"
        };
        if ui_button(BTN_TOGGLE, 20.0, 140.0, 180.0, 35.0, label) {
            RUNNING = !RUNNING;
            if RUNNING {
                RAF_ID = request_animation_frame(CB_ANIMATION);
            } else {
                cancel_animation_frame(RAF_ID);
            }
        }

        // Instructions
        canvas_text(
            20.0,
            h - 60.0,
            12.0,
            140,
            140,
            160,
            255,
            "Physics updated via request_animation_frame callback (on_timer).",
        );
        canvas_text(
            20.0,
            h - 40.0,
            12.0,
            140,
            140,
            160,
            255,
            "on_frame only handles rendering. Cancel/resume via button.",
        );
    }
}
