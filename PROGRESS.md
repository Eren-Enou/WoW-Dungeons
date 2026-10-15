# Progress

Status: **0.6 (Dungeon Forge foundation) implemented and verified** with unit
tests, multi-layout simulation matrices, automated graphical runs and a test
of the packaged ZIP. 0.6 adds the blueprint and kit formats, the deterministic
generator, components, validation, the CLI, a generated dungeon you can play
from the hub, and GLB props with fallback. No human has played 0.2–0.6 yet.
The scripts for that are PLAYTEST_FORGE.md, PLAYTEST_0_5.md,
PLAYTEST_NAVIGATION.md and PLAYTEST_0_3.md.

Environment: Windows 11 Pro, Rust 1.99.0, Bevy 0.20.0 (pinned `=0.20.0`,
Cargo.lock committed), NVIDIA RTX 2070 SUPER (Vulkan).

## 0.6 — Dungeon Forge foundation

Plan: DUNGEON_FORGE_PLAN.md. Formats, results and continuation plan:
DUNGEON_FORGE_0_6.md. Asset research: ASSET_PIPELINE_RESEARCH.md.

Starting point: commit c61cc28 (0.5 complete). Commits:

| commit | content |
|---|---|
| ecbb790 | Phase A: architecture review and plan |
| 19bb66b | Phase B/C: forge crate and CLI, volcanic kit, Emberforge Vaults |
| d272b83 | Phase D: multi-layout matrix; pit-corner and stale-commitment fixes |
| 84781aa | Phase E: GLB props, provenance, asset research |
| c44b416 | Phase F: profiling, bounds-pruned projection |

The version bump and docs commit follow these.

### Checks (run 2026-10-15 after the last code change)
- `cargo fmt --all -- --check`: clean.
- `cargo check --workspace`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --workspace --release`:
  - `dungeon_forge` 11 passed;
  - `dungeon_sim` 157 passed, 1 ignored (`diag_replay`);
  - `dungeon_game` 15 passed;
  - 0 failed.
- `cargo build --release --workspace`: succeeded.

### Acceptance (0.6)
1. A readable blueprint without coordinates: `assets/forge/blueprints/emberforge_vaults.toml`.
2. A deterministic builder: byte-identical output per seed (test).
3. Components cover branching (alternate route, side rooms), elevation
   (ramps, stairs, overlook, drop) and boss arenas.
4. Variation: 40/40 seeds, 39 distinct route shapes; three shipped layouts
   that differ in structure.
5. The output passes the loader plus the structure and encounter-space
   checks on every seed.
6. It loads in the game: listed in the Dungeons tab; graphical demo Victory
   in 390.6 s.
7. AI parties clear it:
   - 1000/1000 5-player and 250/250 10-player Normal runs over 10 layouts;
   - 0 s stuck.
8. Failed generation gives a diagnosis: a tally of causes plus suggestions
   (test with an impossible blueprint).
9. Authored dungeons:
   - unchanged by the forge;
   - the one simulation fix is equal or better on the same seeds;
   - the optimisation is bit-identical.
10. Documented: DUNGEON_FORGE_0_6.md, README, `dungeon-forge --help`.

### Headless (release; details in DUNGEON_FORGE_0_6.md)
- Generated layouts (10 layouts × 5 runs per configuration):

  | Configuration | Wins |
  |---|---|
  | Normal, level 3 | 1250/1250 |
  | Heroic, 5 players at level 5 | 200/200 |
  | Heroic, 10 players at level 3 | 99/100 |
  | Heroic, 5 players at level 3 | 0/50 (final boss) |

- Regression, against the 0.5 binary on the same seeds:
  - Vault 100/100%;
  - Warrens 99% (0.5: 98%);
  - Spire 291/300 (0.5: 289);
  - Cavern 100%;
  - Cinderdeep 30/30 at 5 and 10 players;
  - Cinder Court 30/30.
- Performance (ms/tick):

  | Dungeon | 5 players | 10 players |
  |---|---|---|
  | Cinderdeep | 0.031 (0.5: 0.041) | 0.044 (0.5: 0.065) |
  | Emberforge | 0.011 | 0.017 |
  | Legacy dungeons | 0.005 | ≤ 0.010 |

- Generation: about 5 ms per dungeon; navigation builds in at most 1.4 ms.

### Graphical (automated)
- Area-tour screenshots of the generated dungeon (entrance, ledge with drop
  strip, pit room, arena) were inspected.
- GLB braziers render in the arena. With the model file removed, the
  fallback crystals and a warning appear.
- `--input-test` (moved 30.0 m), `--frame-test`, `--tooltip-test`,
  `--click-test`, `--hub-tour`: OK.
- The hub-tour Dungeons tab lists "The Emberforge Vaults (level 3+)".
- Packaged 0.6.0 ZIP (28.6 MB): extracted and run from another directory;
  the generated dungeon and its GLB props loaded.

### Bugs found and fixed during 0.6
- **Pit-room corner portal 0.7 m wide**: oscillation; the overlap is now 3 m.
- **Stale dither commitment** (all dungeons): 10-player timeouts on
  generated layouts.
- **A drop shortcut from the entrance to the room before the final arena**
  (it skipped the dungeon): shortcuts and alternate routes are now limited to
  two rooms and never bypass a boss.
- **Molten column hiding the lava pit**: the column is smaller.
- **Braziers hidden behind arena walls**: they are scaled to show above.

### Not verified (needs a human)
- Whether generated layouts feel like places or like rooms on a grid;
  pacing; difficulty at level 3; route readability.
- Whether the tools are clear to another developer (PLAYTEST_FORGE.md
  part 3).
- Props on other GPUs; the package on other machines.

## 0.5 — the first true dungeon experience

Plan, layout, encounters, AI work, full tables and limitations:
DUNGEON_0_5.md. Packaging: RELEASE_BUILD.md.

Starting point: commit e2b58ab (0.4 complete). Commits:

| commit | content |
|---|---|
| c1c2a3f | Phase A/B: polygons, ramps in any direction, decor/lava/lights, cave renderer |
| 2298a44 | Phase C: Cinderdeep layout and encounters, polygon sliding, no overpulls |
| 594f738 | Phase D (part 1): lava look, lips, area tour, tests |
| 15aa9d8 | clippy fix |
| a7ad756 | Phase E: portal-edge fix, cone-blocked melee, walkable ranged spots |
| dffeafc | Phase F: packaging script, HOW_TO_PLAY, manual CI workflow, version 0.5.0 |
| cb76f37 | soft crowd separation, banner counts optional encounters |

The documentation commit (with `--start-level`) follows these.

### Checks (run 2026-10-15 after the last code change)
- `cargo fmt --all -- --check`: clean.
- `cargo check --workspace`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --workspace --release`: **156 passed**, 1 ignored (`diag_replay`)
  in `dungeon_sim`; **15** passed in `dungeon_game`; 0 failed.
- `cargo build --release --workspace`: succeeded.

### Headless (release; details in DUNGEON_0_5.md)
- Cinderdeep at level 5, 30 seeds per batch:
  - each player class: 30/30;
  - each AI profile: 30/30;
  - 10 players: 30/30;
  - Raid (10) difficulty at level 6: 30/30;
  - Heroic at level 7: 29/30.
  - Stuck time 0.1–0.5 s per run, unreachable time 0.0 s, deterministic replay
    OK.
  - An AI clear takes about 8 minutes (5 players) and 4.8 minutes (10).
- Against the 0.4 binary on the same seeds:
  - Vault 100%, Warrens 98–100% (0.4: 99%), Spire 96–97% over 300 seeds
    (0.4: 97%), Cavern 100%, Cinder Court 30/30.
  - Dithers are lower everywhere.
  - Rectangle navigation was verified bit-identical after the polygon refactor
    (per-seed log hashes).
- Benchmarks:
  - Cinderdeep: 0.041 ms/tick with 5 players, 0.062 with 10;
  - legacy dungeons: ~25% slower than 0.4 (Vault 0.004, Cinder Court 0.009).

### Graphical (automated)
- Area-tour screenshots of the entrance, fork, ridge ledge, Kiln Span, slag
  pit, Cistern and Heart Island were inspected.
- Cinderdeep demo at level 5: victory in 465 s, 0 deaths, about 60 fps.
- `--input-test`, `--frame-test`, `--tooltip-test`, `--click-test`,
  `--hub-tour`: OK.
- The packaged ZIP was extracted and run from another working directory:
  - the headless runner won 2/2;
  - the game smoke test passed;
  - a Vault demo won, and `saves/profile.json` was created next to the exe.

### Bugs found and fixed during 0.5
- **Hash drift after the polygon refactor.** Fixed with exact rectangle fast
  paths.
- **Healer pinned on a diagonal ramp side.** Sliding only worked along the
  axes; fixed with polygon sliding.
- **Mid-boss overpull from a healer spot.** Fixed by the aggro radius check.
- **Lava that looked like floor.** Fixed with a darker base and crust plates.
- **Slag pit hidden by full walls.** Fixed with lava lips.
- **10-player timeout.** Actors on a slanted portal edge slid along it
  forever.
- **12 s melee shuttle on the Kiln Span.**
- **Healer spots across lava.**
- **Clippy lint that slipped into 594f738.** Caused by `| tail` hiding the exit
  code; checks are now gated without a pipe.

### Not verified (needs a human)
- Everything a person would feel:
  - how long a first human clear takes (target 8–15 min);
  - whether Normal is too easy at level 5;
  - whether the routes, drop and return tunnel read clearly;
  - camera comfort in tunnels and at the boss.
- Companion movement as seen by a player (stacking when converging, remaining
  dithers).
- The package on other machines, GPUs and Windows 10; SmartScreen behaviour.
- The GitHub Actions workflow (never run; nothing pushed).

## 0.4 — three-dimensional navigation and dungeon environments

Design, full results, benchmarks and limitations: NAVIGATION_0_4.md.

Starting point: commit 9a66d86 (0.3 complete). Commits:

| commit | content |
|---|---|
| 97031ff | review of 0.3 navigation and the chosen design |
| ebd8cb7 | Phase A: navigation module and tests |
| aec93a1 | Phase B: simulation integration, spatial combat, LOS-aware AI, renderer port |
| 6656ec8 | Phase D: Cavern Traverse, navigation metrics, traversal tests |
| 4a4d115 | Phase C: debug overlay, camera under ceilings |
| 415b2ca | Phase E: cheaper line of sight |

The documentation commit follows these.

### Checks (run 2026-10-14 after the last code change)
- `cargo fmt --all -- --check` — clean.
- `cargo check --workspace` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo test --workspace` — **142 passed** in `dungeon_sim`, **15** in
  `dungeon_game`, 0 failed.
- `cargo build --release --workspace` — succeeded.

### Headless (Balanced, release)
- Cavern Traverse:
  - 5 players: 100/100 wins;
  - 10 players: 100/100;
  - 25 players: 20/20;
  - 40 players: 10/10;
  - Aggressive and Conservative profiles: 100/100 each;
  - Heroic: 50/50;
  - level 1 with each player class: 97–100% (30 seeds each).
  - 0.0 s stuck in every batch, deterministic replay OK.
- Existing dungeons, compared with 0.3 on the same seeds:
  - Vault: 100% (0.3: 100%);
  - Warrens: 99% (0.3: 97%);
  - Ashen Spire: 97% (0.3: 98%);
  - Cinder Court: 30/30 (0.3: 20/20).
- Benchmarks at 25 players: 0.022 ms/tick (0.3: 0.013); at 40 players: 0.049 ms/tick (0.3: 0.023).

### Graphical (automated)
- The Cavern demo with `--nav-debug --zoom 30` was screenshotted on:
  - the walkway, 5 m up;
  - the bridge fight;
  - the underpass beneath the deck;
  - the lava hollow;
  - the basin, 3 m below the entrance.
- The screenshots exposed a camera collapse under the gallery, now fixed.
- `--input-test`, `--frame-test`, `--tooltip-test`, `--click-test`,
  `--hub-tour` and `--editor-test` pass, and Sunken Vault demo screenshots look as in 0.3.

### Bugs found and fixed during 0.4
- **Healer flip-flopping at a corner.** At a line-of-sight boundary the healer alternated between two goals every tick. Fix: one consistent goal, plus a general dither commitment that never overrides dodging.
- **Stalled in a doorway.** A waypoint 0.046 m away read as "arrived" mid-route, and the leader stood in a doorway forever. Fix: skip waypoints closer than 0.1 m, and count "arrived but the goal is far" as stuck.
- **Double-speed falls.** Heights were settled twice per tick.
- **Ranged DPS out of the healer's sight.** In the Spire boss fight, ranged DPS parked in a stair corridor that the healer couldn't see.

### Not verified (needs a human)
- How climbing, bridges, drops and the camera under ceilings feel with real
  input.
- Whether the AI's movement looks natural to a player.
- Whether line of sight around corners in the old dungeons feels fair.
- A human-controlled run of the Cavern. Its traversal is verified only for AI
  parties and scripted player movement in tests.

## 0.3 — combat UX, controls and gameplay polish

Starting point: commit 14730d3 (0.2 complete). Commits: c24f3c7 + a25727f
(Phase A controls/camera), 905f3b2 (sim APIs), 8878767 (Phase D frames and
mouseover + Phase B action bar + resource readout), f49f356 (Phase C threat),
630effa (Phase F tooltips), 45efc02 (Phase G readability).

### Checks (run 2026-10-14 after 45efc02)
- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo test --workspace` — **118 passed** in `dungeon_sim`, **14 passed** in
  `dungeon_game`, 0 failed.
- `cargo build --release --workspace` — succeeded.
- `dungeon-headless --runs 10` (Vault, L1) — 10/10 wins, deterministic replay OK;
  `--party-size 10 --level 5 --runs 5` — 5/5, replay OK. The sim API work did
  not change combat rules.

### Unit tests added
- Controls (10, `controls.rs`): W moves along facing, right-drag steers,
  left-drag never changes facing, A/D turn vs strafe with the right button,
  both buttons run, auto-follow eases behind a moving character, waits while
  standing and can be disabled, manual camera input wins for a grace period,
  backpedalling doesn't spin the camera, angle wrapping.
- Camera: collision keeps clear of walls including side probes; smoothing pulls
  in fast and ignores sub-deadband changes (no jitter).
- Sim (5): mouseover heal targets the hovered ally and keeps the selection;
  friendly target order mouseover → selected ally → self; hostile spells ignore
  mouseover; `ability_state` agrees with what a cast does (range, resource, GCD,
  cooldown); `threat_view` reports secure / contested / lost and taunt.
- Describe (3): ability text uses real numbers scaled by power; every ability,
  class, profile and stat has text; gear comparison with the equipped item and
  role mismatch.
- HUD (1): action-slot status priority (cooldown > resource with wait time >
  range > blockers; GCD alone isn't an error).

### Graphical runs (automated, real input/picking pipeline)
- `--input-test`: scripted key presses through the real controls: moved 29.7 m
  with W/Q/D, turned, Tab-targeted, casts landed; auto-follow left a 0.03 rad
  gap between facing and camera. Same result as before the HUD rework (compared
  against a stash of the changes).
- `--frame-test` — **FRAME TEST OK**: synthetic pointer hover over a party frame
  set the mouseover to that ally and `cursor_over_ui`; pressing 1 (Mend) with an
  enemy selected healed the hovered ally and kept the enemy selected; clicking
  another frame selected that member and produced no world click.
- `--tooltip-test` — **TOOLTIP TEST OK**: armor stat and a vendor item showed
  their tooltips fully on screen (the vendor one flipped left of the right-hand
  panel). Screenshot of the Mend tooltip flipping above the action bar.
- `--click-test` OK, `--hub-tour` OK, `--editor-test` OK (scratch copy of
  `assets/content/dungeons`).
- `--demo` screenshots: Warden (threat line "SECURE on you", green plates, ToT
  frame, GCD sweeps), Arcanist in a 10-person raid (compact frames, five-second
  rule countdown, "Threat 30% of pull"), Striker (0 energy → every slot "need N
  energy - X s" in blue during the GCD; this was a bug found in the screenshots
  and fixed in 45efc02), Mender (Smite red "Out of range", Purify "nothing to
  dispel").
- Threat alerts end-to-end: `--demo --class warden --profile aggressive
  --dungeon gloomroot_warrens --level 4` logged "Aggro lost: Rootweaver Shaman
  is attacking Mirelle - taunt!" at 5.9 s, "Taunted Rootweaver Shaman" at
  11.2 s, and a second loss at 152.6 s (2 aggro-loss alerts in a 173 s run with
  the Aggressive party); a screenshot shows the orange alert, the red
  "AGGRO LOST" target-frame line and the red plate.

### Not verified in 0.3 (needs a human)
- How the controls and auto-follow *feel* with a real mouse (drag thresholds,
  turn rate, follow speed 3.0, 1.2 s grace).
- Readability of threat colours in big packs, alert frequency, tooltip delay
  (0.2 s), frame sizes for healers in raids.
- Right-drag started over a HUD panel still steers (only left-drag, clicks and
  zoom are blocked over UI) — intentional, but untested by a person.

### Known limitations (0.3)
- Mouseover in 3D picks the nearest friendly whose pick sphere the ray hits;
  allies standing inside each other pick the nearer one.
- Raid frames show only the first debuff; full details are in the tooltip.
- Tooltips are text only; no icons. Aura tooltips are reached through party
  frames, the target frame and your aura row, not per-aura chips.
- Out-of-range checks use the sim's edge-to-edge range; casts finishing get the
  sim's +3 m leeway, which the bar doesn't show.

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

## Not verified / requires human playtesting (0.2)
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
See TASKS.md: the human playtest (PLAYTEST_0_3.md) comes first.
