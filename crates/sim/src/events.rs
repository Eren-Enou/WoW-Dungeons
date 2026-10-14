//! Combat events: the authoritative, inspectable record of everything that happened.

use crate::math::V2;
use crate::sim::{ActorId, RunOutcome, Sim};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastError {
    Dead,
    Busy,
    NoSuchAbility,
    NotReady,
    OnGlobalCooldown,
    NotEnoughMana,
    NoTarget,
    InvalidTarget,
    OutOfRange,
    Moving,
}

impl fmt::Display for CastError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CastError::Dead => "You are dead",
            CastError::Busy => "Already casting",
            CastError::NoSuchAbility => "No ability in that slot",
            CastError::NotReady => "Ability is not ready yet",
            CastError::OnGlobalCooldown => "Global cooldown",
            CastError::NotEnoughMana => "Not enough mana",
            CastError::NoTarget => "No target",
            CastError::InvalidTarget => "Invalid target",
            CastError::OutOfRange => "Out of range",
            CastError::Moving => "Can't cast while moving",
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventKind {
    Damage {
        src: ActorId,
        dst: ActorId,
        what: &'static str,
        amount: f32,
        crit: bool,
    },
    Heal {
        src: ActorId,
        dst: ActorId,
        what: &'static str,
        amount: f32,
        overheal: f32,
    },
    CastStart {
        src: ActorId,
        what: &'static str,
        target: ActorId,
    },
    CastFailed {
        src: ActorId,
        what: &'static str,
        reason: CastError,
    },
    Interrupted {
        src: ActorId,
        what: &'static str,
    },
    Buff {
        src: ActorId,
        dst: ActorId,
        what: &'static str,
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
    PhaseChange {
        boss: ActorId,
        phase: u8,
    },
    Enrage {
        boss: ActorId,
    },
    TelegraphStart {
        id: u32,
        source: ActorId,
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
    Outcome {
        outcome: RunOutcome,
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
        !matches!(
            self.kind,
            EventKind::Damage {
                what: "Melee" | "Wand",
                crit: false,
                ..
            } | EventKind::TargetChange { .. }
                | EventKind::Evade { .. }
        )
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
            crit,
        } => format!(
            "{}'s {what} hits {} for {amount:.0}{}",
            n(*src),
            n(*dst),
            if *crit { " (critical)" } else { "" }
        ),
        EventKind::Heal {
            src,
            dst,
            what,
            amount,
            overheal,
        } => {
            if *overheal > 0.0 {
                format!(
                    "{}'s {what} heals {} for {amount:.0} ({overheal:.0} overheal)",
                    n(*src),
                    n(*dst)
                )
            } else {
                format!("{}'s {what} heals {} for {amount:.0}", n(*src), n(*dst))
            }
        }
        EventKind::CastStart { src, what, target } => {
            format!("{} begins casting {what} on {}", n(*src), n(*target))
        }
        EventKind::CastFailed { src, what, reason } => {
            format!("{} fails to use {what}: {reason}", n(*src))
        }
        EventKind::Interrupted { src, what } => format!("{}'s {what} is interrupted", n(*src)),
        EventKind::Buff { src, dst, what } => {
            if src == dst {
                format!("{} gains {what}", n(*src))
            } else {
                format!("{} gains {what} from {}", n(*dst), n(*src))
            }
        }
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
        EventKind::PhaseChange { boss, phase } => format!(
            "{} enters phase {phase}! The ground begins to smoulder...",
            n(*boss)
        ),
        EventKind::Enrage { boss } => format!("{} becomes ENRAGED", n(*boss)),
        EventKind::TelegraphStart {
            source,
            radius,
            delay,
            ..
        } => format!(
            "{} conjures a Cinder Ring ({radius:.0}m) - detonates in {delay:.1}s, MOVE!",
            n(*source)
        ),
        EventKind::TelegraphHit { hits, .. } => format!("Cinder Ring detonates, hitting {hits}"),
        EventKind::EncounterReset { encounter } => {
            format!("Encounter '{}' resets", sim.encounters[*encounter].id)
        }
        EventKind::EncounterCleared { encounter } => {
            format!("Encounter '{}' cleared", sim.encounters[*encounter].id)
        }
        EventKind::Outcome { outcome } => format!("RUN OVER: {outcome:?}"),
    };
    format!("[{:7.2}] {body}", e.time)
}
