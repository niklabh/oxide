# Oxide + Oxide Forge — 3-Minute Demo Video Script

> Submission: **Built with Claude Opus 4.7** hackathon
> Target runtime: **3:00** (±5 s)
> Format: 1920×1080, 30 fps, screen capture + overlay titles + VO
> Tone: confident, fast, a little cheeky. No fluff.

Pacing is built for ~150 WPM VO. Each section lists **[VISUAL]**, **[ON‑SCREEN TEXT]**, and **[VO]**. Run count is wall‑clock.

---

## Cold open — 0:00 → 0:12  (Hook, 12 s)

**[VISUAL]** Hard cut to a terminal: `cargo run -p oxide-browser`. Browser window springs up. URL bar types itself: `oxide://forge`. Prompt box already focused, cursor blinking.

**[ON‑SCREEN TEXT]** `What if the browser had no HTML?`  →  `What if Claude could write the apps it runs?`

**[VO]**
> The web runs on HTML, JavaScript, and a billion lines of legacy.
> We wondered — what if a browser just ran WebAssembly… and Claude wrote the apps?

---

## Act 1 — Oxide in 30 seconds — 0:12 → 0:45

**[VISUAL]** Quick cuts of existing examples loading in tabs: `hello-oxide` red square → `video-player` playing a clip → `rtc-chat` with two peers → `midi-demo` piano lighting up. All inside the same Oxide window.

**[ON‑SCREEN TEXT]** `Oxide — a binary-first browser` · `Rust + wasmtime + GPUI` · `~150 host capabilities` · `zero WASI, zero network from guest`

**[VO]**
> This is **Oxide**. It's a browser written in Rust that fetches `.wasm` instead of HTML. Every app is a sandboxed WebAssembly module with fuel‑metered execution and a 256 MB memory ceiling.
> Canvas. GPU. Audio. Video via FFmpeg. WebRTC. WebSockets. MIDI. Camera, mic, screen capture. All behind one capability layer — about a hundred and fifty host functions, nothing else reachable.
> The guest can't touch the filesystem, can't read env vars, can't open a socket. If we didn't register it, it doesn't exist.

---

## Act 2 — Forge, live — 0:45 → 2:05  (80 s, the money shot)

### Beat A — The prompt — 0:45 → 1:00

**[VISUAL]** Focus on `oxide://forge`. Type prompt in real time:

> *"Conway's Game of Life on an 80×60 grid. Dark background, phosphor‑green cells, purple grid. Spacebar pauses, R randomises, drag to toggle cells."*

Hit Enter.

**[ON‑SCREEN TEXT]** `oxide://forge` · `Claude Opus 4.7`

**[VO]**
> Meet **Oxide Forge**. I type what I want. Claude Opus 4.7 writes the Rust.

### Beat B — Streaming — 1:00 → 1:25

**[VISUAL]** Rust source streams into the side panel — `use oxide_sdk::*;`, `start_app`, `on_frame`. Pick up speed with 4× playback on the long middle section. Below, a compact status strip: `streaming…` → `writing src/lib.rs` → `cargo build --target wasm32-unknown-unknown`.

**[ON‑SCREEN TEXT]** `forge/SYSTEM_PROMPT.md + CAPABILITIES.md + PATTERNS.md + RECIPES.md → Claude`

**[VO]**
> Forge ships Claude a tight, versioned prompt kit — the full SDK catalog, the idiomatic patterns, a dozen runnable recipes — so it writes against the exact v0.6 surface, not guesses from training data.
> Code streams into a scratch project. Cargo compiles it to `wasm32-unknown-unknown`.

### Beat C — It runs — 1:25 → 1:50

**[VISUAL]** Build finishes (`Finished` line highlights green). A new tab pops open. Conway's Life starts running — phosphor cells pulsing on black, purple grid. Mouse drags paint a glider. Spacebar freezes it. `R` explodes into noise.

**[ON‑SCREEN TEXT]** `one prompt → compiled wasm → live tab`

**[VO]**
> Twenty‑something seconds later, a new tab. A real compiled WebAssembly module. Running in the same sandbox as every other Oxide app — no eval, no escape hatch.

### Beat D — Iterate — 1:50 → 2:05

**[VISUAL]** Back to Forge. Follow‑up prompt:

> *"Add a generation counter and a speed slider from 1 to 30 Hz."*

Stream, rebuild (≈3 s this time — cargo cache is warm), same tab reloads with a slider and counter in the corner.

**[VO]**
> Iteration is just another prompt. Forge keeps the last `lib.rs` in context, Claude rewrites it, cargo hot‑rebuilds, tab reloads.

---

## Act 3 — Why this is interesting — 2:05 → 2:40

**[VISUAL]** Split screen. Left: a self‑debug log scrolling — `build failed · feeding compiler output back to Claude · attempt 2/3 · Finished`. Right: the `forge/` directory tree with `SYSTEM_PROMPT.md`, `CAPABILITIES.md`, `PATTERNS.md`, `RECIPES.md`.

**[ON‑SCREEN TEXT]** `self-healing build loop (up to 3 retries)` · `capability-based sandbox` · `Claude sees the exact API surface`

**[VO]**
> Two things make this actually work.
> First — when `cargo` fails, Forge pipes the compiler errors straight back to Claude and tries again, up to three times, before surfacing anything. Most transient mistakes fix themselves.
> Second — because Oxide's host surface is finite and documented, Claude is writing against a real API, not hallucinating one. The sandbox is airtight *by construction*, so a wrong generation is just a compile error, never a security incident.

---

## Act 4 — Reel — 2:40 → 2:54

**[VISUAL]** Rapid montage (≈2 s each) of Forge‑generated apps running:
1. Streaming LLM chat (OpenAI, dark palette).
2. Whiteboard with pen / eraser / 8‑color palette.
3. WebSocket echo with reconnect badge.
4. GitHub repo dashboard with `N days ago`.
5. Mandelbrot on GPU compute, zooming.

**[ON‑SCREEN TEXT]** `5 apps · one evening · zero hand-written Rust`

**[VO]**
> Streaming chat. A whiteboard. A live WebSocket client. A GPU Mandelbrot. Every one of these was written by Claude inside Forge.

---

## Close — 2:54 → 3:00

**[VISUAL]** Oxide window centered. Overlay:

```
github.com/niklabh/oxide
oxide://forge
Built with Claude Opus 4.7
```

**[VO]**
> Oxide Forge. One prompt. Real Rust. Real WebAssembly. Running in the browser that wrote it.

**[END CARD]** Logo + `github.com/niklabh/oxide` · fade to black.

---

## Shot list / prep checklist

- [ ] Warm cargo cache before recording (first build is 20–60 s; subsequent are 2–5 s). Run the Life prompt once offline so act 2's build lands in ~20 s.
- [ ] Pre‑seed the Forge folder with the 5 reel apps (`target/forge/…`) so act 4 is a click‑through, not a recompile.
- [ ] Dial the terminal/editor font to 16 pt minimum for readability at 1080 p.
- [ ] `export ANTHROPIC_API_KEY=…` before capture; clear it from history in the recording.
- [ ] Record VO separately; the cargo build bar is the only moment where live audio matters.
- [ ] Captions burned in for the on‑screen text lines above (accessibility + mute autoplay on socials).

## Timing budget (for the editor)

| Section | Start | End  | Duration |
|---------|------:|-----:|---------:|
| Cold open | 0:00 | 0:12 | 12 s |
| Act 1 · Oxide | 0:12 | 0:45 | 33 s |
| Act 2 · Forge live | 0:45 | 2:05 | 80 s |
| Act 3 · Why | 2:05 | 2:40 | 35 s |
| Act 4 · Reel | 2:40 | 2:54 | 14 s |
| Close | 2:54 | 3:00 | 6 s |
| **Total** | | | **3:00** |
