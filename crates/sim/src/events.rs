//! Combat events: the authoritative, inspectable record of everything that happened.

use crate::math::V2;
use crate::sim::{ActorId, RunOutcome, Sim};
use crate::stats::{HitOutcome, School};
use std::fmt;
use std::sync::Arc;

/// Cheap-to-clone ability / effect name.
pub type Label = Arc<str>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastError {
    Dead,
    Busy,
    NoSuchAbility,
    NotReady,
    OnGlobalCooldown,
    NotEnoughResource,
    NoTarget,
    InvalidTarget,
    OutOfRange,
    NoLineOfSight,
    Moving,
    Silenced,
    Incapacitated,
    Immune,
    NothingToDispel,
    TargetWeakened,
    /// Lab A: a finisher with no Edge to spend.
    NoEdge,
    /// Lab B: Exploit on a target without an opening.
    NoOpening,
}

impl fmt::Display for CastError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CastError::Dead => "You are dead",
            CastError::Busy => "Already casting",
            CastError::NoSuchAbility => "No ability in that slot",
            CastError::NotReady => "Ability is not ready yet",
            CastError::OnGlobalCooldown => "Global cooldown",
            CastError::NotEnoughResource => "Not enough resource",
            CastError::NoTarget => "No target",
            CastError::InvalidTarget => "Invalid target",
            CastError::OutOfRange => "Out of range",
            CastError::NoLineOfSight => "Target not in line of sight",
            CastError::Moving => "Can't cast while moving",
            CastError::Silenced => "Silenced",
            CastError::Incapacitated => "Can't act while incapacitated",
            CastError::Immune => "Target is immune",
            CastError::NothingToDispel => "Nothing to dispel",
            CastError::TargetWeakened => "Target is Weakened and can't be warded",
            CastError::NoEdge => "No Edge to spend",
            CastError::NoOpening => "No opening on the target",
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventKind {
    Damage {
        src: ActorId,
        dst: ActorId,
        what: Label,
        amount: f32,
        absorbed: f32,
        outcome: HitOutcome,
        school: School,
    },
    Heal {
        src: ActorId,
        dst: ActorId,
        what: Label,
        amount: f32,
        overheal: f32,
        crit: bool,
    },
    CastStart {
        src: ActorId,
        what: Label,
        target: ActorId,
    },
    CastFailed {
        src: ActorId,
        what: Label,
        reason: CastError,
    },
    /// A cast was cancelled by its caster (moving, stop-casting).
    Interrupted {
        src: ActorId,
        what: Label,
    },
    /// `src` interrupted `dst`'s cast of `what`.
    Interrupt {
        src: ActorId,
        dst: ActorId,
        what: Label,
    },
    AuraApplied {
        src: ActorId,
        dst: ActorId,
        what: Label,
        stacks: u32,
    },
    Dispel {
        src: ActorId,
        dst: ActorId,
        what: Label,
    },
    CcBroken {
        dst: ActorId,
        what: Label,
    },
    Taunt {
        src: ActorId,
        dst: ActorId,
    },
    ThreatDrop {
        src: ActorId,
    },
    Aggro {
        encounter: usize,
        by: ActorId,
    },
    TargetChange {
        enemy: ActorId,
        target: ActorId,
    },
    Evade {
        enemy: ActorId,
    },
    Death {
        actor: ActorId,
    },
    /// A fallen party member rallied after combat.
    Revive {
        actor: ActorId,
    },
    PhaseChange {
        boss: ActorId,
        phase: u8,
        announce: Label,
    },
    Enrage {
        boss: ActorId,
    },
    Summon {
        src: ActorId,
        what: Label,
        count: u32,
    },
    TelegraphStart {
        id: u32,
        source: ActorId,
        what: Label,
        center: V2,
        radius: f32,
        delay: f32,
    },
    TelegraphHit {
        id: u32,
        hits: u32,
    },
    EncounterReset {
        encounter: usize,
    },
    EncounterCleared {
        encounter: usize,
    },
    /// A treasure cache was opened (0.9).
    CacheOpened {
        cache: usize,
        by: ActorId,
    },
    Outcome {
        outcome: RunOutcome,
    },
    /// 0.11: the party broke `enemy`'s cast of `what` with damage; it is
    /// exposed.
    Staggered {
        enemy: ActorId,
        what: Label,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub time: f32,
    pub kind: EventKind,
}

impl Event {
    /// Events worth showing in the compact on-screen log (skips auto-attack spam).
    pub fn is_notable(&self) -> bool {
        match &self.kind {
            EventKind::Damage { what, outcome, .. } => {
                !(matches!(&**what, "Melee" | "Wand")
                    && matches!(outcome, HitOutcome::Hit | HitOutcome::Block))
            }
            EventKind::TargetChange { .. } | EventKind::Evade { .. } => false,
            _ => true,
        }
    }
}

pub fn describe(sim: &Sim, e: &Event) -> String {
    let n = |id: ActorId| sim.actors[id].name.as_str();
    let body = match &e.kind {
        EventKind::Damage {
            src,
            dst,
            what,
            amount,
            absorbed,
            outcome,
            school,
        } => {
            if outcome.avoided() {
                format!(
                    "{}'s {what} on {}: {}",
                    n(*src),
                    n(*dst),
                    outcome.label().to_uppercase()
                )
            } else {
                let mut s = format!("{}'s {what} hits {} for {amount:.0}", n(*src), n(*dst));
                if school.is_magic() {
                    s.push_str(&format!(" {}", school.label()));
                }
                match outcome {
                    HitOutcome::Crit => s.push_str(" (critical)"),
                    HitOutcome::Block => s.push_str(" (blocked)"),
                    _ => {}
                }
                if *absorbed > 0.0 {
                    s.push_str(&format!(" ({absorbed:.0} absorbed)"));
                }
                s
            }
        }
        EventKind::Heal {
            src,
            dst,
            what,
            amount,
            overheal,
            crit,
        } => {
            let c = if *crit { " (critical)" } else { "" };
            if *overheal > 0.0 {
                format!(
                    "{}'s {what} heals {} for {amount:.0}{c} ({overheal:.0} overheal)",
                    n(*src),
                    n(*dst)
                )
            } else {
                format!("{}'s {what} heals {} for {amount:.0}{c}", n(*src), n(*dst))
            }
        }
        EventKind::CastStart { src, what, target } => {
            format!("{} begins casting {what} on {}", n(*src), n(*target))
        }
        EventKind::CastFailed { src, what, reason } => {
            format!("{} fails to use {what}: {reason}", n(*src))
        }
        EventKind::Interrupted { src, what } => format!("{}'s {what} is cancelled", n(*src)),
        EventKind::Interrupt { src, dst, what } => {
            format!("{} INTERRUPTS {}'s {what}", n(*src), n(*dst))
        }
        EventKind::AuraApplied {
            src,
            dst,
            what,
            stacks,
        } => {
            let st = if *stacks > 1 {
                format!(" ({stacks})")
            } else {
                String::new()
            };
            if src == dst {
                format!("{} gains {what}{st}", n(*src))
            } else {
                format!("{} is affected by {}'s {what}{st}", n(*dst), n(*src))
            }
        }
        EventKind::Dispel { src, dst, what } => {
            format!("{} removes {what} from {}", n(*src), n(*dst))
        }
        EventKind::CcBroken { dst, what } => format!("{}'s {what} breaks", n(*dst)),
        EventKind::Taunt { src, dst } => format!("{} taunts {}", n(*src), n(*dst)),
        EventKind::ThreatDrop { src } => format!("{} sheds threat", n(*src)),
        EventKind::Aggro { encounter, by } => format!(
            "Pack '{}' engages (pulled by {})",
            sim.encounters[*encounter].id,
            n(*by)
        ),
        EventKind::TargetChange { enemy, target } => {
            format!("{} turns to attack {}", n(*enemy), n(*target))
        }
        EventKind::Evade { enemy } => format!("{} evades (resetting)", n(*enemy)),
        EventKind::Death { actor } => format!("{} dies", n(*actor)),
        EventKind::Revive { actor } => format!("{} rallies and rejoins the party", n(*actor)),
        EventKind::PhaseChange {
            boss,
            phase,
            announce,
        } => {
            if announce.is_empty() {
                format!("{} enters phase {phase}!", n(*boss))
            } else {
                format!("{} enters phase {phase}! {announce}", n(*boss))
            }
        }
        EventKind::Enrage { boss } => format!("{} becomes ENRAGED", n(*boss)),
        EventKind::Summon { src, what, count } => {
            format!("{} uses {what}, summoning {count}", n(*src))
        }
        EventKind::TelegraphStart {
            source,
            what,
            radius,
            delay,
            ..
        } => format!(
            "{} conjures {what} ({radius:.0}m) - detonates in {delay:.1}s, MOVE!",
            n(*source)
        ),
        EventKind::TelegraphHit { hits, .. } => {
            format!("Ground effect detonates, hitting {hits}")
        }
        EventKind::EncounterReset { encounter } => {
            format!("Encounter '{}' resets", sim.encounters[*encounter].id)
        }
        EventKind::EncounterCleared { encounter } => {
            format!("Encounter '{}' cleared", sim.encounters[*encounter].id)
        }
        EventKind::CacheOpened { cache, by } => {
            format!("{} opens {}", n(*by), sim.caches[*cache].name)
        }
        EventKind::Outcome { outcome } => format!("RUN OVER: {outcome:?}"),
        EventKind::Staggered { enemy, what } => {
            format!("{}'s {what} is broken: exposed!", n(*enemy))
        }
    };
    format!("[{:7.2}] {body}", e.time)
}
