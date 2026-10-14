//! Deterministic fixed-timestep dungeon simulation.
//!
//! Everything gameplay-relevant lives here; the Bevy game only feeds player input
//! in and renders `Sim` state out. See `RULES.md` for the combat rules in prose.

use crate::ai;
use crate::class::{AbilityId, ClassId, Effect, GCD, Role, TargetRule};
use crate::content::{CleaveConfig, Content, EnemyRank};
use crate::events::{CastError, Event, EventKind};
use crate::math::V2;
use crate::metrics::Metrics;
use crate::nav::Nav;
use crate::rng::Rng;

/// Fixed simulation timestep in seconds (20 Hz).
pub const DT: f32 = 0.05;
/// Party members are "in combat" for this long after their last combat action.
pub const COMBAT_LINGER: f32 = 5.0;
/// Party members within this distance of a mob when its pack aggroes join its threat table.
pub const PULL_SHARE_RADIUS: f32 = 30.0;
/// Fraction of effective healing converted to threat (split across engaged enemies).
pub const HEAL_THREAT_FACTOR: f32 = 0.5;
/// Threat ratio needed for an enemy to switch to a new target in / out of its melee reach.
pub const SWITCH_MELEE: f32 = 1.1;
pub const SWITCH_RANGED: f32 = 1.3;
pub const CRIT_CHANCE: f32 = 0.05;
pub const CRIT_MULT: f32 = 1.5;
pub const PARTY_RADIUS: f32 = 0.5;

pub type ActorId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Team {
    Party,
    Enemy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum RunOutcome {
    InProgress,
    Victory,
    Wipe,
    Timeout,
    Abandoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Controller {
    Player,
    Ai,
}

#[derive(Debug, Clone)]
pub struct PartyMemberSpec {
    pub name: String,
    pub class: ClassId,
    /// Equipment power: +1% damage and healing per point.
    pub power: u32,
    /// Equipment stamina: +10 max HP per point.
    pub stamina: u32,
    pub player_controlled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CastKind {
    /// Index into the caster's ability list.
    Ability(usize),
    /// The boss's phase-1 Hollow Bolt.
    BossBolt,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cast {
    pub kind: CastKind,
    pub name: &'static str,
    pub target: ActorId,
    pub remaining: f32,
    pub total: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Aura {
    Hot {
        source: ActorId,
        per_tick: f32,
        remaining: f32,
        tick_timer: f32,
    },
    DamageReduction {
        pct: f32,
        remaining: f32,
    },
    Taunted {
        by: ActorId,
        remaining: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyMode {
    Idle,
    Engaged,
    /// Returning home after losing all targets or leashing; immune to damage.
    Resetting,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThreatEntry {
    pub actor: ActorId,
    pub value: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BossState {
    pub phase: u8,
    pub pull_time: f32,
    pub bolt_timer: f32,
    pub ring_timer: f32,
    pub enraged: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnemyState {
    pub template_key: String,
    pub rank: EnemyRank,
    pub encounter: usize,
    pub home: V2,
    pub home_facing: f32,
    pub aggro_radius: f32,
    pub threat: Vec<ThreatEntry>,
    pub mode: EnemyMode,
    pub base_power_mult: f32,
    pub cleave: Option<CleaveConfig>,
    pub cleave_timer: f32,
    pub boss: Option<BossState>,
}

#[derive(Debug, Clone)]
pub struct Actor {
    pub id: ActorId,
    pub name: String,
    pub team: Team,
    pub class: Option<ClassId>,
    pub controller: Controller,
    pub pos: V2,
    pub facing: f32,
    pub radius: f32,
    pub speed: f32,
    pub hp: f32,
    pub max_hp: f32,
    pub mana: f32,
    pub max_mana: f32,
    pub alive: bool,
    /// Outgoing damage/healing multiplier.
    pub power_mult: f32,
    /// Incoming damage multiplier.
    pub damage_taken: f32,
    pub threat_mult: f32,
    pub combat_mana_regen: f32,
    pub attack_damage: f32,
    pub attack_interval: f32,
    pub attack_range: f32,
    pub swing_timer: f32,
    pub auto_attack: bool,
    pub abilities: Vec<AbilityId>,
    pub cooldowns: Vec<f32>,
    pub gcd: f32,
    pub cast: Option<Cast>,
    pub target: Option<ActorId>,
    pub auras: Vec<Aura>,
    pub last_combat: f32,
    /// AI steering goal.
    pub move_goal: Option<V2>,
    /// Player steering direction (world space, unnormalized).
    pub move_dir: V2,
    pub moved_last_tick: bool,
    pub enemy: Option<EnemyState>,
}

impl Actor {
    pub fn role(&self) -> Option<Role> {
        self.class.map(|c| c.role())
    }

    pub fn hp_frac(&self) -> f32 {
        if self.max_hp > 0.0 {
            self.hp / self.max_hp
        } else {
            0.0
        }
    }

    pub fn mana_frac(&self) -> f32 {
        if self.max_mana > 0.0 {
            self.mana / self.max_mana
        } else {
            1.0
        }
    }

    pub fn is_enemy(&self) -> bool {
        self.team == Team::Enemy
    }

    pub fn mode(&self) -> Option<EnemyMode> {
        self.enemy.as_ref().map(|e| e.mode)
    }

    pub fn is_engaged_enemy(&self) -> bool {
        self.alive && self.mode() == Some(EnemyMode::Engaged)
    }

    pub fn has_hot_from(&self, source: ActorId) -> bool {
        self.auras
            .iter()
            .any(|a| matches!(a, Aura::Hot { source: s, .. } if *s == source))
    }

    pub fn damage_reduction(&self) -> f32 {
        self.auras
            .iter()
            .filter_map(|a| match a {
                Aura::DamageReduction { pct, .. } => Some(*pct),
                _ => None,
            })
            .fold(0.0, f32::max)
    }

    pub fn taunted_by(&self) -> Option<ActorId> {
        self.auras.iter().find_map(|a| match a {
            Aura::Taunted { by, .. } => Some(*by),
            _ => None,
        })
    }

    pub fn ability_slot(&self, id: AbilityId) -> Option<usize> {
        self.abilities.iter().position(|a| *a == id)
    }

    pub fn ability_ready(&self, slot: usize) -> bool {
        self.cooldowns.get(slot).is_some_and(|c| *c <= 0.0)
            && self.gcd <= 0.0
            && self.mana >= self.abilities[slot].def().mana_cost
    }
}

#[derive(Debug, Clone)]
pub struct EncounterState {
    pub id: String,
    pub mobs: Vec<ActorId>,
    pub cleared: bool,
    pub cleared_at: Option<f32>,
    pub pulls: u32,
}

#[derive(Debug, Clone)]
pub struct Telegraph {
    pub id: u32,
    pub source: ActorId,
    pub center: V2,
    pub radius: f32,
    pub remaining: f32,
    pub total: f32,
    pub damage: f32,
}

impl Telegraph {
    pub fn elapsed(&self) -> f32 {
        self.total - self.remaining
    }
}

#[derive(Debug, Clone)]
pub struct Sim {
    pub content: Content,
    pub nav: Nav,
    pub seed: u64,
    pub(crate) rng: Rng,
    pub time: f32,
    pub tick: u64,
    pub actors: Vec<Actor>,
    pub party: Vec<ActorId>,
    pub enemies: Vec<ActorId>,
    pub encounters: Vec<EncounterState>,
    pub telegraphs: Vec<Telegraph>,
    next_telegraph_id: u32,
    pub events: Vec<Event>,
    pub metrics: Metrics,
    pub outcome: RunOutcome,
    pub player: Option<ActorId>,
    /// Party member the others follow out of combat.
    pub leader: ActorId,
    pub leader_heading: V2,
    /// When true and the leader is AI-controlled it walks the party through the dungeon.
    pub autopilot: bool,
    pending_player_cast: Option<usize>,
    pub boss: ActorId,
    pub phase2_at: Option<f32>,
    /// Sim time at which the outcome was decided.
    pub ended_at: Option<f32>,
}

impl Sim {
    /// Build a run. `party[0]` is the player slot. The leader is the player if one is
    /// player-controlled, otherwise the tank (or the first member).
    pub fn new(content: &Content, party: &[PartyMemberSpec], seed: u64) -> Sim {
        let content = content.clone();
        let nav = content.nav();
        let mut sim = Sim {
            nav,
            seed,
            rng: Rng::new(seed),
            time: 0.0,
            tick: 0,
            actors: Vec::new(),
            party: Vec::new(),
            enemies: Vec::new(),
            encounters: Vec::new(),
            telegraphs: Vec::new(),
            next_telegraph_id: 1,
            events: Vec::new(),
            metrics: Metrics::default(),
            outcome: RunOutcome::InProgress,
            player: None,
            leader: 0,
            leader_heading: V2::new(0.0, 1.0),
            autopilot: false,
            pending_player_cast: None,
            boss: 0,
            phase2_at: None,
            ended_at: None,
            content,
        };
        let entrance = sim.content.dungeon.entrance;
        for (i, spec) in party.iter().enumerate() {
            let stats = spec.class.stats();
            let offset = V2::new((i as f32 - 2.0) * 1.5, -(i as f32 % 2.0) * 1.5);
            let max_hp = stats.max_hp + spec.stamina as f32 * 10.0;
            let abilities = spec.class.abilities().to_vec();
            let id = sim.actors.len();
            sim.actors.push(Actor {
                id,
                name: spec.name.clone(),
                team: Team::Party,
                class: Some(spec.class),
                controller: if spec.player_controlled {
                    Controller::Player
                } else {
                    Controller::Ai
                },
                pos: sim.nav.project(entrance + offset),
                facing: 0.0,
                radius: PARTY_RADIUS,
                speed: stats.speed,
                hp: max_hp,
                max_hp,
                mana: stats.max_mana,
                max_mana: stats.max_mana,
                alive: true,
                power_mult: 1.0 + spec.power as f32 / 100.0,
                damage_taken: stats.damage_taken,
                threat_mult: stats.threat_mult,
                combat_mana_regen: stats.combat_mana_regen,
                attack_damage: stats.attack_damage,
                attack_interval: stats.attack_interval,
                attack_range: stats.attack_range,
                swing_timer: 0.0,
                auto_attack: false,
                cooldowns: vec![0.0; abilities.len()],
                abilities,
                gcd: 0.0,
                cast: None,
                target: None,
                auras: Vec::new(),
                last_combat: f32::NEG_INFINITY,
                move_goal: None,
                move_dir: V2::ZERO,
                moved_last_tick: false,
                enemy: None,
            });
            sim.party.push(id);
            if spec.player_controlled && sim.player.is_none() {
                sim.player = Some(id);
            }
        }
        sim.leader = sim.player.unwrap_or_else(|| {
            sim.party
                .iter()
                .copied()
                .find(|&p| sim.actors[p].role() == Some(Role::Tank))
                .unwrap_or(0)
        });

        let hp_mult = sim.content.dungeon.hp_multiplier;
        let dmg_mult = sim.content.dungeon.damage_multiplier;
        let encounters = sim.content.encounters.clone();
        for (ei, enc) in encounters.iter().enumerate() {
            let mut mobs = Vec::new();
            for spawn in &enc.mobs {
                let t = sim.content.templates[&spawn.template].clone();
                let id = sim.actors.len();
                let max_hp = (t.hp * hp_mult).round().max(1.0);
                let boss = (t.rank == EnemyRank::Boss).then_some(BossState {
                    phase: 1,
                    pull_time: 0.0,
                    bolt_timer: sim.content.boss.bolt_interval,
                    ring_timer: sim.content.boss.ring_interval,
                    enraged: false,
                });
                if boss.is_some() {
                    sim.boss = id;
                }
                let home_facing = (entrance - spawn.pos).heading();
                sim.actors.push(Actor {
                    id,
                    name: t.name.clone(),
                    team: Team::Enemy,
                    class: None,
                    controller: Controller::Ai,
                    pos: spawn.pos,
                    facing: home_facing,
                    radius: t.radius,
                    speed: t.speed,
                    hp: max_hp,
                    max_hp,
                    mana: 0.0,
                    max_mana: 0.0,
                    alive: true,
                    power_mult: dmg_mult,
                    damage_taken: t.damage_taken,
                    threat_mult: 1.0,
                    combat_mana_regen: 0.0,
                    attack_damage: t.melee_damage,
                    attack_interval: t.attack_interval,
                    attack_range: t.attack_range,
                    swing_timer: 0.0,
                    auto_attack: true,
                    abilities: Vec::new(),
                    cooldowns: Vec::new(),
                    gcd: 0.0,
                    cast: None,
                    target: None,
                    auras: Vec::new(),
                    last_combat: f32::NEG_INFINITY,
                    move_goal: None,
                    move_dir: V2::ZERO,
                    moved_last_tick: false,
                    enemy: Some(EnemyState {
                        template_key: spawn.template.clone(),
                        rank: t.rank,
                        encounter: ei,
                        home: spawn.pos,
                        home_facing,
                        aggro_radius: t.aggro_radius,
                        threat: Vec::new(),
                        mode: EnemyMode::Idle,
                        base_power_mult: dmg_mult,
                        cleave_timer: t.cleave.as_ref().map(|c| c.interval).unwrap_or(0.0),
                        cleave: t.cleave.clone(),
                        boss,
                    }),
                });
                sim.enemies.push(id);
                mobs.push(id);
            }
            sim.encounters.push(EncounterState {
                id: enc.id.clone(),
                mobs,
                cleared: false,
                cleared_at: None,
                pulls: 0,
            });
        }
        sim.metrics = Metrics::new(&sim.actors);
        sim
    }

    // ------------------------------------------------------------ queries --

    pub fn actor(&self, id: ActorId) -> &Actor {
        &self.actors[id]
    }

    pub fn reach(&self, a: ActorId, b: ActorId, range: f32) -> bool {
        let (a, b) = (&self.actors[a], &self.actors[b]);
        a.pos.dist(b.pos) <= range + a.radius + b.radius
    }

    pub fn engaged_enemies(&self) -> Vec<ActorId> {
        self.enemies
            .iter()
            .copied()
            .filter(|&e| self.actors[e].is_engaged_enemy())
            .collect()
    }

    pub fn threat(&self, enemy: ActorId, actor: ActorId) -> f32 {
        self.actors[enemy]
            .enemy
            .as_ref()
            .and_then(|e| e.threat.iter().find(|t| t.actor == actor))
            .map(|t| t.value)
            .unwrap_or(0.0)
    }

    pub fn on_threat_table(&self, enemy: ActorId, actor: ActorId) -> bool {
        self.actors[enemy]
            .enemy
            .as_ref()
            .is_some_and(|e| e.threat.iter().any(|t| t.actor == actor))
    }

    pub fn alive_party(&self) -> impl Iterator<Item = ActorId> + '_ {
        self.party.iter().copied().filter(|&p| self.actors[p].alive)
    }

    pub fn tank(&self) -> Option<ActorId> {
        self.alive_party()
            .find(|&p| self.actors[p].role() == Some(Role::Tank))
    }

    pub fn boss_phase(&self) -> u8 {
        self.actors[self.boss]
            .enemy
            .as_ref()
            .and_then(|e| e.boss.as_ref())
            .map(|b| b.phase)
            .unwrap_or(1)
    }

    pub fn is_hostile(&self, a: ActorId, b: ActorId) -> bool {
        self.actors[a].team != self.actors[b].team
    }

    pub fn in_combat(&self, id: ActorId) -> bool {
        self.time - self.actors[id].last_combat < COMBAT_LINGER
    }

    pub fn emit(&mut self, kind: EventKind) {
        self.events.push(Event {
            time: self.time,
            kind,
        });
    }

    // ------------------------------------------------------- player input --

    /// Set the player's desired movement direction (world x/z), zero to stand still.
    pub fn set_player_move(&mut self, dir: V2) {
        if let Some(p) = self.player {
            self.actors[p].move_dir = dir;
        }
    }

    pub fn set_player_facing(&mut self, heading: f32) {
        if let Some(p) = self.player {
            self.actors[p].facing = heading;
        }
    }

    pub fn set_player_target(&mut self, target: Option<ActorId>) {
        if let Some(p) = self.player {
            let engaged = target.is_some_and(|t| self.actors[t].is_engaged_enemy());
            let a = &mut self.actors[p];
            if a.target != target {
                a.target = target;
                a.auto_attack = engaged;
            }
        }
    }

    /// Queue an ability press (slot 0..3) to be resolved on the next tick.
    pub fn queue_player_cast(&mut self, slot: usize) {
        self.pending_player_cast = Some(slot);
    }

    /// Toggle auto-attack on the player's current target.
    pub fn player_start_attack(&mut self) {
        if let Some(p) = self.player {
            self.actors[p].auto_attack = true;
        }
    }

    /// End the run without a result (e.g. the player left through the portal).
    pub fn abandon(&mut self, outcome: RunOutcome) {
        if self.outcome == RunOutcome::InProgress {
            self.outcome = outcome;
            self.ended_at = Some(self.time);
            self.emit(EventKind::Outcome { outcome });
        }
    }

    // ---------------------------------------------------------- the tick --

    pub fn step(&mut self) {
        self.time += DT;
        self.tick += 1;
        self.refresh_combat_flags();
        self.run_ai();
        self.process_player_cast();
        self.move_actors();
        self.check_aggro();
        self.update_auras();
        self.update_casts_and_cooldowns();
        self.auto_attacks();
        self.enemy_specials();
        self.update_telegraphs();
        self.regen();
        self.check_resets();
        self.check_progress();
    }

    /// Step until the outcome is decided or `max_seconds` of sim time pass.
    pub fn run_to_end(&mut self, max_seconds: f32) -> RunOutcome {
        while self.outcome == RunOutcome::InProgress {
            if self.time >= max_seconds {
                self.abandon(RunOutcome::Timeout);
                break;
            }
            self.step();
        }
        self.outcome
    }

    fn refresh_combat_flags(&mut self) {
        let now = self.time;
        for e in self.enemies.clone() {
            if !self.actors[e].is_engaged_enemy() {
                continue;
            }
            let entries: Vec<ActorId> = self.actors[e]
                .enemy
                .as_ref()
                .map(|s| s.threat.iter().map(|t| t.actor).collect())
                .unwrap_or_default();
            self.actors[e].last_combat = now;
            for a in entries {
                self.actors[a].last_combat = now;
            }
        }
    }

    fn run_ai(&mut self) {
        // Leader heading for formation following.
        let leader = &self.actors[self.leader];
        if leader.moved_last_tick {
            self.leader_heading = V2::from_heading(leader.facing);
        }
        for id in 0..self.actors.len() {
            if !self.actors[id].alive || self.actors[id].controller == Controller::Player {
                continue;
            }
            let decision = if self.actors[id].is_enemy() {
                ai::enemy_decision(self, id)
            } else {
                ai::party_decision(self, id)
            };
            self.apply_decision(id, decision);
        }
        // Player: auto-attack engaged targets automatically.
        if let Some(p) = self.player
            && let Some(t) = self.actors[p].target
            && self.actors[t].is_engaged_enemy()
            && self.on_threat_table(t, p)
        {
            self.actors[p].auto_attack = true;
        }
    }

    fn apply_decision(&mut self, id: ActorId, d: ai::Decision) {
        if d.stop_cast && self.actors[id].cast.is_some() {
            let name = self.actors[id].cast.map(|c| c.name).unwrap_or("");
            self.actors[id].cast = None;
            self.emit(EventKind::Interrupted {
                src: id,
                what: name,
            });
        }
        if let Some(t) = d.target {
            if self.actors[id].is_enemy()
                && self.actors[id].target != t
                && let Some(nt) = t
            {
                self.emit(EventKind::TargetChange {
                    enemy: id,
                    target: nt,
                });
            }
            self.actors[id].target = t;
        }
        self.actors[id].move_goal = d.move_goal;
        self.actors[id].auto_attack = d.auto_attack;
        if let Some((slot, target)) = d.cast {
            let _ = self.try_cast(id, slot, Some(target));
        }
    }

    fn process_player_cast(&mut self) {
        let (Some(p), Some(slot)) = (self.player, self.pending_player_cast.take()) else {
            return;
        };
        let target = self.actors[p].target;
        if let Err(reason) = self.try_cast(p, slot, target) {
            let what = self.actors[p]
                .abilities
                .get(slot)
                .map(|a| a.def().name)
                .unwrap_or("?");
            self.emit(EventKind::CastFailed {
                src: p,
                what,
                reason,
            });
        } else if self.actors[p]
            .abilities
            .get(slot)
            .is_some_and(|a| a.def().target == TargetRule::Enemy)
        {
            self.actors[p].auto_attack = true;
        }
    }

    /// Attempt to use ability `slot`. Validates state, resources, target and range.
    pub fn try_cast(
        &mut self,
        caster: ActorId,
        slot: usize,
        target: Option<ActorId>,
    ) -> Result<(), CastError> {
        let a = &self.actors[caster];
        if !a.alive {
            return Err(CastError::Dead);
        }
        if a.cast.is_some() {
            return Err(CastError::Busy);
        }
        let Some(&ability) = a.abilities.get(slot) else {
            return Err(CastError::NoSuchAbility);
        };
        let def = ability.def();
        if a.cooldowns[slot] > 0.0 {
            return Err(CastError::NotReady);
        }
        if a.gcd > 0.0 {
            return Err(CastError::OnGlobalCooldown);
        }
        if a.mana < def.mana_cost {
            return Err(CastError::NotEnoughMana);
        }
        let target = match def.target {
            TargetRule::SelfOnly => caster,
            TargetRule::Enemy => {
                let t = target.ok_or(CastError::NoTarget)?;
                if !self.actors[t].alive || !self.is_hostile(caster, t) {
                    return Err(CastError::InvalidTarget);
                }
                if self.actors[t].mode() == Some(EnemyMode::Resetting) {
                    return Err(CastError::InvalidTarget);
                }
                t
            }
            TargetRule::Ally => match target {
                Some(t) if self.actors[t].alive && !self.is_hostile(caster, t) => t,
                _ => caster,
            },
        };
        if def.target != TargetRule::SelfOnly && !self.reach(caster, target, def.range) {
            return Err(CastError::OutOfRange);
        }
        if def.cast_time > 0.0 {
            if self.actors[caster].controller == Controller::Player
                && self.actors[caster].move_dir.len_sq() > 0.0
            {
                return Err(CastError::Moving);
            }
            let a = &mut self.actors[caster];
            a.cast = Some(Cast {
                kind: CastKind::Ability(slot),
                name: def.name,
                target,
                remaining: def.cast_time,
                total: def.cast_time,
            });
            a.gcd = GCD;
            a.move_goal = None;
            self.emit(EventKind::CastStart {
                src: caster,
                what: def.name,
                target,
            });
        } else {
            self.actors[caster].gcd = GCD;
            self.execute_ability(caster, slot, target);
        }
        Ok(())
    }

    fn execute_ability(&mut self, caster: ActorId, slot: usize, target: ActorId) {
        let def = self.actors[caster].abilities[slot].def();
        {
            let a = &mut self.actors[caster];
            a.mana = (a.mana - def.mana_cost).max(0.0);
            a.cooldowns[slot] = def.cooldown;
        }
        self.metrics.actor_mut(caster).casts += 1;
        match def.effect {
            Effect::Damage {
                amount,
                threat_mult,
            } => {
                self.deal_damage(caster, target, amount, def.name, threat_mult, true);
            }
            Effect::AoeDamage { amount, radius } => {
                let center = self.actors[caster].pos;
                let hits: Vec<ActorId> = self
                    .engaged_enemies()
                    .into_iter()
                    .filter(|&e| self.actors[e].pos.dist(center) <= radius + self.actors[e].radius)
                    .collect();
                for e in hits {
                    self.deal_damage(caster, e, amount, def.name, 1.0, true);
                }
            }
            Effect::Heal { amount } => {
                self.heal(caster, target, amount, def.name);
            }
            Effect::Hot { total, duration } => {
                let ticks = duration.max(1.0);
                let t = &mut self.actors[target];
                t.auras
                    .retain(|a| !matches!(a, Aura::Hot { source, .. } if *source == caster));
                t.auras.push(Aura::Hot {
                    source: caster,
                    per_tick: total / ticks,
                    remaining: duration,
                    tick_timer: 1.0,
                });
                self.emit(EventKind::Buff {
                    src: caster,
                    dst: target,
                    what: def.name,
                });
            }
            Effect::Taunt { duration } => {
                self.taunt(caster, target, duration);
            }
            Effect::DamageReduction { pct, duration } => {
                let t = &mut self.actors[target];
                t.auras
                    .retain(|a| !matches!(a, Aura::DamageReduction { .. }));
                t.auras.push(Aura::DamageReduction {
                    pct,
                    remaining: duration,
                });
                self.emit(EventKind::Buff {
                    src: caster,
                    dst: target,
                    what: def.name,
                });
            }
            Effect::ThreatDrop { pct } => {
                for e in self.enemies.clone() {
                    if let Some(es) = self.actors[e].enemy.as_mut() {
                        for t in es.threat.iter_mut().filter(|t| t.actor == caster) {
                            t.value *= 1.0 - pct;
                        }
                    }
                }
                self.emit(EventKind::ThreatDrop { src: caster });
            }
        }
    }

    /// Taunt: raise the taunter's threat to the current top value and force the
    /// enemy to attack them for `duration` seconds.
    pub fn taunt(&mut self, caster: ActorId, enemy: ActorId, duration: f32) {
        let enc = match &self.actors[enemy].enemy {
            Some(e) => e.encounter,
            None => return,
        };
        self.engage(enc, caster);
        let top = self.actors[enemy]
            .enemy
            .as_ref()
            .map(|e| e.threat.iter().map(|t| t.value).fold(0.0, f32::max))
            .unwrap_or(0.0);
        let mine = self.threat(enemy, caster);
        if top > mine {
            self.add_threat(enemy, caster, top - mine);
        }
        let a = &mut self.actors[enemy];
        a.auras.retain(|x| !matches!(x, Aura::Taunted { .. }));
        a.auras.push(Aura::Taunted {
            by: caster,
            remaining: duration,
        });
        a.target = Some(caster);
        self.actors[caster].last_combat = self.time;
        self.emit(EventKind::Taunt {
            src: caster,
            dst: enemy,
        });
    }

    pub fn add_threat(&mut self, enemy: ActorId, actor: ActorId, amount: f32) {
        if !self.actors[actor].alive {
            return;
        }
        let Some(es) = self.actors[enemy].enemy.as_mut() else {
            return;
        };
        if es.mode != EnemyMode::Engaged {
            return;
        }
        match es.threat.iter_mut().find(|t| t.actor == actor) {
            Some(t) => t.value = (t.value + amount).max(0.0),
            None => es.threat.push(ThreatEntry {
                actor,
                value: amount.max(0.0),
            }),
        }
    }

    /// Put an encounter (pack) into combat. `trigger` is placed on every mob's
    /// threat table first; other living party members nearby join at zero threat.
    pub fn engage(&mut self, enc: usize, trigger: ActorId) {
        if self.encounters[enc].cleared || !self.actors[trigger].alive {
            return;
        }
        let mobs = self.encounters[enc].mobs.clone();
        let mut newly = false;
        for &m in &mobs {
            if !self.actors[m].alive {
                continue;
            }
            let mode = self.actors[m].mode();
            if mode == Some(EnemyMode::Idle) {
                newly = true;
                let mob_pos = self.actors[m].pos;
                let nearby: Vec<ActorId> = self
                    .alive_party()
                    .filter(|&p| {
                        p != trigger && self.actors[p].pos.dist(mob_pos) <= PULL_SHARE_RADIUS
                    })
                    .collect();
                let now = self.time;
                let es = self.actors[m].enemy.as_mut().expect("enemy");
                es.mode = EnemyMode::Engaged;
                es.threat.clear();
                es.threat.push(ThreatEntry {
                    actor: trigger,
                    value: 0.0,
                });
                for p in nearby {
                    es.threat.push(ThreatEntry {
                        actor: p,
                        value: 0.0,
                    });
                }
                if let Some(c) = &es.cleave {
                    es.cleave_timer = c.interval;
                }
                if let Some(b) = es.boss.as_mut() {
                    b.pull_time = now;
                    b.bolt_timer = self.content.boss.bolt_interval;
                    b.ring_timer = self.content.boss.ring_interval;
                }
                self.actors[m].swing_timer = 0.5;
            } else if mode == Some(EnemyMode::Engaged) && !self.on_threat_table(m, trigger) {
                self.actors[m]
                    .enemy
                    .as_mut()
                    .expect("enemy")
                    .threat
                    .push(ThreatEntry {
                        actor: trigger,
                        value: 0.0,
                    });
            }
        }
        if newly {
            self.encounters[enc].pulls += 1;
            self.emit(EventKind::Aggro {
                encounter: enc,
                by: trigger,
            });
        }
    }

    /// Apply damage with all modifiers. Returns the HP actually removed.
    pub fn deal_damage(
        &mut self,
        src: ActorId,
        dst: ActorId,
        base: f32,
        what: &'static str,
        threat_mult: f32,
        can_crit: bool,
    ) -> f32 {
        if !self.actors[dst].alive || !self.actors[src].alive && what != "Cinder Ring" {
            return 0.0;
        }
        if self.actors[dst].mode() == Some(EnemyMode::Resetting) {
            self.emit(EventKind::Evade { enemy: dst });
            return 0.0;
        }
        let mut amount = base * self.actors[src].power_mult * self.rng.range_f32(0.9, 1.1);
        let crit = can_crit && self.rng.chance(CRIT_CHANCE);
        if crit {
            amount *= CRIT_MULT;
        }
        let d = &self.actors[dst];
        amount *= d.damage_taken * (1.0 - d.damage_reduction());
        let amount = amount.round().max(0.0);
        let applied = amount.min(self.actors[dst].hp);
        self.actors[dst].hp = (self.actors[dst].hp - amount).max(0.0);
        let now = self.time;
        self.actors[dst].last_combat = now;
        self.actors[src].last_combat = now;
        self.metrics.actor_mut(src).damage_done += applied;
        self.metrics.actor_mut(dst).damage_taken += applied;
        self.emit(EventKind::Damage {
            src,
            dst,
            what,
            amount,
            crit,
        });
        if let Some(enc) = self.actors[dst].enemy.as_ref().map(|e| e.encounter)
            && self.actors[src].team == Team::Party
        {
            self.engage(enc, src);
            let threat = amount * threat_mult * self.actors[src].threat_mult;
            self.add_threat(dst, src, threat);
        }
        if self.actors[dst].hp <= 0.0 {
            self.kill(dst);
        }
        applied
    }

    /// Heal `dst`. Effective healing generates threat on every engaged enemy:
    /// `HEAL_THREAT_FACTOR * effective * healer.threat_mult`, split evenly.
    pub fn heal(&mut self, src: ActorId, dst: ActorId, base: f32, what: &'static str) -> f32 {
        if !self.actors[dst].alive || !self.actors[src].alive {
            return 0.0;
        }
        let amount = (base * self.actors[src].power_mult).round();
        let d = &mut self.actors[dst];
        let effective = amount.min(d.max_hp - d.hp).max(0.0);
        let overheal = amount - effective;
        d.hp += effective;
        let m = self.metrics.actor_mut(src);
        m.healing_done += effective;
        m.overhealing += overheal;
        self.emit(EventKind::Heal {
            src,
            dst,
            what,
            amount: effective,
            overheal,
        });
        if effective > 0.0 && self.in_combat(dst) {
            self.actors[src].last_combat = self.time;
            let engaged = self.engaged_enemies();
            if !engaged.is_empty() {
                let share = HEAL_THREAT_FACTOR * effective * self.actors[src].threat_mult
                    / engaged.len() as f32;
                for e in engaged {
                    self.add_threat(e, src, share);
                }
            }
        }
        effective
    }

    pub fn kill(&mut self, id: ActorId) {
        if !self.actors[id].alive {
            return;
        }
        {
            let a = &mut self.actors[id];
            a.alive = false;
            a.hp = 0.0;
            a.cast = None;
            a.auras.clear();
            a.move_goal = None;
            a.target = None;
        }
        self.metrics.actor_mut(id).deaths += 1;
        self.emit(EventKind::Death { actor: id });
        for e in self.enemies.clone() {
            if let Some(es) = self.actors[e].enemy.as_mut() {
                es.threat.retain(|t| t.actor != id);
            }
            if self.actors[e].target == Some(id) {
                self.actors[e].target = None;
            }
            self.actors[e]
                .auras
                .retain(|a| !matches!(a, Aura::Taunted { by, .. } if *by == id));
        }
        for p in self.party.clone() {
            if self.actors[p].target == Some(id) {
                self.actors[p].target = None;
                self.actors[p].auto_attack = false;
            }
        }
        self.telegraphs.retain(|t| t.source != id);
    }

    fn move_actors(&mut self) {
        for id in 0..self.actors.len() {
            self.actors[id].moved_last_tick = false;
            let a = &self.actors[id];
            if !a.alive {
                continue;
            }
            let resetting = a.mode() == Some(EnemyMode::Resetting);
            let speed = a.speed * if resetting { 1.5 } else { 1.0 };
            let step = if a.controller == Controller::Player {
                a.move_dir.normalize_or_zero() * speed * DT
            } else if let Some(goal) = a.move_goal {
                let wp = self.nav.next_waypoint(a.pos, goal);
                let d = wp - a.pos;
                if d.len() < 0.05 {
                    V2::ZERO
                } else {
                    d.normalize_or_zero() * (speed * DT).min(d.len())
                }
            } else {
                V2::ZERO
            };
            if step.len_sq() > 0.0 {
                if let Some(c) = self.actors[id].cast {
                    if self.actors[id].controller == Controller::Player {
                        self.actors[id].cast = None;
                        self.emit(EventKind::Interrupted {
                            src: id,
                            what: c.name,
                        });
                    } else {
                        continue;
                    }
                }
                let a = &mut self.actors[id];
                let new_pos = self.nav.try_move(a.pos, step);
                if new_pos != a.pos {
                    a.moved_last_tick = true;
                    if a.controller != Controller::Player {
                        a.facing = (new_pos - a.pos).heading();
                    }
                    a.pos = new_pos;
                }
            } else if let Some(t) = self.actors[id].target
                && self.actors[id].controller != Controller::Player
            {
                let dir = self.actors[t].pos - self.actors[id].pos;
                if dir.len_sq() > 1e-4 {
                    self.actors[id].facing = dir.heading();
                }
            }
        }
    }

    fn check_aggro(&mut self) {
        for e in self.enemies.clone() {
            let a = &self.actors[e];
            if !a.alive || a.mode() != Some(EnemyMode::Idle) {
                continue;
            }
            let es = a.enemy.as_ref().expect("enemy");
            let (radius, enc, pos) = (es.aggro_radius, es.encounter, a.pos);
            let trigger = self
                .alive_party()
                .find(|&p| self.actors[p].pos.dist(pos) <= radius + self.actors[p].radius);
            if let Some(p) = trigger {
                self.engage(enc, p);
            }
        }
    }

    fn update_auras(&mut self) {
        let mut hot_ticks = Vec::new();
        for id in 0..self.actors.len() {
            if !self.actors[id].alive {
                continue;
            }
            let auras = &mut self.actors[id].auras;
            for aura in auras.iter_mut() {
                match aura {
                    Aura::Hot {
                        source,
                        per_tick,
                        remaining,
                        tick_timer,
                    } => {
                        *remaining -= DT;
                        *tick_timer -= DT;
                        if *tick_timer <= 1e-4 {
                            *tick_timer += 1.0;
                            hot_ticks.push((*source, id, *per_tick));
                        }
                    }
                    Aura::DamageReduction { remaining, .. } | Aura::Taunted { remaining, .. } => {
                        *remaining -= DT;
                    }
                }
            }
            auras.retain(|a| match a {
                Aura::Hot { remaining, .. }
                | Aura::DamageReduction { remaining, .. }
                | Aura::Taunted { remaining, .. } => *remaining > 1e-4,
            });
        }
        for (src, dst, amount) in hot_ticks {
            let name = self.actors[src]
                .abilities
                .iter()
                .find(|a| matches!(a.def().effect, Effect::Hot { .. }))
                .map(|a| a.def().name)
                .unwrap_or("Heal over time");
            // HoT power was already applied at cast time in per_tick? No: apply here.
            self.heal(src, dst, amount, name);
        }
    }

    fn update_casts_and_cooldowns(&mut self) {
        for id in 0..self.actors.len() {
            {
                let a = &mut self.actors[id];
                for c in a.cooldowns.iter_mut() {
                    *c = (*c - DT).max(0.0);
                }
                a.gcd = (a.gcd - DT).max(0.0);
                a.swing_timer = (a.swing_timer - DT).max(0.0);
            }
            if !self.actors[id].alive {
                continue;
            }
            let Some(mut cast) = self.actors[id].cast else {
                continue;
            };
            cast.remaining -= DT;
            if cast.remaining > 1e-4 {
                self.actors[id].cast = Some(cast);
                continue;
            }
            self.actors[id].cast = None;
            let t = cast.target;
            match cast.kind {
                CastKind::Ability(slot) => {
                    let def = self.actors[id].abilities[slot].def();
                    let valid =
                        self.actors[t].alive && self.actors[t].mode() != Some(EnemyMode::Resetting);
                    // Small leeway so targets drifting out at the last moment still land.
                    if !valid {
                        self.emit(EventKind::CastFailed {
                            src: id,
                            what: def.name,
                            reason: CastError::InvalidTarget,
                        });
                    } else if !self.reach(id, t, def.range + 3.0) {
                        self.emit(EventKind::CastFailed {
                            src: id,
                            what: def.name,
                            reason: CastError::OutOfRange,
                        });
                    } else if self.actors[id].mana < def.mana_cost {
                        self.emit(EventKind::CastFailed {
                            src: id,
                            what: def.name,
                            reason: CastError::NotEnoughMana,
                        });
                    } else {
                        self.execute_ability(id, slot, t);
                    }
                }
                CastKind::BossBolt => {
                    let dmg = self.content.boss.bolt_damage;
                    self.deal_damage(id, t, dmg, "Hollow Bolt", 1.0, false);
                }
            }
        }
    }

    fn auto_attacks(&mut self) {
        for id in 0..self.actors.len() {
            let a = &self.actors[id];
            if !a.alive || a.cast.is_some() || !a.auto_attack || a.swing_timer > 0.0 {
                continue;
            }
            if a.is_enemy() && a.mode() != Some(EnemyMode::Engaged) {
                continue;
            }
            let Some(t) = a.target else { continue };
            if !self.actors[t].alive || !self.is_hostile(id, t) {
                continue;
            }
            if self.actors[t].mode() == Some(EnemyMode::Resetting) {
                continue;
            }
            if !self.reach(id, t, a.attack_range) {
                continue;
            }
            let (dmg, interval) = (a.attack_damage, a.attack_interval);
            let what = if a.attack_range > 5.0 {
                "Wand"
            } else {
                "Melee"
            };
            self.actors[id].swing_timer = interval;
            self.deal_damage(id, t, dmg, what, 1.0, true);
        }
    }

    fn enemy_specials(&mut self) {
        let boss_cfg = self.content.boss.clone();
        for e in self.enemies.clone() {
            if !self.actors[e].is_engaged_enemy() {
                continue;
            }
            // Elite cleave.
            let cleave = {
                let es = self.actors[e].enemy.as_mut().expect("enemy");
                match es.cleave.clone() {
                    Some(c) => {
                        es.cleave_timer -= DT;
                        if es.cleave_timer <= 0.0 {
                            es.cleave_timer = c.interval;
                            Some(c)
                        } else {
                            None
                        }
                    }
                    None => None,
                }
            };
            if let Some(c) = cleave {
                let pos = self.actors[e].pos;
                let hits: Vec<ActorId> = self
                    .alive_party()
                    .filter(|&p| {
                        self.actors[p].pos.dist(pos)
                            <= c.radius + self.actors[p].radius + self.actors[e].radius
                    })
                    .collect();
                for p in hits {
                    self.deal_damage(e, p, c.damage, "Rockfall Cleave", 0.0, false);
                }
            }
            if self.actors[e]
                .enemy
                .as_ref()
                .is_some_and(|s| s.boss.is_some())
            {
                self.boss_script(e, &boss_cfg);
            }
        }
    }

    fn boss_script(&mut self, e: ActorId, cfg: &crate::content::BossConfig) {
        let now = self.time;
        let hp_frac = self.actors[e].hp_frac();
        let (phase, pull_time, enraged) = {
            let b = self.actors[e]
                .enemy
                .as_ref()
                .and_then(|s| s.boss.as_ref())
                .expect("boss");
            (b.phase, b.pull_time, b.enraged)
        };
        if phase == 1 && hp_frac <= cfg.phase2_hp_fraction {
            {
                let a = &mut self.actors[e];
                a.power_mult *= cfg.phase2_damage_multiplier;
                if matches!(
                    a.cast,
                    Some(Cast {
                        kind: CastKind::BossBolt,
                        ..
                    })
                ) {
                    a.cast = None;
                }
                let b = a
                    .enemy
                    .as_mut()
                    .and_then(|s| s.boss.as_mut())
                    .expect("boss");
                b.phase = 2;
                b.ring_timer = 2.0;
            }
            if self.phase2_at.is_none() {
                self.phase2_at = Some(now);
            }
            self.emit(EventKind::PhaseChange { boss: e, phase: 2 });
        }
        if !enraged && now - pull_time >= cfg.enrage_seconds {
            let a = &mut self.actors[e];
            a.power_mult *= 5.0;
            a.enemy
                .as_mut()
                .and_then(|s| s.boss.as_mut())
                .expect("boss")
                .enraged = true;
            self.emit(EventKind::Enrage { boss: e });
        }
        let phase = self.boss_phase_of(e);
        if phase == 1 {
            let fire = {
                let b = self.boss_state_mut(e);
                b.bolt_timer -= DT;
                b.bolt_timer <= 0.0
            };
            if fire && self.actors[e].cast.is_none() {
                let tank_target = self.actors[e].target;
                let candidates: Vec<ActorId> = self
                    .alive_party()
                    .filter(|&p| Some(p) != tank_target && self.on_threat_table(e, p))
                    .collect();
                if !candidates.is_empty() {
                    let t = candidates[self.rng.index(candidates.len())];
                    self.actors[e].cast = Some(Cast {
                        kind: CastKind::BossBolt,
                        name: "Hollow Bolt",
                        target: t,
                        remaining: cfg.bolt_cast_time,
                        total: cfg.bolt_cast_time,
                    });
                    self.emit(EventKind::CastStart {
                        src: e,
                        what: "Hollow Bolt",
                        target: t,
                    });
                }
                self.boss_state_mut(e).bolt_timer = cfg.bolt_interval;
            }
        } else {
            let fire = {
                let b = self.boss_state_mut(e);
                b.ring_timer -= DT;
                b.ring_timer <= 0.0
            };
            if fire {
                self.boss_state_mut(e).ring_timer = cfg.ring_interval;
                let targets: Vec<ActorId> = self.alive_party().collect();
                if !targets.is_empty() {
                    let t = targets[self.rng.index(targets.len())];
                    let id = self.next_telegraph_id;
                    self.next_telegraph_id += 1;
                    let center = self.actors[t].pos;
                    self.telegraphs.push(Telegraph {
                        id,
                        source: e,
                        center,
                        radius: cfg.ring_radius,
                        remaining: cfg.ring_telegraph,
                        total: cfg.ring_telegraph,
                        damage: cfg.ring_damage,
                    });
                    self.emit(EventKind::TelegraphStart {
                        id,
                        source: e,
                        center,
                        radius: cfg.ring_radius,
                        delay: cfg.ring_telegraph,
                    });
                }
            }
        }
    }

    fn boss_phase_of(&self, e: ActorId) -> u8 {
        self.actors[e]
            .enemy
            .as_ref()
            .and_then(|s| s.boss.as_ref())
            .map(|b| b.phase)
            .unwrap_or(1)
    }

    fn boss_state_mut(&mut self, e: ActorId) -> &mut BossState {
        self.actors[e]
            .enemy
            .as_mut()
            .and_then(|s| s.boss.as_mut())
            .expect("boss")
    }

    fn update_telegraphs(&mut self) {
        let mut detonate = Vec::new();
        for t in self.telegraphs.iter_mut() {
            t.remaining -= DT;
            if t.remaining <= 1e-4 {
                detonate.push(t.clone());
            }
        }
        self.telegraphs.retain(|t| t.remaining > 1e-4);
        for t in detonate {
            let hits: Vec<ActorId> = self
                .alive_party()
                .filter(|&p| self.actors[p].pos.dist(t.center) <= t.radius + self.actors[p].radius)
                .collect();
            let n = hits.len() as u32;
            for p in hits {
                self.deal_damage(t.source, p, t.damage, "Cinder Ring", 0.0, false);
            }
            self.metrics.ring_hits += n;
            self.emit(EventKind::TelegraphHit { id: t.id, hits: n });
        }
    }

    fn regen(&mut self) {
        for p in self.party.clone() {
            let in_combat = self.in_combat(p);
            let a = &mut self.actors[p];
            if !a.alive {
                continue;
            }
            let mana_rate = if in_combat { a.combat_mana_regen } else { 0.10 };
            a.mana = (a.mana + a.max_mana * mana_rate * DT).min(a.max_mana);
            if !in_combat {
                a.hp = (a.hp + a.max_hp * 0.08 * DT).min(a.max_hp);
            }
        }
    }

    fn check_resets(&mut self) {
        let leash = self.content.dungeon.leash_radius;
        let mut to_reset = Vec::new();
        for e in self.enemies.clone() {
            let a = &self.actors[e];
            if !a.alive {
                continue;
            }
            let es = a.enemy.as_ref().expect("enemy");
            match es.mode {
                EnemyMode::Engaged => {
                    let no_targets = !es.threat.iter().any(|t| self.actors[t.actor].alive);
                    let leashed = a.pos.dist(es.home) > leash;
                    if (no_targets || leashed) && !to_reset.contains(&es.encounter) {
                        to_reset.push(es.encounter);
                    }
                }
                EnemyMode::Resetting => {
                    if a.pos.dist(es.home) < 0.3 {
                        let a = &mut self.actors[e];
                        let es = a.enemy.as_mut().expect("enemy");
                        es.mode = EnemyMode::Idle;
                        a.facing = es.home_facing;
                        a.hp = a.max_hp;
                        a.move_goal = None;
                    }
                }
                EnemyMode::Idle => {}
            }
        }
        for enc in to_reset {
            self.reset_encounter(enc);
        }
    }

    /// Living mobs drop combat, heal to full and walk home. Dead mobs stay dead.
    pub fn reset_encounter(&mut self, enc: usize) {
        let cfg_bolt = self.content.boss.bolt_interval;
        let cfg_ring = self.content.boss.ring_interval;
        for m in self.encounters[enc].mobs.clone() {
            let a = &mut self.actors[m];
            if !a.alive {
                continue;
            }
            a.hp = a.max_hp;
            a.cast = None;
            a.target = None;
            a.auras.clear();
            let es = a.enemy.as_mut().expect("enemy");
            es.mode = EnemyMode::Resetting;
            es.threat.clear();
            a.power_mult = es.base_power_mult;
            if let Some(b) = es.boss.as_mut() {
                b.phase = 1;
                b.enraged = false;
                b.bolt_timer = cfg_bolt;
                b.ring_timer = cfg_ring;
            }
            a.move_goal = Some(es.home);
            self.telegraphs.retain(|t| t.source != m);
        }
        self.emit(EventKind::EncounterReset { encounter: enc });
    }

    fn check_progress(&mut self) {
        for i in 0..self.encounters.len() {
            if self.encounters[i].cleared {
                continue;
            }
            if self.encounters[i]
                .mobs
                .iter()
                .all(|&m| !self.actors[m].alive)
            {
                self.encounters[i].cleared = true;
                self.encounters[i].cleared_at = Some(self.time);
                self.emit(EventKind::EncounterCleared { encounter: i });
            }
        }
        if self.outcome != RunOutcome::InProgress {
            return;
        }
        if self.alive_party().next().is_none() {
            self.outcome = RunOutcome::Wipe;
            self.ended_at = Some(self.time);
            self.emit(EventKind::Outcome {
                outcome: RunOutcome::Wipe,
            });
        } else if self.encounters[self.content.boss_encounter()].cleared {
            self.outcome = RunOutcome::Victory;
            self.ended_at = Some(self.time);
            self.emit(EventKind::Outcome {
                outcome: RunOutcome::Victory,
            });
        }
    }

    /// Enemy target selection: taunt overrides; otherwise keep the current target
    /// unless someone exceeds its threat by 10% (in melee reach) or 30% (out of reach).
    pub fn select_enemy_target(&self, e: ActorId) -> Option<ActorId> {
        let a = &self.actors[e];
        if let Some(by) = a.taunted_by()
            && self.actors[by].alive
        {
            return Some(by);
        }
        let es = a.enemy.as_ref()?;
        let valid: Vec<ThreatEntry> = es
            .threat
            .iter()
            .copied()
            .filter(|t| self.actors[t.actor].alive)
            .collect();
        let mut top: Option<ThreatEntry> = None;
        for t in &valid {
            if top.is_none_or(|b| t.value > b.value) {
                top = Some(*t);
            }
        }
        let top = top?;
        let current = a
            .target
            .and_then(|c| valid.iter().find(|t| t.actor == c).copied());
        match current {
            None => Some(top.actor),
            Some(cur) if cur.actor == top.actor => Some(cur.actor),
            Some(cur) => {
                let ratio = if self.reach(e, top.actor, a.attack_range) {
                    SWITCH_MELEE
                } else {
                    SWITCH_RANGED
                };
                if top.value > cur.value * ratio {
                    Some(top.actor)
                } else {
                    Some(cur.actor)
                }
            }
        }
    }

    /// Human readable description of an event, for the combat log.
    pub fn describe(&self, e: &Event) -> String {
        crate::events::describe(self, e)
    }
}
