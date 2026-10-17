//! Headless run helpers shared by the CLI runner, benchmarks and tests.

use crate::ai::AiProfile;
use crate::class::{ClassId, Role};
use crate::content::Content;
use crate::metrics::RunSummary;
use crate::profile::Profile;
use crate::sim::{PartyMemberSpec, RunOptions, RunOutcome, Sim};

/// Default sim-time limit for one headless run.
pub const DEFAULT_MAX_SECONDS: f32 = 1500.0;

/// A balanced all-AI party built around `player_class` at `level`.
pub fn default_party(player_class: ClassId) -> Vec<PartyMemberSpec> {
    party_at_level(player_class, 1, AiProfile::Balanced)
}

pub fn party_at_level(
    player_class: ClassId,
    level: u32,
    profile: AiProfile,
) -> Vec<PartyMemberSpec> {
    let mut p = Profile::new_default();
    p.player_class = player_class;
    p.auto_fill_party();
    let mut specs = p.party_specs(false);
    for s in specs.iter_mut() {
        s.level = level;
        s.profile = profile;
    }
    specs
}

/// Set every member's dungeon familiarity (0..1).
pub fn with_familiarity(mut party: Vec<PartyMemberSpec>, familiarity: f32) -> Vec<PartyMemberSpec> {
    for s in party.iter_mut() {
        s.familiarity = familiarity;
    }
    party
}

/// A raid-style group of `size` members: 1 tank (5), 2 (up to 15) or 3 tanks, 1 healer
/// per 4 (min 1), the rest alternating ranged and melee damage dealers.
pub fn raid_party(size: usize, level: u32, profile: AiProfile) -> Vec<PartyMemberSpec> {
    let size = size.max(1);
    let tanks = match size {
        0..=5 => 1,
        6..=15 => 2,
        _ => 3,
    }
    .min(size);
    let healers = (size / 4).max(1).min(size - tanks);
    let names = [
        "Brannoc", "Ysolde", "Thessaly", "Orrin", "Kael", "Dovic", "Mirelle", "Senna", "Hale",
        "Ivra", "Tamsin", "Corvo", "Nell", "Bastian", "Rook", "Odessa", "Pell", "Wren", "Garrick",
        "Lio", "Maren", "Sable", "Teodor", "Ura", "Vesna",
    ];
    let mut specs = Vec::new();
    for i in 0..size {
        let class = if i < tanks {
            ClassId::Warden
        } else if i < tanks + healers {
            ClassId::Mender
        } else if (i - tanks - healers).is_multiple_of(2) {
            ClassId::Arcanist
        } else {
            ClassId::Striker
        };
        let mut s = PartyMemberSpec::new(names[i % names.len()], class);
        if i >= names.len() {
            s.name = format!("{} {}", names[i % names.len()], i / names.len() + 1);
        }
        s.level = level;
        s.profile = profile;
        specs.push(s);
    }
    // Put a damage dealer in slot 0 ("player" slot) when possible, like a real group.
    if let Some(i) = specs.iter().position(|s| s.class.role().is_dps()) {
        specs.swap(0, i);
    }
    specs
}

/// Run one full dungeon with every party member AI-controlled and the tank leading.
pub fn run_headless(
    content: &Content,
    party: &[PartyMemberSpec],
    seed: u64,
    max_seconds: f32,
) -> (Sim, RunSummary) {
    run_with(content, party, seed, max_seconds, "normal")
}

pub fn run_with(
    content: &Content,
    party: &[PartyMemberSpec],
    seed: u64,
    max_seconds: f32,
    difficulty: &str,
) -> (Sim, RunSummary) {
    run_with_options(
        content,
        party,
        seed,
        max_seconds,
        RunOptions {
            difficulty: difficulty.to_string(),
            autopilot: true,
            ..Default::default()
        },
    )
}

/// `run_with`, with every option given (comparisons such as
/// `slot_follow_only`).
pub fn run_with_options(
    content: &Content,
    party: &[PartyMemberSpec],
    seed: u64,
    max_seconds: f32,
    options: RunOptions,
) -> (Sim, RunSummary) {
    let mut sim = Sim::with_options(content, party, seed, options);
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
    pub mean_victory_duration: f32,
    pub mean_deaths: f32,
    pub mean_party_dps: f32,
    pub mean_ring_hits: f32,
    pub mean_avoidable: f32,
    pub interrupt_rate: f32,
    pub mean_threat_failures: f32,
    pub mean_overheal_pct: f32,
    pub mean_heal_per_mana: f32,
    pub mean_stuck_seconds: f32,
    pub mean_unreachable_seconds: f32,
    pub mean_dithers: f32,
}

pub fn batch_stats(summaries: &[RunSummary]) -> BatchStats {
    let n = summaries.len().max(1) as f32;
    let count = |o: RunOutcome| summaries.iter().filter(|s| s.outcome == o).count();
    let wins: Vec<&RunSummary> = summaries
        .iter()
        .filter(|s| s.outcome == RunOutcome::Victory)
        .collect();
    let mean = |f: &dyn Fn(&RunSummary) -> f32| summaries.iter().map(f).sum::<f32>() / n;
    let casts: u32 = summaries.iter().map(|s| s.interruptible_casts).sum();
    let kicked: u32 = summaries.iter().map(|s| s.interrupted_casts).sum();
    BatchStats {
        runs: summaries.len(),
        victories: wins.len(),
        wipes: count(RunOutcome::Wipe),
        timeouts: count(RunOutcome::Timeout),
        mean_duration: mean(&|s| s.duration),
        mean_victory_duration: if wins.is_empty() {
            0.0
        } else {
            wins.iter().map(|s| s.duration).sum::<f32>() / wins.len() as f32
        },
        mean_deaths: mean(&|s| s.party_deaths as f32),
        mean_party_dps: mean(&|s| s.party_damage / s.duration.max(0.05)),
        mean_ring_hits: mean(&|s| s.ring_hits as f32),
        mean_avoidable: mean(&|s| s.avoidable_damage),
        interrupt_rate: if casts > 0 {
            kicked as f32 / casts as f32
        } else {
            0.0
        },
        mean_threat_failures: mean(&|s| s.threat_failures as f32),
        mean_overheal_pct: mean(&|s| s.overheal_pct),
        mean_heal_per_mana: mean(&|s| s.heal_per_mana),
        mean_stuck_seconds: mean(&|s| s.nav_stuck_seconds),
        mean_unreachable_seconds: mean(&|s| s.nav_unreachable_seconds),
        mean_dithers: mean(&|s| s.nav_dithers as f32),
    }
}

impl BatchStats {
    pub fn one_line(&self) -> String {
        format!(
            "runs {} | wins {} ({:.0}%) | wipes {} | timeouts {} | mean {:.0}s (wins {:.0}s) | deaths {:.2} | dps {:.0} | avoidable {:.0} | ring hits {:.2} | interrupts {:.0}% | threat fails {:.1} | overheal {:.0}% | heal/mana {:.2} | nav stuck {:.1}s unreachable {:.1}s dithers {:.1}",
            self.runs,
            self.victories,
            100.0 * self.victories as f32 / self.runs.max(1) as f32,
            self.wipes,
            self.timeouts,
            self.mean_duration,
            self.mean_victory_duration,
            self.mean_deaths,
            self.mean_party_dps,
            self.mean_avoidable,
            self.mean_ring_hits,
            100.0 * self.interrupt_rate,
            self.mean_threat_failures,
            self.mean_overheal_pct,
            self.mean_heal_per_mana,
            self.mean_stuck_seconds,
            self.mean_unreachable_seconds,
            self.mean_dithers
        )
    }
}

/// Count the tanks in a party spec list (used by raid tooling).
pub fn tanks_in(party: &[PartyMemberSpec]) -> usize {
    party
        .iter()
        .filter(|s| s.class.role() == Role::Tank)
        .count()
}
