# Dungeon presentation 0.8 — viewer and modular environments

Goal of 0.8: make dungeons easy to inspect, and let the generated spaces of
the Emberforge Vaults and the Hollow Reliquary look like places, without
touching navigation, encounters or simulation results.

This file starts with the architecture review and plan written before
implementation (Phase A), followed by what was built and measured
("Results").

## Phase A — review of the 0.7 renderer

What the game does today when you enter a dungeon (`crates/game`):

| Piece | Where | Reads | Needs a running sim? |
|---|---|---|---|
| Floors, ramps, stairs, rock under raised floors, walls, drop strips | `dungeon.rs::spawn_level_geometry` | `run.sim.nav`, `run.sim.content` | **no**: only `Nav` and `Content` |
| Decor, props (GLB with fallback), lava, embers | `cave.rs::spawn_decor`, `spawn_props`, `spawn_lava` | `run.sim.content` | no |
| Ambient light, fog, sun, entrance portal, boss daises | `dungeon.rs::spawn_world` | content, enemies | only for the boss positions, which are also in content |
| Hazard discs, telegraphs, actors, HUD, nameplates, effects | `dungeon.rs`, `hud.rs`, `overlay.rs`, `fx.rs` | sim state | yes |
| Navigation overlay | `navdebug.rs` | `run.sim.nav`, plus AI paths | the outlines: no; the AI paths: yes |
| Orbit camera with collision | `camera.rs` | `CameraFocus`, `CameraBlockers` | no |
| Area tour (screenshots per area) | `demo.rs::area_tour` | `run.sim.nav` | it starts a whole run to get there |

**Finding:** everything that draws the *place* is a function of `Content` and
the `Nav` built from it. Only the convenience of taking `&ActiveRun` ties it
to a run. The viewer can therefore use **the same code**, not a second
renderer:
1. Move the environment builders into one module (`env.rs`) that takes
   `(&Content, &Nav)` and the state whose exit despawns them.
2. The game calls it from `OnEnter(Dungeon)`, and the viewer from
   `OnEnter(Viewer)`.
3. The navigation overlay's surface and link drawing takes a `&Nav` from
   either.

Dungeon Forge (`crates/forge`) knows the logical structure:
* rooms (kind, route, centre, extents, cuts, pillars);
* corridors (style, role, joints);
* which area belongs to which room (`Composed::owners`).

Today all of that is lost when the file is written. The game must not depend
on the forge (CLAUDE.md), so the structure has to travel **inside the dungeon
file**.

## Plan

### Content schema (backward compatible; all new tables optional)

All of these live in `crates/sim/src/content.rs`. The simulation never reads
them; they are validated on load.

* `[dungeon.generated]`: blueprint, seed, kit, generator version (the viewer's
  "source" line).
* `[[dungeon.rooms]]`: stable `id`, `name`, `role` (entrance, chamber, hall,
  arena, pit, side, overlook, passage, corridor, bridge, stair, ledge),
  `route` (main, alternate, side, shortcut, loop), the `areas` it is made of,
  `connects` (for corridors), `encounter`, an optional `atmosphere` preset.
  Handwritten dungeons without it get rooms derived from area names (the part
  before " (").
* `[dungeon.atmosphere.<preset>]`: ambient colour and brightness, fog colour
  and distances. A room uses the preset named by its `atmosphere`, else by its
  `role`, else none.
* `[dungeon.modules]`: the modular art set, as models with nominal sizes and a
  procedural fallback. It holds `wall` (tiled along wall faces), `corner`
  (posts at wall joints), `door` (frames at doorways) and `pillar`.
* `[[dungeon.anchors]]`: decoration anchors the forge computed (kind,
  position, facing, room, clearance), kept for inspection.

### Modular environments: hybrid, driven by the real geometry

A fixed-size room model can't fit jittered, cut, pillared rooms of any size.
So the game assembles modules **along the boundaries the navigation already
has**:
* **Walls**: each wall face (`Nav::walls`, the same faces the procedural slabs
  use) is split into `round(len / module.length)` pieces. Each piece is
  stretched by at most ±25% to fit, and scaled to the wall's height. Sloped
  walls along ramps keep the procedural slab.
* **Corners**: a post module at every wall joint (de-duplicated).
* **Doors**: at every walk portal between two different rooms, a frame whose
  posts stand outside the portal's width and whose lintel is above head
  height.
* **Floors**: stay procedural prisms (they are the walkable truth), with kit
  tints.
* **Pillars**: the forge emits pillar props sized to the pillar holes (which
  are real holes in the walkable floor), so cover looks exactly as big as it
  is.

A missing model (or no `[dungeon.modules]`) falls back to the 0.7 procedural
geometry, piece by piece. Old dungeons render exactly as before.

### Decoration anchors (forge)

`compose` computes anchors from the plan:
* outer corners, cut corners, door sides, wall runs, pillars, pit centres and
  bridge ends;
* each with position, facing, room, role and clearance.

Kit `[[decorate]]` rules map anchor kinds to props, decor and lights, with a
chance and room-role filters, chosen deterministically from the seed.
* Every placement is checked against the walkable areas, the same rule the
  loader enforces.
* The 0.7 hard-coded placements (arena props, corner rocks, pillar candles)
  become rules.

### Atmosphere

The forge writes presets per room role from the kit, which carries a few
presets per theme. The game blends fog and ambient light toward the preset of
the room the camera focus is in:
* in play, the player's position;
* in the viewer, the camera's position.

The blend takes about a second. Telegraphs are unlit and emissive, so they
stay readable.

### Dungeon viewer

`dungeon-sandbox --viewer [ID | path.toml]` (or a key from the hub) enters
`AppState::Viewer`:
* **It loads** content and builds `Nav`, and calls the shared environment
  builder.
* **It doesn't** create a `Sim`, touch the profile or write saves.
* **Fly camera**: WASD, mouse-look on the right button, E/Q up and down,
  Shift faster, Ctrl slower, wheel for speed.
* **Navigation**: Home for the entrance, Tab for the next room, a room list.
* **Walk mode** (G): eye height, clamped to walkable floor.
* **Orbit** (O): around the selected room.
* **Overlays** (number keys): navigation, structure, encounters,
  modules/anchors, info panel.
* **Selection**: click for a read-only inspector.
* **Files**: F5 reloads the file, F12 takes a screenshot.
* **Seeds**: with `--blueprint`, [ and ] regenerate other seeds by running
  the `dungeon-forge` executable. The game links nothing from the forge.
* **Tour**: `--viewer-tour` screenshots every room and writes an HTML contact
  sheet, plus frame-time statistics.

`dungeon-forge view <blueprint> --seed N` generates a layout and opens it in
the viewer.

### Visibility

Measure first. Room grouping comes for free from `[[dungeon.rooms]]`, and the
tour reports entity counts and frame times. A doorway-visibility experiment is
optional (Phase G), and is adopted only if the measurements justify it.

### Constraints

Simulation hashes must stay identical on all dungeons:
* areas, links and lava are unchanged;
* decor, props, lights and the new tables are not read by the sim.

Regenerated files bump `GENERATOR_VERSION`.

---

# Results

Everything below was verified by tests and automated graphical runs on the
development machine (Windows 11, RTX 2070 SUPER, 1280x760). It is not
covered by a recorded human playtest; PLAYTEST_VIEWER.md is the script for
one.

Commits on top of 8596ce0:

| commit | content |
|---|---|
| 4c70977 | Phase A: this review and plan |
| 2fe8031 | Phase B: the viewer on the shared environment renderer |
| 3c206d6 | Phases C–E: modules, anchors and decoration rules, atmosphere, regenerated content |
| 934be77 | Phase F: viewer self-test, picking tests, `dungeon-forge view` |

The docs, version 0.8.0 and packaging commit follow these.

Guides:
* [DUNGEON_VIEWER_GUIDE.md](DUNGEON_VIEWER_GUIDE.md): using the viewer;
* [MODULAR_ENVIRONMENTS.md](MODULAR_ENVIRONMENTS.md): schema, how pieces
  are placed, adding a piece, fallbacks, performance.

## Architecture as built

```
dungeon file (TOML) --> Content --> Nav --+--> env.rs  spawn_environment   (game + viewer)
                                          |      +- floors, ramps, stairs, lava, decor, props, lights, fog
                                          |      +- modules.rs  plan_modules -> spawn (walls, corners, doors, pillars)
                                          |      +- structure.rs  rooms + doorways  --> EnvStructure
                                          |      +- Atmosphere (per-room fog/ambient blend)
                                          +--> dungeon.rs  + Sim (play)
                                          +--> viewer.rs   (no Sim): camera, layers, inspector, tour, test
Dungeon Forge --> the same file, plus [dungeon.generated], [[dungeon.rooms]],
                  [dungeon.modules], [dungeon.atmosphere.*], [[dungeon.anchors]]
```

The separations the milestone asked for:

| Concern | Where |
|---|---|
| Content | `crates/sim/src/content.rs`: the new tables are optional, validated, and never read by the simulation |
| Simulation | Untouched: 960 handwritten and 480 generated runs are hash-identical to 0.7 |
| Environment rendering | `env.rs` and `modules.rs`; one code path for the game and the viewer, with no viewer-only visuals |
| Viewer controls | `viewer.rs` |
| Development overlays | `viewer.rs` layers and `navdebug.rs` |
| Generation | `crates/forge`; the game still doesn't depend on it, and the viewer runs the `dungeon-forge` *executable* to switch seeds |

## Decisions

* **A viewer mode, not a second application.** The `--viewer` flag of
  `dungeon-sandbox` enters `AppState::Viewer`; the hub's V button opens it
  too. One executable, one renderer, and the package needs nothing new
  except a `.bat`.
* **The structure travels in the file.** The renderer can't call the
  forge, so generated files carry rooms, corridors and their areas.
  Handwritten dungeons get rooms derived from area names. Every surface
  belongs to exactly one room (tested). This gives the viewer, the
  atmosphere and door frames a room graph without guessing, and is the seed
  of a future Studio's selectable rooms with stable ids.
* **Hybrid modules along real boundaries** (see MODULAR_ENVIRONMENTS.md).
  The alternative was fixed-size room prefabs, which would force every room
  of a kind to one size and give up the generator's variety.
* **Anchors computed by the forge, rules from the kit.** The same anchor
  kinds serve both kits: the crypt hangs candle sconces where the forge
  hangs lanterns, and puts pillars where the forge puts nothing.
  Placement is deterministic per seed and validated against walkable
  space.
* **Atmosphere as a few presets per role**, blended at the focus. It costs
  no extra lights.
* **Read-only selection.** The viewer selects and describes rooms, pieces,
  anchors and mobs with their stable ids, which is what a future editor
  would map to overrides (0.7). It edits nothing.

## Problems found and fixed while building

* **Initial `--viewer` state ran before the camera existed.** The enter
  system was skipped silently. The viewer is now entered on the first
  frame from the hub.
* **Module rotation.** A handedness test was inverted, so most wall pieces
  got a reflection instead of a rotation and vanished. Fixed, with a unit
  test.
* **Corner posts.** They appeared wherever navigation splits a straight
  wall, which read as crenellations. Posts are now only where a wall turns
  or ends, and never on walkable floor (a test caught one in a doorway).
* **Door frames.**
  * They were 1.2–1.5 m wide where a corridor meets two floor strips of a
    pit or pillared room: the opening was split over several portals, and
    merging was order-dependent. Portals now merge until stable.
  * The width is measured across the corridor's own floor.
* **A pre-existing navigation oddity, made visible.** In crypt seed 5 a
  0.6 m stub of wall stands in a passage mouth: the corridor overlaps a
  pillared hall's aisle by only 0.6 m, so that aisle's edge keeps its wall.
  It is a real navigation wall (it blocks line of sight in 0.7 too), so the
  art keeps drawing it. Listed in TASKS.md as a forge fix; fixing it would
  change the simulation, so it isn't done in a presentation milestone.
* **Basalt walls looked like a picket fence.** The columns now overlap in
  front of a rock mass, in darker basalt.
* **`dungeon-headless` was stale** after a schema change in one check run.
  It was rebuilt and the comparison redone.

## Verification

* **Checks**: `cargo fmt --all -- --check`, `cargo check --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace --release` and `cargo build --release --workspace`
  all pass. Counts are in PROGRESS.md.
* **New tests**:
  * game:
    * module rotation;
    * placement over all six shipped generated layouts: walls on navigation
      walls, openings walkable, posts off the floor, pillars filling their
      holes, mounted decoration above head height;
    * fallback with missing models; old dungeons without modules;
    * picking of rooms, pieces and mobs;
    * derived rooms of handwritten dungeons;
  * forge:
    * rooms cover every area once; generated info, modules, atmosphere and
      anchors present; deterministic;
    * legacy kits still decorate;
    * bad rules rejected;
    * an override clearing a room's encounter.
* **Simulation**:
  * shipped generated files regenerated with generator v3: 480 runs (6
    layouts × 4 configurations × 20 seeds) hash-identical to their 0.7
    versions;
  * handwritten dungeons: 960 runs (6 dungeons × 8 configurations × 20
    seeds) hash-identical to the 0.7 build.
* **Viewer** (`--viewer-test`):
  * all checks pass on the Hollow Reliquary and the Emberforge Vaults
    (generated) and the Cinderdeep Hollows (handwritten): entrance, room jump,
    flight, walk at eye height on the floor, the four layers, screenshot,
    reload with the camera kept, next seed (generated), back to the hub;
  * no save file was written.
* **Tours**:
  * all six shipped generated layouts, three per kit: 294 screenshots,
    contact sheets, no warnings;
  * inspected: halls, eye-height views, arenas, passages, stairs, the pit
    room, the drop ledge, bridges;
  * a crypt tour with the model folder hidden: procedural fallback drawn,
    one warning per piece kind;
  * an overlay tour with every layer and the panel.
* **Game**:
  * `--demo` of the crypt with the modular art: Victory in 460.9 s;
  * `--input-test`, `--frame-test`, `--tooltip-test`, `--click-test` and
    `--hub-tour` pass; the Dungeons tab shows "Inspect in viewer (V)".
* **Package** (0.8.0, 30.6 MB ZIP, assets 1.01 MB), extracted to a scratch
  folder:
  * `--viewer-test` passes when started from another directory, including
    seed regeneration through the packaged `dungeon-forge.exe`;
  * a tour renders the modular art with no missing files.

## Performance

| Dungeon | Mean ms/frame | Worst view | Meshes (visible avg) |
|---|---|---|---|
| Emberforge, modules | 3.94 | 4.14 | 2120 (308) |
| Emberforge, procedural (0.7 look) | 3.82 | 4.15 | 2080 (301) |
| Reliquary, modules | 3.72 | 4.29 | 2623 (426) |
| Reliquary, procedural (0.7 look) | 3.87 | 4.36 | 1214 (205) |

Measured with `--no-vsync --viewer-tour`, mean over 46–49 views per
dungeon. The procedural rows are the same build with `assets/models/kit`
hidden.

## Visibility (Phase G)

**Not built.** The numbers don't justify it:
* a dungeon renders in about 4 ms;
* frustum culling already keeps visible meshes at 10–16% of the total.

The prerequisites now exist, if art ever makes rooms expensive:
* room grouping (`EnvStructure`);
* doorways with positions and widths;
* the tour's per-view frame times and mesh counts to measure against.

## Known limitations

* **No recorded human playtest** of the viewer or the new look.
* **Wall fading**: walls drawn with modules don't fade when they hide the
  player (the procedural walls did). Camera collision still uses the same
  wall boxes, so the camera stays in front of them.
* **Camera fit**: the camera's collision boxes are the procedural wall
  boxes, not the modules' exact shapes (arches and pillars are
  approximated).
* **The art is simple**: script-generated, untextured, flat-shaded.
* **Sloped walls** along ramps and stairs, and lava lips, stay procedural.
* **Forward compatibility**: 0.8 generated files can't be read by 0.7
  builds (new optional tables). 0.7 files load in 0.8.
* **Room derivation for handwritten dungeons** relies on area names.
  Corridors are only recognised by " - " in the name; Cinderdeep shows 0
  corridors.
* **Wall stub**: the 0.6 m stub in crypt seed 5 above.

## Next steps

1. Run PLAYTEST_VIEWER.md and keep the notes.
2. Studio groundwork:
   * select a room, then emit an override (0.7) from the viewer;
   * show overrides applied.
3. Fade or cut modular walls between the camera and the player.
4. Forge: no narrow corridor overlaps with pillar aisles (the wall stub).
   This changes the simulation, so it needs a measured change.
5. Textured or Blender-authored pieces with the same frames and sizes.
