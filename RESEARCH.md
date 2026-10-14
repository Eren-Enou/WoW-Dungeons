# Research log

## What was consulted

| Source | License | How it was used |
|--------|---------|-----------------|
| Bevy 0.20.0 crate sources and bundled examples (`~/.cargo/registry/src/.../bevy-0.20.0/examples`, `_release-content/migration-guides`) | MIT OR Apache-2.0 | API reference for the pinned version: spawning meshes/materials, lights, UI `Node`/`Text`, `States`/`DespawnOnExit`, `AccumulatedMouseMotion`, `Camera::viewport_to_world`, gizmos, `Screenshot` + `save_to_disk`, the `set_if_neq → set_if_different` rename, `FontSize::Px`. No example code was copied beyond ordinary API usage. |

**Not consulted in this session:** AzerothCore, mod-playerbots and TrinityCore
source code (REFERENCES.md items 5–8) were *not* opened. No GPL or proprietary
code, data or assets were read, copied or translated into this repository.

## Observations vs. design inventions

The mechanics below are modelled on widely known, publicly documented behaviour
of classic tab-target MMO dungeons (general genre knowledge, not any codebase):

* **Observed genre conventions (re-implemented from scratch):** per-enemy threat
  tables; enemies switching targets only when another player exceeds the current
  target's threat by 10% in melee range / 30% outside it; taunts that match the
  top threat and force the target briefly; healing generating half its amount as
  threat split among engaged enemies; packs aggroing together; enemies evading
  (immune, walking home, healing to full) when they lose all targets or are
  dragged too far; tank/healer/DPS role split; boss phase transitions at HP
  thresholds; ground-telegraphed area attacks; hard enrage timers; global
  cooldown.
* **Design inventions for this prototype:** all class, ability, enemy, boss,
  item and place names; every number; the 30 m "join the pull at zero threat"
  rule; the rectangle-graph navigation; the companion AI utility scores, the
  0.5 s telegraph reaction time, the leader autopilot; the healer's
  overheal-cancel rule; the rewards (Vault Marks + one weighted loot roll);
  equipment power/stamina scaling.

If AzerothCore/Playerbots are studied later, record here which files were read,
their exact license (these projects are copyleft, GPL/AGPL family — verify the
specific version in each repository first), and keep
the separation: behavioural observations only, no code transcription.
