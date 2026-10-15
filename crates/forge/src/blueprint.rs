//! Layer A: the blueprint describes the intended experience (theme, level,
//! route shape, encounters in order) without any world coordinates.

use crate::kit::Tag;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Blueprint {
    pub blueprint: BlueprintMeta,
    /// What the dungeon is for: the experience and the challenges it promises.
    #[serde(default)]
    pub identity: Identity,
    #[serde(default)]
    pub layout: LayoutSpec,
    /// Main-route encounters in order. The last one holds the final boss.
    pub encounters: Vec<EncounterSlot>,
    /// Optional encounters, each in its own side chamber off the main route.
    #[serde(default)]
    pub optional: Vec<EncounterSlot>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BlueprintMeta {
    /// Dungeon id (saves, command line). Lowercase letters, digits, `_`.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Theme library: a name (`assets/forge/kits/<kit>.toml`, found next to
    /// the blueprint directory) or a path ending in `.toml`.
    pub kit: String,
    pub level: u32,
    #[serde(default = "five")]
    pub group_size: u32,
    /// Default generator seed (the command line can override it).
    #[serde(default = "one")]
    pub seed: u64,
}

fn five() -> u32 {
    5
}

fn one() -> u64 {
    1
}

/// The design identity: a sentence for people, and the tactical challenges
/// the encounters must cover (checked against recipe tags).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct Identity {
    pub summary: String,
    pub challenges: Vec<Tag>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Shortcut {
    #[default]
    None,
    /// A ledge reached from an earlier room that drops (one-way) into a later,
    /// lower room of the main route.
    Drop,
    /// A walkable return corridor between two non-consecutive main-route rooms.
    Loop,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct LayoutSpec {
    /// Empty rooms inserted on the main route, `[min, max]`.
    pub extra_rooms: [u32; 2],
    /// Elevation steps down along the main route (ramps).
    pub descents: u32,
    /// A second way between two main-route rooms (same as alternate_routes = 1).
    pub alternate_route: bool,
    /// How many second ways between main-route rooms.
    pub alternate_routes: u32,
    pub shortcut: Shortcut,
    /// Main-route corridors that cross a chasm on an open bridge.
    pub bridges: u32,
    /// Share (0..=1) of flat corridors that wind or angle.
    pub winding: f32,
    /// 0 = the route turns often (winding caverns), 1 = it runs straight
    /// (a processional axis).
    pub straightness: f32,
    /// Empty dead-end chambers off the route to explore, `[min, max]`.
    pub side_rooms: [u32; 2],
    /// Chance (0..=1) of a passage bend before each main-route room: longer,
    /// winding approaches.
    pub passages: f32,
}

impl LayoutSpec {
    /// Number of alternate routes asked for (either field).
    pub fn alternates(&self) -> u32 {
        self.alternate_routes.max(self.alternate_route as u32)
    }
}

impl Default for LayoutSpec {
    fn default() -> Self {
        LayoutSpec {
            extra_rooms: [0, 1],
            descents: 1,
            alternate_route: false,
            alternate_routes: 0,
            shortcut: Shortcut::None,
            bridges: 0,
            winding: 0.3,
            straightness: 0.4,
            side_rooms: [0, 0],
            passages: 0.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EncounterSlot {
    /// Recipe key in the kit.
    pub recipe: String,
    /// Display name override (default: the recipe's name).
    #[serde(default)]
    pub name: Option<String>,
}

/// Hard limits that keep generation bounded and the result playable.
pub const MAX_MAIN_ROOMS: usize = 14;
pub const MAX_OPTIONAL: usize = 4;

impl Blueprint {
    pub fn load(path: &Path) -> Result<Blueprint, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("could not read blueprint '{}': {e}", path.display()))?;
        Blueprint::from_toml_str(&text, &path.display().to_string())
    }

    pub fn from_toml_str(text: &str, origin: &str) -> Result<Blueprint, String> {
        toml::from_str(text)
            .map_err(|e| format!("blueprint '{origin}' is not valid TOML for the schema:\n{e}"))
    }

    /// Where the kit lives, relative to the blueprint file.
    pub fn kit_path(&self, blueprint_path: &Path) -> PathBuf {
        let dir = blueprint_path.parent().unwrap_or(Path::new("."));
        let k = &self.blueprint.kit;
        if k.ends_with(".toml") || k.contains('/') || k.contains('\\') {
            dir.join(k)
        } else {
            dir.join("..").join("kits").join(format!("{k}.toml"))
        }
    }

    /// Checks that need no kit. Every problem is listed with its field.
    pub fn validate(&self) -> Vec<String> {
        let mut e = Vec::new();
        let b = &self.blueprint;
        if b.id.is_empty()
            || !b
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            e.push(format!(
                "blueprint.id = '{}': use lowercase letters, digits and '_' only",
                b.id
            ));
        }
        if b.name.trim().is_empty() {
            e.push("blueprint.name must not be empty".into());
        }
        if !(1..=10).contains(&b.level) {
            e.push(format!("blueprint.level = {}: use 1..=10", b.level));
        }
        if b.group_size != 5 && b.group_size != 10 {
            e.push(format!(
                "blueprint.group_size = {}: use 5 (party) or 10 (raid)",
                b.group_size
            ));
        }
        let l = &self.layout;
        if l.extra_rooms[0] > l.extra_rooms[1] {
            e.push(format!(
                "layout.extra_rooms = {:?}: min must not exceed max",
                l.extra_rooms
            ));
        }
        if !(0.0..=1.0).contains(&l.winding) {
            e.push(format!("layout.winding = {}: use 0.0..=1.0", l.winding));
        }
        if !(0.0..=1.0).contains(&l.straightness) {
            e.push(format!(
                "layout.straightness = {}: use 0.0..=1.0",
                l.straightness
            ));
        }
        if !(0.0..=0.6).contains(&l.passages) {
            e.push(format!("layout.passages = {}: use 0.0..=0.6", l.passages));
        }
        if l.side_rooms[0] > l.side_rooms[1] || l.side_rooms[1] > 4 {
            e.push(format!(
                "layout.side_rooms = {:?}: min <= max <= 4",
                l.side_rooms
            ));
        }
        if l.alternates() > 3 {
            e.push(format!(
                "layout.alternate_routes = {}: at most 3",
                l.alternates()
            ));
        }
        if self.encounters.is_empty() {
            e.push("encounters: at least one (the final boss) is required".into());
        }
        let main = 1 + self.encounters.len() + l.extra_rooms[1] as usize;
        if main > MAX_MAIN_ROOMS {
            e.push(format!(
                "the main route would have up to {main} rooms (entrance + {} encounters + {} extra); the limit is {MAX_MAIN_ROOMS} - use fewer encounters or extra_rooms",
                self.encounters.len(),
                l.extra_rooms[1]
            ));
        }
        if self.optional.len() > MAX_OPTIONAL {
            e.push(format!(
                "optional: {} side encounters; the limit is {MAX_OPTIONAL}",
                self.optional.len()
            ));
        }
        let edges = main.saturating_sub(1) as u32;
        if l.descents > edges {
            e.push(format!(
                "layout.descents = {}: the main route has at most {edges} corridors",
                l.descents
            ));
        }
        if l.bridges + l.descents > edges {
            e.push(format!(
                "layout.bridges ({}) + descents ({}) exceed the main route's {edges} corridors (a bridge is flat, a descent is a ramp)",
                l.bridges, l.descents
            ));
        }
        if l.shortcut == Shortcut::Drop && l.descents == 0 {
            e.push(
                "layout.shortcut = \"drop\" needs descents >= 1 (a drop must land on a lower room)"
                    .into(),
            );
        }
        e
    }
}
