//! Class Design Lab (0.10): measure whether playing a Striker style well
//! makes a difference. The player's slot is played by a scripted plan
//! (`LabPlan`); companions run their ordinary AI in every condition. This
//! measures effectiveness, not enjoyment (CLASS_DESIGN_LAB_0_10.md).

use crate::ai::AiProfile;
use crate::class::{ClassId, StrikerStyle, Technique};
use crate::content::{Content, EnemyRank};
use crate::events::EventKind;
use crate::runner::party_at_level;
use crate::sim::{Controller, LabPlan, PartyMemberSpec, RunOptions, RunOutcome, Sim};

/// A Striker party with the player's slot (index 0) marked as the player.
pub fn lab_party(level: u32) -> Vec<PartyMemberSpec> {
    let mut party = party_at_level(ClassId::Striker, level, AiProfile::Balanced);
    party[0].player_controlled = true;
    party
}

/// Build a run whose player's slot is played by the AI with `style` and
/// `plan` (as `--demo` does: the tank leads).
pub fn lab_sim(
    content: &Content,
    party: &[PartyMemberSpec],
    seed: u64,
    style: StrikerStyle,
    plan: LabPlan,
    difficulty: &str,
) -> Sim {
    lab_sim_with(
        content,
        party,
        seed,
        style,
        plan,
        difficulty,
        &LabExtras::default(),
    )
}

/// Conditions for a lab run beyond style and plan.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LabExtras {
    /// Seconds before the scripted player notices an opening.
    pub reaction: f32,
    /// The scripted player never gets the from-behind bonus.
    pub no_behind: bool,
    /// 0.11 experiment content (Brood Frenzy) on.
    pub experiment: bool,
    /// 0.11 Technique for the Flow Striker.
    pub technique: Option<Technique>,
}

impl LabExtras {
    pub fn reacting(reaction: f32) -> Self {
        LabExtras {
            reaction,
            ..Default::default()
        }
    }
}

/// `lab_sim` with extra conditions. The experiment flag must be set before
/// the run is built: it decides which enemy abilities exist.
pub fn lab_sim_with(
    content: &Content,
    party: &[PartyMemberSpec],
    seed: u64,
    style: StrikerStyle,
    plan: LabPlan,
    difficulty: &str,
    x: &LabExtras,
) -> Sim {
    let mut sim = Sim::with_options(
        content,
        party,
        seed,
        RunOptions {
            difficulty: difficulty.to_string(),
            autopilot: true,
            player_style: style,
            lab_plan: plan,
            lab_reaction: x.reaction,
            lab_no_behind: x.no_behind,
            experiment: x.experiment,
            technique: x.technique,
            ..Default::default()
        },
    );
    if let Some(p) = sim.player {
        sim.actors[p].controller = Controller::Ai;
        if let Some(t) = sim.tank() {
            sim.leader = t;
        }
    }
    sim
}

/// What playing a style is like from moment to moment (a proxy for feel,
/// not a measure of fun).
#[derive(Debug, Clone, Default)]
pub struct Feel {
    pub won: bool,
    pub player_damage: f32,
    /// Seconds the player was in combat.
    pub combat: f32,
    /// In combat, off the GCD, not casting, and no attack could be used.
    pub waiting: f32,
    /// In combat, off the GCD and not casting (pressing nothing, by choice
    /// or not).
    pub idle: f32,
    /// Casts that were not the filler (slot 1), and the combat-time gaps
    /// between them.
    pub non_filler: u32,
    pub non_filler_gaps: Vec<f32>,
    /// Casts, and casts where two or more attacks were usable.
    pub casts: u32,
    pub choice_casts: u32,
    /// The slots pressed, in order (0-based).
    pub sequence: Vec<u8>,
    /// Lab A/C: Edge held at each spend (index = Edge, 0..=7).
    pub edge_at_spend: [u32; 8],
    pub builders: u32,
    pub builders_behind: u32,
    /// Lab B: age of each opening when it was Exploited; openings that ran
    /// out unused.
    pub exploit_ages: Vec<f32>,
    pub openings_missed: u32,
    pub openings: u32,
}

/// Would pressing slot `s` now be a sensible attack? Not Evade (the threat
/// dump), not Kick with nothing to interrupt, not an area attack with fewer
/// than two enemies in reach.
fn sensible(sim: &Sim, p: usize, s: usize) -> bool {
    use crate::class::Effect;
    let a = &sim.actors[p];
    match a.abilities[s].def().effect {
        Effect::ThreatDrop { .. } => false,
        Effect::Interrupt { .. } => a
            .target
            .and_then(|t| sim.actors[t].cast.as_ref())
            .is_some_and(|c| c.interruptible),
        Effect::AoeDamage { radius, .. } | Effect::EdgeWhirl { radius, .. } => {
            sim.engaged_enemies()
                .iter()
                .filter(|&&e| sim.actors[e].pos.dist(a.pos) <= radius + sim.actors[e].radius)
                .count()
                >= 2
        }
        _ => true,
    }
}

/// Play one run step by step, sampling what the player could do.
pub fn run_feel(
    content: &Content,
    level: u32,
    seed: u64,
    style: StrikerStyle,
    plan: LabPlan,
    x: &LabExtras,
    max_seconds: f32,
) -> Feel {
    use crate::sim::{AuraKind, DT};
    let party = lab_party(level);
    let mut sim = lab_sim_with(content, &party, seed, style, plan, "normal", x);
    let p = sim.player.expect("player");
    let mut f = Feel::default();
    let mut uses = sim.metrics.actors[p].ability_uses;
    let mut last_non_filler = 0.0;
    while sim.outcome == RunOutcome::InProgress && sim.time < max_seconds {
        let a = &sim.actors[p];
        let fighting = a.alive && sim.in_combat(p);
        // What could be pressed now (with the current target).
        let target = a.target;
        let usable: Vec<usize> = (0..a.abilities.len())
            .filter(|&s| sensible(&sim, p, s))
            .filter(|&s| sim.ability_state(p, s, target).result.is_ok())
            .collect();
        let free = a.gcd <= 0.0 && a.cast.is_none();
        let edge_before = sim.edge(p);
        // Openings from the player, with their age and time left.
        let opens: Vec<(usize, f32, f32)> = sim
            .enemies
            .iter()
            .flat_map(|&e| {
                sim.actors[e]
                    .auras
                    .iter()
                    .filter(|x| x.kind == AuraKind::Opening && x.source == p)
                    .map(move |x| (e, x.duration - x.remaining, x.remaining))
            })
            .collect();
        let exploits_before = sim.metrics.actors[p].exploits;
        sim.step();
        if fighting {
            f.combat += DT;
            if free {
                f.idle += DT;
                if usable.is_empty() {
                    f.waiting += DT;
                }
            }
        }
        let now = sim.metrics.actors[p].ability_uses;
        for s in 0..5 {
            for _ in uses[s]..now[s] {
                f.casts += 1;
                if usable.len() >= 2 {
                    f.choice_casts += 1;
                }
                f.sequence.push(s as u8);
                if s != 0 {
                    f.non_filler += 1;
                    f.non_filler_gaps.push(f.combat - last_non_filler);
                    last_non_filler = f.combat;
                }
                let effect = sim.actors[p].abilities[s].def().effect;
                if matches!(
                    effect,
                    crate::class::Effect::EdgeBleed { .. }
                        | crate::class::Effect::EdgeWhirl { .. }
                        | crate::class::Effect::Finisher { .. }
                ) {
                    f.edge_at_spend[edge_before.min(7) as usize] += 1;
                }
            }
        }
        uses = now;
        // Openings that are gone: Exploited (the oldest on that enemy) or
        // run out.
        let after: Vec<(usize, f32)> = sim
            .enemies
            .iter()
            .flat_map(|&e| {
                sim.actors[e]
                    .auras
                    .iter()
                    .filter(|x| x.kind == AuraKind::Opening && x.source == p)
                    .map(move |x| (e, x.duration - x.remaining))
            })
            .collect();
        let exploited = sim.metrics.actors[p].exploits > exploits_before;
        let mut used_one = false;
        for &(e, age, left) in &opens {
            let still = after
                .iter()
                .any(|&(e2, a2)| e2 == e && (a2 - age - DT).abs() < 1e-3);
            if still {
                continue;
            }
            if exploited && !used_one && left > DT * 1.5 {
                f.exploit_ages.push(age);
                used_one = true;
            } else if left <= DT * 1.5 && sim.actors[e].alive {
                f.openings_missed += 1;
            }
        }
    }
    let m = &sim.metrics.actors[p];
    f.won = sim.outcome == RunOutcome::Victory;
    f.player_damage = m.damage_done;
    f.builders = m.builders;
    f.builders_behind = m.builders_behind;
    f.openings = m.openings;
    f
}

/// How repetitive a press sequence is: (distinct 4-press patterns per 100
/// patterns, share of the single most common pattern).
pub fn repetition(seq: &[u8]) -> (f32, f32) {
    use std::collections::HashMap;
    if seq.len() < 4 {
        return (0.0, 0.0);
    }
    let mut counts: HashMap<&[u8], u32> = HashMap::new();
    for w in seq.windows(4) {
        *counts.entry(w).or_default() += 1;
    }
    let n = (seq.len() - 3) as f32;
    let top = counts.values().copied().max().unwrap_or(0) as f32;
    (100.0 * counts.len() as f32 / n, top / n)
}

/// Which kind of fight an encounter is (its strongest enemy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FightKind {
    Trash,
    Elite,
    Boss,
}

impl FightKind {
    pub const ALL: [FightKind; 3] = [FightKind::Trash, FightKind::Elite, FightKind::Boss];

    pub fn label(self) -> &'static str {
        match self {
            FightKind::Trash => "trash",
            FightKind::Elite => "elite",
            FightKind::Boss => "boss",
        }
    }
}

/// One measured run.
#[derive(Debug, Clone, Default)]
pub struct LabRun {
    pub won: bool,
    pub duration: f32,
    /// Seconds fought (from each pull to its clear).
    pub combat: f32,
    pub player_damage: f32,
    pub party_damage: f32,
    pub party_deaths: u32,
    pub player_deaths: u32,
    pub damage_taken: f32,
    pub avoidable: f32,
    pub player_interrupts: u32,
    pub party_interrupts: u32,
    pub energy_capped: f32,
    /// Seconds the player was in combat.
    pub player_combat: f32,
    pub ability_uses: [u32; 5],
    pub edge_wasted: u32,
    pub openings: u32,
    pub exploits: u32,
    /// 0.11: breakable casts broken, of those started.
    pub staggers: u32,
    pub stagger_casts: u32,
    /// 0.11: damage to the caster during each breakable cast, by the
    /// companions and by the player (until it broke or completed).
    pub channels: Vec<(f32, f32)>,
    /// By fight kind: (fights, seconds, player damage, party damage).
    pub fights: [(u32, f32, f32, f32); 3],
    /// By fight kind: (payoffs, fights with one, seconds from pull to the
    /// first). A payoff is the style's big moment: a (Deep) Lacerate, a
    /// Reaping Whirl or an Exploit.
    pub payoffs: [(u32, u32, f32); 3],
}

/// The cast time of breakable channels measured by `measure` (Brood
/// Frenzy).
const CHANNEL_SECS: f32 = 4.0;

/// The payoff abilities of the three styles (see `LabRun::payoffs`).
pub const PAYOFFS: [&str; 6] = [
    "Lacerate",
    "Deep Lacerate",
    "Reaping Whirl",
    "Exploit",
    "Ravage",
    "Seize",
];

fn kind_index(k: FightKind) -> usize {
    match k {
        FightKind::Trash => 0,
        FightKind::Elite => 1,
        FightKind::Boss => 2,
    }
}

/// Measure a finished run.
pub fn measure(sim: &Sim) -> LabRun {
    let p = sim.player.expect("lab runs have a player");
    let pm = &sim.metrics.actors[p];
    let mut r = LabRun {
        won: sim.outcome == RunOutcome::Victory,
        duration: sim.ended_at.unwrap_or(sim.time),
        player_damage: pm.damage_done,
        player_deaths: pm.deaths,
        player_interrupts: pm.interrupts,
        energy_capped: pm.energy_capped,
        player_combat: pm.combat_secs,
        ability_uses: pm.ability_uses,
        edge_wasted: pm.edge_wasted,
        openings: pm.openings,
        exploits: pm.exploits,
        staggers: sim.metrics.staggers,
        stagger_casts: sim.metrics.stagger_casts,
        ..Default::default()
    };
    for &m in &sim.party {
        let am = &sim.metrics.actors[m];
        r.party_damage += am.damage_done;
        r.party_deaths += am.deaths;
        r.damage_taken += am.damage_taken;
        r.avoidable += am.avoidable_taken;
        r.party_interrupts += am.interrupts;
    }
    // Fights: from an encounter's pull to its clear (or the end).
    let kind_of = |enc: usize| {
        let ranks = sim.encounters[enc]
            .mobs
            .iter()
            .filter_map(|&m| sim.actors[m].rank());
        let mut k = FightKind::Trash;
        for rank in ranks {
            match rank {
                EnemyRank::Boss => k = FightKind::Boss,
                EnemyRank::Elite if k == FightKind::Trash => k = FightKind::Elite,
                _ => {}
            }
        }
        k
    };
    let mut start: Vec<Option<f32>> = vec![None; sim.encounters.len()];
    let mut dmg: Vec<(f32, f32)> = vec![(0.0, 0.0); sim.encounters.len()];
    // Payoffs per encounter: (count, first time); one per cast (an area
    // payoff hits several enemies at the same time).
    let mut pay: Vec<(u32, Option<f32>)> = vec![(0, None); sim.encounters.len()];
    let mut last_pay: Option<(f32, &str)> = None;
    let enc_of = |id: usize| sim.actors[id].enemy.as_ref().map(|e| e.encounter);
    // Breakable casts: (caster, start, companion damage, player damage).
    let mut open_channels: Vec<(usize, f32, f32, f32)> = Vec::new();
    for e in &sim.events {
        match &e.kind {
            EventKind::CastStart { src, what, .. }
                if sim.actors[*src].enemy.as_ref().is_some_and(|es| {
                    es.abilities
                        .iter()
                        .any(|a| a.cfg.stagger.is_some() && *a.label == **what)
                }) =>
            {
                open_channels.push((*src, e.time, 0.0, 0.0));
            }
            EventKind::Damage {
                src,
                dst,
                amount,
                absorbed,
                ..
            } if open_channels.iter().any(|c| c.0 == *dst) && sim.party.contains(src) => {
                for c in open_channels.iter_mut().filter(|c| c.0 == *dst) {
                    if *src == p {
                        c.3 += amount + absorbed;
                    } else {
                        c.2 += amount + absorbed;
                    }
                }
            }
            _ => {}
        }
        // A channel ends when it breaks or its cast time is up.
        open_channels.retain(|c| {
            let ended = e.time >= c.1 + CHANNEL_SECS
                || matches!(&e.kind, EventKind::Staggered { enemy, .. } if *enemy == c.0);
            if ended {
                r.channels.push((c.2, c.3));
            }
            !ended
        });
        match &e.kind {
            EventKind::Aggro { encounter, .. } if start[*encounter].is_none() => {
                start[*encounter] = Some(e.time);
            }
            EventKind::Damage { src, dst, what, .. }
            | EventKind::AuraApplied { src, dst, what, .. }
                if *src == p
                    && PAYOFFS.contains(&&**what)
                    && last_pay != Some((e.time, &**what))
                    && (matches!(e.kind, EventKind::AuraApplied { .. })
                        != matches!(&**what, "Reaping Whirl" | "Exploit" | "Ravage" | "Seize")) =>
            {
                // Bleeds count when applied, hits when they land.
                last_pay = Some((e.time, what));
                if let Some(enc) = enc_of(*dst) {
                    pay[enc].0 += 1;
                    pay[enc].1.get_or_insert(e.time);
                }
                if let EventKind::Damage { amount, .. } = &e.kind
                    && let Some(enc) = enc_of(*dst)
                {
                    dmg[enc].0 += amount;
                    dmg[enc].1 += amount;
                }
            }
            EventKind::Damage {
                src, dst, amount, ..
            } => {
                if let Some(enc) = enc_of(*dst)
                    && sim.party.contains(src)
                {
                    dmg[enc].1 += amount;
                    if *src == p {
                        dmg[enc].0 += amount;
                    }
                }
            }
            EventKind::EncounterCleared { encounter } => {
                if let Some(s) = start[*encounter].take() {
                    let k = kind_index(kind_of(*encounter));
                    let f = &mut r.fights[k];
                    f.0 += 1;
                    f.1 += e.time - s;
                    f.2 += dmg[*encounter].0;
                    f.3 += dmg[*encounter].1;
                    r.combat += e.time - s;
                    let (n, first) = pay[*encounter];
                    let pf = &mut r.payoffs[k];
                    pf.0 += n;
                    if let Some(t) = first {
                        pf.1 += 1;
                        pf.2 += t - s;
                    }
                }
            }
            _ => {}
        }
    }
    r
}

/// Run one lab condition to the end.
#[allow(clippy::too_many_arguments)]
pub fn run_lab(
    content: &Content,
    level: u32,
    seed: u64,
    style: StrikerStyle,
    plan: LabPlan,
    difficulty: &str,
    x: &LabExtras,
    max_seconds: f32,
) -> (Sim, LabRun) {
    let party = lab_party(level);
    let mut sim = lab_sim_with(content, &party, seed, style, plan, difficulty, x);
    sim.run_to_end(max_seconds);
    let r = measure(&sim);
    (sim, r)
}

/// Averages over runs of one condition, for the comparison table.
#[derive(Debug, Clone, Default)]
pub struct LabSummary {
    pub runs: usize,
    pub wins: usize,
    pub duration: f32,
    pub combat: f32,
    pub player_dps: f32,
    pub party_dps: f32,
    pub player_share: f32,
    pub party_deaths: f32,
    pub player_deaths: f32,
    pub damage_taken: f32,
    pub avoidable: f32,
    pub player_interrupts: f32,
    pub party_interrupts: f32,
    pub energy_capped_pct: f32,
    pub ability_uses: [f32; 5],
    pub edge_wasted: f32,
    pub openings: f32,
    pub exploits: f32,
    pub staggers: f32,
    pub stagger_casts: f32,
    /// By fight kind: (fights per run, mean fight seconds, player DPS, party DPS).
    pub fights: [(f32, f32, f32, f32); 3],
    /// By fight kind: (payoffs per fight, share of fights with one, mean
    /// seconds from pull to the first).
    pub payoffs: [(f32, f32, f32); 3],
}

pub fn summarize(runs: &[LabRun]) -> LabSummary {
    let n = runs.len().max(1) as f32;
    let sum = |f: &dyn Fn(&LabRun) -> f32| runs.iter().map(f).sum::<f32>();
    let combat = sum(&|r| r.combat);
    let mut s = LabSummary {
        runs: runs.len(),
        wins: runs.iter().filter(|r| r.won).count(),
        duration: sum(&|r| r.duration) / n,
        combat: combat / n,
        player_dps: sum(&|r| r.player_damage) / combat.max(1.0),
        party_dps: sum(&|r| r.party_damage) / combat.max(1.0),
        player_share: sum(&|r| r.player_damage) / sum(&|r| r.party_damage).max(1.0),
        party_deaths: sum(&|r| r.party_deaths as f32) / n,
        player_deaths: sum(&|r| r.player_deaths as f32) / n,
        damage_taken: sum(&|r| r.damage_taken) / n,
        avoidable: sum(&|r| r.avoidable) / n,
        player_interrupts: sum(&|r| r.player_interrupts as f32) / n,
        party_interrupts: sum(&|r| r.party_interrupts as f32) / n,
        energy_capped_pct: 100.0 * sum(&|r| r.energy_capped) / sum(&|r| r.player_combat).max(1.0),
        edge_wasted: sum(&|r| r.edge_wasted as f32) / n,
        openings: sum(&|r| r.openings as f32) / n,
        exploits: sum(&|r| r.exploits as f32) / n,
        staggers: sum(&|r| r.staggers as f32) / n,
        stagger_casts: sum(&|r| r.stagger_casts as f32) / n,
        ..Default::default()
    };
    for i in 0..5 {
        s.ability_uses[i] = sum(&|r| r.ability_uses[i] as f32) / n;
    }
    for k in 0..3 {
        let fights = sum(&|r| r.fights[k].0 as f32);
        let secs = sum(&|r| r.fights[k].1);
        s.fights[k] = (
            fights / n,
            secs / fights.max(1.0),
            sum(&|r| r.fights[k].2) / secs.max(1.0),
            sum(&|r| r.fights[k].3) / secs.max(1.0),
        );
        let with = sum(&|r| r.payoffs[k].1 as f32);
        s.payoffs[k] = (
            sum(&|r| r.payoffs[k].0 as f32) / fights.max(1.0),
            with / fights.max(1.0),
            sum(&|r| r.payoffs[k].2) / with.max(1.0),
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class::AbilityId;

    #[test]
    fn styles_apply_to_the_player_only_and_classic_is_unchanged() {
        let c = Content::builtin();
        let party = lab_party(3);
        for style in StrikerStyle::ALL {
            let sim = lab_sim(&c, &party, 1, style, LabPlan::Normal, "normal");
            let p = sim.player.unwrap();
            assert_eq!(sim.actors[p].style, style);
            assert_eq!(
                sim.actors[p].abilities,
                ClassId::Striker.abilities_for(style).to_vec()
            );
            // Companion Strikers stay classic.
            for &m in &sim.party {
                if m != p && sim.actors[m].class == Some(ClassId::Striker) {
                    assert_eq!(sim.actors[m].style, StrikerStyle::Classic);
                    assert_eq!(
                        sim.actors[m].abilities,
                        ClassId::Striker.abilities().to_vec()
                    );
                }
            }
        }
        // Classic through the lab path is exactly the classic Striker.
        assert_eq!(
            ClassId::Striker.abilities_for(StrikerStyle::Classic),
            [
                AbilityId::RendStrike,
                AbilityId::Whirl,
                AbilityId::Evade,
                AbilityId::Kick,
                AbilityId::Lacerate
            ]
        );
    }

    #[test]
    fn repetition_counts_distinct_patterns() {
        // One pattern over and over: 1 distinct per 9 windows, top share 1.
        let same = [0u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let (d, t) = repetition(&same);
        assert!((d - 100.0 / 9.0).abs() < 1e-3 && (t - 1.0).abs() < 1e-6);
        let varied = [0u8, 1, 2, 3, 4, 0, 2, 1, 3, 4, 1, 0];
        let (d2, t2) = repetition(&varied);
        assert!(d2 > d && t2 < t);
        assert_eq!(repetition(&[0, 1]), (0.0, 0.0));
    }

    #[test]
    fn feel_sampling_is_consistent_and_reaction_delays_exploits() {
        let c = Content::builtin();
        let quick = run_feel(
            &c,
            3,
            1,
            StrikerStyle::Reactive,
            LabPlan::Expert,
            &LabExtras::reacting(0.0),
            900.0,
        );
        assert!(quick.combat > 0.0 && quick.idle <= quick.combat + 1e-3);
        assert!(quick.waiting <= quick.idle + 1e-3);
        assert_eq!(quick.casts as usize, quick.sequence.len());
        assert!(!quick.exploit_ages.is_empty());
        // A slow player never Exploits an opening younger than its reaction.
        let slow = run_feel(
            &c,
            3,
            1,
            StrikerStyle::Reactive,
            LabPlan::Expert,
            &LabExtras::reacting(1.0),
            900.0,
        );
        assert!(
            slow.exploit_ages.iter().all(|&a| a >= 1.0 - 1e-3),
            "{:?}",
            slow.exploit_ages
        );
        let a = run_feel(
            &c,
            3,
            1,
            StrikerStyle::Planned,
            LabPlan::Expert,
            &LabExtras::reacting(0.0),
            900.0,
        );
        assert!(a.builders > 0 && a.builders_behind <= a.builders);
        assert!(a.edge_at_spend.iter().sum::<u32>() > 0);
    }

    fn gloomroot() -> Content {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/content/dungeons");
        crate::content::ContentLibrary::load(dir)
            .get(crate::experiment::EXPERIMENT_DUNGEON)
            .expect("gloomroot")
            .clone()
    }

    /// A Flow (or other) player standing behind the first enemy, under the
    /// player's control, with plenty of energy.
    fn duel(c: &Content, style: StrikerStyle, x: &LabExtras) -> (Sim, usize, usize) {
        let mut sim = lab_sim_with(c, &lab_party(6), 1, style, LabPlan::Normal, "normal", x);
        let p = sim.player.unwrap();
        sim.actors[p].controller = Controller::Player;
        let e = sim.enemies[0];
        sim.actors[e].hp = 1.0e6;
        sim.actors[e].max_hp = 1.0e6;
        sim.actors[e].facing = 0.0;
        // Behind = on the side the enemy faces away from.
        let pos = sim.actors[e].pos;
        sim.actors[p].pos = pos + crate::math::V2::new(0.0, -1.5);
        sim.actors[p].y = sim.actors[e].y;
        sim.actors[p].surface = sim.actors[e].surface;
        sim.actors[p].resource = 100.0;
        (sim, p, e)
    }

    fn press(
        sim: &mut Sim,
        p: usize,
        slot: usize,
        t: usize,
    ) -> Result<(), crate::events::CastError> {
        sim.actors[p].gcd = 0.0;
        sim.actors[p].resource = 100.0;
        sim.try_cast(p, slot, Some(t))
    }

    #[test]
    fn brood_frenzy_exists_only_with_the_experiment_on() {
        let c = gloomroot();
        let has = |experiment: bool| {
            let x = LabExtras {
                experiment,
                ..Default::default()
            };
            let sim = lab_sim_with(
                &c,
                &lab_party(4),
                1,
                StrikerStyle::Flow,
                LabPlan::Normal,
                "normal",
                &x,
            );
            sim.enemies.iter().any(|&e| {
                sim.actors[e]
                    .enemy
                    .as_ref()
                    .unwrap()
                    .abilities
                    .iter()
                    .any(|a| &*a.label == "Brood Frenzy")
            })
        };
        assert!(!has(false));
        assert!(has(true));
    }

    #[test]
    fn flow_builds_edge_from_cuts_and_seizes_and_ravage_needs_two() {
        use crate::events::CastError;
        let c = gloomroot();
        let (mut sim, p, e) = duel(&c, StrikerStyle::Flow, &LabExtras::default());
        assert_eq!(
            sim.actors[p].abilities,
            ClassId::Striker.abilities_for(StrikerStyle::Flow).to_vec()
        );
        assert_eq!(press(&mut sim, p, 1, e), Err(CastError::NoEdge));
        assert_eq!(press(&mut sim, p, 4, e), Err(CastError::NoOpening));
        // Quick Cut from behind: +2 Edge when it lands (it can be dodged).
        let mut tries = 0;
        while sim.edge(p) < 2 && tries < 20 {
            press(&mut sim, p, 0, e).unwrap();
            tries += 1;
        }
        assert!(sim.edge(p) >= 2);
        let before = sim.metrics.actors[p].builders_behind;
        assert!(before > 0, "the cuts landed from behind");
        // Ravage spends it all.
        press(&mut sim, p, 1, e).unwrap();
        assert_eq!(sim.edge(p), 0);
        // An opening: Seize takes it and gives 2 Edge.
        sim.lab_open_for_test(e, "Recovering", 3.0);
        assert!(sim.has_opening(e, p));
        let mut seized = false;
        for _ in 0..10 {
            if !sim.has_opening(e, p) {
                break;
            }
            press(&mut sim, p, 4, e).unwrap();
            seized = sim.edge(p) == 2;
        }
        assert!(seized || sim.metrics.actors[p].exploits > 0);
    }

    #[test]
    fn weak_point_hits_break_brood_frenzy_and_expose_her() {
        let c = gloomroot();
        let x = LabExtras {
            experiment: true,
            ..Default::default()
        };
        let mut sim = lab_sim_with(
            &c,
            &lab_party(4),
            1,
            StrikerStyle::Flow,
            LabPlan::Normal,
            "normal",
            &x,
        );
        let p = sim.player.unwrap();
        sim.actors[p].controller = Controller::Player;
        // Mother Brakkis: the enemy with Brood Frenzy.
        let (boss, idx) = sim
            .enemies
            .iter()
            .find_map(|&e| {
                sim.actors[e]
                    .enemy
                    .as_ref()?
                    .abilities
                    .iter()
                    .position(|a| &*a.label == "Brood Frenzy")
                    .map(|i| (e, i))
            })
            .unwrap();
        // Start the channel by hand.
        sim.actors[boss].cast = Some(crate::sim::Cast {
            kind: crate::sim::CastKind::Enemy(idx),
            name: crate::events::Label::from("Brood Frenzy"),
            target: p,
            remaining: 4.0,
            total: 4.0,
            interruptible: false,
        });
        let need = sim
            .stagger_left(boss, sim.actors[boss].cast.as_ref().unwrap())
            .unwrap();
        assert!(
            (need - 1200.0).abs() < 1e-3,
            "level 4 = recommended: {need}"
        );
        // An ordinary hit counts once.
        sim.stagger_hit_for_test(boss, p, "Quick Cut", 300.0);
        let left = sim
            .stagger_left(boss, sim.actors[boss].cast.as_ref().unwrap())
            .unwrap();
        assert!((left - 900.0).abs() < 1e-3, "{left}");
        // A Ravage counts triple and breaks it.
        sim.stagger_hit_for_test(boss, p, "Ravage", 300.0);
        assert!(sim.actors[boss].cast.is_none(), "broken");
        assert_eq!(sim.metrics.staggers, 1);
        assert!(
            sim.events
                .iter()
                .any(|ev| matches!(ev.kind, EventKind::Staggered { .. }))
        );
        assert!(sim.actors[boss].auras.iter().any(|a| &*a.name == "Exposed"));
        assert!(
            sim.has_opening(boss, p),
            "the break opens her for the Flow Striker"
        );
    }

    #[test]
    fn techniques_change_how_flow_plays_and_only_flow() {
        use crate::class::Technique;
        let c = gloomroot();
        let patience = LabExtras {
            technique: Some(Technique::Patience),
            ..Default::default()
        };
        let (mut sim, p, e) = duel(&c, StrikerStyle::Flow, &patience);
        assert_eq!(sim.technique(p), Some(Technique::Patience));
        assert_eq!(sim.edge_cap(p), 7);
        sim.lab_open_for_test(e, "Recovering", 3.0);
        for _ in 0..10 {
            if !sim.has_opening(e, p) {
                break;
            }
            press(&mut sim, p, 4, e).unwrap();
        }
        assert!(sim.edge(p) <= 1, "Patience: Seize gives 1 Edge");
        // Riposte: Kick's cooldown is 6 s.
        let riposte = LabExtras {
            technique: Some(Technique::Riposte),
            ..Default::default()
        };
        let (mut sim, p, e) = duel(&c, StrikerStyle::Flow, &riposte);
        press(&mut sim, p, 3, e).unwrap();
        assert!((sim.actors[p].cooldowns[3] - 6.0).abs() < 1e-3);
        // A Technique does nothing for other styles.
        let (sim, p, _) = duel(&c, StrikerStyle::Reactive, &riposte);
        assert_eq!(sim.technique(p), None);
        assert_eq!(sim.edge_cap(p), crate::sim::MAX_EDGE);
    }

    #[test]
    fn every_lab_ability_has_a_full_tooltip() {
        use crate::class::ResourceKind;
        for style in StrikerStyle::ALL {
            for a in ClassId::Striker.abilities_for(style) {
                let t = crate::describe::ability(&a.def(), 1.0, ResourceKind::Energy);
                assert!(t.body.lines().count() >= 3, "{}: {}", t.title, t.body);
            }
        }
    }

    #[test]
    fn lab_runs_are_deterministic() {
        let c = Content::builtin();
        for style in StrikerStyle::ALL {
            let (a, ra) = run_lab(
                &c,
                3,
                7,
                style,
                LabPlan::Expert,
                "normal",
                &LabExtras::default(),
                900.0,
            );
            let (b, rb) = run_lab(
                &c,
                3,
                7,
                style,
                LabPlan::Expert,
                "normal",
                &LabExtras::default(),
                900.0,
            );
            assert_eq!(a.events.len(), b.events.len());
            assert_eq!(a.events, b.events, "{style:?}");
            assert_eq!(ra.player_damage, rb.player_damage);
        }
    }

    #[test]
    fn planned_builds_and_spends_edge() {
        let c = Content::builtin();
        let (sim, r) = run_lab(
            &c,
            3,
            1,
            StrikerStyle::Planned,
            LabPlan::Expert,
            "normal",
            &LabExtras::default(),
            900.0,
        );
        let p = sim.player.unwrap();
        let names: Vec<&str> = sim
            .events
            .iter()
            .filter_map(|e| match &e.kind {
                EventKind::AuraApplied { src, what, .. } if *src == p => Some(&**what),
                _ => None,
            })
            .collect();
        assert!(names.contains(&"Edge"), "no Edge gained");
        assert!(r.ability_uses[0] > 0, "no Honed Strike");
        assert!(
            r.ability_uses[1] + r.ability_uses[4] > 0,
            "Edge never spent: {:?}",
            r.ability_uses
        );
        assert!(r.player_damage > 0.0);
    }

    #[test]
    fn reactive_sees_and_takes_openings() {
        let c = Content::builtin();
        let (_, r) = run_lab(
            &c,
            3,
            1,
            StrikerStyle::Reactive,
            LabPlan::Expert,
            "normal",
            &LabExtras::default(),
            900.0,
        );
        assert!(r.openings > 0, "no openings");
        assert!(r.exploits > 0, "no Exploit used");
        assert!(r.exploits <= r.openings);
        // Classic and A never create openings.
        for style in [StrikerStyle::Classic, StrikerStyle::Planned] {
            let (_, r) = run_lab(
                &c,
                3,
                1,
                style,
                LabPlan::Expert,
                "normal",
                &LabExtras::default(),
                900.0,
            );
            assert_eq!(r.openings, 0);
            assert_eq!(r.exploits, 0);
        }
    }

    #[test]
    fn passive_player_uses_no_abilities() {
        let c = Content::builtin();
        for style in StrikerStyle::ALL {
            let (_, r) = run_lab(
                &c,
                3,
                1,
                style,
                LabPlan::Passive,
                "normal",
                &LabExtras::default(),
                900.0,
            );
            assert_eq!(r.ability_uses, [0; 5], "{style:?}");
            assert_eq!(r.player_interrupts, 0);
            assert!(r.player_damage > 0.0, "auto-attacks still land");
        }
    }

    #[test]
    fn exploit_needs_an_opening_and_deep_lacerate_needs_edge() {
        use crate::events::CastError;
        let c = Content::builtin();
        let party = lab_party(3);
        let mut b = lab_sim(
            &c,
            &party,
            1,
            StrikerStyle::Reactive,
            LabPlan::Normal,
            "normal",
        );
        let p = b.player.unwrap();
        b.actors[p].controller = Controller::Player;
        let e = b.enemies[0];
        let pos = b.actors[e].pos;
        b.actors[p].pos = pos + crate::math::V2::new(0.0, -1.0);
        assert_eq!(b.try_cast(p, 4, Some(e)), Err(CastError::NoOpening));
        let mut a = lab_sim(
            &c,
            &party,
            1,
            StrikerStyle::Planned,
            LabPlan::Normal,
            "normal",
        );
        let p = a.player.unwrap();
        a.actors[p].controller = Controller::Player;
        a.actors[p].pos = pos + crate::math::V2::new(0.0, -1.0);
        assert_eq!(a.try_cast(p, 4, Some(e)), Err(CastError::NoEdge));
    }
}
