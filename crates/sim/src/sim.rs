//! Deterministic fixed-timestep dungeon simulation.
//!
//! Everything gameplay-relevant lives here; the Bevy game only feeds player input
//! in and renders `Sim` state out. See `RULES.md` for the rules in prose and
//! `stats.rs` for the mitigation / hit-table formulas.

use crate::ai::{self, AiProfile, Blackboard};
use crate::class::{AbilityId, ClassId, Effect, GCD, ResourceKind, Role, TargetRule, level_power};
use crate::content::{
    AbilityKind, Content, DebuffEffect, Difficulty, EnemyAbility, EnemyRank, HazardConfig,
    PhaseConfig, TargetSelector,
};
use crate::events::{CastError, Event, EventKind, Label};
use crate::items::ItemStats;
use crate::math::{V2, V3};
use crate::metrics::Metrics;
use crate::nav::{Nav, SurfaceId};
use crate::rng::Rng;
use crate::stats::{self, HitKind, HitOutcome, MELEE_CRIT_MULT, SPELL_CRIT_MULT, School, Stats};

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
pub const PARTY_RADIUS: f32 = 0.5;
/// Height difference absorbed by bodies before it counts toward range.
pub const BODY_ALLOWANCE: f32 = 1.0;
/// Ground effects (rings, hazards, cleaves, AoE) reach this far above/below
/// their own elevation, so an effect on one floor misses the floor below.
pub const EFFECT_HEIGHT: f32 = 1.5;
/// Falling speed after stepping off a drop (m/s).
pub const FALL_SPEED: f32 = 12.0;
/// An AI actor that makes no headway for this long is stuck and recovers.
pub const STUCK_AFTER: f32 = 1.0;
/// Seconds spent steering to the recovery point.
pub const RECOVER_FOR: f32 = 0.8;
/// After two direction reversals in quick succession, keep heading for the
/// previous goal this long (two AI goals fighting over a boundary).
pub const COMMIT_FOR: f32 = 0.75;
/// Seconds the whole party must be out of combat before fallen members rally.
pub const RALLY_DELAY: f32 = 8.0;
/// Fraction of HP and resource fallen members rally with.
pub const RALLY_FRACTION: f32 = 0.4;
/// Mana regenerates at the full rate only this long after the last mana spent.
pub const FIVE_SECOND_RULE: f32 = 5.0;
/// Fraction of normal mana regen that still trickles in during the five seconds.
pub const CASTING_REGEN_FRACTION: f32 = 0.35;
/// Out-of-combat regeneration per second (fraction of max).
pub const OOC_MANA_REGEN: f32 = 0.10;
pub const OOC_HP_REGEN: f32 = 0.08;
pub const ENERGY_PER_SECOND: f32 = 10.0;
pub const RAGE_DECAY_PER_SECOND: f32 = 2.0;
pub const ENRAGE_MULT: f32 = 5.0;
/// [orig] Summoned adds inherit this fraction of their summoner's threat table.
pub const SUMMON_THREAT_INHERIT: f32 = 0.5;
/// [orig] In-run learning: each avoidable hit taken adds alertness (seconds of
/// reaction time saved), up to the cap.
pub const ALERTNESS_PER_HIT: f32 = 0.1;
pub const MAX_ALERTNESS: f32 = 0.3;

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
    pub level: u32,
    pub gear: ItemStats,
    pub player_controlled: bool,
    pub profile: AiProfile,
    /// 0..1 experience with this dungeon (from past clears); sharpens the AI.
    pub familiarity: f32,
}

impl PartyMemberSpec {
    pub fn new(name: &str, class: ClassId) -> Self {
        PartyMemberSpec {
            name: name.to_string(),
            class,
            level: 1,
            gear: ItemStats::default(),
            player_controlled: false,
            profile: AiProfile::Balanced,
            familiarity: 0.0,
        }
    }
}

/// In-game sandbox knobs applied on top of the content file for one run.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Overrides {
    /// Multiplies enemy HP.
    pub hp: f32,
    /// Multiplies enemy damage.
    pub damage: f32,
    /// Enemy abilities fire this many times faster (intervals and casts divided).
    pub ability_speed: f32,
    /// Multiplies ground-telegraph warning time.
    pub telegraph: f32,
}

impl Default for Overrides {
    fn default() -> Self {
        Overrides {
            hp: 1.0,
            damage: 1.0,
            ability_speed: 1.0,
            telegraph: 1.0,
        }
    }
}

impl Overrides {
    pub const MIN: f32 = 0.25;
    pub const MAX: f32 = 4.0;

    pub fn clamped(self) -> Overrides {
        let c = |v: f32| v.clamp(Self::MIN, Self::MAX);
        Overrides {
            hp: c(self.hp),
            damage: c(self.damage),
            ability_speed: c(self.ability_speed),
            telegraph: c(self.telegraph),
        }
    }

    pub fn is_default(&self) -> bool {
        *self == Overrides::default()
    }

    /// [orig] Rewards scale down when the sandbox makes a run easier (never up):
    /// `clamp(hp * damage * ability_speed / telegraph, 0.25, 1)`.
    pub fn reward_factor(&self) -> f32 {
        let o = self.clamped();
        (o.hp * o.damage * o.ability_speed / o.telegraph).clamp(0.25, 1.0)
    }

    /// Apply to a content copy. Keeps every validated invariant (ring delay
    /// shorter than its interval, casts shorter than intervals).
    pub fn apply(&self, content: &mut Content) {
        let o = self.clamped();
        content.dungeon.hp_multiplier *= o.hp;
        content.dungeon.damage_multiplier *= o.damage;
        for t in content.templates.values_mut() {
            for a in t.abilities.iter_mut() {
                a.interval = (a.interval / o.ability_speed).max(1.0);
                if let Some(i) = a.initial.as_mut() {
                    *i /= o.ability_speed;
                }
                a.cast_time = (a.cast_time / o.ability_speed).min(a.interval * 0.9);
                if a.delay > 0.0 {
                    a.delay = (a.delay * o.telegraph).clamp(0.5, a.interval * 0.9);
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub difficulty: String,
    pub overrides: Overrides,
    /// The AI leader walks the party through the dungeon.
    pub autopilot: bool,
    /// Autopilot skips optional encounters.
    pub skip_optional: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        RunOptions {
            difficulty: "normal".into(),
            overrides: Overrides::default(),
            autopilot: false,
            skip_optional: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CastKind {
    /// Index into the caster's (player-side) ability list.
    Ability(usize),
    /// Index into an enemy's ability list.
    Enemy(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cast {
    pub kind: CastKind,
    pub name: Label,
    pub target: ActorId,
    pub remaining: f32,
    pub total: f32,
    pub interruptible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AuraKind {
    Hot {
        per_tick: f32,
    },
    Dot {
        per_tick: f32,
        school: School,
        ignores_armor: bool,
    },
    DamageReduction {
        pct: f32,
    },
    Absorb {
        remaining: f32,
    },
    /// Cannot receive another absorb shield.
    WeakenedSoul,
    Taunted {
        by: ActorId,
    },
    /// No actions or movement; broken by damage.
    Incapacitated,
    /// Cannot cast spells or use special abilities.
    Silenced,
    /// +pct damage taken per stack.
    DamageTaken {
        pct: f32,
    },
    /// −pct healing received per stack.
    HealingTaken {
        pct: f32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Aura {
    pub name: Label,
    pub kind: AuraKind,
    pub source: ActorId,
    pub remaining: f32,
    pub duration: f32,
    pub tick_timer: f32,
    pub stacks: u32,
    pub max_stacks: u32,
    pub dispellable: bool,
    pub harmful: bool,
}

impl Aura {
    fn new(name: Label, kind: AuraKind, source: ActorId, duration: f32, harmful: bool) -> Self {
        Aura {
            name,
            kind,
            source,
            remaining: duration,
            duration,
            tick_timer: 1.0,
            stacks: 1,
            max_stacks: 1,
            dispellable: false,
            harmful,
        }
    }
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

#[derive(Debug, Clone)]
pub struct EnemyAbilityState {
    pub cfg: EnemyAbility,
    pub label: Label,
    pub timer: f32,
}

#[derive(Debug, Clone)]
pub struct EnemyState {
    pub template_key: String,
    pub rank: EnemyRank,
    pub encounter: usize,
    pub home: V3,
    pub home_facing: f32,
    pub aggro_radius: f32,
    pub threat: Vec<ThreatEntry>,
    pub mode: EnemyMode,
    pub base_power_mult: f32,
    pub cc_immune: bool,
    /// Summoned mid-fight: not required for clearing and removed on reset.
    pub summoned: bool,
    /// Removed from the world (summon despawned); not a kill.
    pub despawned: bool,
    pub phase: u8,
    pub phases: Vec<PhaseConfig>,
    pub abilities: Vec<EnemyAbilityState>,
    pub enrage_seconds: Option<f32>,
    pub enraged: bool,
    pub pull_time: f32,
}

#[derive(Debug, Clone)]
pub struct Actor {
    pub id: ActorId,
    pub name: String,
    pub team: Team,
    pub class: Option<ClassId>,
    pub controller: Controller,
    pub profile: AiProfile,
    /// 0..1 dungeon familiarity (persistent learning).
    pub familiarity: f32,
    /// 0..0.3 in-run alertness, raised by taking avoidable damage.
    pub alertness: f32,
    /// Horizontal position. Elevation is `y`; the full position is `loc()`.
    pub pos: V2,
    /// Feet elevation, maintained by the navigation (ramps, falls).
    pub y: f32,
    /// The walkable surface the actor stands on.
    pub surface: SurfaceId,
    pub facing: f32,
    pub radius: f32,
    pub speed: f32,
    pub hp: f32,
    pub max_hp: f32,
    pub resource_kind: ResourceKind,
    pub resource: f32,
    pub max_resource: f32,
    pub last_mana_spend: f32,
    pub alive: bool,
    pub stats: Stats,
    /// Outgoing damage/healing multiplier.
    pub power_mult: f32,
    pub threat_mult: f32,
    pub mana_regen: f32,
    pub attack_damage: f32,
    pub attack_interval: f32,
    pub attack_range: f32,
    pub attack_kind: HitKind,
    pub attack_school: School,
    pub swing_timer: f32,
    pub auto_attack: bool,
    pub abilities: Vec<AbilityId>,
    pub ability_labels: Vec<Label>,
    pub cooldowns: Vec<f32>,
    pub gcd: f32,
    pub cast: Option<Cast>,
    pub target: Option<ActorId>,
    pub auras: Vec<Aura>,
    pub last_combat: f32,
    /// AI steering goal (with elevation: it says which floor).
    pub move_goal: Option<V3>,
    /// Player steering direction (world space, unnormalized).
    pub move_dir: V2,
    pub moved_last_tick: bool,
    pub nav: NavState,
    pub enemy: Option<EnemyState>,
}

/// Per-actor navigation bookkeeping (stuck detection, unreachable goals).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NavState {
    /// The current goal could not be reached last tick.
    pub unreachable: bool,
    /// Seconds of trying to move without getting anywhere.
    pub stalled: f32,
    /// Seconds left steering to `recover_to`.
    pub recover: f32,
    pub recover_to: V2,
    /// Total seconds spent stuck this run.
    pub stuck_time: f32,
    /// Direction of the previous step and the goal it served.
    pub last_step: V2,
    pub prev_goal: Option<V3>,
    /// Decaying count of recent direction reversals.
    pub dither: f32,
    /// Seconds left committed to `commit_goal` after dithering.
    pub commit: f32,
    pub commit_goal: V3,
    /// Times this actor dithered this run.
    pub dithers: u32,
}

impl Actor {
    /// Full world position.
    pub fn loc(&self) -> V3 {
        V3::at(self.pos, self.y)
    }

    /// Body height (for line of sight and ground effects).
    pub fn height(&self) -> f32 {
        if self.is_enemy() {
            (self.radius * 3.2).max(1.6)
        } else {
            2.0
        }
    }

    /// Eye point (line of sight origin).
    pub fn eye(&self) -> V3 {
        V3::at(self.pos, self.y + self.height() * 0.85)
    }

    /// Chest point (line of sight target).
    pub fn chest(&self) -> V3 {
        V3::at(self.pos, self.y + self.height() * 0.5)
    }

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

    pub fn resource_frac(&self) -> f32 {
        if self.max_resource > 0.0 {
            self.resource / self.max_resource
        } else {
            1.0
        }
    }

    /// Mana fraction for mana users, 1.0 for everyone else.
    pub fn mana_frac(&self) -> f32 {
        if self.resource_kind == ResourceKind::Mana {
            self.resource_frac()
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

    pub fn rank(&self) -> Option<EnemyRank> {
        self.enemy.as_ref().map(|e| e.rank)
    }

    pub fn has_aura(&self, f: impl Fn(&AuraKind) -> bool) -> bool {
        self.auras.iter().any(|a| f(&a.kind))
    }

    pub fn has_hot_from(&self, source: ActorId) -> bool {
        self.auras
            .iter()
            .any(|a| a.source == source && matches!(a.kind, AuraKind::Hot { .. }))
    }

    pub fn has_dot_from(&self, source: ActorId) -> bool {
        self.auras
            .iter()
            .any(|a| a.source == source && matches!(a.kind, AuraKind::Dot { .. }))
    }

    pub fn is_incapacitated(&self) -> bool {
        self.has_aura(|k| matches!(k, AuraKind::Incapacitated))
    }

    pub fn is_silenced(&self) -> bool {
        self.has_aura(|k| matches!(k, AuraKind::Silenced))
    }

    pub fn absorb_remaining(&self) -> f32 {
        self.auras
            .iter()
            .map(|a| match a.kind {
                AuraKind::Absorb { remaining } => remaining,
                _ => 0.0,
            })
            .sum()
    }

    pub fn damage_reduction(&self) -> f32 {
        self.auras
            .iter()
            .filter_map(|a| match a.kind {
                AuraKind::DamageReduction { pct } => Some(pct),
                _ => None,
            })
            .fold(0.0, f32::max)
    }

    /// Multiplier from stacking "damage taken" debuffs.
    pub fn damage_taken_mult(&self) -> f32 {
        1.0 + self
            .auras
            .iter()
            .map(|a| match a.kind {
                AuraKind::DamageTaken { pct } => pct * a.stacks as f32,
                _ => 0.0,
            })
            .sum::<f32>()
    }

    pub fn healing_taken_mult(&self) -> f32 {
        (1.0 - self
            .auras
            .iter()
            .map(|a| match a.kind {
                AuraKind::HealingTaken { pct } => pct * a.stacks as f32,
                _ => 0.0,
            })
            .sum::<f32>())
        .max(0.0)
    }

    pub fn has_dispellable_debuff(&self) -> bool {
        self.auras.iter().any(|a| a.harmful && a.dispellable)
    }

    pub fn taunted_by(&self) -> Option<ActorId> {
        self.auras.iter().find_map(|a| match a.kind {
            AuraKind::Taunted { by } => Some(by),
            _ => None,
        })
    }

    pub fn ability_slot(&self, id: AbilityId) -> Option<usize> {
        self.abilities.iter().position(|a| *a == id)
    }

    pub fn ability_ready(&self, slot: usize) -> bool {
        self.cooldowns.get(slot).is_some_and(|c| *c <= 0.0)
            && self.gcd <= 0.0
            && self.resource >= self.abilities[slot].def().cost
    }

    pub fn phase(&self) -> u8 {
        self.enemy.as_ref().map(|e| e.phase).unwrap_or(1)
    }
}

#[derive(Debug, Clone)]
pub struct EncounterState {
    pub id: String,
    pub name: String,
    pub optional: bool,
    pub mobs: Vec<ActorId>,
    pub cleared: bool,
    pub cleared_at: Option<f32>,
    pub pulls: u32,
}

#[derive(Debug, Clone)]
pub struct Telegraph {
    pub id: u32,
    pub source: ActorId,
    pub what: Label,
    pub school: School,
    pub center: V2,
    /// Elevation of the surface the ring is on.
    pub y: f32,
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
pub struct HazardState {
    pub cfg: HazardConfig,
    /// Resolved position (with the elevation of its floor).
    pub pos: V3,
    pub label: Label,
    pub encounter: usize,
    pub active: bool,
    tick_timer: f32,
}

impl HazardState {
    /// Seconds until this hazard next turns on (0 if active).
    pub fn time_to_active(&self, time: f32) -> f32 {
        if self.active {
            return 0.0;
        }
        let period = self.cfg.on + self.cfg.off;
        let phase = (time + self.cfg.offset) % period;
        period - phase
    }
}

/// How close someone must get (fraction of the threat needed to pull) before the
/// UI calls the threat "contested".
pub const THREAT_WARNING: f32 = 0.8;

/// Threat situation of one enemy, seen from a tank's perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TankThreat {
    /// Not in combat.
    Idle,
    /// Attacking me (or taunted by me) and nobody is close to pulling it.
    Secure,
    /// Attacking a tank, but someone is within `THREAT_WARNING` of pulling it.
    Contested,
    /// Held by another tank, nobody close.
    OtherTank,
    /// Attacking a non-tank: needs a taunt.
    Lost,
}

/// Threat situation of one enemy, seen from a non-tank's perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonalThreat {
    Idle,
    /// Comfortably below the threshold.
    Safe,
    /// Within `THREAT_WARNING` of pulling aggro.
    Warning,
    /// The enemy is attacking me.
    Attacking,
}

/// Threat information about `enemy` for `viewer` (see `Sim::threat_view`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThreatView {
    pub attacking: Option<ActorId>,
    pub taunted: bool,
    /// Viewer's threat.
    pub mine: f32,
    /// Highest threat of anyone other than the viewer.
    pub top_other: Option<(ActorId, f32)>,
    /// Threat the viewer needs to pull the enemy (0 if the viewer already holds it).
    pub pull_at: f32,
    /// Viewer's threat as a fraction of `pull_at` (0 when holding it).
    pub pull_fraction: f32,
    /// Closest competitor's fraction of the threat needed to pull from the holder.
    pub contest: f32,
    pub tank: TankThreat,
    pub personal: PersonalThreat,
}

/// Per-second resource regeneration for an actor (negative for rage decay).
pub fn resource_regen_per_sec(a: &Actor, in_combat: bool, now: f32) -> f32 {
    match a.resource_kind {
        ResourceKind::Mana => {
            let rate = if !in_combat {
                OOC_MANA_REGEN
            } else if now - a.last_mana_spend >= FIVE_SECOND_RULE {
                a.mana_regen
            } else {
                a.mana_regen * CASTING_REGEN_FRACTION
            };
            a.max_resource * rate
        }
        ResourceKind::Energy => ENERGY_PER_SECOND,
        ResourceKind::Rage => {
            if in_combat {
                0.0
            } else {
                -RAGE_DECAY_PER_SECOND
            }
        }
    }
}

/// Action-bar view of one ability (see `Sim::ability_state`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AbilityState {
    pub cooldown_left: f32,
    pub cooldown_total: f32,
    pub gcd_left: f32,
    pub gcd_total: f32,
    pub cost: f32,
    /// Seconds until the resource cost is affordable (`None` = not by regeneration).
    pub resource_in: Option<f32>,
    /// `None` when the ability has no target to measure (self abilities, no target).
    pub in_range: Option<bool>,
    /// Exactly what would happen if pressed now.
    pub result: Result<(), CastError>,
}

/// Everything about how one hit is resolved.
#[derive(Debug, Clone)]
pub struct DamageSpec {
    pub base: f32,
    pub school: School,
    pub kind: HitKind,
    pub threat_mult: f32,
    pub ignores_armor: bool,
    pub what: Label,
    /// Counts toward "avoidable damage" metrics (rings, hazards, frontal cleaves).
    pub avoidable: bool,
    /// White melee swings generate rage.
    pub auto_attack: bool,
}

impl DamageSpec {
    pub fn new(base: f32, school: School, kind: HitKind, what: Label) -> Self {
        DamageSpec {
            base,
            school,
            kind,
            threat_mult: 1.0,
            ignores_armor: false,
            what,
            avoidable: false,
            auto_attack: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sim {
    pub content: Content,
    pub difficulty: Difficulty,
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
    pub hazards: Vec<HazardState>,
    next_telegraph_id: u32,
    pub events: Vec<Event>,
    pub metrics: Metrics,
    pub outcome: RunOutcome,
    pub player: Option<ActorId>,
    /// Party member the others follow out of combat.
    pub leader: ActorId,
    pub leader_heading: V2,
    pub options: RunOptions,
    /// Queued ability press: slot and optional explicit (mouseover) target.
    pending_player_cast: Option<(usize, Option<ActorId>)>,
    /// The final boss (shown in the boss frame).
    pub boss: ActorId,
    pub phase2_at: Option<f32>,
    /// Sim time at which the outcome was decided.
    pub ended_at: Option<f32>,
    /// Shared party tactical state, recomputed every tick.
    pub board: Blackboard,
    label_melee: Label,
    label_wand: Label,
}

impl Sim {
    /// Build a run on Normal difficulty with no autopilot.
    pub fn new(content: &Content, party: &[PartyMemberSpec], seed: u64) -> Sim {
        Sim::with_options(content, party, seed, RunOptions::default())
    }

    /// Build a run. `party[0]` is the player slot. The leader is the player if one is
    /// player-controlled, otherwise the first tank (or the first member).
    pub fn with_options(
        content: &Content,
        party: &[PartyMemberSpec],
        seed: u64,
        options: RunOptions,
    ) -> Sim {
        let mut content = content.clone();
        options.overrides.apply(&mut content);
        let nav = content.nav();
        let difficulty = content
            .difficulty(&options.difficulty)
            .cloned()
            .unwrap_or_else(|| content.difficulties[0].clone());
        let mut sim = Sim {
            nav,
            difficulty,
            seed,
            rng: Rng::new(seed),
            time: 0.0,
            tick: 0,
            actors: Vec::new(),
            party: Vec::new(),
            enemies: Vec::new(),
            encounters: Vec::new(),
            telegraphs: Vec::new(),
            hazards: Vec::new(),
            next_telegraph_id: 1,
            events: Vec::new(),
            metrics: Metrics::default(),
            outcome: RunOutcome::InProgress,
            player: None,
            leader: 0,
            leader_heading: V2::new(0.0, 1.0),
            options,
            pending_player_cast: None,
            boss: 0,
            phase2_at: None,
            ended_at: None,
            board: Blackboard::default(),
            label_melee: Label::from("Melee"),
            label_wand: Label::from("Wand"),
            content,
        };
        let (entrance3, _) = Content::place(
            &sim.nav,
            sim.content.dungeon.entrance,
            sim.content.dungeon.entrance_y,
        )
        .expect("validated entrance");
        let entrance = entrance3.xz();
        for (i, spec) in party.iter().enumerate() {
            let cs = spec.class.stats(spec.level);
            // Spread the party in a small grid behind the entrance.
            let col = (i % 5) as f32;
            let row = (i / 5) as f32;
            let offset = V2::new((col - 2.0) * 1.5, -row * 1.5 - (i % 2) as f32 * 0.5);
            let mut st = cs.stats;
            spec.gear.apply(&mut st);
            let max_hp = (cs.max_hp + spec.gear.stamina * 10.0).round();
            let abilities = spec.class.abilities().to_vec();
            let labels = abilities
                .iter()
                .map(|a| Label::from(a.def().name))
                .collect();
            let id = sim.actors.len();
            let start_resource = match cs.resource {
                ResourceKind::Rage => 0.0,
                _ => cs.max_resource,
            };
            let (start, start_surface) = sim.nav.project(entrance + offset, entrance3.y);
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
                profile: spec.profile,
                familiarity: spec.familiarity.clamp(0.0, 1.0),
                alertness: 0.0,
                pos: start.xz(),
                y: start.y,
                surface: start_surface,
                facing: 0.0,
                radius: PARTY_RADIUS,
                speed: cs.speed,
                hp: max_hp,
                max_hp,
                resource_kind: cs.resource,
                resource: start_resource,
                max_resource: cs.max_resource,
                last_mana_spend: f32::NEG_INFINITY,
                alive: true,
                stats: st,
                power_mult: level_power(spec.level) * (1.0 + spec.gear.power / 100.0),
                threat_mult: cs.threat_mult,
                mana_regen: cs.mana_regen,
                attack_damage: cs.attack_damage,
                attack_interval: cs.attack_interval,
                attack_range: cs.attack_range,
                attack_kind: cs.attack_kind,
                attack_school: cs.attack_school,
                swing_timer: 0.0,
                auto_attack: false,
                cooldowns: vec![0.0; abilities.len()],
                abilities,
                ability_labels: labels,
                gcd: 0.0,
                cast: None,
                target: None,
                auras: Vec::new(),
                last_combat: f32::NEG_INFINITY,
                move_goal: None,
                move_dir: V2::ZERO,
                moved_last_tick: false,
                nav: NavState::default(),
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

        let encounters = sim.content.encounters.clone();
        for (ei, enc) in encounters.iter().enumerate() {
            sim.encounters.push(EncounterState {
                id: enc.id.clone(),
                name: enc.display_name().to_string(),
                optional: enc.optional,
                mobs: Vec::new(),
                cleared: false,
                cleared_at: None,
                pulls: 0,
            });
            for spawn in &enc.mobs {
                let (pos, _) =
                    Content::place(&sim.nav, spawn.pos, spawn.y).expect("validated spawn");
                let id = sim.spawn_enemy(&spawn.template, pos, ei, false);
                sim.encounters[ei].mobs.push(id);
            }
            for h in &enc.hazards {
                let (pos, _) = Content::place(&sim.nav, h.pos, h.y).expect("validated hazard");
                sim.hazards.push(HazardState {
                    pos,
                    label: Label::from(h.name.as_str()),
                    cfg: h.clone(),
                    encounter: ei,
                    active: false,
                    tick_timer: 0.0,
                });
            }
        }
        let fin = sim.content.final_encounter();
        sim.boss = sim.encounters[fin]
            .mobs
            .iter()
            .copied()
            .find(|&m| sim.actors[m].rank() == Some(EnemyRank::Boss))
            .unwrap_or(sim.encounters[fin].mobs[0]);
        sim.metrics = Metrics::new(&sim.actors);
        sim
    }

    /// Create an enemy from a template. Summoned enemies start engaged elsewhere.
    fn spawn_enemy(&mut self, key: &str, at: V3, encounter: usize, summoned: bool) -> ActorId {
        let pos = at.xz();
        let surface = self
            .nav
            .locate(pos, at.y)
            .unwrap_or_else(|| self.nav.project(pos, at.y).1);
        let t = self.content.templates[key].clone();
        let id = self.actors.len();
        let hp_mult = self.content.dungeon.hp_multiplier * self.difficulty.hp;
        let dmg_mult = self.content.dungeon.damage_multiplier * self.difficulty.damage;
        let max_hp = (t.hp * hp_mult).round().max(1.0);
        let entrance = self.content.dungeon.entrance;
        let home_facing = (entrance - pos).heading();
        let abilities = t
            .abilities
            .iter()
            .map(|a| EnemyAbilityState {
                label: Label::from(a.name.as_str()),
                timer: a.initial.unwrap_or(a.interval),
                cfg: a.clone(),
            })
            .collect();
        let stats = Stats {
            level: t.level + self.difficulty.level_bonus,
            armor: t.armor,
            resist: t.resist,
            dodge: t.dodge,
            parry: t.parry,
            block: t.block,
            block_value: 0.0,
            crit: t.crit,
            hit: 0.0,
        };
        self.actors.push(Actor {
            id,
            name: t.name.clone(),
            team: Team::Enemy,
            class: None,
            controller: Controller::Ai,
            profile: AiProfile::Balanced,
            familiarity: 0.0,
            alertness: 0.0,
            pos,
            y: self.nav.height(surface, pos),
            surface,
            facing: home_facing,
            radius: t.radius,
            speed: t.speed,
            hp: max_hp,
            max_hp,
            resource_kind: ResourceKind::Mana,
            resource: 0.0,
            max_resource: 0.0,
            last_mana_spend: f32::NEG_INFINITY,
            alive: true,
            stats,
            power_mult: dmg_mult,
            threat_mult: 1.0,
            mana_regen: 0.0,
            attack_damage: t.melee_damage,
            attack_interval: t.attack_interval,
            attack_range: t.attack_range,
            attack_kind: HitKind::Melee,
            attack_school: School::Physical,
            swing_timer: 0.0,
            auto_attack: true,
            abilities: Vec::new(),
            ability_labels: Vec::new(),
            cooldowns: Vec::new(),
            gcd: 0.0,
            cast: None,
            target: None,
            auras: Vec::new(),
            last_combat: f32::NEG_INFINITY,
            move_goal: None,
            move_dir: V2::ZERO,
            moved_last_tick: false,
            nav: NavState::default(),
            enemy: Some(EnemyState {
                template_key: key.to_string(),
                rank: t.rank,
                encounter,
                home: V3::at(pos, self.nav.height(surface, pos)),
                home_facing,
                aggro_radius: t.aggro_radius,
                threat: Vec::new(),
                mode: EnemyMode::Idle,
                base_power_mult: dmg_mult,
                cc_immune: t.is_cc_immune(),
                summoned,
                despawned: false,
                phase: 1,
                phases: t.phases.clone(),
                abilities,
                enrage_seconds: t.enrage_seconds,
                enraged: false,
                pull_time: 0.0,
            }),
        });
        self.enemies.push(id);
        if self.metrics.actors.len() < self.actors.len() {
            self.metrics.push_actor(&self.actors[id]);
        }
        id
    }

    // ------------------------------------------------------------ queries --

    pub fn actor(&self, id: ActorId) -> &Actor {
        &self.actors[id]
    }

    /// Is `b` within `range` of `a`? Horizontal edge-to-edge distance combined
    /// with the height difference beyond `BODY_ALLOWANCE`: a floor between two
    /// actors puts them out of melee range even if they overlap horizontally.
    pub fn reach(&self, a: ActorId, b: ActorId, range: f32) -> bool {
        let (a, b) = (&self.actors[a], &self.actors[b]);
        let h = (a.pos.dist(b.pos) - a.radius - b.radius).max(0.0);
        let v = ((a.y - b.y).abs() - BODY_ALLOWANCE).max(0.0);
        h * h + v * v <= range * range
    }

    /// Can `a` see `b`? From `a`'s eyes to `b`'s chest or eyes, against walls
    /// and solid floors.
    pub fn has_los(&self, a: ActorId, b: ActorId) -> bool {
        if a == b {
            return true;
        }
        let (x, y) = (&self.actors[a], &self.actors[b]);
        self.nav.line_of_sight(x.eye(), y.chest()) || self.nav.line_of_sight(x.eye(), y.eye())
    }

    /// Is actor `id` inside a ground effect of `radius` centred at `center`?
    pub fn in_effect(&self, id: ActorId, center: V3, radius: f32) -> bool {
        let a = &self.actors[id];
        a.pos.dist(center.xz()) <= radius + a.radius && (a.y - center.y).abs() <= EFFECT_HEIGHT
    }

    /// Horizontal proximity on the same level (AI heuristics, AoE counting).
    pub fn near(&self, a: ActorId, b: ActorId, dist: f32) -> bool {
        let (x, y) = (&self.actors[a], &self.actors[b]);
        x.pos.dist(y.pos) <= dist && (x.y - y.y).abs() <= EFFECT_HEIGHT
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

    pub fn top_threat(&self, enemy: ActorId) -> f32 {
        self.actors[enemy]
            .enemy
            .as_ref()
            .map(|e| e.threat.iter().map(|t| t.value).fold(0.0, f32::max))
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

    /// The first living tank.
    pub fn tank(&self) -> Option<ActorId> {
        self.alive_party()
            .find(|&p| self.actors[p].role() == Some(Role::Tank))
    }

    pub fn is_tank(&self, id: ActorId) -> bool {
        self.actors[id].role() == Some(Role::Tank)
    }

    /// Phase of the final boss.
    pub fn boss_phase(&self) -> u8 {
        self.actors[self.boss].phase()
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

    /// Current resource regeneration (per second) of an actor, exactly as `regen` applies it.
    pub fn resource_rate(&self, id: ActorId) -> f32 {
        resource_regen_per_sec(&self.actors[id], self.in_combat(id), self.time)
    }

    /// Seconds until the five-second rule lets full mana regeneration resume (0 if active).
    pub fn five_second_rule_left(&self, id: ActorId) -> f32 {
        let a = &self.actors[id];
        if a.resource_kind != ResourceKind::Mana || !self.in_combat(id) {
            return 0.0;
        }
        (FIVE_SECOND_RULE - (self.time - a.last_mana_spend)).max(0.0)
    }

    /// Everything the action bar needs to know about one ability, computed from the
    /// same rules as `validate_cast`.
    pub fn ability_state(
        &self,
        caster: ActorId,
        slot: usize,
        target: Option<ActorId>,
    ) -> AbilityState {
        let a = &self.actors[caster];
        let def = a.abilities[slot].def();
        let rate = self.resource_rate(caster);
        let missing = (def.cost - a.resource).max(0.0);
        let resource_in = if missing <= 0.0 {
            Some(0.0)
        } else if rate > 0.0 {
            Some(missing / rate)
        } else {
            None
        };
        // Range only matters for targeted abilities with a resolvable target.
        let range_target = match def.target {
            TargetRule::SelfOnly => None,
            TargetRule::Enemy => {
                target.filter(|&t| self.actors[t].alive && self.is_hostile(caster, t))
            }
            TargetRule::Ally => target
                .filter(|&t| self.actors[t].alive && !self.is_hostile(caster, t))
                .or(Some(caster)),
        };
        let in_range = range_target.map(|t| self.reach(caster, t, def.range));
        AbilityState {
            cooldown_left: a.cooldowns[slot],
            cooldown_total: def.cooldown,
            gcd_left: a.gcd,
            gcd_total: GCD,
            cost: def.cost,
            resource_in,
            in_range,
            result: self.validate_cast(caster, slot, target).map(|_| ()),
        }
    }

    /// Threat factor someone needs over the current target's threat to take the
    /// enemy: 1.1 within the enemy's melee reach, 1.3 outside (same rule as
    /// `select_enemy_target`).
    fn pull_factor(&self, enemy: ActorId, who: ActorId) -> f32 {
        if self.reach(enemy, who, self.actors[enemy].attack_range) {
            SWITCH_MELEE
        } else {
            SWITCH_RANGED
        }
    }

    /// Read-only threat summary of `enemy` for `viewer`, from the real threat table.
    pub fn threat_view(&self, enemy: ActorId, viewer: ActorId) -> ThreatView {
        let e = &self.actors[enemy];
        let mine = self.threat(enemy, viewer);
        let engaged = e.is_engaged_enemy();
        let attacking = if engaged { e.target } else { None };
        let taunted = e.taunted_by().is_some();
        let mut top_other: Option<(ActorId, f32)> = None;
        if let Some(es) = &e.enemy {
            for t in es
                .threat
                .iter()
                .filter(|t| t.actor != viewer && self.actors[t.actor].alive)
            {
                if top_other.is_none_or(|(_, v)| t.value > v) {
                    top_other = Some((t.actor, t.value));
                }
            }
        }
        // How close anyone (other than the holder) is to pulling it.
        let mut contest: f32 = 0.0;
        if let (Some(holder), Some(es)) = (attacking, &e.enemy) {
            let held = self.threat(enemy, holder).max(1.0);
            for t in es
                .threat
                .iter()
                .filter(|t| t.actor != holder && self.actors[t.actor].alive)
            {
                contest = contest.max(t.value / (held * self.pull_factor(enemy, t.actor)));
            }
        }
        let (pull_at, pull_fraction) = match attacking {
            Some(h) if h != viewer => {
                let need = self.threat(enemy, h).max(1.0) * self.pull_factor(enemy, viewer);
                (need, mine / need)
            }
            _ => (0.0, 0.0),
        };
        let tank = match attacking {
            None => TankThreat::Idle,
            Some(h) if !self.is_tank(h) => TankThreat::Lost,
            _ if contest >= THREAT_WARNING && !taunted => TankThreat::Contested,
            Some(h) if h == viewer => TankThreat::Secure,
            Some(_) => TankThreat::OtherTank,
        };
        let personal = match attacking {
            None => PersonalThreat::Idle,
            Some(h) if h == viewer => PersonalThreat::Attacking,
            _ if pull_fraction >= THREAT_WARNING => PersonalThreat::Warning,
            _ => PersonalThreat::Safe,
        };
        ThreatView {
            attacking,
            taunted,
            mine,
            top_other,
            pull_at,
            pull_fraction,
            contest,
            tank,
            personal,
        }
    }

    pub fn label(&self, id: ActorId, slot: usize) -> Label {
        self.actors[id].ability_labels[slot].clone()
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

    /// Queue an ability press (slot 0..5) to be resolved on the next tick.
    pub fn queue_player_cast(&mut self, slot: usize) {
        self.pending_player_cast = Some((slot, None));
    }

    /// Queue an ability press on an explicit target (e.g. a mouseover party frame)
    /// without changing the player's selected target. For friendly spells the
    /// target order is: this target if it is a living ally, then the selected
    /// target if it is a living ally, then the player. Hostile spells always use
    /// the selected target. The simulation validates everything as usual.
    pub fn queue_player_cast_on(&mut self, slot: usize, target: Option<ActorId>) {
        self.pending_player_cast = Some((slot, target));
    }

    /// The target a player press of `slot` would use (see `queue_player_cast_on`).
    /// `None` for a friendly spell means "yourself".
    pub fn player_cast_target(&self, slot: usize, explicit: Option<ActorId>) -> Option<ActorId> {
        let p = self.player?;
        let selected = self.actors[p].target;
        let friendly = |t: ActorId| self.actors[t].alive && !self.is_hostile(p, t);
        match self.actors[p].abilities.get(slot).map(|a| a.def().target) {
            Some(TargetRule::Ally) => explicit
                .filter(|&t| friendly(t))
                .or(selected.filter(|&t| friendly(t))),
            _ => selected,
        }
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
        self.update_hazards();
        self.regen();
        self.check_resets();
        self.check_progress();
        self.rally_fallen();
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
        for i in 0..self.enemies.len() {
            let e = self.enemies[i];
            if !self.actors[e].is_engaged_enemy() {
                continue;
            }
            self.actors[e].last_combat = now;
            let n = self.actors[e].enemy.as_ref().map_or(0, |s| s.threat.len());
            for k in 0..n {
                let a = self.actors[e].enemy.as_ref().expect("enemy").threat[k].actor;
                self.actors[a].last_combat = now;
            }
        }
    }

    fn run_ai(&mut self) {
        let leader = &self.actors[self.leader];
        if leader.moved_last_tick {
            self.leader_heading = V2::from_heading(leader.facing);
        }
        self.board = ai::compute_blackboard(self);
        for id in 0..self.actors.len() {
            if !self.actors[id].alive || self.actors[id].controller == Controller::Player {
                continue;
            }
            if self.actors[id].is_incapacitated() {
                self.actors[id].move_goal = None;
                continue;
            }
            let decision = if self.actors[id].is_enemy() {
                ai::enemy_decision(self, id)
            } else {
                ai::party_decision(self, id)
            };
            self.apply_decision(id, decision);
        }
        if let Some(p) = self.player
            && let Some(t) = self.actors[p].target
            && self.actors[t].is_engaged_enemy()
            && self.on_threat_table(t, p)
        {
            self.actors[p].auto_attack = true;
        }
    }

    fn apply_decision(&mut self, id: ActorId, d: ai::Decision) {
        if d.stop_cast
            && let Some(c) = self.actors[id].cast.take()
        {
            self.emit(EventKind::Interrupted {
                src: id,
                what: c.name,
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
                // A non-tank pulled off the tank while a tank is alive: threat failure.
                if !self.is_tank(nt)
                    && self.tank().is_some()
                    && self.actors[id].target.is_some()
                    && self.actors[id].taunted_by().is_none()
                {
                    self.metrics.threat_failures += 1;
                }
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
        let (Some(p), Some((slot, explicit))) = (self.player, self.pending_player_cast.take())
        else {
            return;
        };
        let target = self.player_cast_target(slot, explicit);
        if let Err(reason) = self.try_cast(p, slot, target) {
            let what = self.actors[p]
                .ability_labels
                .get(slot)
                .cloned()
                .unwrap_or_else(|| Label::from("?"));
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

    /// Check whether `caster` could use ability `slot` on `target` right now,
    /// without changing anything. Returns the resolved target. `try_cast` uses
    /// exactly this check, so UI previews can never disagree with the rules.
    pub fn validate_cast(
        &self,
        caster: ActorId,
        slot: usize,
        target: Option<ActorId>,
    ) -> Result<ActorId, CastError> {
        let a = &self.actors[caster];
        if !a.alive {
            return Err(CastError::Dead);
        }
        if a.is_incapacitated() {
            return Err(CastError::Incapacitated);
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
        if a.resource < def.cost {
            return Err(CastError::NotEnoughResource);
        }
        if a.is_silenced() && def.school.is_magic() {
            return Err(CastError::Silenced);
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
        let t = &self.actors[target];
        match def.effect {
            Effect::Incapacitate { .. } if t.enemy.as_ref().is_some_and(|e| e.cc_immune) => {
                return Err(CastError::Immune);
            }
            Effect::Dispel if !t.has_dispellable_debuff() => {
                return Err(CastError::NothingToDispel);
            }
            Effect::Absorb { .. } if t.has_aura(|k| matches!(k, AuraKind::WeakenedSoul)) => {
                return Err(CastError::TargetWeakened);
            }
            _ => {}
        }
        if def.target != TargetRule::SelfOnly && !self.reach(caster, target, def.range) {
            return Err(CastError::OutOfRange);
        }
        if def.target != TargetRule::SelfOnly && !self.has_los(caster, target) {
            return Err(CastError::NoLineOfSight);
        }
        if def.cast_time > 0.0
            && self.actors[caster].controller == Controller::Player
            && self.actors[caster].move_dir.len_sq() > 0.0
        {
            return Err(CastError::Moving);
        }
        Ok(target)
    }

    /// Attempt to use ability `slot`. Validates state, resources, target and range.
    pub fn try_cast(
        &mut self,
        caster: ActorId,
        slot: usize,
        target: Option<ActorId>,
    ) -> Result<(), CastError> {
        let target = self.validate_cast(caster, slot, target)?;
        let def = self.actors[caster].abilities[slot].def();
        if def.cast_time > 0.0 {
            let name = self.label(caster, slot);
            let a = &mut self.actors[caster];
            a.cast = Some(Cast {
                kind: CastKind::Ability(slot),
                name: name.clone(),
                target,
                remaining: def.cast_time,
                total: def.cast_time,
                interruptible: true,
            });
            a.gcd = GCD;
            a.move_goal = None;
            self.emit(EventKind::CastStart {
                src: caster,
                what: name,
                target,
            });
        } else {
            self.actors[caster].gcd = GCD;
            self.execute_ability(caster, slot, target);
        }
        Ok(())
    }

    fn spend(&mut self, id: ActorId, cost: f32) {
        let a = &mut self.actors[id];
        if cost > 0.0 {
            a.resource = (a.resource - cost).max(0.0);
            if a.resource_kind == ResourceKind::Mana {
                a.last_mana_spend = self.time;
            }
            self.metrics.actors[id].resource_spent += cost;
        }
    }

    fn execute_ability(&mut self, caster: ActorId, slot: usize, target: ActorId) {
        let def = self.actors[caster].abilities[slot].def();
        let what = self.label(caster, slot);
        self.spend(caster, def.cost);
        self.actors[caster].cooldowns[slot] = def.cooldown;
        self.metrics.actors[caster].casts += 1;
        match def.effect {
            Effect::Damage {
                amount,
                threat_mult,
                kind,
            } => {
                let mut spec = DamageSpec::new(amount, def.school, kind, what);
                spec.threat_mult = threat_mult;
                self.deal(caster, target, spec);
            }
            Effect::AoeDamage {
                amount,
                radius,
                threat_mult,
            } => {
                let center = self.actors[caster].loc();
                let hits: Vec<ActorId> = self
                    .engaged_enemies()
                    .into_iter()
                    .filter(|&e| {
                        !self.actors[e].is_incapacitated() && self.in_effect(e, center, radius)
                    })
                    .collect();
                for e in hits {
                    let mut spec =
                        DamageSpec::new(amount, def.school, HitKind::Melee, what.clone());
                    spec.threat_mult = threat_mult;
                    self.deal(caster, e, spec);
                }
            }
            Effect::Heal { amount } => {
                self.heal(caster, target, amount, what, true);
            }
            Effect::Hot { total, duration } => {
                let ticks = duration.max(1.0);
                let mut aura = Aura::new(
                    what,
                    AuraKind::Hot {
                        per_tick: total / ticks,
                    },
                    caster,
                    duration,
                    false,
                );
                aura.dispellable = false;
                self.apply_aura(target, aura);
            }
            Effect::Dot {
                total,
                duration,
                ignores_armor,
            } => {
                let ticks = duration.max(1.0);
                let aura = Aura::new(
                    what,
                    AuraKind::Dot {
                        per_tick: total / ticks,
                        school: def.school,
                        ignores_armor,
                    },
                    caster,
                    duration,
                    true,
                );
                self.engage_enemy_of(caster, target);
                self.apply_aura(target, aura);
            }
            Effect::Absorb {
                amount,
                duration,
                lockout,
            } => {
                let shield = (amount * self.actors[caster].power_mult).round();
                self.apply_aura(
                    target,
                    Aura::new(
                        what,
                        AuraKind::Absorb { remaining: shield },
                        caster,
                        duration,
                        false,
                    ),
                );
                self.apply_aura(
                    target,
                    Aura::new(
                        Label::from("Weakened Soul"),
                        AuraKind::WeakenedSoul,
                        caster,
                        lockout,
                        true,
                    ),
                );
            }
            Effect::Taunt { duration } => {
                self.taunt(caster, target, duration);
            }
            Effect::DamageReduction { pct, duration } => {
                self.apply_aura(
                    target,
                    Aura::new(
                        what,
                        AuraKind::DamageReduction { pct },
                        caster,
                        duration,
                        false,
                    ),
                );
            }
            Effect::ThreatDrop { pct } => {
                for i in 0..self.enemies.len() {
                    let e = self.enemies[i];
                    if let Some(es) = self.actors[e].enemy.as_mut() {
                        for t in es.threat.iter_mut().filter(|t| t.actor == caster) {
                            t.value *= 1.0 - pct;
                        }
                    }
                }
                self.emit(EventKind::ThreatDrop { src: caster });
            }
            Effect::Interrupt { lockout, damage } => {
                self.metrics.actors[caster].interrupts_attempted += 1;
                if damage > 0.0 {
                    let mut spec =
                        DamageSpec::new(damage, def.school, HitKind::Melee, what.clone());
                    spec.threat_mult = 2.0;
                    self.deal(caster, target, spec);
                } else {
                    self.engage_enemy_of(caster, target);
                }
                let interruptible = self.actors[target]
                    .cast
                    .as_ref()
                    .is_some_and(|c| c.interruptible);
                if interruptible && self.actors[target].alive {
                    let c = self.actors[target].cast.take().expect("casting");
                    self.metrics.actors[caster].interrupts += 1;
                    self.metrics.interrupted_casts += 1;
                    self.emit(EventKind::Interrupt {
                        src: caster,
                        dst: target,
                        what: c.name,
                    });
                    self.apply_aura(
                        target,
                        Aura::new(
                            Label::from("Silenced"),
                            AuraKind::Silenced,
                            caster,
                            lockout,
                            true,
                        ),
                    );
                }
            }
            Effect::Incapacitate { duration } => {
                // One incapacitate per caster: break the previous one.
                for i in 0..self.enemies.len() {
                    let e = self.enemies[i];
                    self.actors[e].auras.retain(|a| {
                        !(a.source == caster && matches!(a.kind, AuraKind::Incapacitated))
                    });
                }
                self.engage_enemy_of(caster, target);
                let t = &mut self.actors[target];
                t.cast = None;
                t.move_goal = None;
                self.metrics.cc_applied += 1;
                self.apply_aura(
                    target,
                    Aura::new(what, AuraKind::Incapacitated, caster, duration, true),
                );
            }
            Effect::Dispel => {
                if let Some(i) = self.actors[target]
                    .auras
                    .iter()
                    .position(|a| a.harmful && a.dispellable)
                {
                    let aura = self.actors[target].auras.remove(i);
                    self.metrics.dispels += 1;
                    self.emit(EventKind::Dispel {
                        src: caster,
                        dst: target,
                        what: aura.name,
                    });
                }
            }
        }
    }

    /// Put `target`'s pack in combat with `src` (for effects that don't deal damage).
    fn engage_enemy_of(&mut self, src: ActorId, target: ActorId) {
        if let Some(enc) = self.actors[target].enemy.as_ref().map(|e| e.encounter)
            && self.actors[src].team == Team::Party
        {
            self.engage(enc, src);
            self.add_threat(target, src, 1.0);
        }
    }

    /// Apply or refresh an aura. Same-name auras from the same source stack up to
    /// `max_stacks` and refresh their duration.
    pub fn apply_aura(&mut self, dst: ActorId, aura: Aura) {
        if !self.actors[dst].alive {
            return;
        }
        let stacks = {
            let auras = &mut self.actors[dst].auras;
            if let Some(existing) = auras
                .iter_mut()
                .find(|a| a.name == aura.name && a.source == aura.source)
            {
                existing.stacks = (existing.stacks + 1).min(existing.max_stacks.max(1));
                existing.remaining = aura.duration;
                existing.kind = match (existing.kind, aura.kind) {
                    // Refreshing a shield replaces its remaining amount.
                    (AuraKind::Absorb { .. }, k) => k,
                    (k, _) => k,
                };
                existing.stacks
            } else {
                auras.push(aura.clone());
                1
            }
        };
        self.emit(EventKind::AuraApplied {
            src: aura.source,
            dst,
            what: aura.name,
            stacks,
        });
    }

    /// Taunt: raise the taunter's threat to the current top value and force the
    /// enemy to attack them for `duration` seconds.
    pub fn taunt(&mut self, caster: ActorId, enemy: ActorId, duration: f32) {
        let enc = match &self.actors[enemy].enemy {
            Some(e) => e.encounter,
            None => return,
        };
        self.engage(enc, caster);
        let top = self.top_threat(enemy);
        let mine = self.threat(enemy, caster);
        if top > mine {
            self.add_threat(enemy, caster, top - mine);
        }
        let a = &mut self.actors[enemy];
        a.auras
            .retain(|x| !matches!(x.kind, AuraKind::Taunted { .. }));
        a.auras.push(Aura::new(
            Label::from("Taunted"),
            AuraKind::Taunted { by: caster },
            caster,
            duration,
            true,
        ));
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

    fn reset_ability_timers(es: &mut EnemyState) {
        for a in es.abilities.iter_mut() {
            a.timer = a.cfg.initial.unwrap_or(a.cfg.interval);
        }
    }

    /// Put an encounter (pack) into combat. `trigger` is placed on every mob's
    /// threat table first; other living party members nearby join at zero threat.
    pub fn engage(&mut self, enc: usize, trigger: ActorId) {
        if self.encounters[enc].cleared || !self.actors[trigger].alive {
            return;
        }
        let mut newly = false;
        for k in 0..self.encounters[enc].mobs.len() {
            let m = self.encounters[enc].mobs[k];
            if !self.actors[m].alive {
                continue;
            }
            let mode = self.actors[m].mode();
            if mode == Some(EnemyMode::Idle) {
                newly = true;
                let mob_pos = self.actors[m].loc();
                let nearby: Vec<ActorId> = self
                    .alive_party()
                    .filter(|&p| {
                        p != trigger && self.actors[p].loc().dist(mob_pos) <= PULL_SHARE_RADIUS
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
                es.pull_time = now;
                Self::reset_ability_timers(es);
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

    /// Resolve and apply one hit. Returns the HP actually removed.
    pub fn deal(&mut self, src: ActorId, dst: ActorId, spec: DamageSpec) -> f32 {
        if !self.actors[dst].alive {
            return 0.0;
        }
        if self.actors[dst].mode() == Some(EnemyMode::Resetting) {
            self.emit(EventKind::Evade { enemy: dst });
            return 0.0;
        }
        // Hostile actions put the target's pack in combat even when they miss.
        let src_is_party = self.actors[src].team == Team::Party;
        let enemy_enc = self.actors[dst].enemy.as_ref().map(|e| e.encounter);
        if let Some(enc) = enemy_enc
            && src_is_party
        {
            self.engage(enc, src);
        }
        let now = self.time;
        self.actors[dst].last_combat = now;
        self.actors[src].last_combat = now;

        let att = self.actors[src].stats;
        let mut def = self.actors[dst].stats;
        if self.actors[dst].is_incapacitated() {
            def.dodge = 0.0;
            def.parry = 0.0;
            def.block = 0.0;
        }
        let outcome = match spec.kind {
            HitKind::Melee => {
                let front = stats::is_in_front(
                    self.actors[dst].pos,
                    self.actors[dst].facing,
                    self.actors[src].pos,
                );
                let roll = self.rng.next_f32();
                stats::melee_table(&att, &def, front, true, roll)
            }
            HitKind::Spell => {
                let (a, b) = (self.rng.next_f32(), self.rng.next_f32());
                stats::spell_table(&att, &def, true, a, b)
            }
            HitKind::Periodic | HitKind::Unavoidable => HitOutcome::Hit,
        };
        if outcome.avoided() {
            self.emit(EventKind::Damage {
                src,
                dst,
                what: spec.what,
                amount: 0.0,
                absorbed: 0.0,
                outcome,
                school: spec.school,
            });
            return 0.0;
        }
        let variance = match spec.kind {
            HitKind::Periodic => 1.0,
            _ => self.rng.range_f32(0.9, 1.1),
        };
        let mut amount = spec.base * self.actors[src].power_mult * variance;
        if outcome == HitOutcome::Crit {
            amount *= if spec.kind == HitKind::Melee {
                MELEE_CRIT_MULT
            } else {
                SPELL_CRIT_MULT
            };
        }
        if spec.school == School::Physical {
            if !spec.ignores_armor {
                amount *= 1.0 - stats::armor_reduction(def.armor, att.level);
            }
        } else {
            amount *= 1.0 - stats::resist_reduction(def.resist, att.level);
        }
        if outcome == HitOutcome::Block {
            amount = (amount - def.block_value).max(0.0);
        }
        let d = &self.actors[dst];
        amount *= (1.0 - d.damage_reduction()) * d.damage_taken_mult();
        let mut amount = amount.round().max(0.0);
        // Absorb shields soak damage first.
        let mut absorbed = 0.0;
        let mut absorbed_by = Vec::new();
        for aura in self.actors[dst].auras.iter_mut() {
            if amount <= 0.0 {
                break;
            }
            if let AuraKind::Absorb { remaining } = &mut aura.kind {
                let soak = remaining.min(amount);
                *remaining -= soak;
                amount -= soak;
                absorbed += soak;
                absorbed_by.push((aura.source, soak));
            }
        }
        self.actors[dst]
            .auras
            .retain(|a| !matches!(a.kind, AuraKind::Absorb { remaining } if remaining <= 0.0));
        for (s, v) in absorbed_by {
            self.metrics.actors[s].absorb_done += v;
        }
        let outcome = if amount <= 0.0 && absorbed > 0.0 {
            HitOutcome::Absorb
        } else {
            outcome
        };
        let applied = amount.min(self.actors[dst].hp);
        self.actors[dst].hp = (self.actors[dst].hp - amount).max(0.0);
        self.metrics.actors[src].damage_done += applied;
        self.metrics.actors[dst].damage_taken += applied;
        if spec.avoidable && self.actors[dst].team == Team::Party {
            self.metrics.actors[dst].avoidable_taken += applied + absorbed;
            // Getting hit by a mechanic makes this character more alert for the run.
            let a = &mut self.actors[dst];
            a.alertness = (a.alertness + ALERTNESS_PER_HIT).min(MAX_ALERTNESS);
        }
        // Rage: white melee swings dealt, and any damage taken.
        let lvl_src = att.level;
        let lvl_dst = def.level;
        if spec.auto_attack && self.actors[src].resource_kind == ResourceKind::Rage {
            let r = stats::rage_from_dealt(applied, lvl_src);
            let a = &mut self.actors[src];
            a.resource = (a.resource + r).min(a.max_resource);
        }
        if self.actors[dst].resource_kind == ResourceKind::Rage && self.actors[dst].class.is_some()
        {
            let r = stats::rage_from_taken(applied + absorbed, lvl_dst);
            let a = &mut self.actors[dst];
            a.resource = (a.resource + r).min(a.max_resource);
        }
        // Any damage breaks incapacitation.
        if (applied > 0.0 || absorbed > 0.0)
            && let Some(i) = self.actors[dst]
                .auras
                .iter()
                .position(|a| matches!(a.kind, AuraKind::Incapacitated))
        {
            let a = self.actors[dst].auras.remove(i);
            self.metrics.cc_broken += 1;
            self.emit(EventKind::CcBroken { dst, what: a.name });
        }
        self.emit(EventKind::Damage {
            src,
            dst,
            what: spec.what,
            amount: applied,
            absorbed,
            outcome,
            school: spec.school,
        });
        if enemy_enc.is_some() && src_is_party {
            let threat = applied * spec.threat_mult * self.actors[src].threat_mult;
            self.add_threat(dst, src, threat);
        }
        if self.actors[dst].hp <= 0.0 {
            self.kill(dst);
        }
        applied
    }

    /// Heal `dst`. Effective healing generates threat on every engaged enemy:
    /// `HEAL_THREAT_FACTOR * effective * healer.threat_mult`, split evenly.
    pub fn heal(
        &mut self,
        src: ActorId,
        dst: ActorId,
        base: f32,
        what: Label,
        can_crit: bool,
    ) -> f32 {
        if !self.actors[dst].alive || !self.actors[src].alive {
            return 0.0;
        }
        let crit = can_crit && self.rng.chance(self.actors[src].stats.crit);
        let mut amount = base * self.actors[src].power_mult * self.actors[dst].healing_taken_mult();
        if crit {
            amount *= SPELL_CRIT_MULT;
        }
        let amount = amount.round();
        let d = &mut self.actors[dst];
        let effective = amount.min(d.max_hp - d.hp).max(0.0);
        let overheal = amount - effective;
        d.hp += effective;
        let m = &mut self.metrics.actors[src];
        m.healing_done += effective;
        m.overhealing += overheal;
        self.emit(EventKind::Heal {
            src,
            dst,
            what,
            amount: effective,
            overheal,
            crit,
        });
        if effective > 0.0 && self.in_combat(dst) && self.actors[src].team == Team::Party {
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
        self.metrics.actors[id].deaths += 1;
        self.emit(EventKind::Death { actor: id });
        self.forget(id);
    }

    /// Remove `id` from every threat table and target.
    fn forget(&mut self, id: ActorId) {
        for i in 0..self.enemies.len() {
            let e = self.enemies[i];
            if let Some(es) = self.actors[e].enemy.as_mut() {
                es.threat.retain(|t| t.actor != id);
            }
            if self.actors[e].target == Some(id) {
                self.actors[e].target = None;
            }
            self.actors[e]
                .auras
                .retain(|a| !matches!(a.kind, AuraKind::Taunted { by } if by == id));
        }
        for k in 0..self.party.len() {
            let p = self.party[k];
            if self.actors[p].target == Some(id) {
                self.actors[p].target = None;
                self.actors[p].auto_attack = false;
            }
        }
        self.telegraphs.retain(|t| t.source != id);
    }

    /// Remove a summoned enemy from the world without counting it as a kill.
    fn despawn(&mut self, id: ActorId) {
        if !self.actors[id].alive {
            return;
        }
        let a = &mut self.actors[id];
        a.alive = false;
        a.cast = None;
        a.auras.clear();
        if let Some(es) = a.enemy.as_mut() {
            es.despawned = true;
        }
        self.forget(id);
    }

    fn move_actors(&mut self) {
        for id in 0..self.actors.len() {
            self.actors[id].moved_last_tick = false;
            if !self.actors[id].alive {
                continue;
            }
            self.settle_height(id);
            let a = &self.actors[id];
            if a.is_incapacitated() {
                continue;
            }
            let resetting = a.mode() == Some(EnemyMode::Resetting);
            let speed = a.speed * if resetting { 1.5 } else { 1.0 };
            let step = if a.controller == Controller::Player {
                a.move_dir.normalize_or_zero() * speed * DT
            } else if let Some(goal) = a.move_goal {
                self.steer(id, goal, speed)
            } else {
                self.actors[id].nav.unreachable = false;
                self.actors[id].nav.stalled = 0.0;
                V2::ZERO
            };
            if step.len_sq() > 0.0 {
                if self.actors[id].cast.is_some() {
                    if self.actors[id].controller == Controller::Player {
                        let c = self.actors[id].cast.take().expect("casting");
                        self.emit(EventKind::Interrupted {
                            src: id,
                            what: c.name,
                        });
                    } else {
                        continue;
                    }
                }
                let a = &self.actors[id];
                let (new_pos, new_surface, _dropped) = self.nav.try_move(a.pos, a.surface, step);
                let moved = new_pos.dist(a.pos);
                let a = &mut self.actors[id];
                // Stuck detection: trying to move but (almost) not moving.
                if a.controller != Controller::Player {
                    if moved < step.len() * 0.2 {
                        a.nav.stalled += DT;
                    } else {
                        a.nav.stalled = (a.nav.stalled - DT).max(0.0);
                    }
                }
                if new_pos != a.pos || new_surface != a.surface {
                    a.moved_last_tick = true;
                    if a.controller != Controller::Player && moved > 1e-4 {
                        a.facing = (new_pos - a.pos).heading();
                    }
                    a.pos = new_pos;
                    a.surface = new_surface;
                }
                self.settle_height(id);
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

    /// Follow the floor height: ramps exactly, drops by falling at `FALL_SPEED`.
    fn settle_height(&mut self, id: ActorId) {
        let ground = self
            .nav
            .height(self.actors[id].surface, self.actors[id].pos);
        let a = &mut self.actors[id];
        a.y = if a.y > ground + 0.05 {
            (a.y - FALL_SPEED * DT).max(ground)
        } else {
            ground
        };
    }

    /// One tick of AI steering toward `goal`: route, unreachable fallback and
    /// stuck recovery. Returns the step to take.
    /// Is point `p` (with clearance `r`) inside a ring or a live/imminent hazard?
    fn ground_danger(&self, p: V3, r: f32) -> bool {
        let hit = |c: V3, radius: f32| {
            p.xz().dist(c.xz()) <= radius + r && (p.y - c.y).abs() <= EFFECT_HEIGHT
        };
        self.telegraphs
            .iter()
            .any(|t| hit(V3::at(t.center, t.y), t.radius))
            || self.hazards.iter().any(|h| {
                !self.encounters[h.encounter].cleared
                    && (h.active || h.time_to_active(self.time) <= 1.0)
                    && hit(h.pos, h.cfg.radius)
            })
    }

    fn steer(&mut self, id: ActorId, goal: V3, speed: f32) -> V2 {
        // Commitment never overrides dodging.
        let danger = {
            let a = &self.actors[id];
            self.ground_danger(a.loc(), a.radius + 0.5)
                || (a.nav.commit > 0.0 && self.ground_danger(a.nav.commit_goal, a.radius + 0.5))
        };
        let goal = {
            let n = &mut self.actors[id].nav;
            if danger {
                n.commit = 0.0;
                n.dither = 0.0;
            }
            if n.commit > 0.0 {
                n.commit -= DT;
                n.commit_goal
            } else {
                goal
            }
        };
        let a = &self.actors[id];
        let mut wp = self.nav.next_waypoint(a.pos, a.surface, goal);
        let mut unreachable = !wp.reachable;
        if unreachable && !a.is_enemy() && id != self.leader {
            // Can't get there from here: stay with the leader instead.
            let l = &self.actors[self.leader];
            if l.alive {
                let w = self.nav.next_waypoint(a.pos, a.surface, l.loc());
                if w.reachable && a.pos.dist(l.pos) > 3.0 {
                    wp = w;
                }
            }
        }
        if !wp.reachable {
            wp.point = a.pos;
        } else if wp.point == a.pos {
            unreachable = false;
        }
        let a = &mut self.actors[id];
        a.nav.unreachable = unreachable;
        if a.nav.stalled >= STUCK_AFTER && a.nav.recover <= 0.0 {
            // Back off toward the middle of the current surface (or the portal
            // being crossed) and try again; never teleport.
            let surf = &self.nav.surfaces[a.surface as usize];
            a.nav.recover_to = match wp.link {
                Some(l) => self.nav.links[l as usize].portal.center(),
                None => surf.walk.center(),
            };
            a.nav.recover = RECOVER_FOR;
            a.nav.stalled = 0.0;
        }
        if a.nav.stalled > 0.0 || a.nav.recover > 0.0 {
            a.nav.stuck_time += DT;
        }
        let point = if a.nav.recover > 0.0 {
            a.nav.recover -= DT;
            a.nav.recover_to
        } else {
            wp.point
        };
        let d = point - a.pos;
        if d.len() < 0.05 {
            if a.nav.recover > 0.0 {
                a.nav.recover = 0.0;
            }
            return V2::ZERO;
        }
        let dir = d.normalize_or_zero();
        // Dither detection: turning back on ourselves twice within ~0.7 s.
        let n = &mut a.nav;
        n.dither = (n.dither - DT * 1.5).max(0.0);
        if dir.x * n.last_step.x + dir.z * n.last_step.z < -0.6 {
            n.dither += 1.0;
        }
        if n.dither >= 2.0 && n.commit <= 0.0 && n.recover <= 0.0 && !danger {
            n.commit = COMMIT_FOR;
            n.commit_goal = n.prev_goal.unwrap_or(goal);
            n.dither = 0.0;
            n.dithers += 1;
        }
        n.last_step = dir;
        n.prev_goal = Some(goal);
        dir * (speed * DT).min(d.len())
    }

    fn check_aggro(&mut self) {
        for i in 0..self.enemies.len() {
            let e = self.enemies[i];
            let a = &self.actors[e];
            if !a.alive || a.mode() != Some(EnemyMode::Idle) {
                continue;
            }
            let es = a.enemy.as_ref().expect("enemy");
            let (radius, enc, pos) = (es.aggro_radius, es.encounter, a.loc());
            // Noticed within the radius on its own level, or across levels only
            // when it can actually see you (no aggro through floors).
            let trigger = self.alive_party().find(|&p| {
                let q = &self.actors[p];
                q.loc().dist(pos) <= radius + q.radius
                    && ((q.y - pos.y).abs() <= BODY_ALLOWANCE || self.has_los(e, p))
            });
            if let Some(p) = trigger {
                self.engage(enc, p);
            }
        }
    }

    fn update_auras(&mut self) {
        let mut hot_ticks = Vec::new();
        let mut dot_ticks = Vec::new();
        for id in 0..self.actors.len() {
            if !self.actors[id].alive {
                continue;
            }
            let auras = &mut self.actors[id].auras;
            for aura in auras.iter_mut() {
                aura.remaining -= DT;
                match aura.kind {
                    AuraKind::Hot { per_tick } | AuraKind::Dot { per_tick, .. } => {
                        aura.tick_timer -= DT;
                        if aura.tick_timer <= 1e-4 {
                            aura.tick_timer += 1.0;
                            let amount = per_tick * aura.stacks as f32;
                            if matches!(aura.kind, AuraKind::Hot { .. }) {
                                hot_ticks.push((aura.source, id, amount, aura.name.clone()));
                            } else if let AuraKind::Dot {
                                school,
                                ignores_armor,
                                ..
                            } = aura.kind
                            {
                                dot_ticks.push((
                                    aura.source,
                                    id,
                                    amount,
                                    aura.name.clone(),
                                    school,
                                    ignores_armor,
                                ));
                            }
                        }
                    }
                    _ => {}
                }
            }
            auras.retain(|a| a.remaining > 1e-4);
        }
        for (src, dst, amount, name) in hot_ticks {
            self.heal(src, dst, amount, name, false);
        }
        for (src, dst, amount, name, school, ignores_armor) in dot_ticks {
            let mut spec = DamageSpec::new(amount, school, HitKind::Periodic, name);
            spec.ignores_armor = ignores_armor;
            self.deal(src, dst, spec);
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
            let Some(cast) = self.actors[id].cast.as_mut() else {
                continue;
            };
            cast.remaining -= DT;
            if cast.remaining > 1e-4 {
                continue;
            }
            let cast = self.actors[id].cast.take().expect("casting");
            let t = cast.target;
            match cast.kind {
                CastKind::Ability(slot) => {
                    let def = self.actors[id].abilities[slot].def();
                    let valid =
                        self.actors[t].alive && self.actors[t].mode() != Some(EnemyMode::Resetting);
                    let reason = if !valid {
                        Some(CastError::InvalidTarget)
                    } else if !self.reach(id, t, def.range + 3.0) {
                        Some(CastError::OutOfRange)
                    } else if self.actors[id].resource < def.cost {
                        Some(CastError::NotEnoughResource)
                    } else {
                        None
                    };
                    match reason {
                        Some(reason) => self.emit(EventKind::CastFailed {
                            src: id,
                            what: cast.name,
                            reason,
                        }),
                        None => self.execute_ability(id, slot, t),
                    }
                }
                CastKind::Enemy(idx) => {
                    if cast.interruptible {
                        self.metrics.completed_interruptible += 1;
                    }
                    self.enemy_ability_effect(id, idx, t);
                }
            }
        }
    }

    fn auto_attacks(&mut self) {
        for id in 0..self.actors.len() {
            let a = &self.actors[id];
            if !a.alive
                || a.cast.is_some()
                || !a.auto_attack
                || a.swing_timer > 0.0
                || a.is_incapacitated()
            {
                continue;
            }
            if a.is_enemy() && a.mode() != Some(EnemyMode::Engaged) {
                continue;
            }
            let Some(t) = a.target else { continue };
            if !self.actors[t].alive || !self.is_hostile(id, t) {
                continue;
            }
            if self.actors[t].mode() == Some(EnemyMode::Resetting)
                || self.actors[t].is_incapacitated()
            {
                continue;
            }
            if !self.reach(id, t, a.attack_range) || !self.has_los(id, t) {
                continue;
            }
            let what = if a.attack_range > 5.0 {
                self.label_wand.clone()
            } else {
                self.label_melee.clone()
            };
            let mut spec = DamageSpec::new(a.attack_damage, a.attack_school, a.attack_kind, what);
            spec.auto_attack = true;
            self.actors[id].swing_timer = a.attack_interval;
            self.deal(id, t, spec);
        }
    }

    fn enemy_specials(&mut self) {
        for i in 0..self.enemies.len() {
            let e = self.enemies[i];
            if !self.actors[e].is_engaged_enemy() {
                continue;
            }
            self.check_phase(e);
            self.check_enrage(e);
            // Ability timers keep running while the enemy casts or is controlled;
            // abilities only fire when it is free to act.
            let busy = self.actors[e].cast.is_some()
                || self.actors[e].is_incapacitated()
                || self.actors[e].is_silenced();
            let phase = self.actors[e].phase();
            let n = self.actors[e]
                .enemy
                .as_ref()
                .map_or(0, |s| s.abilities.len());
            for idx in 0..n {
                let ready = {
                    let es = self.actors[e].enemy.as_mut().expect("enemy");
                    let ab = &mut es.abilities[idx];
                    if !ab.cfg.phases.is_empty() && !ab.cfg.phases.contains(&phase) {
                        continue;
                    }
                    ab.timer -= DT;
                    ab.timer <= 0.0
                };
                if !ready || busy || self.actors[e].cast.is_some() {
                    continue;
                }
                let cfg = self.actors[e].enemy.as_ref().expect("enemy").abilities[idx]
                    .cfg
                    .clone();
                let Some(target) = self.pick_target(e, &cfg) else {
                    // Nothing valid to do: retry soon.
                    self.actors[e].enemy.as_mut().expect("enemy").abilities[idx].timer = 1.0;
                    continue;
                };
                self.actors[e].enemy.as_mut().expect("enemy").abilities[idx].timer = cfg.interval;
                let label = self.actors[e].enemy.as_ref().expect("enemy").abilities[idx]
                    .label
                    .clone();
                if cfg.cast_time > 0.0 {
                    self.actors[e].cast = Some(Cast {
                        kind: CastKind::Enemy(idx),
                        name: label.clone(),
                        target,
                        remaining: cfg.cast_time,
                        total: cfg.cast_time,
                        interruptible: cfg.interruptible,
                    });
                    if cfg.interruptible {
                        self.metrics.interruptible_casts += 1;
                    }
                    self.emit(EventKind::CastStart {
                        src: e,
                        what: label,
                        target,
                    });
                } else {
                    self.enemy_ability_effect(e, idx, target);
                }
            }
        }
    }

    fn check_phase(&mut self, e: ActorId) {
        loop {
            let (phase, next) = {
                let es = self.actors[e].enemy.as_ref().expect("enemy");
                (es.phase, es.phases.get(es.phase as usize - 1).cloned())
            };
            let Some(next) = next else { return };
            if self.actors[e].hp_frac() > next.hp_below {
                return;
            }
            let new_phase = phase + 1;
            {
                let a = &mut self.actors[e];
                a.power_mult *= next.damage_multiplier;
                let es = a.enemy.as_mut().expect("enemy");
                es.phase = new_phase;
                // Abilities that start in this phase get fresh timers.
                for ab in es.abilities.iter_mut() {
                    if ab.cfg.phases.contains(&new_phase) && !ab.cfg.phases.contains(&phase) {
                        ab.timer = ab.cfg.initial.unwrap_or(ab.cfg.interval);
                    }
                }
            }
            if e == self.boss && new_phase == 2 && self.phase2_at.is_none() {
                self.phase2_at = Some(self.time);
            }
            self.emit(EventKind::PhaseChange {
                boss: e,
                phase: new_phase,
                announce: Label::from(next.announce.as_str()),
            });
            for s in &next.summon {
                self.summon(e, &s.template, s.count, Label::from("Summon"));
            }
        }
    }

    fn check_enrage(&mut self, e: ActorId) {
        let (limit, enraged, pull) = {
            let es = self.actors[e].enemy.as_ref().expect("enemy");
            (es.enrage_seconds, es.enraged, es.pull_time)
        };
        if let Some(limit) = limit
            && !enraged
            && self.time - pull >= limit
        {
            let a = &mut self.actors[e];
            a.power_mult *= ENRAGE_MULT;
            a.enemy.as_mut().expect("enemy").enraged = true;
            self.emit(EventKind::Enrage { boss: e });
        }
    }

    fn pick_target(&mut self, e: ActorId, cfg: &EnemyAbility) -> Option<ActorId> {
        if cfg.kind == AbilityKind::HealAlly {
            let enc = self.actors[e].enemy.as_ref()?.encounter;
            return self.encounters[enc]
                .mobs
                .iter()
                .copied()
                .filter(|&m| self.actors[m].alive && self.actors[m].hp_frac() < 0.8)
                .min_by(|&a, &b| {
                    self.actors[a]
                        .hp_frac()
                        .total_cmp(&self.actors[b].hp_frac())
                        .then(a.cmp(&b))
                });
        }
        let on_table: Vec<ActorId> = self
            .alive_party()
            .filter(|&p| self.on_threat_table(e, p))
            .collect();
        let pool = if on_table.is_empty() {
            self.alive_party().collect()
        } else {
            on_table
        };
        if pool.is_empty() {
            return None;
        }
        let current = self.actors[e].target.filter(|t| self.actors[*t].alive);
        let pos = self.actors[e].pos;
        Some(match cfg.target {
            TargetSelector::Caster => e,
            TargetSelector::Current => current.unwrap_or(pool[0]),
            TargetSelector::Random => pool[self.rng.index(pool.len())],
            TargetSelector::RandomNonTank => {
                let non: Vec<ActorId> = pool
                    .iter()
                    .copied()
                    .filter(|&p| Some(p) != current && !self.is_tank(p))
                    .collect();
                if non.is_empty() {
                    return None;
                }
                non[self.rng.index(non.len())]
            }
            TargetSelector::Farthest => *pool
                .iter()
                .max_by(|&&a, &&b| {
                    self.actors[a]
                        .pos
                        .dist(pos)
                        .total_cmp(&self.actors[b].pos.dist(pos))
                })
                .expect("non-empty"),
            TargetSelector::LowestHealth => *pool
                .iter()
                .min_by(|&&a, &&b| {
                    self.actors[a]
                        .hp_frac()
                        .total_cmp(&self.actors[b].hp_frac())
                })
                .expect("non-empty"),
        })
    }

    fn enemy_ability_effect(&mut self, e: ActorId, idx: usize, target: ActorId) {
        if !self.actors[e].alive {
            return;
        }
        let (cfg, label) = {
            let ab = &self.actors[e].enemy.as_ref().expect("enemy").abilities[idx];
            (ab.cfg.clone(), ab.label.clone())
        };
        match cfg.kind {
            AbilityKind::Bolt => {
                if self.actors[target].alive {
                    let spec = DamageSpec::new(
                        cfg.damage,
                        cfg.school.unwrap_or(School::Shadow),
                        HitKind::Spell,
                        label,
                    );
                    self.deal(e, target, spec);
                }
            }
            AbilityKind::Cleave => {
                let pos = self.actors[e].pos;
                let ey = self.actors[e].y;
                let facing = self.actors[e].facing;
                let r = self.actors[e].radius;
                let hits: Vec<ActorId> = self
                    .alive_party()
                    .filter(|&p| {
                        let a = &self.actors[p];
                        a.pos.dist(pos) <= cfg.radius + a.radius + r
                            && (a.y - ey).abs() <= EFFECT_HEIGHT
                            && (!cfg.frontal || stats::is_in_front(pos, facing, a.pos))
                    })
                    .collect();
                for p in hits {
                    let mut spec = DamageSpec::new(
                        cfg.damage,
                        cfg.school.unwrap_or(School::Physical),
                        HitKind::Unavoidable,
                        label.clone(),
                    );
                    // Standing in front of a frontal cleave is a mistake unless you tank it.
                    spec.avoidable = cfg.frontal && !self.is_tank(p);
                    self.deal(e, p, spec);
                }
            }
            AbilityKind::Ring => {
                let mut picked = vec![target];
                let others: Vec<ActorId> = self.alive_party().filter(|&p| p != target).collect();
                let mut others = others;
                for _ in 1..cfg.count.max(1) {
                    if others.is_empty() {
                        break;
                    }
                    let k = self.rng.index(others.len());
                    picked.push(others.remove(k));
                }
                for t in picked {
                    let id = self.next_telegraph_id;
                    self.next_telegraph_id += 1;
                    let center = self.actors[t].pos;
                    let y = self.actors[t].y;
                    let school = cfg.school.unwrap_or(School::Fire);
                    self.telegraphs.push(Telegraph {
                        id,
                        source: e,
                        what: label.clone(),
                        school,
                        center,
                        y,
                        radius: cfg.radius,
                        remaining: cfg.delay,
                        total: cfg.delay,
                        damage: cfg.damage,
                    });
                    self.emit(EventKind::TelegraphStart {
                        id,
                        source: e,
                        what: label.clone(),
                        center,
                        radius: cfg.radius,
                        delay: cfg.delay,
                    });
                }
            }
            AbilityKind::Pulse => {
                let hits: Vec<ActorId> = self.alive_party().collect();
                for p in hits {
                    let spec = DamageSpec::new(
                        cfg.damage,
                        cfg.school.unwrap_or(School::Shadow),
                        HitKind::Unavoidable,
                        label.clone(),
                    );
                    self.deal(e, p, spec);
                }
            }
            AbilityKind::Summon => {
                let key = cfg.template.clone().unwrap_or_default();
                self.summon(e, &key, cfg.count.max(1), label);
            }
            AbilityKind::Debuff => {
                if let Some(ac) = &cfg.aura
                    && self.actors[target].alive
                {
                    let kind = match ac.effect {
                        DebuffEffect::Dot => AuraKind::Dot {
                            per_tick: ac.value,
                            school: ac.school.unwrap_or(School::Nature),
                            ignores_armor: true,
                        },
                        DebuffEffect::DamageTaken => AuraKind::DamageTaken { pct: ac.value },
                        DebuffEffect::HealingTaken => AuraKind::HealingTaken { pct: ac.value },
                    };
                    let mut aura =
                        Aura::new(Label::from(ac.name.as_str()), kind, e, ac.duration, true);
                    aura.max_stacks = ac.max_stacks;
                    aura.dispellable = ac.dispellable;
                    self.apply_aura(target, aura);
                }
            }
            AbilityKind::HealAlly => {
                if self.actors[target].alive {
                    let amount = cfg.amount;
                    // Enemy heals ignore the party's power scaling.
                    let mult = self.actors[e].power_mult.max(1e-6);
                    self.heal(e, target, amount / mult, label, false);
                }
            }
        }
    }

    /// Spawn `count` engaged adds of `key` around `src`, in `src`'s encounter.
    fn summon(&mut self, src: ActorId, key: &str, count: u32, what: Label) {
        let Some(enc) = self.actors[src].enemy.as_ref().map(|e| e.encounter) else {
            return;
        };
        if !self.content.templates.contains_key(key) {
            return;
        }
        let center = self.actors[src].pos;
        let center_y = self.actors[src].y;
        let threat: Vec<ThreatEntry> = self.actors[src]
            .enemy
            .as_ref()
            .map(|e| {
                e.threat
                    .iter()
                    .map(|t| ThreatEntry {
                        actor: t.actor,
                        value: t.value * SUMMON_THREAT_INHERIT,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let current = self.actors[src].target;
        for i in 0..count {
            let angle = i as f32 * std::f32::consts::TAU / count as f32 + self.time;
            let (pos, _) = self
                .nav
                .project(center + V2::from_heading(angle) * 3.0, center_y);
            let id = self.spawn_enemy(key, pos, enc, true);
            self.encounters[enc].mobs.push(id);
            let now = self.time;
            let a = &mut self.actors[id];
            let es = a.enemy.as_mut().expect("enemy");
            es.mode = EnemyMode::Engaged;
            es.threat = threat.clone();
            es.pull_time = now;
            if let Some(t) = current
                && let Some(entry) = es.threat.iter_mut().find(|x| x.actor == t)
            {
                entry.value = entry.value.max(1.0);
            }
            a.target = current;
            a.swing_timer = 1.0;
        }
        self.emit(EventKind::Summon { src, what, count });
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
            let center = V3::at(t.center, t.y);
            let hits: Vec<ActorId> = self
                .alive_party()
                .filter(|&p| self.in_effect(p, center, t.radius))
                .collect();
            let n = hits.len() as u32;
            for p in hits {
                let mut spec =
                    DamageSpec::new(t.damage, t.school, HitKind::Unavoidable, t.what.clone());
                spec.avoidable = true;
                self.deal(t.source, p, spec);
            }
            self.metrics.ring_hits += n;
            self.emit(EventKind::TelegraphHit { id: t.id, hits: n });
        }
    }

    fn update_hazards(&mut self) {
        for i in 0..self.hazards.len() {
            let enc = self.hazards[i].encounter;
            let cleared = self.encounters[enc].cleared;
            let h = &mut self.hazards[i];
            let period = h.cfg.on + h.cfg.off;
            let phase = (self.time + h.cfg.offset) % period.max(1e-3);
            let was = h.active;
            h.active = !cleared && phase < h.cfg.on;
            if !h.active {
                continue;
            }
            if !was {
                h.tick_timer = 0.0;
            }
            h.tick_timer -= DT;
            if h.tick_timer > 0.0 {
                continue;
            }
            h.tick_timer += 1.0;
            let (pos, radius, damage, school, label) = (
                h.pos,
                h.cfg.radius,
                h.cfg.damage,
                h.cfg.school.unwrap_or(School::Fire),
                h.label.clone(),
            );
            // Hazards are attributed to the first mob of their encounter (for logs).
            let src = self.encounters[enc].mobs[0];
            let hits: Vec<ActorId> = self
                .alive_party()
                .filter(|&p| self.in_effect(p, pos, radius))
                .collect();
            for p in hits {
                let mut spec = DamageSpec::new(damage, school, HitKind::Unavoidable, label.clone());
                spec.avoidable = true;
                // Hazard damage does not scale with the source's phase / enrage.
                spec.base = damage / self.actors[src].power_mult.max(1e-6)
                    * self.content.dungeon.damage_multiplier
                    * self.difficulty.damage;
                self.deal(src, p, spec);
            }
        }
    }

    fn regen(&mut self) {
        for k in 0..self.party.len() {
            let p = self.party[k];
            let in_combat = self.in_combat(p);
            let now = self.time;
            let a = &mut self.actors[p];
            if !a.alive {
                continue;
            }
            let rate = resource_regen_per_sec(a, in_combat, now);
            a.resource = (a.resource + rate * DT).clamp(0.0, a.max_resource);
            if !in_combat {
                a.hp = (a.hp + a.max_hp * OOC_HP_REGEN * DT).min(a.max_hp);
            }
        }
    }

    fn check_resets(&mut self) {
        let leash = self.content.dungeon.leash_radius;
        let mut to_reset = Vec::new();
        for i in 0..self.enemies.len() {
            let e = self.enemies[i];
            let a = &self.actors[e];
            if !a.alive {
                continue;
            }
            let es = a.enemy.as_ref().expect("enemy");
            match es.mode {
                EnemyMode::Engaged => {
                    // Nobody alive that it can reach or at least see: give up.
                    let no_targets = !es.threat.iter().any(|t| self.can_pursue(e, t.actor));
                    let leashed = a.loc().dist(es.home) > leash;
                    if (no_targets || leashed) && !to_reset.contains(&es.encounter) {
                        to_reset.push(es.encounter);
                    }
                }
                EnemyMode::Resetting => {
                    let home = a.pos.dist(es.home.xz()) < 0.3 && (a.y - es.home.y).abs() < 1.0;
                    // A home it can no longer walk back to (it followed someone
                    // down a drop): settle where it stands rather than teleport.
                    if home || a.nav.unreachable {
                        let here = a.loc();
                        let a = &mut self.actors[e];
                        let es = a.enemy.as_mut().expect("enemy");
                        if !home {
                            es.home = here;
                        }
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

    /// Living mobs drop combat, heal to full and walk home. Dead mobs stay dead;
    /// summoned adds vanish.
    pub fn reset_encounter(&mut self, enc: usize) {
        for k in 0..self.encounters[enc].mobs.len() {
            let m = self.encounters[enc].mobs[k];
            if !self.actors[m].alive {
                continue;
            }
            if self.actors[m].enemy.as_ref().is_some_and(|e| e.summoned) {
                self.despawn(m);
                continue;
            }
            let a = &mut self.actors[m];
            a.hp = a.max_hp;
            a.cast = None;
            a.target = None;
            a.auras.clear();
            let es = a.enemy.as_mut().expect("enemy");
            es.mode = EnemyMode::Resetting;
            es.threat.clear();
            es.phase = 1;
            es.enraged = false;
            Self::reset_ability_timers(es);
            a.power_mult = es.base_power_mult;
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
            let done = self.encounters[i].mobs.iter().all(|&m| {
                let a = &self.actors[m];
                !a.alive || a.enemy.as_ref().is_some_and(|e| e.summoned)
            });
            if done {
                self.encounters[i].cleared = true;
                self.encounters[i].cleared_at = Some(self.time);
                // Leftover adds collapse with their master.
                for k in 0..self.encounters[i].mobs.len() {
                    let m = self.encounters[i].mobs[k];
                    if self.actors[m].alive {
                        self.despawn(m);
                    }
                }
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
        } else if self.encounters.iter().all(|e| e.cleared || e.optional) {
            self.outcome = RunOutcome::Victory;
            self.ended_at = Some(self.time);
            self.emit(EventKind::Outcome {
                outcome: RunOutcome::Victory,
            });
        }
    }

    /// Design shortcut instead of resurrection spells: once nobody living has been
    /// in combat for `RALLY_DELAY` seconds and no enemy is engaged, fallen party
    /// members get back up next to a living ally at `RALLY_FRACTION` HP and resource.
    fn rally_fallen(&mut self) {
        if self.outcome != RunOutcome::InProgress || !self.tick.is_multiple_of(10) {
            return;
        }
        let living: Vec<ActorId> = self.alive_party().collect();
        let Some(&anchor) = living
            .iter()
            .find(|&&p| p == self.leader)
            .or(living.first())
        else {
            return;
        };
        if living.len() == self.party.len() {
            return;
        }
        let calm = living
            .iter()
            .all(|&p| self.time - self.actors[p].last_combat >= RALLY_DELAY)
            && self.engaged_enemies().is_empty();
        if !calm {
            return;
        }
        let anchor_pos = self.actors[anchor].pos;
        let anchor_y = self.actors[anchor].y;
        for k in 0..self.party.len() {
            let p = self.party[k];
            if self.actors[p].alive {
                continue;
            }
            let offset = V2::from_heading(p as f32 * 1.3) * 1.5;
            let (pos, surface) = self.nav.project(anchor_pos + offset, anchor_y);
            let a = &mut self.actors[p];
            a.alive = true;
            a.hp = (a.max_hp * RALLY_FRACTION).round();
            a.resource = if a.resource_kind == ResourceKind::Rage {
                0.0
            } else {
                a.max_resource * RALLY_FRACTION
            };
            a.pos = pos.xz();
            a.y = pos.y;
            a.surface = surface;
            a.nav = NavState::default();
            a.cooldowns.iter_mut().for_each(|c| *c = 0.0);
            a.gcd = 0.0;
            a.swing_timer = 0.0;
            self.emit(EventKind::Revive { actor: p });
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
        let mut top: Option<ThreatEntry> = None;
        let mut current: Option<ThreatEntry> = None;
        // Ignore people it can neither reach nor see (e.g. on a ledge it can't climb).
        for t in es.threat.iter().filter(|t| self.can_pursue(e, t.actor)) {
            if top.is_none_or(|b| t.value > b.value) {
                top = Some(*t);
            }
            if a.target == Some(t.actor) {
                current = Some(*t);
            }
        }
        let top = top?;
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

    /// Put an actor at horizontal point `p` on the floor nearest its current
    /// elevation (setup for tests, tools and scripted placement; not movement).
    pub fn set_actor_pos(&mut self, id: ActorId, p: V2) {
        let (q, s) = self.nav.project(p, self.actors[id].y);
        let a = &mut self.actors[id];
        a.pos = q.xz();
        a.y = q.y;
        a.surface = s;
    }

    /// Can enemy `e` do anything about `p`: alive, and reachable on foot or in sight?
    pub fn can_pursue(&self, e: ActorId, p: ActorId) -> bool {
        let (a, b) = (&self.actors[e], &self.actors[p]);
        b.alive && (self.nav.reachable(a.surface, b.surface) || self.has_los(e, p))
    }

    /// Human readable description of an event, for the combat log.
    pub fn describe(&self, e: &Event) -> String {
        crate::events::describe(self, e)
    }
}
