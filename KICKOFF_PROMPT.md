You are the lead engineer for Dungeon Sandbox, an original Rust + Bevy 0.20 third-person MMORPG-style dungeon prototype.

Read CLAUDE.md, SPEC.md, TASKS.md, REFERENCES.md, and PROGRESS.md before starting. Study the relevant current Bevy 0.20 APIs. Consult AzerothCore/Playerbots narrowly for behavioral references (threat, combat, party AI and instance lifecycle); do not directly copy or translate source code or import WoW assets.

Your assignment is to IMPLEMENT, not merely propose, the smallest playable vertical slice described in SPEC.md. Work through milestones M0–M5 sequentially, prioritizing a running game over polish. Use primitives for visuals and make reasonable design decisions without stopping to ask me about every detail.

After each milestone, run the available cargo formatting, compilation, test and lint commands, fix problems, and record verification evidence in PROGRESS.md. Implement a deterministic headless combat test runner so you can validate behavior without a graphical environment. If runtime or tooling prevents visual verification, document that truthfully and provide a concise manual smoke-test checklist.

Continue autonomously through the next unblocked milestone, within your permissions and session limits. Do not hide failed tests, claim a feature works without checking, or bypass security prompts. Do not install arbitrary tools or modify files outside this project. Where scope threatens the functional loop, simplify art/UI/content rather than sacrificing the core simulation or playability.

The finish line is: a human can launch a 3D game, move a character around a hub, assemble a party with four autonomous companions, enter an instance, engage mobs and a two-phase boss, receive persistent rewards, and adjust encounter parameters in configuration. Report exactly which parts you actually verified and which require human playtesting.
