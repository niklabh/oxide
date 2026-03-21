use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui;

use crate::bookmarks::BookmarkStore;
use crate::capabilities::{ConsoleLevel, DrawCommand, HostState, WidgetCommand, WidgetValue};
use crate::engine::ModuleLoader;
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

// Send is required to pass LiveModule through the channel.
// Store<HostState> is Send because HostState fields are Arc<Mutex<>>.
unsafe impl Send for RunResult {}

// ── Per-tab state ───────────────────────────────────────────────────────────

struct TabState {
    id: u64,
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
            canvas_generation: 0,
            pending_history_url: None,
            hovered_link_url: None,
            live_module: None,
            last_frame: Instant::now(),
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

    // ── Navigation ──────────────────────────────────────────────────────

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

    // ── Frame lifecycle ─────────────────────────────────────────────────

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

        self.host_state.widget_clicked.lock().unwrap().clear();
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

    fn handle_pending_navigation(&mut self) {
        let pending = self.host_state.pending_navigation.lock().unwrap().take();
        if let Some(url) = pending {
            self.navigate_to(url, true);
        }
    }

    fn sync_url_bar(&mut self) {
        let cur = self.host_state.current_url.lock().unwrap().clone();
        if !cur.is_empty() && cur != self.url_input {
            let status = self.status.lock().unwrap().clone();
            if matches!(status, PageStatus::Running(_)) {
                self.url_input = cur;
            }
        }
    }

    // ── Rendering ───────────────────────────────────────────────────────

    fn render_toolbar(
        &mut self,
        ctx: &egui::Context,
        bookmark_store: &Option<BookmarkStore>,
        show_bookmarks: &mut bool,
        show_about: &mut bool,
    ) {
        let can_back = self.host_state.navigation.lock().unwrap().can_go_back();
        let can_fwd = self.host_state.navigation.lock().unwrap().can_go_forward();

        let status_icon = match &*self.status.lock().unwrap() {
            PageStatus::Idle => "\u{26AA}",
            PageStatus::Loading(_) => "\u{1F504}",
            PageStatus::Running(_) => "\u{1F7E2}",
            PageStatus::Error(_) => "\u{1F534}",
        }
        .to_string();

        let status = self.status.lock().unwrap().clone();

        let current_url = self.url_input.clone();
        let is_bookmarked = bookmark_store
            .as_ref()
            .map(|s| s.contains(&current_url))
            .unwrap_or(false);

        let mut toggle_bookmark = false;
        let mut toggle_panel = false;

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;

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

                ui.label(egui::RichText::new(&status_icon).size(16.0));

                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.url_input)
                        .desired_width(ui.available_width() - 190.0)
                        .hint_text("Enter .wasm URL...")
                        .font(egui::TextStyle::Monospace),
                );
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.navigate();
                }

                let has_url =
                    !self.url_input.trim().is_empty() && self.url_input.trim() != "https://";

                if has_url {
                    let star = if is_bookmarked {
                        "\u{2605}" // filled star
                    } else {
                        "\u{2606}" // outline star
                    };
                    let star_color = if is_bookmarked {
                        egui::Color32::from_rgb(255, 200, 50)
                    } else {
                        egui::Color32::from_rgb(160, 160, 170)
                    };
                    let star_btn = ui.add(
                        egui::Button::new(egui::RichText::new(star).size(18.0).color(star_color))
                            .frame(false)
                            .min_size(egui::vec2(28.0, 28.0)),
                    );
                    if star_btn.clicked() {
                        toggle_bookmark = true;
                    }
                    star_btn.on_hover_text(if is_bookmarked {
                        "Remove bookmark"
                    } else {
                        "Add bookmark"
                    });
                }

                if ui.button("Go").clicked() {
                    self.navigate();
                }
                if ui.button("Open File").clicked() {
                    self.load_local_file();
                }

                let bm_color = if *show_bookmarks {
                    egui::Color32::from_rgb(255, 200, 50)
                } else {
                    egui::Color32::from_rgb(160, 160, 170)
                };
                let bm_btn = ui.add(
                    egui::Button::new(
                        egui::RichText::new("\u{2605}").size(15.0).color(bm_color),
                    )
                    .frame(false)
                    .min_size(egui::vec2(28.0, 28.0)),
                );
                if bm_btn.clicked() {
                    toggle_panel = true;
                }
                bm_btn.on_hover_text(if *show_bookmarks {
                    "Hide bookmarks"
                } else {
                    "Show bookmarks"
                });

                // ── Three-dots overflow menu ─────────────────────
                let dot_size = egui::vec2(28.0, 28.0);
                let (menu_rect, menu_resp) =
                    ui.allocate_exact_size(dot_size, egui::Sense::click());
                if ui.is_rect_visible(menu_rect) {
                    let c = menu_rect.center();
                    let dot_color = if menu_resp.hovered() {
                        egui::Color32::from_rgb(220, 220, 230)
                    } else {
                        egui::Color32::from_rgb(160, 160, 170)
                    };
                    let r = 2.0;
                    let gap = 5.0;
                    ui.painter()
                        .circle_filled(c + egui::vec2(0.0, -gap), r, dot_color);
                    ui.painter().circle_filled(c, r, dot_color);
                    ui.painter()
                        .circle_filled(c + egui::vec2(0.0, gap), r, dot_color);
                }
                let menu_id = ui.make_persistent_id("toolbar_overflow_menu");
                if menu_resp.clicked() {
                    ui.memory_mut(|mem| mem.toggle_popup(menu_id));
                }
                let menu_resp = menu_resp.on_hover_text("Menu");
                egui::popup_below_widget(
                    ui,
                    menu_id,
                    &menu_resp,
                    egui::PopupCloseBehavior::CloseOnClick,
                    |ui| {
                        ui.set_min_width(160.0);

                        let console_label = if self.show_console {
                            "Hide Console"
                        } else {
                            "Show Console"
                        };
                        if ui.button(console_label).clicked() {
                            self.show_console = !self.show_console;
                        }

                        ui.separator();

                        if ui.button("About Oxide").clicked() {
                            *show_about = true;
                        }
                    },
                );
            });

            if let PageStatus::Error(ref msg) = status {
                ui.colored_label(egui::Color32::from_rgb(220, 50, 50), msg);
            }
        });

        if toggle_bookmark {
            if let Some(store) = bookmark_store.as_ref() {
                if is_bookmarked {
                    let _ = store.remove(&current_url);
                } else {
                    let title = url_to_title(&current_url);
                    let _ = store.add(&current_url, &title);
                }
            }
        }

        if toggle_panel {
            *show_bookmarks = !*show_bookmarks;
        }
    }

    fn render_canvas(&mut self, ctx: &egui::Context) {
        // Phase 1: Update texture cache from decoded images.
        {
            let canvas = self.host_state.canvas.lock().unwrap();
            if canvas.generation != self.canvas_generation {
                self.image_textures.clear();
                self.canvas_generation = canvas.generation;
            }
            let tab_id = self.id;
            for (i, decoded) in canvas.images.iter().enumerate() {
                self.image_textures.entry(i).or_insert_with(|| {
                    let color_image = egui::ColorImage::from_rgba_unmultiplied(
                        [decoded.width as usize, decoded.height as usize],
                        &decoded.pixels,
                    );
                    ctx.load_texture(
                        format!("oxide_img_{i}_tab{tab_id}"),
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

        let host_state = self.host_state.clone();
        let canvas_offset = self.host_state.canvas_offset.clone();

        // Phase 3: Render.
        self.hovered_link_url = None;
        let mut new_hovered: Option<String> = None;
        let mut clicked_link: Option<String> = None;

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

            *canvas_offset.lock().unwrap() = (rect.min.x, rect.min.y);

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
                let mut widget_states = host_state.widget_states.lock().unwrap();
                let mut widget_clicked = host_state.widget_clicked.lock().unwrap();

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
                        new_hovered = Some(link.url.clone());

                        painter.rect_filled(
                            link_rect,
                            0.0,
                            egui::Color32::from_rgba_unmultiplied(120, 140, 255, 30),
                        );

                        if response.clicked() {
                            clicked_link = Some(link.url.clone());
                        }
                        break;
                    }
                }
            }
        });

        self.hovered_link_url = new_hovered;
        if let Some(url) = clicked_link {
            self.navigate_to(url, true);
        }
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

// ── Application ─────────────────────────────────────────────────────────────

pub struct OxideApp {
    tabs: Vec<TabState>,
    active_tab: usize,
    next_tab_id: u64,
    shared_kv_db: Option<Arc<sled::Db>>,
    shared_module_loader: Option<Arc<ModuleLoader>>,
    bookmark_store: Option<BookmarkStore>,
    show_bookmarks: bool,
    show_about: bool,
}

impl OxideApp {
    pub fn new(host_state: HostState, status: Arc<Mutex<PageStatus>>) -> Self {
        let shared_kv_db = host_state.kv_db.clone();
        let shared_module_loader = host_state.module_loader.clone();
        let bookmark_store = host_state.bookmark_store.lock().unwrap().clone();

        let first_tab = TabState::new(0, host_state, status);

        Self {
            tabs: vec![first_tab],
            active_tab: 0,
            next_tab_id: 1,
            shared_kv_db,
            shared_module_loader,
            bookmark_store,
            show_bookmarks: false,
            show_about: false,
        }
    }

    fn create_tab(&mut self) -> usize {
        let bm_shared: crate::bookmarks::SharedBookmarkStore =
            Arc::new(Mutex::new(self.bookmark_store.clone()));
        let host_state = HostState {
            kv_db: self.shared_kv_db.clone(),
            module_loader: self.shared_module_loader.clone(),
            bookmark_store: bm_shared,
            ..Default::default()
        };
        let status = Arc::new(Mutex::new(PageStatus::Idle));
        let tab = TabState::new(self.next_tab_id, host_state, status);
        self.next_tab_id += 1;
        self.tabs.push(tab);
        self.tabs.len() - 1
    }

    fn close_tab(&mut self, idx: usize) {
        if self.tabs.len() <= 1 {
            return;
        }
        self.tabs.remove(idx);
        if self.active_tab == idx {
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len() - 1;
            }
        } else if self.active_tab > idx {
            self.active_tab -= 1;
        }
    }

    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        let (new_tab, close_tab, next_tab, prev_tab, toggle_bookmark, toggle_panel) =
            ctx.input(|i| {
                let cmd = i.modifiers.command;
                (
                    cmd && i.key_pressed(egui::Key::T),
                    cmd && i.key_pressed(egui::Key::W),
                    i.modifiers.ctrl && !i.modifiers.shift && i.key_pressed(egui::Key::Tab),
                    i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::Tab),
                    cmd && i.key_pressed(egui::Key::D),
                    cmd && i.key_pressed(egui::Key::B),
                )
            });

        if new_tab {
            let idx = self.create_tab();
            self.active_tab = idx;
        }
        if close_tab && self.tabs.len() > 1 {
            let active = self.active_tab;
            self.close_tab(active);
        }
        if next_tab && !self.tabs.is_empty() {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
        }
        if prev_tab && !self.tabs.is_empty() {
            if self.active_tab == 0 {
                self.active_tab = self.tabs.len() - 1;
            } else {
                self.active_tab -= 1;
            }
        }
        if toggle_bookmark {
            self.toggle_active_bookmark();
        }
        if toggle_panel {
            self.show_bookmarks = !self.show_bookmarks;
        }
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

impl eframe::App for OxideApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_keyboard_shortcuts(ctx);

        // Drain results and handle navigations for all tabs so background loads complete.
        for tab in &mut self.tabs {
            tab.drain_results();
            tab.handle_pending_navigation();
            tab.sync_url_bar();
        }

        ctx.request_repaint();

        let active = self.active_tab;
        self.tabs[active].capture_input(ctx);
        self.tabs[active].tick_frame();

        self.render_tab_bar(ctx);

        let bm_store = self.bookmark_store.clone();
        let mut show_bm = self.show_bookmarks;
        let mut show_about = self.show_about;
        self.tabs[self.active_tab].render_toolbar(ctx, &bm_store, &mut show_bm, &mut show_about);
        self.show_bookmarks = show_bm;
        self.show_about = show_about;

        let mut nav_to_url: Option<String> = None;
        if self.show_bookmarks {
            nav_to_url = Self::render_bookmarks_panel(ctx, &self.bookmark_store);
        }

        self.tabs[self.active_tab].render_canvas(ctx);

        if self.tabs[self.active_tab].show_console {
            self.tabs[self.active_tab].render_console(ctx);
        }

        if let Some(url) = nav_to_url {
            self.tabs[self.active_tab].navigate_to(url, true);
        }

        if let Some(link_url) = self.tabs[self.active_tab].hovered_link_url.clone() {
            egui::TopBottomPanel::bottom("link_status")
                .default_height(18.0)
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new(&link_url)
                            .monospace()
                            .size(11.0)
                            .color(egui::Color32::from_rgb(140, 140, 180)),
                    );
                });
        }

        if self.show_about {
            self.render_about_modal(ctx);
        }
    }
}

impl OxideApp {
    fn render_about_modal(&mut self, ctx: &egui::Context) {
        egui::Window::new("About Oxide")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([360.0, 0.0])
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.heading(
                        egui::RichText::new("Oxide Browser")
                            .size(24.0)
                            .strong()
                            .color(egui::Color32::from_rgb(180, 120, 255)),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                            .size(13.0)
                            .color(egui::Color32::from_rgb(160, 160, 170)),
                    );
                    ui.add_space(12.0);
                });

                ui.label("A binary-first, decentralized browser that loads and runs WebAssembly modules instead of HTML/JavaScript.");
                ui.add_space(8.0);

                egui::Grid::new("about_details")
                    .num_columns(2)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("Runtime")
                                .strong()
                                .color(egui::Color32::from_rgb(180, 180, 190)),
                        );
                        ui.label("Wasmtime");
                        ui.end_row();

                        ui.label(
                            egui::RichText::new("UI")
                                .strong()
                                .color(egui::Color32::from_rgb(180, 180, 190)),
                        );
                        ui.label("egui / eframe");
                        ui.end_row();

                        ui.label(
                            egui::RichText::new("Sandbox")
                                .strong()
                                .color(egui::Color32::from_rgb(180, 180, 190)),
                        );
                        ui.label("Capability-based, 16 MB memory limit");
                        ui.end_row();

                        ui.label(
                            egui::RichText::new("Storage")
                                .strong()
                                .color(egui::Color32::from_rgb(180, 180, 190)),
                        );
                        ui.label("sled embedded KV store");
                        ui.end_row();

                        ui.label(
                            egui::RichText::new("License")
                                .strong()
                                .color(egui::Color32::from_rgb(180, 180, 190)),
                        );
                        ui.label("MIT");
                        ui.end_row();
                    });

                ui.add_space(12.0);
                ui.vertical_centered(|ui| {
                    if ui.button("Close").clicked() {
                        self.show_about = false;
                    }
                });
                ui.add_space(4.0);
            });
    }
}

impl OxideApp {
    fn render_tab_bar(&mut self, ctx: &egui::Context) {
        let mut switch_to = None;
        let mut close_idx = None;
        let mut open_new = false;
        let num_tabs = self.tabs.len();

        egui::TopBottomPanel::top("tab_bar")
            .exact_height(30.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;

                    for i in 0..num_tabs {
                        let is_active = i == self.active_tab;
                        let title = self.tabs[i].display_title();

                        let bg = if is_active {
                            egui::Color32::from_rgb(55, 55, 65)
                        } else {
                            egui::Color32::TRANSPARENT
                        };

                        egui::Frame::NONE
                            .fill(bg)
                            .inner_margin(4.0)
                            .corner_radius(4.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;

                                    let text_color = if is_active {
                                        egui::Color32::from_rgb(220, 220, 230)
                                    } else {
                                        egui::Color32::from_rgb(150, 150, 160)
                                    };

                                    let max_len = 22;
                                    let display = if title.chars().count() > max_len {
                                        let truncated: String =
                                            title.chars().take(max_len).collect();
                                        format!("{truncated}\u{2026}")
                                    } else {
                                        title
                                    };

                                    let tab_label = ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&display)
                                                .color(text_color)
                                                .size(12.0),
                                        )
                                        .sense(egui::Sense::click()),
                                    );
                                    if tab_label.clicked() {
                                        switch_to = Some(i);
                                    }

                                    if num_tabs > 1 {
                                        let close_color = if is_active {
                                            egui::Color32::from_rgb(160, 160, 170)
                                        } else {
                                            egui::Color32::from_rgb(100, 100, 110)
                                        };
                                        let close_btn = ui.add(
                                            egui::Label::new(
                                                egui::RichText::new("\u{00D7}")
                                                    .color(close_color)
                                                    .size(16.0),
                                            )
                                            .sense(egui::Sense::click()),
                                        );
                                        if close_btn.clicked() {
                                            close_idx = Some(i);
                                        }
                                    }
                                });
                            });
                    }

                    ui.add_space(4.0);

                    let shortcut_hint = if cfg!(target_os = "macos") {
                        "New tab (\u{2318}T)"
                    } else {
                        "New tab (Ctrl+T)"
                    };
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new("+").size(16.0))
                                .frame(false)
                                .min_size(egui::vec2(24.0, 22.0)),
                        )
                        .on_hover_text(shortcut_hint)
                        .clicked()
                    {
                        open_new = true;
                    }
                });
            });

        if let Some(i) = close_idx {
            self.close_tab(i);
        }
        if open_new {
            let idx = self.create_tab();
            self.active_tab = idx;
        }
        if let Some(i) = switch_to {
            if i < self.tabs.len() {
                self.active_tab = i;
            }
        }
    }
}

impl OxideApp {
    fn render_bookmarks_panel(
        ctx: &egui::Context,
        bookmark_store: &Option<BookmarkStore>,
    ) -> Option<String> {
        let store = match bookmark_store.as_ref() {
            Some(s) => s,
            None => return None,
        };

        let all = store.list_all();
        let favorites: Vec<_> = all.iter().filter(|b| b.is_favorite).collect();
        let regular: Vec<_> = all.iter().filter(|b| !b.is_favorite).collect();

        let mut navigate_url: Option<String> = None;
        let mut toggle_fav_url: Option<String> = None;
        let mut remove_url: Option<String> = None;

        egui::SidePanel::left("bookmarks_panel")
            .default_width(260.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading(
                    egui::RichText::new("\u{2605} Bookmarks")
                        .size(16.0)
                        .color(egui::Color32::from_rgb(255, 200, 50)),
                );
                ui.separator();

                if all.is_empty() {
                    ui.add_space(20.0);
                    ui.label(
                        egui::RichText::new("No bookmarks yet.\nClick the \u{2606} star in the toolbar to bookmark a page.")
                            .color(egui::Color32::from_rgb(130, 130, 140))
                            .size(12.0),
                    );
                    return;
                }

                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        if !favorites.is_empty() {
                            ui.label(
                                egui::RichText::new("Favorites")
                                    .strong()
                                    .size(13.0)
                                    .color(egui::Color32::from_rgb(255, 200, 50)),
                            );
                            ui.add_space(4.0);
                            for bm in &favorites {
                                render_bookmark_row(
                                    ui,
                                    bm,
                                    &mut navigate_url,
                                    &mut toggle_fav_url,
                                    &mut remove_url,
                                );
                            }
                            if !regular.is_empty() {
                                ui.add_space(8.0);
                                ui.separator();
                                ui.add_space(4.0);
                            }
                        }

                        if !regular.is_empty() {
                            ui.label(
                                egui::RichText::new("All Bookmarks")
                                    .strong()
                                    .size(13.0)
                                    .color(egui::Color32::from_rgb(180, 180, 190)),
                            );
                            ui.add_space(4.0);
                            for bm in &regular {
                                render_bookmark_row(
                                    ui,
                                    bm,
                                    &mut navigate_url,
                                    &mut toggle_fav_url,
                                    &mut remove_url,
                                );
                            }
                        }
                    });
            });

        if let Some(url) = toggle_fav_url {
            let _ = store.toggle_favorite(&url);
        }
        if let Some(url) = remove_url {
            let _ = store.remove(&url);
        }

        navigate_url
    }
}

fn render_bookmark_row(
    ui: &mut egui::Ui,
    bm: &crate::bookmarks::Bookmark,
    navigate_url: &mut Option<String>,
    toggle_fav_url: &mut Option<String>,
    remove_url: &mut Option<String>,
) {
    let max_title_len = 28;
    let display_title = if bm.title.is_empty() {
        url_to_title(&bm.url)
    } else {
        bm.title.clone()
    };
    let truncated = if display_title.chars().count() > max_title_len {
        let t: String = display_title.chars().take(max_title_len).collect();
        format!("{t}\u{2026}")
    } else {
        display_title
    };

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;

        let fav_icon = if bm.is_favorite {
            "\u{2605}"
        } else {
            "\u{2606}"
        };
        let fav_color = if bm.is_favorite {
            egui::Color32::from_rgb(255, 200, 50)
        } else {
            egui::Color32::from_rgb(120, 120, 130)
        };
        let fav_btn = ui.add(
            egui::Label::new(egui::RichText::new(fav_icon).color(fav_color).size(14.0))
                .sense(egui::Sense::click()),
        );
        if fav_btn.clicked() {
            *toggle_fav_url = Some(bm.url.clone());
        }
        fav_btn.on_hover_text(if bm.is_favorite {
            "Unfavorite"
        } else {
            "Mark as favorite"
        });

        let link = ui.add(
            egui::Label::new(
                egui::RichText::new(&truncated)
                    .color(egui::Color32::from_rgb(170, 190, 255))
                    .size(12.5),
            )
            .sense(egui::Sense::click()),
        );
        if link.clicked() {
            *navigate_url = Some(bm.url.clone());
        }
        link.on_hover_text(&bm.url);

        let del_btn = ui.add(
            egui::Label::new(
                egui::RichText::new("\u{00D7}")
                    .color(egui::Color32::from_rgb(140, 100, 100))
                    .size(14.0),
            )
            .sense(egui::Sense::click()),
        );
        if del_btn.clicked() {
            *remove_url = Some(bm.url.clone());
        }
        del_btn.on_hover_text("Remove bookmark");
    });
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn url_to_title(url: &str) -> String {
    if url == "(local)" {
        return "Local Module".to_string();
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
