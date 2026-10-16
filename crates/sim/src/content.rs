//! Data-driven dungeon content: schema, loading and validation.
//!
//! Each dungeon is one TOML file in `assets/content/dungeons/`. Unknown fields are
//! rejected so typos surface as errors, and a validation pass reports every
//! problem with the path of the offending field.

use crate::items::{ItemSlot, ItemStats, Rarity, RoleAffinity};
use crate::math::{V2, V3};
use crate::nav::{Axis, LinkDef, LinkKind, Nav, NavParams, Rect, SurfaceDef, SurfaceId};
use crate::poly::Poly;
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
    /// Decorative lava pools and rivers (not walkable).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lava: Vec<LavaConfig>,
    /// Decoration: rock pillars, stalagmites, boulders, crystals. Never inside
    /// walkable space (validated), so it never lies about where you can go.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decor: Vec<DecorConfig>,
    /// Extra coloured point lights.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lights: Vec<LightConfig>,
    /// Model props (glTF/GLB) with a primitive fallback. Visual only, and
    /// kept off walkable space like decor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub props: Vec<PropConfig>,
    #[serde(default)]
    pub theme: Theme,
    /// Where a generated dungeon came from (written by Dungeon Forge).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated: Option<GeneratedInfo>,
    /// Logical rooms and corridors and the areas they are made of. Tools,
    /// lighting and modular art use it; the simulation never reads it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rooms: Vec<RoomConfig>,
    /// Lighting and fog presets, chosen per room (by `atmosphere` or `role`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub atmosphere: BTreeMap<String, AtmosphereConfig>,
    /// Modular art drawn along the real walls and doorways (visual only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modules: Option<ModuleSet>,
    /// Decoration anchors the generator computed (kept for inspection).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<AnchorConfig>,
}

/// Where a generated dungeon came from.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedInfo {
    /// Blueprint id.
    pub blueprint: String,
    /// Blueprint file as given to the generator (lets tools regenerate).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub blueprint_path: String,
    pub seed: u64,
    pub kit: String,
    pub generator: u32,
}

/// Room roles tools understand (others are rejected so typos surface).
pub const ROOM_ROLES: &[&str] = &[
    "entrance", "chamber", "hall", "arena", "pit", "side", "overlook", "passage", "corridor",
    "bridge", "stair", "ramp", "ledge", "room",
];

/// A logical room or corridor of the dungeon.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoomConfig {
    /// Stable identifier (e.g. `r3`, `c7`).
    pub id: String,
    pub name: String,
    /// One of [`ROOM_ROLES`].
    pub role: String,
    /// `main`, `alternate`, `side`, `overlook`, `shortcut` or `loop`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub route: String,
    /// Position on the main route (0 = entrance).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<u32>,
    /// Names of the areas this room is made of.
    pub areas: Vec<String>,
    /// For corridors: the ids of the two rooms joined.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connects: Vec<String>,
    /// Encounter staged here (`""` = none, for overrides).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter: Option<String>,
    /// Atmosphere preset (defaults to the role's preset, if any).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atmosphere: Option<String>,
}

fn default_brightness() -> f32 {
    220.0
}

fn default_fog_start() -> f32 {
    18.0
}

fn default_fog_end() -> f32 {
    75.0
}

/// Ambient light and fog for rooms using this preset.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AtmosphereConfig {
    pub ambient: [f32; 3],
    #[serde(default = "default_brightness")]
    pub brightness: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fog: Option<[f32; 3]>,
    #[serde(default = "default_fog_start")]
    pub fog_start: f32,
    #[serde(default = "default_fog_end")]
    pub fog_end: f32,
}

/// The modular art set of a dungeon. Each piece is optional; a missing piece
/// (or model file) falls back to the procedural geometry.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleSet {
    /// Tiled along every wall face, stretched to fit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall: Option<ModuleConfig>,
    /// A post at every wall joint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner: Option<ModuleConfig>,
    /// A frame over every doorway between two rooms (posts outside the
    /// opening, lintel above head height).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub door: Option<ModuleConfig>,
    /// Fills every `pillar` anchor (a hole in the walkable floor), scaled to
    /// the hole, replacing the plain walls round it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pillar: Option<ModuleConfig>,
}

fn variant_chance() -> f32 {
    0.2
}

fn is_default_variant_chance(v: &f32) -> bool {
    *v == 0.2
}

/// One modular piece.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleConfig {
    /// Path relative to `assets/`, `.glb` or `.gltf`.
    pub model: String,
    /// Nominal size in metres: length along the wall (or opening width for
    /// a door), height, depth.
    pub size: [f32; 3],
    /// Alternative models of the same size, used for some pieces (chosen
    /// deterministically from the position).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<String>,
    /// Share of pieces drawn with a variant.
    #[serde(
        default = "variant_chance",
        skip_serializing_if = "is_default_variant_chance"
    )]
    pub variant_chance: f32,
}

/// A place the generator could put decoration (for inspection in the viewer).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AnchorConfig {
    /// `corner`, `cut_corner`, `door_side`, `wall`, `pillar`, `pit`, `bridge_end`.
    pub kind: String,
    pub pos: V2,
    #[serde(default)]
    pub y: f32,
    /// Facing into the room, degrees.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub yaw: f32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub room: String,
    /// Free radius around the anchor outside walkable space.
    #[serde(default = "one")]
    pub clearance: f32,
    /// What the generator placed here, if anything.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub used: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<V2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<V2>,
    /// Convex outline instead of min/max (rivers are several pieces).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<V2>>,
    #[serde(default)]
    pub y: f32,
}

/// Footprint given either as `min`/`max` or as convex `points`.
fn footprint(min: Option<V2>, max: Option<V2>, points: &Option<Vec<V2>>) -> Result<Poly, String> {
    match (min, max, points) {
        (Some(a), Some(b), None) => {
            let r = Rect::new(a, b);
            if r.is_valid() {
                Ok(Poly::from_rect(r))
            } else {
                Err(format!(
                    "min {:?} must be smaller than max {:?} on both axes",
                    <[f32; 2]>::from(a),
                    <[f32; 2]>::from(b)
                ))
            }
        }
        (None, None, Some(p)) => Poly::new(p),
        _ => Err("give either min and max, or points (not both)".into()),
    }
}

impl LavaConfig {
    pub fn shape(&self) -> Result<Poly, String> {
        footprint(self.min, self.max, &self.points)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DecorKind {
    /// A tall rock column.
    Pillar,
    /// A pointed rock rising from the floor.
    Stalagmite,
    /// A low irregular rock.
    Boulder,
    /// A glowing crystal cluster (takes the light colour).
    Crystal,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DecorConfig {
    pub kind: DecorKind,
    pub pos: V2,
    /// Base height.
    #[serde(default)]
    pub y: f32,
    #[serde(default = "one")]
    pub radius: f32,
    #[serde(default = "four")]
    pub height: f32,
    /// Crystal glow colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 3]>,
}

/// A visual prop loaded from a glTF/GLB model under `assets/`. Navigation
/// never depends on it: its footprint must stay off walkable floor (like
/// decor), and when the model file is missing the game draws `fallback`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PropConfig {
    /// Path relative to `assets/`, ending in `.glb` or `.gltf`.
    pub model: String,
    pub pos: V2,
    /// Base height.
    #[serde(default)]
    pub y: f32,
    /// Rotation about the vertical axis, degrees.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub yaw: f32,
    #[serde(default = "one")]
    pub scale: f32,
    /// Footprint radius and height used for validation and the fallback.
    #[serde(default = "one")]
    pub radius: f32,
    #[serde(default = "four")]
    pub height: f32,
    /// Primitive drawn when the model can't be loaded.
    pub fallback: DecorKind,
}

impl PropConfig {
    /// Problems with the model path (relative, inside `assets/`, glTF).
    pub fn path_problem(&self) -> Option<String> {
        model_path_problem(&self.model)
    }
}

/// Problems with a model path: relative to `assets/`, inside it, glTF.
pub fn model_path_problem(m: &str) -> Option<String> {
    {
        if m.is_empty() {
            return Some("model is empty".into());
        }
        if m.starts_with('/') || m.starts_with('\\') || m.contains(':') {
            return Some(format!(
                "model '{m}' must be relative to assets/ (no drive or leading slash)"
            ));
        }
        if m.split(['/', '\\']).any(|part| part == "..") {
            return Some(format!("model '{m}' must not leave assets/ ('..')"));
        }
        let lower = m.to_ascii_lowercase();
        if !(lower.ends_with(".glb") || lower.ends_with(".gltf")) {
            return Some(format!("model '{m}' must be a .glb or .gltf file"));
        }
        None
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LightConfig {
    pub pos: V2,
    #[serde(default = "four")]
    pub y: f32,
    pub color: [f32; 3],
    /// Multiplies the default brightness.
    #[serde(default = "one")]
    pub intensity: f32,
    #[serde(default = "twenty")]
    pub range: f32,
}

/// Height of the lip along an edge that faces lava.
pub const LAVA_LIP: f32 = 0.6;

fn four() -> f32 {
    4.0
}

fn twenty() -> f32 {
    20.0
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct Theme {
    pub floor: [f32; 3],
    pub wall: [f32; 3],
    pub light: [f32; 3],
    /// Distance fog colour (none = no fog).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fog: Option<[f32; 3]>,
    /// Ambient light colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ambient: Option<[f32; 3]>,
    /// Draw walls as jagged rock instead of flat slabs.
    #[serde(skip_serializing_if = "is_false")]
    pub rough_walls: bool,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            floor: [0.22, 0.24, 0.27],
            wall: [0.32, 0.3, 0.34],
            light: [1.0, 0.75, 0.45],
            fog: None,
            ambient: None,
            rough_walls: false,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<V2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<V2>,
    /// Convex outline `[[x, z], ...]` instead of `min`/`max`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<V2>>,
    /// Ramp in any direction: height `y` at `slope_from`, `y_to` at `slope_to`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slope_from: Option<V2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slope_to: Option<V2>,
    /// Floor colour override (regions read differently).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<[f32; 3]>,
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
    /// Pulse: only hits party members who can see the caster (hide behind
    /// pillars or around corners while it is cast).
    #[serde(default, skip_serializing_if = "is_false")]
    pub los_blockable: bool,
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
    /// Patrol route: while idle the mob walks from `pos` through these points
    /// and back again at walking pace (aggro is checked where it is).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patrol: Vec<V2>,
}

/// Most patrol points a mob may have.
pub const MAX_PATROL_POINTS: usize = 8;

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
    /// Rooms, atmosphere, modules and anchors: presentation data the
    /// simulation never reads, checked so tools can trust it.
    fn validate_presentation(&self, check: &mut impl FnMut(bool, String)) {
        let d = &self.dungeon;
        let areas: BTreeSet<&str> = d.areas.iter().map(|a| a.name.as_str()).collect();
        let ids: BTreeSet<&str> = d.rooms.iter().map(|r| r.id.as_str()).collect();
        check(
            ids.len() == d.rooms.len(),
            "dungeon.rooms: room ids must be unique".into(),
        );
        let encounters: BTreeSet<&str> = self.encounters.iter().map(|e| e.id.as_str()).collect();
        for (i, r) in d.rooms.iter().enumerate() {
            let p = format!("dungeon.rooms[{i}] ('{}')", r.id);
            check(
                ROOM_ROLES.contains(&r.role.as_str()),
                format!(
                    "{p}: role '{}' is not one of {}",
                    r.role,
                    ROOM_ROLES.join(", ")
                ),
            );
            check(!r.areas.is_empty(), format!("{p}: needs at least one area"));
            for a in &r.areas {
                check(
                    areas.contains(a.as_str()),
                    format!("{p}: area '{a}' is not in dungeon.areas"),
                );
            }
            for c in &r.connects {
                check(
                    ids.contains(c.as_str()),
                    format!("{p}: connects to unknown room '{c}'"),
                );
            }
            // An override can clear the reference with `encounter = ""`.
            if let Some(e) = r.encounter.as_ref().filter(|e| !e.is_empty()) {
                check(
                    encounters.contains(e.as_str()),
                    format!("{p}: encounter '{e}' is not an encounter id"),
                );
            }
            if let Some(a) = &r.atmosphere {
                check(
                    d.atmosphere.contains_key(a),
                    format!("{p}: atmosphere '{a}' is not in dungeon.atmosphere"),
                );
            }
        }
        let colour = |c: [f32; 3]| c.iter().all(|v| (0.0..=1.0).contains(v));
        for (k, a) in &d.atmosphere {
            check(
                colour(a.ambient)
                    && a.fog.is_none_or(colour)
                    && (0.0..=5000.0).contains(&a.brightness)
                    && a.fog_start >= 0.0
                    && a.fog_end > a.fog_start,
                format!(
                    "dungeon.atmosphere.{k}: colours in 0..=1, brightness in 0..=5000, fog_end > fog_start >= 0"
                ),
            );
        }
        if let Some(m) = &d.modules {
            for (name, piece) in [
                ("wall", &m.wall),
                ("corner", &m.corner),
                ("door", &m.door),
                ("pillar", &m.pillar),
            ] {
                if let Some(piece) = piece {
                    for model in std::iter::once(&piece.model).chain(&piece.variants) {
                        if let Some(problem) = model_path_problem(model) {
                            check(false, format!("dungeon.modules.{name}: {problem}"));
                        }
                    }
                    check(
                        (0.0..=1.0).contains(&piece.variant_chance),
                        format!("dungeon.modules.{name}: variant_chance must be in 0..=1"),
                    );
                    check(
                        piece.size.iter().all(|v| (0.05..=50.0).contains(v)),
                        format!("dungeon.modules.{name}: size components must be in 0.05..=50"),
                    );
                }
            }
        }
        for (i, a) in d.anchors.iter().enumerate() {
            check(
                a.pos.is_finite() && a.y.is_finite() && a.yaw.is_finite() && a.clearance >= 0.0,
                format!("dungeon.anchors[{i}]: position, height and yaw must be finite"),
            );
        }
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

    /// The shipped Cinderdeep Hollows (used by tests).
    pub fn builtin_cinderdeep() -> Content {
        Content::from_toml_str(BUILTIN_CINDERDEEP, "<builtin cinderdeep_hollows>")
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
        let mut errors = Vec::new();
        let mut defs = Vec::new();
        for (i, a) in d.areas.iter().enumerate() {
            let shape = match footprint(a.min, a.max, &a.points) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(format!("dungeon.areas[{i}] ('{}'): {e}", a.name));
                    continue;
                }
            };
            let slope = match (a.slope_from, a.slope_to) {
                (Some(f), Some(t)) => Some((f, t)),
                (None, None) => None,
                _ => {
                    errors.push(format!(
                        "dungeon.areas[{i}] ('{}'): slope_from and slope_to go together",
                        a.name
                    ));
                    None
                }
            };
            defs.push(SurfaceDef {
                name: a.name.clone(),
                shape,
                y: a.y,
                y_to: a.y_to,
                axis: match a.axis {
                    AxisConfig::X => Axis::X,
                    AxisConfig::Z => Axis::Z,
                },
                slope,
                stairs: a.stairs,
                walls: a.walls,
                wall_height: a.wall_height.unwrap_or(crate::nav::WALL_HEIGHT),
                open_under: a.under == UnderConfig::Open,
                unreachable: a.unreachable,
            });
        }
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
        let mut nav = Nav::build(defs, links, params)?;
        // An edge that drops away to lava is a low lip, not a wall: you can
        // see across a slag pit or a lava river (rendering and line of sight).
        let pools: Vec<Poly> = d.lava.iter().filter_map(|l| l.shape().ok()).collect();
        if !pools.is_empty() {
            for w in nav.walls.iter_mut() {
                let mid = (w.a + w.b) * 0.5;
                let outside = mid + w.out * 0.8;
                if pools.iter().any(|p| p.contains(outside)) {
                    w.height = w.height.min(LAVA_LIP);
                }
            }
        }
        Ok(nav)
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
            if let Err(e) = l.shape() {
                check(false, format!("dungeon.lava[{i}]: {e}"));
            }
            check(
                l.y.is_finite(),
                format!("dungeon.lava[{i}]: y must be finite"),
            );
        }
        for (i, l) in d.lights.iter().enumerate() {
            check(
                l.pos.is_finite()
                    && l.y.is_finite()
                    && (0.0..=20.0).contains(&l.intensity)
                    && (1.0..=200.0).contains(&l.range)
                    && l.color.iter().all(|c| (0.0..=1.0).contains(c)),
                format!(
                    "dungeon.lights[{i}]: intensity must be in 0..=20, range in 1..=200 and colour components in 0..=1"
                ),
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

        // Decoration must stand outside walkable space: a pillar inside an area
        // would look like an obstacle the navigation walks through.
        // Footprints (decor and props) must stand outside walkable space.
        let blocks_floor = |pos: V2, y: f32, r: f32, height: f32| -> Option<String> {
            let nav = nav.as_ref()?;
            let foot = Poly::from_rect(Rect::new(
                V2::new(pos.x - r, pos.z - r),
                V2::new(pos.x + r, pos.z + r),
            ));
            for s in &nav.surfaces {
                let Some(ov) = s.def.shape.intersect(&foot) else {
                    continue;
                };
                if ov.area() < 0.01 {
                    continue;
                }
                // Walkable space: from the floor up to head height.
                let floor = s.height_at(ov.centroid());
                if y < floor + 2.2 && y + height > floor - 0.3 {
                    return Some(s.name().to_string());
                }
            }
            None
        };
        for (i, dc) in d.decor.iter().enumerate() {
            let ok_numbers = dc.pos.is_finite()
                && dc.y.is_finite()
                && (0.1..=30.0).contains(&dc.radius)
                && (0.1..=60.0).contains(&dc.height);
            check(
                ok_numbers,
                format!("dungeon.decor[{i}]: radius must be in 0.1..=30 and height in 0.1..=60"),
            );
            if ok_numbers && let Some(area) = blocks_floor(dc.pos, dc.y, dc.radius, dc.height) {
                check(
                    false,
                    format!(
                        "dungeon.decor[{i}] ({:?}) at {:?} stands in walkable area '{area}'; move it outside the area (decoration must never block or fake a path)",
                        dc.kind,
                        <[f32; 2]>::from(dc.pos),
                    ),
                );
            }
        }
        for (i, p) in d.props.iter().enumerate() {
            let what = format!("dungeon.props[{i}] ('{}')", p.model);
            if let Some(problem) = p.path_problem() {
                check(false, format!("{what}: {problem}"));
            }
            let ok_numbers = p.pos.is_finite()
                && p.y.is_finite()
                && p.yaw.is_finite()
                && (0.05..=20.0).contains(&p.scale)
                && (0.1..=30.0).contains(&p.radius)
                && (0.1..=60.0).contains(&p.height);
            check(
                ok_numbers,
                format!(
                    "{what}: scale must be in 0.05..=20, radius in 0.1..=30, height in 0.1..=60"
                ),
            );
            if ok_numbers && let Some(area) = blocks_floor(p.pos, p.y, p.radius, p.height) {
                check(
                    false,
                    format!(
                        "{what} at {:?} stands in walkable area '{area}'; props are scenery and must stay off walkable floor (give it a smaller radius or move it)",
                        <[f32; 2]>::from(p.pos)
                    ),
                );
            }
        }

        self.validate_presentation(&mut check);
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
                check(
                    !a.los_blockable || a.kind == AbilityKind::Pulse,
                    format!("{ap}: los_blockable only applies to kind = \"pulse\""),
                );
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
                if let Some(nav) = &nav {
                    match Content::place(nav, m.pos, m.y) {
                        Err(e) => {
                            check(false, format!("{mp}.pos {:?} {e}", <[f32; 2]>::from(m.pos)))
                        }
                        Ok((start, s0)) => {
                            check(
                                m.patrol.len() <= MAX_PATROL_POINTS,
                                format!("{mp}.patrol: at most {MAX_PATROL_POINTS} points"),
                            );
                            // Every leg of the patrol must be a walkable route.
                            let mut prev = (start, s0);
                            for (k, q) in m.patrol.iter().enumerate() {
                                match Content::place(nav, *q, m.y) {
                                    Err(e) => {
                                        check(
                                            false,
                                            format!(
                                                "{mp}.patrol[{k}] {:?} {e}",
                                                <[f32; 2]>::from(*q)
                                            ),
                                        );
                                        break;
                                    }
                                    Ok((p3, s)) => {
                                        if nav.path_points(prev.0.xz(), prev.1, p3).is_none() {
                                            check(
                                                false,
                                                format!(
                                                    "{mp}.patrol[{k}] {:?} cannot be walked to from the previous point; patrol legs must be walkable routes",
                                                    <[f32; 2]>::from(*q)
                                                ),
                                            );
                                            break;
                                        }
                                        prev = (p3, s);
                                    }
                                }
                            }
                        }
                    }
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
    /// Content location from `DUNGEON_CONTENT` or the default directory
    /// (relative to the working directory, else next to the executable, as
    /// in the packaged build started from elsewhere).
    pub fn default_path() -> String {
        if let Ok(p) = std::env::var(CONTENT_ENV_VAR) {
            return p;
        }
        if !Path::new(DEFAULT_CONTENT_DIR).is_dir()
            && let Some(beside) = std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|d| d.join(DEFAULT_CONTENT_DIR)))
                .filter(|d| d.is_dir())
        {
            return beside.to_string_lossy().into_owned();
        }
        DEFAULT_CONTENT_DIR.to_string()
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
pub const BUILTIN_CINDERDEEP: &str =
    include_str!("../../../assets/content/dungeons/06_cinderdeep_hollows.toml");
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
        for c in [
            Content::builtin(),
            Content::builtin_warrens(),
            Content::builtin_cavern(),
            Content::builtin_cinderdeep(),
        ] {
            let text = c.to_toml().expect("serializes");
            let back = Content::from_toml_str(&text, "roundtrip").expect("re-parses and validates");
            assert_eq!(back.to_toml().unwrap(), text, "stable round trip");
            assert_eq!(back.encounters.len(), c.encounters.len());
            // Heights, ramps, decks and drops survive (the editor saves this way).
            assert_eq!(back.dungeon.areas.len(), c.dungeon.areas.len());
            assert_eq!(back.dungeon.links.len(), c.dungeon.links.len());
            let (a, b) = (c.nav(), back.nav());
            assert_eq!(a.links.len(), b.links.len());
            assert_eq!(a.walls.len(), b.walls.len());
        }
    }

    #[test]
    fn polygon_areas_decor_and_lava_polygons_load_and_validate() {
        // Replace the cavern's narrow tunnel with a diagonal polygon of the same
        // reach, add lava as a polygon and two decorations.
        let text = BUILTIN_CAVERN.replace(
            "name = \"Narrow Tunnel\"
min = [26.0, 40.0]
max = [44.0, 43.0]",
            "name = \"Narrow Tunnel\"
points = [[26.0, 40.0], [44.0, 40.0], [44.0, 43.0], [26.0, 44.0]]",
        );
        assert_ne!(text, BUILTIN_CAVERN, "replacement applied");
        let ok = format!(
            "{text}
[[dungeon.decor]]
kind = \"pillar\"
pos = [5.0, 30.0]
radius = 1.5
height = 6.0

[[dungeon.lava]]
points = [[60.0, 0.0], [70.0, 0.0], [65.0, 8.0]]
y = -0.5
"
        );
        let c = Content::from_toml_str(&ok, "test").expect("valid");
        let nav = c.nav();
        let tunnel = nav.find("Narrow Tunnel").unwrap();
        assert!(!nav.surfaces[tunnel as usize].def.shape.is_axis_rect());
        assert!(c.dungeon.lava.last().unwrap().shape().is_ok());
        // A pillar standing in the entrance cavern is rejected with the fix.
        let bad = format!(
            "{text}
[[dungeon.decor]]
kind = \"pillar\"
pos = [0.0, 0.0]
"
        );
        let err = Content::from_toml_str(&bad, "test")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("stands in walkable area 'Entrance Cavern'"),
            "{err}"
        );
        // Non-convex areas are explained.
        let concave = BUILTIN_CAVERN.replace(
            "name = \"Narrow Tunnel\"
min = [26.0, 40.0]
max = [44.0, 43.0]",
            "name = \"Narrow Tunnel\"
points = [[26.0, 40.0], [44.0, 40.0], [35.0, 41.0], [44.0, 43.0], [26.0, 43.0]]",
        );
        let err = Content::from_toml_str(&concave, "test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("not convex"), "{err}");
    }

    #[test]
    fn ambiguous_or_misplaced_elevations_are_reported() {
        // The bridge skulker stands above the underpass: without `y` it is ambiguous.
        let text = BUILTIN_CAVERN.replace(
            "{ template = \"skulker\", pos = [42.0, 70.0], y = 5.0 }",
            "{ template = \"skulker\", pos = [42.0, 70.0] }",
        );
        let err = Content::from_toml_str(&text, "test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("add y = <height>"), "{err}");
        let text = BUILTIN_CAVERN.replace(
            "{ template = \"skulker\", pos = [42.0, 70.0], y = 5.0 }",
            "{ template = \"skulker\", pos = [42.0, 70.0], y = 2.0 }",
        );
        let err = Content::from_toml_str(&text, "test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("floors there are at y"), "{err}");
        // A solid area over another is rejected with the fix in the message.
        let text = BUILTIN_CAVERN.replacen(
            "under = \"open\"
",
            "",
            1,
        );
        let err = Content::from_toml_str(&text, "test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("under = \"open\""), "{err}");
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
