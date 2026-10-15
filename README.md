# Dungeon Sandbox

An original Rust + Bevy 0.20 third-person dungeon RPG inspired by the
*mechanical feel* of classic tab-target MMORPG dungeons: one player, four
autonomous companions (tank / healer / DPS) with selectable AI personalities,
a WoW-Classic-style combat model (armor, hit tables, rage/energy/mana,
interrupts, crowd control), four data-driven dungeons, a navigation test
level and a 10-player raid with multi-phase bosses, layered navigation with
angled caves, ramps, bridges, drops and line of sight, levels, loot with rarities, and a sandbox for editing
encounters without recompiling. Classic MMO controls, clickable party/raid
frames with mouseover healing, threat-coloured nameplates and tooltips
generated from the real rules. All assets are primitive meshes; all names, maps and numbers are
original.

## Quick start

**Packaged build (Windows):** extract `DungeonSandbox-<version>-windows-x64.zip`
and double-click `dungeon-sandbox.exe`; `HOW_TO_PLAY.txt` inside explains the
game. Saves go to `saves\` next to the exe. Building the package:
[RELEASE_BUILD.md](RELEASE_BUILD.md).

**From source:**

```bash
cargo run --release
```

The first build compiles Bevy and takes several minutes. `cargo run` (debug
profile) also works; dependencies are optimised even in debug.

### Controls

| Where   | Input | Action |
|---------|-------|--------|
| Both    | W / S | move forward / back along your facing (moving cancels casts) |
| Both    | A / D | turn (strafe while the right button is held) |
| Both    | Q / E | strafe |
| Both    | right-drag | rotate the camera and steer the character |
| Both    | left-drag | free look (facing unchanged); a short click selects |
| Both    | both buttons | run forward |
| Both    | arrow keys, wheel | rotate/tilt camera, zoom |
| Both    | V | camera auto-follow on/off (saved) |
| Both    | hover | tooltips on buttons, stats, items, abilities, frames |
| Hub     | mouse | click the panel buttons (recruit, AI profile, equip, dungeon, difficulty...) |
| Hub     | 1-8 / P | switch panel tab / hide panel |
| Hub     | E (at portal) | enter the selected dungeon (re-reads all content files) |
| Hub     | F5 | reload content files |
| Dungeon | Tab / Shift+Tab / click | cycle enemies (engaged first) / reverse / pick any actor |
| Dungeon | click a party frame / F1-F5 | select a party member |
| Dungeon | click the target-of-target frame / T | select your target's target |
| Dungeon | G | target the party's kill target |
| Dungeon | 1-5 | class abilities; friendly spells go to the ally under the mouse (frame or character), else your selected ally, else you |
| Dungeon | F / Esc | start auto-attack / clear target |
| Dungeon | L | toggle verbose combat log |
| Dungeon | Backspace | leave the dungeon (counts as a failed run) |
| Dungeon | N / Shift+N | navigation debug overlay / plus walls and walk areas |
| Results | Enter | back to the hub |

In the dungeon, enemy nameplates are coloured by threat from your point of view
(tank: green secure / yellow contested / blue other tank / red lost; others:
yellow = close to pulling, red = attacking you). The action bar shows range,
cooldown (dark curtain), global cooldown (thin top bar) and resource shortfall
with the time until affordable.

Levels have height (0.4): ramps, stairs, bridges, stacked walkways and one-way
drops (orange strips at the edge). Abilities and auto attacks need line of
sight, so walls and floors block casts and heals ("no line of sight"), and an
enemy on another floor can't be hit or pulled through it.

### Classes

| Class | Role | Resource | 1 | 2 | 3 | 4 | 5 |
|---|---|---|---|---|---|---|---|
| Warden | Tank | rage | Shield Slam | Challenge (taunt) | Bulwark | Shield Bash (interrupt) | Quake Stomp (AoE threat) |
| Mender | Healer | mana | Mend | Renew | Smite | Ward (absorb) | Purify (dispel) |
| Striker | Melee | energy | Rend Strike | Whirl | Evade | Kick (interrupt) | Lacerate (bleed) |
| Arcanist | Ranged | mana | Arc Bolt | Flare | Veil | Hex (CC) | Silence Pulse (interrupt) |

The hub's **Classes** tab lists every ability with cast times and cooldowns.
RULES.md documents every formula.

## Dungeons

* **The Sunken Vault** (level 1+): skulker packs, a cleaving brute, and Morvane,
  the Hollow King — interruptible Hollow Bolts, then Cinder Rings at 50% HP.
* **The Gloomroot Warrens** (level 4+): healer-casters to interrupt, an optional
  spore grotto, Mother Brakkis (adds, stacking dispellable poison, frontal
  cleave), lava vents, and three-phase Vhorlag (party-wide pulses, adds, double
  rings, a stacking tank debuff).

* **The Ashen Spire** (level 8+): healing-reduction brands, armored sentinels,
  a fire-jet corridor, the Twin Altar council (two bosses at once), an optional
  Furnace Golem, and three-phase Ashmaw.
* **The Cinder Court** (raid, 10 players, level 10): gate guards, Twinmaw the
  Behemoth (tank swaps) and the Cinder Regent.

* **The Cinderdeep Hollows** (level 5+, 0.5): an original lava-cavern descent
  from a cold entrance to a molten heart: a fork into a low road and a high
  ridge (with a one-way drop), Kilnmaster Vorsk beside a slag pit, an optional
  Glass Matriarch, the Kiln Span bridge over a lava river, the Magma Cistern
  and Ignarax, the Molten Heart, on an island in a lava moat. Three optional
  fights, a return tunnel. See [DUNGEON_0_5.md](DUNGEON_0_5.md) and
  [PLAYTEST_0_5.md](PLAYTEST_0_5.md).

* **Cavern Traverse** (level 2+, 0.4): a graybox navigation proving ground with
  a winding passage, a ramp to a high walkway, a rope bridge over an underpass,
  a gallery stacked above it, a one-way drop into the Magma Basin, a narrow
  tunnel, a lava pit and a fire vent. See
  [PLAYTEST_NAVIGATION.md](PLAYTEST_NAVIGATION.md).

Each has Normal, Heroic and Raid (10) difficulties; the Party tab switches
between a 5-person party and a 10-person raid group. The Sandbox tab adjusts
enemy health, damage, ability speed and telegraph time without editing files;
the Editor tab tunes individual bosses and abilities and saves a custom dungeon
file. The Vendor sells gear for Vault Marks and buys back what you don't need.
Companions grow more familiar with dungeons they clear and react faster there. Bosses and elites drop loot; kills and
clears give XP (levels 1–10). Your companions level and gear up with you.

## Editing encounters (no recompile)

Every dungeon is a TOML file in [`assets/content/dungeons/`](assets/content/dungeons/):
layout, enemy templates (stats, abilities, phases, enrage), encounters (mobs,
hazards, optional), difficulties, loot and rewards. Change a boss timer, add a
phase, swap a mob, then press **F5** in the hub or just enter the portal.
Example variants live in [`assets/content/variants/`](assets/content/variants/)
(copy one into `dungeons/` to play it). Invalid files are rejected with every
problem listed by field, for example:

```
content file 'assets/content/dungeons/01_sunken_vault.toml' has 2 problem(s):
  - dungeon.hp_multiplier = -3 is out of range; use a value between 0.05 and 20.0 (1.0 = normal)
  - encounters[0].mobs[0].template 'skulkr' is not defined; known templates: brute, hollow_king, skulker
```

Enemy ability kinds: `bolt`, `cleave` (optionally `frontal`), `ring`, `pulse`,
`summon`, `debuff` (dot / damage_taken / healing_taken), `heal_ally`. See
RULES.md for every field.

Heights (0.4, all optional; old files load unchanged at height 0):

```toml
[[dungeon.areas]]
name = "Ash Ramp"
min = [14.0, 47.0]
max = [20.0, 61.0]
y = 0.0          # height (at the min edge for a ramp)
y_to = 5.0       # makes it a ramp; axis = "z" (default) or "x"; stairs = true draws steps

[[dungeon.areas]]
name = "Rope Bridge"
min = [19.0, 68.0]
max = [47.0, 72.0]
y = 5.0
under = "open"   # a deck you can walk under (default "solid" = rock down to the floor)
walls = false

[[dungeon.links]]  # one-way drop: zone straddles the upper area's edge
from = "Hanging Gallery"
to = "Magma Basin"
kind = "drop"
min = [40.0, 96.5]
max = [46.0, 99.5]
```

Caves (0.5): areas can be convex polygons and ramps can run in any
direction; lava pools/rivers can be polygons; scenery and lights are data:

```toml
[[dungeon.areas]]
name = "Glass Passage"
points = [[-14.6, 94.4], [-30.0, 96.7], [-29.3, 101.7], [-13.9, 99.4]]
y = -4.0
y_to = -1.0
slope_from = [-15.0, 97.0]   # height y here ...
slope_to = [-28.9, 99.1]     # ... and y_to here

[[dungeon.decor]]   # pillar | stalagmite | boulder | crystal; must not overlap walkable floor
kind = "stalagmite"
pos = [6.0, 20.0]
radius = 0.8
height = 3.0

[[dungeon.lights]]
pos = [8.0, 88.0]
y = -2.0
color = [1.0, 0.5, 0.2]
intensity = 1.2      # relative brightness (1.0 = normal)
range = 18.0
```

Areas connect only where they overlap **and** are level with each other.
Mob and hazard positions take an optional `y` (required when floors are
stacked there). The full rules are in RULES.md ("Navigation" and its 0.5
additions); the design is in [NAVIGATION_0_4.md](NAVIGATION_0_4.md) and
[DUNGEON_0_5.md](DUNGEON_0_5.md).

Environment overrides: `DUNGEON_CONTENT=<dir or file>`,
`DUNGEON_SAVE_DIR=<dir>` (default `./saves`).

## Headless simulation and AI evaluation

```bash
cargo run --release -p dungeon_sim --bin dungeon-headless -- --runs 20
cargo run --release -p dungeon_sim --bin dungeon-headless -- --dungeon gloomroot_warrens --level 4 --runs 100 --compare-profiles
cargo run --release -p dungeon_sim --bin dungeon-headless -- --dungeon gloomroot_warrens --level 4 --runs 50 --breakdown --quiet
cargo run --release -p dungeon_sim --bin dungeon-headless -- --party-size 25 --level 6 --dungeon gloomroot_warrens --runs 5 --bench --quiet
```

Options: `--dungeon ID`, `--content PATH`, `--difficulty ID`, `--class`,
`--level N` (default: the dungeon's recommended level), `--profile
conservative|balanced|aggressive|support|mechanic`, `--compare-profiles`,
`--breakdown` (damage by source, enemies turning on non-tanks, killing blows),
`--party-size N`, `--bench`, `--log FILE`, `--json`, `--quiet`. Summaries
include win rate, clear time, deaths, DPS, avoidable damage, interrupt rate,
threat failures, overheal and healing per mana; the first seed is replayed to
check determinism (exit code 3 on mismatch).

## Developer flags (graphical build)

```bash
cargo run -- --demo --dungeon gloomroot_warrens --level 6 --speed 4 --screenshots 3 --smoke-test 110
```

* `--demo` enters the portal and lets the AI play your slot (`--dungeon`,
  `--difficulty`, `--level` override the selection).
* `--speed X`, `--screenshots S` (to `screenshots/`), `--smoke-test N` (exit after N s).
* `--input-test` scripted keyboard sequence through the real input systems →
  `logs/input_test.txt`.
* `--hub-tour` screenshots every hub tab; `--click-test` clicks a hub button with
  synthetic mouse events → `logs/click_test.txt`; `--editor-test` edits and saves
  a custom dungeon through the editor (point `DUNGEON_CONTENT` at a scratch copy
  of `assets/content/dungeons`) → `logs/editor_test.txt`.
* `--frame-test` plays a Mender, hovers a party frame and presses 1 with an
  enemy selected (mouseover heal), then clicks another frame →
  `logs/frame_test.txt`; `--tooltip-test` hovers a stat and a vendor item →
  `logs/tooltip_test.txt`.
* `--class warden|mender|striker|arcanist`, `--group 10` and `--profile
  aggressive|conservative|balanced|support|mechanic` pick the player class, group
  size and every companion's AI profile for `--demo`. Threat alerts are also
  written to the log ("threat alert at ...").

* `--nav-debug` starts with the navigation overlay on; `--zoom D` sets an
  overview camera distance for screenshots, e.g.
  `--demo --dungeon cavern_traverse --nav-debug --zoom 30 --screenshots 3`.
* `--area-tour "Area A,Area B"` (or `all`) with `--dungeon ID`: points the
  camera at each named area in turn, saves `screenshots/area_NN_<name>.png`
  and exits.
* `--start-level N` raises the player and all companions to level N in the
  loaded save (never lowers), for playing higher-level dungeons in a playtest.

Manual playtest scenarios: [PLAYTEST_0_5.md](PLAYTEST_0_5.md) (Cinderdeep),
[PLAYTEST_0_3.md](PLAYTEST_0_3.md) (combat UX) and
[PLAYTEST_NAVIGATION.md](PLAYTEST_NAVIGATION.md) (0.4 navigation).

Set `DUNGEON_SAVE_DIR` to a temp folder for these runs so they don't touch your
real profile.

## Development checks

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release --workspace
```

`cargo test -p dungeon_sim --release diag_replay -- --ignored --nocapture`
replays one headless run and prints every actor's position, surface, goal and
waypoint (`DIAG_DUNGEON`, `DIAG_SEED`, `DIAG_LEVEL`, `DIAG_SIZE`,
`DIAG_TIME`; `DIAG_WATCH=<actor>` with `DIAG_N=<ticks>` traces one actor tick
by tick; `DIAG_ALL` includes idle enemies).

## Layout

```
crates/sim    dungeon_sim: stats & formulas, combat, auras, AI + blackboard,
              layered 2.5D navigation on convex polygons + line of sight,
              content schema, items, profile/progression, headless runner
crates/game   dungeon_game: Bevy app (hub, dungeon renderer, HUD, overlays, effects,
              results, camera, controls, tooltips, demo/test automation)
assets/content/dungeons/   the playable dungeons (TOML)
assets/content/variants/   example encounter variations
packaging/    files shipped in the Windows package (HOW_TO_PLAY.txt)
scripts/      package-windows.ps1
RULES.md      authoritative rules and formulas ([WoW] / [orig] tagged)
RESEARCH.md   references and design provenance
PROGRESS.md   verified status, benchmarks, known problems
TASKS.md      prioritized next work
CHANGELOG.md  meaningful changes
```
