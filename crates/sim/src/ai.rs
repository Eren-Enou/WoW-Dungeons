//! Rule-based / utility-scored AI for companions and enemies.
//!
//! Every tick the sim computes a shared [`Blackboard`] (party-level tactical
//! information: kill target, interrupt assignments, crowd-control assignment,
//! danger zones, healer mana, pull readiness). Each companion then makes its own
//! decision from the blackboard plus its role and [`AiProfile`]. Everything is a
//! pure function of sim state, so AI behaviour is deterministic and testable.
//! See RULES.md for the behaviours in prose.

use crate::class::{AbilityId, Effect, Role};
use crate::content::{AbilityKind, EnemyRank};
use crate::math::{V2, V3};
use crate::sim::{ActorId, AuraKind, CastKind, EFFECT_HEIGHT, EnemyMode, Sim};
use serde::{Deserialize, Serialize};

/// How far behind the leader followers stand (local x = right, z = forward).
const FORMATION: [(f32, f32); 8] = [
    (-2.5, -3.0),
    (2.5, -3.0),
    (-2.5, -6.0),
    (2.5, -6.0),
    (0.0, -4.5),
    (-5.0, -4.5),
    (5.0, -4.5),
    (0.0, -8.0),
];
/// Stacks of an enemy's damage-taken debuff at which tanks swap.
pub const TANK_SWAP_STACKS: u32 = 3;
/// Seconds after a pull during which healers avoid non-urgent heals.
const PULL_GRACE: f32 = 4.0;
/// Ranged members stand roughly this far from their enemy.
const RANGED_DISTANCE: f32 = 18.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AiProfile {
    /// Waits longer before attacking, keeps big threat margins, heals early.
    Conservative,
    #[default]
    Balanced,
    /// Attacks immediately, ignores threat margins, skips crowd control.
    Aggressive,
    /// Prioritises dispels, shields and early interrupts over damage.
    SupportFocused,
    /// Fast reactions and wide safety margins around mechanics; interrupts early.
    MechanicFocused,
}

/// Tunable numbers behind a profile. All behaviour differences come from here.
#[derive(Debug, Clone, Copy)]
pub struct ProfileParams {
    /// Seconds before a new telegraph is noticed.
    pub reaction_time: f32,
    /// DPS hold fire when their threat exceeds this fraction of the tank's.
    pub threat_cap: f32,
    /// DPS wait until the tank has at least this much threat on the target.
    pub min_tank_threat: f32,
    /// Tank pops Bulwark below this HP fraction.
    pub defensive_hp: f32,
    /// Healer casts Mend on allies below this HP fraction.
    pub mend_threshold: f32,
    /// Healer applies Renew below this HP fraction.
    pub hot_threshold: f32,
    /// Healer only Smites above this mana fraction.
    pub smite_mana: f32,
    /// Leader pulls only when everyone is above this HP / mana fraction.
    pub ready_hp: f32,
    pub ready_mana: f32,
    /// Interrupt once the enemy cast has this many seconds left (bigger = earlier).
    pub interrupt_window: f32,
    /// Extra metres kept from danger zones.
    pub danger_margin: f32,
    pub use_cc: bool,
    /// Healer dispels before non-emergency healing.
    pub dispel_first: bool,
}

impl AiProfile {
    pub const ALL: [AiProfile; 5] = [
        AiProfile::Conservative,
        AiProfile::Balanced,
        AiProfile::Aggressive,
        AiProfile::SupportFocused,
        AiProfile::MechanicFocused,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AiProfile::Conservative => "Conservative",
            AiProfile::Balanced => "Balanced",
            AiProfile::Aggressive => "Aggressive",
            AiProfile::SupportFocused => "Support",
            AiProfile::MechanicFocused => "Mechanics",
        }
    }

    pub fn parse(s: &str) -> Option<AiProfile> {
        match s.to_ascii_lowercase().as_str() {
            "conservative" => Some(AiProfile::Conservative),
            "balanced" => Some(AiProfile::Balanced),
            "aggressive" => Some(AiProfile::Aggressive),
            "support" | "support_focused" => Some(AiProfile::SupportFocused),
            "mechanic" | "mechanics" | "mechanic_focused" => Some(AiProfile::MechanicFocused),
            _ => None,
        }
    }

    pub fn next(self) -> AiProfile {
        let i = AiProfile::ALL.iter().position(|p| *p == self).unwrap_or(0);
        AiProfile::ALL[(i + 1) % AiProfile::ALL.len()]
    }

    pub fn params(self) -> ProfileParams {
        let balanced = ProfileParams {
            reaction_time: 0.45,
            threat_cap: 0.9,
            min_tank_threat: 40.0,
            defensive_hp: 0.5,
            mend_threshold: 0.6,
            hot_threshold: 0.85,
            smite_mana: 0.75,
            ready_hp: 0.9,
            ready_mana: 0.8,
            interrupt_window: 1.0,
            danger_margin: 0.6,
            use_cc: true,
            dispel_first: false,
        };
        match self {
            AiProfile::Balanced => balanced,
            AiProfile::Conservative => ProfileParams {
                reaction_time: 0.5,
                threat_cap: 0.7,
                min_tank_threat: 200.0,
                defensive_hp: 0.65,
                mend_threshold: 0.7,
                hot_threshold: 0.92,
                smite_mana: 0.95,
                ready_hp: 0.97,
                ready_mana: 0.95,
                danger_margin: 1.2,
                ..balanced
            },
            AiProfile::Aggressive => ProfileParams {
                reaction_time: 0.75,
                threat_cap: 1.05,
                min_tank_threat: 0.0,
                defensive_hp: 0.3,
                mend_threshold: 0.5,
                hot_threshold: 0.75,
                smite_mana: 0.45,
                ready_hp: 0.7,
                ready_mana: 0.5,
                interrupt_window: 0.5,
                danger_margin: 0.2,
                use_cc: false,
                ..balanced
            },
            AiProfile::SupportFocused => ProfileParams {
                threat_cap: 0.85,
                mend_threshold: 0.7,
                hot_threshold: 0.92,
                smite_mana: 0.95,
                interrupt_window: 1.6,
                dispel_first: true,
                ..balanced
            },
            AiProfile::MechanicFocused => ProfileParams {
                reaction_time: 0.2,
                interrupt_window: 1.8,
                danger_margin: 1.8,
                ..balanced
            },
        }
    }
}

/// Profile parameters adjusted by what this character has learned:
/// * familiarity (0..1, from past clears of this dungeon): up to 40% faster
///   reactions, +0.6 m danger margin and +0.4 s interrupt window;
/// * alertness (this run, from avoidable hits taken): reaction time − alertness.
pub fn params_of(a: &crate::sim::Actor) -> ProfileParams {
    let mut p = a.profile.params();
    let f = a.familiarity.clamp(0.0, 1.0);
    p.reaction_time = (p.reaction_time * (1.0 - 0.4 * f) - a.alertness).max(0.1);
    p.danger_margin += 0.6 * f;
    p.interrupt_window += 0.4 * f;
    p
}

/// A circle to stay out of: a telegraph or an (about to be) active hazard.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Danger {
    pub center: V2,
    /// Elevation of the floor the effect is on.
    pub y: f32,
    pub radius: f32,
    /// Seconds since it appeared (telegraph) or 99 for hazards.
    pub age: f32,
}

/// Shared party tactical state, recomputed every tick from sim state.
#[derive(Debug, Clone, Default)]
pub struct Blackboard {
    pub engaged: Vec<ActorId>,
    pub tanks: Vec<ActorId>,
    pub healers: Vec<ActorId>,
    /// Focus target for damage dealers.
    pub kill_target: Option<ActorId>,
    /// Enemy that should be incapacitated, and who should do it.
    pub cc_target: Option<ActorId>,
    pub cc_owner: Option<ActorId>,
    /// (enemy casting something interruptible, assigned interrupter).
    pub interrupts: Vec<(ActorId, ActorId)>,
    /// Non-tank party members currently being attacked.
    pub threatened: Vec<ActorId>,
    pub healer_needs_mana: bool,
    /// An enemy is casting a party-wide pulse.
    pub pulse_incoming: bool,
    pub dangers: Vec<Danger>,
    /// Engaged enemies with a frontal cleave.
    pub frontal: Vec<ActorId>,
    /// Seconds since the most recent pull (large when out of combat).
    pub since_pull: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Decision {
    /// `Some(x)` sets the current target to `x`; `None` leaves it unchanged.
    pub target: Option<Option<ActorId>>,
    pub move_goal: Option<V3>,
    /// Ability slot and target to attempt this tick.
    pub cast: Option<(usize, ActorId)>,
    pub stop_cast: bool,
    pub auto_attack: bool,
}

fn has_ability_kind(sim: &Sim, e: ActorId, kind: AbilityKind, interruptible: bool) -> bool {
    sim.actor(e).enemy.as_ref().is_some_and(|es| {
        es.abilities
            .iter()
            .any(|a| a.cfg.kind == kind && (!interruptible || a.cfg.interruptible))
    })
}

fn interrupt_ability(sim: &Sim, id: ActorId) -> Option<AbilityId> {
    [
        AbilityId::ShieldBash,
        AbilityId::Kick,
        AbilityId::SilencePulse,
    ]
    .into_iter()
    .find(|a| sim.actor(id).ability_slot(*a).is_some())
}

pub fn compute_blackboard(sim: &Sim) -> Blackboard {
    let mut b = Blackboard {
        engaged: sim.engaged_enemies(),
        ..Default::default()
    };
    for p in sim.alive_party() {
        match sim.actor(p).role() {
            Some(Role::Tank) => b.tanks.push(p),
            Some(Role::Healer) => b.healers.push(p),
            _ => {}
        }
    }
    b.healer_needs_mana = b.healers.iter().any(|&h| sim.actor(h).mana_frac() < 0.5);
    b.since_pull = b
        .engaged
        .iter()
        .filter_map(|&e| {
            sim.actor(e)
                .enemy
                .as_ref()
                .map(|es| sim.time - es.pull_time)
        })
        .fold(f32::INFINITY, f32::min);
    let tank = b.tanks.first().copied();
    let tank_target = tank.and_then(|t| sim.actor(t).target);

    for &e in &b.engaged {
        let a = sim.actor(e);
        if let Some(t) = a.target
            && !sim.is_tank(t)
            && !b.threatened.contains(&t)
        {
            b.threatened.push(t);
        }
        if has_ability_kind(sim, e, AbilityKind::Cleave, false)
            && a.enemy.as_ref().is_some_and(|es| {
                es.abilities
                    .iter()
                    .any(|x| x.cfg.kind == AbilityKind::Cleave && x.cfg.frontal)
            })
        {
            b.frontal.push(e);
        }
        if let Some(c) = &a.cast
            && let CastKind::Enemy(idx) = c.kind
            && a.enemy
                .as_ref()
                .is_some_and(|es| es.abilities[idx].cfg.kind == AbilityKind::Pulse)
        {
            b.pulse_incoming = true;
        }
    }

    // Kill target: healers and dangerous casters first, then adds, then the tank's target.
    let score = |e: ActorId| {
        let a = sim.actor(e);
        let mut s = 0.0;
        if Some(e) == tank_target {
            s += 20.0;
        }
        if has_ability_kind(sim, e, AbilityKind::HealAlly, false) {
            s += 40.0;
        }
        if has_ability_kind(sim, e, AbilityKind::Pulse, true)
            || has_ability_kind(sim, e, AbilityKind::Bolt, true)
                && a.rank() != Some(EnemyRank::Boss)
        {
            s += 25.0;
        }
        if a.enemy.as_ref().is_some_and(|es| es.summoned) {
            s += 12.0;
        }
        if a.rank() == Some(EnemyRank::Boss) {
            s -= 10.0;
        }
        s += 8.0 * (1.0 - a.hp_frac());
        if let Some(t) = tank {
            s -= 0.02 * a.pos.dist(sim.actor(t).pos);
        }
        s
    };
    b.kill_target = b
        .engaged
        .iter()
        .copied()
        .filter(|&e| !sim.actor(e).is_incapacitated())
        .max_by(|&x, &y| score(x).total_cmp(&score(y)).then(y.cmp(&x)));

    // Crowd control: with 3+ non-boss enemies, an AI Arcanist hexes the healthiest
    // extra enemy that nobody is focusing.
    let hexers: Vec<ActorId> = sim
        .alive_party()
        .filter(|&p| {
            let a = sim.actor(p);
            a.controller == crate::sim::Controller::Ai
                && a.ability_slot(AbilityId::Hex).is_some()
                && params_of(a).use_cc
        })
        .collect();
    let already_hexed = b
        .engaged
        .iter()
        .copied()
        .find(|&e| sim.actor(e).is_incapacitated());
    if let Some(&owner) = hexers.first() {
        let candidates: Vec<ActorId> = b
            .engaged
            .iter()
            .copied()
            .filter(|&e| {
                let a = sim.actor(e);
                !a.enemy
                    .as_ref()
                    .is_some_and(|es| es.cc_immune || es.summoned)
                    && Some(e) != b.kill_target
                    && Some(e) != tank_target
                    && a.hp_frac() > 0.6
                    && a.max_hp >= 1000.0
            })
            .collect();
        if b.engaged.len() >= 3 {
            b.cc_owner = Some(owner);
            b.cc_target = already_hexed.or_else(|| {
                candidates
                    .iter()
                    .copied()
                    .max_by(|&x, &y| sim.actor(x).hp.total_cmp(&sim.actor(y).hp).then(y.cmp(&x)))
            });
        }
    }

    // Interrupt assignments: one interrupter per interruptible cast.
    let mut used: Vec<ActorId> = Vec::new();
    for &e in &b.engaged {
        let Some(c) = &sim.actor(e).cast else {
            continue;
        };
        if !c.interruptible {
            continue;
        }
        let mut best: Option<(f32, ActorId)> = None;
        for p in sim.alive_party() {
            let a = sim.actor(p);
            if a.controller != crate::sim::Controller::Ai || used.contains(&p) {
                continue;
            }
            let Some(ab) = interrupt_ability(sim, p) else {
                continue;
            };
            let slot = a.ability_slot(ab).expect("has ability");
            if a.cooldowns[slot] > 0.0 || a.resource < ab.def().cost || a.is_incapacitated() {
                continue;
            }
            let range = ab.def().range;
            let gap =
                (a.pos.dist(sim.actor(e).pos) - range - a.radius - sim.actor(e).radius).max(0.0);
            // Time to get in range at run speed must fit in the remaining cast.
            if gap / a.speed.max(0.1) > c.remaining {
                continue;
            }
            if best.is_none_or(|(g, _)| gap < g) {
                best = Some((gap, p));
            }
        }
        if let Some((_, p)) = best {
            used.push(p);
            b.interrupts.push((e, p));
        }
    }

    for t in &sim.telegraphs {
        b.dangers.push(Danger {
            center: t.center,
            y: t.y,
            radius: t.radius,
            age: t.elapsed(),
        });
    }
    for h in &sim.hazards {
        if sim.encounters[h.encounter].cleared {
            continue;
        }
        if h.active || h.time_to_active(sim.time) <= 1.0 {
            b.dangers.push(Danger {
                center: h.pos.xz(),
                y: h.pos.y,
                radius: h.cfg.radius,
                age: 99.0,
            });
        }
    }
    b
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
    if a.is_silenced() && def.school.is_magic() {
        return None;
    }
    if def.target != crate::class::TargetRule::SelfOnly
        && (!sim.reach(id, target, def.range) || !sim.has_los(id, target))
    {
        return None;
    }
    let t = sim.actor(target);
    let valid = match def.effect {
        Effect::Absorb { .. } => !t.has_aura(|k| matches!(k, AuraKind::WeakenedSoul)),
        Effect::Dispel => t.has_dispellable_debuff(),
        Effect::Incapacitate { .. } => !t.enemy.as_ref().is_some_and(|e| e.cc_immune),
        _ => true,
    };
    valid.then_some((s, target))
}

impl Danger {
    /// Does this danger cover point `p` (with clearance `r`)? Only on its own floor.
    pub fn covers(&self, p: V3, r: f32) -> bool {
        p.xz().dist(self.center) <= self.radius + r && (p.y - self.y).abs() <= EFFECT_HEIGHT
    }
}

fn in_danger(sim: &Sim, id: ActorId, margin: f32, reaction: f32) -> bool {
    let me = sim.actor(id);
    sim.board
        .dangers
        .iter()
        .any(|d| d.age >= reaction && d.covers(me.loc(), me.radius + margin))
}

/// Can `id` hit `target` from here (range and line of sight)?
fn can_hit(sim: &Sim, id: ActorId, target: ActorId, range: f32) -> bool {
    sim.reach(id, target, range) && sim.has_los(id, target)
}

/// Would someone standing at `spot` see `target`?
fn spot_sees(sim: &Sim, spot: V3, target: ActorId) -> bool {
    let t = sim.actor(target);
    sim.nav
        .line_of_sight(V3::at(spot.xz(), spot.y + 1.7), t.chest())
}

/// Would standing at `spot` pull an idle pack (within its aggro radius plus a
/// margin, on its level)? Repositioning mid-fight must not start another fight.
fn spot_pulls(sim: &Sim, spot: V3) -> bool {
    sim.enemies.iter().any(|&e| {
        let a = sim.actor(e);
        a.alive
            && a.mode() == Some(EnemyMode::Idle)
            && a.enemy
                .as_ref()
                .is_some_and(|es| a.pos.dist(spot.xz()) <= es.aggro_radius + 3.0)
            && (a.y - spot.y).abs() <= 2.5
    })
}

/// Snap a horizontal point onto the floor nearest elevation `y`.
fn ground(sim: &Sim, p: V2, y: f32) -> V3 {
    sim.nav.project(p, y).0
}

fn point_safe(sim: &Sim, p: V3, radius: f32, margin: f32) -> bool {
    sim.board
        .dangers
        .iter()
        .all(|d| !d.covers(p, radius + margin))
}

/// If standing in a noticed danger zone, the nearest safe point (on our floor).
pub fn escape_point(sim: &Sim, id: ActorId) -> Option<V3> {
    let me = sim.actor(id);
    let p = params_of(me);
    let margin = p.danger_margin;
    let danger = sim
        .board
        .dangers
        .iter()
        .find(|d| d.age >= p.reaction_time && d.covers(me.loc(), me.radius + margin))?;
    let safe_dist = danger.radius + me.radius + margin + 1.0;
    let mut dir = (me.pos - danger.center).normalize_or_zero();
    if dir.len_sq() == 0.0 {
        dir = V2::from_heading(id as f32 * 1.7);
    }
    let mut best: Option<V3> = None;
    for k in 0..16 {
        let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
        let angle = sign * ((k + 1) / 2) as f32 * std::f32::consts::PI / 8.0;
        for dist in [safe_dist, safe_dist + 3.0] {
            let (cand, s) = sim
                .nav
                .project(danger.center + dir.rotate(angle) * dist, me.y);
            // Only spots we can walk to from here.
            if sim.nav.reachable(me.surface, s)
                && point_safe(sim, cand, me.radius, margin * 0.5)
                && best.is_none_or(|b| cand.dist(me.loc()) < b.dist(me.loc()))
            {
                best = Some(cand);
            }
        }
    }
    best
}

/// Reach of an enemy's frontal cleave (0 if it has none).
fn frontal_reach(sim: &Sim, e: ActorId) -> f32 {
    sim.actor(e)
        .enemy
        .as_ref()
        .map(|es| {
            es.abilities
                .iter()
                .filter(|a| a.cfg.kind == AbilityKind::Cleave && a.cfg.frontal)
                .map(|a| a.cfg.radius)
                .fold(0.0, f32::max)
        })
        .unwrap_or(0.0)
}

/// Whether point `p` is inside the frontal cleave cone of any engaged enemy.
fn in_frontal_cone(sim: &Sim, id: ActorId, p: V3, margin: f32) -> bool {
    let me = sim.actor(id);
    sim.board.frontal.iter().any(|&e| {
        let a = sim.actor(e);
        let reach = frontal_reach(sim, e) + a.radius + me.radius + margin;
        p.xz().dist(a.pos) <= reach
            && (p.y - a.y).abs() <= EFFECT_HEIGHT
            && crate::stats::is_in_front(a.pos, a.facing, p.xz())
    })
}

/// Keep a desired position out of danger zones by nudging it outward.
fn safe_spot(sim: &Sim, id: ActorId, want: V3) -> V3 {
    let me = sim.actor(id);
    let margin = params_of(me).danger_margin;
    let want = if !sim.is_tank(id) && in_frontal_cone(sim, id, want, margin) {
        // Swing the spot around to the back of the cleaving enemy.
        let e = sim
            .board
            .frontal
            .iter()
            .copied()
            .min_by(|&a, &b| {
                sim.actor(a)
                    .loc()
                    .dist(want)
                    .total_cmp(&sim.actor(b).loc().dist(want))
            })
            .expect("in a cone implies a frontal enemy");
        let a = sim.actor(e);
        let back = V2::from_heading(a.facing + std::f32::consts::PI);
        let dist = want.xz().dist(a.pos).max(a.radius + me.radius + 1.0);
        ground(sim, a.pos + back * dist, a.y)
    } else {
        want
    };
    if point_safe(sim, want, me.radius, margin) {
        return want;
    }
    for d in &sim.board.dangers {
        if d.covers(want, me.radius + margin) {
            let dir = (want.xz() - d.center).normalize_or_zero();
            let dir = if dir.len_sq() == 0.0 {
                V2::new(1.0, 0.0)
            } else {
                dir
            };
            return ground(
                sim,
                d.center + dir * (d.radius + me.radius + margin + 1.0),
                want.y,
            );
        }
    }
    want
}

pub fn party_decision(sim: &Sim, id: ActorId) -> Decision {
    let mut d = party_decision_inner(sim, id);
    // Escaping a ground effect always wins; otherwise walk around cleave cones.
    if !sim.is_tank(id)
        && escape_point(sim, id).is_none()
        && let Some(goal) = d.move_goal
    {
        d.move_goal = Some(cone_detour(sim, id, goal).0);
    }
    d
}

/// If the straight path to `goal` crosses a frontal cleave cone, steer to a point
/// beside the cleaving enemy first (on our side), so we walk around the front.
#[cfg(test)]
pub(crate) fn route_around_cones(sim: &Sim, id: ActorId, goal: V3) -> V3 {
    cone_detour(sim, id, goal).0
}

/// `route_around_cones`, plus the cleaving enemy in the way when no detour
/// gets closer to `goal` than standing still.
fn cone_detour(sim: &Sim, id: ActorId, goal: V3) -> (V3, Option<ActorId>) {
    let me = sim.actor(id);
    let margin = params_of(me).danger_margin * 0.5;
    for k in 1..=8 {
        let f = k as f32 / 8.0;
        let p2 = me.pos + (goal.xz() - me.pos) * f;
        let p = V3::at(p2, me.y + (goal.y - me.y) * f);
        if !in_frontal_cone(sim, id, p, margin) {
            continue;
        }
        // Find the cone we would cross and go around its side.
        let Some(&e) = sim.board.frontal.iter().find(|&&e| {
            let a = sim.actor(e);
            let reach = frontal_reach(sim, e) + a.radius + me.radius + margin;
            p2.dist(a.pos) <= reach && crate::stats::is_in_front(a.pos, a.facing, p2)
        }) else {
            return (goal, None);
        };
        let a = sim.actor(e);
        let fwd = V2::from_heading(a.facing);
        let right = V2::new(fwd.z, -fwd.x);
        let reach = frontal_reach(sim, e) + a.radius + me.radius + margin + 1.0;
        // Try both flanks (slightly behind them); keep those outside every cone.
        let best = [right, -right]
            .into_iter()
            .map(|side| safe_spot(sim, id, ground(sim, a.pos + side * reach - fwd * 1.5, a.y)))
            .filter(|c| !in_frontal_cone(sim, id, *c, margin))
            .min_by(|x, y| x.dist(me.loc()).total_cmp(&y.dist(me.loc())));
        // No detour, or only one we are already standing at or that gets us no
        // closer: the cone blocks the way.
        let stuck = best.is_none_or(|c| {
            c.dist(me.loc()) < 0.5 || c.xz().dist(goal.xz()) + 0.5 >= me.pos.dist(goal.xz())
        });
        let blocker = stuck.then_some(e);
        return match best {
            Some(detour) if detour.dist(me.loc()) >= 0.5 => (detour, blocker),
            _ => (goal, blocker),
        };
    }
    (goal, None)
}

/// Is engaged enemy `e` casting a line-of-sight pulse?
fn casting_los_pulse(sim: &Sim, e: ActorId) -> bool {
    let a = sim.actor(e);
    let Some(cast) = &a.cast else { return false };
    let crate::sim::CastKind::Enemy(i) = cast.kind else {
        return false;
    };
    a.enemy
        .as_ref()
        .and_then(|es| es.abilities.get(i))
        .is_some_and(|ab| ab.cfg.los_blockable)
}

/// An engaged enemy casting a line-of-sight pulse that can see `id`, if any.
fn los_pulse_caster(sim: &Sim, id: ActorId) -> Option<ActorId> {
    sim.board
        .engaged
        .iter()
        .copied()
        .find(|&e| casting_los_pulse(sim, e) && sim.has_los(e, id))
}

/// Nearest reachable spot (within ~20 m) where `caster` can't see us: behind
/// a pillar or around a corner. Deterministic ring search, nearest first.
/// Would standing at `p` hide `id` from `caster`? Deep in the shadow: still
/// hidden 0.7 m to either side, so standing there doesn't flicker in and out
/// of sight.
pub(crate) fn is_cover(sim: &Sim, id: ActorId, caster: ActorId, p: V3) -> bool {
    let me = sim.actor(id);
    let c = sim.actor(caster);
    let (eye_h, chest_h) = (me.eye().y - me.y, me.chest().y - me.y);
    let hidden_at = |p: V3| {
        !sim.nav.line_of_sight(c.eye(), V3::at(p.xz(), p.y + eye_h))
            && !sim
                .nav
                .line_of_sight(c.eye(), V3::at(p.xz(), p.y + chest_h))
    };
    let side = (p.xz() - c.pos).normalize_or_zero();
    let side = V2::new(-side.z, side.x) * 0.7;
    hidden_at(p) && hidden_at(V3::at(p.xz() + side, p.y)) && hidden_at(V3::at(p.xz() - side, p.y))
}

fn cover_point(sim: &Sim, id: ActorId, caster: ActorId) -> Option<V3> {
    let me = sim.actor(id);
    // Keep heading for the cover already chosen while it still hides us:
    // two shadows at similar distances must not take turns as we move.
    if let Some(g) = me.move_goal
        && g.xz().dist(me.pos) <= 20.0
        && is_cover(sim, id, caster, g)
    {
        return Some(g);
    }
    let hidden = |p: V3| is_cover(sim, id, caster, p);
    // Cover reached by walking through a cleave cone gets rewritten into a
    // detour (`party_decision`), which is not cover: the next tick would pick
    // another shadow and the two would take turns. Prefer cover we can walk
    // to directly; fall back to the nearest other cover.
    let mut fallback: Option<V3> = None;
    for k in 1..=10 {
        let r = 2.0 * k as f32;
        let mut best: Option<(f32, V3)> = None;
        for j in 0..16 {
            let a = j as f32 * std::f32::consts::TAU / 16.0;
            let want = me.pos + V2::new(a.cos(), a.sin()) * r;
            let p = ground(sim, want, me.y);
            if p.xz().dist(want) > 1.0 || (p.y - me.y).abs() > 1.5 || !hidden(p) {
                continue;
            }
            if !point_safe(sim, p, me.radius, 0.5) {
                continue;
            }
            let d = me.pos.dist(p.xz());
            if best.is_none_or(|(bd, _)| d < bd - 1e-4)
                && sim.nav.path_points(me.pos, me.surface, p).is_some()
            {
                if sim.is_tank(id) || cone_detour(sim, id, p).0 == p {
                    best = Some((d, p));
                } else if fallback.is_none() {
                    fallback = Some(p);
                }
            }
        }
        if let Some((_, p)) = best {
            return Some(p);
        }
    }
    fallback
}

fn party_decision_inner(sim: &Sim, id: ActorId) -> Decision {
    let me = sim.actor(id);
    // A line-of-sight pulse is being cast and it can see us: break line of
    // sight first (casting can wait; instants are still allowed).
    if let Some(caster) = los_pulse_caster(sim, id)
        && let Some(cover) = cover_point(sim, id, caster)
    {
        return Decision {
            move_goal: Some(cover),
            stop_cast: me.cast.is_some(),
            auto_attack: true,
            ..Default::default()
        };
    }
    // Already out of sight while it is still casting: stay put (the normal
    // decision would walk straight back into view).
    if sim
        .board
        .engaged
        .iter()
        .any(|&e| casting_los_pulse(sim, e) && !sim.has_los(e, id))
    {
        let mut d = party_decision_inner_rest(sim, id);
        d.move_goal = None;
        return d;
    }
    party_decision_inner_rest(sim, id)
}

fn party_decision_inner_rest(sim: &Sim, id: ActorId) -> Decision {
    let me = sim.actor(id);
    if let Some(escape) = escape_point(sim, id) {
        // Instant abilities are still allowed while running out.
        return Decision {
            move_goal: Some(escape),
            stop_cast: me.cast.is_some(),
            auto_attack: true,
            ..Default::default()
        };
    }
    if sim.board.engaged.is_empty() {
        return out_of_combat(sim, id);
    }
    let params = params_of(me);
    if !sim.is_tank(id) && in_frontal_cone(sim, id, me.loc(), params.danger_margin * 0.5) {
        let goal = safe_spot(sim, id, me.loc());
        if goal.dist(me.loc()) > 0.2 {
            return Decision {
                move_goal: Some(goal),
                auto_attack: me.auto_attack,
                target: None,
                ..Default::default()
            };
        }
    }
    // Assigned interrupt takes priority over everything but dodging.
    if let Some(&(enemy, _)) = sim.board.interrupts.iter().find(|(_, p)| *p == id)
        && let Some(ab) = interrupt_ability(sim, id)
        && let Some(cast) = &sim.actor(enemy).cast
    {
        let window = params_of(me).interrupt_window;
        if let Some(c) = usable(sim, id, ab, enemy) {
            if cast.remaining <= window {
                return Decision {
                    cast: Some(c),
                    target: Some(Some(enemy)),
                    auto_attack: true,
                    ..Default::default()
                };
            }
        } else if me.cast.is_none() && !can_hit(sim, id, enemy, ab.def().range) {
            let mut d = Decision {
                target: Some(Some(enemy)),
                move_goal: Some(safe_spot(sim, id, sim.actor(enemy).loc())),
                auto_attack: true,
                ..Default::default()
            };
            if me.role() == Some(Role::Tank) {
                // Tanks keep their own target while closing in.
                d.target = None;
            }
            return d;
        }
    }
    match me.role() {
        Some(Role::Tank) => tank(sim, id),
        Some(Role::Healer) => healer(sim, id),
        _ => dps(sim, id),
    }
}

fn follow_point(sim: &Sim, id: ActorId) -> Option<V3> {
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
    let ring = (index / FORMATION.len()) as f32 * 3.0;
    let fwd = sim.leader_heading;
    let right = V2::new(fwd.z, -fwd.x);
    Some(ground(
        sim,
        leader.pos + right * ox + fwd * (oz - ring),
        leader.y,
    ))
}

fn out_of_combat(sim: &Sim, id: ActorId) -> Decision {
    let me = sim.actor(id);
    let mut d = Decision {
        target: Some(None),
        ..Default::default()
    };
    if id == sim.leader {
        if sim.options.autopilot {
            return autopilot(sim, id);
        }
        return d;
    }
    // Healers top the party off between pulls and clear lingering debuffs.
    if me.role() == Some(Role::Healer) && me.cast.is_none() {
        if let Some(p) = sim
            .alive_party()
            .find(|&p| sim.actor(p).has_dispellable_debuff())
            && let Some(c) = usable(sim, id, AbilityId::Purify, p)
        {
            d.cast = Some(c);
            return d;
        }
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
        && goal.dist(me.loc()) > 1.5
    {
        d.move_goal = Some(safe_spot(sim, id, goal));
    }
    d
}

/// Leader autopilot (headless / demo): wait until the party is ready and gathered,
/// then walk to the next uncleared encounter and pull it.
fn autopilot(sim: &Sim, id: ActorId) -> Decision {
    let me = sim.actor(id);
    let p = params_of(me);
    let mut d = Decision {
        target: Some(None),
        ..Default::default()
    };
    let Some(next) = sim
        .encounters
        .iter()
        .position(|e| !e.cleared && !(e.optional && sim.options.skip_optional))
    else {
        return d;
    };
    let ready = sim.alive_party().all(|q| {
        let a = sim.actor(q);
        a.hp_frac() >= p.ready_hp && a.mana_frac() >= p.ready_mana
    }) && sim.alive_party().count() == sim.party.len();
    let gathered = sim
        .alive_party()
        .all(|q| sim.actor(q).loc().dist(me.loc()) <= 12.0 + 0.6 * sim.party.len() as f32);
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
            .loc()
            .dist(me.loc())
            .total_cmp(&sim.actor(b).loc().dist(me.loc()))
    }) else {
        return d;
    };
    if let Some(c) = usable(sim, id, AbilityId::Challenge, nearest)
        && sim.actor(nearest).loc().dist(me.loc()) <= 22.0
    {
        d.cast = Some(c);
        d.target = Some(Some(nearest));
        return d;
    }
    d.move_goal = Some(safe_spot(sim, id, sim.actor(nearest).loc()));
    d
}

fn rank_weight(sim: &Sim, e: ActorId) -> f32 {
    match sim.actor(e).rank() {
        Some(EnemyRank::Boss) => 3.0,
        Some(EnemyRank::Elite) => 2.0,
        _ => 1.0,
    }
}

/// Where a melee attacker should stand: behind `target` relative to the tank.
fn behind_spot(sim: &Sim, id: ActorId, target: ActorId) -> V3 {
    let t = sim.actor(target);
    let me = sim.actor(id);
    let behind = V2::from_heading(t.facing + std::f32::consts::PI);
    let side = if id.is_multiple_of(2) { 0.35 } else { -0.35 };
    ground(
        sim,
        t.pos + behind.rotate(side) * (t.radius + me.radius + 1.0),
        t.y,
    )
}

/// Tank: pick up loose enemies (taunt if possible), hold the enemy whose threat lead
/// is smallest, AoE threat on packs, defensives on low HP or incoming pulses.
fn tank(sim: &Sim, id: ActorId) -> Decision {
    let me = sim.actor(id);
    let p = params_of(me);
    let engaged: Vec<ActorId> = sim
        .board
        .engaged
        .iter()
        .copied()
        .filter(|&e| !sim.actor(e).is_incapacitated())
        .collect();
    let mut d = Decision {
        auto_attack: true,
        ..Default::default()
    };
    if engaged.is_empty() {
        return d;
    }
    let mut loose: Vec<ActorId> = engaged
        .iter()
        .copied()
        .filter(|&e| {
            sim.actor(e)
                .target
                .is_some_and(|t| t != id && !sim.is_tank(t))
        })
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
    // Tank swap: stacks of an enemy's damage-taken debuff on its tank.
    let stacks_from = |who: ActorId, e: ActorId| -> u32 {
        sim.actor(who)
            .auras
            .iter()
            .filter(|a| a.source == e && matches!(a.kind, AuraKind::DamageTaken { .. }))
            .map(|a| a.stacks)
            .sum()
    };
    let other_tanks: Vec<ActorId> = sim
        .board
        .tanks
        .iter()
        .copied()
        .filter(|&t| t != id)
        .collect();
    for &e in &engaged {
        let Some(holder) = sim.actor(e).target else {
            continue;
        };
        if holder != id
            && other_tanks.contains(&holder)
            && stacks_from(holder, e) >= TANK_SWAP_STACKS
            && stacks_from(id, e) < 2
        {
            let mut d = Decision {
                target: Some(Some(e)),
                auto_attack: true,
                ..Default::default()
            };
            match usable(sim, id, AbilityId::Challenge, e) {
                Some(c) => d.cast = Some(c),
                None if !can_hit(sim, id, e, 20.0) => {
                    d.move_goal = Some(safe_spot(sim, id, sim.actor(e).loc()))
                }
                None => {}
            }
            return d;
        }
    }
    // Enemies I should let go of: I carry swap-level stacks from them and a
    // fresh tank is around to take over.
    let backing_off: Vec<ActorId> = engaged
        .iter()
        .copied()
        .filter(|&e| {
            stacks_from(id, e) >= TANK_SWAP_STACKS
                && other_tanks.iter().any(|&t| stacks_from(t, e) < 2)
        })
        .collect();
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
    let target = if let Some(&e) = loose.first() {
        if let Some(c) = usable(sim, id, AbilityId::Challenge, e) {
            d.cast = Some(c);
        }
        e
    } else {
        // Leave enemies held by another tank (and those we are backing off from)
        // to them, unless there is nothing else to do.
        let mine: Vec<ActorId> = engaged
            .iter()
            .copied()
            .filter(|&e| {
                !backing_off.contains(&e)
                    && sim
                        .actor(e)
                        .target
                        .is_none_or(|t| t == id || !other_tanks.contains(&t))
            })
            .collect();
        let pool = if mine.is_empty() { &engaged } else { &mine };
        let current = me.target.filter(|t| pool.contains(t));
        let best = pool
            .iter()
            .copied()
            .min_by(|&a, &b| lead(a).total_cmp(&lead(b)).then(a.cmp(&b)))
            .expect("engaged is non-empty");
        match current {
            Some(c) if lead(c) <= lead(best) + 150.0 => c,
            _ => best,
        }
    };
    d.target = Some(Some(target));
    if backing_off.contains(&target) {
        // Nothing else to tank: stand by without adding threat.
        d.auto_attack = false;
        return d;
    }
    let stacks: u32 = me
        .auras
        .iter()
        .filter(|a| matches!(a.kind, AuraKind::DamageTaken { .. }))
        .map(|a| a.stacks)
        .sum();
    if d.cast.is_none() {
        let near = engaged
            .iter()
            .filter(|&&e| sim.near(id, e, 8.0 + sim.actor(e).radius))
            .count();
        let wants_defensive =
            me.hp_frac() < p.defensive_hp || sim.board.pulse_incoming || stacks >= 4;
        let loose_near = loose
            .iter()
            .filter(|&&e| sim.near(id, e, 8.0 + sim.actor(e).radius))
            .count();
        let cc_near_now = sim
            .board
            .engaged
            .iter()
            .any(|&e| sim.actor(e).is_incapacitated() && sim.near(id, e, 9.0));
        d.cast = if wants_defensive {
            usable(sim, id, AbilityId::Bulwark, id)
        } else {
            None
        }
        .or_else(|| {
            (loose_near >= 1 && near >= 2 && !cc_near_now)
                .then(|| usable(sim, id, AbilityId::QuakeStomp, id))
                .flatten()
        })
        .or_else(|| {
            // Quake on packs, but never next to a crowd-controlled enemy.
            let cc_near = sim
                .board
                .engaged
                .iter()
                .any(|&e| sim.actor(e).is_incapacitated() && sim.near(id, e, 9.0));
            (near >= 2 && !cc_near)
                .then(|| usable(sim, id, AbilityId::QuakeStomp, id))
                .flatten()
        })
        .or_else(|| usable(sim, id, AbilityId::ShieldSlam, target));
    }
    // Position: in melee range; for frontal cleavers, on the far side from the party
    // so the cleave points away from everyone else.
    let t = sim.actor(target);
    if sim.board.frontal.contains(&target) {
        let others: Vec<V2> = sim
            .alive_party()
            .filter(|&q| q != id)
            .map(|q| sim.actor(q).pos)
            .collect();
        if !others.is_empty() {
            let centroid =
                others.iter().fold(V2::ZERO, |acc, &v| acc + v) * (1.0 / others.len() as f32);
            let away = (t.pos - centroid).normalize_or_zero();
            if away.len_sq() > 0.0 {
                let spot = ground(sim, t.pos + away * (t.radius + me.radius + 1.2), t.y);
                if spot.dist(me.loc()) > 1.5 {
                    d.move_goal = Some(safe_spot(sim, id, spot));
                }
                return d;
            }
        }
    }
    if !can_hit(sim, id, target, 1.6) {
        d.move_goal = Some(safe_spot(sim, id, t.loc()));
    }
    d
}

fn healer(sim: &Sim, id: ActorId) -> Decision {
    let me = sim.actor(id);
    let p = params_of(me);
    let mut d = Decision::default();
    // Cancel a big heal that would be pure overheal.
    if let Some(c) = &me.cast {
        let t = sim.actor(c.target);
        if &*c.name == "Mend" && t.hp_frac() > 0.97 && c.remaining > 0.3 {
            d.stop_cast = true;
        } else {
            return d;
        }
    }
    let tank = sim.board.tanks.first().copied();
    let mana = me.mana_frac();
    let urgency = |q: ActorId| {
        let a = sim.actor(q);
        let mut f = a.hp_frac();
        if Some(q) == tank {
            f -= 0.1;
        }
        if q == id {
            f -= 0.05;
        }
        if sim.board.threatened.contains(&q) {
            f -= 0.05;
        }
        f
    };
    let mut allies: Vec<ActorId> = sim.alive_party().collect();
    allies.sort_by(|&a, &b| urgency(a).total_cmp(&urgency(b)).then(a.cmp(&b)));
    let emergency = allies
        .first()
        .is_some_and(|&a| sim.actor(a).hp_frac() < 0.35);

    let dispel = || {
        allies
            .iter()
            .copied()
            .filter(|&q| sim.actor(q).has_dispellable_debuff())
            .find_map(|q| usable(sim, id, AbilityId::Purify, q))
    };
    if p.dispel_first && !emergency {
        d.cast = dispel();
    }
    // Shield ahead of a party-wide pulse, or the tank when it is taking a beating.
    if d.cast.is_none() && mana > 0.2 {
        let ward_target = if sim.board.pulse_incoming {
            allies.first().copied()
        } else {
            tank.filter(|&t| sim.actor(t).hp_frac() < 0.75)
        };
        if let Some(t) = ward_target {
            d.cast = usable(sim, id, AbilityId::Ward, t);
        }
    }
    let mend_threshold = if mana < 0.2 { 0.35 } else { p.mend_threshold };
    // Let the tank build threat before generating healing threat with top-ups.
    let fresh_pull = sim.board.since_pull < PULL_GRACE;
    let mut out_of_range_patient = None;
    if d.cast.is_none() {
        for &q in &allies {
            let a = sim.actor(q);
            if a.hp_frac() < mend_threshold && mana > 0.07 {
                if let Some(c) = usable(sim, id, AbilityId::Mend, q) {
                    d.cast = Some(c);
                    break;
                } else if !can_hit(sim, id, q, 30.0) {
                    out_of_range_patient.get_or_insert(q);
                }
            }
        }
    }
    if d.cast.is_none() && !p.dispel_first && !emergency {
        d.cast = dispel();
    }
    if d.cast.is_none() && mana > 0.25 {
        for &q in &allies {
            let a = sim.actor(q);
            let wants_hot = if fresh_pull {
                a.hp_frac() < 0.6
            } else {
                a.hp_frac() < p.hot_threshold
                    || (Some(q) == tank && a.hp_frac() < 0.97 && mana > 0.5)
            };
            if wants_hot
                && !a.has_hot_from(id)
                && let Some(c) = usable(sim, id, AbilityId::Renew, q)
            {
                d.cast = Some(c);
                break;
            }
        }
    }
    // Losing sight of the tank (a corner, a ledge) comes before dealing damage.
    let tank_lost = tank.is_some_and(|t| t != id && !can_hit(sim, id, t, 30.0));
    if d.cast.is_none()
        && !fresh_pull
        && !tank_lost
        && mana > p.smite_mana
        && let Some(t) = sim.board.kill_target
        && tank.is_none_or(|k| sim.threat(t, k) > 300.0)
        && let Some(c) = usable(sim, id, AbilityId::Smite, t)
    {
        d.cast = Some(c);
    }
    let casting_now = d
        .cast
        .is_some_and(|(s, _)| me.abilities[s].def().cast_time > 0.0);
    if !casting_now {
        if let Some(q) = out_of_range_patient {
            d.move_goal = Some(safe_spot(sim, id, sim.actor(q).loc()));
        } else if let Some(goal) = ranged_spot(sim, id, 15.0) {
            // The ranged spot is chosen to see both the enemy and the tank, so it
            // is also where to go after losing sight of the tank (one goal, no
            // flip-flopping at the edge of a corner).
            let far_from_party = sim.alive_party().any(|q| !can_hit(sim, id, q, 26.0));
            if goal.dist(me.loc()) > 6.0
                || far_from_party
                || tank_lost
                || in_danger(sim, id, 0.0, 0.0)
            {
                d.move_goal = Some(safe_spot(sim, id, goal));
            }
        } else if let Some(t) = tank.filter(|_| tank_lost) {
            d.move_goal = Some(safe_spot(sim, id, sim.actor(t).loc()));
        }
    }
    d
}

/// A spot `dist` away from the tank's enemy on the tank's side, offset sideways per actor.
fn ranged_spot(sim: &Sim, id: ActorId, dist: f32) -> Option<V3> {
    let tank = sim.board.tanks.first().copied();
    let enemy = tank
        .and_then(|t| sim.actor(t).target)
        .filter(|e| sim.board.engaged.contains(e))
        .or_else(|| sim.board.engaged.first().copied())?;
    let e = sim.actor(enemy);
    let epos = e.pos;
    // Spread ranged members around an arc so one ring can't catch everyone.
    let side = ((id % 5) as f32 - 2.0) * 0.3;
    let frontal = sim.board.frontal.contains(&enemy);
    // Frontal cleavers: stand behind-and-to-the-side, never in front.
    let flank_dir = V2::from_heading(e.facing + std::f32::consts::PI)
        .rotate(if id.is_multiple_of(2) { 0.9 } else { -0.9 } + side * 0.3);
    let anchor = tank.map(|t| sim.actor(t).pos).unwrap_or(sim.actor(id).pos);
    let mut dir = (anchor - epos).normalize_or_zero();
    if dir.len_sq() == 0.0 {
        dir = (sim.actor(id).pos - epos).normalize_or_zero();
    }
    // A spot behind a corner is useless: come closer until the enemy is in
    // sight, and stay where the healer can see you (healers: where they can
    // see the tank).
    let watch = if sim.actor(id).role() == Some(Role::Healer) {
        tank
    } else {
        sim.board.healers.first().copied()
    };
    // Search outward from the preferred direction: the first spot that is
    // really on the floor (not clamped into a side corridor), sees the enemy and
    // is seen by the watcher.
    let base = if frontal { flank_dir } else { dir.rotate(side) };
    let me = sim.actor(id);
    // A spot across a chasm or lava is no good if the walk there is long.
    let walkable = |spot: V3| {
        sim.nav
            .path_points(me.pos, me.surface, spot)
            .is_some_and(|pts| {
                let mut len = 0.0;
                let mut prev = me.loc();
                for q in pts {
                    len += prev.xz().dist(q.xz());
                    prev = q;
                }
                len <= me.pos.dist(spot.xz()) * 1.5 + 6.0
            })
    };
    for k in [1.0, 0.7, 0.45] {
        for step in 0..9 {
            let sign = if step % 2 == 0 { 1.0 } else { -1.0 };
            let angle = sign * ((step + 1) / 2) as f32 * std::f32::consts::FRAC_PI_6;
            let want = epos + base.rotate(angle) * (dist * k);
            let spot = ground(sim, want, e.y);
            if spot.xz().dist(want) < 2.0
                && !(frontal && crate::stats::is_in_front(epos, e.facing, spot.xz()))
                && !spot_pulls(sim, spot)
                && spot_sees(sim, spot, enemy)
                && watch.is_none_or(|t| spot_sees(sim, spot, t))
                && walkable(spot)
            {
                return Some(spot);
            }
        }
    }
    if frontal {
        return Some(ground(sim, epos + flank_dir * dist, e.y));
    }
    Some(ground(sim, epos + dir * (e.radius + 3.0), e.y))
}

/// Seconds a parked damage dealer may go without attacking anything before it
/// switches to an enemy it can hit from where it stands.
const RETARGET_IDLE: f32 = 3.0;

fn dps(sim: &Sim, id: ActorId) -> Decision {
    let mut d = dps_inner(sim, id);
    let me = sim.actor(id);
    if me.cast.is_some() {
        return d;
    }
    let Some(Some(t)) = d.target else {
        return d;
    };
    let ranged = me.role() == Some(Role::RangedDps);
    let can = |e: ActorId| {
        if ranged {
            sim.has_los(id, e) && sim.reach(id, e, 26.0)
        } else {
            can_hit(sim, id, e, 1.6)
        }
    };
    // Only where we chose to stand and arrived: a ranged member whose
    // sightline is blocked, or a melee member kept off its target.
    let parked = d.move_goal.is_some_and(|g| g.xz().dist(me.pos) < 1.0);
    let blocked = if ranged { !sim.has_los(id, t) } else { !can(t) };
    let idle = sim.time - me.last_attack >= RETARGET_IDLE;
    if !parked || !blocked || !idle {
        return d;
    }
    // Parked where the target can't be hit (a pillar, or the cleave cone of
    // whoever guards it, in the way): hit something the tank holds that can
    // be hit from here instead of standing idle.
    if let Some(alt) = sim
        .board
        .engaged
        .iter()
        .copied()
        .filter(|&e| {
            e != t
                && !sim.actor(e).is_incapacitated()
                && sim
                    .actor(e)
                    .target
                    .is_some_and(|x| sim.actor(x).role() == Some(Role::Tank))
                && can(e)
        })
        .min_by(|&a, &b| {
            sim.actor(a)
                .pos
                .dist(me.pos)
                .total_cmp(&sim.actor(b).pos.dist(me.pos))
                .then(a.cmp(&b))
        })
    {
        d.target = Some(Some(alt));
        d.cast = None;
        d.auto_attack = true;
    }
    d
}

fn dps_inner(sim: &Sim, id: ActorId) -> Decision {
    let me = sim.actor(id);
    let p = params_of(me);
    let ranged = me.role() == Some(Role::RangedDps);
    let mut d = Decision::default();
    if me.cast.is_some() {
        return d;
    }
    let tank = sim.board.tanks.first().copied();
    // Crowd-control duty first.
    if sim.board.cc_owner == Some(id)
        && let Some(cc) = sim.board.cc_target
        && !sim.actor(cc).is_incapacitated()
        && let Some(c) = usable(sim, id, AbilityId::Hex, cc)
    {
        d.cast = Some(c);
        d.target = Some(Some(cc));
        return d;
    }
    let target = sim
        .board
        .kill_target
        .filter(|&t| !sim.actor(t).is_incapacitated())
        .or_else(|| {
            sim.board
                .engaged
                .iter()
                .copied()
                .find(|&e| !sim.actor(e).is_incapacitated())
        });
    let Some(mut target) = target else {
        return d;
    };
    if !ranged {
        // Boxed in (a narrow bridge, a pack lined up): when the only way
        // around a cleave cone to the kill target leads backwards, fight the
        // cleaving enemy in the way from behind instead of shuttling.
        // Already hitting something safely: keep at it rather than walk the
        // long way round.
        let spot = safe_spot(sim, id, behind_spot(sim, id, target));
        let (detour, blocker) = cone_detour(sim, id, spot);
        let fighting =
            |e: ActorId| sim.board.engaged.contains(&e) && !sim.actor(e).is_incapacitated();
        if detour.dist(spot) > 0.5 || blocker.is_some() {
            if let Some(c) = me.target.filter(|&c| {
                c != target
                    && fighting(c)
                    && can_hit(sim, id, c, 1.6)
                    && !in_frontal_cone(sim, id, me.loc(), p.danger_margin * 0.5)
            }) {
                target = c;
            } else if let Some(e) = blocker.filter(|&e| fighting(e)) {
                target = e;
            }
        }
    }
    d.target = Some(Some(target));
    let t = sim.actor(target);

    // Threat discipline.
    let mut hold = false;
    if let Some(tank) = tank {
        let tank_threat = sim.threat(target, tank);
        let my_threat = sim.threat(target, id);
        let on_me = t.target == Some(id);
        let danger = my_threat > p.threat_cap * tank_threat;
        if (on_me || danger) && my_threat > 100.0 {
            let drop = if ranged {
                AbilityId::Veil
            } else {
                AbilityId::Evade
            };
            d.cast = usable(sim, id, drop, id);
        }
        hold = !on_me && (tank_threat < p.min_tank_threat.max(1.0) || danger);
    }
    d.auto_attack = !hold;

    let cc_nearby = |r: f32| {
        sim.board
            .engaged
            .iter()
            .any(|&e| sim.actor(e).is_incapacitated() && sim.near(id, e, r + 1.0))
    };
    if ranged {
        let unseen = sim
            .board
            .healers
            .first()
            .is_some_and(|&h| h != id && !sim.has_los(h, id));
        if !sim.has_los(id, target) {
            // No line of sight (a corner or a floor in the way): walk toward the
            // target along the path until it comes into view.
            d.move_goal = Some(safe_spot(sim, id, t.loc()));
        } else if !sim.reach(id, target, 26.0) || in_danger(sim, id, 0.0, 0.0) || unseen {
            // Out of range, in danger, or out of the healer's sight.
            d.move_goal = ranged_spot(sim, id, RANGED_DISTANCE)
                .map(|s| safe_spot(sim, id, s))
                .or(Some(t.loc()));
        }
        if !hold && d.cast.is_none() {
            d.cast = usable(sim, id, AbilityId::Flare, target)
                .or_else(|| usable(sim, id, AbilityId::ArcBolt, target));
        }
        if d.cast
            .is_some_and(|(s, _)| me.abilities[s].def().cast_time > 0.0)
        {
            if unseen && d.move_goal.is_some() {
                // Getting back into the healer's sight beats a cast.
                d.cast = None;
            } else {
                d.move_goal = None;
            }
        }
    } else {
        let spot = safe_spot(sim, id, behind_spot(sim, id, target));
        if !can_hit(sim, id, target, 1.6) || spot.dist(me.loc()) > 2.5 && tank.is_some() {
            d.move_goal = Some(spot);
        }
        if !hold && d.cast.is_none() {
            let near: Vec<ActorId> = sim
                .board
                .engaged
                .iter()
                .copied()
                .filter(|&e| sim.near(id, e, 6.0 + sim.actor(e).radius))
                .collect();
            let aoe_safe = tank.is_some_and(|tank| {
                near.iter()
                    .all(|&e| sim.threat(e, tank) > 2.0 * sim.threat(e, id) + 150.0)
            }) && !cc_nearby(6.0);
            if near.len() >= 2 && aoe_safe {
                d.cast = usable(sim, id, AbilityId::Whirl, id);
            }
            if d.cast.is_none() && !t.has_dot_from(id) && t.hp_frac() > 0.25 && t.max_hp > 1500.0 {
                d.cast = usable(sim, id, AbilityId::Lacerate, target);
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
            // Patrols walk their route; everyone else stands still.
            move_goal: es.patrolling().then(|| es.patrol[es.patrol_next]),
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
                && (!sim.reach(id, t, me.attack_range * 0.8) || !sim.has_los(id, t))
            {
                d.move_goal = Some(sim.actor(t).loc());
            }
            d
        }
    }
}
