# Tasks

Prioritized, actionable work. Completed milestones are summarized at the bottom;
details and evidence live in PROGRESS.md.

## Next up (after 0.8)

1. **Recorded human playtests**: PLAYTEST_VIEWER.md (viewer and the new look)
   and PLAYTEST_FORGE.md (0.7 section: do the two dungeons play
   differently, is the Hollow Guardian fair). Write the notes into the
   repository; earlier versions were played without notes.
2. **Viewer → Studio groundwork** (DUNGEON_PRESENTATION_0_8.md, "Next steps"):
   - turn a viewer selection into an override (0.7 format) and show
     overrides applied;
   - fade or cut modular walls between the camera and the player (walls drawn
     with modules don't fade yet).
3. **Forge: wall stub in passage mouths**: a corridor that overlaps a
   pillared hall's aisle by only 0.6 m leaves a navigation wall stub in its
   mouth (crypt seed 5). Change the overlap rule. This changes navigation and
   simulation, so measure it.
4. **Art**: textured or Blender-authored modular pieces with the same frames
   and sizes; sloped wall pieces for stairs and ramps; more wall variants.
5. **AI follow-ups measured in 0.7** (DUNGEON_FORGE_0_7.md, "AI"):
   - the remaining Hollow Guardian deaths (wails and bone shards; 4 wipes in
     60 runs; layouts 4 and 6 lose more, not yet diagnosed);
   - cover that accounts for the next wail;
   - don't count cover moves as dithers (they inflate the crypt's 71 per
     run);
   - melee converging on one flank spot;
   - first frontal sweeps catching arriving melee;
   - threat failures from healer-caster packs at the pull.
6. **Designer overrides beyond the proof of concept**:
   - re-run the structure checks after geometry edits;
   - key overrides to room roles rather than seeded names;
   - `test`/`report` with `--overrides`.
7. **Visibility**: only if art makes frames expensive; room grouping and
   doorways exist, and `--viewer-tour` measures it (DUNGEON_PRESENTATION_0_8.md,
   "Visibility").
8. **Forge quality**:
   - free-form (non-grid) placement for more organic caves;
   - room-level culling if frame time ever needs it;
   - a third kit only after the two read as different in play.
9. **Cinderdeep tuning from human feedback** (Normal may be too easy at level
   5); a camera rule for very large bosses.
10. **Decor and props blocking sight**: decide whether large scenery should
   become solid volumes in `Nav`.
11. **Release**: run the manual GitHub Actions workflow once pushing is
   authorised; test the ZIP on another machine and on Windows 10.
12. **UI follow-ups from 0.3**:
   - per-aura icons with tooltips;
   - several debuffs on compact raid frames;
   - key rebinding;
   - a settings panel.
13. **Editor, part 3**: add or remove abilities and phases, place mobs
    in-game. Consider a forge-backed "Studio" later.
14. **Raid tuning, economy and hub**:
    - Cinder Court damage between ×1.0 and ×1.4;
    - vendor rotation;
    - stash scrolling.

## Done

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
