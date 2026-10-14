# Progress

Status: **M0–M5 implemented.** The full hub → recruit → dungeon → boss → rewards →
hub loop runs in the graphical build and in the headless runner. Remaining items
need human playtesting (see bottom).

Environment used for verification: Windows 11 Pro, Rust 1.99.0, Bevy 0.20.0
(pinned `=0.20.0`, Cargo.lock committed), NVIDIA RTX 2070 SUPER (Vulkan).

## Verified (commands actually run, results observed)

### Checks (latest run, 2026-10-14)
- `cargo fmt --all -- --check` — clean.
- `cargo check --workspace` — clean.
- `cargo test --workspace` — **47 passed, 0 failed** (all in `dungeon_sim`).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.

### M0 — Bootstrap
- Workspace with `crates/sim` (rules, no Bevy dependency) and `crates/game` (Bevy 0.20 app); `cargo run` launches the game.
- Graphical launch verified: `cargo run -p dungeon_game -- --smoke-test 8` opened the window and ran 465 frames in the hub with no errors.
- README with commands, controls and layout.

### M1 — Combat core (unit tests in `crates/sim/src/tests.rs`)
- Health bounds (damage capped, death at 0, no damage to the dead), heal cap + overheal.
- Threat: class multiplier, 110% melee / 130% ranged switch rule, taunt, threat drop, healing threat split across engaged enemies, no threat for out-of-combat heals, dead actors removed from tables.
- Targeting: no target / hostile-vs-friendly / out of range / dead target; friendly spells fall back to self.
- Cooldowns and 1 s GCD; cast times complete on time; moving interrupts the player's cast; mana gating.
- Event log (`EventKind`) with readable formatting; per-actor metrics (damage, healing, overheal, taken, deaths, casts).

### M2 — AI party
- Tank taunts an enemy attacking the healer; healer Mends the lowest ally; AI dodges noticed telegraphs and respects 0.5 s reaction time; followers keep up with a moving leader (tests).
- Headless 5-character runs: default content, every player class, seeds 1–3 → all **Victory** (test `default_content_is_clearable_by_ai_party`).
- Batch: `dungeon-headless --runs 10 --class {striker,warden,mender,arcanist}` → 10/10 victories each, deterministic replay OK.

### M3 — Instance and boss
- Dungeon with 3 trash encounters (incl. an elite with cleave) + two-phase boss, defined in `assets/content/dungeon.toml`.
- Phase 2 fires exactly once per pull at the 50% threshold, telegraph appears 2 s later (test); ring damages only those inside (test); `ring_interval` from config controls spacing to ±0.1 s (test).
- Wipe → boss resets to full HP, phase 1, idle at home, threat cleared, telegraphs removed; re-pull triggers phase 2 again once (test). Leash reset when dragged beyond `leash_radius` (test). Proximity aggro pulls the whole pack only (test).
- Visible tell verified in screenshot: boss glows orange and grows in phase 2, ring telegraph drawn on the floor, phase banner.

### M4 — Hub and progression
- Hub scene (plaza, portal, board, companions), party panel: class change, recruit/dismiss (1–8), auto-fill, equip from stash.
- Graphical full loop verified with `--demo --speed 4 --screenshots 2.5 --smoke-test 60` (save dir redirected to a temp folder): hub → portal → dungeon → **Victory** (151 s sim) → results screen (rewards, per-member table, key moments, log path) → hub.
- **Reward survives restart:** relaunched the game against the same save dir; hub showed Vault Marks 10, Runs won 1, companions' run counts 1, and *Silted Chainmail* in the stash (screenshot). Also covered by test `reward_survives_save_and_reload`. Corrupt saves are moved aside, not deleted (test).

### M5 — Sandbox adjustments and QA
- Editable `hp_multiplier`, `damage_multiplier`, boss `ring_interval` (boss timer), `ring_telegraph`, enrage, templates, encounters, loot.
- Config affects subsequent runs: 20-seed release batches — default: mean 138.2 s, ring hits 0.45; `hp_multiplier 1.5` + `ring_interval 5` → mean 197.0 s, ring hits 0.90. Test: `hp_multiplier = 2.0` doubles boss HP and lengthens the run; `hp_multiplier = 10` ends in a non-victory (no hang).
- Invalid config: headless exits 1 listing every problem with its field path (bad multiplier, telegraph ≥ interval, unknown template with list of known ones); typo'd field names rejected with line/column and expected names. In-game, the hub shows the same message and the portal refuses entry (screenshot). Unit tests for these cases.
- Seeded batch runner prints per-run and summary metrics and verifies determinism by replaying the first seed (exit code 3 on mismatch); test `headless_runs_are_reproducible_for_a_seed`.

## Not verified / requires human playtesting
- **Manual play feel**: WASD + camera, Tab/click targeting, ability use and dodging rings were exercised only through the sim's player-input API (test `player_of_every_class_can_move_target_and_use_three_abilities`) and the AI-driven `--demo`. A human has not yet played a run with keyboard and mouse.
- Mouse-click picking (ray vs. sphere) is untested in a live session.
- Difficulty for a human player is untuned; the AI party wins 100% on default content.
- Healer player experience (F1–F5 targeting) not playtested.
- Only Windows/Vulkan tested; other OSes not tried.
- Screenshots are produced only via `--screenshots`; none are committed.

### Manual smoke test (≈5 minutes)
1. `cargo run --release`. The hub appears; party panel on the right.
2. Press **C** until you are a Striker (or any class). Walk with **WASD**, rotate the camera with right-drag.
3. Walk into the purple portal and press **E**.
4. Walk north. Press **Tab** to target a skulker, press **1**, **2**, **3**; companions engage, the tank taunts.
5. Clear the three packs, enter the throne room. At 50% the boss glows orange and a banner appears; step out of red rings.
6. On victory/wipe, read the results screen, press **Enter**. Check Vault Marks and the stash.
7. Quit, relaunch: the marks and item are still there. Press **G** to equip the item.
8. Edit `assets/content/dungeon.toml`: set `hp_multiplier = 2.0`, `ring_interval = 5.0`, press **F5** in the hub: the info panel shows the new values; enter and observe the longer fight / more rings. Set `hp_multiplier = -1`, press **F5**: an error lists the field.

## Decisions
- Rust stable 1.99, Bevy pinned `=0.20.0`; classic `commands.spawn` API (BSN macros not used).
- Simulation is a separate crate with no Bevy dependency, fixed 20 Hz timestep, SplitMix64 RNG; the game interpolates positions for rendering.
- Original classes: Warden (tank), Mender (healer), Striker (melee), Arcanist (ranged). Formulas and numbers are placeholders (see RULES.md).
- Content in TOML with `deny_unknown_fields` and a validation pass that collects all issues.
- Navigation: overlapping rectangles + BFS over the rectangle graph (documented shortcut; no actor collision, no line-of-sight blocking).
- Persistence: JSON profile written atomically (temp file + rename); `DUNGEON_SAVE_DIR` override.
- Party: 8-companion roster; recruit 4; auto-fill gives one tank, one healer and DPS around the player's role. Party without tank/healer is allowed but warned.
- Leaving the dungeon (Backspace) counts as a failed run.
- UI is keyboard-driven text panels + Bevy UI bars (the old `Button`/`Interaction` widgets are deprecated in 0.20).

## Next actions (beyond M5)
- Human playtest pass; tune boss numbers for a human-controlled slot.
- Camera collision with walls; name plates; floating combat text.
- Out-of-combat resurrection so a single death doesn't persist for the whole run.
- Optional: study AzerothCore/Playerbots behaviour and log it in RESEARCH.md.

## Blockers
- None.
