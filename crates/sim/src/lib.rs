//! Dungeon Sandbox simulation: deterministic, renderer-agnostic combat, AI,
//! encounters, content loading and persistence. Used by both the Bevy game and
//! the headless batch runner.

pub mod ai;
pub mod class;
pub mod content;
pub mod events;
pub mod math;
pub mod metrics;
pub mod nav;
pub mod profile;
pub mod rng;
pub mod runner;
pub mod sim;

#[cfg(test)]
mod tests;

pub use class::{AbilityId, ClassId, Role};
pub use content::{Content, ContentError};
pub use events::{CastError, Event, EventKind};
pub use math::V2;
pub use metrics::RunSummary;
pub use profile::Profile;
pub use sim::{ActorId, DT, EnemyMode, PartyMemberSpec, RunOutcome, Sim, Team};
