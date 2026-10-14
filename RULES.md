# Combat and AI rules (prototype 0.1)

Authoritative implementation: `crates/sim/src/sim.rs` (rules) and
`crates/sim/src/ai.rs` (behaviour). Numbers for classes are in `class.rs`;
everything enemy-side is in `assets/content/dungeon.toml`.

## Simulation

* Fixed timestep `DT = 0.05 s` (20 Hz). The game accumulates real time and runs
  whole ticks, interpolating positions for rendering.
* All randomness (damage variance, crits, boss target picks, loot) comes from a
  SplitMix64 RNG seeded per run. Same seed + same content + same inputs ⇒ the
  same event log (checked by hashing the formatted log).
* Tick order: combat flags → AI decisions → player cast → movement → proximity
  aggro → auras (HoTs, buffs, taunts) → casts & cooldowns → auto-attacks →
  enemy specials (cleave, boss script) → telegraphs → regen → resets → progress.

## Resources and stats

* HP is clamped to `[0, max]`. Reaching 0 kills: casts cancel, auras clear, the
  actor is removed from every threat table.
* Mana regenerates per second: in combat `combat_mana_regen × max` (class-specific,
  1.5–4%), out of combat 10%. Out of combat HP regenerates 8%/s.
* "In combat" = dealt/took damage, healed someone in combat, or sat on an engaged
  enemy's threat table within the last 5 s.
* Equipment: +1% damage/healing per *power* point, +10 max HP per *stamina* point.

## Abilities

* Validation order: dead → already casting → cooldown → global cooldown (1 s) →
  mana → target → range. Failures produce a `CastFailed` event with the reason.
* Target rules: *Enemy* spells need a living, non-evading hostile target;
  *Ally* spells use the selected friendly target or fall back to the caster;
  *Self* spells ignore the target.
* Range is edge-to-edge: `distance ≤ range + caster.radius + target.radius`.
* Cast-time spells start the GCD immediately and resolve when the cast bar
  finishes; mana and cooldown are paid on completion. The target must still be
  valid and within range + 3 m leeway. The player's casts are interrupted by
  moving; casting while moving is refused.
* Damage = base × attacker power × U(0.9, 1.1) × (1.5 on a 5% crit) × target
  `damage_taken` × (1 − strongest damage-reduction buff), rounded.
* Heals = base × healer power; effective healing is capped at missing HP and
  the rest is recorded as overheal.

## Threat

* Damage threat = damage × ability threat multiplier × class threat multiplier
  (Warden 1.5, others 1.0). Shield Slam has a 4× ability multiplier.
* **Healing threat** = 0.5 × *effective* healing × healer threat multiplier,
  split **evenly** across all currently engaged enemies, and only when the
  healed target is in combat. Overhealing generates no threat.
* **Taunt** (Challenge): the taunter's threat is raised to equal the current top
  threat and the enemy is forced to attack the taunter for 3 s.
* **Threat drop** (Evade/Veil): the caster's threat on every enemy × 0.5.
* **Target selection**: forced target while taunted; otherwise the enemy keeps
  its current target until someone exceeds that target's threat by **10%** (if
  they are within the enemy's melee reach) or **30%** (outside it).

## Aggro, packs, resets

* Each encounter is a pack. A living party member inside a mob's
  `aggro_radius`, or any damage/taunt on a mob, engages the **whole pack**. The
  trigger goes on every mob's threat table first; other living party members
  within 30 m join at zero threat (design shortcut so packs don't evade when the
  puller dies).
* An engaged enemy whose threat table has no living entries, or that is dragged
  further than `leash_radius` from its spawn, **resets its whole pack**: living
  mobs heal to full, clear threat/auras/casts, become immune (evading) and walk
  home at 1.5× speed, then return to idle. Dead mobs stay dead. The boss also
  resets to phase 1, its timers restart and its telegraphs vanish.
* Run outcome: **Victory** when the boss encounter is cleared; **Wipe** when no
  party member is alive; **Timeout** (headless limit) or **Abandoned** (player
  left).

## Boss: Morvane, the Hollow King

* Phase 1: melee; every `bolt_interval` s casts *Hollow Bolt*
  (`bolt_cast_time` s) on a random non-tank on its threat table.
* Phase 2 triggers **once per pull** at `hp ≤ phase2_hp_fraction × max`:
  damage × `phase2_damage_multiplier`, a `PhaseChange` event, visible tell
  (orange glow, 20% larger, banner). 2 s later and then every `ring_interval` s
  a *Cinder Ring* telegraph appears under a random living party member and
  detonates after `ring_telegraph` s for `ring_damage` to everyone inside.
* Enrage: damage × 5 after `enrage_seconds` of combat.
* Elite cleave: every `cleave.interval` s, damage to every party member within
  `cleave.radius` of the elite.

## Companion AI (interpretable rules, no remote calls)

All AI is a pure function of sim state, evaluated every tick.

* **Danger first**: any party AI standing in a telegraph that has existed for at
  least 0.5 s (reaction time) cancels its cast and moves to the nearest safe
  point outside every telegraph (12 candidate directions).
* **Out of combat**: followers hold formation slots behind the leader (the
  player, or the tank in headless runs). Healers top off anyone under 70%.
* **Leader autopilot** (headless / `--demo`): wait until everyone is ≥90% HP,
  mana users ≥80% mana and gathered within 12 m; walk to the next uncleared
  pack; the tank pulls with Challenge from ≤22 m.
* **Tank**: enemies attacking someone else are "loose"; taunt the most dangerous
  (attacking the healer first, then by rank). Otherwise hold the enemy where its
  threat lead is smallest (with hysteresis). Bulwark below 50% HP; Shield Slam
  on cooldown; stay in melee range.
* **Healer**: urgency = HP fraction, with the tank weighted −10% and self −5%.
  Mend the most urgent ally under 60% (35% when below 20% mana); Renew allies
  under 85% (and pre-HoT the tank when mana > 50%); Smite the tank's target
  only above 75% mana. Cancels a Mend that would be ≥97% overheal. Positions
  ~15 m behind the tank relative to its target, staying within reach of all.
* **DPS**: utility score per engaged enemy — +10 tank's target, +6×(missing HP
  fraction), +2 trash, +3 current target, −0.02/m distance. Holds fire until
  the tank has threat on the target and while its own threat exceeds 90% of the
  tank's; uses its threat drop when an enemy turns on it or it nears the tank's
  threat. Melee stand behind the target; ranged keep ~18 m. Whirl only when ≥2
  enemies are in range and the tank's threat on each exceeds 2× ours + 150.
* **Enemies**: idle until engaged; chase and auto-attack their selected target;
  walk home when resetting.

## Navigation shortcut

Walkable floor = overlapping axis-aligned rectangles from the content file. Each
rectangle is a graph node; overlapping rectangles are edges. Actors BFS the
rectangle graph and steer toward the centre of the overlap with the next
rectangle (straight lines inside convex rectangles are always walkable), and
slide along walls. Actors do not collide with each other; walls do not block
line of sight.
