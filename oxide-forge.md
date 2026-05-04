# Oxide Forge — Claude 4.7 Hackathon Plan

> **Built with Opus 4.7: A Claude Code hackathon**
> 500 $ Claude credits · 3–5 day target · solo build

Oxide today is a mature binary-first WASM browser (Rust + wasmtime + GPUI,
capability-based sandbox, canvas/GPU/WebRTC/FFmpeg, immediate-mode rendering
via `on_frame()`). This hackathon does not rebuild that core.

**Oxide Forge** is an AI-native layer running *inside* Oxide where Claude
Opus 4.7 is the first-class co-creator: you describe an app in natural
language and Claude produces a complete, production-ready guest `.wasm`
module that is compiled and hot-loaded into a tab in the same browser,
following every pattern in `CLAUDE.md`.

---

## 1 · North star

Open Oxide → type `oxide://forge` → enter a prompt like:

> *"A real-time collaborative whiteboard with WebRTC sync and AI stroke
> suggestions"*

Watch Claude stream Rust code into the page, watch `cargo build` run,
watch the resulting `.wasm` load into a new tab and run. Iterate with
follow-up prompts. The whole loop lives inside the sandboxed browser.

### Primary goals

- [ ] A working `oxide://forge` page in the browser.
- [ ] End-to-end pipeline: prompt → Claude stream → cargo build → live app.
- [ ] ≥3 non-trivial apps **generated entirely via Forge** and merged as
      examples.
- [ ] A 2–4 minute demo video.

### Non-goals

- Not replacing existing `CLAUDE.md` guidelines for human contributors.
- Not shipping a wasm-based Rust compiler (we shell out to host `cargo`).
- Not integrating `claude-code` the CLI — we call the Anthropic Messages
  API directly from the host using the existing streaming-fetch plumbing.

---

## 2 · Architecture

```text
┌─────────────────────────────── Oxide Browser ──────────────────────────┐
│  oxide://forge                    ┌─ GPUI UI (native, not a guest)  ─┐ │
│                                   │ prompt input, streamed code,     │ │
│                                   │ build log, "Run" button          │ │
│                                   └──────────────┬───────────────────┘ │
│                                                  │                     │
│  ┌──── oxide-browser/src/forge.rs (NEW) ─────────▼───────────────┐    │
│  │  ForgeState { sessions, template_dir, build_queue }            │    │
│  │  • claude_stream()  — calls api.anthropic.com via reqwest     │    │
│  │  • scaffold()       — writes Cargo.toml + src/lib.rs          │    │
│  │  • compile()        — spawns `cargo build --target wasm32…`  │    │
│  │  • artifact_path()  — returns built .wasm path                │    │
│  └────────────────────────────┬───────────────────────────────────┘    │
│                               │                                        │
│  ┌── chosen Forge dir/<slug>/ ▼───────────────────────────────────┐    │
│  │  Cargo.toml, src/lib.rs, <slug>.wasm, target/wasm32…          │    │
│  └────────────────────────────┬───────────────────────────────────┘    │
│                               │ file://…/main.wasm                    │
│  ┌────────────────────────────▼───────────────────────────────────┐    │
│  │  Existing BrowserHost::run_bytes → new tab with the app        │    │
│  └────────────────────────────────────────────────────────────────┘    │
└────────────────────────────────────────────────────────────────────────┘
```

### Key architectural decisions

| Decision | Choice | Reason |
|---|---|---|
| Primary surface | In-browser `oxide://forge` page | Maximum demo wow-factor. |
| Compile | Shell out to host `cargo` | User already has Rust toolchain. |
| Claude access | Direct Anthropic Messages API | Reuse `fetch.rs` streaming, one key. |
| Forge UI | Native GPUI (not a guest wasm) | Needs privileged access to `cargo`. |
| Generated project layout | User-selected Forge folder, defaulting to `target/forge/<slug>/` | Easy demo default, durable user-owned output. |
| Secrets | `ANTHROPIC_API_KEY` env var | Standard, no secret in-repo. |

---

## 3 · Phases & tickbox progress

### Phase 0 — Vision doc (this file)

- [x] Write `oxide-forge.md` with tickbox plan.
- [x] Confirm scope: in-browser primary, cargo-based compile, 3–5 day budget.

### Phase 1 — Forge Kit (the brain Claude reads)

Everything Claude must know to generate a correct Oxide app, in one
self-contained directory.

- [x] Create `forge/` directory at repo root.
- [x] `forge/SYSTEM_PROMPT.md` — canonical system prompt injected before
      every generation call. Includes the 4-step host function pattern,
      start_app/on_frame contract, sandbox rules, naming conventions.
- [x] `forge/CAPABILITIES.md` — catalog of every public SDK function.
- [x] `forge/PATTERNS.md` — idiomatic patterns.
- [x] `forge/RECIPES.md` — annotated recipes (12 of them).
- [x] `forge/templates/base/Cargo.toml` — minimal guest Cargo.toml.
- [x] `forge/templates/base/src/lib.rs` — minimal `start_app` + `on_frame`.
- [x] `forge/README.md` — high-level index of the kit.

**Verification:** template compiles (`cargo build --target
wasm32-unknown-unknown --release` in `forge/templates/base/` completes in
< 2 s and emits `forge_app.wasm`).

### Phase 2 — Base template scaffold

- [x] `forge/templates/base/` compiles as a standalone example.
- [x] Template is excluded from the root workspace with `[workspace]` in
      its own Cargo.toml.
- [x] Template includes `[lib] crate-type = ["cdylib"]`; Forge rewrites the
      copied `oxide-sdk` dependency to the local absolute SDK path so projects
      build from any user-selected output folder.
- [x] `.gitignore` updated to exclude `target/forge/` and
      `forge/templates/*/target/`.

### Phase 3 — Host module `forge.rs`

- [x] `oxide-browser/src/forge.rs` with `ForgeState`, `ForgePhase`,
      `ForgeSnapshot`.
- [x] `ForgeState::new()` — lazy init; returns `None` if no
      `ANTHROPIC_API_KEY`.
- [x] `drive_anthropic_stream` — POST to
      `https://api.anthropic.com/v1/messages` with `stream: true`, parse
      SSE, append `content_block_delta.text` into the session's code buffer.
      Model defaults to `claude-opus-4-7`, overridable with
      `OXIDE_FORGE_MODEL`.
- [x] `scaffold_project` + `write_lib_rs` — copy template files then
      overwrite `src/lib.rs` with the (un-fenced) generated code.
- [x] `run_build` — spawn `cargo build --target wasm32-unknown-unknown
      --release --quiet --color never`; capture stderr into `build_log`;
      set `artifact_path` on success.
- [x] `extract_rust_block` — strip ```rust … ``` fence if present.
- [x] Registered in `oxide-browser/src/lib.rs` (`pub mod forge;`).
- [x] 7 unit tests pass (SSE parsing, fence extraction, slug shape).

**Verification:**

- [x] `cargo build -p oxide-browser` passes.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [x] `cargo test --workspace` passes (43 host tests + 7 forge tests).

### Phase 4 — `oxide://forge` internal page

- [x] `InternalPage::Forge` variant added.
- [x] `"oxide://forge"` mapped in `try_internal_page`.
- [x] `url_to_title` returns "Forge" for the scheme.
- [x] `TabState` gains `forge_prompt: String` and
      `forge_session_id: Option<u64>`.
- [x] `OxideBrowserView` gains a shared `forge: Arc<Mutex<Option<ForgeState>>>`
      and a dedicated `forge_focus: FocusHandle`.
- [x] `ensure_forge`, `forge_current_snapshot`, `forge_submit`,
      `forge_build`, `forge_run_in_new_tab` helpers on `OxideBrowserView`.
- [x] Forge page rendered in the internal-page match block with:
      - [x] Title card + status badge (API key detection).
      - [x] Prompt input row (focus-aware caret, Submit button).
      - [x] Session header (slug, phase badge, prompt preview).
      - [x] Streamed code pane (monospace, scroll).
      - [x] Build log pane (monospace, scroll).
      - [x] Build + Run-in-new-tab buttons (disabled when not applicable).
- [x] Top-level `on_key_down` routes keystrokes to `forge_prompt` when
      `forge_focus` has focus: Enter submits, Shift+Enter inserts
      newline, Backspace pops, Esc clears, Cmd+V pastes.

**Verification:** `cargo run -p oxide-browser`, navigate to
`oxide://forge` from the URL bar. Page renders; "ANTHROPIC_API_KEY not
set" badge is visible when the env var is unset, "ready" when set.
(Live end-to-end Claude generation requires an API key.)

### Phase 5 — Hot-load generated `.wasm` into a tab

- [x] `forge_run_in_new_tab` reads artifact bytes, creates a new tab via
      existing `create_tab`, and sends `RunRequest::LoadLocal(bytes)`.
- [x] Tab URL set to `oxide://forge/run/<slug>` so the user can
      identify it.
- [x] Build failures stay in the Forge page's error pane; they do not
      spill into tab-load errors.

### Phase 6 — Self-debug loop

- [x] `MAX_AUTO_RETRIES = 3` const.
- [x] `run_stream_then_build` orchestrator: on `ForgePhase::Error` after
      `cargo` fails, compose a retry prompt with the original request,
      the failing `lib.rs`, and the compiler stderr, then re-stream and
      rebuild automatically.
- [x] `build_retry_prompt` + `truncate_middle` keep the context bounded
      (6 KB cap on build log).
- [x] `Session.retries_used` counter; auto-retry short-circuits once it
      hits `MAX_AUTO_RETRIES`.
- [x] `Session.auto_fix` boolean (default `true`);
      `ForgeState::set_auto_fix(id, bool)` lets the UI toggle it.
- [x] Build is triggered **automatically** after `StreamComplete` so the
      user sees the full generate → compile → run flow without clicking.
      Manual "Build" button still works as a one-shot rebuild.
- [x] UI phase label shows `auto-fix N/3` when retries > 0.
- [ ] *Stretch:* token / dollar meter next to the phase badge.

### Phase 7 — Forge-generated demo apps

Every one of these must be produced via the Forge pipeline (not
hand-written) and the prompt that generated it stored alongside.

- [ ] `examples/forge-whiteboard/` — real-time collaborative whiteboard
      using WebRTC DataChannel.
- [ ] `examples/forge-llm-chat/` — chat UI that streams an OpenAI/Claude
      response with the streaming-fetch API.
- [ ] `examples/forge-life/` — Conway's Game of Life with mouse-brush
      editing and GPU compute if time permits.
- [ ] `examples/forge-synth/` — tiny polyphonic synth using audio + MIDI.
- [ ] `examples/forge-dashboard/` — live HTTP dashboard pulling from a
      public JSON feed.

Each example dir contains a `PROMPT.md` with the exact prompt used.

### Phase 8 — Polish & promo

- [x] `README.md` banner ("Built with Opus 4.7" badge linking to this
      plan) and dedicated "Oxide Forge — AI-native app generation"
      section with the flow diagram and file pointers.
- [x] `forge/DEMO_PROMPTS.md` with 10 curated prompts for the live demo.
- [x] `forge/README.md` references DEMO_PROMPTS.md.
- [ ] Record a 2–4 minute demo video (OBS or Screen Studio).
- [ ] Short blog post / X thread draft (separate file, not published
      from here).
- [ ] Tag a `v0-forge-hackathon` git tag once done.

### Phase 9 — Stretch goals (only if time)

- [ ] Guest-facing `api_forge_generate` so apps can recursively spawn
      other apps. (Meta.)
- [ ] Host-capability proposal flow: when Claude says "I need a new
      capability", show a diff of the proposed `register_*_functions`
      addition and let the user apply + rebuild the host.
- [ ] Shareable forge URLs: `oxide://forge/share/<id>` that embed prompt
      + code so others can fork.
- [ ] Inline syntax highlighting in the streamed code pane.

---

## 4 · File-level deliverables checklist

| File | Status | Owner | Notes |
|---|---|---|---|
| `oxide-forge.md` | [x] | — | This file. |
| `forge/README.md` | [ ] | Phase 1 | Index of the kit. |
| `forge/SYSTEM_PROMPT.md` | [ ] | Phase 1 | Authoritative. |
| `forge/CAPABILITIES.md` | [ ] | Phase 1 | |
| `forge/PATTERNS.md` | [ ] | Phase 1 | |
| `forge/RECIPES.md` | [ ] | Phase 1 | |
| `forge/templates/base/Cargo.toml` | [ ] | Phase 2 | |
| `forge/templates/base/src/lib.rs` | [ ] | Phase 2 | |
| `oxide-browser/src/forge.rs` | [ ] | Phase 3 | New host module. |
| `oxide-browser/src/lib.rs` (edit) | [ ] | Phase 3 | `pub mod forge;`. |
| `oxide-browser/src/capabilities.rs` (edit) | [ ] | Phase 3 | Add `forge` to `HostState`. |
| `oxide-browser/src/ui.rs` (edit) | [ ] | Phase 4 | `InternalPage::Forge` + render. |
| `examples/forge-whiteboard/**` | [ ] | Phase 7 | Generated. |
| `examples/forge-llm-chat/**` | [ ] | Phase 7 | Generated. |
| `examples/forge-life/**` | [ ] | Phase 7 | Generated. |
| `examples/forge-synth/**` | [ ] | Phase 7 | Generated. |
| `examples/forge-dashboard/**` | [ ] | Phase 7 | Generated. |
| `.gitignore` (edit) | [ ] | Phase 3 | Ignore `target/forge/`. |
| `README.md` (edit) | [ ] | Phase 8 | Banner + Forge section. |
| demo video | [ ] | Phase 8 | 2–4 min. |

---

## 5 · Verification matrix

Must pass after every phase before moving to the next:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p oxide-browser
cargo build -p oxide-sdk --target wasm32-unknown-unknown
cargo test --workspace
```

Additional, per-phase:

- **Phase 3:** `cargo run -p oxide-browser` launches without panics even
  when `ANTHROPIC_API_KEY` is unset.
- **Phase 4:** `oxide://forge` renders a non-empty page.
- **Phase 5:** A minimal generated app ("red rectangle on black
  background") compiles and opens in a tab in < 60 s.
- **Phase 7:** Every generated example builds with:
  `cargo build --target wasm32-unknown-unknown --release -p forge-<name>`.

---

## 6 · Risks & mitigations

| Risk | Likelihood | Mitigation |
|---|---|---|
| Claude generates code that doesn't compile | High | Self-debug loop (Phase 6); extensive SYSTEM_PROMPT; recipe library. |
| Cargo cold-compile > 60 s | High | Pre-warm `target/forge/` on browser startup; share target across slugs. |
| API key leakage in logs | Medium | Never log the key; redact Authorization header in any error output. |
| Generated code escapes the sandbox | **Low (architecturally impossible)** | Generated code runs as a normal guest — sandbox is untouched. |
| `ANTHROPIC_API_KEY` missing at demo time | Medium | Clear UI prompt, with one-click paste into an in-memory setting. |
| Runaway token spend | Medium | Per-session token budget + hard stop at N retries. |
| Demo machine offline during video recording | Low | Pre-record successful runs as fallback clips. |

---

## 7 · Credits & attribution

- Oxide core runtime: built over ~1 year with Claude Sonnet 3.5/4 under
  the `CLAUDE.md` surgical-coding protocol.
- Oxide Forge layer: built during the "Built with Opus 4.7" hackathon,
  with Claude Opus 4.7 as the first-class co-creator.
- Anthropic for the $500 credit grant.
- Cerebral Valley for hosting the hackathon.
