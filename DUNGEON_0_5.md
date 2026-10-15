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
