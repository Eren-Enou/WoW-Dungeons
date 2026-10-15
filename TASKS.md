# Tasks

Prioritized, actionable work. Completed milestones are summarized at the bottom;
details and evidence live in PROGRESS.md.

## Next up (after 0.7)

1. **Recorded human playtest of 0.7** (PLAYTEST_FORGE.md, 0.7 section):
   - do the Emberforge Vaults and the Hollow Reliquary feel different?
   - do pillars, the Guardian's wail and the patrols read on screen?
   - is the Hollow Guardian fair?

   Write the notes into the repository this time. Earlier versions were
   played, but no notes were kept.
2. **AI follow-ups measured in 0.7** (DUNGEON_FORGE_0_7.md, "AI"):
   - the remaining Hollow Guardian deaths (wails and bone shards; 4 wipes in
     60 runs; layouts 4 and 6 lose more, not yet diagnosed);
   - cover that accounts for the next wail;
   - don't count cover moves as dithers (they inflate the crypt's 71 per
     run);
   - melee converging on one flank spot;
   - first frontal sweeps catching arriving melee;
   - threat failures from healer-caster packs at the pull.
3. **Designer overrides beyond the proof of concept**:
   - re-run the structure checks after geometry edits;
   - key overrides to room roles rather than seeded names;
   - `test`/`report` with `--overrides`.
4. **Milestone 0.8: presentation** (plan in ASSET_PIPELINE_RESEARCH.md):
   - per-component GLB prefabs with procedural fallback;
   - theme definitions, decoration anchors and original Blender art with
     provenance rows;
   - a package size budget.
5. **Forge quality**:
   - free-form (non-grid) placement for more organic caves;
   - room-level culling if frame time ever needs it;
   - a third kit only after the two read as different in play.
6. **Cinderdeep tuning from human feedback** (Normal may be too easy at level
   5); a camera rule for very large bosses.
7. **Decor and props blocking sight**: decide whether large scenery should
   become solid volumes in `Nav`.
8. **Release**: run the manual GitHub Actions workflow once pushing is
   authorised; test the ZIP on another machine and on Windows 10.
9. **UI follow-ups from 0.3**:
   - per-aura icons with tooltips;
   - several debuffs on compact raid frames;
   - key rebinding;
   - a settings panel.
10. **Editor, part 3**: add or remove abilities and phases, place mobs
    in-game. Consider a forge-backed "Studio" later.
11. **Raid tuning, economy and hub**:
    - Cinder Court damage between ×1.0 and ×1.4;
    - vendor rotation;
    - stash scrolling.

## Done

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
