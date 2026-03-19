//! # Oxide SDK
//!
//! Guest-side SDK for building WebAssembly applications that run inside the
//! Oxide browser. This crate provides safe Rust wrappers around the raw
//! host-imported functions exposed by the `"oxide"` module.
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use oxide_sdk::*;
//!
//! #[no_mangle]
//! pub extern "C" fn start_app() {
//!     log("Hello from Oxide!");
//!     canvas_clear(30, 30, 46, 255);
//!     canvas_text(20.0, 40.0, 28.0, 255, 255, 255, "Welcome to Oxide");
//! }
//! ```

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

    #[link_name = "api_canvas_clear"]
    fn _api_canvas_clear(r: u32, g: u32, b: u32, a: u32);

    #[link_name = "api_canvas_rect"]
    fn _api_canvas_rect(x: f32, y: f32, w: f32, h: f32, r: u32, g: u32, b: u32, a: u32);

    #[link_name = "api_canvas_circle"]
    fn _api_canvas_circle(cx: f32, cy: f32, radius: f32, r: u32, g: u32, b: u32, a: u32);

    #[link_name = "api_canvas_text"]
    fn _api_canvas_text(x: f32, y: f32, size: f32, r: u32, g: u32, b: u32, ptr: u32, len: u32);

    #[link_name = "api_canvas_line"]
    fn _api_canvas_line(x1: f32, y1: f32, x2: f32, y2: f32, r: u32, g: u32, b: u32, thickness: f32);

    #[link_name = "api_canvas_dimensions"]
    fn _api_canvas_dimensions() -> u64;

    #[link_name = "api_canvas_image"]
    fn _api_canvas_image(x: f32, y: f32, w: f32, h: f32, data_ptr: u32, data_len: u32);

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
    fn _api_ui_slider(
        id: u32,
        x: f32,
        y: f32,
        w: f32,
        min: f32,
        max: f32,
        initial: f32,
    ) -> f32;

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

/// Draw text on the canvas.
pub fn canvas_text(x: f32, y: f32, size: f32, r: u8, g: u8, b: u8, text: &str) {
    unsafe {
        _api_canvas_text(
            x,
            y,
            size,
            r as u32,
            g as u32,
            b as u32,
            text.as_ptr() as u32,
            text.len() as u32,
        )
    }
}

/// Draw a line between two points.
pub fn canvas_line(x1: f32, y1: f32, x2: f32, y2: f32, r: u8, g: u8, b: u8, thickness: f32) {
    unsafe { _api_canvas_line(x1, y1, x2, y2, r as u32, g as u32, b as u32, thickness) }
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
    unsafe {
        _api_ui_button(
            id,
            x,
            y,
            w,
            h,
            label.as_ptr() as u32,
            label.len() as u32,
        ) != 0
    }
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
