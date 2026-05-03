# Oxide Forge — Kit

> The brain that makes Claude write correct Oxide guest apps.

This directory is the canonical knowledge base fed to Claude Opus 4.7
on every Forge generation call. It is packaged as an **Agent Skill**
following the open [agentskills.io](https://agentskills.io/) specification,
so the same folder can be reused by any skills-compatible agent (Claude
Code, Cursor, OpenCode, etc.) that wants to generate Oxide guest apps.

## Layout

```
forge/
├── README.md                       ← this file
├── DEMO_PROMPTS.md                 ← curated prompts for live demos
├── skills/
│   └── oxide-wasm-app/             ← the Agent Skill
│       ├── SKILL.md                ← metadata + generation contract
│       └── references/
│           ├── CAPABILITIES.md     ← every public SDK symbol
│           ├── PATTERNS.md         ← idiomatic patterns
│           └── RECIPES.md          ← copy-pasteable snippets
└── templates/
    └── base/                       ← minimal Cargo project scaffold
```

| File | Purpose |
|------|---------|
| [`skills/oxide-wasm-app/SKILL.md`](./skills/oxide-wasm-app/SKILL.md) | Skill metadata (YAML frontmatter: `name`, `description`) plus the full generation contract. Must stay terse and prescriptive. |
| [`skills/oxide-wasm-app/references/CAPABILITIES.md`](./skills/oxide-wasm-app/references/CAPABILITIES.md) | Every public SDK symbol with full signatures. |
| [`skills/oxide-wasm-app/references/PATTERNS.md`](./skills/oxide-wasm-app/references/PATTERNS.md) | Idiomatic state, layout, and event patterns. |
| [`skills/oxide-wasm-app/references/RECIPES.md`](./skills/oxide-wasm-app/references/RECIPES.md) | Copy-pasteable snippets for common tasks. |
| [`DEMO_PROMPTS.md`](./DEMO_PROMPTS.md) | 10 curated prompts to paste into `oxide://forge` for testing and demos. |
| [`templates/base/`](./templates/base/) | Minimal Cargo project used as the scaffold. |

## How a generation call works

```text
user prompt
    │
    ▼
┌────────────────────────────────────────────────────────────┐
│ skills/oxide-wasm-app/SKILL.md (frontmatter stripped)      │
│   +                                                        │
│ every markdown file under references/ (alpha-sorted)       │
└────────────────────────┬───────────────────────────────────┘
                         │ Anthropic Messages API
                         ▼
              streaming Rust source
                         │
                         ▼
            selected-forge-folder/<slug>/src/lib.rs
                         │
                         ▼
       cargo build --target wasm32-unknown-unknown
                         │
                         ▼
              load .wasm into a new tab
```

The host wiring lives in `oxide-browser/src/forge.rs::build_system_prompt`.
It reads `SKILL.md`, strips the leading `---\n…\n---\n` frontmatter block
per the agentskills.io spec, then concatenates every `*.md` under
`references/` as successive `# Reference: <NAME>` sections.

## Using the skill from another agent

Because the skill is a plain folder with a `SKILL.md`, any
agentskills.io-compatible client can pick it up:

- **Claude Code / Cursor**: symlink or copy `forge/skills/oxide-wasm-app`
  into the host's skills directory (e.g. `.cursor/skills/` or
  `~/.claude/skills/`).
- **Custom harness**: load `SKILL.md` as the system prompt, and
  lazy-load files from `references/` when the task demands them
  (progressive disclosure).

## Updating the kit

When the SDK gains a new capability:

1. Update `oxide-sdk/src/lib.rs` (with a SDK wrapper) and the host side.
2. Add the new function to `skills/oxide-wasm-app/references/CAPABILITIES.md`
   under the right category.
3. If it introduces a new pattern (e.g. a new stream kind), add a
   recipe to `references/RECIPES.md` and a line to `references/PATTERNS.md`.
4. Do **not** clutter `SKILL.md` — that file is load-bearing and must
   stay small. Detailed material belongs in `references/`.
