# Dungeon Sandbox — Claude Code project instructions

## Mission
Develop a standalone playable Rust + Bevy 0.20 third-person dungeon sandbox inspired by the *mechanical feel* of classic tab-target MMORPG dungeons. This is an original implementation, not a WoW client, private server, or asset extractor.

## Priorities (in order)
1. A working executable that a human can play.
2. A complete hub -> recruit -> dungeon -> boss -> rewards -> hub loop.
3. Autonomous tank, healer and DPS agents.
4. Deterministic, testable combat simulation and data-driven encounters.
5. Visual polish only after the loop works.

## Scope limits
- One small hub and one original dungeon; primitive meshes and materials are acceptable.
- One player-controlled character plus four autonomous companions; initially tank/healer/2 DPS roles configurable according to player's role.
- One enemy archetype, one elite, one boss with two phases; expand only after first working loop.
- Third-person camera, WASD movement, target selection, 3 abilities, HP/mana, cooldowns, threat, healing, loot and persistent party data.
- No networking, accounts, payments, procedural large world, LLM calls for NPC decisions, original Blizzard assets, or exact retail parity.

## Engineering conventions
- Rust stable, pin Bevy to version 0.20.x and pin dependency versions with Cargo.lock. Confirm documented APIs against the pinned version.
- Organize simulation rules separately from rendering and UI; make simulation callable from a headless runner.
- Prefer safe Rust. No unsafe except with explicit reason and tests.
- Fixed simulation timestep; seeded random number generator for reproducibility.
- Data-driven dungeon and encounter configuration with schema validation and actionable error messages.
- Do not silently implement placeholder behavior while claiming feature completion.
- Use original names, maps, models, audio and texture assets. Use primitive stand-ins early.
- Do not import or translate GPL/proprietary source into this repository without explicit authorization and license review. Studying behavior is permitted; record all referenced material in RESEARCH.md.

## Mandatory iteration loop
For each milestone: implement the smallest coherent slice, run `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo test --workspace`, then `cargo clippy --workspace --all-targets -- -D warnings` when feasible. Fix failures, rerun checks, and update PROGRESS.md with what was actually verified. Manually launch the graphical game where supported; if the environment cannot verify graphics, explicitly report that limitation and give user smoke-test steps.

## Autonomy
Make reasonable design decisions independently. Don't block on minor cosmetic choices. Keep work within project folder. Do not run destructive commands, install untrusted scripts, change system settings, or request elevated privileges. Do not use broad permission bypass. When stuck, reduce scope to a working vertical slice, record the obstacle and continue with another unblocked milestone. Never claim tests passed unless run.

## Project state
Read SPEC.md, TASKS.md, REFERENCES.md and PROGRESS.md at the start of each session. Keep PROGRESS.md truthful and current. A task is complete only if its documented acceptance test passes.
