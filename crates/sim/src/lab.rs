//! Class Design Lab (0.10): measure whether playing a Striker style well
//! makes a difference. The player's slot is played by a scripted plan
//! (`LabPlan`); companions run their ordinary AI in every condition. This
//! measures effectiveness, not enjoyment (CLASS_DESIGN_LAB_0_10.md).

use crate::ai::AiProfile;
use crate::class::{ClassId, StrikerStyle};
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
    let mut sim = Sim::with_options(
        content,
        party,
        seed,
        RunOptions {
            difficulty: difficulty.to_string(),
            autopilot: true,
            player_style: style,
            lab_plan: plan,
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
    /// By fight kind: (fights, seconds, player damage, party damage).
    pub fights: [(u32, f32, f32, f32); 3],
    /// By fight kind: (payoffs, fights with one, seconds from pull to the
    /// first). A payoff is the style's big moment: a (Deep) Lacerate, a
    /// Reaping Whirl or an Exploit.
    pub payoffs: [(u32, u32, f32); 3],
}

/// The payoff abilities of the three styles (see `LabRun::payoffs`).
pub const PAYOFFS: [&str; 4] = ["Lacerate", "Deep Lacerate", "Reaping Whirl", "Exploit"];

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
    for e in &sim.events {
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
                        != matches!(&**what, "Reaping Whirl" | "Exploit")) =>
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
pub fn run_lab(
    content: &Content,
    level: u32,
    seed: u64,
    style: StrikerStyle,
    plan: LabPlan,
    difficulty: &str,
    max_seconds: f32,
) -> (Sim, LabRun) {
    let party = lab_party(level);
    let mut sim = lab_sim(content, &party, seed, style, plan, difficulty);
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
            let (a, ra) = run_lab(&c, 3, 7, style, LabPlan::Expert, "normal", 900.0);
            let (b, rb) = run_lab(&c, 3, 7, style, LabPlan::Expert, "normal", 900.0);
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
            900.0,
        );
        assert!(r.openings > 0, "no openings");
        assert!(r.exploits > 0, "no Exploit used");
        assert!(r.exploits <= r.openings);
        // Classic and A never create openings.
        for style in [StrikerStyle::Classic, StrikerStyle::Planned] {
            let (_, r) = run_lab(&c, 3, 1, style, LabPlan::Expert, "normal", 900.0);
            assert_eq!(r.openings, 0);
            assert_eq!(r.exploits, 0);
        }
    }

    #[test]
    fn passive_player_uses_no_abilities() {
        let c = Content::builtin();
        for style in StrikerStyle::ALL {
            let (_, r) = run_lab(&c, 3, 1, style, LabPlan::Passive, "normal", 900.0);
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
