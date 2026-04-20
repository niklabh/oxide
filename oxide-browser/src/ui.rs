//! Desktop shell for Oxide using [GPUI](https://www.gpui.rs/) (Zed’s GPU-accelerated UI framework).
//!
//! Guest canvas commands are painted with [`Window::paint_quad`], [`Window::paint_path`],
//! [`Window::paint_image`], and GPU text shaping — bitmaps (including video frames) are uploaded as
//! [`RenderImage`] textures and composited on the GPU.
//!
//! ## Public API
//!
//! - [`run_browser`] — Start the GPUI [`Application`] and open the main browser window; pass
//!   [`HostState`] and page status from [`crate::runtime::BrowserHost`].
//! - [`OxideBrowserView`] — Root view: tabs, toolbar, canvas [`canvas`] element, console, and bookmarks.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use gpui::prelude::*;
use gpui::{
    canvas, div, font, img, point, px, size, Application, Bounds, ClickEvent, FocusHandle,
    ImageSource, InteractiveElement, KeyDownEvent, KeyUpEvent, Keystroke, MouseButton,
    MouseDownEvent, MouseUpEvent, PathBuilder, Pixels, Point, Render, RenderImage, Rgba,
    ScrollDelta, ScrollWheelEvent, SharedString, TextRun, TitlebarOptions, Window, WindowBounds,
    WindowKind, WindowOptions,
};
use image::Frame;
use smallvec::smallvec;

use crate::bookmarks::BookmarkStore;
use crate::capabilities::{
    ConsoleLevel, DrawCommand, GradientStop, HostState, WidgetCommand, WidgetValue,
};
use crate::download::{format_bytes, DownloadManager, DownloadState};
use crate::engine::ModuleLoader;
use crate::history::HistoryStore;
use crate::navigation::HistoryEntry;
use crate::runtime::{LiveModule, PageStatus};

enum RunRequest {
    FetchAndRun { url: String },
    LoadLocal(Vec<u8>),
}

struct RunResult {
    error: Option<String>,
    live_module: Option<LiveModule>,
}

// SAFETY: `LiveModule` contains a wasmtime `Store<HostState>` whose fields are
// behind `Arc<Mutex<…>>`, making them safe to send across threads. The `error`
// field is a plain `Option<String>`.
unsafe impl Send for RunResult {}

#[derive(Clone, PartialEq)]
enum InternalPage {
    History,
    Bookmarks,
    About,
}

fn try_internal_page(url: &str) -> Option<InternalPage> {
    match url {
        "oxide://history" => Some(InternalPage::History),
        "oxide://bookmarks" => Some(InternalPage::Bookmarks),
        "oxide://about" => Some(InternalPage::About),
        _ => None,
    }
}

fn format_friendly_timestamp(timestamp_ms: u64) -> String {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let diff_secs = now_ms.saturating_sub(timestamp_ms) / 1000;
    if diff_secs < 60 {
        "Just now".to_string()
    } else if diff_secs < 3600 {
        let m = diff_secs / 60;
        if m == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{m} minutes ago")
        }
    } else if diff_secs < 86400 {
        let h = diff_secs / 3600;
        if h == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{h} hours ago")
        }
    } else {
        let d = diff_secs / 86400;
        if d == 1 {
            "Yesterday".to_string()
        } else {
            format!("{d} days ago")
        }
    }
}

struct TabState {
    id: u64,
    url_input: String,
    host_state: HostState,
    status: Arc<Mutex<PageStatus>>,
    show_console: bool,
    run_tx: std::sync::mpsc::Sender<RunRequest>,
    run_rx: Arc<Mutex<std::sync::mpsc::Receiver<RunResult>>>,
    /// GPU texture cache for decoded canvas images (video frames use the same path).
    image_textures: HashMap<usize, Arc<RenderImage>>,
    pip_texture: Option<Arc<RenderImage>>,
    pip_last_serial: u64,
    canvas_generation: u64,
    pending_history_url: Option<String>,
    hovered_link_url: Option<String>,
    live_module: Option<LiveModule>,
    last_frame: Instant,
    keys_held: HashSet<u32>,
    /// Guest `TextInput` widget id with keyboard focus, if any.
    text_input_focus: Option<u32>,
    /// Cursor byte offset in `url_input`.
    url_cursor: usize,
    /// Selection anchor byte offset; when != `url_cursor`, the range between them is selected.
    url_sel_start: usize,
    /// True while the mouse button is held to drag-select in the URL bar.
    url_selecting: bool,
    /// Bounds of the URL text canvas element, for mouse hit-testing.
    url_text_bounds: Arc<Mutex<Bounds<Pixels>>>,
    internal_page: Option<InternalPage>,
}

impl TabState {
    fn new(id: u64, host_state: HostState, status: Arc<Mutex<PageStatus>>) -> Self {
        let (req_tx, req_rx) = std::sync::mpsc::channel::<RunRequest>();
        let (res_tx, res_rx) = std::sync::mpsc::channel::<RunResult>();

        let hs = host_state.clone();
        let st = status.clone();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            while let Ok(request) = req_rx.recv() {
                let mut host = crate::runtime::BrowserHost::recreate(hs.clone(), st.clone());
                let result = match request {
                    RunRequest::FetchAndRun { url } => rt.block_on(host.fetch_and_run(&url)),
                    RunRequest::LoadLocal(bytes) => host.run_bytes(&bytes),
                };
                let (error, live_module) = match result {
                    Ok(live) => (None, live),
                    Err(e) => (Some(e.to_string()), None),
                };
                let _ = res_tx.send(RunResult { error, live_module });
            }
        });

        Self {
            id,
            url_input: String::from("https://"),
            host_state,
            status,
            show_console: true,
            run_tx: req_tx,
            run_rx: Arc::new(Mutex::new(res_rx)),
            image_textures: HashMap::new(),
            pip_texture: None,
            pip_last_serial: 0,
            canvas_generation: 0,
            pending_history_url: None,
            hovered_link_url: None,
            live_module: None,
            last_frame: Instant::now(),
            keys_held: HashSet::new(),
            text_input_focus: None,
            url_cursor: 8, // after "https://"
            url_sel_start: 8,
            url_selecting: false,
            url_text_bounds: Arc::new(Mutex::new(Bounds::default())),
            internal_page: None,
        }
    }

    fn display_title(&self) -> String {
        let status = self.status.lock().unwrap().clone();
        match status {
            PageStatus::Idle => "New Tab".to_string(),
            PageStatus::Loading(_) => "Loading\u{2026}".to_string(),
            PageStatus::Running(ref url) => url_to_title(url),
            PageStatus::Error(_) => "Error".to_string(),
        }
    }

    fn navigate(&mut self, dm: &DownloadManager) {
        let url = self.url_input.trim().to_string();
        if url.is_empty() {
            return;
        }
        if let Some(page) = try_internal_page(&url) {
            self.internal_page = Some(page);
            self.live_module = None;
            *self.status.lock().unwrap() = PageStatus::Running(url.clone());
            let mut nav = self.host_state.navigation.lock().unwrap();
            nav.push(HistoryEntry::new(&url));
            return;
        }
        if is_downloadable_url(&url) {
            dm.start_download(url);
            return;
        }
        self.internal_page = None;
        self.pending_history_url = Some(url.clone());
        let _ = self.run_tx.send(RunRequest::FetchAndRun { url });
    }

    fn navigate_to(&mut self, url: String, push_history: bool, dm: &DownloadManager) {
        self.url_input = url.clone();
        let len = self.url_input.len();
        self.url_cursor = len;
        self.url_sel_start = len;
        if let Some(page) = try_internal_page(&url) {
            self.internal_page = Some(page);
            self.live_module = None;
            *self.status.lock().unwrap() = PageStatus::Running(url.clone());
            if push_history {
                let mut nav = self.host_state.navigation.lock().unwrap();
                nav.push(HistoryEntry::new(&url));
            }
            return;
        }
        if is_downloadable_url(&url) {
            dm.start_download(url);
            return;
        }
        self.internal_page = None;
        if push_history {
            self.pending_history_url = Some(url.clone());
        }
        let _ = self.run_tx.send(RunRequest::FetchAndRun { url });
    }

    fn go_back(&mut self) {
        let entry = {
            let mut nav = self.host_state.navigation.lock().unwrap();
            nav.go_back().cloned()
        };
        if let Some(entry) = entry {
            self.url_input = entry.url.clone();
            self.url_clamp_cursor();
            *self.host_state.current_url.lock().unwrap() = entry.url.clone();
            if let Some(page) = try_internal_page(&entry.url) {
                self.internal_page = Some(page);
                self.live_module = None;
                *self.status.lock().unwrap() = PageStatus::Running(entry.url);
            } else {
                self.internal_page = None;
                let _ = self.run_tx.send(RunRequest::FetchAndRun { url: entry.url });
            }
        }
    }

    fn go_forward(&mut self) {
        let entry = {
            let mut nav = self.host_state.navigation.lock().unwrap();
            nav.go_forward().cloned()
        };
        if let Some(entry) = entry {
            self.url_input = entry.url.clone();
            self.url_clamp_cursor();
            *self.host_state.current_url.lock().unwrap() = entry.url.clone();
            if let Some(page) = try_internal_page(&entry.url) {
                self.internal_page = Some(page);
                self.live_module = None;
                *self.status.lock().unwrap() = PageStatus::Running(entry.url);
            } else {
                self.internal_page = None;
                let _ = self.run_tx.send(RunRequest::FetchAndRun { url: entry.url });
            }
        }
    }

    fn drain_results(&mut self) {
        if let Ok(rx) = self.run_rx.lock() {
            while let Ok(result) = rx.try_recv() {
                if let Some(err) = result.error {
                    *self.status.lock().unwrap() = PageStatus::Error(err);
                    self.pending_history_url = None;
                    self.live_module = None;
                } else {
                    if let Some(url) = self.pending_history_url.take() {
                        let mut nav = self.host_state.navigation.lock().unwrap();
                        nav.push(HistoryEntry::new(&url));
                        drop(nav);
                        if let Some(store) = self.host_state.history_store.lock().unwrap().as_ref()
                        {
                            let title = url_to_title(&url);
                            let _ = store.record(&url, &title);
                        }
                    }
                    self.host_state.widget_states.lock().unwrap().clear();
                    self.host_state.widget_clicked.lock().unwrap().clear();
                    self.host_state.widget_commands.lock().unwrap().clear();
                    self.live_module = result.live_module;
                    self.last_frame = Instant::now();
                }
            }
        }
    }

    fn handle_pending_navigation(&mut self, dm: &DownloadManager) {
        let pending = self.host_state.pending_navigation.lock().unwrap().take();
        if let Some(url) = pending {
            self.navigate_to(url, true, dm);
        }
    }

    fn sync_url_bar(&mut self) {
        let cur = self.host_state.current_url.lock().unwrap().clone();
        if !cur.is_empty() && cur != self.url_input {
            let status = self.status.lock().unwrap().clone();
            if matches!(status, PageStatus::Running(_)) {
                self.url_input = cur;
                self.url_clamp_cursor();
            }
        }
    }

    fn url_clamp_cursor(&mut self) {
        let len = self.url_input.len();
        self.url_cursor = self.url_cursor.min(len);
        self.url_sel_start = self.url_sel_start.min(len);
    }

    fn url_has_selection(&self) -> bool {
        self.url_cursor != self.url_sel_start
    }

    fn url_sel_range(&self) -> std::ops::Range<usize> {
        let lo = self.url_cursor.min(self.url_sel_start);
        let hi = self.url_cursor.max(self.url_sel_start);
        lo..hi
    }

    fn url_prev_boundary(&self) -> usize {
        let text = &self.url_input;
        if self.url_cursor == 0 {
            return 0;
        }
        let mut i = self.url_cursor - 1;
        while i > 0 && !text.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    fn url_next_boundary(&self) -> usize {
        let text = &self.url_input;
        if self.url_cursor >= text.len() {
            return text.len();
        }
        let mut i = self.url_cursor + 1;
        while i < text.len() && !text.is_char_boundary(i) {
            i += 1;
        }
        i
    }

    fn url_move_to(&mut self, offset: usize) {
        let offset = offset.min(self.url_input.len());
        self.url_cursor = offset;
        self.url_sel_start = offset;
    }

    fn url_select_to(&mut self, offset: usize) {
        self.url_cursor = offset.min(self.url_input.len());
    }

    fn url_select_all(&mut self) {
        self.url_sel_start = 0;
        self.url_cursor = self.url_input.len();
    }

    fn url_delete_selection(&mut self) {
        if !self.url_has_selection() {
            return;
        }
        let range = self.url_sel_range();
        self.url_input.replace_range(range.clone(), "");
        self.url_cursor = range.start;
        self.url_sel_start = range.start;
    }

    fn url_insert_at_cursor(&mut self, text: &str) {
        if self.url_has_selection() {
            self.url_delete_selection();
        }
        self.url_input.insert_str(self.url_cursor, text);
        self.url_cursor += text.len();
        self.url_sel_start = self.url_cursor;
    }

    fn url_backspace(&mut self) {
        if self.url_has_selection() {
            self.url_delete_selection();
        } else if self.url_cursor > 0 {
            let prev = self.url_prev_boundary();
            self.url_input.replace_range(prev..self.url_cursor, "");
            self.url_cursor = prev;
            self.url_sel_start = prev;
        }
    }

    fn url_delete_forward(&mut self) {
        if self.url_has_selection() {
            self.url_delete_selection();
        } else if self.url_cursor < self.url_input.len() {
            let next = self.url_next_boundary();
            self.url_input.replace_range(self.url_cursor..next, "");
        }
    }

    fn url_selected_text(&self) -> String {
        if self.url_has_selection() {
            self.url_input[self.url_sel_range()].to_string()
        } else {
            String::new()
        }
    }

    fn sync_keys_held_to_input(&self) {
        let mut input = self.host_state.input_state.lock().unwrap();
        input.keys_down.clear();
        input.keys_down.extend(self.keys_held.iter().copied());
    }

    fn tick_frame(&mut self) {
        if self.live_module.is_none() {
            return;
        }

        let now = Instant::now();
        let dt = now - self.last_frame;
        self.last_frame = now;
        let dt_ms = dt.as_millis().min(100) as u32;

        self.host_state.widget_commands.lock().unwrap().clear();

        if let Some(ref mut live) = self.live_module {
            match live.tick(dt_ms) {
                Ok(()) => {}
                Err(e) => {
                    let msg = if e.to_string().contains("fuel") {
                        "on_frame halted: fuel limit exceeded".to_string()
                    } else {
                        format!("on_frame error: {e}")
                    };
                    crate::capabilities::console_log(
                        &self.host_state.console,
                        crate::capabilities::ConsoleLevel::Error,
                        msg.clone(),
                    );
                    *self.status.lock().unwrap() = PageStatus::Error(msg);
                    self.live_module = None;
                }
            }
        }

        self.host_state.widget_clicked.lock().unwrap().clear();
    }

    fn post_tick_clear_input(&mut self) {
        let mut input = self.host_state.input_state.lock().unwrap();
        input.keys_pressed.clear();
        input.mouse_buttons_clicked = [false; 3];
        input.scroll_x = 0.0;
        input.scroll_y = 0.0;
    }

    fn update_texture_cache(&mut self, _window: &mut Window) {
        let tab_id = self.id;
        let canvas = self.host_state.canvas.lock().unwrap();
        if canvas.generation != self.canvas_generation {
            self.image_textures.clear();
            self.canvas_generation = canvas.generation;
        }
        for (i, decoded) in canvas.images.iter().enumerate() {
            self.image_textures.entry(i).or_insert_with(|| {
                decoded_to_render_image(decoded, format!("oxide_img_{i}_tab{tab_id}"))
            });
        }
    }

    fn refresh_pip_texture(&mut self, _window: &mut Window) {
        let pip = self.host_state.video.lock().unwrap().pip;
        if !pip {
            self.pip_texture = None;
            self.pip_last_serial = 0;
            return;
        }

        let serial = *self.host_state.video_pip_serial.lock().unwrap();
        if serial != self.pip_last_serial {
            self.pip_last_serial = serial;
            self.pip_texture = None;
            let frame = self.host_state.video_pip_frame.lock().unwrap().clone();
            if let Some(decoded) = frame {
                self.pip_texture = Some(decoded_to_render_image(
                    &decoded,
                    format!("oxide_pip_{}_{}", self.id, serial),
                ));
            }
        }
    }
}

/// Decode RGBA guest bytes into a GPU [`RenderImage`] (BGRA upload for the renderer).
fn decoded_to_render_image(
    decoded: &crate::capabilities::DecodedImage,
    _debug_label: String,
) -> Arc<RenderImage> {
    let mut buf = image::RgbaImage::from_raw(decoded.width, decoded.height, decoded.pixels.clone())
        .expect("decoded image dimensions");
    for pixel in buf.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let frame = Frame::new(buf);
    Arc::new(RenderImage::new(smallvec![frame]))
}

fn rgba8(r: u8, g: u8, b: u8, a: u8) -> gpui::Hsla {
    gpui::Hsla::from(Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: a as f32 / 255.0,
    })
}

fn circle_polygon(cx: f32, cy: f32, radius: f32) -> Vec<Point<Pixels>> {
    let n = 24;
    (0..n)
        .map(|i| {
            let t = i as f32 / n as f32 * std::f32::consts::TAU;
            point(px(cx + radius * t.cos()), px(cy + radius * t.sin()))
        })
        .collect()
}

/// Saved canvas state for the transform/clip/opacity stack.
#[derive(Clone)]
struct CanvasPaintState {
    offset_x: f32,
    offset_y: f32,
    clip: Option<Bounds<Pixels>>,
    opacity: f32,
}

fn paint_draw_commands(
    window: &mut Window,
    cx: &mut gpui::App,
    bounds: Bounds<Pixels>,
    cmds: &[DrawCommand],
    textures: &HashMap<usize, Arc<RenderImage>>,
) {
    let rect = bounds;
    let origin_x = f32::from(rect.origin.x);
    let origin_y = f32::from(rect.origin.y);

    let mut state_stack: Vec<CanvasPaintState> = Vec::new();
    let mut off_x = origin_x;
    let mut off_y = origin_y;
    let mut clip: Option<Bounds<Pixels>> = None;
    let mut opacity: f32 = 1.0;

    for cmd in cmds {
        match cmd {
            DrawCommand::Save => {
                state_stack.push(CanvasPaintState {
                    offset_x: off_x,
                    offset_y: off_y,
                    clip,
                    opacity,
                });
            }
            DrawCommand::Restore => {
                if let Some(prev) = state_stack.pop() {
                    off_x = prev.offset_x;
                    off_y = prev.offset_y;
                    clip = prev.clip;
                    opacity = prev.opacity;
                }
            }
            DrawCommand::Transform {
                a: _,
                b: _,
                c: _,
                d: _,
                tx,
                ty,
            } => {
                off_x += *tx;
                off_y += *ty;
            }
            DrawCommand::Clip { x, y, w, h } => {
                let new_clip = Bounds::from_corners(
                    point(px(off_x + *x), px(off_y + *y)),
                    point(px(off_x + *x + *w), px(off_y + *y + *h)),
                );
                clip = Some(match clip {
                    Some(existing) => intersect_bounds(existing, new_clip),
                    None => new_clip,
                });
            }
            DrawCommand::Opacity { alpha } => {
                opacity *= *alpha;
            }

            DrawCommand::Clear { r, g, b, a } => {
                let ca = apply_opacity(*a, opacity);
                window.paint_quad(gpui::fill(rect, rgba8(*r, *g, *b, ca)));
            }
            DrawCommand::Rect {
                x,
                y,
                w,
                h,
                r,
                g,
                b,
                a,
            } => {
                let min = point(px(off_x + *x), px(off_y + *y));
                let cmd_bounds = Bounds::from_corners(min, min + point(px(*w), px(*h)));
                if !clipped_out(clip, cmd_bounds) {
                    let ca = apply_opacity(*a, opacity);
                    window.paint_quad(gpui::fill(cmd_bounds, rgba8(*r, *g, *b, ca)));
                }
            }
            DrawCommand::Circle {
                cx,
                cy,
                radius,
                r,
                g,
                b,
                a,
            } => {
                let pts = circle_polygon(off_x + *cx, off_y + *cy, *radius);
                let mut pb = PathBuilder::fill();
                pb.add_polygon(&pts, true);
                if let Ok(path) = pb.build() {
                    let ca = apply_opacity(*a, opacity);
                    window.paint_path(path, rgba8(*r, *g, *b, ca));
                }
            }
            DrawCommand::Text {
                x,
                y,
                size,
                r,
                g,
                b,
                a,
                text,
            } => {
                let origin = point(px(off_x + *x), px(off_y + *y));
                let text_owned = text.clone();
                let ca = apply_opacity(*a, opacity);
                let run = TextRun {
                    len: text_owned.len(),
                    font: font(".SystemUIFont"),
                    color: rgba8(*r, *g, *b, ca),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let line = window.text_system().shape_line(
                    SharedString::from(text_owned),
                    px(*size),
                    &[run],
                    None,
                );
                let _ = line.paint(origin, px(*size * 1.2), window, cx);
            }
            DrawCommand::Line {
                x1,
                y1,
                x2,
                y2,
                r,
                g,
                b,
                a,
                thickness,
            } => {
                let p1 = point(px(off_x + *x1), px(off_y + *y1));
                let p2 = point(px(off_x + *x2), px(off_y + *y2));
                let mut pb = PathBuilder::stroke(px(*thickness));
                pb.move_to(p1);
                pb.line_to(p2);
                if let Ok(path) = pb.build() {
                    let ca = apply_opacity(*a, opacity);
                    window.paint_path(path, rgba8(*r, *g, *b, ca));
                }
            }
            DrawCommand::Image {
                x,
                y,
                w,
                h,
                image_id,
            } => {
                if let Some(tex) = textures.get(image_id) {
                    let min = point(px(off_x + *x), px(off_y + *y));
                    let img_bounds = Bounds::from_corners(min, min + point(px(*w), px(*h)));
                    let _ = window.paint_image(img_bounds, (0.).into(), tex.clone(), 0, false);
                }
            }
            DrawCommand::RoundedRect {
                x,
                y,
                w,
                h,
                radius,
                r,
                g,
                b,
                a,
            } => {
                let min = point(px(off_x + *x), px(off_y + *y));
                let cmd_bounds = Bounds::from_corners(min, min + point(px(*w), px(*h)));
                if !clipped_out(clip, cmd_bounds) {
                    let ca = apply_opacity(*a, opacity);
                    let pts = rounded_rect_polygon(off_x + *x, off_y + *y, *w, *h, *radius);
                    let mut pb = PathBuilder::fill();
                    pb.add_polygon(&pts, true);
                    if let Ok(path) = pb.build() {
                        window.paint_path(path, rgba8(*r, *g, *b, ca));
                    }
                }
            }
            DrawCommand::Arc {
                cx,
                cy,
                radius,
                start_angle,
                end_angle,
                r,
                g,
                b,
                a,
                thickness,
            } => {
                let pts = arc_polyline(off_x + *cx, off_y + *cy, *radius, *start_angle, *end_angle);
                if pts.len() >= 2 {
                    let mut pb = PathBuilder::stroke(px(*thickness));
                    pb.move_to(pts[0]);
                    for p in &pts[1..] {
                        pb.line_to(*p);
                    }
                    if let Ok(path) = pb.build() {
                        let ca = apply_opacity(*a, opacity);
                        window.paint_path(path, rgba8(*r, *g, *b, ca));
                    }
                }
            }
            DrawCommand::Bezier {
                x1,
                y1,
                cp1x,
                cp1y,
                cp2x,
                cp2y,
                x2,
                y2,
                r,
                g,
                b,
                a,
                thickness,
            } => {
                let p1 = point(px(off_x + *x1), px(off_y + *y1));
                let p2 = point(px(off_x + *x2), px(off_y + *y2));
                let c1 = point(px(off_x + *cp1x), px(off_y + *cp1y));
                let c2 = point(px(off_x + *cp2x), px(off_y + *cp2y));
                let mut pb = PathBuilder::stroke(px(*thickness));
                pb.move_to(p1);
                pb.cubic_bezier_to(p2, c1, c2);
                if let Ok(path) = pb.build() {
                    let ca = apply_opacity(*a, opacity);
                    window.paint_path(path, rgba8(*r, *g, *b, ca));
                }
            }
            DrawCommand::Gradient {
                x,
                y,
                w,
                h,
                kind,
                ax: _,
                ay: _,
                bx: _,
                by: _,
                stops,
            } => {
                paint_gradient(
                    window,
                    &GradientParams {
                        x: off_x + *x,
                        y: off_y + *y,
                        w: *w,
                        h: *h,
                        kind: *kind,
                        stops: stops.clone(),
                        opacity,
                    },
                );
            }
        }
    }
}

fn apply_opacity(a: u8, opacity: f32) -> u8 {
    (a as f32 * opacity).round().clamp(0.0, 255.0) as u8
}

fn clipped_out(clip: Option<Bounds<Pixels>>, target: Bounds<Pixels>) -> bool {
    if let Some(c) = clip {
        let cl = f32::from(c.origin.x);
        let ct = f32::from(c.origin.y);
        let cr = cl + f32::from(c.size.width);
        let cb = ct + f32::from(c.size.height);

        let tl = f32::from(target.origin.x);
        let tt = f32::from(target.origin.y);
        let tr = tl + f32::from(target.size.width);
        let tb = tt + f32::from(target.size.height);

        tr <= cl || tl >= cr || tb <= ct || tt >= cb
    } else {
        false
    }
}

fn intersect_bounds(a: Bounds<Pixels>, b: Bounds<Pixels>) -> Bounds<Pixels> {
    let al = f32::from(a.origin.x);
    let at = f32::from(a.origin.y);
    let ar = al + f32::from(a.size.width);
    let ab = at + f32::from(a.size.height);

    let bl = f32::from(b.origin.x);
    let bt = f32::from(b.origin.y);
    let br = bl + f32::from(b.size.width);
    let bb = bt + f32::from(b.size.height);

    let il = al.max(bl);
    let it = at.max(bt);
    let ir = ar.min(br);
    let ib = ab.min(bb);

    Bounds::from_corners(point(px(il), px(it)), point(px(ir.max(il)), px(ib.max(it))))
}

fn rounded_rect_polygon(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Vec<Point<Pixels>> {
    let r = radius.min(w / 2.0).min(h / 2.0);
    let segs = 4;
    let mut pts = Vec::with_capacity(segs * 4 + 4);
    for corner in 0..4 {
        let (corner_x, corner_y, angle_start) = match corner {
            0 => (x + w - r, y + r, -std::f32::consts::FRAC_PI_2), // top-right
            1 => (x + w - r, y + h - r, 0.0),                      // bottom-right
            2 => (x + r, y + h - r, std::f32::consts::FRAC_PI_2),  // bottom-left
            _ => (x + r, y + r, std::f32::consts::PI),             // top-left
        };
        for i in 0..=segs {
            let t = angle_start + (i as f32 / segs as f32) * std::f32::consts::FRAC_PI_2;
            pts.push(point(
                px(corner_x + r * t.cos()),
                px(corner_y + r * t.sin()),
            ));
        }
    }
    pts
}

fn arc_polyline(cx: f32, cy: f32, radius: f32, start: f32, end: f32) -> Vec<Point<Pixels>> {
    let sweep = end - start;
    let n = ((sweep.abs() / std::f32::consts::TAU) * 24.0)
        .ceil()
        .max(2.0) as usize;
    (0..=n)
        .map(|i| {
            let t = start + (i as f32 / n as f32) * sweep;
            point(px(cx + radius * t.cos()), px(cy + radius * t.sin()))
        })
        .collect()
}

struct GradientParams {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    kind: u8,
    stops: Vec<GradientStop>,
    opacity: f32,
}

fn paint_gradient(window: &mut Window, p: &GradientParams) {
    if p.stops.is_empty() {
        return;
    }

    // Keep band count low — GPUI's Metal scene buffer has per-frame limits and each band
    // is a separate quad.  8 bands gives a smooth-enough look without overwhelming the
    // renderer (64 bands was causing "scene too large" at >800 quads per frame).
    let bands: usize = 8;
    for i in 0..bands {
        let t = i as f32 / (bands - 1).max(1) as f32;
        let (sr, sg, sb, sa) = sample_gradient(&p.stops, t);
        let ca = apply_opacity(sa, p.opacity);

        if p.kind == 1 {
            // Radial: concentric rectangles from outside in.
            let frac = 1.0 - t;
            let bx = p.x + p.w * 0.5 * t;
            let by = p.y + p.h * 0.5 * t;
            let bw = p.w * frac;
            let bh = p.h * frac;
            if bw > 0.0 && bh > 0.0 {
                let min = point(px(bx), px(by));
                let band_bounds = Bounds::from_corners(min, min + point(px(bw), px(bh)));
                window.paint_quad(gpui::fill(band_bounds, rgba8(sr, sg, sb, ca)));
            }
        } else {
            // Linear: vertical bands along the gradient axis.
            let band_h = p.h / bands as f32;
            let by = p.y + i as f32 * band_h;
            let min = point(px(p.x), px(by));
            let band_bounds = Bounds::from_corners(min, min + point(px(p.w), px(band_h.ceil())));
            window.paint_quad(gpui::fill(band_bounds, rgba8(sr, sg, sb, ca)));
        }
    }
}

fn sample_gradient(stops: &[GradientStop], t: f32) -> (u8, u8, u8, u8) {
    if stops.len() == 1 {
        let s = &stops[0];
        return (s.r, s.g, s.b, s.a);
    }
    let t = t.clamp(0.0, 1.0);
    let mut lo = &stops[0];
    let mut hi = &stops[stops.len() - 1];
    for pair in stops.windows(2) {
        if t >= pair[0].offset && t <= pair[1].offset {
            lo = &pair[0];
            hi = &pair[1];
            break;
        }
    }
    let range = hi.offset - lo.offset;
    let frac = if range > 0.0 {
        (t - lo.offset) / range
    } else {
        0.0
    };
    let lerp = |a: u8, b: u8| -> u8 { (a as f32 + (b as f32 - a as f32) * frac).round() as u8 };
    (
        lerp(lo.r, hi.r),
        lerp(lo.g, hi.g),
        lerp(lo.b, hi.b),
        lerp(lo.a, hi.a),
    )
}

/// Result of a background `rfd` file dialog (must not run inside GPUI `App::update` — modal + focus events re-enter and panic).
enum FilePickDone {
    Chosen { path: PathBuf, bytes: Vec<u8> },
    Cancelled,
}

pub struct OxideBrowserView {
    tabs: Vec<TabState>,
    active_tab: usize,
    next_tab_id: u64,
    shared_kv_db: Option<Arc<sled::Db>>,
    shared_module_loader: Option<Arc<ModuleLoader>>,
    bookmark_store: Option<BookmarkStore>,
    history_store: Option<HistoryStore>,
    show_bookmarks: bool,
    show_menu: bool,
    /// Focus for the page (canvas + guest widgets); required for keyboard to reach `on_key_down` on the root.
    canvas_focus: FocusHandle,
    url_focus: FocusHandle,
    /// Receiver for [`FilePickDone`]; dialog runs on a background thread so the main thread never holds `App` during `NSOpenPanel`.
    file_pick_rx: Option<mpsc::Receiver<FilePickDone>>,
    download_manager: DownloadManager,
    show_downloads: bool,
}

impl OxideBrowserView {
    fn new(cx: &mut Context<Self>, host_state: HostState, status: Arc<Mutex<PageStatus>>) -> Self {
        let shared_kv_db = host_state.kv_db.clone();
        let shared_module_loader = host_state.module_loader.clone();
        let bookmark_store = host_state.bookmark_store.lock().unwrap().clone();
        let history_store = host_state.history_store.lock().unwrap().clone();
        let first_tab = TabState::new(0, host_state, status);
        Self {
            tabs: vec![first_tab],
            active_tab: 0,
            next_tab_id: 1,
            shared_kv_db,
            shared_module_loader,
            bookmark_store,
            history_store,
            show_bookmarks: false,
            show_menu: false,
            canvas_focus: cx.focus_handle(),
            url_focus: cx.focus_handle(),
            file_pick_rx: None,
            download_manager: DownloadManager::new(),
            show_downloads: false,
        }
    }

    fn poll_file_pick(&mut self, cx: &mut Context<Self>) {
        let rx = match self.file_pick_rx.take() {
            Some(r) => r,
            None => return,
        };
        match rx.try_recv() {
            Ok(FilePickDone::Chosen { path, bytes }) => {
                let file_url = format!("file://{}", path.display());
                let tab = &mut self.tabs[self.active_tab];
                tab.url_input = file_url.clone();
                tab.pending_history_url = Some(file_url);
                let _ = tab.run_tx.send(RunRequest::LoadLocal(bytes));
                cx.notify();
            }
            Ok(FilePickDone::Cancelled) => {}
            Err(TryRecvError::Empty) => {
                self.file_pick_rx = Some(rx);
            }
            Err(TryRecvError::Disconnected) => {}
        }
    }

    fn create_tab(&mut self) -> usize {
        let bm_shared: crate::bookmarks::SharedBookmarkStore =
            Arc::new(Mutex::new(self.bookmark_store.clone()));
        let hist_shared: crate::history::SharedHistoryStore =
            Arc::new(Mutex::new(self.history_store.clone()));
        let host_state = HostState {
            kv_db: self.shared_kv_db.clone(),
            module_loader: self.shared_module_loader.clone(),
            bookmark_store: bm_shared,
            history_store: hist_shared,
            ..Default::default()
        };
        let status = Arc::new(Mutex::new(PageStatus::Idle));
        let tab = TabState::new(self.next_tab_id, host_state, status);
        self.next_tab_id += 1;
        self.tabs.push(tab);
        self.tabs.len() - 1
    }

    /// Keep `active_tab` in range. Stale close handlers can fire with an old tab index after the strip shrinks.
    fn clamp_active_tab(&mut self) {
        if self.tabs.is_empty() {
            self.active_tab = 0;
            return;
        }
        self.active_tab = self.active_tab.min(self.tabs.len() - 1);
    }

    fn close_tab(&mut self, idx: usize) {
        if self.tabs.len() <= 1 {
            return;
        }
        if idx >= self.tabs.len() {
            self.clamp_active_tab();
            return;
        }
        self.tabs.remove(idx);
        if self.active_tab > idx {
            self.active_tab -= 1;
        } else if self.active_tab == idx && self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len().saturating_sub(1);
        }
        self.clamp_active_tab();
    }

    fn toggle_active_bookmark(&self) {
        let url = self.tabs[self.active_tab].url_input.trim().to_string();
        if url.is_empty() || url == "https://" {
            return;
        }
        if let Some(store) = &self.bookmark_store {
            if store.contains(&url) {
                let _ = store.remove(&url);
            } else {
                let title = url_to_title(&url);
                let _ = store.add(&url, &title);
            }
        }
    }
}

impl Render for OxideBrowserView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.clamp_active_tab();
        self.poll_file_pick(cx);
        let dm = self.download_manager.clone();
        for tab in &mut self.tabs {
            tab.drain_results();
            tab.handle_pending_navigation(&dm);
            tab.sync_url_bar();
        }

        let active = self.active_tab;
        let canvas_focused = self.canvas_focus.is_focused(window);
        {
            let tab = &mut self.tabs[active];
            tab.host_state.focused.store(canvas_focused, Ordering::Relaxed);
            tab.sync_keys_held_to_input();
            tab.tick_frame();
            tab.update_texture_cache(window);
            tab.refresh_pip_texture(window);
        }

        let canvas_offset = self.tabs[active].host_state.canvas_offset.clone();
        let cmds = self.tabs[active]
            .host_state
            .canvas
            .lock()
            .unwrap()
            .commands
            .clone();
        let hyperlinks = self.tabs[active]
            .host_state
            .hyperlinks
            .lock()
            .unwrap()
            .clone();
        let hyperlinks_hover = hyperlinks.clone();
        let widget_commands = self.tabs[active]
            .host_state
            .widget_commands
            .lock()
            .unwrap()
            .clone();
        let widget_cmds_overlay = widget_commands.clone();
        let textures = self.tabs[active].image_textures.clone();
        let show_console = self.tabs[active].show_console;
        let pip_tex = self.tabs[active].pip_texture.clone();

        self.tabs[active].post_tick_clear_input();

        cx.on_next_frame(window, |_this, _window, cx| {
            cx.notify();
        });

        let tab_titles: Vec<String> = self.tabs.iter().map(|t| t.display_title()).collect();
        let num_tabs = self.tabs.len();
        let active_tab = self.active_tab;
        let bm = self.bookmark_store.clone();
        let current_url = self.tabs[active].url_input.clone();
        let is_bookmarked = bm
            .as_ref()
            .map(|s| s.contains(&current_url))
            .unwrap_or(false);
        let url_focused = self.url_focus.is_focused(window);
        let caret_blink_on = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| (d.as_millis() / 530) % 2 == 0)
            .unwrap_or(true);
        let can_back = self.tabs[active]
            .host_state
            .navigation
            .lock()
            .unwrap()
            .can_go_back();
        let can_fwd = self.tabs[active]
            .host_state
            .navigation
            .lock()
            .unwrap()
            .can_go_forward();

        let mut root = div()
            .id("oxide_root")
            .track_focus(&self.canvas_focus)
            .focusable()
            .size_full()
            .flex()
            .flex_col()
            .bg(gpui::rgb(0x1a1a20))
            .on_key_down(cx.listener(
                |this: &mut OxideBrowserView, event: &KeyDownEvent, window, cx| {
                    {
                        let tab = &this.tabs[this.active_tab];
                        let mut input = tab.host_state.input_state.lock().unwrap();
                        input.modifiers_shift = event.keystroke.modifiers.shift;
                        input.modifiers_ctrl =
                            event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
                        input.modifiers_alt = event.keystroke.modifiers.alt;
                    }
                    if event.keystroke.modifiers.secondary() && event.keystroke.key == "t" {
                        let i = this.create_tab();
                        this.active_tab = i;
                        cx.notify();
                        return;
                    }
                    if event.keystroke.modifiers.secondary() && event.keystroke.key == "w" {
                        if this.tabs.len() > 1 {
                            let a = this.active_tab;
                            this.close_tab(a);
                        }
                        cx.notify();
                        return;
                    }
                    if event.keystroke.modifiers.control
                        && !event.keystroke.modifiers.shift
                        && event.keystroke.key == "tab"
                    {
                        if !this.tabs.is_empty() {
                            this.active_tab = (this.active_tab + 1) % this.tabs.len();
                        }
                        cx.notify();
                        return;
                    }
                    if event.keystroke.modifiers.control
                        && event.keystroke.modifiers.shift
                        && event.keystroke.key == "tab"
                    {
                        if !this.tabs.is_empty() {
                            if this.active_tab == 0 {
                                this.active_tab = this.tabs.len() - 1;
                            } else {
                                this.active_tab -= 1;
                            }
                        }
                        cx.notify();
                        return;
                    }
                    if event.keystroke.modifiers.secondary() && event.keystroke.key == "d" {
                        this.toggle_active_bookmark();
                        cx.notify();
                        return;
                    }
                    if event.keystroke.modifiers.secondary() && event.keystroke.key == "b" {
                        this.show_bookmarks = !this.show_bookmarks;
                        cx.notify();
                        return;
                    }
                    if this.url_focus.is_focused(window) {
                        return;
                    }
                    if let Some(id) = this.tabs[this.active_tab].text_input_focus {
                        let mut states = this.tabs[this.active_tab]
                            .host_state
                            .widget_states
                            .lock()
                            .unwrap();
                        let mut text = match states.get(&id) {
                            Some(WidgetValue::Text(t)) => t.clone(),
                            _ => String::new(),
                        };
                        if event.keystroke.modifiers.secondary() {
                            match event.keystroke.key.as_str() {
                                "c" => {
                                    if let Ok(mut cb) = arboard::Clipboard::new() {
                                        let _ = cb.set_text(&text);
                                    }
                                }
                                "v" => {
                                    if let Ok(mut cb) = arboard::Clipboard::new() {
                                        if let Ok(pasted) = cb.get_text() {
                                            text.push_str(&pasted);
                                            states.insert(id, WidgetValue::Text(text));
                                        }
                                    }
                                }
                                "x" => {
                                    if let Ok(mut cb) = arboard::Clipboard::new() {
                                        let _ = cb.set_text(&text);
                                    }
                                    states.insert(id, WidgetValue::Text(String::new()));
                                }
                                "a" => {}
                                _ => {}
                            }
                            cx.notify();
                            return;
                        }
                        if event.keystroke.key == "backspace" {
                            text.pop();
                        } else if let Some(s) = text_insert_from_keystroke(&event.keystroke) {
                            text.push_str(&s);
                        }
                        states.insert(id, WidgetValue::Text(text));
                        cx.notify();
                        return;
                    }
                    if let Some(code) = keystroke_to_oxide(&event.keystroke) {
                        let tab = &mut this.tabs[this.active_tab];
                        tab.keys_held.insert(code);
                        tab.host_state
                            .input_state
                            .lock()
                            .unwrap()
                            .keys_pressed
                            .push(code);
                        cx.notify();
                    }
                },
            ))
            .on_key_up(cx.listener(|this, event: &KeyUpEvent, _, _cx| {
                let tab = &mut this.tabs[this.active_tab];
                {
                    let mut input = tab.host_state.input_state.lock().unwrap();
                    input.modifiers_shift = event.keystroke.modifiers.shift;
                    input.modifiers_ctrl =
                        event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
                    input.modifiers_alt = event.keystroke.modifiers.alt;
                }
                if let Some(code) = keystroke_to_oxide(&event.keystroke) {
                    tab.keys_held.remove(&code);
                }
            }));

        // Tab strip
        root = root.child(
            div()
                .h(px(40.0))
                .flex()
                .flex_row()
                .items_center()
                .px_1()
                .border_b_1()
                .border_color(gpui::rgb(0x2a2a32))
                .children((0..num_tabs).map(|i| {
                    let title = tab_titles[i].clone();
                    let display = truncate_tab_title(&title);
                    let is_active = i == active_tab;
                    div()
                        .id(("oxide_tab", i))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .min_w(px(140.0))
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .cursor_pointer()
                        .when(is_active, |d| d.bg(gpui::rgb(0x373741)))
                        .text_sm()
                        .text_color(if is_active {
                            gpui::rgb(0xdcdce6)
                        } else {
                            gpui::rgb(0x9696a0)
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .overflow_hidden()
                                .child(display),
                        )
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.active_tab = i;
                            cx.notify();
                        }))
                        .when(num_tabs > 1, |d| {
                            d.child(
                                div()
                                    .id(("oxide_tab_close", i))
                                    .flex_shrink_0()
                                    .cursor_pointer()
                                    .text_color(gpui::rgb(0xa0a0aa))
                                    .child("×")
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.close_tab(i);
                                        cx.notify();
                                    })),
                            )
                        })
                }))
                .child(
                    div()
                        .id("oxide_new_tab")
                        .ml_1()
                        .cursor_pointer()
                        .text_color(gpui::rgb(0xc0c0cc))
                        .child("+")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            let i = this.create_tab();
                            this.active_tab = i;
                            cx.notify();
                        })),
                ),
        );

        // Toolbar
        let (status_icon, status_color) = {
            let status = self.tabs[active].status.lock().unwrap();
            let icon = match &*status {
                PageStatus::Idle => "○",
                PageStatus::Loading(_) => "↻",
                PageStatus::Running(_) => "●",
                PageStatus::Error(_) => "●",
            };
            let color = match &*status {
                PageStatus::Error(_) => gpui::rgb(0xf05050),
                PageStatus::Running(_) => gpui::rgb(0x50e070),
                _ => gpui::rgb(0xa0a0a8),
            };
            (icon, color)
        };

        root = root.child(
            div()
                .h(px(44.0))
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_2()
                .border_b_1()
                .border_color(gpui::rgb(0x2a2a32))
                .child(
                    div()
                        .id("oxide_back")
                        .when(can_back, |el| el.cursor_pointer())
                        .text_sm()
                        .text_color(if can_back {
                            gpui::rgb(0xb8b8c4)
                        } else {
                            gpui::rgb(0x50505a)
                        })
                        .child("◀")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.tabs[this.active_tab].go_back();
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .id("oxide_forward")
                        .when(can_fwd, |el| el.cursor_pointer())
                        .text_sm()
                        .text_color(if can_fwd {
                            gpui::rgb(0xb8b8c4)
                        } else {
                            gpui::rgb(0x50505a)
                        })
                        .child("▶")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.tabs[this.active_tab].go_forward();
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(status_color)
                        .child(status_icon.to_string()),
                )
                .child({
                    let url_text_for_canvas =
                        SharedString::from(self.tabs[active].url_input.clone());
                    let url_cursor = self.tabs[active].url_cursor;
                    let url_sel_start = self.tabs[active].url_sel_start;
                    let url_bounds_ref = self.tabs[active].url_text_bounds.clone();
                    div()
                        .id("oxide_url_bar")
                        .flex_1()
                        .flex()
                        .flex_row()
                        .items_center()
                        .h(px(32.0))
                        .px_2()
                        .rounded_md()
                        .bg(gpui::rgb(0x121218))
                        .border_1()
                        .border_color(if url_focused {
                            gpui::rgb(0x6a6aff)
                        } else {
                            gpui::rgb(0x121218)
                        })
                        .track_focus(&self.url_focus)
                        .overflow_hidden()
                        .on_key_down(cx.listener(
                            |this: &mut OxideBrowserView, event: &KeyDownEvent, window, cx| {
                                if !this.url_focus.is_focused(window) {
                                    return;
                                }
                                let shift = event.keystroke.modifiers.shift;
                                if event.keystroke.modifiers.secondary() {
                                    let tab = &mut this.tabs[this.active_tab];
                                    match event.keystroke.key.as_str() {
                                        "a" => {
                                            tab.url_select_all();
                                            cx.notify();
                                            return;
                                        }
                                        "c" => {
                                            let text = tab.url_selected_text();
                                            if !text.is_empty() {
                                                if let Ok(mut cb) = arboard::Clipboard::new() {
                                                    let _ = cb.set_text(text);
                                                }
                                            }
                                            return;
                                        }
                                        "x" => {
                                            let text = tab.url_selected_text();
                                            if !text.is_empty() {
                                                if let Ok(mut cb) = arboard::Clipboard::new() {
                                                    let _ = cb.set_text(text);
                                                }
                                                tab.url_delete_selection();
                                                cx.notify();
                                            }
                                            return;
                                        }
                                        "v" => {
                                            if let Ok(mut cb) = arboard::Clipboard::new() {
                                                if let Ok(text) = cb.get_text() {
                                                    tab.url_insert_at_cursor(&text);
                                                    cx.notify();
                                                }
                                            }
                                            return;
                                        }
                                        _ => {}
                                    }
                                }
                                let tab = &mut this.tabs[this.active_tab];
                                match event.keystroke.key.as_str() {
                                    "left" => {
                                        if shift {
                                            tab.url_select_to(tab.url_prev_boundary());
                                        } else if tab.url_has_selection() {
                                            let lo = tab.url_sel_range().start;
                                            tab.url_move_to(lo);
                                        } else {
                                            let prev = tab.url_prev_boundary();
                                            tab.url_move_to(prev);
                                        }
                                        cx.notify();
                                        return;
                                    }
                                    "right" => {
                                        if shift {
                                            tab.url_select_to(tab.url_next_boundary());
                                        } else if tab.url_has_selection() {
                                            let hi = tab.url_sel_range().end;
                                            tab.url_move_to(hi);
                                        } else {
                                            let next = tab.url_next_boundary();
                                            tab.url_move_to(next);
                                        }
                                        cx.notify();
                                        return;
                                    }
                                    "home" => {
                                        if shift {
                                            tab.url_select_to(0);
                                        } else {
                                            tab.url_move_to(0);
                                        }
                                        cx.notify();
                                        return;
                                    }
                                    "end" => {
                                        let len = tab.url_input.len();
                                        if shift {
                                            tab.url_select_to(len);
                                        } else {
                                            tab.url_move_to(len);
                                        }
                                        cx.notify();
                                        return;
                                    }
                                    "backspace" => {
                                        tab.url_backspace();
                                        cx.notify();
                                        return;
                                    }
                                    "delete" => {
                                        tab.url_delete_forward();
                                        cx.notify();
                                        return;
                                    }
                                    "enter" => {
                                        tab.navigate(&this.download_manager);
                                        this.show_downloads = this.download_manager.has_active()
                                            || this.show_downloads;
                                        cx.notify();
                                        return;
                                    }
                                    _ => {}
                                }
                                if let Some(s) = text_insert_from_keystroke(&event.keystroke) {
                                    tab.url_insert_at_cursor(&s);
                                    cx.notify();
                                }
                            },
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                if !this.url_focus.is_focused(window) {
                                    this.tabs[this.active_tab].url_select_all();
                                    cx.notify();
                                    return;
                                }
                                let tab = &mut this.tabs[this.active_tab];
                                let bounds = *tab.url_text_bounds.lock().unwrap();
                                let rel_x =
                                    f32::from(event.position.x) - f32::from(bounds.origin.x);
                                let text = SharedString::from(tab.url_input.clone());
                                if text.is_empty() {
                                    tab.url_move_to(0);
                                } else {
                                    let run = TextRun {
                                        len: text.len(),
                                        font: font(".SystemUIFont"),
                                        color: rgba8(0xdc, 0xdc, 0xe6, 0xff),
                                        background_color: None,
                                        underline: None,
                                        strikethrough: None,
                                    };
                                    let line = window.text_system().shape_line(
                                        text,
                                        px(14.0),
                                        &[run],
                                        None,
                                    );
                                    let idx = line.closest_index_for_x(px(rel_x));
                                    if event.modifiers.shift {
                                        tab.url_select_to(idx);
                                    } else {
                                        tab.url_move_to(idx);
                                        tab.url_selecting = true;
                                    }
                                }
                                cx.notify();
                            }),
                        )
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|this, _: &MouseUpEvent, _, _cx| {
                                this.tabs[this.active_tab].url_selecting = false;
                            }),
                        )
                        .on_mouse_move(cx.listener(
                            move |this, event: &gpui::MouseMoveEvent, window, _cx| {
                                let tab = &mut this.tabs[this.active_tab];
                                if !tab.url_selecting {
                                    return;
                                }
                                let bounds = *tab.url_text_bounds.lock().unwrap();
                                let rel_x =
                                    f32::from(event.position.x) - f32::from(bounds.origin.x);
                                let text = SharedString::from(tab.url_input.clone());
                                if text.is_empty() {
                                    return;
                                }
                                let run = TextRun {
                                    len: text.len(),
                                    font: font(".SystemUIFont"),
                                    color: rgba8(0xdc, 0xdc, 0xe6, 0xff),
                                    background_color: None,
                                    underline: None,
                                    strikethrough: None,
                                };
                                let line =
                                    window
                                        .text_system()
                                        .shape_line(text, px(14.0), &[run], None);
                                let idx = line.closest_index_for_x(px(rel_x));
                                tab.url_select_to(idx);
                                _cx.notify();
                            },
                        ))
                        .child({
                            let url_bounds_store = url_bounds_ref.clone();
                            canvas(
                                {
                                    let text = url_text_for_canvas.clone();
                                    let bounds_store = url_bounds_store.clone();
                                    move |bounds, window, _cx| {
                                        *bounds_store.lock().unwrap() = bounds;
                                        if text.is_empty() {
                                            return None;
                                        }
                                        let run = TextRun {
                                            len: text.len(),
                                            font: font(".SystemUIFont"),
                                            color: rgba8(0xdc, 0xdc, 0xe6, 0xff),
                                            background_color: None,
                                            underline: None,
                                            strikethrough: None,
                                        };
                                        Some(window.text_system().shape_line(
                                            text.clone(),
                                            px(14.0),
                                            &[run],
                                            None,
                                        ))
                                    }
                                },
                                {
                                    let focused = url_focused;
                                    let blink = caret_blink_on;
                                    move |bounds, line_opt: Option<gpui::ShapedLine>, window, cx| {
                                        let has_sel = url_cursor != url_sel_start;
                                        let sel_lo = url_cursor.min(url_sel_start);
                                        let sel_hi = url_cursor.max(url_sel_start);

                                        if let Some(ref line) = line_opt {
                                            if has_sel {
                                                let sx = line.x_for_index(sel_lo);
                                                let ex = line.x_for_index(sel_hi);
                                                let sel_bounds = Bounds::from_corners(
                                                    point(bounds.origin.x + sx, bounds.origin.y),
                                                    point(
                                                        bounds.origin.x + ex,
                                                        bounds.origin.y + bounds.size.height,
                                                    ),
                                                );
                                                window.paint_quad(gpui::fill(
                                                    sel_bounds,
                                                    rgba8(0x44, 0x66, 0xcc, 0x70),
                                                ));
                                            }

                                            let _ = line.paint(
                                                bounds.origin,
                                                bounds.size.height,
                                                window,
                                                cx,
                                            );

                                            if focused && !has_sel && blink {
                                                let cx_pos = line.x_for_index(url_cursor);
                                                let cursor_bounds = Bounds::from_corners(
                                                    point(
                                                        bounds.origin.x + cx_pos,
                                                        bounds.origin.y,
                                                    ),
                                                    point(
                                                        bounds.origin.x + cx_pos + px(2.0),
                                                        bounds.origin.y + bounds.size.height,
                                                    ),
                                                );
                                                window.paint_quad(gpui::fill(
                                                    cursor_bounds,
                                                    rgba8(0xe8, 0xe8, 0xf0, 0xff),
                                                ));
                                            }
                                        } else if focused && blink {
                                            let cursor_bounds = Bounds::from_corners(
                                                bounds.origin,
                                                point(
                                                    bounds.origin.x + px(2.0),
                                                    bounds.origin.y + bounds.size.height,
                                                ),
                                            );
                                            window.paint_quad(gpui::fill(
                                                cursor_bounds,
                                                rgba8(0xe8, 0xe8, 0xf0, 0xff),
                                            ));
                                        }
                                    }
                                },
                            )
                            .flex_1()
                            .h(px(16.0))
                        })
                })
                .child(
                    div()
                        .id("oxide_bookmark")
                        .cursor_pointer()
                        .text_lg()
                        .text_color(if is_bookmarked {
                            gpui::rgb(0xffc832)
                        } else {
                            gpui::rgb(0xa0a0a8)
                        })
                        .child(if is_bookmarked { "★" } else { "☆" })
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.toggle_active_bookmark();
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .id("oxide_open_file")
                        .cursor_pointer()
                        .text_sm()
                        .text_color(gpui::rgb(0xc8c8d4))
                        .child("Open")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            if this.file_pick_rx.is_some() {
                                return;
                            }
                            let (tx, rx) = mpsc::channel();
                            this.file_pick_rx = Some(rx);
                            std::thread::spawn(move || {
                                let path = rfd::FileDialog::new()
                                    .add_filter("WebAssembly", &["wasm"])
                                    .set_title("Open .wasm Application")
                                    .pick_file();
                                let msg = match path {
                                    Some(p) => match std::fs::read(&p) {
                                        Ok(bytes) => FilePickDone::Chosen { path: p, bytes },
                                        Err(_) => FilePickDone::Cancelled,
                                    },
                                    None => FilePickDone::Cancelled,
                                };
                                let _ = tx.send(msg);
                            });
                            cx.notify();
                        })),
                )
                .child({
                    let has_active_dl = self.download_manager.has_active();
                    let dl_count = self.download_manager.downloads().lock().unwrap().len();
                    div()
                        .id("oxide_downloads_btn")
                        .cursor_pointer()
                        .w(px(28.0))
                        .h(px(28.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .hover(|s| s.bg(gpui::rgb(0x373741)))
                        .text_color(if has_active_dl {
                            gpui::rgb(0x50b0e0)
                        } else if dl_count > 0 {
                            gpui::rgb(0xc8c8d4)
                        } else {
                            gpui::rgb(0x60606a)
                        })
                        .child("⬇")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.show_downloads = !this.show_downloads;
                            cx.notify();
                        }))
                })
                .child(
                    div()
                        .id("oxide_menu_btn")
                        .relative()
                        .cursor_pointer()
                        .w(px(28.0))
                        .h(px(28.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .hover(|s| s.bg(gpui::rgb(0x373741)))
                        .text_color(gpui::rgb(0xc8c8d4))
                        .child("⋮")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.show_menu = !this.show_menu;
                            cx.notify();
                        })),
                ),
        );

        // Main row: optional bookmarks + content
        let mut main_row = div().flex_1().flex().flex_row().min_h_0();

        if self.show_bookmarks {
            if let Some(store) = &self.bookmark_store {
                let items = store.list_all();
                main_row = main_row.child(
                    div()
                        .id("oxide_bookmarks_panel")
                        .w(px(260.0))
                        .h_full()
                        .overflow_scroll()
                        .border_r_1()
                        .border_color(gpui::rgb(0x2a2a32))
                        .p_2()
                        .children(items.iter().enumerate().map(|(bi, bm)| {
                            let url = bm.url.clone();
                            let label = if bm.title.is_empty() {
                                url_to_title(&bm.url)
                            } else {
                                bm.title.clone()
                            };
                            div()
                                .id(("oxide_bm", bi))
                                .py_1()
                                .cursor_pointer()
                                .text_sm()
                                .text_color(gpui::rgb(0xaab4ff))
                                .child(truncate_tab_title(&label))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.tabs[this.active_tab].navigate_to(
                                        url.clone(),
                                        true,
                                        &this.download_manager,
                                    );
                                    this.show_downloads =
                                        this.download_manager.has_active() || this.show_downloads;
                                    cx.notify();
                                }))
                        })),
                );
            }
        }

        let mut content_col = div().flex_1().flex().flex_col().min_h_0();

        if let Some(ref page) = self.tabs[active].internal_page {
            match page {
                InternalPage::History => {
                    let all_entries: Vec<(Vec<u8>, String, String, u64)> = self
                        .history_store
                        .as_ref()
                        .map(|store| {
                            store
                                .list_all()
                                .into_iter()
                                .map(|(key, item)| (key, item.url, item.title, item.visited_at_ms))
                                .collect()
                        })
                        .unwrap_or_default();
                    let has_entries = !all_entries.is_empty();

                    content_col = content_col.child(
                        div()
                            .id("oxide_history_page")
                            .flex_1()
                            .overflow_scroll()
                            .p_4()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .child(
                                                div()
                                                    .text_lg()
                                                    .font_weight(gpui::FontWeight::BOLD)
                                                    .text_color(gpui::rgb(0xb478ff))
                                                    .child("History"),
                                            )
                                            .child(
                                                div()
                                                    .mt_1()
                                                    .text_xs()
                                                    .text_color(gpui::rgb(0x7a7a90))
                                                    .child(format!(
                                                        "{} visited page{}",
                                                        all_entries.len(),
                                                        if all_entries.len() == 1 {
                                                            ""
                                                        } else {
                                                            "s"
                                                        }
                                                    )),
                                            ),
                                    )
                                    .when(has_entries, |d| {
                                        d.child(
                                            div()
                                                .id("oxide_hist_clear_all")
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_1()
                                                .px_3()
                                                .py(px(6.0))
                                                .rounded_md()
                                                .cursor_pointer()
                                                .bg(gpui::rgb(0x2a2a34))
                                                .hover(|s| s.bg(gpui::rgb(0x3a2a2a)))
                                                .text_xs()
                                                .text_color(gpui::rgb(0xf05050))
                                                .child("🗑")
                                                .child("Clear All")
                                                .on_click(cx.listener(
                                                    |this, _: &ClickEvent, _, cx| {
                                                        if let Some(store) = &this.history_store {
                                                            let _ = store.clear();
                                                        }
                                                        cx.notify();
                                                    },
                                                )),
                                        )
                                    }),
                            )
                            .child(div().mt_3().h(px(1.0)).bg(gpui::rgb(0x2a2a32)))
                            .when(!has_entries, |d| {
                                d.child(
                                    div()
                                        .mt_4()
                                        .text_sm()
                                        .text_color(gpui::rgb(0x7a7a90))
                                        .child(
                                            "No history yet. Navigate to a page to see it here.",
                                        ),
                                )
                            })
                            .children(all_entries.into_iter().enumerate().map(
                                |(i, (key, url, title, ts))| {
                                    let url_nav = url.clone();
                                    let key_for_delete = key.clone();
                                    let display_title = if title.is_empty() {
                                        url_to_title(&url)
                                    } else {
                                        title
                                    };
                                    let friendly = format_friendly_timestamp(ts);
                                    div()
                                        .id(("oxide_hist", i))
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .justify_between()
                                        .py_2()
                                        .px_2()
                                        .rounded_md()
                                        .hover(|s| s.bg(gpui::rgb(0x2a2a34)))
                                        .border_b_1()
                                        .border_color(gpui::rgb(0x222230))
                                        .child(
                                            div()
                                                .id(("oxide_hist_link", i))
                                                .flex_1()
                                                .min_w_0()
                                                .overflow_hidden()
                                                .cursor_pointer()
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(gpui::rgb(0xaab4ff))
                                                        .child(display_title),
                                                )
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(gpui::rgb(0x6a6a80))
                                                        .mt(px(2.0))
                                                        .child(url.clone()),
                                                )
                                                .on_click(cx.listener(
                                                    move |this, _: &ClickEvent, _, cx| {
                                                        this.tabs[this.active_tab].navigate_to(
                                                            url_nav.clone(),
                                                            true,
                                                            &this.download_manager,
                                                        );
                                                        this.show_downloads =
                                                            this.download_manager.has_active()
                                                                || this.show_downloads;
                                                        cx.notify();
                                                    },
                                                )),
                                        )
                                        .child(
                                            div()
                                                .flex_shrink_0()
                                                .ml_3()
                                                .text_xs()
                                                .text_color(gpui::rgb(0x7a7a90))
                                                .child(friendly),
                                        )
                                        .child(
                                            div()
                                                .id(("oxide_hist_del", i))
                                                .flex_shrink_0()
                                                .ml_2()
                                                .w(px(24.0))
                                                .h(px(24.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_sm()
                                                .cursor_pointer()
                                                .hover(|s| s.bg(gpui::rgb(0x3a2a2a)))
                                                .text_xs()
                                                .text_color(gpui::rgb(0x9696a0))
                                                .child("🗑")
                                                .on_click(cx.listener(
                                                    move |this, _: &ClickEvent, _, cx| {
                                                        if let Some(store) = &this.history_store {
                                                            let _ = store
                                                                .remove_by_key(&key_for_delete);
                                                        }
                                                        cx.notify();
                                                    },
                                                )),
                                        )
                                },
                            )),
                    );
                }
                InternalPage::Bookmarks => {
                    let items = self
                        .bookmark_store
                        .as_ref()
                        .map(|s| s.list_all())
                        .unwrap_or_default();

                    content_col = content_col.child(
                        div()
                            .id("oxide_bookmarks_page")
                            .flex_1()
                            .overflow_scroll()
                            .p_4()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(gpui::rgb(0xb478ff))
                                    .child("Bookmarks"),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_xs()
                                    .text_color(gpui::rgb(0x7a7a90))
                                    .child(format!(
                                        "{} bookmark{}",
                                        items.len(),
                                        if items.len() == 1 { "" } else { "s" }
                                    )),
                            )
                            .child(
                                div()
                                    .mt_3()
                                    .h(px(1.0))
                                    .bg(gpui::rgb(0x2a2a32)),
                            )
                            .when(items.is_empty(), |d| {
                                d.child(
                                    div()
                                        .mt_4()
                                        .text_sm()
                                        .text_color(gpui::rgb(0x7a7a90))
                                        .child("No bookmarks yet. Press ☆ in the toolbar to bookmark a page."),
                                )
                            })
                            .children(items.into_iter().enumerate().map(|(i, bm)| {
                                let url = bm.url.clone();
                                let url_nav = bm.url.clone();
                                let label = if bm.title.is_empty() {
                                    url_to_title(&bm.url)
                                } else {
                                    bm.title.clone()
                                };
                                div()
                                    .id(("oxide_bmp", i))
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .py_2()
                                    .px_2()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(gpui::rgb(0x2a2a34)))
                                    .border_b_1()
                                    .border_color(gpui::rgb(0x222230))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(gpui::rgb(0xaab4ff))
                                                    .child(label),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(gpui::rgb(0x6a6a80))
                                                    .mt(px(2.0))
                                                    .child(url),
                                            ),
                                    )
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.tabs[this.active_tab]
                                            .navigate_to(url_nav.clone(), true, &this.download_manager);
                                        this.show_downloads = this.download_manager.has_active() || this.show_downloads;
                                        cx.notify();
                                    }))
                            })),
                    );
                }
                InternalPage::About => {
                    content_col = content_col.child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .p_4()
                            .child(
                                div()
                                    .w(px(480.0))
                                    .p_5()
                                    .rounded_lg()
                                    .bg(gpui::rgb(0x222228))
                                    .border_1()
                                    .border_color(gpui::rgb(0x3a3a44))
                                    .child(
                                        div()
                                            .text_xl()
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(gpui::rgb(0xb478ff))
                                            .child("Oxide Browser"),
                                    )
                                    .child(
                                        div()
                                            .mt_1()
                                            .text_sm()
                                            .text_color(gpui::rgb(0x8888a0))
                                            .child(format!(
                                                "Version {}",
                                                env!("CARGO_PKG_VERSION")
                                            )),
                                    )
                                    .child(
                                        div()
                                            .mt_3()
                                            .h(px(1.0))
                                            .bg(gpui::rgb(0x3a3a44)),
                                    )
                                    .child(
                                        div()
                                            .mt_3()
                                            .text_sm()
                                            .text_color(gpui::rgb(0xc0c0cc))
                                            .child("A binary-first browser that fetches and runs .wasm modules in a secure sandbox, powered by a GPU-accelerated native UI."),
                                    )
                                    .child(
                                        div()
                                            .mt_3()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .text_xs()
                                            .text_color(gpui::rgb(0x9696a0))
                                            .child(
                                                div().flex().flex_row().gap_2()
                                                    .child(div().w(px(70.0)).text_color(gpui::rgb(0x7a7a90)).child("Engine"))
                                                    .child(div().child("Wasmtime sandbox")),
                                            )
                                            .child(
                                                div().flex().flex_row().gap_2()
                                                    .child(div().w(px(70.0)).text_color(gpui::rgb(0x7a7a90)).child("UI"))
                                                    .child(div().child("GPUI (Zed's GPU-accelerated framework)")),
                                            )
                                            .child(
                                                div().flex().flex_row().gap_2()
                                                    .child(div().w(px(70.0)).text_color(gpui::rgb(0x7a7a90)).child("Graphics"))
                                                    .child(div().child("Metal / wgpu")),
                                            )
                                            .child(
                                                div().flex().flex_row().gap_2()
                                                    .child(div().w(px(70.0)).text_color(gpui::rgb(0x7a7a90)).child("License"))
                                                    .child(div().child("MIT")),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .mt_3()
                                            .text_xs()
                                            .text_color(gpui::rgb(0x6a6a80))
                                            .child("github.com/niklabh/oxide"),
                                    ),
                            ),
                    );
                }
            }
        } else {
            let text_input_focus_id = self.tabs[active].text_input_focus;
            let caret_blink_on = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| (d.as_millis() / 530) % 2 == 0)
                .unwrap_or(true);

            let canvas_area = div()
                .id("oxide_canvas_area")
                .flex_1()
                .flex()
                .flex_col()
                .min_h_0()
                .relative()
                .on_mouse_move(cx.listener({
                    let hyperlinks_hover = hyperlinks_hover.clone();
                    move |this, event: &gpui::MouseMoveEvent, _, _cx| {
                        let tab = &mut this.tabs[this.active_tab];
                        let mut input = tab.host_state.input_state.lock().unwrap();
                        input.mouse_x = f32::from(event.position.x);
                        input.mouse_y = f32::from(event.position.y);
                        drop(input);
                        let (ox, oy) = *tab.host_state.canvas_offset.lock().unwrap();
                        let lx = f32::from(event.position.x) - ox;
                        let ly = f32::from(event.position.y) - oy;
                        let mut hovered = None;
                        for link in &hyperlinks_hover {
                            if lx >= link.x
                                && ly >= link.y
                                && lx <= link.x + link.w
                                && ly <= link.y + link.h
                            {
                                hovered = Some(link.url.clone());
                                break;
                            }
                        }
                        tab.hovered_link_url = hovered;
                    }
                }))
                .on_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, _, _cx| {
                    let tab = &mut this.tabs[this.active_tab];
                    let mut input = tab.host_state.input_state.lock().unwrap();
                    let b = match event.button {
                        MouseButton::Left => 0,
                        MouseButton::Right => 1,
                        MouseButton::Middle => 2,
                        _ => return,
                    };
                    input.mouse_buttons_down[b] = true;
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseUpEvent, _, _cx| {
                        let tab = &mut this.tabs[this.active_tab];
                        let mut input = tab.host_state.input_state.lock().unwrap();
                        input.mouse_buttons_down[0] = false;
                        input.mouse_buttons_clicked[0] = true;
                    }),
                )
                .on_mouse_up(
                    MouseButton::Right,
                    cx.listener(|this, _: &MouseUpEvent, _, _cx| {
                        let tab = &mut this.tabs[this.active_tab];
                        let mut input = tab.host_state.input_state.lock().unwrap();
                        input.mouse_buttons_down[1] = false;
                        input.mouse_buttons_clicked[1] = true;
                    }),
                )
                .on_mouse_up(
                    MouseButton::Middle,
                    cx.listener(|this, _: &MouseUpEvent, _, _cx| {
                        let tab = &mut this.tabs[this.active_tab];
                        let mut input = tab.host_state.input_state.lock().unwrap();
                        input.mouse_buttons_down[2] = false;
                        input.mouse_buttons_clicked[2] = true;
                    }),
                )
                .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                    if let Some(pos) = event.mouse_position() {
                        let tab = &mut this.tabs[this.active_tab];
                        let (ox, oy) = *tab.host_state.canvas_offset.lock().unwrap();
                        let lx = f32::from(pos.x) - ox;
                        let ly = f32::from(pos.y) - oy;
                        if canvas_point_hits_widget(lx, ly, &widget_cmds_overlay) {
                            return;
                        }
                        let links = tab.host_state.hyperlinks.lock().unwrap().clone();
                        for link in links.iter().rev() {
                            if lx >= link.x
                                && ly >= link.y
                                && lx <= link.x + link.w
                                && ly <= link.y + link.h
                            {
                                tab.navigate_to(link.url.clone(), true, &this.download_manager);
                                this.show_downloads =
                                    this.download_manager.has_active() || this.show_downloads;
                                cx.notify();
                                return;
                            }
                        }
                        tab.text_input_focus = None;
                        this.canvas_focus.focus(window);
                    }
                }))
                .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, _cx| {
                    let tab = &mut this.tabs[this.active_tab];
                    let mut input = tab.host_state.input_state.lock().unwrap();
                    match event.delta {
                        ScrollDelta::Pixels(p) => {
                            input.scroll_x += f32::from(p.x);
                            input.scroll_y += f32::from(p.y);
                        }
                        ScrollDelta::Lines(l) => {
                            input.scroll_x += l.x * 20.0;
                            input.scroll_y += l.y * 20.0;
                        }
                    }
                }))
                .on_drop(cx.listener(|this, paths: &gpui::ExternalPaths, _, _cx| {
                    let tab = &mut this.tabs[this.active_tab];
                    crate::events::enqueue_drop_files(&tab.host_state.events, paths.paths());
                }))
                .child({
                    let cmds = cmds.clone();
                    let textures = textures.clone();
                    let canvas_offset = canvas_offset.clone();
                    let canvas_state_for_dims = self.tabs[active].host_state.canvas.clone();
                    canvas(
                        move |bounds, _window, _cx| {
                            *canvas_offset.lock().unwrap() =
                                (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
                            let mut cs = canvas_state_for_dims.lock().unwrap();
                            cs.width = f32::from(bounds.size.width) as u32;
                            cs.height = f32::from(bounds.size.height) as u32;
                        },
                        move |bounds, (), window, cx| {
                            if cmds.is_empty() {
                                let _ = window
                                    .text_system()
                                    .shape_line(
                                        "Oxide Browser".into(),
                                        px(28.0),
                                        &[TextRun {
                                            len: 13,
                                            font: font(".SystemUIFont"),
                                            color: gpui::hsla(0.75, 0.5, 0.7, 1.0),
                                            background_color: None,
                                            underline: None,
                                            strikethrough: None,
                                        }],
                                        None,
                                    )
                                    .paint(
                                        bounds.origin + point(px(24.0), px(24.0)),
                                        px(32.0),
                                        window,
                                        cx,
                                    );
                            } else {
                                paint_draw_commands(window, cx, bounds, &cmds, &textures);
                            }
                        },
                    )
                    .flex_1()
                });

            let widget_states_snapshot = self.tabs[active]
                .host_state
                .widget_states
                .lock()
                .unwrap()
                .clone();

            let canvas_with_widgets =
                widget_commands
                    .into_iter()
                    .fold(canvas_area, |el, cmd| match cmd {
                        WidgetCommand::Button {
                            id,
                            x,
                            y,
                            w,
                            h,
                            label,
                        } => el.child(
                            div()
                                .id(("oxide_btn", id as usize))
                                .absolute()
                                .left(px(x))
                                .top(px(y))
                                .w(px(w))
                                .h(px(h))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_md()
                                .bg(gpui::rgb(0x3a3a48))
                                .cursor_pointer()
                                .text_sm()
                                .text_color(gpui::rgb(0xe8e8f0))
                                .child(label)
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.tabs[this.active_tab]
                                        .host_state
                                        .widget_clicked
                                        .lock()
                                        .unwrap()
                                        .insert(id);
                                    cx.notify();
                                })),
                        ),
                        WidgetCommand::Checkbox { id, x, y, label } => {
                            let checked = widget_states_snapshot
                                .get(&id)
                                .and_then(|v| match v {
                                    WidgetValue::Bool(b) => Some(*b),
                                    _ => None,
                                })
                                .unwrap_or(false);
                            el.child(
                                div()
                                    .id(("oxide_cb", id as usize))
                                    .absolute()
                                    .left(px(x))
                                    .top(px(y))
                                    .w(px(220.0))
                                    .h(px(30.0))
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        let mut states = this.tabs[this.active_tab]
                                            .host_state
                                            .widget_states
                                            .lock()
                                            .unwrap();
                                        let cur = states
                                            .get(&id)
                                            .and_then(|v| match v {
                                                WidgetValue::Bool(b) => Some(*b),
                                                _ => None,
                                            })
                                            .unwrap_or(false);
                                        states.insert(id, WidgetValue::Bool(!cur));
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(gpui::rgb(0xa0a0aa))
                                            .child(if checked { "☑" } else { "☐" }),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(gpui::rgb(0xd0d0dc))
                                            .child(label),
                                    ),
                            )
                        }
                        WidgetCommand::Slider {
                            id,
                            x,
                            y,
                            w,
                            min,
                            max,
                        } => {
                            let cur = widget_states_snapshot
                                .get(&id)
                                .and_then(|v| match v {
                                    WidgetValue::Float(f) => Some(*f),
                                    _ => None,
                                })
                                .unwrap_or(min);
                            el.child(
                                div()
                                    .id(("oxide_sl", id as usize))
                                    .absolute()
                                    .left(px(x))
                                    .top(px(y))
                                    .w(px(w))
                                    .h(px(28.0))
                                    .flex()
                                    .items_center()
                                    .rounded_md()
                                    .bg(gpui::rgb(0x2a2a32))
                                    .on_click(cx.listener(
                                        move |this, event: &ClickEvent, _, cx| {
                                            if let Some(pos) = event.mouse_position() {
                                                let tab = &mut this.tabs[this.active_tab];
                                                let (ox, _) =
                                                    *tab.host_state.canvas_offset.lock().unwrap();
                                                let lx = f32::from(pos.x) - ox;
                                                let frac = ((lx - x) / w).clamp(0.0, 1.0);
                                                let v = min + frac * (max - min);
                                                tab.host_state
                                                    .widget_states
                                                    .lock()
                                                    .unwrap()
                                                    .insert(id, WidgetValue::Float(v));
                                                cx.notify();
                                            }
                                        },
                                    ))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(gpui::rgb(0xb0b0c0))
                                            .child(format!("{cur:.1}")),
                                    ),
                            )
                        }
                        WidgetCommand::TextInput { id, x, y, w } => {
                            let value = widget_states_snapshot
                                .get(&id)
                                .and_then(|v| match v {
                                    WidgetValue::Text(t) => Some(t.clone()),
                                    _ => None,
                                })
                                .unwrap_or_default();
                            let show_caret = text_input_focus_id == Some(id) && caret_blink_on;
                            el.child(
                                div()
                                    .id(("oxide_ti", id as usize))
                                    .absolute()
                                    .left(px(x))
                                    .top(px(y))
                                    .w(px(w))
                                    .h(px(28.0))
                                    .px_2()
                                    .rounded_md()
                                    .bg(gpui::rgb(0x121218))
                                    .border_1()
                                    .border_color(if text_input_focus_id == Some(id) {
                                        gpui::rgb(0x6a6a8a)
                                    } else {
                                        gpui::rgb(0x3a3a48)
                                    })
                                    .cursor_pointer()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_start()
                                    .gap_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .flex_initial()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .text_sm()
                                            .text_color(gpui::rgb(0xe4e4ec))
                                            .child(SharedString::from(value)),
                                    )
                                    .when(show_caret, |d| {
                                        d.child(
                                            div()
                                                .flex_shrink_0()
                                                .w(px(2.0))
                                                .h(px(16.0))
                                                .mt(px(1.0))
                                                .rounded_sm()
                                                .bg(gpui::rgb(0xe8e8f0)),
                                        )
                                    })
                                    .on_click(cx.listener(
                                        move |this, _: &ClickEvent, window, cx| {
                                            this.tabs[this.active_tab].text_input_focus = Some(id);
                                            this.canvas_focus.focus(window);
                                            cx.notify();
                                        },
                                    )),
                            )
                        }
                    });

            content_col = content_col.child(canvas_with_widgets);
        }

        if let Some(tex) = pip_tex {
            content_col = content_col.child(
                div()
                    .id("oxide_pip")
                    .absolute()
                    .bottom(px(16.0))
                    .right(px(16.0))
                    .w(px(320.0))
                    .h(px(200.0))
                    .rounded_md()
                    .overflow_hidden()
                    .border_1()
                    .border_color(gpui::rgb(0x2a2a32))
                    .child(img(ImageSource::from(tex)).object_fit(gpui::ObjectFit::Contain)),
            );
        }

        if show_console {
            let entries = self.tabs[active].host_state.console.lock().unwrap().clone();
            content_col = content_col.child(
                div()
                    .id("oxide_console")
                    .h(px(160.0))
                    .border_t_1()
                    .border_color(gpui::rgb(0x2a2a32))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .h(px(28.0))
                            .px_2()
                            .border_b_1()
                            .border_color(gpui::rgb(0x2a2a32))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(gpui::rgb(0x9696a0))
                                    .child("Console"),
                            )
                            .child(
                                div()
                                    .id("oxide_console_close")
                                    .cursor_pointer()
                                    .w(px(20.0))
                                    .h(px(20.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_sm()
                                    .hover(|s| s.bg(gpui::rgb(0x3a3a48)))
                                    .text_xs()
                                    .text_color(gpui::rgb(0x9696a0))
                                    .child("✕")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.tabs[this.active_tab].show_console = false;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .id("oxide_console_entries")
                            .flex_1()
                            .overflow_scroll()
                            .p_2()
                            .font_family("Monaco")
                            .text_xs()
                            .children(entries.into_iter().map(|e| {
                                let color = match e.level {
                                    ConsoleLevel::Log => gpui::rgb(0xc8c8c8),
                                    ConsoleLevel::Warn => gpui::rgb(0xf0c83c),
                                    ConsoleLevel::Error => gpui::rgb(0xf05050),
                                };
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_color(gpui::rgb(0x646464))
                                            .child(e.timestamp.clone()),
                                    )
                                    .child(div().text_color(color).child(e.message.clone()))
                            })),
                    ),
            );
        }

        main_row = main_row.child(content_col);

        root = root.child(main_row);

        // Downloads panel
        {
            let downloads = self.download_manager.downloads();
            let list = downloads.lock().unwrap().clone();
            if self.show_downloads && !list.is_empty() {
                let panel_height = (list.len() as f32 * 56.0 + 32.0).min(240.0);
                root = root.child(
                    div()
                        .id("oxide_downloads_panel")
                        .h(px(panel_height))
                        .border_t_1()
                        .border_color(gpui::rgb(0x2a2a32))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_between()
                                .h(px(28.0))
                                .px_2()
                                .border_b_1()
                                .border_color(gpui::rgb(0x2a2a32))
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(gpui::rgb(0x9696a0))
                                        .child("Downloads"),
                                )
                                .child(
                                    div()
                                        .id("oxide_downloads_close")
                                        .cursor_pointer()
                                        .w(px(20.0))
                                        .h(px(20.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded_sm()
                                        .hover(|s| s.bg(gpui::rgb(0x3a3a48)))
                                        .text_xs()
                                        .text_color(gpui::rgb(0x9696a0))
                                        .child("✕")
                                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                            this.show_downloads = false;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .id("oxide_downloads_list")
                                .flex_1()
                                .overflow_y_scroll()
                                .children(list.iter().enumerate().map(|(idx, dl)| {
                                    let dl_id = dl.id;
                                    let filename = SharedString::from(dl.filename.clone());
                                    let (status_text, status_color) = match &dl.state {
                                        DownloadState::InProgress => {
                                            let downloaded = format_bytes(dl.bytes_downloaded);
                                            let total = dl
                                                .total_bytes
                                                .map(format_bytes)
                                                .unwrap_or_else(|| "?".to_string());
                                            let speed = format_bytes(dl.speed_bytes_per_sec as u64);
                                            let pct = dl
                                                .percent()
                                                .map(|p| format!("{p:.0}%"))
                                                .unwrap_or_default();
                                            (
                                                format!("{downloaded} / {total}  {speed}/s  {pct}"),
                                                gpui::rgb(0x50b0e0),
                                            )
                                        }
                                        DownloadState::Completed => {
                                            let total = format_bytes(dl.bytes_downloaded);
                                            (format!("Complete — {total}"), gpui::rgb(0x50e070))
                                        }
                                        DownloadState::Failed(msg) => {
                                            (format!("Failed: {msg}"), gpui::rgb(0xf05050))
                                        }
                                        DownloadState::Cancelled => {
                                            ("Cancelled".to_string(), gpui::rgb(0x9696a0))
                                        }
                                    };

                                    let progress_fraction = match &dl.state {
                                        DownloadState::InProgress => {
                                            dl.percent().map(|p| (p / 100.0) as f32).unwrap_or(0.0)
                                        }
                                        DownloadState::Completed => 1.0,
                                        _ => 0.0,
                                    };

                                    let is_active = dl.state == DownloadState::InProgress;

                                    div()
                                        .id(("oxide_dl", idx))
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_2()
                                        .px_2()
                                        .py_1()
                                        .border_b_1()
                                        .border_color(gpui::rgb(0x24242c))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .flex()
                                                .flex_col()
                                                .gap(px(2.0))
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(gpui::rgb(0xe4e4ec))
                                                        .overflow_hidden()
                                                        .child(filename),
                                                )
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(status_color)
                                                        .child(SharedString::from(status_text)),
                                                )
                                                .when(is_active, |d| {
                                                    d.child(
                                                        div()
                                                            .h(px(4.0))
                                                            .w_full()
                                                            .rounded_sm()
                                                            .bg(gpui::rgb(0x2a2a32))
                                                            .child(
                                                                div()
                                                                    .h_full()
                                                                    .rounded_sm()
                                                                    .bg(gpui::rgb(0x50b0e0))
                                                                    .w(gpui::relative(
                                                                        progress_fraction,
                                                                    )),
                                                            ),
                                                    )
                                                }),
                                        )
                                        .child(if is_active {
                                            div()
                                                .id(("oxide_dl_cancel", idx))
                                                .cursor_pointer()
                                                .flex_shrink_0()
                                                .px_2()
                                                .py(px(4.0))
                                                .rounded_sm()
                                                .text_xs()
                                                .text_color(gpui::rgb(0xf05050))
                                                .hover(|s| s.bg(gpui::rgb(0x3a3a48)))
                                                .child("Cancel")
                                                .on_click(cx.listener(
                                                    move |this, _: &ClickEvent, _, cx| {
                                                        this.download_manager.cancel(dl_id);
                                                        cx.notify();
                                                    },
                                                ))
                                        } else {
                                            div()
                                                .id(("oxide_dl_dismiss", idx))
                                                .cursor_pointer()
                                                .flex_shrink_0()
                                                .px_2()
                                                .py(px(4.0))
                                                .rounded_sm()
                                                .text_xs()
                                                .text_color(gpui::rgb(0x9696a0))
                                                .hover(|s| s.bg(gpui::rgb(0x3a3a48)))
                                                .child("Dismiss")
                                                .on_click(cx.listener(
                                                    move |this, _: &ClickEvent, _, cx| {
                                                        this.download_manager.dismiss(dl_id);
                                                        cx.notify();
                                                    },
                                                ))
                                        })
                                })),
                        ),
                );
            }
        }

        if let Some(url) = self.tabs[active].hovered_link_url.clone() {
            root = root.child(
                div()
                    .id("oxide_link_status")
                    .h(px(18.0))
                    .border_t_1()
                    .border_color(gpui::rgb(0x2a2a32))
                    .px_2()
                    .font_family("Monaco")
                    .text_xs()
                    .text_color(gpui::rgb(0x8c8cb4))
                    .child(url),
            );
        }

        if self.show_menu {
            root = root.child(
                div()
                    .id("oxide_menu_scrim")
                    .absolute()
                    .size_full()
                    .top_0()
                    .left_0()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.show_menu = false;
                        cx.notify();
                    })),
            );
            root = root.child(
                div()
                    .id("oxide_menu_dropdown")
                    .absolute()
                    .top(px(88.0))
                    .right(px(8.0))
                    .w(px(180.0))
                    .rounded_md()
                    .bg(gpui::rgb(0x2c2c36))
                    .border_1()
                    .border_color(gpui::rgb(0x3a3a44))
                    .py_1()
                    .shadow_lg()
                    .child(
                        div()
                            .id("oxide_menu_new_tab")
                            .px_3()
                            .py(px(8.0))
                            .cursor_pointer()
                            .text_sm()
                            .text_color(gpui::rgb(0xdcdce6))
                            .hover(|s| s.bg(gpui::rgb(0x3a3a48)))
                            .rounded_sm()
                            .child("  New Tab")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                let i = this.create_tab();
                                this.active_tab = i;
                                this.show_menu = false;
                                cx.notify();
                            })),
                    )
                    .child(div().h(px(1.0)).mx_2().my_1().bg(gpui::rgb(0x3a3a44)))
                    .child(
                        div()
                            .id("oxide_menu_bookmarks")
                            .px_3()
                            .py(px(8.0))
                            .cursor_pointer()
                            .text_sm()
                            .text_color(gpui::rgb(0xdcdce6))
                            .hover(|s| s.bg(gpui::rgb(0x3a3a48)))
                            .rounded_sm()
                            .child(if self.show_bookmarks {
                                "✓ Bookmarks"
                            } else {
                                "  Bookmarks"
                            })
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.show_bookmarks = !this.show_bookmarks;
                                this.show_menu = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("oxide_menu_console")
                            .px_3()
                            .py(px(8.0))
                            .cursor_pointer()
                            .text_sm()
                            .text_color(gpui::rgb(0xdcdce6))
                            .hover(|s| s.bg(gpui::rgb(0x3a3a48)))
                            .rounded_sm()
                            .child(if show_console {
                                "✓ Console"
                            } else {
                                "  Console"
                            })
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.tabs[this.active_tab].show_console =
                                    !this.tabs[this.active_tab].show_console;
                                this.show_menu = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("oxide_menu_downloads")
                            .px_3()
                            .py(px(8.0))
                            .cursor_pointer()
                            .text_sm()
                            .text_color(gpui::rgb(0xdcdce6))
                            .hover(|s| s.bg(gpui::rgb(0x3a3a48)))
                            .rounded_sm()
                            .child(if self.show_downloads {
                                "✓ Downloads"
                            } else {
                                "  Downloads"
                            })
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.show_downloads = !this.show_downloads;
                                this.show_menu = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("oxide_menu_history")
                            .px_3()
                            .py(px(8.0))
                            .cursor_pointer()
                            .text_sm()
                            .text_color(gpui::rgb(0xdcdce6))
                            .hover(|s| s.bg(gpui::rgb(0x3a3a48)))
                            .rounded_sm()
                            .child("  History")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                let i = this.create_tab();
                                this.active_tab = i;
                                this.tabs[i].navigate_to(
                                    "oxide://history".to_string(),
                                    true,
                                    &this.download_manager,
                                );
                                this.show_menu = false;
                                cx.notify();
                            })),
                    )
                    .child(div().h(px(1.0)).mx_2().my_1().bg(gpui::rgb(0x3a3a44)))
                    .child(
                        div()
                            .id("oxide_menu_about")
                            .px_3()
                            .py(px(8.0))
                            .cursor_pointer()
                            .text_sm()
                            .text_color(gpui::rgb(0xdcdce6))
                            .hover(|s| s.bg(gpui::rgb(0x3a3a48)))
                            .rounded_sm()
                            .child("  About Oxide")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                let i = this.create_tab();
                                this.active_tab = i;
                                this.tabs[i].navigate_to(
                                    "oxide://about".to_string(),
                                    true,
                                    &this.download_manager,
                                );
                                this.show_menu = false;
                                cx.notify();
                            })),
                    ),
            );
        }

        root
    }
}

/// Guest widget bounds in canvas-local coordinates (must match overlay hit-test skip logic).
fn widget_bounds(cmd: &WidgetCommand) -> (f32, f32, f32, f32) {
    match cmd {
        WidgetCommand::Button { x, y, w, h, .. } => (*x, *y, *w, *h),
        WidgetCommand::Checkbox { x, y, .. } => (*x, *y, 220.0, 30.0),
        WidgetCommand::Slider { x, y, w, .. } => (*x, *y, *w, 28.0),
        WidgetCommand::TextInput { x, y, w, .. } => (*x, *y, *w, 28.0),
    }
}

/// True if `(lx, ly)` lies inside any guest widget rect (canvas space).
fn canvas_point_hits_widget(lx: f32, ly: f32, cmds: &[WidgetCommand]) -> bool {
    for cmd in cmds {
        let (x, y, w, h) = widget_bounds(cmd);
        if lx >= x && ly >= y && lx <= x + w && ly <= y + h {
            return true;
        }
    }
    false
}

fn truncate_tab_title(title: &str) -> String {
    let max_len = 30;
    if title.chars().count() > max_len {
        let t: String = title.chars().take(max_len).collect();
        format!("{t}\u{2026}")
    } else {
        title.to_string()
    }
}

/// Typed character for URL bar and guest [`WidgetCommand::TextInput`] fields.
/// Uses `key_char` when set; otherwise mirrors [`Keystroke::with_simulated_ime`] for plain typing.
fn text_insert_from_keystroke(ks: &Keystroke) -> Option<String> {
    if ks.modifiers.control || ks.modifiers.platform || ks.modifiers.function || ks.modifiers.alt {
        return None;
    }
    if let Some(ref c) = ks.key_char {
        return Some(c.clone());
    }
    ks.clone().with_simulated_ime().key_char
}

fn keystroke_to_oxide(k: &Keystroke) -> Option<u32> {
    let key = k.key.as_str();
    match key {
        "a" => Some(0),
        "b" => Some(1),
        "c" => Some(2),
        "d" => Some(3),
        "e" => Some(4),
        "f" => Some(5),
        "g" => Some(6),
        "h" => Some(7),
        "i" => Some(8),
        "j" => Some(9),
        "k" => Some(10),
        "l" => Some(11),
        "m" => Some(12),
        "n" => Some(13),
        "o" => Some(14),
        "p" => Some(15),
        "q" => Some(16),
        "r" => Some(17),
        "s" => Some(18),
        "t" => Some(19),
        "u" => Some(20),
        "v" => Some(21),
        "w" => Some(22),
        "x" => Some(23),
        "y" => Some(24),
        "z" => Some(25),
        "0" => Some(26),
        "1" => Some(27),
        "2" => Some(28),
        "3" => Some(29),
        "4" => Some(30),
        "5" => Some(31),
        "6" => Some(32),
        "7" => Some(33),
        "8" => Some(34),
        "9" => Some(35),
        "enter" => Some(36),
        "escape" => Some(37),
        "tab" => Some(38),
        "backspace" => Some(39),
        "delete" => Some(40),
        "space" => Some(41),
        "up" => Some(42),
        "down" => Some(43),
        "left" => Some(44),
        "right" => Some(45),
        "home" => Some(46),
        "end" => Some(47),
        "pageup" => Some(48),
        "pagedown" => Some(49),
        _ => None,
    }
}

/// Returns `true` when `url` clearly points to a downloadable file rather
/// than a WASM module.  Heuristic: the URL path has a file extension and that
/// extension is *not* `.wasm`.  Bare directories and extensionless paths are
/// assumed to be WASM endpoints (they get `/index.wasm` appended by the
/// runtime).
fn is_downloadable_url(url: &str) -> bool {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return false;
    }
    if let Ok(parsed) = url::Url::parse(trimmed) {
        if !matches!(parsed.scheme(), "http" | "https") {
            return false;
        }
        let path = parsed.path();
        if path.ends_with('/') || path == "/" || path.is_empty() {
            return false;
        }
        if let Some(last_segment) = path.rsplit('/').next() {
            if let Some(dot) = last_segment.rfind('.') {
                let ext = &last_segment[dot + 1..];
                return !ext.eq_ignore_ascii_case("wasm");
            }
        }
    }
    false
}

fn url_to_title(url: &str) -> String {
    if url == "(local)" {
        return "Local Module".to_string();
    }
    match url {
        "oxide://history" => return "History".to_string(),
        "oxide://bookmarks" => return "Bookmarks".to_string(),
        "oxide://about" => return "About Oxide".to_string(),
        _ => {}
    }
    if let Some(stripped) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    {
        stripped.split('/').next().unwrap_or(stripped).to_string()
    } else if let Some(stripped) = url.strip_prefix("file://") {
        stripped
            .rsplit('/')
            .next()
            .unwrap_or("Local File")
            .to_string()
    } else {
        let max = 20;
        if url.chars().count() > max {
            let truncated: String = url.chars().take(max).collect();
            format!("{truncated}\u{2026}")
        } else {
            url.to_string()
        }
    }
}

/// Start the Oxide desktop shell: GPUI event loop and one main window.
///
/// Call this after constructing a [`crate::runtime::BrowserHost`] and cloning its [`HostState`]
/// and status mutex,
/// as the `oxide` binary does.
/// This function does not return until the application exits.
pub fn run_browser(host_state: HostState, status: Arc<Mutex<PageStatus>>) -> anyhow::Result<()> {
    Application::new().run(move |cx: &mut gpui::App| {
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let opts = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1024.0), px(720.0)), cx)),
            titlebar: Some(TitlebarOptions {
                title: Some("Oxide Browser".into()),
                ..Default::default()
            }),
            window_min_size: Some(size(px(600.0), px(400.0))),
            kind: WindowKind::Normal,
            ..Default::default()
        };
        cx.open_window(opts, move |_, cx| {
            cx.new(|cx| OxideBrowserView::new(cx, host_state.clone(), status.clone()))
        })
        .expect("open window");
    });
    Ok(())
}
