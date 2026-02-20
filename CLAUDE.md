# txxt — Project Context

**What this is:** A scheduling portal for CloudPC ops teams. SMO (Small
Multiplayer Online) — 5-20 users on software-rendered VDI desktops with no GPU
and bandwidth-constrained codec connections.

**Who built this:** Ulli (darxtarr) with rotating Mentat partners (Claude
instances — Opus and Sonnet). Multi-session, multi-model collaboration.

**Important framing:** DESIGN.md is the output of brainstorming sessions, not
proven production truth. Every hypothesis needs testing on real CloudPC
hardware. If you can prove something wrong, that's a win.

## Before You Do Anything

**Read DESIGN.md.** The entire document. It explains:
- Why every architectural decision was made
- The VDI constraints that drive everything
- The pit stop interaction model
- What the browser does (almost nothing) and why

The axiom is: **"Everything is possible but nothing is real until the click."**
If you don't understand what this means after reading DESIGN.md, ask Ulli
before writing code.

## Architecture (60-second version)

```
Browser (IRONCLAD)  →  just glass. Displays one image. Tracks mouse.
         ↕ localhost WebSocket
Rust sidecar        →  brain. Renders world to image. Owns hit testing.
         ↕ WAN WebSocket
EC2                 →  memory. Relays mutations. Stores artifacts.
```

The browser does NOT render world state. The browser does NOT do hit testing.
The browser does NOT own state. The sidecar does all of this and sends the
browser a flat image to display. One div materializes on click. Zero divs at
rest.

Spatial queries use the **SDF flashlight** — a fixed-radius circle of signed
distance field evaluations around the cursor. All interactable elements (tasks,
buttons, panels, drop zones) are SDFs. The sidecar pushes the candidate list
to the browser, which can materialize instantly on click without a round-trip.
Elements within the flashlight radius get a proximity glow (canvas overlay) —
this is a VDI heartbeat, continuous proof that the system is alive.

## What NOT To Do

**Do not add a JS framework.** No React, no Vue, no Svelte. The browser is
glass — it displays an image and tracks a mouse. You do not need a component
tree for that.

**Do not add canvas rendering of world state.** The canvas-era code in
ironclad.js v0.8 is legacy. The sidecar renders; the browser displays.
Rendering in two places creates divergence. (A small canvas overlay for cursor
glow and proximity highlights IS part of the design — that's local feedback,
not world rendering.)

**Do not auto-generate task titles.** Email subjects are terrible task names.
Drops create tasks with empty titles and immediately open editing. The human
types the title. This is a deliberate design decision born from NOC
operational experience.

**Do not try to eliminate the "flash" on background swap.** It's intentional
tactile feedback. See DESIGN.md "The Flash Is a Feature."

**Do not add source metadata to tasks** (source: Email, source: Manual, etc.).
The task exists because there's a problem. How it arrived is trivia. The
email is an artifact (attachment), not a task property.

**Do not propose CRDT, event sourcing, or protocol versioning.** These are
explicitly rejected in DESIGN.md with reasoning. Read the reasoning before
re-proposing.

## What's Solid (don't rewrite)

- `world.rs` — pure state machine, 21 tests, the core
- `wire.rs` — binary protocol, 12 tests, proven
- `persist.rs` — redb ACID persistence, 4 tests
- `game.rs` — WebSocket handler with subscribe-before-snapshot

## What's Transitional (being replaced)

- `ironclad.js` v0.8 — canvas rendering era. Wire protocol and interaction
  logic are portable. Canvas drawing code is not.
- `renderer.rs` — proof of concept (tiny-skia works). Needs viewport
  negotiation, chrome layer caching, and wiring into game.rs.

## What's Next

The sidecar image pipeline: renderer.rs → game.rs → new IRONCLAD. See the
"What's next" section at the end of DESIGN.md for the ordered task list.

## Tools & Build

```bash
cd backend && cargo build          # Build sidecar
cd backend && cargo test           # Run all tests (37 tests)
cd backend && cargo run            # Start on :3000, serves frontend/
```

Frontend has no build step. `frontend/index.html` + `frontend/ironclad.js` +
`frontend/styles.css`. Served as static files.

## Key Files

| File | What | Lines |
|------|------|-------|
| `DESIGN.md` | Source of truth — read this first | ~700 |
| `INTERACTIONS.md` | Gesture design decisions | ~110 |
| `KEYBINDS.md` | Active and planned keybinds | ~45 |
| `backend/src/world.rs` | Pure state machine | ~780 |
| `backend/src/wire.rs` | Binary wire protocol | ~590 |
| `backend/src/persist.rs` | redb persistence | ~410 |
| `backend/src/game.rs` | WebSocket game handler | ~120 |
| `backend/src/renderer.rs` | tiny-skia renderer (PoC) | ~280 |
| `backend/src/auth.rs` | JWT + Argon2 (dev mode) | ~130 |
| `backend/src/main.rs` | Boot sequence | ~90 |
| `frontend/ironclad.js` | Canvas-era frontend (v0.8) | ~1230 |
