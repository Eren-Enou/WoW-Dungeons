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
[--slot-follow (companions follow formation slots only, as before 0.9; for comparisons)] \
[--lab (Class Design Lab 0.10: compare Striker styles x player plans) [--levels 3,6]] \
[--lab-feel (moment-to-moment proxies: waiting, choices, repetition, reaction windows)] \
[--experiment (0.11 experiment content on)] [--technique riposte|patience (Flow Striker)]";

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
    lab: bool,
    lab_feel: bool,
    levels: Vec<u32>,
    experiment: bool,
    technique: Option<dungeon_sim::class::Technique>,
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
        lab: false,
        lab_feel: false,
        levels: Vec::new(),
        experiment: false,
        technique: None,
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
            "--lab" => args.lab = true,
            "--lab-feel" => args.lab_feel = true,
            "--experiment" => args.experiment = true,
            "--technique" => {
                let v = val("--technique")?;
                args.technique = Some(
                    dungeon_sim::class::Technique::parse(&v)
                        .ok_or(format!("unknown technique '{v}'\n{USAGE}"))?,
                )
            }
            "--levels" => {
                args.levels = val("--levels")?
                    .split(',')
                    .map(|v| {
                        v.trim()
                            .parse::<u32>()
                            .map_err(|e| format!("--levels: {e}"))
                    })
                    .collect::<Result<_, _>>()?
            }
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

/// `--lab`: every Striker style x player plan on the same seeds (0.10).
fn run_lab_matrix(args: &Args, content: &Content) {
    use dungeon_sim::class::StrikerStyle;
    use dungeon_sim::lab::{FightKind, run_lab, summarize};
    use dungeon_sim::sim::LabPlan;
    let levels = if args.levels.is_empty() {
        vec![content.dungeon.recommended_level.max(1)]
    } else {
        args.levels.clone()
    };
    for level in levels {
        println!(
            "== {} | level {level} | {} | seeds {}..{} ==",
            content.dungeon.id,
            args.difficulty,
            args.seed,
            args.seed + args.runs as u64 - 1
        );
        println!(
            "{:<12} {:<8} {:>5} {:>6} {:>6} {:>6} {:>6} | {:>13} {:>13} {:>13} | {:>5} {:>5} {:>4} {:>5} | {:<24} lab",
            "style",
            "plan",
            "wins",
            "secs",
            "pDPS",
            "share",
            "ptyDPS",
            "trash p/pty",
            "elite p/pty",
            "boss p/pty",
            "dead",
            "taken",
            "int",
            "cap%",
            "uses 1-5",
        );
        for style in StrikerStyle::ALL {
            for plan in [LabPlan::Passive, LabPlan::Simple, LabPlan::Expert] {
                let runs: Vec<dungeon_sim::lab::LabRun> = (0..args.runs)
                    .map(|i| {
                        run_lab(
                            content,
                            level,
                            args.seed + i as u64,
                            style,
                            plan,
                            &args.difficulty,
                            &lab_extras(args, 0.0, false),
                            args.max_seconds,
                        )
                        .1
                    })
                    .collect();
                let s = summarize(&runs);
                let fight = |k: FightKind| {
                    let f = s.fights[k as usize];
                    if f.0 > 0.0 {
                        format!("{:>5.0}/{:<5.0}", f.2, f.3)
                    } else {
                        "-".to_string()
                    }
                };
                let uses = s
                    .ability_uses
                    .iter()
                    .map(|u| format!("{u:.0}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let extra = match style {
                    StrikerStyle::Planned => format!("edge wasted {:.1}", s.edge_wasted),
                    StrikerStyle::Reactive => {
                        format!("openings {:.0}, exploits {:.0}", s.openings, s.exploits)
                    }
                    StrikerStyle::Flow => format!(
                        "openings {:.0}, seizes {:.0}, edge wasted {:.1}",
                        s.openings, s.exploits, s.edge_wasted
                    ),
                    StrikerStyle::Classic => String::new(),
                };
                let extra = if args.experiment {
                    let pct = |mut v: Vec<f32>, q: f32| {
                        v.sort_by(f32::total_cmp);
                        v.get(((v.len() as f32 - 1.0) * q).round() as usize)
                            .copied()
                            .unwrap_or(0.0)
                    };
                    let comp: Vec<f32> = runs
                        .iter()
                        .flat_map(|r| r.channels.iter().map(|c| c.0))
                        .collect();
                    let mine: Vec<f32> = runs
                        .iter()
                        .flat_map(|r| r.channels.iter().map(|c| c.1))
                        .collect();
                    format!(
                        "{extra} broke {:.2}/{:.2} channels; per channel companions p25/50/75 {:.0}/{:.0}/{:.0}, you {:.0}/{:.0}/{:.0}",
                        s.staggers,
                        s.stagger_casts,
                        pct(comp.clone(), 0.25),
                        pct(comp.clone(), 0.5),
                        pct(comp, 0.75),
                        pct(mine.clone(), 0.25),
                        pct(mine.clone(), 0.5),
                        pct(mine, 0.75)
                    )
                } else {
                    extra
                };
                let pay = FightKind::ALL
                    .iter()
                    .map(|&k| {
                        let f = s.payoffs[k as usize];
                        format!("{}:{:.1}/{:.0}%/{:.1}s", k.label(), f.0, 100.0 * f.1, f.2)
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                println!(
                    "{:<12} {:<8} {:>2}/{:<2} {:>6.0} {:>6.1} {:>5.1}% {:>6.1} | {:>13} {:>13} {:>13} | {:>5.2} {:>5.0} {:>4.1} {:>5.1} | {:<24} {} | payoffs {}",
                    style.label(),
                    plan.label(),
                    s.wins,
                    s.runs,
                    s.duration,
                    s.player_dps,
                    100.0 * s.player_share,
                    s.party_dps,
                    fight(FightKind::Trash),
                    fight(FightKind::Elite),
                    fight(FightKind::Boss),
                    s.party_deaths,
                    s.damage_taken,
                    s.player_interrupts,
                    s.energy_capped_pct,
                    uses,
                    extra,
                    pay
                );
            }
        }
        println!();
    }
}

/// `--lab-feel`: what playing each style is like moment to moment (0.10).
/// Proxies for feel, not measures of fun.
fn run_lab_feel(args: &Args, content: &Content) {
    use dungeon_sim::class::StrikerStyle;
    use dungeon_sim::lab::{repetition, run_feel};
    use dungeon_sim::sim::LabPlan;
    let level = args.levels.first().copied().unwrap_or(6);
    println!(
        "== {} | level {level} | seeds {}..{} | feel proxies ==",
        content.dungeon.id,
        args.seed,
        args.seed + args.runs as u64 - 1
    );
    println!(
        "{:<12} {:<7} {:>5} {:>5} {:>7} {:>6} {:>6} {:>7} {:>7} {:>8} | style detail",
        "style", "plan", "react", "wins", "pDPS", "idle%", "wait%", "choice%", "nf gap", "patterns",
    );
    // (style, plan, reaction, never from behind)
    let mut conds: Vec<(StrikerStyle, LabPlan, f32, bool)> = Vec::new();
    for style in StrikerStyle::ALL {
        for plan in [LabPlan::Passive, LabPlan::Simple, LabPlan::Expert] {
            conds.push((style, plan, 0.0, false));
        }
    }
    for style in [StrikerStyle::Reactive, StrikerStyle::Flow] {
        for r in [0.5, 1.0, 1.5] {
            conds.push((style, LabPlan::Expert, r, false));
        }
        // A sloppy player: slow to react and never repositions.
        conds.push((style, LabPlan::Expert, 1.0, true));
        conds.push((style, LabPlan::Simple, 1.0, true));
    }
    for (style, plan, react, no_behind) in conds {
        let feels: Vec<_> = (0..args.runs)
            .map(|i| {
                run_feel(
                    content,
                    level,
                    args.seed + i as u64,
                    style,
                    plan,
                    &lab_extras(args, react, no_behind),
                    args.max_seconds,
                )
            })
            .collect();
        let n = feels.len() as f32;
        let combat: f32 = feels.iter().map(|f| f.combat).sum();
        let dmg: f32 = feels.iter().map(|f| f.player_damage).sum();
        let wait: f32 = feels.iter().map(|f| f.waiting).sum();
        let casts: u32 = feels.iter().map(|f| f.casts).sum();
        let idle: f32 = feels.iter().map(|f| f.idle).sum();
        let mut gaps: Vec<f32> = feels
            .iter()
            .flat_map(|f| f.non_filler_gaps.iter().copied().skip(1))
            .collect();
        gaps.sort_by(f32::total_cmp);
        let gap = gaps.get(gaps.len() / 2).copied().unwrap_or(0.0);
        let choice: u32 = feels.iter().map(|f| f.choice_casts).sum();
        let (mut distinct, mut top) = (0.0, 0.0);
        for f in &feels {
            let (d, t) = repetition(&f.sequence);
            distinct += d / n;
            top += t / n;
        }
        let per_min = 60.0 * casts as f32 / combat.max(1.0);
        let detail0 = format!("{per_min:.1} presses/min (GCD allows 60)");
        let detail = match style {
            StrikerStyle::Planned | StrikerStyle::Flow => {
                let mut at = [0u32; 8];
                for f in &feels {
                    for (sum, n) in at.iter_mut().zip(f.edge_at_spend) {
                        *sum += n;
                    }
                }
                let spends: u32 = at.iter().sum();
                let b: u32 = feels.iter().map(|f| f.builders).sum();
                let bb: u32 = feels.iter().map(|f| f.builders_behind).sum();
                let mut ages: Vec<f32> =
                    feels.iter().flat_map(|f| f.exploit_ages.clone()).collect();
                ages.sort_by(f32::total_cmp);
                let seizes = if style == StrikerStyle::Flow {
                    format!(
                        " | seizes/run {:.0}, openings unused {:.0}%",
                        ages.len() as f32 / n,
                        100.0 * feels.iter().map(|f| f.openings_missed).sum::<u32>() as f32
                            / feels.iter().map(|f| f.openings).sum::<u32>().max(1) as f32
                    )
                } else {
                    String::new()
                };
                format!(
                    "Edge at spend 2-7: {} | builders from behind {:.0}%{seizes}",
                    at[2..]
                        .iter()
                        .map(|c| format!("{:.0}%", 100.0 * *c as f32 / spends.max(1) as f32))
                        .collect::<Vec<_>>()
                        .join(" "),
                    100.0 * bb as f32 / b.max(1) as f32
                )
            }
            StrikerStyle::Reactive => {
                let mut ages: Vec<f32> =
                    feels.iter().flat_map(|f| f.exploit_ages.clone()).collect();
                ages.sort_by(f32::total_cmp);
                let med = ages.get(ages.len() / 2).copied().unwrap_or(0.0);
                let opens: u32 = feels.iter().map(|f| f.openings).sum();
                let missed: u32 = feels.iter().map(|f| f.openings_missed).sum();
                format!(
                    "exploits/run {:.0}, opening age at Exploit median {:.2}s, openings run out unused {:.0}%",
                    ages.len() as f32 / n,
                    med,
                    100.0 * missed as f32 / opens.max(1) as f32
                )
            }
            StrikerStyle::Classic => String::new(),
        };
        println!(
            "{:<12} {:<7} {:>5.1} {:>2}/{:<2} {:>7.1} {:>5.1}% {:>5.1}% {:>6.1}% {:>6.1}s {:>4.0}/{:<3.0}% | {detail0} | {detail}",
            style.label(),
            plan.label(),
            react,
            feels.iter().filter(|f| f.won).count(),
            feels.len(),
            dmg / combat.max(1.0),
            100.0 * idle / combat.max(1.0),
            100.0 * wait / combat.max(1.0),
            100.0 * choice as f32 / casts.max(1) as f32,
            gap,
            distinct,
            100.0 * top,
        );
    }
    println!();
}

/// Lab conditions from the command line plus a reaction delay and the
/// never-from-behind flag.
fn lab_extras(args: &Args, reaction: f32, no_behind: bool) -> dungeon_sim::lab::LabExtras {
    dungeon_sim::lab::LabExtras {
        reaction,
        no_behind,
        experiment: args.experiment,
        technique: args.technique,
    }
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

    if args.lab {
        run_lab_matrix(&args, &content);
        return ExitCode::SUCCESS;
    }
    if args.lab_feel {
        run_lab_feel(&args, &content);
        return ExitCode::SUCCESS;
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
