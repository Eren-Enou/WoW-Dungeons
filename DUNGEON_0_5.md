# 0.5 — The Cinderdeep Hollows

The first dungeon built to be an adventure rather than a test level: an
original lava-cavern descent inspired by the *feel* of classic early MMO
caves (winding tunnels, lava rivers, branching routes, a sense of going
deeper). The layout, names, enemies and encounters are original.

## Phase A plan (written before implementation)

### What 0.4 can't express

The 0.4 navigation (NAVIGATION_0_4.md) only allows axis-aligned rectangles,
with ramps along x or z. Every room is a box and every passage turns at 90°.
Diagonal tunnels, angled walls, round chambers and switchbacks are
impossible. The renderer also draws walls as axis-aligned boxes, and the
camera collides with axis-aligned boxes.

### Minimum extensions

1. **Convex polygon footprints.** An area can give `points = [[x, z], ...]`
   instead of `min`/`max`. A rectangle is a 4-point polygon, so the rest of
   the system works unchanged on polygons: portals become convex
   intersections, walls become polygon edges at any angle, and line of sight
   clips segments against polygons. A cave is several convex pieces that
   overlap where they connect. Non-convex polygons are rejected with a
   message saying how to split them.
2. **Ramps in any direction.** `slope_from = [x, z]` / `slope_to = [x, z]`
   give the heights `y` and `y_to` at two points. `axis` keeps working.
3. **Decoration that can't lie about walkability.** `[[dungeon.decor]]`
   (rock pillars, stalagmites, boulders, crystals) must lie outside every
   walkable area at its height. Lava becomes a polygon (`points`) for rivers
   and pools. Validation enforces both.
4. **Atmosphere data.** `[[dungeon.lights]]` (coloured point lights) and
   per-area `tint`; distance fog per dungeon.
5. **Renderer.** Polygon floor prisms and sloped decks; walls at any angle
   drawn as jagged rock (deterministic jitter that pushes outward only, so
   rock never covers walkable floor); pulsing lava; drifting embers; fog;
   camera blockers along angled walls.

### AI work expected from the new layout

0.4 left these open: dithering; crowding at doorways; formation spots on the
wrong surface; ranged spots in awkward places; separation at one-way drops.
Planned fixes:
* soft separation between party members;
* formation spots kept on surfaces reachable from the leader at the leader's
  level, otherwise the leader's own spot;
* the leader waits for stragglers after drops and long traversals before
  pulling.

Everything else will be measured on the new dungeon first.

### Layout (descending)

| # | Region | Height | Palette | Role |
|---|---|---|---|---|
| 1 | Frostvent Gate | 0 | cold blue-grey | entrance cavern, scout pack |
| 2 | Glowworm Fork | 0 → −2 | teal / dim | winding tunnel that **branches** |
| 3a | Ashgrub Run | −2 → −4 | brown, smoky | low winding route; a caster protected by brutes |
| 3b | Ridge Path | −2 → +2 | grey ledge | high route along a ledge overlooking the gallery; hounds with frontal bites; ends in a drop or ramp into the gallery |
| 4 | Smelter Gallery | −4 | orange slag | **boss 1**: slag pits, adds, dispellable burns |
| 5 | Obsidian Grotto | −4 → −1 | violet-black glass | **optional** side boss off the gallery |
| 6 | Kiln Span | −4 | dark stone over glowing river | the **long bridge** over the lava river; pack on the bridge; view of the Cistern below |
| 7 | Magma Cistern | −8 | bright lava | the **lava chamber**: basalt causeways between lava pools, vents, a pack reachable from two causeways; a **return tunnel** back up to the Fork |
| 8 | Heart of Cinderdeep | −12 | deep red | switchback descent to the **final boss** on an island in a lava moat |

Cooler colours at the surface and hotter ones deeper down tell the player how
far in they are. The Kiln Span looks down on the Cistern, and the Ridge Path
looks down on the Gallery, so the next destination is visible before you
reach it. Two routes (Ashgrub Run and Ridge Path) rejoin at the Gallery.
The Steam Vent tunnel from the Cistern back to the Fork is the return
connection.

### Phases
* **A**: this plan.
* **B**: polygons, ramps in any direction, decor/lava/lights schema, validation, renderer, tests.
* **C**: layout, traversable by the AI before full encounters.
* **D**: encounters, bosses, atmosphere.
* **E**: AI fixes driven by headless batches and screenshots.
* **F**: Windows packaging, regression against 0.4, docs.

---

# Results (what was built and verified)

Everything below was run on the development machine (Windows 11, release
build). **No human has played the Cinderdeep Hollows yet**; the script for
that is [PLAYTEST_0_5.md](PLAYTEST_0_5.md).

## Commits

| commit | content |
|---|---|
| c1c2a3f | Phase A/B: convex polygon footprints, ramps in any direction, decor/lava/light schema, cave renderer, this plan |
| 2298a44 | Phase C: Cinderdeep layout and encounters; polygon wall sliding; ranged spots never overpull |
| 594f738 | Phase D part 1: lava that reads as lava, lips over pits, `--area-tour`, Cinderdeep tests |
| 15aa9d8 | clippy fix for 594f738 |
| a7ad756 | Phase E: portal-edge fix, cone-blocked melee, walkable ranged spots |
| dffeafc | Phase F: Windows packaging, HOW_TO_PLAY, manual CI artifact, version 0.5.0 |
| cb76f37 | soft crowd separation; intro banner counts optional encounters |

The documentation commit follows these.

## Navigation changes (Phase B)

* `Poly` (`crates/sim/src/poly.rs`): a convex CCW polygon with contains,
  clamp, segment clipping (Cyrus–Beck), half-plane clipping, intersection and
  inset. A rectangle caches its `Rect` and takes exact axis-aligned fast paths.
* Surfaces, portals, `inner` steering regions and walls are polygons. Walls
  can run at any angle, and line of sight clips against polygon volumes.
* Ramps in any direction (`slope_from`/`slope_to`).
* Polygon wall sliding: an actor slides to the nearest walkable point, at any
  angle.
* Validation: convexity ("split it into convex pieces"), decor outside walkable
  space, lava polygons, and all the 0.4 rules.
* **Legacy identity**: after the polygon refactor, per-seed log hashes of the
  Vault, Warrens, Spire, Cinder Court and Cavern Traverse (5 and 10 players)
  were compared with the 0.4 binary: identical. Later AI changes (Phases C and
  E) deliberately change outcomes; see "Regression" below.

Alternatives considered and rejected:
* a full navmesh or triangle soup: much more code, and it loses the readable
  data format;
* arbitrary non-convex areas: every query gets harder, and authors can split
  shapes instead;
* true 3D: not needed, since there are no overhangs that a 2.5D stack can't
  express.

**Authoring pattern** for an angled ramp between rooms: room — 6 m landing —
ramp — 6 m landing — room. Each landing overlaps the ramp by 1.5 m, with the
seam perpendicular to the slope. This satisfies the rule that a ramp may
overlap its floor by at most 2 m, at any angle.

## Layout (Phase C)

`assets/content/dungeons/06_cinderdeep_hollows.toml`:
* 49 convex areas, most of them non-rectangular;
* 4 lava polygons, 1 drop link, 20 decor pieces, 15 lights.

Recommended level is 5. The dungeon descends from 0 m to −12 m.

| Region | Height | Notes |
|---|---|---|
| Frostvent Gate | 0 | cold blue light, the entrance pack |
| Frostvent Passage | 0 → −2 | angled ramp down |
| Glowworm Bend / Fork | −2 | **branch point** |
| Ashgrub Run → Hollow → Slide | −2 → −4 | low road; optional Ashgrub Den |
| Ridge Ascent → Ridge Ledge | −2 → +1 | high road along an open ledge; optional Ridge Hounds; **one-way drop** into the Smelter |
| Smelter Gallery (South / West / East / North) | −4 | a ring around a slag pit; Kilnmaster Vorsk |
| Glass Passage → Obsidian Grotto | −4 → −1 | **optional** side boss, the Glass Matriarch |
| Kiln Span | −4 | 5 m wide **bridge** over a lava river 12 m below |
| Cistern Rim → Rim Tunnel | −4 | **return connection** back to Smelter West |
| Cinder Stair (switchback) | −4 → −8 | |
| Magma Cistern: Shore, West / East Causeway, Basalt Island, North Shore | −8 | the **lava chamber**; the island pack is reachable from two causeways |
| Heartward Descent / Stair | −8 → −12 | |
| Heart Causeway, Heart Island | −12 | octagonal **boss arena** in a lava moat |

Routes and sightlines:
* The low and the high road rejoin at the Smelter.
* The Ridge Ledge overlooks the Smelter. Its drop is a shortcut that can't be
  climbed back.
* The Kiln Span looks down on the river.
* The Rim Tunnel loops back to the gallery, so a group that went ahead can
  return to the Grotto.

A test (`cinderdeep_layout_has_routes_shortcut_and_return_connection`) asserts
the branch, the one-way drop and the loop.

Deviation from the plan: the return connection runs from the Cistern Rim back
to the Smelter, not from the Cistern to the Fork. This keeps the loop short
enough to be useful.

## Encounters (Phase D)

| # | Encounter | Where | Positioning problem |
|---|---|---|---|
| 1 | Frostvent Scouts (2 scouts, torchbearer) | Gate | warm-up pull |
| 2 | Ashgrub Den (optional): 2 brutes protecting a pyromancer | Ashgrub Hollow | **caster protected by melee**; interrupt Fireball |
| 3 | Ridge Hounds (optional): 2 hounds with frontal Molten Bite | Ridge Ledge | **elevated pack** on a narrow ledge, frontals |
| 4 | Kilnmaster Vorsk (boss) | Smelter North | **lava-side fight**: frontal slam, dispellable brand; at 50% slaglings and Slag Spill rings, so spread round the pit |
| 5 | The Glass Matriarch (optional boss) | Obsidian Grotto | three rings at once, stacking healing debuff on the tank, interruptible Glass Nova below 40% |
| 6 | Span Guard: shieldbearers with frontal Shield Slam around a Flamecaller (interruptible party pulse) | Kiln Span | **narrow passage with frontals**, caster behind melee |
| 7 | Cistern Guardians: Magma Brute (frontal) and 2 imps, 2 timed fire vents | Basalt Island | **approachable from two causeways**; vents on the approaches |
| 8 | Ignarax, the Molten Heart (boss) | Heart Island | **arena-aware**: frontal lash, interruptible Slag Bolt; at 60% eruptions (rings) and Magma Spawn; at 25% an uninterruptible Heart Pulse |

Bosses use phases and telegraphs rather than inflated numbers. Loot and
rewards are defined per encounter.

## Atmosphere

* Per-dungeon fog and ambient light.
* Per-area tints: cold blue-grey at the gate, teal in the Glowworm tunnels,
  violet glass in the Grotto, orange and red deeper down.
* 15 placed lights: cool at the entrance, warm near lava.
* Jagged rock walls. A deterministic jitter pushes rock outward only, so it
  never covers floor.
* Pillars, stalagmites, boulders and crystals, all outside walkable space.
* Lava: a dark molten base with a pulsing emissive glow, drifting crust plates
  and embers.
* 0.6 m lips at lava edges, so pits and rivers stay visible.

## AI (Phase E)

Measured problems and their fixes:

* **10-player timeout (seed 15) and 46.8 s stuck per run.**
  * Cause: a companion slid onto the slanted edge shared by a landing and its
    ramp. It sat a rounding error outside the portal and slid along the edge
    forever while the group waited.
  * Fix: polygon portals now accept a start point within 1 mm.
  * Result: stuck time fell to 0.6 s per run and the timeout disappeared. A
    regression test was added.
* **12 s back-and-forth on the Kiln Span** (both melee, in almost every run).
  * Cause: the only way round a shieldbearer's cone to the Flamecaller led
    backwards. At the detour point the code returned "walk straight", into the
    cone, then out again.
  * Fix: a cone-blocked melee keeps hitting what it can reach safely, or
    attacks the blocking enemy from behind.
* **Healer shuttling around the Basalt Island.**
  * Cause: ranged spots next to frontal cleavers were used without
    validation, so they could be across lava. Two routes of equal length made
    the waypoint flip.
  * Fix: those spots now go through the validated search, and every spot must
    be a short walk.
* **Overpulls** (Phase C): a healer moving to a spot near the next pack pulled
  it mid-boss. Spots inside an idle pack's aggro radius + 3 m are now
  rejected.
* **Pinned against a diagonal ramp side** (Phase C): sliding only worked along
  the axes. Fixed by polygon sliding.
* **Stacking**: soft separation (0.9 m personal space, 1.2 m/s drift).
  * Idle and following companions no longer stand inside each other.
  * Two melee converging on the same attack spot can still overlap, because
    separation is slower than steering by design.

Tried and reverted, because they measured worse or no better over 20 seeds:
* per-melee flank slots;
* longer commitment after repeated dithers: avoidable damage rose from ~600
  to ~900;
* a rule to exit the cone towards the attack spot.

Dithering is lower but not solved.
* A "dither" counts any two direction reversals within ~0.7 s. Most are
  legitimate re-decisions, such as a target dying or a dodge.
* Long loops (4+ dithers within 8 s by one actor) went from ~111 to ~93 per
  20 runs with 5 players.
* The remaining long loops involve melee and the healer around the Cistern
  pack.

Already present before 0.5, and kept:
* the leader autopilot waits until the party is gathered and topped up before
  each pull;
* formation spots are projected onto the floor at the leader's height;
* companions with an unreachable goal stay with the leader.

## Verification

Checks after the last code change (cb76f37):
* `cargo fmt --all -- --check`: clean.
* `cargo check --workspace`: clean.
* `cargo clippy --workspace --all-targets -- -D warnings`: clean.
* `cargo test --workspace --release`: **156 passed, 1 ignored** (the
  `diag_replay` tool) in `dungeon_sim`, and **15** in `dungeon_game`.

### Cinderdeep headless matrix (Balanced unless noted, level 5, 30 seeds each)

| Batch | Wins | Mean time | Deaths/run | Stuck s/run | Dithers/run |
|---|---|---|---|---|---|
| player Warden | 30/30 | 471 s | 0.00 | 0.2 | 46 |
| player Mender | 30/30 | 474 s | 0.00 | 0.1 | 55 |
| player Striker | 30/30 | 481 s | 0.00 | 0.3 | 75 |
| player Arcanist | 30/30 | 482 s | 0.00 | 0.2 | 60 |
| Conservative | 30/30 | 502 s | 0.00 | 0.3 | 127 |
| Aggressive | 30/30 | 436 s | 0.00 | 0.2 | 35 (ring hits 4.5) |
| Support | 30/30 | 480 s | 0.00 | 0.2 | 78 |
| Mechanics | 30/30 | 496 s | 0.00 | 0.3 | 136 |
| Heroic, level 7 | 29/30 | 606 s | 0.17 | 0.2 | 102 |
| 10 players | 30/30 | 287 s | 0.00 | 0.5 | 57 |
| 10 players, Raid (10) difficulty, level 6 | 30/30 | 589 s | 0.17 | 0.4 | 124 |

Every batch had 0.0 s unreachable time and passed deterministic replay. These
batches ran before the separation commit. After it, 30 seeds at 5 and at 10
players were again 30/30 (mean 483 s and 282 s; stuck 0.3 s and 0.6 s).

An AI-only clear takes about 8 minutes at level 5, including the three
optional fights. A first human clear is expected to be slower. The 8–15
minute target stays unverified until someone plays it.

**Difficulty note**: at its recommended level the AI party almost never dies
on Normal. AI difficulty is not human difficulty: the AI reacts within its
profile's reaction time and never misreads a telegraph. So Normal was not
tuned up before a human playtest; Heroic is the harder option.

### Regression against 0.4 (same seeds; the 0.4 binary and content were kept aside)

| Dungeon | 0.4 | 0.5 |
|---|---|---|
| Sunken Vault, 5 / 10 players (100 seeds) | 100% / 100% | 100% / 100% |
| Gloomroot Warrens, 5 / 10 (100) | 99% / 100% | 98–100% / 100% |
| Ashen Spire, 5 players (300) | 97% (291/300) | 96–97% (289–291/300) |
| Ashen Spire, 10 (100) | 100% | 100% |
| Cavern Traverse, 5 / 10 (100) | 100% / 100% | 100% / 100% |
| Cinder Court, 10 at level 10 (30) | 30/30 | 30/30 |

Ranges show the results before and after the separation commit.
* Dithers fell in every legacy dungeon:

  | Dungeon | 0.4 | 0.5 |
  |---|---|---|
  | Warrens | 45.8 | 35–40 |
  | Spire | 69 | 60 |
  | Spire, 10 players | 50 | 37 |
  | Cinder Court | 175 | 167 |

* Stuck time stayed at 0.0 s.
* Deterministic replay passed throughout.

### Performance (`--bench`, 10 runs, release)

| Dungeon | 0.4 ms/tick | 0.5 ms/tick |
|---|---|---|
| Sunken Vault | 0.003 | 0.004 |
| Ashen Spire | 0.004 | 0.005 |
| Cinder Court (10) | 0.007 | 0.009 |
| Cinderdeep (5) | — | 0.041 (≈1200× real time) |
| Cinderdeep (10) | — | 0.062 (≈800× real time) |

* Legacy dungeons cost ~25% more per tick, from the separation pass and the
  walk-length checks in spot searches.
* Cinderdeep costs more because of its 49 polygon areas and the many angled
  walls in sight checks.
* At 20 ticks/s the budget is 50 ms per tick.
* The graphical game ran Cinderdeep at about 60 fps (1785 frames in 30 s,
  vsync) on an RTX 2070 SUPER.

### Graphical (automated)

* `--area-tour` screenshots were taken and inspected for:
  * Frostvent Gate (entrance);
  * Glowworm Fork (branching tunnel);
  * Ridge Ledge (elevated passage);
  * Kiln Span (bridge over the river);
  * Smelter South (slag pit);
  * Cistern Shore (lava chamber);
  * Heart Island (boss arena).

  Lava reads as lava, the bridge and ledge read as walkable, and the pit lips
  are visible.
* `--demo --dungeon cinderdeep_hollows --level 5 --speed 4 --screenshots 8`:
  Victory in 465 s with 0 deaths. The screenshots of the Kiln Span fight and
  of the Ignarax phase 3 eruptions were readable. A first demo without
  `--level` used a fresh level-1 profile and wiped, as expected.
* `--input-test` (moved 29.8 m), `--frame-test`, `--tooltip-test`,
  `--click-test`, `--hub-tour`: all OK.
* The packaged ZIP was extracted elsewhere and run from another working
  directory (see RELEASE_BUILD.md): the content was found, a Vault demo won,
  and `saves/profile.json` was created next to the exe.

## Known limitations

* Not played by a human. Time-to-clear, difficulty and readability for a
  person are unverified.
* Melee converging on the same spot can still overlap. Dithering is reduced,
  not gone.
* Ignarax is large. With the camera close, the model can fill much of the view
  during the fight.
* Decor does not block sight or movement. It is placed outside walkable space,
  but a ranged attack can pass through a pillar.
* Polygons must be convex, so curved caves are built from overlapping pieces.
* The Windows package is unsigned (SmartScreen warning) and was tested on one
  machine only.
