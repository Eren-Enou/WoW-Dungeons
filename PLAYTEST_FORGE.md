# Playtest script — Dungeon Forge (0.6)

No human has played a generated dungeon yet. This session (about 45 minutes)
checks that a dungeon built by Dungeon Forge plays like a real dungeon, and
that the tools make sense to another developer.

## Part 1 — play the generated dungeon (25 min)

From the packaged build (`DungeonSandbox-0.6.0-windows-x64.zip`, see
RELEASE_BUILD.md) or from source, with a fresh save at the dungeon's level:

```powershell
$env:DUNGEON_SAVE_DIR = "$env:TEMP\forge-playtest"
.\dungeon-sandbox.exe --start-level 3
```

From source, run `cargo run --release -- --start-level 3` instead.

In the hub: **Party** → pick a class → *Auto-fill party*. Then **Dungeons** →
*The Emberforge Vaults (level 3+)* → *Normal*. Walk into the portal and press
**E**.

Note the time (top-right panel) and answer as you go:

| Moment | Question |
|---|---|
| First minute | Can you tell where to go? Does the entrance feel like a start? |
| Ramps and stairs | Do the slopes read as walkable? Any place you got caught on a corner? |
| The ledge (a small raised room near the start with an orange strip at its far end) | Did you notice it? Was it clear that dropping is one way? Was the drop useful? |
| The bridge over lava | Clear where the edges are? Did it feel like a crossing? |
| The fork (two ways into the same room) | Did you notice the alternate route? Which way did you take? |
| Pit room with the Forgewarden | Is the pit edge readable? Room to spread when sparks fall at 50%? |
| Side room (Obsidian Sentinel, optional) | Did you find it? Worth it? |
| The Crucible (Cindermaw) | Does the arena feel like the destination? Are the braziers in its corners visible? Could you see the rings? |

Afterwards:
* Clear time, deaths and wipes.
* Did any room feel repetitive or obviously "generated"? Which?
* Normal at level 3: too easy, right, or too hard?
* Companions: stuck, bunching, back-and-forth movement? Where? (**N** shows
  their paths.)

## Part 2 — a second layout (10 min)

The same blueprint with another seed is a different dungeon. Copy
`assets/content/variants/forge/emberforge_vaults_s6.toml` (or `_s8`) into
`assets/content/dungeons/` and press **F5** in the hub. It appears as *The
Emberforge Vaults (layout 6)*. Play the first few rooms.

* Does it feel like a different place, or the same rooms reshuffled?
* Which layout did you prefer, and why?

## Part 3 — the tools (10 min, needs Rust)

```bash
cargo run --release -p dungeon_forge -- generate assets/forge/blueprints/emberforge_vaults.toml --seed 21 --map forge_out/s21.svg
```

Open `forge_out/s21.svg` in a browser.

* Do the printed topology and the map agree? Is the map readable?
* Edit the blueprint: change `alternate_route` to `false`, `bridges` to `2`,
  or add another `[[encounters]]` with `recipe = "interrupt_pack"` before the
  last one. Generate again. Did the result do what you asked?
* Break it on purpose (for example `recipe = "dragon"`). Is the error message
  clear enough to fix it?
* Run a short simulation check:
  `cargo run --release -p dungeon_forge -- test assets/forge/blueprints/emberforge_vaults.toml --seed 21 --runs 5`.

## What to send back

Notes from all three parts, screenshots of anything confusing, the save's
`profile.json` (history of runs), and any generated `.toml`, `.svg` or
failure `.json` that showed a problem (the file header names the blueprint
and seed, which is enough to reproduce it).
