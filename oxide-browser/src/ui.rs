use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::capabilities::{ConsoleLevel, DrawCommand, HostState};
use crate::navigation::HistoryEntry;
use crate::runtime::PageStatus;

pub struct OxideApp {
    url_input: String,
    host_state: HostState,
    status: Arc<Mutex<PageStatus>>,
    show_console: bool,
    console_scroll_to_bottom: bool,
    tokio_rt: tokio::runtime::Runtime,
    run_tx: std::sync::mpsc::Sender<RunRequest>,
    run_rx: Arc<Mutex<std::sync::mpsc::Receiver<RunResult>>>,
    image_textures: HashMap<usize, egui::TextureHandle>,
    canvas_generation: u64,
    /// When true the next successful result will push to the history stack.
    pending_history_url: Option<String>,
    /// Tooltip shown in the status area when hovering a hyperlink.
    hovered_link_url: Option<String>,
}

enum RunRequest {
    FetchAndRun { url: String, push_history: bool },
    LoadLocal(Vec<u8>),
}

struct RunResult {
    error: Option<String>,
}

impl OxideApp {
    pub fn new(host_state: HostState, status: Arc<Mutex<PageStatus>>) -> Self {
        let tokio_rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
        let (req_tx, req_rx) = std::sync::mpsc::channel::<RunRequest>();
        let (res_tx, res_rx) = std::sync::mpsc::channel::<RunResult>();

        let hs = host_state.clone();
        let st = status.clone();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            loop {
                match req_rx.recv() {
                    Ok(request) => {
                        let mut host =
                            crate::runtime::BrowserHost::recreate(hs.clone(), st.clone());
                        let result = match request {
                            RunRequest::FetchAndRun { url, .. } => {
                                rt.block_on(host.fetch_and_run(&url))
                            }
                            RunRequest::LoadLocal(bytes) => host.run_bytes(&bytes),
                        };
                        let error = result.err().map(|e| e.to_string());
                        let _ = res_tx.send(RunResult { error });
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            url_input: String::from("https://"),
            host_state,
            status,
            show_console: true,
            console_scroll_to_bottom: true,
            tokio_rt,
            run_tx: req_tx,
            run_rx: Arc::new(Mutex::new(res_rx)),
            image_textures: HashMap::new(),
            canvas_generation: 0,
            pending_history_url: None,
            hovered_link_url: None,
        }
    }

    fn navigate(&mut self) {
        let url = self.url_input.trim().to_string();
        if url.is_empty() {
            return;
        }
        self.pending_history_url = Some(url.clone());
        let _ = self.run_tx.send(RunRequest::FetchAndRun {
            url,
            push_history: true,
        });
    }

    fn navigate_to(&mut self, url: String, push_history: bool) {
        self.url_input = url.clone();
        if push_history {
            self.pending_history_url = Some(url.clone());
        }
        let _ = self
            .run_tx
            .send(RunRequest::FetchAndRun { url, push_history });
    }

    fn go_back(&mut self) {
        let entry = {
            let mut nav = self.host_state.navigation.lock().unwrap();
            nav.go_back().cloned()
        };
        if let Some(entry) = entry {
            self.url_input = entry.url.clone();
            *self.host_state.current_url.lock().unwrap() = entry.url.clone();
            let _ = self.run_tx.send(RunRequest::FetchAndRun {
                url: entry.url,
                push_history: false,
            });
        }
    }

    fn go_forward(&mut self) {
        let entry = {
            let mut nav = self.host_state.navigation.lock().unwrap();
            nav.go_forward().cloned()
        };
        if let Some(entry) = entry {
            self.url_input = entry.url.clone();
            *self.host_state.current_url.lock().unwrap() = entry.url.clone();
            let _ = self.run_tx.send(RunRequest::FetchAndRun {
                url: entry.url,
                push_history: false,
            });
        }
    }

    fn load_local_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("WebAssembly", &["wasm"])
            .set_title("Open .wasm Application")
            .pick_file()
        {
            if let Ok(bytes) = std::fs::read(&path) {
                let file_url = format!("file://{}", path.display());
                self.url_input = file_url.clone();
                self.pending_history_url = Some(file_url);
                let _ = self.run_tx.send(RunRequest::LoadLocal(bytes));
            }
        }
    }
}

impl eframe::App for OxideApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain results from the background worker.
        if let Ok(rx) = self.run_rx.lock() {
            while let Ok(result) = rx.try_recv() {
                if let Some(err) = result.error {
                    *self.status.lock().unwrap() = PageStatus::Error(err);
                    self.pending_history_url = None;
                } else if let Some(url) = self.pending_history_url.take() {
                    let mut nav = self.host_state.navigation.lock().unwrap();
                    nav.push(HistoryEntry::new(&url));
                }
            }
        }

        // Check for guest-initiated navigations.
        let pending = self.host_state.pending_navigation.lock().unwrap().take();
        if let Some(url) = pending {
            self.navigate_to(url, true);
        }

        // Sync address bar with current URL (may have changed via push_state).
        {
            let cur = self.host_state.current_url.lock().unwrap().clone();
            if !cur.is_empty() && cur != self.url_input {
                let status = self.status.lock().unwrap().clone();
                if matches!(status, PageStatus::Running(_)) {
                    self.url_input = cur;
                }
            }
        }

        ctx.request_repaint();

        self.render_toolbar(ctx);
        self.render_canvas(ctx);
        if self.show_console {
            self.render_console(ctx);
        }

        if let Some(ref link_url) = self.hovered_link_url.clone() {
            egui::TopBottomPanel::bottom("link_status")
                .default_height(18.0)
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new(link_url)
                            .monospace()
                            .size(11.0)
                            .color(egui::Color32::from_rgb(140, 140, 180)),
                    );
                });
        }
    }
}

impl OxideApp {
    fn render_toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;

                let can_back = self.host_state.navigation.lock().unwrap().can_go_back();
                let can_fwd = self.host_state.navigation.lock().unwrap().can_go_forward();

                let back_btn = ui.add_enabled(
                    can_back,
                    egui::Button::new(egui::RichText::new("\u{2190}").size(16.0)),
                );
                if back_btn.clicked() {
                    self.go_back();
                }
                if back_btn.hovered() && can_back {
                    back_btn.on_hover_text("Back");
                }

                let fwd_btn = ui.add_enabled(
                    can_fwd,
                    egui::Button::new(egui::RichText::new("\u{2192}").size(16.0)),
                );
                if fwd_btn.clicked() {
                    self.go_forward();
                }
                if fwd_btn.hovered() && can_fwd {
                    fwd_btn.on_hover_text("Forward");
                }

                let status_icon = match &*self.status.lock().unwrap() {
                    PageStatus::Idle => "\u{26AA}",
                    PageStatus::Loading(_) => "\u{1F504}",
                    PageStatus::Running(_) => "\u{1F7E2}",
                    PageStatus::Error(_) => "\u{1F534}",
                };
                ui.label(egui::RichText::new(status_icon).size(16.0));

                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.url_input)
                        .desired_width(ui.available_width() - 160.0)
                        .hint_text("Enter .wasm URL...")
                        .font(egui::TextStyle::Monospace),
                );
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.navigate();
                }

                if ui.button("Go").clicked() {
                    self.navigate();
                }
                if ui.button("Open File").clicked() {
                    self.load_local_file();
                }

                let console_label = if self.show_console {
                    "Hide Console"
                } else {
                    "Show Console"
                };
                if ui.button(console_label).clicked() {
                    self.show_console = !self.show_console;
                }
            });

            let status = self.status.lock().unwrap().clone();
            if let PageStatus::Error(ref msg) = status {
                ui.colored_label(egui::Color32::from_rgb(220, 50, 50), msg);
            }
        });
    }

    fn render_canvas(&mut self, ctx: &egui::Context) {
        // Phase 1: Update texture cache from decoded images.
        {
            let canvas = self.host_state.canvas.lock().unwrap();
            if canvas.generation != self.canvas_generation {
                self.image_textures.clear();
                self.canvas_generation = canvas.generation;
            }
            for (i, decoded) in canvas.images.iter().enumerate() {
                if !self.image_textures.contains_key(&i) {
                    let color_image = egui::ColorImage::from_rgba_unmultiplied(
                        [decoded.width as usize, decoded.height as usize],
                        &decoded.pixels,
                    );
                    let handle = ctx.load_texture(
                        format!("oxide_img_{i}"),
                        color_image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.image_textures.insert(i, handle);
                }
            }
        }

        // Phase 2: Clone commands and hyperlinks.
        let commands = self.host_state.canvas.lock().unwrap().commands.clone();
        let hyperlinks = self.host_state.hyperlinks.lock().unwrap().clone();
        let tex_ids: HashMap<usize, egui::TextureId> = self
            .image_textures
            .iter()
            .map(|(k, v)| (*k, v.id()))
            .collect();

        // Phase 3: Render.
        self.hovered_link_url = None;

        egui::CentralPanel::default().show(ctx, |ui| {
            if commands.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() / 3.0);
                    ui.heading(
                        egui::RichText::new("Oxide Browser")
                            .size(32.0)
                            .color(egui::Color32::from_rgb(180, 120, 255)),
                    );
                    ui.label(
                        egui::RichText::new("A binary-first browser for WebAssembly applications")
                            .size(14.0)
                            .color(egui::Color32::GRAY),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(
                            "Enter a .wasm URL above or open a local file to get started.",
                        )
                        .color(egui::Color32::from_rgb(140, 140, 140)),
                    );
                });
                return;
            }

            let available = ui.available_size();
            let (response, painter) = ui.allocate_painter(available, egui::Sense::click());
            let rect = response.rect;

            for cmd in &commands {
                match cmd {
                    DrawCommand::Clear { r, g, b, a } => {
                        painter.rect_filled(
                            rect,
                            0.0,
                            egui::Color32::from_rgba_unmultiplied(*r, *g, *b, *a),
                        );
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
                        let min = rect.min + egui::vec2(*x, *y);
                        let r2 = egui::Rect::from_min_size(min, egui::vec2(*w, *h));
                        painter.rect_filled(
                            r2,
                            0.0,
                            egui::Color32::from_rgba_unmultiplied(*r, *g, *b, *a),
                        );
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
                        let center = rect.min + egui::vec2(*cx, *cy);
                        painter.circle_filled(
                            center,
                            *radius,
                            egui::Color32::from_rgba_unmultiplied(*r, *g, *b, *a),
                        );
                    }
                    DrawCommand::Text {
                        x,
                        y,
                        size,
                        r,
                        g,
                        b,
                        text,
                    } => {
                        let pos = rect.min + egui::vec2(*x, *y);
                        painter.text(
                            pos,
                            egui::Align2::LEFT_TOP,
                            text,
                            egui::FontId::proportional(*size),
                            egui::Color32::from_rgb(*r, *g, *b),
                        );
                    }
                    DrawCommand::Line {
                        x1,
                        y1,
                        x2,
                        y2,
                        r,
                        g,
                        b,
                        thickness,
                    } => {
                        let p1 = rect.min + egui::vec2(*x1, *y1);
                        let p2 = rect.min + egui::vec2(*x2, *y2);
                        painter.line_segment(
                            [p1, p2],
                            egui::Stroke::new(*thickness, egui::Color32::from_rgb(*r, *g, *b)),
                        );
                    }
                    DrawCommand::Image {
                        x,
                        y,
                        w,
                        h,
                        image_id,
                    } => {
                        if let Some(tex_id) = tex_ids.get(image_id) {
                            let img_rect = egui::Rect::from_min_size(
                                rect.min + egui::vec2(*x, *y),
                                egui::vec2(*w, *h),
                            );
                            let uv = egui::Rect::from_min_max(
                                egui::pos2(0.0, 0.0),
                                egui::pos2(1.0, 1.0),
                            );
                            painter.image(*tex_id, img_rect, uv, egui::Color32::WHITE);
                        }
                    }
                }
            }

            // Draw subtle underlines for hyperlink regions.
            for link in &hyperlinks {
                let link_rect = egui::Rect::from_min_size(
                    rect.min + egui::vec2(link.x, link.y),
                    egui::vec2(link.w, link.h),
                );
                painter.line_segment(
                    [link_rect.left_bottom(), link_rect.right_bottom()],
                    egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgba_unmultiplied(120, 140, 255, 80),
                    ),
                );
            }

            // Hyperlink hover / click detection.
            if let Some(pointer_pos) = response.hover_pos() {
                for link in &hyperlinks {
                    let link_rect = egui::Rect::from_min_size(
                        rect.min + egui::vec2(link.x, link.y),
                        egui::vec2(link.w, link.h),
                    );
                    if link_rect.contains(pointer_pos) {
                        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                        self.hovered_link_url = Some(link.url.clone());

                        // Highlight on hover.
                        painter.rect_filled(
                            link_rect,
                            0.0,
                            egui::Color32::from_rgba_unmultiplied(120, 140, 255, 30),
                        );

                        if response.clicked() {
                            let url = link.url.clone();
                            self.navigate_to(url, true);
                        }
                        break;
                    }
                }
            }
        });
    }

    fn render_console(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("console")
            .resizable(true)
            .default_height(160.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Console").strong());
                    if ui.small_button("Clear").clicked() {
                        self.host_state.console.lock().unwrap().clear();
                    }
                });
                ui.separator();

                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        let entries = self.host_state.console.lock().unwrap().clone();
                        for entry in &entries {
                            let color = match entry.level {
                                ConsoleLevel::Log => egui::Color32::from_rgb(200, 200, 200),
                                ConsoleLevel::Warn => egui::Color32::from_rgb(240, 200, 60),
                                ConsoleLevel::Error => egui::Color32::from_rgb(240, 70, 70),
                            };
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(&entry.timestamp)
                                        .monospace()
                                        .color(egui::Color32::from_rgb(100, 100, 100))
                                        .size(11.0),
                                );
                                ui.label(
                                    egui::RichText::new(&entry.message)
                                        .monospace()
                                        .color(color)
                                        .size(12.0),
                                );
                            });
                        }
                    });
            });
    }
}
