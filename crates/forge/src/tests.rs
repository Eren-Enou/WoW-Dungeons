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
    assert_eq!(again.encounters.len(), 6);
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
