# Changelog

## 0.4 — three-dimensional navigation and dungeon environments

Design, verification, benchmarks and limits: NAVIGATION_0_4.md.

### Navigation
- Layered 2.5D navigation replaces overlapping flat rectangles:
  - areas have a height, can be ramps (`y_to`, `axis`, `stairs`) or open decks you can walk under;
  - they connect only where they overlap **and** are level;
  - one-way drops are authored in `[[dungeon.links]]`;
  - unreachable platforms are allowed;
  - limits live in `[dungeon.nav]`.
- Validation rejects a solid area over another, a vertical gap between step and headroom, too-steep ramps, bad links, unreachable areas and ambiguous positions. Every error names the areas and the fix.
- Old maps load unchanged (everything at height 0).
- Routing: weighted shortest paths over portals, precomputed at load; steering uses a portal funnel.
- Movement leaves a surface only through a portal or off a drop edge. Ramps interpolate height, and drops fall at 12 m/s.
- Stuck detection with recovery (no teleporting), dither detection, and companions that stay with the leader when a goal is unreachable.
- New metrics in the headless runner: `nav stuck`, `unreachable` and `dithers`.

### Spatial combat
- Actors carry an elevation and the surface they stand on.
- Range combines horizontal distance with height difference, so there is no melee through floors.
- Line of sight against walls and floors is required for targeted abilities and auto attacks (new error: "Target not in line of sight", shown on the action bar).
- Rings, hazards, cleaves and AoE hit only their own floor.
- Aggro across floors needs sight; leash is 3D.
- Enemies ignore targets they can neither reach nor see, and evade if nobody is left.

### AI
- Goals are 3D.
- Ranged spots are searched for sight of the enemy and of the healer.
- Ranged DPS move back into the healer's sight before casting.
- Healers move instead of smiting when the tank is out of sight.
- Result: the Warrens rose from 97% to 99% wins and the Ashen Spire is at 97% (98% before), with line of sight now enforced.

### Environments and tools
- **Cavern Traverse**, an original graybox test dungeon: winding passage, fork and dead end, ramp to a high walkway, rope bridge over an underpass, a gallery stacked above it, an 8 m one-way drop into the Magma Basin, narrow tunnel, lava pit, fire vent, unreachable perch.
- The renderer builds floors, rock, decks, ramps, stairs, walls that follow ramps, drop strips and lava from the simulation's `Nav`.
- Camera: lowers its pitch under bridges and low ceilings, and hides your own model when pulled within 2 m.
- Navigation debug overlay (**N**; **Shift+N** for walls and walk areas): surfaces by height with labels, portals, drops, every companion's path / waypoint / goal, unreachable goals, stuck actors.
- New flags: `--nav-debug`, `--zoom D`.

## 0.3 — combat UX, controls and gameplay polish

"Make the game better to play before making it bigger." No new dungeons,
classes or balance changes; saves stay compatible (new settings default in).

### Controls and camera
- Classic MMO controls (`controls.rs`, a pure, unit-tested rules function):
  right-drag rotates the camera and steers the character; left-drag is free look
  that never changes facing; a left *click* (under 5 px of travel) still selects;
  both buttons run forward; A/D turn (strafe while right-drag is held); Q/E
  always strafe; arrow keys rotate/tilt the camera.
- Optional auto-follow: while moving, the camera eases behind the character;
  manual camera input suppresses it for 1.2 s. Toggle with **V** (saved in the
  profile; on by default).
- Camera collision probes five rays (0.35 m radius), pulls in fast and eases out
  with a deadband, so it no longer jitters at wall corners.

### Party frames and healing
- Party and raid frames are buttons: the whole frame selects that member; gold
  border = selected, blue = mouseover; dead (red) and stunned/silenced (purple)
  tints; health numbers, debuffs (dispellable ones marked `*`) and buffs.
- Clickable target-of-target frame (T still assists).
- **Mouseover healing**: friendly spells go to the ally under the cursor (party
  frame first, then the ToT frame, then the 3D character), else the selected
  ally, else yourself — without changing your selection. Hostile spells ignore
  mouseover. A blue ground ring shows who the spell would land on. Every cast
  still goes through the simulation's normal validation.
- HUD panels block the world: clicks on them never select characters behind
  them, and drag-look and zoom don't trigger over them. Nameplates, floating
  text, banners and the tooltip never intercept clicks.

### Action bar
- Driven by `Sim::ability_state`, the same checks a key press runs: cooldown
  curtain with seconds left, a separate thin GCD sweep, red tint and
  "Out of range" (edge-to-edge range, as the sim measures it), blue tint and
  "need 40 energy - 2.5s" when short of resource (shown even during the GCD),
  and blockers: no/invalid target, silenced, stunned, stop to cast, nothing to
  dispel, target weakened.
- Failure messages name the ability ("Mend: Out of range") and are not
  re-triggered while a key is mashed.

### Threat
- Enemy nameplates are coloured from your perspective. Tank: green secure,
  yellow contested (someone ≥ 80% of the way to pulling), blue held by another
  tank, red attacking a non-tank. Others: muted when safe, yellow when you're
  close to pulling, red when it's attacking you. The health-bar frame repeats the
  colour (white for your target).
- Target frame: threat status line, a bar toward the pull threshold (110% in
  melee, 130% at range), who it attacks (and whether taunted), your threat, the
  top competitor and the pull value.
- Restrained alerts (at most one per 2.5 s): "Aggro lost: X is attacking Y -
  taunt!" for tanks, "X is attacking you!" for others; "Taunted X" confirms a
  taunt immediately.

### Resources
- Readout above the action bar: energy rate and time until full; mana regen
  with the five-second-rule countdown; rage gain/decay explanation. Action slots
  show the time until an ability becomes affordable.

### Tooltips
- One framework (`tooltip.rs`) with text generated in `dungeon_sim::describe`
  from the real definitions: abilities (costs, range, cast, cooldown, target
  rule, effect numbers scaled by your power), auras (effect, time left, stacks,
  dispellable, source), stats (formulas from RULES.md), gear (role fit,
  comparison with the equipped item, role score), AI profiles (their actual
  parameters), difficulties, classes, dungeons, vendor (price, comparison),
  sandbox knobs, editor operations and hub tabs. The panel flips/clamps to stay
  on screen and is never pickable.

### Readability
- Your own buffs/debuffs above the resource bar; shorter, non-wrapping frame
  names; wider nameplates; cast bars on enemy plates only; the default combat log
  drops companions' failed casts, cast starts and fully overhealed ticks (`L`
  shows everything); log no longer overlaps the action bar at 1280 px.

### Simulation APIs (no rule changes)
- `validate_cast` (side-effect-free; `try_cast` uses it), `queue_player_cast_on`
  and `player_cast_target` (explicit friendly target), `ability_state`,
  `resource_regen_per_sec` / `resource_rate` / `five_second_rule_left`,
  `threat_view` with `TankThreat` / `PersonalThreat`. Headless results and
  deterministic replay unchanged.

### Developer flags
- `--frame-test` (hover a party frame, press 1 as a Mender with an enemy
  selected, click another frame), `--tooltip-test`, and `--class NAME` /
  `--profile NAME` for demo runs.

## 0.2

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
- **Vendor** tab: buy unlocked Common/Uncommon (and later Rare) items for Vault
  Marks; salvage stash items for a third of their price.
- **Encounter editor** tab: tune a boss/elite's health and melee damage and each
  ability's interval, power, cast time and ring warning, then save a validated
  custom dungeon TOML (`90_<id>_custom.toml`) that is reloaded and selected. The
  content schema is serializable with a stable round trip. `--editor-test`.
- AI routes non-tanks around frontal cleave cones via a cone-free flank (escapes
  from ground effects keep priority); Hex is no longer spent on short-lived adds.
- Fourth dungeon, the raid **The Cinder Court** (10 players, level 10): gate
  guards, Twinmaw the Behemoth (tank swaps) and the Cinder Regent; tank-swap AI.

## 0.1 — prototype
- Hub → dungeon → boss → rewards → hub loop, four classes, AI party, headless runner.
