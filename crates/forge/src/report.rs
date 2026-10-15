//! Gameplay analysis: run simulated parties through a dungeon and say where
//! things go wrong — per encounter (pulls, resets, wipes, deaths, killing
//! blows, clear times, optional completion) and per room (dithering and
//! stuck recoveries) — with positions for a map overlay.
//!
//! Everything here reads a finished [`Sim`]: the event log for combat and
//! `Sim::nav_log` for movement trouble, plus party positions sampled at the
//! moment of each death.

use crate::simtest::{Config, party_for};
use dungeon_sim::content::Content;
use dungeon_sim::events::EventKind;
use dungeon_sim::math::V2;
use dungeon_sim::sim::NavEventKind;
use dungeon_sim::{RunOptions, RunOutcome, Sim};
use serde::Serialize;
use std::collections::BTreeMap;

/// Where something happened, for the map overlay.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Spot {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Spot {
    pub fn xz(&self) -> V2 {
        V2::new(self.x, self.z)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EncounterReport {
    pub id: String,
    pub name: String,
    pub optional: bool,
    /// Runs in which the encounter was pulled at least once.
    pub runs_engaged: usize,
    /// Runs in which it was cleared.
    pub runs_cleared: usize,
    pub pulls: u32,
    /// Times the pack reset (party wiped or ran out of its leash).
    pub resets: u32,
    /// Runs that ended in a wipe while this encounter was in progress.
    pub wipes: usize,
    /// Party member deaths while this encounter was the latest one pulled.
    pub deaths: u32,
    /// "Ability (source)" -> killing blows.
    pub killing_blows: BTreeMap<String, u32>,
    /// Class -> deaths.
    pub victims: BTreeMap<String, u32>,
    /// Seconds from the successful pull to the last enemy falling.
    pub clear_seconds: Vec<f32>,
    #[serde(skip)]
    pub death_spots: Vec<Spot>,
}

impl EncounterReport {
    pub fn mean_clear(&self) -> Option<f32> {
        (!self.clear_seconds.is_empty())
            .then(|| self.clear_seconds.iter().sum::<f32>() / self.clear_seconds.len() as f32)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RoomTrouble {
    pub dithers: u32,
    pub stuck: u32,
}

/// One dungeon (one layout) over a set of runs.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub dungeon: String,
    pub name: String,
    pub generated_from: Option<String>,
    pub configs: Vec<String>,
    pub runs: usize,
    pub victories: usize,
    pub wipes: usize,
    pub timeouts: usize,
    pub mean_duration: f32,
    pub mean_victory_duration: f32,
    pub encounters: Vec<EncounterReport>,
    /// Room (surface name up to its first " (") -> navigation trouble.
    pub rooms: BTreeMap<String, RoomTrouble>,
    pub deaths: Vec<Spot>,
    pub dithers: Vec<Spot>,
    pub stuck: Vec<Spot>,
    /// Deaths outside any pulled encounter (hazards on the way, falls).
    pub deaths_between: u32,
}

/// A room's name from one of its surfaces: "Hall (aisle 2)" -> "Hall".
pub fn room_of(surface: &str) -> &str {
    surface.split(" (").next().unwrap_or(surface)
}

/// Run once, noting where each party member stands when they die.
pub fn traced_run(
    content: &Content,
    config: &Config,
    seed: u64,
    max_seconds: f32,
) -> (Sim, Vec<(usize, Spot)>) {
    let party = party_for(config);
    let options = RunOptions {
        difficulty: config.difficulty.clone(),
        autopilot: true,
        ..Default::default()
    };
    let mut sim = Sim::with_options(content, &party, seed, options);
    let mut seen = 0;
    let mut deaths = Vec::new();
    while sim.outcome == RunOutcome::InProgress {
        if sim.time >= max_seconds {
            sim.abandon(RunOutcome::Timeout);
            break;
        }
        sim.step();
        for (k, e) in sim.events.iter().enumerate().skip(seen) {
            if let EventKind::Death { actor } = e.kind
                && sim.party.contains(&actor)
            {
                let a = &sim.actors[actor];
                deaths.push((
                    k,
                    Spot {
                        x: a.pos.x,
                        y: a.y,
                        z: a.pos.z,
                    },
                ));
            }
        }
        seen = sim.events.len();
    }
    (sim, deaths)
}

impl Report {
    pub fn new(content: &Content, origin: Option<String>) -> Report {
        Report {
            dungeon: content.dungeon.id.clone(),
            name: content.dungeon.name.clone(),
            generated_from: origin,
            encounters: content
                .encounters
                .iter()
                .map(|e| EncounterReport {
                    id: e.id.clone(),
                    name: e.display_name().to_string(),
                    optional: e.optional,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    /// Fold one finished run into the report.
    pub fn add_run(&mut self, sim: &Sim, death_spots: &[(usize, Spot)]) {
        self.runs += 1;
        match sim.outcome {
            RunOutcome::Victory => {
                self.victories += 1;
                self.mean_victory_duration += sim.time;
            }
            RunOutcome::Wipe => self.wipes += 1,
            _ => self.timeouts += 1,
        }
        self.mean_duration += sim.time;

        let n = self.encounters.len();
        let mut engaged = vec![false; n];
        let mut pulled_at = vec![0.0f32; n];
        // Encounters in progress, latest pull last.
        let mut active: Vec<usize> = Vec::new();
        let mut last_hit: BTreeMap<usize, String> = BTreeMap::new();
        for (k, e) in sim.events.iter().enumerate() {
            match &e.kind {
                EventKind::Aggro { encounter, .. } => {
                    let i = *encounter;
                    engaged[i] = true;
                    pulled_at[i] = e.time;
                    self.encounters[i].pulls += 1;
                    active.retain(|&a| a != i);
                    active.push(i);
                }
                EventKind::EncounterReset { encounter } => {
                    self.encounters[*encounter].resets += 1;
                    active.retain(|&a| a != *encounter);
                }
                EventKind::EncounterCleared { encounter } => {
                    let i = *encounter;
                    self.encounters[i].runs_cleared += 1;
                    self.encounters[i].clear_seconds.push(e.time - pulled_at[i]);
                    active.retain(|&a| a != i);
                }
                EventKind::Damage {
                    src,
                    dst,
                    what,
                    amount,
                    ..
                } if *amount > 0.0 && sim.party.contains(dst) => {
                    last_hit.insert(*dst, format!("{what} ({})", sim.actors[*src].name));
                }
                EventKind::Death { actor } if sim.party.contains(actor) => {
                    let spot = death_spots.iter().find(|(i, _)| *i == k).map(|(_, s)| *s);
                    if let Some(s) = spot {
                        self.deaths.push(s);
                    }
                    match active.last() {
                        Some(&i) => {
                            let r = &mut self.encounters[i];
                            r.deaths += 1;
                            let blow = last_hit
                                .get(actor)
                                .cloned()
                                .unwrap_or_else(|| "unknown".into());
                            *r.killing_blows.entry(blow).or_default() += 1;
                            let class = sim.actors[*actor]
                                .class
                                .map(|c| c.label().to_string())
                                .unwrap_or_else(|| "-".into());
                            *r.victims.entry(class).or_default() += 1;
                            if let Some(s) = spot {
                                r.death_spots.push(s);
                            }
                        }
                        None => self.deaths_between += 1,
                    }
                }
                _ => {}
            }
        }
        for (i, was) in engaged.into_iter().enumerate() {
            if was {
                self.encounters[i].runs_engaged += 1;
            }
        }
        if sim.outcome == RunOutcome::Wipe {
            // The fight in progress, else the first required one left.
            let at = active.last().copied().or_else(|| {
                sim.encounters
                    .iter()
                    .position(|e| !e.cleared && !e.optional)
            });
            if let Some(i) = at {
                self.encounters[i].wipes += 1;
            }
        }
        for ev in &sim.nav_log {
            if !sim.party.contains(&ev.actor) {
                continue;
            }
            let room = sim
                .nav
                .locate(ev.pos, ev.y)
                .map(|s| room_of(sim.nav.surfaces[s as usize].name()).to_string())
                .unwrap_or_else(|| "(off the floor)".into());
            let t = self.rooms.entry(room).or_default();
            let spot = Spot {
                x: ev.pos.x,
                y: ev.y,
                z: ev.pos.z,
            };
            match ev.kind {
                NavEventKind::Dither => {
                    t.dithers += 1;
                    self.dithers.push(spot);
                }
                NavEventKind::Stuck => {
                    t.stuck += 1;
                    self.stuck.push(spot);
                }
            }
        }
    }

    pub fn finish(&mut self) {
        if self.runs > 0 {
            self.mean_duration /= self.runs as f32;
        }
        if self.victories > 0 {
            self.mean_victory_duration /= self.victories as f32;
        }
    }

    /// Human-readable summary.
    pub fn text(&self) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        let n = self.runs.max(1) as f32;
        let _ = writeln!(
            s,
            "{} - {} runs ({}) | wins {} ({:.0}%) wipes {} timeouts {} | mean {:.0}s (wins {:.0}s)",
            self.name,
            self.runs,
            self.configs.join("; "),
            self.victories,
            100.0 * self.victories as f32 / n,
            self.wipes,
            self.timeouts,
            self.mean_duration,
            self.mean_victory_duration
        );
        if let Some(o) = &self.generated_from {
            let _ = writeln!(s, "  generated: {o}");
        }
        let _ = writeln!(
            s,
            "  {:<34} {:>7} {:>6} {:>6} {:>5} {:>6} {:>7}  top killing blows",
            "encounter", "cleared", "pulls", "resets", "wipes", "deaths", "clear s"
        );
        for e in &self.encounters {
            let mut blows: Vec<(&String, &u32)> = e.killing_blows.iter().collect();
            blows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let top: Vec<String> = blows
                .iter()
                .take(3)
                .map(|(k, v)| format!("{k} x{v}"))
                .collect();
            let _ = writeln!(
                s,
                "  {:<34} {:>7} {:>6} {:>6} {:>5} {:>6} {:>7}  {}",
                format!(
                    "{}{}",
                    e.name.chars().take(31).collect::<String>(),
                    if e.optional { " (o)" } else { "" }
                ),
                format!("{}/{}", e.runs_cleared, self.runs),
                e.pulls,
                e.resets,
                e.wipes,
                e.deaths,
                e.mean_clear()
                    .map(|t| format!("{t:.0}"))
                    .unwrap_or_else(|| "-".into()),
                top.join(", ")
            );
        }
        if self.deaths_between > 0 {
            let _ = writeln!(s, "  deaths between encounters: {}", self.deaths_between);
        }
        let mut rooms: Vec<(&String, &RoomTrouble)> = self.rooms.iter().collect();
        rooms.sort_by(|a, b| {
            (b.1.dithers + 5 * b.1.stuck)
                .cmp(&(a.1.dithers + 5 * a.1.stuck))
                .then(a.0.cmp(b.0))
        });
        if !rooms.is_empty() {
            let _ = writeln!(
                s,
                "  movement trouble per run (dithers / stuck recoveries):"
            );
            for (room, t) in rooms.iter().take(6) {
                let _ = writeln!(
                    s,
                    "    {:<40} {:>6.1} / {:.1}",
                    room,
                    t.dithers as f32 / n,
                    t.stuck as f32 / n
                );
            }
        }
        s
    }
}

/// Run every configuration `runs` times and build the report.
pub fn report(
    content: &Content,
    origin: Option<String>,
    configs: &[Config],
    runs: usize,
    max_seconds: f32,
) -> Report {
    let mut r = Report::new(content, origin);
    r.configs = configs.iter().map(|c| c.label()).collect();
    for c in configs {
        for k in 0..runs {
            let (sim, deaths) = traced_run(content, c, 1 + k as u64, max_seconds);
            r.add_run(&sim, &deaths);
        }
    }
    r.finish();
    r
}
