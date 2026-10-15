//! Gameplay validation: simulated AI parties clear the dungeon across a
//! matrix of group sizes, profiles, classes and difficulties. Anything that
//! looks wrong is written down as a reproducible failure record.

use dungeon_sim::class::ClassId;
use dungeon_sim::content::Content;
use dungeon_sim::metrics::RunSummary;
use dungeon_sim::runner::{BatchStats, batch_stats, party_at_level, raid_party, run_with};
use dungeon_sim::{AiProfile, RunOutcome, Sim};
use serde::Serialize;

/// Thresholds above which a run counts as a navigation failure.
pub const STUCK_LIMIT: f32 = 10.0;
pub const UNREACHABLE_LIMIT: f32 = 10.0;
/// Dithers per party member per minute.
pub const DITHER_RATE_LIMIT: f32 = 6.0;

#[derive(Debug, Clone, Serialize)]
pub struct Config {
    pub size: usize,
    pub class: String,
    pub profile: String,
    pub difficulty: String,
    pub level: u32,
}

impl Config {
    pub fn label(&self) -> String {
        format!(
            "{}p {} {} {} L{}",
            self.size, self.class, self.profile, self.difficulty, self.level
        )
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ActorState {
    pub name: String,
    pub pos: [f32; 3],
    pub surface: String,
    pub goal: Option<[f32; 3]>,
    pub stuck_seconds: f32,
    pub alive: bool,
}

/// Everything needed to reproduce and diagnose one bad run.
#[derive(Debug, Clone, Serialize)]
pub struct FailureRecord {
    pub dungeon: String,
    /// "blueprint <id> seed <n>" for generated dungeons (from the file header).
    pub generated_from: Option<String>,
    pub config: Config,
    pub sim_seed: u64,
    pub reasons: Vec<String>,
    pub outcome: String,
    pub cleared: usize,
    pub total: usize,
    pub stuck_at_encounter: Option<String>,
    pub duration: f32,
    pub nav_stuck: f32,
    pub nav_unreachable: f32,
    pub dithers: u32,
    pub party: Vec<ActorState>,
    pub reproduce: String,
}

pub struct ConfigResult {
    pub config: Config,
    pub stats: BatchStats,
    pub failures: Vec<FailureRecord>,
}

pub fn party_for(c: &Config) -> Vec<dungeon_sim::PartyMemberSpec> {
    let profile = AiProfile::parse(&c.profile).unwrap_or(AiProfile::Balanced);
    if c.size == 5 {
        let class = ClassId::ALL
            .into_iter()
            .find(|k| k.label().eq_ignore_ascii_case(&c.class))
            .unwrap_or(ClassId::Striker);
        party_at_level(class, c.level, profile)
    } else {
        raid_party(c.size, c.level, profile)
    }
}

fn reasons(s: &RunSummary, size: usize) -> Vec<String> {
    let mut r = Vec::new();
    match s.outcome {
        RunOutcome::Victory => {}
        RunOutcome::Wipe => r.push("wipe".into()),
        RunOutcome::Timeout => r.push("timeout".into()),
        RunOutcome::InProgress | RunOutcome::Abandoned => r.push("did not finish".into()),
    }
    if s.nav_stuck_seconds > STUCK_LIMIT {
        r.push(format!("stuck {:.0} s", s.nav_stuck_seconds));
    }
    if s.nav_unreachable_seconds > UNREACHABLE_LIMIT {
        r.push(format!(
            "unreachable goals {:.0} s",
            s.nav_unreachable_seconds
        ));
    }
    let rate = s.nav_dithers as f32 / size as f32 / (s.duration / 60.0).max(1.0);
    if rate > DITHER_RATE_LIMIT {
        r.push(format!("dithering {rate:.1}/member/min"));
    }
    r
}

fn party_state(sim: &Sim) -> Vec<ActorState> {
    sim.party
        .iter()
        .map(|&id| {
            let a = &sim.actors[id];
            ActorState {
                name: a.name.clone(),
                pos: [a.pos.x, a.y, a.pos.z],
                surface: sim.nav.surfaces[a.surface as usize].name().to_string(),
                goal: a.move_goal.map(|g| [g.x, g.y, g.z]),
                stuck_seconds: a.nav.stuck_time,
                alive: a.alive,
            }
        })
        .collect()
}

/// Parse "Blueprint 'x', seed N" from a generated file's header comment.
pub fn generated_from(text: &str) -> Option<String> {
    text.lines()
        .take(4)
        .find(|l| l.starts_with("# Blueprint '"))
        .map(|l| l.trim_start_matches("# ").trim_end_matches('.').to_string())
}

pub fn run_config(
    content: &Content,
    origin: Option<&str>,
    path_hint: &str,
    config: Config,
    runs: usize,
    first_seed: u64,
) -> ConfigResult {
    let party = party_for(&config);
    let mut summaries = Vec::new();
    let mut failures = Vec::new();
    for k in 0..runs {
        let seed = first_seed + k as u64;
        let (sim, s) = run_with(
            content,
            &party,
            seed,
            dungeon_sim::runner::DEFAULT_MAX_SECONDS,
            &config.difficulty,
        );
        let why = reasons(&s, config.size);
        if !why.is_empty() {
            let next = sim
                .encounters
                .iter()
                .find(|e| !e.cleared)
                .map(|e| e.id.clone());
            failures.push(FailureRecord {
                dungeon: content.dungeon.id.clone(),
                generated_from: origin.map(|o| o.to_string()),
                config: config.clone(),
                sim_seed: seed,
                reasons: why,
                outcome: format!("{:?}", s.outcome),
                cleared: s.encounters_cleared,
                total: s.encounters_total,
                stuck_at_encounter: next,
                duration: s.duration,
                nav_stuck: s.nav_stuck_seconds,
                nav_unreachable: s.nav_unreachable_seconds,
                dithers: s.nav_dithers,
                party: party_state(&sim),
                reproduce: format!(
                    "cargo run --release -p dungeon_sim --bin dungeon-headless -- --content {path_hint} --seed {seed} --runs 1 --party-size {} --class {} --profile {} --difficulty {} --level {}",
                    config.size,
                    config.class.to_ascii_lowercase(),
                    config.profile,
                    config.difficulty,
                    config.level
                ),
            });
        }
        summaries.push(s);
    }
    ConfigResult {
        config,
        stats: batch_stats(&summaries),
        failures,
    }
}
