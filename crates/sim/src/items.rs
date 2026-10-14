//! Equipment: slots, rarity, role affinity and item statistics.

use crate::class::Role;
use crate::stats::Stats;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemSlot {
    #[serde(alias = "weapon")]
    MainHand,
    OffHand,
    Head,
    #[serde(alias = "armor")]
    Chest,
    Legs,
    Feet,
    Trinket,
}

impl ItemSlot {
    pub const ALL: [ItemSlot; 7] = [
        ItemSlot::MainHand,
        ItemSlot::OffHand,
        ItemSlot::Head,
        ItemSlot::Chest,
        ItemSlot::Legs,
        ItemSlot::Feet,
        ItemSlot::Trinket,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ItemSlot::MainHand => "Main hand",
            ItemSlot::OffHand => "Off hand",
            ItemSlot::Head => "Head",
            ItemSlot::Chest => "Chest",
            ItemSlot::Legs => "Legs",
            ItemSlot::Feet => "Feet",
            ItemSlot::Trinket => "Trinket",
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "lowercase")]
pub enum Rarity {
    #[default]
    Common,
    Uncommon,
    Rare,
    Epic,
}

impl Rarity {
    pub fn label(self) -> &'static str {
        match self {
            Rarity::Common => "Common",
            Rarity::Uncommon => "Uncommon",
            Rarity::Rare => "Rare",
            Rarity::Epic => "Epic",
        }
    }

    /// [orig] Stat multiplier applied to an item's base stats.
    pub fn stat_mult(self) -> f32 {
        match self {
            Rarity::Common => 1.0,
            Rarity::Uncommon => 1.25,
            Rarity::Rare => 1.5,
            Rarity::Epic => 1.9,
        }
    }

    pub fn upgraded(self, steps: u32) -> Rarity {
        let order = [Rarity::Common, Rarity::Uncommon, Rarity::Rare, Rarity::Epic];
        let i = order.iter().position(|r| *r == self).unwrap_or(0);
        order[(i + steps as usize).min(order.len() - 1)]
    }
}

/// Which role an item is designed for. Anyone may wear anything; the hub warns on mismatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RoleAffinity {
    #[default]
    Any,
    Tank,
    Healer,
    Melee,
    Caster,
}

impl RoleAffinity {
    pub fn suits(self, role: Role) -> bool {
        match self {
            RoleAffinity::Any => true,
            RoleAffinity::Tank => role == Role::Tank,
            RoleAffinity::Healer => role == Role::Healer,
            RoleAffinity::Melee => matches!(role, Role::MeleeDps | Role::Tank),
            RoleAffinity::Caster => matches!(role, Role::RangedDps | Role::Healer),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            RoleAffinity::Any => "any",
            RoleAffinity::Tank => "tank",
            RoleAffinity::Healer => "healer",
            RoleAffinity::Melee => "melee",
            RoleAffinity::Caster => "caster",
        }
    }
}

/// Item statistics. `crit`, `hit` and `dodge` are in percentage points (1.0 = 1%).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ItemStats {
    /// +10 max HP each.
    pub stamina: f32,
    /// +1% damage and healing each.
    pub power: f32,
    pub armor: f32,
    pub resist: f32,
    pub crit: f32,
    pub hit: f32,
    pub dodge: f32,
}

impl ItemStats {
    pub fn add(&mut self, o: &ItemStats) {
        self.stamina += o.stamina;
        self.power += o.power;
        self.armor += o.armor;
        self.resist += o.resist;
        self.crit += o.crit;
        self.hit += o.hit;
        self.dodge += o.dodge;
    }

    pub fn scaled(&self, k: f32) -> ItemStats {
        ItemStats {
            stamina: (self.stamina * k).round(),
            power: (self.power * k).round(),
            armor: (self.armor * k).round(),
            resist: (self.resist * k).round(),
            crit: (self.crit * k * 10.0).round() / 10.0,
            hit: (self.hit * k * 10.0).round() / 10.0,
            dodge: (self.dodge * k * 10.0).round() / 10.0,
        }
    }

    /// Apply gear to combat stats.
    pub fn apply(&self, s: &mut Stats) {
        s.armor += self.armor;
        s.resist += self.resist;
        s.crit += self.crit / 100.0;
        s.hit += self.hit / 100.0;
        s.dodge += self.dodge / 100.0;
    }

    /// A rough single "item score" used for comparisons and AI gear choice.
    pub fn score_for(&self, role: Role) -> f32 {
        let (st, pw, ar, cr, hi, dg) = match role {
            Role::Tank => (1.6, 0.6, 0.04, 2.0, 3.0, 6.0),
            Role::Healer => (0.8, 2.0, 0.01, 3.0, 0.5, 0.5),
            Role::MeleeDps => (0.7, 2.0, 0.01, 6.0, 6.0, 1.0),
            Role::RangedDps => (0.7, 2.0, 0.01, 5.0, 6.0, 0.5),
        };
        self.stamina * st
            + self.power * pw
            + self.armor * ar
            + self.resist * 0.2
            + self.crit * cr
            + self.hit * hi
            + self.dodge * dg
    }

    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        let mut push = |v: f32, label: &str, pct: bool| {
            if v != 0.0 {
                if pct {
                    parts.push(format!("+{v}% {label}"));
                } else {
                    parts.push(format!("+{v:.0} {label}"));
                }
            }
        };
        push(self.stamina, "stamina", false);
        push(self.power, "power", false);
        push(self.armor, "armor", false);
        push(self.resist, "resist", false);
        push(self.crit, "crit", true);
        push(self.hit, "hit", true);
        push(self.dodge, "dodge", true);
        if parts.is_empty() {
            "no stats".into()
        } else {
            parts.join(", ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_slot_names_still_parse() {
        let w: ItemSlot = serde_json::from_str("\"weapon\"").unwrap();
        let a: ItemSlot = serde_json::from_str("\"armor\"").unwrap();
        assert_eq!(w, ItemSlot::MainHand);
        assert_eq!(a, ItemSlot::Chest);
    }

    #[test]
    fn rarity_scales_stats_and_upgrades_saturate() {
        let s = ItemStats {
            stamina: 20.0,
            crit: 1.0,
            ..Default::default()
        };
        let e = s.scaled(Rarity::Epic.stat_mult());
        assert_eq!(e.stamina, 38.0);
        assert!((e.crit - 1.9).abs() < 1e-5);
        assert_eq!(Rarity::Rare.upgraded(5), Rarity::Epic);
    }

    #[test]
    fn gear_applies_to_stats() {
        let mut st = Stats::default();
        ItemStats {
            armor: 100.0,
            crit: 2.0,
            dodge: 1.5,
            ..Default::default()
        }
        .apply(&mut st);
        assert_eq!(st.armor, 100.0);
        assert!((st.crit - 0.07).abs() < 1e-6);
        assert!((st.dodge - 0.015).abs() < 1e-6);
    }

    #[test]
    fn role_scoring_prefers_matching_stats() {
        let tanky = ItemStats {
            stamina: 20.0,
            dodge: 2.0,
            ..Default::default()
        };
        let dps = ItemStats {
            power: 8.0,
            crit: 2.0,
            ..Default::default()
        };
        assert!(tanky.score_for(Role::Tank) > dps.score_for(Role::Tank));
        assert!(dps.score_for(Role::MeleeDps) > tanky.score_for(Role::MeleeDps));
    }
}
