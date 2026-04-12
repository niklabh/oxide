//! Host-side media capture: camera, microphone, and screen (with permission prompts).
//!
//! Guests call [`register_media_capture_functions`] imports from the `oxide` module. Native
//! OS prompts (camera / microphone / screen recording) may appear in addition to Oxide’s
//! in-app confirmation dialogs.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;
use wasmtime::{Caller, Linker};

use crate::capabilities::{console_log, write_guest_bytes, ConsoleLevel, HostState};

const MIC_RING_CAP: usize = 96_000;

/// Shared capture state for a tab (camera stream, mic ring buffer, counters for pipeline stats).
#[derive(Default)]
pub struct MediaCaptureState {
    camera: Option<Camera>,
    last_frame_w: u32,
    last_frame_h: u32,
    camera_frames: u64,
    microphone: Option<MicrophoneInput>,
    screen_w: u32,
    screen_h: u32,
    screen_captures: u64,
}

struct MicrophoneInput {
    #[allow(dead_code)]
    stream: cpal::Stream,
    buffer: Arc<Mutex<VecDeque<f32>>>,
    sample_rate: u32,
}

fn prompt(feature: &str) -> bool {
    matches!(
        rfd::MessageDialog::new()
            .set_title("Oxide")
            .set_description(format!("Allow this page to access {feature}?"))
            .set_buttons(rfd::MessageButtons::OkCancel)
            .show(),
        rfd::MessageDialogResult::Ok | rfd::MessageDialogResult::Yes
    )
}

fn push_mono_f32(data: &[f32], channels: usize, ring: &Arc<Mutex<VecDeque<f32>>>) {
    let ch = channels.max(1);
    let frames = data.len() / ch;
    let mut q = ring.lock().unwrap();
    for i in 0..frames {
        let mut sum = 0.0f32;
        for c in 0..ch {
            sum += data[i * ch + c];
        }
        let m = sum / ch as f32;
        while q.len() >= MIC_RING_CAP {
            q.pop_front();
        }
        q.push_back(m);
    }
}

fn push_mono_i16(data: &[i16], channels: usize, ring: &Arc<Mutex<VecDeque<f32>>>) {
    let ch = channels.max(1);
    let frames = data.len() / ch;
    let mut q = ring.lock().unwrap();
    for i in 0..frames {
        let mut sum = 0.0f32;
        for c in 0..ch {
            sum += data[i * ch + c] as f32 / 32768.0;
        }
        let m = sum / ch as f32;
        while q.len() >= MIC_RING_CAP {
            q.pop_front();
        }
        q.push_back(m);
    }
}

fn push_mono_u16(data: &[u16], channels: usize, ring: &Arc<Mutex<VecDeque<f32>>>) {
    let ch = channels.max(1);
    let frames = data.len() / ch;
    let mut q = ring.lock().unwrap();
    for i in 0..frames {
        let mut sum = 0.0f32;
        for c in 0..ch {
            sum += (data[i * ch + c] as f32 - 32768.0) / 32768.0;
        }
        let m = sum / ch as f32;
        while q.len() >= MIC_RING_CAP {
            q.pop_front();
        }
        q.push_back(m);
    }
}

fn open_microphone(
    console: &Arc<Mutex<Vec<crate::capabilities::ConsoleEntry>>>,
) -> Result<MicrophoneInput, i32> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| log_err(console, -2, "[MIC] No input device".to_string()))?;
    let supported = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            return Err(log_err(console, -3, format!("[MIC] Config: {e}")));
        }
    };
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.clone().into();
    let channels = config.channels as usize;
    let ring = Arc::new(Mutex::new(VecDeque::with_capacity(MIC_RING_CAP)));
    let ring2 = ring.clone();
    let console_err = console.clone();
    let err_fn = move |e| {
        console_log(
            &console_err,
            ConsoleLevel::Warn,
            format!("[MIC] Stream error: {e}"),
        );
    };

    let stream = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _| push_mono_f32(data, channels, &ring2),
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _| push_mono_i16(data, channels, &ring2),
            err_fn,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            &config,
            move |data: &[u16], _| push_mono_u16(data, channels, &ring2),
            err_fn,
            None,
        ),
        other => {
            return Err(log_err(
                console,
                -3,
                format!("[MIC] Unsupported sample format {other:?}"),
            ));
        }
    };
    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            return Err(log_err(console, -3, format!("[MIC] Build stream: {e}")));
        }
    };
    if let Err(e) = stream.play() {
        return Err(log_err(console, -3, format!("[MIC] Play: {e}")));
    }
    let sample_rate = supported.sample_rate();
    Ok(MicrophoneInput {
        stream,
        buffer: ring,
        sample_rate,
    })
}

fn log_err(
    console: &Arc<Mutex<Vec<crate::capabilities::ConsoleEntry>>>,
    code: i32,
    msg: String,
) -> i32 {
    console_log(console, ConsoleLevel::Warn, msg);
    code
}

/// Register `api_camera_*`, `api_microphone_*`, `api_screen_capture`, and `api_media_pipeline_stats`.
pub fn register_media_capture_functions(linker: &mut Linker<HostState>) -> Result<()> {
    linker.func_wrap(
        "oxide",
        "api_camera_open",
        |caller: Caller<'_, HostState>| -> i32 {
            let console = caller.data().console.clone();
            let st = caller.data().media_capture.clone();
            if !prompt("the camera") {
                return -1;
            }
            let mut g = st.lock().unwrap();
            if let Some(mut cam) = g.camera.take() {
                let _ = cam.stop_stream();
            }
            let cams = match nokhwa::query(nokhwa::utils::ApiBackend::Auto) {
                Ok(c) => c,
                Err(e) => {
                    return log_err(&console, -2, format!("[CAMERA] No cameras: {e}"));
                }
            };
            if cams.is_empty() {
                return log_err(&console, -2, "[CAMERA] No cameras found".to_string());
            }
            let req = RequestedFormat::new::<RgbFormat>(RequestedFormatType::HighestResolution(
                nokhwa::utils::Resolution::new(1280, 720),
            ));
            let mut camera = match Camera::new(CameraIndex::Index(0), req) {
                Ok(c) => c,
                Err(e) => {
                    return log_err(&console, -3, format!("[CAMERA] Open failed: {e}"));
                }
            };
            if let Err(e) = camera.open_stream() {
                return log_err(&console, -3, format!("[CAMERA] Stream: {e}"));
            }
            g.camera = Some(camera);
            0
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_camera_close",
        |caller: Caller<'_, HostState>| {
            let st = caller.data().media_capture.clone();
            let mut g = st.lock().unwrap();
            if let Some(mut cam) = g.camera.take() {
                let _ = cam.stop_stream();
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_camera_capture_frame",
        |mut caller: Caller<'_, HostState>, out_ptr: u32, out_cap: u32| -> u32 {
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return 0,
            };
            let st = caller.data().media_capture.clone();
            let mut g = st.lock().unwrap();
            let cam = match g.camera.as_mut() {
                Some(c) => c,
                None => return 0,
            };
            let buffer = match cam.frame() {
                Ok(b) => b,
                Err(e) => {
                    console_log(
                        &caller.data().console,
                        ConsoleLevel::Warn,
                        format!("[CAMERA] Frame: {e}"),
                    );
                    return 0;
                }
            };
            let img = match buffer.decode_image::<RgbFormat>() {
                Ok(i) => i,
                Err(e) => {
                    console_log(
                        &caller.data().console,
                        ConsoleLevel::Warn,
                        format!("[CAMERA] Decode: {e}"),
                    );
                    return 0;
                }
            };
            let w = img.width();
            let h = img.height();
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for px in img.pixels() {
                let p = px.0;
                rgba.push(p[0]);
                rgba.push(p[1]);
                rgba.push(p[2]);
                rgba.push(255);
            }
            g.last_frame_w = w;
            g.last_frame_h = h;
            g.camera_frames = g.camera_frames.saturating_add(1);
            let write_len = rgba.len().min(out_cap as usize);
            if write_guest_bytes(&mem, &mut caller, out_ptr, &rgba[..write_len]).is_err() {
                return 0;
            }
            write_len as u32
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_camera_frame_dimensions",
        |caller: Caller<'_, HostState>| -> u64 {
            let g = caller.data().media_capture.lock().unwrap();
            ((g.last_frame_w as u64) << 32) | (g.last_frame_h as u64)
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_microphone_open",
        |caller: Caller<'_, HostState>| -> i32 {
            let console = caller.data().console.clone();
            let st = caller.data().media_capture.clone();
            if !prompt("the microphone") {
                return -1;
            }
            let mut g = st.lock().unwrap();
            g.microphone = None;
            match open_microphone(&console) {
                Ok(m) => {
                    g.microphone = Some(m);
                    0
                }
                Err(code) => code,
            }
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_microphone_close",
        |caller: Caller<'_, HostState>| {
            let st = caller.data().media_capture.clone();
            st.lock().unwrap().microphone = None;
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_microphone_sample_rate",
        |caller: Caller<'_, HostState>| -> u32 {
            let g = caller.data().media_capture.lock().unwrap();
            g.microphone.as_ref().map(|m| m.sample_rate).unwrap_or(0)
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_microphone_read_samples",
        |mut caller: Caller<'_, HostState>, out_ptr: u32, max_samples: u32| -> u32 {
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return 0,
            };
            let st = caller.data().media_capture.clone();
            let g = st.lock().unwrap();
            let mic = match g.microphone.as_ref() {
                Some(m) => m,
                None => return 0,
            };
            let mut q = mic.buffer.lock().unwrap();
            let take = (max_samples as usize).min(q.len());
            let mut chunk = Vec::with_capacity(take * 4);
            for _ in 0..take {
                if let Some(s) = q.pop_front() {
                    chunk.extend_from_slice(&s.to_le_bytes());
                }
            }
            let write_len = chunk.len().min((max_samples as usize).saturating_mul(4));
            if write_guest_bytes(&mem, &mut caller, out_ptr, &chunk[..write_len]).is_err() {
                return 0;
            }
            (write_len / 4) as u32
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_screen_capture",
        |mut caller: Caller<'_, HostState>, out_ptr: u32, out_cap: u32| -> i32 {
            let mem = match caller.data().memory {
                Some(m) => m,
                None => return -4,
            };
            let console = caller.data().console.clone();
            if !prompt("screen capture (the OS may also ask for screen recording permission)") {
                return -1;
            }
            let screens = match screenshots::Screen::all() {
                Ok(s) => s,
                Err(e) => {
                    console_log(
                        &console,
                        ConsoleLevel::Warn,
                        format!("[SCREEN] Enumerate: {e}"),
                    );
                    return -2;
                }
            };
            let screen = match screens.first() {
                Some(s) => s,
                None => {
                    return log_err(&console, -2, "[SCREEN] No displays".to_string());
                }
            };
            let img = match screen.capture() {
                Ok(i) => i,
                Err(e) => {
                    console_log(
                        &console,
                        ConsoleLevel::Warn,
                        format!("[SCREEN] Capture: {e}"),
                    );
                    return -3;
                }
            };
            let w = img.width();
            let h = img.height();
            let rgba = img.into_raw();
            let st = caller.data().media_capture.clone();
            {
                let mut g = st.lock().unwrap();
                g.screen_w = w;
                g.screen_h = h;
                g.screen_captures = g.screen_captures.saturating_add(1);
            }
            let write_len = rgba.len().min(out_cap as usize);
            if write_guest_bytes(&mem, &mut caller, out_ptr, &rgba[..write_len]).is_err() {
                return -4;
            }
            write_len as i32
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_screen_capture_dimensions",
        |caller: Caller<'_, HostState>| -> u64 {
            let g = caller.data().media_capture.lock().unwrap();
            ((g.screen_w as u64) << 32) | (g.screen_h as u64)
        },
    )?;

    linker.func_wrap(
        "oxide",
        "api_media_pipeline_stats",
        |caller: Caller<'_, HostState>| -> u64 {
            let g = caller.data().media_capture.lock().unwrap();
            let mic_ring = g
                .microphone
                .as_ref()
                .map(|m| m.buffer.lock().unwrap().len() as u32)
                .unwrap_or(0);
            (g.camera_frames << 32) | (mic_ring as u64)
        },
    )?;

    Ok(())
}
