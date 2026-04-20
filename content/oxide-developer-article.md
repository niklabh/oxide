# Building for Oxide: A Developer's Journey into the Binary-First Browser

The first time I opened the Oxide browser and watched a `.wasm` file load — no HTML parser, no DOM tree, no JavaScript engine spinning up — something clicked. The address bar pointed to a binary. The binary drew pixels. That was it. No build pipeline of seventeen tools transpiling, bundling, and tree-shaking my code into something the browser would tolerate. Just code, compiled, running.

If you've spent years writing for the web, you know the weight of it. The HTML spec alone is over a thousand pages. CSS has become its own programming language. JavaScript runtimes are marvels of engineering precisely because they have to be — they're trying to make a document format pretend to be an application platform. We've gotten very good at this strange dance, but it's worth asking whether the dance was ever necessary.

Oxide asks that question and answers it differently. This article is a tour through what that answer looks like in practice, and how you can start building with it today.

## Why Binary-First Matters

Let's start with the obvious objection: WebAssembly already runs in regular browsers. You can compile Rust, C++, or Go to `.wasm` and load it from a `<script>` tag. So what's the point of a browser whose primary citizen is the binary itself?

The point is everything that comes with the old model that you can finally leave behind:

- **No HTML parsing**, no quirks mode, no whitespace ambiguities, no character encoding guesses
- **No CSS cascade** to debug at 2 AM when one nested selector overrides another in ways nobody can predict
- **No JavaScript glue layer** between your app and the platform — your code talks directly to the host
- **No npm dependency tree** with thousands of transitive packages and their associated supply-chain risk
- **No bundler configuration** to maintain, no source maps to misconfigure, no polyfills to ship
- **No retained DOM** that you have to convince to match your application state through reconciliation
- **A real sandbox** rather than the patchwork of same-origin policies, CORS, CSP, and SRI hopes

Instead you get a single binary, a single entry point, a single rendering loop, and a clear list of capabilities the host will let you call. That's it. The whole platform fits in your head.

## What Oxide Actually Is

Oxide is a desktop browser written in Rust. It uses `wasmtime` to execute guest WebAssembly modules, and `GPUI` (the GPU-accelerated UI framework from the Zed editor) to paint the window. When you point it at a URL, it does something refreshingly simple:

1. Fetches the bytes
2. Compiles them as a WebAssembly module with bounded memory and fuel
3. Links them against an `oxide` import namespace containing roughly 150 host functions
4. Instantiates the module and calls `start_app()`
5. If the module exports `on_frame(dt_ms)`, calls it every frame forever

That's the entire lifecycle. There's no parser, no layout engine, no style resolver, no DOM, no event bubbling, no shadow tree. Your code is the application; the browser is the operating system.

A few characteristics define how Oxide feels in practice:

- **Capability-based security**: a freshly loaded module starts with zero ability to affect the world. It can compute, but it cannot draw, network, store, or even log without explicitly calling a host function. The sandbox is airtight by construction because nothing was ever granted in the first place.
- **Immediate-mode rendering**: there is no scene graph. Every frame, you tell the browser what to draw and where. If you don't draw something this frame, it's not on the screen. This sounds like extra work — and sometimes it is — but it eliminates an entire class of state-synchronization bugs.
- **Bounded resources**: each frame call gets 500 million instructions of fuel and your module is capped at 256MB of linear memory. Runaway loops fail loudly and quickly instead of melting your laptop.
- **Rust end-to-end**: the host is Rust, the SDK is Rust, the examples are Rust. You can read the entire stack. There's no opaque C++ engine to defer to.
- **A real set of capabilities**: canvas drawing, GPU compute via WGSL, hardware-accelerated video, multi-channel audio, WebRTC peer connections, WebSockets, MIDI I/O, camera and microphone capture, persistent storage, clipboard, crypto primitives, timers, and more.

This isn't a toy. The example folder ships with a full video player using FFmpeg, a WebRTC chat client, a GPU graphics demo with WGSL shaders, a streaming HTTP demo, and even a fullstack notes app with a Rust backend. The platform is real enough to build real things.

## Setting Up Your Workstation

Getting started takes about ten minutes. Here's what I'd do on a fresh machine:

1. Install Rust from https://rustup.rs — this gives you `rustc`, `cargo`, and the toolchain manager
2. Add the WebAssembly target so you can compile guest modules:
   ```
   rustup target add wasm32-unknown-unknown
   ```
3. Clone the Oxide repository to get both the browser and the SDK:
   ```
   git clone https://github.com/niklabh/oxide.git
   cd oxide
   ```
4. (macOS) Install FFmpeg if you want video support — many of the demos depend on it:
   ```
   brew install ffmpeg
   ```
5. Build and run the browser itself:
   ```
   cargo run -p oxide-browser
   ```

The first compile will take a while because there's a lot to build — wasmtime, GPUI, FFmpeg bindings, the whole graphics stack. Subsequent runs are fast. When the window opens you'll see a URL bar, a canvas area, and a console pane underneath. That's your development environment.

While you're waiting for that first build, try opening the example index app or the demo hub by running:

```
cargo build --target wasm32-unknown-unknown --release -p hello-oxide
```

Then click the "Open" button in the browser and select `target/wasm32-unknown-unknown/release/hello_oxide.wasm`. You'll see a counter app respond to button clicks. Congratulations, you've just run your first Oxide application.

## Your First App, Line by Line

Let's build something from scratch. The goal is a small interactive sketch — a counter with buttons, a mouse-tracking circle, and some text. Nothing fancy, but enough to touch the core APIs.

### Create the project

```bash
cargo new --lib my-first-oxide-app
cd my-first-oxide-app
```

### Configure Cargo.toml

The critical line here is `crate-type = ["cdylib"]`. Without it, Cargo will produce a Rust library (rlib) that Oxide cannot load. The `cdylib` type tells the compiler to emit a standalone WebAssembly module with a proper export table.

```toml
[package]
name = "my-first-oxide-app"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
oxide-sdk = { path = "../oxide/oxide-sdk" }
```

Adjust the path to point at wherever you cloned the Oxide repository. Once `oxide-sdk` is published more widely, you'll be able to use a version from crates.io instead.

### Write the app

Open `src/lib.rs` and replace its contents with:

```rust
use oxide_sdk::*;

static mut COUNTER: i32 = 0;

#[no_mangle]
pub extern "C" fn start_app() {
    log("My first Oxide app has started");
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    canvas_clear(30, 30, 46, 255);

    canvas_text(20.0, 40.0, 28.0, 255, 255, 255, 255, "Hello from Oxide!");

    if ui_button(1, 20.0, 80.0, 140.0, 32.0, "Increment") {
        unsafe { COUNTER += 1; }
    }
    if ui_button(2, 180.0, 80.0, 100.0, 32.0, "Reset") {
        unsafe { COUNTER = 0; }
    }

    let count = unsafe { COUNTER };
    canvas_text(
        20.0, 140.0, 20.0,
        100, 200, 150, 255,
        &format!("Count: {}", count),
    );

    let (mx, my) = mouse_position();
    canvas_circle(mx, my, 8.0, 255, 100, 100, 180);
}
```

A few things worth noting in this tiny program:

- `start_app` runs once when the module loads. Use it for one-time setup. Here it just logs a line to the console.
- `on_frame` runs every frame. Everything you see on screen is redrawn from scratch each time it's called. If you stop drawing the text, the text disappears.
- `static mut` is the simplest way to keep state across frames. Yes, it's `unsafe`. Yes, in larger apps you'll want a more disciplined pattern (a `Mutex<RefCell<State>>` lazy_static, a thread-local, or your own arena). For a counter, this is fine.
- The widget IDs (the `1` and `2` in `ui_button`) are how the host identifies a widget across frames so it can track its state. Pick unique numbers per widget.
- Input is *polled*, not delivered through events. The mouse position is just a function call. The button click is the return value of `ui_button`. This is the immediate-mode style throughout.

### Build and run

```bash
cargo build --target wasm32-unknown-unknown --release
```

The output appears at `target/wasm32-unknown-unknown/release/my_first_oxide_app.wasm`. Open it in Oxide via the file picker, or serve it locally with `python3 -m http.server 8080` and navigate to `http://localhost:8080/my_first_oxide_app.wasm`.

That's a complete app. Compiled, it's probably under 100KB. There's no `index.html`, no `webpack.config.js`, no `tsconfig.json`. Just a binary that draws.

## The Mental Model You Need

A few concepts will save you hours of confusion as you go deeper. They're all simple individually but they shape how every Oxide program is structured.

### The guest contract

Every Oxide module is required to export `start_app()`. It's the entry point and it runs exactly once. After that, the host calls one or more of these optional exports if they exist:

- `on_frame(dt_ms: u32)` — called every frame for interactive apps; this is your render loop
- `on_timer(callback_id: u32)` — called when a timer or `request_animation_frame` callback fires
- That's pretty much it

The guest never starts threads, never opens sockets, never spawns processes. It just runs whichever of these functions the host calls and uses host APIs to interact with the world.

### Polling, not callbacks

Coming from JavaScript, your instinct is to attach event listeners. Oxide doesn't work that way. Almost everything that *could* be event-driven — mouse input, keyboard input, network messages, timer fires, WebSocket frames, WebRTC data channel messages, MIDI packets — is instead something you drain inside `on_frame()`.

This sounds primitive. In practice it's wonderful. The control flow is linear and obvious. There are no race conditions between event handlers. Reasoning about a frame is just reasoning about a function call.

### The host-guest boundary

Data crosses between your guest module and the host through the guest's linear memory. When you call `log("hello")`, the SDK passes a pointer and length into the host, the host reads those bytes out of your memory, decodes them as UTF-8, and writes them to the console. You almost never deal with this directly because the SDK wraps it, but it's worth knowing what's happening underneath.

This is also why Oxide is fast. There's no JSON marshaling, no V8 boundary crossing, no string-to-DOM-Node bridge. Just memory reads.

### Fuel is finite

Each `on_frame` call gets 500 million instructions of fuel. That sounds like a lot — and it is, for normal interactive work — but it's a hard limit. If you try to brute-force a million-iteration computation in a single frame, the engine will trap and your app will halt with a fuel exhaustion error.

The right pattern for heavy computation is to spread the work across frames. Compute a chunk per frame, store progress in your state, and resume next frame. This also keeps your UI responsive, which is the same reason web developers split work across `requestAnimationFrame` calls.

### Memory is not free

256MB is the ceiling. That's enough for almost anything, but if you're loading raw video frames or huge image data into linear memory, watch your allocations. The host can hold large textures and audio buffers on its side without counting against your guest's budget — that's why image decoding and video rendering happen on the host.

## A Tour of What Oxide Can Do

The breadth of capabilities in the SDK is the part that most surprised me. This is not a stripped-down "demo platform." It's a serious application runtime. Let me walk through the categories that matter.

### Drawing on the canvas

The canvas is your primary output surface. There are two ways to use it.

The low-level functions are direct calls that take primitives:

- `canvas_clear(r, g, b, a)` to wipe with a color
- `canvas_rect(x, y, w, h, r, g, b, a)` for filled rectangles
- `canvas_circle(cx, cy, radius, r, g, b, a)` for filled circles
- `canvas_text(x, y, size, r, g, b, a, text)` for hardware-shaped text
- `canvas_line(x1, y1, x2, y2, r, g, b, a, thickness)` for lines
- `canvas_image(x, y, w, h, encoded_bytes)` for PNG/JPEG/WebP/GIF images
- `canvas_dimensions()` to query the current viewport

The high-level API in `oxide_sdk::draw` wraps these in proper types. You get `Color` (with named constants and hex constructors), `Point2D`, `Rect` (with hit testing), and a `Canvas` facade. For new code, prefer the high-level API — it's a thin zero-cost wrapper that makes the call sites read much better:

```rust
use oxide_sdk::draw::*;

let c = Canvas::new();
c.clear(Color::hex(0x1e1e2e));
c.fill_rect(Rect::new(20.0, 20.0, 200.0, 100.0), Color::BLUE);
c.fill_circle(Point2D::new(300.0, 200.0), 50.0, Color::RED);
c.text("Hello!", Point2D::new(20.0, 30.0), 24.0, Color::WHITE);
```

There's also `canvas_rounded_rect`, `canvas_arc`, `canvas_bezier`, gradient fills (linear and radial), affine transformations, clipping regions, and layer opacity for more sophisticated rendering.

### Widgets and input

The widget toolkit is small but covers the basics:

- `ui_button(id, x, y, w, h, label)` returns `true` on click
- `ui_checkbox(id, x, y, label, initial)` returns the current checked state
- `ui_slider(id, x, y, w, min, max, initial)` returns the current value
- `ui_text_input(id, x, y, w, initial)` returns the current text

Widgets render as native GPUI elements overlaid on your canvas, so they get system-quality text input and focus handling for free.

For raw input you have:

- `mouse_position()`, `mouse_button_down(button)`, `mouse_button_clicked(button)`
- `key_down(key)`, `key_pressed(key)` with constants like `KEY_A`, `KEY_ENTER`, `KEY_UP`
- `scroll_delta()` for the mouse wheel
- `shift_held()`, `ctrl_held()`, `alt_held()` and a raw `modifiers()` bitmask

### Networking

There are three ways to talk to the network, depending on what you need.

For ordinary HTTP requests, `fetch_get`, `fetch_post`, `fetch_put`, `fetch_delete`, and the more general `fetch` cover the common cases. They block the calling frame until the response is in, which is fine for small payloads but bad for large ones.

For large or long-lived responses, the streaming API is your friend. You call `fetch_begin_get(url)` to dispatch a request and get back a handle, then poll `fetch_recv(handle)` each frame to receive chunks as they arrive. This is how you'd implement progress bars, server-sent events, or HLS playlist parsing.

For bidirectional, low-latency communication there's WebSocket support: `ws_connect(url)` opens a connection, `ws_send_text` and `ws_send_binary` push messages out, and `ws_recv` drains the inbound queue. The `ws-chat` example is the canonical reference.

And for true peer-to-peer, the WebRTC stack is fully wired up. You can create peer connections, exchange SDP offers and answers, trickle ICE candidates, open data channels, and even add audio/video tracks. The `rtc-chat` example shows the full handshake. There's a built-in signaling client for quick prototypes, but you can use any signaling backend you want via `fetch` or WebSocket.

### Media: audio and video

Audio playback supports MP3, OGG, WAV, and FLAC out of the box. Decode happens on the host so your guest doesn't have to ship a codec. You can play from URL or from in-memory bytes:

```rust
audio_play_url("https://example.com/song.mp3");
audio_set_volume(0.7);
audio_set_loop(true);
```

There's a multi-channel API for games and apps that need background music plus sound effects:

```rust
audio_channel_play(0, &music_bytes);
audio_channel_play(1, &explosion_sfx);
audio_channel_set_volume(0, 0.4);
```

Video is more interesting. The host uses FFmpeg to decode H.264, H.265, AV1, and VP9. You load with `video_load_url` or `video_load`, then call `video_render(x, y, w, h)` from your frame loop to blit the current frame onto the canvas. The host handles the decode pipeline, frame timing, and texture upload — your job is just to say where to draw.

Bonus features include HLS adaptive streaming with explicit variant selection, SRT and VTT subtitle rendering, and picture-in-picture mode that floats the video as an overlay.

### Media capture

You can ask for the camera with `camera_open()`, capture frames with `camera_capture_frame(buf)` into an RGBA8 buffer, and query the dimensions. The microphone exposes raw f32 PCM samples. Screen capture works the same way. The host shows native permission prompts before granting access — your guest never gets implicit hardware access.

### Storage

Two storage tiers, picked by lifetime:

- Session storage (`storage_set`, `storage_get`, `storage_remove`) lives in memory and disappears on browser restart
- Persistent KV store (`kv_store_set`, `kv_store_get`, `kv_store_delete`) is sled-backed, scoped per origin, and survives restarts

That's it. No IndexedDB. No localStorage quirks. No "is this a number or a string" coercion issues. Just bytes in, bytes out.

### GPU

For when the canvas isn't enough, there's a WebGPU-style API. You compile WGSL shaders, allocate buffers and textures, build render or compute pipelines, write data, and dispatch:

- `gpu_create_shader(wgsl_source)` compiles a shader module
- `gpu_create_pipeline(shader, vertex_entry, fragment_entry)` builds a render pipeline
- `gpu_create_compute_pipeline(shader, entry)` builds a compute pipeline
- `gpu_create_buffer(size, usage)` and `gpu_write_buffer(handle, offset, data)` manage GPU memory
- `gpu_draw(...)` and `gpu_dispatch_compute(x, y, z)` issue the work

The `gpu-graphics-demo` example shows both rendering and compute. This is the path for serious data visualization, simulations, or 3D content.

### MIDI

Surprisingly, full MIDI input and output is supported. Enumerate ports, open them by index, send raw MIDI bytes, and poll for incoming packets. The `midi-demo` is a piano visualizer that lights up keys as you press them on a hardware keyboard. If you've ever wanted to build a sequencer or DAW-style tool, the building blocks are there.

### Everything else

A grab bag of utilities round out the platform:

- **Timers**: `set_timeout`, `set_interval`, `clear_timer`
- **Animation frames**: `request_animation_frame`, `cancel_animation_frame` for vsync-aligned callbacks
- **Navigation**: `navigate(url)`, `push_state`, `replace_state`, `history_back`, `history_forward`
- **Hyperlinks**: `register_hyperlink(x, y, w, h, url)` to make canvas regions clickable for navigation
- **Crypto**: `hash_sha256`, `hash_sha256_hex`, `base64_encode`, `base64_decode`
- **Clipboard**: `clipboard_read`, `clipboard_write`
- **Random**: `random_u64`, `random_f64`
- **Time**: `time_now_ms`
- **Notifications**: `notify(title, body)`
- **File upload**: `upload_file()` opens a native picker and returns the bytes
- **Dynamic loading**: `load_module(url)` runs another `.wasm` as a child with isolated memory and fuel — composition without sandbox escape

And tucked into `oxide_sdk::proto` is a zero-dependency Protocol Buffers encoder and decoder, so you can do efficient binary RPC without pulling in a 100KB protobuf runtime.

## Patterns That Will Save You Time

After building a few Oxide apps, certain patterns emerge.

### State management without `static mut`

The simple counter examples use `static mut` because it's the smallest possible thing. For real apps, wrap your state in something safer. A common pattern:

```rust
use std::cell::RefCell;

thread_local! {
    static STATE: RefCell<AppState> = RefCell::new(AppState::default());
}

#[derive(Default)]
struct AppState {
    counter: i32,
    items: Vec<String>,
}

#[no_mangle]
pub extern "C" fn on_frame(_dt_ms: u32) {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        // ... use state ...
    });
}
```

The WASM environment is single-threaded, so `thread_local!` is effectively just a global with a safe API. No locking overhead, no `unsafe`.

### Asynchronous work, frame by frame

When you start a `fetch_begin_get` request, you get a handle. Store it in your state. Each frame, poll `fetch_recv(handle)` until you get `End` or `Error`. Treat each frame like one tick of a state machine. This pattern works identically for streaming HTTP, WebSocket messages, WebRTC data channels, and MIDI input — drain everything that's ready, advance your state, draw the result.

### Layouts without a layout engine

Oxide gives you pixel coordinates, not flexbox. For most apps, this is fine: a few helper functions that compute rectangles from a parent rect plus padding will get you most of the way there. The `Rect` type in `oxide_sdk::draw` makes this pleasant. For more sophisticated needs, you can pull in a small layout crate that targets `wasm32-unknown-unknown` (taffy works), but I'd encourage trying the manual approach first. You'll find you reach for layout engines less often than you think.

### Composition through dynamic loading

If your app gets big, split it into modules and use `load_module` to bring child modules in at runtime. Each child gets its own memory and fuel — there's no way for a misbehaving child to corrupt the parent. This is a much cleaner story than the iframe and message-passing dance you'd do on the traditional web.

## Examples Worth Studying

The repository ships with a thoughtful set of examples. Each one is small enough to read in a sitting:

- `hello-oxide` — the canonical interactive starter, with widgets, mouse tracking, and counter state
- `audio-player` — multi-format playback with seek, volume, and loop controls
- `video-player` — full FFmpeg-backed video with subtitles and PiP
- `rtc-chat` — WebRTC peer-to-peer with the full SDP/ICE handshake
- `ws-chat` — WebSocket bidirectional messaging
- `stream-fetch-demo` — chunked HTTP for large downloads
- `gpu-graphics-demo` — WGSL shaders for rendering and compute
- `media-capture` — camera, microphone, and screen capture
- `midi-demo` — a piano visualizer that responds to hardware MIDI input
- `timer-demo` and `raf-demo` — timer and animation frame patterns
- `index` — a hub app that links to all the other demos using hyperlinks
- `fullstack-notes` — a complete frontend and Rust backend, demonstrating real-world architecture

If you want to learn the platform deeply, read the examples in roughly that order. Each one introduces new APIs and shows how they fit together. By the time you've read all of them, you'll have a good sense of the platform's shape and where the rough edges are.

## Some Honest Caveats

Oxide is not a polished commercial product. It's a serious project with real architecture and real capabilities, but you'll hit some sharp corners:

- The widget toolkit is intentionally minimal. If you want a complex form with validation, you'll be building it yourself.
- Layout is manual. You position everything by coordinates. This is great for games and visualizations, less great for traditional document-style content.
- Text rendering is good but not yet rich. Font loading, advanced typography, and complex script support are on the roadmap.
- The ecosystem is small. There are no Oxide UI libraries on crates.io yet. You're an early adopter — expect to build things yourself or collaborate with a small community.
- Some host capabilities are macOS-first. Cross-platform parity is steadily improving but not perfect.

These are exactly the things that get better over time, and they're the things you can help fix if you get involved.

## How to Think About Building Real Things

If you want to build something nontrivial in Oxide, here's the mental shift I'd suggest:

- Stop thinking in terms of pages, routes, and navigation. Think in terms of a single application with internal screens. Your `on_frame` is the renderer. You decide what's visible.
- Stop thinking in terms of components and reactive state. Think in terms of one piece of state and a function that draws it. There's no diffing, no virtual DOM, no reconciliation. There's just "here's the data, draw it."
- Stop thinking in terms of HTTP requests as exceptional. They're just one of many polled streams. Your frame is a place where you check what's new, advance state, and draw. Network responses, WebSocket messages, and timer fires all fit the same shape.
- Embrace the binary. Use protobuf or your own packed formats over the wire. You're not paying for JSON parsing on every message, so don't pay for it just from habit.
- Lean on the host. Image decode, video decode, audio playback, GPU work — these all happen on the native side at native speed. Your guest just orchestrates.

Once these clicks happen, building feels different. There are fewer abstractions to navigate and more direct cause and effect. When something goes wrong, you can usually find it.

## Getting Involved

If any of this resonates, here's how to go deeper:

1. Read `DOCS.md` in the repository — it's the comprehensive reference for every API
2. Skim `ROADMAP.md` to see where the project is heading and what's in flight
3. Look at `CONTRIBUTING.md` if you want to contribute new capabilities or fix bugs
4. Build the examples, modify them, and try writing your own
5. File issues for things that confuse you — early feedback shapes the platform
6. Share what you build, even rough prototypes — visibility encourages others to try the platform

Adding a new host function is straightforward. The pattern is: register it in `register_host_functions` in `oxide-browser/src/capabilities.rs`, expose it in `oxide-sdk/src/lib.rs` with a safe wrapper, and document it. The existing modules for WebRTC, WebSockets, and GPU are good templates if you're adding a substantial new subsystem.

## Why This Matters

The web platform is a monumental achievement, and it's also drowning in its own success. Every new feature has to coexist with twenty years of legacy decisions. Every browser engine is a multi-million-line C++ codebase that only a handful of organizations on Earth can maintain. The cost of entry — for users, developers, and toolmakers — keeps climbing.

Oxide is a bet that we can do it differently. That a tight, capability-based runtime can host applications that are simpler to write, faster to load, more secure to run, and easier to reason about. That binary distribution is not a step backward but a step forward, especially when paired with WebAssembly's portability and a properly sandboxed execution environment.

It might not replace the web. It probably shouldn't. But for a wide range of applications — games, visualizations, creative tools, peer-to-peer experiences, anything where you'd really like to ship a binary and have it just run — Oxide offers a path that's cleaner and more direct than anything the traditional web can give you.

Spin it up. Open the example index. Build a counter. Then build something weirder. Then tell us what you find.

The future of computing might not be a document. It might be a binary.

---

**Repository**: https://github.com/niklabh/oxide  
**Documentation**: `DOCS.md` in the repository, plus the `examples/` folder

Build something. Run it in Oxide. Share what you make.
