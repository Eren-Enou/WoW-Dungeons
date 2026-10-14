//! Rule-level tests for combat, threat, resources, auras, enemy abilities, AI,
//! encounters and determinism.

use crate::ai::{self, AiProfile};
use crate::class::{AbilityId, ClassId};
use crate::content::{BUILTIN_SUNKEN_VAULT, Content};
use crate::events::{CastError, EventKind, Label};
use crate::math::V2;
use crate::runner::{default_party, party_at_level, raid_party, run_headless, run_with};
use crate::sim::{
    ActorId, Aura, AuraKind, COMBAT_LINGER, Cast, CastKind, Controller, DT, DamageSpec, EnemyMode,
    HEAL_THREAT_FACTOR, PartyMemberSpec, RunOptions, RunOutcome, Sim, Telegraph,
};
use crate::stats::{HitKind, School};

/// Default party (player = Striker): 0 Wayfarer (Striker), 1 Brannoc (Warden),
/// 2 Thessaly (Mender), 3 Mirelle (Arcanist), 4 Kael (Striker).
const PLAYER: ActorId = 0;
const TANK: ActorId = 1;
const HEALER: ActorId = 2;
const RANGED: ActorId = 3;
const MELEE: ActorId = 4;

fn new_sim() -> Sim {
    Sim::new(&Content::builtin(), &default_party(ClassId::Striker), 7)
}

/// A sim where no party member acts on its own (they are all "player" controlled
/// with no input), so tests can drive actions explicitly.
fn passive_sim() -> Sim {
    let mut sim = new_sim();
    for p in sim.party.clone() {
        sim.actors[p].controller = Controller::Player;
    }
    sim
}

fn warrens_passive() -> Sim {
    let mut sim = Sim::new(
        &Content::builtin_warrens(),
        &party_at_level(ClassId::Striker, 6, AiProfile::Balanced),
        3,
    );
    for p in sim.party.clone() {
        sim.actors[p].controller = Controller::Player;
    }
    sim
}

fn first_skulker(sim: &Sim) -> ActorId {
    sim.encounters[0].mobs[0]
}

fn steps(sim: &mut Sim, seconds: f32) {
    let n = (seconds / DT).round() as usize;
    for _ in 0..n {
        sim.step();
    }
}

fn place_near(sim: &mut Sim, who: ActorId, target: ActorId, dist: f32) {
    let tp = sim.actors[target].pos;
    sim.actors[who].pos = sim.nav.project(tp + V2::new(0.0, -dist));
}

fn count(sim: &Sim, f: impl Fn(&EventKind) -> bool) -> usize {
    sim.events.iter().filter(|e| f(&e.kind)).count()
}

fn sure_hit(base: f32) -> DamageSpec {
    DamageSpec::new(
        base,
        School::Physical,
        HitKind::Unavoidable,
        Label::from("Test"),
    )
}

fn make_immortal(sim: &mut Sim, id: ActorId) {
    sim.actors[id].max_hp = 1e9;
    sim.actors[id].hp = 1e9;
}

fn slot(sim: &Sim, id: ActorId, a: AbilityId) -> usize {
    sim.actors[id].ability_slot(a).expect("has ability")
}

#[test]
fn party_roles_match_default_composition() {
    let sim = new_sim();
    let classes: Vec<_> = sim
        .party
        .iter()
        .map(|&p| sim.actors[p].class.unwrap())
        .collect();
    assert_eq!(
        classes,
        vec![
            ClassId::Striker,
            ClassId::Warden,
            ClassId::Mender,
            ClassId::Arcanist,
            ClassId::Striker
        ]
    );
    assert_eq!(sim.leader, TANK, "AI-only party is led by the tank");
    for &p in &sim.party {
        assert_eq!(sim.actors[p].abilities.len(), 5, "five abilities per class");
    }
}

// ------------------------------------------------------------ health ---

#[test]
fn damage_never_takes_health_below_zero_and_kills() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    let applied = sim.deal(PLAYER, mob, sure_hit(100_000.0));
    assert_eq!(applied, 600.0, "applied damage is capped by remaining HP");
    assert_eq!(sim.actors[mob].hp, 0.0);
    assert!(!sim.actors[mob].alive);
    assert_eq!(
        count(
            &sim,
            |k| matches!(k, EventKind::Death { actor } if *actor == mob)
        ),
        1
    );
    assert_eq!(sim.deal(PLAYER, mob, sure_hit(50.0)), 0.0);
}

#[test]
fn healing_is_capped_at_max_hp_and_overheal_is_recorded() {
    let mut sim = passive_sim();
    sim.actors[HEALER].stats.crit = 0.0;
    let max = sim.actors[TANK].max_hp;
    sim.actors[TANK].hp = max - 100.0;
    let eff = sim.heal(HEALER, TANK, 420.0, Label::from("Mend"), true);
    assert_eq!(eff, 100.0);
    assert_eq!(sim.actors[TANK].hp, max);
    assert_eq!(sim.metrics.actors[HEALER].overhealing, 320.0);
}

// ---------------------------------------------------- mitigation -------

#[test]
fn armor_reduces_physical_but_not_bleeds_or_magic() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    make_immortal(&mut sim, mob);
    sim.actors[mob].stats.armor = 485.0; // 50% vs a level-1 attacker
    sim.actors[PLAYER].stats.crit = 0.0;
    let mut avg = |spec: DamageSpec| {
        let mut total = 0.0;
        for _ in 0..200 {
            total += sim.deal(PLAYER, mob, spec.clone());
        }
        total / 200.0
    };
    let physical = avg(sure_hit(100.0));
    let mut bleed = sure_hit(100.0);
    bleed.ignores_armor = true;
    let bleed = avg(bleed);
    let fire = avg(DamageSpec::new(
        100.0,
        School::Fire,
        HitKind::Unavoidable,
        Label::from("Fire"),
    ));
    assert!((physical - 50.0).abs() < 3.0, "{physical}");
    assert!((bleed - 100.0).abs() < 3.0, "{bleed}");
    assert!((fire - 100.0).abs() < 3.0, "{fire}");
}

#[test]
fn parry_and_block_only_happen_from_the_front() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    make_immortal(&mut sim, mob);
    sim.actors[mob].stats.parry = 0.5;
    sim.actors[mob].stats.dodge = 0.0;
    let melee = || DamageSpec::new(10.0, School::Physical, HitKind::Melee, Label::from("Melee"));
    let parries = |sim: &Sim| {
        count(sim, |k| {
            matches!(
                k,
                EventKind::Damage {
                    outcome: crate::stats::HitOutcome::Parry,
                    ..
                }
            )
        })
    };
    let pos = sim.actors[mob].pos;
    sim.actors[PLAYER].pos = pos + V2::new(0.0, -1.5); // behind (mob faces +Z)
    for _ in 0..100 {
        sim.actors[mob].facing = 0.0;
        sim.deal(PLAYER, mob, melee());
    }
    assert_eq!(parries(&sim), 0, "no parries from behind");
    sim.actors[PLAYER].pos = pos + V2::new(0.0, 1.5); // in front
    for _ in 0..100 {
        sim.actors[mob].facing = 0.0;
        sim.deal(PLAYER, mob, melee());
    }
    assert!(
        parries(&sim) > 25,
        "about half should be parried from the front"
    );
}

#[test]
fn spells_can_be_resisted_but_never_dodged() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    make_immortal(&mut sim, mob);
    sim.actors[mob].stats.dodge = 0.5;
    sim.actors[mob].stats.level = 4; // +3: 17% spell miss
    for _ in 0..400 {
        sim.deal(
            RANGED,
            mob,
            DamageSpec::new(10.0, School::Arcane, HitKind::Spell, Label::from("Bolt")),
        );
    }
    use crate::stats::HitOutcome as O;
    let dodges = count(&sim, |k| {
        matches!(
            k,
            EventKind::Damage {
                outcome: O::Dodge,
                ..
            }
        )
    });
    let resists = count(&sim, |k| {
        matches!(
            k,
            EventKind::Damage {
                outcome: O::Resist,
                ..
            }
        )
    });
    assert_eq!(dodges, 0);
    assert!((40..100).contains(&resists), "~17% of 400: {resists}");
}

// ------------------------------------------------------------ threat ---

#[test]
fn damage_generates_threat_with_class_multiplier() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    sim.engage(0, TANK);
    let mut spec = sure_hit(100.0);
    spec.threat_mult = 2.0;
    let dealt = sim.deal(TANK, mob, spec);
    assert!((sim.threat(mob, TANK) - dealt * 2.0 * 1.5).abs() < 1e-3);
}

#[test]
fn enemy_switches_target_at_110_percent_in_melee_and_130_percent_at_range() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    place_near(&mut sim, TANK, mob, 1.0);
    place_near(&mut sim, MELEE, mob, 1.5);
    sim.engage(0, TANK);
    sim.add_threat(mob, TANK, 1000.0);
    sim.actors[mob].target = sim.select_enemy_target(mob);
    assert_eq!(sim.actors[mob].target, Some(TANK));
    sim.add_threat(mob, MELEE, 1090.0);
    assert_eq!(sim.select_enemy_target(mob), Some(TANK), "109% in melee");
    sim.add_threat(mob, MELEE, 20.0);
    assert_eq!(sim.select_enemy_target(mob), Some(MELEE), "111% in melee");

    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    place_near(&mut sim, TANK, mob, 1.0);
    place_near(&mut sim, RANGED, mob, 20.0);
    sim.engage(0, TANK);
    sim.add_threat(mob, TANK, 1000.0);
    sim.actors[mob].target = sim.select_enemy_target(mob);
    sim.add_threat(mob, RANGED, 1250.0);
    assert_eq!(sim.select_enemy_target(mob), Some(TANK), "125% at range");
    sim.add_threat(mob, RANGED, 100.0);
    assert_eq!(sim.select_enemy_target(mob), Some(RANGED), "135% at range");
}

#[test]
fn taunt_takes_top_threat_and_forces_target() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    place_near(&mut sim, TANK, mob, 5.0);
    place_near(&mut sim, MELEE, mob, 1.0);
    sim.engage(0, MELEE);
    sim.add_threat(mob, MELEE, 2000.0);
    sim.add_threat(mob, TANK, 100.0);
    sim.actors[mob].target = sim.select_enemy_target(mob);
    assert_eq!(sim.actors[mob].target, Some(MELEE));
    let s = slot(&sim, TANK, AbilityId::Challenge);
    sim.try_cast(TANK, s, Some(mob)).unwrap();
    assert!(sim.threat(mob, TANK) >= 2000.0);
    assert_eq!(sim.select_enemy_target(mob), Some(TANK));
    sim.add_threat(mob, MELEE, 500.0);
    assert_eq!(sim.select_enemy_target(mob), Some(TANK), "taunt holds");
}

#[test]
fn healing_threat_is_split_across_engaged_enemies() {
    let mut sim = passive_sim();
    sim.actors[HEALER].stats.crit = 0.0;
    for &m in &sim.encounters[0].mobs.clone() {
        place_near(&mut sim, TANK, m, 1.0);
    }
    sim.engage(0, TANK);
    sim.actors[TANK].hp -= 600.0;
    sim.actors[TANK].last_combat = sim.time;
    let eff = sim.heal(HEALER, TANK, 420.0, Label::from("Mend"), false);
    assert_eq!(eff, 420.0);
    let engaged = sim.engaged_enemies();
    assert_eq!(engaged.len(), 3);
    for e in engaged {
        let expected = HEAL_THREAT_FACTOR * eff / 3.0;
        assert!((sim.threat(e, HEALER) - expected).abs() < 1e-3);
    }
}

#[test]
fn healing_out_of_combat_generates_no_threat() {
    let mut sim = passive_sim();
    sim.engage(0, TANK);
    sim.actors[MELEE].hp -= 300.0;
    sim.actors[MELEE].last_combat = sim.time - COMBAT_LINGER - 1.0;
    sim.heal(HEALER, MELEE, 200.0, Label::from("Mend"), false);
    assert_eq!(sim.threat(first_skulker(&sim), HEALER), 0.0);
}

#[test]
fn threat_drop_halves_threat_everywhere() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    sim.engage(0, PLAYER);
    sim.add_threat(mob, PLAYER, 800.0);
    let s = slot(&sim, PLAYER, AbilityId::Evade);
    sim.try_cast(PLAYER, s, None).unwrap();
    assert!((sim.threat(mob, PLAYER) - 400.0).abs() < 1e-3);
}

#[test]
fn dead_actors_leave_threat_tables() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    sim.engage(0, TANK);
    sim.add_threat(mob, TANK, 500.0);
    sim.add_threat(mob, MELEE, 100.0);
    sim.kill(TANK);
    assert!(!sim.on_threat_table(mob, TANK));
    assert_eq!(sim.select_enemy_target(mob), Some(MELEE));
    assert_eq!(sim.try_cast(TANK, 0, Some(mob)), Err(CastError::Dead));
}

// --------------------------------------------------------- targeting ---

#[test]
fn ability_target_validation() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    let rend = slot(&sim, PLAYER, AbilityId::RendStrike);
    assert_eq!(sim.try_cast(PLAYER, rend, None), Err(CastError::NoTarget));
    assert_eq!(
        sim.try_cast(PLAYER, rend, Some(TANK)),
        Err(CastError::InvalidTarget)
    );
    assert_eq!(
        sim.try_cast(PLAYER, rend, Some(mob)),
        Err(CastError::OutOfRange)
    );
    place_near(&mut sim, PLAYER, mob, 1.0);
    assert_eq!(sim.try_cast(PLAYER, rend, Some(mob)), Ok(()));

    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    sim.actors[HEALER].hp -= 300.0;
    let renew = slot(&sim, HEALER, AbilityId::Renew);
    sim.try_cast(HEALER, renew, Some(mob)).unwrap();
    assert!(
        sim.actors[HEALER].has_hot_from(HEALER),
        "falls back to self"
    );

    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    place_near(&mut sim, PLAYER, mob, 1.0);
    sim.kill(mob);
    assert_eq!(
        sim.try_cast(PLAYER, rend, Some(mob)),
        Err(CastError::InvalidTarget)
    );
}

// --------------------------------------------------------- resources ---

#[test]
fn rage_starts_empty_and_builds_from_combat() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    make_immortal(&mut sim, mob);
    place_near(&mut sim, TANK, mob, 1.0);
    assert_eq!(sim.actors[TANK].resource, 0.0);
    let slam = slot(&sim, TANK, AbilityId::ShieldSlam);
    assert_eq!(
        sim.try_cast(TANK, slam, Some(mob)),
        Err(CastError::NotEnoughResource)
    );
    sim.actors[TANK].target = Some(mob);
    sim.actors[TANK].auto_attack = true;
    steps(&mut sim, 6.0);
    assert!(
        sim.actors[TANK].resource >= 15.0,
        "{}",
        sim.actors[TANK].resource
    );
    assert_eq!(sim.try_cast(TANK, slam, Some(mob)), Ok(()));
}

#[test]
fn rage_decays_out_of_combat() {
    let mut sim = passive_sim();
    sim.actors[TANK].resource = 50.0;
    steps(&mut sim, 5.0);
    assert!(sim.actors[TANK].resource < 41.0);
}

#[test]
fn energy_regenerates_ten_per_second() {
    let mut sim = passive_sim();
    sim.actors[PLAYER].resource = 0.0;
    steps(&mut sim, 3.0);
    assert!((sim.actors[PLAYER].resource - 30.0).abs() < 0.6);
}

#[test]
fn five_second_rule_slows_mana_regen_after_spending() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    sim.engage(0, RANGED);
    make_immortal(&mut sim, mob);
    let max = sim.actors[RANGED].max_resource;
    sim.actors[RANGED].resource = 0.5 * max;
    sim.actors[RANGED].last_mana_spend = sim.time;
    let before = sim.actors[RANGED].resource;
    steps(&mut sim, 2.0);
    let during = sim.actors[RANGED].resource - before;
    steps(&mut sim, 4.0);
    let mid = sim.actors[RANGED].resource;
    steps(&mut sim, 2.0);
    let after = sim.actors[RANGED].resource - mid;
    assert!(
        after > 2.0 * during,
        "regen after 5s ({after}) >> during ({during})"
    );
}

#[test]
fn cooldowns_and_global_cooldown() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    make_immortal(&mut sim, mob);
    place_near(&mut sim, PLAYER, mob, 1.0);
    let whirl = slot(&sim, PLAYER, AbilityId::Whirl);
    let evade = slot(&sim, PLAYER, AbilityId::Evade);
    sim.try_cast(PLAYER, evade, None).unwrap();
    assert_eq!(sim.try_cast(PLAYER, evade, None), Err(CastError::NotReady));
    assert_eq!(
        sim.try_cast(PLAYER, whirl, None),
        Err(CastError::OnGlobalCooldown)
    );
    steps(&mut sim, 1.05);
    assert_eq!(
        sim.try_cast(PLAYER, whirl, None),
        Ok(()),
        "GCD over after 1s"
    );
    steps(&mut sim, 1.05);
    assert_eq!(
        sim.try_cast(PLAYER, whirl, None),
        Err(CastError::NotReady),
        "10s cooldown"
    );
    steps(&mut sim, 9.0);
    assert_eq!(sim.try_cast(PLAYER, whirl, None), Ok(()));
}

#[test]
fn cast_time_spells_complete_after_cast_time() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    make_immortal(&mut sim, mob);
    place_near(&mut sim, RANGED, mob, 15.0);
    let bolt = slot(&sim, RANGED, AbilityId::ArcBolt);
    let resolved = |sim: &Sim| {
        count(
            sim,
            |k| matches!(k, EventKind::Damage { what, .. } if &**what == "Arc Bolt"),
        )
    };
    sim.try_cast(RANGED, bolt, Some(mob)).unwrap();
    steps(&mut sim, 1.9);
    assert_eq!(resolved(&sim), 0);
    steps(&mut sim, 0.15);
    assert_eq!(resolved(&sim), 1, "resolved (hit or resist) after 2s");
}

#[test]
fn moving_player_cancels_own_cast() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    place_near(&mut sim, RANGED, mob, 15.0);
    sim.player = Some(RANGED);
    let bolt = slot(&sim, RANGED, AbilityId::ArcBolt);
    sim.try_cast(RANGED, bolt, Some(mob)).unwrap();
    sim.set_player_move(V2::new(1.0, 0.0));
    sim.step();
    assert!(sim.actors[RANGED].cast.is_none());
    assert_eq!(
        count(&sim, |k| matches!(k, EventKind::Interrupted { .. })),
        1
    );
}

// ------------------------------------------------- interrupts & CC -----

fn give_enemy_cast(sim: &mut Sim, e: ActorId, interruptible: bool) {
    sim.actors[e].cast = Some(Cast {
        kind: CastKind::Enemy(0),
        name: Label::from("Test Bolt"),
        target: TANK,
        remaining: 1.5,
        total: 2.0,
        interruptible,
    });
}

#[test]
fn kick_interrupts_and_silences_interruptible_casts_only() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    make_immortal(&mut sim, mob);
    place_near(&mut sim, PLAYER, mob, 1.0);
    sim.engage(0, PLAYER);
    give_enemy_cast(&mut sim, mob, true);
    let kick = slot(&sim, PLAYER, AbilityId::Kick);
    sim.try_cast(PLAYER, kick, Some(mob)).unwrap();
    assert!(sim.actors[mob].cast.is_none());
    assert!(sim.actors[mob].is_silenced());
    assert_eq!(count(&sim, |k| matches!(k, EventKind::Interrupt { .. })), 1);
    assert_eq!(sim.metrics.actors[PLAYER].interrupts, 1);

    let mob2 = sim.encounters[0].mobs[1];
    make_immortal(&mut sim, mob2);
    place_near(&mut sim, MELEE, mob2, 1.0);
    give_enemy_cast(&mut sim, mob2, false);
    let k2 = slot(&sim, MELEE, AbilityId::Kick);
    sim.try_cast(MELEE, k2, Some(mob2)).unwrap();
    assert!(
        sim.actors[mob2].cast.is_some(),
        "uninterruptible cast continues"
    );
}

fn aura(name: &str, kind: AuraKind, source: ActorId, max_stacks: u32, dispellable: bool) -> Aura {
    Aura {
        name: Label::from(name),
        kind,
        source,
        remaining: 12.0,
        duration: 12.0,
        tick_timer: 1.0,
        stacks: 1,
        max_stacks,
        dispellable,
        harmful: true,
    }
}

#[test]
fn silence_blocks_party_spells() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    place_near(&mut sim, RANGED, mob, 10.0);
    sim.apply_aura(RANGED, aura("Silenced", AuraKind::Silenced, mob, 1, false));
    let flare = slot(&sim, RANGED, AbilityId::Flare);
    assert_eq!(
        sim.try_cast(RANGED, flare, Some(mob)),
        Err(CastError::Silenced)
    );
}

#[test]
fn hex_incapacitates_until_damaged_and_bosses_are_immune() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    place_near(&mut sim, RANGED, mob, 15.0);
    let hex = slot(&sim, RANGED, AbilityId::Hex);
    sim.try_cast(RANGED, hex, Some(mob)).unwrap();
    steps(&mut sim, 1.6);
    assert!(sim.actors[mob].is_incapacitated());
    let pos = sim.actors[mob].pos;
    steps(&mut sim, 3.0);
    assert_eq!(sim.actors[mob].pos, pos, "incapacitated enemies don't move");
    sim.deal(PLAYER, mob, sure_hit(5.0));
    assert!(!sim.actors[mob].is_incapacitated(), "damage breaks it");
    assert_eq!(count(&sim, |k| matches!(k, EventKind::CcBroken { .. })), 1);

    let boss = sim.boss;
    place_near(&mut sim, RANGED, boss, 15.0);
    steps(&mut sim, 1.0);
    assert_eq!(
        sim.try_cast(RANGED, hex, Some(boss)),
        Err(CastError::Immune)
    );
}

// ------------------------------------------------------- auras ---------

#[test]
fn lacerate_ticks_every_second_and_ignores_armor() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    make_immortal(&mut sim, mob);
    sim.actors[mob].stats.armor = 10_000.0;
    place_near(&mut sim, PLAYER, mob, 1.0);
    let lac = slot(&sim, PLAYER, AbilityId::Lacerate);
    sim.try_cast(PLAYER, lac, Some(mob)).unwrap();
    steps(&mut sim, 12.5);
    let ticks: Vec<f32> = sim
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::Damage { what, amount, .. } if &**what == "Lacerate" => Some(*amount),
            _ => None,
        })
        .collect();
    assert_eq!(ticks.len(), 12);
    assert!(ticks.iter().all(|&t| t == 20.0), "{ticks:?}");
}

#[test]
fn ward_absorbs_damage_and_weakened_soul_blocks_rewarding() {
    let mut sim = passive_sim();
    let ward = slot(&sim, HEALER, AbilityId::Ward);
    sim.try_cast(HEALER, ward, Some(TANK)).unwrap();
    let shield = sim.actors[TANK].absorb_remaining();
    assert_eq!(shield, 320.0);
    let hp = sim.actors[TANK].hp;
    let mob = first_skulker(&sim);
    sim.deal(
        mob,
        TANK,
        DamageSpec::new(200.0, School::Fire, HitKind::Unavoidable, Label::from("x")),
    );
    assert_eq!(sim.actors[TANK].hp, hp, "fully absorbed");
    assert!(sim.actors[TANK].absorb_remaining() < shield);
    steps(&mut sim, 4.1);
    assert_eq!(
        sim.try_cast(HEALER, ward, Some(TANK)),
        Err(CastError::TargetWeakened)
    );
    assert!(sim.metrics.actors[HEALER].absorb_done > 0.0);
}

fn brand(src: ActorId, dispellable: bool) -> Aura {
    aura(
        "Venom Brand",
        AuraKind::Dot {
            per_tick: 10.0,
            school: School::Nature,
            ignores_armor: true,
        },
        src,
        5,
        dispellable,
    )
}

#[test]
fn purify_removes_dispellable_debuffs_only() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    let purify = slot(&sim, HEALER, AbilityId::Purify);
    assert_eq!(
        sim.try_cast(HEALER, purify, Some(TANK)),
        Err(CastError::NothingToDispel)
    );
    sim.apply_aura(TANK, brand(mob, true));
    sim.try_cast(HEALER, purify, Some(TANK)).unwrap();
    assert!(!sim.actors[TANK].has_dispellable_debuff());
    steps(&mut sim, 1.1);
    sim.apply_aura(TANK, brand(mob, false));
    assert_eq!(
        sim.try_cast(HEALER, purify, Some(TANK)),
        Err(CastError::NothingToDispel)
    );
}

#[test]
fn debuffs_stack_to_max_and_increase_damage_taken() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    for _ in 0..8 {
        sim.apply_aura(
            TANK,
            aura(
                "Withering Mark",
                AuraKind::DamageTaken { pct: 0.1 },
                mob,
                5,
                false,
            ),
        );
    }
    assert_eq!(sim.actors[TANK].auras.len(), 1);
    assert_eq!(sim.actors[TANK].auras[0].stacks, 5);
    assert!((sim.actors[TANK].damage_taken_mult() - 1.5).abs() < 1e-5);
}

// --------------------------------------------------- enemy abilities ---

fn pull_boss(sim: &mut Sim) -> ActorId {
    let boss = sim.boss;
    for p in sim.party.clone() {
        place_near(sim, p, boss, 3.0 + p as f32);
    }
    let enc = sim.content.final_encounter();
    sim.engage(enc, TANK);
    sim.add_threat(boss, TANK, 5000.0);
    boss
}

#[test]
fn boss_phase_two_triggers_once_at_threshold() {
    let mut sim = passive_sim();
    let boss = pull_boss(&mut sim);
    for p in sim.party.clone() {
        make_immortal(&mut sim, p);
    }
    let max = sim.actors[boss].max_hp;
    sim.actors[boss].hp = max * 0.51;
    steps(&mut sim, 0.5);
    assert_eq!(sim.boss_phase(), 1);
    sim.actors[boss].hp = max * 0.50;
    sim.step();
    assert_eq!(sim.boss_phase(), 2);
    let phase_events = |s: &Sim| count(s, |k| matches!(k, EventKind::PhaseChange { .. }));
    assert_eq!(phase_events(&sim), 1);
    sim.actors[boss].hp = max * 0.3;
    steps(&mut sim, 1.0);
    assert_eq!(phase_events(&sim), 1, "fires exactly once per pull");
    steps(&mut sim, 1.2);
    assert_eq!(
        count(&sim, |k| matches!(k, EventKind::TelegraphStart { .. })),
        1,
        "first Cinder Ring 2s into phase 2"
    );
}

#[test]
fn ring_interval_from_config_controls_timing() {
    let text = BUILTIN_SUNKEN_VAULT.replace("interval = 9.0", "interval = 4.0");
    let content = Content::from_toml_str(&text, "test").unwrap();
    let mut sim = Sim::new(&content, &default_party(ClassId::Striker), 3);
    for p in sim.party.clone() {
        sim.actors[p].controller = Controller::Player;
        make_immortal(&mut sim, p);
    }
    let boss = pull_boss(&mut sim);
    sim.actors[boss].hp = sim.actors[boss].max_hp * 0.4;
    steps(&mut sim, 15.0);
    let times: Vec<f32> = sim
        .events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::TelegraphStart { .. }))
        .map(|e| e.time)
        .collect();
    assert!(times.len() >= 3, "{times:?}");
    for w in times.windows(2) {
        assert!((w[1] - w[0] - 4.0).abs() < 0.11, "{times:?}");
    }
}

#[test]
fn cinder_ring_damages_only_those_inside() {
    let mut sim = passive_sim();
    let boss = pull_boss(&mut sim);
    let center = sim.actors[HEALER].pos;
    sim.actors[RANGED].pos = sim.nav.project(center + V2::new(0.0, -12.0));
    sim.telegraphs.push(Telegraph {
        id: 99,
        source: boss,
        what: Label::from("Cinder Ring"),
        school: School::Fire,
        center,
        radius: 5.0,
        remaining: 0.04,
        total: 2.5,
        damage: 100.0,
    });
    let (h0, r0) = (sim.actors[HEALER].hp, sim.actors[RANGED].hp);
    sim.step();
    assert!(sim.actors[HEALER].hp < h0);
    assert_eq!(sim.actors[RANGED].hp, r0);
    assert!(sim.metrics.actors[HEALER].avoidable_taken > 0.0);
}

#[test]
fn hollow_bolt_is_interruptible_by_party_ai() {
    let mut sim = Sim::new(&Content::builtin(), &default_party(ClassId::Striker), 9);
    let boss = pull_boss(&mut sim);
    for p in sim.party.clone() {
        make_immortal(&mut sim, p);
    }
    make_immortal(&mut sim, boss);
    steps(&mut sim, 40.0);
    assert!(sim.metrics.interruptible_casts >= 3);
    assert!(
        sim.metrics.interrupted_casts * 2 >= sim.metrics.interruptible_casts,
        "AI interrupts most bolts: {}/{}",
        sim.metrics.interrupted_casts,
        sim.metrics.interruptible_casts
    );
}

fn encounter_index(sim: &Sim, id: &str) -> usize {
    sim.encounters.iter().position(|e| e.id == id).unwrap()
}

#[test]
fn brakkis_summons_adds_that_vanish_on_reset() {
    let mut sim = warrens_passive();
    let enc = encounter_index(&sim, "brood_chamber");
    let boss = sim.encounters[enc].mobs[0];
    place_near(&mut sim, TANK, boss, 2.0);
    make_immortal(&mut sim, TANK);
    sim.engage(enc, TANK);
    steps(&mut sim, 9.0);
    let adds: Vec<ActorId> = sim.encounters[enc].mobs[1..].to_vec();
    assert_eq!(adds.len(), 2, "Call the Brood after 8s");
    assert!(adds.iter().all(|&a| sim.actors[a].is_engaged_enemy()));
    assert!(count(&sim, |k| matches!(k, EventKind::Summon { .. })) >= 1);
    for p in sim.party.clone() {
        sim.kill(p);
    }
    steps(&mut sim, 0.2);
    assert!(adds.iter().all(|&a| !sim.actors[a].alive));
    assert!(
        adds.iter()
            .all(|&a| sim.actors[a].enemy.as_ref().unwrap().despawned)
    );
}

#[test]
fn frontal_cleave_spares_those_behind() {
    let mut sim = warrens_passive();
    let enc = encounter_index(&sim, "brood_chamber");
    let boss = sim.encounters[enc].mobs[0];
    let bp = sim.actors[boss].pos;
    sim.actors[TANK].pos = bp + V2::new(0.0, -2.5);
    sim.actors[MELEE].pos = bp + V2::new(0.0, 2.5);
    sim.actors[PLAYER].pos = bp + V2::new(0.5, -2.8);
    for p in [TANK, MELEE, PLAYER] {
        make_immortal(&mut sim, p);
    }
    sim.engage(enc, TANK);
    sim.add_threat(boss, TANK, 1e6);
    steps(&mut sim, 5.6);
    let hit = |who: ActorId| {
        count(
            &sim,
            |k| matches!(k, EventKind::Damage { what, dst, .. } if &**what == "Rending Mandibles" && *dst == who),
        )
    };
    assert!(hit(TANK) >= 1, "tank in front is hit");
    assert!(hit(PLAYER) >= 1, "anyone in front is hit");
    assert_eq!(hit(MELEE), 0, "behind is safe");
    assert!(sim.metrics.actors[PLAYER].avoidable_taken > 0.0);
    assert_eq!(
        sim.metrics.actors[TANK].avoidable_taken, 0.0,
        "tanks are expected to take it"
    );
}

#[test]
fn shaman_heals_injured_ally() {
    let mut sim = warrens_passive();
    let shaman = sim.encounters[0].mobs[2];
    let brute = sim.encounters[0].mobs[0];
    place_near(&mut sim, TANK, brute, 2.0);
    make_immortal(&mut sim, TANK);
    sim.engage(0, TANK);
    sim.actors[brute].hp = sim.actors[brute].max_hp * 0.3;
    steps(&mut sim, 7.5);
    assert!(
        count(
            &sim,
            |k| matches!(k, EventKind::Heal { src, .. } if *src == shaman)
        ) >= 1,
        "Mending Sap lands"
    );
    assert!(sim.actors[brute].hp_frac() > 0.3);
}

#[test]
fn hazards_pulse_on_and_off() {
    let mut sim = warrens_passive();
    let h = sim
        .hazards
        .iter()
        .position(|h| h.cfg.name == "Lava Vent")
        .unwrap();
    let pos = sim.hazards[h].cfg.pos;
    sim.actors[MELEE].pos = pos;
    make_immortal(&mut sim, MELEE);
    let (on, off) = (sim.hazards[h].cfg.on, sim.hazards[h].cfg.off);
    steps(&mut sim, on + off);
    let hits = count(
        &sim,
        |k| matches!(k, EventKind::Damage { what, dst, .. } if &**what == "Lava Vent" && *dst == MELEE),
    );
    assert!(
        (2..=5).contains(&hits),
        "about one tick per active second: {hits}"
    );
    assert!(sim.metrics.actors[MELEE].avoidable_taken > 0.0);
}

#[test]
fn optional_encounters_are_not_required_for_victory() {
    let mut sim = warrens_passive();
    for i in 0..sim.encounters.len() {
        if sim.encounters[i].optional {
            continue;
        }
        for m in sim.encounters[i].mobs.clone() {
            sim.kill(m);
        }
    }
    sim.step();
    assert_eq!(sim.outcome, RunOutcome::Victory);
    let opt = sim.encounters.iter().find(|e| e.optional).unwrap();
    assert!(!opt.cleared);
}

#[test]
fn heroic_raises_enemy_health_and_level() {
    let c = Content::builtin();
    let party = default_party(ClassId::Striker);
    let normal = Sim::new(&c, &party, 1);
    let heroic = Sim::with_options(
        &c,
        &party,
        1,
        RunOptions {
            difficulty: "heroic".into(),
            ..Default::default()
        },
    );
    let b = normal.boss;
    assert!(heroic.actors[b].max_hp > normal.actors[b].max_hp * 1.5);
    assert_eq!(
        heroic.actors[b].stats.level,
        normal.actors[b].stats.level + 2
    );
}

#[test]
fn boss_resets_after_wipe_and_phase_retriggers_on_next_pull() {
    let mut sim = passive_sim();
    let boss = pull_boss(&mut sim);
    let max = sim.actors[boss].max_hp;
    let home = sim.actors[boss].enemy.as_ref().unwrap().home;
    for p in sim.party.clone() {
        make_immortal(&mut sim, p);
    }
    sim.actors[boss].hp = max * 0.4;
    steps(&mut sim, 2.5);
    assert_eq!(sim.boss_phase(), 2);
    for p in sim.party.clone() {
        sim.kill(p);
    }
    steps(&mut sim, 0.1);
    assert_eq!(sim.outcome, RunOutcome::Wipe);
    assert_eq!(
        count(&sim, |k| matches!(k, EventKind::EncounterReset { .. })),
        1
    );
    assert!(sim.telegraphs.is_empty());
    steps(&mut sim, 10.0);
    let b = &sim.actors[boss];
    assert_eq!(b.mode(), Some(EnemyMode::Idle));
    assert_eq!(b.hp, max);
    assert_eq!(sim.boss_phase(), 1);
    assert!(b.pos.dist(home) < 0.5);

    let t = &mut sim.actors[TANK];
    t.alive = true;
    t.hp = t.max_hp;
    let enc = sim.content.final_encounter();
    sim.engage(enc, TANK);
    sim.actors[boss].hp = max * 0.45;
    steps(&mut sim, 0.5);
    assert_eq!(
        count(&sim, |k| matches!(k, EventKind::PhaseChange { .. })),
        2
    );
    assert_eq!(sim.encounters[enc].pulls, 2);
}

#[test]
fn enemies_leash_and_reset_when_dragged_too_far() {
    let mut sim = passive_sim();
    let enc = 2;
    let mob = sim.encounters[enc].mobs[0];
    place_near(&mut sim, TANK, mob, 2.0);
    sim.engage(enc, TANK);
    make_immortal(&mut sim, TANK);
    sim.actors[TANK].pos = sim.content.dungeon.entrance;
    steps(&mut sim, 30.0);
    assert!(
        count(
            &sim,
            |k| matches!(k, EventKind::EncounterReset { encounter } if *encounter == enc)
        ) >= 1
    );
    for &m in &sim.encounters[enc].mobs {
        let a = &sim.actors[m];
        assert_ne!(a.mode(), Some(EnemyMode::Engaged));
        assert_eq!(a.hp, a.max_hp);
    }
}

#[test]
fn proximity_aggro_pulls_the_whole_pack() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    place_near(&mut sim, TANK, mob, 5.0);
    sim.step();
    for &m in &sim.encounters[0].mobs {
        assert_eq!(sim.actors[m].mode(), Some(EnemyMode::Engaged));
    }
    assert_eq!(sim.actors[sim.boss].mode(), Some(EnemyMode::Idle));
}

// -------------------------------------------------------------- AI -----

#[test]
fn tank_ai_taunts_enemy_attacking_healer() {
    let mut sim = new_sim();
    let mob = first_skulker(&sim);
    place_near(&mut sim, TANK, mob, 8.0);
    place_near(&mut sim, HEALER, mob, 12.0);
    sim.engage(0, HEALER);
    sim.add_threat(mob, HEALER, 300.0);
    sim.actors[mob].target = Some(HEALER);
    sim.board = ai::compute_blackboard(&sim);
    let d = ai::party_decision(&sim, TANK);
    let challenge = slot(&sim, TANK, AbilityId::Challenge);
    assert_eq!(d.cast, Some((challenge, mob)));
}

#[test]
fn healer_ai_mends_lowest_ally() {
    let mut sim = new_sim();
    let mob = first_skulker(&sim);
    for p in sim.party.clone() {
        place_near(&mut sim, p, mob, 6.0 + p as f32);
    }
    sim.engage(0, TANK);
    sim.actors[TANK].hp = sim.actors[TANK].max_hp * 0.8;
    sim.actors[MELEE].hp = sim.actors[MELEE].max_hp * 0.3;
    sim.board = ai::compute_blackboard(&sim);
    let d = ai::party_decision(&sim, HEALER);
    let mend = slot(&sim, HEALER, AbilityId::Mend);
    assert_eq!(d.cast, Some((mend, MELEE)));
}

#[test]
fn healer_ai_dispels_and_support_profile_dispels_first() {
    let mut sim = new_sim();
    let mob = first_skulker(&sim);
    for p in sim.party.clone() {
        place_near(&mut sim, p, mob, 6.0 + p as f32);
    }
    sim.engage(0, TANK);
    sim.apply_aura(MELEE, brand(mob, true));
    sim.apply_aura(
        TANK,
        aura("Weakened Soul", AuraKind::WeakenedSoul, HEALER, 1, false),
    );
    sim.actors[TANK].hp = sim.actors[TANK].max_hp * 0.5;
    sim.board = ai::compute_blackboard(&sim);
    let purify = slot(&sim, HEALER, AbilityId::Purify);
    let mend = slot(&sim, HEALER, AbilityId::Mend);
    assert_eq!(ai::party_decision(&sim, HEALER).cast, Some((mend, TANK)));
    sim.actors[HEALER].profile = AiProfile::SupportFocused;
    assert_eq!(ai::party_decision(&sim, HEALER).cast, Some((purify, MELEE)));
}

#[test]
fn ai_moves_out_of_noticed_telegraph_and_respects_reaction_time() {
    let mut sim = new_sim();
    let center = sim.actors[RANGED].pos;
    sim.telegraphs.push(Telegraph {
        id: 1,
        source: sim.boss,
        what: Label::from("Ring"),
        school: School::Fire,
        center,
        radius: 5.0,
        remaining: 2.4,
        total: 2.5,
        damage: 100.0,
    });
    sim.board = ai::compute_blackboard(&sim);
    assert!(ai::escape_point(&sim, RANGED).is_none(), "not noticed yet");
    sim.telegraphs[0].remaining = 1.0;
    sim.board = ai::compute_blackboard(&sim);
    let goal = ai::escape_point(&sim, RANGED).expect("dodges");
    assert!(goal.dist(center) > 5.0 + sim.actors[RANGED].radius);
    sim.telegraphs[0].remaining = 2.25;
    sim.actors[RANGED].profile = AiProfile::MechanicFocused;
    sim.board = ai::compute_blackboard(&sim);
    assert!(ai::escape_point(&sim, RANGED).is_some(), "fast reactions");
}

#[test]
fn blackboard_assigns_one_interrupter_and_prioritises_healer_casters() {
    let mut sim = Sim::new(
        &Content::builtin_warrens(),
        &party_at_level(ClassId::Striker, 5, AiProfile::Balanced),
        2,
    );
    let shaman = sim.encounters[0].mobs[2];
    for p in sim.party.clone() {
        place_near(&mut sim, p, shaman, 1.5 + p as f32 * 0.3);
    }
    sim.engage(0, TANK);
    sim.actors[shaman].cast = Some(Cast {
        kind: CastKind::Enemy(0),
        name: Label::from("Mending Sap"),
        target: shaman,
        remaining: 1.5,
        total: 2.0,
        interruptible: true,
    });
    let b = ai::compute_blackboard(&sim);
    assert_eq!(b.interrupts.len(), 1);
    assert_eq!(b.interrupts[0].0, shaman);
    let kill = b.kill_target.unwrap();
    assert!(
        sim.actors[kill].name.contains("Rootweaver"),
        "kill target {}",
        sim.actors[kill].name
    );
}

#[test]
fn arcanist_hexes_an_extra_enemy_in_big_pulls() {
    let mut sim = Sim::new(
        &Content::builtin(),
        &party_at_level(ClassId::Striker, 3, AiProfile::Balanced),
        2,
    );
    let enc = 2; // four skulkers
    let first = sim.encounters[enc].mobs[0];
    for p in sim.party.clone() {
        place_near(&mut sim, p, first, 6.0);
        make_immortal(&mut sim, p);
    }
    sim.engage(enc, TANK);
    steps(&mut sim, 4.0);
    assert!(sim.metrics.cc_applied >= 1);
    assert!(!AiProfile::Aggressive.params().use_cc);
}

#[test]
fn followers_follow_the_leader_out_of_combat() {
    let mut sim = new_sim();
    sim.actors[TANK].controller = Controller::Player;
    sim.player = Some(TANK);
    sim.set_player_move(V2::new(0.0, 1.0));
    steps(&mut sim, 3.0);
    sim.set_player_move(V2::ZERO);
    steps(&mut sim, 3.0);
    let lead = sim.actors[TANK].pos;
    for p in [PLAYER, HEALER, RANGED, MELEE] {
        assert!(
            sim.actors[p].pos.dist(lead) < 9.0,
            "{} lags behind",
            sim.actors[p].name
        );
    }
}

#[test]
fn profiles_produce_measurably_different_behaviour() {
    let c = Content::builtin();
    let (mut agg_fail, mut con_fail) = (0, 0);
    let (mut agg_time, mut con_time) = (0.0, 0.0);
    for seed in 1..=4 {
        let party = |p| party_at_level(ClassId::Striker, 2, p);
        let (_, a) = run_headless(&c, &party(AiProfile::Aggressive), seed, 1500.0);
        let (_, b) = run_headless(&c, &party(AiProfile::Conservative), seed, 1500.0);
        agg_fail += a.threat_failures;
        con_fail += b.threat_failures;
        agg_time += a.duration;
        con_time += b.duration;
    }
    assert!(
        agg_fail > con_fail,
        "threat failures: aggressive {agg_fail} vs conservative {con_fail}"
    );
    assert!(
        agg_time < con_time,
        "duration: aggressive {agg_time} vs conservative {con_time}"
    );
}

// ------------------------------------------------- headless / config ---

#[test]
fn headless_runs_are_reproducible_for_a_seed() {
    let content = Content::builtin();
    let party = default_party(ClassId::Striker);
    let (_, a) = run_headless(&content, &party, 11, 1500.0);
    let (_, b) = run_headless(&content, &party, 11, 1500.0);
    assert_eq!(a, b);
    assert!(a.event_count > 100);
    assert!(a.party_damage > 0.0);
    let (_, c) = run_headless(&content, &party, 12, 1500.0);
    assert_ne!(a.log_hash, c.log_hash, "different seeds diverge");
}

#[test]
fn sunken_vault_is_mostly_clearable_at_level_one() {
    let content = Content::builtin();
    for class in ClassId::ALL {
        let party = default_party(class);
        let wins = (1..=5)
            .filter(|&seed| {
                run_headless(&content, &party, seed, 1500.0).1.outcome == RunOutcome::Victory
            })
            .count();
        assert!(wins >= 3, "{class:?}: only {wins}/5 wins");
    }
}

#[test]
fn warrens_is_clearable_at_level_six() {
    let content = Content::builtin_warrens();
    let party = party_at_level(ClassId::Arcanist, 6, AiProfile::Balanced);
    let wins = (1..=3)
        .filter(|&seed| {
            run_headless(&content, &party, seed, 1500.0).1.outcome == RunOutcome::Victory
        })
        .count();
    assert!(wins >= 2, "only {wins}/3 wins");
}

#[test]
fn hp_multiplier_changes_enemy_health_and_run_length() {
    let text = BUILTIN_SUNKEN_VAULT.replace("hp_multiplier = 1.0", "hp_multiplier = 1.5");
    let tougher = Content::from_toml_str(&text, "test").unwrap();
    let normal = Content::builtin();
    let party = party_at_level(ClassId::Striker, 3, AiProfile::Balanced);
    let s1 = Sim::new(&normal, &party, 1);
    let s2 = Sim::new(&tougher, &party, 1);
    assert_eq!(s2.actors[s2.boss].max_hp, 1.5 * s1.actors[s1.boss].max_hp);
    let (_, r1) = run_headless(&normal, &party, 1, 1500.0);
    let (_, r2) = run_headless(&tougher, &party, 1, 1500.0);
    assert!(
        r2.duration > r1.duration,
        "{} vs {}",
        r2.duration,
        r1.duration
    );
}

#[test]
fn absurd_hp_multiplier_leads_to_enrage_wipe_not_a_hang() {
    let text = BUILTIN_SUNKEN_VAULT.replace("hp_multiplier = 1.0", "hp_multiplier = 10.0");
    let content = Content::from_toml_str(&text, "test").unwrap();
    let (_, s) = run_headless(&content, &default_party(ClassId::Striker), 1, 1500.0);
    assert_ne!(s.outcome, RunOutcome::Victory);
    assert_ne!(s.outcome, RunOutcome::InProgress);
}

#[test]
fn heroic_is_harder_than_normal() {
    let c = Content::builtin();
    let party = party_at_level(ClassId::Striker, 2, AiProfile::Balanced);
    let deaths = |diff: &str| -> u32 {
        (1..=3)
            .map(|seed| run_with(&c, &party, seed, 1500.0, diff).1.party_deaths)
            .sum()
    };
    assert!(deaths("heroic") > deaths("normal"));
}

// ------------------------------------------------------------ rally ----

#[test]
fn fallen_members_rally_after_combat_but_not_during_it() {
    let mut sim = passive_sim();
    sim.kill(MELEE);
    let mob = first_skulker(&sim);
    place_near(&mut sim, TANK, mob, 1.0);
    sim.engage(0, TANK);
    make_immortal(&mut sim, TANK);
    steps(&mut sim, 12.0);
    assert!(
        !sim.actors[MELEE].alive,
        "no rally while enemies are engaged"
    );
    for m in sim.encounters[0].mobs.clone() {
        sim.kill(m);
    }
    steps(&mut sim, crate::sim::RALLY_DELAY + 1.0);
    assert!(sim.actors[MELEE].alive);
    assert_eq!(count(&sim, |k| matches!(k, EventKind::Revive { .. })), 1);
}

#[test]
fn wipe_is_final_no_rally() {
    let mut sim = passive_sim();
    for p in sim.party.clone() {
        sim.kill(p);
    }
    steps(&mut sim, 20.0);
    assert_eq!(sim.outcome, RunOutcome::Wipe);
    assert!(sim.alive_party().next().is_none());
}

#[test]
fn unbalanced_parties_still_finish() {
    let content = Content::builtin();
    let no_healer = vec![
        PartyMemberSpec::new("A", ClassId::Warden),
        PartyMemberSpec::new("B", ClassId::Striker),
        PartyMemberSpec::new("C", ClassId::Striker),
        PartyMemberSpec::new("D", ClassId::Arcanist),
        PartyMemberSpec::new("E", ClassId::Arcanist),
    ];
    let no_tank = vec![
        PartyMemberSpec::new("A", ClassId::Mender),
        PartyMemberSpec::new("B", ClassId::Striker),
        PartyMemberSpec::new("C", ClassId::Arcanist),
    ];
    for party in [no_healer, no_tank] {
        let (_, s) = run_headless(&content, &party, 4, 1500.0);
        assert_ne!(s.outcome, RunOutcome::InProgress);
    }
}

// ------------------------------------------------- player input path ---

#[test]
fn player_of_every_class_can_move_target_and_use_abilities() {
    for class in ClassId::ALL {
        let mut profile = crate::profile::Profile::new_default();
        profile.player_class = class;
        profile.auto_fill_party();
        let mut sim = Sim::new(&Content::builtin(), &profile.party_specs(true), 5);
        let p = sim.player.expect("player slot");
        let start = sim.actors[p].pos;
        sim.set_player_move(V2::new(0.0, 1.0));
        steps(&mut sim, 2.0);
        sim.set_player_move(V2::ZERO);
        assert!(
            sim.actors[p].pos.z > start.z + 6.0,
            "{class:?} did not move"
        );

        let mob = first_skulker(&sim);
        make_immortal(&mut sim, mob);
        sim.actors[mob].stats.level = 1;
        place_near(&mut sim, p, mob, 1.0);
        sim.set_player_target(Some(mob));
        let before = sim.metrics.actors[p].casts;
        for s in 0..4 {
            sim.actors[p].resource = sim.actors[p].max_resource;
            sim.actors[p].hp = sim.actors[p].max_hp * 0.8;
            sim.queue_player_cast(s);
            steps(&mut sim, 2.6);
        }
        let failures: Vec<String> = sim
            .events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::CastFailed { src, .. } if src == p))
            .map(|e| sim.describe(e))
            .collect();
        assert!(failures.is_empty(), "{class:?}: {failures:?}");
        assert_eq!(sim.metrics.actors[p].casts - before, 4, "{class:?} casts");
    }
}

// ------------------------------------------------------------ raids -----

#[test]
fn raid_party_builder_has_tanks_and_healers() {
    for size in [5, 10, 25] {
        let party = raid_party(size, 6, AiProfile::Balanced);
        assert_eq!(party.len(), size);
        let tanks = party.iter().filter(|s| s.class == ClassId::Warden).count();
        let healers = party.iter().filter(|s| s.class == ClassId::Mender).count();
        assert!(
            tanks >= 1 && healers >= 1,
            "{size}: {tanks} tanks {healers} healers"
        );
        assert!(party[0].class.role().is_dps());
    }
}

#[test]
fn large_parties_simulate_without_panics() {
    let content = Content::builtin_warrens();
    for size in [10, 25] {
        let party = raid_party(size, 6, AiProfile::Balanced);
        let (_, s) = run_headless(&content, &party, 1, 1500.0);
        assert_ne!(s.outcome, RunOutcome::InProgress);
        assert_eq!(s.members.len(), size);
    }
}

#[test]
fn non_tanks_step_out_of_frontal_cleave_cones() {
    let mut sim = Sim::new(
        &Content::builtin_warrens(),
        &party_at_level(ClassId::Striker, 6, AiProfile::Balanced),
        3,
    );
    let enc = encounter_index(&sim, "brood_chamber");
    let boss = sim.encounters[enc].mobs[0];
    let bp = sim.actors[boss].pos;
    sim.actors[TANK].pos = bp + V2::new(0.0, -2.5);
    sim.actors[boss].facing = std::f32::consts::PI; // facing the tank (-Z)
    sim.actors[HEALER].pos = bp + V2::new(0.5, -5.0); // in the cone
    sim.engage(enc, TANK);
    sim.add_threat(boss, TANK, 1e6);
    sim.actors[boss].target = Some(TANK);
    sim.board = ai::compute_blackboard(&sim);
    assert!(sim.board.frontal.contains(&boss));
    let d = ai::party_decision(&sim, HEALER);
    let goal = d.move_goal.expect("moves out of the cone");
    assert!(
        !crate::stats::is_in_front(bp, sim.actors[boss].facing, goal),
        "goal {goal:?} is still in front of the boss"
    );
}

#[test]
fn healers_hold_pre_emptive_heals_right_after_a_pull() {
    let mut sim = new_sim();
    let mob = first_skulker(&sim);
    for p in sim.party.clone() {
        place_near(&mut sim, p, mob, 6.0 + p as f32);
    }
    sim.engage(0, TANK);
    sim.actors[TANK].hp = sim.actors[TANK].max_hp * 0.8; // would normally get a Renew
    sim.board = ai::compute_blackboard(&sim);
    assert!(sim.board.since_pull < 1.0);
    let d = ai::party_decision(&sim, HEALER);
    assert!(
        d.cast.is_none(),
        "no top-up heals in the first seconds: {d:?}"
    );
    sim.time += 5.0;
    sim.board = ai::compute_blackboard(&sim);
    let renew = slot(&sim, HEALER, AbilityId::Renew);
    assert_eq!(ai::party_decision(&sim, HEALER).cast, Some((renew, TANK)));
}
