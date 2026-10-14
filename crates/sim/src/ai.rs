//! Rule-based / utility-scored AI for companions and enemies.
//!
//! Every decision is a pure function of the current `Sim` state, so AI behaviour
//! is deterministic and testable. See RULES.md for the role behaviours in prose.

use crate::class::{AbilityId, Role};
use crate::content::EnemyRank;
use crate::math::V2;
use crate::sim::{ActorId, EnemyMode, Sim};

/// Seconds an AI needs to notice a new ground telegraph before reacting.
pub const REACTION_TIME: f32 = 0.5;
/// How far behind the leader followers stand.
const FORMATION: [(f32, f32); 4] = [(-2.5, -3.0), (2.5, -3.0), (-2.5, -6.0), (2.5, -6.0)];
/// Ranged members stand roughly this far from their enemy.
const RANGED_DISTANCE: f32 = 18.0;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Decision {
    /// `Some(x)` sets the current target to `x`; `None` leaves it unchanged.
    pub target: Option<Option<ActorId>>,
    pub move_goal: Option<V2>,
    /// Ability slot and target to attempt this tick.
    pub cast: Option<(usize, ActorId)>,
    pub stop_cast: bool,
    pub auto_attack: bool,
}

fn slot(sim: &Sim, id: ActorId, ability: AbilityId) -> Option<usize> {
    sim.actor(id).ability_slot(ability)
}

/// `Some((slot, target))` if the ability is ready and the target is in range.
fn usable(sim: &Sim, id: ActorId, ability: AbilityId, target: ActorId) -> Option<(usize, ActorId)> {
    let s = slot(sim, id, ability)?;
    let a = sim.actor(id);
    if !a.ability_ready(s) || a.cast.is_some() {
        return None;
    }
    let def = ability.def();
    if def.target != crate::class::TargetRule::SelfOnly && !sim.reach(id, target, def.range) {
        return None;
    }
    Some((s, target))
}

/// If standing in (or about to stand in) a telegraph that we have noticed, a safe point.
pub fn escape_point(sim: &Sim, id: ActorId) -> Option<V2> {
    let me = sim.actor(id);
    let danger = sim.telegraphs.iter().find(|t| {
        t.elapsed() >= REACTION_TIME && me.pos.dist(t.center) <= t.radius + me.radius + 0.3
    })?;
    let safe_dist = danger.radius + me.radius + 1.5;
    let mut dir = (me.pos - danger.center).normalize_or_zero();
    if dir.len_sq() == 0.0 {
        dir = V2::from_heading(id as f32 * 1.7);
    }
    let mut best: Option<V2> = None;
    for k in 0..12 {
        // 0, +30, -30, +60, -60, ... degrees around the direct escape route.
        let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
        let angle = sign * ((k + 1) / 2) as f32 * std::f32::consts::PI / 6.0;
        let p = sim
            .nav
            .project(danger.center + dir.rotate(angle) * safe_dist);
        let safe = sim
            .telegraphs
            .iter()
            .all(|t| p.dist(t.center) > t.radius + me.radius + 0.2);
        if safe && best.is_none_or(|b| p.dist(me.pos) < b.dist(me.pos)) {
            best = Some(p);
        }
    }
    best
}

pub fn party_decision(sim: &Sim, id: ActorId) -> Decision {
    let me = sim.actor(id);
    if let Some(escape) = escape_point(sim, id) {
        return Decision {
            move_goal: Some(escape),
            stop_cast: me.cast.is_some(),
            auto_attack: true,
            ..Default::default()
        };
    }
    let engaged = sim.engaged_enemies();
    if engaged.is_empty() {
        return out_of_combat(sim, id);
    }
    match me.role() {
        Some(Role::Tank) => tank(sim, id, &engaged),
        Some(Role::Healer) => healer(sim, id, &engaged),
        _ => dps(sim, id, &engaged),
    }
}

fn follow_point(sim: &Sim, id: ActorId) -> Option<V2> {
    let leader = sim.actor(sim.leader);
    if !leader.alive || id == sim.leader {
        return None;
    }
    let index = sim
        .party
        .iter()
        .filter(|&&p| p != sim.leader)
        .position(|&p| p == id)?;
    let (ox, oz) = FORMATION[index % FORMATION.len()];
    let fwd = sim.leader_heading;
    let right = V2::new(fwd.z, -fwd.x);
    Some(sim.nav.project(leader.pos + right * ox + fwd * oz))
}

fn out_of_combat(sim: &Sim, id: ActorId) -> Decision {
    let me = sim.actor(id);
    let mut d = Decision {
        target: Some(None),
        ..Default::default()
    };
    if id == sim.leader {
        if sim.autopilot {
            return autopilot(sim, id);
        }
        return d;
    }
    // Healers top the party off between pulls.
    if me.role() == Some(Role::Healer) && me.cast.is_none() {
        let hurt = sim
            .alive_party()
            .filter(|&p| sim.actor(p).hp_frac() < 0.7)
            .min_by(|&a, &b| sim.actor(a).hp_frac().total_cmp(&sim.actor(b).hp_frac()));
        if let Some(p) = hurt
            && me.mana_frac() > 0.3
            && let Some(c) = usable(sim, id, AbilityId::Mend, p)
        {
            d.cast = Some(c);
            return d;
        }
    }
    if me.cast.is_some() {
        return d;
    }
    if let Some(goal) = follow_point(sim, id)
        && goal.dist(me.pos) > 1.5
    {
        d.move_goal = Some(goal);
    }
    d
}

/// Leader autopilot used in headless runs: wait for the party to be ready and
/// gathered, then walk to the next uncleared encounter and pull it.
fn autopilot(sim: &Sim, id: ActorId) -> Decision {
    let me = sim.actor(id);
    let mut d = Decision {
        target: Some(None),
        ..Default::default()
    };
    let Some(next) = sim.encounters.iter().position(|e| !e.cleared) else {
        return d;
    };
    let ready = sim.alive_party().all(|p| {
        let a = sim.actor(p);
        a.hp_frac() >= 0.9 && (a.max_mana < 400.0 || a.mana_frac() >= 0.8)
    });
    let gathered = sim
        .alive_party()
        .all(|p| sim.actor(p).pos.dist(me.pos) <= 12.0);
    if !ready || !gathered {
        return d;
    }
    let mobs: Vec<ActorId> = sim.encounters[next]
        .mobs
        .iter()
        .copied()
        .filter(|&m| sim.actor(m).alive && sim.actor(m).mode() == Some(EnemyMode::Idle))
        .collect();
    let Some(&nearest) = mobs.iter().min_by(|&&a, &&b| {
        sim.actor(a)
            .pos
            .dist(me.pos)
            .total_cmp(&sim.actor(b).pos.dist(me.pos))
    }) else {
        return d;
    };
    if let Some(c) = usable(sim, id, AbilityId::Challenge, nearest)
        && sim.actor(nearest).pos.dist(me.pos) <= 22.0
    {
        d.cast = Some(c);
        d.target = Some(Some(nearest));
        return d;
    }
    d.move_goal = Some(sim.actor(nearest).pos);
    d
}

fn rank_weight(sim: &Sim, e: ActorId) -> f32 {
    match sim.actor(e).enemy.as_ref().map(|s| s.rank) {
        Some(EnemyRank::Boss) => 3.0,
        Some(EnemyRank::Elite) => 2.0,
        _ => 1.0,
    }
}

/// Tank: pick up loose enemies (taunt if possible), otherwise hold the enemy on
/// which our threat lead is smallest. Bulwark when low.
fn tank(sim: &Sim, id: ActorId, engaged: &[ActorId]) -> Decision {
    let me = sim.actor(id);
    let mut d = Decision {
        auto_attack: true,
        ..Default::default()
    };
    // Enemies attacking someone else, most dangerous first (healer, then by rank).
    let mut loose: Vec<ActorId> = engaged
        .iter()
        .copied()
        .filter(|&e| sim.actor(e).target.is_some_and(|t| t != id))
        .collect();
    loose.sort_by(|&a, &b| {
        let score = |e: ActorId| {
            let on_healer = sim
                .actor(e)
                .target
                .is_some_and(|t| sim.actor(t).role() == Some(Role::Healer));
            (if on_healer { 10.0 } else { 0.0 }) + rank_weight(sim, e)
                - sim.actor(e).pos.dist(me.pos) * 0.01
        };
        score(b).total_cmp(&score(a)).then(a.cmp(&b))
    });
    let target = if let Some(&e) = loose.first() {
        if let Some(c) = usable(sim, id, AbilityId::Challenge, e) {
            d.cast = Some(c);
        }
        e
    } else {
        // Smallest threat lead: our threat minus the best other threat.
        let lead = |e: ActorId| {
            let mine = sim.threat(e, id);
            let other = sim
                .actor(e)
                .enemy
                .as_ref()
                .map(|s| {
                    s.threat
                        .iter()
                        .filter(|t| t.actor != id)
                        .map(|t| t.value)
                        .fold(0.0, f32::max)
                })
                .unwrap_or(0.0);
            mine - other
        };
        let current = me.target.filter(|t| engaged.contains(t));
        let best = engaged
            .iter()
            .copied()
            .min_by(|&a, &b| lead(a).total_cmp(&lead(b)).then(a.cmp(&b)))
            .expect("engaged is non-empty");
        match current {
            // Stick with the current target unless another is clearly more at risk.
            Some(c) if lead(c) <= lead(best) + 150.0 => c,
            _ => best,
        }
    };
    d.target = Some(Some(target));
    if d.cast.is_none() {
        if me.hp_frac() < 0.5
            && let Some(c) = usable(sim, id, AbilityId::Bulwark, id)
        {
            d.cast = Some(c);
        } else if let Some(c) = usable(sim, id, AbilityId::ShieldSlam, target) {
            d.cast = Some(c);
        }
    }
    if !sim.reach(id, target, 1.6) {
        d.move_goal = Some(sim.actor(target).pos);
    }
    d
}

fn healer(sim: &Sim, id: ActorId, engaged: &[ActorId]) -> Decision {
    let me = sim.actor(id);
    let mut d = Decision::default();
    // Cancel a big heal that would be pure overheal.
    if let Some(c) = me.cast {
        let t = sim.actor(c.target);
        if c.name == "Mend" && t.hp_frac() > 0.97 && c.remaining > 0.3 {
            d.stop_cast = true;
        } else {
            return d;
        }
    }
    let tank = sim.tank();
    let mana = me.mana_frac();
    // Effective urgency: tank and self are weighted as slightly lower HP.
    let urgency = |p: ActorId| {
        let a = sim.actor(p);
        let mut f = a.hp_frac();
        if Some(p) == tank {
            f -= 0.1;
        }
        if p == id {
            f -= 0.05;
        }
        f
    };
    let mut allies: Vec<ActorId> = sim.alive_party().collect();
    allies.sort_by(|&a, &b| urgency(a).total_cmp(&urgency(b)).then(a.cmp(&b)));
    let mend_threshold = if mana < 0.2 { 0.35 } else { 0.6 };
    let mut out_of_range_patient = None;
    for &p in &allies {
        let a = sim.actor(p);
        if a.hp_frac() < mend_threshold && mana > 0.07 {
            if let Some(c) = usable(sim, id, AbilityId::Mend, p) {
                d.cast = Some(c);
                break;
            } else if !sim.reach(id, p, 30.0) {
                out_of_range_patient.get_or_insert(p);
            }
        }
    }
    if d.cast.is_none() && mana > 0.25 {
        for &p in &allies {
            let a = sim.actor(p);
            let wants_hot =
                a.hp_frac() < 0.85 || (Some(p) == tank && a.hp_frac() < 0.97 && mana > 0.5);
            if wants_hot
                && !a.has_hot_from(id)
                && let Some(c) = usable(sim, id, AbilityId::Renew, p)
            {
                d.cast = Some(c);
                break;
            }
        }
    }
    if d.cast.is_none()
        && mana > 0.75
        && let Some(t) = tank.and_then(|t| sim.actor(t).target)
        && engaged.contains(&t)
        && sim.threat(t, tank.unwrap_or(id)) > 300.0
        && let Some(c) = usable(sim, id, AbilityId::Smite, t)
    {
        d.cast = Some(c);
    }
    // Positioning: stay behind the tank at range, close enough to reach everyone.
    let casting_now = d
        .cast
        .is_some_and(|(s, _)| me.abilities[s].def().cast_time > 0.0);
    if !casting_now {
        if let Some(p) = out_of_range_patient {
            d.move_goal = Some(sim.actor(p).pos);
        } else if let Some(goal) = ranged_spot(sim, id, engaged, 15.0) {
            let far_from_party = sim.alive_party().any(|p| !sim.reach(id, p, 26.0));
            if goal.dist(me.pos) > 6.0 || far_from_party {
                d.move_goal = Some(goal);
            }
        }
    }
    d
}

/// A spot `dist` away from the tank's enemy on the tank's side, offset sideways per actor.
fn ranged_spot(sim: &Sim, id: ActorId, engaged: &[ActorId], dist: f32) -> Option<V2> {
    let tank = sim.tank();
    let enemy = tank
        .and_then(|t| sim.actor(t).target)
        .filter(|e| engaged.contains(e))
        .or_else(|| engaged.first().copied())?;
    let epos = sim.actor(enemy).pos;
    let anchor = tank.map(|t| sim.actor(t).pos).unwrap_or(sim.actor(id).pos);
    let mut dir = (anchor - epos).normalize_or_zero();
    if dir.len_sq() == 0.0 {
        dir = (sim.actor(id).pos - epos).normalize_or_zero();
    }
    let side = if id.is_multiple_of(2) { 0.5 } else { -0.5 };
    Some(sim.nav.project(epos + dir.rotate(side) * dist))
}

fn dps(sim: &Sim, id: ActorId, engaged: &[ActorId]) -> Decision {
    let me = sim.actor(id);
    let ranged = me.role() == Some(Role::RangedDps);
    let mut d = Decision::default();
    if me.cast.is_some() {
        return d;
    }
    let tank = sim.tank();
    let tank_target = tank.and_then(|t| sim.actor(t).target);
    // Utility score: assist the tank, finish low enemies, prefer trash, stickiness.
    let score = |e: ActorId| {
        let a = sim.actor(e);
        let mut s = 0.0;
        if Some(e) == tank_target {
            s += 10.0;
        }
        s += 6.0 * (1.0 - a.hp_frac());
        if a.enemy.as_ref().is_some_and(|x| x.rank == EnemyRank::Trash) {
            s += 2.0;
        }
        if me.target == Some(e) {
            s += 3.0;
        }
        s - a.pos.dist(me.pos) * 0.02
    };
    let target = engaged
        .iter()
        .copied()
        .max_by(|&a, &b| score(a).total_cmp(&score(b)).then(b.cmp(&a)))
        .expect("engaged is non-empty");
    d.target = Some(Some(target));
    let t = sim.actor(target);

    // Threat discipline.
    let mut hold = false;
    if let Some(tank) = tank {
        let tank_threat = sim.threat(target, tank);
        let my_threat = sim.threat(target, id);
        let on_me = t.target == Some(id);
        let danger = my_threat > 0.9 * tank_threat;
        if (on_me || danger) && my_threat > 100.0 {
            let drop = if ranged {
                AbilityId::Veil
            } else {
                AbilityId::Evade
            };
            d.cast = usable(sim, id, drop, id);
        }
        // Let the tank open and keep a lead, unless the enemy is already on us.
        hold = !on_me && (tank_threat < 1.0 || danger);
    }
    d.auto_attack = !hold;

    if ranged {
        if !sim.reach(id, target, 26.0) {
            d.move_goal = ranged_spot(sim, id, engaged, RANGED_DISTANCE).or(Some(t.pos));
        }
        if !hold && d.cast.is_none() {
            d.cast = usable(sim, id, AbilityId::Flare, target)
                .or_else(|| usable(sim, id, AbilityId::ArcBolt, target));
        }
        if d.cast
            .is_some_and(|(s, _)| me.abilities[s].def().cast_time > 0.0)
        {
            d.move_goal = None;
        }
    } else {
        // Melee: get behind the target relative to the tank.
        let anchor = tank.map(|t| sim.actor(t).pos).unwrap_or(me.pos);
        let behind = (t.pos - anchor).normalize_or_zero();
        let spot = sim
            .nav
            .project(t.pos + behind * (t.radius + me.radius + 1.0));
        if !sim.reach(id, target, 1.6) || spot.dist(me.pos) > 2.5 && tank.is_some() {
            d.move_goal = Some(spot);
        }
        if !hold && d.cast.is_none() {
            let near: Vec<ActorId> = engaged
                .iter()
                .copied()
                .filter(|&e| sim.actor(e).pos.dist(me.pos) <= 6.0 + sim.actor(e).radius)
                .collect();
            // Only AoE when the tank holds a comfortable lead on everything we'd hit.
            let aoe_safe = tank.is_some_and(|tank| {
                near.iter()
                    .all(|&e| sim.threat(e, tank) > 2.0 * sim.threat(e, id) + 150.0)
            });
            if near.len() >= 2 && aoe_safe {
                d.cast = usable(sim, id, AbilityId::Whirl, id);
            }
            if d.cast.is_none() {
                d.cast = usable(sim, id, AbilityId::RendStrike, target);
            }
        }
    }
    d
}

pub fn enemy_decision(sim: &Sim, id: ActorId) -> Decision {
    let me = sim.actor(id);
    let es = me.enemy.as_ref().expect("enemy");
    match es.mode {
        EnemyMode::Idle => Decision {
            auto_attack: false,
            ..Default::default()
        },
        EnemyMode::Resetting => Decision {
            move_goal: Some(es.home),
            target: Some(None),
            ..Default::default()
        },
        EnemyMode::Engaged => {
            let target = sim.select_enemy_target(id);
            let mut d = Decision {
                target: Some(target),
                auto_attack: true,
                ..Default::default()
            };
            if let Some(t) = target
                && me.cast.is_none()
                && !sim.reach(id, t, me.attack_range * 0.8)
            {
                d.move_goal = Some(sim.actor(t).pos);
            }
            d
        }
    }
}
