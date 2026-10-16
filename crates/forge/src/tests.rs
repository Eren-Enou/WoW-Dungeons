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
    // A fixed set of seeds that generate (the audit lists the few that do not).
    for seed in [1u64, 2, 4, 5, 6, 7, 8, 10] {
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
        // 0.9: every branch delivers its declared purpose.
        assert!(g.branches.ok(), "seed {seed}: {:?}", g.branches.errors);
        assert!(g.measured.iter().all(|m| m.delivered), "seed {seed}");
    }
    assert!(
        shapes.len() >= 6,
        "only {} distinct main routes in 8 seeds",
        shapes.len()
    );
}

#[test]
fn generated_dungeon_has_the_planned_navigation() {
    let g = build::generate(&bp(), &kit(), 4).expect("seed 4");
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
    b.routes.clear();
    b.side.clear();
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
    // The crypt has pillared halls and a void chasm; the vaults have lava
    // (and no pillars but the landmark's plinth).
    assert!(
        ember
            .pillars
            .iter()
            .zip(&ember.plan.rooms)
            .all(|(p, r)| p.is_empty() || r.kind == crate::layout::RoomKind::Vista)
    );
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
    // The room staging the removed encounter is cleared too (0.8 rooms name
    // their encounter, and the loader checks the reference).
    let room = g
        .content
        .dungeon
        .rooms
        .iter()
        .find(|r| r.encounter.as_deref() == Some("o1_optional_elite"))
        .map(|r| r.id.clone())
        .expect("a room holds the optional encounter");
    let patch = format!(
        "[[encounters]]\nid = \"{fin}\"\nname = \"The Wyrm Below\"\n\n\
         [[encounters]]\nid = \"o1_optional_elite\"\nremove = true\n\n\
         [[loot]]\nid = \"emberforge_vaults_sentinel_shard\"\nsource = \"{fin}\"\n\n\
         [dungeon]\ndescription = \"Edited by hand.\"\n\n\
         [[dungeon.rooms]]\nid = \"{room}\"\nencounter = \"\"\n"
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

// ---- 0.8: presentation data in generated dungeons.

#[test]
fn generated_dungeons_carry_rooms_modules_atmosphere_and_anchors() {
    for (b, k, seed) in [(bp(), kit(), 2u64), (reliquary(), crypt(), 3)] {
        let g = build::generate(&b, &k, seed).expect("generates");
        let d = &g.content.dungeon;
        let info = d.generated.as_ref().expect("generated info");
        assert_eq!(
            (info.seed, info.generator),
            (seed, build::GENERATOR_VERSION)
        );
        assert_eq!(info.kit, k.kit.id);
        // Every area belongs to exactly one room or corridor.
        let mut count = std::collections::BTreeMap::new();
        for r in &d.rooms {
            for a in &r.areas {
                *count.entry(a.clone()).or_insert(0) += 1;
            }
        }
        for a in &d.areas {
            assert_eq!(count.get(&a.name), Some(&1), "area '{}'", a.name);
        }
        assert!(
            d.rooms
                .iter()
                .any(|r| r.role == "arena" && r.encounter.is_some())
        );
        assert!(d.rooms.iter().filter(|r| r.order.is_some()).count() >= 5);
        assert!(
            d.modules
                .as_ref()
                .is_some_and(|m| m.wall.is_some() && m.door.is_some())
        );
        assert!(d.atmosphere.contains_key("arena") && d.atmosphere.contains_key("default"));
        let used = d.anchors.iter().filter(|a| !a.used.is_empty()).count();
        assert!(used >= 20, "{used} anchors decorated");
        // Same seed, same decoration.
        let again = build::generate(&b, &k, seed).unwrap();
        assert_eq!(again.toml, g.toml);
    }
}

#[test]
fn kits_without_rules_decorate_like_before() {
    // The 0.7 palette fields still work: pillar candles and stone pillars,
    // arena braziers, corner rocks.
    let mut k = crypt();
    k.decorate.clear();
    k.palette.pillar_prop = Some(crate::kit::PropSpec {
        model: "models/props/crypt_candelabrum.glb".into(),
        fallback: dungeon_sim::content::DecorKind::Pillar,
        scale: 1.0,
        radius: 0.75,
        height: 4.0,
    });
    let g = build::generate(&reliquary(), &k, 1).expect("generates");
    let d = &g.content.dungeon;
    // Pillar holes (a landmark room's plinth is not a pillar).
    let holes: usize = g
        .pillars
        .iter()
        .zip(&g.plan.rooms)
        .filter(|(_, r)| r.kind != crate::layout::RoomKind::Vista)
        .map(|(p, _)| p.len())
        .sum();
    let pillars = d
        .decor
        .iter()
        .filter(|x| x.kind == dungeon_sim::content::DecorKind::Pillar)
        .count();
    let candles = d
        .props
        .iter()
        .filter(|p| p.model.contains("candelabrum"))
        .count();
    assert_eq!(pillars + candles, holes);
    assert!(candles > 0 && pillars > 0);
    let mut k = kit();
    k.decorate.clear();
    k.palette.arena_prop = Some(crate::kit::PropSpec {
        model: "models/props/forge_brazier.glb".into(),
        fallback: dungeon_sim::content::DecorKind::Crystal,
        scale: 2.0,
        radius: 0.9,
        height: 4.4,
    });
    let g = build::generate(&bp(), &k, 1).expect("generates");
    assert!(
        g.content
            .dungeon
            .props
            .iter()
            .any(|p| p.model.contains("brazier"))
    );
    assert!(!g.content.dungeon.decor.is_empty());
}

#[test]
fn bad_decoration_rules_are_rejected() {
    let mut k = crypt();
    k.decorate[0].anchor = "ceiling".into();
    k.decorate[1].prop = None;
    k.decorate[1].decor.clear();
    let e = k.validate();
    assert!(
        e.iter().any(|m| m.contains("anchor must be one of")),
        "{e:?}"
    );
    assert!(
        e.iter().any(|m| m.contains("give a prop or a decor list")),
        "{e:?}"
    );
}

// ---- 0.9: purposeful branches.

#[test]
fn every_declared_purpose_is_built_and_delivered() {
    use crate::layout::BranchKind as K;
    for (b, k, seeds, want) in [
        (
            bp(),
            kit(),
            [1u64, 2, 4],
            vec![K::Flank, K::Safe, K::Detour, K::Vista, K::Drop],
        ),
        (
            reliquary(),
            crypt(),
            [1, 2, 3],
            vec![K::Flank, K::Detour, K::Optional, K::Vista, K::Loop],
        ),
    ] {
        for seed in seeds {
            let g = build::generate(&b, &k, seed).expect("generates");
            let kinds: Vec<K> = g.plan.branches.iter().map(|x| x.kind).collect();
            for w in &want {
                assert!(kinds.contains(w), "seed {seed}: no {w:?} in {kinds:?}");
            }
            assert!(g.measured.iter().all(|m| m.delivered), "seed {seed}");
            let d = &g.content.dungeon;
            assert_eq!(d.branches.len(), g.plan.branches.len());
            // Rewards are real, and the cache rolls loot of its own.
            let detour = d.branches.iter().find(|x| x.purpose == "detour").unwrap();
            assert!(detour.rewards.iter().any(|r| r.starts_with('o')));
            let cache = &d.caches[0];
            assert!(detour.rewards.contains(&cache.id));
            assert!(
                g.content
                    .loot
                    .iter()
                    .any(|l| l.source.as_deref() == Some(cache.id.as_str()))
            );
            // Every optional encounter drops something (0.8 gave only the first).
            for e in g.content.encounters.iter().filter(|e| e.optional) {
                assert!(
                    g.content
                        .loot
                        .iter()
                        .any(|l| l.source.as_deref() == Some(e.id.as_str())),
                    "{} has no loot",
                    e.id
                );
            }
            // Nothing goes round a required encounter, and the facts say so.
            assert!(d.branches.iter().all(|x| x.bypasses.is_empty()));
            let flank = d.branches.iter().find(|x| x.purpose == "flank").unwrap();
            assert!(flank.target.is_some());
            assert!(
                flank
                    .facts
                    .iter()
                    .any(|f| f.contains("off the pack's front"))
            );
            // Rooms say which branch they belong to.
            for r in &d.rooms {
                assert!(!r.purpose.is_empty(), "{} has no purpose", r.id);
            }
        }
    }
}

#[test]
fn safe_routes_put_vents_on_the_short_way_only() {
    let g = build::generate(&bp(), &kit(), 2).expect("generates");
    let safe = g
        .plan
        .branches
        .iter()
        .find(|b| b.kind == crate::layout::BranchKind::Safe)
        .expect("a safe route");
    let vents: Vec<_> = g
        .content
        .encounters
        .iter()
        .flat_map(|e| &e.hazards)
        .filter(|h| h.name == "Magma Vent")
        .collect();
    assert!(!vents.is_empty());
    // Alternate vents are half a cycle apart: there is always a moment to cross.
    if vents.len() > 1 {
        assert!(vents.iter().any(|v| v.offset > 0.0) && vents.iter().any(|v| v.offset == 0.0));
    }
    let m = &g.measured[g
        .plan
        .branches
        .iter()
        .position(|b| std::ptr::eq(b, safe))
        .unwrap()];
    let ratio = m.branch_len.unwrap() / m.main_len.unwrap();
    assert!((crate::purpose::SAFE_MIN_RATIO..=crate::purpose::SAFE_MAX_RATIO).contains(&ratio));
}

#[test]
fn drops_in_purposeful_blueprints_skip_no_required_fight() {
    for seed in [1u64, 2, 4, 5] {
        let g = build::generate(&bp(), &kit(), seed).expect("generates");
        for (b, m) in g.plan.branches.iter().zip(&g.measured) {
            if b.kind == crate::layout::BranchKind::Drop {
                assert!(m.bypasses.is_empty(), "seed {seed}: {:?}", m.bypasses);
                assert!(m.branch_len.unwrap() < m.main_len.unwrap());
            }
        }
    }
}

#[test]
fn purposeless_or_impossible_branches_are_rejected_with_reasons() {
    // A detour with nothing in it.
    let mut b = reliquary();
    b.routes[1].optional = None;
    b.routes[1].cache = false;
    let e = build::validate_pair(&b, &crypt()).join("\n");
    assert!(e.contains("a detour needs a reward"), "{e}");
    // Mixing 0.9 tables with the purposeless legacy fields.
    let mut b = reliquary();
    b.layout.alternate_routes = 1;
    let e = build::validate_pair(&b, &crypt()).join("\n");
    assert!(e.contains("not both"), "{e}");
    // The crypt has no route hazard, so it cannot build a safe route.
    let mut b = reliquary();
    b.routes.push(crate::blueprint::RouteSpec {
        purpose: crate::blueprint::RoutePurpose::Safe,
        optional: None,
        cache: false,
        target: None,
    });
    let e = build::validate_pair(&b, &crypt()).join("\n");
    assert!(e.contains("route_hazard"), "{e}");
    // A flank target that is not a main encounter.
    let mut b = reliquary();
    b.routes[0].target = Some("crawler_rush".into());
    let e = build::validate_pair(&b, &crypt()).join("\n");
    assert!(e.contains("not a main [[encounters]] recipe"), "{e}");
    // Two quiet rooms.
    let mut b = reliquary();
    for _ in 0..2 {
        b.side.push(crate::blueprint::SideSpec {
            purpose: crate::blueprint::SidePurpose::Quiet,
            optional: None,
            depth: 1,
        });
    }
    let e = build::validate_pair(&b, &crypt()).join("\n");
    assert!(e.contains("at most one deliberately empty room"), "{e}");
}

#[test]
fn legacy_blueprints_keep_their_layouts_and_the_audit_flags_them() {
    // The 0.8.1 blueprint shape: alternate routes and empty side rooms, no
    // purposes. Same layout as before 0.9 (same rooms and corridors); the
    // audit reports every such branch as purposeless.
    let mut b = reliquary();
    b.routes.clear();
    b.side.clear();
    b.layout.alternate_routes = 1;
    b.layout.side_rooms = [1, 2];
    assert!(build::validate_pair(&b, &crypt()).is_empty());
    let a = crate::audit::audit(&b, &crypt(), 1..=3);
    assert_eq!(a.generated, 3);
    assert!(a.purposeless > 0);
    assert!(!a.clean());
    assert!(a.problems.iter().any(|p| p.contains("no declared purpose")));
    // The 0.9 blueprint is clean over the same seeds.
    let a = crate::audit::audit(&reliquary(), &crypt(), 1..=3);
    assert!(a.clean(), "{:?}", a.problems);
    assert_eq!(a.purposeless, 0);
}

#[test]
fn optional_encounters_without_a_home_still_get_a_side_room() {
    let mut b = reliquary();
    // Drop the side entry that held optional 1: it gets a chamber anyway.
    b.side
        .retain(|s| s.purpose != crate::blueprint::SidePurpose::Optional);
    let g = build::generate(&b, &crypt(), 1).expect("generates");
    assert!(g.content.encounters.iter().any(|e| e.id.starts_with("o1_")));
    assert!(g.measured.iter().all(|m| m.delivered));
}
