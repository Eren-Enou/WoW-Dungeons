//! Dungeon Forge: build original dungeons from a blueprint (intent), a kit
//! (theme library) and a seed. Output is an ordinary dungeon TOML file that
//! the game and the headless runner load like any handwritten one.
//!
//! Pipeline: `layout` (topology on a grid) → `compose` (room and corridor
//! components → walkable areas) → `populate` (encounter recipes, loot) →
//! the game's loader → `check` (structure, encounter space) → `simtest`
//! (simulated party clears). See DUNGEON_FORGE_PLAN.md.

pub mod blueprint;
pub mod build;
pub mod check;
pub mod compose;
pub mod inspect;
pub mod kit;
pub mod layout;
pub mod map;
pub mod populate;
pub mod simtest;

#[cfg(test)]
mod tests;

pub use blueprint::Blueprint;
pub use build::{Generated, generate};
pub use kit::Kit;
