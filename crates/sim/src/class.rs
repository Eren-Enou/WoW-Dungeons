//! Player-side classes, roles and ability definitions.
//!
//! Formulas are deliberately simple placeholders for a prototype; see PROGRESS.md.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClassId {
    /// Shield-bearing tank.
    Warden,
    /// Healer.
    Mender,
    /// Melee damage.
    Striker,
    /// Ranged spell damage.
    Arcanist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Tank,
    Healer,
    MeleeDps,
    RangedDps,
}

impl Role {
    pub fn is_dps(self) -> bool {
        matches!(self, Role::MeleeDps | Role::RangedDps)
    }

    pub fn label(self) -> &'static str {
        match self {
            Role::Tank => "Tank",
            Role::Healer => "Healer",
            Role::MeleeDps => "Melee DPS",
            Role::RangedDps => "Ranged DPS",
        }
    }
}

pub struct ClassStats {
    pub max_hp: f32,
    pub max_mana: f32,
    pub speed: f32,
    /// Auto-attack damage per swing.
    pub attack_damage: f32,
    pub attack_interval: f32,
    /// Auto-attack reach (edge to edge).
    pub attack_range: f32,
    /// Incoming damage multiplier (armor).
    pub damage_taken: f32,
    /// Multiplier on all threat generated.
    pub threat_mult: f32,
    /// Fraction of max mana regenerated per second while in combat.
    pub combat_mana_regen: f32,
}

impl ClassId {
    pub const ALL: [ClassId; 4] = [
        ClassId::Warden,
        ClassId::Mender,
        ClassId::Striker,
        ClassId::Arcanist,
    ];

    pub fn role(self) -> Role {
        match self {
            ClassId::Warden => Role::Tank,
            ClassId::Mender => Role::Healer,
            ClassId::Striker => Role::MeleeDps,
            ClassId::Arcanist => Role::RangedDps,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ClassId::Warden => "Warden",
            ClassId::Mender => "Mender",
            ClassId::Striker => "Striker",
            ClassId::Arcanist => "Arcanist",
        }
    }

    pub fn next(self) -> ClassId {
        let i = ClassId::ALL.iter().position(|c| *c == self).unwrap_or(0);
        ClassId::ALL[(i + 1) % ClassId::ALL.len()]
    }

    pub fn stats(self) -> ClassStats {
        match self {
            ClassId::Warden => ClassStats {
                max_hp: 1500.0,
                max_mana: 300.0,
                speed: 7.0,
                attack_damage: 30.0,
                attack_interval: 2.0,
                attack_range: 2.0,
                damage_taken: 0.6,
                threat_mult: 1.5,
                combat_mana_regen: 0.03,
            },
            ClassId::Mender => ClassStats {
                max_hp: 850.0,
                max_mana: 1000.0,
                speed: 7.0,
                attack_damage: 15.0,
                attack_interval: 2.0,
                attack_range: 2.0,
                damage_taken: 0.9,
                threat_mult: 1.0,
                combat_mana_regen: 0.02,
            },
            ClassId::Striker => ClassStats {
                max_hp: 1050.0,
                max_mana: 300.0,
                speed: 7.0,
                attack_damage: 42.0,
                attack_interval: 1.8,
                attack_range: 2.0,
                damage_taken: 0.8,
                threat_mult: 1.0,
                combat_mana_regen: 0.04,
            },
            ClassId::Arcanist => ClassStats {
                max_hp: 850.0,
                max_mana: 900.0,
                speed: 7.0,
                attack_damage: 18.0,
                attack_interval: 2.0,
                attack_range: 25.0,
                damage_taken: 0.9,
                threat_mult: 1.0,
                combat_mana_regen: 0.015,
            },
        }
    }

    /// The three hotkeyed abilities (keys 1, 2, 3).
    pub fn abilities(self) -> [AbilityId; 3] {
        match self {
            ClassId::Warden => [
                AbilityId::ShieldSlam,
                AbilityId::Challenge,
                AbilityId::Bulwark,
            ],
            ClassId::Mender => [AbilityId::Mend, AbilityId::Renew, AbilityId::Smite],
            ClassId::Striker => [AbilityId::RendStrike, AbilityId::Whirl, AbilityId::Evade],
            ClassId::Arcanist => [AbilityId::ArcBolt, AbilityId::Flare, AbilityId::Veil],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AbilityId {
    ShieldSlam,
    Challenge,
    Bulwark,
    Mend,
    Renew,
    Smite,
    RendStrike,
    Whirl,
    Evade,
    ArcBolt,
    Flare,
    Veil,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetRule {
    /// Requires a living hostile target in range.
    Enemy,
    /// Friendly target in range; falls back to self when none/hostile is selected.
    Ally,
    /// Always the caster.
    SelfOnly,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    /// Direct damage with a threat multiplier.
    Damage {
        amount: f32,
        threat_mult: f32,
    },
    /// Damage to every engaged enemy within `radius` of the caster.
    AoeDamage {
        amount: f32,
        radius: f32,
    },
    Heal {
        amount: f32,
    },
    /// Heal over time: `total` spread over `duration` seconds, ticking each second.
    Hot {
        total: f32,
        duration: f32,
    },
    /// Taunt: caster's threat is raised to the top and the enemy is forced to attack them.
    Taunt {
        duration: f32,
    },
    /// Self buff reducing damage taken by `pct` for `duration` seconds.
    DamageReduction {
        pct: f32,
        duration: f32,
    },
    /// Reduce caster's threat on every enemy by `pct`.
    ThreatDrop {
        pct: f32,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct AbilityDef {
    pub id: AbilityId,
    pub name: &'static str,
    pub description: &'static str,
    pub target: TargetRule,
    pub range: f32,
    pub cast_time: f32,
    pub cooldown: f32,
    pub mana_cost: f32,
    pub effect: Effect,
}

/// Global cooldown triggered by every player-side ability.
pub const GCD: f32 = 1.0;

impl AbilityId {
    pub fn def(self) -> AbilityDef {
        use AbilityId::*;
        use Effect::*;
        let d = |id, name, description, target, range, cast_time, cooldown, mana_cost, effect| {
            AbilityDef {
                id,
                name,
                description,
                target,
                range,
                cast_time,
                cooldown,
                mana_cost,
                effect,
            }
        };
        match self {
            ShieldSlam => d(
                ShieldSlam,
                "Shield Slam",
                "Melee hit that generates 4x threat.",
                TargetRule::Enemy,
                2.0,
                0.0,
                5.0,
                20.0,
                Damage {
                    amount: 70.0,
                    threat_mult: 4.0,
                },
            ),
            Challenge => d(
                Challenge,
                "Challenge",
                "Taunt: take the top of the threat table and force the enemy to attack you for 3s.",
                TargetRule::Enemy,
                25.0,
                0.0,
                8.0,
                10.0,
                Taunt { duration: 3.0 },
            ),
            Bulwark => d(
                Bulwark,
                "Bulwark",
                "Reduce damage taken by 40% for 6s.",
                TargetRule::SelfOnly,
                0.0,
                0.0,
                20.0,
                0.0,
                DamageReduction {
                    pct: 0.4,
                    duration: 6.0,
                },
            ),
            Mend => d(
                Mend,
                "Mend",
                "1.5s cast: heal an ally for a large amount.",
                TargetRule::Ally,
                30.0,
                1.5,
                0.0,
                70.0,
                Heal { amount: 420.0 },
            ),
            Renew => d(
                Renew,
                "Renew",
                "Instant: heal an ally for 300 over 9s.",
                TargetRule::Ally,
                30.0,
                0.0,
                0.0,
                45.0,
                Hot {
                    total: 300.0,
                    duration: 9.0,
                },
            ),
            Smite => d(
                Smite,
                "Smite",
                "1.5s cast: ranged holy damage.",
                TargetRule::Enemy,
                30.0,
                1.5,
                0.0,
                40.0,
                Damage {
                    amount: 75.0,
                    threat_mult: 1.0,
                },
            ),
            RendStrike => d(
                RendStrike,
                "Rend Strike",
                "Heavy melee strike.",
                TargetRule::Enemy,
                2.0,
                0.0,
                4.0,
                25.0,
                Damage {
                    amount: 110.0,
                    threat_mult: 1.0,
                },
            ),
            Whirl => d(
                Whirl,
                "Whirl",
                "Hit every engaged enemy within 6m.",
                TargetRule::SelfOnly,
                0.0,
                0.0,
                10.0,
                35.0,
                AoeDamage {
                    amount: 65.0,
                    radius: 6.0,
                },
            ),
            Evade => d(
                Evade,
                "Evade",
                "Drop 50% of your threat on all enemies.",
                TargetRule::SelfOnly,
                0.0,
                0.0,
                20.0,
                0.0,
                ThreatDrop { pct: 0.5 },
            ),
            ArcBolt => d(
                ArcBolt,
                "Arc Bolt",
                "2s cast: ranged arcane damage.",
                TargetRule::Enemy,
                30.0,
                2.0,
                0.0,
                28.0,
                Damage {
                    amount: 125.0,
                    threat_mult: 1.0,
                },
            ),
            Flare => d(
                Flare,
                "Flare",
                "Instant ranged burst.",
                TargetRule::Enemy,
                30.0,
                0.0,
                6.0,
                40.0,
                Damage {
                    amount: 75.0,
                    threat_mult: 1.0,
                },
            ),
            Veil => d(
                Veil,
                "Veil",
                "Drop 50% of your threat on all enemies.",
                TargetRule::SelfOnly,
                0.0,
                0.0,
                20.0,
                0.0,
                ThreatDrop { pct: 0.5 },
            ),
        }
    }
}
