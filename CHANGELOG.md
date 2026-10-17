# Changelog

## 0.9.2 - Combat clarity

Plan and results: COMBAT_CLARITY_0_9_2.md. Playtest script:
PLAYTEST_COMBAT_0_9_2.md. Presentation only: the simulation is unchanged
(identical event-log hashes to 0.9.1 over 320 runs).

- **Your recent actions** above the action bar: up to four lines built
  from the simulation's events. They name the ability, the target and
  the outcome:
  - damage with crits, dodges, misses and absorbs; an AoE is one line;
  - healing with overheal;
  - shields, defensives (from the ability's own numbers), taunts, crowd
    control applied and broken (and by whom), dispels, threat drops;
  - interrupts with the spell stopped, or "nothing to interrupt";
  - companions' interrupts, dimmer.
- **Floating text is ranked:**
  - yours is larger and lasts longer, crits larger still;
  - your ticks are smaller than your direct hits;
  - partial absorbs show "(N abs)";
  - interrupts float the spell stopped;
  - your shields, defensives and crowd control float their name;
  - companions' numbers are small and pale; in a 10-player raid their
    ordinary hits and heals are not floated.
- **This fight's numbers** (bottom right, K hides them), your role's
  first:
  - damage and per second;
  - healing with overheal %, and shields absorbed;
  - damage taken and avoidable damage taken;
  - interrupts.
- **Owner's playtest notes** recorded in PLAYTEST_REMIX_0_9.md.

## 0.9.1 - Dungeon Remix polish

Details and measurements: DUNGEON_POLISH_0_9_1.md.

- **Flanks lead where they matter.** Kit recipes carry a measured
  `flank = "favoured" | "different" | "poor"` rating, and flank targets
  prefer favoured packs and never take poor ones unless named (generator
  v5).
  - Over 40 measured flanks, 19 ease the fight and 20 change who is engaged
    first; 8 only cost (0.9: about 11 of 30).
  - Emberforge now builds 100/100 of seeds 1–100.
- **Treasure is visible:**
  - every loot line says where it came from;
  - the results screen lists each chest as opened or not;
  - the HUD checklist shows chests.
- **`dungeon-headless --slot-follow`** compares companion trail following
  with the 0.8.1 formation following. Trail following does not add
  overpulls (28 vs 30 over 960 runs).
- **Playtest script:** new seeds (Hollow Reliquary 1, 17 and 15; Emberforge
  5) and feedback sections.

## 0.9 - Dungeon Remix

Design, schema and measurements: DUNGEON_REMIX_0_9.md. Playtest script:
PLAYTEST_REMIX_0_9.md.

### Purposeful branches (Dungeon Forge, generator v4)
- Blueprints declare `[[routes]]` with a purpose: `detour` (holds an
  optional fight and/or a cache, rejoins further on), `flank` (a side
  passage into a required pack's room), or `safe` (a longer, vent-free way
  round a crossing with magma vents).
- `[[side]]` declares side chambers: `optional`, `cache`, `vista` (landmark,
  no reward) or `quiet` (at most one); `depth = 2` adds an antechamber.
- Every branch is measured on the navigation, and an attempt is rejected
  unless the branch delivers its purpose:
  - walking length against the main route (detours at least x1.15; flanks
    x1.1 to x3; safe ways x1.15 to x3.5);
  - flank angle (at least 60 degrees off the pack's front) and aggro-free
    staging;
  - vents on the short way only;
  - nothing goes round a required fight or a boss;
  - drops save walking and skip no required fight;
  - the final boss is reachable from every required room.
- Branches never sit in a stretch a drop skips.
- `[[dungeon.branches]]` records each branch's purpose, rewards and measured
  facts.
- Legacy blueprints (`alternate_routes`, `side_rooms`) still generate the
  same layouts, and are reported as purposeless.
- Every optional encounter now has loot (0.8 gave only the first one).
- Generation attempts raised from 64 to 256.
- `dungeon-forge audit` reports branch purposes over many layouts (JSON;
  exit 2 on purposeless or broken branches).
- `dungeon-forge routes` plays each route choice with a scripted leader and
  free companions, and reports:
  - wins, time and deaths;
  - hazard damage;
  - the flanked fight against the same pack approached directly;
  - first hit and first kill;
  - traversal by the leader and the companions;
  - regroup time, caches, overpulls, stuck, unreachable and dithers.

### Simulation
- **Treasure caches** (`[[dungeon.caches]]`): opened by walking up to them
  (`CacheOpened` event); they roll their loot like a cleared encounter.
  Dungeons without caches are unaffected.
- **Companions follow the leader's trail** when their formation slot is out
  of the leader's sight or inside an idle pack's aggro. Movement changes
  everywhere, and hashes with it; measured against 0.8.1 on all eight
  dungeons.
- **Scripted autopilot routes** (`RunOptions::route`) and a
  `slot_follow_only` comparison switch.
- **New metrics:** overpulls and caches opened.

### Game
- **Layout row** on the Dungeons tab for forge dungeons:
  - Classic, Remix (a random seed; unbuildable rolls are named) or a typed
    Seed (never substituted).
  - Built by the external `dungeon-forge` into `generated/` beside the save.
  - Shown in the hub, the HUD, the results screen and the run log.
  - V opens the chosen layout in the viewer.
  - `--remix-test` checks the flow end to end.
- **Caches** drawn with open and closed models, a glint light, and a banner
  when opened.
- **Doorway clues:**
  - crypt: teal tablets at reward branches, warm candles at landmarks, a
    single dim candle at side passages;
  - volcanic: white-hot gate lamps, bright lanterns and ember lanterns.
- **Landmark rooms:** a mourner effigy (crypt) or an anvil shrine
  (volcanic).
- **Viewer:**
  - the structure layer, the 2D map (purpose letters, branch lines, hazard
    rings, cache diamonds) and the inspector show each room's purpose and
    its branch's measured facts;
  - the hazardous crossing is marked.
- **New models:** 8 generated models (caches, landmarks and markers), with
  provenance rows.

## 0.8.1: viewer polish, map navigation and exploration design

Guides: DUNGEON_VIEWER_GUIDE.md (updated). Branch review:
BRANCH_PURPOSE_0_8_1.md. First user feedback: PLAYTEST_NOTES_0_8.md.

### Viewer
- **Flight speed**: a speed bar at the bottom centre shows the speed in m/s
  and its band.
  - presets: slow 3, normal 12 and fast 45 m/s (keys 7/8/9, or buttons);
  - fine steps of ×1.15 (−/=, numpad, or buttons);
  - the wheel range is now 0.5–200 m/s;
  - Shift/Ctrl unchanged.
- **Camera**: free flight eases in and out instead of starting and stopping
  instantly.
- **Preferences**: speed and inspection lighting are remembered for the
  session and saved to `viewer.toml`, beside the save file but never in it.
  Tests and tours never write it.
- **2D map (M)**: a top-down developer map of the whole dungeon.
  - It shows floors shaded by height and lava; outlines coloured by purpose;
    doorways, the main route, drops, spawns by rank, optional packs, the
    entrance and the camera; and room ids and names (more detail as you zoom
    in).
  - Wheel zooms, drag pans, F fits; click a room to fly there.
  - Rebuilt on reload and seed switch. Drawn inside the game, with no forge
    dependency.
- **Inspection lighting (I)**:
  - Distant rooms faded because of the 18–75 m linear fog, not clipping or
    culling.
  - Inspection lighting gives neutral white light with no fog; atmosphere
    mode is unchanged.
  - Viewer only; the simulation and dungeon play are unaffected.
- **Branch purposes**: rooms are classified as main, alternate, optional
  encounter chamber, empty side chamber, overlook/shortcut, return loop or
  passage. The structure layer, the map and the inspector show this, and the
  inspector adds "dead end" and "N rooms off the main route". The crypt's
  cloister-walk rooms count as loop, not alternate.
- **Tours** now also capture `map.png` and two whole-dungeon overhead shots
  (atmospheric and inspection lighting).
- **`--viewer-test`** also checks:
  - the speed presets and fine steps;
  - the map: opening it, and that a room's map position leads back to that
    room;
  - inspection lighting on and off;
  - that the map is rebuilt on seed switch;
  - that no `viewer.toml` is written.

### Reports
- **BRANCH_PURPOSE_0_8_1.md**: an audit of ten seeds per kit, with 0.9
  recommendations.
  - No alternate route holds anything.
  - 14 of 20 bypass a required pack, so they offer no real choice.
  - Every side chamber is a one-room dead end, and the empty ones are
    leftovers of slot counts.
  - The overlook drop is the one branch that changes play.
- **PLAYTEST_NOTES_0_8.md**: the first user's 0.8 feedback, the 0.8.1
  responses, and the questions that remain unvalidated.

No simulation, forge or content changes: `crates/sim`, `crates/forge` and
`assets/` are untouched.

## 0.8 — Dungeon viewer and modular environments

Review, decisions and measurements: DUNGEON_PRESENTATION_0_8.md. Using the
viewer: DUNGEON_VIEWER_GUIDE.md. The art system: MODULAR_ENVIRONMENTS.md.
Playtest script: PLAYTEST_VIEWER.md.

### Dungeon viewer
- **Opening it**: `dungeon-sandbox --viewer [ID | file]`, "Inspect in viewer
  (V)" in the hub, `dungeon-forge view <blueprint> --seed N`, or
  `Dungeon Viewer.bat` in the package. It shows a dungeon's environment
  with the game's own renderer, without a simulation or saves.
- **Camera**:
  - free flight (WASD, E/Q, Shift/Ctrl, wheel speed), right-mouse look;
  - walk at eye height on walkable floor (G), orbit (O);
  - entrance (Home/R), next and previous room (Tab), a clickable room list
    (L), `--room`.
- **Layers**:
  1. navigation, with regions the entrance can't reach in magenta;
  2. structure: rooms, routes, doorways, the main route;
  3. encounters: spawns, aggro, patrols, hazards;
  4. modules and anchors.
- **Panels**: an info panel (source, blueprint/seed/kit, counts, the room
  and area under the camera) and a click inspector for rooms, modular
  pieces, anchors and mobs. Read-only.
- **Iterating**: F5 reloads the file; [ and ] generate the previous or next
  seed with `dungeon-forge`; F12 takes a screenshot.
- **`--viewer-tour`**: screenshots of every room plus eye-height views, an
  HTML contact sheet and per-view frame statistics.
- **`--viewer-test`**: a scripted check of the controls.
- **`--no-vsync`**: for measurements.

### Modular environments
- **Kits** gain:
  - `[modules]`: wall, corner, door and pillar GLB pieces with nominal sizes
    and wall variants;
  - `[atmosphere.<role>]` lighting and fog presets;
  - `[[decorate]]` rules over decoration anchors.
- **Placement**: pieces follow the real geometry.
  - Walls are tiled along navigation walls, stretched to fit.
  - Posts go where walls turn or end.
  - Door frames are sized from the corridor's floor.
  - Pillars fill their holes in the floor exactly.
  - Floors stay procedural.
- **Fallbacks**: a missing model falls back to the 0.7 procedural geometry
  for that piece kind. Dungeons without modules render exactly as before.
- **Anchors** (forge): pillar, pillar face, wall, door side, cut corner and
  outer corner, with facing and clearance. Rules place props, decor and
  lights deterministically, checked against walkable space. Kits without
  rules keep their 0.6/0.7 decoration.
- **Atmosphere**: fog and ambient light blend per room toward the room
  role's preset, in play and in the viewer.
- **Art**: ten original modular GLBs from `scripts/make_prop_models.py`,
  with provenance rows.
  - Crypt: ashlar wall and a niche variant, pilaster, round arch, pillar,
    candle sconce.
  - Volcanic: basalt wall, corner cluster, forge gate, lantern.

### Dungeon Forge
- **Generated files carry their structure**:
  - `[dungeon.generated]` (blueprint, path, seed, kit, generator);
  - `[[dungeon.rooms]]` (rooms and corridors with their areas, roles,
    routes, order and encounters);
  - the kit's modules and atmosphere;
  - `[[dungeon.anchors]]`.

  All optional and validated; the simulation never reads them.
- `GENERATOR_VERSION` 3. The shipped layouts were regenerated; their
  simulation is hash-identical to 0.7 (480 runs).
- `dungeon-forge view`.

### Other
- Packaging ships `dungeon-forge.exe`, the blueprints and kits, and
  `Dungeon Viewer.bat`, and reports the asset size.
- Overrides can clear a room's encounter with `encounter = ""`.

## 0.7 — Meaningful dungeon variety and encounter design

Review, formats, measurements and plan: DUNGEON_FORGE_0_7.md. Playtest
script: PLAYTEST_FORGE.md (0.7 section).

### Dungeon Forge
- **Kit components**: a kit now defines the spaces of its theme:
  - grid pitch, corridor and passage widths;
  - the chasm under bridges (lava or void), corner decoration;
  - per room kind: size, aspect, jitter, corner cuts and shape (plain, pit,
    or a pillared hall whose pillars block movement and sight).

  The defaults reproduce 0.6.
- **Layout**:
  - rooms with per-side extents and cut corners;
  - bend rooms that make longer passages;
  - `straightness`, `side_rooms`, `alternate_routes` (a count) and
    `passages`;
  - loop shortcuts walked through bends.
- **Blueprint identity**: `[identity]` with a summary and promised
  challenges. Validation fails when no encounter delivers a promise, and
  names the recipes that would.
- **Encounter Recipes 2.0**:
  - tags (interrupts, frontal, protect_caster, patrol, area_control,
    summons, council, line_of_sight, multi_phase);
  - formations (line, wedge, screen, ring);
  - placement preference (far, guard_exit, center, patrol);
  - space requirements (size, ranged lane, pillars) and patrol routes.

  A deterministic placement scorer rejects bad spots. When nothing fits,
  the error names the recipe and the room.
- **`dungeon-forge report`**:
  - per encounter: pulls, resets, wipes, deaths, killing blows, deaths by
    class, clear times and optional completion;
  - per room: dithers and stuck recoveries;
  - duration by layout;
  - `--json`, and `--svg DIR` maps with death and trouble markers.

  Maps now draw patrol routes.
- **`generate --overrides FILE`** (proof of concept): a designer patch keyed
  by id or name, re-applied on regeneration. Stale keys and invalid results
  are errors.
- `GENERATOR_VERSION` 2: the same seed now gives a different layout than 0.6.

### Content
- **The Hollow Reliquary** (new, level 4+), from the new crypt kit:
  - narrow passages and pillared naves;
  - the Warden's Screen, a sentinel patrol, Bone Archers, the Twin Abbots
    (a council) and a censer procession;
  - the Hollow Guardian, whose wail only hits those who can see it;
  - optional Sealed Revenant and Crawler Nest.

  `08_hollow_reliquary.toml` (seed 1); seeds 5 and 10 in
  `variants/forge/`.
- **The Emberforge Vaults**, regenerated: irregular caverns, angled passages,
  an ash-hound patrol, Recipes 2.0. Seeds 1, 6 and 8 as before.
- An original candle-stand model (`crypt_candelabrum.glb`), generated by
  `scripts/make_prop_models.py`, with a provenance row.

### Simulation and AI
- **Patrols** (`patrol` on a mob spawn) and **line-of-sight pulses**
  (`los_blockable` on a pulse). Both are opt-in; see RULES.md.
- **Cover AI**: companions hide behind pillars and corners from a
  line-of-sight pulse:
  - they keep a chosen spot while it still hides them;
  - they prefer spots they can reach without crossing a cleave cone;
  - they hold still once hidden.

  On the Hollow Guardian this took deaths from 51 to 29 and wipes from 7 to
  4 over 60 runs.
- **Parked damage dealers retarget**: after 3 s at their chosen spot unable
  to hit their target, they switch to a tank-held enemy they can hit. Against
  0.6 over 1120 handwritten-dungeon runs: 843 → 846 wins, 1458 → 1457
  deaths.
- `Sim::nav_log` records dithers and stuck recoveries with positions, outside
  the event log (replay hashes unchanged). `diag_replay` gains `DIAG_TRACE`
  and `DIAG_CLASS`.

### Docs
- The 0.3–0.6 documents said no human had played those versions. That was
  wrong: earlier versions have been playtested by the maintainer (no written
  notes are in the repository). Corrected in PROGRESS, DUNGEON_0_5,
  DUNGEON_FORGE_0_6 and the playtest scripts.

## 0.6 — Dungeon Forge foundation

Plan: DUNGEON_FORGE_PLAN.md. What was built and measured:
DUNGEON_FORGE_0_6.md. Playtest script: PLAYTEST_FORGE.md. Asset research:
ASSET_PIPELINE_RESEARCH.md.

### Dungeon Forge (new crate `dungeon_forge`, CLI `dungeon-forge`)
- **Blueprints**: describe a dungeon without coordinates. A blueprint gives
  the kit, level, group size, encounter order, optional encounters, extra
  rooms, descents, alternate route, shortcut (drop or loop), bridges and
  winding.
- **Kits** are theme libraries: enemy templates (normal schema, scaled from a
  reference level), encounter recipes with placement roles and hazards, loot
  by role, palette, room names and an arena prop.
- **Generator**: deterministic and bounded.
  - It builds a topology on a 36 m grid: the main route, then an alternate
    route by BFS, side rooms and shortcuts. Shortcuts and alternate routes
    never bypass a boss.
  - Heights stay within ramp limits.
  - It tries up to 64 attempts and reports a tally of causes with
    suggestions.
  - Same blueprint and seed give a byte-identical file.
- **Components**:
  - rooms: entrance, chamber, hall, arena, pit room, side chamber, overlook;
  - corridors: straight, winding, angled, ramp/stair, lava bridge, drop
    walkway.
  - Each is defined once in a local frame and placed by 90° rotation,
    translation and height.
- **Validation**:
  - the game's own loader first;
  - then structure: no unintended links, every corridor as planned, one-way
    drops, everything reachable, a single way into the final arena;
  - then encounter space: staging room, chain pulls, arena size for rings and
    cleaves.
- **Tools**:
  - `validate`, `generate` (`--map`, `--tagged`) and `inspect` (any dungeon,
    with navigation build time);
  - `test` (matrix over `--layouts`, sizes, profiles, classes, difficulties,
    with JSON failure records and reproduce commands);
  - `seeds` (success rate and variety);
  - SVG overview maps.
- **Content**:
  - the volcanic kit (the Emberkin, Forgewarden Hask, Cindermaw);
  - the Emberforge Vaults blueprint;
  - `07_emberforge_vaults.toml` (seed 1), playable from the hub;
  - seeds 6 and 8 in `variants/forge/`, with maps in `assets/forge/maps/`.

### Simulation and AI
- Fixed a stale dither commitment. While committed, an actor recorded the
  commitment target as its previous goal, so the next dither committed to the
  same point again; melee could shuttle at a ramp mouth until the run timed
  out. Handwritten dungeons are equal or slightly better on the same seeds.
- `Nav::project` and `Nav::locate` now skip surfaces by bounds first. Results
  are bit-identical (same event-log hashes). Cinderdeep runs 25–32% faster
  per tick.

### Assets
- `[[dungeon.props]]`: glTF/GLB models with a primitive fallback. They are
  validated (path inside `assets/`, footprint off walkable floor) and drawn
  as the fallback when the file is missing.
- The game's asset root is `<game folder>/assets`.
- `assets/models/props/forge_brazier.glb` is an original model generated by
  `scripts/make_prop_models.py`; `assets/models/PROVENANCE.md` records it.
- The packaging script ships `assets/models`.
- Version 0.6.0.

## 0.5 — the first true dungeon experience

Design, layout, results and limitations: DUNGEON_0_5.md. Playtest script:
PLAYTEST_0_5.md. Packaging: RELEASE_BUILD.md.

### The Cinderdeep Hollows (new dungeon, level 5+)
- An original lava-cavern descent from 0 m to −12 m, built from 49 angled cave
  areas:
  - a cold entrance;
  - a fork into a low road and a high ridge (ending in a one-way drop);
  - the Smelter Gallery around a slag pit;
  - an optional grotto;
  - the Kiln Span bridge over a lava river;
  - a return tunnel;
  - the Magma Cistern lava chamber;
  - Ignarax's island arena in a lava moat.
- 8 encounters, 3 of them optional. Positioning matters throughout: a caster
  guarded by brutes, hounds on a narrow ledge, a lava-side boss with spreading
  rings, frontal-cleaving shieldbearers on the bridge, a pack approachable from
  two causeways with timed vents, and a three-phase final boss.
- Atmosphere:
  - fog, ambient light and per-area tints (cold to hot as you descend);
  - placed lights;
  - jagged rock walls;
  - pillars, stalagmites, boulders and crystals;
  - pulsing lava with crust plates and embers;
  - low lips at lava edges.

### Navigation
- Areas can be convex polygons (`points`) and ramps can run in any direction
  (`slope_from`/`slope_to`). Portals, walls and line of sight work on
  polygons. Non-convex shapes are rejected with a fix suggestion.
- Lava can be polygons. Walls facing lava are 0.6 m lips, which block movement
  but not sight.
- New `[[dungeon.decor]]` and `[[dungeon.lights]]` sections and a per-area
  `tint`. Validation keeps decor off walkable floor.
- Sliding along walls at any angle.
- Fixed: an actor on a slanted portal edge could slide along it forever.
- Rectangle maps navigate exactly as in 0.4: per-seed log hashes were verified
  identical after the refactor.

### AI
- Ranged and healer spots:
  - never within an idle pack's aggro radius + 3 m;
  - never more than a short walk away (no spots across lava);
  - near frontal cleavers they now go through the same validated search.
- Melee boxed in by cleave cones (on a narrow bridge, for example) attack the
  enemy in the way from behind instead of shuttling.
- Soft crowd separation: companions closer than 0.9 m drift apart.
- Result:
  - the 10-player Cinderdeep timeout is gone, and stuck time fell from
    46.8 s to 0.6 s per run;
  - dithers fell in every legacy dungeon;
  - win rates are unchanged within batch noise.

### Distribution and tools
- `scripts/package-windows.ps1` builds a versioned, verified ZIP containing
  the exe, the headless runner, the content and `HOW_TO_PLAY.txt`.
- The game and the headless runner find `assets/` next to the executable, and
  saves go next to the exe.
- Optional manual GitHub Actions workflow (artifact only, no release).
- Version 0.5.0.
- New flags:
  - `--area-tour` (area screenshots);
  - `--start-level N` (playtest convenience);
  - `diag_replay` with `DIAG_WATCH`/`DIAG_N` per-tick traces.
- The intro banner counts optional encounters.

## 0.4 — three-dimensional navigation and dungeon environments

Design, verification, benchmarks and limits: NAVIGATION_0_4.md.

### Navigation
- Layered 2.5D navigation replaces overlapping flat rectangles:
  - areas have a height, can be ramps (`y_to`, `axis`, `stairs`) or open decks you can walk under;
  - they connect only where they overlap **and** are level;
  - one-way drops are authored in `[[dungeon.links]]`;
  - unreachable platforms are allowed;
  - limits live in `[dungeon.nav]`.
- Validation rejects a solid area over another, a vertical gap between step and headroom, too-steep ramps, bad links, unreachable areas and ambiguous positions. Every error names the areas and the fix.
- Old maps load unchanged (everything at height 0).
- Routing: weighted shortest paths over portals, precomputed at load; steering uses a portal funnel.
- Movement leaves a surface only through a portal or off a drop edge. Ramps interpolate height, and drops fall at 12 m/s.
- Stuck detection with recovery (no teleporting), dither detection, and companions that stay with the leader when a goal is unreachable.
- New metrics in the headless runner: `nav stuck`, `unreachable` and `dithers`.

### Spatial combat
- Actors carry an elevation and the surface they stand on.
- Range combines horizontal distance with height difference, so there is no melee through floors.
- Line of sight against walls and floors is required for targeted abilities and auto attacks (new error: "Target not in line of sight", shown on the action bar).
- Rings, hazards, cleaves and AoE hit only their own floor.
- Aggro across floors needs sight; leash is 3D.
- Enemies ignore targets they can neither reach nor see, and evade if nobody is left.

### AI
- Goals are 3D.
- Ranged spots are searched for sight of the enemy and of the healer.
- Ranged DPS move back into the healer's sight before casting.
- Healers move instead of smiting when the tank is out of sight.
- Result: the Warrens rose from 97% to 99% wins and the Ashen Spire is at 97% (98% before), with line of sight now enforced.

### Environments and tools
- **Cavern Traverse**, an original graybox test dungeon: winding passage, fork and dead end, ramp to a high walkway, rope bridge over an underpass, a gallery stacked above it, an 8 m one-way drop into the Magma Basin, narrow tunnel, lava pit, fire vent, unreachable perch.
- The renderer builds floors, rock, decks, ramps, stairs, walls that follow ramps, drop strips and lava from the simulation's `Nav`.
- Camera: lowers its pitch under bridges and low ceilings, and hides your own model when pulled within 2 m.
- Navigation debug overlay (**N**; **Shift+N** for walls and walk areas): surfaces by height with labels, portals, drops, every companion's path / waypoint / goal, unreachable goals, stuck actors.
- New flags: `--nav-debug`, `--zoom D`.

## 0.3 — combat UX, controls and gameplay polish

"Make the game better to play before making it bigger." No new dungeons,
classes or balance changes; saves stay compatible (new settings default in).

### Controls and camera
- Classic MMO controls (`controls.rs`, a pure, unit-tested rules function):
  right-drag rotates the camera and steers the character; left-drag is free look
  that never changes facing; a left *click* (under 5 px of travel) still selects;
  both buttons run forward; A/D turn (strafe while right-drag is held); Q/E
  always strafe; arrow keys rotate/tilt the camera.
- Optional auto-follow: while moving, the camera eases behind the character;
  manual camera input suppresses it for 1.2 s. Toggle with **V** (saved in the
  profile; on by default).
- Camera collision probes five rays (0.35 m radius), pulls in fast and eases out
  with a deadband, so it no longer jitters at wall corners.

### Party frames and healing
- Party and raid frames are buttons: the whole frame selects that member; gold
  border = selected, blue = mouseover; dead (red) and stunned/silenced (purple)
  tints; health numbers, debuffs (dispellable ones marked `*`) and buffs.
- Clickable target-of-target frame (T still assists).
- **Mouseover healing**: friendly spells go to the ally under the cursor (party
  frame first, then the ToT frame, then the 3D character), else the selected
  ally, else yourself — without changing your selection. Hostile spells ignore
  mouseover. A blue ground ring shows who the spell would land on. Every cast
  still goes through the simulation's normal validation.
- HUD panels block the world: clicks on them never select characters behind
  them, and drag-look and zoom don't trigger over them. Nameplates, floating
  text, banners and the tooltip never intercept clicks.

### Action bar
- Driven by `Sim::ability_state`, the same checks a key press runs: cooldown
  curtain with seconds left, a separate thin GCD sweep, red tint and
  "Out of range" (edge-to-edge range, as the sim measures it), blue tint and
  "need 40 energy - 2.5s" when short of resource (shown even during the GCD),
  and blockers: no/invalid target, silenced, stunned, stop to cast, nothing to
  dispel, target weakened.
- Failure messages name the ability ("Mend: Out of range") and are not
  re-triggered while a key is mashed.

### Threat
- Enemy nameplates are coloured from your perspective. Tank: green secure,
  yellow contested (someone ≥ 80% of the way to pulling), blue held by another
  tank, red attacking a non-tank. Others: muted when safe, yellow when you're
  close to pulling, red when it's attacking you. The health-bar frame repeats the
  colour (white for your target).
- Target frame: threat status line, a bar toward the pull threshold (110% in
  melee, 130% at range), who it attacks (and whether taunted), your threat, the
  top competitor and the pull value.
- Restrained alerts (at most one per 2.5 s): "Aggro lost: X is attacking Y -
  taunt!" for tanks, "X is attacking you!" for others; "Taunted X" confirms a
  taunt immediately.

### Resources
- Readout above the action bar: energy rate and time until full; mana regen
  with the five-second-rule countdown; rage gain/decay explanation. Action slots
  show the time until an ability becomes affordable.

### Tooltips
- One framework (`tooltip.rs`) with text generated in `dungeon_sim::describe`
  from the real definitions: abilities (costs, range, cast, cooldown, target
  rule, effect numbers scaled by your power), auras (effect, time left, stacks,
  dispellable, source), stats (formulas from RULES.md), gear (role fit,
  comparison with the equipped item, role score), AI profiles (their actual
  parameters), difficulties, classes, dungeons, vendor (price, comparison),
  sandbox knobs, editor operations and hub tabs. The panel flips/clamps to stay
  on screen and is never pickable.

### Readability
- Your own buffs/debuffs above the resource bar; shorter, non-wrapping frame
  names; wider nameplates; cast bars on enemy plates only; the default combat log
  drops companions' failed casts, cast starts and fully overhealed ticks (`L`
  shows everything); log no longer overlaps the action bar at 1280 px.

### Simulation APIs (no rule changes)
- `validate_cast` (side-effect-free; `try_cast` uses it), `queue_player_cast_on`
  and `player_cast_target` (explicit friendly target), `ability_state`,
  `resource_regen_per_sec` / `resource_rate` / `five_second_rule_left`,
  `threat_view` with `TankThreat` / `PersonalThreat`. Headless results and
  deterministic replay unchanged.

### Developer flags
- `--frame-test` (hover a party frame, press 1 as a Mender with an enemy
  selected, click another frame), `--tooltip-test`, and `--class NAME` /
  `--profile NAME` for demo runs.

## 0.2

### Phase A — playable experience
- Camera: collision against wall/pillar boxes (pull in fast, ease out), zoom 4–32 m.
- Walls that hide the player fade to translucent; walls are 3.2 m tall again.
- Targeting: Tab cycles engaged enemies first, then idle ones in front of the camera;
  Shift+Tab cycles backwards; T assists (target of target); G targets the party's kill
  target; F1–F5 target party members; click picking unchanged.
- Movement: Q/E strafe, both mouse buttons run forward.
- Nameplates for every actor (name, health, live cast bar with "(kick!)" for
  interruptible casts, harmful auras with stacks).
- Floating combat text: damage, crits, heals, miss/dodge/parry/resist/absorb,
  interrupts, dispels, CC breaks, taunts, deaths, rallies.
- Procedural effects coloured by school: spell projectiles, melee flashes, AoE/cleave
  rings, taunt rings, heal sparkles, buff shells, threat-drop puffs, phase blasts.
- Pulsing target ring; danger vignette ("MOVE OUT!") while standing in a telegraph;
  hazard discs that flicker before activating; frontal cleave cones drawn on the floor.
- HUD: five ability slots with resource costs and cooldown bars, resource bars
  coloured by type (mana/rage/energy), target frame shows casts and auras.
- Hub rebuilt as a clickable tabbed panel: Party (recruit/dismiss, AI profile,
  auto-fill, auto-equip, change class), Character (stat sheet, 7 equipment slots,
  stash with upgrade comparison, equip/unequip), Dungeons (select dungeon and
  difficulty, encounter list, reload files, content errors), Classes (all abilities),
  History (last runs).
- Results screen: multiple loot drops, XP and level-ups, interrupts, threat failures,
  avoidable damage, CC, dispels, overheal.

### Phase B — combat model
- Stats: level, armor, resist, dodge, parry, block, block value, crit, hit.
- Armor and resist mitigation, single-roll melee attack table, spell hit table,
  front-only parry/block, crit multipliers, damage variance.
- Resources: mana with the five-second rule, rage from damage dealt/taken, energy.
- Generalized auras: HoT, DoT, absorb, damage reduction, stacking damage-taken and
  healing-taken debuffs, incapacitate, silence, taunt, Weakened Soul; stacking and
  refresh rules; dispellable flag.
- Interrupts with lockout, Hex crowd control (bosses immune), Purify dispels.
- Five abilities per class (added Shield Bash, Quake Stomp, Ward, Purify, Kick,
  Lacerate, Hex, Silence Pulse).

### Phase C — AI
- Party blackboard: kill target, interrupt assignments, CC assignment, threatened
  members, danger zones, pulse warnings, frontal cleavers, time since pull.
- Five AI profiles (Conservative, Balanced, Aggressive, Support, Mechanics) with
  measurably different outcomes; per-companion profile in the hub.
- Frontal-cone avoidance, flank positioning for ranged vs cleavers, facing-based
  "behind" spots, healer pull discipline, tank AoE pickup of adds, defensive
  cooldowns on pulses and stacks, dispels, shields before pulses.
- `dungeon-headless`: `--compare-profiles`, `--breakdown` (damage by source,
  enemies turning on non-tanks, killing blows), `--dungeon`, `--difficulty`,
  `--level`, `--profile`, `--party-size`, `--bench`; richer summaries.

### Phase D / F — content
- Content schema 2.0: data-driven enemy abilities (bolt, cleave/frontal, ring,
  pulse, summon, debuff, heal_ally), HP-threshold phases with announcements and
  summons, enrage, environmental hazards, optional encounters, difficulties,
  per-encounter loot, dungeon themes. Content is a directory of dungeon files.
- New dungeon **The Gloomroot Warrens** (levels 4–6): healer-casters, a priority
  caster with an interruptible party-wide volley, an optional elite with spore rings
  and vents, Mother Brakkis (summons, stacking dispellable poison, frontal cleave),
  lava-vent room, and the three-phase Vhorlag (party pulse, adds, double rings,
  stacking tank debuff).
- The Sunken Vault ported to the new schema; Heroic difficulties for both.
- Example variants in `assets/content/variants/` (Inferno, Training).

### Phase E — progression
- Levels 1–10 with XP from kills and clears; level scaling of stats.
- Items with 7 slots, rarity (stat multipliers), role affinity and stat blocks;
  role-weighted comparison and auto-equip; run history; v1 save migration.

### Phase G — raids
- Configurable party size in the headless runner (5/10/25/40) with multiple tanks
  and healers; leader gather radius scales with party size; benchmarks recorded.
- 10-person raid groups in the game: roster of 12 companions (3 per class, older
  saves get the new recruits), Party / Raid (10) toggle with 2 tanks + 2 healers
  auto-fill, "Raid (10)" difficulty on every dungeon, compact two-column frames.

### Later 0.2 additions
- Learning companions: per-dungeon familiarity (clears/5) sharpens reactions,
  safety margins and interrupt timing; in-run alertness after avoidable hits.
  `--familiarity` in the headless tool; familiarity shown in the Party tab.
- Summoned adds inherit 50% of their summoner's threat (fewer healer pulls).
- In-game **Sandbox** tab: enemy HP, damage, ability speed and telegraph-time
  overrides saved in the profile; easier settings scale rewards down.
- Third dungeon **The Ashen Spire** (levels 8–10): healing-reduction brands,
  armored sentinels with frontal sweeps, a timed fire-jet corridor, the Twin Altar
  council (two bosses), an optional Furnace Golem, and three-phase Ashmaw.
- `--hub-tour`, `--click-test`, `--group N` developer flags.
- **Vendor** tab: buy unlocked Common/Uncommon (and later Rare) items for Vault
  Marks; salvage stash items for a third of their price.
- **Encounter editor** tab: tune a boss/elite's health and melee damage and each
  ability's interval, power, cast time and ring warning, then save a validated
  custom dungeon TOML (`90_<id>_custom.toml`) that is reloaded and selected. The
  content schema is serializable with a stable round trip. `--editor-test`.
- AI routes non-tanks around frontal cleave cones via a cone-free flank (escapes
  from ground effects keep priority); Hex is no longer spent on short-lived adds.
- Fourth dungeon, the raid **The Cinder Court** (10 players, level 10): gate
  guards, Twinmaw the Behemoth (tank swaps) and the Cinder Regent; tank-swap AI.

## 0.1 — prototype
- Hub → dungeon → boss → rewards → hub loop, four classes, AI party, headless runner.
