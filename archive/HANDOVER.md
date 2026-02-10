# TxxT Handover (Design Rationale)

This repo is intentionally not a typical web app. If you came here expecting React + REST + modals, you are in the wrong movie.

The goal is a tool that feels like a professional terminal surface: always-on, low-latency, information-dense, and pleasant on CPU-only rendering (CloudPC constraints).

This document explains the choices, what is “core”, what is “replaceable”, and the intended direction.

## The North Star

- Always-on loop: input -> state update -> render, every frame.
- Realtime is a first-class feature (multi-client collaboration), not a bolt-on.
- CPU Canvas2D is the target; performance is a feature.
- JS exists because browsers exist. We minimize it.

## What We Built (Current Repo)

Today, this repo is a prototype of the runtime model:

- Backend: one Rust server (axum) with redb storage.
- Frontend: C + Clay immediate-mode UI compiled to WASM.
- Rendering: JS draws a packed command stream onto Canvas2D.

The key architectural pivot already implemented:

- JS does NOT parse Clay internal structs anymore.
- WASM emits a stable packed render command stream; JS is a dumb renderer.

## What Is Core vs Replaceable

Core (the value):

- The runtime model: always-on UI loop, deterministic state, realtime deltas.
- The separation of responsibilities: UI state in WASM, IO in a thin shim.
- The packed render command stream boundary (renderer stays dumb).

Replaceable (implementation detail):

- Clay vs another layout engine.
- C-in-WASM vs Rust-in-WASM.
- redb vs SQLite vs Postgres.

If you want to rewrite parts later, preserve the core boundaries and you can swap internals without turning the system into a slow web form.

## The Intended Deployment Model (Where We’re Going)

We are moving toward a “local edge daemon” model:

- There is a central server (authoritative source of truth).
- Each CloudPC runs an always-on edge process that syncs continuously.
- The browser UI talks ONLY to the local edge.

This yields:

- UI is instant (local round trips).
- Sync continues even if the user disconnects from the CloudPC session.
- Central credentials never touch JS.

## Sync Model (How Realtime Should Work)

For 1k-10k tasks and ~10 active users, the system must avoid full reloads.

The intended sync protocol:

- Central assigns a monotonically increasing revision `rev` to every mutation.
- Central emits WS deltas: `TaskUpsert`, `TaskDelete`, etc., each carrying `rev`.
- Edge stores `last_applied_rev`.
- On reconnect, edge requests deltas since `last_applied_rev`.
- If there is a gap or the client is too far behind, edge pulls a snapshot and resumes deltas.

Sanity checks are intentionally used at the edge boundary:

- Validate message shape and IDs.
- Enforce `rev` monotonicity and detect gaps.
- Use schema/version tags and force resync rather than corrupting local state.

## UI Model (Keyboard-First, Mouse-OK)

Humans are not StarCraft players; the perf risks come from constant background churn (realtime deltas + render loop), not from clicking speed.

Design intent:

- Keyboard-first workflows (example: focus -> quick find -> enter).
- Mouse workflows remain functional.
- No full data reloads after initial hydrate.
- Visible-only rendering (viewport culling / virtualization).
- Aggressive caching of expensive primitives like text measurement.

## “JS Is a Necessary Evil” (And What That Means)

JS responsibilities should remain:

- IO: HTTP/WS to edge.
- Input event plumbing.
- Canvas2D pixel drawing.
- Minimal DOM usage for text input only.

JS should NOT become:

- The application state machine.
- The model layer.
- A UI framework.

If you put app logic in JS, you will recreate the very web-app failure modes this repo exists to avoid.

## Security Posture (Deliberately Minimal For Now)

This project prioritizes correctness and performance over hardening.

- Security is intentionally not a concern until the system works flawlessly.
- Current code contains dev bypasses and insecure defaults.
- Do not deploy this publicly.

That said, the intended “enterprise comfort” posture for the edge model:

- A simple local unlock UX gate (PIN) to gate UI access.
- Central credentials/tokens stay inside Rust (edge), not in JS.
- Local cache may be encrypted at rest to avoid “copy the cache file and read everything” class of complaints.
- This is not Fort Knox. It’s basic hygiene on locked-down enterprise CloudPCs.

## Known Technical Debt (We Accept It, For Now)

- Shared-memory task/service buffers currently rely on hard-coded layouts.
  - We fixed UUID identity in the buffer (UUID string is now carried into WASM).
  - Next step is to export layout constants from WASM so JS cannot drift.
- Selection/edit flows still lean on list indices in places; identity should be UUID-based end-to-end.
- WS currently triggers full task reloads in the prototype; intended direction is delta apply.

## How To Keep This Maintainable

If you only remember three things:

1) Preserve the boundaries: packed render stream, explicit snapshot+deltas, edge owns tokens.
2) Never do O(total_tasks) work per frame.
3) If anything looks “haunted”, suspect ABI/layout drift first.

## If This Breaks

If you are debugging a broken system:

- Check sync first: is WS connected, are `rev` values monotonic, is the edge stuck replaying?
- Check ABI second: did the WASM and JS agree on buffer stride/offsets?
- Check performance third: did someone accidentally lay out / measure / draw 10k rows every frame?

If you need to rewrite it:

- You can replace Clay or the entire WASM UI as long as you keep the packed render stream boundary.
- You can replace redb with SQLite/Postgres as long as you keep snapshot+deltas semantics.
