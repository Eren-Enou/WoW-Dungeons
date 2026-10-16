# Branch purpose review (0.8.1)

This review looks at what the branches in generated dungeons are for, and
whether they currently earn their place. It is an evaluation, not a design
change: 0.8.1 adds no reward or encounter-composition system. The
recommendations at the end are proposals for 0.9.

## Method

- **Layouts.** Ten seeds (1 to 10) of each forge blueprint were generated
  with `dungeon-forge seeds <blueprint> --from 1 --count 10`. The blueprints
  are `emberforge_vaults` (volcanic kit) and `hollow_reliquary` (crypt kit).
  Both gave 10/10 seeds with 10 distinct route shapes.
- **Audit script.** A scratch Python script read each file's
  `[[dungeon.rooms]]` (route, role, order, encounter, connects), its
  encounters (the `optional` flag and mob ranks) and its loot sources. It then
  measured:
  - the purpose counts;
  - where each alternate route rejoins the main route, what it bypasses, and
    its length against the main route (straight lines between room centres);
  - side-chamber depth and dead ends;
  - loops and overlook drops;
  - optional-encounter rewards.
- **Viewer.** Representative layouts (the shipped seeds and the tour seeds)
  were inspected in the viewer
  (`--viewer --dungeon <file>`):
  - on the new 2D map (M), with purpose colours;
  - with the structure layer (key 4);
  - with the inspector's new purpose line (purpose, dead end, rooms off the
    main route).
- **Rules.** Checked against RULES.md: victory needs every non-optional
  encounter cleared. Companion AI pulls the first uncleared encounter in file
  order (`crates/sim/src/ai.rs`, the `position(|e| !e.cleared ...)` pick).

The classification used by the viewer, the map and this report comes from
`crates/game/src/structure.rs` (`Purpose`):

| Purpose | Meaning |
|---|---|
| main route | the ordered rooms from the entrance to the boss |
| alternate route | a second way between two main-route rooms |
| optional encounter chamber | a side room holding an encounter marked `optional` |
| empty side chamber | a side room with nothing in it |
| overlook / shortcut | a raised room and walkway above a one-way drop to a later main room |
| return loop | a corridor, plus any rooms along it, back to an earlier main room |
| passage | an ordinary corridor |

During the audit, the crypt's cloister-walk rooms turned out to be tagged
`alternate` by the forge, although they only sit between loop corridors. They
are now classified as **return loop**, so they no longer read as a second
aisle.

## Findings

### Totals over ten seeds each

| | Emberforge Vaults | Hollow Reliquary |
|---|---|---|
| Main-route rooms per layout | 8–12 | 7–10 |
| Alternate routes | 10, one per layout (14 rooms) | 10, one per layout (24 rooms) |
| Alternate routes holding an encounter | **0** | **0** |
| Alternate routes bypassing a required pack | 7 | 7 |
| Alternate length / main length | 0.98–1.03 (one 1.97) | 1.0 or 2.0 (7 of 10 are 2.0) |
| Optional encounter chambers | 10 (all elite, all with dedicated loot) | 20 (10 elite with loot, 10 trash rush without loot) |
| Empty side chambers | 5 | 13 |
| Side chamber depth from the main route | always 1 room | always 1 room |
| Side chambers that are dead ends | 15 / 15 | 33 / 33 |
| Overlook drops | 10, each skipping one main room | 0 |
| Return loops | 0 | 10 (direct corridor, or 2–5 passage rooms) |

### Alternate routes do not create meaningful choices yet

- **Nothing on them.** No alternate route in 20 layouts holds an encounter,
  a reward, a hazard or a vantage point. Each is an empty chain of rooms.
- **The skip isn't real.** In 14 of 20, the route bypasses a main room whose
  pack is required for victory. Taking the alternate therefore only postpones
  that pack: the party must come back for it. Companions ignore the choice
  altogether, because they walk to the next uncleared encounter in order.
- **They are equal or worse.**
  - The Emberforge alternates are almost exactly as long as the main route
    they parallel (within about 3%), so neither path is shorter, safer or more
    interesting.
  - Most Hollow Reliquary alternates are twice as long, with three empty
    chambers, so they are simply worse.
- **Navigation value only.** The one thing they do is make the map read as a
  network rather than a line. On the 2D map and the structure layer that is
  visible and attractive, but in play it is not yet a decision.

### Empty side chambers feel arbitrary

- **Always shallow dead ends.** Every side chamber (48 of 48) is exactly one
  room off the main route and a dead end. That is good for discoverability,
  because each one is a single doorway off a main room. But nothing tells an
  empty one apart from an optional one until you walk in.
- **Nothing to find.** An empty chamber has no loot, no prop set of its own,
  no lore and no view. The crypt gives side rooms their own candle-lit
  atmosphere, which makes them feel deliberate. The volcanic kit has no
  `[atmosphere.side]`, so its empty chambers are just small caves.
- **Placement looks random.** The blueprint asks for `side_rooms = [1, 2]`, and
  each side slot is filled with an optional encounter until the optionals run
  out. The rest stay empty. That is why the crypt (two optionals, 1–2 extra
  side rooms) has 13 empty chambers against Emberforge's 5. Emptiness is a
  by-product of counts, not an intent.

### What works

- **Optional chambers are easy to find**, structurally: always one doorway off
  a main room. Emberforge's are all elite and all have loot, which is a clear
  promise. Whether players actually notice and choose them has not been
  playtested.
- **The overlook drop is the only branch that changes play.** In every
  Emberforge layout it gives a one-way way down that skips one main room. On
  the map it is the most legible branch: orange outline, dashed drop.
- **The return loop has a clear purpose:** a walk back to an earlier hall
  after a wipe, or to reach a missed side chamber. It is labelled as such now.

### Limitations of this review

- **Structural and file-based only.** No human has played these layouts for
  this review. The questions in PLAYTEST_NOTES_0_8.md about how different
  layouts feel, and whether side branches feel worthwhile, remain open.
- **Path lengths are approximate.** They are straight lines between room
  centres, not nav paths.
- **Route use wasn't measured.** It was not checked whether companion paths
  actually pass through alternate rooms. The pathfinder takes the shortest nav
  path to the next pull, so near-equal Emberforge alternates may sometimes be
  walked by accident.

## Recommendations for 0.9

Smallest and most valuable first. None of these needs new enemies, new
mechanics or a loot system beyond the existing loot tables.

1. **Give every alternate route a reason, chosen by the blueprint.** Options:
   - `alternate = "detour"`: it holds an optional encounter or an existing
     loot source;
   - `alternate = "flank"`: it bypasses only rooms with no required pack, and
     ends behind the next pull (a positional choice for the pull);
   - `alternate = "safe"`: it bypasses a hazard room (a lava pit, a patrol)
     at the cost of length.

   The forge should reject an alternate that bypasses a required pack unless
   it is meant to flank that pack.
2. **Make empty side chambers an explicit, small choice.**
   - Add blueprint `side_rooms` intents: `optional`, `cache` (an existing loot
     table, no fight), `vista` (a view over a later room or the boss), or
     `none`.
   - Stop leaving leftover slots empty by default.
   - Generate at most one truly empty chamber per layout, and give it a kit
     prop set (`[[decorate]] rooms = ["side"]`) so it reads as a place.
3. **Signal optional chambers at the doorway.** Use a kit-defined door
   decoration on side rooms with an optional encounter (for example a sealed
   door frame or banner) so they are recognisable before entering.
   Presentation only; no rules change.
4. **Add `[atmosphere.side]` to the volcanic kit** so its side chambers read
   differently from the main caves, as the crypt's already do.
5. **Allow side branches deeper than one room** when a blueprint asks
   (`side_depth = [1, 2]`), so exploration is sometimes a short excursion
   rather than a glance through a door.
6. **Let companions follow the player's route choice.** For example, when the
   player is on an alternate route, path through it to the next pull.
   Otherwise route choices only matter to a solo player walking ahead. Check
   it with the usual 100+ seed batches; it changes AI movement and so the log
   hashes.
7. **Add an audit command to the forge.** Something like
   `dungeon-forge audit <blueprint> --seeds 1..10`, reporting the table above.
   Branch purpose then stays measurable as kits grow, and recommendation 1
   can become a validation rule.

Validate 1 to 3 with a human playtest before building further: walk three
seeds per kit and record whether alternates and side rooms were noticed,
taken and worth it.

## What 0.9 did with these recommendations

DUNGEON_REMIX_0_9.md has the details and measurements.

1. **Alternate routes with a reason:** done. `[[routes]]` purpose detour,
   flank or safe; an attempt is rejected unless the branch delivers its
   purpose.
2. **Side-chamber intents:** done. `[[side]]` purpose optional, cache, vista
   or quiet, with at most one quiet room.
3. **Doorway signals:** done. `branch_door` anchors and kit rules.
4. **`[atmosphere.side]` for the volcanic kit:** done, plus vault and vista
   presets for both kits.
5. **Deeper side branches:** supported (`depth = 2`); not used by the
   shipped blueprints yet.
6. **Companions following the player's route:** done (trail following).
7. **`dungeon-forge audit`:** done. Over 100 layouts per blueprint it now
   reports 0 purposeless branches, against 241 (crypt) and 149 (volcanic)
   for the 0.8.1 blueprints.
