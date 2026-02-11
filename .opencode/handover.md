# Handover (2026-02-11, end of Opus session)

## What this is

txxt is a Small Multiplayer Online (SMO) scheduling portal. Game server
architecture: Rust/axum owns all state in memory, browser clients are
dumb renderers connected via binary WebSocket. NOT a web app.

## Current state: CLEAN, COMPILING, 33 TESTS PASSING

```
cargo test          # 33 tests pass (world: 19, wire: 10, persist: 4)
cargo run           # boots server on :3000, serves frontend/
```

Branch: `main` — all feature work lives here. CloudPC pulls from main.
Nothing is half-done. No uncommitted changes that matter.

## Branch conventions (READ THIS)

- **`main`** — all feature development. This is what CloudPC pulls. Always push here after commits.
- **`oxygen/profiling-lab`** — profiling, benchmarking, and optimization sessions with Codex **only**.
  Do NOT build features on this branch. The correct flow is:
  1. Copy production code to profiling-lab
  2. Run profiling harness / optimize
  3. Cherry-pick optimized code back to main once done
  4. Leave profiling tooling (PROFILING.md, scripts/) on the branch — it does not belong on main

## Architecture (read DESIGN.md for full details)

```
Client (frontend/ironclad.js)
  ↕ binary WebSocket (fixed-stride packed structs)
Server (backend/)
  World (in-memory HashMap<Uuid, Entity>)
  ↕ sync flush on every mutation
  redb save file (tasks.redb)
```

**Key files:**
- `backend/src/world.rs` — THE state machine. Command → apply() → Event. All mutations here.
- `backend/src/wire.rs` — Binary protocol. 192-byte task records, 80-byte services. DataView-readable.
- `backend/src/persist.rs` — redb wrapper. load_world() on boot, flush() on mutation.
- `backend/src/game.rs` — WebSocket handler. Subscribe → snapshot → command loop.
- `backend/src/auth.rs` — Login endpoint + JWT. Dev-mode bypass for WS.
- `frontend/ironclad.js` — Canvas renderer. SoA arrays, flashlight UI, drag-drop, resize, dblclick create.
- `frontend/index.html` — Entry point. Connects to ws://hostname:3000/api/game

**Dead code** lives in `archive/backend-rest-era/` (old REST CRUD layer, kept for reference).

## What was built this session (2026-02-11)

1. **Double-click to create** — dblclick on calendar grid → 30-min task at that slot.
   - `Command::CreateTask` gained optional `day/start_time/duration` fields
   - Wire format `CMD_CREATE_TASK` (0x10) extended: scheduling at [34..40], title at [40..]
   - `day=0xFF` means staged (no scheduling), any other value = create as Scheduled
   - IRONCLAD grabs first service ID from snapshot as default

2. **Drag to resize** — grab top or bottom edge of a task (~8px zone) to change duration/start.
   - Bottom edge: changes duration (end moves, start fixed)
   - Top edge: changes start time (top moves, bottom fixed)
   - Both snap to 15-min grid on drop
   - Same `MoveTask` command — no backend changes needed
   - Cursor hint: `ns-resize` near edges, `grab` in the middle

3. **Docs updated** — DESIGN.md interaction model table, INTERACTIONS.md status markers,
   known quirk about browser extensions blocking dblclick.

## Interaction model status (see DESIGN.md + INTERACTIONS.md)

| Gesture              | Action          | Status   |
|----------------------|-----------------|----------|
| Drag task            | Move to slot    | DONE     |
| Double-click grid    | Create 30m task | DONE     |
| Drag top/bottom edge | Resize duration | DONE     |
| Alt+drag (mod-drag)  | Clone task      | NEXT     |
| Modifier-click       | Manual entry    | DEFERRED |

## NEXT TASK: Alt+drag to clone

See INTERACTIONS.md for the design. Under the hood: sends `CreateTask` with
the original's metadata (title, service, priority) but at the new position.
Straightforward once you understand the existing drag + create code paths.

**Implementation sketch:**
1. In mousedown: if Alt key held during proxy drag → set `dragMode = 'clone'`
2. In mouseup (clone mode): read original task's metadata from SoA arrays,
   send `_sendCreateTask()` with the drop position instead of `_sendMoveTask()`
3. Reset original task position (it didn't actually move)

**Wire note:** `_sendCreateTask` currently hardcodes title="New task" and
priority=Medium. For cloning, you'll want to pass these as parameters. The
wire format already supports them — just needs the JS to pack the original's
values instead of defaults.

## Future ideas (documented in INTERACTIONS.md)

- Different snap resolutions (30-min for move, 5-min for resize)
- Modifier-click for manual time entry panel
- Multi-day tasks: PUSHBACK — use cloning instead
- Recurring tasks: PUSHBACK — separate subsystem later

## Known quirks

- Some browser extensions block dblclick events. Works in incognito.

## User preferences (IMPORTANT)

- **Always push to remote** after commits — CloudPC pulls from origin
- Gamedev mindset, not webdev. Binary, not JSON. Simple, not enterprise.
- Honest analysis > eager coding. Push back on bad ideas.
- Full trust and authority given — make it shine, review later.
- User might send GPT/Gemini to do work between sessions. Expect surprises.

## How to run

```bash
cd backend && cargo run     # server on :3000
# open http://localhost:3000 in browser
# login: admin / admin (or skip — dev bypass on WS)
```

## How to test

```bash
cd backend && cargo test    # 33 tests, all should pass
```
