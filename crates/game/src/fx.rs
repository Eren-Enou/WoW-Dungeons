//! Lightweight procedural combat effects driven by sim events: projectiles for
//! spells, flashes for melee, rings for AoE/taunts, sparkles for heals, and a
//! glow on the selected target.

use crate::common::{AppState, to_vec3};
use crate::dungeon::ActiveRun;
use bevy::prelude::*;
use dungeon_sim::EventKind;
use dungeon_sim::stats::{HitOutcome, School};

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Dungeon), setup_fx_assets)
            .add_systems(
                Update,
                (spawn_fx, animate_fx, target_marker)
                    .chain()
                    .after(crate::dungeon::process_events)
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

/// Effect colour by damage school (physical uses an off-white).
fn school_color(school: School, what: &str) -> Color {
    if what == "Wand" {
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

/// Abilities shown as one expanding ring rather than per-target effects.
fn is_area(what: &str) -> bool {
    matches!(
        what,
        "Whirl"
            | "Quake Stomp"
            | "Rockfall Cleave"
            | "Rending Mandibles"
            | "Root Lash"
            | "Hollow Sweep"
    )
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
    });
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
    let material = materials.add(StandardMaterial {
        base_color: color,
        emissive: color.to_linear() * 2.0,
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
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

fn spawn_fx(
    mut commands: Commands,
    run: Res<ActiveRun>,
    assets: Option<Res<FxAssets>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(assets) = assets else { return };
    let sim = &run.sim;
    let chest = |id: usize| {
        let a = &sim.actors[id];
        to_vec3(a.pos, if a.is_enemy() { a.radius * 1.6 } else { 1.2 })
    };
    let feet = |id: usize| to_vec3(sim.actors[id].pos, 0.1);
    for e in &run.fresh {
        match &e.kind {
            EventKind::Damage {
                src,
                dst,
                what,
                outcome,
                school,
                ..
            } => {
                let crit = outcome == &HitOutcome::Crit;
                let ranged = sim.actors[*src].pos.dist(sim.actors[*dst].pos)
                    > sim.actors[*src].radius + sim.actors[*dst].radius + 3.0;
                let color = school_color(*school, what);
                if ranged && (school.is_magic() || &**what == "Wand") && !is_area(what) {
                    spawn_one(
                        &mut commands,
                        &mut materials,
                        assets.sphere.clone(),
                        color,
                        Motion::Travel {
                            from: chest(*src),
                            to: chest(*dst),
                        },
                        0.22,
                    );
                } else if !ranged && !is_area(what) && !outcome.avoided() {
                    let size = if crit { 1.6 } else { 1.0 };
                    spawn_one(
                        &mut commands,
                        &mut materials,
                        assets.cube.clone(),
                        Color::srgba(1.0, 0.9, 0.8, 0.7),
                        Motion::Grow {
                            at: chest(*dst),
                            from: 0.3,
                            to: size,
                        },
                        0.15,
                    );
                }
            }
            EventKind::Heal { dst, .. } => spawn_one(
                &mut commands,
                &mut materials,
                assets.sphere.clone(),
                Color::srgba(0.3, 1.0, 0.4, 0.8),
                Motion::Rise {
                    at: feet(*dst) + Vec3::Y * 0.5,
                    height: 2.0,
                },
                0.6,
            ),
            EventKind::Taunt { dst, .. } => spawn_one(
                &mut commands,
                &mut materials,
                assets.ring.clone(),
                Color::srgba(1.0, 0.3, 0.1, 0.8),
                Motion::Grow {
                    at: feet(*dst),
                    from: 0.5,
                    to: 3.0,
                },
                0.4,
            ),
            EventKind::AuraApplied { src, dst, .. } if src == dst => spawn_one(
                &mut commands,
                &mut materials,
                assets.sphere.clone(),
                Color::srgba(0.4, 0.6, 1.0, 0.35),
                Motion::Grow {
                    at: chest(*dst),
                    from: 3.0,
                    to: 6.0,
                },
                0.5,
            ),
            EventKind::ThreatDrop { src } => spawn_one(
                &mut commands,
                &mut materials,
                assets.sphere.clone(),
                Color::srgba(0.6, 0.6, 0.6, 0.5),
                Motion::Grow {
                    at: chest(*src),
                    from: 1.0,
                    to: 6.0,
                },
                0.4,
            ),
            EventKind::TelegraphHit { .. } | EventKind::PhaseChange { .. } => {
                let at = feet(sim.boss);
                spawn_one(
                    &mut commands,
                    &mut materials,
                    assets.ring.clone(),
                    Color::srgba(1.0, 0.45, 0.05, 0.8),
                    Motion::Grow {
                        at,
                        from: 1.0,
                        to: 9.0,
                    },
                    0.6,
                );
            }
            _ => {}
        }
    }
    // AoE swings show as one ring per cast rather than per target.
    let mut seen = Vec::new();
    for e in &run.fresh {
        if let EventKind::Damage { src, what, .. } = &e.kind
            && is_area(what)
            && !seen.contains(src)
        {
            seen.push(*src);
            spawn_one(
                &mut commands,
                &mut materials,
                assets.ring.clone(),
                Color::srgba(1.0, 0.6, 0.2, 0.8),
                Motion::Grow {
                    at: feet(*src),
                    from: 1.0,
                    to: 6.0,
                },
                0.3,
            );
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
            t.translation = to_vec3(run.interp_pos(id), 0.08);
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
