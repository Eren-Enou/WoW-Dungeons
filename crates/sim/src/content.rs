//! Data-driven dungeon content: schema, loading and validation.
//!
//! The content file is TOML (see `assets/content/dungeon.toml`). Unknown fields
//! are rejected so typos surface as errors instead of silently using defaults.

use crate::math::V2;
use crate::nav::{Nav, Rect};
use crate::profile::ItemSlot;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

/// Default location of the content file, relative to the working directory.
pub const DEFAULT_CONTENT_PATH: &str = "assets/content/dungeon.toml";
/// Environment variable that overrides the content file path.
pub const CONTENT_ENV_VAR: &str = "DUNGEON_CONTENT";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    pub dungeon: DungeonConfig,
    pub boss: BossConfig,
    pub templates: BTreeMap<String, EnemyTemplate>,
    pub encounters: Vec<EncounterConfig>,
    pub loot: Vec<LootEntry>,
    pub rewards: RewardConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DungeonConfig {
    pub name: String,
    /// Multiplies every enemy's max HP.
    pub hp_multiplier: f32,
    /// Multiplies all enemy damage.
    #[serde(default = "one")]
    pub damage_multiplier: f32,
    pub entrance: V2,
    /// Enemies further than this from their spawn point give up and reset.
    pub leash_radius: f32,
    /// Walkable floor, as overlapping axis-aligned rectangles.
    pub areas: Vec<AreaConfig>,
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaConfig {
    pub name: String,
    pub min: V2,
    pub max: V2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnemyRank {
    Trash,
    Elite,
    Boss,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnemyTemplate {
    pub name: String,
    pub rank: EnemyRank,
    pub hp: f32,
    pub melee_damage: f32,
    pub attack_interval: f32,
    #[serde(default = "default_attack_range")]
    pub attack_range: f32,
    pub aggro_radius: f32,
    pub speed: f32,
    pub radius: f32,
    /// Damage taken multiplier (armor). 1.0 = no mitigation.
    #[serde(default = "one")]
    pub damage_taken: f32,
    #[serde(default)]
    pub cleave: Option<CleaveConfig>,
}

fn default_attack_range() -> f32 {
    2.0
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleaveConfig {
    pub damage: f32,
    pub radius: f32,
    pub interval: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BossConfig {
    /// Template key of the boss in `[templates]`.
    pub template: String,
    /// Boss enters phase 2 when HP falls to or below this fraction.
    pub phase2_hp_fraction: f32,
    pub phase2_damage_multiplier: f32,
    /// Boss timer: seconds between Cinder Ring casts in phase 2.
    pub ring_interval: f32,
    /// Seconds of ground warning before a Cinder Ring detonates.
    pub ring_telegraph: f32,
    pub ring_radius: f32,
    pub ring_damage: f32,
    /// Phase-1 ranged spell on a random non-tank.
    pub bolt_damage: f32,
    pub bolt_interval: f32,
    pub bolt_cast_time: f32,
    /// After this many seconds of combat the boss enrages (x5 damage).
    pub enrage_seconds: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncounterConfig {
    pub id: String,
    pub mobs: Vec<MobSpawn>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MobSpawn {
    pub template: String,
    pub pos: V2,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LootEntry {
    pub id: String,
    pub name: String,
    pub slot: ItemSlot,
    #[serde(default)]
    pub power: u32,
    #[serde(default)]
    pub stamina: u32,
    pub weight: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RewardConfig {
    pub marks_per_clear: u32,
    #[serde(default)]
    pub marks_per_wipe: u32,
}

#[derive(Debug)]
pub enum ContentError {
    Io { path: String, message: String },
    Parse { origin: String, message: String },
    Invalid { origin: String, issues: Vec<String> },
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentError::Io { path, message } => {
                write!(f, "could not read content file '{path}': {message}")
            }
            ContentError::Parse { origin, message } => {
                write!(
                    f,
                    "content file '{origin}' is not valid TOML for the schema:\n{message}"
                )
            }
            ContentError::Invalid { origin, issues } => {
                writeln!(
                    f,
                    "content file '{origin}' has {} problem(s):",
                    issues.len()
                )?;
                for issue in issues {
                    writeln!(f, "  - {issue}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ContentError {}

impl Content {
    /// Path from `DUNGEON_CONTENT` or the default.
    pub fn default_path() -> String {
        std::env::var(CONTENT_ENV_VAR).unwrap_or_else(|_| DEFAULT_CONTENT_PATH.to_string())
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Content, ContentError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|e| ContentError::Io {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
        Content::from_toml_str(&text, &path.display().to_string())
    }

    pub fn from_toml_str(text: &str, origin: &str) -> Result<Content, ContentError> {
        let content: Content = toml::from_str(text).map_err(|e| ContentError::Parse {
            origin: origin.to_string(),
            message: e.to_string(),
        })?;
        content.validate(origin)?;
        Ok(content)
    }

    /// The built-in default content (the same file shipped in assets), used by tests.
    pub fn builtin() -> Content {
        Content::from_toml_str(BUILTIN_CONTENT, "<builtin>").expect("builtin content is valid")
    }

    pub fn nav(&self) -> Nav {
        Nav::new(
            self.dungeon
                .areas
                .iter()
                .map(|a| Rect::new(a.min, a.max))
                .collect(),
        )
    }

    /// Index of the encounter holding the boss (validated to be the last one).
    pub fn boss_encounter(&self) -> usize {
        self.encounters.len() - 1
    }

    pub fn validate(&self, origin: &str) -> Result<(), ContentError> {
        let mut issues = Vec::new();
        let mut check = |ok: bool, msg: String| {
            if !ok {
                issues.push(msg);
            }
        };

        let d = &self.dungeon;
        check(
            !d.name.trim().is_empty(),
            "dungeon.name must not be empty".into(),
        );
        check(
            d.hp_multiplier.is_finite() && (0.05..=20.0).contains(&d.hp_multiplier),
            format!(
                "dungeon.hp_multiplier = {} is out of range; use a value between 0.05 and 20.0 (1.0 = normal)",
                d.hp_multiplier
            ),
        );
        check(
            d.damage_multiplier.is_finite() && (0.0..=20.0).contains(&d.damage_multiplier),
            format!(
                "dungeon.damage_multiplier = {} is out of range; use a value between 0.0 and 20.0",
                d.damage_multiplier
            ),
        );
        check(
            d.leash_radius.is_finite() && d.leash_radius >= 5.0,
            format!(
                "dungeon.leash_radius = {} must be at least 5.0",
                d.leash_radius
            ),
        );
        check(
            !d.areas.is_empty(),
            "dungeon.areas must list at least one walkable rectangle".into(),
        );
        for (i, a) in d.areas.iter().enumerate() {
            check(
                a.min.is_finite() && a.max.is_finite() && a.min.x < a.max.x && a.min.z < a.max.z,
                format!(
                    "dungeon.areas[{i}] ('{}'): min {:?} must be smaller than max {:?} on both axes",
                    a.name,
                    <[f32; 2]>::from(a.min),
                    <[f32; 2]>::from(a.max)
                ),
            );
        }
        let nav = self.nav();
        if !d.areas.is_empty() {
            check(
                nav.is_connected(),
                "dungeon.areas are not all connected; neighbouring rectangles must overlap".into(),
            );
            check(
                nav.contains(d.entrance),
                format!(
                    "dungeon.entrance {:?} is not inside any walkable area",
                    <[f32; 2]>::from(d.entrance)
                ),
            );
        }

        for (key, t) in &self.templates {
            let p = format!("templates.{key}");
            check(
                t.hp.is_finite() && t.hp > 0.0,
                format!("{p}.hp must be > 0 (got {})", t.hp),
            );
            check(
                t.melee_damage.is_finite() && t.melee_damage >= 0.0,
                format!("{p}.melee_damage must be >= 0"),
            );
            check(
                t.attack_interval.is_finite() && t.attack_interval >= 0.2,
                format!(
                    "{p}.attack_interval must be >= 0.2 seconds (got {})",
                    t.attack_interval
                ),
            );
            check(
                t.attack_range.is_finite() && t.attack_range > 0.0,
                format!("{p}.attack_range must be > 0"),
            );
            check(
                t.aggro_radius.is_finite() && t.aggro_radius >= 0.0,
                format!("{p}.aggro_radius must be >= 0"),
            );
            check(
                t.speed.is_finite() && t.speed > 0.0 && t.speed <= 30.0,
                format!("{p}.speed must be in (0, 30] (got {})", t.speed),
            );
            check(
                (0.2..=5.0).contains(&t.radius),
                format!("{p}.radius must be between 0.2 and 5.0 (got {})", t.radius),
            );
            check(
                t.damage_taken.is_finite() && t.damage_taken > 0.0,
                format!("{p}.damage_taken must be > 0"),
            );
            if let Some(c) = &t.cleave {
                check(
                    c.damage >= 0.0 && c.radius > 0.0 && c.interval >= 1.0,
                    format!("{p}.cleave needs damage >= 0, radius > 0 and interval >= 1.0"),
                );
            }
        }

        let b = &self.boss;
        match self.templates.get(&b.template) {
            None => check(
                false,
                format!(
                    "boss.template '{}' is not defined in [templates]; known templates: {}",
                    b.template,
                    self.template_keys()
                ),
            ),
            Some(t) => check(
                t.rank == EnemyRank::Boss,
                format!("boss.template '{}' must have rank = \"boss\"", b.template),
            ),
        }
        check(
            b.phase2_hp_fraction > 0.0 && b.phase2_hp_fraction < 1.0,
            format!(
                "boss.phase2_hp_fraction = {} must be strictly between 0 and 1 (e.g. 0.5 for 50%)",
                b.phase2_hp_fraction
            ),
        );
        check(
            b.phase2_damage_multiplier.is_finite() && b.phase2_damage_multiplier >= 0.0,
            "boss.phase2_damage_multiplier must be >= 0".into(),
        );
        check(
            b.ring_interval.is_finite() && b.ring_interval >= 1.0,
            format!(
                "boss.ring_interval = {} must be at least 1.0 seconds",
                b.ring_interval
            ),
        );
        check(
            b.ring_telegraph.is_finite()
                && b.ring_telegraph >= 0.5
                && b.ring_telegraph < b.ring_interval,
            format!(
                "boss.ring_telegraph = {} must be >= 0.5 and smaller than boss.ring_interval ({})",
                b.ring_telegraph, b.ring_interval
            ),
        );
        check(
            b.ring_radius > 0.0 && b.ring_radius <= 20.0,
            "boss.ring_radius must be in (0, 20]".into(),
        );
        check(b.ring_damage >= 0.0, "boss.ring_damage must be >= 0".into());
        check(b.bolt_damage >= 0.0, "boss.bolt_damage must be >= 0".into());
        check(
            b.bolt_interval >= 1.0,
            "boss.bolt_interval must be >= 1.0".into(),
        );
        check(
            b.bolt_cast_time >= 0.0 && b.bolt_cast_time < b.bolt_interval,
            "boss.bolt_cast_time must be >= 0 and smaller than boss.bolt_interval".into(),
        );
        check(
            b.enrage_seconds >= 30.0,
            format!(
                "boss.enrage_seconds = {} must be at least 30",
                b.enrage_seconds
            ),
        );

        check(
            !self.encounters.is_empty(),
            "at least one [[encounters]] entry is required".into(),
        );
        let mut ids = BTreeSet::new();
        let mut boss_spawns = Vec::new();
        for (i, e) in self.encounters.iter().enumerate() {
            check(
                ids.insert(e.id.as_str()),
                format!("encounters[{i}].id '{}' is used more than once", e.id),
            );
            check(
                !e.mobs.is_empty(),
                format!("encounters[{i}] ('{}') has no mobs", e.id),
            );
            for (j, m) in e.mobs.iter().enumerate() {
                let p = format!("encounters[{i}].mobs[{j}]");
                match self.templates.get(&m.template) {
                    None => check(
                        false,
                        format!(
                            "{p}.template '{}' is not defined; known templates: {}",
                            m.template,
                            self.template_keys()
                        ),
                    ),
                    Some(t) => {
                        if t.rank == EnemyRank::Boss {
                            boss_spawns.push((i, m.template.clone()));
                        }
                    }
                }
                if !d.areas.is_empty() {
                    check(
                        nav.contains(m.pos),
                        format!(
                            "{p}.pos {:?} is outside every walkable area",
                            <[f32; 2]>::from(m.pos)
                        ),
                    );
                }
            }
        }
        check(
            boss_spawns.len() == 1,
            format!(
                "exactly one boss must be spawned across all encounters (found {})",
                boss_spawns.len()
            ),
        );
        if let Some((i, key)) = boss_spawns.first() {
            check(
                *i + 1 == self.encounters.len(),
                "the boss must be in the last encounter".into(),
            );
            check(
                *key == b.template,
                format!(
                    "spawned boss '{key}' does not match boss.template '{}'",
                    b.template
                ),
            );
            check(
                self.encounters[*i].mobs.len() == 1,
                "the boss encounter must contain only the boss".into(),
            );
        }

        check(
            !self.loot.is_empty(),
            "at least one [[loot]] entry is required".into(),
        );
        let mut loot_ids = BTreeSet::new();
        for (i, l) in self.loot.iter().enumerate() {
            check(
                loot_ids.insert(l.id.as_str()),
                format!("loot[{i}].id '{}' is used more than once", l.id),
            );
            check(l.weight > 0, format!("loot[{i}].weight must be > 0"));
            check(
                l.power <= 200 && l.stamina <= 500,
                format!("loot[{i}] stats are unreasonably large (power <= 200, stamina <= 500)"),
            );
        }

        if issues.is_empty() {
            Ok(())
        } else {
            Err(ContentError::Invalid {
                origin: origin.to_string(),
                issues,
            })
        }
    }

    fn template_keys(&self) -> String {
        self.templates
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The shipped content file, embedded for tests and as a fallback reference.
pub const BUILTIN_CONTENT: &str = include_str!("../../../assets/content/dungeon.toml");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_content_is_valid() {
        let c = Content::builtin();
        assert_eq!(c.encounters.len(), 4);
        assert_eq!(c.boss_encounter(), 3);
    }

    #[test]
    fn bad_multiplier_is_reported_with_path() {
        let text = BUILTIN_CONTENT.replace("hp_multiplier = 1.0", "hp_multiplier = -2.0");
        let err = Content::from_toml_str(&text, "test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("dungeon.hp_multiplier"), "{err}");
    }

    #[test]
    fn unknown_template_lists_known_ones() {
        let text = BUILTIN_CONTENT.replacen("template = \"skulker\"", "template = \"skulkr\"", 1);
        let err = Content::from_toml_str(&text, "test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("'skulkr' is not defined"), "{err}");
        assert!(err.contains("known templates"), "{err}");
    }

    #[test]
    fn typo_field_is_rejected() {
        let text = BUILTIN_CONTENT.replace("ring_interval", "ring_intervall");
        let err = Content::from_toml_str(&text, "test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("ring_interval"), "{err}");
    }

    #[test]
    fn telegraph_longer_than_interval_is_rejected() {
        let text = BUILTIN_CONTENT.replace("ring_telegraph = 2.5", "ring_telegraph = 50.0");
        let err = Content::from_toml_str(&text, "test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("boss.ring_telegraph"), "{err}");
    }
}
