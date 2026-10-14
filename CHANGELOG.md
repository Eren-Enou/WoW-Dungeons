# Changelog

## Unreleased (0.2 development)

### Phase A — playable experience
- Camera: collision against wall boxes (pull in fast, ease out), zoom range 4–32 m.
- Walls that hide the player from the camera fade to translucent; walls restored to 3.2 m.
- Targeting: Tab cycles engaged enemies first, then idle enemies in front of the camera
  (nearest first); Shift+Tab cycles backwards; T assists (targets your target's target).
- Nameplates for every actor (name, health bar, live cast bar; colour by state).
- Floating combat text: damage (crits enlarged), heals, interrupts, taunts, deaths, rallies.
- Procedural effects: spell projectiles, melee flashes, AoE/cleave rings, taunt rings,
  heal sparkles, buff shells, threat-drop puffs, phase/ring blasts.
- Pulsing ring under the selected target (red hostile / blue friendly).
- Danger vignette: pulsing red screen border and "MOVE OUT!" while standing in a telegraph.
- Removed the 3D health-bar meshes (replaced by nameplates).

## 0.1 — prototype
- Hub → dungeon → boss → rewards → hub loop, four classes, AI party, headless runner.
