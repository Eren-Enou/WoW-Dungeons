# Dungeon Remix (0.9)

Purposeful exploration, tactical routes and replayable procedural dungeons.

This document is the plan and, as phases land, the record of what was
built and measured. Results live in the "Results" section at the end; the
plan above it says what each phase must prove.

## Starting point (0.8.1)

What exists, from reading the code (not the docs):

- **Forge layout** (`crates/forge/src/layout.rs`): a self-avoiding walk on a
  grid of cells makes the main route. Then come:
  - `alternate_routes`: a BFS path of 1–3 free cells between main rooms `i`
    and `i+2`/`i+3`, never around a boss;
  - side rooms: optional encounters first, then `side_rooms` empty chambers,
    each one cell off a random main room;
  - a `drop` or `loop` shortcut.

  Alternates and side rooms are placed only to fill the counts; nothing
  gives them a job.
- **Populate** (`populate.rs`) places recipes with the placement scorer
  (`placement.rs`), which faces packs toward the main entry and keeps them
  out of aggro of the entry staging spot. Loot sources are kit roles
  (`pack`, `optional`, `boss`, `final_boss`). `optional` maps to the
  *first* optional encounter only, which is why the crypt's second optional
  (Crawler Nest) drops nothing.
- **Progression** (`sim.rs`, RULES.md): victory when every non-optional
  encounter is cleared. Hazards belong to an encounter and pulse until it is
  cleared.
- **AI** (`ai.rs`):
  - In the game, the player leads. Companions out of combat walk to
    formation slots behind the leader (`follow_point`, up to 5 m to the side
    and 8 m back, snapped to the nearest floor).
  - Only the headless/demo `autopilot` picks "the first uncleared encounter
    in file order". Companions never start fights out of combat; packs
    start them by aggro.
- **Branch purpose** (0.8.1): the viewer classifies rooms from the route
  data. BRANCH_PURPOSE_0_8_1.md measured 20 layouts:
  - no alternate holds anything;
  - 14 of 20 bypass a required pack;
  - every side chamber is a one-room dead end;
  - empty ones are leftovers of slot counts.
- **Seeds in the game**: the viewer already runs the external
  `dungeon-forge` executable (`viewer::generate_layout`), writing to
  `forge_out/viewer/`. That path is relative to the working directory, which
  0.9 fixes for gameplay use.

Risks found while reading:

1. **Formation snapping.** Companions' formation slots are snapped to the
   *nearest* floor. When the leader walks a corridor that runs beside
   another (an alternate beside the main route), a slot can snap into the
   other corridor. That companion then walks the other way round, possibly
   into a pack. This is the concrete reason companions might not follow a
   chosen route.
2. **Flank pulls.** A second door into a pack's room can sit inside the
   pack's aggro radius, so arriving by the flank would pull by accident.
3. **Second optional's loot.** Optional loot only reaches the first optional
   encounter; any reward detour holding the second would be unrewarded.

## Design rules

- **Fixed:** a dungeon keeps its identity (kit, theme, the blueprint's
  encounter list in order, bosses, completion rules).
- **Varies by seed:** rooms, corridors, routes, side content and pack
  placement.
- **Every branch declares a purpose** in the blueprint, and the generator
  must *measure* that it delivers it, or the layout attempt is rejected.
- **Required means required.** Victory still needs every non-optional
  encounter. Branches may change how a required room is *approached*, but
  never let the party pass a required room without a way back to it on the
  way forward. No branch goes round a boss.

## Generation schema (blueprint)

New, optional tables. Old blueprints keep working; their alternates and
empty side rooms are reported as "unspecified" by the audit.

```toml
[[routes]]                 # purposeful alternate routes
purpose = "detour"         # detour | flank | safe
optional = 2               # detour: the [[optional]] encounter it holds (1-based)
cache = true               # detour: a treasure cache at its far room
target = "warden_screen"   # flank: the main recipe to flank (default: any eligible pack)

[[side]]                   # purposeful side chambers
purpose = "optional"       # optional | cache | vista | quiet
optional = 1               # optional: which [[optional]] (default: next unassigned)
depth = 2                  # 1 or 2 rooms deep
```

### Route purposes, and how each is measured

Lengths are walking lengths on the navigation (`Nav::path_points`), not
straight lines.

- **detour (reward detour).** A through-route that leaves main room `i` and
  rejoins at `i+1`, or at `i+2` when `i+1` holds no required encounter. It
  carries its reward: an optional encounter with its own loot, and/or a
  treasure cache.
  - *Valid when:* it holds a reward; it bypasses no required encounter; it
    rejoins the main route (no backtracking).
  - *Measured:* extra walking length against the main segment.
- **flank (tactical flank).** A route from main room `i` into the room of a
  required, non-boss pack `j`, entering through a different wall from the
  main entry. Rooms between `i` and `j` hold no required encounter.
  - *Valid when:* the flank door's staging spot is outside every pack
    member's aggro radius, with margin, so arriving does not pull. The flank
    door must also be at least 60° off the pack's front (the pack faces the
    main entry), so it is a side or rear approach.
  - *Measured:* the angle; the distance from the flank door to the pack's
    back row (casters, archers) against the main door's; walking lengths.
- **safe (safer or riskier).** Two ways from main room `i` to `j`. The
  short way (the main corridors) crosses pulsing vents: kit route hazards,
  owned by the next required encounter so they die with it. The long way
  (the alternate) is hazard-free.
  - *Valid when:* the alternate has no hazard on it; the short way has
    hazard coverage; the alternate is at least 15% longer.
  - Only kits that define a route hazard support it (volcanic: magma vents).
    The crypt kit rejects `safe` with a message.

### Side-chamber purposes

- **optional:** a non-required encounter, with its own loot (every optional
  encounter now gets the kit's `optional` loot pool, not only the first).
- **cache:** a treasure cache (below). No fight. Smaller pool, lower rarity.
- **vista:** a landmark room, decorated by kit rules for the `vista` role
  and lit by its own atmosphere. It is environmental only, and labelled as
  such; it is *not* a reward.
- **quiet:** a deliberately empty pacing room. At most one per layout.
- **depth = 2:** an antechamber before the chamber.

Legacy `side_rooms` fill as `quiet`, so the old empty-chamber behaviour stays
available.

### Treasure caches: the minimal compatible interaction

- **Data:** `[[dungeon.caches]]` with `id`, `name`, `pos`, `y`, `radius`, as
  dungeon data like mobs (the simulation reads it).
- **Opening:** the first party member to come within `radius`, on the same
  level, opens the cache. This emits `CacheOpened` and records the id in the
  run report.
- **Loot:** entries with `source = <cache id>` roll exactly like an
  encounter's drop: one roll, through the existing `roll_item`, with the
  difficulty's rarity bonus. On a wipe they behave like cleared encounters.
- **Renderer:** draws the kit's cache model (a GLB with a primitive
  fallback) and dims it once opened.
- **Unchanged without caches:** dungeons without caches produce identical
  event logs; this will be checked by hash.

### Readable branches (environmental clues)

- New decoration anchor kinds, placed by the composer at the room end of the
  doorway into a branch: `branch_door` (detour, flank, cache, optional) and
  `vista_door`.
- Kit `[[decorate]]` rules give each its own treatment: crypt ornate arch
  braziers and a sealed-tomb plaque; volcanic forge-gate posts and ember
  lamps.
- Clues hint that something is there, without revealing what. UI markers
  stay developer-only: the viewer's map and overlays.

### Output

- `[[dungeon.rooms]]` gains `purpose` and `branch`.
- `[[dungeon.branches]]` records, for each branch:
  - id and purpose;
  - its rooms and the main rooms it joins;
  - target and reward ids;
  - what it bypasses;
  - the measured facts (lengths, angle);
  - any warnings.

The simulation ignores both; the viewer, the map and the audit read them.
`GENERATOR_VERSION` becomes 4.

## Validation and the audit

`check::branches` runs on every attempt; a failed branch rejects the
attempt, with the cause tallied. It checks:

- every required encounter's room is reachable from the entrance by walking;
- the final boss is reachable from every required room;
- no branch bypasses a boss;
- detours bypass nothing required, rejoin, and hold a reward;
- flank staging is aggro-free and the approach angle is at least 60°;
- safe routes are hazard-free and at least 15% longer, and their risky
  counterpart has hazards;
- one-way drops: the drop is reachable without passing a required room, and
  every room the drop skips is still walkable back to from the landing.

`dungeon-forge audit <blueprint> --layouts A..B [--json FILE]` generates
layouts and reports:

- successes and distinct route shapes;
- per branch: purpose, nav lengths against the main route, bypassed
  required encounters, rewards;
- empty dead ends and unspecified branches;
- per-purpose pass counts.

It exits 2 when a declared purpose is not delivered.

## Companions and route choice

- **Follow the leader's trail.** When a companion's formation slot is not
  in sight of the leader (it snapped behind a wall or into another
  corridor), the companion walks to a point on the leader's recent trail
  instead. The trail is a breadcrumb list in the simulation.
  - It changes movement only out of combat and only when a slot is hidden,
    so combat AI is untouched.
  - It does change hashes. It is measured on all dungeons against 0.8.1.
- **Scripted headless leader.** A route script for the autopilot,
  `RunOptions::route`, a list of `Goto(point)`, `Pull(encounter)` and
  `Open(cache)`. It stands in for a player choosing a path, so that
  "companions follow a player-selected route" can be measured headless.
- **New run metrics:**
  - overpulls: an encounter engaged while another is still engaged, or one
    that is not the one being pulled;
  - regroup time: waiting for the party at each waypoint;
  - caches opened;
  - rooms visited by the leader and by the companions (from area names).
- **`dungeon-forge routes <dungeon>`:** runs each route choice of a layout
  (direct, each detour, each flank, safe or risky) over many seeds and
  configurations, and compares:
  - time and deaths;
  - hazard damage;
  - damage taken in the flanked fight;
  - rooms traversed;
  - stuck, dither, unreachable and overpulls.

  This is the evidence for "a meaningful choice".

## Playable seeded entry

- The Dungeons tab gets a **Layout** row, for forge dungeons only:
  - **Classic:** the shipped file.
  - **Remix:** a new random seed.
  - **Seed:** typed digits.
- Generation runs the external `dungeon-forge` executable, found beside the
  game or under `target/`. It is never linked into the game.
- Output goes to a managed folder: `generated/` next to the save, or under
  `DUNGEON_SAVE_DIR`. This is separate from authored content and the save
  file.
- The id stays the dungeon's own, so progression, loot ids and companions'
  familiarity carry over.
- The generated file is loaded through the same validator as everything
  else.
- Failure shows the forge's message, and the selection stays on the
  requested seed: no silent substitution.
- The seed and generator version are shown in the hub, the HUD and the
  results screen, and recorded in the run log.
- The packaged ZIP already contains `dungeon-forge.exe` and `assets/forge`.

## Stages and acceptance tests

| Stage | Proves | Acceptance |
|---|---|---|
| A: schema, layout, validation, audit | Purposes are declared, built and measured | Forge unit tests per purpose; `audit` passes on 100 seeds per blueprint; purposeless legacy alternates are flagged |
| B: side purposes, caches, clues | Optional branches carry content | Sim tests: a cache opens, rolls loot, no-cache hashes identical; viewer shows clues |
| C: viewer and map | Designers can see purposes | Map, structure layer and inspector use the file's purposes and branch facts; `--viewer-test` passes |
| D: companions | The party follows the chosen route | `routes` runs: the scripted route is traversed, with no stuck, unreachable or overpull regressions; 100+ seed batches against 0.8.1 on every dungeon |
| E: seeded entry | Remix is playable | Hub: classic, remix and seed; a reproducible file; a run starts, ends and returns to the hub; save untouched; packaged build tested |
| F: evidence | Three Hollow Reliquary runs, one Emberforge run | Seeds recorded with measured differences; screenshots looked at; PLAYTEST_REMIX_0_9.md written |

## Out of scope

- New enemies, abilities, classes or kits.
- A quest or treasure economy beyond one cache roll.
- Procedural trash composition.
- Proceduralising handwritten dungeons.
- A full Studio.

## Results

All numbers below come from runs on the final 0.9 build: generator v4, with
the blueprints and kits as committed. Simulated parties are AI parties. None
of this is a human playtest; PLAYTEST_REMIX_0_9.md asks the questions that
only a person can answer.

### What changed on the way (decisions made from measurements)

These were found by measuring, not by design review, and fixed in the
generator:

- **Flanks and detours shorter than the main route.** The shortest path ran
  through them, so the "direct" party pulled the optional fight by accident
  (Emberforge seed 4: the optional Sentinel was cleared in 12/12 direct
  runs).
  - Detours must now be at least x1.15 the main route, and flanks x1.1.
- **Safe routes placed inside the stretch a drop skips.** Parties dropped
  past both the vents and the safe way, which made the choice invisible.
  - Routes may no longer sit in a drop's span.
- **Vents narrower than their corridor.** A radius-2 m vent in a 5–6 m
  corridor left an edge to sidle past.
  - Vents now cover the corridor wall to wall and keep a pause spot between
    them.
- **Detours rejoining a patrol's hall.** The Sentinel Patrol walked to the
  doorway and joined the Crawler Nest fight (6 of 8 runs).
  - Detours and safe ways no longer open into patrol rooms.
- **Drops in 0.9 blueprints.** Drops now skip two or three empty rooms; a
  drop that skips only one room saves no walking.
  - The blueprint's extra rooms are grouped into one stretch so the drop has
    something to skip.
  - Shortcuts are placed before routes.
- **Attempt budget** raised from 64 to 512. Most failed attempts are drop
  geometry, which fails fast; generating 100 layouts takes 4–6 s.
- **Classic Hollow Reliquary is now seed 2.** Seed 1's Guardian arena lost
  20% of AI runs (48/60), against 95% wins on seed 2 (57/60). Across ten
  layouts, the Guardian's win rate on 0.9 layouts (224/240, 93%) matches
  0.8.1 layouts (228/240, 95%).

### Audit: 100 layouts per blueprint (`dungeon-forge audit --layouts 1..100`)

| | 0.8.1 blueprint | 0.9 blueprint |
|---|---|---|
| Hollow Reliquary generated | 100/100 | 100/100 (97 distinct shapes) |
| purposeless branches | 241 (141 empty dead ends) | 0 |
| alternates going round a required pack | 78 | 0 |
| Emberforge Vaults generated | 100/100 | 98/100 (seeds 46, 56 fail: drop geometry and flank angles; reported, not substituted) |
| purposeless branches | 149 (49 empty dead ends) | 0 |
| drops that skip a required pack | 45/100 | 0 |
| drops that save walking | 7/100 | 98/98 (x0.42–0.69 the main route) |

Every 0.9 branch delivers its purpose:

- **Hollow Reliquary** (100 each):
  - detours: walk x1.82–4.18 the main route, all with a reward;
  - flanks: x1.20–2.99, 60–77° off the pack's front;
  - every layout also has an optional chamber, a landmark and a loop.
- **Emberforge** (98 each):
  - detours: x1.18–4.21;
  - flanks: x1.15–3.00, 60–79° off the pack's front;
  - safe ways: x1.18–3.36;
  - drops and landmarks in every layout.
- **Determinism:** the same seed gives a byte-identical file (forge test,
  and `--remix-test` through the game).

### Route choices (`dungeon-forge routes`)

Setup:

- Scripted leader, companions on their normal AI.
- 4 classes (striker, warden, mender, arcanist), balanced profile, Normal,
  5 players.
- Optional fights are skipped except the one a detour holds.

Whole-run wins and deaths are mostly decided by the Hollow Guardian. Every
loss in the seed-4 runs, whichever route was taken, happened at the Guardian,
which every route reaches the same way. So they are reported but not used as
evidence for or against a branch. The evidence below is local to each
branch.

**Hollow Reliquary, 20 layouts × 12 runs per choice:**

- Companions walked every room of every branch in every run (100%), with
  0–2 overpulls per layout.
- **Detours:** the Crawler Nest and the Reliquary Coffer were taken in
  240/240 detour runs.
- **Flanks:** in 12 of 20 layouts the flank changes which enemy the party
  engages first:
  - Warden's Screen: the Hushed Acolyte healers instead of the Ossuary
    Warden;
  - Censer Procession: a crawler instead of the Censer Bearer.
- **Flanked-fight damage** against the same pack through the main door
  ranges from −11% to +40%; on average a flank does not reduce the AI
  party's damage in the crypt.

**Emberforge Vaults, layouts 1–10 (seeds 1–10) × 16 runs per choice:**

- **Safe way against the vented crossing:**
  - direct: 0–98 vent damage per run, and about 1.3–1.5× the dithers
    (waiting out pulses);
  - safe way: no vent damage (15 in one layout), for 0–14 s more.
- **Flanks** depend on the pack:
  - Pyrecaller's Guard (casters behind a shield wall): damage taken −21% to
    −42% in four of six layouts (−9%, +1% in the other two), with the fight
    5–10 s longer;
  - Emberkin Raiders: −15% and −17%;
  - Ash Hound Pack (frontal bites): +7% and +1%.
- **Detour:** the Obsidian Sentinel and the strongbox in 160/160 runs, for
  about +30 s. One run had deaths in the detour's fight.
- **Following:** companions walked every branch room in all runs.

### The three Hollow Reliquary runs (32 runs per choice)

Each layout is real, generated by the shipped blueprint. Each ships as
`assets/content/variants/forge/hollow_reliquary_s<N>.toml` for the viewer;
type the seed in the hub to play it.

**Run A: seed 19, direct progression**
- Main route: Pilgrims' Porch → … → the Hollow Guardian.
- The flank (Bone Gate → Cloister Walk) enters the Bone Archers only 60° off
  their front. In the sim that fight costs 13% more damage than through the
  main door (288 vs 254), and the opener is the same.
- The detour sits late (Sepulchre → Choir of Dust, +42 m).
- Measured: direct won 31/32 with 10 deaths, the best of the three choices.
- Nothing pulls the party off the main route; optional content is there for
  those who want it.

**Run B: seed 4, rewarding exploration**
- At the very start, a reward detour leaves the Pilgrims' Porch (+68 m) to
  the Crawler Nest and the Reliquary Coffer, and rejoins at the Hall of the
  Hushed.
- Measured: both rewards in 32/32 runs, no party deaths in the Crawler Nest
  fight, Guardian outcomes unaffected.
- The run as a whole won 32/32, against 26/32 direct. All direct losses were
  at the Guardian, so this difference is not credited to the detour.

**Run C: seed 16, tactical flank**
- A servants' passage (Crypt Turn → Narrow Stair → Cramped Passage) enters
  the Warden's Screen hall 83° off the pack's front, staged 7.7 m outside
  aggro.
- Measured: the party's first hit lands on a Hushed Acolyte (one of the
  healers) in 32/32 flank runs, against the Ossuary Warden in 32/32 direct
  runs.
- Damage taken in that fight: 405 vs 448 (−10%), fight length the same
  (30.7 vs 30.1 s).
- Whole-run wins 31/32 vs 28/32. As above, this is not credited to the
  flank.
- The change in approach is real; its value for an AI party is modest.

**Emberforge seed 2** shows the volcanic version of all three choices:
- the flank into the Pyrecaller's Guard takes −32% damage;
- the direct crossing costs 98 vent damage per run, the safe way none;
- the detour gives the Sentinel and the strongbox in 16/16 runs.

### Companions: trail following against 0.8.1 slot following

Same layouts and seeds. `--slot-follow yes` restores 0.8.1 following.
Measured on the generator one rule before the final one (the patrol rule
came later). Both sides used identical layouts.

| Hollow Reliquary, 6 layouts × 12 runs per choice | slot (0.8.1) | trail (0.9) |
|---|---|---|
| companions walked the whole flank | 95.8% | 100% |
| companions walked the whole detour | 97.2% | 100% |
| leader completed the branch | 139/144 | 144/144 |
| dithers, all choices | 20 587 | 14 780 |
| mean direct-run time | 476 s | 447 s |
| stuck / unreachable | 0 / 0 | 0 / 0 |
| overpulls | 0 | 2 |

On Emberforge, the wider caverns already gave 100% following either way,
with dithers and times about equal.

### Regression batches against 0.8.1

8 configurations × 15 seeds per dungeon:

- classes: striker, warden, mender, arcanist;
- 10 players;
- profiles: aggressive, conservative;
- Heroic.

**AI change alone:** both builds on the 0.8.1 content files, all 8 dungeons.

- 125/960 runs identical: companion movement changed, as intended.
- Wins 732 → 732; deaths 1220 → 1216; dithers −1.2%; stuck seconds 40 → 33;
  unreachable 0 → 0.
- Per dungeon, wins moved by at most 3 (Hollow Reliquary 98 → 101, Gloomroot
  100 → 99, Ashen Spire 89 → 87).
- Cinder Court (the raid) was nearly unchanged: 104/120 runs identical.
- Single configurations swing both ways. For example Ashen Spire warden
  deaths went 0 → 8, while Hollow Reliquary aggressive went 22 → 0, from
  divergent runs.
- New overpull metric: 19 on Ashen Spire and 5 on Cinderdeep, out of 120
  runs each; 0 elsewhere. The 0.8.1 build cannot measure overpulls.

**Shipped classic layouts:** 0.8.1 build and content against 0.9 build and
content.

| | wins | deaths | stuck | dithers | overpulls (0.9) |
|---|---|---|---|---|---|
| Emberforge Vaults (seed 1) | 105 → 105 | 75 → 78 | 0 → 0 | 7722 → 10 507 | 4 (3 of them 10-player) |
| Hollow Reliquary (seed 2) | 98 → 102 | 131 → 101 | 0 → 0 | 9925 → 8938 | 0 |

Emberforge runs are 10–30 s longer, and the extra dithers are parties waiting
out the magma vents on the classic layout's direct crossing. That is about
2.5 dithers per member per minute, under the `test` threshold of 6.

### Limitations

- **Human playtest.** No human has played 0.9. Enjoyment, pacing, whether
  the clues are noticed and whether replays stay interesting are not
  validated.
- **Flank value is layout-dependent.** It helps against screened casters in
  the volcanic kit and is neutral to slightly negative in the crypt.
  Blueprints cannot yet prefer good flank targets (TASKS).
- **Safe ways and AI parties.** AI parties time the vents well, so for them
  the safe way is about as fast and simply avoids damage. The short way is
  meant to be faster for a player who times it; that is not measured.
- **The viewer draws no hazards in 3D**, because it has no simulation; they
  show on the 2D map and in play.
- **Remix in the hub** rolls up to 8 seeds when one can't be built, naming
  the skipped seeds; about 2% of Emberforge seeds can't be built. A typed
  seed is never substituted.
- **`depth = 2` side branches** are supported and tested, but not used by
  the shipped blueprints.
