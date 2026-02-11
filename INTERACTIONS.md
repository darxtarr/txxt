# Task Interaction Design

Ideas captured 2026-02-10. Honest assessment of each.

## Known quirks

- **Double-click blocked by browser extensions:** Some extensions intercept
  dblclick events. If double-click-to-create doesn't work, try incognito.
  Confirmed working in clean profiles across Chrome/Edge/Firefox.

## Double-click to create (DONE)

Double-click on the calendar grid → create a 30-minute task at that slot.
Default title "New task", default service, Medium priority.

This is standard calendar behavior. Users expect it. No controversy.

**Needs:** inline title editing after creation (later sprint). For now
the task appears with a placeholder name. Good enough to validate the
full create→render pipeline.

## Drag to move (DONE)

Already works. Task snaps to grid on drop. Server receives MoveTask
command, broadcasts to all clients.

**Current snap:** 15-minute grid (SNAP_Y = HOUR_HEIGHT / 4 = 15px).
Could relax to 30-minute snap for moves (see snap resolution below).

## Drag to resize (DONE)

Grab the bottom edge of a task → drag to change duration.
Detection: if mousedown is within ~8px of the bottom edge, it's a
resize. Otherwise it's a move.

Resize sends a MoveTask command with the same day/start_time but
new duration. The server already handles this — MoveTask accepts
day + start_time + duration.

**Easy to build** once create is working. Same command, different UX.

## Alt+drag to clone (DONE)

Alt+drag duplicates the task. The clone appears at the drop position
with the same title, service, and priority as the original.

Under the hood: on mousedown with Alt held, dragMode='clone' saves the
original position. The entity visually follows the drag. On mouseup,
the original snaps back to where it was and a CreateTask command is sent
with the drop position and the original's metadata. The server creates the
clone and broadcasts it; IRONCLAD adds it to the grid on TaskCreated.

**Genuinely useful** for ops: "I'll do this again tomorrow for 4 hours."

## Different snap resolutions (BUILD SOON)

- **Move:** snap to 30 minutes (coarse positioning — "put it at 2pm")
- **Resize:** snap to 5 minutes (fine-tuning — "this takes 25 minutes")

Ergonomically sound. Move needs to feel fast and chunky. Resize needs
precision. Easy to implement — just vary SNAP_Y based on drag mode.

**Server change needed:** relax validation from 15-min grid to 5-min
grid (change `% 15` to `% 5` in validate_scheduling).

## Modifier-click for manual entry (DEFER)

Shift+click (or right-click) opens a manual time entry panel.
For entering exact start/end times, or tasks spanning unusual durations.

This is a docked panel, not a modal (per design philosophy). Shows
numeric inputs for start time, end time, day. Power-user feature.

**Defer until** the basic interaction loop (create, move, resize) is
solid. The panel needs a proper UI design pass.

## Multi-day tasks (PUSHBACK)

A task that spans Monday–Wednesday isn't one entity — it's three.
The data model uses day (u8) + start_time + duration within a single
day. A multi-day task would need a fundamentally different model
(date ranges, cross-day rendering, split display).

**Instead:** use modifier-drag to clone the task to each day. "I'll
work on the pipeline Mon/Tue/Wed" = three cloned tasks. This is how
ops actually thinks about it — 3 calendar blocks, not 1 spanning block.

If real multi-day support is needed later, it's a new entity type
(a "block" or "span"), not a modification to Task.

## Recurring tasks (PUSHBACK — future system)

"Every Monday at 9am" is a scheduling rule, not a task. Implementing
recurrence means: rule engine, exception handling, "this week only"
overrides, template vs instance distinction.

This is a real feature for the ops use case (weekly standups, recurring
maintenance windows). But it's a separate subsystem that should be
designed deliberately, not bolted onto the current Task model.

**Park it.** When the basic scheduling loop is proven and the team is
using it daily, revisit recurrence as its own design session.
