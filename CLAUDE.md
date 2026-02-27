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
   the flashlight overlay.

The axiom: **"Everything is possible but nothing is real until the click."**
If you don't understand what this means after reading DESIGN.md, ask Ulli
before writing code.

## Architecture (60-second version)

```
[User laptop] ── VDI/WAN (variable quality) ──► [Azure CloudPC]
                                                  │  localhost
                                             [txxt] ◄──WS──► [Sidecar]
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

## Current Phase: End-to-End Working — Refinement

The browser client (`frontend/txxt.js`) is connected to the Rust sidecar via
WebSocket. Background image renders from tiny-skia. Clicking the grid creates
tasks. Overlay traces task edges under the cursor. Tested on CloudPC VDI.

See CURRENT_STATE.md for exactly what is working and what comes next.

## The Codec Is The Constraint

Everything is designed around one fact: the VDI codec only sends pixels that
changed. If nothing on screen moves, the codec sleeps and bandwidth drops to
zero. This is the goal.

**Consequences that must be preserved:**

- The overlay only changes when something meaningful changes (shapes entering
  or leaving the flashlight). The sidecar tracks `was_drawing` state per
  connection and only sends `OverlayCommands` when the overlay set changes.
  Cursor moving through empty space → sidecar sends nothing → codec sleeps.

- The SVG overlay uses a **fixed pool** of pre-allocated `<line>` and `<path>`
  elements. Visibility is toggled with `opacity`. Do NOT tear down and recreate
  elements per frame — DOM mutations cause repaints that wake the codec.

- The cursor position is sent from the browser inside the **rAF loop**, not on
  a fixed-Hz timer. This naturally synchronizes with the display refresh rate
  (32Hz on CloudPC). A fixed timer fights the frame clock and can cause double
  sends or missed frames.

- The background image is a static flat PNG/JPEG sent once per world mutation.
  Between mutations it never changes. The codec never touches it.

## What NOT To Do (ever)

**Do not add a JS framework.** No React, no Vue, no Svelte.

**Do not render world state in the browser.** The sidecar renders; the browser
displays. The SVG overlay IS part of the design — that's local UX, not world
state.

**Do not use a fixed-Hz timer for cursor sends.** Use the rAF loop. The timer
is not synchronized with the display and defeats codec sleep on VDI.

**Do not send overlay commands unconditionally on every cursor move.** The
sidecar tracks `was_drawing` per connection. It only sends when the overlay
set changes (shapes entering/leaving the flashlight). See CURRENT_STATE.md
for the state machine.

**Do not tear down and recreate SVG elements.** The pool with opacity toggles
is correct and proven. DOM mutations cause repaints. Opacity changes do not
(the compositor handles them without a repaint on modern Chromium).

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

**Frontend** (`frontend/`): `txxt.js`, `index.html`, `styles.css` — all live.
`txxt.js` has two classes: `Overlay` (production, pool-based SVG executor) and
`SidecarClient` (WS connection, rAF cursor send, binary frame decode).

**Backend** (`backend/src/`): Full Rust sidecar, 52 tests pass.
- `main.rs` — boot, AppState, Axum router
- `world.rs` — pure state machine
- `persist.rs` — redb save file
- `wire.rs` — hop 1 binary protocol (0x10–0x2E)
- `renderer.rs` — tiny-skia, chrome cache, returns image + Vec<RenderedShape>
- `spatial.rs` — SDF + clip geometry, `trait OverlayGenerator`
- `game.rs` — Axum WS handler, per-connection context

**Archive** (`archive/`): Previous versions — reference only. Do not copy back.

## Tools & Build

```bash
cd backend && cargo build          # Build sidecar
cd backend && cargo test           # Run all tests (52)
cd backend && cargo run            # Start on :3000, serves frontend/

# Environment variables:
TXXT_DB=path/to/txxt.redb          # Save file (default: txxt.redb)
PORT=3000                          # Listen port (default: 3000)
TXXT_FRONTEND=../frontend          # Frontend path (default: ../frontend)
```

Frontend has no build step. Static files only.

## Key Files

| File | What |
|------|------|
| `DESIGN.md` | Architecture source of truth — read first |
| `CURRENT_STATE.md` | Where we are, what to build next |
| `KEYBINDS.md` | Active and planned keybinds |
| `flashlight-overlay.png` | Visual spec for flashlight overlay |
| `frontend/txxt.js` | Browser client — Overlay + SidecarClient |
| `frontend/index.html` | Shell — controls, engine container |
| `backend/src/` | Sidecar — fully built, 52 tests pass |
| `archive/` | Previous versions — reference only |
