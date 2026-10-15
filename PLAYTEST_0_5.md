# Playtest script — 0.5, The Cinderdeep Hollows

A structured session for 0.5. (Correction, 0.7: earlier versions have been playtested by the maintainer; no written notes are in the repository.) It takes
about 45–60 minutes. Everything in DUNGEON_0_5.md "Verification" was checked
by automated runs only.

## 1. Setup (5 min)

Use the packaged build if you have it (this also tests the package):

1. Extract `DungeonSandbox-0.5.0-windows-x64.zip` to a new folder, e.g. your
   Desktop. If SmartScreen warns, *More info → Run anyway*.
2. Open PowerShell in that folder (Shift+right-click → "Open PowerShell window
   here") and start with a fresh save at level 5:

```powershell
$env:DUNGEON_SAVE_DIR = "$env:TEMP\cinderdeep-playtest"
.\dungeon-sandbox.exe --start-level 5
```

From source instead (repository root):

```powershell
$env:DUNGEON_SAVE_DIR = "$env:TEMP\cinderdeep-playtest"
cargo run --release -- --start-level 5
```

`--start-level 5` raises you and all companions to level 5 (the dungeon's
recommended level) in that save; it never lowers levels. Skip it to test a
fresh level-1 character instead (expect to lose in Cinderdeep).

3. In the hub: **Party** tab → pick your class with *Change class* (first run:
   whatever you play best) → *Auto-fill party*. **Dungeons** tab → *The
   Cinderdeep Hollows*, *Normal*. Walk into the portal and press **E**.

Keep a timer or note the `time` value in the top-right panel.

## 2. First clear — explore freely (15–25 min)

Play as you naturally would. Don't use the navigation overlay yet. Note
answers as you go:

| Where | What to look for |
|---|---|
| Frostvent Gate | Does the entrance feel cold and distinct? Is it obvious where to go? |
| Glowworm Fork | Did you notice there are two ways? Which did you take and why? |
| Ashgrub Run (low road) | Optional den: two brutes guard a pyromancer. Could you reach and interrupt the caster? |
| Ridge Path (high road) | Two hounds with a frontal bite on a narrow ledge. Was the ledge edge clear? At the end, the orange strip marks a **one-way drop** into the Smelter — was that clear before you jumped? |
| Smelter Gallery | Kilnmaster Vorsk by the slag pit. At 50% slag spills (rings) and slaglings arrive. Was the pit edge readable? Did anything look walkable that isn't (or the reverse)? |
| Obsidian Grotto (optional, up the Glass Passage) | Did you find it? Shard Storm rings and Glass Nova (interrupt). |
| Kiln Span | The long bridge over the lava river: shieldbearers with frontal slams protect a Flamecaller. Did the fight feel cramped in a good or bad way? Could you tell where the bridge ends? |
| Rim Tunnel | Did you notice it leads back to the Smelter? Useful or confusing? |
| Magma Cistern | Lava chamber with two causeways to the Basalt Island pack and timed fire vents. Which side did you approach from? Were the vents readable? |
| Heart Island | Ignarax: frontal lash, Slag Bolt (interrupt), at 60% eruptions + Magma Spawn, at 25% Heart Pulse. Did the arena feel like a destination? Could you see the eruption rings on the lava-coloured floor? |

Overall, after the clear:

* Total time, number of deaths, wipes.
* Did it feel like one place descending deeper, or a series of rooms?
* Moments you were lost, or unsure where you could walk.
* Camera: any spot where walls, the ridge or the boss blocked your view?
  (Known: Ignarax is big up close — is it a problem?)
* Difficulty on Normal at level 5: too easy / right / too hard? (The AI party
  alone almost never dies here, so human feedback matters.)

## 3. Companion behaviour (during both runs)

Watch your companions, especially in narrow places (ramps, the Kiln Span, the
causeways):

* Do they follow without bunching into one spot or blocking you?
* Does anyone walk back and forth repeatedly ("dithering")? Where?
* Does anyone get stuck, or stay behind after the ridge drop?
* Does the healer keep you in sight, or lose you behind corners?
* Do ranged companions pick sensible spots (not across lava, not pulling the
  next pack)?
* Do melee companions stand in front of cleaving enemies?

If something looks wrong, press **N** (Shift+N for more): it draws every
companion's path, waypoint and goal, unreachable goals and stuck actors. Take
a screenshot (Win+Shift+S) with the overlay on and note the area name.

## 4. Second run (10–15 min)

Back in the hub, either:

* take the **other road** at the Glowworm Fork and skip the optional fights, or
* switch to **Heroic** (Dungeons tab), or
* **Raid (10)** on the Party tab with the *Raid (10)* difficulty.

Compare: was the other road worth it? Does the dungeon hold up on a second
visit?

## 5. Package check (2 min, packaged build only)

* Did the game start by double-click without a console error?
* Close the game. Without `DUNGEON_SAVE_DIR` set, start it again by
  double-click: a `saves` folder should appear next to the exe after your
  first run or a setting change (e.g. press V in the hub).

## 6. What to send back

* Your notes from sections 2–5.
* Screenshots of anything confusing.
* The save file `%TEMP%\cinderdeep-playtest\profile.json`. Its `history`
  holds the outcome, time and deaths of every run.
* Hardware (GPU) and whether the frame rate felt smooth.

## Known issues (don't report these again)

* Two melee companions heading for the same spot can overlap.
* Some back-and-forth movement remains, mostly around the Cistern pack.
* Decor (pillars, crystals) does not block line of sight.
* The build is unsigned, so SmartScreen warns.
