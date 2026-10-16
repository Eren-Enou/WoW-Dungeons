//! The pipeline: plan → compose → populate → serialise → load through the
//! game's own loader → structural and encounter checks. Any failure moves to
//! the next attempt; after `MAX_ATTEMPTS` the error names every cause.

use crate::blueprint::Blueprint;
use crate::check::{self, CheckReport};
use crate::compose::{self, Owner};
use crate::kit::Kit;
use crate::layout::{self, FailureTally, MAX_ATTEMPTS, Plan};
use crate::populate;
use dungeon_sim::content::{Content, Difficulty, DungeonConfig, NavConfig, RewardConfig};

/// Bumped whenever the same blueprint + seed would produce different output.
pub const GENERATOR_VERSION: u32 = 4;

pub struct Generated {
    pub plan: Plan,
    pub content: Content,
    /// The dungeon file, exactly as written to disk.
    pub toml: String,
    pub owners: Vec<Owner>,
    /// Room of each encounter (same order as `content.encounters`).
    pub encounter_rooms: Vec<usize>,
    /// Pack centre and facing (toward the party's approach) per encounter.
    pub anchors: Vec<(dungeon_sim::math::V2, dungeon_sim::math::V2)>,
    /// Pillar holes per room (world space).
    pub pillars: Vec<Vec<dungeon_sim::nav::Rect>>,
    pub structure: CheckReport,
    pub space: CheckReport,
    /// Branch purposes as measured (0.9), one per plan branch.
    pub branches: CheckReport,
    pub measured: Vec<crate::purpose::Measured>,
}

fn difficulties() -> Vec<Difficulty> {
    vec![
        Difficulty::normal(),
        Difficulty {
            id: "heroic".into(),
            name: "Heroic".into(),
            hp: 1.5,
            damage: 1.35,
            level_bonus: 2,
            reward_multiplier: 2.0,
            rarity_bonus: 1,
        },
        Difficulty {
            id: "raid10".into(),
            name: "Raid (10)".into(),
            hp: 2.6,
            damage: 1.6,
            level_bonus: 1,
            reward_multiplier: 1.5,
            rarity_bonus: 1,
        },
    ]
}

/// Problems with the blueprint as used with this kit.
pub fn validate_pair(bp: &Blueprint, kit: &Kit) -> Vec<String> {
    let mut e = bp.validate();
    for (list, what) in [(&bp.encounters, "encounters"), (&bp.optional, "optional")] {
        for (i, s) in list.iter().enumerate() {
            if !kit.recipes.contains_key(&s.recipe) {
                e.push(format!(
                    "{what}[{i}].recipe '{}' is not in kit '{}'; known recipes: {}",
                    s.recipe,
                    kit.kit.id,
                    kit.recipes.keys().cloned().collect::<Vec<_>>().join(", ")
                ));
            }
        }
    }
    if let Some(last) = bp.encounters.last()
        && kit.recipes.contains_key(&last.recipe)
        && !kit.recipe_has_boss(&last.recipe)
    {
        e.push(format!(
            "encounters: the last one ('{}') must be a boss recipe - it is the final boss",
            last.recipe
        ));
    }
    // The identity's promised challenges must be delivered by some encounter.
    let used: std::collections::BTreeSet<crate::kit::Tag> = bp
        .encounters
        .iter()
        .chain(&bp.optional)
        .filter_map(|s| kit.recipes.get(&s.recipe))
        .flat_map(|r| r.tags.iter().copied())
        .collect();
    for c in &bp.identity.challenges {
        if !used.contains(c) {
            let offering: Vec<&String> = kit
                .recipes
                .iter()
                .filter(|(_, r)| r.tags.contains(c))
                .map(|(k, _)| k)
                .collect();
            e.push(format!(
                "identity.challenges promises {c:?} but no encounter delivers it; recipes tagged with it: {}",
                if offering.is_empty() {
                    "none in this kit".to_string()
                } else {
                    offering.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                }
            ));
        }
    }
    // Purposes the kit must support (0.9).
    if bp
        .routes
        .iter()
        .any(|r| r.purpose == crate::blueprint::RoutePurpose::Safe)
        && kit.remix.route_hazard.is_none()
    {
        e.push(format!(
            "routes: a \"safe\" route needs a hazard on its short way, and kit '{}' defines no [remix.route_hazard]",
            kit.kit.id
        ));
    }
    let caches = bp.routes.iter().any(|r| r.cache)
        || bp
            .side
            .iter()
            .any(|s| s.purpose == crate::blueprint::SidePurpose::Cache);
    if caches && kit.remix.cache.is_none() {
        e.push(format!(
            "a cache needs the kit's [remix.cache] (name and model); kit '{}' has none",
            kit.kit.id
        ));
    }
    for (k, r) in bp.routes.iter().enumerate() {
        if let Some(o) = r.optional
            && let Some(slot) = bp.optional.get(o.wrapping_sub(1))
            && kit.recipes.get(&slot.recipe).is_some_and(|x| {
                !matches!(
                    x.room,
                    crate::kit::RoomPref::Chamber | crate::kit::RoomPref::Hall
                )
            })
        {
            e.push(format!(
                "routes[{k}]: optional {o} ('{}') needs a chamber or hall recipe to sit on a detour",
                slot.recipe
            ));
        }
    }
    for (i, s) in bp.optional.iter().enumerate() {
        if kit
            .recipes
            .get(&s.recipe)
            .is_some_and(|r| r.room == crate::kit::RoomPref::Arena)
        {
            e.push(format!(
                "optional[{i}].recipe '{}' needs an arena; side rooms are small - use a chamber or hall recipe",
                s.recipe
            ));
        }
    }
    e
}

pub fn generate(bp: &Blueprint, kit: &Kit, seed: u64) -> Result<Generated, String> {
    let problems = validate_pair(bp, kit);
    if !problems.is_empty() {
        return Err(format!(
            "blueprint '{}' has {} problem(s):\n  - {}",
            bp.blueprint.id,
            problems.len(),
            problems.join("\n  - ")
        ));
    }
    let mut tally = FailureTally::default();
    let mut first_validation: Option<String> = None;
    for attempt in 0..MAX_ATTEMPTS {
        let plan = match layout::plan_attempt(bp, kit, seed, attempt) {
            Ok(p) => p,
            Err(why) => {
                tally.add(why);
                continue;
            }
        };
        match finish(bp, kit, plan) {
            Ok(g) => return Ok(g),
            Err(Failure::Space(what)) => tally.add(format!("space: {what}")),
            Err(Failure::Purpose(what)) => tally.add(format!("purpose: {what}")),
            Err(Failure::Invalid(why)) => {
                tally.add("validation");
                first_validation.get_or_insert(why);
            }
        }
    }
    let causes: Vec<String> = tally
        .counts
        .iter()
        .map(|(k, n)| format!("{k}: {n}/{MAX_ATTEMPTS}"))
        .collect();
    let mut msg = format!(
        "could not generate '{}' with seed {seed} in {MAX_ATTEMPTS} attempts ({}). {}",
        bp.blueprint.id,
        causes.join("; "),
        layout::advice(&tally)
    );
    if let Some(v) = first_validation {
        msg.push_str(&format!("\nFirst validation failure:\n{v}"));
    }
    Err(msg)
}

enum Failure {
    /// An encounter found no spot satisfying its recipe (retry).
    Space(String),
    /// A branch does not deliver its declared purpose (retry).
    Purpose(String),
    /// The composed dungeon failed validation (a generator bug).
    Invalid(String),
}

impl From<String> for Failure {
    fn from(s: String) -> Self {
        Failure::Invalid(s)
    }
}

fn dungeon_config(bp: &Blueprint, kit: &Kit, comp: &compose::Composed) -> DungeonConfig {
    DungeonConfig {
        id: bp.blueprint.id.clone(),
        name: bp.blueprint.name.clone(),
        description: bp.blueprint.description.clone(),
        recommended_level: bp.blueprint.level,
        recommended_group: bp.blueprint.group_size,
        hp_multiplier: 1.0,
        damage_multiplier: 1.0,
        entrance: comp.entrance,
        entrance_y: None,
        leash_radius: 50.0,
        areas: comp.areas.clone(),
        links: comp.links.clone(),
        nav: NavConfig::default(),
        lava: comp.lava.clone(),
        decor: comp.decor.clone(),
        lights: comp.lights.clone(),
        props: comp.props.clone(),
        theme: kit.palette.theme,
        generated: None,
        rooms: Vec::new(),
        atmosphere: Default::default(),
        modules: None,
        anchors: Vec::new(),
        branches: Vec::new(),
        caches: Vec::new(),
    }
}

/// Compose, populate, serialise, reload and check one plan.
fn finish(bp: &Blueprint, kit: &Kit, plan: Plan) -> Result<Generated, Failure> {
    let comp = compose::compose(&plan, kit);
    // Placement needs the navigation (patrol routes, sightlines): build it
    // from the composed geometry alone.
    let shell = Content {
        dungeon: dungeon_config(bp, kit, &comp),
        difficulties: Vec::new(),
        templates: Default::default(),
        encounters: Vec::new(),
        loot: Vec::new(),
        rewards: RewardConfig {
            marks_per_clear: 0,
            marks_per_wipe: 0,
            xp_per_clear: 0,
        },
    };
    let nav = shell
        .build_nav()
        .map_err(|e| Failure::Invalid(e.join("\n")))?;
    let pop = populate::populate(bp, kit, &plan, &comp, &nav).map_err(Failure::Space)?;
    let level = bp.blueprint.level;
    let content = Content {
        dungeon: dungeon_config(bp, kit, &comp),
        difficulties: difficulties(),
        templates: kit.scaled_templates(level),
        encounters: pop.encounters,
        loot: pop.loot,
        rewards: RewardConfig {
            marks_per_clear: 8 + 2 * level,
            marks_per_wipe: 1,
            xp_per_clear: 200 + 200 * level,
        },
    };
    // Presentation (0.8): where it came from, its rooms, the kit's
    // atmosphere and modular art, and the decoration anchors.
    let mut content = content;
    content.dungeon.generated = Some(dungeon_sim::content::GeneratedInfo {
        blueprint: bp.blueprint.id.clone(),
        blueprint_path: bp.source.clone(),
        seed: plan.seed,
        kit: kit.kit.id.clone(),
        generator: GENERATOR_VERSION,
    });
    content.dungeon.caches = pop.caches.clone();
    content.dungeon.rooms = rooms_config(&plan, &comp, &content, &pop.rooms);
    content.dungeon.atmosphere = kit.atmosphere.clone();
    content.dungeon.modules = kit.modules.clone();
    content.dungeon.anchors = comp.anchors.clone();
    // Only the templates the dungeon uses (plus what they summon).
    let mut content = prune_templates(content);
    // Branch purposes, measured on this layout's navigation (0.9).
    let nav0 = content
        .build_nav()
        .map_err(|e| Failure::Invalid(e.join("\n")))?;
    let (branches, measured) = crate::purpose::measure(
        &plan,
        &comp,
        kit,
        &content,
        &nav0,
        &pop.rooms,
        &pop.anchors,
        &pop.cache_rooms,
        bp.purposeful(),
    );
    if let Some(e) = branches.errors.first() {
        // Keep the tally key short: "branch bK (purpose, ...): what".
        let why = e.split_once("): ").map(|(a, b)| {
            let kind = a
                .split('(')
                .nth(1)
                .and_then(|k| k.split(',').next())
                .unwrap_or("?");
            format!("{kind}: {}", b.split(" (").next().unwrap_or(b))
        });
        return Err(Failure::Purpose(why.unwrap_or_else(|| e.clone())));
    }
    content.dungeon.branches = crate::purpose::branch_configs(&plan, &measured);
    let body = content.to_toml()?;
    let toml = format!("{}{body}", header(bp, &plan));
    // The game's loader is the first validator.
    let loaded = Content::from_toml_str(
        &toml,
        &format!("<generated {} seed {}>", bp.blueprint.id, plan.seed),
    )
    .map_err(|e| e.to_string())?;
    let nav = loaded.nav();
    let structure = check::structure(&plan, &comp.owners, &loaded, &nav);
    if !structure.ok() {
        return Err(Failure::Invalid(structure.errors.join("\n")));
    }
    let space = check::encounters(&plan, kit, bp, &loaded, &nav, &pop.rooms);
    if !space.ok() {
        return Err(Failure::Invalid(space.errors.join("\n")));
    }
    Ok(Generated {
        plan,
        content: loaded,
        toml,
        owners: comp.owners,
        encounter_rooms: pop.rooms,
        anchors: pop.anchors,
        pillars: comp.pillars,
        structure,
        space,
        branches,
        measured,
    })
}

/// `[[dungeon.rooms]]`: every plan room and corridor with its areas.
fn rooms_config(
    plan: &Plan,
    comp: &compose::Composed,
    content: &Content,
    encounter_rooms: &[usize],
) -> Vec<dungeon_sim::content::RoomConfig> {
    use crate::layout::{EdgeRole, Route, Style};
    let areas_of = |o: Owner| -> Vec<String> {
        comp.areas
            .iter()
            .zip(&comp.owners)
            .filter(|(_, ow)| **ow == o)
            .map(|(a, _)| a.name.clone())
            .collect()
    };
    let mut out = Vec::new();
    for (i, r) in plan.rooms.iter().enumerate() {
        let (route, order) = match r.route {
            Route::Main(k) => ("main", Some(k as u32)),
            Route::Alternate => ("alternate", None),
            Route::Side => ("side", None),
            Route::Overlook => ("overlook", None),
        };
        let (purpose, branch) = match r.branch {
            Some(b) => (plan.branches[b].kind.label().to_string(), format!("b{b}")),
            None => ("main".to_string(), String::new()),
        };
        let encounter = encounter_rooms
            .iter()
            .position(|&er| er == i)
            .map(|k| content.encounters[k].id.clone());
        out.push(dungeon_sim::content::RoomConfig {
            id: format!("r{i}"),
            name: r.name.clone(),
            role: crate::decorate::room_role(r.kind).into(),
            route: route.into(),
            order,
            areas: areas_of(Owner::Room(i)),
            connects: Vec::new(),
            encounter,
            atmosphere: None,
            purpose,
            branch,
        });
    }
    for (k, c) in plan.corridors.iter().enumerate() {
        let areas = areas_of(Owner::Corridor(k));
        if areas.is_empty() {
            continue;
        }
        let role = match c.style {
            Style::Straight | Style::Winding | Style::Angled => "corridor",
            Style::Ramp if c.stairs => "stair",
            Style::Ramp => "ramp",
            Style::Bridge => "bridge",
            Style::Drop => "ledge",
        };
        let route = match c.role {
            EdgeRole::Main => "main",
            EdgeRole::Alternate => "alternate",
            EdgeRole::Side => "side",
            EdgeRole::Shortcut => "shortcut",
            EdgeRole::Loop => "loop",
        };
        let owner = plan.branches.iter().position(|b| b.corridors.contains(&k));
        let (purpose, branch) = match owner {
            Some(b) => (plan.branches[b].kind.label().to_string(), format!("b{b}")),
            None => ("main".to_string(), String::new()),
        };
        out.push(dungeon_sim::content::RoomConfig {
            id: format!("c{k}"),
            name: format!("{} - {}", plan.rooms[c.a].name, plan.rooms[c.b].name),
            role: role.into(),
            route: route.into(),
            order: None,
            areas,
            connects: vec![format!("r{}", c.a), format!("r{}", c.b)],
            encounter: None,
            atmosphere: None,
            purpose,
            branch,
        });
    }
    out
}

fn prune_templates(mut c: Content) -> Content {
    let mut keep: std::collections::BTreeSet<String> = c
        .encounters
        .iter()
        .flat_map(|e| e.mobs.iter().map(|m| m.template.clone()))
        .collect();
    loop {
        let mut more = Vec::new();
        for k in &keep {
            if let Some(t) = c.templates.get(k) {
                for a in &t.abilities {
                    if let Some(s) = &a.template {
                        more.push(s.clone());
                    }
                }
                for p in &t.phases {
                    for s in &p.summon {
                        more.push(s.template.clone());
                    }
                }
            }
        }
        let before = keep.len();
        keep.extend(more);
        if keep.len() == before {
            break;
        }
    }
    c.templates.retain(|k, _| keep.contains(k));
    c
}

fn header(bp: &Blueprint, plan: &Plan) -> String {
    use crate::layout::{EdgeRole, Style};
    let count = |f: &dyn Fn(&crate::layout::Corridor) -> bool| {
        plan.corridors.iter().filter(|c| f(c)).count()
    };
    format!(
        "# {name} - generated by Dungeon Forge (generator v{GENERATOR_VERSION}).\n\
         # Blueprint '{id}', seed {seed} (layout attempt {attempt}).\n\
         # Main route {main} rooms; {alt} alternate-route rooms; {side} side rooms;\n\
         # {ramps} ramps, {bridges} bridges, {drops} one-way drops, {loops} return loops.\n\
         # Branches: {branches}.\n\
         # This is an ordinary dungeon file: edit it freely, but regenerating\n\
         # with dungeon-forge overwrites it. See DUNGEON_FORGE_0_7.md.\n\n",
        name = bp.blueprint.name,
        id = bp.blueprint.id,
        seed = plan.seed,
        attempt = plan.attempt,
        main = plan.main.len(),
        alt = plan
            .rooms
            .iter()
            .filter(|r| r.route == crate::layout::Route::Alternate)
            .count(),
        side = plan
            .rooms
            .iter()
            .filter(|r| r.route == crate::layout::Route::Side)
            .count(),
        ramps = count(&|c| c.style == Style::Ramp),
        bridges = count(&|c| c.style == Style::Bridge),
        drops = count(&|c| c.style == Style::Drop),
        loops = count(&|c| c.role == EdgeRole::Loop),
        branches = if plan.branches.is_empty() {
            "none".to_string()
        } else {
            plan.branches
                .iter()
                .map(|b| b.kind.label())
                .collect::<Vec<_>>()
                .join(", ")
        },
    )
}
