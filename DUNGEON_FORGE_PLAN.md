# Dungeon Forge — architecture review and plan

Dungeon Forge is a reusable pipeline for making original dungeons:
blueprint → layout → population → validation → simulated playtest →
playable TOML. It is an *additional* way to create content. Handwritten
dungeon files keep working and remain first-class.

This document was written before implementation (0.6 Phase A). What was
actually built and measured is in [DUNGEON_FORGE_0_6.md](DUNGEON_FORGE_0_6.md).

## 1. How a dungeon is made today (review of 0.5)

One TOML file (`assets/content/dungeons/NN_<id>.toml`, schema in
`crates/sim/src/content.rs`) holds everything:

* `[dungeon]` with metadata, entrance and leash radius;
* `[[dungeon.areas]]`: walkable convex polygons with heights and ramps;
* `[[dungeon.links]]`: one-way drops;
* `lava`, `decor`, `lights` and the `theme`;
* `[[difficulties]]`;
* `[templates.*]`: enemy stats, abilities and phases;
* `[[encounters]]`: mobs with world positions, and hazards;
* `[[loot]]` and `[rewards]`.

Loading deserialises the file, then `Content::validate` checks the schema,
references and ranges, and `build_nav` checks geometry. Geometry checks cover
overlaps that must be level, ramp overlap of 2 m or less, slope 0.6 or less,
headroom, reachability, drop zones, decor off walkable floor, and placement
of mobs and hazards. The game and the headless runner then use the same
`Content`.

To build The Cinderdeep Hollows (1252 lines) I had to:

1. Sketch regions on paper, then compute every polygon by hand in a Python
   helper (`area()`, `strip()`, `ramp()` with landings, `octagon()`).
2. Fix overlaps by trial and error against the validator. The angled ramp
   overlaps, a solid ledge over a floor, unreachable pieces and decor on a
   causeway each cost a cycle.
3. Hand-place every mob, hazard, decor piece and light in world coordinates,
   adding `y` where floors stack.
4. Find gameplay problems only by running batches: a healer spot pulling the
   next pack mid-boss, a healer pinned on a diagonal ramp, a 12 s melee
   shuttle on the bridge, a 10-player timeout on a slanted portal edge.
5. Tune encounter spacing (aggro radius against distance) by moving numbers
   in the file and re-running.

What was repetitive, error-prone or layout-bound:

| Pain | Why | Forge answer |
|---|---|---|
| Coordinates for every vertex | geometry, connections and naming are one thing in the file | rooms and corridors are **components** placed on a grid; vertices are derived |
| Ramp and landing overlap rules | easy to get a 2.1 m overlap or a 0.6 slope | corridor components produce valid ramps by construction (slope at most 0.3, overlap 1.5 m) |
| Mob positions in world space | they move whenever a room moves | packs are placed relative to the room and its entry direction by **recipes** |
| Pull distances | only discovered in simulation | the builder checks aggro radius against pack spacing and the entry corridor |
| Duplicate templates across dungeons | each file carries its own bestiary | a **kit** (theme library) holds templates, recipes, loot and palette |
| No statement of intent | the file says *where*, not *why* | a **blueprint** says what experience is wanted; the builder chooses where |
| Validation stops at "it loads" | AI problems show up later | tiered validation: schema → nav → structure → encounter space → simulation |

## 2. Principle

The core engine (simulation, navigation, renderer) does not change to make a
new dungeon. Dungeon Forge produces the same `Content` the game already
loads, through the same validation. Nothing in `dungeon_sim` or the game
depends on the forge.

## 3. Architecture

A new crate, `crates/forge` (`dungeon_forge`, binary `dungeon-forge`),
depends on `dungeon_sim` only. It is Bevy-free, deterministic and testable.

```
blueprint.toml ─┐            kit.toml (theme library: templates, recipes, loot, palette)
                ▼                 │
        ┌──────────────┐          │
        │ 1 Topology   │  graph: rooms with roles, edges with kinds (walk, ramp, bridge, drop, loop)
        └──────┬───────┘          │
               ▼                  │
        ┌──────────────┐          │
        │ 2 Embedding  │  grid cells, bounded seeded search, heights
        └──────┬───────┘          │
               ▼                  │
        ┌──────────────┐          │
        │ 3 Composition│  components → areas, links, lava, decor, lights, entrance
        └──────┬───────┘          │
               ▼                  ▼
        ┌──────────────────────────────┐
        │ 4 Population                 │  recipes → packs, bosses, hazards; loot; rewards
        └──────┬───────────────────────┘
               ▼
        ┌──────────────┐
        │ 5 Validation │  schema+nav (the game's own) → structure → encounter space
        └──────┬───────┘
               ▼
   generated .toml  +  report (topology, rooms, checks)  +  map (SVG/PNG)
               ▼
        ┌──────────────┐
        │ 6 Sim tests  │  headless batches, failure records (blueprint, seed, nav state)
        └──────────────┘
```

Modules: `blueprint` (schema and validation), `kit` (theme library),
`component` (room and corridor definitions, transforms), `topology`,
`layout` (grid embedding and heights), `compose` (geometry),
`populate` (encounters, loot), `check` (structural and encounter checks),
`simtest` (gameplay validation), `map` (overview rendering), `cli`.

### Layer A — blueprint (intent, no coordinates)

```toml
[blueprint]
id = "emberforge_vaults"
name = "The Emberforge Vaults"
kit = "volcanic"          # assets/forge/kits/volcanic.toml
level = 3
group_size = 5

[layout]
extra_rooms = [1, 2]      # empty rooms on the main route (min, max)
descents = 2              # elevation steps down along the main route
alternate_route = true    # a second way between two main-route rooms
optional_rooms = 1        # dead-end side chambers (optional encounters)
shortcut = "drop"         # none | drop (one-way overlook) | loop (walkable return)
bridges = 1               # main-route crossings over lava
winding = 0.4             # share of corridors that wind or angle

[[encounters]]            # in route order; the last must be a final boss
recipe = "melee_pack"
[[encounters]]
recipe = "guarded_caster"
...
[[optional]]
recipe = "optional_elite"
```

### Layer B — spatial composition (components)

Rooms sit at the centres of cells on a grid (36 m pitch). Corridors fill the
gaps between neighbouring cells. Because rooms stay inside their cells and
corridors stay inside the gaps (lateral offset + half width ≤ room half-size
− 2 m), no two pieces can overlap except where they are meant to join. Every
join is a 1.5 m level overlap, or a ramp end overlapping by 1.5 m.

Room components:

| Component | Shape | Use |
|---|---|---|
| entrance | square, 7–8 m half-size | start; the entrance point is placed away from the exits |
| chamber | square or octagon, 7–10 m | packs, junctions, forks |
| hall | octagon, 10–12 m | larger packs |
| arena | octagon, 13–14 m | bosses (room for rings and spreading) |
| pit room | four strips around a central lava pit | hazard encounters; lava-side bosses |
| overlook | small ledge room | the top of a one-way drop shortcut |
| side chamber | small square | optional encounters off the route |

Corridor components (between two adjacent cells, along the cell axis):

| Component | Geometry | Notes |
|---|---|---|
| straight | one rectangle | width 4–6 m |
| winding | three rectangles (S-jog) | the exit is offset sideways |
| angled | one convex parallelogram | uses polygon navigation |
| ramp | one rectangular ramp (optionally drawn as stairs) | dh up to 4.5 m, slope ≤ 0.3 |
| bridge | open deck, no walls, lava 6 m below | a narrow crossing |
| drop | elevated walkway abutting the lower room, plus a drop link | one-way |

Components are defined once in a local frame ("+z is the exit direction")
and transformed by a 90° rotation, a translation and a height offset. All
four orientations reuse the same code; for 90° rotations rectangles stay
axis-aligned, so they keep the exact, cheap rectangle fast paths in `Nav`.
Connection points are the midpoints of the room sides, shifted sideways by
the corridor's offset.

### Layer C — population

A **kit** (`assets/forge/kits/<theme>.toml`) holds:

* enemy templates (the normal schema), authored at a reference level;
* **recipes**: named pack compositions with placement roles (`front`,
  `back`, `flank`, `center`), required room size and preferred room
  component, optional hazards;
* loot and a palette (theme, tints, light colours, decor mix).

The builder:
* scales template HP and damage from the kit's reference level to the
  blueprint level by the same factor party damage scales (×(1 + 0.1·(L−1)));
* places packs on the far side of the room from the entry, so the party can
  stage in the corridor;
* checks pull safety;
* adds lights and decor anchors from the palette;
* picks loot sources per encounter.

## 4. Generation (deterministic, bounded)

1. **Topology**:
   * Main route: entrance, then the blueprint encounters in order, with
     `extra_rooms` empty rooms inserted at seeded positions (never between a
     boss and its approach).
   * Boss recipes require an arena; hazard recipes require a pit room.
2. **Embedding**:
   * A seeded self-avoiding walk places the main route on the grid, with a
     forward bias.
   * Then features attach to free cells:
     * the alternate route: BFS through free cells between two main-route
       rooms 2–3 apart;
     * optional side rooms;
     * the shortcut: an overlook cell adjacent to an earlier room and to a
       later, lower room, or a loop between non-consecutive adjacent rooms.
   * A failed attempt is retried with the next sub-seed, at most
     `MAX_ATTEMPTS` (64) times. If every attempt fails, the error lists which
     constraint failed how often, e.g. "alternate route: no free path in
     41/64 attempts; try fewer rooms or alternate_route = false".
3. **Heights**: `descents` main-route edges step down 2.5–4 m (ramps). The
   alternate route and side rooms take heights that keep every edge within
   ramp limits. The drop shortcut needs a fall of 1–8 m.
4. **Composition**: every node becomes a room component and every edge a
   corridor component.
5. **Population** from recipes; loot and rewards from the kit.
6. **Serialisation** to TOML with a header comment naming the blueprint,
   seed and generator version. The text is then **parsed back through
   `Content::from_toml_str`**, the game's own loader. Generation succeeds
   only if that passes.

Determinism: the only randomness is `dungeon_sim::rng::Rng` (SplitMix64)
seeded from the blueprint seed and the attempt index. All collections that
reach the output are ordered (Vec, BTreeMap). A test checks that two runs give
byte-identical TOML.

## 5. Validation tiers

1. **Schema and navigation**: the normal loader (`Content::from_toml_str`).
2. **Structure** (forge):
   * every graph edge exists in the navigation as authored: walk edges as
     walk links between the two rooms' pieces, drops as one-way drop links;
   * **no walk link joins pieces of rooms that the graph doesn't connect**
     (no accidental shortcuts through stacked floors or touching corridors);
   * every mandatory room is reachable from the entrance;
   * the final arena is only reachable through the main route.
3. **Encounter space** (forge):
   * every mob stands on its room's floor;
   * the pack's aggro radius doesn't reach the entry corridor's mouth (room to
     stage);
   * packs don't overlap in aggro (pulling one doesn't pull another);
   * a boss with rings has an arena at least 2.5× the ring radius;
   * ranged spots with line of sight exist (the room is open).
4. **Simulation** (`dungeon-forge test`): headless batches over party sizes,
   profiles, classes and difficulties. Failures (timeout, wipe, stuck >
   threshold, unreachable time, dither loops) are recorded as JSON with the
   blueprint, generator seed, sim seed, party, the encounter reached and every
   stuck actor's position, surface and goal, so they can be reproduced.

## 6. Developer tools

```
dungeon-forge validate <blueprint>                 # blueprint + kit checks
dungeon-forge generate <blueprint> --seed N [--out FILE] [--map FILE.svg|.png]
dungeon-forge inspect  <dungeon.toml>              # topology summary of any dungeon file
dungeon-forge test     <dungeon.toml|blueprint> [--seed N] --runs 30 [--sizes 5,10] [--profiles ...]
dungeon-forge seeds    <blueprint> --from 1 --count 20   # generate many, summarise variety and failures
```

The map is a top-down overview: rooms coloured by height, corridors, drops as
arrows, packs and bosses as dots, and the main route traced. `inspect` works
on handwritten dungeons too (areas, links, encounters, heights). The game's
`--area-tour` and `--nav-debug` take screenshots of generated dungeons like
any other.

## 7. Asset pipeline (0.6 research, 0.8 implementation)

* Presentation stays separate from navigation. A visual prefab never changes
  walkability, which is still owned by areas.
* Proof of concept: `[[dungeon.props]]` places a glTF/GLB model (`model`,
  `pos`, `y`, `yaw`, `scale`) with a primitive fallback (`fallback =
  "pillar"`) and a footprint radius that the validator keeps off walkable
  floor, like decor. The models are original (generated by a script in the
  repo) and their provenance is recorded.
* Benilla (a Rust/Bevy WoW 1.12 client) will be reviewed for architecture
  lessons only: loaders separate from the world renderer, collision
  structures, material handling. No Blizzard data, no client requirement, and
  no code copied without license review. Report: ASSET_PIPELINE_RESEARCH.md.

## 8. Roadmap

* **0.6 (now)**:
  * blueprint and kit formats;
  * component library;
  * deterministic generator with bounded retries and diagnostics;
  * structural and encounter-space validation;
  * CLI (validate, generate, inspect, test, seeds) and the map;
  * one generated dungeon in the game and two more sample seeds;
  * GLB props proof of concept;
  * Benilla research.
* **0.7**:
  * richer encounter recipes (council, patrols, interrupt-heavy, space-aware
    placement);
  * a difficulty budget calibrated by simulation;
  * AI diagnostics as a forge report;
  * persistent positioning problems (melee convergence).
* **0.8**:
  * themes beyond volcanic;
  * modular original GLB environment pieces per component;
  * prop anchors;
  * lighting per theme;
  * packaging of licensed assets.
* **Later**:
  * a graphical Dungeon Design Studio on top of the same data: blueprint
    editing, component placement, nav preview, "run 30 clears" buttons, export
    and package.

## 9. Acceptance (0.6)

1. A readable blueprint describes a small original dungeon without coordinates.
2. A deterministic builder turns it into a valid dungeon file.
3. Components support branching, elevation change and a boss arena.
4. Several seeds give different, reproducible layouts.
5. The output passes content and navigation validation.
6. The generated dungeon loads in the game.
7. 5-player AI parties traverse and attempt it.
8. Failed generation gives actionable diagnostics.
9. Authored dungeons are unchanged.
10. Documented for another developer.
