//! Route choices, simulated (0.9).
//!
//! A layout's branches are choices for the party. For each choice (the
//! direct route, every detour, flank and safe way) the autopilot leader is
//! given a scripted route (`RunOptions::route`): it clears the required
//! fights up to the branch, walks the branch's rooms, pulls what the branch
//! holds and rejoins. The companions are not scripted: they follow the
//! leader with the normal AI, as they follow a player.
//!
//! Measured per choice: outcome and time, deaths, hazard damage taken,
//! the flanked fight (time and damage from its pull to its clear), whether
//! the leader and each companion walked every room of the branch, regroup
//! time after each leg, caches opened, overpulls and navigation trouble.
//! This is the evidence for "a meaningful choice": it says what the choice
//! changes in the simulation, not whether it is fun.

use crate::simtest::{Config, party_for};
use dungeon_sim::content::Content;
use dungeon_sim::events::EventKind;
use dungeon_sim::math::{V2, V3};
use dungeon_sim::metrics::RunSummary;
use dungeon_sim::nav::Nav;
use dungeon_sim::{RouteStep, RunOptions, RunOutcome, Sim};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// A way through the dungeon the party can be led.
#[derive(Debug, Clone, Serialize)]
pub struct Choice {
    /// `direct`, or `<purpose> b<k>`.
    pub name: String,
    pub purpose: String,
    /// Branch rooms (not corridors) the leader should walk.
    pub rooms: Vec<String>,
    /// Flank: the encounter approached.
    pub target: Option<usize>,
    #[serde(skip)]
    pub steps: Vec<RouteStep>,
}

/// One simulated run of a choice.
#[derive(Debug, Clone, Serialize)]
pub struct RouteRun {
    pub choice: String,
    pub config: String,
    pub seed: u64,
    pub outcome: String,
    pub duration: f32,
    pub deaths: u32,
    pub hazard_damage: f32,
    /// Flank: seconds from the target's pull to its clear, and the party's
    /// damage taken and deaths in that window.
    pub target_fight: Option<f32>,
    pub target_damage: Option<f32>,
    pub target_deaths: Option<u32>,
    /// Did the leader walk every branch room; how many companions did.
    pub leader_traversed: bool,
    pub companions_followed: usize,
    pub companions: usize,
    /// Mean seconds, after the leader reached a waypoint, until everyone
    /// alive was back within gathering distance.
    pub regroup: f32,
    pub caches_opened: usize,
    pub optional_cleared: usize,
    pub overpulls: u32,
    pub stuck: f32,
    pub unreachable: f32,
    pub dithers: u32,
    /// Every cleared fight: encounter -> (seconds from pull to clear, party
    /// damage taken, party deaths), to compare a flanked pack with the same
    /// pack approached directly.
    pub fights: BTreeMap<usize, (f32, f32, u32)>,
    /// Every fight's opening: the first enemy the party hit, and the first
    /// that died (names), to see whether an approach changes who is engaged
    /// first.
    pub openers: BTreeMap<usize, (String, String)>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ChoiceStats {
    pub choice: String,
    pub purpose: String,
    pub runs: usize,
    pub wins: usize,
    pub mean_time: f32,
    pub deaths: u32,
    pub mean_hazard_damage: f32,
    pub mean_target_fight: Option<f32>,
    pub mean_target_damage: Option<f32>,
    pub target_deaths: Option<u32>,
    /// Share of runs where the leader walked the whole branch.
    pub traversed: f32,
    /// Share of companions (over all runs) that walked the whole branch.
    pub followed: f32,
    pub mean_regroup: f32,
    pub caches_opened: usize,
    pub optional_cleared: usize,
    pub overpulls: u32,
    pub stuck: f32,
    pub unreachable: f32,
    pub dithers: u32,
}

/// Room ids per surface, from `[[dungeon.rooms]]`.
fn surface_rooms(content: &Content, nav: &Nav) -> Vec<Option<String>> {
    let mut by_area: BTreeMap<&str, &str> = BTreeMap::new();
    for r in &content.dungeon.rooms {
        for a in &r.areas {
            by_area.insert(a.as_str(), r.id.as_str());
        }
    }
    nav.surfaces
        .iter()
        .map(|s| by_area.get(s.name()).map(|r| r.to_string()))
        .collect()
}

/// A walkable point in room `id`: its largest piece's centre.
fn room_point(content: &Content, nav: &Nav, id: &str) -> Option<V3> {
    let r = content.dungeon.rooms.iter().find(|r| r.id == id)?;
    let s = nav
        .surfaces
        .iter()
        .enumerate()
        .filter(|(_, s)| r.areas.iter().any(|a| a == s.name()))
        .max_by(|a, b| a.1.walk.area().total_cmp(&b.1.walk.area()))?;
    let c = s.1.walk.centroid();
    Some(V3::at(c, s.1.height_at(c)))
}

fn main_order(content: &Content, id: &str) -> Option<u32> {
    content
        .dungeon
        .rooms
        .iter()
        .find(|r| r.id == id)
        .and_then(|r| r.order)
}

/// Every choice the layout offers.
pub fn choices(content: &Content, nav: &Nav) -> Vec<Choice> {
    let mut out = vec![Choice {
        name: "direct".into(),
        purpose: "direct".into(),
        rooms: Vec::new(),
        target: None,
        steps: Vec::new(),
    }];
    let enc_room = |e: usize| {
        content
            .dungeon
            .rooms
            .iter()
            .find(|r| r.encounter.as_deref() == Some(content.encounters[e].id.as_str()))
            .map(|r| r.id.clone())
    };
    for b in &content.dungeon.branches {
        if !matches!(b.purpose.as_str(), "detour" | "flank" | "safe") {
            continue;
        }
        let Some(from) = main_order(content, &b.from) else {
            continue;
        };
        let mut steps = Vec::new();
        // Clear the required fights up to the branch, in order.
        for (e, enc) in content.encounters.iter().enumerate() {
            if enc.optional {
                continue;
            }
            if enc_room(e)
                .and_then(|r| main_order(content, &r))
                .is_some_and(|o| o <= from)
            {
                steps.push(RouteStep::Clear(e));
            }
        }
        let rooms: Vec<String> = b
            .rooms
            .iter()
            .filter(|r| r.starts_with('r'))
            .cloned()
            .collect();
        let mut target = None;
        for r in &rooms {
            // A fight in the room: pull it from the doorway, then walk in.
            if let Some(e) = content
                .encounters
                .iter()
                .position(|x| enc_room_is(content, x, r))
            {
                steps.push(RouteStep::Clear(e));
            }
            if let Some(p) = room_point(content, nav, r) {
                steps.push(RouteStep::Goto(p));
            }
            for c in content
                .dungeon
                .caches
                .iter()
                .filter(|c| b.rewards.contains(&c.id))
            {
                if room_of_point(content, nav, c.pos, c.y.unwrap_or(0.0)).as_deref()
                    == Some(r.as_str())
                {
                    let (p, _) = nav.project(c.pos, c.y.unwrap_or(0.0));
                    steps.push(RouteStep::Goto(p));
                }
            }
        }
        match b.purpose.as_str() {
            "flank" => {
                // Pull the flanked pack from the flank's side.
                let t = b.target.as_deref().unwrap_or("");
                if let Some(e) = content
                    .encounters
                    .iter()
                    .position(|x| enc_room_is(content, x, t))
                {
                    steps.push(RouteStep::Clear(e));
                    target = Some(e);
                }
            }
            _ => {
                if let Some(to) = &b.to
                    && let Some(p) = room_point(content, nav, to)
                {
                    steps.push(RouteStep::Goto(p));
                }
            }
        }
        out.push(Choice {
            name: format!("{} {}", b.purpose, b.id),
            purpose: b.purpose.clone(),
            rooms,
            target,
            steps,
        });
    }
    out
}

fn enc_room_is(content: &Content, enc: &dungeon_sim::content::EncounterConfig, room: &str) -> bool {
    content
        .dungeon
        .rooms
        .iter()
        .any(|r| r.id == room && r.encounter.as_deref() == Some(enc.id.as_str()))
}

fn room_of_point(content: &Content, nav: &Nav, p: V2, y: f32) -> Option<String> {
    let s = nav.locate(p, y + 0.5)?;
    surface_rooms(content, nav)
        .get(s as usize)
        .cloned()
        .flatten()
}

/// Run one choice once.
pub fn run_choice(
    content: &Content,
    choice: &Choice,
    config: &Config,
    seed: u64,
    max_seconds: f32,
    slot_follow_only: bool,
) -> RouteRun {
    let party = party_for(config);
    let options = RunOptions {
        difficulty: config.difficulty.clone(),
        autopilot: true,
        // The choice is the only detour: other optional fights are skipped.
        skip_optional: true,
        route: choice.steps.clone(),
        slot_follow_only,
        ..Default::default()
    };
    let mut sim = Sim::with_options(content, &party, seed, options);
    let rooms = surface_rooms(content, &sim.nav);
    let want: BTreeSet<&str> = choice.rooms.iter().map(|s| s.as_str()).collect();
    let mut visited: Vec<BTreeSet<String>> = vec![BTreeSet::new(); sim.party.len()];
    let mut last_step = 0;
    let mut regroup_from: Option<f32> = None;
    let mut regroups: Vec<f32> = Vec::new();
    let gather = 12.0 + 0.6 * sim.party.len() as f32;
    while sim.outcome == RunOutcome::InProgress && sim.time < max_seconds {
        sim.step();
        if sim.tick.is_multiple_of(5) {
            for (k, &p) in sim.party.iter().enumerate() {
                let a = &sim.actors[p];
                if a.alive
                    && let Some(Some(r)) = rooms.get(a.surface as usize)
                    && want.contains(r.as_str())
                {
                    visited[k].insert(r.clone());
                }
            }
        }
        // A waypoint reached: time until the party is gathered again.
        if sim.route_step != last_step {
            let was_goto = matches!(choice.steps.get(last_step), Some(RouteStep::Goto(_)));
            last_step = sim.route_step;
            if was_goto {
                regroup_from = Some(sim.time);
            }
        }
        if let Some(t0) = regroup_from {
            let l = sim.actors[sim.leader].loc();
            if sim
                .alive_party()
                .all(|q| sim.actors[q].loc().dist(l) <= gather)
            {
                regroups.push(sim.time - t0);
                regroup_from = None;
            }
        }
    }
    if sim.outcome == RunOutcome::InProgress {
        sim.abandon(RunOutcome::Timeout);
    }
    let s = RunSummary::from_sim(&sim);
    let hazards: BTreeSet<&str> = content
        .encounters
        .iter()
        .flat_map(|e| e.hazards.iter().map(|h| h.name.as_str()))
        .collect();
    let party: BTreeSet<usize> = sim.party.iter().copied().collect();
    let mut hazard_damage = 0.0;
    let mut pulls: BTreeMap<usize, f32> = BTreeMap::new();
    let mut fights: BTreeMap<usize, (f32, f32, u32)> = BTreeMap::new();
    let mut first_hit: BTreeMap<usize, String> = BTreeMap::new();
    let mut first_kill: BTreeMap<usize, String> = BTreeMap::new();
    let enc_of = |a: usize| sim.actors[a].enemy.as_ref().map(|es| es.encounter);
    for e in &sim.events {
        match &e.kind {
            EventKind::Damage { src, dst, .. } if party.contains(src) => {
                if let Some(k) = enc_of(*dst) {
                    first_hit
                        .entry(k)
                        .or_insert_with(|| sim.actors[*dst].name.clone());
                }
            }
            EventKind::Death { actor } if !party.contains(actor) => {
                if let Some(k) = enc_of(*actor) {
                    first_kill
                        .entry(k)
                        .or_insert_with(|| sim.actors[*actor].name.clone());
                }
            }
            _ => {}
        }
        match &e.kind {
            EventKind::Damage {
                dst, what, amount, ..
            } if party.contains(dst) && hazards.contains(&**what) => {
                hazard_damage += amount;
            }
            EventKind::Aggro { encounter, .. } => {
                pulls.entry(*encounter).or_insert(e.time);
            }
            EventKind::EncounterCleared { encounter } => {
                if let Some(&a) = pulls.get(encounter) {
                    let mut dmg = 0.0;
                    let mut deaths = 0;
                    for x in sim
                        .events
                        .iter()
                        .filter(|x| x.time >= a && x.time <= e.time)
                    {
                        match &x.kind {
                            EventKind::Damage { dst, amount, .. } if party.contains(dst) => {
                                dmg += amount
                            }
                            EventKind::Death { actor } if party.contains(actor) => deaths += 1,
                            _ => {}
                        }
                    }
                    fights.insert(*encounter, (e.time - a, dmg, deaths));
                }
            }
            _ => {}
        }
    }
    let target = choice.target.and_then(|t| fights.get(&t).copied());
    let full = |v: &BTreeSet<String>| want.iter().all(|r| v.contains(*r));
    let leader_k = sim.party.iter().position(|&p| p == sim.leader).unwrap_or(0);
    RouteRun {
        choice: choice.name.clone(),
        config: config.label(),
        seed,
        outcome: format!("{:?}", s.outcome),
        duration: s.duration,
        deaths: s.party_deaths,
        hazard_damage,
        target_fight: target.map(|f| f.0),
        target_damage: target.map(|f| f.1),
        target_deaths: target.map(|f| f.2),
        leader_traversed: full(&visited[leader_k]),
        companions_followed: visited
            .iter()
            .enumerate()
            .filter(|(k, v)| *k != leader_k && full(v))
            .count(),
        companions: sim.party.len() - 1,
        regroup: if regroups.is_empty() {
            0.0
        } else {
            regroups.iter().sum::<f32>() / regroups.len() as f32
        },
        caches_opened: s.caches_opened,
        optional_cleared: sim
            .encounters
            .iter()
            .filter(|e| e.optional && e.cleared)
            .count(),
        overpulls: s.overpulls,
        stuck: s.nav_stuck_seconds,
        unreachable: s.nav_unreachable_seconds,
        dithers: s.nav_dithers,
        openers: first_hit
            .iter()
            .map(|(k, h)| {
                (
                    *k,
                    (h.clone(), first_kill.get(k).cloned().unwrap_or_default()),
                )
            })
            .collect(),
        fights,
    }
}

/// How often each enemy was the first hit and the first killed in a pack's
/// fights, e.g. "Hushed Acolyte 9/12".
pub fn openers_of(runs: &[RouteRun], encounter: usize) -> (String, String) {
    let mut hit: BTreeMap<&str, usize> = BTreeMap::new();
    let mut kill: BTreeMap<&str, usize> = BTreeMap::new();
    let mut n = 0;
    for r in runs {
        if let Some((h, k)) = r.openers.get(&encounter) {
            n += 1;
            *hit.entry(h.as_str()).or_default() += 1;
            if !k.is_empty() {
                *kill.entry(k.as_str()).or_default() += 1;
            }
        }
    }
    let fmt = |m: BTreeMap<&str, usize>| {
        let mut v: Vec<(&str, usize)> = m.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        v.iter()
            .map(|(name, c)| format!("{name} {c}/{n}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    (fmt(hit), fmt(kill))
}

/// The same pack's fight in other runs (e.g. the direct route's), as
/// (mean seconds, mean damage taken, deaths, runs where it was cleared).
pub fn fight_of(runs: &[RouteRun], encounter: usize) -> Option<(f32, f32, u32, usize)> {
    let v: Vec<&(f32, f32, u32)> = runs
        .iter()
        .filter_map(|r| r.fights.get(&encounter))
        .collect();
    if v.is_empty() {
        return None;
    }
    let n = v.len() as f32;
    Some((
        v.iter().map(|f| f.0).sum::<f32>() / n,
        v.iter().map(|f| f.1).sum::<f32>() / n,
        v.iter().map(|f| f.2).sum(),
        v.len(),
    ))
}

pub fn stats(choice: &Choice, runs: &[RouteRun]) -> ChoiceStats {
    let n = runs.len().max(1) as f32;
    let mean_opt = |f: &dyn Fn(&RouteRun) -> Option<f32>| {
        let v: Vec<f32> = runs.iter().filter_map(f).collect();
        (!v.is_empty()).then(|| v.iter().sum::<f32>() / v.len() as f32)
    };
    let companions: usize = runs.iter().map(|r| r.companions).sum();
    ChoiceStats {
        choice: choice.name.clone(),
        purpose: choice.purpose.clone(),
        runs: runs.len(),
        wins: runs.iter().filter(|r| r.outcome == "Victory").count(),
        mean_time: runs.iter().map(|r| r.duration).sum::<f32>() / n,
        deaths: runs.iter().map(|r| r.deaths).sum(),
        mean_hazard_damage: runs.iter().map(|r| r.hazard_damage).sum::<f32>() / n,
        mean_target_fight: mean_opt(&|r| r.target_fight),
        mean_target_damage: mean_opt(&|r| r.target_damage),
        target_deaths: runs
            .iter()
            .any(|r| r.target_deaths.is_some())
            .then(|| runs.iter().filter_map(|r| r.target_deaths).sum()),
        traversed: if choice.rooms.is_empty() {
            1.0
        } else {
            runs.iter().filter(|r| r.leader_traversed).count() as f32 / n
        },
        followed: if choice.rooms.is_empty() || companions == 0 {
            1.0
        } else {
            runs.iter().map(|r| r.companions_followed).sum::<usize>() as f32 / companions as f32
        },
        mean_regroup: runs.iter().map(|r| r.regroup).sum::<f32>() / n,
        caches_opened: runs.iter().map(|r| r.caches_opened).sum(),
        optional_cleared: runs.iter().map(|r| r.optional_cleared).sum(),
        overpulls: runs.iter().map(|r| r.overpulls).sum(),
        stuck: runs.iter().map(|r| r.stuck).sum(),
        unreachable: runs.iter().map(|r| r.unreachable).sum(),
        dithers: runs.iter().map(|r| r.dithers).sum(),
    }
}

pub fn table(stats: &[ChoiceStats]) -> String {
    let mut s = format!(
        "{:<14} {:>4} {:>5} {:>7} {:>6} {:>8} {:>14} {:>8} {:>8} {:>7} {:>6} {:>5} {:>5} {:>6} {:>6} {:>6}\n",
        "choice",
        "runs",
        "wins",
        "time s",
        "deaths",
        "hazard",
        "target fight",
        "walked",
        "followed",
        "regroup",
        "caches",
        "opt",
        "over",
        "stuck",
        "unrch",
        "dith"
    );
    for c in stats {
        let tf = match (c.mean_target_fight, c.mean_target_damage) {
            (Some(t), Some(d)) => format!("{t:.0}s/{d:.0}"),
            _ => "-".into(),
        };
        s.push_str(&format!(
            "{:<14} {:>4} {:>5} {:>7.0} {:>6} {:>8.0} {:>14} {:>7.0}% {:>7.0}% {:>7.1} {:>6} {:>5} {:>5} {:>6.0} {:>6.0} {:>6}\n",
            c.choice,
            c.runs,
            c.wins,
            c.mean_time,
            c.deaths,
            c.mean_hazard_damage,
            tf,
            c.traversed * 100.0,
            c.followed * 100.0,
            c.mean_regroup,
            c.caches_opened,
            c.optional_cleared,
            c.overpulls,
            c.stuck,
            c.unreachable,
            c.dithers
        ));
    }
    s
}
