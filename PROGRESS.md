# Progress

Status: **0.9.2 (combat clarity) implemented and verified with automated
checks; see the 0.9.2 section.** 0.9.1 (Dungeon Remix polish) and 0.9
(Dungeon Remix) below.

Previous: **0.8.1 (viewer polish, map navigation and exploration design)
implemented and verified**. It adds a speed bar and presets, a 2D map,
inspection lighting, branch-purpose classification, a branch audit
(BRANCH_PURPOSE_0_8_1.md) and the first user's 0.8 feedback
(PLAYTEST_NOTES_0_8.md); see the 0.8.1 section.

0.8 (dungeon viewer and modular environments) was implemented and
verified with unit tests, simulation hash comparisons against 0.7,
scripted viewer tests, screenshot tours of six generated layouts, the
standard UI checks, and a test of the packaged ZIP. 0.8 adds:
* a dungeon viewer (a mode of the game: `--viewer`, or V in the hub) on the
  game's own environment renderer;
* modular GLB environment pieces placed along the real walls, doorways and
  pillars;
* kit decoration rules at anchors;
* per-room atmosphere;
* room structure carried in generated dungeon files.

Earlier versions have been playtested by the maintainer; no written notes are
in the repository. 0.8 is not covered by a recorded human playtest
(PLAYTEST_VIEWER.md is the script). (Correction: the 0.6 status said no human
had played 0.2–0.6; that was wrong. The "Not verified" lists below mean "not
covered by a recorded playtest".)

Environment: Windows 11 Pro, Rust 1.99.0, Bevy 0.20.0 (pinned `=0.20.0`,
Cargo.lock committed), NVIDIA RTX 2070 SUPER (Vulkan).

## 0.9.2 - Combat clarity

Plan and results: COMBAT_CLARITY_0_9_2.md. Playtest script:
PLAYTEST_COMBAT_0_9_2.md.

**Human feedback:** the owner reported a playtest with the 0.9.2 brief
(recorded in PLAYTEST_REMIX_0_9.md "Notes"; build and seeds not
identified):
- combat lacks visual and numerical clarity;
- ability outcomes are hard to recognise;
- controls, camera and companions were good.

0.9.2 itself is not covered by a recorded human playtest.

**Changes (presentation only):**
- recent-action lines above the action bar, built from your own events;
- ranked floating text: yours first, interrupts name the spell, companions
  pale, none for companions' ordinary hits in raids;
- per-fight numbers at the bottom right (K).

**Verified** (details in COMBAT_CLARITY_0_9_2.md "Results"):
- the simulation is untouched (no sim, forge or asset change), and 320
  headless runs are hash-identical to 0.9.1;
- fmt, check, clippy clean; tests forge 28, game 34, sim 164;
- `--demo` screenshots for all four classes and a 10-player raid, looked
  at;
- `--input-test` (now presses K) for four classes;
- `--frame-test`, `--tooltip-test`, `--click-test`, `--hub-tour`,
  `--remix-test`, and `--viewer <id> --viewer-test` on three dungeons:
  all pass.

## 0.9.1 - Dungeon Remix polish

Plan, evidence and results: DUNGEON_POLISH_0_9_1.md. Not covered by a
recorded human playtest. No human observations of 0.9 exist locally; the
playtest script (PLAYTEST_REMIX_0_9.md) has new seeds and pending sections.

Changes:
- **Measured flank ratings** in the kits steer flank targets (generator v5).
  No flank leads to the Bone Archers, the Warden's Screen or the Ash Hounds,
  which flanking only made harder. Flanks that only cost: about 11/30 →
  8/40.
- **Loot names its source;** the results screen and HUD show treasure
  opened or missed.
- **Headless `--slot-follow`** for comparisons. It showed trail following
  does not add overpulls (30 vs 28).

Verified: see DUNGEON_POLISH_0_9_1.md "Results":
- fmt, check, clippy and tests (forge 28, game 27, sim 164) green;
- audits 100/100 for both blueprints (Emberforge 385/400 over seeds
  101–500);
- handwritten dungeons hash-identical to 0.9.0;
- graphical self-tests pass;
- a full demo cycle was seen on screen.

## 0.9 - Dungeon Remix

Design, measurements and limitations: DUNGEON_REMIX_0_9.md ("Results").
Playtest script: PLAYTEST_REMIX_0_9.md. Not covered by a recorded human
playtest.

Verified (2026-10-16, final build):
- fmt, check, clippy -D warnings clean; tests: forge 27, game 26, sim 164
  (1 ignored); release build OK.
- audit 100 layouts: Hollow Reliquary 100/100, Emberforge 98/100 (seeds 46,
  56 fail, reported); 0 purposeless branches (0.8.1 blueprints: 241 / 149).
- routes: companions walked 100% of branch rooms; flank, safe and detour
  effects as listed in DUNGEON_REMIX_0_9.md.
- regression vs 0.8.1 (8 configs x 15 seeds, all dungeons): wins 732 -> 732,
  deaths 1220 -> 1216; classics: Hollow 98 -> 102 wins, Emberforge 105 -> 105.
- --viewer-test (3 dungeons), --remix-test, --input/--frame/--tooltip/
  --click-test, --hub-tour all exit 0; demo run Victory on the classic
  Hollow Reliquary; screenshots looked at.
- Package dist/DungeonSandbox-0.9.0-windows-x64.zip (31.1 MB): --remix-test
  and --viewer-test pass from the extracted package.

Known: the demo/headless default autopilot does not walk to caches (scripted
routes do).

## 0.8.1 — viewer polish, map navigation and exploration design

Usage: DUNGEON_VIEWER_GUIDE.md (speed, map and inspection sections).
Analysis: BRANCH_PURPOSE_0_8_1.md. Feedback: PLAYTEST_NOTES_0_8.md.

Changes are confined to the game crate:
- `viewer.rs`, plus the new `map2d.rs`;
- in `structure.rs`, `Purpose`, `dead_end` and `off_main`;
- in `env.rs`, the `InspectionLighting` resource.

`crates/sim`, `crates/forge` and `assets/` are unchanged, so the simulation
and the generated content are unchanged by construction. `dungeon_sim` does
not depend on the game crate, and no hash comparison was needed or run.

### Checks (run 2026-10-16 after the last code change)
- `cargo fmt --all -- --check`: clean.
- `cargo check --workspace`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean, after
  fixing two lints in `map2d.rs`.
- `cargo test --workspace --release`:
  - `dungeon_forge` 21 passed;
  - `dungeon_game` 24 passed. New: purpose classification on the shipped
    Emberforge and Reliquary files, including a loop-room case;
    handwritten rooms have no route purpose; viewer preferences round-trip
    beside the save, without creating it; map coordinates and floors;
  - `dungeon_sim` 161 passed, 1 ignored;
  - 0 failed.
- `cargo build --release --workspace`: succeeded.

### Graphical checks
All runs used `DUNGEON_SAVE_DIR` pointing at a scratch folder.

- **`--viewer-test`** passed on hollow_reliquary, emberforge_vaults and
  cinderdeep_hollows. It checked:
  - entering, R/Home, Tab, fly, walk and layers 1–4, as before;
  - the 7 slow preset (3.0 m/s);
  - the M map: it opens, and a room's map position leads back to that room;
  - 9 then = (51.75 m/s);
  - I: ambient 650 and fog pushed away; I again restores the atmosphere;
  - F5 reload;
  - ]: the next seed, with the 2D map rebuilt;
  - Backspace.

  No `viewer.toml` and no save were written.
- **`--viewer-tour`** passed (exit 0) on three seeds per kit:
  - Emberforge: the shipped seed 1, s6 and s8;
  - Reliquary: the shipped seed 1, s5 and s10.

  Each tour includes `map.png` and the overhead shots. Looked at:
  - **Maps**: purpose colours match the room data. Reliquary s5's
    cloister-walk rooms are pink (loop) after the classification fix.
    Emberforge s8 shows the overlook in orange, the optional bunker in green
    and the alternate in cyan.
  - **Overheads**: the atmospheric overhead is dark beyond the fog range, as
    diagnosed; the inspection overhead shows the whole dungeon clearly.
  - **Labels**: fitted map labels overlapped on compact crypt layouts. Ids
    are now shown when fitted and names once zoomed in; re-checked on
    Emberforge s8.
- **Standard UI checks**: `--input-test`, `--frame-test`, `--tooltip-test`,
  `--click-test` and `--hub-tour` all exit 0.
- **Demo**: `--demo --level 7 --screenshots` ran a Gloomroot Warrens run to
  "Victory in 257.8s". Demo mode stays open afterwards, so the process was
  stopped by hand (exit 1 from the kill).
- **Inspection lighting doesn't reach gameplay**:
  - the system runs only in `AppState::Viewer`;
  - `apply_atmosphere` only defers to it in the Viewer state;
  - nothing writes dungeon files.

### Packaged build
- `scripts/package-windows.ps1` produced
  `dist/DungeonSandbox-0.8.1-windows-x64.zip` (30.7 MB; assets 1.01 MB).
- Extracted with Expand-Archive, the packaged `dungeon-sandbox.exe --viewer
  <id> --viewer-test` passed on hollow_reliquary and emberforge_vaults, with
  all 16 checks OK. The scratch save folder stayed empty.

### Branch audit
- 10 seeds per blueprint, analysed with a scratch script.
- Findings and 0.9 recommendations are in BRANCH_PURPOSE_0_8_1.md.
- In short:
  - alternate routes hold nothing, and most bypass a required pack;
  - side chambers are always one-room dead ends, and the empty ones are
    leftover slots;
  - the overlook drop is the only branch that changes play.

### Not verified
- **No human playtest of 0.8.1.** Whether the speed bar, smoothing, map and
  inspection lighting satisfy the first user's requests is unconfirmed. The
  open questions in PLAYTEST_NOTES_0_8.md (how layouts feel, whether side
  branches feel worthwhile, discoverability of optional encounters, whether
  the kits feel different, readability in combat) remain **unvalidated**.
- **Camera feel near walls**: assessed by reading the code and the tests,
  not by a person.
- **Hardware**: the package was tested on this machine only.

## 0.8 — dungeon viewer and modular environments

Review, plan, decisions, measurements: DUNGEON_PRESENTATION_0_8.md. Usage:
DUNGEON_VIEWER_GUIDE.md. Art system: MODULAR_ENVIRONMENTS.md.

Starting point: 8596ce0 (0.7.0 plus the wowdecomp research). Commits:

| commit | content |
|---|---|
| 4c70977 | Phase A: architecture review and plan |
| 2fe8031 | Phase B: viewer on the shared environment renderer |
| 3c206d6 | Phases C–E: modules, anchors, decoration rules, atmosphere, regenerated content |
| 934be77 | Phase F: viewer self-test, picking tests, `dungeon-forge view` |

The docs, version 0.8.0 and packaging commit follow these.

### Checks (run 2026-10-16 after the last code change)
- `cargo fmt --all -- --check`: clean.
- `cargo check --workspace`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --workspace --release`:
  - `dungeon_forge` 21 passed;
  - `dungeon_game` 20 passed;
  - `dungeon_sim` 161 passed, 1 ignored (`diag_replay`);
  - 0 failed.
- `cargo build --release --workspace`: succeeded.

### Acceptance (0.8)
1. **A viewer mode**:
   - `dungeon-sandbox --viewer [ID|file]`;
   - "Inspect in viewer (V)" in the hub;
   - `dungeon-forge view <blueprint> --seed N`;
   - `Dungeon Viewer.bat` in the package.
2. **Loads the existing dungeon format**: TOML by id or path.
3. **Generated and authored dungeons**: tours and `--viewer-test` on
   generated (Reliquary, Emberforge, six layouts) and handwritten
   (Cinderdeep) dungeons.
4. **Same renderer as the game**: `env.rs::spawn_environment` is called by
   both states; there are no viewer-only visuals.
5. **Free camera**:
   - flight (WASD, E/Q, Shift, Ctrl, wheel speed), mouse-look on the right
     button;
   - walk at eye height on the floor (G), orbit (O);
   - reset and entrance (R or Home).
6. **Named rooms**: Tab / Shift+Tab, the room list (L), `--room NAME`.
7. **Toggleable overlays**:
   - navigation, with unreachable regions in magenta;
   - structure: rooms, routes, doorways, the main route;
   - encounters: spawns, aggro, patrols, hazards;
   - modules and anchors.
8. **Room and component information**:
   - info panel: source, blueprint, seed, kit, counts, room, area and
     elevation under the camera;
   - click inspector for rooms, modules (asset, transform, bounds,
     fallback), anchors and mobs.
9. **No saves, no combat**: no `Sim` is created; `--viewer-test` confirms no
   save is written.
10. **Portable build**: the packaged viewer passes `--viewer-test` from
    another folder, including seed regeneration.
11. **Reusable visual component definitions**: kit `[modules]`, copied into
    generated files as `[dungeon.modules]`.
12. **Crypt modular art**: ashlar walls with niche variants, pilasters,
    round arches, pillars filling their holes, candle sconces.
13. **Volcanic modular art**: columnar basalt walls, basalt corner clusters,
    iron-bound forge gates with glowing seams, lanterns, braziers.
14. **Correct across seeds**: placement tests over all six shipped layouts
    (three per kit); 294 tour screenshots without warnings.
15. **Data-driven decoration anchors**: `[[decorate]]` rules over six anchor
    kinds, deterministic, validated against walkable space; legacy kits
    keep working.
16. **Fallbacks**: missing models fall back per piece kind (tested and
    screenshotted); old dungeons render as in 0.7.
17. **Navigation and encounters unchanged**: 480 generated and 960
    handwritten runs hash-identical to 0.7.
18. **Handwritten content still works**: loaded, toured, played (demo).
19. **Automated tests pass**: see Checks.
20. **Screenshots**: tours, overlays, fallback, demo and UI shots, in
    DUNGEON_PRESENTATION_0_8.md "Verification".
21. **Performance**: modules versus procedural, per kit.
22. **Documented**: DUNGEON_PRESENTATION_0_8.md, DUNGEON_VIEWER_GUIDE.md,
    MODULAR_ENVIRONMENTS.md, PLAYTEST_VIEWER.md.
23. **Inspection made easier**: generate, open, fly, layers, compare seeds
    with [ ], tour to a contact sheet, reload with F5.

### Simulation (release)
- **Generated files**: the shipped generated files were regenerated
  (generator v3: rooms, modules, atmosphere, anchors, decoration). 480 runs
  (6 layouts × default, 10 players, Mender, Heroic × 20 seeds) are
  hash-identical to the 0.7 files.
- **Handwritten dungeons**: 960 runs (6 dungeons × 8 configurations × 20
  seeds) are hash-identical to the 0.7 binary.

### Graphical (automated, screenshots inspected)
- **`--viewer-test`**: every check passes on the Hollow Reliquary, the
  Emberforge Vaults and the Cinderdeep Hollows. It covers:
  - entrance, room jump, 12 m of flight;
  - walking at 1.70 m eye height on the floor;
  - the four layers, screenshot, reload with the camera kept;
  - `]` from seed 1 to 2 (generated);
  - Backspace back to the hub.
- **`--viewer-tour`** of all six shipped generated layouts: 294 shots,
  contact sheets and `stats.csv`.
  - Inspected: halls (overview and eye height), arenas, passages,
    stairs, the pit room, the drop ledge, bridges.
  - Found and fixed: wall rotation, crenellated corner posts, narrow door
    frames, basalt "fence".
- **Fallback tour** (models hidden): procedural walls, walled pillar holes,
  crystal fallbacks; one warning per piece kind.
- **Overlay tour**: every layer plus the panel.
- **Game**:
  - `--demo` of the Hollow Reliquary with the modular art: Victory in
    460.9 s;
  - `--input-test` (moved 29.7 m), `--frame-test`, `--tooltip-test`,
    `--click-test` and `--hub-tour` pass.
- **Packaged 0.8.0 ZIP** (30.6 MB; assets 1.01 MB):
  - extracted and run from another folder;
  - `--viewer-test` passes;
  - a tour renders the kit models with no missing files.

### Performance (`--no-vsync --viewer-tour`, ms/frame, mean over all views)

| Dungeon | modules | procedural (0.7 look) |
|---|---|---|
| Emberforge Vaults | 3.94 | 3.82 |
| Hollow Reliquary | 3.72 | 3.87 |

Meshes: 2120 / 2623 with modules, of which about 300–430 are visible.
Room-based visibility culling was not built: there's nothing to win.

### Bugs found and fixed during 0.8
- The viewer entered before the camera existed (the enter system was
  silently skipped).
- An inverted handedness test in the module rotation.
- Corner posts at every wall split.
- Door frames sized from partial portals; order-dependent portal merging.
- A test caught the override proof of concept tripping on the new room
  references (`encounter = ""` now clears one).

### Not verified (needs a human)
- Whether the viewer's controls and panels are pleasant, and whether the
  new look reads well and plays well (camera around arches and pillars,
  telegraph readability under the presets). See PLAYTEST_VIEWER.md.
- Other GPUs and resolutions; the package on other machines.

## 0.7 — meaningful dungeon variety and encounter design

Review, plan, formats, measurements and continuation plan:
DUNGEON_FORGE_0_7.md.

Starting point: commit 1ed653a (0.6.0). Commits:

| commit | content |
|---|---|
| 6f758a6 | Phase A: architecture review and plan |
| 3217c15 | Phase B (sim): patrols, line-of-sight pulses, cover AI |
| 56c3742 | Phase D (AI): cover that holds, nav trouble log, parked-DPS retarget |
| e8447b1 | Phases B–E (forge): spatial variety, Recipes 2.0, crypt kit, report |
| 3ac0db2 | content: regenerated Emberforge, The Hollow Reliquary |
| e7e07f4 | Phase F: designer overrides proof of concept |

The version bump (0.7.0) and docs commit follow these.

### Checks (run 2026-10-15 after the last code change)
- `cargo fmt --all -- --check`: clean.
- `cargo check --workspace`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --workspace --release`:
  - `dungeon_forge` 18 passed;
  - `dungeon_sim` 161 passed, 1 ignored (`diag_replay`);
  - `dungeon_game` 15 passed;
  - 0 failed.
- `cargo build --release --workspace`: succeeded.

### Acceptance (0.7)
1. **Architecture review before implementation**: DUNGEON_FORGE_0_7.md
   Phase A (commit 6f758a6).
2. **Data-driven design identity**: blueprint `[identity]` with promised
   challenges. Validation fails when nothing delivers one (test). The checks
   report what was delivered.
3. **A second, distinct kit through the same pipeline**: the crypt
   (`assets/forge/kits/crypt.toml`) and The Hollow Reliquary. It differs in:
   - grid pitch, corridor widths and room shapes (pillared naves);
   - chasm (void), lighting and props;
   - recipes: screen, patrol, line of sight, council, wailing guardian;
   - route structure (axial, cloister loop, burial cells).

   The test `the_two_kits_build_different_dungeons` checks this.
4. **Spatial variety**:
   - per-side room extents, corner cuts, jitter and aspect;
   - bends (longer passages);
   - pillared rooms;
   - straightness, side rooms and several alternate routes.

   Navigation guarantees hold: structure and space checks pass on every
   seed. 0.6 → 0.7 Emberforge distinct room footprints went from 8.1 to
   12.1 per layout, and non-rectangular rooms from 5.0 to 10.3.
5. **Encounter Recipes 2.0**:
   - tags, formations, placement preferences, space requirements and
     patrols;
   - deterministic placement scoring that rejects bad spots;
   - a failure names the recipe and room (test).
6. **AI improvements from reproducible failures**, each measured before and
   after (DUNGEON_FORGE_0_7.md, "AI"):
   - cover flicker;
   - idle parked damage dealers;
   - cover versus cleave-cone detours (Guardian deaths 51 → 29, wipes
     7 → 4).

   No teleporting. Handwritten dungeons: 843 → 846 wins over 1120 runs;
   every other simulation change is hash-identical to 0.6.
7. **Analysis tools**: `dungeon-forge report`:
   - per-encounter deaths, killing blows, wipes, resets and clear times;
   - optional completion;
   - dithers and stuck recoveries per room;
   - duration by layout;
   - JSON output and SVG overlays.
8. **Improved volcanic dungeon, a crypt dungeon, multiple layouts and a
   comparative evaluation**: the 10-layout table in DUNGEON_FORGE_0_7.md.
   Three layouts of each blueprint are shipped.
9. **Designer modifications**: `--overrides` proof of concept with stale
   and invalid detection (test).
10. **Visual identity**:
    - an original candle-stand GLB (`crypt_candelabrum.glb`), row in
      PROVENANCE.md;
    - pillar candles, cold light, void bridges.
11. **Regression**: authored dungeons still load and play. The comparison
    against 0.6 is above.
12. **Verification and docs**: this file, DUNGEON_FORGE_0_7.md, README,
    TASKS, CHANGELOG, CLAUDE.md, RULES.md ("Patrols, line-of-sight pulses and
    cover"), and PLAYTEST_FORGE.md (0.7 section).

### Headless (release)
- **Handwritten dungeons against 0.6**: 7 dungeons × 8 configurations
  (4 classes, 10 players, Aggressive, Conservative, Heroic) × 20 seeds.
  - 843 → 846 wins, 1458 → 1457 deaths.
  - 538 of 1120 runs are hash-identical. The others change through the
    parked-DPS retarget only: with it disabled, 0 of 280 differ.
  - Ashen Spire with a Striker player: 0 → 9 deaths in 3 of 20 runs, still
    20/20 wins.
- **Shipped generated dungeons**: 10 runs per configuration; Balanced,
  Aggressive and Conservative; stuck time 0 everywhere.

  | | 5p Normal | 5p Heroic | 10p Normal | 10p Heroic |
  |---|---|---|---|---|
  | Emberforge (L3) | 30/30 | 0/30 (also 0 in 0.6) | 30/30 | 30/30 |
  | Hollow Reliquary (L4) | 25/30 | 3/30 | 30/30 | 29/30 |

- **Ten layouts each** (4 classes × 3 runs, Balanced, Normal): Emberforge
  120/120 (0.6: 120/120); Reliquary 112/120. Every crypt loss is at the
  Hollow Guardian.
- **Generation**: 10/10 seeds for both blueprints, 10 distinct route shapes
  each. Emberforge 11 ms per dungeon (1.5 attempts); Reliquary 33 ms
  (1.7 attempts).
- **Navigation build**: Emberforge 0.5 ms (34 surfaces); Reliquary 6.8 ms
  (83 surfaces, pillared halls).
- **Performance** (ms/tick):

  | Dungeon | 5 players | 10 players |
  |---|---|---|
  | Reliquary | 0.022 | 0.034 |
  | Emberforge | 0.017 | — |

### Graphical (automated, screenshots inspected)
- `--area-tour` of the Hollow Reliquary:
  - entrance, branch hall, entry bend corridor;
  - Bone Colonnade (encounter space);
  - Narrow Stair ramp and the Hollow Choir stair (elevation);
  - the Sepulchre of the Guardian (arena).
- `--area-tour` of the Emberforge Vaults:
  - entrance, branch hall, slant passage;
  - Furnace Hall, the Watchers' Shelf ledge, the Flue bridge;
  - Molten Sanctum.
- `--nav-debug` tours of both: pillar holes and strip pieces match the
  walls; ledge and ramp outlines are correct.
- `--demo --dungeon hollow_reliquary`: Victory in 465.7 s, back to the hub.
- `--input-test` (moved 29.8 m), `--frame-test`, `--tooltip-test`,
  `--click-test` and `--hub-tour` all pass. The Dungeons tab lists "The
  Hollow Reliquary (level 4+)".
- **Packaged 0.7.0 ZIP** (30.1 MB), extracted to a scratch folder and run from
  there:
  - the crypt's area tour renders with the candle-stand GLBs (no fallback
    warnings);
  - the packaged headless runner clears it.

### Bugs found and fixed during 0.7
- **Crypt generation failed on every seed**: the loop shortcut was
  impossible on a straight axis, and the crawler pack didn't fit burial
  cells. Fixed with a BFS cloister walk through bend rooms, a compact ring
  and a far placement. Space failures now name the recipe and room.
- **Council named after one abbot** → uses the recipe's name.
- **Loop shortcut ending at a bend room** → bends are excluded as loop ends.
- **Cover flicker; idle parked DPS; cover rewritten by cone detours**: see
  "AI" in DUNGEON_FORGE_0_7.md.

### Not verified (needs a human)
- Whether the two dungeons feel different to play; whether pillars, the wail
  and the patrols read on screen; pacing and fairness of the Hollow
  Guardian (the hardest fight in the simulation).
- Whether the report and overrides are clear to another designer.
- Props and lighting on other GPUs; the package on other machines.

## 0.6 — Dungeon Forge foundation

Plan: DUNGEON_FORGE_PLAN.md. Formats, results and continuation plan:
DUNGEON_FORGE_0_6.md. Asset research: ASSET_PIPELINE_RESEARCH.md.

Starting point: commit c61cc28 (0.5 complete). Commits:

| commit | content |
|---|---|
| ecbb790 | Phase A: architecture review and plan |
| 19bb66b | Phase B/C: forge crate and CLI, volcanic kit, Emberforge Vaults |
| d272b83 | Phase D: multi-layout matrix; pit-corner and stale-commitment fixes |
| 84781aa | Phase E: GLB props, provenance, asset research |
| c44b416 | Phase F: profiling, bounds-pruned projection |

The version bump and docs commit follow these.

### Checks (run 2026-10-15 after the last code change)
- `cargo fmt --all -- --check`: clean.
- `cargo check --workspace`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --workspace --release`:
  - `dungeon_forge` 11 passed;
  - `dungeon_sim` 157 passed, 1 ignored (`diag_replay`);
  - `dungeon_game` 15 passed;
  - 0 failed.
- `cargo build --release --workspace`: succeeded.

### Acceptance (0.6)
1. A readable blueprint without coordinates: `assets/forge/blueprints/emberforge_vaults.toml`.
2. A deterministic builder: byte-identical output per seed (test).
3. Components cover branching (alternate route, side rooms), elevation
   (ramps, stairs, overlook, drop) and boss arenas.
4. Variation: 40/40 seeds, 39 distinct route shapes; three shipped layouts
   that differ in structure.
5. The output passes the loader plus the structure and encounter-space
   checks on every seed.
6. It loads in the game: listed in the Dungeons tab; graphical demo Victory
   in 390.6 s.
7. AI parties clear it:
   - 1000/1000 5-player and 250/250 10-player Normal runs over 10 layouts;
   - 0 s stuck.
8. Failed generation gives a diagnosis: a tally of causes plus suggestions
   (test with an impossible blueprint).
9. Authored dungeons:
   - unchanged by the forge;
   - the one simulation fix is equal or better on the same seeds;
   - the optimisation is bit-identical.
10. Documented: DUNGEON_FORGE_0_6.md, README, `dungeon-forge --help`.

### Headless (release; details in DUNGEON_FORGE_0_6.md)
- Generated layouts (10 layouts × 5 runs per configuration):

  | Configuration | Wins |
  |---|---|
  | Normal, level 3 | 1250/1250 |
  | Heroic, 5 players at level 5 | 200/200 |
  | Heroic, 10 players at level 3 | 99/100 |
  | Heroic, 5 players at level 3 | 0/50 (final boss) |

- Regression, against the 0.5 binary on the same seeds:
  - Vault 100/100%;
  - Warrens 99% (0.5: 98%);
  - Spire 291/300 (0.5: 289);
  - Cavern 100%;
  - Cinderdeep 30/30 at 5 and 10 players;
  - Cinder Court 30/30.
- Performance (ms/tick):

  | Dungeon | 5 players | 10 players |
  |---|---|---|
  | Cinderdeep | 0.031 (0.5: 0.041) | 0.044 (0.5: 0.065) |
  | Emberforge | 0.011 | 0.017 |
  | Legacy dungeons | 0.005 | ≤ 0.010 |

- Generation: about 5 ms per dungeon; navigation builds in at most 1.4 ms.

### Graphical (automated)
- Area-tour screenshots of the generated dungeon (entrance, ledge with drop
  strip, pit room, arena) were inspected.
- GLB braziers render in the arena. With the model file removed, the
  fallback crystals and a warning appear.
- `--input-test` (moved 30.0 m), `--frame-test`, `--tooltip-test`,
  `--click-test`, `--hub-tour`: OK.
- The hub-tour Dungeons tab lists "The Emberforge Vaults (level 3+)".
- Packaged 0.6.0 ZIP (28.6 MB): extracted and run from another directory;
  the generated dungeon and its GLB props loaded.

### Bugs found and fixed during 0.6
- **Pit-room corner portal 0.7 m wide**: oscillation; the overlap is now 3 m.
- **Stale dither commitment** (all dungeons): 10-player timeouts on
  generated layouts.
- **A drop shortcut from the entrance to the room before the final arena**
  (it skipped the dungeon): shortcuts and alternate routes are now limited to
  two rooms and never bypass a boss.
- **Molten column hiding the lava pit**: the column is smaller.
- **Braziers hidden behind arena walls**: they are scaled to show above.

### Not verified (needs a human)
- Whether generated layouts feel like places or like rooms on a grid;
  pacing; difficulty at level 3; route readability.
- Whether the tools are clear to another developer (PLAYTEST_FORGE.md
  part 3).
- Props on other GPUs; the package on other machines.

## 0.5 — the first true dungeon experience

Plan, layout, encounters, AI work, full tables and limitations:
DUNGEON_0_5.md. Packaging: RELEASE_BUILD.md.

Starting point: commit e2b58ab (0.4 complete). Commits:

| commit | content |
|---|---|
| c1c2a3f | Phase A/B: polygons, ramps in any direction, decor/lava/lights, cave renderer |
| 2298a44 | Phase C: Cinderdeep layout and encounters, polygon sliding, no overpulls |
| 594f738 | Phase D (part 1): lava look, lips, area tour, tests |
| 15aa9d8 | clippy fix |
| a7ad756 | Phase E: portal-edge fix, cone-blocked melee, walkable ranged spots |
| dffeafc | Phase F: packaging script, HOW_TO_PLAY, manual CI workflow, version 0.5.0 |
| cb76f37 | soft crowd separation, banner counts optional encounters |

The documentation commit (with `--start-level`) follows these.

### Checks (run 2026-10-15 after the last code change)
- `cargo fmt --all -- --check`: clean.
- `cargo check --workspace`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --workspace --release`: **156 passed**, 1 ignored (`diag_replay`)
  in `dungeon_sim`; **15** passed in `dungeon_game`; 0 failed.
- `cargo build --release --workspace`: succeeded.

### Headless (release; details in DUNGEON_0_5.md)
- Cinderdeep at level 5, 30 seeds per batch:
  - each player class: 30/30;
  - each AI profile: 30/30;
  - 10 players: 30/30;
  - Raid (10) difficulty at level 6: 30/30;
  - Heroic at level 7: 29/30.
  - Stuck time 0.1–0.5 s per run, unreachable time 0.0 s, deterministic replay
    OK.
  - An AI clear takes about 8 minutes (5 players) and 4.8 minutes (10).
- Against the 0.4 binary on the same seeds:
  - Vault 100%, Warrens 98–100% (0.4: 99%), Spire 96–97% over 300 seeds
    (0.4: 97%), Cavern 100%, Cinder Court 30/30.
  - Dithers are lower everywhere.
  - Rectangle navigation was verified bit-identical after the polygon refactor
    (per-seed log hashes).
- Benchmarks:
  - Cinderdeep: 0.041 ms/tick with 5 players, 0.062 with 10;
  - legacy dungeons: ~25% slower than 0.4 (Vault 0.004, Cinder Court 0.009).

### Graphical (automated)
- Area-tour screenshots of the entrance, fork, ridge ledge, Kiln Span, slag
  pit, Cistern and Heart Island were inspected.
- Cinderdeep demo at level 5: victory in 465 s, 0 deaths, about 60 fps.
- `--input-test`, `--frame-test`, `--tooltip-test`, `--click-test`,
  `--hub-tour`: OK.
- The packaged ZIP was extracted and run from another working directory:
  - the headless runner won 2/2;
  - the game smoke test passed;
  - a Vault demo won, and `saves/profile.json` was created next to the exe.

### Bugs found and fixed during 0.5
- **Hash drift after the polygon refactor.** Fixed with exact rectangle fast
  paths.
- **Healer pinned on a diagonal ramp side.** Sliding only worked along the
  axes; fixed with polygon sliding.
- **Mid-boss overpull from a healer spot.** Fixed by the aggro radius check.
- **Lava that looked like floor.** Fixed with a darker base and crust plates.
- **Slag pit hidden by full walls.** Fixed with lava lips.
- **10-player timeout.** Actors on a slanted portal edge slid along it
  forever.
- **12 s melee shuttle on the Kiln Span.**
- **Healer spots across lava.**
- **Clippy lint that slipped into 594f738.** Caused by `| tail` hiding the exit
  code; checks are now gated without a pipe.

### Not verified (needs a human)
- Everything a person would feel:
  - how long a first human clear takes (target 8–15 min);
  - whether Normal is too easy at level 5;
  - whether the routes, drop and return tunnel read clearly;
  - camera comfort in tunnels and at the boss.
- Companion movement as seen by a player (stacking when converging, remaining
  dithers).
- The package on other machines, GPUs and Windows 10; SmartScreen behaviour.
- The GitHub Actions workflow (never run; nothing pushed).

## 0.4 — three-dimensional navigation and dungeon environments

Design, full results, benchmarks and limitations: NAVIGATION_0_4.md.

Starting point: commit 9a66d86 (0.3 complete). Commits:

| commit | content |
|---|---|
| 97031ff | review of 0.3 navigation and the chosen design |
| ebd8cb7 | Phase A: navigation module and tests |
| aec93a1 | Phase B: simulation integration, spatial combat, LOS-aware AI, renderer port |
| 6656ec8 | Phase D: Cavern Traverse, navigation metrics, traversal tests |
| 4a4d115 | Phase C: debug overlay, camera under ceilings |
| 415b2ca | Phase E: cheaper line of sight |

The documentation commit follows these.

### Checks (run 2026-10-14 after the last code change)
- `cargo fmt --all -- --check` — clean.
- `cargo check --workspace` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo test --workspace` — **142 passed** in `dungeon_sim`, **15** in
  `dungeon_game`, 0 failed.
- `cargo build --release --workspace` — succeeded.

### Headless (Balanced, release)
- Cavern Traverse:
  - 5 players: 100/100 wins;
  - 10 players: 100/100;
  - 25 players: 20/20;
  - 40 players: 10/10;
  - Aggressive and Conservative profiles: 100/100 each;
  - Heroic: 50/50;
  - level 1 with each player class: 97–100% (30 seeds each).
  - 0.0 s stuck in every batch, deterministic replay OK.
- Existing dungeons, compared with 0.3 on the same seeds:
  - Vault: 100% (0.3: 100%);
  - Warrens: 99% (0.3: 97%);
  - Ashen Spire: 97% (0.3: 98%);
  - Cinder Court: 30/30 (0.3: 20/20).
- Benchmarks at 25 players: 0.022 ms/tick (0.3: 0.013); at 40 players: 0.049 ms/tick (0.3: 0.023).

### Graphical (automated)
- The Cavern demo with `--nav-debug --zoom 30` was screenshotted on:
  - the walkway, 5 m up;
  - the bridge fight;
  - the underpass beneath the deck;
  - the lava hollow;
  - the basin, 3 m below the entrance.
- The screenshots exposed a camera collapse under the gallery, now fixed.
- `--input-test`, `--frame-test`, `--tooltip-test`, `--click-test`,
  `--hub-tour` and `--editor-test` pass, and Sunken Vault demo screenshots look as in 0.3.

### Bugs found and fixed during 0.4
- **Healer flip-flopping at a corner.** At a line-of-sight boundary the healer alternated between two goals every tick. Fix: one consistent goal, plus a general dither commitment that never overrides dodging.
- **Stalled in a doorway.** A waypoint 0.046 m away read as "arrived" mid-route, and the leader stood in a doorway forever. Fix: skip waypoints closer than 0.1 m, and count "arrived but the goal is far" as stuck.
- **Double-speed falls.** Heights were settled twice per tick.
- **Ranged DPS out of the healer's sight.** In the Spire boss fight, ranged DPS parked in a stair corridor that the healer couldn't see.

### Not verified (needs a human)
- How climbing, bridges, drops and the camera under ceilings feel with real
  input.
- Whether the AI's movement looks natural to a player.
- Whether line of sight around corners in the old dungeons feels fair.
- A human-controlled run of the Cavern. Its traversal is verified only for AI
  parties and scripted player movement in tests.

## 0.3 — combat UX, controls and gameplay polish

Starting point: commit 14730d3 (0.2 complete). Commits: c24f3c7 + a25727f
(Phase A controls/camera), 905f3b2 (sim APIs), 8878767 (Phase D frames and
mouseover + Phase B action bar + resource readout), f49f356 (Phase C threat),
630effa (Phase F tooltips), 45efc02 (Phase G readability).

### Checks (run 2026-10-14 after 45efc02)
- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo test --workspace` — **118 passed** in `dungeon_sim`, **14 passed** in
  `dungeon_game`, 0 failed.
- `cargo build --release --workspace` — succeeded.
- `dungeon-headless --runs 10` (Vault, L1) — 10/10 wins, deterministic replay OK;
  `--party-size 10 --level 5 --runs 5` — 5/5, replay OK. The sim API work did
  not change combat rules.

### Unit tests added
- Controls (10, `controls.rs`): W moves along facing, right-drag steers,
  left-drag never changes facing, A/D turn vs strafe with the right button,
  both buttons run, auto-follow eases behind a moving character, waits while
  standing and can be disabled, manual camera input wins for a grace period,
  backpedalling doesn't spin the camera, angle wrapping.
- Camera: collision keeps clear of walls including side probes; smoothing pulls
  in fast and ignores sub-deadband changes (no jitter).
- Sim (5): mouseover heal targets the hovered ally and keeps the selection;
  friendly target order mouseover → selected ally → self; hostile spells ignore
  mouseover; `ability_state` agrees with what a cast does (range, resource, GCD,
  cooldown); `threat_view` reports secure / contested / lost and taunt.
- Describe (3): ability text uses real numbers scaled by power; every ability,
  class, profile and stat has text; gear comparison with the equipped item and
  role mismatch.
- HUD (1): action-slot status priority (cooldown > resource with wait time >
  range > blockers; GCD alone isn't an error).

### Graphical runs (automated, real input/picking pipeline)
- `--input-test`: scripted key presses through the real controls: moved 29.7 m
  with W/Q/D, turned, Tab-targeted, casts landed; auto-follow left a 0.03 rad
  gap between facing and camera. Same result as before the HUD rework (compared
  against a stash of the changes).
- `--frame-test` — **FRAME TEST OK**: synthetic pointer hover over a party frame
  set the mouseover to that ally and `cursor_over_ui`; pressing 1 (Mend) with an
  enemy selected healed the hovered ally and kept the enemy selected; clicking
  another frame selected that member and produced no world click.
- `--tooltip-test` — **TOOLTIP TEST OK**: armor stat and a vendor item showed
  their tooltips fully on screen (the vendor one flipped left of the right-hand
  panel). Screenshot of the Mend tooltip flipping above the action bar.
- `--click-test` OK, `--hub-tour` OK, `--editor-test` OK (scratch copy of
  `assets/content/dungeons`).
- `--demo` screenshots: Warden (threat line "SECURE on you", green plates, ToT
  frame, GCD sweeps), Arcanist in a 10-person raid (compact frames, five-second
  rule countdown, "Threat 30% of pull"), Striker (0 energy → every slot "need N
  energy - X s" in blue during the GCD; this was a bug found in the screenshots
  and fixed in 45efc02), Mender (Smite red "Out of range", Purify "nothing to
  dispel").
- Threat alerts end-to-end: `--demo --class warden --profile aggressive
  --dungeon gloomroot_warrens --level 4` logged "Aggro lost: Rootweaver Shaman
  is attacking Mirelle - taunt!" at 5.9 s, "Taunted Rootweaver Shaman" at
  11.2 s, and a second loss at 152.6 s (2 aggro-loss alerts in a 173 s run with
  the Aggressive party); a screenshot shows the orange alert, the red
  "AGGRO LOST" target-frame line and the red plate.

### Not verified in 0.3 (needs a human)
- How the controls and auto-follow *feel* with a real mouse (drag thresholds,
  turn rate, follow speed 3.0, 1.2 s grace).
- Readability of threat colours in big packs, alert frequency, tooltip delay
  (0.2 s), frame sizes for healers in raids.
- Right-drag started over a HUD panel still steers (only left-drag, clicks and
  zoom are blocked over UI) — intentional, but untested by a person.

### Known limitations (0.3)
- Mouseover in 3D picks the nearest friendly whose pick sphere the ray hits;
  allies standing inside each other pick the nearer one.
- Raid frames show only the first debuff; full details are in the tooltip.
- Tooltips are text only; no icons. Aura tooltips are reached through party
  frames, the target frame and your aura row, not per-aura chips.
- Out-of-range checks use the sim's edge-to-edge range; casts finishing get the
  sim's +3 m leeway, which the bar doesn't show.

## Starting condition of 0.2 (recorded 2026-10-14, commit b8e3fc2)

fmt/clippy clean, 50/50 tests. Prototype limits: 3 abilities per class, one
dungeon with a hard-coded boss script, flat damage (no mitigation/avoidance),
mana only, simple auras, no levels, 2 item slots, keyboard-only text hub, 3D
health bars, no camera collision, AI without coordination.

## Verified (commands actually run, results observed)

### Checks (latest)
- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo test --workspace --release` — **110 passed** in `dungeon_sim` + 2 in
  `dungeon_game` (camera ray tests), 0 failed.
- `cargo build --release --workspace` — succeeded; the release binary played a
  full `--demo` loop (hub → Sunken Vault → Victory in 163 s sim → results → hub).

### Phase A — playable experience
- Graphical runs via `--demo` (AI drives the player slot) on both dungeons with
  screenshots: nameplates with cast bars and debuffs, floating combat text,
  school-coloured effects, target ring, translucent occluding walls, hazard
  discs, frontal-cleave cones, phase tints, summon/phase/ring banners, five-slot
  action bar with resource costs, resource-coloured party bars, victory banner,
  results screen with loot/XP/level-ups.
- `--hub-tour`: screenshots of every hub tab (Party, Character for the player and
  a companion with upgrade comparisons, Dungeons with difficulty, Classes,
  History); switched the selected dungeon through the button action function.
- `--click-test`: synthetic mouse move/press/release through Bevy's picking
  backend on the "History" tab button → `Activate` observer → tab switched
  ("CLICK TEST OK").
- `--input-test`: scripted `ButtonInput` key presses through the game's own input
  systems: moved 33.7 m, Tab-targeted, 3 abilities landed, pack engaged.
- Unit tests for camera ray/box collision.

### Phase B — combat (unit tests in `stats.rs`, `items.rs`, `tests.rs`)
Armor formula and cap; resist curve; melee miss vs level gap and hit; spell miss
table; single-roll table order; parry/block only from the front (statistical
test); spells resisted but never dodged; physical vs bleed vs magic mitigation;
rage from dealt/taken and decay; energy regen; five-second rule; cooldowns and
GCD; cast completion timing; player movement cancels casts; interrupts and
silence (only interruptible casts); silence blocks spells; Hex incapacitates,
stops movement, breaks on damage, bosses immune; Lacerate ticks 12× ignoring
armor; Ward absorbs and Weakened Soul blocks re-casting; Purify removes only
dispellable debuffs; stacking cap and damage-taken multiplier; threat rules
(class multiplier, 110/130%, taunt, healing threat split, OOC healing, threat
drop, dead actors removed).

### Phase C — AI (tests + 100-run evaluations)
- Tests: tank taunts enemy on healer; healer mends lowest; dispel priority by
  profile; telegraph dodge with reaction time (Mechanics profile faster);
  blackboard assigns one interrupter and prioritizes healer-casters; Arcanist
  hexes in 4-mob pulls; AI interrupts most Hollow Bolts; followers follow;
  non-tanks leave frontal cones; healers hold top-up heals for 4 s after a pull;
  Aggressive vs Conservative profiles differ in threat failures and clear time.
- Profile comparison (`--compare-profiles`, 50 runs, Warrens, level 4, after
  all AI fixes below):

  | profile | wins | mean time | deaths | avoidable dmg | interrupts | threat fails |
  |---|---|---|---|---|---|---|
  | Conservative | 100% | 371 s | 0.02 | 331 | 89% | 2.8 |
  | Balanced | 98% | 356 s | 0.10 | 621 | 85% | 6.5 |
  | Aggressive | 46% | 330 s | 2.86 | 1774 | 56% | 7.2 |
  | Support | 100% | 360 s | 0.18 | 689 | 91% | 7.4 |
  | Mechanics | 100% | 360 s | 0.04 | 550 | 94% | 7.8 |

- Summoned adds inheriting 50% of the summoner's threat cut threat failures in
  the Warrens from 19 to 6.7 per run (Balanced, 100 runs; spiderlings no longer
  appear in the breakdown).
- Learning (`--familiarity`, Warrens L4, 100 runs each): Balanced 99% → 100%
  wins, deaths 0.12 → 0.04, ring hits 0.26 → 0; Aggressive 59% → 82% wins,
  deaths 2.32 → 1.12, avoidable damage 1669 → 655, interrupts 58% → 85%.
  Tests: familiarity/alertness sharpen parameters; victories build familiarity
  per dungeon.

- Evidence-driven fixes (`--breakdown`): non-tank frontal-cleave hits fell from
  12.5 to 3.9 per Warrens run, avoidable damage 2,500 → 770 per run,
  deaths 0.29 → 0.14 (Balanced, 100 runs).

### Phase D/F — content
- Schema 2.0 validation tests (bad multiplier, unknown template with known list,
  typo'd field, ring delay ≥ interval, phase thresholds); the library loads both
  shipped dungeons and both example variants.
- Encounter tests: phase 2 once per pull at 50% and ring 2 s later; ring timing
  follows `interval`; rings hit only those inside; Brakkis summons adds which
  despawn on reset; frontal cleave spares those behind; shaman heals injured
  allies; hazards pulse on/off; optional encounters not required; heroic raises
  HP and level; boss reset after wipe and phase re-trigger; leash; pack aggro.
- Difficulty (Balanced AI, headless): Sunken Vault normal L1 100% (100 runs);
  heroic L3 50% (20 runs, before the AI fixes); Warrens normal L4 96–98%, L5–6
  100%; tests assert the Vault is mostly clearable at L1 for every class and
  the Warrens at L6.
- Graphical: the Warrens played to Victory in `--demo --level 6` (279 s sim).
- Third dungeon, the Ashen Spire (40 runs each): Normal L8 98% wins, L10 100%;
  Heroic L10 25% (endgame challenge). Test asserts 2/3 wins at L10 and that the
  Twin Altar has two bosses. Graphical `--demo --dungeon ashen_spire --level 10`
  played to Victory (403 s sim).
- Sandbox overrides: test verifies HP scaling, ability interval and telegraph
  scaling with invariants kept, and the reward factor; the Sandbox tab rendered
  in `--hub-tour`.
- Encounter editor: tests for clamped edits staying valid, custom id/name, and a
  stable TOML round trip of both shipped dungeons. `--editor-test` (against a
  scratch copy of the content directory) edited Morvane's Hollow Bolt interval
  8 → 5 s and HP +10%, saved `90_sunken_vault_custom.toml`, reloaded with no
  errors and selected it; the headless runner then cleared it (5/5). The Editor
  tab rendered in `--hub-tour`.
- Vendor: test for unlocked stock by level, buying with marks and salvage
  refunds; the Vendor tab rendered in `--hub-tour`.
- Cone routing (Warrens L4, 100 runs): non-tank Root Lash hits 1.6 → 0.6 and
  Rending Mandibles 2.3 → 1.4 per run; avoidable damage 618 → 538, 97–99% wins;
  Ashen Spire L8 deaths 0.12 → 0.05 per run, 100% wins (40 runs). Test asserts
  paths crossing a cone are detoured and clear paths are left alone.

### Phase E — progression (tests in `profile.rs`)
Reward survives save/reload; one item per cleared loot source; wipes keep kill
XP but no clear loot; XP levelling and cap; heroic rarity upgrade and marks
multiplier; equip/swap and auto-equip respecting role affinity; v1 save
migration; corrupt save backup. Graphically: after a Warrens run the hub showed
level 7, 20 marks, loot in the stash with comparisons.

### Phase G — raids
- In-game 10-person raid: `--demo --group 10 --difficulty raid10` on the Warrens
  rendered compact two-column party frames and ran to Victory (359 s sim).
  Raid (10) headless (30 runs): Warrens L6 97% wins / 0.83 deaths; Vault L3 100%.
  Test: raid auto-fill gives 2 tanks + 2 healers for every player class.

#### Headless scaling
`--party-size` builds groups with ~1 tank per 10 and 1 healer per 4. All sizes
clear the Warrens at level 6. Benchmarks (release, `--bench`):

| party | normal content | stress (enemy HP ×8, sustained combat) |
|---|---|---|
| 5 | 0.002 ms/tick (~400k ticks/s) | 0.003 ms/tick |
| 10 | 0.004 ms/tick | 0.004 ms/tick |
| 25 | 0.011 ms/tick | 0.012 ms/tick (~4,200× real time) |
| 40 | — | 0.022 ms/tick (~2,300× real time) |

Bottleneck candidates: per-call `engaged_enemies()` allocations and O(party ×
enemies) AI scans; not a problem at these sizes.

## Not verified / requires human playtesting (0.2)
- Feel of the controls and camera with real keyboard/mouse; readability of the
  new effects for a human; difficulty when a human controls the player slot.
- Healer and tank player experience with the new abilities (Ward, Purify,
  Shield Bash, Quake Stomp) in the graphical game.
- Hub buttons other than the History tab were exercised through the action
  function, not real clicks (the click path itself is verified).
- 25+ player groups are headless only (the game offers 5 and 10).
- Only Windows/Vulkan tested.

## Known problems
- Remaining threat failures (~6.5 per Warrens run) come mostly from
  healer-caster packs at the pull.
- The Sunken Vault is easy for the AI party at level 1 (100% wins); the human
  player slot is expected to be the variable. Heroic provides a challenge.
- Frontal cleaves still clip non-tanks ~4 times per Warrens run (moving through
  the cone; no path planning around cones).

## Decisions (0.2)
- Formulas are tagged [WoW]/[WoW-approx]/[orig] in RULES.md; the resist formula
  deviates from WoW on purpose (too steep at levels 1–6).
- Content is a directory of dungeon TOML files; schema 2.0 replaced the hard-coded
  boss script (the Vault was ported; old single-file content is not loaded).
- Labels are `Arc<str>` because enemy ability names come from data.
- The blackboard is recomputed every tick from state (no hidden memory), keeping
  AI deterministic and testable; companions still decide individually.
- Hub buttons use `bevy::ui_widgets::Button` + `Activate` observers (the old
  `Interaction` API is deprecated in 0.20).
- Save format v2 with automatic v1 migration.

## Next actions
See TASKS.md: the human playtest (PLAYTEST_0_3.md) comes first.
