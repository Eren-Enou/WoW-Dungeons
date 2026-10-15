//! Data-driven dungeon content: schema, loading and validation.
//!
//! Each dungeon is one TOML file in `assets/content/dungeons/`. Unknown fields are
//! rejected so typos surface as errors, and a validation pass reports every
//! problem with the path of the offending field.

use crate::items::{ItemSlot, ItemStats, Rarity, RoleAffinity};
use crate::math::{V2, V3};
use crate::nav::{Axis, LinkDef, LinkKind, Nav, NavParams, Rect, SurfaceDef, SurfaceId};
use crate::stats::School;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

/// Default content directory, relative to the working directory.
pub const DEFAULT_CONTENT_DIR: &str = "assets/content/dungeons";
/// Environment variable that overrides the content location (file or directory).
pub const CONTENT_ENV_VAR: &str = "DUNGEON_CONTENT";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    pub dungeon: DungeonConfig,
    #[serde(default)]
    pub difficulties: Vec<Difficulty>,
    pub templates: BTreeMap<String, EnemyTemplate>,
    pub encounters: Vec<EncounterConfig>,
    pub loot: Vec<LootEntry>,
    pub rewards: RewardConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DungeonConfig {
    /// Stable identifier (used in saves and on the command line).
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "one_u32")]
    pub recommended_level: u32,
    /// Group size the content is designed for (5 = party, 10 = raid).
    #[serde(default = "five_u32")]
    pub recommended_group: u32,
    /// Multiplies every enemy's max HP.
    pub hp_multiplier: f32,
    /// Multiplies all enemy damage.
    #[serde(default = "one")]
    pub damage_multiplier: f32,
    pub entrance: V2,
    /// Elevation of the entrance; needed only when several floors lie under it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrance_y: Option<f32>,
    /// Enemies further than this from their spawn point give up and reset.
    pub leash_radius: f32,
    /// Walkable surfaces: axis-aligned rectangles, optionally raised or sloped.
    /// Areas at the same height that overlap are connected; nothing else is.
    pub areas: Vec<AreaConfig>,
    /// Authored connections (one-way drops).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<LinkConfig>,
    /// Navigation limits (step, slope, drop, headroom).
    #[serde(default, skip_serializing_if = "NavConfig::is_default")]
    pub nav: NavConfig,
    /// Decorative lava pools (not walkable).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lava: Vec<LavaConfig>,
    #[serde(default)]
    pub theme: Theme,
}

fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

fn is_false(v: &bool) -> bool {
    !*v
}

fn is_true(v: &bool) -> bool {
    *v
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AxisConfig {
    X,
    #[default]
    Z,
}

impl AxisConfig {
    fn is_default(&self) -> bool {
        *self == AxisConfig::Z
    }
}

/// What is below a raised area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UnderConfig {
    /// Rock down to the lowest floor (blocks sight and movement below).
    #[default]
    Solid,
    /// A thin deck you can walk and see under (bridges, walkways).
    Open,
}

impl UnderConfig {
    fn is_solid(&self) -> bool {
        *self == UnderConfig::Solid
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkKindConfig {
    /// One-way: step off the `from` area's edge inside the zone and fall.
    Drop,
    /// Assert that two areas connect by walking (they must already be level
    /// where they overlap); a typo or height mistake becomes an error.
    Walk,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LinkConfig {
    pub from: String,
    pub to: String,
    pub kind: LinkKindConfig,
    /// Drop zone (required for drops): covers the edge of `from` and the floor
    /// of `to` right beside or below it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<V2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<V2>,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct NavConfig {
    pub max_step: f32,
    pub max_slope: f32,
    pub max_drop: f32,
    pub headroom: f32,
}

impl Default for NavConfig {
    fn default() -> Self {
        let p = NavParams::default();
        NavConfig {
            max_step: p.max_step,
            max_slope: p.max_slope,
            max_drop: p.max_drop,
            headroom: p.headroom,
        }
    }
}

impl NavConfig {
    fn is_default(&self) -> bool {
        *self == NavConfig::default()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LavaConfig {
    pub min: V2,
    pub max: V2,
    #[serde(default)]
    pub y: f32,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct Theme {
    pub floor: [f32; 3],
    pub wall: [f32; 3],
    pub light: [f32; 3],
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            floor: [0.22, 0.24, 0.27],
            wall: [0.32, 0.3, 0.34],
            light: [1.0, 0.75, 0.45],
        }
    }
}

fn one() -> f32 {
    1.0
}

fn one_u32() -> u32 {
    1
}

fn five_u32() -> u32 {
    5
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Difficulty {
    pub id: String,
    pub name: String,
    #[serde(default = "one")]
    pub hp: f32,
    #[serde(default = "one")]
    pub damage: f32,
    /// Added to every enemy's level (affects hit, crit and mitigation tables).
    #[serde(default)]
    pub level_bonus: u32,
    /// Multiplies marks and experience.
    #[serde(default = "one")]
    pub reward_multiplier: f32,
    /// Loot rarity upgrade steps.
    #[serde(default)]
    pub rarity_bonus: u32,
}

impl Difficulty {
    pub fn normal() -> Difficulty {
        Difficulty {
            id: "normal".into(),
            name: "Normal".into(),
            hp: 1.0,
            damage: 1.0,
            level_bonus: 0,
            reward_multiplier: 1.0,
            rarity_bonus: 0,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AreaConfig {
    pub name: String,
    pub min: V2,
    pub max: V2,
    /// Floor height (at the `min` edge for a ramp). Default 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub y: f32,
    /// Makes the area a ramp: height at the `max` edge along `axis`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_to: Option<f32>,
    #[serde(default, skip_serializing_if = "AxisConfig::is_default")]
    pub axis: AxisConfig,
    /// Draw a ramp as steps.
    #[serde(default, skip_serializing_if = "is_false")]
    pub stairs: bool,
    /// Outline gets walls (default true; bridges and ledges usually false).
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub walls: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall_height: Option<f32>,
    #[serde(default, skip_serializing_if = "UnderConfig::is_solid")]
    pub under: UnderConfig,
    /// Intentionally unreachable from the entrance (e.g. a sniper perch).
    #[serde(default, skip_serializing_if = "is_false")]
    pub unreachable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EnemyRank {
    Trash,
    Elite,
    Boss,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnemyTemplate {
    pub name: String,
    pub rank: EnemyRank,
    #[serde(default = "one_u32")]
    pub level: u32,
    pub hp: f32,
    pub melee_damage: f32,
    pub attack_interval: f32,
    #[serde(default = "default_attack_range")]
    pub attack_range: f32,
    pub aggro_radius: f32,
    pub speed: f32,
    pub radius: f32,
    #[serde(default)]
    pub armor: f32,
    #[serde(default)]
    pub resist: f32,
    #[serde(default = "default_dodge")]
    pub dodge: f32,
    #[serde(default)]
    pub parry: f32,
    #[serde(default)]
    pub block: f32,
    #[serde(default = "default_crit")]
    pub crit: f32,
    /// Immune to incapacitate effects. Defaults to true for bosses.
    #[serde(default)]
    pub cc_immune: Option<bool>,
    /// Damage x5 after this many seconds in combat.
    #[serde(default)]
    pub enrage_seconds: Option<f32>,
    #[serde(default)]
    pub abilities: Vec<EnemyAbility>,
    /// Phase transitions in order of decreasing `hp_below`.
    #[serde(default)]
    pub phases: Vec<PhaseConfig>,
}

impl EnemyTemplate {
    pub fn is_cc_immune(&self) -> bool {
        self.cc_immune.unwrap_or(self.rank == EnemyRank::Boss)
    }
}

fn default_attack_range() -> f32 {
    2.0
}

fn default_dodge() -> f32 {
    0.05
}

fn default_crit() -> f32 {
    0.05
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityKind {
    /// Cast (optionally) then hit one target with spell damage.
    Bolt,
    /// Instant physical damage to everyone near the caster (optionally frontal only).
    Cleave,
    /// Ground telegraph(s) under target(s) that detonate after `delay`.
    Ring,
    /// Damage to the whole party after the cast (needs healing / defensives).
    Pulse,
    /// Summon `count` enemies of `template` next to the caster.
    Summon,
    /// Apply an aura (debuff) to the target.
    Debuff,
    /// Heal the most injured allied enemy (interrupt it!).
    HealAlly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TargetSelector {
    /// The enemy's current (threat) target.
    #[default]
    Current,
    Random,
    RandomNonTank,
    Farthest,
    LowestHealth,
    #[serde(rename = "self")]
    Caster,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnemyAbility {
    pub name: String,
    pub kind: AbilityKind,
    /// Seconds between uses.
    pub interval: f32,
    /// Seconds after pull (or phase start) before the first use. Defaults to `interval`.
    #[serde(default)]
    pub initial: Option<f32>,
    #[serde(default)]
    pub cast_time: f32,
    #[serde(default)]
    pub interruptible: bool,
    /// Phases (1-based) in which the ability is used. Empty = every phase.
    #[serde(default)]
    pub phases: Vec<u8>,
    #[serde(default)]
    pub target: TargetSelector,
    #[serde(default)]
    pub damage: f32,
    #[serde(default)]
    pub school: Option<School>,
    #[serde(default)]
    pub radius: f32,
    /// Cleave: only hits targets in front of the caster.
    #[serde(default)]
    pub frontal: bool,
    /// Ring: seconds of warning before detonation.
    #[serde(default)]
    pub delay: f32,
    /// Ring: number of rings (each on a different target); Summon: number of adds.
    #[serde(default)]
    pub count: u32,
    /// Summon: template key.
    #[serde(default)]
    pub template: Option<String>,
    /// HealAlly: amount healed.
    #[serde(default)]
    pub amount: f32,
    /// Debuff: the aura applied.
    #[serde(default)]
    pub aura: Option<AuraConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DebuffEffect {
    /// `value` damage per second.
    Dot,
    /// +`value` (fraction) damage taken per stack.
    DamageTaken,
    /// −`value` (fraction) healing received per stack.
    HealingTaken,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuraConfig {
    pub name: String,
    pub effect: DebuffEffect,
    pub value: f32,
    pub duration: f32,
    #[serde(default = "one_u32")]
    pub max_stacks: u32,
    #[serde(default = "yes")]
    pub dispellable: bool,
    #[serde(default)]
    pub school: Option<School>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PhaseConfig {
    /// Enter this phase when HP fraction falls to or below this value.
    pub hp_below: f32,
    #[serde(default = "one")]
    pub damage_multiplier: f32,
    #[serde(default)]
    pub announce: String,
    #[serde(default)]
    pub summon: Vec<SummonSpec>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SummonSpec {
    pub template: String,
    #[serde(default = "one_u32")]
    pub count: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EncounterConfig {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// Optional encounters are not required for victory (but drop their loot).
    #[serde(default)]
    pub optional: bool,
    pub mobs: Vec<MobSpawn>,
    #[serde(default)]
    pub hazards: Vec<HazardConfig>,
}

impl EncounterConfig {
    pub fn display_name(&self) -> &str {
        if self.name.is_empty() {
            &self.id
        } else {
            &self.name
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MobSpawn {
    pub template: String,
    pub pos: V2,
    /// Elevation; needed only when several floors lie under `pos`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
}

/// An environmental hazard that pulses on and off while its encounter is alive.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HazardConfig {
    pub name: String,
    pub pos: V2,
    /// Elevation; needed only when several floors lie under `pos`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    pub radius: f32,
    /// Damage per second to party members inside while active.
    pub damage: f32,
    #[serde(default)]
    pub school: Option<School>,
    /// Seconds active, then seconds dormant.
    pub on: f32,
    pub off: f32,
    #[serde(default)]
    pub offset: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LootEntry {
    pub id: String,
    pub name: String,
    pub slot: ItemSlot,
    #[serde(default)]
    pub rarity: Rarity,
    #[serde(default)]
    pub role: RoleAffinity,
    pub weight: u32,
    /// Encounter that drops this item; default: the final boss.
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub stats: ItemStats,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RewardConfig {
    pub marks_per_clear: u32,
    #[serde(default)]
    pub marks_per_wipe: u32,
    /// Experience for clearing the dungeon (kills give extra).
    #[serde(default)]
    pub xp_per_clear: u32,
}

/// One change made in the in-game encounter editor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContentEdit {
    /// Multiply max HP.
    Hp(f32),
    /// Multiply melee damage.
    Melee(f32),
    /// Add seconds to ability `i`'s interval.
    Interval(usize, f32),
    /// Multiply ability `i`'s damage (and heal amount).
    Damage(usize, f32),
    /// Add seconds to ability `i`'s cast time.
    CastTime(usize, f32),
    /// Add seconds to ring ability `i`'s warning delay.
    Delay(usize, f32),
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
    pub fn load(path: impl AsRef<Path>) -> Result<Content, ContentError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|e| ContentError::Io {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
        Content::from_toml_str(&text, &path.display().to_string())
    }

    pub fn from_toml_str(text: &str, origin: &str) -> Result<Content, ContentError> {
        let mut content: Content = toml::from_str(text).map_err(|e| ContentError::Parse {
            origin: origin.to_string(),
            message: e.to_string(),
        })?;
        if content.difficulties.is_empty() {
            content.difficulties.push(Difficulty::normal());
        }
        content.validate(origin)?;
        Ok(content)
    }

    /// Serialize back to TOML (used by the in-game encounter editor).
    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string_pretty(self).map_err(|e| e.to_string())
    }

    /// Templates worth editing in the in-game editor: bosses then elites.
    pub fn editable_templates(&self) -> Vec<String> {
        let mut keys: Vec<(u8, String)> = self
            .templates
            .iter()
            .filter_map(|(k, t)| match t.rank {
                EnemyRank::Boss => Some((0, k.clone())),
                EnemyRank::Elite => Some((1, k.clone())),
                EnemyRank::Trash => None,
            })
            .collect();
        keys.sort();
        keys.into_iter().map(|(_, k)| k).collect()
    }

    /// Apply one editor change, clamped so the content stays valid.
    pub fn apply_edit(&mut self, template: &str, edit: ContentEdit) {
        let Some(t) = self.templates.get_mut(template) else {
            return;
        };
        match edit {
            ContentEdit::Hp(f) => t.hp = (t.hp * f).clamp(1.0, 10_000_000.0).round(),
            ContentEdit::Melee(f) => {
                t.melee_damage = (t.melee_damage * f).clamp(0.0, 100_000.0).round()
            }
            ContentEdit::Interval(i, d) => {
                if let Some(a) = t.abilities.get_mut(i) {
                    a.interval = (a.interval + d).clamp(1.0, 600.0);
                    a.cast_time = a.cast_time.min(a.interval * 0.9);
                    if a.delay > 0.0 {
                        a.delay = a.delay.min(a.interval * 0.9).max(0.5);
                    }
                }
            }
            ContentEdit::Damage(i, f) => {
                if let Some(a) = t.abilities.get_mut(i) {
                    a.damage = (a.damage * f)
                        .round()
                        .max(if a.damage > 0.0 { 1.0 } else { 0.0 });
                    a.amount = (a.amount * f).round();
                }
            }
            ContentEdit::CastTime(i, d) => {
                if let Some(a) = t.abilities.get_mut(i) {
                    a.cast_time = (a.cast_time + d).clamp(0.0, a.interval * 0.9);
                }
            }
            ContentEdit::Delay(i, d) => {
                if let Some(a) = t.abilities.get_mut(i)
                    && a.kind == AbilityKind::Ring
                {
                    a.delay = (a.delay + d).clamp(0.5, a.interval * 0.9);
                }
            }
        }
    }

    /// Turn this content into a "custom" copy with its own id and name.
    pub fn as_custom(&self) -> Content {
        let mut c = self.clone();
        if !c.dungeon.id.ends_with("_custom") {
            c.dungeon.id = format!("{}_custom", c.dungeon.id);
            c.dungeon.name = format!("{} (Custom)", c.dungeon.name);
        }
        c
    }

    /// The shipped Sunken Vault (used by tests).
    pub fn builtin() -> Content {
        Content::from_toml_str(BUILTIN_SUNKEN_VAULT, "<builtin sunken_vault>")
            .expect("builtin content is valid")
    }

    /// The shipped Gloomroot Warrens (used by tests).
    pub fn builtin_warrens() -> Content {
        Content::from_toml_str(BUILTIN_WARRENS, "<builtin gloomroot_warrens>")
            .expect("builtin content is valid")
    }

    /// The shipped Cavern Traverse navigation test dungeon (used by tests).
    pub fn builtin_cavern() -> Content {
        Content::from_toml_str(BUILTIN_CAVERN, "<builtin cavern_traverse>")
            .expect("builtin content is valid")
    }

    /// The navigation for this (validated) content.
    pub fn nav(&self) -> Nav {
        self.build_nav()
            .expect("content was validated when it was loaded")
    }

    /// Build and validate the navigation from `dungeon.areas` / `dungeon.links`.
    pub fn build_nav(&self) -> Result<Nav, Vec<String>> {
        let d = &self.dungeon;
        let defs = d
            .areas
            .iter()
            .map(|a| SurfaceDef {
                name: a.name.clone(),
                rect: Rect::new(a.min, a.max),
                y: a.y,
                y_to: a.y_to,
                axis: match a.axis {
                    AxisConfig::X => Axis::X,
                    AxisConfig::Z => Axis::Z,
                },
                stairs: a.stairs,
                walls: a.walls,
                wall_height: a.wall_height.unwrap_or(crate::nav::WALL_HEIGHT),
                open_under: a.under == UnderConfig::Open,
                unreachable: a.unreachable,
            })
            .collect();
        let mut errors = Vec::new();
        let links = d
            .links
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                let zone = match (l.kind, l.min, l.max) {
                    (LinkKindConfig::Drop, Some(min), Some(max)) => Rect::new(min, max),
                    (LinkKindConfig::Drop, _, _) => {
                        errors.push(format!(
                            "dungeon.links[{i}] ('{}' -> '{}'): a drop needs min and max (the drop zone)",
                            l.from, l.to
                        ));
                        return None;
                    }
                    (LinkKindConfig::Walk, _, _) => Rect::new(V2::ZERO, V2::ZERO),
                };
                Some(LinkDef {
                    from: l.from.clone(),
                    to: l.to.clone(),
                    kind: match l.kind {
                        LinkKindConfig::Drop => LinkKind::Drop,
                        LinkKindConfig::Walk => LinkKind::Walk,
                    },
                    zone,
                })
            })
            .collect();
        let params = NavParams {
            max_step: d.nav.max_step,
            max_slope: d.nav.max_slope,
            max_drop: d.nav.max_drop,
            headroom: d.nav.headroom,
        };
        if !(0.05..=2.0).contains(&params.max_step)
            || !(0.05..=2.0).contains(&params.max_slope)
            || !(0.5..=50.0).contains(&params.max_drop)
            || !(1.5..=20.0).contains(&params.headroom)
        {
            errors.push(
                "dungeon.nav: max_step must be in 0.05..=2, max_slope in 0.05..=2, max_drop in 0.5..=50, headroom in 1.5..=20".into(),
            );
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        Nav::build(defs, links, params)
    }

    /// Resolve a horizontal position (and optional elevation) onto a surface.
    /// Without `y`, every floor under `pos` must be at the same height,
    /// otherwise the position is ambiguous.
    pub fn place(nav: &Nav, pos: V2, y: Option<f32>) -> Result<(V3, SurfaceId), String> {
        let under: Vec<(SurfaceId, f32)> = nav
            .surfaces
            .iter()
            .enumerate()
            .filter(|(_, s)| s.walk.contains(pos))
            .map(|(i, s)| (i as SurfaceId, s.height_at(pos)))
            .collect();
        if under.is_empty() {
            return Err("is outside every walkable area".into());
        }
        match y {
            Some(y) => {
                let s = nav.locate(pos, y).expect("pos is on a surface");
                let h = nav.height(s, pos);
                if (h - y).abs() > 0.6 {
                    let floors: Vec<String> =
                        under.iter().map(|(_, h)| format!("{h:.1}")).collect();
                    return Err(format!(
                        "has y = {y} but the floors there are at y = {}",
                        floors.join(", ")
                    ));
                }
                Ok((V3::at(pos, h), s))
            }
            None => {
                let (s, h) = under[0];
                if under
                    .iter()
                    .any(|(_, o)| (o - h).abs() > nav.params.max_step)
                {
                    let floors: Vec<String> =
                        under.iter().map(|(_, h)| format!("{h:.1}")).collect();
                    return Err(format!(
                        "is above several floors (y = {}); add y = <height> to say which one",
                        floors.join(", ")
                    ));
                }
                Ok((V3::at(pos, h), s))
            }
        }
    }

    /// Index of the final (required) encounter, which holds the final boss.
    pub fn final_encounter(&self) -> usize {
        self.encounters
            .iter()
            .rposition(|e| !e.optional)
            .unwrap_or(self.encounters.len() - 1)
    }

    pub fn difficulty(&self, id: &str) -> Option<&Difficulty> {
        self.difficulties.iter().find(|d| d.id == id)
    }

    pub fn encounter_has_boss(&self, i: usize) -> bool {
        self.encounters[i]
            .mobs
            .iter()
            .any(|m| self.templates.get(&m.template).map(|t| t.rank) == Some(EnemyRank::Boss))
    }

    pub fn validate(&self, origin: &str) -> Result<(), ContentError> {
        let mut issues = Vec::new();
        let mut check = |ok: bool, msg: String| {
            if !ok {
                issues.push(msg);
            }
        };
        let known = self.template_keys();

        let d = &self.dungeon;
        check(
            !d.id.trim().is_empty() && d.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            format!(
                "dungeon.id '{}' must be non-empty and use only a-z, 0-9 and _",
                d.id
            ),
        );
        check(
            !d.name.trim().is_empty(),
            "dungeon.name must not be empty".into(),
        );
        check(
            (1..=crate::class::MAX_LEVEL).contains(&d.recommended_level),
            format!(
                "dungeon.recommended_level = {} must be between 1 and {}",
                d.recommended_level,
                crate::class::MAX_LEVEL
            ),
        );
        check(
            (1..=40).contains(&d.recommended_group),
            format!(
                "dungeon.recommended_group = {} must be between 1 and 40",
                d.recommended_group
            ),
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
        for (i, l) in d.lava.iter().enumerate() {
            check(
                Rect::new(l.min, l.max).is_valid() && l.y.is_finite(),
                format!("dungeon.lava[{i}]: min must be smaller than max on both axes"),
            );
        }
        let nav = match self.build_nav() {
            Ok(nav) => Some(nav),
            Err(errs) => {
                for e in errs {
                    check(false, format!("dungeon.areas/links: {e}"));
                }
                None
            }
        };
        if let Some(nav) = &nav {
            match Content::place(nav, d.entrance, d.entrance_y) {
                Err(e) => check(
                    false,
                    format!("dungeon.entrance {:?} {e}", <[f32; 2]>::from(d.entrance)),
                ),
                Ok((_, start)) => {
                    for (i, s) in nav.surfaces.iter().enumerate() {
                        check(
                            s.def.unreachable || nav.reachable(start, i as SurfaceId),
                            if nav
                                .surfaces
                                .iter()
                                .all(|s| s.min_y() == 0.0 && !s.is_ramp())
                            {
                                "dungeon.areas are not all connected; neighbouring rectangles must overlap".into()
                            } else {
                                format!(
                                    "dungeon.areas['{}'] cannot be reached from the entrance; connect it with a ramp or a drop, or mark it unreachable = true if that is intended",
                                    s.name()
                                )
                            },
                        );
                    }
                }
            }
        }

        let mut diff_ids = BTreeSet::new();
        for (i, df) in self.difficulties.iter().enumerate() {
            let p = format!("difficulties[{i}] ('{}')", df.id);
            check(
                diff_ids.insert(df.id.as_str()),
                format!("{p}: id is used more than once"),
            );
            check(
                (0.05..=20.0).contains(&df.hp) && (0.0..=20.0).contains(&df.damage),
                format!("{p}: hp must be in 0.05..=20 and damage in 0..=20"),
            );
            check(
                df.level_bonus <= 5,
                format!("{p}: level_bonus must be at most 5"),
            );
            check(
                (0.0..=10.0).contains(&df.reward_multiplier),
                format!("{p}: reward_multiplier must be in 0..=10"),
            );
        }

        for (key, t) in &self.templates {
            let p = format!("templates.{key}");
            check(
                t.hp.is_finite() && t.hp > 0.0,
                format!("{p}.hp must be > 0 (got {})", t.hp),
            );
            check(
                (1..=crate::class::MAX_LEVEL + 5).contains(&t.level),
                format!(
                    "{p}.level must be between 1 and {}",
                    crate::class::MAX_LEVEL + 5
                ),
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
                t.attack_range > 0.0,
                format!("{p}.attack_range must be > 0"),
            );
            check(
                t.aggro_radius >= 0.0,
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
                t.armor >= 0.0 && t.resist >= 0.0,
                format!("{p}.armor and resist must be >= 0"),
            );
            for (name, v) in [
                ("dodge", t.dodge),
                ("parry", t.parry),
                ("block", t.block),
                ("crit", t.crit),
            ] {
                check(
                    (0.0..=0.5).contains(&v),
                    format!(
                        "{p}.{name} = {v} must be a fraction between 0 and 0.5 (e.g. 0.05 = 5%)"
                    ),
                );
            }
            if let Some(e) = t.enrage_seconds {
                check(
                    e >= 30.0,
                    format!("{p}.enrage_seconds = {e} must be at least 30"),
                );
            }
            let mut last = 1.0;
            for (i, ph) in t.phases.iter().enumerate() {
                let pp = format!("{p}.phases[{i}]");
                check(
                    ph.hp_below > 0.0 && ph.hp_below < last,
                    format!(
                        "{pp}.hp_below = {} must be between 0 and 1 and lower than the previous phase ({last})",
                        ph.hp_below
                    ),
                );
                last = ph.hp_below;
                check(
                    ph.damage_multiplier >= 0.0,
                    format!("{pp}.damage_multiplier must be >= 0"),
                );
                for (j, s) in ph.summon.iter().enumerate() {
                    check(
                        self.templates.contains_key(&s.template),
                        format!(
                            "{pp}.summon[{j}].template '{}' is not defined; known templates: {known}",
                            s.template
                        ),
                    );
                    check(
                        (1..=10).contains(&s.count),
                        format!("{pp}.summon[{j}].count must be 1..=10"),
                    );
                }
            }
            let phase_count = t.phases.len() as u8 + 1;
            for (i, a) in t.abilities.iter().enumerate() {
                let ap = format!("{p}.abilities[{i}] ('{}')", a.name);
                check(
                    a.interval.is_finite() && a.interval >= 1.0,
                    format!(
                        "{ap}: interval = {} must be at least 1.0 seconds",
                        a.interval
                    ),
                );
                if let Some(init) = a.initial {
                    check(init >= 0.0, format!("{ap}: initial must be >= 0"));
                }
                check(
                    a.cast_time >= 0.0 && a.cast_time < a.interval,
                    format!(
                        "{ap}: cast_time = {} must be >= 0 and smaller than interval ({})",
                        a.cast_time, a.interval
                    ),
                );
                for ph in &a.phases {
                    check(
                        *ph >= 1 && *ph <= phase_count,
                        format!(
                            "{ap}: phase {ph} does not exist (this enemy has {phase_count} phase(s))"
                        ),
                    );
                }
                check(a.damage >= 0.0, format!("{ap}: damage must be >= 0"));
                match a.kind {
                    AbilityKind::Bolt | AbilityKind::Pulse => {
                        check(
                            a.damage > 0.0,
                            format!("{ap}: {:?} needs damage > 0", a.kind),
                        );
                    }
                    AbilityKind::Cleave => {
                        check(
                            a.damage > 0.0 && a.radius > 0.0,
                            format!("{ap}: cleave needs damage > 0 and radius > 0"),
                        );
                    }
                    AbilityKind::Ring => {
                        check(
                            a.damage > 0.0 && a.radius > 0.0 && a.radius <= 20.0,
                            format!("{ap}: ring needs damage > 0 and radius in (0, 20]"),
                        );
                        check(
                            a.delay >= 0.5 && a.delay < a.interval,
                            format!(
                                "{ap}: ring delay = {} must be >= 0.5 and smaller than interval ({})",
                                a.delay, a.interval
                            ),
                        );
                        check(a.count <= 5, format!("{ap}: ring count must be at most 5"));
                    }
                    AbilityKind::Summon => match &a.template {
                        Some(k) => {
                            check(
                                self.templates.contains_key(k),
                                format!(
                                    "{ap}: summon template '{k}' is not defined; known templates: {known}"
                                ),
                            );
                            check(
                                (1..=10).contains(&a.count),
                                format!("{ap}: summon count must be 1..=10"),
                            );
                        }
                        None => check(false, format!("{ap}: summon needs a `template`")),
                    },
                    AbilityKind::Debuff => match &a.aura {
                        Some(aura) => {
                            check(
                                aura.duration > 0.0 && aura.value > 0.0,
                                format!("{ap}: aura needs duration > 0 and value > 0"),
                            );
                            check(
                                (1..=20).contains(&aura.max_stacks),
                                format!("{ap}: aura max_stacks must be 1..=20"),
                            );
                        }
                        None => check(false, format!("{ap}: debuff needs an `aura` table")),
                    },
                    AbilityKind::HealAlly => {
                        check(a.amount > 0.0, format!("{ap}: heal_ally needs amount > 0"));
                    }
                }
            }
        }

        check(
            !self.encounters.is_empty(),
            "at least one [[encounters]] entry is required".into(),
        );
        let mut ids = BTreeSet::new();
        let mut boss_count = 0;
        for (i, e) in self.encounters.iter().enumerate() {
            let p = format!("encounters[{i}] ('{}')", e.id);
            check(
                ids.insert(e.id.as_str()),
                format!("{p}: id is used more than once"),
            );
            check(!e.mobs.is_empty(), format!("{p}: has no mobs"));
            for (j, m) in e.mobs.iter().enumerate() {
                let mp = format!("encounters[{i}].mobs[{j}]");
                match self.templates.get(&m.template) {
                    None => check(
                        false,
                        format!(
                            "{mp}.template '{}' is not defined; known templates: {known}",
                            m.template
                        ),
                    ),
                    Some(t) => {
                        if t.rank == EnemyRank::Boss {
                            boss_count += 1;
                        }
                    }
                }
                if let Some(nav) = &nav
                    && let Err(e) = Content::place(nav, m.pos, m.y)
                {
                    check(false, format!("{mp}.pos {:?} {e}", <[f32; 2]>::from(m.pos)));
                }
            }
            for (j, h) in e.hazards.iter().enumerate() {
                let hp = format!("{p}.hazards[{j}] ('{}')", h.name);
                check(
                    h.radius > 0.0
                        && h.damage >= 0.0
                        && h.on > 0.0
                        && h.off >= 0.0
                        && h.offset >= 0.0,
                    format!("{hp}: needs radius > 0, damage >= 0, on > 0, off >= 0, offset >= 0"),
                );
                if let Some(nav) = &nav
                    && let Err(e) = Content::place(nav, h.pos, h.y)
                {
                    check(false, format!("{hp}.pos {:?} {e}", <[f32; 2]>::from(h.pos)));
                }
            }
        }
        check(
            boss_count >= 1,
            "the dungeon must spawn at least one boss".into(),
        );
        if !self.encounters.is_empty() {
            let fin = self.final_encounter();
            check(
                !self.encounters.iter().all(|e| e.optional),
                "at least one encounter must be required (not optional)".into(),
            );
            check(
                self.encounter_has_boss(fin),
                format!(
                    "the last required encounter ('{}') must contain a boss",
                    self.encounters[fin].id
                ),
            );
        }

        check(
            !self.loot.is_empty(),
            "at least one [[loot]] entry is required".into(),
        );
        let mut loot_ids = BTreeSet::new();
        for (i, l) in self.loot.iter().enumerate() {
            let p = format!("loot[{i}] ('{}')", l.id);
            check(
                loot_ids.insert(l.id.as_str()),
                format!("{p}: id is used more than once"),
            );
            check(l.weight > 0, format!("{p}: weight must be > 0"));
            let s = &l.stats;
            check(
                s.stamina <= 300.0
                    && s.power <= 100.0
                    && s.armor <= 3000.0
                    && s.crit <= 10.0
                    && s.hit <= 10.0
                    && s.dodge <= 10.0,
                format!(
                    "{p}: stats are unreasonably large (stamina <= 300, power <= 100, armor <= 3000, crit/hit/dodge <= 10)"
                ),
            );
            if let Some(src) = &l.source {
                check(
                    self.encounters.iter().any(|e| e.id == *src),
                    format!("{p}: source '{src}' is not an encounter id"),
                );
            }
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

/// All dungeons found in a directory (or a single file), plus files that failed.
#[derive(Debug, Default)]
pub struct ContentLibrary {
    pub dungeons: Vec<Content>,
    pub errors: Vec<ContentError>,
    pub source: String,
}

impl ContentLibrary {
    /// Content location from `DUNGEON_CONTENT` or the default directory.
    pub fn default_path() -> String {
        std::env::var(CONTENT_ENV_VAR).unwrap_or_else(|_| DEFAULT_CONTENT_DIR.to_string())
    }

    /// Load every `*.toml` in a directory (sorted by file name), or one file.
    pub fn load(path: impl AsRef<Path>) -> ContentLibrary {
        let path = path.as_ref();
        let mut lib = ContentLibrary {
            source: path.display().to_string(),
            ..Default::default()
        };
        let files: Vec<PathBuf> = if path.is_dir() {
            match std::fs::read_dir(path) {
                Ok(rd) => {
                    let mut v: Vec<PathBuf> = rd
                        .filter_map(|e| e.ok().map(|e| e.path()))
                        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
                        .collect();
                    v.sort();
                    v
                }
                Err(e) => {
                    lib.errors.push(ContentError::Io {
                        path: lib.source.clone(),
                        message: e.to_string(),
                    });
                    return lib;
                }
            }
        } else {
            vec![path.to_path_buf()]
        };
        for f in files {
            match Content::load(&f) {
                Ok(c) => {
                    if lib.dungeons.iter().any(|d| d.dungeon.id == c.dungeon.id) {
                        lib.errors.push(ContentError::Invalid {
                            origin: f.display().to_string(),
                            issues: vec![format!(
                                "dungeon.id '{}' is already used by another file",
                                c.dungeon.id
                            )],
                        });
                    } else {
                        lib.dungeons.push(c);
                    }
                }
                Err(e) => lib.errors.push(e),
            }
        }
        if lib.dungeons.is_empty() && lib.errors.is_empty() {
            lib.errors.push(ContentError::Io {
                path: lib.source.clone(),
                message: "no dungeon .toml files found".into(),
            });
        }
        lib
    }

    pub fn get(&self, id: &str) -> Option<&Content> {
        self.dungeons.iter().find(|d| d.dungeon.id == id)
    }
}

/// The shipped content files, embedded for tests.
pub const BUILTIN_CAVERN: &str =
    include_str!("../../../assets/content/dungeons/05_cavern_traverse.toml");
pub const BUILTIN_SUNKEN_VAULT: &str =
    include_str!("../../../assets/content/dungeons/01_sunken_vault.toml");
pub const BUILTIN_WARRENS: &str =
    include_str!("../../../assets/content/dungeons/02_gloomroot_warrens.toml");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_content_is_valid() {
        let c = Content::builtin();
        assert_eq!(c.encounters.len(), 4);
        assert_eq!(c.final_encounter(), 3);
        let w = Content::builtin_warrens();
        let bosses = (0..w.encounters.len())
            .filter(|&i| w.encounter_has_boss(i))
            .count();
        assert!(bosses >= 2, "second dungeon needs two bosses");
        assert!(w.encounters.iter().any(|e| e.optional));
        assert!(w.difficulties.len() >= 2);
    }

    #[test]
    fn content_round_trips_through_toml() {
        for c in [Content::builtin(), Content::builtin_warrens()] {
            let text = c.to_toml().expect("serializes");
            let back = Content::from_toml_str(&text, "roundtrip").expect("re-parses and validates");
            assert_eq!(back.to_toml().unwrap(), text, "stable round trip");
            assert_eq!(back.encounters.len(), c.encounters.len());
        }
    }

    #[test]
    fn editor_changes_stay_valid_and_save_as_a_custom_dungeon() {
        let mut c = Content::builtin().as_custom();
        assert_eq!(c.dungeon.id, "sunken_vault_custom");
        assert_eq!(c.editable_templates(), vec!["hollow_king", "brute"]);
        let ring = c.templates["hollow_king"]
            .abilities
            .iter()
            .position(|a| a.kind == AbilityKind::Ring)
            .unwrap();
        // Hammer the interval down: delay and cast time follow so it stays valid.
        for _ in 0..30 {
            c.apply_edit("hollow_king", ContentEdit::Interval(ring, -1.0));
        }
        c.apply_edit("hollow_king", ContentEdit::Delay(ring, 10.0));
        c.apply_edit("hollow_king", ContentEdit::Hp(2.0));
        c.validate("edited").expect("edits keep the content valid");
        let a = &c.templates["hollow_king"].abilities[ring];
        assert_eq!(a.interval, 1.0);
        assert!(a.delay < a.interval);
        assert_eq!(c.templates["hollow_king"].hp, 28000.0);
        let text = c.to_toml().unwrap();
        let back = Content::from_toml_str(&text, "saved").unwrap();
        assert_eq!(back.dungeon.name, "The Sunken Vault (Custom)");
        assert_eq!(
            back.as_custom().dungeon.id,
            "sunken_vault_custom",
            "idempotent"
        );
    }

    #[test]
    fn bad_multiplier_is_reported_with_path() {
        let text = BUILTIN_SUNKEN_VAULT.replace("hp_multiplier = 1.0", "hp_multiplier = -2.0");
        let err = Content::from_toml_str(&text, "test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("dungeon.hp_multiplier"), "{err}");
    }

    #[test]
    fn unknown_template_lists_known_ones() {
        let text =
            BUILTIN_SUNKEN_VAULT.replacen("template = \"skulker\"", "template = \"skulkr\"", 1);
        let err = Content::from_toml_str(&text, "test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("'skulkr' is not defined"), "{err}");
        assert!(err.contains("known templates"), "{err}");
    }

    #[test]
    fn typo_field_is_rejected() {
        let text = BUILTIN_SUNKEN_VAULT.replace("interval = 9.0", "intervall = 9.0");
        let err = Content::from_toml_str(&text, "test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("intervall"), "{err}");
    }

    #[test]
    fn telegraph_longer_than_interval_is_rejected() {
        let text = BUILTIN_SUNKEN_VAULT.replace("delay = 2.5", "delay = 50.0");
        let err = Content::from_toml_str(&text, "test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("ring delay"), "{err}");
    }

    #[test]
    fn phase_thresholds_must_descend() {
        let text = BUILTIN_SUNKEN_VAULT.replace("hp_below = 0.5", "hp_below = 1.5");
        let err = Content::from_toml_str(&text, "test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("hp_below"), "{err}");
    }

    #[test]
    fn library_loads_shipped_directory() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/content/dungeons");
        let lib = ContentLibrary::load(dir);
        let errs: Vec<String> = lib.errors.iter().map(|e| e.to_string()).collect();
        assert!(errs.is_empty(), "{errs:?}");
        assert!(lib.get("sunken_vault").is_some());
        assert!(lib.get("gloomroot_warrens").is_some());
    }

    #[test]
    fn example_variants_are_valid() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/content/variants");
        let lib = ContentLibrary::load(dir);
        let errs: Vec<String> = lib.errors.iter().map(|e| e.to_string()).collect();
        assert!(errs.is_empty(), "{errs:?}");
        assert!(lib.dungeons.len() >= 2);
    }
}
