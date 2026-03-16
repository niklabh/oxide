use oxide_sdk::proto::ProtoEncoder;
use oxide_sdk::*;

#[no_mangle]
pub extern "C" fn start_app() {
    log("Hello from the Oxide guest app!");

    let (width, height) = canvas_dimensions();
    log(&format!("Canvas size: {}x{}", width, height));

    canvas_clear(30, 30, 46, 255);

    // Title bar
    canvas_rect(0.0, 0.0, width as f32, 60.0, 50, 40, 80, 255);
    canvas_text(20.0, 18.0, 24.0, 220, 200, 255, "Hello, Oxide!");

    // Info text
    canvas_text(
        20.0,
        80.0,
        16.0,
        200,
        200,
        200,
        "This app is running as a .wasm module inside the Oxide browser.",
    );
    canvas_text(
        20.0,
        105.0,
        16.0,
        200,
        200,
        200,
        "It has zero access to the host filesystem or network sockets.",
    );

    // ── Protobuf Demo ────────────────────────────────────────────────
    canvas_text(
        20.0,
        150.0,
        18.0,
        180,
        140,
        255,
        "Protobuf (native wire format)",
    );

    let msg = ProtoEncoder::new()
        .string(1, "alice")
        .uint64(2, 42)
        .bool(3, true)
        .double(4, 3.14159);
    let encoded = msg.finish();

    canvas_text(
        30.0,
        175.0,
        14.0,
        160,
        220,
        160,
        &format!("Encoded {} bytes: {:02X?}", encoded.len(), &encoded),
    );
    log(&format!("Proto encoded: {} bytes", encoded.len()));

    let mut decoder = proto::ProtoDecoder::new(&encoded);
    let mut decoded_parts = Vec::new();
    while let Some(field) = decoder.next() {
        let desc = match field.number {
            1 => format!("name={}", field.as_str()),
            2 => format!("age={}", field.as_u64()),
            3 => format!("active={}", field.as_bool()),
            4 => format!("pi={:.5}", field.as_f64()),
            _ => format!("?{}", field.number),
        };
        decoded_parts.push(desc);
    }
    canvas_text(
        30.0,
        195.0,
        14.0,
        160,
        220,
        160,
        &format!("Decoded: {}", decoded_parts.join(", ")),
    );
    log(&format!("Proto decoded: {}", decoded_parts.join(", ")));

    // ── Crypto / Hash Demo ───────────────────────────────────────────
    canvas_text(20.0, 230.0, 18.0, 180, 140, 255, "SHA-256 & Base64");

    let hash_hex = hash_sha256_hex(b"Hello, Oxide!");
    canvas_text(
        30.0,
        255.0,
        14.0,
        160,
        220,
        160,
        &format!("sha256(\"Hello, Oxide!\") = {}...", &hash_hex[..32]),
    );
    log(&format!("SHA-256: {}", hash_hex));

    let b64 = base64_encode(b"Oxide Browser v0.1");
    canvas_text(
        30.0,
        275.0,
        14.0,
        160,
        220,
        160,
        &format!("base64 encode = {}", b64),
    );
    let roundtrip = base64_decode(&b64);
    canvas_text(
        30.0,
        295.0,
        14.0,
        160,
        220,
        160,
        &format!("base64 decode = {}", String::from_utf8_lossy(&roundtrip)),
    );

    // ── Existing API demos ───────────────────────────────────────────
    canvas_text(20.0, 330.0, 18.0, 180, 140, 255, "Platform APIs");

    let location = get_location();
    canvas_text(
        30.0,
        355.0,
        14.0,
        160,
        220,
        160,
        &format!("Geolocation: {}", location),
    );

    storage_set("visit_count", "1");
    let count = storage_get("visit_count");
    canvas_text(
        30.0,
        375.0,
        14.0,
        160,
        220,
        160,
        &format!("Storage: visit_count = {}", count),
    );

    let now = time_now_ms();
    canvas_text(
        30.0,
        395.0,
        14.0,
        160,
        220,
        160,
        &format!("Time: {} ms since epoch", now),
    );

    let rand_val = random_f64();
    canvas_text(
        30.0,
        415.0,
        14.0,
        160,
        220,
        160,
        &format!("Random: {:.6}", rand_val),
    );

    // Decorative circles
    canvas_circle(width as f32 - 120.0, 300.0, 60.0, 180, 120, 255, 150);
    canvas_circle(width as f32 - 160.0, 260.0, 30.0, 255, 180, 100, 130);
    canvas_circle(width as f32 - 80.0, 280.0, 40.0, 100, 220, 180, 130);

    notify("Oxide App", "Guest application loaded successfully!");
    log("start_app() completed.");
}
