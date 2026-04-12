//! Sample MP4 playback demo (requires FFmpeg on the host and network for the sample URL).

use oxide_sdk::*;

/// Public sample MP4 (Google-hosted test asset; replace with your own URL if needed).
const SAMPLE_MP4: &str = "https://d2qguwbxlx1sbt.cloudfront.net/TextInMotion-VideoSample-1080p.mp4";

#[no_mangle]
pub extern "C" fn start_app() {
    log("Video demo: loading sample MP4 (Big Buck Bunny)...");
    match video_load_url(SAMPLE_MP4) {
        0 => {
            log("video_load_url OK");
            video_set_loop(true);
            video_play();
            video_set_pip(true);
        }
        e => log(&format!("video_load_url failed: {e}")),
    }
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    canvas_clear(20, 22, 30, 255);
    let (cw, ch) = canvas_dimensions();
    let vw = (cw as f32) - 80.0;
    let vh = ((ch as f32) - 120.0).max(200.0);
    if video_render(40.0, 40.0, vw, vh) != 0 {
        canvas_text(
            40.0,
            40.0,
            20.0,
            220,
            120,
            120,
            255,
            "video_render failed — load a video or check network / FFmpeg",
        );
        return;
    }
    let pos = video_position();
    let dur = video_duration();
    canvas_text(
        40.0,
        40.0 + vh + 12.0,
        16.0,
        180,
        180,
        200,
        255,
        &format!("{pos} / {dur} ms"),
    );
    if ui_button(1, 40.0, ch as f32 - 52.0, 120.0, 36.0, "Pause") {
        video_pause();
    }
    if ui_button(2, 180.0, ch as f32 - 52.0, 120.0, 36.0, "Resume") {
        video_play();
    }
}
