//! Rule-level tests for combat, threat, resources, auras, enemy abilities, AI,
//! encounters and determinism.

use crate::ai::{self, AiProfile};
use crate::class::{AbilityId, ClassId};
use crate::content::{BUILTIN_SUNKEN_VAULT, Content};
use crate::events::{CastError, EventKind, Label};
use crate::math::{V2, V3};
use crate::runner::{default_party, party_at_level, raid_party, run_headless, run_with};
use crate::sim::{
    ActorId, Aura, AuraKind, COMBAT_LINGER, Cast, CastKind, Controller, DT, DamageSpec, EnemyMode,
    FALL_SPEED, HEAL_THREAT_FACTOR, PartyMemberSpec, RunOptions, RunOutcome, Sim, Telegraph,
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
    sim.set_actor_pos(who, tp + V2::new(0.0, -dist));
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
    sim.set_actor_pos(PLAYER, pos + V2::new(0.0, -1.5)); // behind (mob faces +Z)
    for _ in 0..100 {
        sim.actors[mob].facing = 0.0;
        sim.deal(PLAYER, mob, melee());
    }
    assert_eq!(parries(&sim), 0, "no parries from behind");
    sim.set_actor_pos(PLAYER, pos + V2::new(0.0, 1.5)); // in front
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
    sim.set_actor_pos(RANGED, center + V2::new(0.0, -12.0));
    sim.telegraphs.push(Telegraph {
        y: 0.0,
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
    sim.set_actor_pos(TANK, bp + V2::new(0.0, -2.5));
    sim.set_actor_pos(MELEE, bp + V2::new(0.0, 2.5));
    sim.set_actor_pos(PLAYER, bp + V2::new(0.5, -2.8));
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
    sim.set_actor_pos(MELEE, pos);
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
    assert!(b.loc().dist(home) < 0.5);

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
    sim.set_actor_pos(TANK, sim.content.dungeon.entrance);
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
        y: 0.0,
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
    assert!(goal.xz().dist(center) > 5.0 + sim.actors[RANGED].radius);
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
    // The Weavers' Den: four sturdy enemies, worth controlling one.
    let mut sim = Sim::new(
        &Content::builtin_warrens(),
        &party_at_level(ClassId::Striker, 5, AiProfile::Balanced),
        2,
    );
    let enc = 0;
    let first = sim.encounters[enc].mobs[0];
    for p in sim.party.clone() {
        place_near(&mut sim, p, first, 6.0);
        make_immortal(&mut sim, p);
    }
    sim.engage(enc, TANK);
    steps(&mut sim, 4.0);
    assert!(sim.metrics.cc_applied >= 1);
    assert!(!AiProfile::Aggressive.params().use_cc);

    // Short-lived enemies (600 HP skulkers) are not worth a Hex.
    let mut sim = Sim::new(
        &Content::builtin(),
        &party_at_level(ClassId::Striker, 3, AiProfile::Balanced),
        2,
    );
    let first = sim.encounters[2].mobs[0];
    for p in sim.party.clone() {
        place_near(&mut sim, p, first, 6.0);
        make_immortal(&mut sim, p);
    }
    sim.engage(2, TANK);
    steps(&mut sim, 4.0);
    assert_eq!(sim.metrics.cc_applied, 0);
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
    sim.set_actor_pos(TANK, bp + V2::new(0.0, -2.5));
    sim.actors[boss].facing = std::f32::consts::PI; // facing the tank (-Z)
    sim.set_actor_pos(HEALER, bp + V2::new(0.5, -5.0)); // in the cone
    sim.engage(enc, TANK);
    sim.add_threat(boss, TANK, 1e6);
    sim.actors[boss].target = Some(TANK);
    sim.board = ai::compute_blackboard(&sim);
    assert!(sim.board.frontal.contains(&boss));
    let d = ai::party_decision(&sim, HEALER);
    let goal = d.move_goal.expect("moves out of the cone");
    assert!(
        !crate::stats::is_in_front(bp, sim.actors[boss].facing, goal.xz()),
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

#[test]
fn summoned_adds_inherit_half_the_summoners_threat() {
    let mut sim = warrens_passive();
    let enc = encounter_index(&sim, "brood_chamber");
    let boss = sim.encounters[enc].mobs[0];
    place_near(&mut sim, TANK, boss, 2.0);
    make_immortal(&mut sim, TANK);
    sim.engage(enc, TANK);
    sim.add_threat(boss, TANK, 1000.0);
    sim.actors[boss].target = Some(TANK);
    steps(&mut sim, 8.1);
    let adds: Vec<ActorId> = sim.encounters[enc].mobs[1..].to_vec();
    assert_eq!(adds.len(), 2);
    for &a in &adds {
        let t = sim.threat(a, TANK);
        assert!(
            (t - 0.5 * sim.threat(boss, TANK)).abs() < 60.0,
            "add threat {t} vs boss {}",
            sim.threat(boss, TANK)
        );
        assert_eq!(sim.actors[a].target, Some(TANK));
    }
}

#[test]
fn familiarity_and_alertness_sharpen_reactions() {
    let mut sim = new_sim();
    let base = ai::params_of(&sim.actors[RANGED]).reaction_time;
    sim.actors[RANGED].familiarity = 1.0;
    let familiar = ai::params_of(&sim.actors[RANGED]);
    assert!(familiar.reaction_time < base * 0.7);
    assert!(familiar.interrupt_window > AiProfile::Balanced.params().interrupt_window);
    // Taking an avoidable hit raises alertness for the rest of the run.
    let mut spec = DamageSpec::new(
        10.0,
        School::Fire,
        HitKind::Unavoidable,
        Label::from("Ring"),
    );
    spec.avoidable = true;
    let src = sim.boss;
    sim.deal(src, MELEE, spec.clone());
    assert!(sim.actors[MELEE].alertness > 0.0);
    for _ in 0..10 {
        sim.deal(src, MELEE, spec.clone());
    }
    assert!((sim.actors[MELEE].alertness - crate::sim::MAX_ALERTNESS).abs() < 1e-6);
}

#[test]
fn victories_build_companion_familiarity() {
    use crate::profile::{Profile, RunReport};
    let content = Content::builtin();
    let mut p = Profile::new_default();
    let report = RunReport {
        dungeon: content.dungeon.id.clone(),
        difficulty: "normal".into(),
        reward_multiplier: 1.0,
        rarity_bonus: 0,
        outcome: RunOutcome::Victory,
        cleared: vec![],
        kills: vec![],
        duration: 1.0,
        deaths: 0,
        seed: 1,
    };
    for _ in 0..3 {
        p.grant_rewards(&content, &report);
    }
    let specs = p.party_specs_for(&content.dungeon.id, true);
    assert_eq!(specs[0].familiarity, 0.0, "the player is not AI-driven");
    for s in &specs[1..] {
        assert!((s.familiarity - 0.6).abs() < 1e-6, "{}", s.familiarity);
    }
    let other = p.party_specs_for("gloomroot_warrens", true);
    assert!(other[1..].iter().all(|s| s.familiarity == 0.0));
}

#[test]
fn sandbox_overrides_scale_health_timers_and_rewards() {
    use crate::sim::Overrides;
    let c = Content::builtin();
    let party = default_party(ClassId::Striker);
    let normal = Sim::new(&c, &party, 1);
    let o = Overrides {
        hp: 2.0,
        damage: 1.5,
        ability_speed: 2.0,
        telegraph: 0.5,
    };
    let tuned = Sim::with_options(
        &c,
        &party,
        1,
        RunOptions {
            overrides: o,
            ..Default::default()
        },
    );
    let b = normal.boss;
    assert_eq!(tuned.actors[b].max_hp, 2.0 * normal.actors[b].max_hp);
    let ring = |s: &Sim| {
        s.content.templates["hollow_king"]
            .abilities
            .iter()
            .find(|a| a.name == "Cinder Ring")
            .cloned()
            .unwrap()
    };
    assert!((ring(&tuned).interval - ring(&normal).interval / 2.0).abs() < 1e-4);
    assert!(ring(&tuned).delay < ring(&normal).delay);
    assert!(ring(&tuned).delay < ring(&tuned).interval, "invariant kept");
    // Harder settings never raise rewards; easier ones lower them.
    assert_eq!(o.reward_factor(), 1.0);
    let easy = Overrides {
        hp: 0.5,
        ..Default::default()
    };
    assert!((easy.reward_factor() - 0.5).abs() < 1e-6);
    assert_eq!(
        Overrides {
            hp: 100.0,
            ..Default::default()
        }
        .clamped()
        .hp,
        Overrides::MAX
    );
}

#[test]
fn raid_groups_auto_fill_two_tanks_and_two_healers() {
    use crate::class::Role;
    let mut p = crate::profile::Profile::new_default();
    assert_eq!(p.roster.len(), 12);
    for class in ClassId::ALL {
        p.player_class = class;
        p.set_group_size(10);
        assert_eq!(p.party.len(), 9);
        let mut roles: Vec<Role> = p
            .party_companions()
            .iter()
            .map(|c| c.class.role())
            .collect();
        roles.push(class.role());
        assert_eq!(
            roles.iter().filter(|r| **r == Role::Tank).count(),
            2,
            "{class:?}"
        );
        assert_eq!(
            roles.iter().filter(|r| **r == Role::Healer).count(),
            2,
            "{class:?}"
        );
        assert!(
            p.party_warnings().is_empty(),
            "{class:?}: {:?}",
            p.party_warnings()
        );
    }
    p.set_group_size(5);
    assert_eq!(p.party.len(), 4);
    // Old saves with 8 companions get the new recruits.
    p.roster.truncate(8);
    p.ensure_roster();
    assert_eq!(p.roster.len(), 12);
}

#[test]
fn raid10_difficulty_exists_and_ten_person_group_can_clear() {
    let c = Content::builtin_warrens();
    assert!(c.difficulty("raid10").is_some());
    let party = raid_party(10, 6, AiProfile::Balanced);
    let (_, s) = run_with(&c, &party, 2, 1500.0, "raid10");
    assert_eq!(s.outcome, RunOutcome::Victory, "{s:?}");
}

#[test]
fn ashen_spire_loads_and_is_clearable_at_level_ten() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/content/dungeons");
    let lib = crate::content::ContentLibrary::load(dir);
    let spire = lib.get("ashen_spire").expect("third dungeon ships").clone();
    let council = spire
        .encounters
        .iter()
        .position(|e| e.id == "twin_altar")
        .unwrap();
    let bosses = spire.encounters[council]
        .mobs
        .iter()
        .filter(|m| spire.templates[&m.template].rank == crate::content::EnemyRank::Boss)
        .count();
    assert_eq!(bosses, 2, "council encounter has two bosses");
    let party = party_at_level(ClassId::Mender, 10, AiProfile::Balanced);
    let wins = (1..=3)
        .filter(|&seed| run_headless(&spire, &party, seed, 1500.0).1.outcome == RunOutcome::Victory)
        .count();
    assert!(wins >= 2, "only {wins}/3 wins");
}

#[test]
fn non_tanks_walk_around_frontal_cones_instead_of_through_them() {
    let mut sim = Sim::new(
        &Content::builtin_warrens(),
        &party_at_level(ClassId::Striker, 6, AiProfile::Balanced),
        3,
    );
    let enc = encounter_index(&sim, "brood_chamber");
    let boss = sim.encounters[enc].mobs[0];
    let bp = sim.actors[boss].pos;
    sim.set_actor_pos(TANK, bp + V2::new(0.0, -2.5));
    sim.actors[boss].facing = std::f32::consts::PI; // facing -Z
    sim.engage(enc, TANK);
    sim.add_threat(boss, TANK, 1e6);
    sim.actors[boss].target = Some(TANK);
    sim.set_actor_pos(HEALER, bp + V2::new(-10.0, -2.0));
    sim.board = ai::compute_blackboard(&sim);
    let goal = V3::at(bp + V2::new(10.0, -2.0), 0.0); // straight line crosses the cone
    let step = ai::route_around_cones(&sim, HEALER, goal);
    assert_ne!(step, goal, "takes a detour");
    assert!(
        !crate::stats::is_in_front(bp, sim.actors[boss].facing, step.xz())
            || step.xz().dist(bp) > 9.0,
        "detour {step:?} stays out of the cone"
    );
    // A path that never touches the cone is left alone.
    let clear = V3::at(bp + V2::new(-10.0, 6.0), 0.0);
    assert_eq!(ai::route_around_cones(&sim, HEALER, clear), clear);
}

// ------------------------------------------------- 0.3 UI-facing APIs ---

fn player_mender_sim() -> Sim {
    let mut p = crate::profile::Profile::new_default();
    p.player_class = ClassId::Mender;
    p.auto_fill_party();
    let mut sim = Sim::new(&Content::builtin(), &p.party_specs(true), 5);
    for q in sim.party.clone() {
        sim.actors[q].controller = Controller::Player;
    }
    sim.player = Some(0);
    sim
}

#[test]
fn mouseover_heal_targets_the_hovered_ally_and_keeps_selection() {
    let mut sim = player_mender_sim();
    let mob = first_skulker(&sim);
    sim.set_player_target(Some(mob));
    let ally = 3;
    sim.actors[ally].hp = sim.actors[ally].max_hp * 0.5;
    let renew = slot(&sim, 0, AbilityId::Renew);
    sim.queue_player_cast_on(renew, Some(ally));
    sim.step();
    assert!(sim.actors[ally].has_hot_from(0), "hovered ally got the HoT");
    assert_eq!(sim.actors[0].target, Some(mob), "selection unchanged");
}

#[test]
fn friendly_spell_target_order_is_mouseover_then_selected_ally_then_self() {
    let mut sim = player_mender_sim();
    let renew = slot(&sim, 0, AbilityId::Renew);
    // Invalid (dead) mouseover falls back to the selected friendly target.
    sim.kill(4);
    sim.set_player_target(Some(2));
    sim.queue_player_cast_on(renew, Some(4));
    sim.step();
    assert!(sim.actors[2].has_hot_from(0));
    // A hostile mouseover is not a valid friendly target either: self-cast.
    let mut sim = player_mender_sim();
    let mob = first_skulker(&sim);
    sim.set_player_target(Some(mob));
    sim.queue_player_cast_on(renew, Some(mob));
    sim.step();
    assert!(sim.actors[0].has_hot_from(0), "self-cast fallback");
}

#[test]
fn hostile_spells_ignore_mouseover() {
    let mut sim = player_mender_sim();
    let mob = first_skulker(&sim);
    place_near(&mut sim, 0, mob, 10.0);
    sim.set_player_target(Some(mob));
    let smite = slot(&sim, 0, AbilityId::Smite);
    sim.queue_player_cast_on(smite, Some(2));
    sim.step();
    assert_eq!(sim.actors[0].cast.as_ref().map(|c| c.target), Some(mob));
}

#[test]
fn ability_state_matches_what_a_cast_would_do() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    sim.player = Some(PLAYER);
    let rend = slot(&sim, PLAYER, AbilityId::RendStrike);
    let st = sim.ability_state(PLAYER, rend, Some(mob));
    assert_eq!(st.in_range, Some(false));
    assert_eq!(st.result, Err(CastError::OutOfRange));
    place_near(&mut sim, PLAYER, mob, 1.0);
    let st = sim.ability_state(PLAYER, rend, Some(mob));
    assert_eq!(st.in_range, Some(true));
    assert_eq!(st.result, Ok(()));
    assert_eq!(
        sim.validate_cast(PLAYER, rend, Some(mob)).is_ok(),
        st.result.is_ok()
    );
    sim.try_cast(PLAYER, rend, Some(mob)).unwrap();
    let st = sim.ability_state(PLAYER, rend, Some(mob));
    assert!(
        st.gcd_left > 0.0 && st.cooldown_left == 0.0,
        "GCD, not a cooldown"
    );
    assert_eq!(st.result, Err(CastError::OnGlobalCooldown));
    // Energy: 100 - 40 = 60 left after Rend; another needs 40 -> available now,
    // but after draining to 10 it needs 3 s at 10 energy/s.
    sim.actors[PLAYER].resource = 10.0;
    let st = sim.ability_state(PLAYER, rend, Some(mob));
    assert!((st.resource_in.unwrap() - 3.0).abs() < 1e-4);
    assert!((sim.resource_rate(PLAYER) - 10.0).abs() < 1e-6);
}

#[test]
fn threat_view_reports_secure_contested_lost_and_taunt() {
    use crate::sim::{PersonalThreat, TankThreat};
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    place_near(&mut sim, TANK, mob, 1.0);
    place_near(&mut sim, MELEE, mob, 1.5);
    sim.engage(0, TANK);
    sim.add_threat(mob, TANK, 1000.0);
    sim.actors[mob].target = Some(TANK);
    let v = sim.threat_view(mob, TANK);
    assert_eq!(v.tank, TankThreat::Secure);
    assert_eq!(sim.threat_view(mob, MELEE).personal, PersonalThreat::Safe);
    // DPS climbs to 95% of the 110% melee threshold: contested / warning.
    sim.add_threat(mob, MELEE, 1045.0);
    assert_eq!(sim.threat_view(mob, TANK).tank, TankThreat::Contested);
    let dv = sim.threat_view(mob, MELEE);
    assert_eq!(dv.personal, PersonalThreat::Warning);
    assert!((dv.pull_at - 1100.0).abs() < 1e-3);
    // The DPS pulls it: lost for the tank, attacking for the DPS.
    sim.add_threat(mob, MELEE, 100.0);
    sim.actors[mob].target = sim.select_enemy_target(mob);
    assert_eq!(sim.threat_view(mob, TANK).tank, TankThreat::Lost);
    assert_eq!(
        sim.threat_view(mob, MELEE).personal,
        PersonalThreat::Attacking
    );
    // A taunt immediately makes it secure again.
    let s = slot(&sim, TANK, AbilityId::Challenge);
    sim.try_cast(TANK, s, Some(mob)).unwrap();
    assert_eq!(sim.threat_view(mob, TANK).tank, TankThreat::Secure);
}

// ------------------------------------------- 0.4 navigation and space ---

fn cavern_passive() -> Sim {
    let mut sim = Sim::new(
        &Content::builtin_cavern(),
        &party_at_level(ClassId::Striker, 2, AiProfile::Balanced),
        5,
    );
    for p in sim.party.clone() {
        sim.actors[p].controller = Controller::Player;
    }
    sim
}

fn surf(sim: &Sim, name: &str) -> crate::nav::SurfaceId {
    sim.nav
        .find(name)
        .unwrap_or_else(|| panic!("no area {name}"))
}

/// The skulker waiting in the underpass, right under the rope bridge.
fn lurker(sim: &Sim) -> ActorId {
    let e = sim
        .encounters
        .iter()
        .position(|e| e.id == "underpass_lurker")
        .unwrap();
    sim.encounters[e].mobs[0]
}

#[test]
fn shipped_dungeons_keep_their_flat_navigation() {
    let mut flat = 0;
    for text in [
        crate::content::BUILTIN_SUNKEN_VAULT,
        crate::content::BUILTIN_WARRENS,
        include_str!("../../../assets/content/dungeons/03_ashen_spire.toml"),
        include_str!("../../../assets/content/dungeons/04_cinder_court.toml"),
    ] {
        let c = Content::from_toml_str(text, "test").unwrap();
        let nav = c.nav();
        flat += 1;
        // Old maps: everything at height 0, no ramps or drops, all connected.
        assert!(
            nav.surfaces
                .iter()
                .all(|s| s.min_y() == 0.0 && !s.is_ramp()),
            "{}",
            c.dungeon.id
        );
        assert!(
            nav.links
                .iter()
                .all(|l| l.kind == crate::nav::LinkKind::Walk)
        );
        let (_, start) = Content::place(&nav, c.dungeon.entrance, None).unwrap();
        for s in 0..nav.surfaces.len() {
            assert!(nav.reachable(start, s as u16), "{} area {s}", c.dungeon.id);
        }
    }
    assert_eq!(flat, 4);
}

#[test]
fn cavern_layout_connects_by_height_not_by_overlap() {
    let sim = cavern_passive();
    let nav = &sim.nav;
    let bridge = surf(&sim, "Rope Bridge");
    let under = surf(&sim, "Underpass");
    let walkway = surf(&sim, "High Walkway");
    let gallery = surf(&sim, "Hanging Gallery");
    let basin = surf(&sim, "Magma Basin");
    let perch = surf(&sim, "Sniper Perch");
    let linked = |a, b| nav.links.iter().any(|l| l.from == a && l.to == b);
    // The bridge lies over the underpass but they are not connected.
    assert!(!linked(bridge, under) && !linked(under, bridge));
    assert!(linked(walkway, bridge) && linked(bridge, walkway));
    // The drop goes down only; the basin gets back up the long way round.
    assert!(
        nav.links
            .iter()
            .any(|l| l.from == gallery && l.to == basin && l.kind == crate::nav::LinkKind::Drop)
    );
    assert!(!linked(basin, gallery));
    assert!(nav.reachable(basin, gallery));
    let up = nav
        .path_points(V2::new(43.0, 110.0), basin, V3::new(43.0, 5.0, 90.0))
        .unwrap();
    assert!(up.len() > 6, "walks all the way round: {up:?}");
    let down = nav
        .path_points(V2::new(43.0, 92.0), gallery, V3::new(43.0, -3.0, 110.0))
        .unwrap();
    assert!(down.len() <= 4, "takes the drop: {down:?}");
    // The perch is unreachable from everywhere else.
    for s in 0..nav.surfaces.len() as u16 {
        if s != perch {
            assert!(!nav.reachable(s, perch));
        }
    }
}

#[test]
fn melee_cannot_hit_through_the_bridge() {
    let mut sim = cavern_passive();
    let mob = lurker(&sim);
    let mpos = sim.actors[mob].pos;
    // Directly above it on the bridge: horizontally overlapping, 5 m up.
    sim.set_actor_loc(PLAYER, V3::at(mpos + V2::new(0.0, 1.0), 5.0));
    assert_eq!(sim.actors[PLAYER].surface, surf(&sim, "Rope Bridge"));
    let range = sim.actors[PLAYER].attack_range;
    assert!(!sim.reach(PLAYER, mob, range));
    assert!(!sim.has_los(PLAYER, mob));
    // Same spot on the underpass floor: in range and in sight.
    sim.set_actor_loc(PLAYER, V3::at(mpos + V2::new(0.0, 1.5), 0.0));
    assert_eq!(sim.actors[PLAYER].surface, surf(&sim, "Underpass"));
    assert!(sim.reach(PLAYER, mob, range));
    assert!(sim.has_los(PLAYER, mob));
}

#[test]
fn casts_need_line_of_sight_through_floors_and_walls() {
    let mut sim = cavern_passive();
    let mob = lurker(&sim);
    let mpos = sim.actors[mob].pos;
    let slot = sim.actors[RANGED].ability_slot(AbilityId::ArcBolt).unwrap();
    // Arcanist on the bridge right above the lurker: in range, no sight.
    sim.set_actor_loc(RANGED, V3::at(mpos + V2::new(-1.0, 2.0), 5.0));
    assert_eq!(
        sim.validate_cast(RANGED, slot, Some(mob)),
        Err(CastError::NoLineOfSight)
    );
    // Down in the underpass, 12 m away along the passage: clear.
    sim.set_actor_loc(RANGED, V3::at(mpos + V2::new(0.0, 12.0), 0.0));
    assert_eq!(sim.validate_cast(RANGED, slot, Some(mob)), Ok(mob));
    // In the lava hollow, around the corner: in range, blocked by walls.
    sim.set_actor_loc(RANGED, V3::new(52.0, 0.0, 42.0));
    assert!(sim.reach(RANGED, mob, 30.0));
    assert_eq!(
        sim.validate_cast(RANGED, slot, Some(mob)),
        Err(CastError::NoLineOfSight)
    );
}

#[test]
fn ground_effects_only_hit_their_own_floor() {
    let mut sim = cavern_passive();
    let boss = sim.boss;
    let c = V2::new(42.0, 70.0);
    for p in [PLAYER, RANGED, MELEE] {
        sim.set_actor_loc(p, V3::new(0.0, 0.0, 0.0));
    }
    sim.set_actor_loc(TANK, V3::at(c + V2::new(0.0, 0.5), 5.0)); // on the bridge
    sim.set_actor_loc(HEALER, V3::at(c, 0.0)); // in the underpass below
    let (tank_hp, healer_hp) = (sim.actors[TANK].hp, sim.actors[HEALER].hp);
    sim.telegraphs.push(Telegraph {
        id: 1,
        source: boss,
        what: Label::from("Ring"),
        school: School::Fire,
        center: c,
        y: 5.0,
        radius: 4.0,
        remaining: 0.05,
        total: 2.0,
        damage: 300.0,
    });
    sim.step();
    assert!(
        sim.actors[TANK].hp < tank_hp,
        "the ring on the bridge hits the bridge"
    );
    assert_eq!(
        sim.actors[HEALER].hp, healer_hp,
        "and not the floor 5 m below"
    );
}

#[test]
fn walking_over_an_enemy_on_a_lower_floor_does_not_pull_it() {
    let mut sim = cavern_passive();
    let mob = lurker(&sim);
    let mpos = sim.actors[mob].pos;
    for p in sim.party.clone() {
        sim.set_actor_loc(p, V3::new(0.0, 0.0, 0.0));
    }
    // Right above it on the bridge: 5 m away, inside its 10 m aggro radius.
    sim.set_actor_loc(TANK, V3::at(mpos + V2::new(0.0, 1.0), 5.0));
    steps(&mut sim, 1.0);
    assert_eq!(sim.actors[mob].mode(), Some(EnemyMode::Idle));
    // The same distance on its own floor pulls it.
    sim.set_actor_loc(TANK, V3::at(mpos + V2::new(0.0, 5.0), 0.0));
    steps(&mut sim, 0.2);
    assert_eq!(sim.actors[mob].mode(), Some(EnemyMode::Engaged));
}

#[test]
fn stepping_off_the_drop_falls_instead_of_teleporting() {
    let mut sim = cavern_passive();
    sim.player = Some(PLAYER);
    let p = PLAYER;
    for q in sim.party.clone() {
        if q != p {
            sim.set_actor_loc(q, V3::new(0.0, 0.0, 0.0));
        }
    }
    sim.set_actor_loc(p, V3::new(43.0, 5.0, 95.0));
    assert_eq!(sim.actors[p].surface, surf(&sim, "Hanging Gallery"));
    sim.set_player_move(V2::new(0.0, 1.0));
    let mut last_y = sim.actors[p].y;
    let mut fell = false;
    for _ in 0..80 {
        sim.step();
        let y = sim.actors[p].y;
        assert!(
            last_y - y <= FALL_SPEED * DT + 1e-3,
            "fell {} in one tick",
            last_y - y
        );
        assert!(y <= last_y + 1e-3, "never goes up");
        fell |= y < 4.0 && y > -3.0;
        last_y = y;
    }
    assert!(fell, "passed through intermediate heights");
    assert_eq!(sim.actors[p].surface, surf(&sim, "Magma Basin"));
    assert!((sim.actors[p].y + 3.0).abs() < 1e-3);
    // Walking off the gallery's side (no drop zone) is blocked by the edge.
    sim.set_actor_loc(p, V3::new(43.0, 5.0, 85.0));
    sim.set_player_move(V2::new(1.0, 0.0));
    steps(&mut sim, 2.0);
    assert_eq!(sim.actors[p].surface, surf(&sim, "Hanging Gallery"));
    assert!((sim.actors[p].y - 5.0).abs() < 1e-3);
}

#[test]
fn enemies_give_up_on_targets_they_cannot_reach_or_see() {
    let mut sim = cavern_passive();
    let mob = lurker(&sim);
    for p in sim.party.clone() {
        sim.set_actor_loc(p, V3::new(0.0, 0.0, -6.0));
    }
    // Pull it, then put everyone on the unreachable perch, out of its sight.
    let near = sim.actors[mob].pos + V2::new(0.0, 5.0);
    sim.set_actor_loc(TANK, V3::at(near, 0.0));
    steps(&mut sim, 0.2);
    assert_eq!(sim.actors[mob].mode(), Some(EnemyMode::Engaged));
    for p in sim.party.clone() {
        sim.set_actor_loc(p, V3::new(62.0, 3.0, 110.0));
        assert_eq!(sim.actors[p].surface, surf(&sim, "Sniper Perch"));
        assert!(!sim.can_pursue(mob, p));
    }
    steps(&mut sim, 0.5);
    assert_ne!(
        sim.actors[mob].mode(),
        Some(EnemyMode::Engaged),
        "it evades instead of standing stuck"
    );
}

/// Full autopilot run. Every tick, every living party member stays on the
/// floor of its surface, never moves faster than it can run (plus a drop
/// landing), never rises more than a step and falls at most `FALL_SPEED`.
fn traverse(size: usize, seed: u64) -> (RunOutcome, f32, f32, u32) {
    traverse_in(&Content::builtin_cavern(), 2, size, seed)
}

fn traverse_in(
    content: &Content,
    level: u32,
    size: usize,
    seed: u64,
) -> (RunOutcome, f32, f32, u32) {
    let party = if size == 5 {
        party_at_level(ClassId::Striker, level, AiProfile::Balanced)
    } else {
        raid_party(size, level, AiProfile::Balanced)
    };
    let options = crate::sim::RunOptions {
        autopilot: true,
        ..Default::default()
    };
    let mut sim = Sim::with_options(content, &party, seed, options);
    let max_step = sim.nav.params.max_step;
    while sim.outcome == RunOutcome::InProgress && sim.time < 900.0 {
        let before: Vec<(V2, f32, bool)> =
            sim.actors.iter().map(|a| (a.pos, a.y, a.alive)).collect();
        sim.step();
        for &p in &sim.party {
            let a = &sim.actors[p];
            if !a.alive || !before[p].2 {
                continue;
            }
            let walk = &sim.nav.surfaces[a.surface as usize].walk;
            assert!(
                walk.contains_tol(a.pos, 0.01),
                "seed {seed}: {} left the floor of '{}' at {:?}",
                a.name,
                sim.nav.surfaces[a.surface as usize].name(),
                a.pos
            );
            let moved = a.pos.dist(before[p].0);
            assert!(
                moved <= a.speed * DT * 1.5 + 1.2,
                "seed {seed}: {} jumped {moved} m",
                a.name
            );
            let dy = a.y - before[p].1;
            assert!(dy <= max_step + 1e-3, "seed {seed}: {} rose {dy} m", a.name);
            assert!(
                -dy <= FALL_SPEED * DT + max_step + 1e-3,
                "seed {seed}: {} dropped {dy}",
                a.name
            );
        }
    }
    let stuck: f32 = sim
        .party
        .iter()
        .map(|&p| sim.actors[p].nav.stuck_time)
        .sum();
    let unreachable: f32 = sim
        .party
        .iter()
        .map(|&p| sim.actors[p].nav.unreachable_time)
        .sum();
    let dithers: u32 = sim.party.iter().map(|&p| sim.actors[p].nav.dithers).sum();
    (sim.outcome, stuck, unreachable, dithers)
}

#[test]
fn five_person_party_traverses_the_cavern() {
    for seed in 1..=3 {
        let (outcome, stuck, unreachable, dithers) = traverse(5, seed);
        assert_eq!(outcome, RunOutcome::Victory, "seed {seed}");
        assert!(stuck < 3.0, "seed {seed}: stuck {stuck} s");
        assert!(
            unreachable < 1.0,
            "seed {seed}: unreachable {unreachable} s"
        );
        assert!(dithers < 30, "seed {seed}: {dithers} dithers");
    }
}

#[test]
fn ten_person_raid_traverses_the_cavern() {
    for seed in 1..=2 {
        let (outcome, stuck, unreachable, dithers) = traverse(10, seed);
        assert_eq!(outcome, RunOutcome::Victory, "seed {seed}");
        assert!(stuck < 6.0, "seed {seed}: stuck {stuck} s");
        assert!(
            unreachable < 2.0,
            "seed {seed}: unreachable {unreachable} s"
        );
        assert!(dithers < 60, "seed {seed}: {dithers} dithers");
    }
}

#[test]
fn cavern_runs_replay_identically() {
    let content = Content::builtin_cavern();
    let party = party_at_level(ClassId::Striker, 2, AiProfile::Balanced);
    let (_, a) = run_headless(&content, &party, 11, 900.0);
    let (_, b) = run_headless(&content, &party, 11, 900.0);
    assert_eq!(a.log_hash, b.log_hash);
    assert_eq!(a.outcome, RunOutcome::Victory);
}

// ------------------------------------------ 0.5: The Cinderdeep Hollows ---

#[test]
fn actor_on_a_slanted_portal_edge_crosses_it() {
    // Regression (0.5, 10-player seed 15): a companion slid onto the slanted
    // edge shared by the Glass Passage landing and its ramp sat a rounding
    // error outside the portal, so every step slid along the edge instead of
    // crossing and it never arrived.
    let c = Content::builtin_cinderdeep();
    let nav = c.nav();
    let top = nav.find("Glass Passage (top)").unwrap();
    let goal = V3::new(-15.09, -4.0, 96.02);
    // Slid onto the edge: the nearest walkable point to a spot past it.
    let mut pos = nav.surfaces[top as usize].walk.clamp(V2::new(-27.9, 98.84));
    let mut s = top;
    for _ in 0..200 {
        let wp = nav.next_waypoint(pos, s, goal);
        let d = wp.point - pos;
        if d.len() < 0.05 {
            break;
        }
        let step = d.normalize_or_zero() * (7.0 * crate::sim::DT).min(d.len());
        (pos, s, _) = nav.try_move(pos, s, step);
    }
    assert!(pos.dist(goal.xz()) < 0.5, "stuck at {pos:?} on {s}");
}

#[test]
fn cinderdeep_layout_has_routes_shortcut_and_return_connection() {
    let c = Content::builtin_cinderdeep();
    let nav = c.nav();
    let id = |n: &str| nav.find(n).unwrap_or_else(|| panic!("no area {n}"));
    let linked = |a: u16, b: u16| nav.links.iter().any(|l| l.from == a && l.to == b);
    let walk_linked = |a: u16, b: u16| {
        nav.links
            .iter()
            .any(|l| l.from == a && l.to == b && l.kind == crate::nav::LinkKind::Walk)
    };
    // Natural shapes: most areas are non-rectangular polygons.
    let polys = nav
        .surfaces
        .iter()
        .filter(|s| !s.def.shape.is_axis_rect())
        .count();
    assert!(
        polys * 2 > nav.surfaces.len(),
        "{polys} of {}",
        nav.surfaces.len()
    );
    // Branch: the fork leads to both the low and the high road.
    let fork = id("Glowworm Fork");
    assert!(
        walk_linked(fork, id("Ashgrub Run (top)")) && walk_linked(fork, id("Ridge Ascent (foot)"))
    );
    // Shortcut: the ridge drops into the gallery, one way.
    let (ledge, east) = (id("Ridge Ledge"), id("Smelter East"));
    assert!(
        nav.links
            .iter()
            .any(|l| l.from == ledge && l.to == east && l.kind == crate::nav::LinkKind::Drop)
    );
    assert!(!linked(east, ledge));
    // Return connection: the Rim Tunnel joins the rim back to the gallery.
    assert!(walk_linked(id("Cistern Rim"), id("Rim Tunnel North")));
    assert!(walk_linked(id("Rim Tunnel"), id("Smelter West")));
    // The Kiln Span touches only its two ends (nothing below it connects).
    let span = id("Kiln Span");
    let ends: Vec<&str> = nav
        .links
        .iter()
        .filter(|l| l.from == span)
        .map(|l| nav.surfaces[l.to as usize].name())
        .collect();
    assert_eq!(ends, vec!["Smelter North", "Cistern Rim"], "{ends:?}");
    // Every area is reachable from the entrance; the final island too.
    let (_, start) = Content::place(&nav, c.dungeon.entrance, c.dungeon.entrance_y).unwrap();
    assert!(nav.reachable(start, id("Heart Island")));
    // You can see across the slag pit (its edges are lips, not walls) ...
    assert!(nav.line_of_sight(V3::new(1.0, -2.3, 88.0), V3::new(15.0, -3.0, 88.0)));
    // ... but not through the gallery's outer rock.
    assert!(!nav.line_of_sight(V3::new(-10.0, -2.3, 90.0), V3::new(-40.0, 0.5, 100.0)));
}

#[test]
fn five_person_party_clears_the_cinderdeep() {
    let c = Content::builtin_cinderdeep();
    for seed in 1..=2 {
        let (outcome, stuck, unreachable, _) = traverse_in(&c, 5, 5, seed);
        assert_eq!(outcome, RunOutcome::Victory, "seed {seed}");
        assert!(stuck < 5.0, "seed {seed}: stuck {stuck} s");
        assert!(
            unreachable < 2.0,
            "seed {seed}: unreachable {unreachable} s"
        );
    }
}

#[test]
fn ten_person_raid_clears_the_cinderdeep() {
    let c = Content::builtin_cinderdeep();
    let (outcome, stuck, unreachable, _) = traverse_in(&c, 5, 10, 1);
    assert_eq!(outcome, RunOutcome::Victory);
    assert!(stuck < 10.0, "stuck {stuck} s");
    assert!(unreachable < 4.0, "unreachable {unreachable} s");
}

/// Diagnostic replay (not a regular test): `DIAG_DUNGEON=<file stem>
/// DIAG_SEED=n DIAG_LEVEL=n DIAG_TIME=s cargo test --release -p dungeon_sim
/// diag_replay -- --ignored --nocapture` prints everyone's state at that time.
#[test]
#[ignore]
fn diag_replay() {
    let var = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
    let path = format!(
        "../../assets/content/dungeons/{}.toml",
        var("DIAG_DUNGEON", "06_cinderdeep_hollows")
    );
    let content = Content::load(&path).unwrap();
    let seed: u64 = var("DIAG_SEED", "1").parse().unwrap();
    let level: u32 = var("DIAG_LEVEL", "5").parse().unwrap();
    let until: f32 = var("DIAG_TIME", "300").parse().unwrap();
    let size: usize = var("DIAG_SIZE", "5").parse().unwrap();
    let party = if size == 5 {
        party_at_level(ClassId::Striker, level, AiProfile::Balanced)
    } else {
        raid_party(size, level, AiProfile::Balanced)
    };
    let options = crate::sim::RunOptions {
        autopilot: true,
        ..Default::default()
    };
    let mut sim = Sim::with_options(&content, &party, seed, options);
    while sim.time < until && sim.outcome == RunOutcome::InProgress {
        sim.step();
    }
    if let Ok(w) = std::env::var("DIAG_WATCH") {
        let w: usize = w.parse().unwrap();
        let s = sim.actors[w].surface as usize;
        eprintln!(
            "surf {} walk {:?}",
            sim.nav.surfaces[s].name(),
            sim.nav.surfaces[s].walk.points()
        );
        for k in 0..sim.nav.links.len() {
            if sim.nav.links[k].from as usize != s {
                continue;
            }
            let l = &sim.nav.links[k];
            eprintln!(
                "link {k} -> {} {:?} portal {:?} inner {:?}",
                sim.nav.surfaces[l.to as usize].name(),
                l.kind,
                l.portal.points(),
                l.inner.points()
            );
            eprintln!(
                "   to walk {:?}",
                sim.nav.surfaces[l.to as usize].walk.points()
            );
        }
        for _ in 0..std::env::var("DIAG_N").map_or(12, |v| v.parse().unwrap()) {
            let a = &sim.actors[w];
            let wp = a
                .move_goal
                .map(|g| sim.nav.next_waypoint(a.pos, a.surface, g));
            eprintln!(
                "  watch ({:.3},{:.3}) s{} wp {:?} goal {:?} recover {:.2} commit {:.2} commit_goal ({:.2},{:.2}) prev {:?}",
                a.pos.x,
                a.pos.z,
                a.surface,
                wp.map(|w| (w.point.x, w.point.z, w.link)),
                a.move_goal.map(|g| (g.x, g.z)),
                a.nav.recover,
                a.nav.commit,
                a.nav.commit_goal.x,
                a.nav.commit_goal.z,
                a.nav.prev_goal.map(|g| (g.x, g.z))
            );
            sim.step();
        }
    }
    eprintln!(
        "t {:.1} outcome {:?} leader {}",
        sim.time, sim.outcome, sim.leader
    );
    for (i, e) in sim.encounters.iter().enumerate() {
        eprintln!("enc {i} {} cleared {}", e.id, e.cleared);
    }
    for (id, a) in sim.actors.iter().enumerate() {
        if !a.alive {
            continue;
        }
        if a.is_enemy() && a.mode() == Some(EnemyMode::Idle) && std::env::var("DIAG_ALL").is_err() {
            continue;
        }
        let wp = a
            .move_goal
            .map(|g| sim.nav.next_waypoint(a.pos, a.surface, g));
        eprintln!(
            "   cast {:?} incap {}",
            a.cast.as_ref().map(|c| (c.name.clone(), c.remaining)),
            a.is_incapacitated()
        );
        eprintln!(
            "{id} {} ({:.1},{:.1}) y {:.1} '{}' goal {:?} wp {:?} mode {:?} tgt {:?} hp {:.0} nav {:?}",
            a.name,
            a.pos.x,
            a.pos.z,
            a.y,
            sim.nav.surfaces[a.surface as usize].name(),
            a.move_goal.map(|g| (g.x, g.y, g.z)),
            wp.map(|w| (w.point.x, w.point.z, w.reachable)),
            a.mode(),
            a.target,
            a.hp,
            (
                a.nav.unreachable,
                a.nav.stalled,
                a.nav.commit,
                a.nav.dithers
            )
        );
    }
}

#[test]
fn props_validate_paths_and_stay_off_walkable_floor() {
    use crate::content::{DecorKind, PropConfig};
    let base = Content::builtin();
    let prop = |model: &str, pos: V2| PropConfig {
        model: model.into(),
        pos,
        y: 0.0,
        yaw: 0.0,
        scale: 1.0,
        radius: 0.8,
        height: 2.0,
        fallback: DecorKind::Pillar,
    };
    // Outside every area of the Sunken Vault: fine.
    let mut ok = base.clone();
    ok.dungeon.props.push(prop(
        "models/props/forge_brazier.glb",
        V2::new(-30.0, -30.0),
    ));
    assert!(ok.validate("ok").is_ok());
    let text = ok.to_toml().expect("serialises");
    assert!(Content::from_toml_str(&text, "round trip").is_ok());
    for (model, pos, expect) in [
        (
            "../secrets/x.glb",
            V2::new(-30.0, -30.0),
            "must not leave assets/",
        ),
        ("C:/x.glb", V2::new(-30.0, -30.0), "must be relative"),
        ("models/x.fbx", V2::new(-30.0, -30.0), ".glb or .gltf"),
        (
            "models/props/forge_brazier.glb",
            V2::new(0.0, 0.0),
            "must stay off walkable floor",
        ),
    ] {
        let mut c = base.clone();
        c.dungeon.props.push(prop(model, pos));
        let err = c.validate("bad").expect_err(model).to_string();
        assert!(err.contains(expect), "{model}: {err}");
    }
}

/// A small L-shaped test map: hall A, a corridor east, room C around the
/// corner (out of sight of most of A). `extra` is appended (encounters etc.).
fn l_map(extra: &str) -> String {
    test_map(L_AREAS, extra)
}

const L_AREAS: &str = r#"[[dungeon.areas]]
name = "Hall A"
min = [0.0, 0.0]
max = [20.0, 40.0]

[[dungeon.areas]]
name = "Corridor B"
min = [18.0, 8.0]
max = [32.0, 12.0]

[[dungeon.areas]]
name = "Room C"
min = [30.0, 8.0]
max = [42.0, 30.0]
"#;

/// A hall with a 4 x 2 m pillar (a hole in the floor, walled in) just north
/// of where the boss stands.
const PILLAR_AREAS: &str = r#"[[dungeon.areas]]
name = "South"
min = [0.0, 0.0]
max = [30.0, 12.0]

[[dungeon.areas]]
name = "North"
min = [0.0, 14.0]
max = [30.0, 40.0]

[[dungeon.areas]]
name = "West"
min = [0.0, 9.0]
max = [14.0, 17.0]

[[dungeon.areas]]
name = "East"
min = [18.0, 9.0]
max = [30.0, 17.0]
"#;

fn test_map(areas: &str, extra: &str) -> String {
    format!(
        r#"[dungeon]
id = "l_map"
name = "L Map"
hp_multiplier = 1.0
entrance = [2.0, 2.0]
leash_radius = 80.0

{areas}
[templates.walker]
name = "Walker"
rank = "trash"
hp = 500.0
melee_damage = 10.0
attack_interval = 2.0
aggro_radius = 6.0
speed = 6.0
radius = 0.5

[templates.seer]
name = "Seer"
rank = "boss"
hp = 50000.0
melee_damage = 1.0
attack_interval = 5.0
aggro_radius = 6.0
speed = 0.01
radius = 1.0

[[templates.seer.abilities]]
name = "Gaze"
kind = "pulse"
interval = 6.0
initial = 1.0
cast_time = 3.0
damage = 100.0
los_blockable = true

{extra}

[[loot]]
id = "x"
name = "X"
slot = "trinket"
weight = 1

[rewards]
marks_per_clear = 1
"#
    )
}

#[test]
fn patrols_walk_their_route_and_are_pulled_where_they_are() {
    let text = l_map(
        r#"[[encounters]]
id = "patrol"
mobs = [{ template = "walker", pos = [10.0, 35.0], patrol = [[10.0, 15.0]] }]

[[encounters]]
id = "boss"
mobs = [{ template = "seer", pos = [38.0, 26.0] }]"#,
    );
    let content = Content::from_toml_str(&text, "l_map").expect("valid");
    let party = default_party(ClassId::Warden);
    let mut sim = Sim::new(&content, &party, 1);
    let walker = sim.encounters[0].mobs[0];
    // Hold the party still at the entrance (player-controlled, no input).
    for &p in &sim.party.clone() {
        sim.actors[p].controller = crate::sim::Controller::Player;
    }
    let start = sim.actors[walker].pos;
    let mut min_z = f32::MAX;
    for _ in 0..(12.0 / crate::sim::DT) as usize {
        sim.step();
        min_z = min_z.min(sim.actors[walker].pos.z);
    }
    assert!(min_z < 16.0, "walked to the far point (min z {min_z})");
    assert_eq!(sim.actors[walker].mode(), Some(EnemyMode::Idle));
    // Walk back toward the spawn after turning.
    for _ in 0..(3.0 / crate::sim::DT) as usize {
        sim.step();
    }
    assert!(sim.actors[walker].pos.z > min_z + 1.0, "turned around");
    assert_ne!(sim.actors[walker].pos, start);
    // Stand in its path: it pulls where it is, and that becomes its home.
    let p = sim.party[0];
    let here = sim.actors[walker].pos;
    sim.actors[p].pos = V2::new(here.x + 3.0, here.z);
    for _ in 0..4 {
        sim.step();
    }
    assert_eq!(sim.actors[walker].mode(), Some(EnemyMode::Engaged));
    let home = sim.actors[walker].enemy.as_ref().unwrap().home;
    assert!(
        home.xz().dist(here) < 2.0,
        "home {home:?} vs caught at {here:?}"
    );
}

#[test]
fn los_pulses_spare_hidden_party_members() {
    let text = l_map(
        r#"[[encounters]]
id = "boss"
mobs = [{ template = "seer", pos = [10.0, 10.0] }]"#,
    );
    let content = Content::from_toml_str(&text, "l_map").expect("valid");
    let party = default_party(ClassId::Warden);
    let mut sim = Sim::new(&content, &party, 1);
    for &p in &sim.party.clone() {
        sim.actors[p].controller = crate::sim::Controller::Player;
    }
    let (seen, hidden) = (sim.party[0], sim.party[1]);
    // `seen` stands in the hall; `hidden` around the corner in room C.
    sim.actors[seen].pos = V2::new(4.0, 30.0);
    let c_surface = sim.nav.find("Room C").unwrap();
    sim.actors[hidden].pos = V2::new(38.0, 26.0);
    sim.actors[hidden].surface = c_surface;
    for &p in &sim.party.clone()[2..] {
        sim.actors[p].pos = V2::new(40.0, 28.0);
        sim.actors[p].surface = c_surface;
    }
    assert!(!sim.has_los(sim.encounters[0].mobs[0], hidden));
    sim.engage(0, seen);
    for _ in 0..(6.0 / crate::sim::DT) as usize {
        sim.step();
    }
    let hits = |dst: ActorId| {
        sim.events
            .iter()
            .filter(|e| matches!(&e.kind, EventKind::Damage { what, dst: d, .. } if &**what == "Gaze" && *d == dst))
            .count()
    };
    assert!(hits(seen) >= 1, "the visible member was hit");
    assert_eq!(hits(hidden), 0, "the hidden member was spared");
}

#[test]
fn ai_companions_take_cover_from_los_pulses() {
    // Companions under AI fighting the seer: most pulses should miss
    // non-tanks because they step around the corner while it is cast.
    let text = test_map(
        PILLAR_AREAS,
        r#"[[encounters]]
id = "boss"
mobs = [{ template = "seer", pos = [16.0, 6.0] }]"#,
    );
    let content = Content::from_toml_str(&text, "pillar_map").expect("valid");
    let party = party_at_level(ClassId::Warden, 5, AiProfile::Balanced);
    let mut sim = Sim::new(&content, &party, 3);
    let tank = sim.party[0];
    sim.engage(0, tank);
    for _ in 0..(40.0 / crate::sim::DT) as usize {
        sim.step();
    }
    let casts = sim
        .events
        .iter()
        .filter(|e| matches!(&e.kind, EventKind::CastStart { what, .. } if &**what == "Gaze"))
        .count();
    let hits = sim
        .events
        .iter()
        .filter(|e| matches!(&e.kind, EventKind::Damage { what, .. } if &**what == "Gaze"))
        .count();
    assert!(casts >= 6, "{casts} casts");
    let possible = casts * sim.party.len();
    assert!(
        hits * 4 < possible,
        "{hits} hits out of {possible} possible: companions did not take cover"
    );
}

#[test]
fn bad_patrols_and_misused_los_flags_are_rejected() {
    let off_map = l_map(
        r#"[[encounters]]
id = "boss"
mobs = [{ template = "seer", pos = [10.0, 10.0], patrol = [[100.0, 100.0]] }]"#,
    );
    let err = Content::from_toml_str(&off_map, "l")
        .unwrap_err()
        .to_string();
    assert!(err.contains("patrol[0]"), "{err}");
    let misused = l_map(
        r#"[[encounters]]
id = "boss"
mobs = [{ template = "seer", pos = [10.0, 10.0] }]"#,
    )
    .replace("kind = \"pulse\"", "kind = \"bolt\"");
    let err = Content::from_toml_str(&misused, "l")
        .unwrap_err()
        .to_string();
    assert!(err.contains("los_blockable only applies"), "{err}");
}
