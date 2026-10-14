//! Per-actor combat metrics and run summaries.

use crate::sim::{Actor, ActorId, RunOutcome, Sim, Team};
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct ActorMetrics {
    pub name: String,
    pub damage_done: f32,
    pub healing_done: f32,
    pub overhealing: f32,
    pub damage_taken: f32,
    pub deaths: u32,
    pub casts: u32,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Metrics {
    pub actors: Vec<ActorMetrics>,
    pub ring_hits: u32,
}

impl Metrics {
    pub fn new(actors: &[Actor]) -> Self {
        Metrics {
            actors: actors
                .iter()
                .map(|a| ActorMetrics {
                    name: a.name.clone(),
                    ..Default::default()
                })
                .collect(),
            ring_hits: 0,
        }
    }

    pub fn actor_mut(&mut self, id: ActorId) -> &mut ActorMetrics {
        &mut self.actors[id]
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MemberSummary {
    pub name: String,
    pub class: String,
    pub damage_done: f32,
    pub dps: f32,
    pub healing_done: f32,
    pub hps: f32,
    pub overhealing: f32,
    pub damage_taken: f32,
    pub deaths: u32,
    pub casts: u32,
    pub alive_at_end: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RunSummary {
    pub seed: u64,
    pub outcome: RunOutcome,
    pub duration: f32,
    pub encounters_cleared: usize,
    pub encounters_total: usize,
    pub encounter_resets: u32,
    pub boss_pulls: u32,
    pub phase2_at: Option<f32>,
    pub ring_casts: u32,
    pub ring_hits: u32,
    pub party_deaths: u32,
    pub party_damage: f32,
    pub party_healing: f32,
    pub event_count: usize,
    /// FNV-1a hash over the formatted event log, for determinism checks.
    pub log_hash: u64,
    pub members: Vec<MemberSummary>,
}

pub fn fnv1a(hash: u64, bytes: &[u8]) -> u64 {
    let mut h = hash;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

impl RunSummary {
    pub fn from_sim(sim: &Sim) -> Self {
        use crate::events::EventKind;
        let duration = sim.time.max(crate::sim::DT);
        let mut log_hash = 0xCBF2_9CE4_8422_2325;
        let mut resets = 0;
        let mut rings = 0;
        for e in &sim.events {
            log_hash = fnv1a(log_hash, sim.describe(e).as_bytes());
            log_hash = fnv1a(log_hash, b"\n");
            match e.kind {
                EventKind::EncounterReset { .. } => resets += 1,
                EventKind::TelegraphStart { .. } => rings += 1,
                _ => {}
            }
        }
        let members: Vec<MemberSummary> = sim
            .party
            .iter()
            .map(|&p| {
                let m = &sim.metrics.actors[p];
                let a = &sim.actors[p];
                MemberSummary {
                    name: a.name.clone(),
                    class: a.class.map(|c| c.label()).unwrap_or("?").to_string(),
                    damage_done: m.damage_done,
                    dps: m.damage_done / duration,
                    healing_done: m.healing_done,
                    hps: m.healing_done / duration,
                    overhealing: m.overhealing,
                    damage_taken: m.damage_taken,
                    deaths: m.deaths,
                    casts: m.casts,
                    alive_at_end: a.alive,
                }
            })
            .collect();
        RunSummary {
            seed: sim.seed,
            outcome: sim.outcome,
            duration: sim.time,
            encounters_cleared: sim.encounters.iter().filter(|e| e.cleared).count(),
            encounters_total: sim.encounters.len(),
            encounter_resets: resets,
            boss_pulls: sim.encounters[sim.content.boss_encounter()].pulls,
            phase2_at: sim.phase2_at,
            ring_casts: rings,
            ring_hits: sim.metrics.ring_hits,
            party_deaths: members.iter().map(|m| m.deaths).sum(),
            party_damage: sim
                .actors
                .iter()
                .filter(|a| a.team == Team::Party)
                .map(|a| sim.metrics.actors[a.id].damage_done)
                .sum(),
            party_healing: members.iter().map(|m| m.healing_done).sum(),
            event_count: sim.events.len(),
            log_hash,
            members,
        }
    }

    pub fn format_table(&self) -> String {
        let mut s = format!(
            "seed {:>6} | {:<9} | {:>6.1}s | cleared {}/{} | resets {} | deaths {} | phase2 {} | rings {} (hits {}) | hash {:016x}\n",
            self.seed,
            format!("{:?}", self.outcome),
            self.duration,
            self.encounters_cleared,
            self.encounters_total,
            self.encounter_resets,
            self.party_deaths,
            self.phase2_at
                .map(|t| format!("{t:.1}s"))
                .unwrap_or_else(|| "-".into()),
            self.ring_casts,
            self.ring_hits,
            self.log_hash
        );
        for m in &self.members {
            s.push_str(&format!(
                "    {:<10} {:<9} dmg {:>7.0} ({:>5.1} dps)  heal {:>7.0} ({:>5.1} hps)  taken {:>7.0}  deaths {}\n",
                m.name, m.class, m.damage_done, m.dps, m.healing_done, m.hps, m.damage_taken, m.deaths
            ));
        }
        s
    }
}
