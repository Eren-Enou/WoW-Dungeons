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

(Filled in as stages complete.)
