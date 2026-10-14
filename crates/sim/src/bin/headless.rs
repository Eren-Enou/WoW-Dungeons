//! Seeded headless batch runner.
//!
//! Usage: dungeon-headless [--seed N] [--runs N] [--class warden|mender|striker|arcanist]
//!                         [--content PATH] [--max-seconds S] [--log FILE] [--json]

use dungeon_sim::ClassId;
use dungeon_sim::content::Content;
use dungeon_sim::metrics::RunSummary;
use dungeon_sim::runner::{DEFAULT_MAX_SECONDS, batch_stats, default_party, run_headless};
use std::process::ExitCode;

const USAGE: &str = "usage: dungeon-headless [--seed N] [--runs N] [--class warden|mender|striker|arcanist] [--content PATH] [--max-seconds S] [--log FILE] [--json]";

struct Args {
    seed: u64,
    runs: usize,
    class: ClassId,
    content: String,
    max_seconds: f32,
    log: Option<String>,
    json: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        seed: 1,
        runs: 10,
        class: ClassId::Striker,
        content: Content::default_path(),
        max_seconds: DEFAULT_MAX_SECONDS,
        log: None,
        json: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut val = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "--seed" => args.seed = val("--seed")?.parse().map_err(|e| format!("--seed: {e}"))?,
            "--runs" => args.runs = val("--runs")?.parse().map_err(|e| format!("--runs: {e}"))?,
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
            "--content" => args.content = val("--content")?,
            "--log" => args.log = Some(val("--log")?),
            "--json" => args.json = true,
            "-h" | "--help" => return Err(USAGE.into()),
            other => return Err(format!("unknown argument '{other}'\n{USAGE}")),
        }
    }
    if args.runs == 0 {
        return Err("--runs must be at least 1".into());
    }
    Ok(args)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let content = match Content::load(&args.content) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };
    let party = default_party(args.class);
    println!(
        "Dungeon Sandbox headless runner | '{}' | hp x{} | ring every {}s | player class {:?} | {} run(s) from seed {}",
        content.dungeon.name,
        content.dungeon.hp_multiplier,
        content.boss.ring_interval,
        args.class,
        args.runs,
        args.seed
    );
    let mut summaries: Vec<RunSummary> = Vec::new();
    for i in 0..args.runs {
        let seed = args.seed + i as u64;
        let (sim, summary) = run_headless(&content, &party, seed, args.max_seconds);
        if i == 0
            && let Some(path) = &args.log
        {
            let text: String = sim.events.iter().map(|e| sim.describe(e) + "\n").collect();
            if let Err(e) = std::fs::write(path, text) {
                eprintln!("could not write log {path}: {e}");
            }
        }
        if args.json {
            println!("{}", serde_json::to_string(&summary).expect("serializable"));
        } else {
            print!("{}", summary.format_table());
        }
        summaries.push(summary);
    }

    // Determinism self-check: replay the first seed and compare the log hash.
    let (_, replay) = run_headless(&content, &party, args.seed, args.max_seconds);
    let deterministic = replay.log_hash == summaries[0].log_hash;

    let s = batch_stats(&summaries);
    println!(
        "SUMMARY: runs {} | victories {} | wipes {} | timeouts {} | win rate {:.0}% | mean duration {:.1}s | mean deaths {:.2} | mean party dps {:.1} | mean ring hits {:.2} | deterministic replay: {}",
        s.runs,
        s.victories,
        s.wipes,
        s.timeouts,
        100.0 * s.victories as f32 / s.runs as f32,
        s.mean_duration,
        s.mean_deaths,
        s.mean_party_dps,
        s.mean_ring_hits,
        if deterministic { "OK" } else { "MISMATCH" }
    );
    if deterministic {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(3)
    }
}
