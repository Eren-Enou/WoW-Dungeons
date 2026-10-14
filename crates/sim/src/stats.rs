//! Combat statistics, damage schools and the mitigation / hit-table formulas.
//!
//! Formulas marked **[WoW]** follow publicly documented World of Warcraft Classic
//! behaviour; those marked **[orig]** are simplifications invented for this project.
//! See RULES.md for the prose version.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum School {
    #[default]
    Physical,
    Holy,
    Fire,
    Frost,
    Shadow,
    Arcane,
    Nature,
}

impl School {
    pub fn is_magic(self) -> bool {
        self != School::Physical
    }

    pub fn label(self) -> &'static str {
        match self {
            School::Physical => "physical",
            School::Holy => "holy",
            School::Fire => "fire",
            School::Frost => "frost",
            School::Shadow => "shadow",
            School::Arcane => "arcane",
            School::Nature => "nature",
        }
    }
}

/// Final combat statistics of an actor (base + level + gear).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub level: u32,
    /// Physical armor.
    pub armor: f32,
    /// Resistance to all magic schools (simplified single value).
    pub resist: f32,
    /// Chance to dodge melee attacks (0..1).
    pub dodge: f32,
    /// Chance to parry melee attacks from the front (0..1).
    pub parry: f32,
    /// Chance to block melee attacks from the front (0..1).
    pub block: f32,
    /// Damage removed by a successful block.
    pub block_value: f32,
    /// Critical strike chance (0..1).
    pub crit: f32,
    /// Bonus chance to hit (0..1), subtracted from miss chances.
    pub hit: f32,
}

impl Default for Stats {
    fn default() -> Self {
        Stats {
            level: 1,
            armor: 0.0,
            resist: 0.0,
            dodge: 0.0,
            parry: 0.0,
            block: 0.0,
            block_value: 0.0,
            crit: 0.05,
            hit: 0.0,
        }
    }
}

/// How an attack is resolved against the defender.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    /// Weapon attacks and melee specials: full attack table.
    Melee,
    /// Spells: can miss (resist fully), cannot be dodged/parried/blocked.
    Spell,
    /// Damage-over-time ticks and auras: always hit, never crit.
    Periodic,
    /// Area / environmental effects: always hit, can't crit, mitigation still applies.
    Unavoidable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum HitOutcome {
    Hit,
    Crit,
    Miss,
    Dodge,
    Parry,
    /// Hit, but partially blocked.
    Block,
    /// A spell that fully missed.
    Resist,
    /// Fully absorbed by a shield.
    Absorb,
}

impl HitOutcome {
    pub fn avoided(self) -> bool {
        matches!(
            self,
            HitOutcome::Miss | HitOutcome::Dodge | HitOutcome::Parry | HitOutcome::Resist
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            HitOutcome::Hit => "hit",
            HitOutcome::Crit => "crit",
            HitOutcome::Miss => "miss",
            HitOutcome::Dodge => "dodge",
            HitOutcome::Parry => "parry",
            HitOutcome::Block => "block",
            HitOutcome::Resist => "resist",
            HitOutcome::Absorb => "absorb",
        }
    }
}

/// [WoW] Classic armor damage reduction against an attacker of `attacker_level`:
/// `armor / (armor + 400 + 85 * level)`, capped at 75%.
pub fn armor_reduction(armor: f32, attacker_level: u32) -> f32 {
    if armor <= 0.0 {
        return 0.0;
    }
    let dr = armor / (armor + 400.0 + 85.0 * attacker_level as f32);
    dr.min(0.75)
}

/// [orig] Average fraction of magic damage resisted, a hyperbolic curve shaped like
/// armor: `resist / (resist + 100 + 15 * attacker_level)`, capped at 75%. (WoW Classic
/// used `0.75 * resist / (5 * level)` with partial-resist bins; that is far too steep
/// at the low levels this game uses, so it is replaced. Applied as a smooth average.)
pub fn resist_reduction(resist: f32, attacker_level: u32) -> f32 {
    if resist <= 0.0 {
        return 0.0;
    }
    (resist / (resist + 100.0 + 15.0 * attacker_level as f32)).min(0.75)
}

/// [WoW-approx] Chance for a melee attack to miss: 5% base, +1% per level the defender
/// is above the attacker (a +3 boss gives 8%), minus attacker hit. Floor 0.
pub fn melee_miss_chance(attacker: &Stats, defender: &Stats) -> f32 {
    let diff = defender.level as f32 - attacker.level as f32;
    (0.05 + 0.01 * diff.max(0.0) - attacker.hit).max(0.0)
}

/// [WoW] Spell miss chance by level difference (defender − attacker): 4%, 5%, 6% for
/// 0..=2 and 17% at +3 (then +11% per further level), minus attacker hit, floor 1%.
pub fn spell_miss_chance(attacker: &Stats, defender: &Stats) -> f32 {
    let diff = defender.level as i64 - attacker.level as i64;
    let base = match diff {
        d if d <= 0 => 0.04,
        1 => 0.05,
        2 => 0.06,
        d => 0.17 + 0.11 * (d - 3) as f32,
    };
    (base - attacker.hit).clamp(0.01, 0.99)
}

/// [WoW-approx] Crit chance: attacker crit −1% per level the defender is above. Floor 0.
pub fn crit_chance(attacker: &Stats, defender: &Stats) -> f32 {
    let diff = defender.level as f32 - attacker.level as f32;
    (attacker.crit - 0.01 * diff.max(0.0)).max(0.0)
}

/// Critical multipliers [WoW]: melee x2.0, spells and heals x1.5.
pub const MELEE_CRIT_MULT: f32 = 2.0;
pub const SPELL_CRIT_MULT: f32 = 1.5;

/// [WoW] Single-roll melee attack table. `roll` is uniform in [0, 1).
/// Order: miss, dodge, parry (front only), block (front only), crit, hit.
/// Defenders gain +0.2% dodge per level above the attacker [orig simplification].
pub fn melee_table(
    attacker: &Stats,
    defender: &Stats,
    from_front: bool,
    can_crit: bool,
    roll: f32,
) -> HitOutcome {
    let diff = (defender.level as f32 - attacker.level as f32).max(0.0);
    let mut edge = melee_miss_chance(attacker, defender);
    if roll < edge {
        return HitOutcome::Miss;
    }
    edge += defender.dodge + 0.002 * diff;
    if roll < edge {
        return HitOutcome::Dodge;
    }
    if from_front {
        edge += defender.parry;
        if roll < edge {
            return HitOutcome::Parry;
        }
        edge += defender.block;
        if roll < edge {
            return HitOutcome::Block;
        }
    }
    if can_crit {
        edge += crit_chance(attacker, defender);
        if roll < edge {
            return HitOutcome::Crit;
        }
    }
    HitOutcome::Hit
}

/// Spell table: a miss roll, then a crit roll (two independent rolls, as in WoW).
pub fn spell_table(
    attacker: &Stats,
    defender: &Stats,
    can_crit: bool,
    miss_roll: f32,
    crit_roll: f32,
) -> HitOutcome {
    if miss_roll < spell_miss_chance(attacker, defender) {
        return HitOutcome::Resist;
    }
    if can_crit && crit_roll < crit_chance(attacker, defender) {
        return HitOutcome::Crit;
    }
    HitOutcome::Hit
}

/// [WoW] Rage conversion constant at a level: `0.0091·L² + 3.23·L + 4.27`.
pub fn rage_conversion(level: u32) -> f32 {
    let l = level as f32;
    0.0091 * l * l + 3.23 * l + 4.27
}

/// [WoW] Rage from dealing melee damage: `7.5 · damage / c`.
pub fn rage_from_dealt(damage: f32, level: u32) -> f32 {
    7.5 * damage / rage_conversion(level)
}

/// [WoW] Rage from taking damage: `2.5 · damage / c`.
pub fn rage_from_taken(damage: f32, level: u32) -> f32 {
    2.5 * damage / rage_conversion(level)
}

/// Whether `attacker_pos` is within the front 180° arc of a defender facing `heading`.
pub fn is_in_front(
    defender_pos: crate::math::V2,
    heading: f32,
    attacker_pos: crate::math::V2,
) -> bool {
    let to = (attacker_pos - defender_pos).normalize_or_zero();
    let fwd = crate::math::V2::from_heading(heading);
    to.x * fwd.x + to.z * fwd.z >= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::V2;

    fn lvl(level: u32) -> Stats {
        Stats {
            level,
            crit: 0.0,
            ..Default::default()
        }
    }

    #[test]
    fn armor_formula_matches_classic_and_caps() {
        // 1000 armor vs level 10: 1000 / (1000 + 400 + 850) = 0.4444
        assert!((armor_reduction(1000.0, 10) - 0.4444).abs() < 1e-3);
        assert_eq!(armor_reduction(0.0, 10), 0.0);
        assert_eq!(armor_reduction(1e9, 1), 0.75);
    }

    #[test]
    fn resist_formula_caps_at_75_percent() {
        // 50 resist vs level 10: 50 / (50 + 100 + 150) = 1/6.
        assert!((resist_reduction(50.0, 10) - 1.0 / 6.0).abs() < 1e-4);
        assert_eq!(resist_reduction(1e6, 1), 0.75);
        assert_eq!(resist_reduction(0.0, 5), 0.0);
    }

    #[test]
    fn melee_miss_grows_with_level_gap_and_hit_reduces_it() {
        assert!((melee_miss_chance(&lvl(10), &lvl(10)) - 0.05).abs() < 1e-6);
        assert!((melee_miss_chance(&lvl(10), &lvl(13)) - 0.08).abs() < 1e-6);
        let mut a = lvl(10);
        a.hit = 0.06;
        assert!((melee_miss_chance(&a, &lvl(13)) - 0.02).abs() < 1e-6);
        a.hit = 0.5;
        assert_eq!(melee_miss_chance(&a, &lvl(10)), 0.0);
    }

    #[test]
    fn spell_miss_table() {
        assert!((spell_miss_chance(&lvl(5), &lvl(5)) - 0.04).abs() < 1e-6);
        assert!((spell_miss_chance(&lvl(5), &lvl(7)) - 0.06).abs() < 1e-6);
        assert!((spell_miss_chance(&lvl(5), &lvl(8)) - 0.17).abs() < 1e-6);
        let mut a = lvl(5);
        a.hit = 0.2;
        assert!(
            (spell_miss_chance(&a, &lvl(5)) - 0.01).abs() < 1e-6,
            "1% floor"
        );
    }

    #[test]
    fn melee_table_order_and_front_rules() {
        let a = Stats {
            level: 10,
            crit: 0.10,
            ..Default::default()
        };
        let d = Stats {
            level: 10,
            dodge: 0.10,
            parry: 0.10,
            block: 0.10,
            ..Default::default()
        };
        // From the front: miss [0,.05) dodge [.05,.15) parry [.15,.25) block [.25,.35) crit [.35,.45)
        assert_eq!(melee_table(&a, &d, true, true, 0.01), HitOutcome::Miss);
        assert_eq!(melee_table(&a, &d, true, true, 0.10), HitOutcome::Dodge);
        assert_eq!(melee_table(&a, &d, true, true, 0.20), HitOutcome::Parry);
        assert_eq!(melee_table(&a, &d, true, true, 0.30), HitOutcome::Block);
        assert_eq!(melee_table(&a, &d, true, true, 0.40), HitOutcome::Crit);
        assert_eq!(melee_table(&a, &d, true, true, 0.50), HitOutcome::Hit);
        // From behind: no parry or block, so crit starts right after dodge.
        assert_eq!(melee_table(&a, &d, false, true, 0.20), HitOutcome::Crit);
        assert_eq!(melee_table(&a, &d, false, true, 0.30), HitOutcome::Hit);
        // Crit disabled.
        assert_eq!(melee_table(&a, &d, false, false, 0.20), HitOutcome::Hit);
    }

    #[test]
    fn spell_table_misses_then_crits() {
        let a = Stats {
            level: 5,
            crit: 0.1,
            ..Default::default()
        };
        let d = lvl(5);
        assert_eq!(spell_table(&a, &d, true, 0.01, 0.5), HitOutcome::Resist);
        assert_eq!(spell_table(&a, &d, true, 0.5, 0.05), HitOutcome::Crit);
        assert_eq!(spell_table(&a, &d, true, 0.5, 0.5), HitOutcome::Hit);
    }

    #[test]
    fn rage_formulas() {
        let c = rage_conversion(10);
        assert!((c - (0.91 + 32.3 + 4.27)).abs() < 1e-3);
        assert!((rage_from_dealt(c, 10) - 7.5).abs() < 1e-4);
        assert!((rage_from_taken(c, 10) - 2.5).abs() < 1e-4);
    }

    #[test]
    fn front_arc() {
        let d = V2::new(0.0, 0.0);
        // Facing +Z.
        assert!(is_in_front(d, 0.0, V2::new(0.0, 5.0)));
        assert!(!is_in_front(d, 0.0, V2::new(0.0, -5.0)));
        assert!(
            is_in_front(d, 0.0, V2::new(5.0, 0.0)),
            "side counts as front"
        );
    }
}
