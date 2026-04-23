//! Oxide Forge — base template.
//!
//! When Forge generates an app, the contents of this file are replaced with
//! Claude's output. The empty template renders a title card so a developer
//! can sanity-check that the pipeline works before typing a prompt.

use oxide_sdk::*;

#[no_mangle]
pub extern "C" fn start_app() {
    log("forge: base template loaded");
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let (w, h) = canvas_dimensions();
    canvas_clear(18, 18, 26, 255);
    canvas_text(
        20.0,
        20.0,
        28.0,
        220,
        220,
        255,
        255,
        "Oxide Forge — empty template",
    );
    canvas_text(
        20.0,
        60.0,
        14.0,
        140,
        140,
        160,
        255,
        "Type a prompt in oxide://forge to replace this file.",
    );
    let _ = (w, h);
}
