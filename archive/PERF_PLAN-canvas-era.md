# Performance Plan (Single-Window Phase)

Last updated: 2026-02-13

This plan is for the current phase: one active browser window, push until break,
then optimize intentionally. Multi-window/tab orchestration is deferred.

## Why this exists

- Avoid hand-wavy "feels fast" decisions.
- Make optimization sessions reproducible.
- Prevent regressions during rapid feature work.

## Scope (for now)

- One client window in focus.
- Local or near-local server environment.
- Frontend rendering/input latency and protocol handling.
- No security hardening work here.

## Performance targets (phase 1)

- Frame time p95: <= 16.7ms at baseline load.
- Input-to-visible-response p95 (drag/nudge): <= 40ms.
- No long stalls > 120ms during normal interactions.
- Timeline nudge feels immediate when key-spammed.

Notes:
- On VDI/cloud desktops, p95 consistency matters more than peak FPS.
- If p95 target is not realistic for a scenario, define scenario-specific SLOs.

## Test scenarios (repeatable)

Priority: interaction scenarios first, synthetic load second.

1. Interaction bench (automated drag path)
- Use `Interaction bench` button in UI.
- Capture frame p50/p95 and drag latency p50/p95 from console table.

2. Idle render
- No drag, no key spam, no incoming burst.
- Capture p50/p95 frame time for 30s.

3. Drag loop (manual)
- Continuous drag + drop + drag on dense hour blocks.
- Capture drag latency p50/p95 and dropped-frame bursts.

4. Timeline spam
- Hold/spam Left/Right in day, week, and month views.
- Ensure no progressive lag or event backlog growth.

5. View churn
- Rapid `Alt-C` / `Alt-M` toggles while moving cursor.
- Verify stable layout + no jank spikes.

6. Data scale sweep (synthetic, secondary)
- Run at controlled task counts (e.g. 1k, 5k, 10k, 25k).
- Same deterministic seed/pattern each run.

## Metrics to log

- Frontend
  - frame time p50/p95
  - FPS rolling average
  - drag latency p50/p95
  - candidate/proxy counts
  - visible vs loaded entities

- Transport/runtime
  - WS bytes/sec in/out (rough)
  - event burst sizes
  - reconnect/replay timing (when applicable)

## Guardrails (degrade gracefully)

- If frame time p95 blows past threshold:
  - reduce visual detail first
  - clamp interactive proxy activation
  - coalesce non-critical updates per frame

- Never degrade correctness:
  - server-authoritative ordering and final state must stay intact

## Work order

1. Lock instrumentation (cheap, always-on debug counters).
2. Build repeatable synthetic load harness.
3. Record baseline table in this file.
4. Optimize one bottleneck at a time.
5. Re-run full scenario matrix after every meaningful change.

## Baseline table (fill as we run)

| Date | Build | Scenario | Task Count | Frame p95 (ms) | Drag p95 (ms) | Notes |
|------|-------|----------|------------|----------------|---------------|-------|
| 2026-02-13 | current | pending | pending | pending | pending | baseline not recorded yet |

## Out of scope (deferred)

- Cross-tab/window coordination policies
- Focus-aware throttling for background tabs
- Server-side interest-window protocol tuning for multi-client scale
- Security/perimeter hardening
