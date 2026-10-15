//! A kit is a theme library: enemy templates (the ordinary dungeon schema),
//! encounter recipes, loot and a palette. Blueprints pick a kit by name; the
//! same kit serves any number of dungeons.

use dungeon_sim::content::{DecorKind, EnemyRank, EnemyTemplate, LootEntry, Theme};
use dungeon_sim::stats::School;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Kit {
    pub kit: KitMeta,
    pub palette: Palette,
    pub templates: BTreeMap<String, EnemyTemplate>,
    pub recipes: BTreeMap<String, Recipe>,
    #[serde(default)]
    pub loot: Vec<LootEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KitMeta {
    pub id: String,
    pub name: String,
    /// Level the templates' numbers were authored for. Generated dungeons scale
    /// HP and damage from here to the blueprint level.
    pub reference_level: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    pub theme: Theme,
    /// Floor tint near the entrance and at the deepest point (interpolated).
    pub tint_shallow: [f32; 3],
    pub tint_deep: [f32; 3],
    /// Light colour near the entrance and at the deepest point.
    pub light_shallow: [f32; 3],
    pub light_deep: [f32; 3],
    /// Decoration mix around rooms.
    pub decor: Vec<DecorKind>,
    /// Name pools per room kind (unique names are drawn by seed).
    pub names: RoomNames,
    /// Model prop for the cut corners of boss arenas (visual only).
    #[serde(default)]
    pub arena_prop: Option<PropSpec>,
}

/// A model prefab: a glTF/GLB file under `assets/` with a primitive fallback.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PropSpec {
    pub model: String,
    pub fallback: DecorKind,
    #[serde(default = "one_f")]
    pub scale: f32,
    /// Footprint radius and height (validation and fallback size).
    pub radius: f32,
    pub height: f32,
}

fn one_f() -> f32 {
    1.0
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoomNames {
    pub entrance: Vec<String>,
    pub chamber: Vec<String>,
    pub hall: Vec<String>,
    pub arena: Vec<String>,
    pub pit: Vec<String>,
    pub side: Vec<String>,
    pub overlook: Vec<String>,
}

/// Which room component a recipe needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RoomPref {
    /// Small room: single elites, optional side fights.
    Chamber,
    /// Large room: packs.
    Hall,
    /// Boss arena (large octagon).
    Arena,
    /// Room around a lava pit (hazard fights, lava-side bosses).
    Pit,
}

/// Where a mob stands relative to the direction the party enters from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Placement {
    /// Nearest the party: the melee screen.
    Front,
    /// Behind the front line: casters.
    Back,
    /// Left and right of the pack.
    Flank,
    /// The middle of the pack (bosses).
    Center,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeMob {
    pub template: String,
    #[serde(default = "center")]
    pub place: Placement,
    #[serde(default = "one")]
    pub count: u32,
}

fn center() -> Placement {
    Placement::Center
}

fn one() -> u32 {
    1
}

/// Where a recipe's hazards go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HazardPlace {
    /// Either side of the approach to the pack.
    Approach,
    /// Either side of the pack.
    Flanks,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeHazard {
    pub name: String,
    pub place: HazardPlace,
    pub radius: f32,
    pub damage: f32,
    #[serde(default)]
    pub school: Option<School>,
    pub on: f32,
    pub off: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    /// Encounter display name (bosses default to the boss's name).
    pub name: String,
    /// One line for designers: what the encounter tests.
    #[serde(default)]
    pub tests: String,
    pub room: RoomPref,
    pub mobs: Vec<RecipeMob>,
    #[serde(default)]
    pub hazards: Vec<RecipeHazard>,
}

impl Kit {
    pub fn load(path: &Path) -> Result<Kit, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("could not read kit '{}': {e}", path.display()))?;
        Kit::from_toml_str(&text, &path.display().to_string())
    }

    pub fn from_toml_str(text: &str, origin: &str) -> Result<Kit, String> {
        let kit: Kit = toml::from_str(text)
            .map_err(|e| format!("kit '{origin}' is not valid TOML for the schema:\n{e}"))?;
        let issues = kit.validate();
        if issues.is_empty() {
            Ok(kit)
        } else {
            Err(format!(
                "kit '{origin}' has {} problem(s):\n  - {}",
                issues.len(),
                issues.join("\n  - ")
            ))
        }
    }

    pub fn validate(&self) -> Vec<String> {
        let mut e = Vec::new();
        if !(1..=10).contains(&self.kit.reference_level) {
            e.push("kit.reference_level: use 1..=10".into());
        }
        let n = &self.palette.names;
        for (k, pool) in [
            ("entrance", &n.entrance),
            ("chamber", &n.chamber),
            ("hall", &n.hall),
            ("arena", &n.arena),
            ("pit", &n.pit),
            ("side", &n.side),
            ("overlook", &n.overlook),
        ] {
            if pool.is_empty() {
                e.push(format!("palette.names.{k}: give at least one name"));
            }
        }
        if self.palette.decor.is_empty() {
            e.push("palette.decor: give at least one decoration kind".into());
        }
        for (key, r) in &self.recipes {
            if r.mobs.is_empty() {
                e.push(format!("recipes.{key}: needs at least one mob"));
            }
            for (i, m) in r.mobs.iter().enumerate() {
                if !self.templates.contains_key(&m.template) {
                    e.push(format!(
                        "recipes.{key}.mobs[{i}].template '{}' is not a kit template; known: {}",
                        m.template,
                        self.templates
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                if !(1..=6).contains(&m.count) {
                    e.push(format!("recipes.{key}.mobs[{i}].count: use 1..=6"));
                }
            }
            let bosses = r
                .mobs
                .iter()
                .filter(|m| {
                    self.templates.get(&m.template).map(|t| t.rank) == Some(EnemyRank::Boss)
                })
                .count();
            if bosses > 0 && r.room != RoomPref::Arena && r.room != RoomPref::Pit {
                e.push(format!(
                    "recipes.{key}: a boss recipe needs room = \"arena\" or \"pit\" (space for its mechanics)"
                ));
            }
            for (i, h) in r.hazards.iter().enumerate() {
                if !(0.5..=6.0).contains(&h.radius) || h.on <= 0.0 || h.off < 0.0 {
                    e.push(format!(
                        "recipes.{key}.hazards[{i}]: radius 0.5..=6, on > 0, off >= 0"
                    ));
                }
            }
        }
        for (i, l) in self.loot.iter().enumerate() {
            match l.source.as_deref() {
                Some("final_boss" | "boss" | "optional" | "pack") | None => {}
                Some(other) => e.push(format!(
                    "loot[{i}] ('{}').source = '{other}': in a kit use final_boss, boss, optional or pack",
                    l.id
                )),
            }
        }
        e
    }

    pub fn recipe_has_boss(&self, key: &str) -> bool {
        self.recipes.get(key).is_some_and(|r| {
            r.mobs
                .iter()
                .any(|m| self.templates.get(&m.template).map(|t| t.rank) == Some(EnemyRank::Boss))
        })
    }

    /// Templates for a dungeon at `level`: HP and damage scale like party
    /// power (×(1 + 0.1·(L−1))), template levels shift by the same offset.
    pub fn scaled_templates(&self, level: u32) -> BTreeMap<String, EnemyTemplate> {
        let f = |l: u32| 1.0 + 0.1 * (l as f32 - 1.0);
        let k = f(level) / f(self.kit.reference_level);
        let shift = level as i32 - self.kit.reference_level as i32;
        self.templates
            .iter()
            .map(|(key, t)| {
                let mut t = t.clone();
                let round = |v: f32| (v * k).round();
                t.hp = round(t.hp).max(1.0);
                t.melee_damage = round(t.melee_damage);
                for a in &mut t.abilities {
                    a.damage = round(a.damage);
                    a.amount = round(a.amount);
                    if let Some(aura) = &mut a.aura
                        && aura.effect == dungeon_sim::content::DebuffEffect::Dot
                    {
                        aura.value = (aura.value * k).round();
                    }
                }
                t.level = (t.level as i32 + shift).clamp(1, 12) as u32;
                (key.clone(), t)
            })
            .collect()
    }
}
