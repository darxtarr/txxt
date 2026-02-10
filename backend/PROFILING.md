# txxt Backend Profiling Lab ("oxygen" branch)

Updated: 2026-02-10
Branch: `oxygen/profiling-lab`

This document explains what we are building, why this branch exists, and why each profiling/tooling choice was made.

## What We Are Building

`txxt` backend is a stateful, authoritative game-style server for collaborative scheduling.

Core shape:

- In-memory world (`World`) is the runtime source of truth.
- Clients send binary commands over WebSocket.
- Server validates + applies command, increments revision, persists to `redb`, broadcasts binary event.
- Browser is a thin renderer and input surface; backend owns decisions/state.

Operational north star for this phase:

- Keep the hot path simple and inspectable.
- Keep dependencies intentionally selected.
- Make performance measurable before adding complexity.

This means we do not optimize by vibes. We instrument first, then decide.

## Why This Branch Exists

`oxygen/profiling-lab` is a safe dyno cell, not product polish work.

Purpose:

1. Add temporary/high-signal instrumentation.
2. Stress and profile realistic paths.
3. Keep everything removable and isolated.
4. Merge only what survives evidence.

Branch policy:

- This branch can be noisy and experimental.
- Mainline stays lean.
- Any retained profiling hooks must be feature-gated and low overhead.

## What Changed Here

## 1) Dependency pruning before profiling

Already applied in backend:

- Removed direct `jsonwebtoken` dependency.
- Removed direct `chrono` dependency.
- Removed direct `futures-util` dependency in app code.
- Reduced Tokio features from `full` to explicit minimal set:
  - `macros`, `rt-multi-thread`, `net`, `sync`, `time`

Rationale:

- `tokio = { features = ["full"] }` hides cost and weakens dependency intent.
- JWT/chrono stack was heavy for current PoC auth posture.
- `futures-util` was only needed due to split WS task style; loop was rewritten to pure `tokio::select!`.

## 2) Profiling feature set (this branch)

Added Cargo features in `Cargo.toml`:

- `profile`: enables `tracing`, `tracing-subscriber`, and `tokio/tracing`
- `profile-console`: includes `profile` and enables `console-subscriber`

Why features, not always-on deps:

- zero cost in default runtime path
- deterministic opt-in for profiling sessions
- easy to remove once lessons are absorbed

## 3) Runtime instrumentation points

Instrumentation was added at hot path boundaries, not everywhere.

`src/game.rs` tracks:

- client connect/disconnect lifecycle
- snapshot pack duration
- snapshot send duration
- command unpack duration
- world write-lock acquisition delay
- `world.apply` execution duration
- persistence flush duration
- event pack duration
- broadcast send duration
- total command pipeline duration

`src/persist.rs` tracks:

- table-open duration
- row+revision write duration
- transaction commit duration
- total flush duration

`src/main.rs` adds:

- feature-gated tracing initialization
- optional Tokio console subscriber integration
- baseline startup metadata events

## Tool Choices and Rationale

This section explains why we chose each profiling tool and where it helps.

## `tracing`

Role:

- Structured telemetry events/spans in Rust code.

Why:

- Better than ad-hoc print timing.
- Fields are queryable/filterable (`elapsed_us`, `bytes`, etc.).
- Scales from local debug to richer collectors.

Where it fits:

- command pipeline and persistence flush instrumentation.

## `tracing-subscriber`

Role:

- Log/event collection + filtering + formatting.

Why:

- Enables environment-driven filtering (`RUST_LOG`).
- Keeps runtime output configurable without code edits.

Where it fits:

- all `tracing` output routing.

## `console-subscriber` (Tokio console path)

Role:

- Introspect async runtime behavior (tasks, wakes, polling patterns).

Why:

- If latency appears but code timings look fine, scheduler/task behavior may be the culprit.
- Gives visibility into runtime-level contention and async behavior.

Where it fits:

- deeper async diagnostics sessions, not mandatory for every run.

## Why we did NOT add heavier profilers yet

Examples: continuous pprof endpoint, eBPF stacks, custom metrics backend.

Reason:

- This stage is about local evidence and rapid iteration.
- Start with low-friction, source-local instrumentation.
- Escalate only if data says we are blind.

## How To Run

From `backend/`:

Default build (no profiling hooks active):

```bash
cargo run
```

Tracing profiling mode:

```bash
RUST_LOG=txxt_server=debug cargo run --features profile
```

Tracing + Tokio console mode:

```bash
RUST_LOG=txxt_server=debug cargo run --features profile-console
```

Validation commands:

```bash
cargo check
cargo check --features profile
cargo check --features profile-console
cargo test
cargo test --features profile
```

Smoke + baseline capture (recommended first run):

```bash
# from repo root
./backend/scripts/profile_smoke.sh 40
```

Artifacts are written under `tmp/profile-smoke/<timestamp>/`:

- `server.log` - tracing output from server run
- `client-metrics.json` - websocket client-side smoke data
- `baseline-summary.json` - parsed p50/p95/max summary

Notes:

- Script requires `./.venv-profile/bin/python` with `websockets` installed.
- Script expects port `3000` to be free before it starts.
- The workload currently sends `CreateTask` commands only (fast, deterministic hot-path signal).
- Script runs the server against an isolated save file per run via `TXXT_SAVE_FILE`, so it does not pollute the default `backend/tasks.redb`.

Manual isolated runs are also supported:

```bash
TXXT_SAVE_FILE=/tmp/txxt-profile.redb cargo run --features profile
```

## Reading the Data (Practical)

When profiling a user action (e.g., drag/move task), read the timeline in this order:

1. `command unpacked`
2. `world write lock acquired`
3. `world.apply completed`
4. `save file flush completed`
5. `event packed`
6. `command pipeline complete`

Use this to classify bottlenecks:

- Lock wait high -> contention in mutation path.
- Apply high -> validation/state logic cost.
- Flush high -> storage transaction latency.
- Pack high -> serialization/layout cost.
- Total high with low internals -> network/backpressure or scheduling effects.

## Rules for Merging Back to Main

Only merge changes that satisfy one of these:

1. Demonstrably improve latency or throughput.
2. Reduce complexity/dependency footprint without losing capability.
3. Preserve observability with negligible steady-state overhead.

Do not merge:

- noisy instrumentation that nobody reads
- broad dependencies with no measurable value
- profiling features that are always-on by accident

## Current Caveats

- Existing app warning for currently-unused world helper methods is unrelated to profiling and currently tolerated.
- Login token behavior is currently lightweight PoC behavior; production auth hardening is intentionally deferred.
- This branch is for performance learning, not final security posture.

## Explicitly Deferred (By Choice)

The following are intentionally deferred to dedicated hardening sessions so this branch can stay focused on performance:

- WebSocket auth enforcement (current flow remains dev-mode for fast iteration).
- Flush failure escalation policy beyond logs/telemetry.

When this is tackled, we will implement a focused operational pass including loud alerting (for example: prominent client warning + email notification) and clear degraded-state behavior.

## Next Recommended Steps

1. Add a repeatable WS load script (fixed scenario: connect -> burst move/schedule -> reconnect).
2. Capture 3 baseline runs and record p50/p95 for command total and flush.
3. Decide whether replacing `tower-http` static serving is worth complexity at current scale.

The principle is simple: measure -> decide -> cut.

## Session Changelog (2026-02-10)

This section is the high-fidelity handover log for future sessions/models.

- Branch created: `oxygen/profiling-lab`.
- Dependency trim pass completed:
  - removed direct `jsonwebtoken`, `chrono`, and app-level `futures-util` usage.
  - narrowed Tokio features to explicit subset (no `full`).
- Runtime path simplification completed:
  - WS handling rewritten to a single `tokio::select!` loop.
  - permissive CORS layer removed from server setup for same-origin local flow.
  - unused wire `ERROR` constant removed.
- Profiling stack added behind features:
  - `profile`, `profile-console` in `Cargo.toml`.
  - tracing init in `src/main.rs`.
  - hot-path timing in `src/game.rs` and `src/persist.rs`.
- Isolated profiling storage added:
  - server now honors `TXXT_SAVE_FILE` env var.
  - smoke script writes to per-run DB file in `tmp/profile-smoke/<ts>/tasks-profile.redb`.
- Smoke tooling added:
  - `backend/scripts/profile_smoke.sh` with server launch, WS command burst, metrics parse, JSON summary output.
  - parser strips ANSI sequences before regex extraction.
- Validation completed repeatedly across this session:
  - `cargo check`
  - `cargo check --features profile`
  - `cargo check --features profile-console`
  - `cargo test`
  - `cargo test --features profile`

Primary conclusion from baseline data:

- command latency is currently dominated by persistence flush commit time, not world mutation logic.

## First Baseline Snapshot (oxygen lab)

Source run:

- Command: `./backend/scripts/profile_smoke.sh 40`
- Summary file: `tmp/profile-smoke/20260210-223219/baseline-summary.json`

Server metrics (microseconds):

| metric | n | p50 | p95 | max |
| --- | ---: | ---: | ---: | ---: |
| pipeline total | 40 | 2541 | 4351 | 25618 |
| lock wait | 40 | 0 | 0 | 2 |
| world.apply | 40 | 5 | 20 | 45 |
| flush | 40 | 2481 | 4307 | 25560 |
| event pack | 40 | 2 | 5 | 6 |

Interpretation:

- Current command latency is dominated by `flush` (storage transaction), not world logic.
- `world.apply` and event pack are already very cheap at this scale.
- Lock contention is effectively absent in this single-client smoke.

This baseline is intentionally simple and local; use it as a comparison anchor, not as a production SLA.
