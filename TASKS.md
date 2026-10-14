# Tasks

Prioritized, actionable work. Completed milestones are summarized at the bottom;
details and evidence live in PROGRESS.md.

## Next up (0.3)

1. **Human playtest pass** (needs a person): play both dungeons with each class,
   note control/camera friction, readability of telegraphs, difficulty for a human
   player slot. Adjust numbers in the TOML files accordingly.
2. **Encounter editor, part 2**: the Sandbox tab covers global knobs; a per-ability
   editor that writes a variant TOML file would cover the rest. Today: TOML + F5.
3. **Raid content**: a purpose-built 10-player encounter (today raids reuse the
   dungeons on the "Raid (10)" difficulty); 25-player graphical groups.
4. **AI**: path planning around frontal cones (members still clip cones ~3–4
   times per Warrens run while moving); smarter CC target choice (don't Hex
   short-lived adds).
5. **Vendor / economy**: spend Vault Marks on items; salvage unwanted items.
6. **Performance**: cache `engaged_enemies()` per tick and avoid per-call Vec
   allocations in the AI (~0.012 ms/tick at 25 characters — fine, but the
   obvious next win).
7. **Hub polish**: stash scrolling instead of pages.

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
- 0.2 extras: learning companions (familiarity/alertness), summoned-add threat
  inheritance, in-game Sandbox tab, third dungeon (Ashen Spire).
