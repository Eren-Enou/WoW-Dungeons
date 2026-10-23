# Playtest: One More Run (0.11)

**The question:** after finishing the Gloomroot Warrens, do you want to
run it again, and why?

**Format:** three runs, about 35–45 minutes in all:
1. the game as it was;
2. the experiment;
3. the experiment again, with the reward you earned in run 2.

**What we want from you:** specific moments, not ratings. "The moment I
broke Brood Frenzy with a Ravage" is worth more than "8/10". If something
was dull or annoying, say exactly when.

The simulation measured pace, choices and effectiveness
(DUNGEON_FUN_0_11.md). It can't tell whether any of it is fun; that is
what this session is for. "The old way was better" is a useful answer.

## Setup (5 minutes)

Two separate save folders keep your real character untouched and give
both first runs the same start (level 4). In PowerShell, in the game's
folder:

**Baseline (run 1):**
```powershell
$env:DUNGEON_SAVE_DIR = "playtest_011_base"
.\dungeon-sandbox.exe --start-level 4
```

**Experiment (runs 2 and 3):**
```powershell
$env:DUNGEON_SAVE_DIR = "playtest_011"
.\dungeon-sandbox.exe --start-level 4 --experiment
```

From the repository, use `cargo run --release -p dungeon_game -- <the same
flags>`.

- **A fresh save** starts as a Striker with a full party.
- **In the hub:** on the Dungeons tab choose **The Gloomroot Warrens**,
  Normal, Classic layout.
- **With `--experiment`,** the hub's left panel says "ONE MORE RUN
  EXPERIMENT ON" and "CLASS LAB ACTIVE: Striker C: Flow".
- **Without it,** neither line appears and the Striker is Classic.
- **The Sandbox tab** (key 7) shows the experiment, its Technique choice
  and a "Reset progress" button.

## Run 1: the game as it was (about 8 minutes)

Use the baseline save, with the Classic Striker and no experiment. Play the
Gloomroot Warrens to the end, normally.

## Run 2: the experiment (about 12 minutes)

Use the experiment save, with the C: Flow Striker.

**Learn (first two packs).** Hover the action bar for the rules.

| Slot | Ability | Cost and effect |
|---|---|---|
| 1 | **Quick Cut** | cheap and quick: +1 Edge, +2 from behind |
| 2 | **Ravage** | spends all Edge (needs 2+); at 4+ it also hits enemies next to your target |
| 4 | **Kick** | interrupts, opens the enemy and gives Edge |
| 5 | **Seize** | takes an opening for damage and 2 Edge; it lights up gold when your target has one |

- **Openings:**
  - an enemy casting;
  - an enemy that just used an ability;
  - an enemy you kicked;
  - an enemy below 30%.
- **What shows them:** a gold marker over the enemy, its nameplate, and an
  "Opening: … - Seize!" line above your action bar.
- **Edge** shows as pips next to your energy.

**Mother Brakkis** (the third fight) now and then channels **Brood
Frenzy**:
- **the countdown:** her frame shows "Brood Frenzy in N s", then "BREAK
  IT: N to go";
- **breaking it:** hit her hard while she channels. **Ravage and Seize
  count triple.** It stops three more spiderlings and exposes her;
- **or don't:** keep fighting the brood instead.

**Then play to the end.** Read the results screen's "ONE MORE RUN"
section.

## Between runs (2 minutes)

If you won, Techniques are unlocked. In the Sandbox tab, choose one:
- **Riposte (react):** Seize an opening within 1.5 s of it appearing: it
  hits harder and costs nothing. Openings after enemy abilities last
  longer.
- **Patience (plan):** Edge holds 7. A Ravage with 6+ Edge hits everything
  around you and refunds energy. Seize gives 1 Edge.

If you lost, run 3 is still a second attempt. Note what you want to do
differently.

## Run 3: again, with your Technique (about 10 minutes)

Same save, same dungeon. Your party will probably have levelled up after
run 2: say if that, rather than the Technique, made the difference.

## After each run, write down

1. **The most enjoyable moment.**
2. **The most boring or frustrating moment.**
3. **The most meaningful decision you made** (or "none").
4. **Would you start another run right now? Why, or why not?**

## Once, at the end

**Moment to moment**
- Was there something interesting to think about during ordinary fights?
- Did actions feel meaningful, or repetitive? Was there still a lot of
  waiting?

**Mastery**
- Did you notice a mistake, or a chance to do better? What was it?
- Could you imagine getting better with practice? At what?

**The encounter**
- At Brood Frenzy, did you have to decide something? What did you decide,
  and was it worth it?
- Did it feel like your choice, or like the party's?

**The party**
- Did you feel valuable next to your companions, or like a spectator?
  Compare run 1 and runs 2–3.

**Progression**
- Was the Technique worth getting? Did it make run 3 play differently?
- Did the results screen give you a reason to go again, such as the
  coaching lines, the best time or the unlock?

**Coming back**
- Would you voluntarily start a fourth run? What specifically would make
  you?
- Compared with run 1, is the experiment better, the same, or worse?

**Anything else:** what felt good, what felt bad, what was confusing.

## Known limits

- **Experiment only:** the Gloomroot Warrens and the Striker; other
  classes rarely break Brood Frenzy.
- **Levels:** your party levels up between runs, so a faster run 3 may be
  levels as much as skill. The best time shows its level.
- **Edge past the cap is lost:** the results screen counts it.
- **Reset:** "Reset progress" on the Sandbox tab locks Techniques again;
  "Turn off" returns the game to normal. Your real save is never touched
  if you use the playtest folders.

## Notes

Write observations here with the date and build. Keep them separate from
the automated results in DUNGEON_FUN_0_11.md.
