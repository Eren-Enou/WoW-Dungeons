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
pub const GENERATOR_VERSION: u32 = 1;

pub struct Generated {
    pub plan: Plan,
    pub content: Content,
    /// The dungeon file, exactly as written to disk.
    pub toml: String,
    pub owners: Vec<Owner>,
    /// Room of each encounter (same order as `content.encounters`).
    pub encounter_rooms: Vec<usize>,
    pub structure: CheckReport,
    pub space: CheckReport,
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
            Err(why) => {
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

/// Compose, populate, serialise, reload and check one plan.
fn finish(bp: &Blueprint, kit: &Kit, plan: Plan) -> Result<Generated, String> {
    let comp = compose::compose(&plan, kit);
    let pop = populate::populate(bp, kit, &plan, &comp);
    let level = bp.blueprint.level;
    let content = Content {
        dungeon: DungeonConfig {
            id: bp.blueprint.id.clone(),
            name: bp.blueprint.name.clone(),
            description: bp.blueprint.description.clone(),
            recommended_level: level,
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
            theme: kit.palette.theme,
        },
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
    // Only the templates the dungeon uses (plus what they summon).
    let content = prune_templates(content);
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
        return Err(structure.errors.join("\n"));
    }
    let space = check::encounters(&plan, &loaded, &pop.rooms);
    if !space.ok() {
        return Err(space.errors.join("\n"));
    }
    Ok(Generated {
        plan,
        content: loaded,
        toml,
        owners: comp.owners,
        encounter_rooms: pop.rooms,
        structure,
        space,
    })
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
         # This is an ordinary dungeon file: edit it freely, but regenerating\n\
         # with dungeon-forge overwrites it. See DUNGEON_FORGE_0_6.md.\n\n",
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
    )
}
