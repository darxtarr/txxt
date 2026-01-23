# Task Tracker (txxt)

Multi-user task tracking app with Clay/WASM frontend and Rust backend.

## Project Status

**Phase 1 (Skeleton) - IN PROGRESS**

### Completed:
- [x] Project structure created
- [x] `frontend/clay.h` - Clay UI library copied
- [x] `frontend/build.sh` - WASM build script
- [x] `frontend/main.c` - Clay UI implementation (~700 lines)
  - Login screen
  - Sidebar with status filters
  - Task list with cards
  - Task detail panel
  - Click handling infrastructure
  - State management
- [x] `frontend/dist/index.html` - Canvas2D renderer + JS glue (~600 lines)
  - WASM loading and Clay initialization
  - Canvas rendering loop
  - API client functions (login, CRUD tasks)
  - WebSocket connection for real-time updates
  - Modal for task creation
  - Input overlay system for login
- [x] `backend/Cargo.toml` - Rust dependencies configured

### TODO:
- [ ] `backend/src/main.rs` - Server entry point
- [ ] `backend/src/models.rs` - Data structures
- [ ] `backend/src/db.rs` - redb storage layer
- [ ] `backend/src/auth.rs` - JWT authentication
- [ ] `backend/src/api.rs` - REST endpoints
- [ ] `backend/src/ws.rs` - WebSocket handler
- [ ] Build and test WASM compilation
- [ ] Test end-to-end flow

## Stack

- **Frontend**: Clay (C) → WASM + Canvas 2D + minimal JS glue
- **Backend**: Rust + axum (HTTP/WebSocket) + redb
- **Deployment**: Single Rust binary serving static files

## Project Structure

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

## Data Model

- **Task**: id, title, description, status, priority, category, tags, due_date, created_by, assigned_to, timestamps
- **User**: id, username, password_hash (argon2), created_at
- **Status**: Pending, InProgress, Completed
- **Priority**: Low, Medium, High, Urgent
