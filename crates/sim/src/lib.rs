//! Dungeon Sandbox simulation: deterministic, renderer-agnostic combat, AI,
//! encounters, content loading and persistence. Used by both the Bevy game and
//! the headless batch runner.

pub mod ai;
pub mod class;
pub mod content;
pub mod describe;
pub mod events;
pub mod items;
pub mod lab;
pub mod math;
pub mod metrics;
pub mod nav;
pub mod poly;
pub mod profile;
pub mod rng;
pub mod runner;
pub mod sim;
pub mod stats;

#[cfg(test)]
mod tests;

pub use ai::AiProfile;
pub use class::{AbilityId, ClassId, Role};
pub use content::{Content, ContentError, ContentLibrary};
pub use events::{CastError, Event, EventKind, Label};
pub use math::V2;
pub use metrics::RunSummary;
pub use profile::Profile;
pub use sim::{
    ActorId, DT, EnemyMode, PartyMemberSpec, RouteStep, RunOptions, RunOutcome, Sim, Team,
};
