# Dungeon Sandbox — prototype 0.1 specification

## Product vision
A player-controlled third-person character runs instanced MMORPG-style dungeons with four autonomous, persistent NPC adventurers. Between runs, the party gathers in a small hub to configure roles, change equipment and select an expedition. The developer can customize dungeon encounters without recompiling the engine.

## The player experience
1. Start in a compact 3D hub with a party interface and dungeon portal/board.
2. Recruit a five-person party including the player. Companions have persistent identities, roles and simple equipment.
3. Enter one original, short dungeon made of 3 trash encounters and 1 two-phase boss.
4. Move using WASD, rotate camera with mouse, select enemies and use three hotkeyed abilities.
5. The tank manages threat, the healer prioritizes injured allies and mana, and DPS prioritizes targets while avoiding danger.
6. Win or wipe, inspect combat log/results, receive loot on a win, and return to the hub with persistent rewards.
7. Change enemy HP multiplier and a boss timer using a content config file, restart/reload as appropriate, and verify behavior changes.

## Functional mechanics
- Explicit combat rules for health, resource pools, target validity, ability range, cast/cooldown/global cooldown (if used), damage, healing, death, and combat resets.
- Threat table per enemy. Healing threat may be simplified but must be specified and tested.
- NPC behavior implemented with interpretable rules/utility scoring, not remote model calls.
- Basic pathfinding/navigation for companion following and enemy engagement; document shortcuts.
- Boss enters second phase at 50% HP and uses a clear telegraphed ability.
- Combat events are inspectable in a readable debug log or overlay.
- Seeded headless battle runner outputs victory/wipe and summary metrics.

## Acceptance tests
A. `cargo run` (or documented runner command) launches a playable window on the target OS.
B. Player can move, select targets and use three meaningful abilities.
C. Four NPCs join the player's instance and autonomously execute their roles.
D. The party can clear at least one enemy pack and one boss, or wipe through valid combat mechanics.
E. Boss phase trigger fires once at intended threshold, with a visible tell.
F. Completing an instance awards an item/resource that remains after restart.
G. Changing config impacts a subsequent run and invalid config produces a useful error.
H. Headless mode completes seeded repeated encounters reproducibly with nonempty metrics.
I. No build or test failures are knowingly left hidden in the final report.

## Out of scope
Any Blizzard-derived maps, models, textures, icons, sound or client data; multiplayer; raids; full class/talent trees; auction houses; open-world content; procedural infinite dungeons; production-quality UI.

## Unspecified design choices
Claude may choose an original fantasy art direction, simple class names, temporary ability formulas and the specific content schema. Record decisions in PROGRESS.md.
