//! Magic-byte sniffing and MIME mapping for supported guest audio containers.

/// Unknown or not recognized as a supported Oxide audio format.
pub const AUDIO_FORMAT_UNKNOWN: u32 = 0;
/// WAV / RIFF WAVE.
pub const AUDIO_FORMAT_WAV: u32 = 1;
/// MP3 (MPEG-1/2 Audio Layer III).
pub const AUDIO_FORMAT_MP3: u32 = 2;
/// Ogg (Vorbis, Opus, etc.).
pub const AUDIO_FORMAT_OGG: u32 = 3;
/// FLAC lossless.
pub const AUDIO_FORMAT_FLAC: u32 = 4;

/// `Accept` header sent with [`super::capabilities`] URL fetches so servers can pick a codec/container.
pub const AUDIO_HTTP_ACCEPT: &str = "audio/wav,audio/wave,audio/x-wav;q=0.9,audio/mpeg,audio/mp3;q=0.9,audio/ogg,audio/flac,audio/*;q=0.5,*/*;q=0.1";

fn skip_id3_prefix(data: &[u8]) -> usize {
    if data.len() >= 10 && data[0..3] == *b"ID3" {
        let size = ((data[6] as usize) << 21)
            | ((data[7] as usize) << 14)
            | ((data[8] as usize) << 7)
            | (data[9] as usize);
        10 + size
    } else {
        0
    }
}

fn mp3_sync_at(data: &[u8], offset: usize) -> bool {
    if offset + 2 > data.len() {
        return false;
    }
    let b0 = data[offset];
    let b1 = data[offset + 1];
    (b0 == 0xFF) && ((b1 & 0xE0) == 0xE0)
}

/// Inspect leading bytes (and minimal MP3 frame sync after ID3) to guess the container/codec.
pub fn sniff_audio_format(data: &[u8]) -> u32 {
    if data.len() < 4 {
        return AUDIO_FORMAT_UNKNOWN;
    }
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WAVE" {
        return AUDIO_FORMAT_WAV;
    }
    if &data[0..4] == b"fLaC" {
        return AUDIO_FORMAT_FLAC;
    }
    if &data[0..4] == b"OggS" {
        return AUDIO_FORMAT_OGG;
    }
    let off = skip_id3_prefix(data);
    if off < data.len() && mp3_sync_at(data, off) {
        return AUDIO_FORMAT_MP3;
    }
    if mp3_sync_at(data, 0) {
        return AUDIO_FORMAT_MP3;
    }
    AUDIO_FORMAT_UNKNOWN
}

/// Map a `Content-Type` (or similar) value to an `AUDIO_FORMAT_*` constant, or [`AUDIO_FORMAT_UNKNOWN`].
pub fn mime_to_audio_format(mime: &str) -> u32 {
    let s = mime
        .split(';')
        .next()
        .unwrap_or(mime)
        .trim()
        .to_ascii_lowercase();
    match s.as_str() {
        "audio/wav" | "audio/wave" | "audio/x-wav" => AUDIO_FORMAT_WAV,
        "audio/mpeg" | "audio/mp3" => AUDIO_FORMAT_MP3,
        "audio/ogg" | "application/ogg" => AUDIO_FORMAT_OGG,
        "audio/flac" | "audio/x-flac" => AUDIO_FORMAT_FLAC,
        _ => AUDIO_FORMAT_UNKNOWN,
    }
}

/// MIME types that usually indicate an HTML/JSON error body rather than a media stream.
pub fn is_likely_non_audio_document(mime: &str) -> bool {
    let s = mime
        .split(';')
        .next()
        .unwrap_or(mime)
        .trim()
        .to_ascii_lowercase();
    s.starts_with("text/html")
        || s.starts_with("text/plain")
        || s.starts_with("text/css")
        || s == "application/json"
        || s == "application/javascript"
        || s.starts_with("application/xml")
        || s.starts_with("text/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_wav() {
        let mut b = vec![0u8; 12];
        b[0..4].copy_from_slice(b"RIFF");
        b[8..12].copy_from_slice(b"WAVE");
        assert_eq!(sniff_audio_format(&b), AUDIO_FORMAT_WAV);
    }

    #[test]
    fn sniff_flac() {
        assert_eq!(sniff_audio_format(b"fLaCxxxx"), AUDIO_FORMAT_FLAC);
    }

    #[test]
    fn sniff_ogg() {
        assert_eq!(sniff_audio_format(b"OggSxxxx"), AUDIO_FORMAT_OGG);
    }

    #[test]
    fn sniff_mp3_sync() {
        let b = [0xFF, 0xFB, 0x90, 0x00];
        assert_eq!(sniff_audio_format(&b), AUDIO_FORMAT_MP3);
    }

    #[test]
    fn mime_maps() {
        assert_eq!(mime_to_audio_format("audio/mpeg"), AUDIO_FORMAT_MP3);
        assert_eq!(
            mime_to_audio_format("audio/ogg; codecs=vorbis"),
            AUDIO_FORMAT_OGG
        );
        assert!(is_likely_non_audio_document("text/html; charset=utf-8"));
    }
}
