# Modular environments (0.8)

How dungeons get their look:
* modular art pieces placed along the real geometry;
* decoration at anchors;
* lighting and fog per room.

What stays authoritative: walkable areas, walls, line of sight and
encounters. The art is a *representation* of the dungeon. It is never a
source of gameplay rules.

## The principle

* **Navigation and the simulation never read any of it.** The art is
  drawn *from* the navigation's walls, doorways and pillar holes.
  * Regenerating the shipped dungeons with modules, anchors, rooms and
    atmosphere left 480 simulation runs hash-identical.
  * The handwritten dungeons are hash-identical to 0.7 (960 runs).
* **Walls are the navigation's walls.** A wall piece's inner face lies on
  a navigation wall: a test checks every piece in every shipped layout.
  Where navigation has a wall, the art shows one, and vice versa.
* **Openings stay open.** Door frames are sized from the corridor's real
  floor. Tests check that every opening point is walkable and that every
  post stands off walkable floor.
* **Cover looks like cover.** Pillars are holes in the walkable floor;
  the pillar piece is scaled to exactly the hole, replacing the plain walls
  round it.
* **Decoration never covers floor.**
  * Floor-standing decoration is checked against the walkable areas when
    generated, and again by the loader.
  * Wall- and pillar-mounted pieces hang at 2.6 m, above head height (also
    tested).

## Variable-size rooms: why pieces follow boundaries

Generated rooms vary in every dimension (jitter, aspect, cut corners,
pillars), so a fixed-size room model can't fit them. The system is hybrid:

| Element | How it's drawn |
|---|---|
| Floors, ramps, stairs | Procedural prisms from the walkable areas (the walkable truth), with kit tints |
| Walls | The `wall` piece tiled along every flat navigation wall: `round(length / piece)` copies, each stretched by at most ±25%, scaled to the wall's height. Sloped walls (beside ramps) and lava lips stay procedural |
| Wall joints | The `corner` piece where a wall turns or ends; not where navigation merely split a straight wall, and never on walkable floor (door frames stand at openings) |
| Doorways | The `door` piece wherever a corridor opens into a room. Its width is measured across the corridor's floor at the wall, so it is right for slanted corridors and for mouths that meet several floor strips |
| Pillars | The `pillar` piece at every `pillar` anchor, scaled to the hole |
| Everything else | Decoration at anchors (below) and the kit's lights |

Each piece is designed in a shared local frame:
* x runs along the wall (or across the opening), centred on 0;
* y is up from the floor;
* z points away from walkable space, with the inner face at z = 0.

Trims may reach 0.15 m in front of a wall, which is inside the 0.4 m every
walk area keeps from its walls.

## Kit schema

In `assets/forge/kits/<kit>.toml`; copied into every generated dungeon.

```toml
[modules]
# size = nominal [length along the wall (or opening width), height, depth] in metres
wall = { model = "models/kit/crypt_wall.glb", size = [4.0, 4.5, 0.7],
         variants = ["models/kit/crypt_wall_niche.glb"], variant_chance = 0.2 }
corner = { model = "models/kit/crypt_corner.glb", size = [0.9, 4.8, 0.9] }
door   = { model = "models/kit/crypt_arch.glb",   size = [4.0, 5.2, 0.8] }
pillar = { model = "models/kit/crypt_pillar.glb", size = [1.0, 7.0, 1.0] }

[atmosphere.hall]             # one table per room role, plus "default"
ambient = [0.32, 0.36, 0.42]  # colour
brightness = 210.0            # default 220 (the 0.7 value)
fog = [0.025, 0.033, 0.045]
fog_start = 16.0              # default 18
fog_end = 70.0                # default 75

[[decorate]]
anchor = "pillar_face"        # pillar, pillar_face, wall, door_side, cut_corner, outer_corner
rooms = ["hall", "arena"]     # room roles (empty = all)
every = 4                     # every n-th anchor of this kind in a room ...
offset = 0                    # ... starting at this one
chance = 1.0                  # deterministic from the seed
prop = { model = "models/kit/crypt_sconce.glb", fallback = "crystal", radius = 0.5, height = 0.7 }
# or: decor = ["stalagmite", "pillar"], fit = true, height = 7.0
light = { color = [1.0, 0.68, 0.38], intensity = 0.4, range = 8.0, height = 0.3, every = 2 }
```

* **Roles**:
  * rooms: `entrance`, `chamber`, `hall`, `arena`, `pit`, `side`,
    `overlook`, `passage`;
  * corridors: `corridor`, `bridge`, `stair`, `ramp`, `ledge`.
* **Which preset a room uses**: its own `atmosphere`, else its role's, else
  `corridor` for corridors, else `default`.
* **Old kits**: kits without `[[decorate]]` keep working. Their
  `arena_prop`, `pillar_prop` and `components.corner_decor` become
  equivalent rules (test `kits_without_rules_decorate_like_before`).

## In the dungeon file

Generated dungeons carry all of this as ordinary, optional, validated data:

| Table | Contents |
|---|---|
| `[dungeon.generated]` | blueprint, blueprint path, seed, kit, generator version |
| `[[dungeon.rooms]]` | id (`r3`, `c7`), name, role, route, main-route order, areas, `connects` (corridors), encounter, optional atmosphere |
| `[dungeon.atmosphere.<preset>]` | lighting and fog presets |
| `[dungeon.modules]` | the modular set |
| `[[dungeon.anchors]]` | every anchor with what was placed there (`used`) |

* **Handwritten dungeons** may use any of these too.
* **Without `[dungeon.modules]`**: everything renders procedurally,
  exactly as in 0.7.
* **Without `[[dungeon.rooms]]`**: rooms are derived from area names.
* **Overrides** (0.7 proof of concept) can patch these tables too. Clear a
  room's encounter with `encounter = ""`.

## Decoration anchors

`crates/forge/src/decorate.rs` computes anchors from the plan:

| Kind | Where | Height | Facing |
|---|---|---|---|
| `pillar` | centre of each pillar hole (clearance = half its size) | floor | — |
| `pillar_face` | middle of each face of a pillar | 2.6 m | outward |
| `wall` | along each room side, about every 7 m, not at doors or corners | 2.6 m | into the room |
| `door_side` | either side of each doorway on the room's wall | 2.6 m | into the room |
| `cut_corner` | inside a cut corner (behind its diagonal wall) | floor | diagonal |
| `outer_corner` | outside each room corner, pushed out by the footprint | floor | — |

Rules are applied in order, and an anchor is used once. Every anchor goes
into the file with what was placed there. Layer 4 in the viewer shows them;
a click shows the details.

## Adding a modular piece

1. **Make the model** in the shared frame above. Either add a function to
   `scripts/make_prop_models.py` (standard-library Python, deterministic
   output) and call it from `write_kit()`, or export a `.glb` from Blender.
   * Put walls' inner face at z = 0 and keep geometry below 2.2 m out of
     z < -0.15.
   * Put door posts outside |x| = size[0] / 2, and the lintel above 2.6 m.
2. **Put the file** in `assets/models/kit/` and add a row to
   `assets/models/PROVENANCE.md` (origin, author, license). No game-extracted
   or uncertain assets.
3. **Reference it** in a kit's `[modules]` (with its nominal `size`) or a
   `[[decorate]]` rule.
4. **Regenerate** the dungeons
   (`dungeon-forge generate ... --out assets/content/...`) and inspect them:
   `dungeon-sandbox --viewer <file>`, layer 4, then `--viewer-tour` over
   three or more seeds.
5. **Run the tests**: `cargo test -p dungeon_game modules` (placement over
   the shipped layouts), then the full suite.

## Fallbacks

* **Missing module model**: that piece kind falls back, and the log says so
  once:
  * walls: the procedural slabs or rock of 0.7;
  * pillars: walled holes;
  * corners and doors: none.

  The viewer marks fallback pieces red in layer 4.
* **Missing decoration model**: the prop's primitive `fallback` is drawn
  (as in 0.6).
* **Missing or invalid tables**: old dungeons and handwritten ones without
  tables render exactly as before. Invalid tables are rejected by the loader
  with the path of the field.

## Atmosphere

`env.rs` blends ambient light and distance fog toward the preset of the
room at the focus: the player in play, the camera in the viewer. It reaches
about 95% of the way in 1.2 s.

* Only dungeons with presets do this; the others keep their single fog.
* Telegraphs are unlit and emissive, so they read the same under any
  preset.
* It costs no lights: lights come from the kit's room lights and decoration
  rules (crypt seed 1: 75 point lights; Emberforge seed 1: 49).

## Performance

Measured with `--no-vsync --viewer-tour` on an RTX 2070 SUPER at 1280x760,
over every room view. The procedural rows are the same build with the model
folder hidden (the 0.7 look).

| Dungeon | Mean ms/frame | Worst view | Meshes (visible avg) |
|---|---|---|---|
| Emberforge, modules | 3.94 | 4.14 | 2120 (308) |
| Emberforge, procedural | 3.82 | 4.15 | 2080 (301) |
| Reliquary, modules | 3.72 | 4.29 | 2623 (426) |
| Reliquary, procedural | 3.87 | 4.36 | 1214 (205) |

Modular art costs nothing measurable at this scale. Room-based visibility
culling was not built: there's nothing to win yet. See
DUNGEON_PRESENTATION_0_8.md, "Visibility".
