# Tasks

Prioritized, actionable work. Completed milestones are summarized at the bottom;
details and evidence live in PROGRESS.md.

## Next up (0.5)

1. **Human playtests** (need a person): PLAYTEST_NAVIGATION.md (Cavern
   Traverse, AI movement, camera under bridges, line of sight in the old
   dungeons) and PLAYTEST_0_3.md (controls, healing, threat, tooltips).
2. **Ragefire-inspired cave** on the 0.4 navigation: original layout with
   winding lava caverns, elevation changes and branching paths. Likely needs
   convex-polygon footprints (NAVIGATION_0_4.md section 8) for curved passages.
3. **Navigation follow-ups**: reduce AI dithering in the Cinder Court
   (~175 per run) by resolving the conflicting goals rather than committing;
   crowd separation at doorways; formation spots that stay on the leader's
   floor for 25-40 groups.
4. **UI follow-ups from 0.3**: per-aura icons/chips with their own tooltips;
   more than one debuff on compact raid frames; key rebinding; a settings panel
   for follow speed and mouse sensitivity (stored in `profile.settings`).
5. **Threat alerts**: decide with playtesters whether 2.5 s between alerts is
   right and whether non-tanks also want "your healer is being attacked".
6. **Editor, part 3** (now including heights and drops): add/remove abilities and phases, edit aura values and mob
   placement in-game (today: tuning existing numbers in the Editor tab; structure
   via TOML + F5).
7. **Raid tuning**: The Cinder Court (10-player raid, tank swaps) is 100% wins / 0 deaths
   at damage x1.0 but 5% at x1.4 (20 runs each) - find the middle; add a raid test
   for tank swaps; 25-player graphical groups.
8. **AI**: first frontal sweeps right after a pull still catch melee arriving
   before enemies have turned; remaining threat failures come from healer-caster
   packs at the pull.
9. **Economy**: vendor stock rotation, item upgrades with marks.
10. **Performance**: 0.4 is 0.022 ms/tick at 25 characters (0.3: 0.013).
   Next wins: fewer per-tick line-of-sight questions from healers (they check
   every member), cache `engaged_enemies()`.
11. **Hub polish**: stash scrolling instead of pages.

## Done

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
