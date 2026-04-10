//! Minimal SRT and WebVTT cue lists for host-side caption rendering.

#[derive(Clone, Debug)]
pub struct SubtitleCue {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

fn parse_timestamp(s: &str) -> Option<u64> {
    let s = s.trim();
    // 00:00:01,234 or 00:00:01.234
    let (hms, ms_part) = if let Some(i) = s.rfind([',', '.']) {
        (&s[..i], &s[i + 1..])
    } else {
        return None;
    };
    let parts: Vec<&str> = hms.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let h: u64 = parts[0].parse().ok()?;
    let m: u64 = parts[1].parse().ok()?;
    let sec: u64 = parts[2].parse().ok()?;
    let ms: u64 = ms_part.parse().ok()?;
    Some(((h * 60 + m) * 60 + sec) * 1000 + ms)
}

/// Parse SubRip (`.srt`) text into cues.
pub fn parse_srt(data: &str) -> Vec<SubtitleCue> {
    let mut out = Vec::new();
    let blocks: Vec<&str> = data.split("\n\n").collect();
    for block in blocks {
        let lines: Vec<&str> = block.lines().filter(|l| !l.trim().is_empty()).collect();
        if lines.len() < 2 {
            continue;
        }
        let mut i = 0;
        if lines[0].trim().chars().all(|c| c.is_ascii_digit()) {
            i = 1;
        }
        if i >= lines.len() {
            continue;
        }
        let time_line = lines[i];
        let Some(arrow) = time_line.find("-->") else {
            continue;
        };
        let left = time_line[..arrow].trim();
        let right = time_line[arrow + 3..].trim();
        // optional cue settings after end time
        let right = right.split_whitespace().next().unwrap_or(right);
        let Some(start) = parse_timestamp(left) else {
            continue;
        };
        let Some(end) = parse_timestamp(right) else {
            continue;
        };
        let text = lines[i + 1..].join("\n");
        if text.is_empty() {
            continue;
        }
        out.push(SubtitleCue {
            start_ms: start,
            end_ms: end,
            text,
        });
    }
    out
}

/// Parse WebVTT (`.vtt`) into cues (ignores styles and notes).
pub fn parse_vtt(data: &str) -> Vec<SubtitleCue> {
    let mut out = Vec::new();
    let mut lines = data.lines().peekable();
    if let Some(first) = lines.peek() {
        if first.trim().eq_ignore_ascii_case("WEBVTT") {
            lines.next();
        }
    }
    let mut buf: Vec<String> = Vec::new();
    for line in lines {
        let line = line.trim_end();
        if line.is_empty() {
            if !buf.is_empty() {
                if let Some(cue) = parse_vtt_block(&buf) {
                    out.push(cue);
                }
                buf.clear();
            }
            continue;
        }
        buf.push(line.to_string());
    }
    if !buf.is_empty() {
        if let Some(cue) = parse_vtt_block(&buf) {
            out.push(cue);
        }
    }
    out
}

fn parse_vtt_block(lines: &[String]) -> Option<SubtitleCue> {
    if lines.is_empty() {
        return None;
    }
    let mut i = 0;
    let time_line = if lines[0].contains("-->") {
        &lines[0]
    } else if lines.len() >= 2 && lines[1].contains("-->") {
        i = 1;
        &lines[1]
    } else {
        return None;
    };
    let arrow = time_line.find("-->")?;
    let left = time_line[..arrow].trim();
    let right = time_line[arrow + 3..].trim();
    let right = right.split_whitespace().next()?;
    let start = parse_timestamp(left)?;
    let end = parse_timestamp(right)?;
    let text = lines[i + 1..].join("\n");
    if text.is_empty() {
        return None;
    }
    Some(SubtitleCue {
        start_ms: start,
        end_ms: end,
        text,
    })
}

/// Active subtitle text at `t_ms`, if any.
pub fn cue_text_at(cues: &[SubtitleCue], t_ms: u64) -> Option<&str> {
    cues.iter()
        .find(|c| t_ms >= c.start_ms && t_ms <= c.end_ms)
        .map(|c| c.text.as_str())
}
