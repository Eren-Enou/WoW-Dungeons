//! Lightweight procedural combat effects driven by sim events and sim state:
//! projectiles for spells, flashes for melee, rings for AoE/taunts, sparkles
//! for heals, and a glow on the selected target.
//!
//! 0.9.3 (ability impact): your abilities have their own look (gold strikes,
//! bigger spells, rings at the ability's real radius), targets you hit react,
//! interrupts burst at the stopped caster, and shields, defensives and crowd
//! control stay visible while their aura lasts. Presentation only: nothing
//! here feeds back into the simulation.

use crate::common::AppState;
use crate::dungeon::ActiveRun;
use bevy::prelude::*;
use dungeon_sim::class::Effect;
use dungeon_sim::sim::{ActorId, AuraKind};
use dungeon_sim::stats::{HitOutcome, School};
use dungeon_sim::{Event, EventKind, Sim};

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Dungeon), setup_fx_assets)
            .add_systems(
                Update,
                (
                    spawn_fx,
                    animate_fx,
                    target_marker,
                    aura_visuals,
                    hit_reaction,
                )
                    .chain()
                    .after(crate::dungeon::sync_actors)
                    .run_if(in_state(AppState::Dungeon))
                    .run_if(resource_exists::<ActiveRun>),
            );
    }
}

#[derive(Resource)]
struct FxAssets {
    sphere: Handle<Mesh>,
    ring: Handle<Mesh>,
    cube: Handle<Mesh>,
    shield: Handle<StandardMaterial>,
    shell: Handle<StandardMaterial>,
    cc: Handle<StandardMaterial>,
    opening: Handle<StandardMaterial>,
}

#[derive(Component)]
struct TargetMarker;

#[derive(Clone, Copy)]
enum Motion {
    /// Travel from `from` to `to`.
    Travel { from: Vec3, to: Vec3 },
    /// Scale from `from` to `to` at a fixed position.
    Grow { at: Vec3, from: f32, to: f32 },
    /// Rise upward.
    Rise { at: Vec3, height: f32 },
}

#[derive(Component)]
struct Fx {
    motion: Motion,
    age: f32,
    life: f32,
    material: Handle<StandardMaterial>,
    base_alpha: f32,
}

/// What one event looks like in the world (0.9.3). `mine`: caused by the
/// player, drawn larger and brighter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Impact {
    /// A melee hit that landed.
    Strike {
        src: ActorId,
        dst: ActorId,
        crit: bool,
        mine: bool,
        /// Lab B (0.10): an Exploit, drawn as a heavy hit.
        heavy: bool,
    },
    /// A ranged spell or wand shot.
    Bolt {
        src: ActorId,
        dst: ActorId,
        school: School,
        wand: bool,
        crit: bool,
        mine: bool,
    },
    /// One area attack, drawn once per caster and ability, at its radius.
    Area {
        src: ActorId,
        radius: f32,
        mine: bool,
    },
    Heal {
        dst: ActorId,
        crit: bool,
        mine: bool,
    },
    /// A cast was stopped: burst at the caster that was interrupted.
    Interrupt {
        dst: ActorId,
        mine: bool,
    },
    Taunt {
        dst: ActorId,
    },
    /// A self-buff (defensive) was applied.
    SelfBuff {
        dst: ActorId,
    },
    ThreatDrop {
        src: ActorId,
    },
    /// A telegraph went off or a boss changed phase.
    Boom,
    /// The player's hit: the target reacts (strength 1, crit 2).
    React {
        dst: ActorId,
        strength: f32,
    },
    /// A heavy hit on the player (a crit, or 8% of health or more).
    Hurt {
        dst: ActorId,
    },
}

/// Enemy area attacks drawn as one ring (their size isn't in the event).
fn is_enemy_area(what: &str) -> bool {
    matches!(
        what,
        "Rockfall Cleave" | "Rending Mandibles" | "Root Lash" | "Hollow Sweep"
    )
}

/// The definition's effect of a party member's ability, by its label.
fn effect_of(sim: &Sim, src: ActorId, what: &str) -> Option<Effect> {
    let a = &sim.actors[src];
    a.ability_labels
        .iter()
        .position(|l| &**l == what)
        .and_then(|i| a.abilities.get(i))
        .map(|id| id.def().effect)
}

/// Classify a batch of new events into world effects. Pure, so it can be
/// tested on real simulation runs.
pub fn impacts(sim: &Sim, events: &[Event]) -> Vec<Impact> {
    let player = sim.player;
    let raid = sim.party.len() > 5;
    let mine = |id: ActorId| Some(id) == player;
    let mut out = Vec::new();
    let mut areas: Vec<(ActorId, &str)> = Vec::new();
    for e in events {
        match &e.kind {
            EventKind::Damage {
                src,
                dst,
                what,
                amount,
                absorbed,
                outcome,
                school,
            } => {
                let crit = *outcome == HitOutcome::Crit;
                let effect = effect_of(sim, *src, what);
                // Over-time ticks have no strike of their own.
                if matches!(effect, Some(Effect::Dot { .. })) {
                    continue;
                }
                let heavy = matches!(effect, Some(Effect::ExploitOpening { .. }));
                let area = match effect {
                    Some(Effect::AoeDamage { radius, .. })
                    | Some(Effect::EdgeWhirl { radius, .. }) => Some(radius),
                    _ if is_enemy_area(what) => Some(6.0),
                    _ => None,
                };
                if let Some(radius) = area {
                    if !areas.contains(&(*src, &**what)) {
                        areas.push((*src, &**what));
                        out.push(Impact::Area {
                            src: *src,
                            radius,
                            mine: mine(*src),
                        });
                    }
                } else {
                    let s = &sim.actors[*src];
                    let d = &sim.actors[*dst];
                    let ranged = s.pos.dist(d.pos) > s.radius + d.radius + 3.0;
                    let wand = &**what == "Wand";
                    if ranged && (school.is_magic() || wand) {
                        out.push(Impact::Bolt {
                            src: *src,
                            dst: *dst,
                            school: *school,
                            wand,
                            crit,
                            mine: mine(*src),
                        });
                    } else if !ranged && !outcome.avoided() {
                        // Raids: companions' ordinary swings are not drawn.
                        let skip = raid && !mine(*src) && !crit && !sim.actors[*src].is_enemy();
                        if !skip {
                            out.push(Impact::Strike {
                                src: *src,
                                dst: *dst,
                                crit,
                                mine: mine(*src),
                                heavy,
                            });
                        }
                    }
                }
                if mine(*src) && !mine(*dst) && (*amount > 0.0 || *absorbed > 0.0) {
                    out.push(Impact::React {
                        dst: *dst,
                        strength: if crit || heavy { 2.0 } else { 1.0 },
                    });
                }
                if mine(*dst) && (crit || *amount >= 0.08 * sim.actors[*dst].max_hp) {
                    out.push(Impact::Hurt { dst: *dst });
                }
            }
            EventKind::Heal {
                src,
                dst,
                what,
                amount,
                crit,
                ..
            } => {
                // Heal-over-time ticks: only yours, and only when they heal.
                let tick = matches!(effect_of(sim, *src, what), Some(Effect::Hot { .. }));
                if tick && (!mine(*src) || *amount < 1.0) {
                    continue;
                }
                if raid && !mine(*src) && !mine(*dst) && !*crit {
                    continue;
                }
                out.push(Impact::Heal {
                    dst: *dst,
                    crit: *crit,
                    mine: mine(*src),
                });
            }
            EventKind::Interrupt { src, dst, .. } => out.push(Impact::Interrupt {
                dst: *dst,
                mine: mine(*src),
            }),
            EventKind::Taunt { dst, .. } => out.push(Impact::Taunt { dst: *dst }),
            EventKind::AuraApplied { src, dst, .. } if src == dst => {
                out.push(Impact::SelfBuff { dst: *dst })
            }
            EventKind::ThreatDrop { src } => out.push(Impact::ThreatDrop { src: *src }),
            EventKind::TelegraphHit { .. } | EventKind::PhaseChange { .. } => {
                out.push(Impact::Boom)
            }
            _ => {}
        }
    }
    out
}

/// Effect colour by damage school (physical uses an off-white).
fn school_color(school: School, wand: bool) -> Color {
    if wand {
        return Color::srgb(0.7, 0.4, 1.0);
    }
    match school {
        School::Physical => Color::srgb(0.9, 0.85, 0.75),
        School::Holy => Color::srgb(1.0, 0.95, 0.5),
        School::Fire => Color::srgb(1.0, 0.45, 0.1),
        School::Frost => Color::srgb(0.5, 0.8, 1.0),
        School::Shadow => Color::srgb(0.45, 0.15, 0.65),
        School::Arcane => Color::srgb(0.75, 0.45, 1.0),
        School::Nature => Color::srgb(0.4, 0.9, 0.3),
    }
}

fn fx_material(color: Color, glow: f32) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        emissive: color.to_linear() * glow,
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    }
}

fn setup_fx_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let ring = meshes.add(Torus::new(0.9, 1.0));
    commands.insert_resource(FxAssets {
        sphere: meshes.add(Sphere::new(0.25)),
        ring: ring.clone(),
        cube: meshes.add(Cuboid::new(0.5, 0.5, 0.5)),
        shield: materials.add(fx_material(Color::srgba(0.45, 0.75, 1.0, 0.22), 0.6)),
        shell: materials.add(fx_material(Color::srgba(1.0, 0.8, 0.35, 0.14), 0.5)),
        cc: materials.add(fx_material(Color::srgba(0.75, 0.45, 1.0, 0.85), 1.5)),
        opening: materials.add(fx_material(Color::srgba(1.0, 0.82, 0.15, 0.95), 2.0)),
    });
    commands.insert_resource(HitReact::default());
    commands.insert_resource(AuraFx::default());
    commands.spawn((
        Mesh3d(ring),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.2, 0.15, 0.8),
            emissive: LinearRgba::rgb(2.0, 0.3, 0.2),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_scale(Vec3::ZERO),
        TargetMarker,
        DespawnOnExit(AppState::Dungeon),
    ));
}

fn spawn_one(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    mesh: Handle<Mesh>,
    color: Color,
    motion: Motion,
    life: f32,
) {
    let alpha = color.alpha();
    let material = materials.add(fx_material(color, 2.0));
    let start = match motion {
        Motion::Travel { from, .. } => Transform::from_translation(from),
        Motion::Grow { at, from, .. } => {
            Transform::from_translation(at).with_scale(Vec3::splat(from))
        }
        Motion::Rise { at, .. } => Transform::from_translation(at),
    };
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material.clone()),
        start,
        Fx {
            motion,
            age: 0.0,
            life,
            material,
            base_alpha: alpha,
        },
        DespawnOnExit(AppState::Dungeon),
    ));
}

/// Height of an actor's chest, and its rough size, for placing effects.
fn chest_h(sim: &Sim, id: ActorId) -> f32 {
    let a = &sim.actors[id];
    if a.is_enemy() { a.radius * 1.6 } else { 1.2 }
}

fn spawn_fx(
    mut commands: Commands,
    run: Res<ActiveRun>,
    assets: Option<Res<FxAssets>>,
    mut react: Option<ResMut<HitReact>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(assets) = assets else { return };
    let sim = &run.sim;
    let chest = |id: ActorId| run.world(id, chest_h(sim, id));
    let feet = |id: ActorId| run.world(id, 0.1);
    let gold = Color::srgba(1.0, 0.8, 0.3, 0.9);
    for imp in impacts(sim, &run.fresh) {
        let mut one = |mesh: &Handle<Mesh>, color: Color, motion: Motion, life: f32| {
            spawn_one(
                &mut commands,
                &mut materials,
                mesh.clone(),
                color,
                motion,
                life,
            )
        };
        match imp {
            Impact::Strike {
                dst,
                crit,
                mine,
                heavy,
                ..
            } => {
                let at = chest(dst);
                if mine {
                    // A gold burst and a flash on the ground under the target.
                    let size = if heavy {
                        3.0
                    } else if crit {
                        2.4
                    } else {
                        1.6
                    };
                    one(
                        &assets.cube,
                        gold,
                        Motion::Grow {
                            at,
                            from: 0.4,
                            to: size,
                        },
                        0.2,
                    );
                    let r = sim.actors[dst].radius + if crit { 1.4 } else { 0.9 };
                    one(
                        &assets.ring,
                        Color::srgba(1.0, 0.85, 0.4, 0.8),
                        Motion::Grow {
                            at: feet(dst),
                            from: r * 0.5,
                            to: r,
                        },
                        0.22,
                    );
                } else {
                    let size = if crit { 1.4 } else { 0.8 };
                    one(
                        &assets.cube,
                        Color::srgba(1.0, 0.9, 0.8, 0.5),
                        Motion::Grow {
                            at,
                            from: 0.3,
                            to: size,
                        },
                        0.15,
                    );
                }
            }
            Impact::Bolt {
                src,
                dst,
                school,
                wand,
                crit,
                mine,
            } => {
                let color = school_color(school, wand);
                if mine {
                    // A bigger projectile, then a burst where it lands.
                    let from = chest(src);
                    let to = chest(dst);
                    one(&assets.sphere, color, Motion::Travel { from, to }, 0.22);
                    let size = if crit { 5.0 } else { 3.2 };
                    one(
                        &assets.sphere,
                        color.with_alpha(0.7),
                        Motion::Grow {
                            at: to,
                            from: 1.0,
                            to: size,
                        },
                        0.3,
                    );
                } else {
                    one(
                        &assets.sphere,
                        color,
                        Motion::Travel {
                            from: chest(src),
                            to: chest(dst),
                        },
                        0.22,
                    );
                }
            }
            Impact::Area { src, radius, mine } => {
                // Enemies' area attacks keep their full strength (danger).
                let color = if mine {
                    Color::srgba(1.0, 0.75, 0.3, 0.9)
                } else if sim.actors[src].is_enemy() {
                    Color::srgba(1.0, 0.6, 0.2, 0.8)
                } else {
                    Color::srgba(1.0, 0.6, 0.2, 0.55)
                };
                one(
                    &assets.ring,
                    color,
                    Motion::Grow {
                        at: feet(src),
                        from: 1.0,
                        to: radius,
                    },
                    if mine { 0.35 } else { 0.3 },
                );
            }
            Impact::Heal { dst, crit, mine } => {
                let (color, height, life) = if mine {
                    (Color::srgba(0.4, 1.0, 0.5, 0.95), 2.8, 0.7)
                } else {
                    (Color::srgba(0.3, 1.0, 0.4, 0.55), 1.8, 0.5)
                };
                one(
                    &assets.sphere,
                    color,
                    Motion::Rise {
                        at: feet(dst) + Vec3::Y * 0.5,
                        height,
                    },
                    life,
                );
                if mine {
                    // A ring at the feet: bigger for a crit.
                    let r = if crit { 1.8 } else { 1.2 };
                    one(
                        &assets.ring,
                        Color::srgba(0.4, 1.0, 0.5, 0.7),
                        Motion::Grow {
                            at: feet(dst),
                            from: 0.4,
                            to: r,
                        },
                        0.45,
                    );
                }
            }
            Impact::Interrupt { dst, mine } => {
                // A cyan burst and four shards flying out of the caster.
                let at = chest(dst);
                let s = if mine { 1.0 } else { 0.6 };
                let cyan = Color::srgba(0.45, 0.9, 1.0, 0.9);
                one(
                    &assets.sphere,
                    cyan,
                    Motion::Grow {
                        at,
                        from: 1.0,
                        to: 7.0 * s,
                    },
                    0.3,
                );
                for k in 0..4 {
                    let ang = k as f32 * std::f32::consts::FRAC_PI_2 + 0.4;
                    let dir = Vec3::new(ang.cos(), 0.6, ang.sin());
                    one(
                        &assets.cube,
                        cyan,
                        Motion::Travel {
                            from: at,
                            to: at + dir * 2.2 * s,
                        },
                        0.35,
                    );
                }
            }
            Impact::Taunt { dst } => one(
                &assets.ring,
                Color::srgba(1.0, 0.3, 0.1, 0.8),
                Motion::Grow {
                    at: feet(dst),
                    from: 0.5,
                    to: 3.0,
                },
                0.4,
            ),
            Impact::SelfBuff { dst } => one(
                &assets.sphere,
                Color::srgba(0.4, 0.6, 1.0, 0.35),
                Motion::Grow {
                    at: chest(dst),
                    from: 3.0,
                    to: 6.0,
                },
                0.5,
            ),
            Impact::ThreatDrop { src } => one(
                &assets.sphere,
                Color::srgba(0.6, 0.6, 0.6, 0.5),
                Motion::Grow {
                    at: chest(src),
                    from: 1.0,
                    to: 6.0,
                },
                0.4,
            ),
            Impact::Boom => one(
                &assets.ring,
                Color::srgba(1.0, 0.45, 0.05, 0.8),
                Motion::Grow {
                    at: feet(sim.boss),
                    from: 1.0,
                    to: 9.0,
                },
                0.6,
            ),
            Impact::React { dst, strength } => {
                if let Some(r) = react.as_mut() {
                    r.start(dst, strength, Color::WHITE);
                }
            }
            Impact::Hurt { dst } => {
                if let Some(r) = react.as_mut() {
                    r.start(dst, 1.0, Color::srgb(1.0, 0.15, 0.1));
                }
            }
        }
    }
}

fn animate_fx(
    mut commands: Commands,
    time: Res<Time>,
    mut q: Query<(Entity, &mut Fx, &mut Transform)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (e, mut fx, mut t) in &mut q {
        fx.age += time.delta_secs();
        let k = (fx.age / fx.life).min(1.0);
        match fx.motion {
            Motion::Travel { from, to } => t.translation = from.lerp(to, k),
            Motion::Grow { at, from, to } => {
                t.translation = at;
                t.scale = Vec3::splat(from + (to - from) * k);
            }
            Motion::Rise { at, height } => t.translation = at + Vec3::Y * height * k,
        }
        if let Some(mut m) = materials.get_mut(&fx.material) {
            m.base_color = m.base_color.with_alpha(fx.base_alpha * (1.0 - k));
        }
        if fx.age >= fx.life {
            materials.remove(&fx.material);
            commands.entity(e).despawn();
        }
    }
}

/// Pulsing ring under the selected target, coloured hostile red / friendly blue.
fn target_marker(
    time: Res<Time>,
    run: Res<ActiveRun>,
    mut marker: Single<(&mut Transform, &MeshMaterial3d<StandardMaterial>), With<TargetMarker>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let sim = &run.sim;
    let target = sim
        .player
        .and_then(|p| sim.actors[p].target)
        .filter(|&t| sim.actors[t].alive);
    let (t, mat) = &mut *marker;
    match target {
        Some(id) => {
            let a = &sim.actors[id];
            let pulse = 1.0 + (time.elapsed_secs() * 5.0).sin() * 0.06;
            let r = (a.radius + 0.6) * pulse;
            t.translation = run.world(id, 0.08);
            t.scale = Vec3::new(r, 0.3, r);
            if let Some(mut m) = materials.get_mut(&mat.0) {
                let c = if a.is_enemy() {
                    Color::srgba(1.0, 0.2, 0.15, 0.85)
                } else {
                    Color::srgba(0.3, 0.7, 1.0, 0.85)
                };
                if m.base_color != c {
                    m.base_color = c;
                }
            }
        }
        None => t.scale = Vec3::ZERO,
    }
}

// ------------------------------------------------------------ lasting states

/// Which lasting state an aura shows (0.9.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuraLook {
    /// An absorb shield: a bubble.
    Shield,
    /// Damage reduction (a defensive): a faint shell.
    Shell,
    /// Incapacitated: a turning ring over the head.
    Controlled,
    /// Lab B (0.10): an opening for the player's Exploit: a gold marker.
    Opening,
}

/// The lasting states an actor shows now, from its auras.
pub fn aura_looks(sim: &Sim, id: ActorId) -> Vec<AuraLook> {
    let a = &sim.actors[id];
    if !a.alive || a.enemy.as_ref().is_some_and(|e| e.despawned) {
        return Vec::new();
    }
    let mut out = Vec::new();
    if a.has_aura(|k| matches!(k, AuraKind::Absorb { remaining } if *remaining > 0.0)) {
        out.push(AuraLook::Shield);
    }
    if a.has_aura(|k| matches!(k, AuraKind::DamageReduction { .. })) {
        out.push(AuraLook::Shell);
    }
    if a.is_incapacitated() {
        out.push(AuraLook::Controlled);
    }
    if sim.player.is_some_and(|p| sim.has_opening(id, p)) {
        out.push(AuraLook::Opening);
    }
    out
}

/// The entities drawing lasting states, by actor and look.
#[derive(Resource, Default)]
struct AuraFx {
    shown: Vec<(ActorId, AuraLook, Entity)>,
}

fn aura_visuals(
    mut commands: Commands,
    time: Res<Time>,
    run: Res<ActiveRun>,
    assets: Option<Res<FxAssets>>,
    mut fx: Option<ResMut<AuraFx>>,
    mut q: Query<&mut Transform, Without<Fx>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (Some(assets), Some(fx)) = (assets, fx.as_mut()) else {
        return;
    };
    let sim = &run.sim;
    // Remove what ended; a shield that ends pops.
    let mut keep = Vec::new();
    for (id, look, e) in fx.shown.drain(..) {
        if aura_looks(sim, id).contains(&look) {
            keep.push((id, look, e));
            continue;
        }
        commands.entity(e).despawn();
        if look == AuraLook::Shield && sim.actors[id].alive {
            spawn_one(
                &mut commands,
                &mut materials,
                assets.sphere.clone(),
                Color::srgba(0.6, 0.85, 1.0, 0.6),
                Motion::Grow {
                    at: run.world(id, chest_h(sim, id)),
                    from: shield_scale(sim, id),
                    to: shield_scale(sim, id) * 1.5,
                },
                0.25,
            );
        }
    }
    fx.shown = keep;
    // Add what started.
    for id in 0..sim.actors.len() {
        for look in aura_looks(sim, id) {
            if fx.shown.iter().any(|(i, l, _)| *i == id && *l == look) {
                continue;
            }
            let (mesh, mat) = match look {
                AuraLook::Shield => (assets.sphere.clone(), assets.shield.clone()),
                AuraLook::Shell => (assets.sphere.clone(), assets.shell.clone()),
                AuraLook::Controlled => (assets.ring.clone(), assets.cc.clone()),
                AuraLook::Opening => (assets.cube.clone(), assets.opening.clone()),
            };
            let e = commands
                .spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(mat),
                    Transform::from_scale(Vec3::ZERO),
                    DespawnOnExit(AppState::Dungeon),
                ))
                .id();
            fx.shown.push((id, look, e));
        }
    }
    // Follow the actors.
    let t = time.elapsed_secs();
    for &(id, look, e) in &fx.shown {
        let Ok(mut tr) = q.get_mut(e) else { continue };
        match look {
            AuraLook::Shield => {
                let pulse = 1.0 + (t * 3.0).sin() * 0.03;
                tr.translation = run.world(id, chest_h(sim, id));
                tr.scale = Vec3::splat(shield_scale(sim, id) * pulse);
            }
            AuraLook::Shell => {
                tr.translation = run.world(id, chest_h(sim, id));
                tr.scale = Vec3::splat(shield_scale(sim, id) * 1.12);
            }
            AuraLook::Opening => {
                // A spinning gold diamond over the head, pulsing.
                let a = &sim.actors[id];
                let h = if a.is_enemy() {
                    a.radius * 3.2 + 0.6
                } else {
                    2.8
                };
                let pulse = 1.0 + (t * 8.0).sin() * 0.15;
                tr.translation = run.world(id, h);
                tr.rotation = Quat::from_rotation_y(t * 3.0)
                    * Quat::from_rotation_x(std::f32::consts::FRAC_PI_4)
                    * Quat::from_rotation_z(std::f32::consts::FRAC_PI_4);
                tr.scale = Vec3::splat(0.9 * pulse);
            }
            AuraLook::Controlled => {
                let a = &sim.actors[id];
                let h = if a.is_enemy() { a.radius * 3.2 } else { 2.4 };
                let r = a.radius.max(0.5) * 0.8;
                tr.translation = run.world(id, h);
                tr.rotation = Quat::from_rotation_y(t * 2.0) * Quat::from_rotation_x(0.25);
                tr.scale = Vec3::new(r, 0.4, r);
            }
        }
    }
}

/// Bubble scale for the unit sphere (radius 0.25) around an actor.
fn shield_scale(sim: &Sim, id: ActorId) -> f32 {
    let a = &sim.actors[id];
    let r = if a.is_enemy() {
        a.radius * 1.5 + 0.4
    } else {
        1.15
    };
    r / 0.25
}

// -------------------------------------------------------------- hit reaction

/// Targets reacting to a hit: actor, age, strength, flash colour.
#[derive(Resource, Default)]
struct HitReact {
    active: Vec<(ActorId, f32, f32, Color)>,
}

const REACT_SECS: f32 = 0.18;

impl HitReact {
    fn start(&mut self, id: ActorId, strength: f32, color: Color) {
        self.active.retain(|r| r.0 != id);
        self.active.push((id, 0.0, strength, color));
    }
}

/// The emissive an actor's model has when not reacting (see `spawn_new_actors`).
fn base_emissive(sim: &Sim, id: ActorId) -> LinearRgba {
    if Some(id) == sim.player {
        LinearRgba::rgb(0.15, 0.12, 0.02)
    } else {
        LinearRgba::BLACK
    }
}

/// Swell and flash a model briefly after the player hits it (runs after the
/// actors' transforms are set for the frame).
fn hit_reaction(
    time: Res<Time>,
    run: Res<ActiveRun>,
    react: Option<ResMut<HitReact>>,
    mut q: Query<(&crate::dungeon::ActorVisual, &mut Transform)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(mut react) = react else { return };
    if react.active.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    let sim = &run.sim;
    for (vis, mut t) in &mut q {
        let Some(i) = react.active.iter().position(|r| r.0 == vis.id) else {
            continue;
        };
        let (_, age, strength, color) = &mut react.active[i];
        *age += dt;
        let life = REACT_SECS * strength.max(1.0);
        let k = (1.0 - *age / life).clamp(0.0, 1.0);
        if sim.actors[vis.id].alive {
            t.scale *= 1.0 + 0.07 * *strength * k;
        }
        if let Some(mut m) = materials.get_mut(&vis.material) {
            m.emissive = if k > 0.0 {
                base_emissive(sim, vis.id) + color.to_linear() * (0.8 * *strength * k)
            } else {
                base_emissive(sim, vis.id)
            };
        }
    }
    react
        .active
        .retain(|(_, age, strength, _)| *age < REACT_SECS * strength.max(1.0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use dungeon_sim::class::ClassId;
    use dungeon_sim::content::Content;
    use dungeon_sim::sim::{Controller, PartyMemberSpec, RunOptions, RunOutcome};

    fn party(class: ClassId, size: u32) -> Vec<PartyMemberSpec> {
        let mut pr = dungeon_sim::Profile::new_default();
        pr.player_class = class;
        pr.set_group_size(size);
        pr.party_specs(true)
    }

    /// Every impact over an autopilot run (the AI plays the player's slot),
    /// one step at a time, and the lasting looks seen along the way.
    fn run(class: ClassId, size: u32, seed: u64) -> (Sim, Vec<Impact>, Vec<AuraLook>) {
        let c = Content::builtin();
        let opts = RunOptions {
            autopilot: true,
            ..Default::default()
        };
        let mut sim = Sim::with_options(&c, &party(class, size), seed, opts);
        let p = sim.player.unwrap();
        sim.actors[p].controller = Controller::Ai;
        if let Some(t) = sim.tank() {
            sim.leader = t;
        }
        let (mut seen, mut out, mut looks) = (0, Vec::new(), Vec::new());
        while sim.outcome == RunOutcome::InProgress && sim.time < 900.0 {
            sim.step();
            out.extend(impacts(&sim, &sim.events[seen..]));
            seen = sim.events.len();
            for id in 0..sim.actors.len() {
                for l in aura_looks(&sim, id) {
                    if !looks.contains(&l) {
                        looks.push(l);
                    }
                }
            }
        }
        (sim, out, looks)
    }

    #[test]
    fn warden_strikes_are_mine_and_quake_stomp_uses_its_radius() {
        let (sim, imp, looks) = run(ClassId::Warden, 5, 1);
        let p = sim.player.unwrap();
        assert!(
            imp.iter()
                .any(|i| matches!(i, Impact::Strike { mine: true, .. }))
        );
        assert!(
            imp.iter()
                .any(|i| matches!(i, Impact::Strike { mine: false, .. }))
        );
        // Quake Stomp is 8 m, the striker companion's Whirl 6 m.
        assert!(imp.iter().any(|i| matches!(i,
            Impact::Area { src, radius, mine: true } if *src == p && *radius == 8.0)));
        assert!(imp.iter().any(|i| matches!(i,
            Impact::Area { radius, mine: false, .. } if *radius == 6.0)));
        assert!(
            imp.iter()
                .any(|i| matches!(i, Impact::React { strength, .. } if *strength == 2.0))
        );
        // Bulwark and the healer's Ward show as lasting states.
        assert!(looks.contains(&AuraLook::Shell), "{looks:?}");
        assert!(looks.contains(&AuraLook::Shield), "{looks:?}");
    }

    #[test]
    fn arcanist_bolts_interrupts_and_hex() {
        let (_, imp, looks) = run(ClassId::Arcanist, 5, 1);
        assert!(
            imp.iter()
                .any(|i| matches!(i, Impact::Bolt { mine: true, .. }))
        );
        assert!(imp.iter().any(|i| matches!(i, Impact::Interrupt { .. })));
        // Area rings are one per cast, never one per target.
        let mut multi = 0;
        let (_, imp5, _) = run(ClassId::Striker, 5, 2);
        for w in imp5.windows(2) {
            if let [Impact::Area { src: a, .. }, Impact::Area { src: b, .. }] = w
                && a == b
            {
                multi += 1;
            }
        }
        assert_eq!(multi, 0, "an area attack drew more than one ring");
        assert!(looks.contains(&AuraLook::Controlled), "{looks:?}");
    }

    #[test]
    fn mender_heals_and_ticks() {
        let (_, imp, _) = run(ClassId::Mender, 5, 1);
        let mine = imp
            .iter()
            .filter(|i| matches!(i, Impact::Heal { mine: true, .. }))
            .count();
        assert!(mine > 0);
    }

    #[test]
    fn lab_effects_reaping_whirl_ring_exploit_hits_and_openings() {
        use dungeon_sim::class::StrikerStyle;
        use dungeon_sim::sim::LabPlan;
        let c = Content::builtin();
        let party = dungeon_sim::lab::lab_party(3);
        for style in [StrikerStyle::Planned, StrikerStyle::Reactive] {
            let mut sim =
                dungeon_sim::lab::lab_sim(&c, &party, 1, style, LabPlan::Expert, "normal");
            let p = sim.player.unwrap();
            let (mut seen, mut imp, mut looks) = (0, Vec::new(), Vec::new());
            while sim.outcome == RunOutcome::InProgress && sim.time < 900.0 {
                sim.step();
                imp.extend(impacts(&sim, &sim.events[seen..]));
                seen = sim.events.len();
                for id in 0..sim.actors.len() {
                    for l in aura_looks(&sim, id) {
                        if !looks.contains(&l) {
                            looks.push(l);
                        }
                    }
                }
            }
            match style {
                StrikerStyle::Planned => {
                    // Reaping Whirl draws its own 6 m ring.
                    assert!(imp.iter().any(|i| matches!(i,
                        Impact::Area { src, radius, mine: true } if *src == p && *radius == 6.0)));
                    assert!(!looks.contains(&AuraLook::Opening));
                }
                _ => {
                    assert!(imp.iter().any(|i| matches!(
                        i,
                        Impact::Strike {
                            mine: true,
                            heavy: true,
                            ..
                        }
                    )));
                    assert!(looks.contains(&AuraLook::Opening), "{looks:?}");
                }
            }
        }
    }

    #[test]
    fn raids_drop_companions_ordinary_swings() {
        let (sim, imp, _) = run(ClassId::Striker, 10, 1);
        assert!(
            !imp.iter().any(|i| matches!(i,
                Impact::Strike { src, mine: false, crit: false, .. }
                    if !sim.actors[*src].is_enemy())),
            "ordinary companion swings drawn in a raid"
        );
        // Enemies' swings are still drawn.
        assert!(imp.iter().any(|i| matches!(i,
            Impact::Strike { src, .. } if sim.actors[*src].is_enemy())));
        assert!(
            imp.iter()
                .any(|i| matches!(i, Impact::Strike { mine: true, .. }))
        );
    }
}
