# Oxide Forge — Kit

> The brain that makes Claude write correct Oxide guest apps.

This directory is the canonical knowledge base fed to Claude Opus 4.7
on every Forge generation call. It is *also* useful reference material
for humans writing Oxide apps by hand.

## Files

| File | Purpose |
|------|---------|
| [`SYSTEM_PROMPT.md`](./SYSTEM_PROMPT.md) | The prompt injected before every generation. Must stay terse and prescriptive. |
| [`CAPABILITIES.md`](./CAPABILITIES.md) | Every public SDK symbol with full signatures. |
| [`PATTERNS.md`](./PATTERNS.md) | Idiomatic state, layout, and event patterns. |
| [`RECIPES.md`](./RECIPES.md) | Copy-pasteable snippets for common tasks. |
| [`DEMO_PROMPTS.md`](./DEMO_PROMPTS.md) | 10 curated prompts to paste into `oxide://forge` for testing and demos. |
| [`templates/base/`](./templates/base/) | Minimal Cargo project used as the scaffold. |

## How a generation call works

```text
user prompt
    │
    ▼
┌─────────────────────────────────────────────────────┐
│ SYSTEM_PROMPT.md                                    │
│   +                                                 │
│ CAPABILITIES.md (full content, or on-demand slice)  │
│   +                                                 │
│ PATTERNS.md  (full content)                         │
│   +                                                 │
│ RECIPES.md   (full content, ≤ 20 KB)                │
└────────────────────────┬────────────────────────────┘
                         │ Anthropic Messages API
                         ▼
              streaming Rust source
                         │
                         ▼
            target/forge/<slug>/src/lib.rs
                         │
                         ▼
       cargo build --target wasm32-unknown-unknown
                         │
                         ▼
              load .wasm into a new tab
```

## Updating the kit

When the SDK gains a new capability:

1. Update `oxide-sdk/src/lib.rs` (with a SDK wrapper) and the host side.
2. Add the new function to `CAPABILITIES.md` under the right category.
3. If it introduces a new pattern (e.g. a new stream kind), add a
   recipe to `RECIPES.md` and a line to `PATTERNS.md`.
4. Do **not** clutter `SYSTEM_PROMPT.md` — that file is load-bearing and
   must stay small.
