use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui;

use crate::capabilities::{ConsoleLevel, DrawCommand, HostState, WidgetCommand, WidgetValue};
use crate::navigation::HistoryEntry;
use crate::runtime::{LiveModule, PageStatus};

pub struct OxideApp {
    url_input: String,
    host_state: HostState,
    status: Arc<Mutex<PageStatus>>,
    show_console: bool,
    run_tx: std::sync::mpsc::Sender<RunRequest>,
    run_rx: Arc<Mutex<std::sync::mpsc::Receiver<RunResult>>>,
    image_textures: HashMap<usize, egui::TextureHandle>,
    canvas_generation: u64,
    pending_history_url: Option<String>,
    hovered_link_url: Option<String>,
    live_module: Option<LiveModule>,
    last_frame: Instant,
}

enum RunRequest {
    FetchAndRun { url: String },
    LoadLocal(Vec<u8>),
}

struct RunResult {
    error: Option<String>,
    live_module: Option<LiveModule>,
}

// Send is required to pass LiveModule through the channel.
// Store<HostState> is Send because HostState fields are Arc<Mutex<>>.
unsafe impl Send for RunResult {}

impl OxideApp {
    pub fn new(host_state: HostState, status: Arc<Mutex<PageStatus>>) -> Self {
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
            url_input: String::from("https://"),
            host_state,
            status,
            show_console: true,
            run_tx: req_tx,
            run_rx: Arc::new(Mutex::new(res_rx)),
            image_textures: HashMap::new(),
            canvas_generation: 0,
            pending_history_url: None,
            hovered_link_url: None,
            live_module: None,
            last_frame: Instant::now(),
        }
    }

    fn navigate(&mut self) {
        let url = self.url_input.trim().to_string();
        if url.is_empty() {
            return;
        }
        self.pending_history_url = Some(url.clone());
        let _ = self.run_tx.send(RunRequest::FetchAndRun { url });
    }

    fn navigate_to(&mut self, url: String, push_history: bool) {
        self.url_input = url.clone();
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
            *self.host_state.current_url.lock().unwrap() = entry.url.clone();
            let _ = self.run_tx.send(RunRequest::FetchAndRun { url: entry.url });
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
            let _ = self.run_tx.send(RunRequest::FetchAndRun { url: entry.url });
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

    fn capture_input(&self, ctx: &egui::Context) {
        let mut input = self.host_state.input_state.lock().unwrap();

        ctx.input(|i| {
            if let Some(pos) = i.pointer.hover_pos() {
                input.mouse_x = pos.x;
                input.mouse_y = pos.y;
            }

            input.mouse_buttons_down[0] = i.pointer.primary_down();
            input.mouse_buttons_down[1] = i.pointer.secondary_down();
            input.mouse_buttons_down[2] = i.pointer.middle_down();

            input.mouse_buttons_clicked[0] = i.pointer.primary_clicked();
            input.mouse_buttons_clicked[1] = i.pointer.secondary_clicked();
            input.mouse_buttons_clicked[2] = i.pointer.middle_down() && i.pointer.any_pressed();

            input.modifiers_shift = i.modifiers.shift;
            input.modifiers_ctrl = i.modifiers.ctrl;
            input.modifiers_alt = i.modifiers.alt;

            input.scroll_x = i.smooth_scroll_delta.x;
            input.scroll_y = i.smooth_scroll_delta.y;

            input.keys_down.clear();
            input.keys_pressed.clear();
            for event in &i.events {
                if let egui::Event::Key { key, pressed, .. } = event {
                    if let Some(code) = egui_key_to_oxide(key) {
                        if *pressed {
                            input.keys_pressed.push(code);
                        }
                        // For keys_down we'd need sustained state; approximate with pressed.
                        if *pressed {
                            input.keys_down.push(code);
                        }
                    }
                }
            }
        });
    }

    fn tick_frame(&mut self) {
        if self.live_module.is_none() {
            return;
        }

        let now = Instant::now();
        let dt = now - self.last_frame;
        self.last_frame = now;
        let dt_ms = dt.as_millis().min(100) as u32;

        // Clear widget commands so on_frame fills them fresh.
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
                        ConsoleLevel::Error,
                        msg.clone(),
                    );
                    *self.status.lock().unwrap() = PageStatus::Error(msg);
                    self.live_module = None;
                    return;
                }
            }
        }

        // Clear clicked set after on_frame has read it, before next render adds new clicks.
        self.host_state.widget_clicked.lock().unwrap().clear();
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
                    self.live_module = None;
                } else {
                    if let Some(url) = self.pending_history_url.take() {
                        let mut nav = self.host_state.navigation.lock().unwrap();
                        nav.push(HistoryEntry::new(&url));
                    }
                    // Reset widget state for the new module.
                    self.host_state.widget_states.lock().unwrap().clear();
                    self.host_state.widget_clicked.lock().unwrap().clear();
                    self.host_state.widget_commands.lock().unwrap().clear();
                    self.live_module = result.live_module;
                    self.last_frame = Instant::now();
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

        // Capture input and run the guest frame loop before rendering.
        self.capture_input(ctx);
        self.tick_frame();

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
                    egui::Button::new(egui::RichText::new("\u{25C0}").size(14.0))
                        .corner_radius(12.0)
                        .min_size(egui::vec2(28.0, 28.0))
                        .frame(false),
                );
                if back_btn.clicked() {
                    self.go_back();
                }
                if back_btn.hovered() && can_back {
                    back_btn.on_hover_text("Back");
                }

                let fwd_btn = ui.add_enabled(
                    can_fwd,
                    egui::Button::new(egui::RichText::new("\u{25B6}").size(14.0))
                        .corner_radius(12.0)
                        .min_size(egui::vec2(28.0, 28.0))
                        .frame(false),
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
                self.image_textures.entry(i).or_insert_with(|| {
                    let color_image = egui::ColorImage::from_rgba_unmultiplied(
                        [decoded.width as usize, decoded.height as usize],
                        &decoded.pixels,
                    );
                    ctx.load_texture(
                        format!("oxide_img_{i}"),
                        color_image,
                        egui::TextureOptions::LINEAR,
                    )
                });
            }
        }

        // Phase 2: Clone commands and hyperlinks.
        let commands = self.host_state.canvas.lock().unwrap().commands.clone();
        let hyperlinks = self.host_state.hyperlinks.lock().unwrap().clone();
        let widget_commands = self.host_state.widget_commands.lock().unwrap().clone();
        let tex_ids: HashMap<usize, egui::TextureId> = self
            .image_textures
            .iter()
            .map(|(k, v)| (*k, v.id()))
            .collect();

        // Phase 3: Render.
        self.hovered_link_url = None;

        egui::CentralPanel::default().show(ctx, |ui| {
            if commands.is_empty() && widget_commands.is_empty() {
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

            // Store canvas offset so input coordinates can be canvas-relative.
            *self.host_state.canvas_offset.lock().unwrap() = (rect.min.x, rect.min.y);

            // ── Draw commands ───────────────────────────────────────
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

            // ── Interactive widgets ─────────────────────────────────
            if !widget_commands.is_empty() {
                let mut widget_states = self.host_state.widget_states.lock().unwrap();
                let mut widget_clicked = self.host_state.widget_clicked.lock().unwrap();

                for cmd in &widget_commands {
                    match cmd {
                        WidgetCommand::Button {
                            id,
                            x,
                            y,
                            w,
                            h,
                            label,
                        } => {
                            let wr = egui::Rect::from_min_size(
                                rect.min + egui::vec2(*x, *y),
                                egui::vec2(*w, *h),
                            );
                            if ui.put(wr, egui::Button::new(label.as_str())).clicked() {
                                widget_clicked.insert(*id);
                            }
                        }
                        WidgetCommand::Checkbox { id, x, y, label } => {
                            let mut checked = match widget_states.get(id) {
                                Some(WidgetValue::Bool(b)) => *b,
                                _ => false,
                            };
                            let wr = egui::Rect::from_min_size(
                                rect.min + egui::vec2(*x, *y),
                                egui::vec2(250.0, 24.0),
                            );
                            if ui
                                .put(wr, egui::Checkbox::new(&mut checked, label.as_str()))
                                .changed()
                            {
                                widget_states.insert(*id, WidgetValue::Bool(checked));
                            }
                        }
                        WidgetCommand::Slider {
                            id,
                            x,
                            y,
                            w,
                            min,
                            max,
                        } => {
                            let mut value = match widget_states.get(id) {
                                Some(WidgetValue::Float(v)) => *v,
                                _ => *min,
                            };
                            let wr = egui::Rect::from_min_size(
                                rect.min + egui::vec2(*x, *y),
                                egui::vec2(*w, 24.0),
                            );
                            if ui
                                .put(wr, egui::Slider::new(&mut value, *min..=*max))
                                .changed()
                            {
                                widget_states.insert(*id, WidgetValue::Float(value));
                            }
                        }
                        WidgetCommand::TextInput { id, x, y, w } => {
                            let mut text = match widget_states.get(id) {
                                Some(WidgetValue::Text(t)) => t.clone(),
                                _ => String::new(),
                            };
                            let wr = egui::Rect::from_min_size(
                                rect.min + egui::vec2(*x, *y),
                                egui::vec2(*w, 24.0),
                            );
                            let te = egui::TextEdit::singleline(&mut text)
                                .desired_width(*w)
                                .id(egui::Id::new(("oxide_text_input", *id)));
                            if ui.put(wr, te).changed() {
                                widget_states.insert(*id, WidgetValue::Text(text));
                            }
                        }
                    }
                }
            }

            // ── Hyperlink underlines ────────────────────────────────
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

            // ── Hyperlink hover / click detection ───────────────────
            if let Some(pointer_pos) = response.hover_pos() {
                for link in &hyperlinks {
                    let link_rect = egui::Rect::from_min_size(
                        rect.min + egui::vec2(link.x, link.y),
                        egui::vec2(link.w, link.h),
                    );
                    if link_rect.contains(pointer_pos) {
                        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                        self.hovered_link_url = Some(link.url.clone());

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

// ── Key mapping ─────────────────────────────────────────────────────────────

fn egui_key_to_oxide(key: &egui::Key) -> Option<u32> {
    use egui::Key::*;
    match key {
        A => Some(0),
        B => Some(1),
        C => Some(2),
        D => Some(3),
        E => Some(4),
        F => Some(5),
        G => Some(6),
        H => Some(7),
        I => Some(8),
        J => Some(9),
        K => Some(10),
        L => Some(11),
        M => Some(12),
        N => Some(13),
        O => Some(14),
        P => Some(15),
        Q => Some(16),
        R => Some(17),
        S => Some(18),
        T => Some(19),
        U => Some(20),
        V => Some(21),
        W => Some(22),
        X => Some(23),
        Y => Some(24),
        Z => Some(25),
        Num0 => Some(26),
        Num1 => Some(27),
        Num2 => Some(28),
        Num3 => Some(29),
        Num4 => Some(30),
        Num5 => Some(31),
        Num6 => Some(32),
        Num7 => Some(33),
        Num8 => Some(34),
        Num9 => Some(35),
        Enter => Some(36),
        Escape => Some(37),
        Tab => Some(38),
        Backspace => Some(39),
        Delete => Some(40),
        Space => Some(41),
        ArrowUp => Some(42),
        ArrowDown => Some(43),
        ArrowLeft => Some(44),
        ArrowRight => Some(45),
        Home => Some(46),
        End => Some(47),
        PageUp => Some(48),
        PageDown => Some(49),
        _ => None,
    }
}
