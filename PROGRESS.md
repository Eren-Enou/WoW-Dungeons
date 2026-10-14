# Progress

Status: **0.2 in development — Phases A–G implemented and verified headlessly
and through automated graphical runs.** No human has played 0.2 yet.

Environment: Windows 11 Pro, Rust 1.99.0, Bevy 0.20.0 (pinned `=0.20.0`,
Cargo.lock committed), NVIDIA RTX 2070 SUPER (Vulkan).

## Starting condition of 0.2 (recorded 2026-10-14, commit b8e3fc2)

fmt/clippy clean, 50/50 tests. Prototype limits: 3 abilities per class, one
dungeon with a hard-coded boss script, flat damage (no mitigation/avoidance),
mana only, simple auras, no levels, 2 item slots, keyboard-only text hub, 3D
health bars, no camera collision, AI without coordination.

## Verified (commands actually run, results observed)

### Checks (latest)
- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo test --workspace --release` — **110 passed** in `dungeon_sim` + 2 in
  `dungeon_game` (camera ray tests), 0 failed.
- `cargo build --release --workspace` — succeeded; the release binary played a
  full `--demo` loop (hub → Sunken Vault → Victory in 163 s sim → results → hub).

### Phase A — playable experience
- Graphical runs via `--demo` (AI drives the player slot) on both dungeons with
  screenshots: nameplates with cast bars and debuffs, floating combat text,
  school-coloured effects, target ring, translucent occluding walls, hazard
  discs, frontal-cleave cones, phase tints, summon/phase/ring banners, five-slot
  action bar with resource costs, resource-coloured party bars, victory banner,
  results screen with loot/XP/level-ups.
- `--hub-tour`: screenshots of every hub tab (Party, Character for the player and
  a companion with upgrade comparisons, Dungeons with difficulty, Classes,
  History); switched the selected dungeon through the button action function.
- `--click-test`: synthetic mouse move/press/release through Bevy's picking
  backend on the "History" tab button → `Activate` observer → tab switched
  ("CLICK TEST OK").
- `--input-test`: scripted `ButtonInput` key presses through the game's own input
  systems: moved 33.7 m, Tab-targeted, 3 abilities landed, pack engaged.
- Unit tests for camera ray/box collision.

### Phase B — combat (unit tests in `stats.rs`, `items.rs`, `tests.rs`)
Armor formula and cap; resist curve; melee miss vs level gap and hit; spell miss
table; single-roll table order; parry/block only from the front (statistical
test); spells resisted but never dodged; physical vs bleed vs magic mitigation;
rage from dealt/taken and decay; energy regen; five-second rule; cooldowns and
GCD; cast completion timing; player movement cancels casts; interrupts and
silence (only interruptible casts); silence blocks spells; Hex incapacitates,
stops movement, breaks on damage, bosses immune; Lacerate ticks 12× ignoring
armor; Ward absorbs and Weakened Soul blocks re-casting; Purify removes only
dispellable debuffs; stacking cap and damage-taken multiplier; threat rules
(class multiplier, 110/130%, taunt, healing threat split, OOC healing, threat
drop, dead actors removed).

### Phase C — AI (tests + 100-run evaluations)
- Tests: tank taunts enemy on healer; healer mends lowest; dispel priority by
  profile; telegraph dodge with reaction time (Mechanics profile faster);
  blackboard assigns one interrupter and prioritizes healer-casters; Arcanist
  hexes in 4-mob pulls; AI interrupts most Hollow Bolts; followers follow;
  non-tanks leave frontal cones; healers hold top-up heals for 4 s after a pull;
  Aggressive vs Conservative profiles differ in threat failures and clear time.
- Profile comparison (`--compare-profiles`, 50 runs, Warrens, level 4, after
  all AI fixes below):

  | profile | wins | mean time | deaths | avoidable dmg | interrupts | threat fails |
  |---|---|---|---|---|---|---|
  | Conservative | 100% | 371 s | 0.02 | 331 | 89% | 2.8 |
  | Balanced | 98% | 356 s | 0.10 | 621 | 85% | 6.5 |
  | Aggressive | 46% | 330 s | 2.86 | 1774 | 56% | 7.2 |
  | Support | 100% | 360 s | 0.18 | 689 | 91% | 7.4 |
  | Mechanics | 100% | 360 s | 0.04 | 550 | 94% | 7.8 |

- Summoned adds inheriting 50% of the summoner's threat cut threat failures in
  the Warrens from 19 to 6.7 per run (Balanced, 100 runs; spiderlings no longer
  appear in the breakdown).
- Learning (`--familiarity`, Warrens L4, 100 runs each): Balanced 99% → 100%
  wins, deaths 0.12 → 0.04, ring hits 0.26 → 0; Aggressive 59% → 82% wins,
  deaths 2.32 → 1.12, avoidable damage 1669 → 655, interrupts 58% → 85%.
  Tests: familiarity/alertness sharpen parameters; victories build familiarity
  per dungeon.

- Evidence-driven fixes (`--breakdown`): non-tank frontal-cleave hits fell from
  12.5 to 3.9 per Warrens run, avoidable damage 2,500 → 770 per run,
  deaths 0.29 → 0.14 (Balanced, 100 runs).

### Phase D/F — content
- Schema 2.0 validation tests (bad multiplier, unknown template with known list,
  typo'd field, ring delay ≥ interval, phase thresholds); the library loads both
  shipped dungeons and both example variants.
- Encounter tests: phase 2 once per pull at 50% and ring 2 s later; ring timing
  follows `interval`; rings hit only those inside; Brakkis summons adds which
  despawn on reset; frontal cleave spares those behind; shaman heals injured
  allies; hazards pulse on/off; optional encounters not required; heroic raises
  HP and level; boss reset after wipe and phase re-trigger; leash; pack aggro.
- Difficulty (Balanced AI, headless): Sunken Vault normal L1 100% (100 runs);
  heroic L3 50% (20 runs, before the AI fixes); Warrens normal L4 96–98%, L5–6
  100%; tests assert the Vault is mostly clearable at L1 for every class and
  the Warrens at L6.
- Graphical: the Warrens played to Victory in `--demo --level 6` (279 s sim).
- Third dungeon, the Ashen Spire (40 runs each): Normal L8 98% wins, L10 100%;
  Heroic L10 25% (endgame challenge). Test asserts 2/3 wins at L10 and that the
  Twin Altar has two bosses. Graphical `--demo --dungeon ashen_spire --level 10`
  played to Victory (403 s sim).
- Sandbox overrides: test verifies HP scaling, ability interval and telegraph
  scaling with invariants kept, and the reward factor; the Sandbox tab rendered
  in `--hub-tour`.
- Encounter editor: tests for clamped edits staying valid, custom id/name, and a
  stable TOML round trip of both shipped dungeons. `--editor-test` (against a
  scratch copy of the content directory) edited Morvane's Hollow Bolt interval
  8 → 5 s and HP +10%, saved `90_sunken_vault_custom.toml`, reloaded with no
  errors and selected it; the headless runner then cleared it (5/5). The Editor
  tab rendered in `--hub-tour`.
- Vendor: test for unlocked stock by level, buying with marks and salvage
  refunds; the Vendor tab rendered in `--hub-tour`.
- Cone routing (Warrens L4, 100 runs): non-tank Root Lash hits 1.6 → 0.6 and
  Rending Mandibles 2.3 → 1.4 per run; avoidable damage 618 → 538, 97–99% wins;
  Ashen Spire L8 deaths 0.12 → 0.05 per run, 100% wins (40 runs). Test asserts
  paths crossing a cone are detoured and clear paths are left alone.

### Phase E — progression (tests in `profile.rs`)
Reward survives save/reload; one item per cleared loot source; wipes keep kill
XP but no clear loot; XP levelling and cap; heroic rarity upgrade and marks
multiplier; equip/swap and auto-equip respecting role affinity; v1 save
migration; corrupt save backup. Graphically: after a Warrens run the hub showed
level 7, 20 marks, loot in the stash with comparisons.

### Phase G — raids
- In-game 10-person raid: `--demo --group 10 --difficulty raid10` on the Warrens
  rendered compact two-column party frames and ran to Victory (359 s sim).
  Raid (10) headless (30 runs): Warrens L6 97% wins / 0.83 deaths; Vault L3 100%.
  Test: raid auto-fill gives 2 tanks + 2 healers for every player class.

#### Headless scaling
`--party-size` builds groups with ~1 tank per 10 and 1 healer per 4. All sizes
clear the Warrens at level 6. Benchmarks (release, `--bench`):

| party | normal content | stress (enemy HP ×8, sustained combat) |
|---|---|---|
| 5 | 0.002 ms/tick (~400k ticks/s) | 0.003 ms/tick |
| 10 | 0.004 ms/tick | 0.004 ms/tick |
| 25 | 0.011 ms/tick | 0.012 ms/tick (~4,200× real time) |
| 40 | — | 0.022 ms/tick (~2,300× real time) |

Bottleneck candidates: per-call `engaged_enemies()` allocations and O(party ×
enemies) AI scans; not a problem at these sizes.

## Not verified / requires human playtesting
- Feel of the controls and camera with real keyboard/mouse; readability of the
  new effects for a human; difficulty when a human controls the player slot.
- Healer and tank player experience with the new abilities (Ward, Purify,
  Shield Bash, Quake Stomp) in the graphical game.
- Hub buttons other than the History tab were exercised through the action
  function, not real clicks (the click path itself is verified).
- 25+ player groups are headless only (the game offers 5 and 10).
- Only Windows/Vulkan tested.

## Known problems
- Remaining threat failures (~6.5 per Warrens run) come mostly from
  healer-caster packs at the pull.
- The Sunken Vault is easy for the AI party at level 1 (100% wins); the human
  player slot is expected to be the variable. Heroic provides a challenge.
- Frontal cleaves still clip non-tanks ~4 times per Warrens run (moving through
  the cone; no path planning around cones).

## Decisions (0.2)
- Formulas are tagged [WoW]/[WoW-approx]/[orig] in RULES.md; the resist formula
  deviates from WoW on purpose (too steep at levels 1–6).
- Content is a directory of dungeon TOML files; schema 2.0 replaced the hard-coded
  boss script (the Vault was ported; old single-file content is not loaded).
- Labels are `Arc<str>` because enemy ability names come from data.
- The blackboard is recomputed every tick from state (no hidden memory), keeping
  AI deterministic and testable; companions still decide individually.
- Hub buttons use `bevy::ui_widgets::Button` + `Activate` observers (the old
  `Interaction` API is deprecated in 0.20).
- Save format v2 with automatic v1 migration.

## Next actions
See TASKS.md (human playtest, add pickup AI, raid UI, third dungeon, optional
in-game encounter editor).
