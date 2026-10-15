# Combat, AI and progression rules (0.2)

Authoritative implementation:
`crates/sim/src/stats.rs` (formulas), `sim.rs` (rules), `ai.rs` (behaviour),
`class.rs` (classes/abilities), `items.rs` + `profile.rs` (gear, XP, rewards),
`content.rs` (data schema). Every rule below has unit tests in
`crates/sim/src/tests.rs` or the module's own tests.

Provenance tags: **[WoW]** follows publicly documented World of Warcraft Classic
behaviour; **[WoW-approx]** is a simplification of it; **[orig]** is an original
design decision for this project.

## Simulation

* Fixed timestep `DT = 0.05 s` (20 Hz). The game accumulates real time, runs
  whole ticks and interpolates positions for rendering.
* All randomness (damage variance, hit-table rolls, crits, target picks, loot)
  comes from one SplitMix64 RNG seeded per run. Same seed + content + inputs ⇒
  the same event log (checked by hashing the formatted log).
* Tick order: combat flags → blackboard + AI decisions → player cast → movement →
  heights (ramps, falls) → proximity aggro → auras (HoT/DoT ticks, expiry) → casts & cooldowns →
  auto-attacks → enemy phases / enrage / abilities → telegraphs → hazards →
  regeneration → resets → progress → rally.

## Statistics

Each actor has `level, armor, resist, dodge, parry, block, block_value, crit, hit`
plus HP, a resource, a power multiplier and a threat multiplier.

* **Levels** (party 1–10) [orig]: damage/healing ×(1 + 0.10·(L−1)); HP and mana
  ×(1 + 0.12·(L−1)).
* **Gear** [orig]: stamina +10 HP; power +1% damage/healing; armor, resist; crit,
  hit, dodge in percentage points.

## Mitigation

* **Armor** [WoW]: `DR = armor / (armor + 400 + 85·attacker_level)`, max 75%.
  Applies to physical damage except bleeds.
* **Resistance** [orig]: `resist / (resist + 100 + 15·attacker_level)`, max 75%,
  applied as a smooth average to magic damage. (WoW's `0.75·resist/(5·L)` was
  tried first; at levels 1–6 a handful of resist meant 75% reduction.)
* Then: damage-reduction buffs (strongest one), stacking "damage taken" debuffs
  (+pct per stack, additive), block value (subtracted), absorb shields (soak
  first). Amounts are rounded.

## Hit resolution

* **Melee single-roll table** [WoW]: one roll in [0,1) walks
  miss → dodge → parry → block → crit → hit.
  * Miss 5% + 1% per level the defender is above the attacker, − hit [WoW-approx].
  * Dodge = defender dodge + 0.2% per level above [orig].
  * Parry and block only when the attacker is in the defender's front 180° [WoW].
  * Crit chance − 1% per level the defender is above [WoW-approx].
  * Incapacitated defenders cannot dodge, parry or block.
* **Spells** [WoW]: miss chance 4/5/6% at 0/+1/+2 levels, 17% at +3 (+11% per
  further level), − hit, minimum 1%. Spells are never dodged/parried/blocked. An
  independent crit roll follows.
* **Periodic** ticks and **unavoidable** effects (rings, pulses, cleaves, hazards)
  always hit and never crit.
* Crit multipliers [WoW]: melee ×2.0, spells and heals ×1.5.
* Direct damage varies ±10%; periodic damage does not.

## Resources

* **Mana** (Mender, Arcanist) [WoW five-second rule]: full regeneration
  (class rate, % of max per second) only 5 s after the last mana spent; 35% of
  that rate while casting continuously; 10%/s out of combat.
* **Rage** (Warden) [WoW]: starts at 0, max 100. White melee hits give
  `7.5·damage/c`, damage taken gives `2.5·damage/c`, with
  `c = 0.0091·L² + 3.23·L + 4.27`. Decays 2/s out of combat.
* **Energy** (Striker) [WoW-approx]: 10 per second, max 100.
* HP regenerates 8%/s out of combat. "In combat" = any combat action in the
  last 5 s, or being on an engaged enemy's threat table.

## Abilities (5 per class, keys 1–5)

Validation order: dead → incapacitated → casting → cooldown → 1 s global cooldown
→ resource → silenced (magic school) → target → effect-specific (immune, nothing
to dispel, Weakened Soul) → range → line of sight (0.4). Range is edge-to-edge
horizontally, combined with any height difference beyond 1 m (see Navigation).
Line of sight is needed for every ability aimed at someone other than yourself
and for auto attacks. Cast-time spells start
the GCD immediately and resolve at the end of the cast (target must still be
valid, range + 3 m leeway); resource and cooldown are paid on completion. The
player's casts are cancelled by moving.

| Class | Resource | 1 | 2 | 3 | 4 | 5 |
|---|---|---|---|---|---|---|
| Warden (tank) | rage | Shield Slam: 75 dmg, 4× threat, 6 s cd, 15 rage | Challenge: taunt 3 s, 8 s cd | Bulwark: −40% damage 8 s, 30 s cd | Shield Bash: interrupt + 4 s silence, 12 s cd, 10 rage | Quake Stomp: 35 to all engaged within 8 m, 3× threat, 6 s cd, 20 rage |
| Mender (healer) | mana | Mend: 1.5 s, heal 420 | Renew: 300 over 9 s | Smite: 1.5 s, 75 holy | Ward: absorb 320 for 15 s, Weakened Soul 15 s, 4 s cd | Purify: remove one dispellable harmful effect |
| Striker (melee) | energy | Rend Strike: 95, 40 energy | Whirl: 60 to all within 6 m, 10 s cd | Evade: −50% threat, 20 s cd | Kick: interrupt + 4 s silence, 10 s cd | Lacerate: bleed 240 over 12 s (ignores armor) |
| Arcanist (ranged) | mana | Arc Bolt: 2 s, 125 arcane | Flare: instant 80 fire, 6 s cd | Veil: −50% threat, 20 s cd | Hex: 1.5 s, incapacitate a non-boss 20 s (breaks on damage, one at a time) | Silence Pulse: ranged interrupt, 20 s cd |

## Auras

HoT, DoT, damage reduction, absorb, Weakened Soul, taunted, incapacitated,
silenced, damage-taken (stacking), healing-taken (stacking). Re-applying an aura
with the same name and source refreshes its duration and adds a stack up to
`max_stacks`. HoTs/DoTs tick every second for `per_tick × stacks`. Any damage
breaks incapacitation. Purify removes the first harmful, dispellable aura.

## Threat

* Damage threat = damage × ability threat multiplier × class threat multiplier
  (Warden 1.5). Avoided attacks generate none.
* **Healing threat** [WoW]: 0.5 × effective healing × healer multiplier, split
  evenly across all engaged enemies, only if the healed target is in combat.
* **Taunt** [WoW]: raises the taunter's threat to the current top value and
  forces the target for 3 s.
* **Threat drop**: Evade/Veil multiply the caster's threat by 0.5 everywhere.
* **Target selection** [WoW]: forced target while taunted; otherwise keep the
  current target until someone exceeds it by 10% (within the enemy's melee reach)
  or 30% (outside it).

## Interrupts and crowd control

* Interrupting an **interruptible** enemy cast cancels it and silences the
  enemy for 4 s (no special abilities). Uninterruptible casts are unaffected.
* Hex incapacitates (no actions, no movement) for 20 s; bosses (and templates
  with `cc_immune = true`) are immune; one Hex per caster.

## Encounters

* Each encounter is a pack. Proximity (`aggro_radius`, 3D distance) or any
  hostile action engages the whole pack. Across floors (more than 1 m of height
  difference) proximity only counts if the enemy can see you, so walking over a
  pack on a bridge does not pull it [0.4]; the trigger goes on every threat table first and
  other living party members within 30 m join at zero threat [orig].
* **Reset / leash**: an engaged enemy with nobody left it can reach on foot or
  see, or more than `leash_radius` (3D) from home, resets its pack: living mobs heal to full, clear
  threat/auras/casts, return to phase 1 with fresh ability timers, become immune
  and walk home at 1.5× speed. Summoned adds despawn. Dead mobs stay dead. An
  enemy whose home became unreachable (it followed someone down a one-way drop)
  settles where it stands instead of teleporting [0.4].
* Enemies ignore threat-table entries they can neither reach nor see when picking
  a target [0.4].
* **Ground effects** (rings, hazards, cleaves, AoE abilities) hit only within
  1.5 m of their own elevation: a ring on a bridge misses the floor below [0.4].
* **Enemy abilities** (data-driven, `kind`):
  `bolt` (spell at a target), `cleave` (unavoidable physical around the caster,
  optionally `frontal` = front 180° only), `ring` (ground telegraph(s) that
  detonate after `delay`), `pulse` (party-wide), `summon` (adds that join the
  fight), `debuff` (aura: dot / damage_taken / healing_taken, stacking,
  dispellable or not), `heal_ally` (heals the most injured pack member below
  80%). Each has `interval`, `initial` delay, `cast_time`, `interruptible`,
  `phases`, and a target selector (`current`, `random`, `random_non_tank`,
  `farthest`, `lowest_health`, `self`). Ability timers keep running while the
  enemy is casting or controlled; abilities only fire when it is free.
* **Phases**: `[[templates.X.phases]]` with descending `hp_below` thresholds,
  each with `damage_multiplier`, `announce` text and optional `summon`. Each
  threshold fires once per pull. Abilities gated to a phase get fresh timers
  when it starts.
* **Enrage**: damage ×5 after `enrage_seconds` in combat.
* **Hazards**: per-encounter circles that are active for `on` seconds then
  dormant for `off` (offset by `offset`), dealing unavoidable damage each
  second to party members inside while their encounter is uncleared.
* **Difficulty**: per-dungeon `[[difficulties]]` scale enemy HP and damage, add
  enemy levels (which shifts every hit table), multiply rewards and upgrade loot
  rarity.
* **Victory** when every non-optional encounter is cleared; **Wipe** when no
  party member is alive; **Timeout** (headless limit); **Abandoned** (left).
* **Rally** [orig, instead of resurrection]: 8 s after the whole party leaves
  combat with no engaged enemy, fallen members rise beside the leader at 40% HP
  and resource. A wipe is final.

## Progression

* **XP** [orig]: kills give 20/60/250 × enemy level (trash/elite/boss), a quarter
  if the enemy is 4+ levels below; victory adds the dungeon's `xp_per_clear`;
  all × the difficulty reward multiplier. Player and recruited companions gain
  XP. `xp_to_next(L) = 250·L + 50·L²`; max level 10.
* **Loot** [orig]: every cleared encounter that has `[[loot]]` entries with a
  matching `source` (default: the final boss) rolls one weighted item. Difficulty
  rarity bonus upgrades rarity; stats scale by the rarity multiplier
  (Common 1.0, Uncommon 1.25, Rare 1.5, Epic 1.9).
* **Equipment**: 7 slots (main hand, off hand, head, chest, legs, feet, trinket).
  Items have a role affinity; the hub compares items with a role-weighted score
  and "Auto-equip" fills each party member's slots with suitable upgrades.
* **Marks**: per clear / per failed run × reward multiplier.

## Companion AI

Every tick a shared **blackboard** is computed: engaged enemies, tanks, healers,
**kill target** (healer-casters +40, interruptible casters +25, tank's target
+20, summoned adds +12, bosses −10, low HP bonus), **crowd-control assignment**
(an AI Arcanist hexes the healthiest unfocused non-immune enemy when 3+ are
engaged), **interrupt assignments** (one interrupter per interruptible cast:
the closest member whose interrupt is ready and can reach in time), threatened
members, healer mana, pulse-incoming flag, danger zones (telegraphs; hazards
that are active or turn on within 1 s), frontal cleavers and time since the pull.
Each companion then decides independently:

* **Danger first**: in a noticed telegraph/hazard (after the profile's reaction
  time, plus a safety margin) → move to the nearest point outside all dangers.
  Non-tanks inside a frontal cleave cone step to the enemy's back.
* **Assigned interrupt** when the cast's remaining time ≤ the profile's window.
* **Tank**: taunt loose enemies (on the healer first), Quake Stomp loose adds,
  hold the enemy with the smallest threat lead, Bulwark at low HP, during
  party-wide pulses or at 4+ damage-taken stacks; drag frontal cleavers so they
  face away from the party.
* **Healer**: urgency = HP fraction (tank −10%, self −5%, threatened −5%); Ward
  the lowest member before a pulse or the tank below 75%; Mend below the
  profile's threshold; Purify dispellable debuffs (first under the Support
  profile); Renew; Smite only with spare mana. No non-urgent heals for 4 s after
  a pull (lets the tank build threat). Cancels Mends that would be ≥97% overheal.
* **DPS**: attack the kill target; hold until the tank has threat and while near
  the tank's threat (profile cap); shed threat when targeted; melee stand behind
  (by the enemy's facing), ranged ~18 m away (flank/back of frontal cleavers);
  Whirl only when safe and no crowd-controlled enemy is nearby; keep Lacerate up
  on sturdy targets.
* **Leader autopilot** (headless/demo): wait until everyone is alive, above the
  profile's HP/mana readiness and gathered, then walk to the next uncleared
  encounter (optional ones too unless skipped) and pull with Challenge.

### Profiles

| | reaction | threat cap | min tank threat | defensive HP | Mend < | interrupt window | danger margin | CC |
|---|---|---|---|---|---|---|---|---|
| Conservative | 0.5 s | 70% | 200 | 65% | 70% | 1.0 s | 1.2 m | yes |
| Balanced | 0.45 s | 90% | 40 | 50% | 60% | 1.0 s | 0.6 m | yes |
| Aggressive | 0.75 s | 105% | 0 | 30% | 50% | 0.5 s | 0.2 m | no |
| Support | 0.45 s | 85% | 40 | 50% | 70% | 1.6 s | 0.6 m | yes, dispels first |
| Mechanics | 0.2 s | 90% | 40 | 50% | 60% | 1.8 s | 1.8 m | yes |

## Navigation (0.4, layered 2.5D) [orig]

Full design: NAVIGATION_0_4.md.

* **Surfaces**: `[[dungeon.areas]]` are axis-aligned footprints with a height:
  flat at `y` (default 0), or a ramp from `y` at the `min` edge to `y_to` at the
  `max` edge along `axis` (`"z"` default, or `"x"`); `stairs = true` only changes
  the drawing. `under = "open"` makes a thin deck (0.4 m) you can walk and see
  under; otherwise the area is solid rock down to the lowest floor. `walls`
  (default true) and `wall_height` (default 3.2 m) shape the outline.
  `unreachable = true` marks an intentionally unreachable area.
* **Connections**: two areas connect only where their footprints (shrunk by the
  0.4 m wall margin) overlap **and** their heights agree within `max_step`
  (0.5 m). For a ramp only the level strip of the overlap is the doorway.
  Overlap alone never connects. One-way drops are authored in
  `[[dungeon.links]]` (`from`, `to`, `kind = "drop"`, a `min`/`max` zone over the
  edge); the drop must be more than `max_step` and at most `max_drop` (8 m).
  `kind = "walk"` links may be declared to assert a connection exists.
* **Validation**: every pair of overlapping areas must be level with each other,
  stacked at least `headroom` (2.5 m) apart with the upper one `open`, or a ramp
  overlapping the floor it joins by at most 2 m. Ramps may not be steeper than
  `max_slope` (0.6 rise/run). Every area must be reachable from the entrance
  unless marked unreachable. Entrance, mob and hazard positions take an optional
  `y`; without it all floors under the point must be level, otherwise the
  position is ambiguous and rejected. Limits live in `[dungeon.nav]`.
  Old maps (no heights) load unchanged: everything at 0, overlaps connect.
* **Routing**: shortest paths over directed portals (Floyd–Warshall at load,
  3D distances, +2 cost per drop). Actors steer to the point of the next
  portal nearest to the following waypoint. Waypoints closer than 0.1 m are
  skipped.
* **Movement**: a step stays on the current surface if it lands inside it; it
  leaves only through a portal it crosses, or off the edge inside a drop zone.
  Elevation follows the surface (ramps interpolate); after a drop the actor falls
  at 12 m/s. No actor-actor collision.
* **Robustness**: an AI actor that tries to move but makes < 20% headway, or
  stands at a waypoint while its reachable goal is > 1 m away, for 1 s is stuck:
  it steers to the current portal (or surface) centre for 0.8 s. Two reversals
  of direction within ~0.7 s commit it to its previous goal for 0.75 s, unless
  it or that goal is in a ground effect. A companion whose goal is unreachable
  stays with the leader.
* **Range and sight**: `reach(a, b, r)` ⇔ `h² + v² ≤ r²` with `h` = horizontal
  edge-to-edge distance and `v` = height difference minus 1 m. Line of sight is a
  segment from the eyes (85% of body height) to the target's chest or eyes,
  blocked by wall faces (from their floor up to `wall_height`) and by any
  surface's solid volume (deck or rock).
* **AI and sight**: a cast without line of sight becomes "walk toward the
  target"; ranged spots are searched (angles around the preferred direction, at
  100/70/45% distance) for one that sees the enemy and is seen by the first
  healer (healers: sees the tank); ranged DPS out of the healer's sight move
  before casting; healers who lose the tank stop damage casts and move.
