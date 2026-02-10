# Handover (2026-02-10, end of Opus session)

## What this is

txxt is a Small Multiplayer Online (SMO) scheduling portal. Game server
architecture: Rust/axum owns all state in memory, browser clients are
dumb renderers connected via binary WebSocket. NOT a web app.

## Current state: CLEAN, COMPILING, 30 TESTS PASSING

```
cargo test          # 30 tests pass (world: 17, wire: 9, persist: 4)
cargo run           # boots server on :3000, serves frontend/
```

Nothing is half-done. No broken branches. No uncommitted changes that matter.

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
- `frontend/ironclad.js` — Canvas renderer. SoA arrays, flashlight UI, drag-drop.
- `frontend/index.html` — Entry point. Connects to ws://hostname:3000/api/game

**Dead code** lives in `archive/backend-rest-era/` (old REST CRUD layer, kept for reference).

## What was just built (this session)

Phases 1-4 of the game server + IRONCLAD connection:
1. World struct (pure state machine, zero IO)
2. redb persistence (save file pattern)
3. axum WebSocket handler (binary protocol)
4. Fixed-stride wire protocol (no JSON anywhere)
5. Connected IRONCLAD renderer to game server
6. Consolidated txxt2 repo into txxt/frontend/

## NEXT TASK: Double-click to create

User wants: double-click on calendar grid → create 30-minute task at that slot.

**Implementation plan (4 touch points):**

### 1. world.rs — Add optional scheduling to CreateTask

Add `day: Option<u8>, start_time: Option<u16>, duration: Option<u16>` to
`Command::CreateTask`. In `apply()`, if all three are Some, validate and
create as Scheduled directly. If None, create as Staged (existing behavior).

**Watch out:** There are ~6 places that construct `Command::CreateTask` —
the enum definition, apply() match arm, test helper `create_task()`, and
3 inline test calls. ALL must get the new fields or it won't compile.

### 2. wire.rs — Update CMD_CREATE_TASK (0x10) format

Current format:
```
[0]      type (0x10)
[1]      priority
[2..18]  service_id
[18..34] assigned_to
[34..]   title
```

New format (add scheduling between assigned_to and title):
```
[0]      type (0x10)
[1]      priority
[2..18]  service_id
[18..34] assigned_to
[34]     day (0xFF = no scheduling / staged)
[35]     _pad
[36..38] start_time (u16 LE)
[38..40] duration (u16 LE)
[40..]   title
```

Update `unpack_command()` for CMD_CREATE_TASK: min length becomes 40,
read day at [34] (0xFF → None, else Some), start_time/duration from LE u16s.
Update the `unpack_create_task_command` test too.

### 3. ironclad.js — Add dblclick handler + _sendCreateTask

- Add `CMD_CREATE_TASK: 0x10` to WIRE constants
- Store `this.defaultServiceId` from first service in snapshot
- Add `_sendCreateTask(day, startTime, duration)` method that packs the new format
- Add `dblclick` handler in `_bindInput()`:
  - Convert click pixel position to day + startTime (same math as _sendMoveTask)
  - Duration = 30 (minutes, default)
  - Title = "New task"
  - Call `_sendCreateTask()`
- The existing `_onTaskCreated` handler already handles scheduled tasks
  (checks day !== 0xFF), so no changes needed there

### 4. Optional: Relax grid validation

INTERACTIONS.md mentions relaxing from 15-min to 5-min grid for future
resize precision. Could do `% 5` instead of `% 15` in validate_scheduling().
Not required for dblclick — 15-min snap is fine for now.

## Future ideas (documented in INTERACTIONS.md)

- Drag bottom edge to resize (same MoveTask command, different duration)
- Alt+drag to clone (CreateTask with original's metadata + new position)
- Different snap resolutions (30-min for move, 5-min for resize)
- Modifier-click for manual time entry panel
- Multi-day tasks: PUSHBACK — use cloning instead
- Recurring tasks: PUSHBACK — separate subsystem later

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
cd backend && cargo test    # 30 tests, all should pass
```
