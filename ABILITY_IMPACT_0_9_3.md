# Ability impact (0.9.3)

**Why:** the owner's playtest (PLAYTEST_REMIX_0_9.md, 2026-10-17) said
combat lacks visual clarity and "needs further development before it
becomes truly satisfying". 0.9.2 made outcomes readable as text. In the
world, though, `fx.rs` still draws every party member's melee hit as the
same small cube and every heal as the same green sphere. Your Shield Slam
looks like a companion's auto-attack. Interrupts, shields, defensives and
crowd control leave no trace in the world.

**Rule:** presentation only, as in 0.9.2. Effects are driven by simulation
events and simulation state (auras) and change nothing in the simulation;
event-log hashes must stay identical. No camera shake: camera and controls
were reported good, so they stay as they are.

## Plan

1. **Hit reaction.** A target hit by one of your abilities or auto-attacks
   briefly swells and flashes white; a crit swells more and flashes longer.
   A heavy hit on you (a crit, or 8% of your health or more) flashes your
   model red. Companions' hits cause no reaction, which keeps the 0.9.2
   hierarchy.
2. **Your ability signatures**, chosen by the ability's effect in
   `class.rs`:
   - single-target melee (Shield Slam, Rend Strike, Kick's hit): a gold
     burst and a ground flash under the target;
   - spells (Arc Bolt, Smite, Flare): a larger projectile and an impact
     burst at the target;
   - area attacks (Whirl, Quake Stomp): the ring is drawn at the ability's
     real radius (6 m and 8 m; today every ring is 6 m), for companions'
     area attacks too;
   - your heals: a taller, brighter column; crits larger. Companions' heals
     are smaller.
3. **Interrupts** (anyone's): a cyan burst with shards at the stopped
   caster. Yours are larger.
4. **Lasting states**, read from auras each frame:
   - **absorb shields** (Ward, and enemy shields): a translucent bubble
     while the shield holds, and a pop when it ends;
   - **defensives** (Bulwark): a faint shell;
   - **crowd control** (Hex): a slowly turning ring over the enemy's head,
     on top of the existing purple tint.
5. **Raids.** In a 10-player raid, companions' ordinary melee flashes are
   dropped, as their floating numbers were in 0.9.2.

## Testing

- A pure classification (`impacts()`) from events to effect kinds, unit
  tested on autopilot runs of all four classes.
- Hash identity against 0.9.2: 8 dungeons × 4 classes × 10 seeds.
- `--demo` screenshots every 1-2 s for each class and a raid, looked at.
- The standard self-tests.
- The playtest script PLAYTEST_COMBAT_0_9_2.md gets questions for these
  effects.

## Results (verified 2026-10-17, automated only)

**What was built:** as planned. `fx.rs` has a pure `impacts()`
classification, lasting looks read from auras (`aura_looks()`), and a hit
reaction that runs after the actors' transforms are set for the frame. An
area ring's outer edge is the ability's radius.

**Simulation unchanged:**
- No file under `crates/sim`, `crates/forge` or `assets` changed.
- 320 headless runs (8 dungeons × 4 classes × 10 seeds) have `log_hash`
  values identical to the 0.9.2 package's.

**Tests:**
- fmt, check, `clippy -D warnings` clean.
- Tests: forge 28, game 38 (+4 for impacts), sim 164 (1 ignored).
- The impact tests run autopilot parties step by step and check:
  - your strikes are marked as yours, and companions' aren't;
  - Quake Stomp's ring is 8 m and a companion's Whirl 6 m;
  - an area attack draws one ring per cast, never one per target;
  - crits react twice as strongly;
  - Bulwark, Ward and Hex show as lasting looks;
  - Arcanist bolts and interrupts are classified;
  - in a 10-player raid no ordinary companion swing is drawn, while
    enemies' swings still are.

**Graphical (release build, 1280×760; screenshots looked at):**
- **Warden:**
  - a half-speed burst (one frame per 0.125 s of play) shows Quake Stomp's
    gold ring growing to its radius;
  - the skulker hit turns pale with the hit flash;
  - Ward from the companion healer shows as a blue bubble around you
    while "Ward 9s" is up.
- **Arcanist and Mender runs:** the Hex ring turns over the hexed
  Cragbound Brute.
- **10-player Mender raid** (Cinder Court): no panics.
- **Self-tests, all exit 0:**
  - `--input-test`, `--frame-test`, `--tooltip-test`, `--click-test`,
    `--hub-tour`, `--remix-test`;
  - `--viewer <id> --viewer-test` on three dungeons.

**Seen, not changed:**
- In `--demo`, the camera sometimes faces a wall or away from the boss, for
  ranged and healer characters. This is the demo camera, from before 0.9.3;
  it made the Mender's own Ward hard to catch in stills.
- **Interrupt bursts and strike bursts** last 0.2–0.35 s and were not
  caught in a still. They are covered by the classification tests only.

## Not verified
- **Whether the effects make combat more satisfying, or too busy.** Not
  covered by a recorded human playtest. PLAYTEST_COMBAT_0_9_2.md has
  questions for 0.9.3.
- **Frame rate with many lasting looks** in a raid was not measured.
  Lasting looks cost one entity per state, and the frame test passes.
