//! Seeded headless batch runner and AI evaluation tool.
//!
//! Examples:
//!   dungeon-headless --runs 20
//!   dungeon-headless --dungeon gloomroot_warrens --level 5 --runs 50
//!   dungeon-headless --compare-profiles --runs 40 --dungeon gloomroot_warrens --level 5
//!   dungeon-headless --party-size 25 --level 10 --runs 3 --bench

use dungeon_sim::ClassId;
use dungeon_sim::ai::AiProfile;
use dungeon_sim::content::{Content, ContentLibrary};
use dungeon_sim::metrics::RunSummary;
use dungeon_sim::runner::{
    DEFAULT_MAX_SECONDS, batch_stats, party_at_level, raid_party, with_familiarity,
};
use dungeon_sim::sim::PartyMemberSpec;
use std::collections::BTreeMap;
use std::process::ExitCode;
use std::time::Instant;

const USAGE: &str = "usage: dungeon-headless [--seed N] [--runs N] [--dungeon ID] [--content PATH] \
[--difficulty ID] [--class warden|mender|striker|arcanist] [--level N] \
[--profile conservative|balanced|aggressive|support|mechanic] [--party-size N] \
[--compare-profiles] [--bench] [--breakdown] [--familiarity 0..1] [--max-seconds S] [--log FILE] [--json] [--quiet] \
[--slot-follow (companions follow formation slots only, as before 0.9; for comparisons)]";

struct Args {
    seed: u64,
    runs: usize,
    dungeon: Option<String>,
    content: String,
    difficulty: String,
    class: ClassId,
    level: u32,
    profile: AiProfile,
    party_size: Option<usize>,
    compare_profiles: bool,
    bench: bool,
    max_seconds: f32,
    log: Option<String>,
    json: bool,
    quiet: bool,
    breakdown: bool,
    familiarity: f32,
    slot_follow: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        seed: 1,
        runs: 10,
        dungeon: None,
        content: ContentLibrary::default_path(),
        difficulty: "normal".into(),
        class: ClassId::Striker,
        level: 0,
        profile: AiProfile::Balanced,
        party_size: None,
        compare_profiles: false,
        bench: false,
        max_seconds: DEFAULT_MAX_SECONDS,
        log: None,
        json: false,
        quiet: false,
        breakdown: false,
        familiarity: 0.0,
        slot_follow: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut val = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "--seed" => args.seed = val("--seed")?.parse().map_err(|e| format!("--seed: {e}"))?,
            "--runs" => args.runs = val("--runs")?.parse().map_err(|e| format!("--runs: {e}"))?,
            "--level" => {
                args.level = val("--level")?
                    .parse()
                    .map_err(|e| format!("--level: {e}"))?
            }
            "--party-size" => {
                args.party_size = Some(
                    val("--party-size")?
                        .parse()
                        .map_err(|e| format!("--party-size: {e}"))?,
                )
            }
            "--max-seconds" => {
                args.max_seconds = val("--max-seconds")?
                    .parse()
                    .map_err(|e| format!("--max-seconds: {e}"))?
            }
            "--class" => {
                args.class = match val("--class")?.to_lowercase().as_str() {
                    "warden" => ClassId::Warden,
                    "mender" => ClassId::Mender,
                    "striker" => ClassId::Striker,
                    "arcanist" => ClassId::Arcanist,
                    other => return Err(format!("unknown class '{other}'\n{USAGE}")),
                }
            }
            "--profile" => {
                let v = val("--profile")?;
                args.profile =
                    AiProfile::parse(&v).ok_or(format!("unknown profile '{v}'\n{USAGE}"))?
            }
            "--dungeon" => args.dungeon = Some(val("--dungeon")?),
            "--content" => args.content = val("--content")?,
            "--difficulty" => args.difficulty = val("--difficulty")?,
            "--log" => args.log = Some(val("--log")?),
            "--json" => args.json = true,
            "--quiet" => args.quiet = true,
            "--breakdown" => args.breakdown = true,
            "--familiarity" => {
                args.familiarity = val("--familiarity")?
                    .parse()
                    .map_err(|e| format!("--familiarity: {e}"))?
            }
            "--compare-profiles" => args.compare_profiles = true,
            "--slot-follow" => args.slot_follow = true,
            "--bench" => args.bench = true,
            "-h" | "--help" => return Err(USAGE.into()),
            other => return Err(format!("unknown argument '{other}'\n{USAGE}")),
        }
    }
    if args.runs == 0 {
        return Err("--runs must be at least 1".into());
    }
    Ok(args)
}

fn build_party(args: &Args, content: &Content, profile: AiProfile) -> Vec<PartyMemberSpec> {
    let level = if args.level == 0 {
        content.dungeon.recommended_level
    } else {
        args.level
    };
    let party = match args.party_size {
        Some(n) if n != 5 => raid_party(n, level, profile),
        _ => party_at_level(args.class, level, profile),
    };
    with_familiarity(party, args.familiarity)
}

#[derive(Default)]
struct Breakdown {
    /// Damage taken by the party, by ability name: (total, hits on non-tanks).
    taken: BTreeMap<String, (f32, u32)>,
    /// Threat failures by enemy name.
    pulled_by: BTreeMap<String, u32>,
    deaths_by: BTreeMap<String, u32>,
}

impl Breakdown {
    fn add(&mut self, sim: &dungeon_sim::Sim) {
        use dungeon_sim::EventKind;
        let mut last_hit: Vec<Option<String>> = vec![None; sim.actors.len()];
        for e in &sim.events {
            match &e.kind {
                EventKind::Damage {
                    dst, what, amount, ..
                } if !sim.actors[*dst].is_enemy() => {
                    let entry = self.taken.entry(what.to_string()).or_default();
                    entry.0 += amount;
                    if !sim.is_tank(*dst) {
                        entry.1 += 1;
                    }
                    last_hit[*dst] = Some(what.to_string());
                }
                EventKind::TargetChange { enemy, target } if !sim.is_tank(*target) => {
                    *self
                        .pulled_by
                        .entry(sim.actors[*enemy].name.clone())
                        .or_default() += 1;
                }
                EventKind::Death { actor } if !sim.actors[*actor].is_enemy() => {
                    let k = last_hit[*actor].clone().unwrap_or_else(|| "?".into());
                    *self.deaths_by.entry(k).or_default() += 1;
                }
                _ => {}
            }
        }
    }

    fn print(&self, runs: usize) {
        let n = runs.max(1) as f32;
        let mut taken: Vec<_> = self.taken.iter().collect();
        taken.sort_by(|a, b| b.1.0.total_cmp(&a.1.0));
        println!("  damage taken per run by source (non-tank hits per run):");
        for (k, (v, hits)) in taken.iter().take(12) {
            println!("    {:<22} {:>8.0}  ({:.1})", k, v / n, *hits as f32 / n);
        }
        println!("  enemies turning on non-tanks per run:");
        let mut pulled: Vec<_> = self.pulled_by.iter().collect();
        pulled.sort_by(|a, b| b.1.cmp(a.1));
        for (k, v) in pulled.iter().take(8) {
            println!("    {:<28} {:>6.2}", k, **v as f32 / n);
        }
        println!("  party deaths by killing blow (total):");
        for (k, v) in &self.deaths_by {
            println!("    {:<22} {:>4}", k, v);
        }
    }
}

fn run_args(
    args: &Args,
    content: &Content,
    party: &[PartyMemberSpec],
    seed: u64,
) -> (dungeon_sim::Sim, RunSummary) {
    dungeon_sim::runner::run_with_options(
        content,
        party,
        seed,
        args.max_seconds,
        dungeon_sim::RunOptions {
            difficulty: args.difficulty.clone(),
            autopilot: true,
            slot_follow_only: args.slot_follow,
            ..Default::default()
        },
    )
}

fn run_batch(
    args: &Args,
    content: &Content,
    party: &[PartyMemberSpec],
    print: bool,
) -> (Vec<RunSummary>, f64, u64) {
    let mut breakdown = Breakdown::default();
    let mut summaries = Vec::new();
    let start = Instant::now();
    let mut ticks = 0u64;
    for i in 0..args.runs {
        let seed = args.seed + i as u64;
        let (sim, summary) = run_args(args, content, party, seed);
        ticks += sim.tick;
        if args.breakdown {
            breakdown.add(&sim);
        }
        if i == 0
            && let Some(path) = &args.log
        {
            let text: String = sim.events.iter().map(|e| sim.describe(e) + "\n").collect();
            if let Err(e) = std::fs::write(path, text) {
                eprintln!("could not write log {path}: {e}");
            }
        }
        if print {
            if args.json {
                println!("{}", serde_json::to_string(&summary).expect("serializable"));
            } else if !args.quiet {
                print!("{}", summary.format_table());
            }
        }
        summaries.push(summary);
    }
    if args.breakdown {
        breakdown.print(args.runs);
    }
    (summaries, start.elapsed().as_secs_f64(), ticks)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let lib = ContentLibrary::load(&args.content);
    if !lib.errors.is_empty() {
        for e in &lib.errors {
            eprintln!("error: {e}");
        }
        return ExitCode::from(1);
    }
    let content = match &args.dungeon {
        Some(id) => match lib.get(id) {
            Some(c) => c.clone(),
            None => {
                let known: Vec<&str> = lib.dungeons.iter().map(|d| d.dungeon.id.as_str()).collect();
                eprintln!(
                    "error: no dungeon '{id}' in {}; known: {}",
                    lib.source,
                    known.join(", ")
                );
                return ExitCode::from(1);
            }
        },
        None => lib.dungeons[0].clone(),
    };
    if content.difficulty(&args.difficulty).is_none() {
        let known: Vec<&str> = content.difficulties.iter().map(|d| d.id.as_str()).collect();
        eprintln!(
            "error: dungeon '{}' has no difficulty '{}'; known: {}",
            content.dungeon.id,
            args.difficulty,
            known.join(", ")
        );
        return ExitCode::from(1);
    }

    let profiles: Vec<AiProfile> = if args.compare_profiles {
        AiProfile::ALL.to_vec()
    } else {
        vec![args.profile]
    };
    let mut deterministic = true;
    for profile in profiles {
        let party = build_party(&args, &content, profile);
        println!(
            "== '{}' [{}] | {} members at level {} | profile {} | hp x{} | {} run(s) from seed {}",
            content.dungeon.name,
            args.difficulty,
            party.len(),
            party[0].level,
            profile.label(),
            content.dungeon.hp_multiplier,
            args.runs,
            args.seed
        );
        let (summaries, secs, ticks) = run_batch(&args, &content, &party, !args.compare_profiles);
        // Determinism self-check: replay the first seed and compare the log hash.
        let (_, replay) = run_args(&args, &content, &party, args.seed);
        let same = replay.log_hash == summaries[0].log_hash;
        deterministic &= same;
        let s = batch_stats(&summaries);
        println!(
            "SUMMARY [{}]: {} | deterministic replay: {}",
            profile.label(),
            s.one_line(),
            if same { "OK" } else { "MISMATCH" }
        );
        if args.bench {
            println!(
                "BENCH: {} runs, {} ticks in {:.3}s wall = {:.0} ticks/s ({:.3} ms/tick, {:.1}x real time)",
                args.runs,
                ticks,
                secs,
                ticks as f64 / secs.max(1e-9),
                1000.0 * secs / ticks.max(1) as f64,
                ticks as f64 * dungeon_sim::DT as f64 / secs.max(1e-9)
            );
        }
    }
    if deterministic {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(3)
    }
}
