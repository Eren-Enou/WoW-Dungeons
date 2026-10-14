//! Rule-level tests for combat, threat, AI, encounters and determinism.

use crate::ai;
use crate::class::{AbilityId, ClassId};
use crate::content::{BUILTIN_CONTENT, Content};
use crate::events::{CastError, EventKind};
use crate::math::V2;
use crate::runner::{default_party, run_headless};
use crate::sim::{
    ActorId, COMBAT_LINGER, Controller, DT, EnemyMode, HEAL_THREAT_FACTOR, RunOutcome, Sim,
    Telegraph,
};

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
}

// ------------------------------------------------------------ health ---

#[test]
fn damage_never_takes_health_below_zero_and_kills() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    let applied = sim.deal_damage(PLAYER, mob, 100_000.0, "Test", 1.0, false);
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
    // Dead actors take no further damage.
    assert_eq!(sim.deal_damage(PLAYER, mob, 50.0, "Test", 1.0, false), 0.0);
}

#[test]
fn healing_is_capped_at_max_hp_and_overheal_is_recorded() {
    let mut sim = passive_sim();
    let max = sim.actors[TANK].max_hp;
    sim.actors[TANK].hp = max - 100.0;
    let eff = sim.heal(HEALER, TANK, 420.0, "Mend");
    assert_eq!(eff, 100.0);
    assert_eq!(sim.actors[TANK].hp, max);
    assert_eq!(sim.metrics.actors[HEALER].overhealing, 320.0);
}

// ------------------------------------------------------------ threat ---

#[test]
fn damage_generates_threat_with_class_multiplier() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    sim.engage(0, TANK);
    let dealt = sim.deal_damage(TANK, mob, 100.0, "Test", 2.0, false);
    // Warden threat multiplier is 1.5.
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
    assert_eq!(
        sim.select_enemy_target(mob),
        Some(TANK),
        "109% in melee: no switch"
    );
    sim.add_threat(mob, MELEE, 20.0);
    assert_eq!(
        sim.select_enemy_target(mob),
        Some(MELEE),
        "111% in melee: switch"
    );

    // A ranged attacker out of melee reach needs 130%.
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    place_near(&mut sim, TANK, mob, 1.0);
    place_near(&mut sim, RANGED, mob, 20.0);
    sim.engage(0, TANK);
    sim.add_threat(mob, TANK, 1000.0);
    sim.actors[mob].target = sim.select_enemy_target(mob);
    sim.add_threat(mob, RANGED, 1250.0);
    assert_eq!(
        sim.select_enemy_target(mob),
        Some(TANK),
        "125% at range: no switch"
    );
    sim.add_threat(mob, RANGED, 100.0);
    assert_eq!(
        sim.select_enemy_target(mob),
        Some(RANGED),
        "135% at range: switch"
    );
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

    let slot = sim.actors[TANK].ability_slot(AbilityId::Challenge).unwrap();
    sim.try_cast(TANK, slot, Some(mob)).unwrap();
    assert!(sim.threat(mob, TANK) >= 2000.0);
    assert_eq!(sim.actors[mob].target, Some(TANK));
    assert_eq!(sim.select_enemy_target(mob), Some(TANK));
    // Even if the DPS gains a little more threat during the taunt, the enemy stays.
    sim.add_threat(mob, MELEE, 500.0);
    assert_eq!(sim.select_enemy_target(mob), Some(TANK));
}

#[test]
fn healing_threat_is_split_across_engaged_enemies() {
    let mut sim = passive_sim();
    for &m in &sim.encounters[0].mobs.clone() {
        place_near(&mut sim, TANK, m, 1.0);
    }
    sim.engage(0, TANK);
    sim.actors[TANK].hp -= 600.0;
    sim.actors[TANK].last_combat = sim.time;
    let eff = sim.heal(HEALER, TANK, 420.0, "Mend");
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
    sim.heal(HEALER, MELEE, 200.0, "Mend");
    assert_eq!(sim.threat(first_skulker(&sim), HEALER), 0.0);
}

#[test]
fn threat_drop_halves_threat_everywhere() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    sim.engage(0, PLAYER);
    sim.add_threat(mob, PLAYER, 800.0);
    let slot = sim.actors[PLAYER].ability_slot(AbilityId::Evade).unwrap();
    sim.try_cast(PLAYER, slot, None).unwrap();
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
    let rend = sim.actors[PLAYER]
        .ability_slot(AbilityId::RendStrike)
        .unwrap();
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

    // Friendly spells fall back to self when a hostile is targeted.
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    sim.actors[HEALER].hp -= 300.0;
    let renew = sim.actors[HEALER].ability_slot(AbilityId::Renew).unwrap();
    sim.try_cast(HEALER, renew, Some(mob)).unwrap();
    assert!(sim.actors[HEALER].has_hot_from(HEALER));

    // Dead targets are invalid.
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    place_near(&mut sim, PLAYER, mob, 1.0);
    sim.kill(mob);
    assert_eq!(
        sim.try_cast(PLAYER, rend, Some(mob)),
        Err(CastError::InvalidTarget)
    );
}

#[test]
fn cast_requires_mana() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    place_near(&mut sim, RANGED, mob, 10.0);
    sim.actors[RANGED].mana = 0.0;
    let bolt = sim.actors[RANGED].ability_slot(AbilityId::ArcBolt).unwrap();
    assert_eq!(
        sim.try_cast(RANGED, bolt, Some(mob)),
        Err(CastError::NotEnoughMana)
    );
}

// --------------------------------------------------------- cooldowns ---

#[test]
fn cooldowns_and_global_cooldown() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    sim.actors[mob].max_hp = 100_000.0;
    sim.actors[mob].hp = 100_000.0;
    place_near(&mut sim, PLAYER, mob, 1.0);
    let rend = sim.actors[PLAYER]
        .ability_slot(AbilityId::RendStrike)
        .unwrap();
    let evade = sim.actors[PLAYER].ability_slot(AbilityId::Evade).unwrap();
    sim.try_cast(PLAYER, rend, Some(mob)).unwrap();
    assert_eq!(
        sim.try_cast(PLAYER, rend, Some(mob)),
        Err(CastError::NotReady)
    );
    assert_eq!(
        sim.try_cast(PLAYER, evade, None),
        Err(CastError::OnGlobalCooldown)
    );
    steps(&mut sim, 1.05);
    assert_eq!(
        sim.try_cast(PLAYER, evade, None),
        Ok(()),
        "GCD over after 1s"
    );
    assert_eq!(
        sim.try_cast(PLAYER, rend, Some(mob)),
        Err(CastError::NotReady),
        "own cooldown is checked first"
    );
    steps(&mut sim, 1.05);
    assert_eq!(
        sim.try_cast(PLAYER, rend, Some(mob)),
        Err(CastError::NotReady),
        "4s cooldown"
    );
    steps(&mut sim, 2.0);
    assert_eq!(sim.try_cast(PLAYER, rend, Some(mob)), Ok(()));
}

#[test]
fn cast_time_spells_complete_after_cast_time() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    sim.actors[mob].max_hp = 100_000.0;
    sim.actors[mob].hp = 100_000.0;
    place_near(&mut sim, RANGED, mob, 15.0);
    let bolt = sim.actors[RANGED].ability_slot(AbilityId::ArcBolt).unwrap();
    sim.try_cast(RANGED, bolt, Some(mob)).unwrap();
    steps(&mut sim, 1.9);
    assert_eq!(sim.actors[mob].hp, 100_000.0, "not landed before 2s");
    steps(&mut sim, 0.15);
    assert!(sim.actors[mob].hp < 100_000.0, "landed after 2s");
}

#[test]
fn moving_player_interrupts_cast() {
    let mut sim = passive_sim();
    let mob = first_skulker(&sim);
    place_near(&mut sim, RANGED, mob, 15.0);
    sim.player = Some(RANGED);
    let bolt = sim.actors[RANGED].ability_slot(AbilityId::ArcBolt).unwrap();
    sim.try_cast(RANGED, bolt, Some(mob)).unwrap();
    sim.set_player_move(V2::new(1.0, 0.0));
    sim.step();
    assert!(sim.actors[RANGED].cast.is_none());
    assert_eq!(
        count(&sim, |k| matches!(k, EventKind::Interrupted { .. })),
        1
    );
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
    let d = ai::party_decision(&sim, TANK);
    let challenge = sim.actors[TANK].ability_slot(AbilityId::Challenge).unwrap();
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
    sim.actors[TANK].hp = sim.actors[TANK].max_hp * 0.7;
    sim.actors[MELEE].hp = sim.actors[MELEE].max_hp * 0.3;
    let d = ai::party_decision(&sim, HEALER);
    let mend = sim.actors[HEALER].ability_slot(AbilityId::Mend).unwrap();
    assert_eq!(d.cast, Some((mend, MELEE)));
}

#[test]
fn ai_moves_out_of_noticed_telegraph() {
    let mut sim = new_sim();
    let center = sim.actors[RANGED].pos;
    sim.telegraphs.push(Telegraph {
        id: 1,
        source: sim.boss,
        center,
        radius: 5.0,
        remaining: 1.0,
        total: 2.5,
        damage: 100.0,
    });
    let d = ai::party_decision(&sim, RANGED);
    let goal = d.move_goal.expect("dodges");
    assert!(goal.dist(center) > 5.0 + sim.actors[RANGED].radius);
}

#[test]
fn ai_has_reaction_time_for_new_telegraphs() {
    let mut sim = new_sim();
    let center = sim.actors[RANGED].pos;
    sim.telegraphs.push(Telegraph {
        id: 1,
        source: sim.boss,
        center,
        radius: 5.0,
        remaining: 2.4,
        total: 2.5,
        damage: 100.0,
    });
    assert!(ai::escape_point(&sim, RANGED).is_none());
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

// ----------------------------------------------------- encounters ------

fn pull_boss(sim: &mut Sim) -> ActorId {
    let boss = sim.boss;
    for p in sim.party.clone() {
        place_near(sim, p, boss, 3.0 + p as f32);
    }
    let enc = sim.content.boss_encounter();
    sim.engage(enc, TANK);
    sim.add_threat(boss, TANK, 5000.0);
    boss
}

#[test]
fn boss_phase_two_triggers_once_at_threshold() {
    let mut sim = passive_sim();
    let boss = pull_boss(&mut sim);
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
    // The telegraphed Cinder Ring appears ~2s after the phase change.
    steps(&mut sim, 1.2);
    assert_eq!(
        count(&sim, |k| matches!(k, EventKind::TelegraphStart { .. })),
        1
    );
    assert_eq!(sim.telegraphs.len(), 1);
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
        center,
        radius: 5.0,
        remaining: 0.04,
        total: 2.5,
        damage: 300.0,
    });
    let (h0, r0) = (sim.actors[HEALER].hp, sim.actors[RANGED].hp);
    sim.step();
    assert!(sim.actors[HEALER].hp < h0);
    assert_eq!(sim.actors[RANGED].hp, r0);
}

#[test]
fn ring_interval_from_config_controls_timing() {
    let text = BUILTIN_CONTENT.replace("ring_interval = 9.0", "ring_interval = 4.0");
    let content = Content::from_toml_str(&text, "test").unwrap();
    let mut sim = Sim::new(&content, &default_party(ClassId::Striker), 3);
    for p in sim.party.clone() {
        sim.actors[p].controller = Controller::Player;
        sim.actors[p].max_hp = 1e9;
        sim.actors[p].hp = 1e9;
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
fn boss_resets_after_wipe_and_phase_retriggers_on_next_pull() {
    let mut sim = passive_sim();
    let boss = pull_boss(&mut sim);
    let max = sim.actors[boss].max_hp;
    let home = sim.actors[boss].enemy.as_ref().unwrap().home;
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
    assert!(b.enemy.as_ref().unwrap().threat.is_empty());

    // Revive the tank and pull again: phase 2 triggers again, once.
    let t = &mut sim.actors[TANK];
    t.alive = true;
    t.hp = t.max_hp;
    let enc = sim.content.boss_encounter();
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
    // Pull the reliquary pack and run back to the entrance.
    let enc = 2;
    let mob = sim.encounters[enc].mobs[0];
    place_near(&mut sim, TANK, mob, 2.0);
    sim.engage(enc, TANK);
    sim.actors[TANK].max_hp = 1e9;
    sim.actors[TANK].hp = 1e9;
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
    // Other packs are untouched.
    assert_eq!(sim.actors[sim.boss].mode(), Some(EnemyMode::Idle));
}

// ------------------------------------------------- headless / config ---

#[test]
fn headless_runs_are_reproducible_for_a_seed() {
    let content = Content::builtin();
    let party = default_party(ClassId::Striker);
    let (_, a) = run_headless(&content, &party, 11, 1200.0);
    let (_, b) = run_headless(&content, &party, 11, 1200.0);
    assert_eq!(a, b);
    assert!(a.event_count > 100);
    assert!(a.party_damage > 0.0);
    assert!(a.duration > 0.0);
    let (_, c) = run_headless(&content, &party, 12, 1200.0);
    assert_ne!(a.log_hash, c.log_hash, "different seeds diverge");
}

#[test]
fn default_content_is_clearable_by_ai_party() {
    let content = Content::builtin();
    for class in ClassId::ALL {
        let party = default_party(class);
        for seed in 1..=3 {
            let (_, s) = run_headless(&content, &party, seed, 1200.0);
            assert_eq!(
                s.outcome,
                RunOutcome::Victory,
                "{class:?} seed {seed}: {s:?}"
            );
            assert_eq!(s.encounters_cleared, 4);
            assert!(s.phase2_at.is_some());
        }
    }
}

#[test]
fn hp_multiplier_changes_enemy_health_and_run_length() {
    let text = BUILTIN_CONTENT.replace("hp_multiplier = 1.0", "hp_multiplier = 2.0");
    let doubled = Content::from_toml_str(&text, "test").unwrap();
    let normal = Content::builtin();
    let party = default_party(ClassId::Striker);
    let s1 = Sim::new(&normal, &party, 1);
    let s2 = Sim::new(&doubled, &party, 1);
    assert_eq!(s2.actors[s2.boss].max_hp, 2.0 * s1.actors[s1.boss].max_hp);
    let (_, r1) = run_headless(&normal, &party, 1, 1200.0);
    let (_, r2) = run_headless(&doubled, &party, 1, 1200.0);
    assert!(
        r2.duration > r1.duration,
        "{} vs {}",
        r2.duration,
        r1.duration
    );
}

#[test]
fn absurd_hp_multiplier_leads_to_enrage_wipe_not_a_hang() {
    let text = BUILTIN_CONTENT.replace("hp_multiplier = 1.0", "hp_multiplier = 10.0");
    let content = Content::from_toml_str(&text, "test").unwrap();
    let (_, s) = run_headless(&content, &default_party(ClassId::Striker), 1, 1200.0);
    assert_ne!(s.outcome, RunOutcome::Victory);
    assert_ne!(s.outcome, RunOutcome::InProgress);
}
