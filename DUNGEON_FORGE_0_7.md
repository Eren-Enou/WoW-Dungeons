# Dungeon Forge 0.7 — meaningful variety

Goal of 0.7: changing a blueprint should change the **experience** of a
dungeon (its spatial challenges, enemy behaviour, exploration decisions,
atmosphere), not only rearrange a familiar set of rooms.

This file starts with the architecture review and plan written before
implementation (Phase A), followed by what was built and measured
(Phases B–F, from "Results" on).

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

---

# Results (Phases B–F)

Everything below was verified by tests, simulated parties and automated
graphical runs on the development machine (Windows 11, RTX 2070 SUPER). It is
not covered by a recorded human playtest; see "Not verified" at the end.

Commits on top of 0.6 (1ed653a):

| commit | content |
|---|---|
| 6f758a6 | Phase A: this review and plan |
| 3217c15 | Phase B (sim): patrols, line-of-sight pulses, cover AI |
| 56c3742 | Phase D (AI): cover that holds, nav trouble log, parked-DPS retarget |
| e8447b1 | Phases B–E (forge): spatial variety, Recipes 2.0, crypt kit, report |
| 3ac0db2 | content: regenerated Emberforge, The Hollow Reliquary |
| e7e07f4 | Phase F: designer overrides proof of concept |

## Formats added

### Kit `[components]` — what the theme's spaces are made of

```toml
[components]
cell = 32.0                  # grid pitch (24–60); 0.6 used 36
corridor_width = [3.5, 4.5]  # room-to-room corridors
passage_width = [3.0, 3.5]   # corridors through bends
chasm = "void"               # under bridges: "lava" (0.6 look) or "void"
corner_decor = false         # rock columns outside cut corners

[components.rooms.hall]      # one table per room kind
size = [8.5, 9.0]            # half-size range (m)
aspect = [1.2, 1.3]          # stretched along the way in
jitter = 0.0                 # per-side extent noise (irregular caverns)
cut = [0.0, 0.0]             # corner cut range (octagonal/irregular rooms)
shape = "pillars"            # "plain" | "pit" | "pillars"
pillars = { size = 1.8, spacing = 6.5 }
```

Room kinds: `entrance`, `chamber`, `hall`, `arena`, `pit`, `side`,
`overlook`, `bend` (a passage node, new). The defaults reproduce 0.6. The
kit validator checks that every room fits its cell with room for corridors,
pillar spacing ≥ pillar size + 3 m, and no cuts on pillared rooms.

The palette gains `light_intensity` and `pillar_prop` (a GLB in every other
pillar, with a primitive fallback, and a dim candle light).

### Blueprint `[identity]` and layout bias

```toml
[identity]
summary = "A tactical fortress: ..."
challenges = ["protect_caster", "line_of_sight", "patrol", "council", "interrupts", "multi_phase"]

[layout]
alternate_routes = 1      # count (0.6: alternate_route = true|false, still accepted)
straightness = 0.85       # 0 winding .. 1 axial
passages = 0.15           # chance of a bend room between main-route rooms
side_rooms = [1, 2]       # extra empty dead ends to explore
```

Every promised challenge must be delivered by a tagged recipe the blueprint
uses. Otherwise validation fails and names the recipes that would deliver it:

```
identity.challenges promises Council but no encounter delivers it; recipes tagged with it: twin_abbots
```

The generator reports the challenges as delivered in its checks.

### Recipes 2.0

```toml
[recipes.warden_screen]
name = "Warden's Screen"
tests = "a shield wall guarding two healer-acolytes: get past the frontal to interrupt the heals"
room = "hall"
tags = ["protect_caster", "interrupts", "frontal"]
formation = "screen"         # line | wedge | screen | ring
placement = "guard_exit"     # far | guard_exit | center | patrol
space = { ranged_lane = true, min_size = 0.0, pillars = false }
mobs = [
  { template = "warden", place = "front" },
  { template = "acolyte", place = "back", count = 2 },
]

[recipes.sentinel_patrol]
placement = "patrol"
patrol = { length = 14.0 }
```

### Placement scorer (`placement.rs`)

* **Candidates**: anchors on a 1.5 m lattice over the room's real floor.
* **Formation**: the formation and the anchor fix every member's spot, facing
  the way in.
* **Rejected** when any member is:
  * off the floor (1 m margin);
  * close enough to aggro the party waiting at the staging spot;
  * within aggro + 6 m of another pack at a similar height;
  * within 2.5 m of a doorway.
* **Ranged lane**: a recipe that needs one also needs a floor spot 12–18 m
  from the pack with line of sight to it (or, in a small room, a staging
  spot 10–20 m away that sees it).
* **Score**: the preference (far from the entry, in front of another exit, in
  the middle), plus 0.12 × the free floor within 7 m (room to dodge). The
  best candidate wins; ties go to lattice order, so placement is
  deterministic.
* **Patrols**: routes run across the room, at right angles to the entry. Each
  leg is sampled for floor and checked as a navigation path. Members walk in
  single file.

When nothing fits, generation fails with the recipe and the room, for example
`space: bone_archers in Hall 'Bone Colonnade'`. After 64 attempts the
diagnosis suggests a bigger room kind or a smaller requirement.

### Simulation (opt-in, RULES.md "Patrols, line-of-sight pulses and cover")

* `patrol = [[x, z], ...]` on a mob spawn.
* `los_blockable = true` on a `pulse`.
* Cover AI for companions.

A dungeon that doesn't use these is unaffected.

### Analysis: `dungeon-forge report`

```
dungeon-forge report <dungeon.toml|blueprint.toml> [--seed N | --layouts A..B] [--runs N]
                     [--sizes ..] [--profiles ..] [--classes ..] [--difficulties ..]
                     [--json FILE.json] [--svg DIR]
```

Per encounter:
* runs cleared, pulls, resets and wipes;
* party deaths, attributed to the encounter most recently pulled;
* killing blows ("ability (source)");
* deaths by class;
* mean clear time from the successful pull;
* optional completion (cleared/runs on optional rows).

Per room: dithers and stuck recoveries per run. These come from
`Sim::nav_log`, kept outside the event log so replay hashes don't change.

Per layout: wins and mean duration.

`--json` writes all of it. `--svg DIR` draws each layout's map with death
markers (red ×), dithers (cyan) and stuck recoveries (yellow). Maps also draw
patrol routes now.

Example (crypt layout 1, 20 runs, 4 player classes; abridged):

```
encounter                          cleared  pulls resets wipes deaths clear s  top killing blows
Warden's Screen                      20/20     20      0     0      0      33
The Twin Abbots                      20/20     20      0     0      0      86
The Hollow Guardian                  19/20     20      1     1      8     201  Sepulchral Wail x5, Last Wail x2, Melee (Bone Shard) x1
movement trouble per run (dithers / stuck recoveries):
  Sepulchre of the Guardian                  35.5 / 0.0
```

### Designer overrides (proof of concept)

`dungeon-forge generate ... --overrides FILE.toml` applies a patch to the
generated dungeon and loads the result through the game's loader:

```toml
[[encounters]]
id = "e6_hollow_guardian"     # arrays of tables match by id, else name, else index = N
name = "The Warden of Bones"

[[encounters]]
id = "o1_optional_elite"
remove = true                  # or new = true to append

[dungeon]
description = "Edited by hand."   # tables merge; other values replace
```

* **Re-applying**: regenerating the same seed and applying the same patch
  gives the same file (tested).
* **Stale override**: one whose target no longer exists stops generation,
  names the key and says the layout has changed.
* **Invalid result**: one that would make the dungeon invalid is rejected
  by the loader with its usual message, for example a boss moved onto the
  wrong floor, or loot left pointing at a removed encounter.
* **Limits**: overrides address generated ids and names. Room names come
  from seeded name lists, so a patch written for one seed is not meant for
  another. Geometry edits (moving areas) are possible but not re-checked by
  the structure checks. The proof of concept covers the common case:
  renaming, retuning, moving or removing encounters and loot.

## The two dungeons

**The Emberforge Vaults (volcanic, improved).**
* **Layout**: irregular caverns (per-side jitter, cut corners), angled
  passages and bends, a pit room, a lava bridge and an overlook with a
  one-way drop.
* **Fights**: a pack, an ash-hound **patrol**, a guarded pyrecaller,
  Forgewarden Hask, a frontal pack, the optional elite and Cindermaw.
* **Promises**: area control, patrol, frontal, interrupts, summons.

**The Hollow Reliquary (crypt, new).** A buried monastery laid out on a
tighter grid along a processional axis, with:
* narrow passages (3–4.5 m against 5–6 m);
* naves with two rows of **solid pillars** (they block movement and sight);
* burial cells off the nave, a parallel aisle and a cloister loop;
* a stair down to the lower crypt;
* cold light and the candle-stand prop.

Fights:
* the **Warden's Screen**: a frontal shield wall in front of healers;
* **Sentinel Patrol**;
* **Bone Archers**: ranged, in a pillared hall; line of sight;
* the **Twin Abbots**: a council. Vael heals Mire, Mire chants party-wide
  dirges, and both have phases;
* the **Censer Procession**: ground clouds and interrupts;
* the **Hollow Guardian**, in a four-pillar sepulchre:
  * *Sepulchral Wail*, a line-of-sight pulse every 18 s, then *Last Wail*
    every 12 s in phase 3;
  * Collapse rings;
  * the frontal Grave Hammer;
  * bone-shard adds.
* Optional: the Sealed Revenant and the Crawler Nest.

Same generator, same validation, different kit and blueprint. Maps:
`assets/forge/maps/`. Screenshots: see Verification.

## Comparative evaluation

Ten layouts (seeds 1–10) per set:
* **Geometry**: read from the generated files.
* **Simulation**: 4 player classes × 3 runs per layout, 120 runs per set,
  Balanced companions, Normal.
* **Builds**: "0.6" is the 0.6 generator run by the 0.6 build; the 0.7 rows
  use this build.

| | 0.6 Emberforge | 0.7 Emberforge | 0.7 Reliquary |
|---|---|---|---|
| rooms per layout | 11.1 | 13.9 | 18.0 |
| corridors | 13.1 | 15.3 | 19.8 |
| distinct room footprints | 8.1 | 12.1 | 10.8 |
| room-area spread (CV) | 0.46 | 0.55 | 0.65 |
| non-rectangular rooms | 5.0 | 10.3 | 0 (cut stone by design) |
| corridor length mean / max (m) | 18.3 / 26 | 21.5 / 30 | 19.1 / 27 |
| corridor width (m) | 5.7 | 5.3 | 3.6 |
| floor heights | 4.7 | 4.6 | 3.6 |
| patrolling mobs | 0 | 2 | 2 |
| challenge tags delivered | — | 7 | 9 |
| walk through every encounter (m) | 330 | 452 | 405 |
| wins | 120/120 | 120/120 | 112/120 |
| mean duration (s) | 383 | 403 | 481 |
| party deaths per run | 0.01 | 0.00 | 0.47 |
| dithers per run | 46 | 50 | 71 |
| generation: success, attempts, time | 10/10, 4.7, 11 ms | 10/10, 1.5, 11 ms | 10/10, 1.7, 33 ms |

The 0.6 layouts run with the 0.7 AI give the same picture: 120/120, 382 s,
0.01 deaths and 45 dithers per run. The AI changes don't move the 0.6 content.

Reading it:
* **Variety** (rooms, footprints, spread, walk length): 0.7 Emberforge varies
  more than 0.6 within one theme. The crypt varies along different axes:
  more, smaller and narrower spaces, pillars, burial cells and a loop.
* **Difficulty**: the crypt is the harder dungeon. Every lost run and nearly
  every death is at the Hollow Guardian. Per layout the wins are 12, 11, 12,
  9, 12, 9, 12, 11, 12 and 12 out of 12; why layouts 4 and 6 lose more has
  not been diagnosed. Simulated win rates are a feasibility check, not a
  measure of fun.
* **Dithering**: the crypt's dithers are mostly companions moving into and
  out of cover in the Guardian's arena (about 35 per run there). Turning back
  when the wail ends is counted as a reversal. Stuck time is 0.

### Shipped layouts

| file | seed | why |
|---|---|---|
| `07_emberforge_vaults.toml` | 1 | same seed as 0.6; compact, every feature |
| `variants/forge/emberforge_vaults_s6.toml`, `_s8.toml` | 6, 8 | as in 0.6 |
| `08_hollow_reliquary.toml` | 1 | 12/12, a compact axis with the loop |
| `variants/forge/hollow_reliquary_s5.toml` | 5 | a straight western axis with angled passages |
| `variants/forge/hollow_reliquary_s10.toml` | 10 | the longest route (568 s) |

## AI: problems found, fixed and measured

Each problem was reproduced on a seed with `diag_replay`. The new
`DIAG_TRACE=N` prints, every half second, the boss's cast and each member's
position, goal and visibility. Each fix was then measured.

1. **Hiding from line-of-sight pulses** (Phase B). Without cover logic a
   pulse hits everyone; with it, a companion hides behind a pillar. Test
   `ai_companions_take_cover_from_los_pulses`: 1 hit out of 35 possible.
2. **Cover flicker.** Two shadows at similar distances took turns as the
   companion moved, so it never arrived. Fixes:
   * cover must be hidden 0.7 m to either side, so it is deep in the shadow;
   * a companion holds still once hidden;
   * the chosen cover is kept while it still hides.

   Crypt wins rose from 75–85% to 85–95%.
3. **Parked damage dealers idle.** On crypt layout 1 the first pull timed
   out: DPS stood where they had chosen to stand (behind a pillar, or out of
   the warden's cleave cone) and could not hit the kill target. Now, after
   3 s parked without attacking, they switch to a tank-held enemy they can
   hit. The replay wins in 496 s. Rollout:
   * The first version (no idle time, any enemy) changed 812 of 1120 legacy
     runs and added deaths on the Ashen Spire.
   * The final version changes 582 runs.
   * Over all 1120 handwritten-dungeon runs (7 dungeons × 8 configurations
     × 20 seeds) against 0.6: **843 → 846 wins, 1458 → 1457 deaths**.
   * Ashen Spire with a Striker player: 0 → 9 deaths in 3 of 20 runs, all
     at the final boss, still 20/20 wins. On seed 1 the final fight ran 3 s
     longer, and the tank, healer and one DPS died in its last 10 s.
4. **Cover vs cleave cones (the Guardian wipes).** `dungeon-forge report`
   put every crypt wipe at the Hollow Guardian, most of them from the wails.
   `DIAG_TRACE` on a wipe showed what happened:
   * the cover goal was rewritten by the cone detour (walking around the
     Grave Hammer cone);
   * the detour point is not cover, so the sticky check dropped it;
   * the next search picked the shadow on the other side of the pillar;
   * the companions shuttled across the arena until Last Wail hit all four.

   Cover is now preferred where the companion can walk to it without crossing
   a cleave cone. Measured on 3 layouts × 4 classes × 5 runs:

   | | before | after |
   |---|---|---|
   | Guardian deaths | 51 | 29 |
   | Guardian wipes | 7 | 4 |
   | killing blows from the wails | 34 | 15 |

   Handwritten dungeons have no line-of-sight pulses, so they are unchanged
   (identical comparison output).

Every other 0.7 simulation change produces hashes identical to 0.6 on the
handwritten dungeons: patrols, line-of-sight pulses, the cover code and
`nav_log`. Check: retarget disabled, 280 runs, 0 differences.

Not fixed in 0.7 (measured, listed in TASKS.md):
* melee still converge on the same flank spot;
* companions reversing in and out of cover inflate the dither count;
* the remaining Guardian deaths (killing blows still mostly the wails, then
  bone-shard adds) are not yet diagnosed.

## Verification

Commands and results on this build:
* `cargo fmt --all -- --check`, `cargo check --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`: clean.
* `cargo test --workspace --release`: forge 18, game 15, sim 161 (1
  ignored: `diag_replay`), all pass. New forge tests:
  * crypt determinism and identity, pillars as holes, patrol, council and
    line-of-sight pulse;
  * the two kits differ (cell, pillars, lava, enemies);
  * a broken identity promise is rejected with the recipe that would keep it;
  * a recipe that can't fit names itself;
  * the report attributes clears and deaths, and the overlay renders;
  * 5-player crypt clears (≥ 75% over 2 layouts × 2 classes × 2 seeds, no
    wipe before the Guardian, no stuck time);
  * designer overrides: apply, re-apply, stale and invalid.
* **Handwritten regression**: covered under "AI" above.
* **Generated matrix** on the shipped files: 10 runs per configuration, sizes
  5 and 10, profiles Balanced, Aggressive and Conservative, Normal and Heroic.
  Results in PROGRESS.md.
* **Graphical** (release build, `DUNGEON_SAVE_DIR` in a scratch folder),
  screenshots inspected:
  * `--area-tour` of the crypt:
    * entrance: Sealed Gate;
    * branch: Choir of Dust;
    * corridor: the entry bend;
    * encounter space: Bone Colonnade;
    * elevated: Narrow Stair ramp and the Hollow Choir stair;
    * arena: Sepulchre of the Guardian.
  * `--area-tour` of Emberforge: Emberwatch Door, Hall of Bellows, a slant
    passage, Furnace Hall, the Watchers' Shelf ledge, the Flue bridge and
    the Molten Sanctum.
  * `--nav-debug` tours of both: the pillar holes and the strip pieces line
    up with the walls; the shelf, ramp and drop outlines are correct.
  * `--demo --dungeon hollow_reliquary`: Victory in 465.7 s; the hub then
    offers the crypt as next.
  * `--input-test`, `--frame-test`, `--tooltip-test`, `--click-test`,
    `--hub-tour`: all pass. The hub's Dungeons tab lists the Hollow
    Reliquary.
* Windows package: see PROGRESS.md.

## Not verified

* **No recorded human playtest of 0.7.** Earlier versions have been
  playtested by the maintainer; no written notes are in the repository.
  Whether the crypt *feels* different to play, whether pillars and the wail
  read on screen, and whether the patrol timing is fair need a person.
  PLAYTEST_FORGE.md has a 0.7 section.
* Visual quality beyond "renders correctly": candle light levels, the
  readability of the void chasm.

## Continuation plan

1. Use the human playtest notes to choose between more kits and deeper
   encounters.
2. AI:
   * cover that accounts for the next wail's timing;
   * melee flank spreading;
   * don't count cover moves as dithers.
3. Overrides: geometry edits re-checked by the structure checks; keep
   overrides keyed to room roles rather than seeded names.
4. A third kit only after the two read as different in play.
