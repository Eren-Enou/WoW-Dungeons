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

**Run A: seed 19 (direct progression)**
- The fights come in the familiar order.
- There is a side passage into the Bone Archers' hall, a detour late in the
  dungeon to a nest and a coffer, an optional tomb, and a landmark chapel.
- In the simulation, going straight is the most efficient run.

**Run B: seed 4 (rewarding exploration)**
- Right at the start, beside the entrance hall's door, there is a doorway with
  a glowing tablet.
- Through it are a nest of crawlers and the Reliquary Coffer. The route
  rejoins at the first hall.

**Run C: seed 16 (tactical alternative)**
- Early on, a narrow side passage with a single dim candle leads into the
  Warden's Screen hall from the side.
- Coming in that way puts you nearer the healers behind the shield-bearer.
- The direct door faces the shield-bearer.

Don't read the run descriptions above before each run if you want to test
whether the clues work on their own. Each one names its seed first, so you
can cover the rest.

## The Emberforge run (optional, 10 minutes)

**Emberforge Vaults, seed 2:**
- A corridor near the start pulses with magma vents; a longer tunnel goes
  round it.
- A crawlway leads into the Pyrecaller's hall from the side.
- A detour holds the Obsidian Sentinel and a strongbox.
- A ledge from the Anvil Court drops you past three empty caverns.
- The detour is late, off the Pouring Floor.

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
11. **The flank.** In Run C, did the flank change how the Warden's Screen
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

(Human observations go here, with the date and build. Keep them separate
from automated results; DUNGEON_REMIX_0_9.md has those.)
