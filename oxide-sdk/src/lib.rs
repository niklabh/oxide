#![allow(clippy::too_many_arguments)]
#![allow(clippy::doc_overindented_list_items)]

//! # Oxide SDK
//!
//! Guest-side SDK for building WebAssembly applications that run inside the
//! [Oxide browser](https://github.com/niklabh/oxide). This crate provides
//! safe Rust wrappers around the raw host-imported functions exposed by the
//! `"oxide"` wasm import module.
//!
//! The desktop shell uses [GPUI](https://www.gpui.rs/) (Zed's GPU-accelerated
//! UI framework) to render guest draw commands. The SDK exposes a drawing API
//! that maps directly onto GPUI primitives — filled quads, GPU-shaped text,
//! vector paths, and image textures — so your canvas output gets full GPU
//! acceleration without you having to link GPUI itself.
//!
//! ## Quick Start
//!
//! ```toml
//! [lib]
//! crate-type = ["cdylib"]
//!
//! [dependencies]
//! oxide-sdk = "0.4"
//! ```
//!
//! ### Static app (one-shot render)
//!
//! ```rust,ignore
//! use oxide_sdk::*;
//!
//! #[no_mangle]
//! pub extern "C" fn start_app() {
//!     log("Hello from Oxide!");
//!     canvas_clear(30, 30, 46, 255);
//!     canvas_text(20.0, 40.0, 28.0, 255, 255, 255, 255, "Welcome to Oxide");
//! }
//! ```
//!
//! ### Interactive app (frame loop)
//!
//! ```rust,ignore
//! use oxide_sdk::*;
//!
//! #[no_mangle]
//! pub extern "C" fn start_app() {
//!     log("Interactive app started");
//! }
//!
//! #[no_mangle]
//! pub extern "C" fn on_frame(_dt_ms: u32) {
//!     canvas_clear(30, 30, 46, 255);
//!     let (mx, my) = mouse_position();
//!     canvas_circle(mx, my, 20.0, 255, 100, 100, 255);
//!
//!     if ui_button(1, 20.0, 20.0, 100.0, 30.0, "Click me!") {
//!         log("Button was clicked!");
//!     }
//! }
//! ```
//!
//! ### High-level drawing API
//!
//! The [`draw`] module provides GPUI-inspired ergonomic types for less
//! boilerplate:
//!
//! ```rust,ignore
//! use oxide_sdk::draw::*;
//!
//! #[no_mangle]
//! pub extern "C" fn start_app() {
//!     let c = Canvas::new();
//!     c.clear(Color::hex(0x1e1e2e));
//!     c.fill_rect(Rect::new(10.0, 10.0, 200.0, 100.0), Color::rgb(80, 120, 200));
//!     c.fill_circle(Point2D::new(300.0, 200.0), 50.0, Color::RED);
//!     c.text("Hello!", Point2D::new(20.0, 30.0), 24.0, Color::WHITE);
//! }
//! ```
//!
//! Build with `cargo build --target wasm32-unknown-unknown --release`.
//!
//! ## API Categories
//!
//! | Category | Key types / functions |
//! |----------|-----------|
//! | **Drawing (high-level)** | [`draw::Canvas`], [`draw::Color`], [`draw::Rect`], [`draw::Point2D`], [`draw::GradientStop`] |
//! | **Canvas (low-level)** | [`canvas_clear`], [`canvas_rect`], [`canvas_circle`], [`canvas_text`], [`canvas_line`], [`canvas_image`], [`canvas_dimensions`] |
//! | **Extended shapes** | [`canvas_rounded_rect`], [`canvas_arc`], [`canvas_bezier`], [`canvas_gradient`] |
//! | **Canvas state** | [`canvas_save`], [`canvas_restore`], [`canvas_transform`], [`canvas_clip`], [`canvas_opacity`] |
//! | **GPU** | [`gpu_create_buffer`], [`gpu_create_texture`], [`gpu_create_shader`], [`gpu_create_pipeline`], [`gpu_draw`], [`gpu_dispatch_compute`] |
//! | **Console** | [`log`], [`warn`], [`error`] |
//! | **HTTP** | [`fetch`], [`fetch_get`], [`fetch_post`], [`fetch_post_proto`], [`fetch_put`], [`fetch_delete`] |
//! | **HTTP (streaming)** | [`fetch_begin`], [`fetch_begin_get`], [`fetch_state`], [`fetch_status`], [`fetch_recv`], [`fetch_error`], [`fetch_abort`], [`fetch_remove`] |
//! | **Protobuf** | [`proto::ProtoEncoder`], [`proto::ProtoDecoder`] |
//! | **Storage** | [`storage_set`], [`storage_get`], [`storage_remove`], [`kv_store_set`], [`kv_store_get`], [`kv_store_delete`] |
//! | **Audio** | [`audio_play`], [`audio_play_url`], [`audio_detect_format`], [`audio_play_with_format`], [`audio_pause`], [`audio_channel_play`] |
//! | **Video** | [`video_load`], [`video_load_url`], [`video_render`], [`video_play`], [`video_hls_open_variant`], [`subtitle_load_srt`] |
//! | **Media capture** | [`camera_open`], [`camera_capture_frame`], [`microphone_open`], [`microphone_read_samples`], [`screen_capture`] |
//! | **WebRTC** | [`rtc_create_peer`], [`rtc_create_offer`], [`rtc_create_answer`], [`rtc_create_data_channel`], [`rtc_send`], [`rtc_recv`], [`rtc_signal_connect`] |
//! | **WebSocket** | [`ws_connect`], [`ws_send_text`], [`ws_send_binary`], [`ws_recv`], [`ws_ready_state`], [`ws_close`], [`ws_remove`] |
//! | **MIDI** | [`midi_input_count`], [`midi_output_count`], [`midi_input_name`], [`midi_output_name`], [`midi_open_input`], [`midi_open_output`], [`midi_send`], [`midi_recv`], [`midi_close`] |
//! | **Timers** | [`set_timeout`], [`set_interval`], [`clear_timer`], [`request_animation_frame`], [`cancel_animation_frame`], [`time_now_ms`] |
//! | **Events** | [`on_event`], [`off_event`], [`emit_event`], [`event_type`], [`event_data`], [`event_data_into`] |
//! | **Navigation** | [`navigate`], [`push_state`], [`replace_state`], [`get_url`], [`history_back`], [`history_forward`] |
//! | **Input** | [`mouse_position`], [`mouse_button_down`], [`mouse_button_clicked`], [`key_down`], [`key_pressed`], [`scroll_delta`], [`modifiers`] |
//! | **Widgets** | [`ui_button`], [`ui_checkbox`], [`ui_slider`], [`ui_text_input`] |
//! | **Crypto** | [`hash_sha256`], [`hash_sha256_hex`], [`base64_encode`], [`base64_decode`] |
//! | **Other** | [`clipboard_write`], [`clipboard_read`], [`random_u64`], [`random_f64`], [`notify`], [`upload_file`], [`load_module`] |
//!
//! ## Guest Module Contract
//!
//! Every `.wasm` module loaded by Oxide must:
//!
//! 1. **Export `start_app`** — `extern "C" fn()` entry point, called once on load.
//! 2. **Optionally export `on_frame`** — `extern "C" fn(dt_ms: u32)` for
//!    interactive apps with a render loop (called every frame, fuel replenished).
//! 3. **Optionally export `on_timer`** — `extern "C" fn(callback_id: u32)`
//!    to receive callbacks from [`set_timeout`], [`set_interval`], and [`request_animation_frame`].
//! 4. **Optionally export `on_event`** — `extern "C" fn(callback_id: u32)`
//!    to receive built-in (`resize`, `focus`, `touch_*`, `gamepad_*`, `drop_files`, …)
//!    and custom events registered via [`on_event`] / [`emit_event`].
//! 5. **Compile as `cdylib`** — `crate-type = ["cdylib"]` in `Cargo.toml`.
//! 6. **Target `wasm32-unknown-unknown`** — no WASI, pure capability-based I/O.
//!
//! ## Full API Documentation
//!
//! See <https://docs.oxide.foundation/oxide_sdk/> for the complete API
//! reference, or browse the individual function documentation below.

pub mod draw;
pub mod proto;

// ─── Raw FFI imports from the host ──────────────────────────────────────────

#[link(wasm_import_module = "oxide")]
extern "C" {
    #[link_name = "api_log"]
    fn _api_log(ptr: u32, len: u32);

    #[link_name = "api_warn"]
    fn _api_warn(ptr: u32, len: u32);

    #[link_name = "api_error"]
    fn _api_error(ptr: u32, len: u32);

    #[link_name = "api_get_location"]
    fn _api_get_location(out_ptr: u32, out_cap: u32) -> u32;

    #[link_name = "api_upload_file"]
    fn _api_upload_file(name_ptr: u32, name_cap: u32, data_ptr: u32, data_cap: u32) -> u64;

    #[link_name = "api_file_pick"]
    fn _api_file_pick(
        title_ptr: u32,
        title_len: u32,
        filters_ptr: u32,
        filters_len: u32,
        multiple: u32,
        out_ptr: u32,
        out_cap: u32,
    ) -> i32;

    #[link_name = "api_folder_pick"]
    fn _api_folder_pick(title_ptr: u32, title_len: u32) -> u32;

    #[link_name = "api_folder_entries"]
    fn _api_folder_entries(handle: u32, out_ptr: u32, out_cap: u32) -> i32;

    #[link_name = "api_file_read"]
    fn _api_file_read(handle: u32, out_ptr: u32, out_cap: u32) -> i64;

    #[link_name = "api_file_read_range"]
    fn _api_file_read_range(
        handle: u32,
        offset_lo: u32,
        offset_hi: u32,
        len: u32,
        out_ptr: u32,
        out_cap: u32,
    ) -> i64;

    #[link_name = "api_file_metadata"]
    fn _api_file_metadata(handle: u32, out_ptr: u32, out_cap: u32) -> i32;

    #[link_name = "api_canvas_clear"]
    fn _api_canvas_clear(r: u32, g: u32, b: u32, a: u32);

    #[link_name = "api_canvas_rect"]
    fn _api_canvas_rect(x: f32, y: f32, w: f32, h: f32, r: u32, g: u32, b: u32, a: u32);

    #[link_name = "api_canvas_circle"]
    fn _api_canvas_circle(cx: f32, cy: f32, radius: f32, r: u32, g: u32, b: u32, a: u32);

    #[link_name = "api_canvas_text"]
    fn _api_canvas_text(
        x: f32,
        y: f32,
        size: f32,
        r: u32,
        g: u32,
        b: u32,
        a: u32,
        ptr: u32,
        len: u32,
    );

    #[link_name = "api_canvas_line"]
    fn _api_canvas_line(
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        r: u32,
        g: u32,
        b: u32,
        a: u32,
        thickness: f32,
    );

    #[link_name = "api_canvas_dimensions"]
    fn _api_canvas_dimensions() -> u64;

    #[link_name = "api_canvas_image"]
    fn _api_canvas_image(x: f32, y: f32, w: f32, h: f32, data_ptr: u32, data_len: u32);

    // ── Extended Shape Primitives ──────────────────────────────────

    #[link_name = "api_canvas_rounded_rect"]
    fn _api_canvas_rounded_rect(
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        r: u32,
        g: u32,
        b: u32,
        a: u32,
    );

    #[link_name = "api_canvas_arc"]
    fn _api_canvas_arc(
        cx: f32,
        cy: f32,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        r: u32,
        g: u32,
        b: u32,
        a: u32,
        thickness: f32,
    );

    #[link_name = "api_canvas_bezier"]
    fn _api_canvas_bezier(
        x1: f32,
        y1: f32,
        cp1x: f32,
        cp1y: f32,
        cp2x: f32,
        cp2y: f32,
        x2: f32,
        y2: f32,
        r: u32,
        g: u32,
        b: u32,
        a: u32,
        thickness: f32,
    );

    #[link_name = "api_canvas_gradient"]
    fn _api_canvas_gradient(
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        kind: u32,
        ax: f32,
        ay: f32,
        bx: f32,
        by: f32,
        stops_ptr: u32,
        stops_len: u32,
    );

    // ── Canvas State (transform / clip / opacity) ─────────────────

    #[link_name = "api_canvas_save"]
    fn _api_canvas_save();

    #[link_name = "api_canvas_restore"]
    fn _api_canvas_restore();

    #[link_name = "api_canvas_transform"]
    fn _api_canvas_transform(a: f32, b: f32, c: f32, d: f32, tx: f32, ty: f32);

    #[link_name = "api_canvas_clip"]
    fn _api_canvas_clip(x: f32, y: f32, w: f32, h: f32);

    #[link_name = "api_canvas_opacity"]
    fn _api_canvas_opacity(alpha: f32);

    #[link_name = "api_storage_set"]
    fn _api_storage_set(key_ptr: u32, key_len: u32, val_ptr: u32, val_len: u32);

    #[link_name = "api_storage_get"]
    fn _api_storage_get(key_ptr: u32, key_len: u32, out_ptr: u32, out_cap: u32) -> u32;

    #[link_name = "api_storage_remove"]
    fn _api_storage_remove(key_ptr: u32, key_len: u32);

    #[link_name = "api_clipboard_write"]
    fn _api_clipboard_write(ptr: u32, len: u32);

    #[link_name = "api_clipboard_read"]
    fn _api_clipboard_read(out_ptr: u32, out_cap: u32) -> u32;

    #[link_name = "api_time_now_ms"]
    fn _api_time_now_ms() -> u64;

    #[link_name = "api_set_timeout"]
    fn _api_set_timeout(callback_id: u32, delay_ms: u32) -> u32;

    #[link_name = "api_set_interval"]
    fn _api_set_interval(callback_id: u32, interval_ms: u32) -> u32;

    #[link_name = "api_clear_timer"]
    fn _api_clear_timer(timer_id: u32);

    #[link_name = "api_request_animation_frame"]
    fn _api_request_animation_frame(callback_id: u32) -> u32;

    #[link_name = "api_cancel_animation_frame"]
    fn _api_cancel_animation_frame(request_id: u32);

    #[link_name = "api_on_event"]
    fn _api_on_event(type_ptr: u32, type_len: u32, callback_id: u32) -> u32;

    #[link_name = "api_off_event"]
    fn _api_off_event(listener_id: u32) -> u32;

    #[link_name = "api_emit_event"]
    fn _api_emit_event(type_ptr: u32, type_len: u32, data_ptr: u32, data_len: u32);

    #[link_name = "api_event_type_len"]
    fn _api_event_type_len() -> u32;

    #[link_name = "api_event_type_read"]
    fn _api_event_type_read(out_ptr: u32, out_cap: u32) -> u32;

    #[link_name = "api_event_data_len"]
    fn _api_event_data_len() -> u32;

    #[link_name = "api_event_data_read"]
    fn _api_event_data_read(out_ptr: u32, out_cap: u32) -> u32;

    #[link_name = "api_random"]
    fn _api_random() -> u64;

    #[link_name = "api_notify"]
    fn _api_notify(title_ptr: u32, title_len: u32, body_ptr: u32, body_len: u32);

    #[link_name = "api_fetch"]
    fn _api_fetch(
        method_ptr: u32,
        method_len: u32,
        url_ptr: u32,
        url_len: u32,
        ct_ptr: u32,
        ct_len: u32,
        body_ptr: u32,
        body_len: u32,
        out_ptr: u32,
        out_cap: u32,
    ) -> i64;

    #[link_name = "api_fetch_begin"]
    fn _api_fetch_begin(
        method_ptr: u32,
        method_len: u32,
        url_ptr: u32,
        url_len: u32,
        ct_ptr: u32,
        ct_len: u32,
        body_ptr: u32,
        body_len: u32,
    ) -> u32;

    #[link_name = "api_fetch_state"]
    fn _api_fetch_state(id: u32) -> u32;

    #[link_name = "api_fetch_status"]
    fn _api_fetch_status(id: u32) -> u32;

    #[link_name = "api_fetch_recv"]
    fn _api_fetch_recv(id: u32, out_ptr: u32, out_cap: u32) -> i64;

    #[link_name = "api_fetch_error"]
    fn _api_fetch_error(id: u32, out_ptr: u32, out_cap: u32) -> i32;

    #[link_name = "api_fetch_abort"]
    fn _api_fetch_abort(id: u32) -> i32;

    #[link_name = "api_fetch_remove"]
    fn _api_fetch_remove(id: u32);

    #[link_name = "api_load_module"]
    fn _api_load_module(url_ptr: u32, url_len: u32) -> i32;

    #[link_name = "api_hash_sha256"]
    fn _api_hash_sha256(data_ptr: u32, data_len: u32, out_ptr: u32) -> u32;

    #[link_name = "api_base64_encode"]
    fn _api_base64_encode(data_ptr: u32, data_len: u32, out_ptr: u32, out_cap: u32) -> u32;

    #[link_name = "api_base64_decode"]
    fn _api_base64_decode(data_ptr: u32, data_len: u32, out_ptr: u32, out_cap: u32) -> u32;

    #[link_name = "api_kv_store_set"]
    fn _api_kv_store_set(key_ptr: u32, key_len: u32, val_ptr: u32, val_len: u32) -> i32;

    #[link_name = "api_kv_store_get"]
    fn _api_kv_store_get(key_ptr: u32, key_len: u32, out_ptr: u32, out_cap: u32) -> i32;

    #[link_name = "api_kv_store_delete"]
    fn _api_kv_store_delete(key_ptr: u32, key_len: u32) -> i32;

    // ── Navigation ──────────────────────────────────────────────────

    #[link_name = "api_navigate"]
    fn _api_navigate(url_ptr: u32, url_len: u32) -> i32;

    #[link_name = "api_push_state"]
    fn _api_push_state(
        state_ptr: u32,
        state_len: u32,
        title_ptr: u32,
        title_len: u32,
        url_ptr: u32,
        url_len: u32,
    );

    #[link_name = "api_replace_state"]
    fn _api_replace_state(
        state_ptr: u32,
        state_len: u32,
        title_ptr: u32,
        title_len: u32,
        url_ptr: u32,
        url_len: u32,
    );

    #[link_name = "api_get_url"]
    fn _api_get_url(out_ptr: u32, out_cap: u32) -> u32;

    #[link_name = "api_get_state"]
    fn _api_get_state(out_ptr: u32, out_cap: u32) -> i32;

    #[link_name = "api_history_length"]
    fn _api_history_length() -> u32;

    #[link_name = "api_history_back"]
    fn _api_history_back() -> i32;

    #[link_name = "api_history_forward"]
    fn _api_history_forward() -> i32;

    // ── Hyperlinks ──────────────────────────────────────────────────

    #[link_name = "api_register_hyperlink"]
    fn _api_register_hyperlink(x: f32, y: f32, w: f32, h: f32, url_ptr: u32, url_len: u32) -> i32;

    #[link_name = "api_clear_hyperlinks"]
    fn _api_clear_hyperlinks();

    // ── Input Polling ────────────────────────────────────────────────

    #[link_name = "api_mouse_position"]
    fn _api_mouse_position() -> u64;

    #[link_name = "api_mouse_button_down"]
    fn _api_mouse_button_down(button: u32) -> u32;

    #[link_name = "api_mouse_button_clicked"]
    fn _api_mouse_button_clicked(button: u32) -> u32;

    #[link_name = "api_key_down"]
    fn _api_key_down(key: u32) -> u32;

    #[link_name = "api_key_pressed"]
    fn _api_key_pressed(key: u32) -> u32;

    #[link_name = "api_scroll_delta"]
    fn _api_scroll_delta() -> u64;

    #[link_name = "api_modifiers"]
    fn _api_modifiers() -> u32;

    // ── Interactive Widgets ─────────────────────────────────────────

    #[link_name = "api_ui_button"]
    fn _api_ui_button(
        id: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        label_ptr: u32,
        label_len: u32,
    ) -> u32;

    #[link_name = "api_ui_checkbox"]
    fn _api_ui_checkbox(
        id: u32,
        x: f32,
        y: f32,
        label_ptr: u32,
        label_len: u32,
        initial: u32,
    ) -> u32;

    #[link_name = "api_ui_slider"]
    fn _api_ui_slider(id: u32, x: f32, y: f32, w: f32, min: f32, max: f32, initial: f32) -> f32;

    #[link_name = "api_ui_text_input"]
    fn _api_ui_text_input(
        id: u32,
        x: f32,
        y: f32,
        w: f32,
        init_ptr: u32,
        init_len: u32,
        out_ptr: u32,
        out_cap: u32,
    ) -> u32;

    // ── Audio Playback ──────────────────────────────────────────────

    #[link_name = "api_audio_play"]
    fn _api_audio_play(data_ptr: u32, data_len: u32) -> i32;

    #[link_name = "api_audio_play_url"]
    fn _api_audio_play_url(url_ptr: u32, url_len: u32) -> i32;

    #[link_name = "api_audio_detect_format"]
    fn _api_audio_detect_format(data_ptr: u32, data_len: u32) -> u32;

    #[link_name = "api_audio_play_with_format"]
    fn _api_audio_play_with_format(data_ptr: u32, data_len: u32, format_hint: u32) -> i32;

    #[link_name = "api_audio_last_url_content_type"]
    fn _api_audio_last_url_content_type(out_ptr: u32, out_cap: u32) -> u32;

    #[link_name = "api_audio_pause"]
    fn _api_audio_pause();

    #[link_name = "api_audio_resume"]
    fn _api_audio_resume();

    #[link_name = "api_audio_stop"]
    fn _api_audio_stop();

    #[link_name = "api_audio_set_volume"]
    fn _api_audio_set_volume(level: f32);

    #[link_name = "api_audio_get_volume"]
    fn _api_audio_get_volume() -> f32;

    #[link_name = "api_audio_is_playing"]
    fn _api_audio_is_playing() -> u32;

    #[link_name = "api_audio_position"]
    fn _api_audio_position() -> u64;

    #[link_name = "api_audio_seek"]
    fn _api_audio_seek(position_ms: u64) -> i32;

    #[link_name = "api_audio_duration"]
    fn _api_audio_duration() -> u64;

    #[link_name = "api_audio_set_loop"]
    fn _api_audio_set_loop(enabled: u32);

    #[link_name = "api_audio_channel_play"]
    fn _api_audio_channel_play(channel: u32, data_ptr: u32, data_len: u32) -> i32;

    #[link_name = "api_audio_channel_play_with_format"]
    fn _api_audio_channel_play_with_format(
        channel: u32,
        data_ptr: u32,
        data_len: u32,
        format_hint: u32,
    ) -> i32;

    #[link_name = "api_audio_channel_stop"]
    fn _api_audio_channel_stop(channel: u32);

    #[link_name = "api_audio_channel_set_volume"]
    fn _api_audio_channel_set_volume(channel: u32, level: f32);

    // ── Video ─────────────────────────────────────────────────────────

    #[link_name = "api_video_detect_format"]
    fn _api_video_detect_format(data_ptr: u32, data_len: u32) -> u32;

    #[link_name = "api_video_load"]
    fn _api_video_load(data_ptr: u32, data_len: u32, format_hint: u32) -> i32;

    #[link_name = "api_video_load_url"]
    fn _api_video_load_url(url_ptr: u32, url_len: u32) -> i32;

    #[link_name = "api_video_last_url_content_type"]
    fn _api_video_last_url_content_type(out_ptr: u32, out_cap: u32) -> u32;

    #[link_name = "api_video_hls_variant_count"]
    fn _api_video_hls_variant_count() -> u32;

    #[link_name = "api_video_hls_variant_url"]
    fn _api_video_hls_variant_url(index: u32, out_ptr: u32, out_cap: u32) -> u32;

    #[link_name = "api_video_hls_open_variant"]
    fn _api_video_hls_open_variant(index: u32) -> i32;

    #[link_name = "api_video_play"]
    fn _api_video_play();

    #[link_name = "api_video_pause"]
    fn _api_video_pause();

    #[link_name = "api_video_stop"]
    fn _api_video_stop();

    #[link_name = "api_video_seek"]
    fn _api_video_seek(position_ms: u64) -> i32;

    #[link_name = "api_video_position"]
    fn _api_video_position() -> u64;

    #[link_name = "api_video_duration"]
    fn _api_video_duration() -> u64;

    #[link_name = "api_video_render"]
    fn _api_video_render(x: f32, y: f32, w: f32, h: f32) -> i32;

    #[link_name = "api_video_set_volume"]
    fn _api_video_set_volume(level: f32);

    #[link_name = "api_video_get_volume"]
    fn _api_video_get_volume() -> f32;

    #[link_name = "api_video_set_loop"]
    fn _api_video_set_loop(enabled: u32);

    #[link_name = "api_video_set_pip"]
    fn _api_video_set_pip(enabled: u32);

    #[link_name = "api_subtitle_load_srt"]
    fn _api_subtitle_load_srt(ptr: u32, len: u32) -> i32;

    #[link_name = "api_subtitle_load_vtt"]
    fn _api_subtitle_load_vtt(ptr: u32, len: u32) -> i32;

    #[link_name = "api_subtitle_clear"]
    fn _api_subtitle_clear();

    // ── Media capture ─────────────────────────────────────────────────

    #[link_name = "api_camera_open"]
    fn _api_camera_open() -> i32;

    #[link_name = "api_camera_close"]
    fn _api_camera_close();

    #[link_name = "api_camera_capture_frame"]
    fn _api_camera_capture_frame(out_ptr: u32, out_cap: u32) -> u32;

    #[link_name = "api_camera_frame_dimensions"]
    fn _api_camera_frame_dimensions() -> u64;

    #[link_name = "api_microphone_open"]
    fn _api_microphone_open() -> i32;

    #[link_name = "api_microphone_close"]
    fn _api_microphone_close();

    #[link_name = "api_microphone_sample_rate"]
    fn _api_microphone_sample_rate() -> u32;

    #[link_name = "api_microphone_read_samples"]
    fn _api_microphone_read_samples(out_ptr: u32, max_samples: u32) -> u32;

    #[link_name = "api_screen_capture"]
    fn _api_screen_capture(out_ptr: u32, out_cap: u32) -> i32;

    #[link_name = "api_screen_capture_dimensions"]
    fn _api_screen_capture_dimensions() -> u64;

    #[link_name = "api_media_pipeline_stats"]
    fn _api_media_pipeline_stats() -> u64;

    // ── GPU / WebGPU-style API ────────────────────────────────────

    #[link_name = "api_gpu_create_buffer"]
    fn _api_gpu_create_buffer(size_lo: u32, size_hi: u32, usage: u32) -> u32;

    #[link_name = "api_gpu_create_texture"]
    fn _api_gpu_create_texture(width: u32, height: u32) -> u32;

    #[link_name = "api_gpu_create_shader"]
    fn _api_gpu_create_shader(src_ptr: u32, src_len: u32) -> u32;

    #[link_name = "api_gpu_create_render_pipeline"]
    fn _api_gpu_create_render_pipeline(
        shader: u32,
        vs_ptr: u32,
        vs_len: u32,
        fs_ptr: u32,
        fs_len: u32,
    ) -> u32;

    #[link_name = "api_gpu_create_compute_pipeline"]
    fn _api_gpu_create_compute_pipeline(shader: u32, ep_ptr: u32, ep_len: u32) -> u32;

    #[link_name = "api_gpu_write_buffer"]
    fn _api_gpu_write_buffer(
        handle: u32,
        offset_lo: u32,
        offset_hi: u32,
        data_ptr: u32,
        data_len: u32,
    ) -> u32;

    #[link_name = "api_gpu_draw"]
    fn _api_gpu_draw(pipeline: u32, target: u32, vertex_count: u32, instance_count: u32) -> u32;

    #[link_name = "api_gpu_dispatch_compute"]
    fn _api_gpu_dispatch_compute(pipeline: u32, x: u32, y: u32, z: u32) -> u32;

    #[link_name = "api_gpu_destroy_buffer"]
    fn _api_gpu_destroy_buffer(handle: u32) -> u32;

    #[link_name = "api_gpu_destroy_texture"]
    fn _api_gpu_destroy_texture(handle: u32) -> u32;

    // ── WebRTC / Real-Time Communication ─────────────────────────

    #[link_name = "api_rtc_create_peer"]
    fn _api_rtc_create_peer(stun_ptr: u32, stun_len: u32) -> u32;

    #[link_name = "api_rtc_close_peer"]
    fn _api_rtc_close_peer(peer_id: u32) -> u32;

    #[link_name = "api_rtc_create_offer"]
    fn _api_rtc_create_offer(peer_id: u32, out_ptr: u32, out_cap: u32) -> i32;

    #[link_name = "api_rtc_create_answer"]
    fn _api_rtc_create_answer(peer_id: u32, out_ptr: u32, out_cap: u32) -> i32;

    #[link_name = "api_rtc_set_local_description"]
    fn _api_rtc_set_local_description(
        peer_id: u32,
        sdp_ptr: u32,
        sdp_len: u32,
        is_offer: u32,
    ) -> i32;

    #[link_name = "api_rtc_set_remote_description"]
    fn _api_rtc_set_remote_description(
        peer_id: u32,
        sdp_ptr: u32,
        sdp_len: u32,
        is_offer: u32,
    ) -> i32;

    #[link_name = "api_rtc_add_ice_candidate"]
    fn _api_rtc_add_ice_candidate(peer_id: u32, cand_ptr: u32, cand_len: u32) -> i32;

    #[link_name = "api_rtc_connection_state"]
    fn _api_rtc_connection_state(peer_id: u32) -> u32;

    #[link_name = "api_rtc_poll_ice_candidate"]
    fn _api_rtc_poll_ice_candidate(peer_id: u32, out_ptr: u32, out_cap: u32) -> i32;

    #[link_name = "api_rtc_create_data_channel"]
    fn _api_rtc_create_data_channel(
        peer_id: u32,
        label_ptr: u32,
        label_len: u32,
        ordered: u32,
    ) -> u32;

    #[link_name = "api_rtc_send"]
    fn _api_rtc_send(
        peer_id: u32,
        channel_id: u32,
        data_ptr: u32,
        data_len: u32,
        is_binary: u32,
    ) -> i32;

    #[link_name = "api_rtc_recv"]
    fn _api_rtc_recv(peer_id: u32, channel_id: u32, out_ptr: u32, out_cap: u32) -> i64;

    #[link_name = "api_rtc_poll_data_channel"]
    fn _api_rtc_poll_data_channel(peer_id: u32, out_ptr: u32, out_cap: u32) -> i32;

    #[link_name = "api_rtc_add_track"]
    fn _api_rtc_add_track(peer_id: u32, kind: u32) -> u32;

    #[link_name = "api_rtc_poll_track"]
    fn _api_rtc_poll_track(peer_id: u32, out_ptr: u32, out_cap: u32) -> i32;

    #[link_name = "api_rtc_signal_connect"]
    fn _api_rtc_signal_connect(url_ptr: u32, url_len: u32) -> u32;

    #[link_name = "api_rtc_signal_join_room"]
    fn _api_rtc_signal_join_room(room_ptr: u32, room_len: u32) -> i32;

    #[link_name = "api_rtc_signal_send"]
    fn _api_rtc_signal_send(data_ptr: u32, data_len: u32) -> i32;

    #[link_name = "api_rtc_signal_recv"]
    fn _api_rtc_signal_recv(out_ptr: u32, out_cap: u32) -> i32;

    // ── WebSocket API ────────────────────────────────────────────────

    #[link_name = "api_ws_connect"]
    fn _api_ws_connect(url_ptr: u32, url_len: u32) -> u32;

    #[link_name = "api_ws_send_text"]
    fn _api_ws_send_text(id: u32, data_ptr: u32, data_len: u32) -> i32;

    #[link_name = "api_ws_send_binary"]
    fn _api_ws_send_binary(id: u32, data_ptr: u32, data_len: u32) -> i32;

    #[link_name = "api_ws_recv"]
    fn _api_ws_recv(id: u32, out_ptr: u32, out_cap: u32) -> i64;

    #[link_name = "api_ws_ready_state"]
    fn _api_ws_ready_state(id: u32) -> u32;

    #[link_name = "api_ws_close"]
    fn _api_ws_close(id: u32) -> i32;

    #[link_name = "api_ws_remove"]
    fn _api_ws_remove(id: u32);

    // ── MIDI API ────────────────────────────────────────────────────

    #[link_name = "api_midi_input_count"]
    fn _api_midi_input_count() -> u32;

    #[link_name = "api_midi_output_count"]
    fn _api_midi_output_count() -> u32;

    #[link_name = "api_midi_input_name"]
    fn _api_midi_input_name(index: u32, out_ptr: u32, out_cap: u32) -> u32;

    #[link_name = "api_midi_output_name"]
    fn _api_midi_output_name(index: u32, out_ptr: u32, out_cap: u32) -> u32;

    #[link_name = "api_midi_open_input"]
    fn _api_midi_open_input(index: u32) -> u32;

    #[link_name = "api_midi_open_output"]
    fn _api_midi_open_output(index: u32) -> u32;

    #[link_name = "api_midi_send"]
    fn _api_midi_send(handle: u32, data_ptr: u32, data_len: u32) -> i32;

    #[link_name = "api_midi_recv"]
    fn _api_midi_recv(handle: u32, out_ptr: u32, out_cap: u32) -> i32;

    #[link_name = "api_midi_close"]
    fn _api_midi_close(handle: u32);

    // ── URL Utilities ───────────────────────────────────────────────

    #[link_name = "api_url_resolve"]
    fn _api_url_resolve(
        base_ptr: u32,
        base_len: u32,
        rel_ptr: u32,
        rel_len: u32,
        out_ptr: u32,
        out_cap: u32,
    ) -> i32;

    #[link_name = "api_url_encode"]
    fn _api_url_encode(input_ptr: u32, input_len: u32, out_ptr: u32, out_cap: u32) -> u32;

    #[link_name = "api_url_decode"]
    fn _api_url_decode(input_ptr: u32, input_len: u32, out_ptr: u32, out_cap: u32) -> u32;
}

// ─── Console API ────────────────────────────────────────────────────────────

/// Print a message to the browser console (log level).
pub fn log(msg: &str) {
    unsafe { _api_log(msg.as_ptr() as u32, msg.len() as u32) }
}

/// Print a warning to the browser console.
pub fn warn(msg: &str) {
    unsafe { _api_warn(msg.as_ptr() as u32, msg.len() as u32) }
}

/// Print an error to the browser console.
pub fn error(msg: &str) {
    unsafe { _api_error(msg.as_ptr() as u32, msg.len() as u32) }
}

// ─── Geolocation API ────────────────────────────────────────────────────────

/// Get the device's mock geolocation as a `"lat,lon"` string.
pub fn get_location() -> String {
    let mut buf = [0u8; 128];
    let len = unsafe { _api_get_location(buf.as_mut_ptr() as u32, buf.len() as u32) };
    String::from_utf8_lossy(&buf[..len as usize]).to_string()
}

// ─── File Upload API ────────────────────────────────────────────────────────

/// File returned from the native file picker.
pub struct UploadedFile {
    pub name: String,
    pub data: Vec<u8>,
}

/// Opens the native OS file picker and returns the selected file.
/// Returns `None` if the user cancels.
pub fn upload_file() -> Option<UploadedFile> {
    let mut name_buf = [0u8; 256];
    let mut data_buf = vec![0u8; 1024 * 1024]; // 1MB max

    let result = unsafe {
        _api_upload_file(
            name_buf.as_mut_ptr() as u32,
            name_buf.len() as u32,
            data_buf.as_mut_ptr() as u32,
            data_buf.len() as u32,
        )
    };

    if result == 0 {
        return None;
    }

    let name_len = (result >> 32) as usize;
    let data_len = (result & 0xFFFF_FFFF) as usize;

    Some(UploadedFile {
        name: String::from_utf8_lossy(&name_buf[..name_len]).to_string(),
        data: data_buf[..data_len].to_vec(),
    })
}

// ─── File / Folder Picker API ───────────────────────────────────────────────
//
// Handle-based picker. Paths never cross the sandbox boundary — the host
// keeps a `HashMap<handle, PathBuf>` and returns opaque `u32` handles.
// Use [`file_read`] / [`file_read_range`] / [`file_metadata`] with the
// handle; [`folder_entries`] lists a picked directory.

/// Metadata returned by [`file_metadata`], parsed from the host's JSON reply.
pub struct FileMetadata {
    pub name: String,
    pub size: u64,
    pub mime: String,
    pub modified_ms: u64,
    pub is_dir: bool,
}

/// One child returned by [`folder_entries`].
pub struct FolderEntry {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub handle: u32,
}

/// Open the native file picker and return the selected file handles.
///
/// `filters` is a comma-separated list of extensions (e.g. `"png,jpg,gif"`);
/// pass `""` to allow any file. Set `multiple = true` for multi-select.
/// Returns an empty `Vec` if the user cancels.
pub fn file_pick(title: &str, filters: &str, multiple: bool) -> Vec<u32> {
    let mut buf = [0u32; 64];
    let n = unsafe {
        _api_file_pick(
            title.as_ptr() as u32,
            title.len() as u32,
            filters.as_ptr() as u32,
            filters.len() as u32,
            if multiple { 1 } else { 0 },
            buf.as_mut_ptr() as u32,
            (buf.len() * 4) as u32,
        )
    };
    if n <= 0 {
        return Vec::new();
    }
    buf[..n as usize].to_vec()
}

/// Open the native folder picker and return a directory handle.
///
/// Returns `None` if the user cancels. Use [`folder_entries`] to list the
/// selected directory.
pub fn folder_pick(title: &str) -> Option<u32> {
    let h = unsafe { _api_folder_pick(title.as_ptr() as u32, title.len() as u32) };
    if h == 0 {
        None
    } else {
        Some(h)
    }
}

fn read_json_len(handle: u32, call: impl Fn(u32, u32, u32) -> i32) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; 8 * 1024];
    let n = call(handle, buf.as_mut_ptr() as u32, buf.len() as u32);
    if n >= 0 {
        buf.truncate(n as usize);
        return Some(buf);
    }
    // Negative magnitude: required size. Retry once with the exact capacity.
    if n < -1 {
        let required = (-n) as usize;
        let mut big = vec![0u8; required];
        let n2 = call(handle, big.as_mut_ptr() as u32, big.len() as u32);
        if n2 >= 0 {
            big.truncate(n2 as usize);
            return Some(big);
        }
    }
    None
}

/// List the children of a picked folder handle.
///
/// Each returned entry includes a fresh sub-handle that can be passed to
/// [`file_read`], [`file_read_range`], or [`file_metadata`] (or recursively
/// to `folder_entries` for directories).
pub fn folder_entries(handle: u32) -> Vec<FolderEntry> {
    let bytes = match read_json_len(handle, |h, p, c| unsafe { _api_folder_entries(h, p, c) }) {
        Some(b) => b,
        None => return Vec::new(),
    };
    parse_folder_entries(&bytes)
}

fn parse_folder_entries(bytes: &[u8]) -> Vec<FolderEntry> {
    // Minimal hand-rolled parser: the host emits a strict, flat JSON array
    // with the four fields in a fixed order. Avoids pulling in serde_json.
    let s = core::str::from_utf8(bytes).unwrap_or("");
    let mut out = Vec::new();
    let mut rest = s.trim();
    if !rest.starts_with('[') {
        return out;
    }
    rest = &rest[1..];
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
        if rest.starts_with(']') || rest.is_empty() {
            break;
        }
        let Some(end) = rest.find('}') else { break };
        let obj = &rest[..=end];
        rest = &rest[end + 1..];
        let name = json_str_field(obj, "\"name\":").unwrap_or_default();
        let size = json_num_field(obj, "\"size\":").unwrap_or(0);
        let is_dir = json_bool_field(obj, "\"is_dir\":").unwrap_or(false);
        let handle = json_num_field(obj, "\"handle\":").unwrap_or(0) as u32;
        out.push(FolderEntry {
            name,
            size,
            is_dir,
            handle,
        });
    }
    out
}

fn json_str_field(obj: &str, key: &str) -> Option<String> {
    let idx = obj.find(key)?;
    let after = &obj[idx + key.len()..];
    let start = after.find('"')? + 1;
    let mut out = String::new();
    let bytes = after.as_bytes();
    let mut i = start;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'n' => out.push('\n'),
                b'r' => out.push('\r'),
                b't' => out.push('\t'),
                _ => out.push(bytes[i + 1] as char),
            }
            i += 2;
        } else if c == b'"' {
            return Some(out);
        } else {
            out.push(c as char);
            i += 1;
        }
    }
    None
}

fn json_num_field(obj: &str, key: &str) -> Option<u64> {
    let idx = obj.find(key)?;
    let after = obj[idx + key.len()..].trim_start();
    let end = after
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after.len());
    after[..end].parse().ok()
}

fn json_bool_field(obj: &str, key: &str) -> Option<bool> {
    let idx = obj.find(key)?;
    let after = obj[idx + key.len()..].trim_start();
    if after.starts_with("true") {
        Some(true)
    } else if after.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

/// Read the full contents of a picked file.
///
/// Returns `None` if the handle is unknown, the file cannot be read, or the
/// file is larger than 64 MiB (the wrapper's retry cap).
pub fn file_read(handle: u32) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; 64 * 1024];
    let n = unsafe { _api_file_read(handle, buf.as_mut_ptr() as u32, buf.len() as u32) };
    if n >= 0 {
        buf.truncate(n as usize);
        return Some(buf);
    }
    if n < -1 {
        let required = (-n) as usize;
        if required > 64 * 1024 * 1024 {
            return None;
        }
        let mut big = vec![0u8; required];
        let n2 = unsafe { _api_file_read(handle, big.as_mut_ptr() as u32, big.len() as u32) };
        if n2 >= 0 {
            big.truncate(n2 as usize);
            return Some(big);
        }
    }
    None
}

/// Read `len` bytes from `offset` of a picked file.
///
/// Returns the bytes actually read (may be shorter than `len` at EOF).
/// `None` indicates an invalid handle or I/O error.
pub fn file_read_range(handle: u32, offset: u64, len: u32) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; len as usize];
    let n = unsafe {
        _api_file_read_range(
            handle,
            offset as u32,
            (offset >> 32) as u32,
            len,
            buf.as_mut_ptr() as u32,
            buf.len() as u32,
        )
    };
    if n < 0 {
        return None;
    }
    buf.truncate(n as usize);
    Some(buf)
}

/// Inspect a picked file or folder: name, size, MIME type, last-modified.
pub fn file_metadata(handle: u32) -> Option<FileMetadata> {
    let bytes = read_json_len(handle, |h, p, c| unsafe { _api_file_metadata(h, p, c) })?;
    let s = core::str::from_utf8(&bytes).ok()?;
    Some(FileMetadata {
        name: json_str_field(s, "\"name\":").unwrap_or_default(),
        size: json_num_field(s, "\"size\":").unwrap_or(0),
        mime: json_str_field(s, "\"mime\":").unwrap_or_default(),
        modified_ms: json_num_field(s, "\"modified_ms\":").unwrap_or(0),
        is_dir: json_bool_field(s, "\"is_dir\":").unwrap_or(false),
    })
}

// ─── Canvas API ─────────────────────────────────────────────────────────────

/// Clear the canvas with a solid RGBA color.
pub fn canvas_clear(r: u8, g: u8, b: u8, a: u8) {
    unsafe { _api_canvas_clear(r as u32, g as u32, b as u32, a as u32) }
}

/// Draw a filled rectangle.
pub fn canvas_rect(x: f32, y: f32, w: f32, h: f32, r: u8, g: u8, b: u8, a: u8) {
    unsafe { _api_canvas_rect(x, y, w, h, r as u32, g as u32, b as u32, a as u32) }
}

/// Draw a filled circle.
pub fn canvas_circle(cx: f32, cy: f32, radius: f32, r: u8, g: u8, b: u8, a: u8) {
    unsafe { _api_canvas_circle(cx, cy, radius, r as u32, g as u32, b as u32, a as u32) }
}

/// Draw text on the canvas with RGBA color.
pub fn canvas_text(x: f32, y: f32, size: f32, r: u8, g: u8, b: u8, a: u8, text: &str) {
    unsafe {
        _api_canvas_text(
            x,
            y,
            size,
            r as u32,
            g as u32,
            b as u32,
            a as u32,
            text.as_ptr() as u32,
            text.len() as u32,
        )
    }
}

/// Draw a line between two points with RGBA color.
pub fn canvas_line(x1: f32, y1: f32, x2: f32, y2: f32, r: u8, g: u8, b: u8, a: u8, thickness: f32) {
    unsafe {
        _api_canvas_line(
            x1, y1, x2, y2, r as u32, g as u32, b as u32, a as u32, thickness,
        )
    }
}

/// Returns `(width, height)` of the canvas in pixels.
pub fn canvas_dimensions() -> (u32, u32) {
    let packed = unsafe { _api_canvas_dimensions() };
    ((packed >> 32) as u32, (packed & 0xFFFF_FFFF) as u32)
}

/// Draw an image on the canvas from encoded image bytes (PNG, JPEG, GIF, WebP).
/// The browser decodes the image and renders it at the given rectangle.
pub fn canvas_image(x: f32, y: f32, w: f32, h: f32, data: &[u8]) {
    unsafe { _api_canvas_image(x, y, w, h, data.as_ptr() as u32, data.len() as u32) }
}

// ─── Extended Shape Primitives ──────────────────────────────────────────────

/// Draw a filled rounded rectangle with uniform corner radius.
pub fn canvas_rounded_rect(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
) {
    unsafe { _api_canvas_rounded_rect(x, y, w, h, radius, r as u32, g as u32, b as u32, a as u32) }
}

/// Draw a circular arc stroke from `start_angle` to `end_angle` (in radians, clockwise from +X).
pub fn canvas_arc(
    cx: f32,
    cy: f32,
    radius: f32,
    start_angle: f32,
    end_angle: f32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
    thickness: f32,
) {
    unsafe {
        _api_canvas_arc(
            cx,
            cy,
            radius,
            start_angle,
            end_angle,
            r as u32,
            g as u32,
            b as u32,
            a as u32,
            thickness,
        )
    }
}

/// Draw a cubic Bézier curve stroke from `(x1,y1)` to `(x2,y2)` with two control points.
pub fn canvas_bezier(
    x1: f32,
    y1: f32,
    cp1x: f32,
    cp1y: f32,
    cp2x: f32,
    cp2y: f32,
    x2: f32,
    y2: f32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
    thickness: f32,
) {
    unsafe {
        _api_canvas_bezier(
            x1, y1, cp1x, cp1y, cp2x, cp2y, x2, y2, r as u32, g as u32, b as u32, a as u32,
            thickness,
        )
    }
}

/// Gradient type constants.
pub const GRADIENT_LINEAR: u32 = 0;
pub const GRADIENT_RADIAL: u32 = 1;

/// Draw a gradient-filled rectangle.
///
/// `kind`: [`GRADIENT_LINEAR`] or [`GRADIENT_RADIAL`].
/// For linear gradients, `(ax,ay)` and `(bx,by)` define the gradient axis.
/// For radial gradients, `(ax,ay)` is the center and `by` is the radius.
/// `stops` is a slice of `(offset, r, g, b, a)` tuples.
pub fn canvas_gradient(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    kind: u32,
    ax: f32,
    ay: f32,
    bx: f32,
    by: f32,
    stops: &[(f32, u8, u8, u8, u8)],
) {
    let mut buf = Vec::with_capacity(stops.len() * 8);
    for &(offset, r, g, b, a) in stops {
        buf.extend_from_slice(&offset.to_le_bytes());
        buf.push(r);
        buf.push(g);
        buf.push(b);
        buf.push(a);
    }
    unsafe {
        _api_canvas_gradient(
            x,
            y,
            w,
            h,
            kind,
            ax,
            ay,
            bx,
            by,
            buf.as_ptr() as u32,
            buf.len() as u32,
        )
    }
}

// ─── Canvas State API ───────────────────────────────────────────────────────

/// Push the current canvas state (transform, clip, opacity) onto an internal stack.
/// Use with [`canvas_restore`] to scope transformations and effects.
pub fn canvas_save() {
    unsafe { _api_canvas_save() }
}

/// Pop and restore the most recently saved canvas state.
pub fn canvas_restore() {
    unsafe { _api_canvas_restore() }
}

/// Apply a 2D affine transformation to subsequent draw commands.
///
/// The six values represent a column-major 3×2 matrix:
/// ```text
/// | a  c  tx |
/// | b  d  ty |
/// | 0  0   1 |
/// ```
///
/// For a simple translation, use `canvas_transform(1.0, 0.0, 0.0, 1.0, tx, ty)`.
pub fn canvas_transform(a: f32, b: f32, c: f32, d: f32, tx: f32, ty: f32) {
    unsafe { _api_canvas_transform(a, b, c, d, tx, ty) }
}

/// Intersect the current clipping region with an axis-aligned rectangle.
/// Coordinates are in the current (possibly transformed) canvas space.
pub fn canvas_clip(x: f32, y: f32, w: f32, h: f32) {
    unsafe { _api_canvas_clip(x, y, w, h) }
}

/// Set the layer opacity for subsequent draw commands (0.0 = transparent, 1.0 = opaque).
/// Multiplied with any parent opacity set via nested [`canvas_save`]/[`canvas_opacity`].
pub fn canvas_opacity(alpha: f32) {
    unsafe { _api_canvas_opacity(alpha) }
}

// ─── GPU / WebGPU-style API ─────────────────────────────────────────────────

/// GPU buffer usage flags (matches WebGPU `GPUBufferUsage`).
pub mod gpu_usage {
    pub const VERTEX: u32 = 0x0020;
    pub const INDEX: u32 = 0x0010;
    pub const UNIFORM: u32 = 0x0040;
    pub const STORAGE: u32 = 0x0080;
}

/// Create a GPU buffer of `size` bytes. Returns a handle (0 = failure).
///
/// `usage` is a bitmask of [`gpu_usage`] flags.
pub fn gpu_create_buffer(size: u64, usage: u32) -> u32 {
    unsafe { _api_gpu_create_buffer(size as u32, (size >> 32) as u32, usage) }
}

/// Create a 2D RGBA8 texture. Returns a handle (0 = failure).
pub fn gpu_create_texture(width: u32, height: u32) -> u32 {
    unsafe { _api_gpu_create_texture(width, height) }
}

/// Compile a WGSL shader module. Returns a handle (0 = failure).
pub fn gpu_create_shader(source: &str) -> u32 {
    unsafe { _api_gpu_create_shader(source.as_ptr() as u32, source.len() as u32) }
}

/// Create a render pipeline from a shader. Returns a handle (0 = failure).
///
/// `vertex_entry` and `fragment_entry` are the WGSL function names.
pub fn gpu_create_pipeline(shader: u32, vertex_entry: &str, fragment_entry: &str) -> u32 {
    unsafe {
        _api_gpu_create_render_pipeline(
            shader,
            vertex_entry.as_ptr() as u32,
            vertex_entry.len() as u32,
            fragment_entry.as_ptr() as u32,
            fragment_entry.len() as u32,
        )
    }
}

/// Create a compute pipeline from a shader. Returns a handle (0 = failure).
pub fn gpu_create_compute_pipeline(shader: u32, entry_point: &str) -> u32 {
    unsafe {
        _api_gpu_create_compute_pipeline(
            shader,
            entry_point.as_ptr() as u32,
            entry_point.len() as u32,
        )
    }
}

/// Write data to a GPU buffer at the given byte offset.
pub fn gpu_write_buffer(handle: u32, offset: u64, data: &[u8]) -> bool {
    unsafe {
        _api_gpu_write_buffer(
            handle,
            offset as u32,
            (offset >> 32) as u32,
            data.as_ptr() as u32,
            data.len() as u32,
        ) != 0
    }
}

/// Submit a render pass: draw `vertex_count` vertices with `instance_count` instances.
pub fn gpu_draw(
    pipeline: u32,
    target_texture: u32,
    vertex_count: u32,
    instance_count: u32,
) -> bool {
    unsafe { _api_gpu_draw(pipeline, target_texture, vertex_count, instance_count) != 0 }
}

/// Submit a compute dispatch with the given workgroup counts.
pub fn gpu_dispatch_compute(pipeline: u32, x: u32, y: u32, z: u32) -> bool {
    unsafe { _api_gpu_dispatch_compute(pipeline, x, y, z) != 0 }
}

/// Destroy a GPU buffer.
pub fn gpu_destroy_buffer(handle: u32) -> bool {
    unsafe { _api_gpu_destroy_buffer(handle) != 0 }
}

/// Destroy a GPU texture.
pub fn gpu_destroy_texture(handle: u32) -> bool {
    unsafe { _api_gpu_destroy_texture(handle) != 0 }
}

// ─── Local Storage API ──────────────────────────────────────────────────────

/// Store a key-value pair in sandboxed local storage.
pub fn storage_set(key: &str, value: &str) {
    unsafe {
        _api_storage_set(
            key.as_ptr() as u32,
            key.len() as u32,
            value.as_ptr() as u32,
            value.len() as u32,
        )
    }
}

/// Retrieve a value from local storage. Returns empty string if not found.
pub fn storage_get(key: &str) -> String {
    let mut buf = [0u8; 4096];
    let len = unsafe {
        _api_storage_get(
            key.as_ptr() as u32,
            key.len() as u32,
            buf.as_mut_ptr() as u32,
            buf.len() as u32,
        )
    };
    String::from_utf8_lossy(&buf[..len as usize]).to_string()
}

/// Remove a key from local storage.
pub fn storage_remove(key: &str) {
    unsafe { _api_storage_remove(key.as_ptr() as u32, key.len() as u32) }
}

// ─── Clipboard API ──────────────────────────────────────────────────────────

/// Copy text to the system clipboard.
pub fn clipboard_write(text: &str) {
    unsafe { _api_clipboard_write(text.as_ptr() as u32, text.len() as u32) }
}

/// Read text from the system clipboard.
pub fn clipboard_read() -> String {
    let mut buf = [0u8; 4096];
    let len = unsafe { _api_clipboard_read(buf.as_mut_ptr() as u32, buf.len() as u32) };
    String::from_utf8_lossy(&buf[..len as usize]).to_string()
}

// ─── Timer / Clock API ─────────────────────────────────────────────────────

/// Get the current time in milliseconds since the UNIX epoch.
pub fn time_now_ms() -> u64 {
    unsafe { _api_time_now_ms() }
}

/// Schedule a one-shot timer that fires after `delay_ms` milliseconds.
/// When it fires the host calls your exported `on_timer(callback_id)`.
/// Returns a timer ID that can be passed to [`clear_timer`].
pub fn set_timeout(callback_id: u32, delay_ms: u32) -> u32 {
    unsafe { _api_set_timeout(callback_id, delay_ms) }
}

/// Schedule a repeating timer that fires every `interval_ms` milliseconds.
/// When it fires the host calls your exported `on_timer(callback_id)`.
/// Returns a timer ID that can be passed to [`clear_timer`].
pub fn set_interval(callback_id: u32, interval_ms: u32) -> u32 {
    unsafe { _api_set_interval(callback_id, interval_ms) }
}

/// Cancel a timer previously created with [`set_timeout`] or [`set_interval`].
pub fn clear_timer(timer_id: u32) {
    unsafe { _api_clear_timer(timer_id) }
}

/// Schedule a callback for the next animation frame (vsync-aligned repaint).
///
/// The host calls your exported `on_timer(callback_id)` with the provided ID on the
/// subsequent frame. Returns a request ID usable with [`cancel_animation_frame`].
/// Call `request_animation_frame` again from inside the callback to keep animating.
pub fn request_animation_frame(callback_id: u32) -> u32 {
    unsafe { _api_request_animation_frame(callback_id) }
}

/// Cancel a pending animation frame request.
pub fn cancel_animation_frame(request_id: u32) {
    unsafe { _api_cancel_animation_frame(request_id) }
}

// ─── Event System ───────────────────────────────────────────────────────────
//
// Register listeners for built-in or custom events. Built-in event types
// produced by the host:
//
// | Event              | Payload                                                          |
// |--------------------|------------------------------------------------------------------|
// | `resize`           | 8 bytes: `width: u32, height: u32` (little-endian)               |
// | `focus` / `blur`   | empty                                                            |
// | `visibility_change`| UTF-8 string `"visible"` or `"hidden"`                           |
// | `online`/`offline` | empty                                                            |
// | `touch_start`      | 8 bytes: `x: f32, y: f32` (little-endian)                        |
// | `touch_move`       | 8 bytes: `x: f32, y: f32`                                        |
// | `touch_end`        | 8 bytes: `x: f32, y: f32`                                        |
// | `gamepad_connected`| UTF-8 device name                                                |
// | `gamepad_button`   | 12 bytes: `id: u32, code: u32, pressed: u32`                     |
// | `gamepad_axis`     | 12 bytes: `id: u32, code: u32, value: f32`                       |
// | `drop_files`       | UTF-8 JSON array of dropped file paths, e.g. `["/tmp/a.png"]`    |
//
// Events fire via the guest-exported `on_event(callback_id: u32)` function,
// which the host calls once per pending event each frame (before timers and
// `on_frame`). Inside that callback, use [`event_type`] / [`event_data`] /
// [`event_data_into`] to inspect the current event.

/// Register a listener for events of `event_type`. When an event fires, the
/// host invokes the guest-exported `on_event(callback_id)` and exposes the
/// event payload via [`event_type`] / [`event_data`].
///
/// Returns a non-zero listener ID for [`off_event`], or `0` on failure
/// (empty event type, missing memory).
pub fn on_event(event_type: &str, callback_id: u32) -> u32 {
    unsafe {
        _api_on_event(
            event_type.as_ptr() as u32,
            event_type.len() as u32,
            callback_id,
        )
    }
}

/// Cancel a previously-registered listener. Returns `true` if a listener
/// with that ID existed and was removed.
pub fn off_event(listener_id: u32) -> bool {
    unsafe { _api_off_event(listener_id) != 0 }
}

/// Emit a custom event with an arbitrary payload. Listeners registered for
/// this event type via [`on_event`] will be invoked on the next frame
/// (before timers and `on_frame`).
pub fn emit_event(event_type: &str, data: &[u8]) {
    unsafe {
        _api_emit_event(
            event_type.as_ptr() as u32,
            event_type.len() as u32,
            data.as_ptr() as u32,
            data.len() as u32,
        )
    }
}

/// The type name of the event currently being delivered. Only meaningful
/// inside an `on_event` callback; returns an empty string otherwise.
pub fn event_type() -> String {
    let len = unsafe { _api_event_type_len() } as usize;
    if len == 0 {
        return String::new();
    }
    let mut buf = vec![0u8; len];
    let written = unsafe { _api_event_type_read(buf.as_mut_ptr() as u32, len as u32) } as usize;
    buf.truncate(written);
    String::from_utf8_lossy(&buf).into_owned()
}

/// Copy the current event's payload bytes into `out` and return the number
/// of bytes written. Truncates if `out` is smaller than the payload.
pub fn event_data(out: &mut [u8]) -> usize {
    let cap = out.len() as u32;
    if cap == 0 {
        return 0;
    }
    unsafe { _api_event_data_read(out.as_mut_ptr() as u32, cap) as usize }
}

/// Allocate a fresh `Vec<u8>` containing the current event's payload.
pub fn event_data_into() -> Vec<u8> {
    let len = unsafe { _api_event_data_len() } as usize;
    if len == 0 {
        return Vec::new();
    }
    let mut buf = vec![0u8; len];
    let written = unsafe { _api_event_data_read(buf.as_mut_ptr() as u32, len as u32) } as usize;
    buf.truncate(written);
    buf
}

// ─── Random API ─────────────────────────────────────────────────────────────

/// Get a random u64 from the host.
pub fn random_u64() -> u64 {
    unsafe { _api_random() }
}

/// Get a random f64 in [0, 1).
pub fn random_f64() -> f64 {
    (random_u64() >> 11) as f64 / (1u64 << 53) as f64
}

// ─── Notification API ───────────────────────────────────────────────────────

/// Send a notification to the user (rendered in the browser console).
pub fn notify(title: &str, body: &str) {
    unsafe {
        _api_notify(
            title.as_ptr() as u32,
            title.len() as u32,
            body.as_ptr() as u32,
            body.len() as u32,
        )
    }
}

// ─── Audio Playback API ─────────────────────────────────────────────────────

/// Detected or hinted audio container (host codes: 0 unknown, 1 WAV, 2 MP3, 3 Ogg, 4 FLAC).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioFormat {
    /// Could not classify from bytes (try decode anyway).
    Unknown = 0,
    Wav = 1,
    Mp3 = 2,
    Ogg = 3,
    Flac = 4,
}

impl From<u32> for AudioFormat {
    fn from(code: u32) -> Self {
        match code {
            1 => AudioFormat::Wav,
            2 => AudioFormat::Mp3,
            3 => AudioFormat::Ogg,
            4 => AudioFormat::Flac,
            _ => AudioFormat::Unknown,
        }
    }
}

impl From<AudioFormat> for u32 {
    fn from(f: AudioFormat) -> u32 {
        f as u32
    }
}

/// Play audio from encoded bytes (WAV, MP3, OGG, FLAC).
/// The host decodes and plays the audio. Returns 0 on success, negative on error.
pub fn audio_play(data: &[u8]) -> i32 {
    unsafe { _api_audio_play(data.as_ptr() as u32, data.len() as u32) }
}

/// Sniff the container/codec from raw bytes (magic bytes / MP3 sync). Does not decode audio.
pub fn audio_detect_format(data: &[u8]) -> AudioFormat {
    let code = unsafe { _api_audio_detect_format(data.as_ptr() as u32, data.len() as u32) };
    AudioFormat::from(code)
}

/// Play with an optional format hint (`AudioFormat::Unknown` = same as [`audio_play`]).
/// If the hint disagrees with what the host sniffs from the bytes, the host logs a warning but still decodes.
pub fn audio_play_with_format(data: &[u8], format: AudioFormat) -> i32 {
    unsafe {
        _api_audio_play_with_format(data.as_ptr() as u32, data.len() as u32, u32::from(format))
    }
}

/// Fetch audio from a URL and play it.
/// The host sends an `Accept` header listing supported codecs, records the response `Content-Type`,
/// and rejects obvious HTML/JSON error bodies when no audio signature is found (`-4`).
/// Returns 0 on success, negative on error.
pub fn audio_play_url(url: &str) -> i32 {
    unsafe { _api_audio_play_url(url.as_ptr() as u32, url.len() as u32) }
}

/// `Content-Type` header from the last successful [`audio_play_url`] response (may be empty).
pub fn audio_last_url_content_type() -> String {
    let mut buf = [0u8; 512];
    let len =
        unsafe { _api_audio_last_url_content_type(buf.as_mut_ptr() as u32, buf.len() as u32) };
    let n = (len as usize).min(buf.len());
    String::from_utf8_lossy(&buf[..n]).to_string()
}

/// Pause audio playback.
pub fn audio_pause() {
    unsafe { _api_audio_pause() }
}

/// Resume paused audio playback.
pub fn audio_resume() {
    unsafe { _api_audio_resume() }
}

/// Stop audio playback and clear the queue.
pub fn audio_stop() {
    unsafe { _api_audio_stop() }
}

/// Set audio volume. 1.0 is normal, 0.0 is silent, up to 2.0 for boost.
pub fn audio_set_volume(level: f32) {
    unsafe { _api_audio_set_volume(level) }
}

/// Get the current audio volume.
pub fn audio_get_volume() -> f32 {
    unsafe { _api_audio_get_volume() }
}

/// Returns `true` if audio is currently playing (not paused and not empty).
pub fn audio_is_playing() -> bool {
    unsafe { _api_audio_is_playing() != 0 }
}

/// Get the current playback position in milliseconds.
pub fn audio_position() -> u64 {
    unsafe { _api_audio_position() }
}

/// Seek to a position in milliseconds. Returns 0 on success, negative on error.
pub fn audio_seek(position_ms: u64) -> i32 {
    unsafe { _api_audio_seek(position_ms) }
}

/// Get the total duration of the currently loaded track in milliseconds.
/// Returns 0 if unknown or nothing is loaded.
pub fn audio_duration() -> u64 {
    unsafe { _api_audio_duration() }
}

/// Enable or disable looping on the default channel.
/// When enabled, subsequent `audio_play` calls will loop indefinitely.
pub fn audio_set_loop(enabled: bool) {
    unsafe { _api_audio_set_loop(if enabled { 1 } else { 0 }) }
}

// ─── Multi-Channel Audio API ────────────────────────────────────────────────

/// Play audio on a specific channel. Multiple channels play simultaneously.
/// Channel 0 is the default used by `audio_play`. Use channels 1+ for layered
/// sound effects, background music, etc.
pub fn audio_channel_play(channel: u32, data: &[u8]) -> i32 {
    unsafe { _api_audio_channel_play(channel, data.as_ptr() as u32, data.len() as u32) }
}

/// Like [`audio_channel_play`] with an optional [`AudioFormat`] hint.
pub fn audio_channel_play_with_format(channel: u32, data: &[u8], format: AudioFormat) -> i32 {
    unsafe {
        _api_audio_channel_play_with_format(
            channel,
            data.as_ptr() as u32,
            data.len() as u32,
            u32::from(format),
        )
    }
}

/// Stop playback on a specific channel.
pub fn audio_channel_stop(channel: u32) {
    unsafe { _api_audio_channel_stop(channel) }
}

/// Set volume for a specific channel (0.0 silent, 1.0 normal, up to 2.0 boost).
pub fn audio_channel_set_volume(channel: u32, level: f32) {
    unsafe { _api_audio_channel_set_volume(channel, level) }
}

// ─── Video API ─────────────────────────────────────────────────────────────

/// Container or hint for [`video_load_with_format`] (host codes: 0 unknown, 1 MP4, 2 WebM, 3 AV1).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoFormat {
    Unknown = 0,
    Mp4 = 1,
    Webm = 2,
    Av1 = 3,
}

impl From<u32> for VideoFormat {
    fn from(code: u32) -> Self {
        match code {
            1 => VideoFormat::Mp4,
            2 => VideoFormat::Webm,
            3 => VideoFormat::Av1,
            _ => VideoFormat::Unknown,
        }
    }
}

impl From<VideoFormat> for u32 {
    fn from(f: VideoFormat) -> u32 {
        f as u32
    }
}

/// Sniff container from leading bytes (magic only; does not decode).
pub fn video_detect_format(data: &[u8]) -> VideoFormat {
    let code = unsafe { _api_video_detect_format(data.as_ptr() as u32, data.len() as u32) };
    VideoFormat::from(code)
}

/// Load video from encoded bytes (MP4, WebM, etc.). Requires FFmpeg on the host.
/// Returns 0 on success, negative on error.
pub fn video_load(data: &[u8]) -> i32 {
    unsafe {
        _api_video_load(
            data.as_ptr() as u32,
            data.len() as u32,
            VideoFormat::Unknown as u32,
        )
    }
}

/// Load with a [`VideoFormat`] hint (unknown = same as [`video_load`]).
pub fn video_load_with_format(data: &[u8], format: VideoFormat) -> i32 {
    unsafe { _api_video_load(data.as_ptr() as u32, data.len() as u32, u32::from(format)) }
}

/// Open a progressive or adaptive (HLS) URL. The host uses FFmpeg; master playlists may list variants.
pub fn video_load_url(url: &str) -> i32 {
    unsafe { _api_video_load_url(url.as_ptr() as u32, url.len() as u32) }
}

/// `Content-Type` from the last successful [`video_load_url`] (may be empty).
pub fn video_last_url_content_type() -> String {
    let mut buf = [0u8; 512];
    let len =
        unsafe { _api_video_last_url_content_type(buf.as_mut_ptr() as u32, buf.len() as u32) };
    let n = (len as usize).min(buf.len());
    String::from_utf8_lossy(&buf[..n]).to_string()
}

/// Number of variant stream URIs parsed from the last HLS master playlist (0 if not a master).
pub fn video_hls_variant_count() -> u32 {
    unsafe { _api_video_hls_variant_count() }
}

/// Resolved variant URL for `index`, written into `buf`-style API (use fixed buffer).
pub fn video_hls_variant_url(index: u32) -> String {
    let mut buf = [0u8; 2048];
    let len =
        unsafe { _api_video_hls_variant_url(index, buf.as_mut_ptr() as u32, buf.len() as u32) };
    let n = (len as usize).min(buf.len());
    String::from_utf8_lossy(&buf[..n]).to_string()
}

/// Open a variant playlist by index (after loading a master with [`video_load_url`]).
pub fn video_hls_open_variant(index: u32) -> i32 {
    unsafe { _api_video_hls_open_variant(index) }
}

pub fn video_play() {
    unsafe { _api_video_play() }
}

pub fn video_pause() {
    unsafe { _api_video_pause() }
}

pub fn video_stop() {
    unsafe { _api_video_stop() }
}

pub fn video_seek(position_ms: u64) -> i32 {
    unsafe { _api_video_seek(position_ms) }
}

pub fn video_position() -> u64 {
    unsafe { _api_video_position() }
}

pub fn video_duration() -> u64 {
    unsafe { _api_video_duration() }
}

/// Draw the current video frame into the given rectangle (same coordinate space as canvas).
pub fn video_render(x: f32, y: f32, w: f32, h: f32) -> i32 {
    unsafe { _api_video_render(x, y, w, h) }
}

/// Volume multiplier for the video track (0.0–2.0; embedded audio mixing may follow in future hosts).
pub fn video_set_volume(level: f32) {
    unsafe { _api_video_set_volume(level) }
}

pub fn video_get_volume() -> f32 {
    unsafe { _api_video_get_volume() }
}

pub fn video_set_loop(enabled: bool) {
    unsafe { _api_video_set_loop(if enabled { 1 } else { 0 }) }
}

/// Floating picture-in-picture preview (host mirrors the last rendered frame).
pub fn video_set_pip(enabled: bool) {
    unsafe { _api_video_set_pip(if enabled { 1 } else { 0 }) }
}

/// Load SubRip subtitles (cues rendered on [`video_render`]).
pub fn subtitle_load_srt(text: &str) -> i32 {
    unsafe { _api_subtitle_load_srt(text.as_ptr() as u32, text.len() as u32) }
}

/// Load WebVTT subtitles.
pub fn subtitle_load_vtt(text: &str) -> i32 {
    unsafe { _api_subtitle_load_vtt(text.as_ptr() as u32, text.len() as u32) }
}

pub fn subtitle_clear() {
    unsafe { _api_subtitle_clear() }
}

// ─── Media capture API ─────────────────────────────────────────────────────

/// Opens the default camera after a host permission dialog.
///
/// Returns `0` on success. Negative codes: `-1` user denied, `-2` no camera, `-3` open failed.
pub fn camera_open() -> i32 {
    unsafe { _api_camera_open() }
}

/// Stops the camera stream opened by [`camera_open`].
pub fn camera_close() {
    unsafe { _api_camera_close() }
}

/// Captures one RGBA8 frame into `out`. Returns the number of bytes written (`0` if the camera
/// is not open or capture failed). Query [`camera_frame_dimensions`] after a successful write.
pub fn camera_capture_frame(out: &mut [u8]) -> u32 {
    unsafe { _api_camera_capture_frame(out.as_mut_ptr() as u32, out.len() as u32) }
}

/// Width and height in pixels of the last [`camera_capture_frame`] buffer.
pub fn camera_frame_dimensions() -> (u32, u32) {
    let packed = unsafe { _api_camera_frame_dimensions() };
    let w = (packed >> 32) as u32;
    let h = packed as u32;
    (w, h)
}

/// Starts microphone capture (mono `f32` ring buffer) after a host permission dialog.
///
/// Returns `0` on success. Negative codes: `-1` denied, `-2` no input device, `-3` stream error.
pub fn microphone_open() -> i32 {
    unsafe { _api_microphone_open() }
}

pub fn microphone_close() {
    unsafe { _api_microphone_close() }
}

/// Sample rate of the opened input stream in Hz (`0` if the microphone is not open).
pub fn microphone_sample_rate() -> u32 {
    unsafe { _api_microphone_sample_rate() }
}

/// Dequeues up to `out.len()` mono `f32` samples from the microphone ring buffer.
/// Returns how many samples were written.
pub fn microphone_read_samples(out: &mut [f32]) -> u32 {
    unsafe { _api_microphone_read_samples(out.as_mut_ptr() as u32, out.len() as u32) }
}

/// Captures the primary display as RGBA8 after permission dialogs (OS may prompt separately).
///
/// Returns `Ok(bytes_written)` or an error code: `-1` denied, `-2` no display, `-3` capture failed, `-4` buffer error.
pub fn screen_capture(out: &mut [u8]) -> Result<usize, i32> {
    let n = unsafe { _api_screen_capture(out.as_mut_ptr() as u32, out.len() as u32) };
    if n >= 0 {
        Ok(n as usize)
    } else {
        Err(n)
    }
}

/// Width and height of the last [`screen_capture`] image.
pub fn screen_capture_dimensions() -> (u32, u32) {
    let packed = unsafe { _api_screen_capture_dimensions() };
    let w = (packed >> 32) as u32;
    let h = packed as u32;
    (w, h)
}

/// Host-side pipeline counters: total camera frames captured (high 32 bits) and current microphone
/// ring depth in samples (low 32 bits).
pub fn media_pipeline_stats() -> (u64, u32) {
    let packed = unsafe { _api_media_pipeline_stats() };
    let camera_frames = packed >> 32;
    let mic_ring = packed as u32;
    (camera_frames, mic_ring)
}

// ─── WebRTC / Real-Time Communication API ───────────────────────────────────

/// Connection state returned by [`rtc_connection_state`].
pub const RTC_STATE_NEW: u32 = 0;
/// Peer is attempting to connect.
pub const RTC_STATE_CONNECTING: u32 = 1;
/// Peer connection is established.
pub const RTC_STATE_CONNECTED: u32 = 2;
/// Transport was temporarily interrupted.
pub const RTC_STATE_DISCONNECTED: u32 = 3;
/// Connection attempt failed.
pub const RTC_STATE_FAILED: u32 = 4;
/// Peer connection has been closed.
pub const RTC_STATE_CLOSED: u32 = 5;

/// Track kind: audio.
pub const RTC_TRACK_AUDIO: u32 = 0;
/// Track kind: video.
pub const RTC_TRACK_VIDEO: u32 = 1;

/// Received data channel message.
pub struct RtcMessage {
    /// Channel on which the message arrived.
    pub channel_id: u32,
    /// `true` when the payload is raw bytes, `false` for UTF-8 text.
    pub is_binary: bool,
    /// Message payload.
    pub data: Vec<u8>,
}

impl RtcMessage {
    /// Interpret the payload as UTF-8 text.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.data).to_string()
    }
}

/// Information about a newly opened remote data channel.
pub struct RtcDataChannelInfo {
    /// Handle to use with [`rtc_send`] and [`rtc_recv`].
    pub channel_id: u32,
    /// Label chosen by the remote peer.
    pub label: String,
}

/// Create a new WebRTC peer connection.
///
/// `stun_servers` is a comma-separated list of STUN/TURN URLs (e.g.
/// `"stun:stun.l.google.com:19302"`). Pass `""` for the built-in default.
///
/// Returns a peer handle (`> 0`) or `0` on failure.
pub fn rtc_create_peer(stun_servers: &str) -> u32 {
    unsafe { _api_rtc_create_peer(stun_servers.as_ptr() as u32, stun_servers.len() as u32) }
}

/// Close and release a peer connection.
pub fn rtc_close_peer(peer_id: u32) -> bool {
    unsafe { _api_rtc_close_peer(peer_id) != 0 }
}

/// Generate an SDP offer for the peer and set it as the local description.
///
/// Returns the SDP string or an error code.
pub fn rtc_create_offer(peer_id: u32) -> Result<String, i32> {
    let mut buf = vec![0u8; 16 * 1024];
    let n = unsafe { _api_rtc_create_offer(peer_id, buf.as_mut_ptr() as u32, buf.len() as u32) };
    if n < 0 {
        Err(n)
    } else {
        Ok(String::from_utf8_lossy(&buf[..n as usize]).to_string())
    }
}

/// Generate an SDP answer (after setting the remote offer) and set it as the local description.
pub fn rtc_create_answer(peer_id: u32) -> Result<String, i32> {
    let mut buf = vec![0u8; 16 * 1024];
    let n = unsafe { _api_rtc_create_answer(peer_id, buf.as_mut_ptr() as u32, buf.len() as u32) };
    if n < 0 {
        Err(n)
    } else {
        Ok(String::from_utf8_lossy(&buf[..n as usize]).to_string())
    }
}

/// Set the local SDP description explicitly.
///
/// `is_offer` — `true` for an offer, `false` for an answer.
pub fn rtc_set_local_description(peer_id: u32, sdp: &str, is_offer: bool) -> i32 {
    unsafe {
        _api_rtc_set_local_description(
            peer_id,
            sdp.as_ptr() as u32,
            sdp.len() as u32,
            if is_offer { 1 } else { 0 },
        )
    }
}

/// Set the remote SDP description received from the other peer.
pub fn rtc_set_remote_description(peer_id: u32, sdp: &str, is_offer: bool) -> i32 {
    unsafe {
        _api_rtc_set_remote_description(
            peer_id,
            sdp.as_ptr() as u32,
            sdp.len() as u32,
            if is_offer { 1 } else { 0 },
        )
    }
}

/// Add a trickled ICE candidate (JSON string from the remote peer).
pub fn rtc_add_ice_candidate(peer_id: u32, candidate_json: &str) -> i32 {
    unsafe {
        _api_rtc_add_ice_candidate(
            peer_id,
            candidate_json.as_ptr() as u32,
            candidate_json.len() as u32,
        )
    }
}

/// Poll the current connection state of a peer.
pub fn rtc_connection_state(peer_id: u32) -> u32 {
    unsafe { _api_rtc_connection_state(peer_id) }
}

/// Poll for a locally gathered ICE candidate (JSON). Returns `None` when the
/// queue is empty.
pub fn rtc_poll_ice_candidate(peer_id: u32) -> Option<String> {
    let mut buf = vec![0u8; 4096];
    let n =
        unsafe { _api_rtc_poll_ice_candidate(peer_id, buf.as_mut_ptr() as u32, buf.len() as u32) };
    if n <= 0 {
        None
    } else {
        Some(String::from_utf8_lossy(&buf[..n as usize]).to_string())
    }
}

/// Create a data channel on a peer connection.
///
/// `ordered` — `true` for reliable ordered delivery (TCP-like), `false` for
/// unordered (UDP-like). Returns a channel handle (`> 0`) or `0` on failure.
pub fn rtc_create_data_channel(peer_id: u32, label: &str, ordered: bool) -> u32 {
    unsafe {
        _api_rtc_create_data_channel(
            peer_id,
            label.as_ptr() as u32,
            label.len() as u32,
            if ordered { 1 } else { 0 },
        )
    }
}

/// Send a UTF-8 text message on a data channel.
pub fn rtc_send_text(peer_id: u32, channel_id: u32, text: &str) -> i32 {
    unsafe {
        _api_rtc_send(
            peer_id,
            channel_id,
            text.as_ptr() as u32,
            text.len() as u32,
            0,
        )
    }
}

/// Send binary data on a data channel.
pub fn rtc_send_binary(peer_id: u32, channel_id: u32, data: &[u8]) -> i32 {
    unsafe {
        _api_rtc_send(
            peer_id,
            channel_id,
            data.as_ptr() as u32,
            data.len() as u32,
            1,
        )
    }
}

/// Send data on a channel, choosing text or binary mode.
pub fn rtc_send(peer_id: u32, channel_id: u32, data: &[u8], is_binary: bool) -> i32 {
    unsafe {
        _api_rtc_send(
            peer_id,
            channel_id,
            data.as_ptr() as u32,
            data.len() as u32,
            if is_binary { 1 } else { 0 },
        )
    }
}

/// Poll for an incoming message on any channel of the peer (pass `channel_id = 0`)
/// or on a specific channel.
///
/// Returns `None` when no message is queued.
pub fn rtc_recv(peer_id: u32, channel_id: u32) -> Option<RtcMessage> {
    let mut buf = vec![0u8; 64 * 1024];
    let packed = unsafe {
        _api_rtc_recv(
            peer_id,
            channel_id,
            buf.as_mut_ptr() as u32,
            buf.len() as u32,
        )
    };
    if packed <= 0 {
        return None;
    }
    let packed = packed as u64;
    let data_len = (packed & 0xFFFF_FFFF) as usize;
    let is_binary = (packed >> 32) & 1 != 0;
    let ch = (packed >> 48) as u32;
    Some(RtcMessage {
        channel_id: ch,
        is_binary,
        data: buf[..data_len].to_vec(),
    })
}

/// Poll for a remotely-created data channel that the peer opened.
///
/// Returns `None` when no new channels are pending.
pub fn rtc_poll_data_channel(peer_id: u32) -> Option<RtcDataChannelInfo> {
    let mut buf = vec![0u8; 1024];
    let n =
        unsafe { _api_rtc_poll_data_channel(peer_id, buf.as_mut_ptr() as u32, buf.len() as u32) };
    if n <= 0 {
        return None;
    }
    let info = String::from_utf8_lossy(&buf[..n as usize]).to_string();
    let (id_str, label) = info.split_once(':').unwrap_or(("0", ""));
    Some(RtcDataChannelInfo {
        channel_id: id_str.parse().unwrap_or(0),
        label: label.to_string(),
    })
}

/// Attach a media track (audio or video) to a peer connection.
///
/// `kind` — [`RTC_TRACK_AUDIO`] or [`RTC_TRACK_VIDEO`].
/// Returns a track handle (`> 0`) or `0` on failure.
pub fn rtc_add_track(peer_id: u32, kind: u32) -> u32 {
    unsafe { _api_rtc_add_track(peer_id, kind) }
}

/// Information about a remote media track received from a peer.
pub struct RtcTrackInfo {
    /// `RTC_TRACK_AUDIO` (0) or `RTC_TRACK_VIDEO` (1).
    pub kind: u32,
    /// Track identifier chosen by the remote peer.
    pub id: String,
    /// Media stream identifier the track belongs to.
    pub stream_id: String,
}

/// Poll for a remote media track added by the peer.
///
/// Returns `None` when no new tracks are pending.
pub fn rtc_poll_track(peer_id: u32) -> Option<RtcTrackInfo> {
    let mut buf = vec![0u8; 1024];
    let n = unsafe { _api_rtc_poll_track(peer_id, buf.as_mut_ptr() as u32, buf.len() as u32) };
    if n <= 0 {
        return None;
    }
    let info = String::from_utf8_lossy(&buf[..n as usize]).to_string();
    let mut parts = info.splitn(3, ':');
    let kind = parts.next().unwrap_or("2").parse().unwrap_or(2);
    let id = parts.next().unwrap_or("").to_string();
    let stream_id = parts.next().unwrap_or("").to_string();
    Some(RtcTrackInfo {
        kind,
        id,
        stream_id,
    })
}

/// Connect to a signaling server at `url` for bootstrapping peer connections.
///
/// Returns `1` on success, `0` on failure.
pub fn rtc_signal_connect(url: &str) -> bool {
    unsafe { _api_rtc_signal_connect(url.as_ptr() as u32, url.len() as u32) != 0 }
}

/// Join (or create) a signaling room for peer discovery.
pub fn rtc_signal_join_room(room: &str) -> i32 {
    unsafe { _api_rtc_signal_join_room(room.as_ptr() as u32, room.len() as u32) }
}

/// Send a signaling message (JSON bytes) to the connected signaling server.
pub fn rtc_signal_send(data: &[u8]) -> i32 {
    unsafe { _api_rtc_signal_send(data.as_ptr() as u32, data.len() as u32) }
}

/// Poll for an incoming signaling message.
pub fn rtc_signal_recv() -> Option<Vec<u8>> {
    let mut buf = vec![0u8; 16 * 1024];
    let n = unsafe { _api_rtc_signal_recv(buf.as_mut_ptr() as u32, buf.len() as u32) };
    if n <= 0 {
        None
    } else {
        Some(buf[..n as usize].to_vec())
    }
}

// ─── WebSocket API ───────────────────────────────────────────────────────────

/// WebSocket ready-state: connection is being established.
pub const WS_CONNECTING: u32 = 0;
/// WebSocket ready-state: connection is open and ready.
pub const WS_OPEN: u32 = 1;
/// WebSocket ready-state: close handshake in progress.
pub const WS_CLOSING: u32 = 2;
/// WebSocket ready-state: connection is closed.
pub const WS_CLOSED: u32 = 3;

/// A received WebSocket message.
pub struct WsMessage {
    /// `true` when the payload is raw binary; `false` for UTF-8 text.
    pub is_binary: bool,
    /// Frame payload.
    pub data: Vec<u8>,
}

impl WsMessage {
    /// Interpret the payload as a UTF-8 string.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.data).to_string()
    }
}

/// Open a WebSocket connection to `url` (e.g. `"ws://example.com/chat"`).
///
/// Returns a connection handle (`> 0`) on success, or `0` on error.
/// The connection is established asynchronously; poll [`ws_ready_state`] until
/// it returns [`WS_OPEN`] before sending frames.
pub fn ws_connect(url: &str) -> u32 {
    unsafe { _api_ws_connect(url.as_ptr() as u32, url.len() as u32) }
}

/// Send a UTF-8 text frame on the given connection.
///
/// Returns `0` on success, `-1` if the connection is unknown or closed.
pub fn ws_send_text(id: u32, text: &str) -> i32 {
    unsafe { _api_ws_send_text(id, text.as_ptr() as u32, text.len() as u32) }
}

/// Send a binary frame on the given connection.
///
/// Returns `0` on success, `-1` if the connection is unknown or closed.
pub fn ws_send_binary(id: u32, data: &[u8]) -> i32 {
    unsafe { _api_ws_send_binary(id, data.as_ptr() as u32, data.len() as u32) }
}

/// Poll for the next queued incoming frame on `id`.
///
/// Returns `Some(WsMessage)` if a frame is available, or `None` if the queue
/// is empty.  The internal receive buffer is 64 KB; larger frames are
/// truncated to that size.
pub fn ws_recv(id: u32) -> Option<WsMessage> {
    let mut buf = vec![0u8; 64 * 1024];
    let result = unsafe { _api_ws_recv(id, buf.as_mut_ptr() as u32, buf.len() as u32) };
    if result < 0 {
        return None;
    }
    let len = (result & 0xFFFF_FFFF) as usize;
    let is_binary = (result >> 32) & 1 == 1;
    Some(WsMessage {
        is_binary,
        data: buf[..len].to_vec(),
    })
}

/// Query the current ready-state of a connection.
///
/// Returns one of [`WS_CONNECTING`], [`WS_OPEN`], [`WS_CLOSING`], or [`WS_CLOSED`].
pub fn ws_ready_state(id: u32) -> u32 {
    unsafe { _api_ws_ready_state(id) }
}

/// Initiate a graceful close handshake on `id`.
///
/// Returns `1` if the close was initiated, `0` if the handle is unknown.
/// After calling this function the connection will transition to [`WS_CLOSED`]
/// asynchronously.  Call [`ws_remove`] once the state is [`WS_CLOSED`] to free
/// host resources.
pub fn ws_close(id: u32) -> i32 {
    unsafe { _api_ws_close(id) }
}

/// Release host-side resources for a closed connection.
///
/// Call this after [`ws_ready_state`] returns [`WS_CLOSED`] to avoid resource
/// leaks.
pub fn ws_remove(id: u32) {
    unsafe { _api_ws_remove(id) }
}

// ─── MIDI API ────────────────────────────────────────────────────────────────

/// Number of available MIDI input ports (physical and virtual).
pub fn midi_input_count() -> u32 {
    unsafe { _api_midi_input_count() }
}

/// Number of available MIDI output ports.
pub fn midi_output_count() -> u32 {
    unsafe { _api_midi_output_count() }
}

/// Name of the MIDI input port at `index`.
///
/// Returns an empty string if the index is out of range.
pub fn midi_input_name(index: u32) -> String {
    let mut buf = [0u8; 128];
    let len = unsafe { _api_midi_input_name(index, buf.as_mut_ptr() as u32, buf.len() as u32) };
    String::from_utf8_lossy(&buf[..len as usize]).to_string()
}

/// Name of the MIDI output port at `index`.
///
/// Returns an empty string if the index is out of range.
pub fn midi_output_name(index: u32) -> String {
    let mut buf = [0u8; 128];
    let len = unsafe { _api_midi_output_name(index, buf.as_mut_ptr() as u32, buf.len() as u32) };
    String::from_utf8_lossy(&buf[..len as usize]).to_string()
}

/// Open a MIDI input port by index and start receiving messages.
///
/// Returns a handle (`> 0`) on success, or `0` if the port could not be opened.
/// Incoming messages are queued internally; drain them with [`midi_recv`].
pub fn midi_open_input(index: u32) -> u32 {
    unsafe { _api_midi_open_input(index) }
}

/// Open a MIDI output port by index for sending messages.
///
/// Returns a handle (`> 0`) on success, or `0` on failure.
pub fn midi_open_output(index: u32) -> u32 {
    unsafe { _api_midi_open_output(index) }
}

/// Send raw MIDI bytes on an output `handle`.
///
/// Returns `0` on success, `-1` if the handle is unknown or the send failed.
pub fn midi_send(handle: u32, data: &[u8]) -> i32 {
    unsafe { _api_midi_send(handle, data.as_ptr() as u32, data.len() as u32) }
}

/// Poll for the next queued MIDI message on an input `handle`.
///
/// Returns `Some(bytes)` with exactly one MIDI message if one is available,
/// or `None` if the queue is empty. Channel-voice messages are 2–3 bytes;
/// SysEx can be longer. The wrapper first tries a 256-byte stack buffer and
/// transparently retries with a 64 KB heap buffer for large SysEx dumps.
pub fn midi_recv(handle: u32) -> Option<Vec<u8>> {
    let mut buf = [0u8; 256];
    let n = unsafe { _api_midi_recv(handle, buf.as_mut_ptr() as u32, buf.len() as u32) };
    if n >= 0 {
        return Some(buf[..n as usize].to_vec());
    }
    // -2 = buffer too small; message is still queued. Retry with 64 KB heap buffer.
    if n == -2 {
        let mut big = vec![0u8; 64 * 1024];
        let n2 = unsafe { _api_midi_recv(handle, big.as_mut_ptr() as u32, big.len() as u32) };
        if n2 >= 0 {
            big.truncate(n2 as usize);
            return Some(big);
        }
    }
    None
}

/// Close a MIDI input or output handle and free host-side resources.
pub fn midi_close(handle: u32) {
    unsafe { _api_midi_close(handle) }
}

// ─── HTTP Fetch API ─────────────────────────────────────────────────────────

/// Response from an HTTP fetch call.
pub struct FetchResponse {
    pub status: u32,
    pub body: Vec<u8>,
}

impl FetchResponse {
    /// Interpret the response body as UTF-8 text.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
}

/// Perform an HTTP request.  Returns the status code and response body.
///
/// `content_type` sets the `Content-Type` header (pass `""` to omit).
/// Protobuf is the native format — use `"application/protobuf"` for binary
/// payloads.
pub fn fetch(
    method: &str,
    url: &str,
    content_type: &str,
    body: &[u8],
) -> Result<FetchResponse, i64> {
    let mut out_buf = vec![0u8; 4 * 1024 * 1024]; // 4 MB response buffer
    let result = unsafe {
        _api_fetch(
            method.as_ptr() as u32,
            method.len() as u32,
            url.as_ptr() as u32,
            url.len() as u32,
            content_type.as_ptr() as u32,
            content_type.len() as u32,
            body.as_ptr() as u32,
            body.len() as u32,
            out_buf.as_mut_ptr() as u32,
            out_buf.len() as u32,
        )
    };
    if result < 0 {
        return Err(result);
    }
    let status = (result >> 32) as u32;
    let body_len = (result & 0xFFFF_FFFF) as usize;
    Ok(FetchResponse {
        status,
        body: out_buf[..body_len].to_vec(),
    })
}

/// HTTP GET request.
pub fn fetch_get(url: &str) -> Result<FetchResponse, i64> {
    fetch("GET", url, "", &[])
}

/// HTTP POST with raw bytes.
pub fn fetch_post(url: &str, content_type: &str, body: &[u8]) -> Result<FetchResponse, i64> {
    fetch("POST", url, content_type, body)
}

/// HTTP POST with protobuf body (sets `Content-Type: application/protobuf`).
pub fn fetch_post_proto(url: &str, msg: &proto::ProtoEncoder) -> Result<FetchResponse, i64> {
    fetch("POST", url, "application/protobuf", msg.as_bytes())
}

/// HTTP PUT with raw bytes.
pub fn fetch_put(url: &str, content_type: &str, body: &[u8]) -> Result<FetchResponse, i64> {
    fetch("PUT", url, content_type, body)
}

/// HTTP DELETE.
pub fn fetch_delete(url: &str) -> Result<FetchResponse, i64> {
    fetch("DELETE", url, "", &[])
}

// ─── Streaming / non-blocking fetch ─────────────────────────────────────────
//
// The [`fetch`] family above blocks the guest until the response is fully
// downloaded. For LLM token streams, large downloads, chunked feeds, or any
// app that wants to keep rendering while a request is in flight, use the
// handle-based API below. It mirrors the WebSocket API: dispatch with
// `fetch_begin`, then poll `fetch_state`, `fetch_status`, and `fetch_recv`.

/// Request dispatched; waiting for response headers.
pub const FETCH_PENDING: u32 = 0;
/// Headers received; body chunks may still be arriving.
pub const FETCH_STREAMING: u32 = 1;
/// Body fully delivered (the queue may still have trailing chunks to drain).
pub const FETCH_DONE: u32 = 2;
/// Request failed. Call [`fetch_error`] for the message.
pub const FETCH_ERROR: u32 = 3;
/// Request was aborted by the guest.
pub const FETCH_ABORTED: u32 = 4;

/// Result of a non-blocking [`fetch_recv`] poll.
pub enum FetchChunk {
    /// One body chunk (may be part of a larger network chunk if it didn't fit
    /// in the caller's buffer).
    Data(Vec<u8>),
    /// No chunk is available right now, but more may still arrive. Call
    /// [`fetch_recv`] again next frame.
    Pending,
    /// The body has been fully delivered and all chunks have been drained.
    End,
    /// The request failed or was aborted. Inspect [`fetch_state`] and
    /// [`fetch_error`] for details.
    Error,
}

/// Dispatch an HTTP request that streams its response back to the guest.
///
/// Returns a handle (`> 0`) that identifies the request for subsequent polls,
/// or `0` if the host could not initialise the fetch subsystem. The call
/// returns immediately — the request is driven by a background task.
///
/// Pass `""` for `content_type` to omit the header, and `&[]` for `body` on
/// requests without a payload.
pub fn fetch_begin(method: &str, url: &str, content_type: &str, body: &[u8]) -> u32 {
    unsafe {
        _api_fetch_begin(
            method.as_ptr() as u32,
            method.len() as u32,
            url.as_ptr() as u32,
            url.len() as u32,
            content_type.as_ptr() as u32,
            content_type.len() as u32,
            body.as_ptr() as u32,
            body.len() as u32,
        )
    }
}

/// Convenience wrapper for GET.
pub fn fetch_begin_get(url: &str) -> u32 {
    fetch_begin("GET", url, "", &[])
}

/// Current lifecycle state of a streaming request. See the `FETCH_*` constants.
pub fn fetch_state(handle: u32) -> u32 {
    unsafe { _api_fetch_state(handle) }
}

/// HTTP status code for `handle`, or `0` until the response headers arrive.
pub fn fetch_status(handle: u32) -> u32 {
    unsafe { _api_fetch_status(handle) }
}

/// Poll the next body chunk into a caller-provided scratch buffer.
///
/// Use this form when you want to avoid per-chunk heap allocations. Prefer
/// [`fetch_recv`] for ergonomics in higher-level code.
///
/// Returns the number of bytes written into `buf` (which may be smaller than
/// the chunk the host has queued — in which case the remainder will be
/// returned on the next call), or one of the negative sentinels documented by
/// the host (`-1` pending, `-2` EOF, `-3` error, `-4` unknown handle).
pub fn fetch_recv_into(handle: u32, buf: &mut [u8]) -> i64 {
    unsafe { _api_fetch_recv(handle, buf.as_mut_ptr() as u32, buf.len() as u32) }
}

/// Poll the next body chunk as an owned `Vec<u8>`.
///
/// Chunks larger than 64 KiB are read in 64 KiB slices; call `fetch_recv`
/// repeatedly to drain the full network chunk.
pub fn fetch_recv(handle: u32) -> FetchChunk {
    let mut buf = vec![0u8; 64 * 1024];
    let n = fetch_recv_into(handle, &mut buf);
    match n {
        -1 => FetchChunk::Pending,
        -2 => FetchChunk::End,
        -3 | -4 => FetchChunk::Error,
        n if n >= 0 => {
            buf.truncate(n as usize);
            FetchChunk::Data(buf)
        }
        _ => FetchChunk::Error,
    }
}

/// Retrieve the error message for a failed request, if any.
pub fn fetch_error(handle: u32) -> Option<String> {
    let mut buf = [0u8; 512];
    let n = unsafe { _api_fetch_error(handle, buf.as_mut_ptr() as u32, buf.len() as u32) };
    if n < 0 {
        None
    } else {
        Some(String::from_utf8_lossy(&buf[..n as usize]).into_owned())
    }
}

/// Abort an in-flight request. Returns `true` if the handle was known.
///
/// The request transitions to [`FETCH_ABORTED`]; any body chunks already
/// queued remain readable via [`fetch_recv`] until drained.
pub fn fetch_abort(handle: u32) -> bool {
    unsafe { _api_fetch_abort(handle) != 0 }
}

/// Free host-side resources for a completed or aborted request.
///
/// Call this once you've finished draining [`fetch_recv`]. After removal the
/// handle is invalid.
pub fn fetch_remove(handle: u32) {
    unsafe { _api_fetch_remove(handle) }
}

// ─── Dynamic Module Loading ─────────────────────────────────────────────────

/// Fetch and execute another `.wasm` module from a URL.
/// The loaded module shares the same canvas, console, and storage context.
/// Returns 0 on success, negative error code on failure.
pub fn load_module(url: &str) -> i32 {
    unsafe { _api_load_module(url.as_ptr() as u32, url.len() as u32) }
}

// ─── Crypto / Hash API ─────────────────────────────────────────────────────

/// Compute the SHA-256 hash of the given data. Returns 32 bytes.
pub fn hash_sha256(data: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    unsafe {
        _api_hash_sha256(
            data.as_ptr() as u32,
            data.len() as u32,
            out.as_mut_ptr() as u32,
        );
    }
    out
}

/// Return SHA-256 hash as a lowercase hex string.
pub fn hash_sha256_hex(data: &[u8]) -> String {
    let hash = hash_sha256(data);
    let mut hex = String::with_capacity(64);
    for byte in &hash {
        hex.push(HEX_CHARS[(*byte >> 4) as usize]);
        hex.push(HEX_CHARS[(*byte & 0x0F) as usize]);
    }
    hex
}

const HEX_CHARS: [char; 16] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
];

// ─── Base64 API ─────────────────────────────────────────────────────────────

/// Base64-encode arbitrary bytes.
pub fn base64_encode(data: &[u8]) -> String {
    let mut buf = vec![0u8; data.len() * 4 / 3 + 8];
    let len = unsafe {
        _api_base64_encode(
            data.as_ptr() as u32,
            data.len() as u32,
            buf.as_mut_ptr() as u32,
            buf.len() as u32,
        )
    };
    String::from_utf8_lossy(&buf[..len as usize]).to_string()
}

/// Decode a base64-encoded string back to bytes.
pub fn base64_decode(encoded: &str) -> Vec<u8> {
    let mut buf = vec![0u8; encoded.len()];
    let len = unsafe {
        _api_base64_decode(
            encoded.as_ptr() as u32,
            encoded.len() as u32,
            buf.as_mut_ptr() as u32,
            buf.len() as u32,
        )
    };
    buf[..len as usize].to_vec()
}

// ─── Persistent Key-Value Store API ─────────────────────────────────────────

/// Store a key-value pair in the persistent on-disk KV store.
/// Returns `true` on success.
pub fn kv_store_set(key: &str, value: &[u8]) -> bool {
    let rc = unsafe {
        _api_kv_store_set(
            key.as_ptr() as u32,
            key.len() as u32,
            value.as_ptr() as u32,
            value.len() as u32,
        )
    };
    rc == 0
}

/// Convenience wrapper: store a UTF-8 string value.
pub fn kv_store_set_str(key: &str, value: &str) -> bool {
    kv_store_set(key, value.as_bytes())
}

/// Retrieve a value from the persistent KV store.
/// Returns `None` if the key does not exist.
pub fn kv_store_get(key: &str) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; 64 * 1024]; // 64 KB read buffer
    let rc = unsafe {
        _api_kv_store_get(
            key.as_ptr() as u32,
            key.len() as u32,
            buf.as_mut_ptr() as u32,
            buf.len() as u32,
        )
    };
    if rc < 0 {
        return None;
    }
    Some(buf[..rc as usize].to_vec())
}

/// Convenience wrapper: retrieve a UTF-8 string value.
pub fn kv_store_get_str(key: &str) -> Option<String> {
    kv_store_get(key).map(|v| String::from_utf8_lossy(&v).into_owned())
}

/// Delete a key from the persistent KV store. Returns `true` on success.
pub fn kv_store_delete(key: &str) -> bool {
    let rc = unsafe { _api_kv_store_delete(key.as_ptr() as u32, key.len() as u32) };
    rc == 0
}

// ─── Navigation API ─────────────────────────────────────────────────────────

/// Navigate to a new URL.  The URL can be absolute or relative to the current
/// page.  Navigation happens asynchronously after the current `start_app`
/// returns.  Returns 0 on success, negative on invalid URL.
pub fn navigate(url: &str) -> i32 {
    unsafe { _api_navigate(url.as_ptr() as u32, url.len() as u32) }
}

/// Push a new entry onto the browser's history stack without triggering a
/// module reload.  This is analogous to `history.pushState()` in web browsers.
///
/// - `state`:  Opaque binary data retrievable later via [`get_state`].
/// - `title`:  Human-readable title for the history entry.
/// - `url`:    The URL to display in the address bar (relative or absolute).
///             Pass `""` to keep the current URL.
pub fn push_state(state: &[u8], title: &str, url: &str) {
    unsafe {
        _api_push_state(
            state.as_ptr() as u32,
            state.len() as u32,
            title.as_ptr() as u32,
            title.len() as u32,
            url.as_ptr() as u32,
            url.len() as u32,
        )
    }
}

/// Replace the current history entry (no new entry is pushed).
/// Analogous to `history.replaceState()`.
pub fn replace_state(state: &[u8], title: &str, url: &str) {
    unsafe {
        _api_replace_state(
            state.as_ptr() as u32,
            state.len() as u32,
            title.as_ptr() as u32,
            title.len() as u32,
            url.as_ptr() as u32,
            url.len() as u32,
        )
    }
}

/// Get the URL of the currently loaded page.
pub fn get_url() -> String {
    let mut buf = [0u8; 4096];
    let len = unsafe { _api_get_url(buf.as_mut_ptr() as u32, buf.len() as u32) };
    String::from_utf8_lossy(&buf[..len as usize]).to_string()
}

/// Retrieve the opaque state bytes attached to the current history entry.
/// Returns `None` if no state has been set.
pub fn get_state() -> Option<Vec<u8>> {
    let mut buf = vec![0u8; 64 * 1024]; // 64 KB
    let rc = unsafe { _api_get_state(buf.as_mut_ptr() as u32, buf.len() as u32) };
    if rc < 0 {
        return None;
    }
    Some(buf[..rc as usize].to_vec())
}

/// Return the total number of entries in the history stack.
pub fn history_length() -> u32 {
    unsafe { _api_history_length() }
}

/// Navigate backward in history.  Returns `true` if a navigation was queued.
pub fn history_back() -> bool {
    unsafe { _api_history_back() == 1 }
}

/// Navigate forward in history.  Returns `true` if a navigation was queued.
pub fn history_forward() -> bool {
    unsafe { _api_history_forward() == 1 }
}

// ─── Hyperlink API ──────────────────────────────────────────────────────────

/// Register a rectangular region on the canvas as a clickable hyperlink.
///
/// When the user clicks inside the rectangle the browser navigates to `url`.
/// Coordinates are in the same canvas-local space used by the drawing APIs.
/// Returns 0 on success.
pub fn register_hyperlink(x: f32, y: f32, w: f32, h: f32, url: &str) -> i32 {
    unsafe { _api_register_hyperlink(x, y, w, h, url.as_ptr() as u32, url.len() as u32) }
}

/// Remove all previously registered hyperlinks.
pub fn clear_hyperlinks() {
    unsafe { _api_clear_hyperlinks() }
}

// ─── URL Utility API ────────────────────────────────────────────────────────

/// Resolve a relative URL against a base URL (WHATWG algorithm).
/// Returns `None` if either URL is invalid.
pub fn url_resolve(base: &str, relative: &str) -> Option<String> {
    let mut buf = [0u8; 4096];
    let rc = unsafe {
        _api_url_resolve(
            base.as_ptr() as u32,
            base.len() as u32,
            relative.as_ptr() as u32,
            relative.len() as u32,
            buf.as_mut_ptr() as u32,
            buf.len() as u32,
        )
    };
    if rc < 0 {
        return None;
    }
    Some(String::from_utf8_lossy(&buf[..rc as usize]).to_string())
}

/// Percent-encode a string for safe inclusion in URL components.
pub fn url_encode(input: &str) -> String {
    let mut buf = vec![0u8; input.len() * 3 + 4];
    let len = unsafe {
        _api_url_encode(
            input.as_ptr() as u32,
            input.len() as u32,
            buf.as_mut_ptr() as u32,
            buf.len() as u32,
        )
    };
    String::from_utf8_lossy(&buf[..len as usize]).to_string()
}

/// Decode a percent-encoded string.
pub fn url_decode(input: &str) -> String {
    let mut buf = vec![0u8; input.len() + 4];
    let len = unsafe {
        _api_url_decode(
            input.as_ptr() as u32,
            input.len() as u32,
            buf.as_mut_ptr() as u32,
            buf.len() as u32,
        )
    };
    String::from_utf8_lossy(&buf[..len as usize]).to_string()
}

// ─── Input Polling API ──────────────────────────────────────────────────────

/// Get the mouse position in canvas-local coordinates.
pub fn mouse_position() -> (f32, f32) {
    let packed = unsafe { _api_mouse_position() };
    let x = f32::from_bits((packed >> 32) as u32);
    let y = f32::from_bits((packed & 0xFFFF_FFFF) as u32);
    (x, y)
}

/// Returns `true` if the given mouse button is currently held down.
/// Button 0 = primary (left), 1 = secondary (right), 2 = middle.
pub fn mouse_button_down(button: u32) -> bool {
    unsafe { _api_mouse_button_down(button) != 0 }
}

/// Returns `true` if the given mouse button was clicked this frame.
pub fn mouse_button_clicked(button: u32) -> bool {
    unsafe { _api_mouse_button_clicked(button) != 0 }
}

/// Returns `true` if the given key is currently held down.
/// See `KEY_*` constants for key codes.
pub fn key_down(key: u32) -> bool {
    unsafe { _api_key_down(key) != 0 }
}

/// Returns `true` if the given key was pressed this frame.
pub fn key_pressed(key: u32) -> bool {
    unsafe { _api_key_pressed(key) != 0 }
}

/// Get the scroll wheel delta for this frame.
pub fn scroll_delta() -> (f32, f32) {
    let packed = unsafe { _api_scroll_delta() };
    let x = f32::from_bits((packed >> 32) as u32);
    let y = f32::from_bits((packed & 0xFFFF_FFFF) as u32);
    (x, y)
}

/// Returns modifier key state as a bitmask: bit 0 = Shift, bit 1 = Ctrl, bit 2 = Alt.
pub fn modifiers() -> u32 {
    unsafe { _api_modifiers() }
}

/// Returns `true` if Shift is held.
pub fn shift_held() -> bool {
    modifiers() & 1 != 0
}

/// Returns `true` if Ctrl (or Cmd on macOS) is held.
pub fn ctrl_held() -> bool {
    modifiers() & 2 != 0
}

/// Returns `true` if Alt is held.
pub fn alt_held() -> bool {
    modifiers() & 4 != 0
}

// ─── Key Constants ──────────────────────────────────────────────────────────

pub const KEY_A: u32 = 0;
pub const KEY_B: u32 = 1;
pub const KEY_C: u32 = 2;
pub const KEY_D: u32 = 3;
pub const KEY_E: u32 = 4;
pub const KEY_F: u32 = 5;
pub const KEY_G: u32 = 6;
pub const KEY_H: u32 = 7;
pub const KEY_I: u32 = 8;
pub const KEY_J: u32 = 9;
pub const KEY_K: u32 = 10;
pub const KEY_L: u32 = 11;
pub const KEY_M: u32 = 12;
pub const KEY_N: u32 = 13;
pub const KEY_O: u32 = 14;
pub const KEY_P: u32 = 15;
pub const KEY_Q: u32 = 16;
pub const KEY_R: u32 = 17;
pub const KEY_S: u32 = 18;
pub const KEY_T: u32 = 19;
pub const KEY_U: u32 = 20;
pub const KEY_V: u32 = 21;
pub const KEY_W: u32 = 22;
pub const KEY_X: u32 = 23;
pub const KEY_Y: u32 = 24;
pub const KEY_Z: u32 = 25;
pub const KEY_0: u32 = 26;
pub const KEY_1: u32 = 27;
pub const KEY_2: u32 = 28;
pub const KEY_3: u32 = 29;
pub const KEY_4: u32 = 30;
pub const KEY_5: u32 = 31;
pub const KEY_6: u32 = 32;
pub const KEY_7: u32 = 33;
pub const KEY_8: u32 = 34;
pub const KEY_9: u32 = 35;
pub const KEY_ENTER: u32 = 36;
pub const KEY_ESCAPE: u32 = 37;
pub const KEY_TAB: u32 = 38;
pub const KEY_BACKSPACE: u32 = 39;
pub const KEY_DELETE: u32 = 40;
pub const KEY_SPACE: u32 = 41;
pub const KEY_UP: u32 = 42;
pub const KEY_DOWN: u32 = 43;
pub const KEY_LEFT: u32 = 44;
pub const KEY_RIGHT: u32 = 45;
pub const KEY_HOME: u32 = 46;
pub const KEY_END: u32 = 47;
pub const KEY_PAGE_UP: u32 = 48;
pub const KEY_PAGE_DOWN: u32 = 49;

// ─── Interactive Widget API ─────────────────────────────────────────────────

/// Render a button at the given position. Returns `true` if it was clicked
/// on the previous frame.
///
/// Must be called from `on_frame()` — widgets are only rendered for
/// interactive applications that export a frame loop.
pub fn ui_button(id: u32, x: f32, y: f32, w: f32, h: f32, label: &str) -> bool {
    unsafe { _api_ui_button(id, x, y, w, h, label.as_ptr() as u32, label.len() as u32) != 0 }
}

/// Render a checkbox. Returns the current checked state.
///
/// `initial` sets the value the first time this ID is seen.
pub fn ui_checkbox(id: u32, x: f32, y: f32, label: &str, initial: bool) -> bool {
    unsafe {
        _api_ui_checkbox(
            id,
            x,
            y,
            label.as_ptr() as u32,
            label.len() as u32,
            if initial { 1 } else { 0 },
        ) != 0
    }
}

/// Render a slider. Returns the current value.
///
/// `initial` sets the value the first time this ID is seen.
pub fn ui_slider(id: u32, x: f32, y: f32, w: f32, min: f32, max: f32, initial: f32) -> f32 {
    unsafe { _api_ui_slider(id, x, y, w, min, max, initial) }
}

/// Render a single-line text input. Returns the current text content.
///
/// `initial` sets the text the first time this ID is seen.
pub fn ui_text_input(id: u32, x: f32, y: f32, w: f32, initial: &str) -> String {
    let mut buf = [0u8; 4096];
    let len = unsafe {
        _api_ui_text_input(
            id,
            x,
            y,
            w,
            initial.as_ptr() as u32,
            initial.len() as u32,
            buf.as_mut_ptr() as u32,
            buf.len() as u32,
        )
    };
    String::from_utf8_lossy(&buf[..len as usize]).to_string()
}
