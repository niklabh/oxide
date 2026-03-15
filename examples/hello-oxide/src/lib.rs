use oxide_sdk::*;

#[no_mangle]
pub extern "C" fn start_app() {
    log("Hello from the Oxide guest app!");

    let (width, height) = canvas_dimensions();
    log(&format!("Canvas size: {}x{}", width, height));

    // Dark background
    canvas_clear(30, 30, 46, 255);

    // Title bar
    canvas_rect(0.0, 0.0, width as f32, 60.0, 50, 40, 80, 255);
    canvas_text(20.0, 18.0, 24.0, 220, 200, 255, "Hello, Oxide!");

    // Decorative circles
    canvas_circle(400.0, 300.0, 80.0, 180, 120, 255, 200);
    canvas_circle(350.0, 260.0, 40.0, 255, 180, 100, 180);
    canvas_circle(460.0, 280.0, 50.0, 100, 220, 180, 180);

    // Info text
    canvas_text(20.0, 100.0, 16.0, 200, 200, 200, "This app is running as a .wasm module inside the Oxide browser.");
    canvas_text(20.0, 130.0, 16.0, 200, 200, 200, "It has zero access to the host filesystem or network.");

    // Demonstrate geolocation API
    let location = get_location();
    canvas_text(20.0, 180.0, 14.0, 160, 220, 160, &format!("Mock location: {}", location));
    log(&format!("Geolocation: {}", location));

    // Demonstrate storage API
    storage_set("visit_count", "1");
    let count = storage_get("visit_count");
    canvas_text(20.0, 210.0, 14.0, 160, 220, 160, &format!("Storage test: visit_count = {}", count));
    log(&format!("Storage: visit_count = {}", count));

    // Demonstrate time API
    let now = time_now_ms();
    canvas_text(20.0, 240.0, 14.0, 160, 220, 160, &format!("Current time: {} ms since epoch", now));

    // Demonstrate random API
    let rand_val = random_f64();
    canvas_text(20.0, 270.0, 14.0, 160, 220, 160, &format!("Random value: {:.6}", rand_val));

    // Grid lines
    let grid_y = 320.0;
    for i in 0..8 {
        let x = 20.0 + (i as f32) * 90.0;
        canvas_line(x, grid_y, x, grid_y + 100.0, 80, 80, 100, 1.0);
    }
    for i in 0..4 {
        let y = grid_y + (i as f32) * 33.0;
        canvas_line(20.0, y, 650.0, y, 80, 80, 100, 1.0);
    }

    // Notification
    notify("Oxide App", "Guest application loaded successfully!");

    log("start_app() completed.");
}
