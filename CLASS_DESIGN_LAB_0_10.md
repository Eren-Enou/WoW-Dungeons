# Class Design Lab (0.10): the Striker

**Question:** what kind of ability interactions make a player want to get
better at their class?

**What this is:** an experiment, not a class overhaul. The Striker gets two
playable alternative styles beside the unchanged original. Automated
measurement can say whether playing a style well makes a difference.
Whether it is fun is for the playtest (PLAYTEST_CLASS_LAB_0_10.md). Nothing
here is a final class design.

## Human feedback driving this (owner, reported 2026-10-17 with the 0.10 brief)

Positive:
- movement, controls and character control feel good;
- companions are very effective;
- the camera is good;
- the 0.9.2/0.9.3 clarity and impact changes made combat easier to
  understand;
- ground telegraphs are reasonably clear.

Problems:
- classes are very basic; most abilities feel like filler or simple
  numbers;
- there is little sense of a rotation;
- playing DPS feels unnecessary because companions handle encounters;
- more visual information alone doesn't make a shallow rotation
  satisfying;
- equipment works but has little personality;
- levelling needs more meaningful long-term development;
- there is nothing to master or progress toward yet.

Interest: talents, more consequential equipment, persistent progression.
These come after we know which core mechanics are fun.

## Findings: the current Striker (code review)

**Abilities** (`class.rs`; RULES.md "Classes"). Energy regenerates at 10
per second, maximum 100. Auto-attack: 40 every 1.8 s.

| Slot | Ability | Cost | Effect |
|---|---|---|---|
| 1 | Rend Strike | 40 | 95 melee damage |
| 2 | Whirl | 30, 10 s cooldown | 60 to every engaged enemy within 6 m |
| 3 | Evade | 20 s cooldown | −50% threat |
| 4 | Kick | 25, 10 s cooldown | 15 damage + interrupt, 4 s silence |
| 5 | Lacerate | 35 | bleed 240 over 12 s, ignores armour |

**Why it feels like filler:**
- **No ability changes another.** Each is a self-contained transaction of
  energy for damage, and nothing happens between them.
- **One correct answer.** Lacerate is by far the best damage per energy
  (6.9 against Rend's 2.4), so the answer is always the same: keep
  Lacerate up, press Rend with the rest. There is no tension between
  spending now and spending later.
- **Kick is a tax.** It takes 25 energy and gives nothing back to the
  rotation.
- **Positioning has one reason:** standing behind avoids parries (5%).
- **Short fights:** a trash pack dies in about 8–15 s. Lacerate's 12 s
  barely pays off, and nothing rewards adapting to that.
- **The AI already plays this optimally** (`ai.rs`, `dps_inner`):
  - Whirl on two or more targets when threat allows;
  - Lacerate on targets over 1,500 health and above 25%;
  - otherwise Rend;
  - interrupts assigned party-wide.

  A human can at best match a companion. That is the owner's
  "DPS feels unnecessary" in mechanical form.

**How the systems can host an experiment:**
- **Abilities** are an `AbilityId` → `AbilityDef { effect, cost, ... }`
  table. An actor carries `abilities: Vec<AbilityId>` and labels, filled
  from `ClassId::abilities()` when the run is built (`Sim::with_options`).
  A variant can give the player a different list of `AbilityId`s without
  touching the classic definitions.
- **Auras** already carry stacks, duration and source, appear on the
  player's buff line, enemy nameplates and the target frame, and expire on
  their own. A secondary resource or a temporary target state can be an
  aura: no new save data, and it is visible in the existing UI.
- **Readiness:** `Sim::validate_cast` and `Sim::ability_state` drive both
  the action bar and the AI's `usable`. A new requirement ("needs Edge",
  "needs an opening") becomes a `CastError`, and the action bar shows it.
- **Run options:** `RunOptions` is per run and not saved, which makes it the
  right place for a lab switch.
  - The profile (`profile.rs`) is untouched, so saved characters can't
    change.
  - Companions keep `ClassId::abilities()`.
  - The variant applies to the player's actor only, and only when the
    player is a Striker.
- **Determinism:** the new mechanics use no random numbers beyond the
  existing hit and crit rolls. With the classic style nothing new runs, so
  classic runs must stay hash-identical to 0.9.3.

## The experiments

The experiments are player-only. Companion Strikers always play the
classic Striker: their AI (`dps_inner`) is written for it, and giving them
the experimental abilities would silently change every party. The
comparisons below keep the companions identical.

Shared rules:
- the five-button layout stays: slots 3 (Evade) and 4 (Kick) are the same
  abilities;
- melee range and auto-attack are unchanged;
- the numbers are first guesses, tuned only so a well-played style lands
  near the classic Striker's damage rather than far above it.

### Experiment A: Planned (build and spend)

**Edge** is a secondary resource on you, from 0 to 5, shown as pips and on
your buff line. It lasts 20 s from the last gain (so it fades after a
fight) and carries over when you switch targets.

| Slot | Ability | Cost | Effect |
|---|---|---|---|
| 1 | **Honed Strike** | 40 | 80 damage; +1 Edge, **+2 from behind** |
| 2 | **Reaping Whirl** | 30, 6 s cooldown | spends all Edge: 40 + 30 per Edge to every engaged enemy within 6 m; usable at 0 Edge |
| 3 | Evade | - | unchanged |
| 4 | Kick | 25 | unchanged; **a successful interrupt gives +2 Edge** |
| 5 | **Deep Lacerate** | 25, needs Edge | spends all Edge: a bleed of 60 per Edge over 12 s; it replaces your previous bleed on that target (its remaining damage is lost) |

**Value per Edge:** 60 as a bleed on one target that lives 12 s; 30 per
target as Reaping Whirl, immediately.

**Decisions:**
1. **Spend now or build to 5?**
   - A pack about to die rewards cashing in early with Reaping Whirl.
   - A boss rewards a full 5-Edge bleed.
   - At 5 Edge, more builders are wasted.
2. **Future or immediate damage on one target?**
   - Deep Lacerate is worth twice as much per Edge, but only if the target
     lives 12 s and the bleed isn't refreshed early.
   - Refreshing early throws away the old bleed's remainder, so a live
     bleed makes Reaping Whirl the better spend.
3. **Energy for Kick or the builder?**
   - Holding 25 energy for an interrupt delays the builder.
   - But a landed Kick pays +2 Edge, so a good interrupt is also good
     damage.
4. **Positioning.** Getting behind the target doubles Edge per builder. On
   a boss that turns, or in a cleave, that is a real trade-off against
   safety.

**Failure modes we are watching for:**
- a fixed sequence (5 builders → finisher) every time;
- ramp-up too slow for trash;
- a finisher that is just a bigger number.

The measurements and the playtest look for these.

### Experiment B: Reactive (openings)

An **opening** is a short state on an enemy, shown above its nameplate
(and on the target frame). Your **Exploit** can only be used on a target
with an opening. Openings come from things the enemy does, or that you
do; none are random:

| Opening | When | Lasts |
|---|---|---|
| **Distracted** | the enemy starts casting | while it casts |
| **Recovering** | the enemy finishes a special ability (cleave, ring, bolt, summon, pulse, debuff, heal) | 3 s |
| **Off Balance** | your Kick interrupts it | 4 s |
| **Faltering** | it drops below 30% health (once per enemy) | 5 s |

| Slot | Ability | Cost | Effect |
|---|---|---|---|
| 1 | Rend Strike | 40 | unchanged (the dependable filler) |
| 2 | Whirl | 30, 10 s cooldown | unchanged |
| 3 | Evade | - | unchanged |
| 4 | Kick | 25 | unchanged; a landed Kick opens the target (Off Balance) |
| 5 | **Exploit** | 30, needs an opening on the target | 200 damage, consumes the opening, refunds 20 energy if it lands |

There is no bleed: B has no state to maintain, only windows to take.

**Decisions:**
1. **Exploit a cast or Kick it?**
   - A Distracted caster can be exploited and then kicked before the cast
     completes, if you hold 55 energy.
   - Or kick it at once, which also opens it (Off Balance).
   - Or let a companion's kick handle it.

   The greedy line pays more and risks the cast going off.
2. **Pool or spend?** Rend now, or keep 30 energy ready for the next
   opening? Energy at the cap is waste; an opening without energy is a
   missed chance.
3. **Switch targets?** An opening on an add next to the kill target is
   worth a short switch; one across the room isn't.
4. **Whirl or hold?** On a pack, Whirl is guaranteed damage now; openings
   come from several enemies at once.
5. **Faltering:** a free window near the end of every enemy's life,
   decided against the kill target's priority.

**Failure modes we are watching for:**
- openings too rare (B collapses to Rend spam) or too frequent (Exploit
  spam with no choice);
- unreadable triggers;
- windows closed by the GCD.

### The baseline

**Classic Striker:** exactly as before, the default. With it, the
simulation must stay hash-identical to 0.9.3.

## Measuring "does playing well matter?"

The player's slot is played by a scripted strategy (`RunOptions` lab
plan). Companions run their ordinary AI in every condition.

| Plan | What the player's character does |
|---|---|
| **passive** | follows and auto-attacks, no abilities, no interrupts |
| **simple** | one repeatable priority per style: classic: Lacerate if missing, else Rend; A: Deep Lacerate at 5 Edge, else Honed Strike; B: Exploit when the target is open, else Rend. Interrupts as assigned |
| **expert** | the intended play: classic: the companion AI's rotation; A and B: the decisions above (cash-out timing, bleed care, pooling for Kick, Exploit-then-Kick, switching to an open neighbour) |

**Measured per style × plan, on equal seeds, at levels 3 and 6, over
trash, elite and boss fights:**
- player damage and DPS, and party DPS;
- encounter duration;
- wins, deaths, damage taken, avoidable damage;
- interrupts;
- energy wasted at the cap;
- ability use by slot;
- for A: Edge wasted at the cap;
- for B: openings seen and openings used.

**A strong result:**
- expert is clearly better than simple, and simple better than passive;
- the gap is larger than for the classic Striker;
- party DPS and duration show the player's play matters;
- wins don't depend on it;
- companions are unchanged.

## Progression concepts (design only; not implemented)

### For A (Planned)

**Talents:**
- *Patient Blade:* Edge cap 7, but builders give +1 only (bigger, slower
  payoffs; favours bosses).
- *Opportunist's Edge:* Reaping Whirl refunds 1 Edge per enemy killed (a
  trash specialist).
- *Lingering Wounds:* Deep Lacerate adds the old bleed's remainder
  instead of losing it (removes the refresh penalty; trades away a
  decision for comfort).

**Equipment:**
- *Duelist's Grips:* builders from the front also give +2 Edge, but you
  take 10% more damage from the front.
- *Serrated Edge:* each Edge spent on a bleed also slows the enemy's
  attacks; Reaping Whirl costs 10 more energy.
- *Interrupter's Band:* a landed Kick gives +3 Edge; Kick's cooldown is
  2 s longer.

**Higher-level upgrade:** at 5 Edge, Honed Strike becomes "Overflow": no
Edge gain, but it extends your bleed by 3 s. Capping stops being waste and
becomes a choice.

### For B (Reactive)

**Talents:**
- *Read the Fight:* Recovering lasts 5 s (more forgiving windows).
- *Chain Opening:* an Exploit that kills opens the nearest enemy (Faltering
  2 s), rewarding target choice in packs.
- *Calculated:* Exploit no longer refunds energy, but deals 50% more to a
  Distracted caster (pushes the greedy exploit-then-kick line).

**Equipment:**
- *Watcher's Hood:* Faltering at 40% instead of 30%; Exploit costs 5 more.
- *Counterweight Blade:* Off Balance lasts 6 s; Kick costs 10 more energy.
- *Gambler's Ring:* your crits also open the target (1 s), but your
  openings from enemy abilities last 1 s less (a different, riskier
  rhythm).

**Higher-level upgrade:** a second Exploit charge, so two openings close
together can both be taken.

## Results

To be filled in from measurements (section below, after implementation).
