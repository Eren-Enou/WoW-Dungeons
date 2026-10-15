# Navigation 0.4 — layered 2.5D navigation and spatial combat

This document records the review of the 0.3 navigation, the design chosen for
0.4, how it is wired into the simulation, renderer and AI, and its measured
cost and known limits.

## 1. The 0.3 architecture and its limits (review before changes)

0.3 navigation lived in `crates/sim/src/nav.rs` (326 lines):

* **Positions** are `V2 { x, z }` (`math.rs`, documented "the sim never needs
  height"). `Actor::pos`, `move_goal`, telegraph centres, hazard and mob
  positions, the dungeon entrance and every AI helper are 2D.
* **Walkable space** is a list of overlapping axis-aligned rectangles
  (`dungeon.areas`). Two rectangles are connected *whenever they overlap*;
  routing is a breadth-first search over that rectangle graph, steering toward
  the centre of the next overlap.
* **Movement** (`try_move`) accepts any step that lands in *any* rectangle and
  otherwise slides along x or z. Walls are the outline of the union of all
  rectangles and exist only for rendering and the camera.

Consequences, by area:

| Area | 0.3 behaviour | Why it blocks vertical dungeons |
|---|---|---|
| Player / AI movement | membership in *any* rectangle | a bridge and the passage under it would merge: walking "into" the bridge from below is legal |
| Routing | BFS, unweighted, overlap = connection | no costs (a long detour equals a short one), no one-way links, a stacked floor is connected just by overlapping |
| Content | `areas = [{name, min, max}]` | no height, no ramps, no drops, no way to forbid a connection |
| Targeting / range | `reach` = horizontal distance | a mob on a floor 4 m below is "in melee range" |
| Line of sight | none ("walls do not block line of sight") | casts and heals go through walls and floors |
| Aggro / leash | horizontal radius | a pack below a bridge aggroes the party walking overhead |
| Encounter placement | `[x, z]` | cannot say *which* floor a mob stands on |
| Ground effects | horizontal circle test | a ring on a platform hits the people underneath |
| Camera | collision against wall boxes only | no floors overhead to collide with; focus always at y = 0 |
| Rendering | floor slabs at y = 0, walls from union outline | no platforms, ramps, bridges, drops |
| Determinism / headless | fine (pure 2D maths on a fixed step) | must be preserved |

The engine never needed height because nothing could be above anything else.
Adding a visual Y coordinate would leave all of the rows above wrong.

## 2. Options considered

1. **Full 3D navmesh** (Recast-style voxelisation, polygon mesh, funnel
   algorithm). Correct and general, but a large amount of code or a heavy
   dependency, slow iteration on hand-authored TOML levels, and much more
   than graybox caverns need.
2. **Physics engine** (rapier) for movement and raycasts. Brings a large
   dependency into the deterministic sim crate, floating-point behaviour we
   don't control across versions, and still needs a navigation layer on top.
3. **Layered 2.5D surfaces with explicit links** (chosen). Each walkable
   surface is a convex axis-aligned footprint with a height function (flat or
   a linear ramp). Surfaces connect only through *validated* portals: same
   height where they touch (auto), or an explicit one-way drop. Routing is a
   weighted shortest path over portals, precomputed at load.

Option 3 keeps the existing rectangle authoring model (old maps are simply
surfaces at height 0), stays tiny and deterministic, is easy to validate with
actionable errors, and supports everything 0.4 asks for: floors at different
heights, ramps and stairs, bridges, stacked platforms, passages under
passages, drops, unreachable platforms and alternative routes. What it cannot
do (arbitrary polygons, overhangs on the same footprint without separate
surfaces, jumping, climbing) is listed in section 7.

## 3. Design

### World positions

* `Actor` keeps `pos: V2` as the **horizontal** component (facing, steering,
  formation offsets stay planar) and gains `y: f32` (feet elevation) and
  `surface: SurfaceId` (the surface it stands on). `y` is authoritative and
  maintained by the navigation each tick; `Actor::loc() -> V3` gives the full
  world position.
* `math.rs` gains `V3 { x, y, z }` (`xz()`, `at(V2, y)`, distances).
* AI movement goals become `V3`, so a goal says *which* floor it is on.

### Surfaces

`dungeon.areas` entries gain optional fields (defaults reproduce 0.3 exactly):

| field | default | meaning |
|---|---|---|
| `y` | 0 | floor height (height at the `min` edge for a ramp) |
| `y_to` | — | makes the area a ramp rising/falling to `y_to` at the `max` edge |
| `axis` | `"z"` | ramp direction |
| `stairs` | false | draw a ramp as steps (navigation identical) |
| `walls` | true | outline gets walls (bridges and ledges use `false`) |
| `under` | `"solid"` | `"open"` = thin slab you can walk/see under (bridges) |

Validation: finite values, slope ≤ `max_slope`, unique names, and every pair of
horizontally overlapping surfaces must be either *connected* (same height
where they overlap), *stacked* (≥ `headroom` apart with the upper one `open`),
or a *ramp foot/head* (connected over a short strip at the ramp's end).
Anything else is rejected with a message naming both areas and the fix.

### Connections

* **Walk links (bidirectional, automatic)**: two surfaces connect only where
  their shrunk footprints overlap *and* their heights agree within
  `max_step` (0.5 m). For a ramp the portal is the strip of the overlap where
  that holds. Overlap alone never connects anything.
* **Drop links (one-way, explicit)** in `[[dungeon.links]]`
  (`from`, `to`, `kind = "drop"`, `min`, `max` zone): walking off the upper
  surface's edge inside the zone lands you on the lower surface and you fall
  (finite fall speed; no instant change of height). Height difference must be
  between `max_step` and `max_drop`.
* **Unreachable platforms** are simply surfaces without a valid link.

### Routing

Nodes are directed portals. Costs are 3D distances between portal points on
the shared surface (plus the drop distance and a small penalty for drops).
All-pairs shortest paths (Floyd–Warshall with a next-hop table) are computed
once at load; a query costs `O(out-portals of start × in-portals of goal)`, with
no allocation. Steering aims at the point of the next portal nearest to the
following waypoint (a cheap funnel), so paths hug inside corners less and do
not zig-zag between portal centres.

### Movement

A step stays on the current surface if it lands inside it. Leaving it is only
possible through a link whose portal the step crosses (walk) or inside a drop
zone (drop). Otherwise the actor slides along the edge. Elevation follows the
surface height (ramps interpolate); after a drop the actor falls at a finite
speed. Stuck detection: an AI actor with a goal that makes no progress for
1.5 s steers to the current portal centre for a second (no teleporting); the
time spent stuck is recorded in the metrics.

### Spatial combat

* **Range** (`Sim::reach`): horizontal edge-to-edge distance combined with the
  vertical gap beyond a 1 m body allowance — a ceiling between two actors puts
  them out of melee range.
* **Line of sight** (`Sim::has_los`): eye-to-body segment against wall faces
  (from their floor up to their wall height) and against every surface's solid
  volume (thin slab for `open`, rock down to the lowest floor for `solid`).
  Needed for abilities with a target other than yourself and for auto attacks
  (new `CastError::NoLineOfSight`).
* **Ground effects** (telegraph rings, hazards, cleaves, AoE): horizontal
  radius *and* within 1.5 m of the effect's elevation.
* **Aggro**: within radius on the same level, or within radius and in sight
  across levels. Leash distance is 3D.
* **AI**: a cast without line of sight turns into "move toward the target"
  (along the path); enemies skip party members they can neither reach nor
  see, and evade if nobody is left.

### Rendering and tools

The renderer builds floors, ramps (sloped slabs or steps), bridges, rock
supports, walls (following ramps), drop-zone markers and lava from the same
`Nav` the simulation uses, so the two cannot disagree. Floors and supports are
camera blockers. A toggleable debug overlay (`N`) draws surfaces coloured by
height with labels, portals and drop arrows, each companion's path and current
waypoint, unreachable goals and stuck actors.
