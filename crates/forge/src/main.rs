//! `dungeon-forge`: generate, validate, inspect and test dungeons.
//! Run without arguments for help.

use dungeon_forge::build::{self, Generated};
use dungeon_forge::layout::{EdgeRole, Plan, Route, Style};
use dungeon_forge::simtest::{self, Config};
use dungeon_forge::{Blueprint, Kit, inspect, map};
use dungeon_sim::content::Content;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const HELP: &str = "dungeon-forge - build original dungeons from blueprints

USAGE
  dungeon-forge validate <blueprint.toml>
      Check a blueprint and its kit (no generation).
  dungeon-forge generate <blueprint.toml> [--seed N] [--out FILE] [--map FILE.svg] [--tagged] [--quiet]
      Build and validate a dungeon; write it (default: forge_out/<id>_s<seed>.toml)
      and print the topology, checks and summary. --tagged gives the dungeon the
      id <id>_s<seed> and name '<name> (layout <seed>)' so several seeds can be
      installed side by side.
  dungeon-forge inspect <dungeon.toml> [--map FILE.svg]
      Summarise any dungeon file (handwritten or generated); optionally draw its map.
  dungeon-forge test <dungeon.toml|blueprint.toml> [--seed N | --layouts A..B] [--runs N] [--sizes 5,10]
                     [--profiles balanced,aggressive,...] [--classes striker,mender,...]
                     [--difficulties normal,heroic] [--level N] [--failures FILE.json]
      Simulated AI clears over every combination; failures (wipes, timeouts,
      stuck or unreachable time, dithering) are written as reproducible JSON.
  dungeon-forge seeds <blueprint.toml> [--from N] [--count N] [--out-dir DIR]
      Generate many seeds; report which succeed and how different they are.

The kit named by a blueprint is read from <blueprint dir>/../kits/<kit>.toml.
Exit codes: 0 ok, 1 error, 2 tests found failures.
";

struct Args {
    positional: Vec<String>,
    opts: Vec<(String, String)>,
    flags: Vec<String>,
}

impl Args {
    fn parse(raw: &[String]) -> Result<Args, String> {
        let mut a = Args {
            positional: Vec::new(),
            opts: Vec::new(),
            flags: Vec::new(),
        };
        let mut i = 0;
        while i < raw.len() {
            let s = &raw[i];
            if s == "--quiet" || s == "--tagged" {
                a.flags.push(s.clone());
            } else if let Some(k) = s.strip_prefix("--") {
                let v = raw
                    .get(i + 1)
                    .ok_or_else(|| format!("--{k} needs a value"))?;
                a.opts.push((k.to_string(), v.clone()));
                i += 1;
            } else {
                a.positional.push(s.clone());
            }
            i += 1;
        }
        Ok(a)
    }

    fn get(&self, k: &str) -> Option<&str> {
        self.opts
            .iter()
            .rev()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
    }

    fn num<T: std::str::FromStr>(&self, k: &str) -> Result<Option<T>, String> {
        self.get(k)
            .map(|v| {
                v.parse::<T>()
                    .map_err(|_| format!("--{k}: '{v}' is not a number"))
            })
            .transpose()
    }

    fn list(&self, k: &str, default: &[&str]) -> Vec<String> {
        self.get(k)
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_else(|| default.iter().map(|s| s.to_string()).collect())
    }

    fn known(&self, allowed: &[&str]) -> Result<(), String> {
        for (k, _) in &self.opts {
            if !allowed.contains(&k.as_str()) {
                return Err(format!("unknown option --{k}; see dungeon-forge --help"));
            }
        }
        Ok(())
    }
}

fn load_pair(path: &Path) -> Result<(Blueprint, Kit), String> {
    let bp = Blueprint::load(path)?;
    let kit = Kit::load(&bp.kit_path(path))?;
    Ok((bp, kit))
}

fn is_blueprint(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|t| t.lines().any(|l| l.trim() == "[blueprint]"))
        .unwrap_or(false)
}

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.is_empty() || raw[0] == "--help" || raw[0] == "-h" || raw[0] == "help" {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let cmd = raw[0].clone();
    let result = Args::parse(&raw[1..]).and_then(|args| match cmd.as_str() {
        "validate" => cmd_validate(&args),
        "generate" => cmd_generate(&args),
        "inspect" => cmd_inspect(&args),
        "test" => cmd_test(&args),
        "seeds" => cmd_seeds(&args),
        other => Err(format!(
            "unknown command '{other}'; run dungeon-forge --help"
        )),
    });
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

fn first_path(args: &Args, what: &str) -> Result<PathBuf, String> {
    args.positional
        .first()
        .map(PathBuf::from)
        .ok_or_else(|| format!("give the {what} file"))
}

fn cmd_validate(args: &Args) -> Result<ExitCode, String> {
    args.known(&[])?;
    let path = first_path(args, "blueprint")?;
    let (bp, kit) = load_pair(&path)?;
    let problems = build::validate_pair(&bp, &kit);
    if problems.is_empty() {
        println!(
            "OK: blueprint '{}' with kit '{}' ({} recipes, {} templates): {} encounters, {} optional",
            bp.blueprint.id,
            kit.kit.id,
            kit.recipes.len(),
            kit.templates.len(),
            bp.encounters.len(),
            bp.optional.len()
        );
        Ok(ExitCode::SUCCESS)
    } else {
        Err(format!(
            "blueprint '{}' has {} problem(s):\n  - {}",
            bp.blueprint.id,
            problems.len(),
            problems.join("\n  - ")
        ))
    }
}

/// Human-readable topology of a plan.
pub fn topology(plan: &Plan) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let label = |i: usize| {
        let r = &plan.rooms[i];
        format!("{} [{:?} {:+.1} m]", r.name, r.kind, r.y)
    };
    let _ = writeln!(s, "seed {} (layout attempt {})", plan.seed, plan.attempt);
    let _ = writeln!(s, "main route:");
    for (k, &r) in plan.main.iter().enumerate() {
        let via = if k == 0 {
            String::new()
        } else {
            plan.corridors
                .iter()
                .find(|c| c.a == plan.main[k - 1] && c.b == r)
                .map(|c| format!("  <- {:?}", c.style))
                .unwrap_or_default()
        };
        let enc = plan.rooms[r]
            .encounter
            .map(|e| format!("  encounter {e:?}"))
            .unwrap_or_default();
        let _ = writeln!(s, "  {k}. {}{enc}{via}", label(r));
    }
    for c in &plan.corridors {
        let what = match c.role {
            EdgeRole::Main => continue,
            EdgeRole::Alternate => "alternate route",
            EdgeRole::Side => "side room",
            EdgeRole::Shortcut => {
                if c.style == Style::Drop {
                    "one-way drop"
                } else {
                    "to overlook"
                }
            }
            EdgeRole::Loop => "return loop",
        };
        let _ = writeln!(
            s,
            "  {what}: {} -> {} ({:?})",
            label(c.a),
            label(c.b),
            c.style
        );
    }
    let alt = plan
        .rooms
        .iter()
        .filter(|r| r.route == Route::Alternate)
        .count();
    let _ = writeln!(
        s,
        "{} rooms ({} main, {alt} alternate), {} corridors",
        plan.rooms.len(),
        plan.main.len(),
        plan.corridors.len()
    );
    s
}

fn write_out(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("could not write {}: {e}", path.display()))
}

fn generate_from(path: &Path, args: &Args) -> Result<(Blueprint, Generated), String> {
    let (mut bp, kit) = load_pair(path)?;
    let seed = args.num::<u64>("seed")?.unwrap_or(bp.blueprint.seed);
    if args.flags.iter().any(|f| f == "--tagged") {
        bp.blueprint.id = format!("{}_s{seed}", bp.blueprint.id);
        bp.blueprint.name = format!("{} (layout {seed})", bp.blueprint.name);
    }
    let g = build::generate(&bp, &kit, seed)?;
    Ok((bp, g))
}

fn cmd_generate(args: &Args) -> Result<ExitCode, String> {
    args.known(&["seed", "out", "map"])?;
    let path = first_path(args, "blueprint")?;
    let (bp, g) = generate_from(&path, args)?;
    let out = args.get("out").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(format!(
            "forge_out/{}_s{}.toml",
            bp.blueprint.id, g.plan.seed
        ))
    });
    write_out(&out, &g.toml)?;
    if let Some(m) = args.get("map") {
        write_out(Path::new(m), &map::svg(&g.content, Some(&g.plan)))?;
    }
    if !args.flags.iter().any(|f| f == "--quiet") {
        print!("{}", topology(&g.plan));
        println!("checks:");
        for p in g.structure.passed.iter().chain(&g.space.passed) {
            println!("  ok  {p}");
        }
        for w in g.structure.warnings.iter().chain(&g.space.warnings) {
            println!("  warn {w}");
        }
        print!("{}", inspect::summary(&g.content));
    }
    println!("wrote {}", out.display());
    if let Some(m) = args.get("map") {
        println!("wrote {m}");
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_inspect(args: &Args) -> Result<ExitCode, String> {
    args.known(&["map"])?;
    let path = first_path(args, "dungeon")?;
    let c = Content::load(&path).map_err(|e| e.to_string())?;
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    if let Some(origin) = simtest::generated_from(&text) {
        println!("generated: {origin}");
    }
    print!("{}", inspect::summary(&c));
    if let Some(m) = args.get("map") {
        write_out(Path::new(m), &map::svg(&c, None))?;
        println!("wrote {m}");
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_test(args: &Args) -> Result<ExitCode, String> {
    args.known(&[
        "seed",
        "layouts",
        "runs",
        "sizes",
        "profiles",
        "classes",
        "difficulties",
        "level",
        "failures",
    ])?;
    let path = first_path(args, "dungeon or blueprint")?;
    // What to test: one dungeon file, one generated layout, or a range of layouts.
    let mut targets: Vec<(Content, Option<String>, String)> = Vec::new();
    if is_blueprint(&path) {
        let (bp, kit) = load_pair(&path)?;
        let seeds: Vec<u64> = match args.get("layouts") {
            Some(r) => {
                let (a, b) = r
                    .split_once("..")
                    .ok_or_else(|| format!("--layouts: use FROM..TO, e.g. 1..10 (got '{r}')"))?;
                let a: u64 = a
                    .parse()
                    .map_err(|_| format!("--layouts: '{a}' is not a number"))?;
                let b: u64 = b
                    .parse()
                    .map_err(|_| format!("--layouts: '{b}' is not a number"))?;
                (a..=b).collect()
            }
            None => vec![args.num::<u64>("seed")?.unwrap_or(bp.blueprint.seed)],
        };
        for seed in seeds {
            let g = build::generate(&bp, &kit, seed)?;
            let tmp = PathBuf::from(format!("forge_out/{}_s{seed}.toml", bp.blueprint.id));
            write_out(&tmp, &g.toml)?;
            let origin = simtest::generated_from(&g.toml);
            targets.push((g.content, origin, tmp.display().to_string()));
        }
    } else {
        let c = Content::load(&path).map_err(|e| e.to_string())?;
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        targets.push((
            c,
            simtest::generated_from(&text),
            path.display().to_string(),
        ));
    }
    let runs = args.num::<usize>("runs")?.unwrap_or(10);
    let sizes: Vec<usize> = args
        .list("sizes", &["5"])
        .iter()
        .map(|s| {
            s.parse()
                .map_err(|_| format!("--sizes: '{s}' is not a number"))
        })
        .collect::<Result<_, _>>()?;
    let profiles = args.list("profiles", &["balanced"]);
    let classes = args.list("classes", &["striker"]);
    let diffs = args.list("difficulties", &["normal"]);
    let mut all_failures = Vec::new();
    // Totals per configuration across layouts: (runs, wins, stuck, unreachable, dithers, ms).
    let mut totals: std::collections::BTreeMap<String, (usize, usize, f32, f32, f32, f64)> =
        Default::default();
    for (content, origin, hint) in &targets {
        let level = args
            .num::<u32>("level")?
            .unwrap_or(content.dungeon.recommended_level);
        for d in &diffs {
            if content.difficulty(d).is_none() {
                return Err(format!(
                    "--difficulties: '{d}' is not defined in {}",
                    content.dungeon.id
                ));
            }
        }
        println!("{} - {runs} runs per configuration", content.dungeon.name);
        if let Some(o) = origin {
            println!("generated: {o}");
        }
        for &size in &sizes {
            for profile in &profiles {
                // Classes only matter for 5-player parties (the player's slot).
                let class_list: Vec<String> = if size == 5 {
                    classes.clone()
                } else {
                    vec!["-".into()]
                };
                for class in &class_list {
                    for diff in &diffs {
                        let cfg = Config {
                            size,
                            class: class.clone(),
                            profile: profile.clone(),
                            difficulty: diff.clone(),
                            level,
                        };
                        let t0 = std::time::Instant::now();
                        let r = simtest::run_config(content, origin.as_deref(), hint, cfg, runs, 1);
                        let ms = t0.elapsed().as_secs_f64() * 1000.0;
                        println!(
                            "  {:<38} {} | failures {}",
                            r.config.label(),
                            r.stats.one_line(),
                            r.failures.len()
                        );
                        let t = totals.entry(r.config.label()).or_default();
                        t.0 += r.stats.runs;
                        t.1 += r.stats.victories;
                        t.2 += r.stats.mean_stuck_seconds * r.stats.runs as f32;
                        t.3 += r.stats.mean_unreachable_seconds * r.stats.runs as f32;
                        t.4 += r.stats.mean_dithers * r.stats.runs as f32;
                        t.5 += ms;
                        all_failures.extend(r.failures);
                    }
                }
            }
        }
    }
    if targets.len() > 1 {
        println!("totals over {} layouts:", targets.len());
        for (label, t) in &totals {
            let n = t.0.max(1) as f32;
            println!(
                "  {label:<38} wins {}/{} ({:.0}%) | stuck {:.1}s unreachable {:.1}s dithers {:.1} per run | {:.0} ms per run",
                t.1,
                t.0,
                100.0 * t.1 as f32 / n,
                t.2 / n,
                t.3 / n,
                t.4 / n,
                t.5 / n as f64
            );
        }
    }
    if let Some(f) = args.get("failures") {
        let json = serde_json::to_string_pretty(&all_failures).map_err(|e| e.to_string())?;
        write_out(Path::new(f), &json)?;
        println!("wrote {} failure record(s) to {f}", all_failures.len());
    } else if !all_failures.is_empty() {
        println!(
            "{} failure(s); add --failures FILE.json for full records",
            all_failures.len()
        );
        for f in all_failures.iter().take(5) {
            println!(
                "  {} seed {}: {} (cleared {}/{}, next: {})",
                f.config.label(),
                f.sim_seed,
                f.reasons.join(", "),
                f.cleared,
                f.total,
                f.stuck_at_encounter.as_deref().unwrap_or("-")
            );
        }
    }
    Ok(if all_failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    })
}

/// A compact signature of a layout's structure (for variety statistics).
fn signature(plan: &Plan) -> String {
    let main: Vec<String> = plan
        .main
        .windows(2)
        .map(|w| {
            let c = plan
                .corridors
                .iter()
                .find(|c| c.a == w[0] && c.b == w[1])
                .expect("main corridor");
            format!("{:?}", c.dir)
        })
        .collect();
    format!(
        "{} | alt {} | side {} | {}",
        main.join(""),
        plan.rooms
            .iter()
            .filter(|r| r.route == Route::Alternate)
            .count(),
        plan.rooms.iter().filter(|r| r.route == Route::Side).count(),
        plan.corridors
            .iter()
            .filter(|c| c.role == EdgeRole::Shortcut || c.role == EdgeRole::Loop)
            .map(|c| format!("{:?}", c.style))
            .collect::<Vec<_>>()
            .join("+")
    )
}

fn cmd_seeds(args: &Args) -> Result<ExitCode, String> {
    args.known(&["from", "count", "out-dir"])?;
    let path = first_path(args, "blueprint")?;
    let (bp, kit) = load_pair(&path)?;
    let from = args.num::<u64>("from")?.unwrap_or(1);
    let count = args.num::<u64>("count")?.unwrap_or(10);
    let mut sigs = std::collections::BTreeSet::new();
    let mut failed = 0;
    let mut attempts = 0u32;
    let t0 = std::time::Instant::now();
    for seed in from..from + count {
        match build::generate(&bp, &kit, seed) {
            Ok(g) => {
                let sig = signature(&g.plan);
                attempts += g.plan.attempt + 1;
                println!(
                    "seed {seed:>4}: ok  rooms {:>2} attempt {:>2}  {sig}",
                    g.plan.rooms.len(),
                    g.plan.attempt
                );
                sigs.insert(sig);
                if let Some(dir) = args.get("out-dir") {
                    let p = Path::new(dir).join(format!("{}_s{seed}.toml", bp.blueprint.id));
                    write_out(&p, &g.toml)?;
                    write_out(
                        &p.with_extension("svg"),
                        &map::svg(&g.content, Some(&g.plan)),
                    )?;
                }
            }
            Err(e) => {
                failed += 1;
                println!("seed {seed:>4}: FAILED {}", e.lines().next().unwrap_or(""));
            }
        }
    }
    let ok = count - failed;
    println!(
        "{ok}/{count} seeds generated ({} distinct route shapes); mean attempts {:.1}; {:.1} ms per dungeon",
        sigs.len(),
        attempts as f32 / ok.max(1) as f32,
        t0.elapsed().as_secs_f64() * 1000.0 / count.max(1) as f64
    );
    Ok(if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    })
}
