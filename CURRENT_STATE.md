# txxt — Current State

Last updated: 2026-02-26

Fast handoff for incoming sessions. Architecture rationale lives in `DESIGN.md`.

---

## Phase: Flashlight Overlay Stress Test

**Testbed implemented and working locally.** Waiting for CloudPC upgrade to
complete before running the real stress test on VDI hardware.

### What was built (2026-02-26)

Three fresh files in `frontend/` — no server, open `index.html` directly:

- **`index.html`** — control bar (radius, shape count, overlap density, show
  circle toggle, regenerate button), perf counter display, bootstrap script.
- **`styles.css`** — dark theme, SVG overlay with 80ms opacity fade
  transitions, perf counter overlay.
- **`ironclad.js`** — two clearly separated sections:
  1. **Overlay class (PRODUCTION)** — SVG pool pattern. 256 `<line>` + 64
     `<path>` elements allocated at init, never created/destroyed. `update()`
     maps draw commands to pool slots (active → opacity 1, inactive → opacity
     0). CSS transition handles fade. Zero DOM churn, zero GC pressure. This
     module has no knowledge of shapes, SDF, or cursors — it receives commands
     and executes them. Survives into final IRONCLAD unchanged.
  2. **TestHarness class (THROWAWAY)** — simulates the Rust sidecar. Shape
     generation with cluster-based overlap density. Canvas background baking.
     SDF functions (rect, circle, rounded rect, triangle). Clip geometry:
     circle ∩ segment (parametric quadratic), circle ∩ arc (two-circle
     intersection with angle range clipping). rAF loop with per-phase
     `performance.now()` timing. Rolling 60-frame perf averages.

### Bug fixed: full-circle SVG arcs

SVG `<path>` arcs cannot represent a full circle — when startAngle and
endAngle produce the same (x,y) point, the arc draws nothing. This caused
circles fully inside the flashlight to go dark (only a dot visible from
`stroke-linecap: round`). Fix: `arcToPath()` now detects sweepAngle ≥ 2π and
splits into two semicircular arc commands.

### Local test results

FPS rock solid at all shape counts tested locally. No performance cliff found
on a dev machine — the real test is CloudPC hardware under VDI codec, pending
upgrade.

### Next: CloudPC stress test

The goal: prove the flashlight overlay interaction model works under VDI codec
compression on a resource-constrained CloudPC. Not a pretty demo — a stress
test. We push shape count until performance degrades, find the ceiling, and
use that data to decide what to build next.

---

## What the flashlight overlay IS

Look at `flashlight-overlay.png` in the repo root. That sketch is the spec.

- The **background image** is static. It contains shapes (task cards, buttons,
  etc.) baked into a flat image. Between interactions the VDI codec sleeps
  because nothing changes.
- The **flashlight** is an invisible circle centered on the cursor. It has a
  configurable radius.
- When the flashlight circle overlaps a shape's boundary, the **portion of
  that boundary inside the circle** is drawn as a green SVG line on top of
  the background. Only the clipped fragment — not the full edge, not the full
  shape.
- Multiple overlapping shapes (common in production — same time slot, multiple
  users) all trace simultaneously. If 5 shapes overlap under the flashlight,
  all 5 shapes' clipped edges appear.
- Move the cursor away → the green traces fade out (CSS opacity transition,
  ~80ms).
- In empty space: nothing. No traces, no glow, no dirty pixels. Codec sleeps.

This is the **liveness signal**. The cursor moves locally (VDI client renders
it). The overlay updates through the browser → codec → screen pipeline. If
the overlay tracks the cursor: session is alive. If it lags: session is slow.
If it freezes: session is dead. No status widget needed — the physics of the
interaction IS the health check.

---

## What we are building (three files, all fresh)

```
frontend/
  index.html      — page structure, control bar, script tag
  styles.css      — dark theme, SVG overlay styling, perf counter styling
  ironclad.js     — two sections:
                     1. Overlay module (PRODUCTION — survives into final IRONCLAD)
                     2. Test harness (THROWAWAY — replaced by real sidecar later)
```

No server needed. Open `index.html` in a browser (file:// or localhost).

### Overlay module (production code)

A dumb SVG executor. Receives an array of draw commands. Draws them. Knows
nothing about shapes, SDF, cursor position, or the world.

```
class Overlay {
  constructor(container, opts)  // creates SVG element + fixed pool of <line>/<path>
  update(commands)              // maps commands to pool elements, shows active, hides rest
  destroy()                     // cleanup
}
```

**Pool pattern**: fixed set of `<line>` and `<path>` SVG elements allocated at
startup. Never created or destroyed after init. Active elements get attributes
updated + `opacity: 1`. Inactive elements get `opacity: 0`. CSS transition
handles fade-out. Zero GC pressure. Zero DOM churn.

**Command format** (mirrors what the sidecar will send as `0x1A OverlayCommands`):
```
{ type: 'segment', x1, y1, x2, y2 }
{ type: 'arc', cx, cy, r, startAngle, sweepAngle }
{ type: 'clear' }
```

This module's API is the contract. When the real sidecar exists, it sends
binary `0x1A` messages. IRONCLAD decodes them into these same command objects
and calls `overlay.update()`. The module doesn't change.

### Test harness (throwaway code)

Simulates what the sidecar does. Replaced entirely when the Rust sidecar
exists. Clearly marked in the source.

Responsibilities:
1. **Define shapes** — random/semi-random placement with deliberate overlaps
2. **Bake background** — draw shapes to offscreen canvas → export as data URL
   → set as `background-image` on container. This simulates the sidecar's
   tiny-skia render output.
3. **SDF evaluation** — on each cursor update, compute signed distance from
   cursor to every shape boundary. O(n) brute force — we want to find where
   n kills performance.
4. **Clip geometry** — for shapes within flashlight radius, clip their edge
   primitives to the flashlight circle interior. Two intersection primitives:
   circle ∩ line segment (quadratic solve) and circle ∩ arc (two-circle
   intersection). Output: array of draw commands.
5. **Feed overlay** — call `overlay.update(commands)` every frame.

### Shape decomposition

Every shape decomposes to line segments and arcs before clipping:
- Rectangle → 4 segments
- Rounded rectangle → 4 segments + 4 quarter-circle arcs
- Circle → 1 full arc
- Triangle/polygon → N segments

The harness stores both the shape (for SDF) and its edge primitives (for
clipping). The clip math only knows segments and arcs — it doesn't care what
shape they came from.

---

## Controls and diagnostics

**Control bar:**
- Flashlight radius: slider 20–500px, default 120
- Shape count: slider 10–2000, default 50 (triggers regenerate)
- Overlap density: slider (spread vs pile-up, controls clustering)
- Show flashlight circle: checkbox (yellow SVG circle, default ON — turn off
  once you've confirmed the radius visually)
- Regenerate button

**Performance counters** (always visible, top-right):
```
FPS: 60 | frame: 16.7ms
SDF:    0.12ms (50 shapes)
Clip:   0.08ms (12 edges)
SVG:    0.05ms (12/48 lines, 3/16 arcs)
Total:  0.25ms
```

Using `performance.now()`. Rolling average over 60 frames.

**Test procedure:**
1. Start at 50 shapes. Confirm traces align with shapes. Confirm fade works.
2. Crank to 200, 500, 1000, 2000. Watch FPS and frame time.
3. Increase overlap density. Watch SVG active count (more overlaps = more
   simultaneous traces = more draw commands per frame).
4. Find the cliff: where does FPS drop below 30?
5. On CloudPC: repeat with Task Manager open. Note CPU% at each shape count.
6. Key question: do the green traces read clearly through the VDI codec?

---

## Architecture recap (locked)

```
[Laptop] ── VDI/WAN ──► [Azure CloudPC]
                          │  localhost
                     [IRONCLAD] ◄──WS──► [Sidecar]
                          │
                       WAN/corporate
                          │
                     [AWS EC2 Ireland]
```

The axiom: **everything is possible but nothing is real until the click.**

In production, the sidecar sends two independent outputs from the same SDF pass:
- `0x11` CandidateList — click semantics (what to materialize on mousedown)
- `0x1A` OverlayCommands — UX only (clipped element edges for flashlight)

The testbed only implements the overlay path. Materialization comes next, as a
separate module with a separate test, after the overlay is proven on hardware.

---

## Data model (locked 2026-02-19)

- `Task.start: Option<u32>` — minutes since Unix epoch. `None` = staged.
- `Task.duration: Option<u16>` — minutes.
- Derive: `epoch_day = start/1440`, `dow = (epoch_day+3)%7`, `time = start%1440`.
- Views: sliding windows `window_start..window_end` (minutes since epoch).

---

## Build order (after testbed passes)

Do not start here until the overlay stress test has run on real CloudPC
hardware and we know the performance ceiling.

1. `world.rs` — pure state machine, no I/O
2. `wire.rs` — binary protocol, both hops
3. `renderer.rs` — tiny-skia, two-layer chrome cache
4. `game.rs` — per-connection context, wires everything together
5. IRONCLAD — replace test harness with real WebSocket; overlay module is
   already written and tested from the testbed
6. Materialization module — candidate cache, mousedown → div, mouseup → remove
7. Spatial query in Rust — SDF + clip geometry, matching the JS harness output
8. EC2 relay — do not wire prematurely

Phase 1 target: single-user, localhost only, full interaction loop.

---

## Key files

| File | What |
|------|------|
| `DESIGN.md` | Architecture source of truth — read first |
| `CURRENT_STATE.md` | This file — where we are and what to build next |
| `CLAUDE.md` | Session instructions for AI partners |
| `KEYBINDS.md` | Active and planned keybinds |
| `flashlight-overlay.png` | Visual spec for the flashlight overlay |
| `frontend/` | IRONCLAD — testbed in progress |
| `backend/src/` | Sidecar — empty until testbed passes |
| `archive/` | Previous versions — reference only |
