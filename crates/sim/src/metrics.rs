//! Per-actor combat metrics and run summaries.

use crate::sim::{Actor, ActorId, RunOutcome, Sim, Team};
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct ActorMetrics {
    pub name: String,
    pub damage_done: f32,
    pub healing_done: f32,
    pub overhealing: f32,
    pub absorb_done: f32,
    pub damage_taken: f32,
    /// Damage from rings, hazards and frontal cleaves (non-tanks).
    pub avoidable_taken: f32,
    pub deaths: u32,
    pub casts: u32,
    pub resource_spent: f32,
    pub interrupts_attempted: u32,
    pub interrupts: u32,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Metrics {
    pub actors: Vec<ActorMetrics>,
    pub ring_hits: u32,
    /// Interruptible enemy casts started / interrupted / completed.
    pub interruptible_casts: u32,
    pub interrupted_casts: u32,
    pub completed_interruptible: u32,
    /// An enemy switched to a non-tank while a tank was alive (outside taunts).
    pub threat_failures: u32,
    pub cc_applied: u32,
    pub cc_broken: u32,
    pub dispels: u32,
    /// Packs pulled while another was still fighting (0.9).
    pub overpulls: u32,
}

impl Metrics {
    pub fn new(actors: &[Actor]) -> Self {
        let mut m = Metrics::default();
        for a in actors {
            m.push_actor(a);
        }
        m
    }

    pub fn push_actor(&mut self, a: &Actor) {
        self.actors.push(ActorMetrics {
            name: a.name.clone(),
            ..Default::default()
        });
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
    pub absorb_done: f32,
    pub damage_taken: f32,
    pub avoidable_taken: f32,
    pub deaths: u32,
    pub casts: u32,
    pub resource_spent: f32,
    pub interrupts: u32,
    pub alive_at_end: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RunSummary {
    pub seed: u64,
    pub dungeon: String,
    pub difficulty: String,
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
    pub overheal_pct: f32,
    pub avoidable_damage: f32,
    pub interruptible_casts: u32,
    pub interrupted_casts: u32,
    pub completed_interruptible: u32,
    pub threat_failures: u32,
    pub cc_applied: u32,
    pub cc_broken: u32,
    pub dispels: u32,
    /// Effective healing per point of healer mana spent.
    pub heal_per_mana: f32,
    pub enemies_killed: u32,
    /// Party navigation quality: seconds stuck, seconds with an unreachable
    /// goal, and dither events (all summed over party members).
    pub nav_stuck_seconds: f32,
    pub nav_unreachable_seconds: f32,
    pub nav_dithers: u32,
    /// Packs pulled while another was still fighting (0.9).
    pub overpulls: u32,
    /// Treasure caches opened (0.9).
    pub caches_opened: usize,
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
        let duration = sim.ended_at.unwrap_or(sim.time).max(crate::sim::DT);
        let mut log_hash = 0xCBF2_9CE4_8422_2325;
        let mut resets = 0;
        let mut rings = 0;
        let mut kills = 0;
        for e in &sim.events {
            log_hash = fnv1a(log_hash, sim.describe(e).as_bytes());
            log_hash = fnv1a(log_hash, b"\n");
            match e.kind {
                EventKind::EncounterReset { .. } => resets += 1,
                EventKind::TelegraphStart { .. } => rings += 1,
                EventKind::Death { actor } if sim.actors[actor].is_enemy() => kills += 1,
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
                    absorb_done: m.absorb_done,
                    damage_taken: m.damage_taken,
                    avoidable_taken: m.avoidable_taken,
                    deaths: m.deaths,
                    casts: m.casts,
                    resource_spent: m.resource_spent,
                    interrupts: m.interrupts,
                    alive_at_end: a.alive,
                }
            })
            .collect();
        let healing: f32 = members.iter().map(|m| m.healing_done).sum();
        let overheal: f32 = members.iter().map(|m| m.overhealing).sum();
        let healer_mana: f32 = sim
            .party
            .iter()
            .filter(|&&p| sim.actors[p].role() == Some(crate::class::Role::Healer))
            .map(|&p| sim.metrics.actors[p].resource_spent)
            .sum();
        let healer_healing: f32 = sim
            .party
            .iter()
            .filter(|&&p| sim.actors[p].role() == Some(crate::class::Role::Healer))
            .map(|&p| sim.metrics.actors[p].healing_done + sim.metrics.actors[p].absorb_done)
            .sum();
        RunSummary {
            seed: sim.seed,
            dungeon: sim.content.dungeon.id.clone(),
            difficulty: sim.difficulty.id.clone(),
            outcome: sim.outcome,
            duration,
            encounters_cleared: sim.encounters.iter().filter(|e| e.cleared).count(),
            encounters_total: sim.encounters.len(),
            encounter_resets: resets,
            boss_pulls: sim.encounters[sim.content.final_encounter()].pulls,
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
            party_healing: healing,
            overheal_pct: if healing + overheal > 0.0 {
                100.0 * overheal / (healing + overheal)
            } else {
                0.0
            },
            avoidable_damage: members.iter().map(|m| m.avoidable_taken).sum(),
            interruptible_casts: sim.metrics.interruptible_casts,
            interrupted_casts: sim.metrics.interrupted_casts,
            completed_interruptible: sim.metrics.completed_interruptible,
            threat_failures: sim.metrics.threat_failures,
            cc_applied: sim.metrics.cc_applied,
            cc_broken: sim.metrics.cc_broken,
            dispels: sim.metrics.dispels,
            heal_per_mana: if healer_mana > 0.0 {
                healer_healing / healer_mana
            } else {
                0.0
            },
            enemies_killed: kills,
            nav_stuck_seconds: sim
                .party
                .iter()
                .map(|&p| sim.actors[p].nav.stuck_time)
                .sum(),
            nav_unreachable_seconds: sim
                .party
                .iter()
                .map(|&p| sim.actors[p].nav.unreachable_time)
                .sum(),
            nav_dithers: sim.party.iter().map(|&p| sim.actors[p].nav.dithers).sum(),
            overpulls: sim.metrics.overpulls,
            caches_opened: sim.caches.iter().filter(|c| c.opened_at.is_some()).count(),
            event_count: sim.events.len(),
            log_hash,
            members,
        }
    }

    pub fn format_table(&self) -> String {
        let mut s = format!(
            "seed {:>6} | {:<9} | {:>6.1}s | cleared {}/{} | resets {} | deaths {} | phase2 {} | rings {} (hits {}) | interrupts {}/{} | threat fails {} | avoidable {:.0} | hash {:016x}\n",
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
            self.interrupted_casts,
            self.interruptible_casts,
            self.threat_failures,
            self.avoidable_damage,
            self.log_hash
        );
        for m in &self.members {
            s.push_str(&format!(
                "    {:<10} {:<9} dmg {:>7.0} ({:>5.1} dps)  heal {:>7.0} ({:>5.1} hps)  taken {:>7.0} (avoidable {:>5.0})  kicks {}  deaths {}\n",
                m.name,
                m.class,
                m.damage_done,
                m.dps,
                m.healing_done,
                m.hps,
                m.damage_taken,
                m.avoidable_taken,
                m.interrupts,
                m.deaths
            ));
        }
        s
    }
}
