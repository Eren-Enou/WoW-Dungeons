# Dungeon Sandbox

An original Rust + Bevy 0.20 third-person dungeon prototype inspired by the
*mechanical feel* of classic tab-target MMORPG dungeons: one player, four
autonomous companions (tank / healer / DPS), threat, a two-phase boss with a
telegraphed ground attack, loot, and persistent progression. All assets are
primitive meshes; all names, maps and numbers are original.

## Quick start

```bash
cargo run --release
```

The first build compiles Bevy and takes several minutes. `cargo run` (debug
profile) also works; dependencies are optimised even in debug.

### Controls

| Where   | Key | Action |
|---------|-----|--------|
| Hub     | WASD | move (relative to camera) |
| Hub     | E (at portal) | enter the dungeon (re-reads the content file) |
| Hub     | P | show / hide party panel |
| Hub     | C | change your class (party is re-balanced) |
| Hub     | 1-8 | recruit / dismiss companion |
| Hub     | R | auto-fill a balanced party for your role |
| Hub     | [ ] or Tab | select stash item |
| Hub     | G / Shift+1-8 | equip selected item on yourself / companion N |
| Hub     | F5 | reload content file |
| Dungeon | WASD | move (moving cancels casts) |
| Dungeon | Tab / left click | target nearest enemy / click any actor |
| Dungeon | F1-F5 | target party member (healers) |
| Dungeon | 1, 2, 3 | class abilities |
| Dungeon | F | start auto-attack |
| Dungeon | Esc | clear target |
| Dungeon | L | toggle verbose combat log |
| Dungeon | Backspace | leave the dungeon (counts as a failed run) |
| Both    | Right mouse drag / arrow keys | rotate camera; wheel zooms |
| Results | Enter | back to the hub |

### Classes

| Class | Role | 1 | 2 | 3 |
|-------|------|---|---|---|
| Warden | Tank | Shield Slam (4x threat) | Challenge (taunt) | Bulwark (-40% damage) |
| Mender | Healer | Mend (1.5s big heal) | Renew (HoT) | Smite (1.5s damage) |
| Striker | Melee DPS | Rend Strike | Whirl (AoE) | Evade (drop 50% threat) |
| Arcanist | Ranged DPS | Arc Bolt (2s cast) | Flare (instant) | Veil (drop 50% threat) |

Friendly spells (Mend, Renew) target your selected ally, or yourself if none.

## The dungeon: The Sunken Vault

Three trash encounters (a skulker pack, an elite Cragbound Brute with a cleave
and two adds, a four-skulker pack) and **Morvane, the Hollow King**:

* **Phase 1:** heavy melee on the tank, *Hollow Bolt* (cast) on a random non-tank.
* **Phase 2 (at 50% HP, once per pull):** the boss glows and grows, a banner
  appears, damage rises, and every `ring_interval` seconds a **Cinder Ring**
  is drawn on the ground under a random party member. It detonates after
  `ring_telegraph` seconds for heavy damage to everyone inside — step out.
* Hard enrage after `enrage_seconds`.

Victory grants Vault Marks and one rolled item, saved to `saves/profile.json`.

## Editing encounters (no recompile)

Everything about the dungeon is in [`assets/content/dungeon.toml`](assets/content/dungeon.toml):
enemy HP multiplier, damage multiplier, enemy templates, encounter layouts,
the boss timers, loot table and rewards. The game re-reads it every time you
enter the portal (or press F5 in the hub). Invalid files are rejected with a
list of problems naming each field, e.g.:

```
content file 'assets/content/dungeon.toml' has 2 problem(s):
  - dungeon.hp_multiplier = -3 is out of range; use a value between 0.05 and 20.0 (1.0 = normal)
  - encounters[0].mobs[0].template 'skulkr' is not defined; known templates: brute, hollow_king, skulker
```

Environment overrides: `DUNGEON_CONTENT=<path>` (content file),
`DUNGEON_SAVE_DIR=<dir>` (save location, default `./saves`).

## Headless simulation

The rules live in the renderer-agnostic `dungeon_sim` crate. A seeded batch
runner plays the whole dungeon with an all-AI party:

```bash
cargo run --release -p dungeon_sim --bin dungeon-headless -- --runs 20 --seed 1
```

Options: `--class warden|mender|striker|arcanist` (the "player" slot's class),
`--content PATH`, `--max-seconds S`, `--log FILE` (full combat log of the first
run), `--json` (one JSON summary per run). It prints per-run outcome, duration,
encounters cleared, resets, deaths, boss phase-2 time, ring casts/hits, per-member
DPS/HPS, a hash of the event log, and a summary — and re-runs the first seed to
check the result is bit-for-bit reproducible (exit code 3 if not).

## Developer flags (graphical build)

```bash
cargo run -- --demo --speed 4 --screenshots 2.5 --smoke-test 60
```

* `--demo` enters the portal automatically and lets the AI play your slot.
* `--speed X` speeds up the simulation (demo only).
* `--screenshots S` saves `screenshots/shot_NNN.png` every S seconds.
* `--smoke-test N` exits after N seconds.

Tip: set `DUNGEON_SAVE_DIR` to a temp folder for demo runs so they don't touch
your real profile.

## Development checks

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Layout

```
crates/sim    dungeon_sim: combat rules, AI, nav, content schema, profile, headless runner
crates/game   dungeon_game: Bevy app (hub, dungeon renderer, HUD, results, camera)
assets/content/dungeon.toml   the editable dungeon
RULES.md      combat and AI rules in prose
RESEARCH.md   what was consulted and what was invented
PROGRESS.md   verified status per milestone
```
