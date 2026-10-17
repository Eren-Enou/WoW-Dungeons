# Playtest script: 0.9 Dungeon Remix

**Purpose:** find out whether remixed layouts give a player different, worthwhile
decisions about how to explore and approach familiar fights.

**What is already known:**
- The simulation measured what each branch changes for AI parties
  (DUNGEON_REMIX_0_9.md, "Results").
- It cannot say whether that is fun, readable or worth replaying.

Those are the questions below. Please write the answers in this file under
"Notes", in your own words. Leave a question blank rather than guessing.

## Setup (10 minutes)

1. **Start the game.** Start `dungeon-sandbox.exe` (or `cargo run --release
   -p dungeon_game`).
2. **Get the party ready.** In the hub, recruit a full party at level 4 or
   more. The Sandbox tab can set levels if needed.
3. **Open the dungeon.** On the Dungeons tab, select **The Hollow
   Reliquary**.
4. **Choose a layout.** In the **Layout** row:
   - click **Seed**;
   - type the run's seed;
   - press Enter.

   The line below the row should read "Next run: seed N (generator v4)".
5. **Optional: look before you play.** Press V to look at that layout in the
   viewer first, or don't. You can do runs either way, but say which you did:
   the viewer shows everything, and a player wouldn't.

A run takes 8–12 minutes. Play normally: you lead, your companions follow.

## The three Hollow Reliquary runs

Seeds updated for 0.9.1 (generator v5). The 0.9.0 seeds 19, 4 and 16 now
build different layouts.

**Run A: seed 1 (direct progression)**
- The fights come in the familiar order.
- A side passage leads into the Censer Procession's hall. Here it measured
  +16% damage taken with the same opening, so it is not worth it.
- A detour to a crawler nest and a coffer costs about +47 s, the most of 20
  measured layouts.
- In the simulation, going straight is the sensible run.

**Run B: seed 17 (rewarding exploration)**
- A doorway with a glowing tablet leads through the Crawler Nest to the
  Reliquary Coffer, and rejoins the main route further on.
- In the simulation it cost about 1 s of run time. The coffer was opened and
  the nest cleared in 16/16 runs, with no deaths there.

**Run C: seed 15 (tactical alternative)**
- A narrow side passage with a single dim candle enters the Censer
  Procession's hall from the side.
- By that way the first enemy engaged is a crawler, not the Censer Bearer.
  The fight measured 15% less damage taken.

Don't read the run descriptions above before each run if you want to test
whether the clues work on their own. Each one names its seed first, so you
can cover the rest.

## The Emberforge run (optional, 10 minutes)

**Emberforge Vaults, seed 5:**
- A corridor pulses with magma vents: about 56 vent damage per run for AI
  parties. A longer tunnel goes round it at about the same time.
- A crawlway leads into the Pyrecaller's hall from the side: 42% less damage
  taken there in the simulation.
- A detour holds the Obsidian Sentinel and a strongbox (about +23 s).
- A ledge drops you past empty caverns.

## New in 0.9.1: check these too

- **Treasure:**
  - The HUD's checklist (top right) now lists the chest as "(treasure)" and
    ticks it when opened.
  - The results screen says "Treasure: Reliquary Coffer opened", or "not
    opened this run".
  - Each loot line now says where it came from ("- from Reliquary Coffer").
  - Was it clear what the chest gave you?
- **Flanks** now lead only into packs where the measured approach either
  eases the fight or changes who you meet first:
  - in the crypt that is only the Censer Procession;
  - in the volcanic kit, the Pyrecaller's Guard or the Emberkin Raiders.
  - Did the side passage feel like a real alternative?

## Questions after each run

Answer these after each run:

1. **Different?** Did the routes feel meaningfully different from the
   previous run, or just different turns?
2. **A reason to explore?** Was there a recognizable reason to explore? What
   made you take, or skip, a side doorway?
3. **Worth it?** Did optional content feel worthwhile for the time and risk?
   Did you open the chest? Was the loot interesting?
4. **Companions.** Could the party follow the route you chose? Note anyone
   getting stuck, lagging, or running off another way.
5. **Dead ends.** Were there frustrating or pointless dead ends?
6. **Accidents.** Were any encounters accidentally avoided or triggered
   (something pulled that you didn't mean to)?
7. **Recognizable?** Did the dungeon still feel like the Hollow Reliquary:
   the same fights and bosses, recognizable halls?
8. **Coming back.** Would you enjoy returning for another run with a new
   seed?

And once, at the end:

9. **Clues.** Did the doorway clues work without the map? Did the glowing
   tablet or lamp read as "something is through here", and the dim candle as
   "a side passage"?
10. **Remix and seeds.** Did Classic / Remix / Seed in the hub make sense?
    Was the seed easy to note and reuse?
11. **The flank.** In Run C, did the flank change how the Censer Procession
    fight went for you?
12. **Vents (Emberforge).** Did timing the vents feel like a real choice
    against the long way round?

## Known limits to keep in mind

- **Flanks.** In the simulation, flanks help most against shielded casters
  (Emberforge's Pyrecaller's Guard). In the crypt they change who you engage
  first, but barely change damage for an AI party. Your experience as a
  player may differ: that is the point of asking.
- **Vents.** AI companions time the vents well, so whether the short way is
  dangerous depends mostly on you.
- **Hazards in the viewer.** The viewer does not draw the vents in 3D; they
  show in play.

## Notes

Human observations go here, with the date and build. Keep them separate
from automated results, which are in DUNGEON_REMIX_0_9.md and
DUNGEON_POLISH_0_9_1.md.

Status 2026-10-16: no human observations of 0.9 or 0.9.1 have been recorded.
The last recorded feedback is the owner's 0.8 notes (PLAYTEST_NOTES_0_8.md).
The headings below are where they go.

### Positive impressions
Pending.

### Problems observed
Pending (dungeon, seed, what happened, how to reproduce).

### Confusing features or interactions
Pending.

### Companion behaviour
Pending.

### Camera and controls
Pending.

### Exploration and navigation
Pending.

### Optional rewards and encounter pacing
Pending.

### Procedural variation and replayability
Pending. From the 0.8 notes: side exploration was interesting at first but
felt unnecessary after about five runs.

### Suggestions
Pending.
