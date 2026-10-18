# Tasks

Prioritized, actionable work. Completed milestones are summarized at the bottom;
details and evidence live in PROGRESS.md.

## Next up (after 0.10)

00. **Class Design Lab playtest** (PLAYTEST_CLASS_LAB_0_10.md, 30–45
    minutes). Then decide:
    - which style (or neither) becomes the Striker's direction;
    - what the other classes take from it;
    - only then, talents and gear (concepts in CLASS_DESIGN_LAB_0_10.md).
0. **Recorded before/after playtest of 0.9.2/0.9.3 combat clarity and impact**:
   PLAYTEST_COMBAT_0_9_2.md. Open questions:
   - whether the recent-action lines get read during a fight;
   - whether their right-of-centre spot covers companions too often;
   - whether the fight numbers are useful.

   Only then expand (e.g. Ward's shield size, a "lifesaving heal" cue,
   which needs pre-heal health in the event).
1. **Recorded human playtest of 0.9/0.9.1**: PLAYTEST_REMIX_0_9.md (three
   Hollow Reliquary layouts, one Emberforge layout, the new loot and treasure
   lines). No human observations of 0.9 exist yet; whether routes feel like
   choices, clues are noticed and replays stay interesting is not validated.
2. **Branch follow-ups** (DUNGEON_REMIX_0_9.md, DUNGEON_POLISH_0_9_1.md):
   - flank ratings rest on 12-16 AI runs per layout; re-measure with more
     runs, and only promote a pack to "favoured" with consistent evidence;
     in the crypt only the Censer Procession is a flank target now;
   - against AI parties the safe way is about as fast as timing the vents;
     tune only with player evidence;
   - Ashen Spire (handwritten) overpulls in about 1 run in 6, with or
     without 0.9 trail following: a layout matter, not the AI;
   - side branches two rooms deep exist (`depth = 2`) but no shipped
     blueprint uses them yet.

3. **Viewer → Studio groundwork** (DUNGEON_PRESENTATION_0_8.md, "Next steps"):
   - turn a viewer selection into an override (0.7 format) and show
     overrides applied;
   - fade or cut modular walls between the camera and the player (walls drawn
     with modules don't fade yet).
4. **Forge: wall stub in passage mouths**: a corridor that overlaps a
   pillared hall's aisle by only 0.6 m leaves a navigation wall stub in its
   mouth (crypt seed 5). Change the overlap rule. This changes navigation and
   simulation, so measure it.
5. **Art**: textured or Blender-authored modular pieces with the same frames
   and sizes; sloped wall pieces for stairs and ramps; more wall variants.
6. **AI follow-ups measured in 0.7** (DUNGEON_FORGE_0_7.md, "AI"):
   - the remaining Hollow Guardian deaths (wails and bone shards; 4 wipes in
     60 runs; layouts 4 and 6 lose more, not yet diagnosed);
   - cover that accounts for the next wail;
   - don't count cover moves as dithers (they inflate the crypt's 71 per
     run);
   - melee converging on one flank spot;
   - first frontal sweeps catching arriving melee;
   - threat failures from healer-caster packs at the pull.
7. **Designer overrides beyond the proof of concept**:
   - re-run the structure checks after geometry edits;
   - key overrides to room roles rather than seeded names;
   - `test`/`report` with `--overrides`.
8. **Visibility**: only if art makes frames expensive; room grouping and
   doorways exist, and `--viewer-tour` measures it (DUNGEON_PRESENTATION_0_8.md,
   "Visibility").
9. **Forge quality**:
   - free-form (non-grid) placement for more organic caves;
   - room-level culling if frame time ever needs it;
   - a third kit only after the two read as different in play.
10. **Cinderdeep tuning from human feedback** (Normal may be too easy at level
   5); a camera rule for very large bosses.
11. **Decor and props blocking sight**: decide whether large scenery should
   become solid volumes in `Nav`.
12. **Release**: run the manual GitHub Actions workflow once pushing is
   authorised; test the ZIP on another machine and on Windows 10.
13. **UI follow-ups from 0.3**:
   - per-aura icons with tooltips;
   - several debuffs on compact raid frames;
   - key rebinding;
   - a settings panel.
14. **Editor, part 3**: add or remove abilities and phases, place mobs
    in-game. Consider a forge-backed "Studio" later.
15. **Raid tuning, economy and hub**:
    - Cinder Court damage between ×1.0 and ×1.4;
    - vendor rotation;
    - stash scrolling.

## Done

- 0.10: Class Design Lab (CLASS_DESIGN_LAB_0_10.md):
  - two experimental Striker styles, player-only and per session;
  - the switch, HUD, feed and effects for them;
  - `dungeon-headless --lab` measurements;
  - talent and gear concepts.

  The classic game is hash-identical.
- 0.9.3: ability impact (ABILITY_IMPACT_0_9_3.md):
  - effects of your own, and hit reactions;
  - area rings at the real radius;
  - interrupt bursts;
  - shield, defensive and crowd-control looks;
  - presentation only.
- 0.9.2: combat clarity (COMBAT_CLARITY_0_9_2.md):
  - recent-action lines from your own events;
  - ranked floating text;
  - per-fight numbers (K);
  - presentation only; the simulation is hash-identical to 0.9.1.
- 0.9.1: polish: measured flank ratings steer flank targets (no flank
  into a pack it only makes harder); loot names its source and the results
  and HUD show treasure opened or missed.
- 0.9: Dungeon Remix (DUNGEON_REMIX_0_9.md): purposeful routes and side
  chambers, measured branch validation, `audit` and `routes`, treasure
  caches, companion trail following, Classic/Remix/Seed entry in the hub.
- 0.8.1: viewer polish, map navigation and exploration design:
  - speed bar, presets and smoothing;
  - viewer preferences;
  - the 2D map;
  - inspection lighting;
  - purpose classification;
  - the branch-purpose audit;
  - PLAYTEST_NOTES_0_8.md.
- 0.8: dungeon viewer and modular environments:
  - a viewer mode on the shared environment renderer (`env.rs`), with
    camera modes, rooms, layers, inspector, tours, seed switching and a
    self-test;
  - modular kit pieces along navigation walls, doorways and pillars;
  - decoration anchors and rules;
  - per-room atmosphere;
  - room structure in generated files;
  - `dungeon-forge view`;
  - packaging (forge, kits, viewer launcher).

  See DUNGEON_PRESENTATION_0_8.md.
- 0.7: meaningful dungeon variety and encounter design:
  - kit components (pitch, widths, room shapes, pillared halls, chasm);
  - blueprint identity with promised challenges;
  - Recipes 2.0 with a placement scorer;
  - patrols and line-of-sight pulses with cover AI;
  - the crypt kit and The Hollow Reliquary;
  - the improved Emberforge;
  - `dungeon-forge report` (JSON, SVG overlays);
  - an overrides proof of concept;
  - parked-DPS retarget;
  - corrected playtest statements.

  See DUNGEON_FORGE_0_7.md.
- 0.6: Dungeon Forge foundation:
  - blueprint and kit formats;
  - deterministic grid generator with bounded attempts and diagnostics;
  - room and corridor components;
  - structural and encounter-space validation;
  - the `dungeon-forge` CLI (validate, generate, inspect, test, seeds; SVG
    maps);
  - the Emberforge Vaults (generated, playable) and two more layouts;
  - fixes for a stale dither commitment and pit-corner portals;
  - bounds-pruned projection (−25–32% per tick on Cinderdeep);
  - GLB props with fallback and provenance.

  See DUNGEON_FORGE_0_6.md.
- 0.5: The Cinderdeep Hollows (original lava-cavern dungeon, 8 encounters, 3
  optional, branch, one-way drop, bridge, return tunnel, island boss); convex
  polygon areas and ramps in any direction; decor, lava polygons, lights,
  fog and tints; portal-edge, cone-blocked melee, walkable ranged spot and
  overpull fixes; soft separation; Windows packaging script, HOW_TO_PLAY,
  manual CI artifact workflow. See DUNGEON_0_5.md.
- 0.1: M0–M5 prototype (hub → dungeon → boss → rewards loop, headless runner).
- 0.2 Phase A: camera collision/fade, targeting, nameplates, floating text, VFX,
  danger warnings, clickable tabbed hub, results screen.
- 0.2 Phase B: stats, mitigation, hit tables, resources, auras, interrupts, CC,
  dispels, five abilities per class.
- 0.2 Phase C: blackboard, five profiles, evaluation tooling, two evidence-driven
  AI fixes.
- 0.2 Phase D/F: content schema 2.0, second dungeon, difficulties, variants.
- 0.2 Phase E: levels/XP, 7-slot gear with rarity and roles, auto-equip, history.
- 0.2 Phase G: configurable party sizes, multiple tanks/healers, benchmarks,
  10-person raid groups in the game.
- 0.4: layered 2.5D navigation (heights, ramps, decks, drops, weighted
  routing), elevation-aware range / line of sight / ground effects / aggro,
  LOS-aware AI, stuck and dither handling, renderer from the same Nav, camera
  under ceilings, navigation debug overlay, Cavern Traverse. See
  NAVIGATION_0_4.md.
- 0.3: MMO controls and camera auto-follow; clickable party/raid and ToT
  frames; mouseover healing; UI click-through blocking; action bar with
  range/GCD/cooldown/resource states; threat-coloured plates, target-frame
  threat and aggro alerts; resource readouts; tooltip framework; readability
  pass. See PLAYTEST_0_3.md.
- 0.2 extras: learning companions (familiarity/alertness), summoned-add threat
  inheritance, in-game Sandbox tab, third dungeon (Ashen Spire), vendor and
  salvage, in-game encounter editor, cone-avoiding movement.
