# Asset pipeline research (0.6)

The goal is to prepare Dungeon Sandbox for better-looking dungeons without
tying navigation, encounters or distribution to any particular art. This
document covers:

1. the review of Benilla;
2. the conventions adopted;
3. the proof of concept that ships in 0.6;
4. the plan for 0.8.

## 1. Benilla review (architecture lessons only)

[Benilla](https://github.com/samwhosung/benilla) is a from-scratch World of
Warcraft 1.12.1 client in Rust and Bevy. Reviewed on 2026-10-15 from its
README, `docs/MAP.md` and `Cargo.toml`. **No Benilla code was copied or
adapted into this repository.**

Facts:

| | Benilla | Dungeon Sandbox |
|---|---|---|
| License (own code) | MIT OR Apache-2.0 (vendored `third_party/` keeps upstream licenses) | MIT OR Apache-2.0 |
| Bevy | 0.18.1, `default-features = false` with a chosen feature list | 0.20.0, default features |
| Game data | requires an installed English 1.12.1 client and reads MPQ archives at runtime; ships no Blizzard content | none: all content original |

Structure (from `docs/MAP.md`):
* **Format readers are separate crates with no Bevy dependency:**
  `benilla-mpq`, `-blp` (textures), `-m2` (models), `-wmo` (buildings),
  `-adt`/`-wdt` (terrain), `-dbc` (tables), on a shared bounds-checked
  byte/IFF reader (`benilla-bytes`).
* **One crate adapts them to Bevy:** `benilla-assets` registers an
  `AssetSource` (the archive chain) and `AssetLoader`s per format, plus
  material and texture handling.
* **The world renderer is its own crate:** `benilla-world` covers terrain
  streaming, portal-culled buildings (`wmo_portal`, `exterior_cull`), models,
  lighting, collision and a frame budget. A **`benilla-worldview`** runs the
  engine with no game attached.
* Shaders are embedded (`embedded://`) per crate.

Lessons that fit Dungeon Sandbox:

1. **Keep readers engine-free.** Our equivalent is already true for
   gameplay: `dungeon_sim` and `dungeon_forge` don't depend on Bevy. If we
   ever read a model format ourselves (for collision footprints, say), that
   reader belongs in a Bevy-free crate. Bevy's own glTF loader is enough for
   rendering, so we don't need a reader now.
2. **One adapter layer between content and the engine.** Props are already
   loaded in one place (`cave::spawn_props`) with one fallback path. That is
   the seam where per-component prefabs, themes and LODs plug in.
3. **A world viewer with no game attached** is a valuable tool. Our
   `--area-tour` and `--nav-debug` are a small version of it. A future Studio
   preview mode should run the renderer on a `Content` without starting a
   simulation.
4. **Portal culling and streaming** matter for large outdoor worlds and
   multi-room buildings. Our dungeons are small (under 300 m across, under
   60 areas), and 1785 frames in 30 s (vsync) showed no need yet. The grid
   layout of forge dungeons would make room-level culling easy later (rooms
   and corridors are known cells).
5. **Collision from art.** Benilla derives collision from WMO/M2 geometry.
   We deliberately do the opposite: navigation is authored and validated, and
   art must not change it. Our rule is that a prop footprint may never
   overlap walkable floor. We keep this.

What we will not do:
* read or require any Blizzard data, MPQ archives, BLP/M2/WMO files or an
  installed client;
* merge Benilla's networking, UI or client systems;
* copy code. Its license would allow MIT/Apache code with attribution, but
  none of it is needed, and it targets Bevy 0.18 where we pin 0.20. Any future
  adaptation must go through a license and dependency review first and be
  recorded in RESEARCH.md.

## 2. Conventions adopted

* **Format:** glTF 2.0, preferably binary `.glb` (one file, PBR materials,
  supported by Bevy's built-in loader, exported by Blender).
* **Location:** `assets/models/<category>/<name>.glb`, referenced from
  content by a path relative to `assets/`. The validator rejects absolute
  paths, `..` and other extensions.
* **Provenance:** `assets/models/PROVENANCE.md` lists every file with its
  origin, author and license. A model without a row must not be committed.
  Third-party models need a license that allows redistribution in this
  project; no game rips.
* **Authority:** walkability, line of sight and encounters come from areas
  and the simulation, never from models. A prop's footprint (`radius`,
  `height`) must stay off walkable floor (validated like decor).
* **Fallback:** every prop names a primitive `fallback` (`pillar`,
  `stalagmite`, `boulder`, `crystal`). A missing file logs a warning and
  draws the fallback, so content works without the art and the packaged game
  never depends on optional files.
* **Asset root:** the game sets Bevy's asset root to `<game folder>/assets`.
  This is the folder content is read from, so `cargo run` and the packaged
  exe find the same files.

## 3. Proof of concept (shipped in 0.6)

* Schema: `[[dungeon.props]]` with `model`, `pos`, `y`, `yaw` (degrees),
  `scale`, `radius`, `height`, `fallback`
  (`crates/sim/src/content.rs`, `PropConfig`). Validated for path and
  footprint; unit test `props_validate_paths_and_stay_off_walkable_floor`.
* Model: `assets/models/props/forge_brazier.glb` (11.7 KB). It is original
  and generated deterministically by `scripts/make_prop_models.py` (standard
  library only): four materials, emissive coals and flame through
  `KHR_materials_emissive_strength`.
* Rendering: `cave::spawn_props` spawns
  `WorldAssetRoot(GltfAssetLabel::Scene(0))` with position, yaw and scale.
  Missing files are drawn as their fallback decor.
* Forge: a kit's `palette.arena_prop` puts four props in the cut corners of
  every boss arena (inside the room's square, outside its walkable octagon).
  The Emberforge Vaults uses it.
* Packaging: `scripts/package-windows.ps1` copies `assets/models`
  (`.glb/.gltf/.bin/.png/.ktx2` and `PROVENANCE.md`).

Verified on the development machine (Windows 11, RTX 2070 SUPER): the arena
screenshot shows the four braziers above the walls with orange flames.
With the file renamed, the log reads "prop model
'assets/models/props/forge_brazier.glb' not found; drawing the Crystal
fallback", and purple crystals appear in the same places. Not verified: other
GPUs, models with textures or skins, Blender round trips.

## 4. Plan for 0.8 (presentation)

1. **Per-component prefabs:** a kit names a model per room or corridor
   component (e.g. `arena_floor`, `bridge_deck`, `wall_segment`) with the
   current procedural geometry as fallback. Prefab dimensions are checked
   against the component's walkable footprint at load time.
2. **Theme definitions:** kits gain material palettes, light rigs, fog and
   prop sets per room role, so a second theme (crypt, fungal) is data only.
3. **Anchors:** components expose decoration anchors (corners, wall
   niches, pit centres, bridge ends) instead of the composer hard-coding
   positions.
4. **Original modular art in Blender**, exported as GLB, with provenance rows.
   Textures as KTX2 or PNG, with sizes budgeted for the package.
5. **Room-level culling** using the forge grid if frame time demands it
   (measure first).
6. **Package size budget:** today's ZIP is 28.6 MB (almost all executable).
   The packaging script should report asset size separately and fail above
   a budget.
