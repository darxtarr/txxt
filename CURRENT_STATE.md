# txxt — Current State

Last updated: 2026-02-27

Fast handoff for incoming sessions. Architecture rationale lives in `DESIGN.md`.

---

## Phase: End-to-End Working — Overlay Refinement

### What is done

**Flashlight testbed (2026-02-26)** — DONE. Browser-only stress test proved the
SDF + clip geometry + SVG pool at up to 2000 shapes on real CloudPC VDI.
Results: 32 FPS, 1.04ms frame time at 1260 shapes. Codec lag on fast cursor
moves is expected and is VDI tax — not fixable, not a problem.

**Rust sidecar (2026-02-27)** — DONE. 52 tests pass.

**Frontend integration (2026-02-27)** — DONE. `frontend/txxt.js` connects to
the sidecar via WebSocket. Background image displays. Click the grid to create
tasks. Cursor movement traces task edges. Perf counters and event log visible.
Tested locally and on CloudPC.

---

## What's being built now: Overlay correctness + client robustness

Three issues identified during CloudPC testing, being fixed together:

### 1. Overlay persistence bug (sidecar)

**Problem:** `game.rs` only sends `OverlayCommands` when `!commands.is_empty()`.
When the cursor moves away from shapes, the sidecar sends nothing, so the
browser never clears the last frame's lines.

**Fix:** `was_drawing: bool` per connection in `ConnContext`. State machine:

```
on CursorMove:
    compute commands (SDF pass)

    if commands.is_empty() && was_drawing:
        send empty OverlayCommands   ← clear the lines (state changed)
        was_drawing = false

    elif !commands.is_empty():
        send commands                ← draw/update (state changed)
        was_drawing = true

    else:                            ← empty, was already empty
        send nothing                 ← codec sleeps, no wire traffic
```

This is the key design: the codec only wakes when the overlay set changes.
Cursor gliding through empty space generates zero wire traffic.

### 2. Cursor send frequency (client)

**Problem:** Cursor was sent on a `setInterval` at 20Hz (50ms). This is not
synchronized with the display and can double-send or miss frames. On CloudPC
the display is capped at 32Hz — a 20Hz timer is worse than the framerate.

**Fix:** Cursor send moves into the rAF loop. One check per frame: if cursor
moved since last frame, send. rAF naturally runs at the display refresh rate
(32Hz on CloudPC, 60Hz locally). No separate timer.

### 3. Visibility / focus loss (client)

**Problem:** When a user alt-tabs away, cursor events stop. The overlay lines
from the last position stay visible until the user returns and moves again.

**Fix:** Two layers:
- `visibilitychange` → hidden: call `overlay.update([])` immediately (client-side
  clear, no round-trip), hide flashlight circle, send `0x22 VisibilityChange`
  to sidecar.
- `visibilitychange` → visible: re-send viewport size (window may have been
  resized while hidden), dirty the cursor to trigger a fresh send on next frame.

Window `blur`/`focus` handles the same for focus-loss without full tab switch.

### 4. Viewport resize (client)

**Problem:** `ViewportSize` (0x20) is only sent on connect. If the user resizes
the window, the sidecar keeps rendering at the old size.

**Fix:** `ResizeObserver` on the engine container. Fires on any size change,
sends a fresh `0x20`. Sidecar calls `renderer.resize()`, re-renders, broadcasts.

### 5. Log download (client)

**Problem:** Taking a screenshot on CloudPC takes 3-4 seconds (Windows tooling
is slow on VDI). Not useful for capturing momentary events.

**Fix:** "Save log" button. Exports the full accumulated event log as a `.txt`
file via browser download. Millisecond timestamps throughout. Shareable.

---

## After overlay refinement

- CloudPC stress test with real sidecar (not testbed shapes)
- Materialization module (mousedown → div, fed by candidate list)
- Short-ID mapping (wire uses u16 task IDs, world uses UUIDs)
- EC2 relay (hop 2 — do not wire prematurely)

---

## Sidecar module map

```
backend/src/
  main.rs       — boot, AppState, Axum router, serves frontend/
  world.rs      — pure state machine (+artifact_count, -password_hash)
  persist.rs    — redb save file, flush() on every mutation
  wire.rs       — hop 1 binary protocol (0x10–0x2E), hop 2 stubs
  renderer.rs   — tiny-skia, chrome cache, returns image + Vec<RenderedShape>
  spatial.rs    — SDF + clip geometry, trait OverlayGenerator (THE seam)
  game.rs       — Axum WS handler, subscribe-before-snapshot, was_drawing state
```

## Modularity seams

| What | Mechanism | Where |
|------|-----------|-------|
| Overlay algorithm | `trait OverlayGenerator` | `Arc::new(SdfOverlay)` in main.rs |
| Image format | `ImageFormat` enum (Png/Jpeg/Rgba) | `image_format` in AppState |
| Chrome cache | `ChromeCacheKey` invalidation | renderer.rs |
| Hop 2 transport | `wire::pack_snapshot`/`pack_event` stubs | game.rs when EC2 ready |

Only ONE trait: `OverlayGenerator`. Everything else is data boundaries or config.

---

## Architecture recap (locked)

```
[Laptop] ── VDI/WAN ──► [Azure CloudPC]
                          │  localhost
                     [txxt] ◄──WS──► [Sidecar]
                          │
                       WAN/corporate
                          │
                     [AWS EC2 Ireland]
```

The axiom: **everything is possible but nothing is real until the click.**

---

## Wire protocol summary (hop 1)

All multi-byte values little-endian.

| Byte | Direction | Message | Payload |
|------|-----------|---------|---------|
| 0x10 | S→B | BackgroundImage | rev:u64, format:u8, w:u16, h:u16, bytes |
| 0x11 | S→B | CandidateList | count:u8, per-entry: id:u16 x/y/w/h:u16 color:u32 sdf:i16 title |
| 0x1A | S→B | OverlayCommands | count:u8, per-cmd (see below) |
| 0x20 | B→S | ViewportSize | w:u16, h:u16 |
| 0x22 | B→S | VisibilityChange | visible:u8, last_rev:u64 |
| 0x23 | B→S | CursorMove | x:u16, y:u16 |
| 0x27 | B→S | CreateAt | x:u16, y:u16 |

OverlayCommands per-command encoding:

| cmd byte | Type | Payload | Total |
|----------|------|---------|-------|
| 0 | Clear | 12 bytes padding | 13 |
| 1 | Arc | cx:f32 cy:f32 r:f32 start:f32 sweep:f32 color:u32 | 25 |
| 2 | Segment | x1:u16 y1:u16 x2:u16 y2:u16 color:u32 | 13 |

---

## Data model (locked 2026-02-19)

- `Task.start: Option<u32>` — minutes since Unix epoch. `None` = staged.
- `Task.duration: Option<u16>` — minutes.
- `Task.artifact_count: u8` — attached files count.
- Derive: `epoch_day = start/1440`, `dow = (epoch_day+3)%7`, `time = start%1440`.
- Views: sliding windows `window_start..window_end` (minutes since epoch).

---

## Running the sidecar

```bash
cd backend && cargo build          # Build
cd backend && cargo test           # 52 tests
cd backend && cargo run            # Starts on :3000, serves frontend/

# Environment variables:
TXXT_DB=path/to/txxt.redb          # Save file (default: txxt.redb)
PORT=3000                          # Listen port (default: 3000)
TXXT_FRONTEND=../frontend          # Frontend path (default: ../frontend)
```

---

## Key files

| File | What |
|------|------|
| `DESIGN.md` | Architecture source of truth — read first |
| `CURRENT_STATE.md` | This file |
| `CLAUDE.md` | Session instructions for AI partners |
| `KEYBINDS.md` | Active and planned keybinds |
| `flashlight-overlay.png` | Visual spec for flashlight overlay |
| `frontend/txxt.js` | Browser client — Overlay class + SidecarClient |
| `frontend/index.html` | Shell |
| `backend/src/` | Sidecar — fully built, 52 tests pass |
| `archive/` | Previous versions — reference only |
