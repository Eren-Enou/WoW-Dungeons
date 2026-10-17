# Playtest script: 0.9.2 combat clarity

**Purpose:** find out whether you can now tell what each of your abilities
did, without reading the combat log.

**What changed (presentation only; the fights themselves are identical to
0.9.1):**
- **Your recent actions:** up to four lines above the action bar, for
  example:
  - "Shield Slam > Cragbound Brute: 79";
  - "Mend > Brannoc: +120 (300 overheal)";
  - "Shield Bash > interrupted Morvane's Hollow Bolt";
  - "Silence Pulse: nothing to interrupt".
- **Floating numbers are ranked:**
  - yours are larger, and your crits larger still;
  - your ticks are smaller than your direct hits;
  - companions' numbers are small and pale;
  - in a 10-player raid, companions' ordinary hits and heals are not
    floated.
- **Interrupts** float the name of the spell they stopped.
- **Your shields, defensives and crowd control** float their name where they
  land.
- **This fight's numbers** show at the bottom right; press **K** to hide or
  show them.

The automated checks (COMBAT_CLARITY_0_9_2.md, "Results") show that the
lines appear and that the simulation is unchanged. They cannot say whether
the screen is easier to read. That is what this script asks.

## Setup (5 minutes)

**Two builds:**
- **0.9.1 (before):** `dist/DungeonSandbox-0.9.1-windows-x64/dungeon-sandbox.exe`.
- **0.9.2 (after):** `cargo run --release -p dungeon_game`, or the 0.9.2
  package.

Each build keeps its own `saves/` folder (beside the program, or in the
repository for `cargo run`). To play both with the same party, copy
`saves/profile.json` from one to the other, or use the Sandbox tab to set
levels.

**Two runs per build, about 10 minutes each:**
1. **Run 1:** The Sunken Vault (Normal) as the class you usually play.
2. **Run 2:** The Hollow Reliquary (Classic) as a different class. If you
   usually play a damage class, try the Mender.

**Optional:** one Cinder Court raid (Party tab > Raid (10)) on 0.9.2 only,
to judge the screen with ten people.

Play the 0.9.1 runs first, so the comparison is "before, then after".

## Questions after each 0.9.2 run

1. **Your last button press.** After pressing an ability, could you tell
   what it did (hit, crit, dodged, how much)? Did you look at the feed above
   the action bar, the floating number, or neither?
2. **Interrupts.** When you interrupted, was it clear that it worked and what
   you stopped? When you pressed it with nothing to stop, did you notice the
   "nothing to interrupt" line?
3. **Heals (Mender).**
   - Could you tell effective healing from overheal?
   - Was it clear when Ward shielded someone, and when it absorbed damage?
4. **Your numbers vs the party's.** Did your floating numbers stand out from
   the companions'? Was anything too noisy, or too faint?
5. **Danger.** Could you still see the dangerous things:
   - enemy cast bars;
   - ground telegraphs;
   - "MOVE OUT!";
   - the threat alerts?

   Did any text cover them or get in the way of clicking?
6. **Fight numbers (K).** Did you look at the bottom-right numbers? Were they
   useful, distracting, or ignored?

## Once, at the end

7. **Before and after.** Compared with 0.9.1, is combat:
   - easier to read;
   - the same;
   - or harder?

   What made the difference?
8. **Raid (if played).** With ten people, could you follow your own
   actions?
9. **Anything wrong.** Was a number wrong, missing, or attributed to the
   wrong ability or person? Give the ability, the target and roughly when.

## Known limits

- **What the feed leaves out:** your auto-attacks, and the ticks of your
  heal and damage over time. Those still float above heads, smaller.
- **Ward's shield size is not shown** when it is cast. Only the damage it
  absorbs shows, as "(N abs)" or "Absorbed N".
- **No "lifesaving heal" highlight:** the simulation doesn't record health
  before a heal.
- **Hiding the fight numbers:** K is a toggle for this run only; it is not
  saved.

## Notes

Human observations go here, with the date and build. Keep them separate from
the automated results in COMBAT_CLARITY_0_9_2.md.
