# Task Tracker (txxt)

Multi-user task tracking app with Clay/WASM frontend and Rust backend.

## Project Status

**Phase 1-3 COMPLETE - Backend and Frontend builds working**

### Completed:
- [x] Project structure created
- [x] `frontend/clay.h` - Clay UI library
- [x] `frontend/build.sh` - WASM build script
- [x] `frontend/main.c` - Clay UI implementation (~875 lines)
  - Login screen with input placeholders
  - Sidebar with status filters (All/Pending/In Progress/Completed)
  - Task list with cards showing priority, status, due date
  - Task detail panel
  - Click handling infrastructure
  - State management
- [x] `frontend/dist/index.html` - Canvas2D renderer + JS glue (~700 lines)
  - WASM loading and Clay initialization
  - Canvas rendering loop (software-rendering compatible)
  - API client functions (login, CRUD tasks)
  - WebSocket connection for real-time updates
  - Modal for task creation (HTML overlay)
  - Input overlay system for login
- [x] `frontend/dist/app.wasm` - Compiled WASM (117KB)
- [x] `backend/Cargo.toml` - Rust dependencies
- [x] `backend/src/main.rs` - Server entry, static file serving
- [x] `backend/src/models.rs` - Task, User, API types, WebSocket messages
- [x] `backend/src/db.rs` - redb storage with CRUD operations
- [x] `backend/src/auth.rs` - JWT authentication, argon2 password hashing
- [x] `backend/src/api.rs` - REST endpoints with broadcast
- [x] `backend/src/ws.rs` - WebSocket handler for real-time sync

### TODO:
- [ ] Test end-to-end flow
- [ ] Polish and bug fixes

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
