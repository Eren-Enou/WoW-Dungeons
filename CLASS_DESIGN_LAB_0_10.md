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
| 1 | **Honed Strike** | 40 | 60 damage; +1 Edge, **+2 from behind** |
| 2 | **Reaping Whirl** | 30, 6 s cooldown | spends all Edge: 40 + 45 per Edge to every engaged enemy within 6 m; usable at 0 Edge |
| 3 | Evade | - | unchanged |
| 4 | Kick | 25 | unchanged; **a successful interrupt gives +2 Edge** |
| 5 | **Deep Lacerate** | 25, needs Edge | spends all Edge: a bleed of 80 per Edge over 12 s; it replaces your previous bleed on that target (its remaining damage is lost) |

**Value per Edge:** 80 as a bleed on one target that lives 12 s; 45 per
target as Reaping Whirl, immediately.

The first guesses were Honed Strike 80, Reaping Whirl 30 per Edge and a
bleed of 60 per Edge. With them, Edge was too small a share of the damage
for any decision about it to matter (see "Results").

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
| 5 | **Exploit** | 30, needs an opening on the target | 120 damage, consumes the opening, refunds 10 energy if it lands |

There is no bleed: B has no state to maintain, only windows to take.

The first guess was 200 damage with a 20-energy refund. That made B 30–60%
stronger than Classic (see "Results").

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

## What was built

| Piece | Where |
|---|---|
| Styles, abilities, effects | `class.rs`: `StrikerStyle`, `abilities_for`; `HonedStrike`, `ReapingWhirl`, `DeepLacerate`, `Exploit`; `Effect::{Builder, EdgeWhirl, EdgeBleed, ExploitOpening}` |
| Edge and openings | `sim.rs`: as auras (`AuraKind::Edge`, `AuraKind::Opening`); `gain_edge`, `spend_edge`, `lab_open`; hooks on Kick, enemy casts and abilities, and the 30% health line |
| Requirements | `CastError::NoEdge`, `CastError::NoOpening` |
| Scripted play | `ai.rs`: `LabPlan` (passive, simple, expert) for the player's slot; `lab_cast` |
| Measurements | `lab.rs`, `dungeon-headless --lab [--levels 3,6]` |
| Switch | hub Sandbox tab "Class Design Lab", or `--striker-style classic\|a\|b`; session only |
| Feedback | HUD: Edge pips, "CLASS LAB" labels, gold "OPENING!" Exploit slot, "no Edge" / "no opening". Feed: "[Edge n/5]", bleeds, "Opening: ... - Exploit!". `fx.rs`: opening marker, heavy Exploit hits, Reaping Whirl ring |

**What it doesn't touch:**
- **Saves:** the style is not in the profile.
- **Companions:** always classic.
- **Classic hooks:** every hook is inert for the classic style.

## Results (verified 2026-10-17, automated only)

**The simulation measures effectiveness. Whether any style is fun is not
measured;** that is PLAYTEST_CLASS_LAB_0_10.md.

### The classic game is unchanged
- **Hashes:** 320 headless runs (8 dungeons × 4 classes × 10 seeds) have
  `log_hash` values identical to the 0.9.2 package's.
- **Classic through the lab:** a classic Striker run through the lab has
  the classic abilities.
- **Companion Strikers** keep the classic abilities in every style (a
  test).

### How the measurements were made
```
dungeon-headless --lab --dungeon <id> --runs 20 --levels 3,6
```
- **Coverage:** 5 dungeons (Sunken Vault, Gloomroot Warrens, Hollow
  Reliquary, Emberforge Vaults, Cinderdeep Hollows); levels 3 and 6;
  seeds 1–20; 3 styles × 3 plans; Normal difficulty.
- **Companions:** the same AI and party in every condition.
- **What level 6 is for:** at or above every one of these dungeons'
  recommended level, so it is used for damage comparisons.
- **What level 3 is for:** below the recommended level for Gloomroot (4)
  and Cinderdeep (5), so it shows whether the player's play decides
  survival.
- **Player DPS** is the player's damage divided by the time spent in fights
  (pull to clear).

### Does playing well matter? Player DPS at level 6

| Dungeon | passive | Classic simple → expert | A simple → expert | B simple → expert |
|---|---|---|---|---|
| Cinderdeep Hollows | 33 | 73 → 74 (+2%) | 74 → 79 (+7%) | 76 → 85 (+11%) |
| Emberforge Vaults | 31 | 68 → 70 (+2%) | 71 → 75 (+7%) | 75 → 82 (+10%) |
| Hollow Reliquary | 29 | 65 → 68 (+5%) | 66 → 70 (+6%) | 67 → 73 (+9%) |
| Gloomroot Warrens | 30 | 71 → 71 (0%) | 73 → 74 (+2%) | 73 → 75 (+3%) |
| Sunken Vault | 33 | 68 → 76 (+12%) | 69 → 71 (+4%) | 72 → 76 (+5%) |

**Classic: playing well barely matters.**
- A simple priority (keep Lacerate up, else Rend) is within 0–5% of the
  companion rotation in four dungeons out of five.
- This is the owner's "filler" observation, measured.

**A: a small but real skill gap** (+2% to +7%).
- **Expert vs simple:** the expert cashes in Edge on packs and dying
  targets and doesn't overwrite a fresh bleed. The simple plan wastes
  about 20 Edge per run against 0.5 for the expert, and still loses only
  2–7%.
- **Expert vs Classic expert:** −7% (Sunken Vault) to +8%.
- **The simple A plan already stands behind the target:** the AI's
  movement does that, so the +2 Edge from behind is free for both plans.
  For a human, positioning should matter more than these numbers show.
- **Not measured:** how much a player who doesn't position loses.

**B: the clearest skill gap** (+3% to +11%; +5% on the short Sunken
Vault).
- **Expert vs Classic expert:** 0% to +14%.
- **What the expert does:**
  - takes about 40% more openings than the simple plan (Cinderdeep: 95
    Exploits against 66);
  - switches to an open neighbour;
  - keeps 30 energy back;
  - kicks more: 9.6 interrupts against 6.2 at Cinderdeep, because pooled
    energy leaves Kick affordable.
- **The cost of pooling:** 9–17% of combat at full energy, 2–4 points
  more than Classic expert in the same dungeon (6–13%).

**The player matters to the party in every style:**
- **Damage:** a passive player (auto-attacks only) deals 12–14% of the
  party's damage; an active one 24–29%.
- **Run length:** the run takes 12–17% longer with a passive player
  (Cinderdeep L6: 494 s passive, 415–426 s active).

### When the party is under-levelled, play decides survival (level 3, wins out of 20)

| Dungeon (recommended level) | passive | Classic simple / expert | A simple / expert | B simple / expert |
|---|---|---|---|---|
| Cinderdeep Hollows (5) | 0 | 13 / 10 | 10 / 16 | 10 / 17 |
| Hollow Reliquary (4) | 13 | 13 / 18 | 16 / 19 | 14 / 14 |
| Gloomroot Warrens (4) | 0 | 1 / 1 | 3 / 0 | 0 / 3 |

**Cinderdeep at level 3:**
- with a passive player, the party never wins;
- with expert A or B, it wins 16–17 of 20.

The Gloomroot Warrens at level 3 is too hard for every plan. DPS figures
from level-3 runs are left out: they mix wins and wipes.

### Short fights: can each style complete its cycle?

"Payoff" means a Lacerate, Deep Lacerate, Reaping Whirl or Exploit. The
figures are payoffs per trash fight, the share of trash fights with one,
and the seconds from the pull to the first one (level 6):

| | Cinderdeep | Sunken Vault |
|---|---|---|
| Classic expert | 0 (the companion rotation skips Lacerate on small enemies) | 0 |
| Classic simple | 2.4, 100%, 2.0 s | 3.2, 100%, 2.0 s |
| A expert | 0.9, 88%, 5.0 s | 0.7, 68%, 5.6 s |
| B expert | 2.3, 100%, 3.9 s | 2.4, 95%, 3.5 s |

**A on trash:**
- A completes about one build-and-spend cycle per trash fight, the first
  after about 5 s.
- On the Sunken Vault's short fights a third of them end before any
  payoff.
- This is the "ramp-up too slow for trash" failure mode, partly present.

**B on trash:** B gets its first opening within about 4 s. On bosses
the expert takes 23–27 Exploits per fight in the four longer dungeons,
and 6.5 on the Sunken Vault's.

### Openings by dungeon (B, level 6, per run)

| Dungeon | Openings | Exploits (expert) |
|---|---|---|
| Hollow Reliquary | 129 | 66 |
| Cinderdeep Hollows | 138 | 95 |
| Gloomroot Warrens | 88 | 64 |
| Sunken Vault | 17 | 14 |

B depends on the encounter design. A dungeon whose enemies rarely cast or
use abilities gives it little to react to.

### Tuning done (and why)

**The first numbers:**
- B: 30–60% above Classic. An Exploit cost a net 10 energy for 200
  damage.
- A: flat. Expert and simple were within 1%, and below Classic.

**The changes:**
- Exploit 200 → 120 damage, refund 20 → 10.
- A: Honed Strike 80 → 60, bleed 60 → 80 per Edge, Reaping Whirl 30 → 45
  per Edge. This makes Edge a bigger share of damage, so the spending
  decisions carry weight.

**The A expert script was revised once.**
- **Before:** it cashed in with Reaping Whirl whenever the bleed had more
  than 2 s left.
- **The finding:** the simple plan refreshes the bleed every 5 Edge, about
  every 12 s, close to the bleed's own length. So the refresh penalty
  rarely bites.
- **After:** the expert compares the bleed's value (minus the lost
  remainder) with the whirl's value on the enemies in reach.

No balance was changed for any other class.

### What the numbers suggest (not conclusions about fun)

- **A:** the decisions exist but pay little against a competent simple
  priority, and the cycle is slow for short trash.

  If the playtest likes A's feel, candidates are:
  - faster Edge on pulls (an opener);
  - a stronger cash-out on dying targets.
- **B:** the most measurable difference between playing well and simply.
  It also has the biggest risk: it depends on enemies doing things, so
  each dungeon's fights would need tagging with that in mind.
- **Classic:** confirms the owner's feedback. Its only skill expression is
  keeping Lacerate up.
- **For other classes:**
  - mechanics that react to enemy actions (B) give the clearest skill
    gap;
  - mechanics that add a resource (A) need the payoff to be a large share
    of output, or the decisions don't register.

### The game loop with the experiments
- **Full runs:** `--demo` played the Gloomroot Warrens (level 6) to Victory
  with A (270.5 s) and with B (282.0 s), through results back to the hub.
- **Screenshots looked at:**
  - Edge pips and "[Edge n/5]" feed tags;
  - "no Edge" / "no opening" on the action bar;
  - "Opening: ... - Exploit!" lines and the gold marker over open enemies;
  - the Sandbox switch and the "CLASS LAB" labels.
- **Rewards:** a lab run gives ordinary rewards (XP, loot, Marks). Only
  the style is never saved.

## What is not validated

- **Fun, readability, flow and mastery** of any style: not covered by a
  recorded human playtest. PLAYTEST_CLASS_LAB_0_10.md is the guide.
- **How human positioning affects A:** the AI always stands behind, so
  the +2 Edge from behind was never lost in measurement.
- **Human reaction time for B's 3 s windows:** the scripted expert reacts
  within a tick.
- **Raids** (10 players) and **Heroic** difficulty were not measured.
- **Talents and equipment:** concepts only.
- **The tuning:** two passes, on 5 dungeons; the numbers are not final.
