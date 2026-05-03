# Oxide Promo — Visual Identity

Adapted from **Data Drift** (Refik Anadol) — futuristic, immersive, AI-flavored — but tightened for a developer-tool narrative. Less "particle simulation art piece," more "manifesto inside a clean reactor core." Black canvas, two electric accents, one mono register for code, one geometric sans for statements.

## Style Prompt

A pitch-black canvas with electric purple and cyan as the only accents. Type is thin, weightless, geometric — but the code panels and stat readouts are razor-sharp monospace, tabular, telemetric. Nothing decorative is opaque; everything glows at low alpha. Motion is fluid `sine.inOut` for ambient drift, sharp `expo.out` for hero arrivals. The voice is confident and quiet — restraint with a single moment of intensity per scene. The background never feels like a slide; it always feels like a live system breathing.

## Colors

| Role            | Hex       | Usage                                                                |
| --------------- | --------- | -------------------------------------------------------------------- |
| `--bg`          | `#070711` | Canvas — near-black tinted toward purple, never pure `#000`          |
| `--bg-elev`     | `#0e0e1c` | Card / panel surfaces                                                |
| `--fg`          | `#f0f3ff` | Primary text — slightly cool white                                   |
| `--fg-muted`    | `#9aa3c7` | Secondary text, labels                                               |
| `--accent`      | `#a78bfa` | Electric purple — Forge / AI moments                                 |
| `--accent-2`    | `#22d3ee` | Cyan — Oxide / system moments                                        |
| `--danger`      | `#f87171` | Used sparingly for "no" / negative space callouts                    |
| `--ok`          | `#34d399` | Build success, "Finished"                                            |

Both accents are deliberately at the high end of luminance so contrast against `--bg` clears WCAG AA at body sizes (4.5:1+).

## Typography

| Voice                      | Family                      | Weight | Notes                                                            |
| -------------------------- | --------------------------- | ------ | ---------------------------------------------------------------- |
| Headlines / statements     | **Space Grotesque** family — using `Space Grotesk` | 300 / 700 | Geometric, slightly weird — not Inter, not Outfit. Tight tracking on display. |
| Code, terminals, data, IDs | **JetBrains Mono**          | 400 / 700 | `font-variant-ligatures: none` in code blocks               |

Two fonts only. Sans + mono — the boundary cross. Headlines lean on extreme weight contrast (300 vs 700), never 400 vs 700.

## Motion Rules

- Ambient: `sine.inOut`, 4–8s loops, 5–10% scale/opacity range
- Hero entrances: `expo.out`, 0.5–0.7s, `y: 30–60` + opacity, never scale-bounce
- Code streams in line-by-line via `clip-path` or staggered opacity (0.03–0.06s stagger), NOT typewriter — typewriter feels old-web
- Stats count up from 0 with `power2.out`, ~0.8s
- Transitions: **focus pull** (blur crossfade) as primary — fluid, "drift with me" — with one **cinematic-ish zoom** at the Forge reveal as the climax accent. CSS-only, no shaders.

## What NOT to Do

1. **No `Inter`, `Outfit`, `Sora`, `Syne`, or any banned LLM-default font.** This is the single biggest tell.
2. **No purple-to-blue gradient text** (`background-clip: text`). Use solid `--accent` or `--accent-2`. The classic AI-design mistake we're explicitly told to avoid.
3. **No pure `#000` background.** It's `#070711`. The faint purple tint makes everything else feel correct.
4. **No equal-weight centered grids.** Lead the eye somewhere. Asymmetry on stat panels.
5. **No emoji, no rounded card stacks, no neon glow as wallpaper.** The glow is localized — under hero text, under the cursor, under the build success — never a full-screen wash.
6. **No bouncy/elastic eases.** Data Drift glides. `back.out` is banned in this composition.
