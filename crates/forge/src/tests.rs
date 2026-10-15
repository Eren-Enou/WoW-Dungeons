//! Forge tests: determinism, variety, validation of good and bad inputs, and
//! that generated dungeons load and are cleared by simulated parties.

use crate::blueprint::Blueprint;
use crate::build;
use crate::kit::Kit;
use crate::layout::{Route, Style};

const KIT: &str = include_str!("../../../assets/forge/kits/volcanic.toml");
const BP: &str = include_str!("../../../assets/forge/blueprints/emberforge_vaults.toml");

fn kit() -> Kit {
    Kit::from_toml_str(KIT, "volcanic").expect("kit is valid")
}

fn bp() -> Blueprint {
    Blueprint::from_toml_str(BP, "emberforge").expect("blueprint parses")
}

#[test]
fn shipped_blueprint_and_kit_validate() {
    assert!(build::validate_pair(&bp(), &kit()).is_empty());
}

#[test]
fn same_seed_same_dungeon_byte_for_byte() {
    let (b, k) = (bp(), kit());
    let a = build::generate(&b, &k, 7).expect("seed 7");
    let c = build::generate(&b, &k, 7).expect("seed 7 again");
    assert_eq!(a.toml, c.toml);
}

#[test]
fn seeds_give_different_layouts_with_every_feature() {
    let (b, k) = (bp(), kit());
    let mut shapes = std::collections::BTreeSet::new();
    for seed in 1..=8 {
        let g = build::generate(&b, &k, seed).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let p = &g.plan;
        let cells: Vec<(i32, i32)> = p.main.iter().map(|&r| p.rooms[r].cell).collect();
        shapes.insert(cells);
        // Blueprint features are present in every layout.
        assert!(
            p.rooms.iter().any(|r| r.route == Route::Alternate),
            "seed {seed}: alternate route"
        );
        assert!(
            p.rooms.iter().any(|r| r.route == Route::Side),
            "seed {seed}: side room"
        );
        assert!(
            p.corridors.iter().any(|c| c.style == Style::Drop),
            "seed {seed}: drop"
        );
        assert!(
            p.corridors.iter().any(|c| c.style == Style::Bridge),
            "seed {seed}: bridge"
        );
        assert!(
            p.corridors
                .iter()
                .filter(|c| c.style == Style::Ramp)
                .count()
                >= 2,
            "seed {seed}: ramps"
        );
        assert!(g.structure.ok() && g.space.ok());
    }
    assert!(
        shapes.len() >= 6,
        "only {} distinct main routes in 8 seeds",
        shapes.len()
    );
}

#[test]
fn generated_dungeon_has_the_planned_navigation() {
    let g = build::generate(&bp(), &kit(), 3).expect("seed 3");
    let nav = g.content.nav();
    // One-way drop: a drop link exists and nothing walks back up it.
    let drops: Vec<_> = nav
        .links
        .iter()
        .filter(|l| l.kind == dungeon_sim::nav::LinkKind::Drop)
        .collect();
    assert_eq!(drops.len(), 1);
    let d = drops[0];
    assert!(!nav.links.iter().any(|l| l.from == d.to && l.to == d.from));
    assert!(d.entry.y - d.exit.y >= crate::layout::MIN_FALL - 0.01);
    // The final boss is the last required encounter and stands in the arena.
    let fin = &g.content.encounters[g.content.final_encounter()];
    assert!(fin.mobs.iter().any(|m| g.content.templates[&m.template].rank
        == dungeon_sim::content::EnemyRank::Boss));
}

#[test]
fn generated_dungeon_round_trips_through_the_loader() {
    let g = build::generate(&bp(), &kit(), 2).expect("seed 2");
    let again = dungeon_sim::Content::from_toml_str(&g.toml, "again").expect("loads");
    assert_eq!(again.dungeon.areas.len(), g.content.dungeon.areas.len());
    assert_eq!(again.encounters.len(), 7);
}

#[test]
fn bad_blueprints_are_rejected_with_reasons() {
    let k = kit();
    let mut b = bp();
    b.encounters.last_mut().unwrap().recipe = "melee_pack".into();
    let e = build::validate_pair(&b, &k);
    assert!(
        e.iter().any(|m| m.contains("must be a boss recipe")),
        "{e:?}"
    );

    let mut b = bp();
    b.encounters[0].recipe = "dragon_pack".into();
    let e = build::validate_pair(&b, &k);
    assert!(
        e.iter().any(|m| m.contains("'dragon_pack' is not in kit")),
        "{e:?}"
    );

    let mut b = bp();
    b.layout.descents = 40;
    b.layout.winding = 2.0;
    b.blueprint.id = "Bad Id".into();
    let e = build::validate_pair(&b, &k);
    assert!(e.iter().any(|m| m.contains("layout.descents")), "{e:?}");
    assert!(e.iter().any(|m| m.contains("layout.winding")), "{e:?}");
    assert!(e.iter().any(|m| m.contains("blueprint.id")), "{e:?}");
    assert!(build::generate(&b, &k, 1).is_err());

    let mut b = bp();
    b.optional[0].recipe = "final_cindermaw".into();
    let e = build::validate_pair(&b, &k);
    assert!(e.iter().any(|m| m.contains("needs an arena")), "{e:?}");
}

#[test]
fn impossible_layouts_fail_with_a_diagnosis_not_a_loop() {
    let mut b = bp();
    // Too many side rooms for the free cells around a short route.
    b.encounters.truncate(1);
    b.encounters[0].recipe = "final_cindermaw".into();
    b.layout.extra_rooms = [0, 0];
    b.layout.descents = 0;
    b.layout.bridges = 0;
    b.layout.alternate_route = true;
    b.layout.shortcut = crate::blueprint::Shortcut::Loop;
    b.identity.challenges.clear();
    let err = match build::generate(&b, &kit(), 1) {
        Ok(_) => panic!("a two-room dungeon has no alternate route or loop"),
        Err(e) => e,
    };
    assert!(err.contains("attempts"), "{err}");
    assert!(
        err.contains("alternate route") || err.contains("shortcut"),
        "{err}"
    );
    assert!(err.contains("Suggestions"), "{err}");
}

#[test]
fn bad_kits_are_rejected_with_reasons() {
    let broken = KIT.replace(
        "template = \"pyrecaller\", place = \"back\"",
        "template = \"pyrocaller\", place = \"back\"",
    );
    let err = Kit::from_toml_str(&broken, "broken").expect_err("unknown template");
    assert!(err.contains("'pyrocaller' is not a kit template"), "{err}");
    let broken = KIT.replace("room = \"arena\"", "room = \"hall\"");
    let err = Kit::from_toml_str(&broken, "broken").expect_err("boss in a hall");
    assert!(err.contains("a boss recipe needs room"), "{err}");
}

#[test]
fn templates_scale_to_the_blueprint_level() {
    let k = kit();
    let t5 = &k.scaled_templates(5)["raider"];
    let t3 = &k.scaled_templates(3)["raider"];
    assert_eq!(t5.hp, k.templates["raider"].hp);
    assert!((t3.hp / t5.hp - 1.2 / 1.4).abs() < 0.01);
    assert_eq!(t3.level, 3);
}

#[test]
fn five_player_parties_clear_generated_layouts() {
    let (b, k) = (bp(), kit());
    for seed in [1u64, 2] {
        let g = build::generate(&b, &k, seed).expect("generates");
        let party = dungeon_sim::runner::party_at_level(
            dungeon_sim::ClassId::Striker,
            b.blueprint.level,
            dungeon_sim::AiProfile::Balanced,
        );
        let (_, s) = dungeon_sim::runner::run_headless(&g.content, &party, 1, 1500.0);
        assert_eq!(
            s.outcome,
            dungeon_sim::RunOutcome::Victory,
            "layout seed {seed}: cleared {}/{} in {:.0}s, stuck {:.1}s",
            s.encounters_cleared,
            s.encounters_total,
            s.duration,
            s.nav_stuck_seconds
        );
        assert!(s.nav_stuck_seconds < 10.0);
    }
}

#[test]
fn ten_player_raids_clear_generated_layouts_without_loops() {
    // Regression (0.6): companions committed to a stale point at a ramp
    // mouth shuttled for 20+ minutes (the dither commitment renewed itself
    // with its own target). Found by `dungeon-forge test` on layout 8.
    let (b, k) = (bp(), kit());
    for seed in [6u64, 8] {
        let g = build::generate(&b, &k, seed).expect("generates");
        let party = dungeon_sim::runner::raid_party(
            10,
            b.blueprint.level,
            dungeon_sim::AiProfile::Balanced,
        );
        for sim_seed in 1..=3 {
            let (_, s) = dungeon_sim::runner::run_headless(&g.content, &party, sim_seed, 1500.0);
            assert_eq!(
                s.outcome,
                dungeon_sim::RunOutcome::Victory,
                "layout {seed} sim {sim_seed}: cleared {}/{}, dithers {}",
                s.encounters_cleared,
                s.encounters_total,
                s.nav_dithers
            );
            assert!(
                s.nav_dithers < 1500,
                "layout {seed} sim {sim_seed}: {} dithers",
                s.nav_dithers
            );
        }
    }
}

// ---- 0.7: a second kit, Recipes 2.0, identity, analysis.

const CRYPT: &str = include_str!("../../../assets/forge/kits/crypt.toml");
const RELIQUARY: &str = include_str!("../../../assets/forge/blueprints/hollow_reliquary.toml");

fn crypt() -> Kit {
    Kit::from_toml_str(CRYPT, "crypt").expect("crypt kit is valid")
}

fn reliquary() -> Blueprint {
    Blueprint::from_toml_str(RELIQUARY, "reliquary").expect("blueprint parses")
}

#[test]
fn crypt_layouts_are_deterministic_and_deliver_their_identity() {
    let (b, k) = (reliquary(), crypt());
    assert!(build::validate_pair(&b, &k).is_empty());
    let a = build::generate(&b, &k, 4).expect("seed 4");
    let again = build::generate(&b, &k, 4).expect("seed 4 again");
    assert_eq!(a.toml, again.toml);
    for seed in 1..=4 {
        let g = build::generate(&b, &k, seed).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        assert!(g.structure.ok() && g.space.ok(), "seed {seed}");
        // Pillared halls: pillars exist and their holes are not walkable.
        let count: usize = g.pillars.iter().map(|p| p.len()).sum();
        assert!(count >= 8, "seed {seed}: {count} pillars");
        let nav = g.content.nav();
        for (room, holes) in g.pillars.iter().enumerate() {
            let y = g.plan.rooms[room].y;
            for h in holes {
                let mid = (h.min + h.max) * 0.5;
                assert!(
                    nav.locate(mid, y).is_none(),
                    "seed {seed}: pillar at {mid:?} is floor"
                );
            }
        }
        // A patrol walks its route; the council has two bosses; the
        // guardian's wail can be hidden from.
        let c = &g.content;
        assert!(
            c.encounters
                .iter()
                .any(|e| e.mobs.iter().any(|m| !m.patrol.is_empty()))
        );
        let council = c
            .encounters
            .iter()
            .find(|e| e.id.ends_with("twin_abbots"))
            .expect("council");
        let bosses = council
            .mobs
            .iter()
            .filter(|m| c.templates[&m.template].rank == dungeon_sim::content::EnemyRank::Boss)
            .count();
        assert_eq!(bosses, 2, "seed {seed}");
        assert!(
            c.templates
                .values()
                .any(|t| t.abilities.iter().any(|a| a.los_blockable))
        );
    }
}

#[test]
fn the_two_kits_build_different_dungeons() {
    let ember = build::generate(&bp(), &kit(), 1).expect("emberforge");
    let crypt = build::generate(&reliquary(), &crypt(), 1).expect("reliquary");
    assert_ne!(ember.plan.cell, crypt.plan.cell);
    // The crypt has pillared halls and a void chasm; the vaults have lava.
    assert!(ember.pillars.iter().all(|p| p.is_empty()));
    assert!(crypt.pillars.iter().any(|p| !p.is_empty()));
    assert!(!ember.content.dungeon.lava.is_empty());
    assert!(crypt.content.dungeon.lava.is_empty());
    // Different enemies entirely.
    let names = |g: &crate::Generated| -> std::collections::BTreeSet<String> {
        g.content
            .templates
            .values()
            .map(|t| t.name.clone())
            .collect()
    };
    assert!(names(&ember).is_disjoint(&names(&crypt)));
}

#[test]
fn identity_promises_must_be_kept() {
    let (mut b, k) = (reliquary(), crypt());
    // Without the council recipe nothing delivers a council.
    b.encounters.retain(|e| e.recipe != "twin_abbots");
    let e = build::validate_pair(&b, &k);
    assert!(
        e.iter()
            .any(|m| m.contains("promises Council") && m.contains("twin_abbots")),
        "{e:?}"
    );
}

#[test]
fn encounters_that_never_fit_name_the_recipe_and_room() {
    let (b, k) = (reliquary(), crypt());
    // Archers that need a 40 m room fit nowhere in a crypt.
    let mut k2 = k.clone();
    k2.recipes.get_mut("bone_archers").unwrap().space.min_size = 40.0;
    let err = match build::generate(&b, &k2, 1) {
        Ok(_) => panic!("no crypt room is 40 m across"),
        Err(e) => e,
    };
    assert!(err.contains("bone_archers"), "{err}");
}

#[test]
fn report_attributes_clears_and_deaths_to_encounters() {
    let g = build::generate(&bp(), &kit(), 1).expect("generates");
    let config = crate::simtest::Config {
        size: 5,
        class: "striker".into(),
        profile: "balanced".into(),
        difficulty: "normal".into(),
        level: bp().blueprint.level,
    };
    let r = crate::report::report(&g.content, None, &[config], 2, 1500.0);
    assert_eq!(r.runs, 2);
    assert_eq!(r.encounters.len(), g.content.encounters.len());
    for e in r.encounters.iter().filter(|e| !e.optional) {
        assert!(e.pulls >= 1, "{} never pulled", e.id);
    }
    if r.victories == 2 {
        assert!(
            r.encounters
                .iter()
                .filter(|e| !e.optional)
                .all(|e| e.runs_cleared == 2)
        );
    }
    let deaths: u32 = r.encounters.iter().map(|e| e.deaths).sum::<u32>() + r.deaths_between;
    assert_eq!(deaths as usize, r.deaths.len());
    // The overlay map draws every marker.
    let svg = crate::map::svg_with(
        &g.content,
        Some(&g.plan),
        &crate::map::Overlay {
            deaths: &r.deaths,
            dithers: &r.dithers,
            stuck: &r.stuck,
        },
    );
    assert!(svg.ends_with("</svg>\n"));
}

#[test]
fn five_player_parties_clear_crypt_layouts() {
    let (b, k) = (reliquary(), crypt());
    let mut wins = 0;
    let mut runs = 0;
    for seed in [1u64, 2] {
        let g = build::generate(&b, &k, seed).expect("generates");
        for class in [dungeon_sim::ClassId::Striker, dungeon_sim::ClassId::Mender] {
            let party = dungeon_sim::runner::party_at_level(
                class,
                b.blueprint.level,
                dungeon_sim::AiProfile::Balanced,
            );
            for sim_seed in 1..=2 {
                let (_, s) =
                    dungeon_sim::runner::run_headless(&g.content, &party, sim_seed, 1500.0);
                runs += 1;
                wins += (s.outcome == dungeon_sim::RunOutcome::Victory) as usize;
                assert!(s.nav_stuck_seconds < 10.0, "layout {seed}: stuck");
                assert!(
                    s.encounters_cleared + 1 >= s.encounters_total,
                    "layout {seed}: wiped before the guardian"
                );
            }
        }
    }
    assert!(wins * 4 >= runs * 3, "{wins}/{runs} clears");
}

#[test]
fn designer_overrides_survive_regeneration_and_go_stale_loudly() {
    let g = build::generate(&bp(), &kit(), 1).expect("generates");
    let fin = g.content.encounters[g.content.final_encounter()].id.clone();
    let patch = format!(
        "[[encounters]]\nid = \"{fin}\"\nname = \"The Wyrm Below\"\n\n\
         [[encounters]]\nid = \"o1_optional_elite\"\nremove = true\n\n\
         [[loot]]\nid = \"emberforge_vaults_sentinel_shard\"\nsource = \"{fin}\"\n\n\
         [dungeon]\ndescription = \"Edited by hand.\"\n"
    );
    // Removing an encounter that loot still drops from is caught by the loader.
    let dangling = "[[encounters]]\nid = \"o1_optional_elite\"\nremove = true\n";
    let text = crate::overrides::apply(&g.toml, dangling, "test.toml").expect("applies");
    let err = dungeon_sim::Content::from_toml_str(&text, "patched").unwrap_err();
    assert!(err.to_string().contains("o1_optional_elite"), "{err}");
    let text = crate::overrides::apply(&g.toml, &patch, "test.toml").expect("applies");
    let c = dungeon_sim::Content::from_toml_str(&text, "patched").expect("loads");
    assert_eq!(c.encounters.len(), g.content.encounters.len() - 1);
    assert!(c.encounters.iter().any(|e| e.name == "The Wyrm Below"));
    assert_eq!(c.dungeon.description, "Edited by hand.");
    assert!(text.starts_with("# The Emberforge Vaults - generated"));
    // Regenerating the same seed and re-applying gives the same file.
    let again = build::generate(&bp(), &kit(), 1).expect("generates");
    assert_eq!(
        crate::overrides::apply(&again.toml, &patch, "test.toml").unwrap(),
        text
    );
    // An override for something the layout no longer has is an error.
    let stale = "[[encounters]]\nid = \"e9_dragon\"\nname = \"x\"\n";
    let err = crate::overrides::apply(&g.toml, stale, "stale.toml").unwrap_err();
    assert!(
        err.contains("e9_dragon") && err.contains("layout has changed"),
        "{err}"
    );
}
