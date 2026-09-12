//! Page-local Find: collect matches from guest canvas text, widgets, the
//! console, and internal history/bookmarks pages.

use crate::capabilities::{ConsoleEntry, DrawCommand, WidgetCommand};

/// Where a match was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindSource {
    Canvas,
    Widget,
    Console,
    History,
    Bookmark,
}

/// One case-insensitive substring match.
#[derive(Clone, Debug, PartialEq)]
pub struct FindHit {
    pub source: FindSource,
    pub text: String,
    /// Guest-space highlight rectangle `(x, y, w, h)` when the match is painted
    /// on the canvas or as a widget.
    pub bounds: Option<(f32, f32, f32, f32)>,
    /// Row index for console / history / bookmark lists.
    pub row: Option<usize>,
}

/// True when `haystack` contains `query` (case-insensitive). Empty queries
/// never match, so opening Find does not highlight the whole page.
pub fn text_matches(haystack: &str, query: &str) -> bool {
    let q = query.trim();
    if q.is_empty() {
        return false;
    }
    haystack.to_lowercase().contains(&q.to_lowercase())
}

/// Approximate glyph box for canvas text. Width is a conservative estimate so
/// the highlight covers the run without needing the text shaper at collect time.
pub fn estimate_text_bounds(
    x: f32,
    y: f32,
    size: f32,
    text: &str,
    align: u8,
) -> (f32, f32, f32, f32) {
    let w = (size * 0.55 * text.chars().count() as f32).max(size * 0.4);
    let h = size * 1.25;
    let left = match align {
        1 => x - w / 2.0,
        2 => x - w,
        _ => x,
    };
    (left, y, w, h)
}

/// Collect matches from the current frame's draw list.
pub fn collect_canvas_hits(cmds: &[DrawCommand], query: &str) -> Vec<FindHit> {
    let mut hits = Vec::new();
    if query.trim().is_empty() {
        return hits;
    }
    for cmd in cmds {
        match cmd {
            DrawCommand::Text {
                x, y, size, text, ..
            } if text_matches(text, query) => {
                hits.push(FindHit {
                    source: FindSource::Canvas,
                    text: text.clone(),
                    bounds: Some(estimate_text_bounds(*x, *y, *size, text, 0)),
                    row: None,
                });
            }
            DrawCommand::TextEx {
                x,
                y,
                size,
                align,
                text,
                ..
            } if text_matches(text, query) => {
                hits.push(FindHit {
                    source: FindSource::Canvas,
                    text: text.clone(),
                    bounds: Some(estimate_text_bounds(*x, *y, *size, text, *align)),
                    row: None,
                });
            }
            _ => {}
        }
    }
    hits
}

/// Collect matches from guest widget labels and visible values.
pub fn collect_widget_hits(
    cmds: &[WidgetCommand],
    values: &std::collections::HashMap<u32, crate::capabilities::WidgetValue>,
    query: &str,
) -> Vec<FindHit> {
    use crate::capabilities::WidgetValue;

    let mut hits = Vec::new();
    if query.trim().is_empty() {
        return hits;
    }
    for cmd in cmds {
        let (text, bounds) = match cmd {
            WidgetCommand::Button {
                x, y, w, h, label, ..
            } => (label.as_str(), Some((*x, *y, *w, *h))),
            WidgetCommand::Checkbox { x, y, label, .. } => {
                (label.as_str(), Some((*x, *y, 220.0, 26.0)))
            }
            WidgetCommand::Switch { x, y, label, .. } => {
                (label.as_str(), Some((*x, *y, 220.0, 24.0)))
            }
            WidgetCommand::TextInput {
                id,
                x,
                y,
                w,
                placeholder,
            } => {
                let live = values.get(id).and_then(|v| match v {
                    WidgetValue::Text(t) => Some(t.as_str()),
                    _ => None,
                });
                let text = live
                    .filter(|t| !t.is_empty())
                    .unwrap_or(placeholder.as_str());
                (text, Some((*x, *y, *w, 36.0)))
            }
            WidgetCommand::Textarea {
                id,
                x,
                y,
                w,
                h,
                placeholder,
            } => {
                let live = values.get(id).and_then(|v| match v {
                    WidgetValue::Text(t) => Some(t.as_str()),
                    _ => None,
                });
                let text = live
                    .filter(|t| !t.is_empty())
                    .unwrap_or(placeholder.as_str());
                (text, Some((*x, *y, *w, *h)))
            }
            WidgetCommand::Card {
                x,
                y,
                w,
                h,
                title,
                description,
            } => {
                let combined = if text_matches(title, query) || text_matches(description, query) {
                    if text_matches(title, query) {
                        title.as_str()
                    } else {
                        description.as_str()
                    }
                } else {
                    continue;
                };
                hits.push(FindHit {
                    source: FindSource::Widget,
                    text: combined.to_string(),
                    bounds: Some((*x, *y, *w, *h)),
                    row: None,
                });
                continue;
            }
            WidgetCommand::Badge { x, y, label, .. } => {
                (label.as_str(), Some((*x, *y, 80.0, 22.0)))
            }
            WidgetCommand::Label {
                x, y, text, size, ..
            } => (
                text.as_str(),
                Some(estimate_text_bounds(*x, *y, *size, text, 0)),
            ),
            WidgetCommand::Slider { .. }
            | WidgetCommand::Separator { .. }
            | WidgetCommand::Progress { .. } => continue,
        };
        if text_matches(text, query) {
            hits.push(FindHit {
                source: FindSource::Widget,
                text: text.to_string(),
                bounds,
                row: None,
            });
        }
    }
    hits
}

/// Collect matches from console log lines.
pub fn collect_console_hits(entries: &[ConsoleEntry], query: &str) -> Vec<FindHit> {
    let mut hits = Vec::new();
    if query.trim().is_empty() {
        return hits;
    }
    for (i, e) in entries.iter().enumerate() {
        if text_matches(&e.message, query) || text_matches(&e.timestamp, query) {
            hits.push(FindHit {
                source: FindSource::Console,
                text: e.message.clone(),
                bounds: None,
                row: Some(i),
            });
        }
    }
    hits
}

/// Collect matches from history or bookmark rows (`title` + `url`).
pub fn collect_list_hits(
    rows: &[(String, String)],
    query: &str,
    source: FindSource,
) -> Vec<FindHit> {
    let mut hits = Vec::new();
    if query.trim().is_empty() {
        return hits;
    }
    for (i, (title, url)) in rows.iter().enumerate() {
        if text_matches(title, query) || text_matches(url, query) {
            let text = if text_matches(title, query) {
                title.clone()
            } else {
                url.clone()
            };
            hits.push(FindHit {
                source,
                text,
                bounds: None,
                row: Some(i),
            });
        }
    }
    hits
}

/// Advance `index` through `len` matches. `forward` wraps around.
pub fn step_index(index: usize, len: usize, forward: bool) -> usize {
    if len == 0 {
        return 0;
    }
    if forward {
        (index + 1) % len
    } else if index == 0 {
        len - 1
    } else {
        index - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_never_matches() {
        assert!(!text_matches("Hello", ""));
        assert!(!text_matches("Hello", "   "));
        assert!(collect_canvas_hits(
            &[DrawCommand::Text {
                x: 0.0,
                y: 0.0,
                size: 14.0,
                r: 0,
                g: 0,
                b: 0,
                a: 255,
                text: "Hello".into(),
            }],
            ""
        )
        .is_empty());
    }

    #[test]
    fn match_is_case_insensitive() {
        assert!(text_matches("Oxide Browser", "oxide"));
        assert!(text_matches("Oxide Browser", "BROWSER"));
        assert!(!text_matches("Oxide Browser", "wasm"));
    }

    #[test]
    fn canvas_collects_text_and_textex() {
        let cmds = vec![
            DrawCommand::Text {
                x: 10.0,
                y: 20.0,
                size: 16.0,
                r: 0,
                g: 0,
                b: 0,
                a: 255,
                text: "Welcome to Oxide".into(),
            },
            DrawCommand::TextEx {
                x: 100.0,
                y: 40.0,
                size: 12.0,
                r: 0,
                g: 0,
                b: 0,
                a: 255,
                family: String::new(),
                weight: 400,
                style: 0,
                align: 1,
                text: "Centered title".into(),
            },
            DrawCommand::Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
        ];
        let oxide = collect_canvas_hits(&cmds, "oxide");
        assert_eq!(oxide.len(), 1);
        assert_eq!(oxide[0].source, FindSource::Canvas);
        assert!(oxide[0].bounds.is_some());
        assert_eq!(collect_canvas_hits(&cmds, "title").len(), 1);
        assert!(collect_canvas_hits(&cmds, "missing").is_empty());
    }

    #[test]
    fn widget_and_console_hits() {
        let widgets = vec![WidgetCommand::Button {
            id: 1,
            x: 0.0,
            y: 0.0,
            w: 80.0,
            h: 24.0,
            label: "Reload".into(),
            variant: crate::capabilities::WidgetVariant::Default,
        }];
        let hits = collect_widget_hits(&widgets, &Default::default(), "re");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, FindSource::Widget);

        let logs = vec![ConsoleEntry {
            timestamp: "12:00:00".into(),
            level: crate::capabilities::ConsoleLevel::Log,
            message: "module started".into(),
        }];
        let ch = collect_console_hits(&logs, "started");
        assert_eq!(ch.len(), 1);
        assert_eq!(ch[0].row, Some(0));
    }

    #[test]
    fn list_hits_search_title_and_url() {
        let rows = vec![
            ("Home".into(), "oxide://home".into()),
            ("Docs".into(), "https://oxide.foundation".into()),
        ];
        assert_eq!(
            collect_list_hits(&rows, "foundation", FindSource::Bookmark).len(),
            1
        );
        assert_eq!(
            collect_list_hits(&rows, "home", FindSource::History).len(),
            1
        );
    }

    #[test]
    fn step_wraps() {
        assert_eq!(step_index(0, 0, true), 0);
        assert_eq!(step_index(2, 3, true), 0);
        assert_eq!(step_index(0, 3, false), 2);
        assert_eq!(step_index(1, 3, false), 0);
    }

    #[test]
    fn centre_align_shifts_bounds_left() {
        let (x, _, w, _) = estimate_text_bounds(100.0, 0.0, 10.0, "abcd", 1);
        assert!(x < 100.0);
        assert!(x + w > 100.0);
    }
}
