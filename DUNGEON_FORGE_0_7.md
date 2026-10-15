# Dungeon Forge 0.7 — meaningful variety

Goal of 0.7: changing a blueprint should change the **experience** of a
dungeon (its spatial challenges, enemy behaviour, exploration decisions,
atmosphere), not only rearrange a familiar set of rooms.

This file starts with the architecture review and plan written before
implementation (Phase A). Results are appended as phases land.

## Phase A — architecture review of 0.6

What 0.6 does well, and keeps:
* the pipeline: blueprint + kit + seed → plan → compose → populate →
  the game's own loader → structure and encounter-space checks → simulation;
* determinism (byte-identical output per seed) and bounded attempts with a
  diagnosis;
* components defined once in a local frame and placed by rotation and
  translation; the structural check that the navigation matches the plan;
* the game and simulation stay independent of the forge.

Obstacles to different *experiences*:

| # | Obstacle | Where | Effect |
|---|---|---|---|
| 1 | Recipes are mob lists with four placement words and one fixed anchor (30% of the room toward the far wall) | `kit.rs`, `populate.rs` | every pack stands in the same relative spot; no idea of exits, hazards, sightlines or formation shape |
| 2 | No enemy behaviour beyond fixed templates: no patrols, no mechanic that uses the room | `dungeon_sim` | rooms are backdrops; positioning only matters for frontals and rings |
| 3 | The component vocabulary is volcanic and hard-coded: pit rooms, lava under every bridge, corner rock columns, room sizes in code | `layout.rs::new_room`, `compose.rs` | a second theme could only be a recolour |
| 4 | One topology style: a self-avoiding walk with fixed weights, rooms centred in 36 m cells, near-square rooms | `layout.rs` | every layout reads as "rooms on a grid" |
| 5 | Blueprints can't state intent ("this dungeon is about interrupts and line of sight") | `blueprint.rs` | nothing checks that a dungeon delivers what it promises |
| 6 | Simulation reports are aggregates per configuration | `simtest.rs` | can't see which encounter kills, or which room makes companions dither |
| 7 | Regenerating overwrites designer edits | `build.rs` | generated content can't be hand-finished |

## Plan

Smallest set of extensions that makes two dungeons from the same system feel
designed for different experiences:

### B. Encounter Recipes 2.0 and placement
* Recipes gain:
  * `tags`: the tactical challenges an encounter poses (`interrupts`,
    `frontal`, `protect_caster`, `patrol`, `area_control`, `summons`,
    `council`, `line_of_sight`, `multi_phase`);
  * `space` (minimum room size, a ranged lane, pillars wanted);
  * `placement` (`far`, `guard_exit`, `center`, `patrol`);
  * `formation` (`line`, `wedge`, `ring`, `screen`) for how the
    front/back/flank roles are laid out.
* A **placement scorer** (`placement.rs`) samples candidate anchors on a
  lattice over the room's floor. Candidates are rejected if any member would
  be off the floor, too close to the entry (no staging), or the room has no
  ranged position with a sightline when the recipe needs one. The rest are
  scored on the preference, the entry direction, distance from other exits
  and hazards, and room to escape rings. The best wins, deterministically.
* **Patrols** (new simulation capability, default off): a mob spawn may list
  `patrol` points. While idle it walks the route back and forth at walking
  pace, and aggro checks use where it is. Pulled, it fights from there; reset,
  it walks home and resumes. The forge places a patrol route along the room
  and validates every leg as a navigation route.
* **Line-of-sight mechanics** (new, default off): a `pulse` with
  `los_blockable = true` only hits party members who can see the caster. AI
  companions hide behind cover (pillars, corners) while it is cast. This makes
  pillared rooms tactical, not decorative.

### C. Second kit: the Hollow Reliquary (crypt)
A buried monastery of the Hushed (original): axial processional halls, burial
side chambers, pillared halls where pillars are **real obstacles** (holes in
the walkable floor walled in, so they block movement and sight), narrow
doorways, stairs down, cold candle light, a council of abbots and an enclosed
guardian chamber with a line-of-sight mechanic. It uses kit-level component
definitions, not a separate generator:
* the kit declares room shapes and proportions (rectangular naves, pillar
  grids, irregular caverns), corridor widths, chasm type under bridges
  (`lava` or `void`), corner decoration, light rig and props;
* the composer builds a pillared room as a rectangle with holes, decomposed
  into overlapping convex pieces (3 m overlaps, the 0.6 lesson).

### D. Spatial variety
* Rooms get per-side extents and per-corner cuts. This gives non-square,
  off-centre and irregular silhouettes while keeping straight walls where
  corridors join, and a per-side range where a corridor may attach.
* The kit sets the grid pitch (the crypt is tighter than volcanic caverns).
* Layout bias from the blueprint: `straightness` (axial vs winding),
  `side_rooms` (explorable dead ends), `alternate_routes` (count) and
  `passages` (bend nodes that stretch corridors into longer winding routes).
* Not done, and why: free placement off the grid (it loses the overlap
  guarantee the structural check relies on) and diagonal cell links
  (corridor crossings). Rotated or angled pieces inside cells give much of
  the look for little risk.

### E. AI and diagnostics
* Reproduce known problems on generated layouts (melee convergence,
  cautious-melee dithering, frontals at the pull) and fix measured causes.
  Compare before and after on handwritten and generated dungeons.
* `dungeon-forge report`:
  * per-encounter attempts, wipes, deaths and time;
  * per-room stuck and dither counts (from a navigation-event log the
    simulation keeps outside the event log, so replay hashes don't change);
  * optional encounters completed;
  * duration by layout.

  It writes JSON and an SVG overlay of trouble spots.

### F. Content, verification, docs
* An improved Emberforge (recipes 2.0, patrols, irregular caverns) and the
  Hollow Reliquary.
* Layouts compared side by side.
* The full regression matrix.
* Screenshots of each kit.
* Package.

Designer edits: a small proof of concept if time allows, as an overrides file
applied after generation and keyed by stable ids (encounter ids, room names).
Otherwise the design is documented.

### Constraints carried forward
* Handwritten dungeons keep working.
* New simulation behaviour is opt-in through data, so handwritten content
  keeps identical event-log hashes unless a fix is intended and measured.
* No balance work beyond keeping required encounters completable. Simulated
  win rates are a feasibility check, not a measure of fun.

### A note on playtesting
The 0.3–0.6 documents say no human had played those versions. That is wrong:
earlier versions have been playtested by the maintainer (no written notes are
in the repository). From 0.7 on, the docs say "not covered by a recorded human
playtest" where that is what is meant, and keep automated verification
separate from human evaluation.
