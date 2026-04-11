use oxide_sdk::*;

static mut COUNTER: i32 = 0;

#[no_mangle]
pub extern "C" fn start_app() {
    log("Interactive Oxide app loaded! (on_frame loop active)");
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let (width, _height) = canvas_dimensions();

    canvas_clear(30, 30, 46, 255);

    // ── Title bar ───────────────────────────────────────────────────
    canvas_rect(0.0, 0.0, width as f32, 56.0, 50, 40, 80, 255);
    canvas_text(
        20.0,
        16.0,
        24.0,
        220,
        200,
        255,
        255,
        "Oxide Interactive Widgets",
    );

    // ── Button demo ─────────────────────────────────────────────────
    canvas_text(20.0, 75.0, 16.0, 180, 140, 255, 255, "Button");

    if ui_button(1, 20.0, 100.0, 120.0, 28.0, "Click Me!") {
        unsafe {
            COUNTER += 1;
        }
    }
    if ui_button(2, 150.0, 100.0, 80.0, 28.0, "Reset") {
        unsafe {
            COUNTER = 0;
        }
    }
    let count = unsafe { COUNTER };
    canvas_text(
        245.0,
        105.0,
        14.0,
        160,
        220,
        160,
        255,
        &format!("Count: {count}"),
    );

    // ── Checkbox demo ───────────────────────────────────────────────
    canvas_text(20.0, 150.0, 16.0, 180, 140, 255, 255, "Checkbox");

    let dark_mode = ui_checkbox(10, 20.0, 175.0, "Dark mode", false);
    let notifications = ui_checkbox(11, 20.0, 205.0, "Enable notifications", true);

    canvas_text(
        280.0,
        180.0,
        13.0,
        120,
        120,
        120,
        255,
        &format!("dark={dark_mode}  notif={notifications}"),
    );

    // ── Slider demo ─────────────────────────────────────────────────
    canvas_text(20.0, 250.0, 16.0, 180, 140, 255, 255, "Slider");

    let volume = ui_slider(20, 20.0, 275.0, 300.0, 0.0, 100.0, 50.0);
    canvas_text(
        330.0,
        278.0,
        14.0,
        160,
        220,
        160,
        255,
        &format!("Volume: {volume:.0}%"),
    );

    let speed = ui_slider(21, 20.0, 310.0, 300.0, 0.1, 10.0, 1.0);
    canvas_text(
        330.0,
        313.0,
        14.0,
        160,
        220,
        160,
        255,
        &format!("Speed: {speed:.1}x"),
    );

    // ── Text input demo ─────────────────────────────────────────────
    canvas_text(20.0, 355.0, 16.0, 180, 140, 255, 255, "Text Input");

    let name = ui_text_input(30, 20.0, 380.0, 300.0, "");
    if !name.is_empty() {
        canvas_text(
            20.0,
            415.0,
            18.0,
            160,
            220,
            160,
            255,
            &format!("Hello, {name}!"),
        );
    } else {
        canvas_text(
            20.0,
            415.0,
            14.0,
            120,
            120,
            120,
            255,
            "Type your name above...",
        );
    }

    // ── Mouse info ──────────────────────────────────────────────────
    let (mx, my) = mouse_position();
    canvas_text(
        20.0,
        455.0,
        12.0,
        100,
        100,
        120,
        255,
        &format!(
            "Mouse: ({mx:.0}, {my:.0})  LMB: {}  RMB: {}",
            mouse_button_down(0),
            mouse_button_down(1),
        ),
    );

    // ── Decorative circles (react to slider) ────────────────────────
    let radius = volume * 0.6 + 10.0;
    canvas_circle(
        width as f32 - 100.0,
        120.0,
        radius,
        180,
        120,
        255,
        (volume * 2.5) as u8,
    );
    canvas_circle(
        width as f32 - 140.0,
        180.0,
        radius * 0.6,
        255,
        180,
        100,
        130,
    );
}
