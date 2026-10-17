# Dungeon Remix polish (0.9.1)

**Scope:** a focused quality milestone. This document records what was
reviewed, what the evidence says, and which one or two improvements were
made. The "Results" section, at the end, records what was verified.

## Playtest evidence available (2026-10-16)

- **No human observations of 0.9 exist locally:**
  - PLAYTEST_REMIX_0_9.md "Notes" is empty;
  - the only save (`saves/profile.json`, 2026-10-15) records six manual runs
    of handwritten dungeons from 0.8.x;
  - no `generated/` folder exists, so no Remix or Seed run was played on
    this machine;
  - `logs/` holds automated test output only.
- **A Discord link was given with the request,** but it cannot be read from
  here (it needs a signed-in session). Its contents are not used.
- **The only recorded human feedback** is the owner's 0.8 notes
  (PLAYTEST_NOTES_0_8.md), restated in the 0.9 brief:
  - procedural variation is exciting;
  - different routes beat memorised turns;
  - side exploration is interesting at first but may feel unnecessary after
    about five runs;
  - optional encounters are easy to find;
  - the viewer is enjoyable;
  - the themes feel slightly different, and more assets would help.

Everything below is therefore based on automated measurements and code
review, and is labelled as such.

## Findings and priorities

Severity: **high** means it misleads or blocks the player; **medium** hurts
the experience; **low** is cosmetic or documentation.

| # | Issue | Evidence and reproduction | Severity | Proposed correction | Expected improvement | Verification |
|---|---|---|---|---|---|---|
| 1 | **Misleading flanks.** Some flank targets are strictly worse than the main door | 0.9 final `routes` runs (12–16 runs per choice, `scratchpad final/*.json`): flanks into the Bone Archers took +12% damage in 5/5 layouts with the same opener; into the Ash Hound Pack +4% (2/2). Recipe tags don't predict the result: Warden's Screen and Pyrecaller's Guard are both "screen" and "protect_caster", but measure +15% and −23%. Reproduce: `dungeon-forge routes assets/forge/blueprints/hollow_reliquary.toml --seed 2 --runs 3` and compare "by the flank" with "by the main door" | high (a route choice that only costs) | Each kit recipe gets a measured `flank` rating: `favoured` (easier by the flank), `different` (changes who is engaged first), or `poor` (no change, costs). Flank targets prefer favoured, then different, and never take poor unless the blueprint names it | Every flank either eases the fight or changes its opening, and no flank leads only to a worse fight | `routes` over 20 layouts per blueprint against the 0.9 table; `audit` 100 layouts; determinism; generator v5 |
| 2 | **Cache rewards are not attributed.** The player cannot tell what a chest gave | Code review: the results screen lists "Loot: ..." without a source; the HUD checklist lists encounters only; the History tab shows item names only. A cache's banner says "its loot is yours at the end of the run", but the end screen doesn't connect the item to it | medium (usability) | Say where each item came from ("from Reliquary Coffer", "from The Hollow Guardian"). The HUD checklist shows caches ("[x] Reliquary Coffer"). The results screen lists caches opened and missed | The player sees that the chest was opened and what it gave, or that a chest was missed | Unit test for item sources; `--remix-test` checks the HUD and results text; screenshots |
| 3 | **Overpulls with trail following** | Same build and seeds with and without `--slot-follow` (new headless flag): 8 configs × 15 seeds × 8 dungeons. 30 overpulls with 0.8.1-style following, 28 with the trail. Ashen Spire 26 → 19, Cinderdeep 1 → 5 (mostly Heroic), Emberforge 3 → 4, 0 elsewhere | low: not caused by 0.9 | None now. Ashen Spire's overpulls belong to its handwritten layout (TASKS) | - | Recorded |
| 4 | **Emberforge generation** 98/100 (seeds 46, 56 fail: drop geometry and flank angles) | 0.9 audit | low (Remix rolls again visibly) | None now. Re-measure after #1 changes the flank candidates | - | `audit 1..100` |
| 5 | **Safe way vs vents for AI parties.** AI parties time the vents, so the safe way costs 0–14 s and avoids 0–98 damage | 0.9 `routes` | low until a player reports | No tuning without player evidence | - | Playtest question 12 |
| 6 | **TASKS.md is out of date.** It says Emberforge generates 90/100; the final measurement is 98/100 | TASKS.md | low (documentation) | Fix | - | Re-measure |

**Selected:** #1 (a misleading route choice) and #2 (a player-facing
usability gap). #3–#6 are recorded, with no code change.

## Not changed (scope)

- No new kits, classes or combat mechanics.
- No new flank mechanic: the rating only steers which pack a flank leads to.
- Companion AI is unchanged; the headless `--slot-follow` flag is for
  measurement only.

## Results (verified 2026-10-16/17, automated only)

### 1. Measured flank ratings steer flank targets (generator v5)

**The rule.** Kit recipes carry
`flank = "favoured" | "different" | "poor"`, rated from `routes`
measurements:

| Kit | Recipe | Rating |
|---|---|---|
| volcanic | Pyrecaller's Guard (`guarded_caster`) | favoured |
| volcanic | Emberkin Raiders (`melee_pack`) | favoured |
| volcanic | `frontal_pack` | poor |
| crypt | Censer Procession | different |
| crypt | Bone Archers | poor |
| crypt | Warden's Screen | poor |

- **Warden's Screen** was first rated "different" on its 0.9 numbers, but
  the first 0.9.1 batch showed that over 17 layouts it is +13% damage with
  the opener changed in only 7. It was re-rated poor.
- **Selection:** flanks prefer favoured, then different/unrated, and never
  take poor unless the blueprint names that recipe; a named poor flank
  carries a warning in its facts. The viewer's branch facts show the rating.
- **Evidence strength:** the favoured `melee_pack` and poor `frontal_pack`
  ratings rest on few layouts (2 in 0.9, 6 more for `melee_pack` in 0.9.1).

**Flank targets, 100 layouts each (`audit`):**

| | 0.9 (v4) | 0.9.1 (v5) |
|---|---|---|
| Hollow Reliquary | Warden 49, Censer 36, Bone Archers 15 | Censer 100 |
| Emberforge | Pyrecaller 59, Raiders 21, Ash Hounds 18 | Pyrecaller 74, Raiders 26 |

**Flank effect, `routes` over 20 layouts per blueprint, 12–16 runs per
choice:**

| | 0.9 (v4, 30 flanks) | 0.9.1 (v5, 40 flanks) |
|---|---|---|
| flanks that make the fight easier (damage taken −5% or better) | 10 | 19 |
| flanks that change who is engaged first | 14 | 20 |
| flanks that do neither (only cost) | about 11 | 8 |

Per target in 0.9.1:

| Target | Layouts | Mean damage taken | Easier | Opener changed |
|---|---|---|---|---|
| Pyrecaller's Guard | 14 | −19% | 10/14 | - |
| Emberkin Raiders | 6 | −6% | 3/6 | - |
| Censer Procession | 20 | +2% | 6/20 | 18/20 |

The Censer Procession is "different": the approach changes, not the
difficulty. A flank is still not a guarantee: 8 of 40 only cost, and the
per-layout numbers are noisy at 12–16 runs.

**Generation:**
- `audit 1..100`: Hollow Reliquary 100/100, Emberforge 100/100 (0.9:
  98/100). Every branch delivers its purpose, and 0 are purposeless.
- `seeds 101..500`: Hollow Reliquary 400/400, Emberforge 385/400. About 3%
  of Emberforge seeds still fail, mostly on drop geometry; seed 109 is the
  `--remix-test` example.
- Same seed, same file (forge test).
- `GENERATOR_VERSION` 5. The classic layouts changed:
  - Hollow Reliquary seed 2 wins 56/60 in a 4-class report;
  - Emberforge seed 1;
  - the playtest seeds moved to 1, 17 and 15 (Hollow Reliquary) and 5
    (Emberforge).

### 2. Loot names its source; treasure opened or missed is shown

- `RunRewards.sources` records the encounter or cache each item rolled from;
  `Content::source_name` turns it into a name.
- **Results screen:** "Loot: ... - from Crawler Nest", and "Treasure:
  Reliquary Coffer opened" or "... not opened this run (walk up to a chest
  to open it)".
- **HUD checklist:** "[x] Reliquary Coffer (treasure)".
- **Tests:**
  - a sim test checks the sources (cache and encounter names);
  - a game test checks that the results text names the source and reports
    both an opened and a missed chest;
  - `--remix-test` passes with a cache in the run.
- **Full hub → dungeon → boss → rewards → hub cycle** (`--demo`, Hollow
  Reliquary classic): Victory, the results screen showed each item's source
  and the coffer as not opened (the AI demo leader does not seek chests),
  then back to the hub with the run in History. Screenshots looked at.

### Regressions

- **Against the 0.9.0 release** (its packaged binary and content; 8 configs
  × 15 seeds):
  - all six handwritten dungeons are hash-identical (720/720 runs), so the
    simulation is unchanged;
  - Emberforge classic: wins 105 → 105, deaths 78 → 75;
  - Hollow Reliquary classic (new v5 layout): wins 102 → 99, deaths
    101 → 125, all at the final boss. A 60-run report gives 56/60, within
    the 83–97% spread of seven layouts measured (1, 2, 3, 5, 7, 9).
- **Overpulls with and without trail following:** 30 vs 28 (finding #3).
- **Checks:**
  - fmt, check, clippy -D warnings clean;
  - tests: forge 28, game 27, sim 164 (1 ignored);
  - `--viewer-test` (3 dungeons), `--remix-test`, `--input-test`,
    `--frame-test`, `--tooltip-test`, `--click-test` and `--hub-tour` all
    exit 0.

### Not verified

- **Human playtest.** No human has played 0.9 or 0.9.1. Whether flanks now
  feel like real alternatives, and whether the treasure lines are clear,
  are playtest questions.
- **Flank ratings for packs not yet measured as flank targets** (e.g.
  `interrupt_pack`): unrated.
