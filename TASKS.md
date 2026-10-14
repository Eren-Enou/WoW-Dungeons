# Milestones and verification

## M0 — Bootstrap and architecture
- Create Rust workspace and Bevy 0.20 app with pinned dependencies.
- Add basic 3D camera, lighting, floor and controllable actor.
- Add developer README, commands and small smoke tests.
- Verify project builds before adding combat.

## M1 — Combat core
- Implement entity attributes, abilities, targeting, damage, healing, cooldowns and threat.
- Add unit tests for threat shifts, health bounds, targeting, cooldown and death.
- Expose combat metrics and event log.

## M2 — AI party
- Spawn four role-based autonomous companions.
- Implement follow, engage, tank threat maintenance, healing, and DPS strategies.
- Validate 5-character combat against an enemy pack in headless mode.

## M3 — Instance and boss
- Build small original dungeon with three trash encounters and one two-phase boss.
- Implement death/wipe, victory, spawn/reset, encounter configuration, and boss tell.
- Test boss threshold and encounter resets.

## M4 — Hub and progression
- Add hub-to-dungeon transitions, party setup, rewards and local save/load.
- Verify at least one reward survives restart.

## M5 — Sandbox adjustments and QA
- Add editable dungeon config (HP multiplier + boss timer).
- Seeded batch simulation runner, summary statistics, validation for bad config.
- Run all checks; user-facing smoke test; screenshot if environment allows.
- Make a final progress report with verified vs unverified functionality.

At the end of each milestone: check build, run tests, update PROGRESS.md, and commit if repo is configured and safe to do so. Do not mark milestones complete on inference.
