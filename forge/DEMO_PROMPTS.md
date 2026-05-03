# Oxide Forge — Demo Prompts

Curated prompts to paste into `oxide://forge` once you have
`ANTHROPIC_API_KEY` set and the browser running. Each is tuned to hit
different capability surfaces so you end up with a diverse demo reel.

Copy the prompt text, paste it into the Forge prompt box, hit Enter,
wait for the stream + build (first build per session cold-starts cargo
at ~20–60 s; subsequent builds are ~2–5 s).

---

## 1. Warmup — red rectangle on black

> Create a simple app that clears the canvas to black and draws a red
> rectangle at (100, 100) with size 200x120.

Purpose: end-to-end sanity check. Should compile and run in < 30 s.

---

## 2. Interactive counter

> A centered counter app. Two large buttons "−" and "+" side by side
> control an integer display in the middle. "Reset" button below resets
> to zero. Persist the count across restarts using kv_store.

Exercises: widgets, layout, persistent storage.

---

## 3. Cellular automaton (Conway's Life)

> Conway's Game of Life simulator on a 80x60 grid. Mouse drag toggles
> cells. Spacebar toggles pause. "R" randomises. Running at ~10 Hz.
> Dark background, phosphor-green cells, purple grid lines.

Exercises: game loop, grid rendering, keyboard input, fixed-step update
loop. Non-trivial but fits in one file.

---

## 4. Draw whiteboard with tools

> A whiteboard app. Left sidebar has three tool buttons: Pen (stroke),
> Eraser, Clear-all. Mouse drag draws with the current tool. Color
> palette of 8 preset colors across the top. Show the current tool name
> and color in the top-right.

Exercises: tool state machine, multiple widgets, stroke buffers,
canvas drawing.

---

## 5. LLM streaming chat (OpenAI-compatible)

> A single-column chat UI. Text input at the bottom, conversation above
> (newest at bottom). When the user hits Enter, POST to
> https://api.openai.com/v1/chat/completions with
> {"model":"gpt-4o-mini","stream":true,"messages":[…]} and use the
> streaming fetch APIs to append tokens to the assistant's message as
> they arrive. Include a "stop" button during streaming. Read the API
> key from kv_store under key "openai.api_key" — if missing, show a
> settings panel to paste it in. Dark palette, monospace for user
> messages, serif for assistant messages.

Exercises: streaming fetch, SSE parsing, kv_store, text input, rich
layout. This is the showcase demo.

---

## 6. WebSocket echo with reconnect

> A WebSocket echo client that connects to ws://echo.websocket.events.
> Shows a connection badge (green = open, orange = connecting, red =
> closed). Text input at the bottom; Enter sends the message. The log
> above shows "→ you said" and "← echo" entries with timestamps. On
> disconnect, reconnect automatically with a 1-second backoff.

Exercises: WebSocket API, timers, state machine, timestamp formatting.

---

## 7. WebRTC two-peer chat (signaling-room based)

> Two-peer text chat over WebRTC DataChannel. Use
> rtc_signal_connect("wss://…") and rtc_signal_join_room. Show "Your
> ID: <peer>" and a list of connected peers. Click a peer to initiate
> an offer. Once the data channel is open, the bottom input sends text
> to all peers. Display received messages with "[peer-id]:" prefix.

Exercises: the full RTC handshake, signaling loop, data-channel send
and receive.

---

## 8. Simple JSON dashboard

> Fetch https://api.github.com/repos/rust-lang/rust every 60 seconds
> and display: stars, forks, open issues, last push date (formatted as
> "N days ago"). Large numbers with labels. Refresh button for
> immediate update. Last-updated timestamp in the corner.

Exercises: non-streaming fetch, string parsing (no serde available),
timers, layout. Good medium-difficulty test.

---

## 9. GPU compute — Mandelbrot

> Render the Mandelbrot set on a GPU compute pipeline. Canvas-sized
> texture, 1024 iterations max, smooth colouring with an RGB gradient.
> Mouse wheel zooms in/out, drag pans. Title text shows current zoom
> level.

Exercises: GPU buffer / texture / shader / compute pipeline. Harder —
be prepared for retries.

---

## 10. Tiny synth (MIDI controlled)

> A MIDI-controlled synth. On startup, enumerate all MIDI inputs and
> open the first one. Keyboard keys (A S D F G H J K) also trigger the
> bottom octave. Visualize the keyboard: pressed keys glow. Use
> audio_channel_play with a small prebuilt WAV for each note (just
> trigger the same short sample at different channels for now).

Exercises: MIDI input, multi-channel audio, keyboard polling.

---

## Iteration tips

- If a prompt fails after 3 auto-retries, **simplify and resubmit**.
  Break complex prompts into two stages.
- Claude is better at one focused capability at a time. The LLM chat
  prompt (#5) works reliably; the Mandelbrot (#9) is ambitious and may
  need hand-tuning.
- After a successful run, type another prompt to iterate on the same
  app idea — e.g. "make the brush stroke smoother" after #4.
- The session's selected Forge folder contains `<slug>/src/lib.rs` with the generated
  code verbatim — you can open it in your editor, edit, and click
  "Build" again for a manual rebuild.
