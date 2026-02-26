# txxt Current State

Last updated: 2026-02-13

This file is the fast handoff for future us. It is intentionally practical.
For deep architecture rationale, read `DESIGN.md`.

## Doc map (what to trust)

- `CURRENT_STATE.md` — live implementation/behavior contract (this file).
- `DESIGN.md` — architecture rationale and protocol-level intent.
- `CENTRAL_CLIENT_PROTOCOL.md` — planned Central (EC2) <-> Client (local Rust) split contract.
- `KEYBINDS.md` — user-facing binding list; source map lives in `frontend/keybinds.js`.
- `INTERACTIONS.md` — interaction rationale and ideas; may lag implementation.
- `analysis-sonnet-arch-review-2026-02-11.md` and `.opencode/handover.md` — historical snapshots.

## Product intent (plain language)

- Build a multiplayer scheduling system that feels like a terminal/game, not a CRUD web app.
- Keep the Rust server authoritative and stateful.
- Keep the browser thin: render pixels, capture input, send compact commands.
- Optimize for VDI/CloudPC constraints where every extra client-side cost hurts.

## Current architecture

- Backend: Rust + axum single binary (`backend/src/main.rs`).
- Runtime truth: in-memory world (`backend/src/world.rs`).
- Persistence: redb save file (`backend/tasks.redb`) loaded on boot, flushed on mutation.
- Realtime protocol: binary WebSocket (`/api/game`) with fixed-stride records (`backend/src/wire.rs`).
- Frontend: no-build JS engine (`frontend/ironclad.js`) + keybind module (`frontend/keybinds.js`).

## Frontend interaction contract (as of now)

- Views
  - Week view: default, 7-day lane.
  - Day view: collapsed mode (`Alt-C`) showing single-day behavior.
  - Month view: `Alt-M`, constrained centered panel.

- Time navigation
  - `ArrowLeft` / `ArrowRight` only.
  - Step size depends on current view:
    - day view: 1 day
    - week view: 1 week
    - month view: 1 month
  - Works while dragging too.

- View switching
  - `Alt-M`: toggle month <-> week.
  - `Alt-C`: toggle week/day collapse; from month view, jump directly to day view.

- Scheduling interactions
  - Double-click grid: create 30-minute task at slot.
  - Drag: move task (snap grid).
  - Edge drag: resize task.
  - Alt+drag: clone task.

## Rendering and "flashlight" reality

- Canvas always draws tasks that belong to the current visible time window.
- The DOM pool (15 proxies) is for interaction hydration near cursor, not full entity rendering.
- Flashlight currently controls proxy activation, not data loading/unloading from the client model.

## Counts and diagnostics

- Status bar shows: `visible / loaded` task counts.
- Perf panel shows: `Entities: visible / loaded`.
- This is expected: loaded can be higher than currently visible window.

## Operational notes

- Save file location: `backend/tasks.redb`.
- To reset data to a clean start, delete that file and restart backend.
- Current script cache-bust in HTML: `frontend/index.html` loads `ironclad.js?v=12`.

## Team decisions in force

- Use code as source of truth when docs lag.
- Discuss larger UX/keybind behavior before implementing.
- Keep keybind mapping isolated in `frontend/keybinds.js`.
- Focus now: single-window performance and responsiveness until breakpoints are known.

## Near-term roadmap (agreed)

- Document first, then performance harness.
- Add perf targets and repeatable stress scenarios before deep optimization.
- Keep multi-window/tab orchestration as a later phase after single-window baseline is strong.
- Evolve toward Central/Client split with browser as pure scene shell.

Performance execution plan lives in `PERF_PLAN.md`.
