# Dungeon Forge 0.6 — foundation

What was built in 0.6, how to use it, what was measured, and what comes next.
The plan written before implementation is
[DUNGEON_FORGE_PLAN.md](DUNGEON_FORGE_PLAN.md). Human playtest script:
[PLAYTEST_FORGE.md](PLAYTEST_FORGE.md). Asset research:
[ASSET_PIPELINE_RESEARCH.md](ASSET_PIPELINE_RESEARCH.md).

**Automated verification only.** Everything below was verified by tests,
simulated parties and automated graphical runs on the development machine
(Windows 11, RTX 2070 SUPER); it is not covered by a recorded human playtest.
(Correction, 0.7: earlier versions have been playtested by the maintainer; no written notes are in the repository.)

## Commits

| commit | content |
|---|---|
| ecbb790 | Phase A: architecture review and plan |
| 19bb66b | Phase B/C: `dungeon_forge` crate and CLI, volcanic kit, Emberforge Vaults |
| d272b83 | Phase D: multi-layout simulation matrix; pit-corner portal and stale-commitment fixes |
| 84781aa | Phase E: GLB props with fallback, model provenance, asset pipeline research |
| c44b416 | Phase F: profiling; bounds-pruned surface projection |

The version bump to 0.6.0 and these docs follow.

## Quick start

```bash
# Check a blueprint and its kit.
cargo run --release -p dungeon_forge -- validate assets/forge/blueprints/emberforge_vaults.toml
# Build seed 7, write the dungeon and an overview map, print topology and checks.
cargo run --release -p dungeon_forge -- generate assets/forge/blueprints/emberforge_vaults.toml --seed 7 --map forge_out/s7.svg
# Make it playable: write it into the content folder (F5 in the hub reloads).
cargo run --release -p dungeon_forge -- generate assets/forge/blueprints/emberforge_vaults.toml --seed 7 --tagged --out assets/content/dungeons/50_emberforge_s7.toml
# Simulated clears over 10 layouts, all classes and profiles, 5 and 10 players.
cargo run --release -p dungeon_forge -- test assets/forge/blueprints/emberforge_vaults.toml --layouts 1..10 --runs 5 --sizes 5,10 --classes warden,mender,striker,arcanist --profiles balanced,conservative,aggressive,support,mechanic --failures forge_out/failures.json
# Summarise any dungeon (handwritten too) and draw its map.
cargo run --release -p dungeon_forge -- inspect assets/content/dungeons/06_cinderdeep_hollows.toml --map forge_out/cinderdeep.svg
# Generate many seeds and report success rate and variety.
cargo run --release -p dungeon_forge -- seeds assets/forge/blueprints/emberforge_vaults.toml --count 40
```

`forge_out/` is git-ignored scratch space. Exit codes: 0 ok, 1 error,
2 failures found (`test`, `seeds`).

## Architecture

```
crates/forge (dungeon_forge, Bevy-free, depends on dungeon_sim only)
  blueprint.rs  Layer A: intent (schema + validation)
  kit.rs        theme library: templates, recipes, loot, palette, level scaling
  layout.rs     topology on a grid: main route, alternate route, side rooms,
                shortcut; heights; corridor styles; names
  compose.rs    Layer B: room and corridor components -> areas, links, lava,
                decor, lights, props
  populate.rs   Layer C: encounters from recipes, loot, rewards
  build.rs      pipeline, bounded attempts, serialisation, reload through the
                game's own loader, checks
  check.rs      structural and encounter-space validation
  simtest.rs    simulated party matrix, failure records
  inspect.rs    summary of any dungeon file
  map.rs        SVG overview map
  main.rs       dungeon-forge CLI
assets/forge/
  kits/volcanic.toml                 theme library (Emberkin, two bosses)
  blueprints/emberforge_vaults.toml  the proof-of-concept blueprint
  maps/*.svg                         maps of the shipped layouts
assets/content/dungeons/07_emberforge_vaults.toml     seed 1, playable
assets/content/variants/forge/emberforge_vaults_s6/_s8.toml  more seeds
```

The game and the simulation do not depend on the forge. A generated dungeon
is an ordinary dungeon file: the hub lists it, the headless runner runs it,
and a designer can edit it by hand (regenerating overwrites the edits).

## Blueprint format (Layer A)

```toml
[blueprint]
id = "emberforge_vaults"      # lowercase, digits, _
name = "The Emberforge Vaults"
description = "..."
kit = "volcanic"              # assets/forge/kits/volcanic.toml (or a path ending .toml)
level = 3                     # 1..=10; templates scale from the kit's reference level
group_size = 5                # 5 or 10
seed = 1                      # default seed (--seed overrides)

[layout]
extra_rooms = [1, 2]          # empty rooms on the main route (min, max)
descents = 2                  # ramps down along the main route
alternate_route = true        # a second way between two main rooms (never around a boss)
shortcut = "drop"             # none | drop (overlook with a one-way drop) | loop (walkable return)
bridges = 1                   # main-route corridors over lava
winding = 0.4                 # share of flat corridors that wind (S-bend) or angle

[[encounters]]                # main route in order; the last must be a boss recipe
recipe = "melee_pack"
name = "Optional display name"

[[optional]]                  # each in its own side chamber; never an arena recipe
recipe = "optional_elite"
```

Validation rejects every problem with its field and a fix:
* unknown recipes (listing the known ones);
* a last encounter that isn't a boss;
* an optional arena recipe;
* a bad id;
* levels or group sizes out of range;
* more descents or bridges than corridors;
* more than 14 main rooms or 4 optional encounters.

## Kit format (theme library)

* `[kit]`: id, name and `reference_level` (the level the numbers were
  authored for).
* `[palette]`:
  * `theme` (the normal dungeon theme: floor, wall, light, fog, ambient,
    rough walls);
  * `tint_shallow` and `tint_deep`;
  * `light_shallow` and `light_deep` (interpolated by depth);
  * the `decor` mix;
  * name pools per room kind (`[palette.names]`);
  * optional `arena_prop` (a GLB prop for arena corners).
* `[templates.*]`: the ordinary enemy schema (RULES.md). In a generated
  dungeon, HP, melee damage, ability damage, heals and DoT values are scaled
  by `(1 + 0.1·(L−1)) / (1 + 0.1·(Lref−1))`, the same factor party power
  scales by. Template levels shift by `L − Lref`.
* `[recipes.*]`: a reusable encounter.

```toml
[recipes.guarded_caster]
name = "Pyrecaller's Guard"
tests = "melee screen a healer-caster: reach it, interrupt or crowd-control its heals"
room = "hall"                 # chamber | hall | arena | pit  (boss recipes: arena or pit)
mobs = [
  { template = "bulwark", place = "front" },     # front | back | flank | center
  { template = "raider", place = "front" },
  { template = "pyrecaller", place = "back", count = 1 },
]
hazards = [ { name = "Fire Vent", place = "approach", radius = 2.5, damage = 120.0, school = "fire", on = 2.0, off = 3.0 } ]
```

* `[[loot]]`: the normal loot schema, with `source` as a **role**:
  `final_boss`, `boss` (the first required non-final boss), `optional`, or
  `pack`. Ids are prefixed with the dungeon id.

The volcanic kit has these 8 recipes:

| Recipe | Tests |
|---|---|
| melee pack | trash with one caster |
| guarded caster | melee screen around a healer-caster |
| frontal pack | hounds and a shieldwall with frontals |
| interrupt pack | two party-wide pulses |
| hazard pack | a hulk and sparks in a pit room with timed vents |
| optional elite | an obsidian sentinel with a frontal and a ring |
| Forgewarden Hask | pit boss: adds and rings at 50% |
| Cindermaw | three-phase final boss: frontal breath, interruptible spit, then rings and adds, then a pulse race |

## Components (Layer B)

Rooms sit at the centres of 36 m grid cells and corridors fill the gaps
between neighbouring cells. Each component is defined once, in a local frame
(`s` along the corridor, `t` to its left), and placed by a 90° rotation, a
translation and a floor height. With 90° rotations rectangles stay
axis-aligned, so they use the navigation's exact rectangle path.

| Room | Footprint | Use |
|---|---|---|
| entrance | square, half-size 7–8.5 | start; the spawn point is on the far side from the exit |
| chamber | square or chamfered square, 8–9.5 | junctions, empty rooms, alternate route |
| hall | chamfered octagon, 11–12.5 | packs |
| arena | octagon (cut 45%), 13–14 | bosses; props in the cut corners |
| pit room | four strips round a central lava pit (3 m corner overlaps), 11–12.5 | hazard fights, the Forgewarden |
| side chamber | square, 7–8.5 | optional encounters |
| overlook | square, 5 | top of a one-way drop |

| Corridor | Geometry |
|---|---|
| straight | one rectangle, 4.5–6 m wide |
| winding | three rectangles (an S-bend; the jog exceeds the width) |
| angled | one convex parallelogram (polygon navigation) |
| ramp / stair | one rectangular ramp (slope ≤ 0.3); steps drawn on 40% of them |
| bridge | open deck without walls over a lava chasm 6 m below, with rock stacks |
| drop | raised walkway flush with the lower room's edge, plus an authored drop link |

Room shapes guarantee that corridors from different sides never touch:
lateral offset + half width ≤ the room's straight-wall half-length − 0.6 m.
Every join overlaps by 1.5 m. Lights go over every room (cool near the
entrance, hot deep down) and over bridges at lava level. Rock columns stand
outside room corners, and a molten column stands in each pit.

## Generation (topology first, geometry second)

1. **Main route**: the entrance, then the encounters in order, with
   `extra_rooms` empty chambers inserted at seeded positions. Boss recipes
   take arenas or pit rooms; others take halls or chambers.
2. **Embedding**: a seeded self-avoiding walk on the grid (straight weight 3,
   turns 2, no dead ends before the last room).
3. **Alternate route**: BFS through free cells, 1–3 rooms, between main rooms
   2–3 apart, never around a boss and never into the final arena.
4. **Side rooms**: one free neighbour cell of a main room (not the final
   arena) per optional encounter.
5. **Shortcut**:
   * a `drop` is an overlook cell touching an earlier and a later room, at
     most two rooms ahead, never past a boss. The overlook is raised so the
     fall is 3.6–7.5 m, which clears the lower room's 3.2 m walls.
   * a `loop` is a walkable corridor between non-consecutive adjacent main
     rooms.
6. **Heights**:
   * descents on distinct main corridors (2.5–3.5 m, capped by the ramp
     length);
   * the alternate route interpolates;
   * side rooms are level with their host.
7. **Styles**:
   * ramps where heights differ;
   * bridges on flat main corridors;
   * `winding` decides S-bends and slants (falling back to straight when the
     rooms are too small);
   * names drawn without repeats from the kit's pools.
8. **Composition and population**, then serialisation with a header naming
   the blueprint, seed and attempt.
9. **The game's own loader** parses the text back. Generation succeeds only if
   it loads, and only then do the forge checks run.

Each of up to 64 attempts uses its own sub-seed, so a failure never changes
the next attempt. After 64 failures the error tallies the causes and
suggests fixes, for example:

> could not generate 'emberforge_vaults' with seed 1 in 64 attempts
> (alternate route: 64/64). Suggestions: no room for a second way; set
> alternate_route = false or use fewer optional rooms.

**Determinism**: same blueprint + kit + seed + generator version ⇒
byte-identical TOML (test `same_seed_same_dungeon_byte_for_byte`). The only
randomness is the simulation's SplitMix64 `Rng`, and every collection that
reaches the output is ordered. `GENERATOR_VERSION` (1) is in the header.

## Validation tiers

1. **Loader**: the game's schema, references and navigation validation
   (overlaps, slopes, headroom, reachability, drop zones, decor and props off
   floors, mob placement).
2. **Structure** (`check::structure`, on the navigation the game builds):
   * no walk link joins pieces of rooms the plan doesn't connect (no
     accidental shortcut through touching or stacked geometry);
   * every planned corridor is walk-linked to both rooms, both ways;
   * every drop is a one-way drop link with no walk link back;
   * every room is reachable from the entrance;
   * the final arena has exactly one way in.
3. **Encounter space** (`check::encounters`):
   * each pack's aggro radius leaves a safe staging spot in its entry
     corridor;
   * no two encounters are close enough to chain-pull (aggro + 6 m at a
     similar height);
   * boss rooms have at least 2.5 × ring radius and 1.6 × cleave radius from
     centre to wall.
4. **Simulation** (`dungeon-forge test`):
   * party matrices over layouts, sizes, profiles, classes and difficulties;
   * failures (wipe, timeout, stuck > 10 s, unreachable > 10 s, dithering >
     6 per member per minute) are written as JSON with the generator origin,
     configuration, sim seed, encounter reached, every party member's
     position, surface, goal and stuck time, and a reproduce command.

## Results

### Generation
* 40/40 seeds of the Emberforge blueprint generate, with 39 distinct
  main-route shapes, a mean of 2.9 attempts and about 5 ms per dungeon
  (release).
* Every layout has the alternate route, a side room, a drop, a bridge and at
  least two ramps (test over 8 seeds).
* Three shipped layouts with clearly different structure:
  * **seed 1** (`07_emberforge_vaults`): a compact east–west zigzag with a
    one-room alternate;
  * **seed 6**: a long westward chain with a three-room southern loop;
  * **seed 8**: a northward climb whose alternate route loops south of the
    entrance.

  The maps are in `assets/forge/maps/`.

### Simulated play (10 layouts, 5 runs per configuration per layout)
* **Normal, level 3, 5 players**: 4 classes × 5 profiles: 1000/1000 wins.
* **Normal, level 3, 10 players**: 5 profiles: 250/250 wins.
* **Stuck and unreachable time**: 0.0 s in every configuration.
* **Heroic**:
  * 5 players at level 5 (recommended + 2, as for the other dungeons): 4
    classes, 200/200 wins;
  * 10 players at level 3: 99/100;
  * 5 players at level 3: 0/50, wiping at the final boss.
* **Dithers per run**: 25–60 for most configurations, 100–215 for parties
  with two melee on cautious profiles (the known melee convergence issue).

Two real problems were found by this matrix and fixed:
1. **Pit-room corners**: the strips overlapped 1.5 m. After the 0.4 m walk
   inset that left a 0.7 m portal with a 0.1 m steering target, and
   companions turning the corner oscillated. The overlap is now 3 m.
2. **Stale dither commitment (simulation, all dungeons)**: when an actor
   dithered, it committed to its previous goal, but while committed it
   recorded the commitment target as its "previous goal". The next dither
   therefore committed to the same stale point again. Three melee shuttled
   at a ramp mouth for 20 minutes and the raid timed out. The previous goal
   is now always the AI's own goal (one line in `Sim::steer`). The test
   `ten_player_raids_clear_generated_layouts_without_loops` fails without
   the fix.

### Graphical (automated)
* The generated dungeon is listed in the hub's Dungeons tab ("The Emberforge
  Vaults (level 3+)").
* Area-tour screenshots of the entrance, overlook ledge (with the orange drop
  strip), pit room, bridge and arena (with GLB braziers) were inspected.
* `--demo --dungeon emberforge_vaults --level 3`: Victory in 390.6 s, about
  60 fps (7184 frames in 120 s).
* `--input-test`, `--frame-test`, `--tooltip-test`, `--click-test` and
  `--hub-tour` passed.
* Packaged 0.6.0 ZIP, extracted and started from another working directory:
  the generated dungeon and its GLB props loaded, with no fallback warning.

### Regression and performance
* Handwritten dungeons after the commitment fix (same seeds, against the 0.5
  binary): equal or slightly better. Warrens went from 98 to 99%, the Spire
  from 289 to 291 of 300, and the Spire's 10-player dithers from 98 to 40.
* The projection optimisation is bit-identical: event-log hashes match on 20
  seeds × 7 dungeons × 5 and 10 players.

| | ms/tick, 5 players | ms/tick, 10 players |
|---|---|---|
| Sunken Vault | 0.005 | 0.008 |
| Ashen Spire | 0.005 | 0.009 |
| Cinderdeep Hollows | 0.041 → **0.031** | 0.065 → **0.044** |
| Emberforge Vaults (generated) | 0.013 → **0.011** | 0.021 → **0.017** |

Profiling (temporary phase timers, removed) showed that AI decisions were
about 85% of a Cinderdeep tick. Most of that was `Nav::project` clamping all
49 polygons about 17 times a tick. Line of sight was negligible (about 7
calls per tick at 0.2 µs). Navigation build times are at most 1.4 ms
(Cinderdeep) and 0.2 ms for the generated dungeon; `inspect` reports them.

## Known limitations

* **No recorded human playtest.** Pacing, difficulty and readability of
  generated layouts are not covered by written playtest notes.
* **Grid look**: rooms sit on a 36 m grid with axis-aligned exits, so layouts
  read as "rooms and corridors", not organic caves. Angled corridors and
  octagons soften this. Free-form placement is future work.
* **One kit, one blueprint.** Themes beyond volcanic need kits; recipes don't
  yet cover patrols, councils or space-aware placement (0.7).
* **Difficulty**: no budget yet. Recipes are hand-sized, and AI win rates are
  not human difficulty.
* **Dithering** remains high for melee-heavy parties (it reflects the
  known melee convergence issue, not the forge).
* **Decor and props** don't block sight; the camera can still crowd at large
  bosses.
* **Designer edits** to a generated file are lost on regeneration (there is
  no merge).

## Continuation plan (0.7: encounter composition and AI validation)

1. **Recipes v2**: patrols (moving packs), councils (two bosses sharing a
   room), interrupt-heavy and add-wave recipes, and a `size` field with
   room-fit checks (melee ring, ranged ring at 15 m with line of sight).
2. **Space-aware placement**: sample candidate pack positions per room, score
   them by staging distance, line-of-sight-protected ranged spots and escape
   room for rings, and keep the best (deterministic).
3. **Difficulty budget**: points per rank and mechanic, scaled by level and
   group size. Calibrate the curve by `dungeon-forge test` on Normal at the
   recommended level and mark it as AI-calibrated, not human-calibrated.
4. **AI diagnostics report** (`dungeon-forge report`): per-room heatmaps of
   stuck and dither events and encounter-by-encounter deaths, from the failure
   records.
5. **Persistent positioning problems**: melee converging on one spot (give
   melee distinct attack slots without the regression measured in 0.5),
   cautious-profile dithering.
6. **A second kit** (crypt or fungal), to prove that themes are data only.
