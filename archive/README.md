# Txx Tracker (txxt)

Task tracker built as a deliberate anti-framework demo: Clay immediate-mode UI in C compiled to WASM, rendered via Canvas2D, backed by a single Rust server binary.

This is intended to run on an enterprise CloudPC / remote desktop environment with no GPU acceleration. The goal is “crisp enough to feel native” without pulling in a traditional web stack.

## Philosophy (Read This First)

Most internal web tools fail in the same predictable ways: modal-driven flows, infinite spinners, and a pile of frameworks that make every interaction feel like a networked form submission.

This project is a deliberate rejection of that. It aims to feel like a professional terminal/tooling surface: always-on, stateful, fast, information-dense, and built for the people who use it daily.

### The mental model

Think “Bloomberg terminal delivered in a browser” (or in-engine dev tools), not “web app with pages.”

- The UI is a continuously running loop: input → state update → render.
- Data changes are continuous; the UI is never “reloaded” in the page-centric sense.
- The interface is designed for power users, not onboarding funnels.

### Non-negotiables

- No blocking windows or popups. Ever. (The one exception is the once-daily boot/login screen, which intentionally covers initial data hydration.)
- Create/edit/detail surfaces are docked panels, not modals.
- JS is a platform shim, not an app framework.
- Performance is a feature: no per-frame reallocations, minimal allocations in hot paths, stable ABIs across the JS↔WASM boundary.

### JS is a necessary evil

This project uses JS only because the browser forces our hand.

- **Keep JS small and dumb**: IO (HTTP/WS), input event plumbing, and Canvas2D pixels. That's it.
- **WASM owns the app**: UI state, layout, animation, and as much logic as practical.
- Avoid JS frameworks/libraries and avoid “JS as model layer.” If a feature can live in WASM, it should.

### Security is not a concern (yet)

Security is intentionally de-prioritized until the system is solid and ruthlessly performant.

- **This repo is insecure by design right now. Do not deploy it.**
- Auth contains dev bypasses; CORS is permissive; secrets are hard-coded.
- Treat this as a local/dev-only tool until we explicitly enter a hardening/testing phase.

### What “fast” means here

- Input is acknowledged immediately (UI responds on the next frame).
- Rendering is predictable under CPU-only Canvas2D.
- Data loads are pushed toward a brief, intentional boot phase; everything after that is incremental and snappy.

### Why the browser then?

Because it is deployable. The point is to ship a terminal-like tool without turning it into a full native app program.


## Architecture (Short)

- Backend is one axum server that serves:
  - Static frontend files from `frontend/dist/`
  - REST API under `/api/*`
  - WebSocket under `/api/ws`
- Frontend is:
  - `frontend/main.c`: app state + Clay layout → render command list
  - `frontend/dist/index.html`: minimal JS “platform shim” + Canvas2D renderer
  - DOM is used only where unavoidable (text input + modal form). Everything else is drawn.

The intended direction is: WASM owns UI state and animations; JS owns IO (HTTP/WS) and pixels.

## UI Paradigm

- Left rail is **Services/Applications** (the primary axis for “who pays for the time”).
- Click a service to select it and drive context.
- Double-click a service to open a new task pre-scoped to that service.
- Task details and task creation live in a **docked panel** in the lower third of the main area.
- Incoming updates are signaled deliberately (brief pulse). Even when the client reloads data, the signal is the feature, not a bug.

## Why redb (instead of sqlite)

redb is a pragmatic choice for this prototype:
- single-file embedded DB with a small dependency surface
- no external service, no schema migration ceremony
- fits the “one binary, one folder” deployment vibe

If/when we need full SQL features, concurrency behavior under contention, or ecosystem tooling, we can revisit.

## Running

Backend (serves frontend + API):

```bash
cd backend
cargo run
```

Open: `http://localhost:3000`

Default login: `admin` / `admin`

Frontend rebuild:

```bash
cd frontend
./build.sh
```

No backend restart is required after rebuilding WASM as long as the server is still running and serving `frontend/dist/app.wasm`.

## Perf / Debug

- Perf HUD: press `F2` to toggle.
- Clay debug tools (layout inspector): press `Ctrl+D` to toggle.
- The renderer is intentionally Canvas2D (software-friendly). Avoiding “canvas resize every frame” matters a lot on CloudPC.

## API

```
txxt/
├── frontend/
│   ├── main.c              # Clay UI implementation
│   ├── clay.h              # Clay library
│   ├── build.sh            # WASM compilation script
│   └── dist/
│       ├── index.html      # HTML + Canvas renderer + JS glue
│       └── app.wasm        # Compiled WASM (after build)
│
├── backend/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs         # Server entry point
│       ├── api.rs          # REST endpoints
│       ├── ws.rs           # WebSocket handler
│       ├── db.rs           # redb storage layer
│       ├── auth.rs         # Authentication
│       └── models.rs       # Data structures
│
└── shared/
    └── protocol.h          # Shared message format (if needed)
```

## Building

### Frontend
```bash
cd frontend
chmod +x build.sh
./build.sh
```

### Backend
```bash
cd backend
cargo build --release
./target/release/txxt-server
```

Server will run on http://localhost:3000

## API Endpoints

```
POST   /api/auth/login      # Login, returns JWT
POST   /api/auth/logout     # Logout
GET    /api/tasks           # List tasks (with filters)
POST   /api/tasks           # Create task
GET    /api/tasks/:id       # Get single task
PUT    /api/tasks/:id       # Update task
DELETE /api/tasks/:id       # Delete task
 GET    /api/users           # List users (for assignment dropdown)
 GET    /api/services        # List services/applications
 WS     /api/ws              # Real-time sync
```

## Smoke test

Backend API smoke test:

```bash
cd backend
./scripts/smoke_api.sh
```

## Data model

- **Task**: id, title, description, status, priority, category, tags, due_date, created_by, assigned_to, timestamps
- **User**: id, username, password_hash (argon2), created_at
- **Status**: Pending, InProgress, Completed
- **Priority**: Low, Medium, High, Urgent

## Known sharp edges (we plan to fix)

- Task IDs: backend uses UUIDs; frontend now stores UUID strings end-to-end. Selection is still list-index-based, so true stable identity is the next step.
- JS↔WASM ABI: task data is written by hard-coded memory offsets in JS. This is brittle (struct padding/alignment). Planned fix is explicit WASM setters.
- WS behavior: client currently responds to WS events by reloading the full task list.

## Extended Notes (For People Who Like Tools)

This repo is intentionally opinionated:

- The renderer should stay dumb. It consumes a stable, packed command stream produced by WASM.
- Clay should stay in its lane (layout). JS should not parse Clay internal structs.
- If we need more “native feel,” we do it with deterministic state and animations in WASM, not with a UI framework.
- Deployability matters: the backend is a single Rust binary; the frontend is static assets + a wasm file.

If you want the longer-form philosophy and operating model, see `.opencode/runtime_philosophy.md`.
