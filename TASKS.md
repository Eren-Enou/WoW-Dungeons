# Tasks

Prioritized, actionable work. Completed milestones are summarized at the bottom;
details and evidence live in PROGRESS.md.

## Next up (0.4)

1. **Human playtest pass** (needs a person): run PLAYTEST_0_3.md — controls and
   auto-follow feel, clickable/mouseover healing, threat colours, action-bar
   states, tooltips, the 10-person raid. Tune control constants
   (`controls.rs`), tooltip delay and frame sizes from the notes.
2. **UI follow-ups from 0.3**: per-aura icons/chips with their own tooltips;
   more than one debuff on compact raid frames; key rebinding; a settings panel
   for follow speed and mouse sensitivity (stored in `profile.settings`).
3. **Threat alerts**: decide with playtesters whether 2.5 s between alerts is
   right and whether non-tanks also want "your healer is being attacked".
4. **Editor, part 3**: add/remove abilities and phases, edit aura values and mob
   placement in-game (today: tuning existing numbers in the Editor tab; structure
   via TOML + F5).
5. **Raid tuning**: The Cinder Court (10-player raid, tank swaps) is 100% wins / 0 deaths
   at damage x1.0 but 5% at x1.4 (20 runs each) - find the middle; add a raid test
   for tank swaps; 25-player graphical groups.
6. **AI**: first frontal sweeps right after a pull still catch melee arriving
   before enemies have turned; remaining threat failures come from healer-caster
   packs at the pull.
7. **Economy**: vendor stock rotation, item upgrades with marks.
8. **Performance**: cache `engaged_enemies()` per tick and avoid per-call Vec
   allocations in the AI (~0.012 ms/tick at 25 characters — fine, but the
   obvious next win).
9. **Hub polish**: stash scrolling instead of pages.

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
- 0.3: MMO controls and camera auto-follow; clickable party/raid and ToT
  frames; mouseover healing; UI click-through blocking; action bar with
  range/GCD/cooldown/resource states; threat-coloured plates, target-frame
  threat and aggro alerts; resource readouts; tooltip framework; readability
  pass. See PLAYTEST_0_3.md.
- 0.2 extras: learning companions (familiarity/alertness), summoned-add threat
  inheritance, in-game Sandbox tab, third dungeon (Ashen Spire), vendor and
  salvage, in-game encounter editor, cone-avoiding movement.
