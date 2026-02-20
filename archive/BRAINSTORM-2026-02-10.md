# Brainstorm Session — 2026-02-10

Captured at the end of a long session. This file is for fresh-Opus to pick up
where tired-Opus left off. Read this AFTER reading DESIGN.md.

## Context

The backend is being restructured from a webdev REST+WS CRUD server into a
game-server-style stateful WebSocket server. DESIGN.md has the full
architecture. This file captures the brainstorming about what to build and
how, plus honest pushback on ideas that don't work.

## The revision counter is more powerful than it looks

Every mutation gets a monotonic revision number. This gives us:

### Undo/redo (nearly free)
- Log `(rev, command, inverse_command)` on each mutation
- Undo = apply the inverse command
- For a drag-and-drop calendar where people accidentally drop tasks in the
  wrong slot, this is genuinely useful
- The inverse of MoveTask is just MoveTask back to the original position
- The inverse of CreateTask is DeleteTask, etc.

### Audit trail (killer feature for ops)
- "Who moved what and when?" is a constant ops question
- The revision log IS the audit trail — no separate logging system
- Every mutation is a timestamped, attributed event
- Walk the log backward to answer "why was this rescheduled?"
- This is a real feature for the user's environment, not speculative

### Reconnect as log replay
- Client sends last_seen_rev, server replays log entries since then
- Much simpler than computing a diff between two states
- If client is too far behind, send a full snapshot instead

### Log size is trivial
- 5-20 users × ~100 mutations/day = maybe 5000 entries/week
- 5000 × ~100 bytes = 500KB in memory
- Keep a week in memory, flush older entries to redb for archival
- Or just keep growing — even a year's log is ~25MB

## What NOT to build

### No tick loop
The server is event-driven, not tick-driven. A scheduling app has no physics.
99% of the time nothing happens. A tick loop would just burn CPU.

The one exception: auto-promotion. Tasks scheduled for 14:00 should become
"Active" at 14:00. That's a `tokio::time::interval` checking once per minute,
not a game loop.

### No view-based subscriptions (premature)
Tempting to filter deltas per client based on what they're viewing. But:
- Full snapshot of 200 tasks at 192 bytes = ~38KB. Negligible.
- A delta (task moved) is ~32 bytes.
- Broadcasting everything to everyone is free at this scale.
- Filtering adds per-client codepaths and state tracking for zero benefit.
Just broadcast everything. Revisit at 10,000 tasks or 100 users.

### No CRDT
Last-write-wins. 5-20 users. Two people drag the same task simultaneously?
Server processes in order, second write wins, both clients see the result.
CRDT is massive over-engineering for this scale.

### No event sourcing
The revision log gives 90% of the benefits without the architectural
complexity. Keep current state + recent history. Don't build an event store.

### No cursor/presence sharing
Mouse positions at 60hz × 20 users = 1200 messages/sec. Value is marginal
for a scheduling tool. If ever wanted, it's a separate unreliable channel.

### No batching into ticks
Process each command immediately. Lower latency, simpler code. No physics
that need deterministic stepping.

### No server-side rendering
IRONCLAD is the renderer. Server sends data, not pixels.

## Staging queue — server-owned computation

Tasks without a time slot live in staging, sorted by urgency:
- Sort by: priority descending, then deadline ascending
- Re-sort on every mutation that touches a staged task
- Push the sorted list to clients

This is a server responsibility because other users' actions affect sort
order (someone escalates a task → it jumps up in YOUR staging list).

Don't build a priority queue data structure. Just re-sort the Vec. Sorting
200 tasks by two fields is microseconds.

## Binary wire format — practical approach

### Fixed-size records for snapshots

Task wire format (example, ~192 bytes with title):
```
[0..16]    uuid (16 bytes)
[16]       status (u8: 0=Staged, 1=Scheduled, 2=Active, 3=Completed)
[17]       priority (u8: 0=Low, 1=Medium, 2=High, 3=Urgent)
[18..19]   day (u16, 0-6 Mon-Sun, 0xFFFF = not scheduled / staged)
[20..21]   start_time (u16, minutes from midnight, 15-min snap)
[22..23]   duration (u16, minutes, 15-min snap)
[24..40]   service_id (16 bytes uuid)
[40..56]   assigned_to (16 bytes uuid, zeroed if unassigned)
[56..184]  title (128 bytes, UTF-8, zero-padded)
[184..192] reserved
```

200 tasks × 192 bytes = ~38KB snapshot. Sent once on connect. Fine.

### Deltas are tiny
"Task X moved to day 3, 14:00" → task_id(16) + day(2) + start(2) + dur(2)
+ rev(8) + msg_type(1) = ~31 bytes. Plus header.

### String handling options
a. Fixed buffers (title[128]) — simple, wasteful, easy for JS DataView
b. String table after record array — clever, adds complexity
c. Send title only on create/update, not on every snapshot

Option (a) is right for this scale. 128 bytes × 200 tasks = 25KB of "waste"
that nobody will ever notice.

### JS reading
```javascript
const view = new DataView(buffer);
const taskOffset = HEADER_SIZE + (i * TASK_STRIDE);
const status = view.getUint8(taskOffset + 16);
const day = view.getUint16(taskOffset + 18, true); // little-endian
// etc.
```

Exactly like the old PackRenderCommands in the Clay frontend, but for data.

## The World struct — the center of everything

### Pure state machine, testable without IO
```rust
let mut world = World::new();
world.create_task(...);
world.move_task(task_id, day: 3, start: 840);
assert_eq!(world.tasks[&task_id].day, 3);
assert_eq!(world.revision, 2);
```

No HTTP server. No database. No async. Pure synchronous state machine.
This is a massive testing advantage over the current REST architecture.

### Self-serializing
The World can pack itself into a binary snapshot. Same codepath for:
- Sending snapshot to a new client
- Writing a backup
- Populating a test fixture
- Diffing two states

### Command handlers as methods
```rust
impl World {
    fn apply(&mut self, cmd: Command, user_id: Uuid) -> Result<Event, Error> {
        match cmd {
            Command::MoveTask { task_id, day, start_time, duration } => {
                // validate: task exists, user has permission, no conflict
                // mutate: update task position
                // increment revision
                // return: Event::TaskMoved { ... }
            }
            // ...
        }
    }
}
```

The Event returned is exactly what gets broadcast to clients.

## Implementation plan (for fresh-Opus)

### Phase 1: The World (pure Rust, no IO)
1. Define the World struct with HashMap<Uuid, Task/User/Service>
2. Define Command enum (MoveTask, CreateTask, UpdateTask, DeleteTask)
3. Define Event enum (TaskMoved, TaskCreated, TaskUpdated, TaskDeleted, Snapshot)
4. Implement World::apply(cmd) → Event
5. Implement World::snapshot() → binary blob
6. Write tests: create tasks, move them, verify state

### Phase 2: Persistence (redb as save file)
1. World::load(db: &Db) → World (boot from redb)
2. World::flush(db: &Db, event: &Event) (write-through on mutation)
3. Update db.rs for new Task model (scheduling fields, new status enum)
4. Delete old REST-specific code from api.rs (or gut it entirely)

### Phase 3: Wire it up (axum + WebSocket)
1. WS handler: on connect → send snapshot
2. WS handler: on binary message → deserialize Command → World::apply → broadcast Event
3. Shared World state: Arc<RwLock<World>> or similar
4. Keep REST only for POST /api/auth/login
5. Static file serving points at IRONCLAD (../txxt2 or copied in)

### Phase 4: Connect IRONCLAD
1. Add WebSocket client to ironclad.js
2. On connect: receive binary snapshot → populate SoA arrays
3. On delta: apply incremental update to SoA arrays
4. On drag-drop: send binary MoveTask command over WS
5. Remove Math.random() entity generation

### What to keep from current backend
- Cargo.toml (deps are clean)
- db.rs (postcard storage, table definitions — modify for new model)
- auth.rs (login handler, JWT, middleware — keep for now)
- main.rs (axum setup, router — modify for new routes)

### What to throw away
- api.rs — REST CRUD endpoints. Replaced by WS command handlers.
- The REST task/user/service routes in main.rs router.
- models.rs — mostly replaced. Keep LoginRequest/LoginResponse for auth.
  Everything else gets rewritten around the new Task model and Command/Event enums.
- ws.rs — current implementation is a JSON broadcast skeleton.
  Replaced by binary command/event handler.

## Open questions (for next session to decide)

1. **Arc<RwLock<World>> vs actor model** — RwLock is simpler but means
   write contention on every mutation. An actor (mpsc channel → single
   owner task) avoids locking. For 5-20 users, either works. RwLock is
   probably fine and simpler.

2. **Sync vs async flush to redb** — write-through (sync) is simpler and
   guarantees durability. Async flush risks losing the last few mutations
   on crash. At this write volume (<100/day), sync is fine.

3. **Where does the revision log live?** — In World (Vec<(rev, Event)>)?
   In a separate struct? Flushed to redb? Kept only in memory?

4. **When to define the exact binary wire format?** — Could use postcard
   for the wire too (serde, variable-length, not fixed-stride). Or hand-roll
   fixed-stride for DataView simplicity. Postcard is faster to implement;
   fixed-stride is better for JS. Probably fixed-stride for snapshot,
   postcard for commands (which are smaller and less frequent).

5. **New Task model fields** — the design says day/start_time/duration.
   Should start_time be minutes-from-midnight (u16) or an actual time type?
   Minutes-from-midnight is simplest and maps directly to IRONCLAD's grid
   (which is already in pixel offsets from CONFIG.TOP_HEADER).
