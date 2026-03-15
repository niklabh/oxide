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
    fn _api_upload_file(
        name_ptr: u32,
        name_cap: u32,
        data_ptr: u32,
        data_cap: u32,
    ) -> u64;

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
        method_ptr: u32, method_len: u32,
        url_ptr: u32, url_len: u32,
        ct_ptr: u32, ct_len: u32,
        body_ptr: u32, body_len: u32,
        out_ptr: u32, out_cap: u32,
    ) -> i64;

    #[link_name = "api_load_module"]
    fn _api_load_module(url_ptr: u32, url_len: u32) -> i32;

    #[link_name = "api_hash_sha256"]
    fn _api_hash_sha256(data_ptr: u32, data_len: u32, out_ptr: u32) -> u32;

    #[link_name = "api_base64_encode"]
    fn _api_base64_encode(data_ptr: u32, data_len: u32, out_ptr: u32, out_cap: u32) -> u32;

    #[link_name = "api_base64_decode"]
    fn _api_base64_decode(data_ptr: u32, data_len: u32, out_ptr: u32, out_cap: u32) -> u32;
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
            x, y, size,
            r as u32, g as u32, b as u32,
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
    unsafe {
        _api_canvas_image(x, y, w, h, data.as_ptr() as u32, data.len() as u32)
    }
}

// ─── Local Storage API ──────────────────────────────────────────────────────

/// Store a key-value pair in sandboxed local storage.
pub fn storage_set(key: &str, value: &str) {
    unsafe {
        _api_storage_set(
            key.as_ptr() as u32, key.len() as u32,
            value.as_ptr() as u32, value.len() as u32,
        )
    }
}

/// Retrieve a value from local storage. Returns empty string if not found.
pub fn storage_get(key: &str) -> String {
    let mut buf = [0u8; 4096];
    let len = unsafe {
        _api_storage_get(
            key.as_ptr() as u32, key.len() as u32,
            buf.as_mut_ptr() as u32, buf.len() as u32,
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
            title.as_ptr() as u32, title.len() as u32,
            body.as_ptr() as u32, body.len() as u32,
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
pub fn fetch(method: &str, url: &str, content_type: &str, body: &[u8]) -> Result<FetchResponse, i64> {
    let mut out_buf = vec![0u8; 4 * 1024 * 1024]; // 4 MB response buffer
    let result = unsafe {
        _api_fetch(
            method.as_ptr() as u32, method.len() as u32,
            url.as_ptr() as u32, url.len() as u32,
            content_type.as_ptr() as u32, content_type.len() as u32,
            body.as_ptr() as u32, body.len() as u32,
            out_buf.as_mut_ptr() as u32, out_buf.len() as u32,
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
        _api_hash_sha256(data.as_ptr() as u32, data.len() as u32, out.as_mut_ptr() as u32);
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
    '0', '1', '2', '3', '4', '5', '6', '7',
    '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
];

// ─── Base64 API ─────────────────────────────────────────────────────────────

/// Base64-encode arbitrary bytes.
pub fn base64_encode(data: &[u8]) -> String {
    let mut buf = vec![0u8; data.len() * 4 / 3 + 8];
    let len = unsafe {
        _api_base64_encode(
            data.as_ptr() as u32, data.len() as u32,
            buf.as_mut_ptr() as u32, buf.len() as u32,
        )
    };
    String::from_utf8_lossy(&buf[..len as usize]).to_string()
}

/// Decode a base64-encoded string back to bytes.
pub fn base64_decode(encoded: &str) -> Vec<u8> {
    let mut buf = vec![0u8; encoded.len()];
    let len = unsafe {
        _api_base64_decode(
            encoded.as_ptr() as u32, encoded.len() as u32,
            buf.as_mut_ptr() as u32, buf.len() as u32,
        )
    };
    buf[..len as usize].to_vec()
}
