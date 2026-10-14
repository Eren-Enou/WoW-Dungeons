# Changelog

## 0.2 (in development)

### Phase A — playable experience
- Camera: collision against wall/pillar boxes (pull in fast, ease out), zoom 4–32 m.
- Walls that hide the player fade to translucent; walls are 3.2 m tall again.
- Targeting: Tab cycles engaged enemies first, then idle ones in front of the camera;
  Shift+Tab cycles backwards; T assists (target of target); G targets the party's kill
  target; F1–F5 target party members; click picking unchanged.
- Movement: Q/E strafe, both mouse buttons run forward.
- Nameplates for every actor (name, health, live cast bar with "(kick!)" for
  interruptible casts, harmful auras with stacks).
- Floating combat text: damage, crits, heals, miss/dodge/parry/resist/absorb,
  interrupts, dispels, CC breaks, taunts, deaths, rallies.
- Procedural effects coloured by school: spell projectiles, melee flashes, AoE/cleave
  rings, taunt rings, heal sparkles, buff shells, threat-drop puffs, phase blasts.
- Pulsing target ring; danger vignette ("MOVE OUT!") while standing in a telegraph;
  hazard discs that flicker before activating; frontal cleave cones drawn on the floor.
- HUD: five ability slots with resource costs and cooldown bars, resource bars
  coloured by type (mana/rage/energy), target frame shows casts and auras.
- Hub rebuilt as a clickable tabbed panel: Party (recruit/dismiss, AI profile,
  auto-fill, auto-equip, change class), Character (stat sheet, 7 equipment slots,
  stash with upgrade comparison, equip/unequip), Dungeons (select dungeon and
  difficulty, encounter list, reload files, content errors), Classes (all abilities),
  History (last runs).
- Results screen: multiple loot drops, XP and level-ups, interrupts, threat failures,
  avoidable damage, CC, dispels, overheal.

### Phase B — combat model
- Stats: level, armor, resist, dodge, parry, block, block value, crit, hit.
- Armor and resist mitigation, single-roll melee attack table, spell hit table,
  front-only parry/block, crit multipliers, damage variance.
- Resources: mana with the five-second rule, rage from damage dealt/taken, energy.
- Generalized auras: HoT, DoT, absorb, damage reduction, stacking damage-taken and
  healing-taken debuffs, incapacitate, silence, taunt, Weakened Soul; stacking and
  refresh rules; dispellable flag.
- Interrupts with lockout, Hex crowd control (bosses immune), Purify dispels.
- Five abilities per class (added Shield Bash, Quake Stomp, Ward, Purify, Kick,
  Lacerate, Hex, Silence Pulse).

### Phase C — AI
- Party blackboard: kill target, interrupt assignments, CC assignment, threatened
  members, danger zones, pulse warnings, frontal cleavers, time since pull.
- Five AI profiles (Conservative, Balanced, Aggressive, Support, Mechanics) with
  measurably different outcomes; per-companion profile in the hub.
- Frontal-cone avoidance, flank positioning for ranged vs cleavers, facing-based
  "behind" spots, healer pull discipline, tank AoE pickup of adds, defensive
  cooldowns on pulses and stacks, dispels, shields before pulses.
- `dungeon-headless`: `--compare-profiles`, `--breakdown` (damage by source,
  enemies turning on non-tanks, killing blows), `--dungeon`, `--difficulty`,
  `--level`, `--profile`, `--party-size`, `--bench`; richer summaries.

### Phase D / F — content
- Content schema 2.0: data-driven enemy abilities (bolt, cleave/frontal, ring,
  pulse, summon, debuff, heal_ally), HP-threshold phases with announcements and
  summons, enrage, environmental hazards, optional encounters, difficulties,
  per-encounter loot, dungeon themes. Content is a directory of dungeon files.
- New dungeon **The Gloomroot Warrens** (levels 4–6): healer-casters, a priority
  caster with an interruptible party-wide volley, an optional elite with spore rings
  and vents, Mother Brakkis (summons, stacking dispellable poison, frontal cleave),
  lava-vent room, and the three-phase Vhorlag (party pulse, adds, double rings,
  stacking tank debuff).
- The Sunken Vault ported to the new schema; Heroic difficulties for both.
- Example variants in `assets/content/variants/` (Inferno, Training).

### Phase E — progression
- Levels 1–10 with XP from kills and clears; level scaling of stats.
- Items with 7 slots, rarity (stat multipliers), role affinity and stat blocks;
  role-weighted comparison and auto-equip; run history; v1 save migration.

### Phase G — raids
- Configurable party size in the headless runner (5/10/25/40) with multiple tanks
  and healers; leader gather radius scales with party size; benchmarks recorded.
- 10-person raid groups in the game: roster of 12 companions (3 per class, older
  saves get the new recruits), Party / Raid (10) toggle with 2 tanks + 2 healers
  auto-fill, "Raid (10)" difficulty on every dungeon, compact two-column frames.

### Later 0.2 additions
- Learning companions: per-dungeon familiarity (clears/5) sharpens reactions,
  safety margins and interrupt timing; in-run alertness after avoidable hits.
  `--familiarity` in the headless tool; familiarity shown in the Party tab.
- Summoned adds inherit 50% of their summoner's threat (fewer healer pulls).
- In-game **Sandbox** tab: enemy HP, damage, ability speed and telegraph-time
  overrides saved in the profile; easier settings scale rewards down.
- Third dungeon **The Ashen Spire** (levels 8–10): healing-reduction brands,
  armored sentinels with frontal sweeps, a timed fire-jet corridor, the Twin Altar
  council (two bosses), an optional Furnace Golem, and three-phase Ashmaw.
- `--hub-tour`, `--click-test`, `--group N` developer flags.

## 0.1 — prototype
- Hub → dungeon → boss → rewards → hub loop, four classes, AI party, headless runner.
