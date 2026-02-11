# txxt Codebase Analysis
*Generated 2026-02-11*

## Project Overview

**txxt** is a Small Multiplayer Online (SMO) scheduling portal built with a **game server architecture** (not a traditional web app). It targets 5-20 ops users on CloudPCs with a single Rust/axum backend binary and a zero-dependency Canvas/DOM hybrid frontend called IRONCLAD.

**Mental Model**: The server owns all state in memory (authoritative). Clients are thin renderers that send binary commands over WebSocket and receive packed binary state deltas. Think Bloomberg terminal + game engine, not REST API + React SPA.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│  IRONCLAD (frontend/)                                        │
│  - Canvas/DOM hybrid renderer                                │
│  - Binary WebSocket consumer                                 │
│  - Drag-drop, SoA entity storage, spatial bucketing          │
│  - Zero deps, 878 lines of handwritten JS                    │
└─────────────────────────────────────────────────────────────┘
              ↕ Binary frames (fixed-stride packed records)
┌─────────────────────────────────────────────────────────────┐
│  txxt Server (backend/)                                      │
│  ┌─────────────────────────────────────────────────────────┐│
│  │ World (in-memory HashMap<Uuid, Entity>)                 ││
│  │  - tasks, users, services                               ││
│  │  - revision counter (monotonic)                         ││
│  │  - event log for reconnect                              ││
│  └─────────────────────────────────────────────────────────┘│
│              ↕ postcard serialization                        │
│  ┌─────────────────────────────────────────────────────────┐│
│  │ redb (save file, ACID, single-file)                     ││
│  │  - Loaded on boot, flushed on every mutation            ││
│  │  - Never queried at runtime                             ││
│  └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

---

## Backend Architecture (Rust)

### Core Modules (~2000 lines, 33 passing unit tests)

#### **world.rs** (750 lines, 19 tests) — The State Machine

Pure, deterministic state machine with zero I/O.

**Key Types:**
- **Task**: id, title, status (Staged→Scheduled→Active→Completed), priority (Low/Medium/High/Urgent), service_id, created_by, assigned_to, day (0-6), start_time (u16 mins), duration (u16 mins)
- **Command**: Enum with variants CreateTask, ScheduleTask, MoveTask, UnscheduleTask, CompleteTask, DeleteTask
- **Event**: Enum with TaskCreated, TaskScheduled, TaskMoved, etc. — these are broadcast to all clients
- **World**: HashMap-based entity storage + u64 revision counter + Vec event log

**Mutation Flow:**
```rust
pub fn apply(&mut self, cmd: Command, user_id: Uuid) -> Result<Event, WorldError>
```
Every command:
1. Validates against business rules (day 0-6, time on 15-min grid, duration ≥ 15 mins)
2. Mutates state in memory
3. Increments revision counter
4. Appends to event log
5. Returns the Event for broadcast

**State Transitions** (validated):
- Staged → Scheduled (via ScheduleTask)
- Scheduled ↔ Scheduled (via MoveTask — can move or change duration)
- Scheduled → Active (implicit, not a command yet)
- Scheduled/Active → Completed (via CompleteTask)
- Scheduled/Active → Staged (via UnscheduleTask)

**Validation Helpers:**
- `validate_scheduling(day, start_time, duration)` — enforces day 0-6, time mod 15 = 0, duration > 0 and not past midnight

**Tests Cover:**
- Task creation (starts Staged)
- State transition rules (can't double-schedule, can't move Staged tasks)
- Scheduling validation (day/time/duration bounds)
- Event log and revision counter
- Staging queue (returns Staged tasks sorted by priority)
- Reconnect replay via `events_since(rev)`

---

#### **wire.rs** (600 lines, 10 tests) — Binary Protocol

**Philosophy:** "JSON is never used in the data path."

Hand-rolled fixed-stride packed binary, readable by JS DataView at known offsets.

**Task Record (192 bytes fixed stride):**
```
[0..16]    id (UUID)
[16]       status (u8: 0=Staged, 1=Scheduled, 2=Active, 3=Completed)
[17]       priority (u8: 0=Low, 1=Medium, 2=High, 3=Urgent)
[18]       day (u8: 0-6 Mon-Sun, 0xFF = not scheduled / staged)
[19]       _pad
[20..22]   start_time (u16 LE, minutes from midnight)
[22..24]   duration (u16 LE, minutes)
[24..40]   service_id (UUID)
[40..56]   assigned_to (UUID, zeroed = unassigned)
[56..184]  title (128 bytes UTF-8 zero-padded)
[184..192] _reserved
```

**Service Record (80 bytes fixed stride):**
```
[0..16]    id (UUID)
[16..80]   name (64 bytes UTF-8 zero-padded)
```

**Server → Client Messages:**
- `0x01` Snapshot: full state dump at startup
- `0x02` TaskCreated: new task (includes full 192-byte task record)
- `0x03` TaskScheduled: task scheduled (day, start, duration)
- `0x04` TaskMoved: task moved (same fields as Scheduled)
- `0x05` TaskUnscheduled: task removed from grid
- `0x06` TaskCompleted: task marked done
- `0x07` TaskDeleted: task removed entirely

All events include revision counter (u64 LE at offset 1) for sync arbitration.

**Client → Server Commands:**
- `0x10` CreateTask: [type][priority][service_id][assigned_to][day][pad][start u16][dur u16][title]
- `0x11` ScheduleTask: [type][task_id][day][start][duration]
- `0x12` MoveTask: same as ScheduleTask
- `0x13` UnscheduleTask: [type][task_id]
- `0x14` CompleteTask: [type][task_id]
- `0x15` DeleteTask: [type][task_id]

**Packing/Unpacking:**
- `pack_snapshot()` — serializes entire World into binary frame
- `pack_event()` — serializes individual Event
- `unpack_command()` — deserializes client command with validation (frame bounds, UTF-8)

**Tests:** Round-trip layout validation, field offset checks, garbage rejection.

---

#### **persist.rs** (400 lines, 4 tests) — redb Save File Layer

**Philosophy:** "redb stays as the save file. Loaded once, flushed on mutation, never queried."

**SaveFile struct:**
- Thin Arc<Database> wrapper
- `open(path)` — creates or opens .redb file, initializes tables
- `load_world()` — loads entire World from disk (called once at boot)
- `flush(world, event)` — writes affected entity + revision in one transaction

**Seeding:**
- `ensure_default_services()` — inserts 12 hardcoded services (Billing Portal, Customer Support, etc.) if none exist
- `ensure_default_user()` — creates admin/admin user (Argon2-hashed) if none exist
- `save_user()`, `save_service()` — explicit persistence

**Tables:**
- `world_tasks` — postcard-serialized Task values
- `world_users` — postcard-serialized User values
- `world_services` — postcard-serialized Service values
- `world_meta` — revision counter as key-value

**Tests:**
- Round-trip empty world
- Seed and reload
- Flush task mutations and reload
- Delete removes from disk

---

#### **game.rs** (120 lines) — WebSocket Handler

**Lifecycle:**
1. Client connects to `/api/game` (WebSocket upgrade)
2. Server subscribes to broadcast channel (before sending snapshot to avoid race)
3. Server sends full binary snapshot to this client
4. Spawns two async tasks:
   - **send_task**: forwards broadcast events to this client
   - **recv_task**: reads incoming commands from this client, calls `handle_command()`
5. `select!` waits for either task to finish (client disconnect), aborts the other

**Command Processing** (`handle_command`):
- Deserialize binary frame via `wire::unpack_command()`
- Acquire write lock on World
- Call `world.apply(cmd, user_id)`
- Flush to save file (sync)
- Broadcast packed event via `game_tx.send()`
- On error, log and return (silent to client)

**Dev Mode:** Uses first user in World or Uuid::nil if none. No token validation on WS yet.

---

#### **auth.rs** (130 lines) — Login & JWT

**Handlers:**
- `POST /api/auth/login` — LoginRequest → LoginResponse with JWT token
- `POST /api/auth/logout` — Stub (returns OK)

**JWT Implementation:**
- Claims: sub (user_id), username, exp (24 hours), iat
- Secret: hardcoded `"your-secret-key-change-in-production"` (dev only)
- Uses `jsonwebtoken` crate with Argon2 password verification

**⚠️ Issues:**
- JWT secret hardcoded (dev mode)
- WS doesn't validate token yet (dev bypass)
- No rate limiting on login

---

#### **main.rs** (90 lines) — Boot Sequence

1. Open save file
2. Load World
3. Seed defaults (services + user) if empty
4. Create broadcast channel (256-message buffer)
5. Wrap in Arc<AppState>
6. Build Router:
   - `/api/auth/login` (POST)
   - `/api/auth/logout` (POST)
   - `/api/game` (GET, WebSocket)
   - Fallback: ServeDir(frontend/)
7. CORS layer (allow all origins)
8. Listen on 0.0.0.0:3000

---

### Dependencies (11 direct)

```toml
axum         # web framework + WebSocket
tokio        # async runtime
redb         # ACID save file
serde        # serialization trait
postcard     # binary serialization (interim → rkyv)
uuid         # entity IDs
argon2       # password hashing
jsonwebtoken # JWT (heavy — future: replace with HMAC-SHA256)
tower-http   # CORS + ServeDir
chrono       # JWT timestamps (future: replace with time crate)
futures-util # async utilities
```

---

## Frontend Architecture (IRONCLAD.js)

### Overview

Zero-dependency vanilla JS. Canvas + DOM hybrid. Connects to `/api/game` via binary WebSocket.

**Design Philosophy:** WASM-like mindset — SoA entity storage, zero allocations in hot paths, spatial bucketing, frame-stamp dedup.

### Core Components

#### SoA Entity Storage (max 10,000 entities)

- `ids` (Int32Array)
- `xs`, `ys`, `ws`, `hs` (Float32Array)
- `types` (Uint8Array) — drives color mapping
- `labels` (Array of strings)
- `uuids` (Uint8Array flat, 16 bytes per entity)

#### Wire Protocol (JS side)

Mirrors backend/src/wire.rs exactly:
```javascript
const WIRE = {
    SNAPSHOT: 0x01,
    TASK_CREATED: 0x02,
    TASK_SCHEDULED: 0x03,
    TASK_MOVED: 0x04,
    TASK_UNSCHEDULED: 0x05,
    TASK_COMPLETED: 0x06,
    TASK_DELETED: 0x07,
    CMD_MOVE_TASK: 0x12,
    TASK_STRIDE: 192,
    // ... field offsets
};
```

#### Input Handling (Implemented)

**Drag-to-move:** mousedown on proxy div → mousemove → mouseup → snap to 15-min grid → send MoveTask command

**Double-click to create:** dblclick on grid → 30-min Scheduled task at cursor → send CreateTask command

**Drag-to-resize:** mousedown on top/bottom ~8px edge zone → drag → snap to 15-min grid → send MoveTask with new duration

#### Performance Optimizations

1. SoA layout — better cache locality
2. Spatial bucketing — flashlight only checks ~3 buckets
3. Frame-stamp dedup — avoid processing entities spanning buckets twice
4. Pre-allocated candidates array — zero allocations in flashlight loop
5. Insertion sort — typically < 50 candidates
6. transform-only DOM updates — no layout recalc
7. Canvas DPR scaling — handles Retina and VDI scaling

**Observed Performance (CloudPC):** 32fps @ 5000 entities, drag latency < 20ms, no GC pauses in hot paths.

---

## Branch conventions

- **`main`** — all feature development. CloudPC pulls from here. Always push here after commits.
- **`oxygen/profiling-lab`** — profiling, benchmarking, and optimization sessions (Codex) **only**.
  Do NOT build features on this branch. Flow: copy code → profile/optimize → cherry-pick improvements back to main.
  Profiling tooling (PROFILING.md, scripts/) stays on the branch and does not belong on main.

## Feature Status

| Feature | Status |
|---------|--------|
| Drag to move | DONE |
| Double-click to create | DONE |
| Drag to resize (top + bottom edge) | DONE |
| Alt+drag to clone | NEXT |
| Modifier-click manual entry | DEFERRED |
| Multi-day tasks | DEFERRED |
| Recurring tasks | DEFERRED |

---

## Rough Edges & Known Issues

### 🔴 High

**1. JWT secret hardcoded**
`auth.rs`: `const JWT_SECRET: &[u8] = b"your-secret-key-change-in-production";`
Needs env var before any non-dev deployment. WS also has no token validation yet.

### 🟡 Medium

**2. Event log unbounded**
`World::log` is a Vec that grows forever. Fine for years at 20-user ops scale, but worth capping (e.g. trim on restart, or cap at N entries).

**3. Synchronous flush under write lock**
Every mutation flushes to disk sync. Blocks all other clients during I/O. Works at 20 users. Future: async flush with buffering if latency becomes noticeable.

**4. No wire protocol versioning**
No magic byte or version field. Old clients break silently on any field change. Low risk for single-team internal tool, but worth a protocol version byte in the snapshot header.

**5. No conflict resolution**
Last write wins. Two clients moving the same task simultaneously: last one wins. Fine for internal ops, but optimistic locking (revision check) is an option later.

**6. Snapshot size at scale**
Full World on every connect. ~2MB for 10k tasks. Fine now. Future: delta sync or lazy load.

### 🟢 Low

**7. UUID lookup is O(N)** — `_findByUuid()` linear scan. Fine for < 1000 entities.

**8. Title silently truncated at 128 UTF-8 bytes** — could warn or reject on create.

**9. Service color mapping incomplete** — priority drives color, but service_id would be a more meaningful visual axis for multi-service ops teams.

**10. No keyboard navigation** — mouse/touch only. Keyboard shortcuts would improve ops UX.

---

## Architecture Assessment

| Aspect | Rating | Notes |
|--------|--------|-------|
| Architecture | A+ | Clean separation of concerns throughout |
| Testing | A | 33 unit tests, state machine + wire coverage |
| Error handling | B+ | Result/enum throughout, no hot-path panics |
| Performance | A | Validated at CloudPC ceiling, zero hot-path allocs |
| Documentation | A | DESIGN.md, INTERACTIONS.md, handover.md |
| Tech debt | B | JWT secret, event log, sync flush — all manageable |

---

## Discussion Notes (2026-02-11)

Key design questions worth keeping in mind as the UI deepens:

**Service vs priority as visual axis:**
Right now priority drives task color (blue/teal/amber). For an ops team, *which service* is probably more useful at a glance than *how urgent*. Worth revisiting before the palette gets baked in.

**Alt+drag clone UX:**
The main open question is what title the clone gets. Same as original (cleanest) or "Copy of X"? Recommendation: same title, no prefix. Ops teams clone to create parallel tasks, not to annotate copies.

**Auth timing:**
WS has no token validation. For CloudPC behind VPN that may be fine indefinitely. Worth being intentional about *when* this gets closed rather than leaving it as an implicit TODO.

**postcard → rkyv:**
Leave deferred. postcard is fine until serialization shows up in profiling. At 20 users it never will.
