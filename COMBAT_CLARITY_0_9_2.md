# Combat clarity and ability impact (0.9.2)

**Why:** the owner's playtest (PLAYTEST_REMIX_0_9.md, "Notes", 2026-10-17)
found that combat lacks visual and numerical clarity, and that individual
ability outcomes are hard to recognise. Controls, camera and companions were
reported as good, so they are not touched.

**Rule:** presentation only. The simulation, AI, balance, controls and
enemies don't change. Every number shown comes from a simulation event
(`events.rs`), an ability definition (`class.rs`) or the metrics
(`metrics.rs`); nothing is estimated. Simulation event-log hashes must stay
identical to 0.9.1.

## What a player sees today (review)

| System | File | What it shows | Gap |
|---|---|---|---|
| Floating text | `overlay.rs` | Damage, heals, avoided hits, interrupts, taunts, deaths above heads | Your hits, your ticks, companions' hits and every party member's auto-attacks look alike: same colours, sizes differ only slightly. Partial absorbs are hidden. "Interrupted!" doesn't say what was stopped or by whom |
| Banner / alert / error | `dungeon.rs`, `hud.rs` | Your interrupt ("You interrupted X's Y!"), taunt alert, failed casts in red | Good; keep |
| Combat log | `hud.rs` (bottom left) | 10 px text, every notable event, everyone's | Too small and too busy to read during a fight |
| Action bar | `hud.rs` | Cooldown, GCD, cost, status | Says what is ready, not what just happened |
| Results | `results.rs` | Per-member damage/healing/deaths after the run | Only after the run |

What is missing is one place that answers "what did my last button press
do?", and a visual ranking so that your own numbers stand out from the
party's.

## Plan

### 1. Your recent actions, above the action bar (new, small)
Up to four lines, newest at the bottom, each fading after about 4 s, on a
light dark backing. They sit above the right half of the action bar: first
screenshots showed that centred lines covered the player's own character,
which the camera keeps at the centre. Built
from your own events by a pure function (`combat_feed.rs`), grouped by
ability and simulation time, so an AoE gives one line:

- **Damage:** "Shield Slam > Ossuary Warden 82 CRIT",
  "Quake Stomp > 3 enemies 105 (1 crit)", "Rend Strike > Ghoul dodged",
  "Arc Bolt > Censer Bearer 64 (20 absorbed)".
- **Healing:** "Mend > Torvald +420", "Mend > Torvald +120 (300 overheal)".
- **Utility, from the ability's own effect:**
  - shields: "Ward > Torvald shielded";
  - heal and damage over time: "Renew > Torvald heal over time",
    "Lacerate > Ghoul bleeding x2";
  - defensives: "Bulwark: 40% less damage taken for 8 s" (the ability's
    definition, not an invented number);
  - "Veil: threat reduced 50%";
  - crowd control: "Hex > Ghoul incapacitated 8 s", "Hex on Ghoul broken";
  - "Purify > Torvald removed Creeping Rot";
  - taunt: "Challenge > Ghoul taunted".
- **Interrupts:** "Kick > interrupted Censer Bearer's Dark Mend"
  (highlighted), or "Kick > Ghoul 20 (nothing to interrupt)".
- **Companion interrupts** appear dimmer ("Torvald interrupted X's Y"):
  the third tier of the brief's hierarchy.
- **Not shown:** auto-attacks, ticks of your over-time effects (they still
  float), and failed casts (the existing red error line covers them).

### 2. Floating-text hierarchy (refine `overlay.rs`)
1. **Your outcomes:**
   - largest and brightest; crits bigger with "!";
   - your direct hits are larger than your ticks;
   - partial absorbs show "82 (20 abs)";
   - your heals on others are as visible as heals on you.
2. **Dangerous enemy casts:** unchanged (cast bars, telegraphs, alerts).
3. **Companion interrupts:** "Interrupted: Dark Mend" (pale).
   **Your interrupts:** larger and brighter.
4. **Companion hits and ticks:**
   - smaller and more transparent;
   - in a 10-player raid, companions' non-crit auto-attacks are not
     floated (they remain in the log and metrics).

### 3. This fight's numbers (optional, K)
A compact panel at the bottom right shows your numbers for the current or
last fight:
- duration;
- damage and damage per second;
- healing with overheal %, and absorbs;
- damage taken and avoidable damage taken;
- interrupts (succeeded/attempted).

It shows the role's lines first. The numbers are differences of
`ActorMetrics` between the pull and now; this is not a damage meter. On by
default; K hides it.

## Testing
- Unit tests for the feed on real simulation runs:
  - each class's abilities give the expected lines;
  - an AoE gives one line;
  - companions' auto-attacks give none;
  - a heal reports overheal.
- Simulation unchanged:
  - headless batches against the 0.9.1 package, compared by `log_hash`;
  - the game's own self-tests.
- Graphical:
  - `--demo` for each class, with screenshots;
  - a 10-player raid;
  - boss fights with telegraphs;
  - the standard self-tests (`--input-test`, `--frame-test`,
    `--tooltip-test`, `--click-test`, `--hub-tour`, `--viewer-test`,
    `--remix-test`).
- A short before/after human playtest script (PLAYTEST_COMBAT_0_9_2.md).

## Not in scope
- New mechanics, damage meter, combat log redesign, sounds.
- "Lifesaving heal" detection: the events don't record health before a
  heal, so it would have to be estimated.

## Results (verified 2026-10-17, automated only)

**What was built:** as planned above, with these differences found while
testing:
- **Feed position.** The feed moved right of centre. Centred, its oldest
  lines covered the player character.
- **Shields line.** The healer's shield line got its own row in the fight
  panel; it wrapped.
- **Fight panel.** It stays hidden until the first fight.
- **Interrupt banner.** The existing "You interrupted X's Y!" banner is
  kept. Your interrupt is now also a feed line and a floating name, which
  repeats it on purpose: it is the action the brief singled out.

**Simulation unchanged:**
- No file under `crates/sim`, `crates/forge` or `assets` changed.
- `dungeon-headless --json`, 8 dungeons × 4 classes × 10 seeds: all 320
  `log_hash` values are identical to the 0.9.1 package's.

**Tests:**
- fmt, check, `clippy -D warnings` clean.
- Tests: forge 28, game 34 (+7 for the feed and fight panel), sim 164
  (1 ignored).
- The feed is tested on autopilot runs of all four classes, fed one
  simulation step at a time as the game does. They check:
  - each class's abilities give named lines;
  - Quake Stomp's hits are one line;
  - auto-attacks and Renew ticks give none;
  - player and companion interrupts both appear.

  Constructed events check:
  - the AoE wording;
  - overheal;
  - a crit heal on yourself;
  - a Silence Pulse that stops nothing (no event, counted from the
    metrics).

  The fight panel is tested on a real pull: the first fight counts from
  zero, and the damage shown equals the metrics.

**Graphical (release build, 1280×760):**
- **`--demo` runs, screenshots looked at:**
  - Warden, Sunken Vault (full loop to results and hub);
  - Mender, Hollow Reliquary (twin abbots, boss phase);
  - Striker, Gloomroot Warrens (Mother Brakkis);
  - Arcanist, Sunken Vault boss;
  - a 10-player Cinder Court raid (Twinmaw, Earthshake);
  - the Mender again after the feed moved.

  In them:
  - interrupts show the spell stopped: "Shield Bash > interrupted Morvane,
    the Hollow King's Hollow Bolt", "Mirelle interrupted Abbot Vael's
    Litany of Mending";
  - "Lacerate > Mother Brakkis: bleeding";
  - "Veil: threat reduced 50%", "Ward > Brannoc: shielded";
  - crits stand out;
  - the healer's panel reads "Healing 904 (4% overheal) / Shields absorbed
    416".

  In the raid, companions' ordinary hits are not floated; damage taken
  still is.
- **`--input-test`** for Striker, Warden, Mender and Arcanist. It now also
  presses K and fails if the panel stays shown; all pass. Feed lines seen
  include "Arc Bolt > Gloomfang Skulker: 123 CRIT" and "Mend > you: +0 (420
  overheal)".
- **Other self-tests, all exit 0:**
  - `--frame-test`, `--tooltip-test`, `--click-test`, `--hub-tour`, `--remix-test`;
  - `--viewer <id> --viewer-test` on hollow_reliquary, emberforge_vaults
    and sunken_vault.

  (Run without `--viewer <id>`, `--viewer-test` panics; the 0.9.1 package
  does the same. It is how the test is invoked, not a regression.)

**Package (local only, not published):**
- `dist/DungeonSandbox-0.9.2-windows-x64.zip` is 31.2 MB.
- Extracted with Expand-Archive, `--input-test` and `--remix-test` pass,
  and HOW_TO_PLAY lists K.

**Seen in the screenshots, not fixed:**
- **Feed over the world.** Lines can still cover a companion standing to
  the player's right. The backing is light, and the lines are marked
  `Pickable::IGNORE`, so they don't take clicks (by code; no test clicks
  through them). Floating numbers over a large boss overlap the target
  frame, as before.
- **Long lines** (companion interrupts on long boss names) wrap to two rows.
- **Window size.** At 1280 wide the fight panel clears the action bar.
  Below about 1060 px they would meet (not tested; the default window is
  1280×760).

## Not verified
- **Human readability.** Whether combat is now easier to read is not
  covered by a recorded human playtest. PLAYTEST_COMBAT_0_9_2.md is the
  before/after script.
- **Camera and controls.** No camera or control code changed except the
  new K key. No one has played 0.9.2.
