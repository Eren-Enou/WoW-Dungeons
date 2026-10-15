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
speed. Stuck detection: an AI actor that makes (almost) no
headway for 1 s, or stands at a waypoint while its reachable goal is still far,
steers to the current portal (or surface) centre for 0.8 s; it never teleports.
Dithering (two direction reversals within ~0.7 s, i.e. two AI goals fighting
over a boundary) commits the actor to its previous goal for 0.75 s, except
when it or that goal is in a ground effect. Stuck time, unreachable-goal time
and dithers are reported by the headless runner.

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

## 4. Where it lives

| Piece | File |
|---|---|
| `V3`, horizontal `V2` | `crates/sim/src/math.rs` |
| Surfaces, links, validation, routing, movement, line of sight | `crates/sim/src/nav.rs` |
| Schema (`areas` fields, `links`, `nav`, `lava`, optional `y`), `Content::place` | `crates/sim/src/content.rs` |
| Actor `y` / `surface` / `NavState`, `reach`, `has_los`, `in_effect`, steering, falls, stuck and dither handling, aggro / leash / pursuit | `crates/sim/src/sim.rs` |
| AI goals in 3D, danger zones per floor, line-of-sight-aware positioning | `crates/sim/src/ai.rs` |
| Level geometry built from the `Nav` (floors, rock, decks, ramps, stairs, walls, drop strips, lava) | `crates/game/src/dungeon.rs` (`spawn_level_geometry`) |
| Camera under ceilings | `crates/game/src/camera.rs` (`ceiling_pitch`) |
| Debug overlay (N / Shift+N) | `crates/game/src/navdebug.rs` |
| Test dungeon | `assets/content/dungeons/05_cavern_traverse.toml` |

## 5. Verification

All numbers below come from commands run on 2026-10-14 (Windows 11, release
build). "nav stuck / unreachable / dithers" are per-run totals summed over the
party.

### Tests
* `nav.rs` (17 tests):
  * stacked floors don't connect;
  * solid-over-floor and too-close-vertically layouts are rejected, with the fix in the message;
  * ramps work both ways, with interpolated height and hand-overs within a step;
  * too-steep ramps are rejected;
  * a one-way drop goes down but not up, and edges block everywhere else;
  * an unreachable platform is reported;
  * the shorter of two routes is chosen;
  * a winding corridor is followed with no direction reversals;
  * thin boundaries are crossed exactly once;
  * walls block movement and sight;
  * floors block sight between levels, and rock under a ledge hides the floor behind it;
  * walls open at connections and drop zones, and follow ramps;
  * projection prefers the nearby level;
  * bad links give actionable errors;
  * routing is deterministic.
* Simulation (`tests.rs` / `content.rs`):
  * legacy dungeons stay flat and fully connected;
  * Cavern areas connect by height, not by overlap;
  * no melee, casts or aggro through the bridge;
  * walls block casts;
  * rings stay on their floor;
  * the drop falls at 12 m/s and is one-way;
  * enemies give up on targets they can neither reach nor see;
  * full autopilot traversals of the Cavern with 5 members (3 seeds) and 10 members (2 seeds). They check **every tick** that nobody leaves the floor, moves faster than they can run (plus a drop landing), rises more than a step or falls faster than 12 m/s;
  * deterministic replay;
  * the Cavern round-trips through TOML;
  * ambiguous or wrong elevations are reported.
* Camera: `camera_dips_under_a_bridge_instead_of_entering_the_player`.

### Cavern Traverse (headless, Balanced unless noted)

| group | seeds | wins | deaths | nav stuck | unreachable | dithers |
|---|---|---|---|---|---|---|
| 5, level 2 | 100 | 100% | 0.00 | 0.0 s | 0.0 s | 7.3 |
| 10, level 2 | 100 | 100% | 0.00 | 0.0 s | 0.0 s | 9.5 |
| 25, level 4 | 20 | 100% | 0.00 | 0.0 s | 0.7 s | 7.8 |
| 40, level 6 | 10 | 100% | 0.00 | 0.0 s | 3.0 s | 4.2 |
| 5, Aggressive | 100 | 100% | - | 0.0 s | 0.0 s | 3.7 |
| 5, Conservative | 100 | 100% | - | 0.0 s | 0.0 s | 9.1 |
| 5, Heroic, level 4 | 50 | 100% | - | 0.0 s | 0.0 s | 8.9 |

Deterministic replay: OK in every batch.

### Existing dungeons, 0.3 vs 0.4 (Balanced, same seeds)

| dungeon | 0.3 | 0.4 |
|---|---|---|
| Sunken Vault | 100% (30 seeds) | 100% (100 seeds) |
| Gloomroot Warrens | 97% (100 seeds) | 99% (100 seeds) |
| Ashen Spire | 98% (300 seeds) | 97% (100 seeds), 97% (300 seeds) during tuning |
| Cinder Court (10) | 100% (20 seeds) | 100% (30 seeds) |

Line of sight changed the old dungeons: before 0.4, heals and spells went
through walls. With the first 0.4 AI the Ashen Spire fell to 94% (300 seeds).
An ablation run showed the whole gap was line of sight: with it disabled,
0.4 cleared 299/300.

The fixes changed AI positioning, not the rules:
* ranged spots must see the enemy and be seen by the healer;
* ranged DPS move back into the healer's sight before casting;
* healers move instead of smiting when the tank is out of sight;
* a healer now keeps one consistent goal near corners. Before, she flipped between two goals every tick at a sight boundary.

### Benchmarks (release, `--bench`, ms per 20 Hz tick)

| group | Warrens L6, 0.3 | Warrens L6, 0.4 | Cavern, 0.4 |
|---|---|---|---|
| 5 | 0.003 | 0.004 | 0.005 |
| 10 | 0.005 | 0.007 | 0.010 |
| 25 | 0.013 | 0.022 | 0.028 |
| 40 | 0.023 | 0.049 | 0.059 |

0.4 costs 1.3-2.1x the 0.3 tick and is still ~1,000x real time at 40
characters. The extra cost comes from the AI's line-of-sight questions, which
grow with group size (healers check everyone), and from per-tick route
queries.
* A bounding-box reject made line of sight about 20% cheaper.
* A per-tick sight cache was measured (13% fewer calls, no measurable time saved) and dropped.
* Routing tables are built once at load (a few hundred links at most).

### Graphical
`--demo --dungeon cavern_traverse --nav-debug --zoom 30 --screenshots N`
produced screenshots of the elevated walkway, the fight on the rope bridge,
the underpass with the bridge overhead, the lava hollow and the basin. The
overlay showed surfaces, portals and labels. The screenshots exposed a camera
collapse under the hanging gallery, fixed by `ceiling_pitch`. The 0.3 checks
(`--input-test`, `--frame-test`, `--tooltip-test`, `--click-test`,
`--hub-tour`, `--editor-test`) still pass.

## 6. Schema compatibility

* Old content needs no changes and means the same thing. Every new field is
  optional; the defaults are height 0, solid, walled and reachable.
* The editor writes the new fields only when they differ from the defaults.
* A 0.3 build cannot read files that use the new fields; it rejects them with
  "unknown field".
* Saves are unaffected (positions are not saved).

## 7. Known limitations

* Footprints are axis-aligned rectangles. Curves and diagonal passages are
  built from several rectangles. Ramps run only along x or z.
* Two ramps that overlap with slopes in different directions connect only if
  they are level across the whole overlap.
* No actor-actor collision; a crowd can stand inside each other in a doorway.
* Walls are thin faces at footprint edges. There is no separate ceiling, so
  sight upward is blocked only by other surfaces' volumes. Bodies never block
  sight.
* The drop landing spot is the step clamped onto the lower floor (up to about
  1.1 m sideways). There is no jumping, climbing, swimming or knock-back.
* Dither commitment hides conflicts between AI goals rather than resolving
  them. The old dungeons still record 7-175 dithers per run, with no
  measurable harm. Cinder Court is highest: 10 members plus tank swaps.
* At 25-40 members, a few seconds of unreachable goals occur per run, summed
  over the group. These are formation spots projected next to the leader; the
  fallback (stay with the leader) handles them.
* Enemies can follow the party down a one-way drop. If their home is above,
  they settle where they stand when they reset.
* Rendering:
  * Ramps are a smooth deck over rock steps, and walled ramps get stepped 1 m wall pieces.
  * The demo camera forces its yaw to the character's facing. In very narrow passages the camera ends up close and the player's own model hides.

## 8. Extension points (toward a Ragefire-style cave)

* **Polygons**: `Surface` hides its footprint behind `walk.contains`, `clamp`,
  `clip` and `height_at`. Convex polygons could replace rectangles without
  touching routing or the sim.
* **More link kinds**: `LinkKind` and `Nav::attempt` are the places to add
  ledges you can only jump down from (with a landing arc), one-way doors, or
  ladders (a link with a fixed traversal time).
* **Spatial hash**: if big caves have many walls, bucket walls and surfaces in a
  grid inside `line_of_sight`.
* **Crowd separation**: a soft separation term in `Sim::steer` would spread
  groups in doorways without changing routing.
* **Authoring**: the overlay already shows names and heights. An editor mode
  could drag `min`/`max` and reuse `Nav::build` for live validation.
