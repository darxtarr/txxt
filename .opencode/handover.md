# Handover (2026-01-26)

This repo started as a one-shot prototype: Clay (C) -> WASM for UI layout, rendered via Canvas2D in the browser, backed by a single Rust server (axum + redb). Target environment is an enterprise CloudPC / remote desktop with no GPU acceleration: keep the UI crisp and reactive on CPU.

## What Changed / Why

### 1) Sandbox + repo hygiene
- Created `tmp/` as the repo-local scratch dir (and gitignored it).
- Added a backend API smoke script at `backend/scripts/smoke_api.sh`.
- Avoid `/tmp` for artifacts (sandbox can isolate it).

### 2) Frontend performance + basic UX
- Fixed the canvas loop so it no longer resizes the canvas every frame (huge CPU win on software rendering).
- Added an on-canvas perf HUD (toggle `F2`): FPS + wasm ms + draw ms + command counts.
- Fixed login input overlap/opacity by removing a CSS override that forced transparent backgrounds.

### 3) Clay/WASM linkage + debug tools
- Clay’s wasm build expects an imported `clay.queryScrollOffsetFunction`; we provided a stub.
- Added Clay debug tools toggle via `Ctrl+D` (avoid browser-reserved keys); it calls `Clay_SetDebugModeEnabled`.

### 4) The big one: stop parsing Clay structs in JS
Problem: JS was reading Clay internal structs from WASM memory using hand-written struct definitions and hard-coded offsets. This is brittle across Clay versions/packing/alignment and caused “haunted UI” behavior.

Fix: WASM now emits a stable packed command stream:
- `frontend/main.c` packs Clay render commands into a fixed 64-byte-per-command buffer with a 16-byte header.
- `frontend/dist/index.html` renders from this packed buffer (no Clay struct parsing in JS).

Critical ABI fix:
- `UpdateDrawFrame` previously returned a struct; in WASM that uses a hidden sret pointer and broke our assumptions about arguments.
- `UpdateDrawFrame` now returns `void` and writes packed commands to the passed buffer address.

### 5) Dev-mode auth bypass (temporary)
To focus on layout/UX first, auth was bypassed:
- Backend middleware treats missing Authorization as the default admin user.
- Frontend skips login and goes straight to tasks.

This is intentionally temporary; tighten it later.

## Current State

- UI renders reliably again, including Clay debug tools.
- We now have instrumentation (HUD) to guide CPU performance work.
- The repo is currently dirty with changes after the earlier commit `fc839dd`.

## Goals

- Crisp UI on CPU Canvas2D (CloudPC): no per-frame reallocations, minimal allocations in hot paths.
- Keep Clay in its lane: immediate-mode layout + stable IDs; renderer stays dumb.
- Minimal web stack: JS only as platform shim (IO + pixels), not “app framework”.

## Next Work (Recommended Order)

1) Kill JS task-struct offset poking
- Right now task data is still being written into WASM memory by hard-coded offsets in JS. Replace with explicit WASM setters or a packed task input format.

2) Fix task identity end-to-end
- Backend uses UUID; frontend currently uses `uint32_t id` for tasks. Make UUID a first-class ID in WASM state (string or 16 bytes) or provide a stable mapping layer.

3) Stop full reload on WS events
- Current client behavior is “WS event => reload all tasks”. Apply patches incrementally.

4) Decide on animation model
- Use WASM-owned animation state for panel transitions/hover/press to get “snappy feel” without needing a DOM framework.

## Controls

- `F2`: perf HUD toggle.
- `Ctrl+D`: Clay debug tools toggle.

## Notes

- Browser caching of WASM can be sticky; `index.html` loads `app.wasm` with a cachebust query.
- If the UI ever goes white again, check the packed command header and `UpdateDrawFrame` signature first.
