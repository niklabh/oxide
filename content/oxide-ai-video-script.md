# Oxide AI Video Script

**Video Title:** Oxide: The World's First Binary-First Decentralized Browser

**Target Length:** 6-8 minutes  
**Style:** Futuristic tech explainer. Dark cyber theme with neon accents (matches landing page). Use screen recordings of Oxide browser running demos, smooth animations of WASM execution, code highlights, architecture diagrams, particle effects.  
**Voice:** Confident, enthusiastic male narrator with tech authority (or AI voice like "Adam" or similar). Upbeat electronic/tech soundtrack that builds during demos.  
**Pacing:** 140-160 words per minute. Include pauses for visuals.

---

## Scene 1: Hook (0:00 - 0:35)

**[VISUAL: Dramatic opening with glowing particles forming the Oxide logo on black background. Fast cuts of traditional browser chaos (HTML trees, JS bundles) dissolving into clean binary WASM modules flying into a sleek GPUI window. Text overlays: "The Web is Broken" → "Meet Oxide"]**

**NARRATION:**  
"Imagine the web without the bloat. No massive JavaScript bundles. No fragile DOM. No endless security vulnerabilities from running untrusted code directly on your machine.  

What if every app was a secure, compact WebAssembly binary?  

Welcome to **Oxide** — the world's first decentralized, binary-first browser. Built in Rust, powered by wasmtime, and designed for the future of computing."

**[TRANSITION: Logo zoom with whoosh sound]**

---

## Scene 2: What is Oxide? (0:35 - 1:30)

**[VISUAL: Split screen - left: traditional browser architecture diagram (HTML/CSS/JS → V8), right: Oxide architecture. Show Oxide browser UI (URL bar, canvas, console). Animate .wasm file loading. Cut to running examples: hello-oxide, video player, RTC chat.]**

**NARRATION:**  
"Oxide is a desktop browser that fetches and executes `.wasm` modules instead of HTML, CSS, and JavaScript.  

Your guest applications run in a **secure sandbox** with zero direct access to the filesystem, environment variables, or raw sockets. Every capability — from drawing pixels to making network requests — is explicitly granted through a powerful set of host functions.

The browser itself is built with:
- **wasmtime** for WASM execution with fuel metering and memory limits (256MB max, 500M instructions per frame)
- **GPUI** for a beautiful, GPU-accelerated interface
- **Rust** end-to-end for both host and guest

This creates a radically simpler, more secure, and performant model for web applications."

**[ON SCREEN TEXT: "Binary-First • Sandboxed • Capability-Based • Immediate Mode Rendering"]**

---

## Scene 3: The Architecture (1:30 - 2:30)

**[VISUAL: Animated 3D architecture diagram (from README/DOCS). Show flow: Fetch → Compile → Link "oxide" functions → Instantiate → start_app() + on_frame() loop. Highlight HostState, linear memory boundary, immediate-mode draw commands queue.]**

**NARRATION:**  
"Here's how it works. When you open a `.wasm` URL or file:

1. The host fetches the raw bytes.
2. wasmtime compiles it with strict sandbox policies.
3. We register ~150 host functions under the 'oxide' import module.
4. The guest gets a bounded linear memory and a `HostState` shared via FFI.
5. Your app exports `start_app()` which runs once, and optionally `on_frame(dt_ms)` which is called every frame.

All communication happens through `(ptr, len)` pairs in guest memory. The SDK provides safe Rust wrappers. No WASI. Pure capability-based security.

The rendering is **immediate-mode**: your guest code issues canvas commands, UI widgets, and GPU calls every frame. The host drains these queues and paints with GPUI primitives. No retained DOM. You control every pixel."

**[Cut to console output, canvas drawing live]**

---

## Scene 4: Building Apps with Oxide (2:30 - 5:00)

**[VISUAL: Step-by-step screen recording + code animations. Show terminal, VSCode-like editor with syntax highlight, live browser. Use the hello-oxide and interactive examples.]**

**NARRATION:**  
"Building apps with Oxide is surprisingly simple. Everything is Rust.

**Step 1: Setup**
```bash
rustup target add wasm32-unknown-unknown
cargo new --lib my-oxide-app
cd my-oxide-app
```

**Step 2: Configure Cargo.toml**
```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
oxide-sdk = { path = "../oxide/oxide-sdk" }  # or from crates.io
```

**Step 3: Write your app**

For a simple static render:

```rust
use oxide_sdk::*;

#[no_mangle]
pub extern "C" fn start_app() {
    log("Hello from Oxide!");
    canvas_clear(30, 30, 46, 255);  // dark background
    canvas_text(100.0, 100.0, 32.0, 255, 255, 255, 255, "Hello, Oxide!");
    canvas_rect(50.0, 150.0, 200.0, 100.0, 100, 150, 255, 200);
}
```

For interactive apps with input and widgets, implement `on_frame`:

```rust
use oxide_sdk::*;

static mut COUNT: i32 = 0;

#[no_mangle]
pub extern "C" fn on_frame(_dt: u32) {
    canvas_clear(30, 30, 46, 255);
    
    if ui_button(1, 50.0, 50.0, 120.0, 40.0, "Increment") {
        unsafe { COUNT += 1; }
    }
    
    let count = unsafe { COUNT };
    canvas_text(50.0, 150.0, 24.0, 100, 255, 100, 255, &format!("Count: {}", count));
    
    let (mx, my) = mouse_position();
    canvas_circle(mx as f32, my as f32, 15.0, 255, 100, 100, 180);
}
```

**Step 4: Build and run**
```bash
cargo build --target wasm32-unknown-unknown --release
# Then open the .wasm in Oxide browser
```

You get access to high-level `draw::Canvas`, full WebRTC, streaming fetch, GPU compute with WGSL, audio/video with FFmpeg backend, MIDI, media capture, persistent storage, and more."

**[Show montage of different examples running: video-player with subtitles, rtc-chat connecting peers, gpu demo with shaders, fullstack-notes app.]**

---

## Scene 5: Why It Matters & Advanced Capabilities (5:00 - 6:30)

**[VISUAL: Feature grid with icons/animations. Live demos of WebRTC P2P, video HLS streaming, GPU compute, dynamic module loading. Show security sandbox animation (blocked FS access).]**

**NARRATION:**  
"Why does this matter? Oxide apps are:

- **Secure by default** — malicious code can't escape the sandbox
- **Performant** — native WASM speed + direct GPUI/wgpu access
- **Decentralized** — apps can be served from anywhere. No app stores. No gatekeepers.
- **Full-stack capable** — combine frontend WASM with backend services via WebRTC/fetch

Advanced features include:
- Peer-to-peer chat with WebRTC
- Hardware-accelerated video with subtitles and PiP
- GPU compute shaders
- Immediate-mode UI widgets that feel native
- Dynamic loading of child WASM modules with isolated state

The possibilities are endless: games, creative tools, decentralized apps, data visualizers, and more."

---

## Scene 6: Call to Action (6:30 - 7:30)

**[VISUAL: Final screen with Oxide browser open to index demo hub. QR code or links. GitHub stars counter animation. Community logos. $OXIDE token mention with pump.fun link if appropriate. End with logo and "Build the Future" text. Fade to particle explosion.]**

**NARRATION:**  
"Ready to build the future of the web?

1. Clone the repo: `git clone https://github.com/niklabh/oxide.git`
2. `cargo run -p oxide-browser`
3. Build the examples or create your own
4. Open your `.wasm` files and watch the magic

Check out the full documentation, examples for video players, WebRTC, GPU graphics, and full-stack apps.

Star us on GitHub, join the community, and start shipping unstoppable binary apps today.

Oxide — where the web meets WebAssembly.

Thank you for watching. Now go build something amazing."

**[END SCREEN: Links - GitHub, Docs, Examples, oxide.foundation. Subscribe/call-to-action overlay.]**

---

## Production Notes (Updated 2026)

### Recommended Tools for This Script
**Best overall for direct "script → video":**
1. **Hypernatural.ai** (hypernatural.ai/script-to-video) — Top choice. Paste the entire script (scene descriptions + narrator text). It auto-parses screenplay format, generates consistent visuals/B-roll, AI voices (ElevenLabs quality), captions, and editable shots. Perfect for narrator-driven tech explainers. Select "Voiceover" mode or "Screenplay".
2. **Mostly.so** — Excellent for long-form (up to 30min). AI turns ideas/scripts into structured scenes with character consistency and multi-language support.
3. **Synthesia** (synthesia.io/tools/script-to-video-maker) — Easiest free/quick option. Paste narration, choose AI avatar (tech presenter style), add your B-roll/images.

**For highest cinematic quality (hybrid workflow):**
- **Runway Gen-4.5** or **Kling AI 3.0** — Generate individual 5-15s clips from each **[VISUAL]** prompt + reference screenshots of the Oxide browser (use Image-to-Video). Kling excels at motion/natural physics and native audio sync; Runway at precise camera control and physics.
- Assemble in **CapCut** (free AI editor with auto-captions, effects) or Runway's timeline.

### Step-by-Step to Create the Video
1. **Prepare assets**:
   - Record real Oxide demos: `cargo run -p oxide-browser`, run examples (video-player, rtc-chat, gpu-graphics-demo), screen-record canvas/console interactions.
   - Generate reference images: screenshots of Oxide UI, architecture diagrams, logo animations.
   - Voiceover: Use ElevenLabs (clone a tech narrator voice) or tool's built-in for the full narration text.

2. **For Hypernatural / Mostly / Synthesia (easiest)**:
   - Copy the narration blocks + **[VISUAL]** cues into their script/prompt box.
   - It will auto-generate shots. Edit individual scenes to incorporate your real Oxide footage as B-roll.
   - Add code highlights as text overlays or picture-in-picture.
   - Generate → review → export (adds auto-captions).

3. **For Runway/Kling (premium quality)**:
   - Break into per-scene prompts using cinematographic language: "Slow tracking shot of futuristic dark UI with neon accents, particles forming Oxide logo, smooth camera dolly in, cyberpunk tech aesthetic, sharp details, 4K".
   - Upload Oxide screenshots as first frame/reference image for consistency.
   - Generate clips matching timing (e.g. Scene 1: 35s).
   - Combine with real screen recordings for code/build sections.
   - Sync with separately generated voiceover + cyberpunk music.

4. **Polish**:
   - Add subtle text overlays for key terms ("Binary-First", "wasm", "on_frame()").
   - Ensure architecture diagram is animated (can generate with Runway or use simple tools like Canva/Mantra).
   - Test pacing — add pauses after complex concepts like the host/guest boundary.
   - Export in 16:9 or 9:16 for YouTube/Shorts.

- **B-roll priority:** Real Oxide footage > pure AI generation for credibility (especially code and live demos).
- **Music:** Royalty-free cyberpunk/tech track (swells on capability reveals and demos).
- **Length:** ~7 minutes. Trim narration slightly if needed.

**Total Word Count:** ~850 (perfect pacing for 7 minutes with visuals)

---

**This script is ready for 2026 AI tools.** It accurately reflects the Oxide project (README, DOCS.md, examples). Paste sections directly into Hypernatural for fastest results. Let me know if you want per-scene optimized prompts, reference image generation, or further edits!