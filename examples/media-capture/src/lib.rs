//! Demonstrates [`oxide_sdk`] media capture: camera preview (PNG-encoded for [`canvas_image`]),
//! microphone level meter, and a screen screenshot thumbnail.

use oxide_sdk::*;

use png::Encoder;

/// RGBA buffers for camera and screen (filled by host; we encode to PNG for `canvas_image`).
const MAX_RGBA: usize = 1920 * 1080 * 4;

static mut CAM_BUF: Vec<u8> = Vec::new();
static mut SCR_BUF: Vec<u8> = Vec::new();
static mut CAMERA_ON: bool = false;
static mut MIC_ON: bool = false;
static mut MIC_SMOOTH: f32 = 0.0;
static mut LAST_SCR_OK: bool = false;
static mut SCR_BYTES: usize = 0;

fn rgba_to_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let expected = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    if rgba.len() < expected {
        return None;
    }
    let mut out = Vec::new();
    {
        let mut enc = Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(&rgba[..expected]).ok()?;
    }
    Some(out)
}

#[no_mangle]
pub extern "C" fn start_app() {
    log("Media capture demo — approve camera, microphone, and screen when prompted.");
    unsafe {
        CAM_BUF = vec![0u8; MAX_RGBA];
        SCR_BUF = vec![0u8; MAX_RGBA];
    }
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let (cw, ch) = canvas_dimensions();
    let w = cw as f32;
    let h = ch as f32;

    canvas_clear(18, 20, 28, 255);

    canvas_text(20.0, 16.0, 22.0, 220, 210, 255, "Media capture");
    canvas_text(
        20.0,
        44.0,
        13.0,
        140,
        150,
        170,
        "Camera / mic / screenshot use host dialogs + OS permissions.",
    );

    let preview_x = 20.0;
    let preview_y = 72.0;
    let preview_w = (w - 40.0).max(200.0);
    // Reserve vertical space for buttons, thumbnail, and mic meter.
    let preview_h = (h - 260.0).clamp(100.0, 280.0);
    let btn_row_y = preview_y + preview_h + 12.0;
    let thumb_x = 20.0;
    let thumb_y = btn_row_y + 80.0;
    let thumb_w = (w - 40.0).min(420.0);
    let thumb_h = 110.0;
    let meter_y = thumb_y + thumb_h + 36.0;

    // ── Buttons ───────────────────────────────────────────────────────
    if ui_button(1, 20.0, btn_row_y, 140.0, 32.0, "Open camera") {
        unsafe {
            let code = camera_open();
            if code == 0 {
                CAMERA_ON = true;
                log("camera_open OK");
            } else {
                log(&format!("camera_open failed: {code}"));
            }
        }
    }
    if ui_button(2, 175.0, btn_row_y, 120.0, 32.0, "Close camera") {
        unsafe {
            camera_close();
            CAMERA_ON = false;
        }
    }
    if ui_button(3, 20.0, btn_row_y + 44.0, 140.0, 32.0, "Open mic") {
        unsafe {
            let code = microphone_open();
            if code == 0 {
                MIC_ON = true;
                log("microphone_open OK");
            } else {
                log(&format!("microphone_open failed: {code}"));
            }
        }
    }
    if ui_button(4, 175.0, btn_row_y + 44.0, 120.0, 32.0, "Close mic") {
        unsafe {
            microphone_close();
            MIC_ON = false;
            MIC_SMOOTH = 0.0;
        }
    }
    if ui_button(5, 310.0, btn_row_y, 160.0, 32.0, "Screenshot") {
        unsafe {
            match screen_capture(&mut SCR_BUF[..]) {
                Ok(n) if n > 0 => {
                    LAST_SCR_OK = true;
                    SCR_BYTES = n;
                    log(&format!("screen_capture OK ({n} bytes)"));
                }
                Ok(_) => log("screen_capture: 0 bytes"),
                Err(e) => log(&format!("screen_capture failed: {e}")),
            }
        }
    }

    // ── Camera preview ────────────────────────────────────────────────
    unsafe {
        if CAMERA_ON {
            let n = camera_capture_frame(&mut CAM_BUF[..]) as usize;
            let (iw, ih) = camera_frame_dimensions();
            let expected = (iw as usize).saturating_mul(ih as usize).saturating_mul(4);
            if n > 0 && n == expected {
                if let Some(png) = rgba_to_png(iw, ih, &CAM_BUF[..n]) {
                    canvas_image(preview_x, preview_y, preview_w, preview_h, &png);
                }
            }
        } else {
            canvas_rect(preview_x, preview_y, preview_w, preview_h, 35, 38, 48, 255);
            canvas_text(
                preview_x + 12.0,
                preview_y + preview_h * 0.45,
                16.0,
                120,
                128,
                140,
                "Open camera to preview",
            );
        }
    }

    // ── Screen thumbnail (below controls; drawn before mic so the meter stays on top) ──
    canvas_text(
        thumb_x,
        thumb_y - 18.0,
        13.0,
        160,
        165,
        180,
        "Last screenshot",
    );

    unsafe {
        if LAST_SCR_OK {
            let (sw, sh) = screen_capture_dimensions();
            let exp = (sw as usize).saturating_mul(sh as usize).saturating_mul(4);
            if exp > 0 && SCR_BYTES >= exp && exp <= MAX_RGBA {
                if let Some(png) = rgba_to_png(sw, sh, &SCR_BUF[..exp]) {
                    canvas_image(thumb_x, thumb_y, thumb_w, thumb_h, &png);
                }
            }
        } else {
            canvas_rect(thumb_x, thumb_y, thumb_w, thumb_h, 32, 36, 42, 255);
            canvas_text(
                thumb_x + 24.0,
                thumb_y + 48.0,
                13.0,
                100,
                110,
                125,
                "No grab yet",
            );
        }
    }

    // ── Mic level ─────────────────────────────────────────────────────
    let meter_x = 20.0;
    let meter_w = (w - 40.0).min(520.0);
    canvas_text(
        meter_x,
        meter_y - 22.0,
        14.0,
        180,
        185,
        200,
        "Microphone (mono)",
    );
    canvas_rect(meter_x, meter_y, meter_w, 14.0, 40, 44, 52, 255);

    unsafe {
        if MIC_ON {
            let mut samples = [0.0f32; 4096];
            let got = microphone_read_samples(&mut samples);
            if got > 0 {
                let g = got as usize;
                let mut acc = 0.0f32;
                for s in samples.iter().take(g) {
                    acc += s * s;
                }
                let rms = (acc / g as f32).sqrt();
                MIC_SMOOTH = MIC_SMOOTH * 0.85 + rms * 0.15;
            }
            let level = (MIC_SMOOTH * 8.0).min(1.0);
            let fill = meter_w * level;
            canvas_rect(meter_x, meter_y, fill, 14.0, 80, 200, 140, 255);
            let hz = microphone_sample_rate();
            canvas_text(
                meter_x + meter_w + 12.0,
                meter_y - 2.0,
                12.0,
                130,
                140,
                155,
                &format!("{hz} Hz"),
            );
        } else {
            canvas_text(meter_x + 8.0, meter_y - 2.0, 12.0, 100, 110, 125, "closed");
        }
    }

    // ── Pipeline stats ────────────────────────────────────────────────
    let (frames, ring) = media_pipeline_stats();
    let footer_y = (meter_y + 36.0).min(h - 18.0);
    canvas_text(
        20.0,
        footer_y,
        11.0,
        90,
        95,
        105,
        &format!("pipeline: cam_frames={frames} mic_ring~{ring}"),
    );
}
