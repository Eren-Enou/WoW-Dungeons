//! A kit is a theme library: enemy templates (the ordinary dungeon schema),
//! encounter recipes, loot, a palette and the kit's spatial vocabulary (room
//! shapes and proportions, corridor widths, what bridges cross). Blueprints
//! pick a kit by name; the same kit serves any number of dungeons.

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
    /// The kit's spatial vocabulary. Defaults reproduce the 0.6 caverns.
    #[serde(default)]
    pub components: Components,
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
    /// Model prop stood in some pillar holes (instead of a stone column).
    #[serde(default)]
    pub pillar_prop: Option<PropSpec>,
    /// Multiplies every room light (dim crypts, bright furnaces).
    #[serde(default = "one_f")]
    pub light_intensity: f32,
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
    #[serde(default = "bend_names")]
    pub bend: Vec<String>,
}

fn bend_names() -> Vec<String> {
    vec!["Passage".into()]
}

// ---------------------------------------------------------------- components

/// What a bridge crosses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Chasm {
    /// A lava river (with rock stacks and a glow).
    #[default]
    Lava,
    /// A dark drop with nothing at the bottom.
    Void,
}

/// Room footprint styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ShapeSpec {
    /// One convex polygon (rectangle, or with cut corners).
    #[default]
    Plain,
    /// Four strips round a central lava pit.
    Pit,
    /// A rectangle with a grid of solid pillars (walled holes that block
    /// movement and sight).
    Pillars,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct PillarSpec {
    /// Side of a square pillar (m).
    pub size: f32,
    /// Centre-to-centre spacing (at least size + 3).
    pub spacing: f32,
}

impl Default for PillarSpec {
    fn default() -> Self {
        PillarSpec {
            size: 2.0,
            spacing: 7.0,
        }
    }
}

/// How one kind of room is built.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoomSpec {
    /// Base half-size range (m, from the cell centre to a wall).
    pub size: [f32; 2],
    /// Elongation along the direction of travel (1 = square; 1.6 = a nave).
    #[serde(default = "unit_range")]
    pub aspect: [f32; 2],
    /// Per-side random variation of the half-size, as a fraction (irregular,
    /// off-centre rooms).
    #[serde(default)]
    pub jitter: f32,
    /// Corner cut as a fraction of the smaller half-size, drawn per corner.
    #[serde(default)]
    pub cut: [f32; 2],
    #[serde(default)]
    pub shape: ShapeSpec,
    /// Pit half-size range (pit rooms).
    #[serde(default = "pit_range")]
    pub pit: [f32; 2],
    #[serde(default)]
    pub pillars: PillarSpec,
}

fn unit_range() -> [f32; 2] {
    [1.0, 1.0]
}

fn pit_range() -> [f32; 2] {
    [3.5, 4.5]
}

impl RoomSpec {
    fn plain(lo: f32, hi: f32, cut: [f32; 2]) -> RoomSpec {
        RoomSpec {
            size: [lo, hi],
            aspect: unit_range(),
            jitter: 0.0,
            cut,
            shape: ShapeSpec::Plain,
            pit: pit_range(),
            pillars: PillarSpec::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct RoomSpecs {
    pub entrance: RoomSpec,
    pub chamber: RoomSpec,
    pub hall: RoomSpec,
    pub arena: RoomSpec,
    pub pit: RoomSpec,
    pub side: RoomSpec,
    pub overlook: RoomSpec,
    /// Passage bends: small elbows that lengthen winding routes.
    pub bend: RoomSpec,
}

impl Default for RoomSpecs {
    fn default() -> Self {
        RoomSpecs {
            entrance: RoomSpec::plain(7.0, 8.5, [0.0, 0.0]),
            chamber: RoomSpec::plain(8.0, 9.5, [0.0, 0.3]),
            hall: RoomSpec::plain(11.0, 12.5, [0.3, 0.3]),
            arena: RoomSpec::plain(13.0, 14.0, [0.45, 0.45]),
            pit: RoomSpec {
                shape: ShapeSpec::Pit,
                ..RoomSpec::plain(11.0, 12.5, [0.0, 0.0])
            },
            side: RoomSpec::plain(7.0, 8.5, [0.0, 0.0]),
            overlook: RoomSpec::plain(5.0, 5.0, [0.0, 0.0]),
            bend: RoomSpec::plain(4.0, 4.0, [0.0, 0.0]),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct Components {
    /// Distance between neighbouring cell centres.
    pub cell: f32,
    /// Corridor width range on the main route.
    pub corridor_width: [f32; 2],
    /// Width of passages through bends, side corridors and alternate routes.
    pub passage_width: [f32; 2],
    /// What bridges cross.
    pub chasm: Chasm,
    /// Rock columns just outside room corners.
    pub corner_decor: bool,
    pub rooms: RoomSpecs,
}

impl Default for Components {
    fn default() -> Self {
        Components {
            cell: 36.0,
            corridor_width: [5.0, 6.0],
            passage_width: [4.5, 5.5],
            chasm: Chasm::Lava,
            corner_decor: true,
            rooms: RoomSpecs::default(),
        }
    }
}

// ------------------------------------------------------------------- recipes

/// Which room component a recipe needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RoomPref {
    /// Small room: single elites, optional side fights.
    Chamber,
    /// Large room: packs.
    Hall,
    /// Boss arena.
    Arena,
    /// Room around a lava pit (hazard fights, lava-side bosses).
    Pit,
}

/// Where a mob stands in its pack's formation.
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

/// The tactical challenge an encounter poses (for blueprint intent, reports
/// and validation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tag {
    /// Casts worth interrupting.
    Interrupts,
    /// Frontal cleaves: the tank controls facing.
    Frontal,
    /// Casters or healers behind a melee screen.
    ProtectCaster,
    /// A moving pack: timing and pull decisions.
    Patrol,
    /// Ground effects or hazards change where it is safe to stand.
    AreaControl,
    /// Adds arrive mid-fight.
    Summons,
    /// Several significant enemies at once.
    Council,
    /// Line of sight matters (hide, or keep sight of the healer).
    LineOfSight,
    /// The fight changes over phases.
    MultiPhase,
}

/// Where a pack wants to stand in its room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Preference {
    /// Far from the entry (room to stage and pull).
    #[default]
    Far,
    /// In front of another exit, guarding the way on.
    GuardExit,
    /// The middle of the room (bosses, councils).
    Center,
    /// Walking a route across the room.
    Patrol,
}

/// How the front/back/flank roles are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Formation {
    /// Fronts in a row toward the entry, backs in a row behind.
    #[default]
    Line,
    /// A point mob in front, the rest fanned out behind.
    Wedge,
    /// Fronts shoulder to shoulder, backs well behind them (protected
    /// casters).
    Screen,
    /// Around the anchor (councils).
    Ring,
}

/// Space an encounter needs; placement rejects rooms and spots without it.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct SpaceSpec {
    /// Smallest half-size of the room (m).
    pub min_size: f32,
    /// A floor spot 12-18 m from the pack with line of sight to it, so
    /// ranged members and healers can work.
    pub ranged_lane: bool,
    /// The room must have pillars (line-of-sight fights).
    pub pillars: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PatrolSpec {
    /// Route length across the room (m).
    pub length: f32,
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
    /// Tactical challenges.
    #[serde(default)]
    pub tags: Vec<Tag>,
    #[serde(default)]
    pub placement: Preference,
    #[serde(default)]
    pub formation: Formation,
    #[serde(default)]
    pub space: SpaceSpec,
    /// Required when `placement = "patrol"`.
    #[serde(default)]
    pub patrol: Option<PatrolSpec>,
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

    /// The spec for a room kind.
    pub fn room_spec(&self, kind: crate::layout::RoomKind) -> &RoomSpec {
        use crate::layout::RoomKind as K;
        let r = &self.components.rooms;
        match kind {
            K::Entrance => &r.entrance,
            K::Chamber => &r.chamber,
            K::Hall => &r.hall,
            K::Arena => &r.arena,
            K::Pit => &r.pit,
            K::Side => &r.side,
            K::Overlook => &r.overlook,
            K::Bend => &r.bend,
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
            ("bend", &n.bend),
        ] {
            if pool.is_empty() {
                e.push(format!("palette.names.{k}: give at least one name"));
            }
        }
        if self.palette.decor.is_empty() {
            e.push("palette.decor: give at least one decoration kind".into());
        }
        if !(0.1..=5.0).contains(&self.palette.light_intensity) {
            e.push("palette.light_intensity: use 0.1..=5".into());
        }
        let c = &self.components;
        if !(24.0..=60.0).contains(&c.cell) {
            e.push(format!("components.cell = {}: use 24..=60 m", c.cell));
        }
        for (k, w) in [
            ("corridor_width", c.corridor_width),
            ("passage_width", c.passage_width),
        ] {
            if w[0] < 2.5 || w[0] > w[1] || w[1] > 8.0 {
                e.push(format!(
                    "components.{k} = {w:?}: use 2.5 <= min <= max <= 8 m"
                ));
            }
        }
        let max_half = c.cell / 2.0 - 4.0;
        for (k, spec) in [
            ("entrance", &c.rooms.entrance),
            ("chamber", &c.rooms.chamber),
            ("hall", &c.rooms.hall),
            ("arena", &c.rooms.arena),
            ("pit", &c.rooms.pit),
            ("side", &c.rooms.side),
            ("overlook", &c.rooms.overlook),
            ("bend", &c.rooms.bend),
        ] {
            let reach = spec.size[1] * spec.aspect[1].max(1.0) * (1.0 + spec.jitter);
            if spec.size[0] < 3.0
                || spec.size[0] > spec.size[1]
                || spec.aspect[0] < 1.0
                || spec.aspect[0] > spec.aspect[1]
                || !(0.0..=0.4).contains(&spec.jitter)
                || reach > max_half
            {
                e.push(format!(
                    "components.rooms.{k}: size {:?}, aspect {:?}, jitter {} reach {reach:.1} m; keep 3 <= size min <= max, 1 <= aspect min <= max, jitter 0..=0.4, and the largest half-size under {max_half:.1} m (cell / 2 - 4, room for corridors)",
                    spec.size, spec.aspect, spec.jitter
                ));
            }
            if spec.cut[0] < 0.0 || spec.cut[0] > spec.cut[1] || spec.cut[1] > 0.6 {
                e.push(format!(
                    "components.rooms.{k}: cut {:?} must be fractions 0..=0.6, min <= max",
                    spec.cut
                ));
            }
            if spec.shape == ShapeSpec::Pillars {
                if spec.pillars.spacing < spec.pillars.size + 3.0 || spec.pillars.size < 1.0 {
                    e.push(format!(
                        "components.rooms.{k}: pillar size {} must be >= 1 and spacing {} at least size + 3",
                        spec.pillars.size, spec.pillars.spacing
                    ));
                }
                if spec.cut[1] > 0.0 {
                    e.push(format!(
                        "components.rooms.{k}: pillared rooms are rectangles; use cut = [0.0, 0.0]"
                    ));
                }
            }
        }
        for (key, r) in &self.recipes {
            if r.placement == Preference::Patrol && r.patrol.is_none() {
                e.push(format!(
                    "recipes.{key}: placement = \"patrol\" needs patrol = {{ length = <m> }}"
                ));
            }
            if let Some(p) = r.patrol
                && !(6.0..=40.0).contains(&p.length)
            {
                e.push(format!("recipes.{key}.patrol.length: use 6..=40 m"));
            }
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
            if r.space.pillars {
                let spec = match r.room {
                    RoomPref::Chamber => &c.rooms.chamber,
                    RoomPref::Hall => &c.rooms.hall,
                    RoomPref::Arena => &c.rooms.arena,
                    RoomPref::Pit => &c.rooms.pit,
                };
                if spec.shape != ShapeSpec::Pillars {
                    e.push(format!(
                        "recipes.{key}: space.pillars needs the kit's {:?} rooms to have shape = \"pillars\"",
                        r.room
                    ));
                }
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
