# Playtest notes: 0.8 Dungeon Viewer

These notes record feedback from the first user session with the 0.8 dungeon
viewer, as the user reported it. Nothing here was measured by the developers.
The open questions below have **not** been validated.

## Who and what

- **Tester:** the project owner, the first user.
- **Build:** 0.8.0, the dungeon viewer (`--viewer`), on the generated dungeons
  (The Hollow Reliquary and The Emberforge Vaults) and the handwritten ones.

## Confirmed positive

- The viewer is enjoyable and useful.
- The inspection layers are particularly impressive.
- Room and component information is valuable.
- Procedural seed switching is exciting.
- Seeing different layouts has changed the long-term vision of the project.

## Requested improvements, and what 0.8.1 did about them

| Request | 0.8.1 response |
|---|---|
| More accessible and flexible flight-speed adjustment | Bottom-centre speed bar with a readable m/s value and preset name. Slow/normal/fast presets on 7/8/9 or buttons; fine steps on −/= (×1.15) or buttons. The mouse wheel and Shift/Ctrl stay. Speed range 0.5–200 m/s, remembered for the session and saved to `viewer.toml`. |
| More comfortable camera behaviour | Fly movement eases in and out instead of starting and stopping instantly. Near walls, walk mode keeps the eye on the walkable floor, 0.4 m from walls. Fly mode deliberately passes through geometry, and the near plane is 0.1 m. |
| A proper whole-dungeon 2D map | M opens a top-down developer map: it fits the dungeon, zooms with the wheel and pans by dragging. It shows room outlines in purpose colours, routes, encounters, elevation shading, drops, room names and ids, and the camera. Clicking a room flies the 3D camera there. It is rebuilt on seed switch and reload. |
| Better distant visibility during overhead inspection | Cause found: linear distance fog from 18–75 m, not clipping or culling. I toggles inspection lighting (neutral white ambient light, fog pushed out of range). It affects the viewer only, and the atmospheric mode is unchanged. |

PLAYER: Useful improvements overall nothing made wrong.

## Outstanding questions (not yet validated)

- **How different do layouts feel to navigate?** The audit in
  BRANCH_PURPOSE_0_8_1.md shows they differ structurally, with 10 distinct
  route shapes per ten seeds. PLAYER: Interesting in concept, where the map is not just muscle memory memorized of which turns to make, a nice change to the core loop.
- **Do side branches feel worthwhile?** Not tested with a player. The
  structural audit suggests many don't yet: empty dead ends, and alternates
  with nothing on them. PLAYER: Depends on how many runs of the dungeon it has been, first few are nice to test how they branch and feel, after 5 feels unnecessary to explore. 
- **Are optional encounters easy to discover?** Structurally they are always
  one doorway off the main route, but nothing marks them from outside. PLAYER: Easy to discover, not confusing or chance of getting lost.
- **Do the crypt and volcanic environments feel different?** PLAYER: They feel slightly different, as more assets come into play the difference will probably be more noticeable
- **Does the environment remain readable during combat?** PLAYER: The environment does not feel difficult to manage during combat, again more assets introduced will adjust how positioning can be affected more

## Suggested next session

1. **Speed bar and camera feel.** Walk and fly through one seed of each kit
   with the new speed bar and smoothing, and note whether they feel right.
2. **The 2D map.** Use M on three seeds per kit. Note whether the purpose
   colours make each branch's role obvious, and whether clicking a room lands
   the camera where expected.
3. **Overhead visibility.** Fly overhead with I on and off. Note whether
   inspection lighting is enough for whole-dungeon inspection.
4. **The open questions.** Play one generated dungeon per kit and answer the
   questions above, especially whether you noticed and took the optional
   chambers and the alternate route.
