# Contributing to Oxide

Thank you for your interest in contributing to Oxide — the world's first decentralized, binary-first browser. Whether you're fixing a typo, adding a host function, building an example app, or proposing an entirely new subsystem, every contribution matters.

## Table of Contents

- [Getting Started](#getting-started)
- [Project Structure](#project-structure)
- [Development Workflow](#development-workflow)
- [Coding Guidelines](#coding-guidelines)
- [Adding a Host Function](#adding-a-host-function)
- [Building a Guest App](#building-a-guest-app)
- [Pull Request Process](#pull-request-process)
- [Issue Labels & Bounties](#issue-labels--bounties)
- [Contributor Tiers & Rewards](#contributor-tiers--rewards)
- [Community](#community)
- [Code of Conduct](#code-of-conduct)

---

## Getting Started

### Prerequisites

- **Rust** (stable, latest) — [install via rustup](https://rustup.rs/)
- **wasm32-unknown-unknown** target
- **FFmpeg** development libraries (for video decode in `oxide-browser`): on macOS `brew install ffmpeg`; on Debian/Ubuntu `sudo apt-get install libavcodec-dev libavformat-dev libavutil-dev libavfilter-dev libavdevice-dev libswscale-dev libswresample-dev pkg-config`

```bash
rustup toolchain install stable
rustup target add wasm32-unknown-unknown
```

### Clone and Build

```bash
git clone https://github.com/niklabh/oxide.git
cd oxide

# Build the browser
cargo build -p oxide-browser

# Build an example guest app
cargo build --target wasm32-unknown-unknown --release -p hello-oxide

# Run the browser
cargo run -p oxide-browser
```

### Run Tests

```bash
cargo test --workspace
```

### Verify Formatting & Lints

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

---

## Project Structure

```
oxide/
├── oxide-browser/              # Host browser application (Rust, egui)
│   └── src/
│       ├── main.rs             # eframe bootstrap
│       ├── engine.rs           # WasmEngine, SandboxPolicy
│       ├── runtime.rs          # BrowserHost, fetch/load/instantiate
│       ├── capabilities.rs     # Host functions registered into wasmtime Linker
│       ├── navigation.rs       # History stack, back/forward
│       ├── url.rs              # WHATWG-style URL parser
│       └── ui.rs               # egui UI — toolbar, canvas, console, widgets
├── oxide-sdk/                  # Guest-side SDK (no_std compatible, pure FFI)
│   └── src/
│       ├── lib.rs              # Safe Rust wrappers over host imports
│       └── proto.rs            # Zero-dependency protobuf codec
├── examples/
│   ├── hello-oxide/            # Minimal interactive guest app
│   └── fullstack-notes/        # Full-stack example (Rust frontend + backend)
├── oxide-landing/              # Landing page (static HTML/CSS/JS)
├── ROADMAP.md                  # Phased development roadmap
├── Security.md                 # Security policy and bug bounty scope
└── Cargo.toml                  # Workspace root
```

### Key Crates

| Crate | What it does | When to touch it |
|---|---|---|
| `oxide-browser` | The host runtime — compiles WASM, registers host functions, renders UI | Adding capabilities, fixing sandbox bugs, improving the UI |
| `oxide-sdk` | The guest SDK — safe wrappers around `oxide::*` imports | Exposing new host functions to guest apps |
| `examples/*` | Example guest applications | Demonstrating features, onboarding new contributors |

---

## Development Workflow

1. **Fork** the repository and create a branch from `main`.
2. **Name your branch** descriptively: `feat/audio-api`, `fix/memory-leak-canvas`, `docs/sdk-examples`.
3. **Make small, focused commits.** Each commit should compile and pass tests.
4. **Write tests** for new functionality when applicable.
5. **Run the full check suite** before pushing:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

6. **Open a pull request** against `main`.

---

## Coding Guidelines

### Rust Style

- Follow standard Rust idioms and the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/).
- Run `cargo fmt` before every commit. The CI will reject unformatted code.
- Zero `clippy` warnings. If a lint is genuinely a false positive, suppress it with `#[allow(...)]` and a comment explaining why.
- Prefer `&str` over `String` in function parameters.
- Prefer returning `Result` or `Option` over panicking.

### Naming Conventions

| Entity | Convention | Example |
|---|---|---|
| Host function (in `capabilities.rs`) | `api_<category>_<action>` | `api_canvas_rect`, `api_audio_play` |
| SDK wrapper (in `oxide-sdk/src/lib.rs`) | `<category>_<action>` | `canvas_rect`, `audio_play` |
| FFI import (in SDK) | `_api_<name>` | `_api_canvas_rect` |
| Constants | `UPPER_SNAKE_CASE` | `KEY_ENTER`, `MAX_MEMORY_PAGES` |
| Types/Structs | `PascalCase` | `FetchResponse`, `UploadedFile` |

### Documentation

- Every public function in the SDK must have a doc comment (`///`).
- Host functions in `capabilities.rs` should have a brief comment explaining the guest-visible behavior.
- Non-obvious design decisions should be documented inline.

### Security

- **Never expose filesystem, environment, or raw socket access** to guest modules.
- All data crosses the host–guest boundary through `(ptr, len)` pairs in linear memory. Validate lengths before reading.
- New capabilities must be explicitly registered — the sandbox is additive by design.
- When in doubt, open a discussion before implementing.

---

## Adding a Host Function

This is the most common type of contribution. Here's the end-to-end process:

### 1. Register the host function in `capabilities.rs`

```rust
linker.func_wrap("oxide", "api_my_feature", |mut caller: Caller<'_, HostState>, arg: u32| -> u32 {
    // Read from guest memory if needed
    // Perform host-side logic
    // Write results back to guest memory if needed
    0 // return value
})?;
```

### 2. Add the FFI import in `oxide-sdk/src/lib.rs`

```rust
#[link(wasm_import_module = "oxide")]
extern "C" {
    #[link_name = "api_my_feature"]
    fn _api_my_feature(arg: u32) -> u32;
}
```

### 3. Add the safe wrapper in `oxide-sdk/src/lib.rs`

```rust
/// Brief description of what this does.
pub fn my_feature(arg: u32) -> u32 {
    unsafe { _api_my_feature(arg) }
}
```

### 4. Add an example or update an existing one

Show the new API in action in `examples/hello-oxide/src/lib.rs` or create a new example.

### 5. Update documentation

- Add the function to the capability table in `README.md`.
- Update `DOCS.md` if it exists.

---

## Building a Guest App

Guest apps are regular Rust libraries compiled to `wasm32-unknown-unknown`.

### Scaffold

```bash
cargo new --lib my-app
cd my-app
```

### `Cargo.toml`

```toml
[package]
name = "my-app"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
oxide-sdk = { path = "../oxide-sdk" }
```

### `src/lib.rs`

```rust
use oxide_sdk::*;

#[no_mangle]
pub extern "C" fn start_app() {
    canvas_clear(30, 30, 46, 255);
    canvas_text(20.0, 40.0, 28.0, 255, 255, 255, "Hello, Oxide!");
}
```

### Build and Run

```bash
cargo build --target wasm32-unknown-unknown --release
# Open the .wasm file in the Oxide browser
```

---

## Pull Request Process

1. **Fill out the PR template** (if one exists) or include:
   - **What** — a concise summary of the change
   - **Why** — motivation, linked issue number
   - **How** — implementation approach, trade-offs
   - **Testing** — what you tested and how

2. **Keep PRs focused.** One feature or fix per PR. Large PRs are harder to review and more likely to stall.

3. **CI must pass.** The PR will be checked for:
   - `cargo fmt` — formatting
   - `cargo clippy` — lints
   - `cargo test` — unit and integration tests
   - `cargo build --target wasm32-unknown-unknown` — SDK compiles for WASM

4. **Request a review.** Tag a maintainer or let the auto-assignment handle it.

5. **Address feedback promptly.** If changes are requested, push follow-up commits (don't force-push during review).

6. **Squash on merge.** We squash-merge to keep history clean. Your commit message will become the merge commit message.

---

## Issue Labels & Bounties

### Labels

| Label | Meaning |
|---|---|
| `good first issue` | Great for newcomers — well-scoped, low complexity |
| `help wanted` | Open for community contributions |
| `phase:N` | Belongs to roadmap Phase N (see [ROADMAP.md](ROADMAP.md)) |
| `bounty:small` | $OXIDE token reward — small scope |
| `bounty:medium` | $OXIDE token reward — medium scope |
| `bounty:large` | $OXIDE token reward — large scope |
| `bug` | Something isn't working |
| `enhancement` | New feature or improvement |
| `security` | Security-related (see [Security.md](Security.md)) |
| `docs` | Documentation improvements |
| `sdk` | Changes to `oxide-sdk` |
| `browser` | Changes to `oxide-browser` |

### Claiming a Bounty

1. Comment on the issue to claim it. First-come, first-served.
2. A maintainer will assign you within 24 hours.
3. Submit a PR within the agreed timeframe (usually 2 weeks for small, 4 weeks for large).
4. Once merged, the bounty is distributed to your provided Solana wallet address.

---

## Contributor Tiers & Rewards

We recognize and reward contributors through a tiered system backed by the $OXIDE token.

| Tier | Criteria | Perks |
|---|---|---|
| **Explorer** | First merged PR | Welcome NFT badge, listed on contributors page |
| **Builder** | 5+ merged PRs | Monthly $OXIDE airdrop, Discord role, early access to features |
| **Core** | Consistent contributor, deep domain expertise | Larger $OXIDE allocation, governance voting weight, roadmap input |
| **Architect** | Major feature or subsystem owner | Grant funding, co-author credit, conference sponsorship |

### Reward Types

- **PR bounties** — one-time token payment for issues tagged `bounty:*`
- **Code review rewards** — tokens for thorough, quality reviews
- **Documentation rewards** — tokens for guides, tutorials, API docs
- **Translation rewards** — tokens for i18n contributions
- **Bug bounty** — tiered rewards for security vulnerabilities (see [Security.md](Security.md))

Rewards are distributed on Solana. Provide your wallet address in your GitHub profile or PR description.

---

## Community

- **Telegram** — [t.me/oxide_browser](https://t.me/oxide_browser) — general chat, questions, support
- **X (Twitter)** — [@ForgeX_ai](https://x.com/ForgeX_ai) — announcements, updates
- **GitHub Discussions** — feature proposals, architecture discussions, RFC-style threads

### Getting Help

If you're stuck:

1. Search existing issues and discussions first.
2. Ask in the Telegram group — the community is friendly and responsive.
3. Open a GitHub Discussion for design questions or broad topics.
4. Open an Issue for specific bugs or well-defined feature requests.

---

## Code of Conduct

We are committed to providing a welcoming and inclusive experience for everyone. All participants are expected to:

- **Be respectful.** Disagreements are fine; personal attacks are not.
- **Be constructive.** Provide actionable feedback, not just criticism.
- **Be patient.** Not everyone has the same background or experience level.
- **Be collaborative.** We're building something together.

Harassment, discrimination, and toxic behavior will not be tolerated. Violations may result in removal from the project.

For concerns, contact [nikhil@polkassembly.io](mailto:nikhil@polkassembly.io).

---

## License

By contributing to Oxide, you agree that your contributions will be licensed under the same license as the project.

---

*Thank you for helping build the future of the decentralized web.*
