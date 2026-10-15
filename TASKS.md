# Tasks

Prioritized, actionable work. Completed milestones are summarized at the bottom;
details and evidence live in PROGRESS.md.

## Next up (after 0.5)

1. **Human playtests** (need a person): PLAYTEST_0_5.md first (Cinderdeep
   from the packaged ZIP: time to clear, difficulty, route readability,
   companion movement, camera), then PLAYTEST_NAVIGATION.md and
   PLAYTEST_0_3.md.
2. **Cinderdeep tuning from that feedback**: Normal may be too easy at level 5
   (the AI party almost never dies); decide with players before changing
   numbers. Consider a camera rule for very large bosses (Ignarax fills the
   view up close).
3. **AI movement follow-ups**: melee converging on the same attack spot still
   overlap (give each attacker its own spot without hurting outcomes: a
   per-melee slot was tried and measured worse); remaining dither loops around
   the Cistern pack (melee and healer); dithers are still ~35–135 per run
   depending on profile. Still open from 0.4: first frontal sweeps right after
   a pull catch melee arriving before enemies have turned; threat failures
   from healer-caster packs at the pull.
4. **Decor collision/sight**: pillars and stalagmites are visual only; decide
   whether large decor should block line of sight (would need solid volumes in
   `Nav`).
5. **Release**: run the manual GitHub Actions workflow once pushing is
   authorised; test the ZIP on another machine and on Windows 10.
6. **UI follow-ups from 0.3**: per-aura icons/chips with their own tooltips;
   more than one debuff on compact raid frames; key rebinding; a settings panel
   for follow speed and mouse sensitivity (stored in `profile.settings`).
7. **Threat alerts**: decide with playtesters whether 2.5 s between alerts is
   right and whether non-tanks also want "your healer is being attacked".
8. **Editor, part 3** (heights, polygons and drops included): add/remove
   abilities and phases, edit aura values and mob placement in-game.
9. **Raid tuning**: The Cinder Court is 100% wins at damage x1.0 but 5% at
   x1.4 - find the middle; a raid test for tank swaps.
10. **Performance**: Cinderdeep is 0.041 ms/tick (5 players); legacy dungeons
   rose ~25% in 0.5. Next wins: cache spot-search path lengths per tick, fewer
   per-tick line-of-sight questions from healers, cache `engaged_enemies()`.
11. **Economy and hub**: vendor stock rotation, item upgrades with marks,
   stash scrolling.

## Done

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
