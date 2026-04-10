//! FFmpeg-backed video decode, playback clock, HLS variant metadata, and subtitle cues.

use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Instant;

use ffmpeg::format::{self, Pixel};
use ffmpeg::media::Type;
use ffmpeg::software::scaling::{context::Context as ScalerContext, flag::Flags as ScaleFlags};
use ffmpeg::util::frame::video::Video;
use ffmpeg::util::mathematics::{rescale, Rescale};
use ffmpeg_next as ffmpeg;
use tempfile::NamedTempFile;
use url::Url;

use crate::subtitle::SubtitleCue;

static FFMPEG_INIT: OnceLock<Result<(), ffmpeg::Error>> = OnceLock::new();

fn ensure_ffmpeg() -> Result<(), ffmpeg::Error> {
    *FFMPEG_INIT.get_or_init(|| {
        let r = ffmpeg::init();
        if r.is_ok() {
            // Avoid spamming stderr with EOF/packet noise during normal decode.
            ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Quiet);
        }
        r
    })
}

fn frame_pts_ms_from_tb(time_base: ffmpeg::Rational, frame: &Video) -> Option<u64> {
    let pts = frame.timestamp().or_else(|| frame.pts())?;
    if pts < 0 {
        return None;
    }
    let ms = pts.rescale(time_base, (1, 1000));
    if ms < 0 {
        None
    } else {
        Some(ms as u64)
    }
}

fn scale_frame_to_rgba(
    scaler: &mut ScalerContext,
    decoded: &Video,
) -> Result<(Vec<u8>, u32, u32), String> {
    let mut rgb = Video::empty();
    scaler.run(decoded, &mut rgb).map_err(|e| e.to_string())?;
    let w = rgb.width();
    let h = rgb.height();
    let stride = rgb.stride(0);
    let mut packed = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h as usize {
        let start = row * stride;
        let end = start + (w as usize * 4);
        packed.extend_from_slice(&rgb.data(0)[start..end]);
    }
    Ok((packed, w, h))
}

/// Decodes one video stream to RGBA via libswscale.
pub struct VideoPlayer {
    input: format::context::Input,
    video_stream_index: usize,
    decoder: ffmpeg::decoder::Video,
    scaler: ScalerContext,
    time_base: ffmpeg::Rational,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    /// Last presented frame PTS (ms), for incremental decode.
    last_pts_ms: Option<u64>,
    /// Cached RGBA for last decoded frame.
    last_rgba: Option<(Vec<u8>, u32, u32, u64)>,
    /// Whether [`ffmpeg::decoder::Opened::send_eof`] was already sent (must not repeat).
    decoder_eof_sent: bool,
}

impl VideoPlayer {
    fn open_input(input: format::context::Input) -> Result<Self, String> {
        ensure_ffmpeg().map_err(|e| e.to_string())?;

        let stream = input
            .streams()
            .best(Type::Video)
            .ok_or_else(|| "no video stream".to_string())?;
        let video_stream_index = stream.index();
        let time_base = stream.time_base();

        let context = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
            .map_err(|e| e.to_string())?;
        let decoder = context.decoder().video().map_err(|e| e.to_string())?;

        let duration_ms = Self::probe_duration_ms(&input, &stream);

        let width = decoder.width();
        let height = decoder.height();

        let scaler = ScalerContext::get(
            decoder.format(),
            width,
            height,
            Pixel::RGBA,
            width,
            height,
            ScaleFlags::BILINEAR,
        )
        .map_err(|e| e.to_string())?;

        Ok(Self {
            input,
            video_stream_index,
            decoder,
            scaler,
            time_base,
            duration_ms,
            width,
            height,
            last_pts_ms: None,
            last_rgba: None,
            decoder_eof_sent: false,
        })
    }

    pub fn open_path(path: &Path) -> Result<Self, String> {
        let input = format::input(path).map_err(|e| e.to_string())?;
        Self::open_input(input)
    }

    pub fn open_url(url: &str) -> Result<Self, String> {
        let input = format::input(url).map_err(|e| e.to_string())?;
        Self::open_input(input)
    }

    fn probe_duration_ms(
        input: &format::context::Input,
        stream: &ffmpeg::format::stream::Stream,
    ) -> u64 {
        let d = input.duration();
        if d > 0 {
            let ms = d.rescale(rescale::TIME_BASE, (1, 1000));
            if ms > 0 {
                return ms as u64;
            }
        }
        let sd = stream.duration();
        if sd > 0 {
            let ms = sd.rescale(stream.time_base(), (1, 1000));
            return ms.max(0) as u64;
        }
        0
    }

    fn seek_to_ms(&mut self, target_ms: u64) -> Result<(), String> {
        let ts = (target_ms as i64).rescale((1, 1000), rescale::TIME_BASE);
        self.input.seek(ts, ts..ts).map_err(|e| e.to_string())?;
        self.decoder.flush();
        self.last_pts_ms = None;
        self.last_rgba = None;
        self.decoder_eof_sent = false;
        Ok(())
    }

    /// Decode a frame appropriate for `target_ms` (last frame with PTS ≤ target, or first at/after seek).
    pub fn decode_frame_at(&mut self, target_ms: u64) -> Result<(Vec<u8>, u32, u32), String> {
        let target_ms = target_ms.min(self.duration_ms.saturating_add(500));

        let need_seek = match self.last_pts_ms {
            None => true,
            Some(last) => target_ms < last || target_ms.saturating_sub(last) > 2_500,
        };

        if need_seek {
            self.seek_to_ms(target_ms)?;
        }

        let mut best_before: Option<(Vec<u8>, u32, u32, u64)> = None;
        let mut best_after: Option<(Vec<u8>, u32, u32, u64)> = None;

        {
            let VideoPlayer {
                ref mut input,
                video_stream_index,
                ref mut decoder,
                ref mut scaler,
                time_base,
                ..
            } = self;

            'outer: for (stream, packet) in input.packets() {
                if stream.index() != *video_stream_index {
                    continue;
                }
                decoder.send_packet(&packet).map_err(|e| e.to_string())?;
                let mut decoded = Video::empty();
                while decoder.receive_frame(&mut decoded).is_ok() {
                    let pts_ms = frame_pts_ms_from_tb(*time_base, &decoded).unwrap_or(0);
                    let (buf, w, h) = scale_frame_to_rgba(scaler, &decoded)?;
                    if pts_ms >= target_ms {
                        best_after = Some((buf, w, h, pts_ms));
                        break 'outer;
                    }
                    best_before = Some((buf, w, h, pts_ms));
                }
            }
        }

        // Pull frames that are already buffered in the decoder (no new packets yet).
        if best_after.is_none() {
            let VideoPlayer {
                ref mut decoder,
                ref mut scaler,
                time_base,
                ..
            } = self;
            let mut decoded = Video::empty();
            while decoder.receive_frame(&mut decoded).is_ok() {
                let pts_ms = frame_pts_ms_from_tb(*time_base, &decoded).unwrap_or(0);
                let (buf, w, h) = scale_frame_to_rgba(scaler, &decoded)?;
                if pts_ms >= target_ms {
                    best_after = Some((buf, w, h, pts_ms));
                    break;
                }
                best_before = Some((buf, w, h, pts_ms));
            }
        }

        if let Some((buf, w, h, pts)) = best_after {
            self.last_pts_ms = Some(pts);
            self.last_rgba = Some((buf.clone(), w, h, pts));
            return Ok((buf, w, h));
        }

        if let Some((buf, w, h, pts)) = best_before {
            self.last_pts_ms = Some(pts);
            self.last_rgba = Some((buf.clone(), w, h, pts));
            return Ok((buf, w, h));
        }

        // Demuxer exhausted: flush the decoder exactly once, then drain remaining frames.
        if !self.decoder_eof_sent {
            let _ = self.decoder.send_eof();
            self.decoder_eof_sent = true;
            let VideoPlayer {
                ref mut decoder,
                ref mut scaler,
                time_base,
                ..
            } = self;
            let mut decoded = Video::empty();
            while decoder.receive_frame(&mut decoded).is_ok() {
                let pts_ms = frame_pts_ms_from_tb(*time_base, &decoded).unwrap_or(0);
                let (buf, w, h) = scale_frame_to_rgba(scaler, &decoded)?;
                if pts_ms >= target_ms {
                    best_after = Some((buf, w, h, pts_ms));
                    break;
                }
                best_before = Some((buf, w, h, pts_ms));
            }
        }

        if let Some((buf, w, h, pts)) = best_after {
            self.last_pts_ms = Some(pts);
            self.last_rgba = Some((buf.clone(), w, h, pts));
            return Ok((buf, w, h));
        }

        if let Some((buf, w, h, pts)) = best_before {
            self.last_pts_ms = Some(pts);
            self.last_rgba = Some((buf.clone(), w, h, pts));
            return Ok((buf, w, h));
        }

        if let Some((buf, w, h, _pts)) = self.last_rgba.clone() {
            return Ok((buf, w, h));
        }

        Err("no video frame decoded".into())
    }
}

/// FFmpeg decoder state is synchronized through [`std::sync::Mutex`] on the host; not shared across threads concurrently.
unsafe impl Send for VideoPlayer {}

/// Global playback + optional [`VideoPlayer`] and subtitle list.
pub struct VideoPlaybackState {
    pub player: Option<VideoPlayer>,
    pub playing: bool,
    play_start: Option<Instant>,
    pub base_position_ms: u64,
    pub volume: f32,
    pub looping: bool,
    pub pip: bool,
    pub subtitles: Vec<SubtitleCue>,
    pub last_url_content_type: String,
    pub hls_variants: Vec<String>,
    pub hls_base_url: String,
    temp_file: Option<NamedTempFile>,
}

impl Default for VideoPlaybackState {
    fn default() -> Self {
        Self {
            player: None,
            playing: false,
            play_start: None,
            base_position_ms: 0,
            volume: 1.0,
            looping: false,
            pip: false,
            subtitles: Vec::new(),
            last_url_content_type: String::new(),
            hls_variants: Vec::new(),
            hls_base_url: String::new(),
            temp_file: None,
        }
    }
}

impl VideoPlaybackState {
    pub fn duration_ms(&self) -> u64 {
        self.player.as_ref().map(|p| p.duration_ms).unwrap_or(0)
    }

    pub fn current_position_ms(&self) -> u64 {
        let dur = self.duration_ms();
        let pos = if self.playing {
            let start = self.play_start.expect("play_start when playing");
            self.base_position_ms + start.elapsed().as_millis() as u64
        } else {
            self.base_position_ms
        };
        if self.looping && dur > 0 {
            pos % dur
        } else if dur > 0 {
            pos.min(dur)
        } else {
            pos
        }
    }

    pub fn play(&mut self) {
        self.play_start = Some(Instant::now());
        self.playing = true;
    }

    pub fn pause(&mut self) {
        if self.playing {
            self.base_position_ms = self.current_position_ms();
            self.playing = false;
            self.play_start = None;
        }
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.play_start = None;
        self.base_position_ms = 0;
        self.player = None;
        self.temp_file = None;
        self.hls_variants.clear();
        self.hls_base_url.clear();
    }

    /// Reset clock only (after swapping the underlying stream, e.g. HLS variant).
    pub fn reset_playback_clock(&mut self) {
        self.base_position_ms = 0;
        self.playing = false;
        self.play_start = None;
    }

    pub fn seek(&mut self, position_ms: u64) {
        let dur = self.duration_ms();
        let pos = if dur > 0 {
            position_ms.min(dur)
        } else {
            position_ms
        };
        self.base_position_ms = pos;
        if self.playing {
            self.play_start = Some(Instant::now());
        }
    }

    pub fn open_path(&mut self, path: &Path) -> Result<(), String> {
        self.stop();
        let p = VideoPlayer::open_path(path)?;
        self.player = Some(p);
        Ok(())
    }

    pub fn open_bytes(&mut self, data: &[u8], format_hint: u32) -> Result<(), String> {
        self.stop();
        let ext = crate::video_format::suffix_for_format(format_hint);
        let mut tmp = tempfile::Builder::new()
            .suffix(ext)
            .tempfile()
            .map_err(|e| e.to_string())?;
        tmp.write_all(data).map_err(|e| e.to_string())?;
        tmp.flush().map_err(|e| e.to_string())?;
        let path = tmp.path().to_owned();
        self.temp_file = Some(tmp);
        self.open_path(&path)
    }
}

/// Parse `#EXT-X-STREAM-INF` master playlist variant URIs (best-effort).
pub fn parse_hls_master_variants(body: &str) -> Vec<String> {
    let lines: Vec<&str> = body.lines().collect();
    let mut out = Vec::new();
    for i in 0..lines.len() {
        if lines[i].starts_with("#EXT-X-STREAM-INF") && i + 1 < lines.len() {
            let u = lines[i + 1].trim();
            if !u.starts_with('#') && !u.is_empty() {
                out.push(u.to_string());
            }
        }
    }
    out
}

pub fn resolve_against_base(base: &str, relative: &str) -> Option<String> {
    let b = Url::parse(base).ok()?;
    b.join(relative).ok().map(|u| u.to_string())
}
