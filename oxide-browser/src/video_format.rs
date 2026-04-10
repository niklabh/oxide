//! Magic-byte sniffing and MIME mapping for guest video containers.

/// Unknown or not recognized as a supported Oxide video format.
pub const VIDEO_FORMAT_UNKNOWN: u32 = 0;
/// MP4 / ISO BMFF (`ftyp`).
pub const VIDEO_FORMAT_MP4: u32 = 1;
/// WebM / Matroska (EBML).
pub const VIDEO_FORMAT_WEBM: u32 = 2;
/// AV1 bitstream (often in MP4 or WebM; hint only).
pub const VIDEO_FORMAT_AV1: u32 = 3;

/// `Accept` header for [`super::capabilities`] URL fetches (progressive + adaptive).
pub const VIDEO_HTTP_ACCEPT: &str = "video/mp4,video/webm,video/quicktime,video/x-matroska,application/vnd.apple.mpegurl,application/x-mpegURL,audio/mpegurl,video/*;q=0.9,*/*;q=0.1";

/// File suffix for temp files when saving bytes (`.mp4`, `.webm`, …).
pub fn suffix_for_format(code: u32) -> &'static str {
    match code {
        VIDEO_FORMAT_WEBM => ".webm",
        VIDEO_FORMAT_AV1 => ".mp4",
        VIDEO_FORMAT_MP4 => ".mp4",
        _ => ".bin",
    }
}

/// Inspect leading bytes to guess container (does not validate full file).
pub fn sniff_video_format(data: &[u8]) -> u32 {
    if data.len() < 12 {
        return VIDEO_FORMAT_UNKNOWN;
    }
    // ISO BMFF: size + "ftyp" at offset 4, or "ftyp" at 0 in some files
    if data.len() >= 8 && &data[4..8] == b"ftyp" {
        return VIDEO_FORMAT_MP4;
    }
    if data.len() >= 12
        && data[0..4] == [0x00, 0x00, 0x00, 0x1c]
        && &data[4..8] == b"ftyp"
    {
        return VIDEO_FORMAT_MP4;
    }
    // EBML / WebM / Matroska
    if data.len() >= 4 && data[0] == 0x1a && data[1] == 0x45 && data[2] == 0xdf && data[3] == 0xa3 {
        return VIDEO_FORMAT_WEBM;
    }
    VIDEO_FORMAT_UNKNOWN
}

/// Map `Content-Type` to [`VIDEO_FORMAT_*`].
pub fn mime_to_video_format(mime: &str) -> u32 {
    let s = mime
        .split(';')
        .next()
        .unwrap_or(mime)
        .trim()
        .to_ascii_lowercase();
    match s.as_str() {
        "video/mp4" | "video/quicktime" | "application/mp4" => VIDEO_FORMAT_MP4,
        "video/webm" | "video/x-matroska" => VIDEO_FORMAT_WEBM,
        "video/av1" => VIDEO_FORMAT_AV1,
        "application/vnd.apple.mpegurl" | "application/x-mpegURL" | "audio/mpegurl" => {
            // HLS manifest
            VIDEO_FORMAT_UNKNOWN
        }
        _ => VIDEO_FORMAT_UNKNOWN,
    }
}

/// MIME types that usually indicate an HTML/JSON error body rather than media.
pub fn is_likely_non_video_document(mime: &str) -> bool {
    let s = mime
        .split(';')
        .next()
        .unwrap_or(mime)
        .trim()
        .to_ascii_lowercase();
    s.starts_with("text/html")
        || s.starts_with("text/plain")
        || s == "application/json"
        || s.starts_with("text/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_mp4_ftyp() {
        let mut b = vec![0u8; 12];
        b[4..8].copy_from_slice(b"ftyp");
        assert_eq!(sniff_video_format(&b), VIDEO_FORMAT_MP4);
    }

    #[test]
    fn sniff_webm_ebml() {
        let b = [
            0x1a, 0x45, 0xdf, 0xa3, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        ];
        assert_eq!(sniff_video_format(&b), VIDEO_FORMAT_WEBM);
    }
}
