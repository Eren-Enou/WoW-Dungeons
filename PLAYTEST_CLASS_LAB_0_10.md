# Playtest: Class Design Lab (0.10)

**Purpose:** find out which Striker style is fun to play and worth building
talents and gear around:
- **Classic**, the original;
- **A: Planned**, build and spend Edge;
- **B: Reactive**, exploit openings.

The simulation measured which style does more damage when played well
(CLASS_DESIGN_LAB_0_10.md, "Results"). It cannot tell whether a style is
fun. Higher simulated damage is not a reason to prefer a style: tell us
what you enjoyed.

**Time:** 30–45 minutes.

## Setup (5 minutes)

**Use a separate save folder**, so your own character is never touched.
In PowerShell, in the game's folder:

```powershell
$env:DUNGEON_SAVE_DIR = "playtest_lab"
.\dungeon-sandbox.exe --start-level 6
```

From the repository, use `cargo run --release -p dungeon_game -- --start-level 6`
with the same variable set.

- **A fresh save** starts as a level-1 Striker with a default party;
  `--start-level 6` raises everyone to level 6 in that save only.
- **Check the hub's left panel.** It should read "Wayfarer the Striker -
  level 6". If not, open the Classes tab and pick the Striker.
- **Choose the dungeon.** On the Dungeons tab, pick **The Gloomroot
  Warrens**, Normal, **Classic** layout. All three styles use this same
  dungeon. Its fights include:
  - packs with casters (interrupts);
  - a long elite (the Sporecap Hermit);
  - a boss that summons adds (target switching).

**To switch style:**
- open the **Sandbox** tab (key 7);
- under "Class Design Lab", click **Classic**, **A: Planned** or
  **B: Reactive**;
- the hub's left panel then shows "CLASS LAB ACTIVE: Striker ...", and in
  the dungeon the top-right panel shows "CLASS LAB: Striker ...";
- hover a style's button for its rules.

The choice lasts until you quit the game. It is never saved.

## The runs

Play each style in this order. In each one, play normally and lead the
party: your companions are as good as before, whatever you do.

### 1. Classic (about 8 minutes)
A full run of the Gloomroot Warrens. This is the baseline: notice what you
pay attention to.

### 2. A: Planned (about 12 minutes)

**Learn (first two packs):** hover the action bar to read the five
abilities.
- **Edge** shows as pips next to your energy ("Edge [###--] 3/5") and on
  your buff line.
- **Honed Strike** builds 1 Edge, or **2 from behind** the target.
- **A landed Kick** gives 2 Edge.
- **Deep Lacerate** spends all Edge on a bleed: worth the most on a target
  that lives 12 seconds or more. It replaces your previous bleed.
- **Reaping Whirl** spends all Edge on damage to everyone near you, now.
- The feed above the action bar says what each press did ("Honed Strike >
  ...: 63 [Edge 3/5]").

**Then:** the rest of the run. Notice:
- when you chose to spend early;
- whether you went for the bleed or the whirl;
- whether you held energy for Kick.

### 3. B: Reactive (about 12 minutes)

**Learn (first two packs):**
- An **opening** shows as a spinning gold marker over an enemy, by name on
  its nameplate, and as a feed line ("Opening: ... is recovering -
  Exploit!").
- The **Exploit** slot lights up gold ("OPENING!") when your target has
  one.
- **Openings come from:**
  - an enemy casting (Distracted);
  - an enemy that just used an ability (Recovering, 3 s);
  - your landed Kick (Off Balance, 4 s);
  - an enemy dropping below 30% health (Faltering, 5 s).
- **Exploit** uses up the opening, hits hard and gives back some energy.
- **Rend Strike** is the filler in between.

**Then:** the rest of the run. Notice:
- whether you saw openings in time;
- whether you kept energy ready for them;
- whether you exploited a caster before or after kicking it.

## Questions after each experimental style (A, then B)

Write a few words, not just a number. Leave anything blank rather than
guess.

**Engagement**
1. Did you pay attention to which ability you wanted next?
2. Did you ever make a real decision? Describe one.

**Satisfaction**

3. Was there a moment where a sequence or an opening felt rewarding?

**Flow**

4. Did a natural rhythm develop?
5. Was there too much waiting, or too much button mashing?

**Clarity**

6. Did the screen tell you what you needed:
   - the Edge pips;
   - the openings;
   - the gold Exploit slot;
   - the feed lines?
7. Could you tell why an ability was stronger or available?

**Mastery**

8. Could you see yourself getting better at this style?
9. Would you enjoy practising it?

## Once, at the end

**Preference**
10. Which of the three was most enjoyable?
11. Which became repetitive fastest?
12. Which would you want to play across several dungeons?

**Companions**

13. Did you feel your play mattered, compared with the Classic run?

**Progression**

14. Could you imagine talents or gear that expand one of these styles?
    CLASS_DESIGN_LAB_0_10.md lists ideas; say which, if any, appeal to
    you.

**Anything else**

15. Freeform impressions: what felt good, what felt bad, what was
    confusing.

## Known limits

- **Experiments.** These are first versions with rough numbers; a style
  that feels nearly right but has wrong numbers is still a useful result.
- **Player-only.** Companion Strikers always play Classic.
- **Not saved.** The style switch lasts for this session only.
- **Short dungeons give B little to react to.** The Sunken Vault has few
  enemy abilities: about 17 openings in a whole run, against about 90 at
  level 6 in the Gloomroot Warrens (simulated parties).
- **Pooling energy has a cost.** Holding energy for openings or Kick means
  sometimes sitting at full energy. Whether that feels like a decision or
  like waste is a question for you.

## Notes

Write observations here with the date and build. Keep them separate from
the automated results in CLASS_DESIGN_LAB_0_10.md.
