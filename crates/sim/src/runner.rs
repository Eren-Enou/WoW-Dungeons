//! Headless run helpers shared by the CLI runner and tests.

use crate::class::ClassId;
use crate::content::Content;
use crate::metrics::RunSummary;
use crate::profile::Profile;
use crate::sim::{PartyMemberSpec, RunOutcome, Sim};

/// Default sim-time limit for one headless run.
pub const DEFAULT_MAX_SECONDS: f32 = 1200.0;

/// A balanced all-AI party built around `player_class` (the "player" slot is AI-driven).
pub fn default_party(player_class: ClassId) -> Vec<PartyMemberSpec> {
    let mut profile = Profile::new_default();
    profile.player_class = player_class;
    profile.auto_fill_party();
    profile.party_specs(false)
}

/// Run one full dungeon with every party member AI-controlled and the tank leading.
pub fn run_headless(
    content: &Content,
    party: &[PartyMemberSpec],
    seed: u64,
    max_seconds: f32,
) -> (Sim, RunSummary) {
    let mut sim = Sim::new(content, party, seed);
    sim.autopilot = true;
    sim.run_to_end(max_seconds);
    let summary = RunSummary::from_sim(&sim);
    (sim, summary)
}

#[derive(Debug, Clone)]
pub struct BatchStats {
    pub runs: usize,
    pub victories: usize,
    pub wipes: usize,
    pub timeouts: usize,
    pub mean_duration: f32,
    pub mean_deaths: f32,
    pub mean_party_dps: f32,
    pub mean_ring_hits: f32,
}

pub fn batch_stats(summaries: &[RunSummary]) -> BatchStats {
    let n = summaries.len().max(1) as f32;
    let count = |o: RunOutcome| summaries.iter().filter(|s| s.outcome == o).count();
    BatchStats {
        runs: summaries.len(),
        victories: count(RunOutcome::Victory),
        wipes: count(RunOutcome::Wipe),
        timeouts: count(RunOutcome::Timeout),
        mean_duration: summaries.iter().map(|s| s.duration).sum::<f32>() / n,
        mean_deaths: summaries.iter().map(|s| s.party_deaths as f32).sum::<f32>() / n,
        mean_party_dps: summaries
            .iter()
            .map(|s| s.party_damage / s.duration.max(0.05))
            .sum::<f32>()
            / n,
        mean_ring_hits: summaries.iter().map(|s| s.ring_hits as f32).sum::<f32>() / n,
    }
}
