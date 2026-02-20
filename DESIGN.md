# txxt — Design Document

Updated 2026-02-20. This is the source of truth for architectural decisions.

If you are a new instance arriving at this codebase: read this entire document
before writing a single line of code. Then understand what it IS and what it
ISN'T:

**This document is the output of a brainstorming session.** It represents the
best thinking of the humans and models who have worked on this project. It is
NOT proven in production. It is NOT the final word. Every hypothesis here needs
to be tested on real CloudPC hardware with real users. Some ideas will float.
Some will sink. That is the point — we documented the reasoning so you can
test it, challenge it, and improve it.

If you disagree with something, say so — but understand *why* it exists before
proposing an alternative. If you can prove something wrong on real hardware,
that's a win, not a failure.

---

## The Constraint

txxt runs on **CloudPCs**. These are Windows 11 virtual desktops hosted on
shared hardware, accessed through VDI protocols (Citrix, RDS, VMware). The
users sit at thin clients or personal machines running a VDI client app. What
they see is a compressed video stream of the remote desktop.

This means:

**No GPU.** The CloudPC is a VM on a hypervisor with zero GPU acceleration.
This is not a "might not have" — it's confirmed from `edge://gpu` on the
actual machines. Hardware acceleration is disabled. WebGL falls back to
software. All rendering must work on the CPU alone.

**Bandwidth is the bottleneck.** The VDI codec (ICA, RDP, Blast) compresses
the screen into a video stream. It's clever — it tracks dirty regions and only
transmits changed pixels. A static screen costs nearly zero bandwidth. A screen
where everything moves every frame (like a canvas redrawing 500 task rectangles
on every mousemove) is the worst case: the codec sees the entire viewport as
dirty, compresses a full frame, ships it over the wire. On a marginal
connection (and corporate WAN links are always marginal), this means dropped
frames, visual artifacts, and a laggy experience that makes users hate the
tool.

**32 FPS cap.** CloudPC display output is typically capped at 32 frames per
second. There is no benefit to rendering faster. A 60Hz animation loop is
burning CPU for frames nobody will ever see.

**The browser is Chromium-based.** Edge or Chrome on Windows 11. Consistent
behavior. HTML5 Drag and Drop API available. No exotic browser constraints.

**5-20 simultaneous users.** A small ops team. Not a consumer app. Not a
thousand concurrent connections. But all users are working the same schedule
simultaneously — when User A moves a task, Users B through F need to see it.

**The users are concierge tech support for an enterprise.** They manage
CloudPC environments, handle tickets, coordinate with developers, and juggle
simultaneous requests. They're on calls, getting interrupted by colleagues,
context-switching between incidents. They glance at the schedule, grab a task,
drop it in a time slot, and go back to the actual work. The scheduling tool
needs to be fast, obvious, and impossible to use wrong under stress.

(The operational instincts informing this design — task titling, the wrong-task
problem, visual distinction under pressure — come from Ulli's decade of NOC
experience. The deployment context is enterprise tech support, but the design
principles are battle-tested in higher-pressure environments.)

---

## The Axiom

> **Everything is possible but nothing is real until the click.**

This is the load-bearing sentence. Every architectural decision in this project
derives from it.

The browser's job is exactly two things:
1. Display one flat image.
2. Track the mouse.

That is all. The browser is a terminal — a piece of glass. It does not render
world state. It does not decide what goes where. It does not own any data. It
forwards raw events and shows what it is told to show.

Why? Because a flat image is the best possible input for a VDI codec. Static
pixels between interactions = the codec sleeps = zero bandwidth. When
something changes, one deliberate full-frame swap at human speed = one clean
encode, not sixty chaotic partial updates per second.

If you are about to write code that creates a DOM element before the user
clicks, stop. If you are about to repaint a canvas on mouse movement, stop.
If you are about to `npm install` anything, stop. Read this section again.

---

## Topology

```
CloudPC (one per user, no GPU, software-rendered VDI)
┌─────────────────────────────────────────────────────┐
│  IRONCLAD (browser)                                  │
│  — displays one flat background image               │
│  — tracks mouse, forwards raw events                │
│  — materializes exactly one div on click            │
│  — dematerializes it on mouseup                     │
│         ↕ WebSocket (localhost or fast LAN)          │
│  Rust sidecar (one binary per CloudPC)              │
│  — owns world state in memory                       │
│  — renders world to image (software 2D, tiny-skia)  │
│  — runs spatial query, maintains candidate list      │
│  — knows what is under the cursor at all times      │
│  — on click: confirms, triggers div materialization  │
│  — on drop: mutates world, re-renders, sends image  │
└─────────────────────────────────────────────────────┘
         ↕ WebSocket (WAN)
EC2 central server (one instance)
— receives state mutations from sidecars
— broadcasts mutations to all other sidecars
— persists canonical world state (redb)
— stores shared artifacts (email attachments, files)
— resyncs sidecars that were offline or crashed
```

### Why one sidecar per CloudPC?

CloudPCs have no GPU. All rendering is software on the local CPU. Hit testing,
spatial queries, image generation — all of this must happen locally to be fast.
A round trip to EC2 for hit testing would add 20-100ms of latency on every
mouse movement. Unacceptable for a tool where the interaction model is
drag-and-drop.

The sidecar runs on the same machine as the browser, or on the local LAN with
sub-millisecond latency. The WebSocket between browser and sidecar is
localhost. This is not a network hop — it's an IPC channel.

### Why a central EC2?

Multiple users work the same schedule simultaneously. When User A moves a task,
Users B through F must see it. The EC2 is the single point of broadcast and the
canonical persistence layer. It does not run business logic. It does not render.
It receives mutations and fans them out.

The EC2 is also the **artifact repository**. When a user attaches an email or
a file to a task, the artifact goes to EC2 so all users can access it. Artifacts
are shared knowledge — they don't live on one person's CloudPC. More on this
in the Artifacts section.

### Why not just EC2 + browser (no sidecar)?

Latency. The EC2 is on the WAN. Even a fast corporate link adds 10-50ms per
round trip. For cursor tracking at 20Hz, that means cursor feedback is 10-50ms
stale. For click-to-materialize, the user waits 10-50ms for the div to appear.
For drag operations, every position update round-trips through the WAN.

With a local sidecar, all of those operations are sub-millisecond. The sidecar
handles the real-time interaction; the EC2 handles the persistence and
broadcast. Each layer does what it's good at.

---

## The Pit Stop Model

This is the intellectual core of the interaction architecture.

An F1 pit stop takes 2-3 seconds. But the car arrives at 60mph and leaves at
60mph. The stop feels instantaneous because the crew is already in position
before the car arrives. Every tool is placed. Every person knows their role.
The car stops. Twenty actions execute simultaneously. The car leaves.

txxt works the same way. The "car" is the user's click.

### Before the click — the crew is in position

The sidecar continuously tracks the cursor and pre-computes the answer to a
question that hasn't been asked yet: "If the user clicks right now, what
should happen?"

```
CONTINUOUS (20Hz, background process):
  Browser sends cursor position → sidecar (5 bytes: type + x:u16 + y:u16)
  Sidecar runs spatial query (see below — trivially cheap)
  If candidate list changed → push update to browser
  Browser caches locally: [{task_id, x, y, w, h, color, title}, ...]
```

The candidate list is the answer to "what is under the cursor right now?" It
contains everything the browser needs to materialize a div: position, size,
color (priority-coded — the sidecar decides the color, not the browser), and
title text.

The browser doesn't ask the sidecar for permission to materialize. It already
has the data. The candidate list is pushed on every change, not pulled on
every click.

### The click — the stop

```
MOUSEDOWN:
  Browser checks LOCAL candidate cache (zero network, pure JS)
  Cursor inside a candidate's bounding box?
    YES → materialize div IMMEDIATELY with cached data
    NO  → nothing to materialize (clicked empty space)
  Browser ALSO sends click event to sidecar (5 bytes: type + x:u16 + y:u16)
  Sidecar confirms (or corrects — almost never happens, see below)
```

The materialization latency is **zero network round-trips**. The browser
performs a local point-in-rectangle check against cached data and creates the
div. The sidecar confirmation arrives ~0.5ms later on localhost and is almost
always "yes, that's correct."

When would the sidecar correct? Only if the candidate list was stale — the
cursor crossed a task boundary in the <50ms since the last candidate update
AND the user clicked in that exact window. In practice: essentially never.
Humans decelerate before clicking. But the correction path exists as a safety
net.

### During the drag — the stop in progress

```
DRAG:
  Background: FROZEN (no rendering, no updates, VDI codec sleeping)
  One div: moving via CSS transform (local, no sidecar involvement)
  Browser sends cursor position updates to sidecar
  Sidecar computes snap hints (nearest valid grid slot)
  Sidecar sends snap guides to browser (optional, lightweight)
  VDI: one small rectangle moving = codec's ideal case
```

### The drop — the car leaves

```
MOUSEUP:
  Browser sends final position + task_id to sidecar
  Sidecar: validates position, mutates world state, increments revision
  Sidecar: re-renders world to image
  Sidecar → browser: new background image
  Browser: removes div, sets new background
  *snap* — the satisfying click of a tactile switch (see "The Flash")
  Sidecar → EC2: mutation event
  EC2 → other sidecars: broadcast
  Other browsers: receive new background image, see the change
```

### The flashlight — spatial proximity queries

The core question: given a cursor position, what interactable elements are
nearby, how far away are they, and which one is the user most likely trying
to click?

The current proposal is the **SDF flashlight** — evaluate signed distance
fields for all elements within a radius around the cursor. But SDFs are not
the only tool. Multiple mathematical approaches exist, and the right answer
might be a combination. This section documents the landscape so future
instances can evaluate, test, and improve.

**Important:** The SDF flashlight was proposed because Ulli knows and
understands SDFs. It is a good starting point, not a proven optimal. If
you find a better approach, propose it. The constraint is: CPU-only, no GPU,
no shaders, math must be cheap, and the approach must generalize beyond
grid-aligned rectangles to any interactable element (buttons, panels, menus,
drop zones).

#### Approach A: Pure SDF evaluation (current proposal)

Every interactable element has an SDF — a function returning signed distance
from any point to the element's boundary:

```
sdf_rect(p, center, half_size) = length(max(abs(p - center) - half_size, 0))
sdf_circle(p, center, radius)  = length(p - center) - radius
```

A few arithmetic operations. Nanoseconds per evaluation.

The **flashlight** is a fixed-radius circle centered on the cursor. On every
cursor update, the sidecar evaluates the SDF for every interactable element
and collects those where `sdf(cursor_pos) < flashlight_radius`. These are
the candidates.

**Strengths:**
- Simple. No data structures, no pre-computation, no maintenance.
- Exact. Gives continuous distance values, not just yes/no.
- General. Works for any shape with an SDF formula.
- Composable. Union (min), intersection (max), subtraction — combine shapes.
- The math IS the algorithm. No separation between query and feedback.

**Weaknesses:**
- O(n) per cursor update. At n=200, 20Hz: 200 × ~5ns × 20 = 20µs/sec.
  Invisible. At n=2000+, it starts to matter.
- No gradient (direction to nearest boundary) without extra computation.
- Evaluates ALL elements, even those obviously far away.

**Cost at our scale:** ~20µs/second. Not a concern. Start here.

#### Approach B: Felzenszwalb-Huttenlocher distance transform

Paper: [Felzenszwalb & Huttenlocher, "Distance Transforms of Sampled
Functions," Theory of Computing, 2012](https://cs.brown.edu/people/pfelzens/papers/dt-final.pdf).

The F-H algorithm computes the exact Euclidean distance transform on a
sampled grid in **O(n) time** (linear in pixel count). The trick: decompose
the 2D transform into two 1D passes (horizontal then vertical). The 1D
squared Euclidean distance field is a set of overlapping quadratic parabolas
— the algorithm walks these parabolas in linear time.

**Applied to txxt:** The sidecar already renders the world to an image on
every layout change. What if it also computes a **distance map** — a second
image where each pixel stores the distance to the nearest interactable
boundary?

```
On every layout change (human speed, >500ms apart):
  1. Render visual background image → send to browser
  2. Render binary mask of interactable element boundaries
  3. Run F-H distance transform on mask → distance map
  4. Render ID map (each pixel colored by nearest element ID)
  5. Keep both maps on sidecar (never sent to browser)

On every cursor update (20Hz):
  distance = distance_map[cursor.y][cursor.x]  → O(1)
  element  = id_map[cursor.y][cursor.x]         → O(1)
```

The distance map IS the pre-rasterized SDF. No per-element evaluation.
Cursor query = memory read. The ID map tells you which element the distance
refers to.

**Strengths:**
- O(1) per cursor update. Literally a pixel lookup. Can't be faster.
- Pre-computation cost is amortized over human-speed layout changes.
- The sidecar is already rendering — distance transform piggybacks.

**Weaknesses:**
- Only identifies the NEAREST element, not all within radius. The flashlight
  needs ALL nearby elements for the candidate list.
- Requires re-computation on every layout change (but so does rendering).
- Memory cost: two additional viewport-sized buffers (~8MB at 1920x1080).
- More complex implementation than brute-force SDF.

**Best use:** Broadphase early-out. If `distance_map[y][x] > flashlight_radius`,
the cursor is in empty space — skip everything. This is the common case (most
of the viewport is empty grid). Saves evaluating hundreds of SDFs.

#### Approach C: Hybrid broadphase/narrowphase

The pattern from game physics collision detection — use the cheap technique to
eliminate most work, then use the precise technique on what's left:

```
BROADPHASE (O(1) — distance map lookup):
  distance = distance_map[cursor.y][cursor.x]
  if distance > flashlight_radius:
    → empty space, no candidates, skip everything
    (this is the COMMON CASE — most of the viewport is empty)

NARROWPHASE (O(k) — SDF evaluation on nearby elements):
  nearest_id = id_map[cursor.y][cursor.x]
  → SDF for nearest_id (known to be in range)
  → SDF for spatial neighbors of nearest_id
  → build full candidate list with exact distances
```

The broadphase turns most 20Hz updates into a single memory read that says
"nothing here." The narrowphase only fires when the cursor is actually near
something — and evaluates only a few elements, not all of them.

**This is probably the optimal end state,** but only worth building if
brute-force SDF (Approach A) proves too slow on real hardware. Start with A,
measure, add the broadphase if needed.

#### Approach D: Potential fields (from robotics)

[Khatib, "Real-Time Obstacle Avoidance for Manipulators and Mobile Robots,"
1986](https://www.sciencedirect.com/topics/computer-science/artificial-potential-field)
introduced artificial potential fields for robot navigation.

Applied to UI: each interactable element emits an attractive potential field
that falls off with distance. The total field at the cursor position is the
sum of all element potentials.

**Where SDF uses `min(distances)` (nearest wins), potential fields use
`sum(1/distance²)` (all contribute).** The difference matters for visual
feedback:

- **SDF composition (min):** Discrete highlighting. The nearest element gets
  the glow; others get nothing. Binary feel.
- **Potential field composition (sum):** Smooth blending. Multiple nearby
  elements create a combined influence. The cursor glow reflects "density of
  interactability" — brighter in busy regions, dimmer in sparse ones. You
  feel the topology of the interactive landscape through the cursor.

The potential field approach is not an alternative to SDF for spatial queries
(you still need exact distances for the candidate list). It's an alternative
for **visual feedback computation**. The cursor glow intensity could be driven
by the summed potential rather than the nearest SDF, giving a richer sense of
the interactive environment.

**Hypothesis to test:** Does potential-field-driven glow feel better than
nearest-element-driven glow on a real CloudPC? The math cost is the same
(O(n) evaluation). The perceptual effect might be significantly different.

#### Approach E: Fitts's Law as candidate ranking

[Fitts's Law](https://en.wikipedia.org/wiki/Fitts%27s_law): the time to
acquire a target is `T = a + b × log₂(2D/W + 1)` where D is distance and
W is target width.

This is not a spatial query method — it's a **ranking method** for the
candidate list. After finding nearby elements, rank them by Fitts index:

```
fitts_index(element) = log₂(2 × sdf_distance / element_width + 1)
```

A large task 80px away (low Fitts index) is EASIER to click than a small
button 30px away (higher Fitts index). Raw SDF distance doesn't capture
this — Fitts's Law does.

The element with the lowest Fitts index gets the strongest proximity glow.
This is perceptually correct: the glow reflects "how easy is this to click?"
not "how many pixels away is this?"

**This is independent of spatial query method.** It works with SDF, distance
maps, potential fields — any approach that gives you distances. It's an
enhancement layer, not a replacement.

#### Approach F: Temporal coherence

The cursor doesn't teleport. Between consecutive 20Hz samples, it moves
10-50 pixels. The candidate list from the previous frame is almost certainly
still valid.

Exploit this: don't recompute from scratch every frame. Maintain the
candidate list incrementally:

1. Check if any NEW elements entered the flashlight (elements near the
   boundary — small set)
2. Check if any OLD candidates left the flashlight
3. Update in-place

At 20Hz with smooth cursor movement, most frames change nothing. O(n)
amortizes to O(1). This optimization applies to any spatial query method.

#### What we recommend

**Start with:** Pure SDF evaluation (Approach A). It's simple, it's fast
enough at our scale, and it gives you everything you need. Don't build the
distance map until brute-force SDF shows up in profiling (it almost certainly
won't at <500 elements).

**The optimization path (if needed):**

```
A (pure SDF)
  → add F (temporal coherence) for free speedup
  → add B (distance map broadphase) if still too slow
  → add E (Fitts ranking) for better candidate ordering
  → add D (potential fields) for richer visual feedback
```

Each layer is independent and additive. You can stop at any point. The layers
compose without interfering.

**What we're NOT sure about:** Whether potential fields (smooth blending) feel
better than SDF min (discrete highlighting) for the proximity glow. Whether
Fitts ranking is perceptibly different from raw distance ranking at typical
flashlight radii. These are empirical questions that need testing, not
theoretical arguments.

The flashlight radius is a design parameter: 80-120px is the hypothesis.
Large enough that mouse deceleration before a click means the target is
already in the candidate list. Needs testing on real CloudPC hardware.

### Why push the candidate list, not pull on click?

Three reasons:

1. **Zero-latency materialization.** The browser has the answer before the
   question is asked. Click response time is literally a local JS operation.

2. **Cursor feedback.** The browser can change cursor style (pointer vs
   default) without a round-trip. It checks the local candidate cache on
   mousemove. If cursor is inside a candidate's bounds → pointer. If not →
   default. Local, instant, no sidecar involvement for cosmetic feedback.

3. **Decoupled cadence.** The sidecar pushes candidate updates only when the
   list changes (cursor crosses a task boundary). Not on every cursor message.
   The browser sends coordinates at 20Hz, but the sidecar might only push 2-3
   candidate updates per second during active mousing, and zero when idle.

---

## Proximity Glow — The VDI Heartbeat

### The problem: VDI kills tactile feedback

On a remote desktop, there is no tactile feedback. When everything freezes —
and on VDI, things freeze regularly — the user cannot distinguish between:
- "The connection dropped"
- "The app froze"
- "The PC froze"
- "The system is processing my last action"

This creates a constant low-grade anxiety. Did my click register? Should I
click again (risking a double-action)? Should I wait (wasting time if the
connection actually dropped)? There is no way to know. The screen just... sits
there.

The proximity glow is a **continuous proof of life**. The world visibly
responds to cursor movement. Elements react when the cursor approaches. The
user sees: "The system is here. My connection is alive. My next click will
work." This eliminates the uncertainty that makes VDI users tentative and slow.

### The mental model: adventure games

Think of LucasArts adventure games (Monkey Island, Day of the Tentacle). You
enter a room. You sweep the cursor across the scene. Interactable objects
light up — a door, a key, a lever. You're mapping the interactive topology
of the scene through cursor exploration. You don't need a manual or a label.
The scene TELLS you what's interactive by responding to proximity.

txxt works the same way. The SDF flashlight sweeps with the cursor. Elements
within range respond visually. The user discovers what's interactive by
moving the mouse. No tooltips needed. No hover states in the traditional
CSS sense. The world itself communicates.

### The constraint that makes this possible

Desktop has no multi-touch. There is exactly **one cursor**. One point of
contact. One flashlight. This is both the bottleneck and the gift:

- Only ONE neighborhood of the screen needs proximity feedback at any time
- Everything outside the flashlight radius is provably static
- The VDI codec only needs to handle dirty pixels near the cursor
- The sidecar only tracks one cursor position per window

Multi-touch would break this model (multiple flashlights, multiple candidate
lists, multiple simultaneous interactions). But on desktop with a mouse —
one cursor = one flashlight = everything else is guaranteed background.
The constraint makes the optimization possible.

### How to make elements react — the VDI cost hierarchy

The VDI codec transmits changed pixels. Every pixel that changes costs
bandwidth. The goal is maximum visual feedback with minimum pixel dirt.
The approaches below are ordered from cheapest to most expensive:

**Tier 0 — Cursor glow color change (essentially free):**

The cursor glow is a small canvas-overlay circle that follows the mouse.
It's ALREADY a dirty region — the codec is already encoding this area every
frame because the cursor moves. Changing the glow's color when an
interactable element enters the flashlight radius costs **zero additional
dirty pixels**. The codec was already going to encode this region.

Neutral glow → colored glow when something is within reach. This is the
adventure game "interactable object detected" signal. The thing the user is
already watching (the cursor) carries the information.

**Tier 1 — Intersection markers (very cheap, mathematically elegant):**

The SDF flashlight is a circle. Interactable elements are rectangles (tasks),
circles (buttons), or other shapes. The intersection of the flashlight circle
with an element's boundary is analytic geometry:

```
For each edge of the rectangle:
  Solve circle-line-segment intersection
  → 0 or 2 intersection points per edge
```

Draw tiny marks (single pixels, short line segments) at those intersection
points. As the cursor approaches an element, the marks appear at the far
edges and sweep inward — the intersection arc grows. Move away, and the arc
shrinks until the marks disappear.

**Why this is beautiful:** The math IS the rendering. The SDF evaluation
gives you the distance (spatial query). The circle-rectangle intersection
gives you the visual effect. No separate "compute highlights" pass. No
separate "am I hovering" check. The distance field does double duty —
spatial query AND visual feedback from the same calculation.

The dirty region is minimal: a few pixels at intersection points, appearing
for 2-3 frames (62-93ms at 32 FPS), then static. Persistence of vision
does the heavy lifting — the human eye reads a brief flash as "something's
here" without needing a persistent highlight. The codec sees a handful of
transient dirty pixels, then silence.

**Tier 2 — Corner accents (cheap):**

Flash 4 small marks at the corners of a nearby element for a few frames.
More expensive than intersection markers (fixed positions rather than
mathematically derived) but more legible in crowded scenes. The accents are
transient — appear, persist for ~100ms, fade. The codec sees 16-32 dirty
pixels for 2-3 frames.

**Tier 3 — Edge segments (moderate):**

Short line segments along the edges of a nearby element. More visible,
higher pixel cost. Could overlap with existing drawn edges of the element
(a color change on existing lines rather than new lines), which would mean
the dirty region is bounded by the element's edge, not additional geometry.

**Tier 4 — Full border highlight (expensive, probably unnecessary):**

All four edges of the element highlighted persistently. Hundreds of pixels
of continuous dirty region for as long as the cursor is near. This is the
obvious approach but the most expensive for VDI. Use only if the cheaper
tiers prove insufficient on real hardware.

### What we DON'T do (and why)

We are not fanatical purists about minimizing DOM or canvas elements. If we
need 20 lightweight objects on the overlay to create good feedback, we use 20.
The principle is: **optimize away what is not needed.** If a single-pixel
flash at intersection points communicates "this is interactive," don't draw
a full border. If the cursor glow color change is enough, don't draw anything
on the element at all. Test the cheapest approach first, escalate only if it
doesn't communicate clearly through the VDI codec.

No shaders. Zero GPU acceleration means all overlay drawing is canvas 2D
context on the CPU. Keep it simple: `fillRect`, `arc`, `moveTo`/`lineTo`.
The math (SDF evaluation, circle-rectangle intersection) is cheap arithmetic.
The drawing is cheap canvas primitives. The expensive part is always the
codec — minimize dirty pixels, maximize visual information.

### The two-tier feedback model (hypothesis to test)

```
CURSOR APPROACHING (element enters flashlight radius):
  Tier 0: cursor glow shifts color → "something's nearby"
  Tier 1: intersection markers appear → "it's THERE" (directional)

CURSOR ON ELEMENT (SDF ≤ 0, point inside bounds):
  Cursor glow intensifies or changes shape
  Intersection markers collapse to element boundary (full contact)
  Candidate is "hot" — instant materialization on click

CURSOR LEAVING (element exits flashlight radius):
  Markers disappear, glow returns to neutral
  Candidate removed from hydrated list
```

This is a hypothesis. The right approach depends on how well each tier
survives the VDI codec — compression artifacts, frame drops, and encoding
latency all affect whether a subtle visual cue actually reaches the user's
eyes. Test on real CloudPC hardware. The cheapest approach that communicates
clearly is the right one.

---

## The Flash Is a Feature

When the user drops a task and the background swaps, there is a visible
transition — the div disappears and the new background appears. This is not
simultaneous. There may be a frame where neither the div nor the updated
background shows the task in its final position.

We do not try to eliminate this. We lean into it.

The flash is **tactile feedback**. It is the visual equivalent of the click of
a mechanical keyboard switch. It tells the user: "your action was received, the
world has changed, here is the new state." In an era of bland minimalism where
buttons don't look like buttons and actions provide no feedback, a deliberate
visual snap is a feature.

This eliminates an entire category of engineering complexity. No double-
buffering. No requestAnimationFrame batching. No pixel-perfect font matching
between sidecar rendering and DOM text. The sidecar renders text with tiny-skia.
The materialized div renders text with CSS. They look slightly different. Nobody
cares, because the div exists for 1-3 seconds during a drag operation, and the
flash on drop tells you the operation is complete.

Do not try to make the swap seamless. If a future instance spends tokens
on sub-frame synchronization, they have missed the point.

---

## Alt-Tab and Visibility Recovery

The user alt-tabs to Outlook, reads emails for ten minutes, alt-tabs back to
txxt. The cursor is sitting in the middle of the grid. What happens?

### The cold mouse problem

No `mousemove` has fired since the page became visible. The browser does not
know the cursor position. The candidate list is stale — possibly minutes old.
Worse: while the user was away, another user may have moved tasks via EC2
broadcast. The background image itself may be stale.

### The resync protocol

```
ON VISIBILITY CHANGE (hidden → visible):
  Browser: mark candidate cache STALE
  Browser → sidecar: "I'm back" message with last_seen_revision

  Sidecar checks revision gap:
    Small gap → send missed event deltas
    Large gap → send fresh snapshot
  Sidecar sends current background image
  Browser: swaps background, clears candidate cache

ON FIRST MOUSEMOVE (after resync):
  Normal 20Hz path resumes
  Candidate list rebuilds within one tick (~50ms)

ON CLICK WITH STALE CACHE (user clicks before moving mouse):
  Browser: cache is stale, DON'T materialize locally
  Browser → sidecar: click coordinates (reactive fallback)
  Sidecar: computes candidate from scratch, responds with bounds
  Browser: materializes (~1ms later on localhost, imperceptible)
```

The 99% path: user moves the mouse before clicking (even slightly —
reorienting after alt-tab). The normal candidate pipeline handles it.

The 1% path: user clicks immediately without moving. Falls back to reactive
(ask sidecar, wait for response). Still sub-millisecond on localhost.

### WebSocket survival

If the tab was hidden long enough, the browser may throttle or kill the
WebSocket. Chrome generally keeps WebSockets alive in background tabs but
behavior varies. The resync protocol handles this: if the WebSocket died,
reconnect and request a fresh snapshot. Same path as sidecar crash recovery.

One resync protocol handles all cases: alt-tab return, WebSocket drop,
sidecar restart, EC2 reconnect. The revision number makes it clean — the
sidecar knows exactly what the browser has seen and what it's missed.

---

## Multi-Window Architecture

Expecting to fit all information on one sub-fullHD screen is unrealistic.
Some users don't even run the VDI fullscreen — they keep the local taskbar
visible underneath. txxt supports multiple browser windows, each showing a
different view of the same world.

### Typical setup

```
Window A: Week view — the current week, primary scheduling workspace
Window B: Month view — planning ahead, seeing the big picture
Window C: Staging inbox — unscheduled tasks, drag source (Quake-console panel)
```

### How it works

Each window opens its own WebSocket to the **same sidecar**. Each connection
carries its own context:

- Viewport size (width × height)
- View mode (week / month / day)
- Visible date range (which week/month the user is viewing)
- Candidate list (per-window, per-cursor)
- Cursor tracking (independent per window)

The sidecar is the **multiplexer**. One world state, N rendering contexts.
When a task is mutated in any window:

```
1. Any window → sidecar: mutation command
2. Sidecar: validates, mutates world, increments revision
3. Sidecar: re-renders background for EACH connected window
   (different views, different viewports = different images)
4. Sidecar → each window: appropriate background image
5. Sidecar → EC2: mutation event (broadcast to other users' sidecars)
```

The cost is 2-3 renders instead of 1 per mutation. But mutations happen at
human speed (>500ms apart) and the chrome layer is cached per-window, so each
render is mostly a memcpy + task rectangles. Negligible.

### Cross-window drag

HTML5 Drag and Drop works across same-origin browser windows natively.

```
Window C (staging inbox):
  mousedown on task → dragstart
  dataTransfer.setData('application/x-txxt-task', task_id_string)

User drags across screen...

Window A (week view):
  dragenter → show drop zone feedback (highlight grid slot)
  dragover  → update snap hint as cursor moves
  drop      → extract task_id from dataTransfer
  Window A → sidecar: "schedule task_id at (day, time)"
  Sidecar: mutates, re-renders all windows, broadcasts
```

The sidecar doesn't know or care which window the command came from. It
processes "schedule this task" the same way regardless of origin. The
browser handles the cross-window plumbing.

---

## Cross-App Drag and Drop — The Universal Verb

Drag and drop is the single most powerful and efficient interaction in
computing. It predates the web. It works across every desktop application.
txxt uses it as the **universal verb** for getting information in and out.

The CloudPC is Windows 11 with Chromium-based Edge or Chrome. The HTML5
Drag and Drop API can receive drops from any Windows application that
participates in the system DnD contract, and can emit drags that any
Windows application can receive.

### Dragging INTO txxt

| Source | What the browser receives | What txxt does |
|--------|-------------------------|----------------|
| Outlook email | `.msg` file via `dataTransfer.files` | Creates task, attaches email as artifact |
| Selected text (any app) | `text/plain` via `getData` | Creates task at drop position |
| File from Explorer | `File` object with name/size/type | Creates task, attaches file as artifact |
| URL from browser | `text/uri-list` via `getData` | Creates task with URL reference |
| Another txxt window | `application/x-txxt-task` custom type | Schedules/moves the task |

### The drop-to-create workflow

Here is where we made a deliberate decision against the obvious approach.

**The obvious approach (WRONG):** User drags email onto timeline. txxt reads
the email subject line (`"FW:FW:RE:RE:AWS Server Maintenance Planned on
January 23"`). Creates a task with that as the title. Done.

**Why this is wrong:** That title is useless. It's an email threading artifact,
not a description of work. Now you have two tasks for the same service with
email-subject titles that wrap across two lines and are indistinguishable at a
glance. During an incident, when you're on a call and a colleague taps your
shoulder, you will add your notes to the wrong task. This is not hypothetical —
this is a daily failure mode in busy support operations.

**The correct approach:** The drop creates a task and **immediately materializes
it for editing**. The email becomes an attachment (an artifact stored on EC2),
not the title. The user types the real title: "Fix nginx ingress - prod cl7."
The human gives meaning. The system gives a time slot, a color, and a
reference back to the originating email.

```
1. Drag email from Outlook → drop on timeline at Wednesday 14:00
2. Task created at that slot with EMPTY title
3. Task IMMEDIATELY materializes — title field focused, cursor blinking
4. Email artifact: uploading to EC2 in background (user doesn't wait)
5. User types: "Migrate community ingress controllers"
6. User sets priority (keyboard shortcut — quick, one hand)
7. Enter → task confirmed, div dematerializes, *snap*
8. Background re-renders: task in grid, proper title, priority color
   Small indicator shows "this task has an attachment"
```

**Why this matters:** Crafting a good task title is an art form. In a NOC,
a well-titled task is the difference between smooth handoff and confused
escalation. Two tasks for the same service need to be instantly
distinguishable:

```
┌────────────────────────────────┐
│ ░░░ PLANNED  Thu 14:00 ░░░░░░ │  ← teal (Medium)
│ Upgrade community nginx        │
│ ingress across all clusters    │
│ 📎                             │
└────────────────────────────────┘

┌────────────────────────────────┐
│ ███ P1  Tue 09:15 ████████████ │  ← red (Urgent)
│ Fix deleted ingress - prod cl3 │
│ dev attempted upgrade early    │
│ 📎📎                           │
└────────────────────────────────┘
```

Same service. Same general topic. Completely different priority, completely
different day, completely different situation. The operator who wrote these
titles knows the domain. No algorithm can replicate this.

### Dragging OUT of txxt

txxt is not self-contained. It lives in an ecosystem of Outlook, Teams,
ServiceNow (when IT allows it), spreadsheets, and chat windows. Getting
information out of txxt must be as easy as getting it in.

When dragging a task FROM txxt to another application:

```javascript
// On dragstart for a task
e.dataTransfer.setData('text/plain',
    'P1 | Fix deleted ingress - prod cl3 | Tue 09:15-10:00');
e.dataTransfer.setData('text/html',
    '<b>P1</b> Fix deleted ingress - prod cl3<br>Tue 09:15-10:00');
e.dataTransfer.setData('application/x-txxt-task', task_id);
```

Drop a task onto:
- **Outlook compose** → pastes formatted task summary into email body
- **Teams chat** → pastes task summary as message
- **Notepad** → pastes plain text task summary
- **Another txxt window** → moves/copies the task

Every Windows application that accepts text drops becomes a txxt consumer.
No API integration. No plugins. Just the standard DnD contract.

### Copy/paste as fallback

Not everything is draggable. Ctrl+C on a selected task copies it to the
clipboard in multiple formats (plain text + rich text). Ctrl+V in any
application pastes the task summary. Same information, different transport.

---

## Task Identity — What Matters Under Pressure

A task's identity — what makes it distinguishable from other tasks — comes
from exactly four things:

1. **Title** — human-crafted, domain-specific, clear
2. **Priority** — color-coded, visible from across the room (P1 red, planned blue)
3. **Service** — who pays for the time, the primary organizational axis
4. **Position** — spatial location on the grid IS the schedule

That's it.

### What does NOT define a task

**Source metadata.** "This task came from an email" is trivia. The task exists
because there is a problem. How the problem arrived at your desk — email,
phone call, Slack message, someone yelling across the NOC — is not relevant to
anyone looking at the schedule. The email itself is an artifact (attached,
stored on EC2, retrievable on demand). But `source: Email` as a field on the
task is noise.

**Creation timestamp.** When the task was created is rarely interesting. When
it's scheduled for — yes. When it was created — no. The event log captures
this if anyone ever needs it (they won't).

**Creator.** Who created the task matters for audit trails (which the event
log provides). It does not matter for the rendered view. The schedule doesn't
care who typed it in.

### The wrong-task problem

This is a real failure mode in busy tech support operations and a design
constraint for txxt:

```
You're on a phone call troubleshooting a P1 incident.
You need to add a note to the incident task.
A colleague walks up and starts talking.
You glance at the screen.
Two tasks for the same service. Similar titles. Adjacent time slots.
You click the wrong one.
You type your update into the wrong task.
You don't realize until the handoff meeting.
The audit trail is now corrupt.
```

This means visual distinction is not a cosmetic concern — it is a correctness
requirement. The rendering must make same-service tasks unmistakable:

- **Priority color** is the strongest signal. Red P1 vs blue planned vs amber
  high — visible from across the room.
- **Title** is the primary differentiator. This is why auto-titling from email
  subjects is unacceptable. Human-crafted titles ARE the UX.
- **Spatial position** provides context. Tuesday 09:15 vs Thursday 14:00.
- **Active task indicator** is critical. When you're editing a task, it must be
  OBVIOUS which one. A glow, a border, a visual treatment that screams "your
  keystrokes are going HERE." Because you will look away (at the call, at the
  colleague, at another monitor) and look back, and you need to confirm you're
  in the right place in under 200ms.

---

## EC2 as Artifact Repository

The EC2 is not just a mutation relay. It is the shared memory of the team.

When a user drops an email onto a task, that email is evidence. When User2
jumps in to assist with the task, they need to read that email. The email
cannot live on User1's CloudPC — it needs to be centrally available.

### Artifact lifecycle

```
UPLOAD (drop event):
  Browser receives file from DnD (e.g., .msg from Outlook)
  Browser → sidecar: create task + artifact blob
  Sidecar: creates task locally (instant response to user)
  Sidecar → EC2: mutation event + artifact upload (background)
  EC2: persists task mutation + stores artifact blob
  EC2 → other sidecars: broadcast task creation with artifact_count

RETRIEVE (on demand):
  User2 clicks task → materializes div with task details
  User2 sees attachment indicator (📎), clicks it
  Browser → sidecar: fetch artifact for task_id
  Sidecar → EC2: request artifact
  EC2 → sidecar → browser: artifact blob
  Browser: opens/downloads the file
```

Artifacts are stored by reference after the initial upload. The background
image does not carry artifact data — just a visual indicator (`artifact_count
> 0` → show paperclip icon). The heavy blob only moves when someone actually
wants to look at it.

### What's an artifact?

Anything dropped onto a task that isn't the task itself:
- Email files (.msg, .eml)
- Documents (.pdf, .xlsx, .docx)
- Screenshots (.png, .jpg)
- Text snippets (stored as plain text files)
- URLs (stored as bookmark files)

Artifacts are opaque to txxt. It stores them, associates them with task IDs,
and serves them on request. It does not parse, index, or render them. The user
opens them with whatever Windows application handles that file type.

### Why not store artifacts on the sidecar?

Because sidecars are per-user. If User1's sidecar stores the artifact, User2
can't access it without routing through User1's machine — which may be offline,
asleep, or on a different network. EC2 is always available. One copy, everyone
can reach it.

---

## Render Strategy

The sidecar renders the world to a flat image using tiny-skia (software 2D,
no GPU required). This is the image the browser displays as its background.

### When rendering happens

Rendering triggers on **state changes only**:
- Task created, moved, deleted, completed
- Week/month navigation
- View mode change (week ↔ month)
- Viewport resize
- Visibility resync (alt-tab return)

Rendering **never** triggers on:
- Cursor movement (that's the candidate list, not the image)
- Hover events
- Time passing (no tick loop)

At 500ms+ between user actions, the render budget is generous. Even a 50ms
render + 50ms encode is invisible at human speed.

### The two-layer cache

```
Layer 1: CHROME — grid lines, day labels, hour markers, headers
  Rendered once per: view change, viewport resize, week navigation
  Cached as a pixel buffer in memory
  Rarely changes. Expensive but infrequent.

Layer 2: TASKS — colored rectangles with titles on the grid
  Rendered on top of chrome layer on every state change
  Cheap: filled rectangles + text strings at known positions
```

On a state change:
1. Copy cached chrome layer to output buffer (`memcpy`, fast)
2. Render all visible tasks on top
3. Encode to image format
4. Send to browser

The chrome layer is the expensive part (grid lines, text labels, layout math).
Caching it means most renders are just "stamp the tasks onto the grid." At
50-100 visible tasks, this is microseconds in tiny-skia.

### Image format

Currently: PNG. Proven, browser-native, good compression for solid-color
rectangles and text. The encode cost is the main concern — PNG encoding a
1316x632 RGBA buffer takes real CPU time.

The wire format includes a `format: u8` byte (message 0x10) specifically so
we can switch formats without protocol changes:

- **PNG** (current): lossless, good compression, moderate encode cost
- **JPEG**: lossy but fast encode, smaller wire size. At quality 85+, text is
  readable. Worth benchmarking on actual CloudPC hardware.
- **Raw RGBA**: zero encode cost, ~3.3MB per frame. The sidecar→browser hop is
  localhost, so bandwidth is free. Browser creates ImageBitmap from the blob.
  Fastest possible path but large WebSocket frames.

The right answer depends on CloudPC CPU specs, which vary. The format byte
lets us benchmark and switch without touching the protocol.

### Dirty-region rendering (future optimization)

When a task moves from slot A to slot B, only two rectangles changed. A full
re-render redraws 50-100 tasks. A dirty render redraws 2.

Not worth building for v1 — full re-render at human speed is fine. But the
architecture allows it:

1. Sidecar knows old bounds and new bounds (from the mutation)
2. Re-render only those two rectangles against the chrome layer
3. Encode as small patches instead of full frame
4. Browser composites patches onto existing background

If CloudPC CPU benchmarks show that full re-render is too slow (unlikely at
500ms+ intervals), dirty rendering is the optimization path.

### Viewport negotiation

The renderer needs to know the browser's viewport size. This is a client→server
message sent on WebSocket connect and on window resize.

The sidecar renders at exactly the requested size. No scaling, no CSS
transforms. What the sidecar renders is pixel-for-pixel what the user sees.
This matters for VDI — the codec doesn't have to deal with CSS scaling
artifacts.

---

## IRONCLAD: The Glass

IRONCLAD is the browser-side code. In the sidecar image pipeline architecture,
it is approximately 200 lines of JavaScript. No dependencies. No build step.
No framework.

### What IRONCLAD does

- Open WebSocket to local sidecar
- Receive background image → create blob URL → set as CSS `background-image`
- Track mouse → send cursor coordinates to sidecar at 20Hz
- Receive candidate list from sidecar (with SDF distances) → cache locally
- Draw proximity feedback on canvas overlay (cursor glow, intersection markers)
- On click → check local candidate cache → materialize one div immediately
- Drag div → report position updates to sidecar for snap hints
- On mouseup → report final position → dematerialize div → receive new background
- Handle keybinds → send mode signals to sidecar
- Handle external DnD (drag from Outlook/Explorer/etc.)
- Handle visibility change → resync with sidecar
- Handle window resize → report new viewport to sidecar

### What IRONCLAD does NOT do

- Render world state (tasks, grid lines, text, labels — sidecar does this)
- Do hit testing (sidecar runs SDF flashlight, browser caches results)
- Own any world state beyond the candidate cache
- Run spatial queries
- Repaint world during drag (background is frozen, div moves via CSS transform)
- Import any library, framework, or runtime

### On element count

IRONCLAD is not dogmatically minimalist about DOM or canvas elements. The
principle is "optimize away what is not needed," not "use exactly one of
everything." If the overlay needs 20 small canvas-drawn marks for proximity
feedback, that's fine. If a drag operation needs a div and a snap-guide
overlay, that's fine. The constraint is: **zero elements that render world
state**. Proximity feedback, cursor glow, snap guides, and the single
materialized interaction div are all local UI — they carry no world data,
they don't duplicate the sidecar's rendering, and they're cheap for the
VDI codec.

### Current state

IRONCLAD v0.8 is the canvas-era implementation (~1,231 lines). It does
everything in the "does not" list above. The wire protocol, SoA entity
storage, and input handling logic are proven and portable. The canvas drawing
code, the flashlight hit detection, and the DOM proxy pool are all replaced
by the sidecar image pipeline.

The v0.8 code is a proof of concept. It demonstrated that the binary wire
protocol works, that drag-and-drop scheduling is the right interaction model,
and that the spatial bucketing approach is sound. Its rendering approach is
the opposite of the axiom, and it is being replaced.

---

## The Sidecar: The Brain

The sidecar is the Rust binary that runs on each CloudPC. It is the
authoritative local state machine, the renderer, the spatial oracle, and
the candidate manager.

### Responsibilities

**State management:**
- Boot: load world state from local redb cache (or sync from EC2)
- Maintain full world state in memory (HashMap<Uuid, Task/User/Service>)
- Apply mutations → validate → increment revision → persist → broadcast

**Rendering:**
- Maintain chrome layer cache (grid, labels, headers)
- Re-render on state change (chrome + tasks → image → encode → send)
- One rendering context per connected browser window
- Software 2D via tiny-skia (no GPU dependency)

**Spatial oracle:**
- Receive cursor coordinates from browser at 20Hz
- Compute candidate list via grid-cell lookup (O(1) + O(k))
- Push candidate list to browser when it changes
- On click confirmation: validate materialization

**Artifact relay:**
- Receive artifact uploads from browser
- Forward to EC2 for persistent shared storage
- Fetch artifacts from EC2 on demand

**EC2 communication:**
- Send local mutations to EC2
- Receive mutations from other users via EC2 broadcast
- Re-render affected windows when remote mutations arrive
- Resync on reconnect (revision-based delta or full snapshot)

### What the sidecar is NOT

It is not a web server. It serves no HTML, no CSS, no JavaScript. The static
frontend files are served by whatever mechanism deploys the CloudPC image
(local file, CDN, EC2 static hosting — doesn't matter, they change rarely).

It is not a database server. redb is a local save file, loaded once on boot,
flushed on mutation. Never queried at runtime.

It is not a game server in the traditional sense. There is no tick loop, no
physics, no simulation. Events arrive, state changes, images render. Between
events: silence.

---

## EC2: The Coordinator

The EC2 is deliberately thin. It does not run business logic. It does not
render. It does not compute layouts or hit test.

### Responsibilities

- Accept WebSocket connections from all sidecars
- Receive state mutations: `[user_id][event_type][payload]`
- Broadcast mutations to all other connected sidecars
- Persist canonical world state to redb
- Store and serve shared artifacts (email attachments, files, etc.)
- On sidecar reconnect: send full snapshot or event deltas since last_seen_rev
- The EC2 does NOT render for sidecars. Each user's view is unique (different
  viewport, different view mode, different visible date range). Only the local
  sidecar knows what image to produce.

---

## Drop Zones — Every Region Is a Semantic Target

When the user drops something on the browser window, the drop position carries
meaning. The sidecar interprets the coordinates based on which region they
fall in:

| Drop target | What happens |
|-------------|--------------|
| Timeline grid slot | Create/schedule task at that day + time |
| Staging panel | Create unscheduled task (inbox for later) |
| Day header | Create all-day task for that date |
| Service column | Assign dropped task to that service |
| Trash/archive zone | Complete or delete the task |

The drop zone map is maintained by the sidecar (it knows the layout because
it rendered it). The browser sends raw drop coordinates; the sidecar
determines intent from position.

This means adding new drop zones is a sidecar change, not a browser change.
The browser doesn't know what regions mean — it just reports where things land.

---

## Input Mode State Machine

The sidecar tracks user input mode. Mode gates what computation runs:

```
IDLE
  Spatial queries: active (20Hz cursor tracking)
  Canvas overlay: cursor glow only
  Background: static

HOVER (cursor near candidates)
  Spatial queries: running, candidate list fresh
  Canvas overlay: cursor glow, optional candidate highlight
  Background: static
  Pit crew: ready

DRAG (mousedown on materialized div)
  Spatial queries: SUSPENDED (candidate already known)
  Background: FROZEN (no repaints, no re-renders)
  One div: moving via CSS transform
  Canvas overlay: snap guides if desired
  VDI: minimal dirty region

RESIZE (mousedown on edge of materialized div)
  Same as DRAG

TYPING (focus in a text input — e.g., editing task title)
  Spatial queries: SUSPENDED
  Background: static
  No div materialization possible

MONTH_VIEW (Alt-M)
  Spatial queries: scoped to day-cell math only (no entity search)
  Background: month image from sidecar
  No task div materialization (read-only)

COLLAPSED (Alt-C)
  Everything suspended
  Calendar not visible, not computing
```

Modes are explicit signals, not inferred. A keydown starts TYPING. A mousedown
on a div starts DRAG. Alt-M switches to MONTH_VIEW. The sidecar receives mode
signals and gates computation accordingly.

---

## Data Model

### Task — the unit of work

```rust
struct Task {
    id: Uuid,
    title: String,
    status: TaskStatus,      // Staged | Scheduled | Active | Completed
    priority: Priority,      // Low | Medium | High | Urgent
    service_id: Uuid,
    created_by: Uuid,
    assigned_to: Option<Uuid>,
    date: Option<u16>,       // epoch days since 1970-01-01 (None = Staged)
    start_time: Option<u16>, // minutes from midnight (None = Staged)
    duration: Option<u16>,   // minutes (None = Staged)
    artifact_count: u8,      // number of attached artifacts (for render indicator)
}
```

Day-of-week derived from epoch days: `(date + 3) % 7` → 0=Mon .. 6=Sun.
The sidecar owns time. Browser never computes week boundaries.

### Service — who pays for the time

```rust
struct Service { id: Uuid, name: String }
```

12 default services. Metadata TBD when real data sources arrive.

### User — a player

```rust
struct User { id: Uuid, username: String, password_hash: String }
```

---

## Wire Protocol — WebSocket Binary

All game data over WebSocket uses fixed-stride packed binary, readable by
DataView at known offsets. JSON is never used in the data path.

### Task record (192 bytes, fixed stride)

```
[0..16]    id (UUID, 16 bytes)
[16]       status (u8: 0=Staged, 1=Scheduled, 2=Active, 3=Completed)
[17]       priority (u8: 0=Low, 1=Medium, 2=High, 3=Urgent)
[18..20]   date (u16 LE, epoch days since 1970-01-01, 0xFFFF = not scheduled)
[20..22]   start_time (u16 LE, minutes from midnight, 15-min grid)
[22..24]   duration (u16 LE, minutes, 15-min grid)
[24..40]   service_id (UUID, 16 bytes)
[40..56]   assigned_to (UUID, 16 bytes, zeroed = unassigned)
[56..184]  title (128 bytes, UTF-8, zero-padded)
[184..192] _reserved (available for artifact_count and future fields)
```

### Service record (80 bytes, fixed stride)

```
[0..16]    id (UUID, 16 bytes)
[16..80]   name (64 bytes, UTF-8, zero-padded)
```

### Server → Client messages

First byte is message type:

**State messages (existing, implemented):**
- `0x01` Snapshot: `[type][rev:u64][task_count:u32][svc_count:u32][tasks...][svcs...]`
- `0x02` TaskCreated: `[type][rev:u64][task_record:192]`
- `0x03` TaskScheduled: `[type][rev:u64][task_id:16][date:u16][start:u16][dur:u16]`
- `0x04` TaskMoved: same layout as TaskScheduled
- `0x05` TaskUnscheduled: `[type][rev:u64][task_id:16]`
- `0x06` TaskCompleted: `[type][rev:u64][task_id:16]`
- `0x07` TaskDeleted: `[type][rev:u64][task_id:16]`

**Image pipeline messages (designed, not yet implemented):**
- `0x10` BackgroundImage: `[type][rev:u64][format:u8][width:u16][height:u16][image_bytes...]`
  - format: 0=PNG, 1=JPEG, 2=raw RGBA (extensible)
- `0x11` CandidateList: `[type][count:u8][[task_id:16][x:u16][y:u16][w:u16][h:u16][color:u32][sdf_dist:i16][title_len:u8][title_bytes...]...]`
  - `sdf_dist`: signed distance in pixels (i16, negative = cursor inside element). Browser uses this for tiered proximity feedback — intersection markers, glow intensity, etc.

**Sidecar → browser feedback:**
- `0x18` SnapHint: `[type][x:u16][y:u16]` (snapped grid position during drag)
- `0x19` CursorStyle: `[type][style:u8]` (0=default, 1=pointer, 2=resize-ns, etc.)

### Client → Server commands

**Task commands (existing, implemented):**
- `0x10` CreateTask: `[type][priority:u8][service_id:16][assigned_to:16][date:u16][start:u16][dur:u16][title:UTF-8...]`
- `0x11` ScheduleTask: `[type][task_id:16][date:u16][start:u16][dur:u16]`
- `0x12` MoveTask: same layout as ScheduleTask
- `0x13` UnscheduleTask: `[type][task_id:16]`
- `0x14` CompleteTask: `[type][task_id:16]`
- `0x15` DeleteTask: `[type][task_id:16]`

**Input pipeline messages (designed, not yet implemented):**
- `0x20` CursorMove: `[type][x:u16][y:u16]` (sent at 20Hz)
- `0x21` ModeChange: `[type][mode:u8]` (IDLE=0, DRAG=1, TYPING=2, MONTH=3, COLLAPSED=4)
- `0x22` ClickAt: `[type][x:u16][y:u16]`
- `0x23` ViewportSize: `[type][width:u16][height:u16]` (sent on connect + resize)
- `0x24` VisibilityChange: `[type][visible:u8][last_seen_rev:u64]` (alt-tab resync)

**Artifact messages (designed, not yet implemented):**
- `0x30` UploadArtifact: `[type][task_id:16][filename_len:u8][filename...][blob...]`
- `0x31` RequestArtifact: `[type][task_id:16][artifact_index:u8]`
- `0x32` ArtifactData: `[type][task_id:16][filename_len:u8][filename...][blob...]` (response)

### Sync semantics

Every state event carries a revision number (u64 LE at offset 1). Browser
tracks last_seen_rev. On reconnect or alt-tab return: browser sends
last_seen_rev, sidecar sends deltas since then (or full snapshot if gap too
large).

---

## Interaction Gestures

All scheduling happens through direct manipulation. No forms. No modals.
The world IS the interface.

| Gesture | Action | Status |
|---------|--------|--------|
| Click task | Materialize, select | Canvas-era impl, to port |
| Drag task | Move to slot | Canvas-era impl, to port |
| Double-click grid | Create 30m task | Canvas-era impl, to port |
| Drag bottom edge | Resize duration | Canvas-era impl, to port |
| Alt+drag | Clone task | Canvas-era impl, to port |
| Drop from external app | Create task + attach artifact | DESIGNED |
| Drag task to external app | Export task summary | DESIGNED |
| Alt-C | Collapse calendar | DONE |
| Alt-M | Monthly view | DONE (read-only) |
| Inline title editing | Edit task title after creation | DEFERRED |
| Modifier-click | Manual time entry | DEFERRED |

"Canvas-era impl" = proven in current IRONCLAD v0.8, needs porting to the
sidecar image pipeline model. The interaction logic is validated; the
rendering approach changes.

### Snap resolution (planned)

- **Move:** snap to 30 minutes (coarse — "put it at 2pm")
- **Resize:** snap to 5 minutes (fine — "this takes 25 minutes")

Server validation needs relaxing from `% 15` to `% 5` to support this.

---

## Key Decisions

### No JSON in the data path
JSON only at the auth boundary (login). Everything else is binary packed
structs. Fast to encode, fast to decode, no parsing overhead, no ambiguity.

### redb stays
Pure Rust, single-file, ACID. Treated as a save file. Load once, flush on
mutation, never query at runtime. Both sidecar and EC2 use it.

### One div at a time
The browser never has more than one interactive DOM element at any moment.
Zero divs at rest. This is an architectural constraint, not a guideline.

### Sidecar renders, browser displays
The browser does not compute pixel positions from task data. The sidecar
renders to an image and sends it. Rendering logic lives in exactly one place.
Client-side rendering divergence is impossible.

### Browser never does hit testing
The browser forwards click coordinates. The sidecar holds the candidate list
and the browser caches it. No client-side spatial queries beyond the trivial
point-in-rect check against cached candidates.

### 20Hz cursor forwarding
Human reaction time is 100-200ms. 20Hz (50ms intervals) is sufficient for
responsive hit detection. 60Hz burns CPU and bandwidth for no perceptible
gain. The 32 FPS CloudPC cap makes >32Hz pointless anyway.

### The flash is intentional
Background swap on drop is deliberately non-seamless. It provides tactile
feedback. Do not add double-buffering or sub-frame synchronization.

### Drop creates, human titles
External drops (email, file, text) create a task and immediately open it for
editing. The dropped content becomes an artifact, not the title. Auto-titling
from email subjects is an anti-pattern for NOC operations.

### EC2 stores artifacts
Attachments go to EC2, not the sidecar. Artifacts are shared knowledge.
Other users must be able to access them without routing through the
uploader's machine.

### No protocol versioning — intentional
Server and clients are always deployed together from the same repo. No window
where mismatched versions talk to each other. Wire format changes are breaking
changes by design.

### No CRDT
Last-write-wins. 5-20 users. Two people move the same task simultaneously?
Server processes in order, second write wins. CRDT is massive over-engineering
for this scale.

### No framework, no runtime, no build step
IRONCLAD is vanilla JS. It does not need React because it does not render a
component tree. It does not need a build step because it is one file with zero
imports. It does not need a bundler because there is nothing to bundle. The
browser is glass — it displays an image and tracks a mouse. You do not need
a framework for that.

### Security deferred
Dev-mode auth bypass active. Hardcoded JWT secret. Internal tool for a known
workgroup on a VPN. Hardening is a future phase. Correctness and speed first.

---

## What's Archived

`archive/` contains:
- The Clay/WASM era: dead frontend experiment
- Old handover docs and screenshots
- `BRAINSTORM-2026-02-10.md`: pre-axiom brainstorming (many decisions now
  reversed — e.g., "No server-side rendering" is now the entire architecture)
- `analysis-sonnet-arch-review-2026-02-11.md`: stale Sonnet review from before
  the epoch-days migration and the axiom rewrite

The current `frontend/ironclad.js` (canvas rendering era, v0.8) is living code
but represents the pre-axiom implementation. The wire protocol, SoA structure,
and interaction logic are proven and portable. The canvas rendering code is not
the final form. See the "Current state" note in the IRONCLAD section above.

---

## What's Built and Working

| Component | Status | Notes |
|-----------|--------|-------|
| `world.rs` | Solid | Pure state machine, 21 tests, full lifecycle |
| `wire.rs` | Solid | Binary protocol, 12 tests, round-trip validated |
| `persist.rs` | Solid | redb ACID, 4 tests, survives format migrations |
| `game.rs` | Solid | WebSocket handler, subscribe-before-snapshot |
| `auth.rs` | Dev mode | JWT + Argon2, hardcoded secret, WS bypass |
| `renderer.rs` | Proof of concept | tiny-skia renders world to PNG, /api/render endpoint |
| `ironclad.js` | Canvas era (v0.8) | Full interaction model, wrong rendering architecture |

### What's designed but not yet implemented

| Feature | Designed in | Blocks on |
|---------|-------------|-----------|
| BackgroundImage (0x10) pipeline | This document | renderer.rs → game.rs wiring |
| CandidateList (0x11) push | This document | Spatial query in sidecar |
| CursorMove (0x20) handling | This document | New IRONCLAD |
| Viewport negotiation (0x23) | This document | New IRONCLAD + renderer resize |
| Visibility resync (0x24) | This document | New IRONCLAD |
| Cross-app DnD (in/out) | This document | New IRONCLAD |
| Artifact upload/retrieve | This document | EC2 development |
| Multi-window rendering | This document | Per-connection render contexts |
| New IRONCLAD (~200 lines) | This document | All of the above |

### What's next

The backend organs (world.rs, wire.rs, persist.rs) are keepers. The renderer
proof of concept (renderer.rs) confirms tiny-skia works. The next phase is:

1. Add new wire message types (0x10-0x24, 0x30-0x32) to wire.rs
2. Wire renderer into game.rs (state change → render → broadcast image)
3. Write new IRONCLAD from scratch (glass, not canvas)
4. Add spatial query and candidate list management to sidecar
5. Port interaction gestures from v0.8 to the new model
6. Build EC2 relay and artifact storage

---

## Philosophy

- **Everything is possible but nothing is real until the click.**
- The browser is glass. The sidecar is the brain. EC2 is the memory.
- Static image between interactions. One div during interaction.
- Not fanatical purists — optimize away what's not needed, but use what you
  need. 20 lightweight overlay elements for good feedback is fine. Zero
  elements that render world state is the actual rule.
- Performance is not a feature. On a CloudPC, performance is survival.
- VDI bandwidth is precious. Don't move pixels you don't have to.
- The cursor is the only point of contact. Everything not under the flashlight
  is provably background. This is both the constraint and the gift.
- Proximity glow is a VDI heartbeat — continuous proof of life, not cosmetics.
- The flash is feedback, not a bug. Lean into it.
- Do everything you can in math. SDFs are cheap. Intersection geometry is
  cheap. The expensive part is always the codec. Minimize dirty pixels,
  maximize visual information.
- Keyboard-first. Mouse works too.
- Drag and drop is the universal verb. In and out. Across windows and apps.
- Task titles are crafted by humans. Never auto-generated from email subjects.
- The wrong-task problem is a correctness constraint, not a cosmetic concern.
- Services are the primary axis of meaning ("who pays for the time").
- No speculative features. Build what ops teams need, not what might be needed.
- No frameworks. The browser is glass. You do not need React for glass.
- Binary everything. JSON only at auth boundary.
- Security later. Correctness and speed now.
- Everything in this document is hypothesis until tested on real CloudPC
  hardware. If you prove something wrong, that's a win.
