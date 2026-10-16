# Playtest script — Dungeon Viewer and modular environments (0.8)

A structured session of about 40 minutes for a person.

* **Automated already**: everything in PROGRESS.md "0.8"; the viewer's own
  `--viewer-test`; screenshot tours.
* **What this session is for**: whether the tool is *pleasant and clear* to
  use, and whether the new environments *look and play right* to a person.

Write your notes into the repository (for example `PLAYTEST_NOTES_0_8.md`).
Earlier versions were played, but no notes were kept.

## Setup

From the packaged build (`DungeonSandbox-0.8.0-windows-x64.zip`, extracted):

```powershell
$env:DUNGEON_SAVE_DIR = "$env:TEMP\viewer-playtest"
.\dungeon-sandbox.exe --start-level 5
```

## Part 1 — open and find your way (10 min)

1. In the hub: **Dungeons** → select *The Hollow Reliquary* → **Inspect in
   viewer (V)**.
2. Without reading DUNGEON_VIEWER_GUIDE.md, try to:
   * fly into the first hall;
   * look at a pillar up close;
   * go back to the entrance;
   * jump to the boss arena.

| Question | Notes |
|---|---|
| Was the on-screen help enough? Which control did you look for and not find? | |
| Did mouse-look (hold right button) feel right? Too fast or slow? | |
| Did Tab / L get you where you wanted? Did rooms come in a sensible order? | |
| Is the info panel useful, or in the way? (H hides it) | |

## Part 2 — inspect (10 min)

1. Press 1, 2, 3 and 4 one at a time.
2. Click a wall, a pillar, a sconce anchor (with 4 on) and a mob (with 3
   on).

| Question | Notes |
|---|---|
| Could you tell why an area is or isn't walkable (layer 1)? | |
| Does the structure layer (2) match the rooms you see? | |
| Do the encounter markers (3) make it clear where fights happen and how far aggro reaches? | |
| Does the inspector tell you what a piece is and where it came from? What is missing? | |
| G (walk at eye height): do rooms feel the right size from a player's view? | |

## Part 3 — compare layouts (8 min)

1. Press ] twice and [ once.
2. Fly through each layout.
3. Press F12 in rooms you like and dislike.

| Question | Notes |
|---|---|
| Do the walls, arches and pillars fit every layout, or did you see gaps, overlaps, walls across doorways, floating decoration? Where (F12 + room name)? | |
| Is regenerating with ] fast enough to compare seeds? | |

## Part 4 — play the new look (12 min)

1. Backspace to the hub, auto-fill a party, and play the Hollow Reliquary on
   Normal.
2. Then play the Emberforge Vaults, at least to the second encounter.

| Question | Notes |
|---|---|
| Do the crypt and the forge look like different places now? | |
| Do the pillars read as cover? Did the Guardian's wail make sense with them? | |
| Are telegraphs (rings, cones) still easy to read under the new lighting and fog? | |
| Did the camera get stuck on, or hidden behind, any new wall pieces, arches or pillars? | |
| Did the light and haze changing between rooms feel natural, or distracting? | |
| Performance: any stutter? | |

## What to send back

* The notes above.
* Screenshots: F12 in the viewer, or the game's own.
* The dungeon file and seed of anything wrong. The viewer's info panel shows
  the file, blueprint and seed.
