# txxt — Design Document

Updated 2026-02-18. This is the source of truth for architectural decisions.

---

## The Axiom

> **Everything is possible but nothing is real until the click.**

This is the load-bearing sentence. Every architectural decision in this project
derives from it. If you are about to write code that creates a DOM element
before the user clicks, stop. If you are about to repaint a canvas on mouse
movement, stop. Read this section again.

The browser's job is exactly two things:
1. Display one flat image.
2. Track the mouse.

That is all. The browser is a terminal. It does not render. It does not decide.
It does not own state. It forwards raw events and shows what it is told to show.

---

## Topology

```
CloudPC (one per user, no GPU, software-rendered VDI)
┌─────────────────────────────────────────────────────┐
│  IRONCLAD (browser)                                  │
│  — displays one flat background image               │
│  — tracks mouse, forwards raw events                │
│  — materializes exactly one div on click            │
│  — dematerializes it on mouseup                     │
│         ↕ WebSocket (localhost or fast LAN)          │
│  Rust sidecar (one binary per CloudPC)              │
│  — owns world state in memory                       │
│  — renders world to image (software 2D)             │
│  — runs spatial query, maintains candidate list      │
│  — knows what is under the cursor at all times      │
│  — on click: confirms bounds, triggers div          │
│  — on drop: mutates world, re-renders, sends image  │
└─────────────────────────────────────────────────────┘
         ↕ WebSocket (WAN)
EC2 central server (thin relay, one instance)
— receives state mutations ("user X moved task Y")
— broadcasts to all other sidecars
— persists canonical world state (redb)
— resyncs sidecars that were offline or crashed
```

The sidecar is the brain. The EC2 is the coordinator. The browser is the glass.

**Why one sidecar per CloudPC?**
CloudPCs have no GPU. All rendering is software on the local CPU. Hit testing,
spatial queries, image generation — all of this must happen locally to be fast.
A round trip to EC2 for hit testing would add 20-100ms of latency on every
click. Unacceptable. The sidecar runs on the same machine as the browser, or
on the local LAN with sub-millisecond latency.

**Why a central EC2?**
Multiple users work the same schedule simultaneously. When user A moves a task,
users B through F must see it. The EC2 is the single point of broadcast. It
does not do business logic. It receives mutations and fans them out.

---

## The Interaction Loop

The F1 pit stop model. Everything is pre-positioned. The stop itself is
instantaneous.

```
WORLD STATE CHANGES (task moved, created, deleted)
  Sidecar mutates world state
  Sidecar re-renders world to image
  Sidecar sends image over WebSocket
  Browser sets image as CSS background
  Browser shows updated world
  → zero DOM elements. flat image. done.

CURSOR MOVES (continuous, between interactions)
  Browser forwards cursor coordinates to sidecar
  Sidecar runs spatial query against world state
  Sidecar updates candidate list: [task A at (x,y,w,h), task B at ...]
  Sidecar sends lightweight cursor effect hint to browser
  Browser draws cheap local glow at cursor position (canvas overlay, tiny)
  Candidate list is ready. Pit crew in position.

USER CLICKS (the moment of materialization)
  Browser reports click coordinates to sidecar
  Sidecar checks candidate list — answer already known, no computation needed
  Sidecar responds: "task X, bounds (x, y, w, h)"
  Browser materializes exactly ONE div at exactly those bounds
  Div appears to "lift" the task out of the background image
  User interacts with the div (drag, resize, type)

MOUSEUP (dematerialization)
  Div reports final position to sidecar
  Sidecar validates, mutates world state
  Sidecar re-renders world to image
  Sidecar sends new image
  Browser swaps background image
  Browser dematerializes div
  → zero DOM elements. flat image. pit crew resets.
```

The background image never changes during an interaction. It is frozen. The
single div moves above a static backdrop. The VDI codec sees one small moving
rectangle and a completely static background — near-zero bandwidth.

---

## Input Mode State Machine

The sidecar tracks user input mode. Mode gates what computation runs.

```
IDLE
  Spatial queries: throttled (20Hz — humans react at 100-200ms, 60Hz is waste)
  Canvas overlay: cursor glow only
  Background: static

HOVER (cursor near candidates)
  Spatial queries: running, candidate list fresh
  Canvas overlay: cursor glow, candidate highlight if desired
  Background: static
  Pit crew: ready

DRAG (mousedown on materialized div)
  Spatial queries: SUSPENDED (candidate already known)
  Background: FROZEN (no repaints)
  One div: moving via CSS transform
  Canvas overlay: snap guides if desired, nothing else

RESIZE (mousedown on edge of materialized div)
  Same as DRAG

TYPING (focus in a text input)
  Spatial queries: SUSPENDED
  Background: static
  No div materialization possible

MONTH_VIEW (Alt-M)
  Spatial queries: scoped to day-cell hit testing only (simple math, no entity search)
  Background: month image from sidecar
  No task div materialization (read-only)

COLLAPSED (Alt-C)
  Everything suspended
  Calendar not visible, not computing
```

Modes are not guessed — they are explicit signals. A keydown starts TYPING.
A mousedown on a div starts DRAG. Alt-M switches to MONTH_VIEW. The sidecar
receives these mode signals and gates computation accordingly.

---

## Why This Matters for VDI

VDI codecs (Citrix, RDS, VMware) transmit changed pixels over the network.
A full canvas repaint every frame = entire screen changed = bandwidth spike =
codec struggles = visual artifacts on an already-marginal connection.

The background image approach solves this at the application layer:

- Static background = zero codec work between interactions
- One moving div = one small dirty region (the div's bounding box)
- Background swap on world change = one deliberate full-frame update at human
  speed, not 60 chaotic partial updates per second

This stacks with whatever dirty-region optimization the VDI codec itself does.
The result: the system looks fast on connections where a standard web app
would visibly lag.

The cursor glow overlay (small canvas element) changes every frame but covers
a tiny area. VDI codecs handle small dirty regions efficiently.

---

## What IRONCLAD Does (and Does Not Do)

**Does:**
- Open WebSocket to local sidecar
- Receive background image → set as CSS `background-image` (blob URL)
- Track mouse → send cursor coordinates to sidecar at 20Hz
- Receive candidate list from sidecar → cache it
- On click → send coordinates to sidecar → receive bounds → materialize one div
- Drag div → report position updates to sidecar
- On mouseup → report final position → dematerialize div
- Draw cursor glow (tiny canvas overlay, local, no sidecar involvement)
- Handle keybinds → send mode signals to sidecar

**Does not:**
- Render tasks, grid lines, text, or any world state (sidecar does this)
- Do hit testing (sidecar does this)
- Own any world state
- Create DOM elements except the one active interaction div
- Run spatial queries
- Repaint during drag

**IRONCLAD's data model (minimal — just enough for div management):**
```
uuids[]       — task UUIDs (to pack commands back to sidecar)
xs[], ys[]    — candidate positions (for div placement)
ws[], hs[]    — candidate sizes
```

No labels. No priorities. No colors. No service IDs. The browser does not
need rendering data because it does not render.

**Current implementation state:**
IRONCLAD currently does canvas rendering (world.rs entities → canvas draw calls).
This was built iteratively and diverged from the intended architecture. The
canvas code is a correct proof-of-concept but is slated for replacement by
the sidecar image pipeline. The wire protocol and SoA structure are keepers;
the rendering code is not.

---

## What the Sidecar Does

The sidecar is the Rust binary that runs locally on each CloudPC. It is the
authoritative local state machine and the renderer.

**Responsibilities:**
- Boot: load world state from redb (or sync from EC2 if offline)
- Accept WebSocket connection from local IRONCLAD
- Maintain full world state in memory (HashMap<Uuid, Task/User/Service>)
- Track cursor coordinates (received from browser at 20Hz)
- Run spatial queries → maintain candidate list
- Send candidate list updates to browser
- On click: confirm which task/element, send bounds to browser
- On drop: validate position, apply mutation, increment revision
- Re-render world to image after any state change → send to browser
- Relay mutations to EC2 → receive broadcasts from EC2 → apply deltas
- Persist world state to local redb (fast local writes)

**Rendering:**
The sidecar renders the world to a flat image using a software 2D library
(no GPU required). The image is the authoritative visual representation.
What the sidecar renders is exactly what the user sees. No client-side
rendering divergence is possible.

Rendering triggers: task created, task moved, task deleted, task completed,
week navigation, view mode change. Never on cursor movement.

---

## What the EC2 Does

The EC2 is deliberately thin. It does not run business logic.

**Responsibilities:**
- Accept WebSocket connections from all sidecars
- Receive state mutations: `[user_id][event_type][payload]`
- Broadcast mutations to all other connected sidecars
- Persist canonical world state to redb
- On sidecar reconnect: send full snapshot or event log since last_seen_rev

The EC2 is a stateful relay, not a game server. The sidecars do the work.
The EC2 coordinates and persists.

---

## Data Model

### Task — the unit of work

```rust
struct Task {
    id: Uuid,
    title: String,
    status: TaskStatus,      // Staged | Scheduled | Active | Completed
    priority: Priority,      // Low | Medium | High | Urgent
    service_id: Uuid,
    created_by: Uuid,
    assigned_to: Option<Uuid>,
    date: Option<u16>,       // epoch days since 1970-01-01 (None = Staged)
    start_time: Option<u16>, // minutes from midnight, 15-min grid (None = Staged)
    duration: Option<u16>,   // minutes, 15-min grid (None = Staged)
}
```

Day-of-week derived from epoch days: `(date + 3) % 7` → 0=Mon .. 6=Sun.
The sidecar owns time. Browser never computes week boundaries.

### Service — who pays for the time

```rust
struct Service { id: Uuid, name: String }
```

12 default services. Metadata TBD when real data sources arrive.

### User — a player

```rust
struct User { id: Uuid, username: String, password_hash: String }
```

---

## Wire Protocol — WebSocket Binary

All game data over WebSocket uses fixed-stride packed binary, readable by
DataView at known offsets. JSON is never used in the data path.

### Task record (192 bytes, fixed stride)

```
[0..16]    id (UUID, 16 bytes)
[16]       status (u8: 0=Staged, 1=Scheduled, 2=Active, 3=Completed)
[17]       priority (u8: 0=Low, 1=Medium, 2=High, 3=Urgent)
[18..20]   date (u16 LE, epoch days since 1970-01-01, 0xFFFF = not scheduled)
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
- `0x03` TaskScheduled: `[type][rev:u64][task_id:16][date:u16][start:u16][dur:u16]`
- `0x04` TaskMoved: same layout as TaskScheduled
- `0x05` TaskUnscheduled: `[type][rev:u64][task_id:16]`
- `0x06` TaskCompleted: `[type][rev:u64][task_id:16]`
- `0x07` TaskDeleted: `[type][rev:u64][task_id:16]`
- `0x10` BackgroundImage: `[type][rev:u64][format:u8][width:u16][height:u16][bytes...]`
- `0x11` CandidateList: `[type][count:u8][[task_id:16][x:u16][y:u16][w:u16][h:u16]...]`

### Client → Server commands

- `0x10` CreateTask: `[type][priority:u8][service_id:16][assigned_to:16][date:u16][start:u16][dur:u16][title:UTF-8...]`
- `0x11` ScheduleTask: `[type][task_id:16][date:u16][start:u16][dur:u16]`
- `0x12` MoveTask: same layout as ScheduleTask
- `0x13` UnscheduleTask: `[type][task_id:16]`
- `0x14` CompleteTask: `[type][task_id:16]`
- `0x15` DeleteTask: `[type][task_id:16]`
- `0x20` CursorMove: `[type][x:u16][y:u16]` (sent at 20Hz)
- `0x21` ModeChange: `[type][mode:u8]` (IDLE=0, DRAG=1, TYPING=2, MONTH=3, COLLAPSED=4)
- `0x22` ClickAt: `[type][x:u16][y:u16]`

### Sync semantics

Every event carries a revision number (u64 LE at offset 1). Client tracks
last_seen_rev. On reconnect: client sends last_seen_rev, sidecar sends deltas
since then (or full snapshot if gap too large).

---

## Server Internals — World State Machine

The sidecar's core is a pure state machine with zero I/O.

```rust
struct World {
    tasks: HashMap<Uuid, Task>,
    users: HashMap<Uuid, User>,
    services: HashMap<Uuid, Service>,
    revision: u64,
    connections: HashMap<ConnectionId, PlayerSession>,
}

impl World {
    fn apply(&mut self, cmd: Command, user_id: Uuid) -> Result<Event, WorldError>
}
```

Every mutation: validate → apply to memory → increment revision → return Event.
The Event is what gets rendered and broadcast.

**Persistence:** redb is a save file. Loaded on boot, flushed on mutation.
Never queried at runtime.

---

## Key Decisions

### No JSON in the data path
JSON only at the auth boundary (login). Everything else is binary packed structs.

### redb stays
Pure Rust, single-file, ACID. Treated as a save file. Load once, flush on
mutation, never query at runtime.

### One div at a time
The browser never has more than one interactive DOM element at any moment.
This is an architectural constraint, not a guideline. Zero divs at rest.

### Sidecar renders, browser displays
The browser does not compute pixel positions from task data. The sidecar
renders the world to an image and sends it. Rendering logic lives in exactly
one place.

### Browser never does hit testing
The browser forwards click coordinates. The sidecar holds the candidate list
and responds with bounds. No client-side spatial queries.

### 20Hz cursor forwarding
Human reaction time is 100-200ms. 20Hz (50ms intervals) is sufficient for
responsive hit detection. Forwarding at 60Hz burns CPU and bandwidth for no
perceptible gain.

### No protocol versioning — intentional
Server and clients are always deployed together from the same repo. No window
where mismatched versions talk to each other. Wire format changes are breaking
changes by design — update everything and redeploy.

### No CRDT
Last-write-wins. 5-20 users. Two people move the same task simultaneously?
Server processes in order, second write wins. CRDT is massive over-engineering
for this scale.

### No cursor/presence sharing (yet)
Mouse positions at 20Hz × 20 users = 400 messages/sec to EC2. Value is real
for a collaborative scheduling tool but the architecture for it (unreliable
broadcast channel, ghost cursors rendered into image) is a future phase.

### Security deferred
Dev-mode auth bypass active. Hardcoded JWT secret. Internal tool for a known
workgroup on a VPN. Hardening is a future phase.

### No protocol versioning
No version bytes in wire format. Intentional. See above.

---

## Interaction Gestures

All scheduling happens through direct manipulation. No forms. No modals.
The world IS the interface.

| Gesture           | Action               | Status       |
|-------------------|----------------------|--------------|
| Click task        | Materialize, select  | IN PROGRESS  |
| Drag task         | Move to slot         | DONE (canvas era, to port) |
| Double-click grid | Create 30m task      | DONE (canvas era, to port) |
| Drag bottom edge  | Resize duration      | DONE (canvas era, to port) |
| Alt+drag          | Clone task           | DONE (canvas era, to port) |
| Alt-C             | Collapse calendar    | DONE         |
| Alt-M             | Monthly view         | DONE (read-only) |
| Modifier-click    | Manual entry         | DEFERRED     |

"Canvas era" = implemented in current IRONCLAD canvas code, to be ported
to the div-materialization model once the sidecar image pipeline is built.

---

## Keybinds

Keybinds receive equal design priority to mouse gestures. Any action
reachable by mouse must have a keyboard path. Alt-X bindings are
view-independent: wherever you are, Alt-X does its thing.

See KEYBINDS.md for the full active and candidate list.

---

## What's Archived

`archive/` contains the Clay/WASM era: dead frontend experiment, old handover
docs, screenshots, scratch files. Kept for reference.

The current `frontend/ironclad.js` (canvas rendering era) is living code but
represents the intermediate step before the sidecar image pipeline. The wire
protocol, SoA structure, and input handling are keepers. The canvas drawing
code is not the final form.

---

## Philosophy

- **Everything is possible but nothing is real until the click.**
- The browser is glass. The sidecar is the brain.
- Static image between interactions. One div during interaction. Nothing else.
- Performance is a feature. On a CloudPC, performance is survival.
- VDI bandwidth is precious. Don't move pixels you don't have to.
- Keyboard-first. Mouse works too.
- Services are the primary axis of meaning ("who pays for the time").
- No speculative features. Build what ops teams need, not what might be needed.
- Security later. Correctness and speed now.
- Binary everything. JSON only at auth boundary.
