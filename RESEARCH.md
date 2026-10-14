# Research log

## What was consulted

| Source | License | How it was used |
|--------|---------|-----------------|
| Bevy 0.20.0 crate sources and bundled examples (`~/.cargo/registry/src/.../bevy-0.20.0/examples`, `_release-content/migration-guides`) | MIT OR Apache-2.0 | API reference for the pinned version: spawning meshes/materials, lights, UI `Node`/`Text`, `States`/`DespawnOnExit`, `AccumulatedMouseMotion`, `Camera::viewport_to_world`, gizmos, `Screenshot` + `save_to_disk`, the `set_if_neq → set_if_different` rename, `FontSize::Px`. No example code was copied beyond ordinary API usage. |

**Not consulted in this session:** AzerothCore, mod-playerbots and TrinityCore
source code (REFERENCES.md items 5–8) were *not* opened. No GPL or proprietary
code, data or assets were read, copied or translated into this repository.

## Observations vs. design inventions

The mechanics below are modelled on widely known, publicly documented behaviour
of classic tab-target MMO dungeons (general genre knowledge, not any codebase):

* **Observed genre conventions (re-implemented from scratch):** per-enemy threat
  tables; enemies switching targets only when another player exceeds the current
  target's threat by 10% in melee range / 30% outside it; taunts that match the
  top threat and force the target briefly; healing generating half its amount as
  threat split among engaged enemies; packs aggroing together; enemies evading
  (immune, walking home, healing to full) when they lose all targets or are
  dragged too far; tank/healer/DPS role split; boss phase transitions at HP
  thresholds; ground-telegraphed area attacks; hard enrage timers; global
  cooldown.
* **Design inventions for this prototype:** all class, ability, enemy, boss,
  item and place names; every number; the 30 m "join the pull at zero threat"
  rule; the rectangle-graph navigation; the companion AI utility scores, the
  0.5 s telegraph reaction time, the leader autopilot; the healer's
  overheal-cancel rule; the rewards (Vault Marks + one weighted loot roll);
  equipment power/stamina scaling.

## 0.2 combat model: WoW-derived vs original

All formulas were written from general, publicly documented knowledge of WoW
Classic mechanics (community wikis and theorycrafting folklore), not from any
source code. RULES.md tags each rule **[WoW]**, **[WoW-approx]** or **[orig]**.

* **WoW-derived:** armor DR `armor/(armor+400+85·L)` capped at 75%; the
  single-roll melee attack table (miss → dodge → parry → block → crit → hit);
  parry/block only from the front; 5% base melee miss growing with the level gap;
  spell miss 4/5/6/17% by level gap; crit multipliers (melee ×2, spells ×1.5);
  the five-second mana rule; rage from damage dealt/taken with the level-based
  conversion constant; energy at 10/s; taunt matching top threat; healing threat
  at half of effective healing split across enemies; interrupt lockouts;
  incapacitate broken by damage; Weakened-Soul-style shield lockout.
* **Original / simplified:** the resistance curve (WoW's level-relative formula
  is too steep at levels 1–6); +0.2% dodge per level gap; level scaling of
  stats; item stats, rarity multipliers and role affinity; XP curve; loot per
  encounter; the rally rule; the blackboard, profiles and every AI heuristic;
  all numbers, names, enemies and dungeons.

## AI evaluation notes (0.2)

Headless breakdowns (`dungeon-headless --breakdown`) drove two AI fixes:
frontal-cleave positioning (non-tank cleave hits in the Warrens fell from 12.5
to 3.9 per run) and healer pull discipline (bosses briefly turning on the healer
after parried opening attacks). See PROGRESS.md for numbers.

If AzerothCore/Playerbots are studied later, record here which files were read,
their exact license (these projects are copyleft, GPL/AGPL family — verify the
specific version in each repository first), and keep
the separation: behavioural observations only, no code transcription.
