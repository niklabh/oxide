# oxide-docs — Oxide Documentation Hub

This crate generates the API documentation for the Oxide browser project,
served at **<https://docs.oxide.foundation>** via GitHub Pages.

## Building Docs Locally

```bash
# From the workspace root
cargo doc --no-deps -p oxide-docs -p oxide-sdk -p oxide-browser --open
```

The generated docs will be at `target/doc/` and your browser will open to the
`oxide_docs` landing page.

To build with the custom theme (matching the production site):

```bash
RUSTDOCFLAGS="--html-in-header oxide-docs/rustdoc-header.html" \
  cargo doc --no-deps -p oxide-docs -p oxide-sdk -p oxide-browser
```

## Deployment

Documentation is automatically deployed to GitHub Pages on every push to
`main` via the `.github/workflows/docs.yml` workflow. The workflow:

1. Builds rustdoc for `oxide-docs`, `oxide-sdk`, and `oxide-browser`
2. Copies the custom landing page (`oxide-docs/index.html`) to `target/doc/`
3. Deploys to GitHub Pages at <https://docs.oxide.foundation>

## How to Add Documentation for New Capabilities

When you add a new host capability (a new `api_*` function), follow these
steps to ensure it appears in the documentation:

### Step 1 — Implement the host function

In `oxide-browser/src/capabilities.rs`, add the `linker.func_wrap(...)` call
inside `register_host_functions()`:

```rust
linker.func_wrap(
    "oxide",
    "api_my_feature",
    |caller: Caller<'_, HostState>, ptr: u32, len: u32| {
        let mem = caller.data().memory.expect("memory not set");
        let value = read_guest_string(&mem, &caller, ptr, len).unwrap_or_default();
        // ... implementation ...
    },
)?;
```

### Step 2 — Add the SDK wrapper

In `oxide-sdk/src/lib.rs`, add the raw FFI import and a safe wrapper:

```rust
// Inside the `extern "C"` block:
#[link_name = "api_my_feature"]
fn _api_my_feature(ptr: u32, len: u32);

// Below the extern block, add the safe wrapper with a doc comment:

/// Brief description of what my_feature does.
///
/// Longer explanation if needed, including parameter meanings,
/// return values, and usage examples.
///
/// # Example
///
/// ```rust,ignore
/// use oxide_sdk::*;
///
/// my_feature("hello");
/// ```
pub fn my_feature(value: &str) {
    unsafe { _api_my_feature(value.as_ptr() as u32, value.len() as u32) }
}
```

**Documentation requirements for SDK functions:**

- `///` doc comment on every public function
- Brief one-line summary
- Parameter descriptions if not obvious
- Return value semantics (especially for `Result` and `Option`)
- An `# Example` section with `rust,ignore` code block
- Cross-references using `[`backtick links`]` to related functions

### Step 3 — Update the docs hub

In `oxide-docs/src/lib.rs`, add the new function to the appropriate API
category table:

```rust
//! | [`oxide_sdk::my_feature`] | Brief description |
```

If the new capability introduces a whole new category (e.g., "WebRTC",
"GPU"), add a new section heading:

```rust
//! ## WebRTC
//!
//! | Function | Description |
//! |----------|-------------|
//! | [`oxide_sdk::rtc_connect`] | Establish a peer connection |
//! | [`oxide_sdk::rtc_send`] | Send data over the connection |
```

### Step 4 — Update the SDK crate-level table

In `oxide-sdk/src/lib.rs`, add the new function to the `## API Categories`
table in the crate-level doc comment:

```rust
//! | **WebRTC** | [`rtc_connect`], [`rtc_send`], [`rtc_close`] |
```

### Step 5 — Add host-side doc comments

If you added new types to `capabilities.rs` (structs, enums, etc.), add
`///` doc comments:

```rust
/// State for the WebRTC subsystem.
///
/// Manages peer connections and data channels for real-time communication
/// between guest modules.
pub struct RtcState {
    /// Active peer connections keyed by connection ID.
    pub connections: HashMap<u32, PeerConnection>,
}
```

### Step 6 — Verify docs build

```bash
cargo doc --no-deps -p oxide-docs -p oxide-sdk -p oxide-browser 2>&1 | grep -i warning
```

Fix any broken links or missing references before merging.

### Step 7 — Update DOCS.md

Add the new API to the `DOCS.md` developer documentation with a usage
example and API reference table entry.

## Doc Comment Style Guide

### Module-level comments (`//!`)

Every `.rs` file with public items should have a `//!` module-level doc
comment explaining:

- What the module does (one sentence)
- Key types and functions it provides
- How it fits into the overall architecture

### Item-level comments (`///`)

Every public item (`pub fn`, `pub struct`, `pub enum`, `pub type`) must
have a `///` doc comment with:

1. **Summary line** — One sentence describing the item
2. **Details** (if needed) — Parameters, return values, behavior notes
3. **Example** (for SDK functions) — `rust,ignore` code block
4. **Cross-references** — Link to related items with `[`backticks`]`

### Don't

- Don't add comments that just repeat the function name
- Don't document private items unless the logic is non-obvious
- Don't use `# Safety` sections unless there's actual `unsafe` code
  exposed to consumers

## File Structure

```
oxide-docs/
├── Cargo.toml              # Depends on oxide-sdk and oxide-browser
├── README.md               # This file
├── index.html              # Custom landing page for docs.oxide.foundation
├── rustdoc-header.html     # Custom CSS injected into rustdoc pages
└── src/
    └── lib.rs              # Re-exports + comprehensive API reference prose
```
