# txxt — Project Context

**What this is:** A scheduling portal for CloudPC ops teams. SMO (Small
Multiplayer Online) — 5-20 users on software-rendered VDI desktops with no GPU
and bandwidth-constrained codec connections.

**Who built this:** Ulli (darxtarr) with rotating AI partners (Claude, GPT,
Gemini). Multi-session, multi-model collaboration.

**Important framing:** DESIGN.md is the output of research and brainstorming
sessions, not proven production truth. Every hypothesis needs testing on real
CloudPC hardware. If you can prove something wrong, that's a win.

## Before You Do Anything

1. **Read CURRENT_STATE.md.** It tells you what phase we're in and what to
   build next.
2. **Read DESIGN.md.** The entire document. It explains every architectural
   decision and the constraints that drive them.
3. **Look at `flashlight-overlay.png`.** That sketch is the visual spec for
   the current phase.

The axiom: **"Everything is possible but nothing is real until the click."**
If you don't understand what this means after reading DESIGN.md, ask Ulli
before writing code.

## Architecture (60-second version)

```
[User laptop] ── VDI/WAN (variable quality) ──► [Azure CloudPC]
                                                  │  localhost
                                             [IRONCLAD] ◄──WS──► [Sidecar]
                                                  │
                                               WAN (corporate)
                                                  │
                                             [AWS EC2 Ireland]
```

The browser does NOT render world state. The browser does NOT do hit testing.
The browser does NOT own state. The sidecar does all of this and sends the
browser a flat image to display.

The sidecar also sends **OverlayCommands (0x1A)** — draw commands for the
flashlight overlay (clipped element edges). The browser executes these on an
SVG overlay. This is UX only, independent of the candidate list (which is for
click semantics). Same SDF pass, two separate outputs.

## Current Phase: Flashlight Overlay Stress Test

**Read CURRENT_STATE.md for the full spec.** Short version:

We are building a browser-only testbed that proves the flashlight overlay
works on real CloudPC/VDI hardware. Three files in `frontend/`: index.html,
styles.css, ironclad.js. No server needed.

The test harness generates random shapes, bakes them as a background image,
then on every cursor move: evaluates SDF for all shapes, clips their edges to
the flashlight circle, and feeds draw commands to the overlay module. The
overlay module draws clipped edge fragments as green SVG lines.

This is a stress test. We push shape count (10 → 2000) and overlap density
until performance degrades, find the ceiling, and use that data to decide
what comes next.

**What to build:**
- Overlay module (production quality — survives into final IRONCLAD)
- Test harness (throwaway — replaced by Rust sidecar later)

**What NOT to build yet:**
- Materialization module (mousedown → div). Comes after overlay is proven.
- Anything in `backend/src/`. Sidecar waits for testbed results.
- Wire protocol. The harness feeds the overlay module directly via JS objects.

## What NOT To Do (ever)

**Do not add a JS framework.** No React, no Vue, no Svelte.

**Do not render world state in the browser.** The sidecar renders; the browser
displays. The SVG overlay for flashlight feedback IS part of the design —
that's local UX, not world state.

**Do not auto-generate task titles.** Drops create tasks with empty titles and
immediately open editing. The human types the title.

**Do not try to eliminate the "flash" on background swap.** It's intentional
tactile feedback. See DESIGN.md "The Flash Is a Feature."

**Do not add source metadata to tasks** (source: Email, etc.).

**Do not propose CRDT, event sourcing, or protocol versioning.** Explicitly
rejected in DESIGN.md with reasoning.

**Do not shoehorn old source code into the new architecture.** The archive is
reference material for patterns. Do not copy files back. Write everything new.

## Source Code Status

**Clean slate.** `backend/src/` is empty. `frontend/` is being rewritten for
the testbed (index.html, styles.css, ironclad.js — all fresh).

The archive (`archive/backend-pre-axiom/`, `archive/frontend-pre-axiom/`)
exists for reference:
- redb patterns (`archive/backend-pre-axiom/src/persist.rs`)
- tiny-skia draw primitives (`archive/backend-pre-axiom/src/renderer.rs`)
- Axum WebSocket handler (`archive/backend-pre-axiom/src/game.rs`)
- State machine patterns (`archive/backend-pre-axiom/src/world.rs`)
- Previous IRONCLAD engine (`archive/frontend-pre-axiom/ironclad.js`)

## Build Order

**Testbed first.** Then the formal stack, only after the testbed passes on
real hardware. See CURRENT_STATE.md for the full sequence.

## Tools & Build

```bash
# Testbed phase — no server needed
# Open frontend/index.html directly in a browser

# After testbed passes:
cd backend && cargo build          # Build sidecar
cd backend && cargo test           # Run all tests
cd backend && cargo run            # Start on :3000, serves frontend/
```

Frontend has no build step. Static files only.

## Key Files

| File | What |
|------|------|
| `DESIGN.md` | Architecture source of truth — read first |
| `CURRENT_STATE.md` | Where we are, what to build next |
| `KEYBINDS.md` | Active and planned keybinds |
| `flashlight-overlay.png` | Visual spec for flashlight overlay |
| `frontend/` | IRONCLAD — testbed in progress |
| `backend/src/` | Sidecar — empty until testbed passes |
| `archive/` | Previous versions — reference only |
