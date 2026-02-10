# txxt — Design Document

Updated 2026-02-10. This is the source of truth for architectural decisions.

## Profiling companion

For performance-lab context, instrumentation rationale, and profiling workflow,
see `backend/PROFILING.md` (branch workflow: `oxygen/profiling-lab`).

## What this is

A Small Multiplayer Online (SMO) scheduling portal for 5-20 ops users on
enterprise CloudPCs (software-rendered VDI, no GPU). Not a web app. Not
Google Calendar + Tasks. Think game server + thin client renderer.

The goal: stop wasting time filling in Sharepoint text fields. Make scheduling
and time tracking feel like a Bloomberg terminal — always-on, stateful,
information-dense, built for people who use it 8 hours a day.

## Mental model: game server, not web API

This is NOT a REST API with a database and a frontend that makes fetch calls.
This IS a stateful server that:

- Boots, loads the world into memory
- Accepts player connections (WebSocket)
- Receives inputs (commands), validates and applies them to the world state
- Broadcasts state deltas to all connected players
- Persists the world to disk (redb) as a save file, not a query engine

The browser is a thin renderer. It sends inputs, receives state, draws pixels.
It does not own state. It does not make decisions. It does not have a "model
layer." IRONCLAD is the GPU; the Rust server is the CPU.

## Two repos, one product

### txxt2 (IRONCLAD) — the renderer

Canvas/DOM hybrid. Validated on CloudPC at 32fps (VDI ceiling) with 5000
entities. Zero dependencies, zero build step.

- Canvas layer: static grid, passive entity rendering, DPR-aware
- DOM pool: 15 recycled divs, "flashlight" hydrated near cursor (SDF)
- SoA typed arrays, spatial bucketing, frame-stamp dedup
- Drag-and-drop with 15-minute grid snap
- Currently generates random test data — no server connection yet

### txxt (this repo) — the game server

Rust/axum single binary. Authoritative state machine. Owns the world.

## Architecture

```
Enterprise systems (ServiceNow, etc.)
         | Nightly ETL (future)
         v
   txxt server (Rust, single binary)
     - In-memory world state (the runtime truth)
     - redb (save file, loaded on boot, flushed on mutation)
     - WebSocket: THE data protocol (binary frames)
     - REST: auth only (login, maybe health)
         |
         | Binary frames (packed structs, DataView on client)
         v
   IRONCLAD (browser)
     - Sends: player commands (move_task, create_task, etc.)
     - Receives: snapshots + deltas
     - Renders: Canvas + DOM pool
     - Owns NOTHING except pixels and input events
```

## Server internals — pseudo-ECS

The server thinks in entities and components, not ORM objects.

### World state (in-memory, authoritative)

```rust
struct World {
    // Entity storage — HashMap for now, SoA later if needed
    tasks: HashMap<Uuid, Task>,
    users: HashMap<Uuid, User>,
    services: HashMap<Uuid, Service>,

    // Monotonic revision counter — every mutation increments this
    revision: u64,

    // Connected players
    connections: HashMap<ConnectionId, PlayerSession>,
}

struct PlayerSession {
    user_id: Uuid,
    last_seen_rev: u64,
    // What view they're looking at (which week, which service filter)
    // So we can send targeted deltas later if needed
}
```

### Boot sequence

1. Open redb, load all entities into World
2. Bind port, start accepting connections
3. Ready (no cold queries ever — everything served from memory)

### Mutation flow (the hot path)

```
Client sends binary command over WS
  → Server deserializes command
  → Server validates against World state
    (conflict detection, permission checks, business rules)
  → Server applies mutation to World (memory)
  → Server increments revision
  → Server flushes mutation to redb (async or sync — TBD)
  → Server packs binary delta
  → Server broadcasts delta to all connected clients
```

One codepath. One protocol. One serialization format.

### Persistence

redb is a save file. The server does NOT query redb at runtime.

- Boot: load everything from redb into World
- Mutation: write-through to redb after applying to memory
- Crash recovery: reboot, reload from redb (ACID guarantees)
- redb transactions are cheap for single writes at this scale

## Protocol — WebSocket binary (implemented)

All data over WebSocket uses fixed-stride packed binary, readable by JS
DataView at known offsets. See `backend/src/wire.rs` for the authoritative
byte layout. JSON is never used in the data path.

### Task record (192 bytes, fixed stride)

```
[0..16]    id (UUID, 16 bytes)
[16]       status (u8: 0=Staged, 1=Scheduled, 2=Active, 3=Completed)
[17]       priority (u8: 0=Low, 1=Medium, 2=High, 3=Urgent)
[18]       day (u8: 0-6 Mon-Sun, 0xFF = not scheduled)
[19]       _pad
[20..22]   start_time (u16 LE, minutes from midnight, 15-min grid)
[22..24]   duration (u16 LE, minutes, 15-min grid)
[24..40]   service_id (UUID, 16 bytes)
[40..56]   assigned_to (UUID, 16 bytes, zeroed = unassigned)
[56..184]  title (128 bytes, UTF-8, zero-padded)
[184..192] _reserved
```

### Service record (80 bytes, fixed stride)

```
[0..16]    id (UUID, 16 bytes)
[16..80]   name (64 bytes, UTF-8, zero-padded)
```

### Server → Client messages

First byte is message type:
- `0x01` Snapshot: `[type][rev:u64][task_count:u32][svc_count:u32][tasks...][services...]`
- `0x02` TaskCreated: `[type][rev:u64][task_record:192]`
- `0x03` TaskScheduled: `[type][rev:u64][task_id:16][day:u8][start:u16][dur:u16]`
- `0x04` TaskMoved: same layout as TaskScheduled
- `0x05` TaskUnscheduled: `[type][rev:u64][task_id:16]`
- `0x06` TaskCompleted: `[type][rev:u64][task_id:16]`
- `0x07` TaskDeleted: `[type][rev:u64][task_id:16]`

### Client → Server commands

- `0x10` CreateTask: `[type][priority:u8][service_id:16][assigned_to:16][title:UTF-8...]`
- `0x11` ScheduleTask: `[type][task_id:16][day:u8][start:u16][dur:u16]`
- `0x12` MoveTask: same layout as ScheduleTask
- `0x13` UnscheduleTask: `[type][task_id:16]`
- `0x14` CompleteTask: `[type][task_id:16]`
- `0x15` DeleteTask: `[type][task_id:16]`

### JS reading example

```javascript
const view = new DataView(buffer);
// In a snapshot, task records start at offset 17
const TASK_STRIDE = 192;
const taskOffset = 17 + (i * TASK_STRIDE);
const status    = view.getUint8(taskOffset + 16);
const priority  = view.getUint8(taskOffset + 17);
const day       = view.getUint8(taskOffset + 18);
const startTime = view.getUint16(taskOffset + 20, true);  // LE
const duration  = view.getUint16(taskOffset + 22, true);  // LE
```

### Sync semantics

- Every event carries a revision number (u64 LE at offset 1)
- Client tracks last_seen_rev
- On reconnect: client sends last_seen_rev, server sends deltas since then
  (or full snapshot if gap is too large)
- Server arbitrates conflicts (last write wins for now, smarter later)

## Data model — minimal until real data arrives

The model stays thin until we connect to real enterprise data sources
(ServiceNow, custom APIs). Only what IRONCLAD needs to render:

### Task (the unit of work)

Core identity:
- id, title, service_id, created_by

Scheduling (what IRONCLAD renders on the grid):
- day (0-6, Mon-Sun)
- start_time (minutes from midnight, snapped to 15-min grid)
- duration (minutes, snapped to 15-min grid)
- assigned_to (who owns this time slot)

State:
- status: Staged | Scheduled | Active | Completed
  (Staged = no time slot. Scheduled = has a slot. Active = being worked now.)
- priority: Low | Medium | High | Urgent (drives staging auto-sort)

### Service (who pays for the time)

- id, name
- Metadata TBD when real data sources arrive

### User (a player)

- id, username, password_hash

Everything else (category, tags, description, due_date) is deferred until
real data tells us what it should be.

## Key decisions

### No JSON in the data path

- **Storage**: postcard (binary, serde-compatible). Interim until rkyv
  (zero-copy) when we optimize the in-memory serve layer.
- **Wire (WebSocket)**: hand-rolled packed binary. DataView on client.
- **REST (auth only)**: JSON is fine. Called once per session.

### redb stays

Pure Rust, single-file, ACID. Treated as a save file, not a query engine.
Loaded on boot, flushed on mutation.

### IRONCLAD is the renderer, period

The server decides what exists and where. IRONCLAD draws it.

### Security is deferred

Dev-mode auth bypass active on WS. Login token behavior is intentionally
lightweight PoC mode. Do not deploy publicly.
Hardening is a future phase after the system works.

Current explicit defer list for dedicated hardening sessions:
- Enforced WebSocket auth/authz
- Flush-failure escalation policy (beyond logging/telemetry)

## Current backend state (updated 2026-02-10)

The game server architecture is implemented. The old webdev-CRUD layer has
been removed. What exists now:

### Modules

- **world.rs** — Pure state machine. `World` struct with HashMap entity
  storage, `Command`/`Event` enums, `apply()` mutation codepath, revision
  counter, event log, staging queue. Zero IO, fully unit-tested (17 tests).

- **persist.rs** — `SaveFile` wrapper around redb. `load_world()` on boot,
  `flush()` on every mutation (sync write-through). Seeding helpers for
  default services/user. Tested with real redb files (4 tests).

- **game.rs** — WebSocket handler at `/api/game`. On connect: subscribes to
  broadcast, sends binary Snapshot, then enters command/event loop.
  Fixed-stride binary protocol for snapshots + events.

- **auth.rs** — Login handler at `POST /api/auth/login`. Reads users from
  World (not from a separate database). Lightweight token response for PoC.
  Dev-mode auth bypass on WS (no token required yet).

- **main.rs** — Boot sequence: open SaveFile → load World → seed defaults →
  create broadcast channel → start axum. ~90 lines.

### Removed
- `api.rs` — REST CRUD endpoints (replaced by WS commands)
- `ws.rs` — JSON WebSocket handler (replaced by binary game.rs)
- `db.rs` — Old database layer (replaced by persist.rs + World)
- `models.rs` — Old model types (replaced by world.rs types)

### Test coverage
21 unit tests covering: task lifecycle (create/schedule/move/unschedule/
complete/delete), validation (day/time/duration bounds, status transitions),
revision counter, event log, staging queue sorting, redb round-trips.

## Dependencies — audit in progress

Current runtime-focused set is intentionally slimmer than earlier PoC passes.
Core deps: axum, tokio, redb, serde, postcard, uuid, argon2, tower-http.

Profiling-only tooling is feature-gated (`profile`, `profile-console`) and
documented in `backend/PROFILING.md`.

**Needs a deep analysis session:**
- **tower-http** — currently used for static serving; evaluate custom handler
  only if profiling shows meaningful gain.
- **postcard** — persistence-only right now; rkyv remains a future option if
  it materially improves hot-path memory/serialization behavior.

## What's archived

`archive/` contains the Clay/WASM era: dead frontend, old handover docs,
screenshots, scratch files. Kept for reference.

## Philosophy

- This is a game server, not a web API.
- The server owns the world. Clients are renderers.
- No blocking UI. Docked panels, not modals.
- Performance is a feature. Zero allocations in hot paths.
- Binary everything. JSON only at auth boundary.
- Keyboard-first. Mouse works too.
- Services are the primary axis ("who pays for the time").
- Security later. Correctness and speed now.
- No speculative features. Build what's needed, not what might be.
