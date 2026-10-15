# Dungeon presentation 0.8 — viewer and modular environments

Goal of 0.8: make dungeons easy to inspect, and let the generated spaces of
the Emberforge Vaults and the Hollow Reliquary look like places, without
touching navigation, encounters or simulation results.

This file starts with the architecture review and plan written before
implementation (Phase A). Results follow from "Results" on.

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
