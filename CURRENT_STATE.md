# txxt — Current State

Last updated: 2026-02-27

Fast handoff for incoming sessions. Architecture rationale lives in `DESIGN.md`.

---

## Phase: Sidecar Built — Frontend Integration Next

### Flashlight testbed (2026-02-26) — DONE

Browser-only testbed in `frontend/` works locally. FPS rock solid. SVG arc bug
fixed. Still waiting for CloudPC upgrade to run real VDI stress test, but the
sidecar was built in parallel to unblock progress.

### Rust sidecar (2026-02-27) — DONE

Full modular sidecar built. 52 tests, all pass. `cargo build` and `cargo test`
clean (warnings are expected dead code from Phase 1 stubs).

```
backend/src/
  main.rs       — boot, shared state, Axum router, serves frontend/
  world.rs      — pure state machine (ported from archive, +artifact_count, -password_hash)
  persist.rs    — redb save file (ported from archive, no auth/argon2)
  wire.rs       — hop 1 binary protocol (0x10–0x2E), hop 2 stubs
  renderer.rs   — tiny-skia with chrome cache, outputs image + Vec<RenderedShape>
  spatial.rs    — SDF + clip geometry (ported from JS testbed), OverlayGenerator trait
  game.rs       — Axum WS handler, per-connection context, subscribe-before-snapshot
```

**What each module does:**

- **world.rs** — Command/Event state machine. Task has `artifact_count: u8`.
  User has no `password_hash` (auth removed). All archive tests carried forward.
- **persist.rs** — redb save file. `flush()` after every mutation. `load_world()`
  on boot. Seeds default services + user. No argon2.
- **wire.rs** — Hop 1: `BackgroundImage(0x10)`, `CandidateList(0x11)`,
  `OverlayCommands(0x1A)`, `ClientMsg(0x20–0x2E)`. Hop 2: `pack_snapshot` and
  `pack_event` as `#[allow(dead_code)]` stubs with task record stride (192 bytes).
- **renderer.rs** — Chrome cache (grid + headers) rebuilt only on viewport/view
  change. Task layer stamped on clone of chrome. Returns `RenderOutput`:
  image bytes + `Vec<RenderedShape>` (pixel-space bounding boxes for spatial).
- **spatial.rs** — `trait OverlayGenerator` (THE modularity seam). `SdfOverlay`
  is the Phase 1 impl (O(n) brute force, direct port of JS testbed). Separate
  `build_candidate_list()` function. SDF functions for Rect and RoundedRect.
  Clip geometry: segment ∩ circle, arc ∩ circle.
- **game.rs** — Axum WS handler. Subscribe-before-snapshot ordering (critical).
  `CursorMove` → read-lock shapes → spatial pass → send overlay + candidates.
  Mutation → write-lock world → flush → re-render → broadcast.
- **main.rs** — Boot sequence. `AppState` with `Arc<dyn OverlayGenerator>`.
  Router: `/ws` for game, fallback to `ServeDir` for frontend.

### What's next: Frontend integration (Step 7 from the plan)

Wire the browser to the real sidecar. This is the last step before end-to-end
testing on CloudPC.

1. **Delete TestHarness class** from `ironclad.js` (it's throwaway code).
2. **Add SidecarClient class** — WebSocket connection to `ws://localhost:3000/ws`,
   binary message decode, 20Hz cursor position send.
3. **Overlay class stays unchanged** — just gets fed from WS instead of harness.
4. **Rename IRONCLAD → txxt** everywhere:
   - `frontend/ironclad.js` → `frontend/txxt.js`
   - HTML `<title>` and visible text
   - CLAUDE.md, CURRENT_STATE.md, DESIGN.md references
   - JS comments and CSS class names

**Gate:** Same visual behavior as testbed, but driven by real sidecar. Browser
connects, background image appears, cursor movement produces overlay traces.

### After frontend integration

- CloudPC stress test (VDI codec, resource-constrained hardware)
- Materialization module (mousedown → div, candidate list)
- Short-ID mapping (wire uses u16 task IDs, world uses UUIDs)
- EC2 relay (hop 2 — do not wire prematurely)

---

## Modularity seams — what can be swapped

| What | Mechanism | Where |
|------|-----------|-------|
| Overlay algorithm | `trait OverlayGenerator` | `Arc::new(SdfOverlay)` in main.rs |
| Image format | `ImageFormat` enum (Png/Jpeg/Rgba) | `image_format` field in AppState |
| Chrome cache | `ChromeCacheKey` invalidation | Change key fields in renderer.rs |
| Hop 2 transport | `wire::pack_snapshot`/`pack_event` stubs | Wire in game.rs when EC2 ready |

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
| `CURRENT_STATE.md` | This file — where we are and what to build next |
| `CLAUDE.md` | Session instructions for AI partners |
| `KEYBINDS.md` | Active and planned keybinds |
| `flashlight-overlay.png` | Visual spec for flashlight overlay |
| `frontend/` | Browser client — testbed, pending WS integration |
| `backend/src/` | Sidecar — fully built, 52 tests pass |
| `archive/` | Previous versions — reference only |
