# Oxide Roadmap

> Building the world's first decentralized, binary-first browser — one milestone at a time.

This roadmap outlines the planned evolution of Oxide from its current foundation to a full-featured decentralized application platform. Phases are sequential but individual features within a phase may ship incrementally.

---

## Phase 0 — Foundation (Shipped)

The core architecture is live: a Rust-native browser that fetches and executes `.wasm` modules in a capability-based sandbox.

- [x] Wasmtime runtime with fuel metering and bounded memory (16 MB / 500M instructions)
- [x] Immediate-mode canvas rendering (rect, circle, line, text, image)
- [x] Interactive widget toolkit (button, checkbox, slider, text input)
- [x] Full HTTP fetch API with GET/POST/PUT/DELETE
- [x] Session storage and persistent key-value store (sled-backed)
- [x] Navigation with history stack (push/replace state, back/forward)
- [x] Clipboard read/write
- [x] SHA-256 hashing and Base64 encode/decode
- [x] Dynamic child module loading with isolated memory and fuel
- [x] Zero-dependency protobuf wire-format codec in the SDK
- [x] Hyperlink regions and URL utilities (resolve, encode, decode)
- [x] Input polling (mouse, keyboard, scroll, modifiers)
- [x] File upload via native OS picker
- [x] Geolocation API (mock)
- [x] Notification API

---

## Phase 1 — Media & Rich Content

**Goal:** Make Oxide a viable platform for media-rich applications — video players, music apps, podcasts, streaming, and interactive content.

### Audio

- [x] `audio_play(data, format)` — decode and play audio buffers (MP3, OGG, WAV, FLAC)
- [x] `audio_play_url(url)` — stream audio from a URL
- [x] `audio_pause()` / `audio_resume()` / `audio_stop()` — playback control
- [x] `audio_set_volume(level)` / `audio_get_volume()` — volume control (0.0–1.0)
- [x] `audio_seek(position_ms)` / `audio_position()` — seek and query playback position
- [ ] `audio_duration()` — get total duration of loaded track
- [ ] `audio_set_loop(enabled)` — loop playback toggle
- [ ] Multiple simultaneous audio channels for sound effects and background music
- [ ] Audio format detection and codec negotiation

### Video

- [ ] `video_load(data, format)` — load video from bytes (MP4, WebM, AV1)
- [ ] `video_load_url(url)` — stream video from a URL
- [ ] `video_play()` / `video_pause()` / `video_stop()` — playback control
- [ ] `video_seek(position_ms)` / `video_position()` / `video_duration()`
- [ ] `video_render(x, y, w, h)` — draw current video frame onto the canvas
- [ ] `video_set_volume(level)` — control video audio track
- [ ] Adaptive bitrate streaming (HLS/DASH support)
- [ ] Subtitle/caption rendering (SRT, VTT)
- [ ] Picture-in-picture mode

### Media Capture

- [ ] `camera_open()` / `camera_capture_frame()` — access device camera with user permission prompt
- [ ] `microphone_open()` / `microphone_read_samples()` — access microphone input
- [ ] `screen_capture()` — screenshot or screen recording with permission
- [ ] Media stream pipelines for real-time processing

---

## Phase 2 — GPU & Graphics

**Goal:** Unlock hardware-accelerated rendering for games, data visualization, 3D applications, and compute-heavy workloads.

### 2D Acceleration

- [ ] GPU-backed canvas renderer (replace CPU-painted egui primitives)
- [ ] `canvas_rounded_rect()`, `canvas_arc()`, `canvas_bezier()` — extended shape primitives
- [ ] `canvas_gradient(type, stops)` — linear and radial gradients
- [ ] `canvas_transform(matrix)` — 2D affine transformations (translate, rotate, scale, skew)
- [ ] `canvas_clip(region)` — clipping regions
- [ ] `canvas_opacity(alpha)` — layer-level opacity
- [ ] Sprite batching and texture atlases for game-like workloads
- [ ] Font rendering with glyph caching (variable fonts, custom font loading)

### 3D / WebGPU-style API

- [ ] `gpu_create_buffer()` / `gpu_create_texture()` / `gpu_create_shader()` — low-level GPU resource creation
- [ ] `gpu_create_pipeline()` — configurable render and compute pipelines
- [ ] `gpu_draw()` / `gpu_dispatch_compute()` — submit draw calls and compute dispatches
- [ ] WGSL (WebGPU Shading Language) shader support
- [ ] Depth buffer, stencil operations, and blending modes
- [ ] Instanced rendering for large scenes
- [ ] GPU readback for compute results

### GPU Compute

- [ ] General-purpose GPU compute via compute shaders
- [ ] Shared memory and workgroup synchronization
- [ ] Use cases: ML inference, physics simulation, image processing, cryptography

---

## Phase 3 — Real-Time Communication (RTC)

**Goal:** Enable peer-to-peer communication for video calls, multiplayer games, collaborative tools, and decentralized messaging.

### WebRTC-style API

- [ ] `rtc_create_peer()` — create a peer connection
- [ ] `rtc_create_offer()` / `rtc_create_answer()` — SDP offer/answer exchange
- [ ] `rtc_set_local_description()` / `rtc_set_remote_description()`
- [ ] `rtc_add_ice_candidate()` — ICE candidate trickle
- [ ] `rtc_on_connection_state_change(callback)` — connection lifecycle events
- [ ] STUN/TURN server configuration

### Data Channels

- [ ] `rtc_create_data_channel(label, options)` — reliable or unreliable data channels
- [ ] `rtc_send(channel, data)` / `rtc_on_message(channel, callback)`
- [ ] Ordered and unordered delivery modes
- [ ] Binary and text message support

### Media Streams

- [ ] `rtc_add_track(stream, track)` — attach audio/video tracks to peer connections
- [ ] `rtc_on_track(callback)` — receive remote media tracks
- [ ] Codec negotiation (VP8, VP9, AV1, Opus)
- [ ] Bandwidth estimation and adaptive quality

### Signaling

- [ ] Built-in signaling relay for bootstrapping connections
- [ ] Support for custom signaling servers
- [ ] Room-based connection management

---

## Phase 4 — Tasks, Events & Background Processing

**Goal:** Give guest applications the ability to schedule work, respond to system events, and run background operations.

### Timer & Scheduling

- [ ] `set_timeout(callback_id, delay_ms)` — one-shot timer
- [ ] `set_interval(callback_id, interval_ms)` — repeating timer
- [ ] `clear_timeout(id)` / `clear_interval(id)` — cancel timers
- [ ] `request_animation_frame(callback_id)` — vsync-aligned frame callback
- [ ] Cron-style scheduled tasks for long-running apps

### Event System

- [ ] `on_event(event_type, callback_id)` — register event listeners
- [ ] `emit_event(event_type, data)` — emit custom events
- [ ] Built-in event types: `resize`, `focus`, `blur`, `visibility_change`, `online`, `offline`
- [ ] Touch events: `touch_start`, `touch_move`, `touch_end`, `touch_cancel`
- [ ] Gamepad events: `gamepad_connected`, `gamepad_button`, `gamepad_axis`
- [ ] Drag and drop events with file data

### Background Workers

- [ ] `spawn_worker(wasm_url)` — launch a background WASM worker with its own fuel and memory
- [ ] `worker_post_message(worker_id, data)` / `worker_on_message(worker_id, callback)`
- [ ] Shared memory regions between main module and workers (opt-in)
- [ ] Worker pool management and load balancing
- [ ] `worker_terminate(worker_id)` — graceful and forced termination

### Async I/O

- [ ] Non-blocking fetch with callback/promise-style API
- [ ] Streaming response bodies (chunked transfer)
- [ ] WebSocket support: `ws_connect(url)`, `ws_send()`, `ws_on_message()`
- [ ] Server-sent events (SSE) for push updates

---

## Phase 5 — Plugin Framework

**Goal:** Allow the browser itself to be extended through a safe, versioned plugin architecture.

### Plugin API

- [ ] Plugin manifest format (`oxide-plugin.toml`) with metadata, capabilities, and version
- [ ] Plugin lifecycle: `on_install`, `on_activate`, `on_deactivate`, `on_uninstall`
- [ ] Sandboxed plugin execution (plugins are WASM modules themselves)
- [ ] Capability-gated access: plugins declare required host APIs in manifest

### Extension Points

- [ ] **UI plugins** — add toolbar buttons, side panels, and status bar items
- [ ] **Content plugins** — transform or augment rendered content (ad blocking, translation, accessibility)
- [ ] **Protocol plugins** — register custom URL schemes (`ipfs://`, `dat://`, `hyper://`)
- [ ] **Storage plugins** — custom storage backends (SQLite, IndexedDB-like, encrypted vaults)
- [ ] **Network plugins** — middleware for request/response (proxy, caching, compression)
- [ ] **Theme plugins** — custom browser chrome themes and color schemes

### Plugin Distribution

- [ ] Built-in plugin registry with search and discovery
- [ ] One-click install from `oxide://plugins`
- [ ] Signed plugins with developer identity verification
- [ ] Automatic update mechanism with rollback support
- [ ] User ratings, reviews, and download counts

### Inter-Plugin Communication

- [ ] Message-passing API between plugins
- [ ] Shared service registration (e.g., a "wallet" plugin that other plugins can call)
- [ ] Dependency resolution and load ordering

---

## Phase 6 — Decentralized Infrastructure

**Goal:** Make Oxide truly decentralized — content can be hosted, resolved, and served without relying on centralized servers.

### P2P Content Network

- [ ] IPFS integration for content-addressed module hosting
- [ ] DHT-based module discovery and resolution
- [ ] `oxide://` scheme resolves modules via decentralized name registry
- [ ] Content pinning and local caching with expiry policies
- [ ] BitTorrent-style swarming for popular modules

### Decentralized Identity

- [ ] Built-in key pair generation and management
- [ ] DID (Decentralized Identifier) support
- [ ] Verifiable credentials for app authentication
- [ ] Sign-in with wallet (Solana, Ethereum, Polkadot)
- [ ] Guest-accessible identity API: `identity_sign()`, `identity_verify()`, `identity_public_key()`

### Decentralized Storage

- [ ] IPFS-backed persistent storage for guest apps
- [ ] Encrypted storage vaults with user-held keys
- [ ] Cross-device data sync via CRDTs
- [ ] Storage quotas and garbage collection

### Name Resolution

- [ ] Decentralized domain registry (ENS, SNS, or custom)
- [ ] Human-readable names mapped to content hashes
- [ ] DNS-over-HTTPS fallback for traditional web interop

---

## Phase 7 — Open Source Contributions & Rewards

**Goal:** Build a sustainable, incentive-aligned open-source ecosystem around Oxide.

### Contribution Framework

- [ ] Contributor tiers: **Explorer** (first PR), **Builder** (5+ merged PRs), **Core** (consistent contributor), **Architect** (major features)
- [ ] Automated contribution tracking via GitHub integration
- [ ] Contributor leaderboard on `oxide.foundation`
- [ ] Monthly contributor spotlight and blog features

### $OXIDE Token Rewards

- [ ] **Bug bounty program** — $OXIDE rewards for security vulnerabilities (critical, high, medium, low tiers)
- [ ] **PR rewards** — token bounties for tagged issues (`bounty:small`, `bounty:medium`, `bounty:large`)
- [ ] **Code review rewards** — tokens for thorough, quality code reviews
- [ ] **Documentation rewards** — tokens for docs, tutorials, and guides
- [ ] **Translation rewards** — tokens for i18n contributions
- [ ] On-chain reward distribution via Solana smart contract
- [ ] Vesting schedule for large contributions to align long-term incentives

### Early Adopter Program

- [ ] **Genesis Badge NFT** — minted for users who download and run Oxide before v1.0
- [ ] **Pioneer Airdrop** — $OXIDE token airdrop to early adopters based on usage metrics
- [ ] **Beta Tester Rewards** — bonus tokens for users who report bugs during beta
- [ ] **Referral Program** — earn $OXIDE for bringing new users (tracked on-chain)
- [ ] **Community Ambassador** — extra rewards for organizing meetups, writing articles, creating content
- [ ] Time-weighted rewards: earlier adoption = higher multiplier

### App Builder Incentives

- [ ] **App Store revenue sharing** — builders earn $OXIDE based on app usage and ratings
- [ ] **Developer grants** — quarterly grant program for ambitious Oxide apps
- [ ] **Hackathon prizes** — sponsored hackathons with $OXIDE prize pools
- [ ] **Showcase program** — featured apps on the landing page and plugin registry
- [ ] **SDK bounties** — rewards for porting the SDK to new languages (C, C++, Go, Zig, AssemblyScript)
- [ ] **Template marketplace** — builders earn from starter templates and boilerplates

### Governance

- [ ] $OXIDE token-weighted voting on roadmap priorities
- [ ] Proposal system for new features and protocol changes
- [ ] Treasury management with transparent on-chain spending
- [ ] Quarterly community calls and roadmap reviews

---

## Phase 8 — Developer Experience & Ecosystem

**Goal:** Make building on Oxide as frictionless as building for the traditional web.

### Developer Tools

- [ ] Built-in developer console with WASM introspection
- [ ] Hot-reload for guest modules during development
- [ ] `oxide-cli` — command-line tool for scaffolding, building, and deploying apps
- [ ] Source-map support for debugging guest modules
- [ ] Performance profiler: fuel consumption, memory usage, frame times
- [ ] Network inspector for fetch/WebSocket traffic

### Multi-Language SDK

- [ ] **Rust** (shipped) — `oxide-sdk` crate
- [ ] **C/C++** — header-only SDK with FFI bindings
- [ ] **Go** — TinyGo-compatible SDK
- [ ] **Zig** — native Zig SDK
- [ ] **AssemblyScript** — TypeScript-like SDK for JS developers
- [ ] **Python** — via Pyodide or custom Python-to-WASM toolchain
- [ ] Unified documentation across all language SDKs

### App Registry

- [ ] `oxide://apps` — browsable, searchable app store within the browser
- [ ] Developer accounts with verified identity
- [ ] App versioning, changelogs, and rollback
- [ ] Category-based discovery (games, tools, social, media, finance)
- [ ] User reviews and ratings
- [ ] Automatic security scanning of submitted WASM modules

### Templates & Starters

- [ ] `oxide new --template game` — project scaffolding
- [ ] Templates: blank, game, dashboard, social, media-player, notes, chat
- [ ] Example gallery with live demos

---

## Phase 9 — Platform Maturity

**Goal:** Polish, harden, and scale Oxide for mainstream adoption.

### Browser Features

- [x] Multi-tab support with per-tab isolation
- [x] Bookmarks and favorites
- [ ] Download manager
- [ ] Print-to-PDF
- [ ] Zoom and accessibility controls
- [ ] Keyboard shortcuts and command palette
- [ ] Dark/light theme toggle

### Accessibility

- [ ] Screen reader integration via host accessibility tree
- [ ] Keyboard-only navigation for all UI elements
- [ ] High-contrast mode and custom color schemes
- [ ] ARIA-like semantic hints in the widget API
- [ ] Text scaling and dyslexia-friendly font options

### Performance

- [ ] Ahead-of-time (AOT) compilation cache for frequently loaded modules
- [ ] Parallel module compilation
- [ ] Streaming compilation (compile while downloading)
- [ ] Memory pool recycling for module instances
- [ ] Frame budget management and adaptive quality

### Security Hardening

- [ ] Formal capability audit and threat model publication
- [ ] Fuzzing suite for all host functions
- [ ] Sandboxed networking with per-origin policies
- [ ] Content Security Policy (CSP) equivalent for WASM apps
- [ ] Automatic vulnerability scanning in CI

### Cross-Platform

- [ ] macOS (shipped)
- [ ] Linux (shipped)
- [ ] Windows (shipped)
- [ ] Android (via native Rust + egui)
- [ ] iOS (via native Rust + egui)
- [ ] Web version (Oxide running inside a traditional browser via wasm-bindgen)

---

## Timeline (Estimated)

| Phase | Target | Status |
|-------|--------|--------|
| Phase 0 — Foundation | Q1 2026 | **Shipped** |
| Phase 1 — Media & Rich Content | Q2 2026 | In Progress |
| Phase 2 — GPU & Graphics | Q3 2026 | Planned |
| Phase 3 — Real-Time Communication | Q3 2026 | Planned |
| Phase 4 — Tasks, Events & Background | Q4 2026 | Planned |
| Phase 5 — Plugin Framework | Q4 2026 | Planned |
| Phase 6 — Decentralized Infrastructure | Q1 2027 | Planned |
| Phase 7 — Contributions & Rewards | Q1 2027 | Planned |
| Phase 8 — Developer Experience | Q2 2027 | Planned |
| Phase 9 — Platform Maturity | Q3 2027 | Planned |

---

## How to Contribute

Every phase has open issues tagged with the phase label. Pick what excites you:

1. **Browse open issues** — Look for `phase:1`, `phase:2`, etc. labels
2. **Claim a bounty** — Issues tagged `bounty:*` have $OXIDE token rewards
3. **Propose a feature** — Open a discussion in the `proposals` category
4. **Build an app** — The best way to shape the platform is to build on it

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup instructions and coding guidelines.

---

*This roadmap is a living document. Priorities may shift based on community feedback and $OXIDE governance votes. Last updated: March 2026.*
