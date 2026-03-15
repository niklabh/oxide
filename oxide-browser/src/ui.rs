use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::capabilities::{ConsoleLevel, DrawCommand, HostState};
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
}

enum RunRequest {
    FetchAndRun(String),
    LoadLocal(Vec<u8>),
}

struct RunResult {
    error: Option<String>,
}

impl OxideApp {
    pub fn new(
        host_state: HostState,
        status: Arc<Mutex<PageStatus>>,
    ) -> Self {
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
                        let mut host = crate::runtime::BrowserHost::recreate(hs.clone(), st.clone());
                        let result = match request {
                            RunRequest::FetchAndRun(url) => {
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
        }
    }

    fn navigate(&mut self) {
        let url = self.url_input.trim().to_string();
        if url.is_empty() {
            return;
        }
        let _ = self.run_tx.send(RunRequest::FetchAndRun(url));
    }

    fn load_local_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("WebAssembly", &["wasm"])
            .set_title("Open .wasm Application")
            .pick_file()
        {
            if let Ok(bytes) = std::fs::read(&path) {
                self.url_input = format!("file://{}", path.display());
                let _ = self.run_tx.send(RunRequest::LoadLocal(bytes));
            }
        }
    }
}

impl eframe::App for OxideApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain any background results (check for errors).
        if let Ok(rx) = self.run_rx.lock() {
            while let Ok(result) = rx.try_recv() {
                if let Some(err) = result.error {
                    *self.status.lock().unwrap() = PageStatus::Error(err);
                }
            }
        }

        // Request continuous repainting for animations.
        ctx.request_repaint();

        self.render_toolbar(ctx);
        self.render_canvas(ctx);
        if self.show_console {
            self.render_console(ctx);
        }
    }
}

impl OxideApp {
    fn render_toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;

                let status_icon = match &*self.status.lock().unwrap() {
                    PageStatus::Idle => "⚪",
                    PageStatus::Loading(_) => "🔄",
                    PageStatus::Running(_) => "🟢",
                    PageStatus::Error(_) => "🔴",
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

                let console_label = if self.show_console { "Hide Console" } else { "Show Console" };
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

    fn render_canvas(&self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let canvas = self.host_state.canvas.lock().unwrap();

            if canvas.commands.is_empty() {
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
                        egui::RichText::new("Enter a .wasm URL above or open a local file to get started.")
                            .color(egui::Color32::from_rgb(140, 140, 140)),
                    );
                });
                return;
            }

            let available = ui.available_size();
            let (response, painter) =
                ui.allocate_painter(available, egui::Sense::hover());
            let rect = response.rect;

            for cmd in &canvas.commands {
                match cmd {
                    DrawCommand::Clear { r, g, b, a } => {
                        painter.rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(*r, *g, *b, *a));
                    }
                    DrawCommand::Rect { x, y, w, h, r, g, b, a } => {
                        let min = rect.min + egui::vec2(*x, *y);
                        let r2 = egui::Rect::from_min_size(min, egui::vec2(*w, *h));
                        painter.rect_filled(r2, 0.0, egui::Color32::from_rgba_unmultiplied(*r, *g, *b, *a));
                    }
                    DrawCommand::Circle { cx, cy, radius, r, g, b, a } => {
                        let center = rect.min + egui::vec2(*cx, *cy);
                        painter.circle_filled(center, *radius, egui::Color32::from_rgba_unmultiplied(*r, *g, *b, *a));
                    }
                    DrawCommand::Text { x, y, size, r, g, b, text } => {
                        let pos = rect.min + egui::vec2(*x, *y);
                        painter.text(
                            pos,
                            egui::Align2::LEFT_TOP,
                            text,
                            egui::FontId::proportional(*size),
                            egui::Color32::from_rgb(*r, *g, *b),
                        );
                    }
                    DrawCommand::Line { x1, y1, x2, y2, r, g, b, thickness } => {
                        let p1 = rect.min + egui::vec2(*x1, *y1);
                        let p2 = rect.min + egui::vec2(*x2, *y2);
                        painter.line_segment(
                            [p1, p2],
                            egui::Stroke::new(*thickness, egui::Color32::from_rgb(*r, *g, *b)),
                        );
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
