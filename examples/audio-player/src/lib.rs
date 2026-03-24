use oxide_sdk::*;

const NOTE_A4: f32 = 440.0;
const NOTE_C5: f32 = 523.25;
const NOTE_E5: f32 = 659.25;
const NOTE_G5: f32 = 783.99;

const BG_COLOR: (u8, u8, u8) = (25, 25, 40);
const ACCENT: (u8, u8, u8) = (100, 80, 200);
const TEXT_DIM: (u8, u8, u8) = (140, 130, 160);
const TEXT_BRIGHT: (u8, u8, u8) = (230, 220, 255);
const GREEN: (u8, u8, u8) = (80, 220, 120);
const ORANGE: (u8, u8, u8) = (240, 180, 60);
const RED: (u8, u8, u8) = (220, 80, 80);

const WIDGET_FREQ: u32 = 100;
const WIDGET_DUR: u32 = 101;
const WIDGET_VOL: u32 = 102;
const WIDGET_URL: u32 = 103;

const BTN_TONE_A: u32 = 200;
const BTN_TONE_C: u32 = 201;
const BTN_TONE_E: u32 = 202;
const BTN_TONE_G: u32 = 203;
const BTN_PLAY_CUSTOM: u32 = 204;
const BTN_FETCH: u32 = 205;
const BTN_PAUSE: u32 = 210;
const BTN_RESUME: u32 = 211;
const BTN_STOP: u32 = 212;
const BTN_SFX_BLIP: u32 = 220;
const BTN_SFX_BEEP: u32 = 221;
const BTN_SFX_CHIRP: u32 = 222;

const WIDGET_LOOP: u32 = 110;
const WIDGET_SFX_VOL: u32 = 111;

const SFX_CHANNEL: u32 = 1;

static mut LAST_NOTE: &str = "";

#[no_mangle]
pub extern "C" fn start_app() {
    log("Oxide Audio Player loaded!");
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    let (width, _height) = canvas_dimensions();
    let w = width as f32;

    canvas_clear(BG_COLOR.0, BG_COLOR.1, BG_COLOR.2, 255);

    // ── Header ──────────────────────────────────────────────────────
    canvas_rect(0.0, 0.0, w, 52.0, ACCENT.0, ACCENT.1, ACCENT.2, 255);
    canvas_text(20.0, 14.0, 22.0, 255, 255, 255, "Oxide Audio Player");

    // ── Tone Pads ───────────────────────────────────────────────────
    canvas_text(
        20.0,
        72.0,
        14.0,
        TEXT_DIM.0,
        TEXT_DIM.1,
        TEXT_DIM.2,
        "TONE PADS",
    );

    let pad_w = 90.0;
    let pad_h = 36.0;
    let pad_y = 95.0;

    if ui_button(BTN_TONE_A, 20.0, pad_y, pad_w, pad_h, "A4  440Hz") {
        play_tone(NOTE_A4, 1.5);
        unsafe { LAST_NOTE = "A4 (440 Hz)" };
    }
    if ui_button(BTN_TONE_C, 120.0, pad_y, pad_w, pad_h, "C5  523Hz") {
        play_tone(NOTE_C5, 1.5);
        unsafe { LAST_NOTE = "C5 (523 Hz)" };
    }
    if ui_button(BTN_TONE_E, 220.0, pad_y, pad_w, pad_h, "E5  659Hz") {
        play_tone(NOTE_E5, 1.5);
        unsafe { LAST_NOTE = "E5 (659 Hz)" };
    }
    if ui_button(BTN_TONE_G, 320.0, pad_y, pad_w, pad_h, "G5  784Hz") {
        play_tone(NOTE_G5, 1.5);
        unsafe { LAST_NOTE = "G5 (784 Hz)" };
    }

    // ── Custom Tone ─────────────────────────────────────────────────
    canvas_text(
        20.0,
        155.0,
        14.0,
        TEXT_DIM.0,
        TEXT_DIM.1,
        TEXT_DIM.2,
        "CUSTOM TONE",
    );

    canvas_text(
        20.0,
        180.0,
        13.0,
        TEXT_BRIGHT.0,
        TEXT_BRIGHT.1,
        TEXT_BRIGHT.2,
        "Frequency (Hz)",
    );
    let freq = ui_slider(WIDGET_FREQ, 140.0, 178.0, 250.0, 100.0, 2000.0, 440.0);
    canvas_text(
        400.0,
        180.0,
        13.0,
        GREEN.0,
        GREEN.1,
        GREEN.2,
        &format!("{freq:.0} Hz"),
    );

    canvas_text(
        20.0,
        210.0,
        13.0,
        TEXT_BRIGHT.0,
        TEXT_BRIGHT.1,
        TEXT_BRIGHT.2,
        "Duration (sec)",
    );
    let dur = ui_slider(WIDGET_DUR, 140.0, 208.0, 250.0, 0.1, 5.0, 1.0);
    canvas_text(
        400.0,
        210.0,
        13.0,
        GREEN.0,
        GREEN.1,
        GREEN.2,
        &format!("{dur:.1} s"),
    );

    let looping = ui_checkbox(WIDGET_LOOP, 170.0, 245.0, "Loop", false);
    audio_set_loop(looping);

    if ui_button(BTN_PLAY_CUSTOM, 20.0, 240.0, 130.0, 30.0, "Play Tone") {
        play_tone(freq, dur);
        unsafe { LAST_NOTE = "Custom" };
    }

    // ── URL Player ──────────────────────────────────────────────────
    canvas_text(
        20.0,
        290.0,
        14.0,
        TEXT_DIM.0,
        TEXT_DIM.1,
        TEXT_DIM.2,
        "PLAY FROM URL",
    );

    let url = ui_text_input(WIDGET_URL, 20.0, 313.0, 350.0, "");
    if ui_button(BTN_FETCH, 380.0, 313.0, 80.0, 26.0, "Fetch") && !url.is_empty() {
        let rc = audio_play_url(&url);
        if rc == 0 {
            unsafe { LAST_NOTE = "URL stream" };
        }
    }

    // ── Playback Controls ───────────────────────────────────────────
    canvas_line(20.0, 355.0, w - 20.0, 355.0, 50, 45, 70, 1.0);

    canvas_text(
        20.0, 370.0, 14.0, TEXT_DIM.0, TEXT_DIM.1, TEXT_DIM.2, "CONTROLS",
    );

    if ui_button(BTN_PAUSE, 20.0, 393.0, 80.0, 30.0, "Pause") {
        audio_pause();
    }
    if ui_button(BTN_RESUME, 110.0, 393.0, 80.0, 30.0, "Resume") {
        audio_resume();
    }
    if ui_button(BTN_STOP, 200.0, 393.0, 80.0, 30.0, "Stop") {
        audio_stop();
    }

    canvas_text(
        20.0, 440.0, 13.0, TEXT_BRIGHT.0, TEXT_BRIGHT.1, TEXT_BRIGHT.2, "Volume",
    );
    let vol = ui_slider(WIDGET_VOL, 80.0, 438.0, 250.0, 0.0, 1.5, 1.0);
    audio_set_volume(vol);
    canvas_text(
        340.0, 440.0, 13.0, GREEN.0, GREEN.1, GREEN.2,
        &format!("{:.0}%", vol * 100.0),
    );

    // ── SFX Channel ─────────────────────────────────────────────────
    canvas_line(20.0, 468.0, w - 20.0, 468.0, 50, 45, 70, 1.0);

    canvas_text(
        20.0, 480.0, 14.0, TEXT_DIM.0, TEXT_DIM.1, TEXT_DIM.2,
        "SFX CHANNEL (plays over main audio)",
    );

    if ui_button(BTN_SFX_BLIP, 20.0, 503.0, 80.0, 28.0, "Blip") {
        let wav = generate_wav(1200.0, 0.08);
        audio_channel_play(SFX_CHANNEL, &wav);
    }
    if ui_button(BTN_SFX_BEEP, 110.0, 503.0, 80.0, 28.0, "Beep") {
        let wav = generate_wav(880.0, 0.25);
        audio_channel_play(SFX_CHANNEL, &wav);
    }
    if ui_button(BTN_SFX_CHIRP, 200.0, 503.0, 80.0, 28.0, "Chirp") {
        let wav = generate_chirp();
        audio_channel_play(SFX_CHANNEL, &wav);
    }

    canvas_text(
        300.0, 508.0, 13.0, TEXT_BRIGHT.0, TEXT_BRIGHT.1, TEXT_BRIGHT.2, "SFX Vol",
    );
    let sfx_vol = ui_slider(WIDGET_SFX_VOL, 365.0, 506.0, 100.0, 0.0, 1.5, 0.8);
    audio_channel_set_volume(SFX_CHANNEL, sfx_vol);

    // ── Status ──────────────────────────────────────────────────────
    canvas_line(20.0, 545.0, w - 20.0, 545.0, 50, 45, 70, 1.0);

    let playing = audio_is_playing();
    let pos_ms = audio_position();
    let dur_ms = audio_duration();
    let pos_secs = pos_ms as f32 / 1000.0;
    let dur_secs = dur_ms as f32 / 1000.0;

    let (status_text, color) = if playing {
        ("Playing", GREEN)
    } else if pos_ms > 0 {
        ("Paused", ORANGE)
    } else {
        ("Stopped", RED)
    };

    canvas_text(20.0, 558.0, 14.0, color.0, color.1, color.2, status_text);

    let note = unsafe { LAST_NOTE };
    if !note.is_empty() {
        canvas_text(
            100.0, 558.0, 14.0, TEXT_DIM.0, TEXT_DIM.1, TEXT_DIM.2,
            &format!("  {note}"),
        );
    }

    let time_info = if dur_ms > 0 {
        format!("Position: {pos_secs:.1}s / {dur_secs:.1}s")
    } else {
        format!("Position: {pos_secs:.1}s")
    };
    canvas_text(
        20.0, 580.0, 13.0, TEXT_DIM.0, TEXT_DIM.1, TEXT_DIM.2, &time_info,
    );

    if looping {
        canvas_text(
            250.0, 580.0, 13.0, ORANGE.0, ORANGE.1, ORANGE.2, "LOOP",
        );
    }

    // ── Visualiser bar ──────────────────────────────────────────────
    if playing {
        let t = time_now_ms() as f32 / 200.0;
        let bar_y = 600.0;
        let bar_count = 24;
        let bar_w = (w - 40.0) / bar_count as f32;
        for i in 0..bar_count {
            let phase = t + i as f32 * 0.3;
            let h = (phase.sin() * 0.5 + 0.5) * 25.0 + 3.0;
            let hue_shift = (i as f32 / bar_count as f32 * 255.0) as u8;
            canvas_rect(
                20.0 + i as f32 * bar_w,
                bar_y + 25.0 - h,
                bar_w - 2.0,
                h,
                100 + hue_shift / 3,
                80,
                200,
                200,
            );
        }
    }
}

fn generate_wav(frequency: f32, duration_secs: f32) -> Vec<u8> {
    let sample_rate: u32 = 44100;
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    let data_size = (num_samples * 2) as u32;

    let mut wav = Vec::with_capacity(44 + data_size as usize);

    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());

    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());

    let two_pi = 2.0 * core::f32::consts::PI;
    let attack_samples = (sample_rate as f32 * 0.01) as usize;
    let release_samples = (sample_rate as f32 * 0.05) as usize;
    let release_start = num_samples.saturating_sub(release_samples);

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sample = (t * frequency * two_pi).sin();

        let envelope = if i < attack_samples {
            i as f32 / attack_samples as f32
        } else if i >= release_start {
            let rel_pos = (i - release_start) as f32 / release_samples as f32;
            1.0 - rel_pos
        } else {
            1.0
        };

        let s16 = (sample * envelope * 0.7 * 32767.0) as i16;
        wav.extend_from_slice(&s16.to_le_bytes());
    }

    wav
}

fn generate_chirp() -> Vec<u8> {
    let sample_rate: u32 = 44100;
    let duration_secs: f32 = 0.15;
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    let data_size = (num_samples * 2) as u32;

    let mut wav = Vec::with_capacity(44 + data_size as usize);

    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());

    let two_pi = 2.0 * core::f32::consts::PI;
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let progress = i as f32 / num_samples as f32;
        let freq = 400.0 + progress * 1600.0;
        let envelope = 1.0 - progress;
        let sample = (t * freq * two_pi).sin() * envelope * 0.7;
        let s16 = (sample * 32767.0) as i16;
        wav.extend_from_slice(&s16.to_le_bytes());
    }

    wav
}

fn play_tone(frequency: f32, duration_secs: f32) {
    let wav = generate_wav(frequency, duration_secs);
    audio_play(&wav);
}
