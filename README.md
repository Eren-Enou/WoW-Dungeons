# Dungeon Sandbox

An original Rust + Bevy 0.20 third-person dungeon RPG inspired by the
*mechanical feel* of classic tab-target MMORPG dungeons: one player, four
autonomous companions (tank / healer / DPS) with selectable AI personalities,
a WoW-Classic-style combat model (armor, hit tables, rage/energy/mana,
interrupts, crowd control), three data-driven dungeons and a 10-player raid
with multi-phase bosses, levels, loot with rarities, and a sandbox for editing
encounters without recompiling. Classic MMO controls, clickable party/raid
frames with mouseover healing, threat-coloured nameplates and tooltips
generated from the real rules. All assets are primitive meshes; all names, maps and numbers are
original.

## Quick start

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
| Results | Enter | back to the hub |

In the dungeon, enemy nameplates are coloured by threat from your point of view
(tank: green secure / yellow contested / blue other tank / red lost; others:
yellow = close to pulling, red = attacking you). The action bar shows range,
cooldown (dark curtain), global cooldown (thin top bar) and resource shortfall
with the time until affordable.

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

Manual playtest scenarios for 0.3: [PLAYTEST_0_3.md](PLAYTEST_0_3.md).

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

## Layout

```
crates/sim    dungeon_sim: stats & formulas, combat, auras, AI + blackboard, nav,
              content schema, items, profile/progression, headless runner
crates/game   dungeon_game: Bevy app (hub, dungeon renderer, HUD, overlays, effects,
              results, camera, controls, tooltips, demo/test automation)
assets/content/dungeons/   the playable dungeons (TOML)
assets/content/variants/   example encounter variations
RULES.md      authoritative rules and formulas ([WoW] / [orig] tagged)
RESEARCH.md   references and design provenance
PROGRESS.md   verified status, benchmarks, known problems
TASKS.md      prioritized next work
CHANGELOG.md  meaningful changes
```
