# Central-Client Protocol Contract

Last updated: 2026-02-13

This doc defines the planned 2-tier runtime model:

- **Central**: EC2 authoritative world coordinator (multi-user truth)
- **Client**: local Rust runtime sidecar (low-latency scene engine)
- **Browser**: ultra-thin viewport/input shell

## Intent

- Keep global correctness in Central.
- Keep interaction latency local in Client.
- Keep browser dumb: no full world model, no heavy scheduling math.

## Authority boundaries

## Central (authoritative)

- Owns canonical world state and revision history.
- Validates and applies user commands from all clients.
- Broadcasts ordered deltas/revisions to connected Clients.
- Resolves conflicts and permissions.

## Client (local scene authority)

- Maintains a local projection/cache of Central state.
- Performs positional math, viewport slicing, and hit-testing.
- Materializes interactive entities for browser only when needed.
- Sends canonical commands upstream to Central.

## Browser (presentation shell)

- Draws scenery supplied by Client.
- Sends input intents (pointer/key/gesture) to Client.
- Does not store or reason over full task graph.

## Runtime channels

1. Central <-> Client (authoritative replication)
- Transport: binary stream (WS/TCP/QUIC, implementation choice).
- Pattern: snapshot + ordered deltas + revision ack.

2. Client <-> Browser (local control plane)
- Transport: local WS (or loopback IPC + embedded webview bridge).
- Pattern: scene chunks + hydrate/dehydrate commands + input intents.

## Message contract (v0 draft)

## Central -> Client

- `WorldSnapshot { rev, tasks[], services[], users[] }`
- `WorldDelta { rev, events[] }`
- `CommandResult { cmd_id, rev, ok|error }`

## Client -> Central

- `SubmitCommand { cmd_id, user_id, command }`
- `ClientAck { last_rev_applied }`
- `InterestHint { viewport, focus_state, filters }` (optional optimization)

## Client -> Browser

- `SceneFrame { frame_id, background_ref, scenery_patches[] }`
- `HydrateEntity { entity_id, rect, visual, text, interaction_flags }`
- `DehydrateEntity { entity_id }`
- `InteractionFeedback { kind, latency_ms, status }`

## Browser -> Client

- `PointerMove { x, y, modifiers }`
- `PointerDown { x, y, button, modifiers }`
- `PointerUp { x, y, button, modifiers }`
- `KeyIntent { key, modifiers, type }`
- `ViewIntent { mode, offset_hint }`

## Materialization model (agreed direction)

- Entity is **not real in browser** by default.
- Browser sees scenery only.
- On eligible interaction (minimum: mousedown), Client sends `HydrateEntity`.
- Hydrated entity becomes interactive DOM surface.
- On de-focus/de-illumination, Client sends `DehydrateEntity`.

Notes:
- Hover preload remains optional and configurable.
- "Flashlight" can be retained as a Client-side hydration policy, not a browser data model.

## Background/scenery pipeline

- Client composes stable background/scenery representation for the current view.
- Browser consumes that as passive pixels.
- Hydrated entities visually replace matching scenery region while active.

## Consistency and reconciliation

- Browser interactions are optimistic only at presentation level.
- Client sends command to Central.
- Central returns authoritative revision/event.
- Client reconciles and updates browser scene/hydration state.

## Failure modes

- Central unavailable: Client enters degraded local mode (read-only or queued commands).
- Client unavailable: Browser is non-functional by design.
- Revision gap: Client requests fresh snapshot and rebuilds local projection.

## Migration plan (incremental)

1. Keep current single-binary path for feature development.
2. Introduce Client-side scene/hydration boundary in-process first.
3. Split Central and Client runtimes with same binary protocol.
4. Move browser to pure scene shell once hydration API is stable.

## Non-goals for this phase

- Full security hardening and zero-trust transport.
- Multi-window/tab orchestration policy.
- Long-term protocol version negotiation.
