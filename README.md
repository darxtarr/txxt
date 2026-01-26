# Txx Tracker (txxt)

Task tracker built as a deliberate anti-framework demo: Clay immediate-mode UI in C compiled to WASM, rendered via Canvas2D, backed by a single Rust server binary.

This is intended to run on an enterprise CloudPC / remote desktop environment with no GPU acceleration. The goal is “crisp enough to feel native” without pulling in a traditional web stack.

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

- Task IDs: backend uses UUIDs; frontend currently treats IDs like integers. This breaks stable identity.
- JS↔WASM ABI: task data is written by hard-coded memory offsets in JS. This is brittle (struct padding/alignment). Planned fix is explicit WASM setters.
- WS behavior: client currently responds to WS events by reloading the full task list.
