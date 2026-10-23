# The One More Run experiment (0.11)

**Question:** what makes a player finish a dungeon, return to the hub, and
willingly start another?

**What this milestone is:** one experiment in one vertical slice, the
Striker in the Gloomroot Warrens. It is opt-in, from the hub's Sandbox tab,
and saved. With it off, the game plays exactly as before. Fun is not
claimed anywhere below: the simulation measures pace, choice and
effectiveness, and PLAYTEST_DUNGEON_FUN_0_11.md is where a person judges it.

## What we started from

**The problems** (owner feedback, CLASS_DESIGN_LAB_0_10.md):
- **Pacing:** every Striker style pressed only 15–23 times a minute; for
  a third to two thirds of combat there was nothing sensible to press.
- **Reactive play** (B) gave the most frequent meaningful moments.
- **Players don't feel essential:** the AI companions are very competent.
  A player's play mattered on paper (a passive player made runs 12–17%
  longer) but didn't feel important.
- **Equipment** is numbers only; there was no between-run choice that
  changes how you play.

**The hypothesis:** a dungeon is worth running twice when:
- **combat** gives something to think about in ordinary fights;
- **at least one encounter** asks a tactical question the player, not the
  AI, answers;
- **the end of a run** hands over a concrete reason to try again: something
  new to play with, and something specific to do better.

## Design and implementation

Everything below is gated on the experiment. Classic runs stay
hash-identical (see "Regression").

### 1. Combat: the Flow Striker (style C)

Built from the 0.10 findings: B's openings, a short build-and-spend
taken from A, a positioning bonus that isn't a gate, and a faster pace.
Five buttons; slots 3 and 4 are the usual Evade and Kick.

| Slot | Ability | Cost | Effect |
|---|---|---|---|
| 1 | **Quick Cut** | 20 | 35 damage; +1 Edge, **+2 from behind** |
| 2 | **Ravage** | 25, needs 2+ Edge | spends all Edge: 30 + 25 per Edge; at 4+ Edge, half of it to enemies within 5 m of the target |
| 3 | Evade | - | unchanged |
| 4 | Kick | 25 | unchanged; a landed Kick opens the target (Off Balance) and gives +1 Edge |
| 5 | **Seize** | 25, needs an opening | 80 damage, +2 Edge, consumes the opening, refunds 10 energy |

- **Edge:** 0–5, lasts 20 s.
- **Openings:** as in 0.10 B:
  - Distracted while casting;
  - Recovering for 3 s after an ability;
  - Off Balance after your Kick;
  - Faltering below 30%.

**Decisions it asks for:**
- **Seize now, or cut toward 4 Edge?** Seize gives 2 Edge but can push
  past the cap.
- **When to Ravage:**
  - at 2–3 Edge on a dying target;
  - at 4+ for the splash on a pack;
  - or hold it for Brood Frenzy.
- **Where to stand:** behind for double Edge, or wherever is safe.
- **Which opening:** the target's, or a neighbour's?

**How the numbers were set:**
- The first numbers made Flow 40% stronger than Classic. All of its
  ability damage was scaled to about 0.7.
- Quick Cut first gave Edge only from behind. That punished a player who
  doesn't reposition by about 20%, so it now gives +1 from the front.
- The scripted "simple" player presses whatever lights up: that is how a
  beginner plays.

### 2. Encounter: Brood Frenzy (Mother Brakkis)

Experimental content in `02_gloomroot_warrens.toml` (`experimental =
true`; the loader drops it unless the experiment is on):
- **The channel:** every 24 s (first at 14 s), Brakkis channels for 4 s,
  then summons 3 spiderlings. It can't be interrupted.
- **Breaking it:** dealing **1,200 damage** to her during the channel
  breaks it. **Finishers and opening strikes count triple** (Seize,
  Ravage, Exploit, Deep Lacerate, Reaping Whirl). The threshold scales
  with the party's level against the dungeon's recommended level.
- **A break:** stops the summon, exposes her for 6 s (+30% damage taken)
  and opens her for Seize.
- **What the player sees:**
  - the boss frame shows "Brood Frenzy in N s", then "BREAK IT: N to go";
  - a banner announces the channel;
  - "BROKEN!" floats over her, and a feed line confirms the break.

**What the player decides:**
- **Prepare:** hold 4+ Edge and energy while the countdown runs, at the
  cost of damage on the adds already out.
- **Switch:** leave the spiderlings to the companions and unload on
  Brakkis.
- **Or ignore it:** take three more adds and let the party deal with them.

All three are reasonable. Breaking makes the fight shorter and safer, and
ignoring it is not fatal.

**Why it works this way (a measured redesign):**
- **The first version was a plain damage race.** Breaking simply took
  enough damage of any kind.
- **That put the companions in charge.** During a channel they deal about
  540 damage at level 4 (25th–75th percentile: 400–680); the player deals
  215–340.
- **So the player didn't decide anything.** The companions' run-to-run
  spread (±140 around the median) was bigger than anything a prepared
  player added (about 100). Breaks were a coin flip, the same for a passive
  and an expert player.
- **The fix:** weak-point hits count triple, and the threshold sits above
  what the companions usually manage alone.
- **Companions are unchanged.** They don't know the mechanic, and play as
  they always did.

### 3. Between runs: Techniques

Clearing the Gloomroot Warrens with the experiment on unlocks two
Techniques for the Flow Striker. You choose in the Sandbox tab, can swap
any time between runs, and can reset with "Reset progress".

| Technique | Rules | Plays as |
|---|---|---|
| **Riposte** (react) | A Seize within 1.5 s of its opening appearing hits 25% harder and costs no energy. Openings after enemy abilities last 4.5 s. | watch the gold marker and strike as it appears |
| **Patience** (plan) | Edge holds 7. A Ravage with 6+ Edge hits every enemy within 8 m in full and refunds 25 energy. Seize gives 1 Edge. | build long, then one big Ravage |

- **First Riposte design:** Kick every 6 s, a landed Kick doubling the next
  Seize. Measured: the player lands only about 2 Kicks a run in the
  Gloomroot Warrens, because there are few casters in melee reach and the
  companions take most of them. It was reworked to reward reaction speed.

### 4. The loop: what a run hands back

**The results screen's "ONE MORE RUN" section** (`experiment.rs`) shows:
- news: Techniques unlocked, a new best clear time (with the party's
  level), the most Brood Frenzy breaks;
- coaching lines from the run's facts, each with something to try:
  - "Brood Frenzy: broke 2 of 4. Next time: … keep 4+ Edge and 25 energy
    …";
  - openings seized out of those offered;
  - Edge lost at the cap;
  - the Technique's own goal (quick Seizes, big Ravages).

**The hub's left panel** shows the experiment, the Technique and what is
locked.

**Saved state** (`Profile::experiment`, `serde(default)`):
- on/off;
- unlocked;
- the Technique;
- clears;
- best clear time and its level;
- the most breaks.

Old saves load with the experiment off, and older builds ignore the field.
"Reset progress" clears everything except the on/off switch.

**Switches:** `--experiment [--technique riposte|patience]` turns it on in
the save in use, for demos and playtests. `dungeon-headless --experiment
--technique …` does the same for measurements.

## Measurements (verified 2026-10-22, automated)

**Tools:**
- `dungeon-headless --lab` (styles × passive/simple/expert);
- `--lab-feel` (pace and reaction-time proxies; the "1.0nb" rows are a
  sloppy player: 1 s reaction and never repositioning);
- 10–20 seeds.

**Measuring is not playing.** The scripted expert reacts within a tick
unless a delay is set.

**Re-run on the final build.** After the Riposte rework (a Seize or
Exploit now takes the newest opening) the five-dungeon table was run
again. Every figure moved by 1.5 DPS or less, and the ranges below cover
both runs.

### Pace and waiting, five dungeons, level 6 (experiment off)

| | presses/min | idle | waiting (nothing sensible) | DPS (range over dungeons) |
|---|---|---|---|---|
| Classic expert | 15–16 | 74–76% | 59–67% | 63–67 |
| B expert | 19–23 | 64–70% | 33–44% | 65–76 |
| **C Flow simple** | **25–29** | **54–60%** | **51–56%** | **59–67** |
| **C Flow expert** | **26–30** | **52–59%** | **33–46%** | **66–78** |
| C Flow expert, 1 s reaction | 25–30 | 53–61% | 33–45% | 63–74 |
| C Flow sloppy expert (1 s, never behind) | 26–30 | 53–60% | 33–45% | 60–72 |
| C Flow sloppy simple | 25–29 | 54–60% | 50–56% | 56–64 |

**What the numbers show:**
- **Pace:** Flow raises it to 25–30 presses a minute, against 15–23 for
  every other style, and cuts idle time by 16–22 points against Classic.
- **Waiting with nothing sensible to press** falls to B's level for an
  expert, but stays around 50% for simple play.
- **Skill range:** simple to expert is +11–20%. A sloppy expert keeps most
  of it. A sloppy simple player ranges from 11% below Classic simple
  (Gloomroot) to 1% above (Emberforge).

### Brood Frenzy, Gloomroot Warrens with the experiment on (20 seeds)

| | Level 4 breaks | Level 4 run | Level 6 breaks | Level 6 run |
|---|---|---|---|---|
| passive player | 0 of 7.5 | 475 s | 0.1 of 4.6 | 342 s |
| Classic simple | 0 of 5.75 | 390 s | 0.35 of 3.5 | 293 s |
| Classic expert | 0.05 of 5.4 | 387 s | 0.5 of 3.2 | 291 s |
| Flow simple | 1.3 of 4.75 | 380 s | 1.3 of 3.0 | 286 s |
| **Flow expert** | **2.4 of 4.05** | **353 s** | **1.9 of 2.75** | **270 s** |

**What the numbers show:**
- **The break rate follows the player's play,** not the level.
- **Breaking shortens the fight:** fewer channels happen, and fewer adds.
  Flow expert runs are 7–9% shorter than Classic expert runs.
- **Wins are 19–20 of 20 everywhere.** Breaking is a reward, not a gate.
- **The Classic Striker can rarely break it:** it has no weak-point
  abilities. This encounter rewards the experiment's kit by design.

### Techniques, Gloomroot Warrens with the experiment on

Flow expert DPS by reaction time (`--lab-feel`, 10 seeds):

| Technique | L6 instant | L6 0.5 s | L6 1.0 s | L6 1.5 s | L4 1.0 s | Ravages at Edge 7 |
|---|---|---|---|---|---|---|
| none | 72.5 | 71.4 | 69.5 | 68.7 | 56.3 | - |
| **Riposte** | 85.0 | 85.0 | 77.1 | 71.5 | 62.3 | - |
| **Patience** | 80.1 | 76.3 | 75.7 | 74.3 | 60.6 | 57–68% of spends |

**What the numbers show:**
- **Riposte pays for speed:** −16% from an instant reaction to a 1.5 s one.
  It also raises the pace to 31–40 presses a minute.
- **Patience barely depends on reaction** (−7%) and changes the shape of
  play: spends move from 5 Edge to 7.
- **At about 1 s reaction** the two are equal (77 and 76 at level 6). So
  the choice is about how you like to play, not which is stronger.
- **A Technique is worth about +10%,** a reward that is felt.
- **In the lab matrix** (instant reactions), Riposte is the strongest:
  92.6 against 84.1 and 78.8 at level 6. It favours fast players; human
  reaction times decide which comes out ahead.

### Does the player matter alongside the companions?

**Damage share, level 6:**
- **A passive player:** 12–13% of the party's damage.
- **Flow expert:** about 28%.

**At level 4 with the experiment:** a passive player's runs take 475 s,
a Flow expert's 353 s, and 1 of 20 passive runs was lost.

**Brood Frenzy** is the clearest case: whether it breaks depends almost
entirely on the player. With a passive player, the party breaks about 2%
of channels.

### The full loop (graphical, release build)

**Run 1:** `--demo --class striker --level 4 --dungeon gloomroot_warrens
--experiment`, on a scratch save.
- **Victory in 358 s.**
- **The results show:**
  - "New best: 2 Brood Frenzy channel(s) broken", "First clear 5:58",
    "TECHNIQUES UNLOCKED";
  - the coaching: broke 2 of 4, openings seized 84 of 129, 59 Edge lost
    at the cap.
- **Back in the hub,** it shows the experiment and "Technique: none
  chosen".

**Run 2:** the same save, `--technique patience`.
- **Victory in 263 s,** "New best clear 4:23 (was 5:58)".
- **During the run:** Ravage at Edge 7/7.

The party had levelled between runs. That is why bests now say their
level ("4:23 at level 6"): a faster run at a higher level is not the same
achievement.

**Screenshots looked at:**
- the Sandbox section;
- Brakkis broken (the "BROKEN!" float, "Exposed, Staggered" on her
  nameplate);
- the feed (Seize, Edge tags);
- the results screen and the hub.

Fixed after looking:
- the feed said "Exploit!" to a Flow player;
- the Edge tag landed on the opening line;
- three opening lines at once now show as at most one per enemy every
  2 s.

### Regression

- **The classic game is unchanged:** with the experiment off, 320 headless
  runs (8 dungeons × 4 classes × 10 seeds) have `log_hash` values identical
  to 0.9.2/0.9.3, the Gloomroot Warrens included.
- **Checks:** fmt, check and clippy clean; tests forge 28, game 41, sim
  181.
- **Self-tests, all exit 0:**
  - `--input-test` (plain, Flow, experiment);
  - `--frame-test`, `--tooltip-test`, `--click-test`;
  - `--hub-tour --experiment`, `--remix-test`;
  - `--viewer <id> --viewer-test` (Gloomroot, Hollow Reliquary).

## Limitations and what is not validated

- **Fun:** whether any of this is fun is not covered by a recorded human
  playtest. PLAYTEST_DUNGEON_FUN_0_11.md is the session.
- **Reactions are simulated:** delays of 0.5–1.5 s stand in for people.
  Riposte's value depends on real reaction times.
- **Waiting is still high for simple play** (about 50%): Flow's pace
  helps experts more than beginners.
- **Edge at the cap:** the scripted expert loses 16–60 Edge a run (Seize's
  +2 when near 5). The coaching line points it out; whether players find
  it fiddly is a playtest question.
- **Brood Frenzy suits the experiment's kit:**
  - the Classic Striker rarely breaks it, and other classes weren't
    designed for it (their weak-point hits don't exist);
  - it is one encounter in one dungeon;
  - raids and Heroic were not measured.
- **Techniques are two options with one unlock.** There is no long-term
  progression; this tests whether one gameplay-changing reward makes the
  next run more appealing.
- **Personal bests mix skill and levels.** They show the party's level,
  but levelling still speeds runs up.
