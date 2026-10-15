//! Dungeon state: owns the running `Sim`, maps input to sim commands at a fixed
//! timestep, and renders actors, walls, hazards and telegraphs.

use crate::camera::{CameraBlockers, CameraFocus, MainCamera, OrbitCamera};
use crate::common::{AppState, GameData, LastResult, rgb, to_vec3};
use crate::controls::{ControlSet, PlayerControl};
use crate::demo::DemoConfig;
use crate::hub::class_color;
use bevy::prelude::*;
use dungeon_sim::content::EnemyRank;
use dungeon_sim::profile::RunReport;
use dungeon_sim::sim::{Actor, Controller};
use dungeon_sim::{
    ActorId, DT, EnemyMode, Event, EventKind, RunOptions, RunOutcome, RunSummary, Sim, V2,
};
use std::collections::{HashMap, VecDeque};
use std::f32::consts::FRAC_PI_2;

pub struct DungeonPlugin;

impl Plugin for DungeonPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(AppState::Dungeon),
            (
                start_run,
                spawn_world,
                spawn_new_actors,
                crate::hud::spawn_hud,
            )
                .chain(),
        )
        .add_systems(
            OnExit(AppState::Dungeon),
            |mut c: Commands,
             mut blockers: ResMut<CameraBlockers>,
             camera: Single<Entity, With<MainCamera>>| {
                c.remove_resource::<ActiveRun>();
                blockers.0.clear();
                c.entity(*camera).remove::<bevy::pbr::DistanceFog>();
            },
        )
        .add_systems(
            Update,
            (
                player_input.after(ControlSet),
                step_sim,
                process_events,
                spawn_new_actors,
                sync_actors,
                fade_occluders,
                sync_telegraphs,
                sync_hazards,
                draw_gizmos,
                finish_run,
            )
                .chain()
                .run_if(in_state(AppState::Dungeon))
                .run_if(resource_exists::<ActiveRun>),
        );
    }
}

/// Seconds between the run ending and the results screen.
const END_DELAY: f32 = 3.0;
const LOG_LINES: usize = 11;
/// How long a failed-action message stays on screen.
const ERROR_SECS: f32 = 1.5;

#[derive(Resource)]
pub struct ActiveRun {
    pub sim: Sim,
    pub acc: f32,
    pub prev_pos: Vec<V2>,
    /// Elevations at the previous tick (interpolated like positions).
    pub prev_y: Vec<f32>,
    pub events_seen: usize,
    /// Events produced since the previous frame (for effects and floating text).
    pub fresh: Vec<Event>,
    pub log: VecDeque<String>,
    pub verbose_log: bool,
    pub banner: Option<(String, f32, Color)>,
    pub error: Option<(String, f32)>,
    /// Threat alert ("X is attacking Y!") with its remaining time and colour.
    pub alert: Option<(String, f32, Color)>,
    /// Seconds before another threat alert may be shown (keeps them restrained).
    pub alert_cooldown: f32,
    pub end_timer: Option<f32>,
    /// Number of actors that already have visuals (summons appear later).
    pub visuals_spawned: usize,
    pub wall_color: Color,
}

impl ActiveRun {
    pub fn alpha(&self) -> f32 {
        (self.acc / DT).clamp(0.0, 1.0)
    }

    pub fn interp_pos(&self, id: ActorId) -> V2 {
        let b = self.sim.actors[id].pos;
        let a = self.prev_pos.get(id).copied().unwrap_or(b);
        a + (b - a) * self.alpha()
    }

    pub fn interp_y(&self, id: ActorId) -> f32 {
        let b = self.sim.actors[id].y;
        let a = self.prev_y.get(id).copied().unwrap_or(b);
        a + (b - a) * self.alpha()
    }

    /// Interpolated world position of an actor, `h` metres above its feet.
    pub fn world(&self, id: ActorId, h: f32) -> Vec3 {
        to_vec3(self.interp_pos(id), self.interp_y(id) + h)
    }

    fn set_banner(&mut self, text: impl Into<String>, secs: f32, color: Color) {
        self.banner = Some((text.into(), secs, color));
    }
}

#[derive(Component)]
pub struct ActorVisual {
    pub id: ActorId,
    pub material: Handle<StandardMaterial>,
    pub base_color: Color,
    pub center_y: f32,
    pub lie_y: f32,
}

/// A wall that fades out when it hides the player from the camera.
#[derive(Component)]
struct Occluder {
    material: Handle<StandardMaterial>,
    min: Vec3,
    max: Vec3,
}

#[derive(Component)]
struct TelegraphVisual {
    id: u32,
    inner: bool,
}

#[derive(Component)]
struct HazardVisual {
    index: usize,
    material: Handle<StandardMaterial>,
}

fn start_run(
    mut commands: Commands,
    mut data: ResMut<GameData>,
    mut orbit: ResMut<OrbitCamera>,
    mut pc: ResMut<PlayerControl>,
    demo: Option<Res<DemoConfig>>,
) {
    let content = data
        .selected_content()
        .cloned()
        .expect("hub only enters with valid content");
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(1)
        % 1_000_000;
    let party = data.profile.party_specs_for(&content.dungeon.id, true);
    let autoplay = demo.as_ref().is_some_and(|d| d.autoplay);
    let options = RunOptions {
        difficulty: data.profile.selected_difficulty.clone(),
        overrides: data.profile.sandbox,
        autopilot: autoplay,
        skip_optional: false,
    };
    let mut sim = Sim::with_options(&content, &party, seed, options);
    if autoplay && let Some(p) = sim.player {
        // Demo: the AI drives the player's slot and the tank leads the way.
        sim.actors[p].controller = Controller::Ai;
        if let Some(t) = sim.tank() {
            sim.leader = t;
        }
    }
    info!(
        "Entering '{}' [{}] with seed {seed} and {} party members",
        content.dungeon.name,
        sim.difficulty.name,
        party.len()
    );
    orbit.yaw = 0.0;
    pc.state.facing = 0.0;
    pc.state.yaw = 0.0;
    let prev_pos = sim.actors.iter().map(|a| a.pos).collect();
    let prev_y = sim.actors.iter().map(|a| a.y).collect();
    let required = sim.encounters.iter().filter(|e| !e.optional).count();
    let boss_name = sim.actors[sim.boss].name.clone();
    let mut run = ActiveRun {
        sim,
        acc: 0.0,
        prev_pos,
        prev_y,
        events_seen: 0,
        fresh: Vec::new(),
        log: VecDeque::new(),
        verbose_log: false,
        banner: None,
        error: None,
        alert: None,
        alert_cooldown: 0.0,
        end_timer: None,
        visuals_spawned: 0,
        wall_color: rgb(content.dungeon.theme.wall),
    };
    let optional = content.encounters.len() - required;
    let extra = if optional > 0 {
        format!(" (+{optional} optional)")
    } else {
        String::new()
    };
    run.set_banner(
        format!(
            "{} - {required} encounters{extra}, final boss: {boss_name}",
            content.dungeon.name
        ),
        5.0,
        Color::srgb(0.9, 0.85, 0.6),
    );
    data.message = None;
    commands.insert_resource(run);
}

#[allow(clippy::too_many_arguments)]
fn spawn_world(
    mut commands: Commands,
    run: Res<ActiveRun>,
    camera: Single<Entity, With<MainCamera>>,
    mut blockers: ResMut<CameraBlockers>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    let scoped = DespawnOnExit(AppState::Dungeon);
    let theme = run.sim.content.dungeon.theme;
    commands.insert_resource(GlobalAmbientLight {
        color: theme
            .ambient
            .map(rgb)
            .unwrap_or(Color::srgb(0.6, 0.65, 0.8)),
        brightness: 220.0,
        ..default()
    });
    if let Some(fog) = theme.fog {
        commands.entity(*camera).insert(bevy::pbr::DistanceFog {
            color: rgb(fog),
            falloff: bevy::pbr::FogFalloff::Linear {
                start: 18.0,
                end: 75.0,
            },
            ..default()
        });
    }
    commands.spawn((
        DirectionalLight {
            illuminance: 1800.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(10.0, 30.0, -20.0).looking_at(Vec3::ZERO, Vec3::Y),
        scoped.clone(),
    ));
    spawn_level_geometry(
        &mut commands,
        &run,
        &mut blockers,
        &mut meshes,
        &mut materials,
    );
    let props = crate::cave::spawn_props(&mut commands, &run, &asset_server);
    blockers.0.extend(props);
    // Exit portal at the entrance.
    let entrance = run.sim.content.dungeon.entrance;
    let entrance_y = run.sim.content.dungeon.entrance_y.unwrap_or_else(|| {
        let nav = &run.sim.nav;
        nav.locate(entrance, 0.0)
            .map(|s| nav.height(s, entrance))
            .unwrap_or(0.0)
    });
    commands.spawn((
        Mesh3d(meshes.add(Torus::new(1.2, 1.6))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.1, 0.5),
            emissive: LinearRgba::rgb(1.5, 0.5, 3.0),
            ..default()
        })),
        Transform::from_translation(
            to_vec3(entrance, entrance_y + 1.8) + Vec3::new(0.0, 0.0, -5.0),
        )
        .with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
        scoped.clone(),
    ));
    // A dais under every boss.
    for &e in &run.sim.enemies {
        let a = &run.sim.actors[e];
        if a.rank() == Some(EnemyRank::Boss) {
            commands.spawn((
                Mesh3d(meshes.add(Cylinder::new(a.radius + 3.0, 0.15))),
                MeshMaterial3d(materials.add(Color::srgb(0.2, 0.12, 0.25))),
                Transform::from_translation(to_vec3(a.pos, a.y + 0.02)),
                scoped.clone(),
            ));
        }
    }
    // Hazards: discs whose colour shows whether they are active.
    let disc = meshes.add(Circle::new(1.0));
    for (i, h) in run.sim.hazards.iter().enumerate() {
        let material = materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.4, 0.1, 0.15),
            emissive: LinearRgba::rgb(0.3, 0.1, 0.0),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
        commands.spawn((
            Mesh3d(disc.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(to_vec3(h.pos.xz(), h.pos.y + 0.035))
                .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
                .with_scale(Vec3::new(h.cfg.radius, h.cfg.radius, 1.0)),
            HazardVisual { index: i, material },
            scoped.clone(),
        ));
    }
}

/// Build floors, ramps, stairs, bridges, rock supports, walls, drop markers,
/// decoration and lava from the simulation's `Nav` and content, so what you
/// see is what the sim walks on.
fn spawn_level_geometry(
    commands: &mut Commands,
    run: &ActiveRun,
    blockers: &mut CameraBlockers,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    use crate::cave::{prism_mesh, wall_chunks};
    use dungeon_sim::nav::{Axis, LinkKind, SLAB};
    use dungeon_sim::poly::Poly;
    let scoped = DespawnOnExit(AppState::Dungeon);
    let nav = &run.sim.nav;
    let dungeon = &run.sim.content.dungeon;
    let theme = dungeon.theme;
    let [fr, fg, fb] = theme.floor;
    // Floors get lighter with height so levels read at a glance.
    let (lo, hi) = nav
        .surfaces
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), s| {
            (lo.min(s.min_y()), hi.max(s.max_y()))
        });
    let shade = |base: [f32; 3], y: f32| {
        let t = if hi > lo { (y - lo) / (hi - lo) } else { 0.0 };
        Color::srgb(base[0] + 0.12 * t, base[1] + 0.1 * t, base[2] + 0.07 * t)
    };
    let rock = materials.add(StandardMaterial {
        base_color: Color::srgb(fr * 0.55, fg * 0.55, fb * 0.6),
        perceptual_roughness: 0.95,
        ..default()
    });
    blockers.0.clear();
    let auto_lights = dungeon.lights.is_empty();
    for (i, s) in nav.surfaces.iter().enumerate() {
        let tint = dungeon
            .areas
            .get(i)
            .and_then(|a| a.tint)
            .unwrap_or(theme.floor);
        let alt = if i % 2 == 0 { 0.0 } else { 0.02 };
        let top_mat = materials.add(StandardMaterial {
            base_color: if s.def.open_under {
                Color::srgb(0.42, 0.3, 0.2)
            } else {
                shade([tint[0] + alt, tint[1] + alt, tint[2]], s.max_y())
            },
            perceptual_roughness: 0.9,
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        let bottom = if s.def.open_under {
            s.min_y() - SLAB
        } else {
            nav.base_y
        };
        let shape = &s.def.shape;
        let b = s.bounds;
        let rect_stairs = s.def.stairs && s.def.slope.is_none() && shape.is_axis_rect();
        if rect_stairs {
            // Steps of at most 0.5 m along an axis ramp.
            let len = match s.def.axis {
                Axis::Z => b.size().z,
                Axis::X => b.size().x,
            };
            let n = ((len / 0.5).ceil() as usize).max(1);
            let step = len / n as f32;
            for k in 0..n {
                let (a0, a1) = (k as f32 * step, (k + 1) as f32 * step);
                let (min, max) = match s.def.axis {
                    Axis::Z => (
                        V2::new(b.min.x, b.min.z + a0),
                        V2::new(b.max.x, b.min.z + a1),
                    ),
                    Axis::X => (
                        V2::new(b.min.x + a0, b.min.z),
                        V2::new(b.min.x + a1, b.max.z),
                    ),
                };
                let h = s.height_at((min + max) * 0.5);
                let piece = Poly::from_rect(dungeon_sim::nav::Rect::new(min, max));
                commands.spawn((
                    Mesh3d(meshes.add(prism_mesh(&piece, |_| h, bottom))),
                    MeshMaterial3d(top_mat.clone()),
                    Transform::IDENTITY,
                    scoped.clone(),
                ));
                blockers.0.push((
                    Vec3::new(min.x, bottom, min.z),
                    Vec3::new(max.x, h - 0.1, max.z),
                ));
            }
        } else {
            // A prism whose top follows the surface (flat floor, ramp, deck).
            let z = -(i as f32) * 0.001;
            commands.spawn((
                Mesh3d(meshes.add(prism_mesh(shape, |p| s.height_at(p) + z, bottom))),
                MeshMaterial3d(top_mat.clone()),
                Transform::IDENTITY,
                scoped.clone(),
            ));
            if !s.def.open_under && bottom < s.min_y() - 0.25 {
                // Rock under raised floors reads as rock, not floor.
                commands.spawn((
                    Mesh3d(meshes.add(prism_mesh(shape, |p| s.height_at(p) - 0.25, bottom))),
                    MeshMaterial3d(rock.clone()),
                    Transform::from_scale(Vec3::new(1.0, 1.0, 1.0)),
                    scoped.clone(),
                ));
            }
            // Camera blockers: ramps in 1 m slices along their bounds, floors whole.
            if s.is_ramp() {
                let n = (b.size().x.max(b.size().z).ceil() as usize).max(1);
                for k in 0..n {
                    let f0 = k as f32 / n as f32;
                    let f1 = (k + 1) as f32 / n as f32;
                    let along_x = b.size().x >= b.size().z;
                    let (min, max) = if along_x {
                        (
                            V2::new(b.min.x + b.size().x * f0, b.min.z),
                            V2::new(b.min.x + b.size().x * f1, b.max.z),
                        )
                    } else {
                        (
                            V2::new(b.min.x, b.min.z + b.size().z * f0),
                            V2::new(b.max.x, b.min.z + b.size().z * f1),
                        )
                    };
                    let low = s
                        .height_at(min)
                        .min(s.height_at(max))
                        .min(s.height_at(V2::new(min.x, max.z)))
                        .min(s.height_at(V2::new(max.x, min.z)));
                    blockers.0.push((
                        Vec3::new(min.x, bottom, min.z),
                        Vec3::new(max.x, low - 0.15, max.z),
                    ));
                }
            } else {
                blockers.0.push((
                    Vec3::new(b.min.x, bottom, b.min.z),
                    Vec3::new(b.max.x, s.max_y() - 0.05, b.max.z),
                ));
            }
        }
        if auto_lights {
            let c = shape.centroid();
            commands.spawn((
                PointLight {
                    color: rgb(theme.light),
                    intensity: 600_000.0,
                    range: 35.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_xyz(c.x, s.max_y() + 7.0, c.z),
                scoped.clone(),
            ));
        }
    }
    // Walls: smooth slabs or jagged rock, at any angle, never over the floor.
    for w in &nav.walls {
        for c in wall_chunks(w, theme.rough_walls) {
            blockers.0.push((c.min, c.max));
            let material = materials.add(StandardMaterial {
                base_color: run.wall_color,
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 0.95,
                ..default()
            });
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(c.size.x, c.size.y, c.size.z))),
                MeshMaterial3d(material.clone()),
                Occluder {
                    material,
                    min: c.min,
                    max: c.max,
                },
                Transform::from_translation(c.center).with_rotation(c.rotation),
                scoped.clone(),
            ));
        }
    }
    // One-way drops: an orange strip on the edge you can step off.
    let drop_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.55, 0.1, 0.55),
        emissive: LinearRgba::rgb(0.6, 0.25, 0.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    for l in nav.links.iter().filter(|l| l.kind == LinkKind::Drop) {
        let y = l.entry.y;
        commands.spawn((
            Mesh3d(meshes.add(prism_mesh(&l.portal, |_| y + 0.04, y + 0.01))),
            MeshMaterial3d(drop_mat.clone()),
            Transform::IDENTITY,
            scoped.clone(),
        ));
    }
    crate::cave::spawn_lava(commands, run, meshes, materials);
    let decor = crate::cave::spawn_decor(commands, run, meshes, materials);
    blockers.0.extend(decor);
    for l in &dungeon.lights {
        commands.spawn((
            PointLight {
                color: rgb(l.color),
                intensity: 500_000.0 * l.intensity,
                range: l.range,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(l.pos.x, l.y, l.pos.z),
            scoped.clone(),
        ));
    }
}

fn actor_shape(a: &Actor, meshes: &mut Assets<Mesh>) -> Option<(Mesh3d, Color, f32, f32)> {
    match (&a.enemy, a.class) {
        (None, Some(class)) => Some((
            Mesh3d(meshes.add(Capsule3d::new(0.5, 1.0))),
            class_color(class),
            1.0,
            0.5,
        )),
        (Some(e), _) => {
            let r = a.radius;
            Some(match e.rank {
                EnemyRank::Trash => (
                    Mesh3d(meshes.add(Capsule3d::new(r, r * 1.2))),
                    if e.summoned {
                        Color::srgb(0.35, 0.3, 0.15)
                    } else {
                        Color::srgb(0.2, 0.35, 0.25)
                    },
                    r * 1.6,
                    r,
                ),
                EnemyRank::Elite => (
                    Mesh3d(meshes.add(Cuboid::new(r * 2.0, r * 2.4, r * 1.6))),
                    Color::srgb(0.45, 0.35, 0.25),
                    r * 1.2,
                    r * 0.8,
                ),
                EnemyRank::Boss => (
                    Mesh3d(meshes.add(Capsule3d::new(r, r * 1.8))),
                    Color::srgb(0.3, 0.15, 0.4),
                    r * 1.9,
                    r,
                ),
            })
        }
        _ => None,
    }
}

/// Spawn visuals and nameplates for actors that don't have them yet (all actors
/// on entering; summoned adds later).
fn spawn_new_actors(
    mut commands: Commands,
    mut run: ResMut<ActiveRun>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let total = run.sim.actors.len();
    if run.visuals_spawned >= total {
        return;
    }
    let scoped = DespawnOnExit(AppState::Dungeon);
    let nose = meshes.add(Cuboid::new(0.3, 0.3, 0.5));
    let nose_mat = materials.add(Color::srgb(0.9, 0.8, 0.3));
    for id in run.visuals_spawned..total {
        let a = &run.sim.actors[id];
        let Some((mesh, color, center_y, lie_y)) = actor_shape(a, &mut meshes) else {
            continue;
        };
        let material = materials.add(StandardMaterial {
            base_color: color,
            emissive: if Some(a.id) == run.sim.player {
                LinearRgba::rgb(0.15, 0.12, 0.02)
            } else {
                LinearRgba::BLACK
            },
            ..default()
        });
        let mut e = commands.spawn((
            mesh,
            MeshMaterial3d(material.clone()),
            Transform::from_translation(to_vec3(a.pos, a.y + center_y))
                .with_rotation(Quat::from_rotation_y(a.facing)),
            ActorVisual {
                id: a.id,
                material,
                base_color: color,
                center_y,
                lie_y,
            },
            scoped.clone(),
        ));
        e.with_child((
            Mesh3d(nose.clone()),
            MeshMaterial3d(nose_mat.clone()),
            Transform::from_xyz(0.0, center_y * 0.4, a.radius + 0.1),
        ));
        if Some(a.id) == run.sim.player {
            e.insert(CameraFocus);
        }
        if a.rank() == Some(EnemyRank::Boss) {
            e.with_child((
                Mesh3d(meshes.add(Torus::new(0.6, 0.9))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.8, 0.6, 0.2),
                    emissive: LinearRgba::rgb(0.8, 0.5, 0.1),
                    ..default()
                })),
                Transform::from_xyz(0.0, center_y + 0.4, 0.0),
            ));
        }
        if Some(a.id) != run.sim.player {
            crate::overlay::spawn_nameplate(&mut commands, a);
        }
    }
    run.visuals_spawned = total;
}

#[allow(clippy::too_many_arguments)]
pub fn player_input(
    keys: Res<ButtonInput<KeyCode>>,
    orbit: Res<OrbitCamera>,
    pc: Res<PlayerControl>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    demo: Option<Res<DemoConfig>>,
    mouseover: Res<crate::hud::Mouseover>,
    mut run: ResMut<ActiveRun>,
) {
    let Some(player) = run.sim.player else {
        return;
    };
    let alive = run.sim.actors[player].alive;
    let dir = pc.output.move_dir;
    let mv = if alive && run.end_timer.is_none() {
        V2::new(dir.x, dir.y)
    } else {
        V2::ZERO
    };
    run.sim.set_player_move(mv);
    // The character faces where the controls say (keyboard turning / mouse steering).
    if !demo.as_ref().is_some_and(|d| d.autoplay) {
        run.sim.set_player_facing(pc.state.facing);
    }

    for (key, slot) in [
        (KeyCode::Digit1, 0),
        (KeyCode::Digit2, 1),
        (KeyCode::Digit3, 2),
        (KeyCode::Digit4, 3),
        (KeyCode::Digit5, 4),
    ] {
        // Friendly spells go to the mouseover ally (if any); the sim ignores it
        // for hostile spells and validates everything as usual.
        if keys.just_pressed(key) {
            run.sim.queue_player_cast_on(slot, mouseover.0);
        }
    }
    if keys.just_pressed(KeyCode::KeyF) {
        run.sim.player_start_attack();
    }
    if keys.just_pressed(KeyCode::Escape) {
        run.sim.set_player_target(None);
    }
    if keys.just_pressed(KeyCode::KeyL) {
        run.verbose_log = !run.verbose_log;
    }
    if keys.just_pressed(KeyCode::Backspace) && run.end_timer.is_none() {
        run.sim.abandon(RunOutcome::Abandoned);
    }
    for (i, key) in [
        KeyCode::F1,
        KeyCode::F2,
        KeyCode::F3,
        KeyCode::F4,
        KeyCode::F5,
    ]
    .into_iter()
    .enumerate()
    {
        if keys.just_pressed(key)
            && let Some(&p) = run.sim.party.get(i)
        {
            run.sim.set_player_target(Some(p));
        }
    }
    if keys.just_pressed(KeyCode::Tab) {
        let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        let me = run.sim.actors[player].pos;
        let fwd = V2::new(orbit.forward().x, orbit.forward().z);
        // Engaged enemies first, then idle ones roughly in front of the camera; nearest first.
        let mut hostiles: Vec<(bool, f32, ActorId)> = run
            .sim
            .enemies
            .iter()
            .copied()
            .filter_map(|e| {
                let a = &run.sim.actors[e];
                let d = a.pos.dist(me);
                let engaged = a.is_engaged_enemy();
                let dir = (a.pos - me).normalize_or_zero();
                let in_front = dir.x * fwd.x + dir.z * fwd.z > -0.2;
                let ok = a.alive
                    && a.mode() != Some(EnemyMode::Resetting)
                    && d < 45.0
                    && (engaged || in_front);
                ok.then_some((!engaged, d, e))
            })
            .collect();
        hostiles.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
        if !hostiles.is_empty() {
            let n = hostiles.len();
            let current = run.sim.actors[player].target;
            let idx = match current.and_then(|c| hostiles.iter().position(|h| h.2 == c)) {
                Some(i) if shift => (i + n - 1) % n,
                Some(i) => (i + 1) % n,
                None => 0,
            };
            run.sim.set_player_target(Some(hostiles[idx].2));
        }
    }
    // T: assist - take the target of your current target (e.g. the tank's enemy).
    if keys.just_pressed(KeyCode::KeyT)
        && let Some(t) = run.sim.actors[player].target
        && let Some(tt) = run.sim.actors[t].target
    {
        run.sim.set_player_target(Some(tt));
    }
    // G: target the party's current kill target.
    if keys.just_pressed(KeyCode::KeyG)
        && let Some(k) = run.sim.board.kill_target
    {
        run.sim.set_player_target(Some(k));
    }
    if let Some(cursor) = pc.world_click
        && let Ok(ray) = camera.0.viewport_to_world(camera.1, cursor)
        && let Some(id) = pick_actor(&run.sim, ray, |_| true)
    {
        run.sim.set_player_target(Some(id));
    }
}

/// Nearest living actor accepted by `filter` whose pick sphere the ray hits.
pub fn pick_actor(sim: &Sim, ray: Ray3d, filter: impl Fn(&Actor) -> bool) -> Option<ActorId> {
    let mut best: Option<(f32, ActorId)> = None;
    for a in sim.actors.iter().filter(|a| a.alive && filter(a)) {
        let center = to_vec3(a.pos, a.y + if a.is_enemy() { a.radius * 1.6 } else { 1.0 });
        let r = a.radius + 0.6;
        let oc = ray.origin - center;
        let b = oc.dot(*ray.direction);
        let c = oc.length_squared() - r * r;
        let disc = b * b - c;
        if disc < 0.0 {
            continue;
        }
        let t = -b - disc.sqrt();
        if t > 0.0 && best.is_none_or(|(bt, _)| t < bt) {
            best = Some((t, a.id));
        }
    }
    best.map(|(_, id)| id)
}

fn step_sim(time: Res<Time>, mut run: ResMut<ActiveRun>, demo: Option<Res<DemoConfig>>) {
    if demo.as_ref().is_some_and(|d| d.area_tour.is_some()) {
        return;
    }
    let speed = demo.map(|d| d.speed).unwrap_or(1.0);
    run.acc += time.delta_secs().min(0.25) * speed;
    let max_steps = (8.0 * speed).ceil() as usize;
    let mut steps = 0;
    while run.acc >= DT && steps < max_steps {
        let positions: Vec<V2> = run.sim.actors.iter().map(|a| a.pos).collect();
        run.prev_pos = positions;
        let ys: Vec<f32> = run.sim.actors.iter().map(|a| a.y).collect();
        run.prev_y = ys;
        run.sim.step();
        run.acc -= DT;
        steps += 1;
    }
    if steps == max_steps {
        run.acc = 0.0;
    }
    let n = run.sim.actors.len();
    if run.prev_pos.len() < n {
        let extra: Vec<V2> = run.sim.actors[run.prev_pos.len()..]
            .iter()
            .map(|a| a.pos)
            .collect();
        run.prev_pos.extend(extra);
        let extra: Vec<f32> = run.sim.actors[run.prev_y.len()..]
            .iter()
            .map(|a| a.y)
            .collect();
        run.prev_y.extend(extra);
    }
    let dt = time.delta_secs();
    if let Some((_, t, _)) = run.banner.as_mut() {
        *t -= dt;
        if *t <= 0.0 {
            run.banner = None;
        }
    }
    if let Some((_, t)) = run.error.as_mut() {
        *t -= dt;
        if *t <= 0.0 {
            run.error = None;
        }
    }
    if let Some((_, t, _)) = run.alert.as_mut() {
        *t -= dt;
        if *t <= 0.0 {
            run.alert = None;
        }
    }
    run.alert_cooldown = (run.alert_cooldown - dt).max(0.0);
}

/// Turn new sim events into log lines, banners and error messages.
pub fn process_events(mut run: ResMut<ActiveRun>) {
    let start = run.events_seen;
    let end = run.sim.events.len();
    run.fresh.clear();
    if start == end {
        return;
    }
    let player = run.sim.player;
    let events: Vec<_> = run.sim.events[start..end].to_vec();
    run.fresh = events.clone();
    for e in &events {
        // The default log skips companions' failed casts and fully overhealed
        // ticks; `L` (verbose) shows everything.
        let noise = match &e.kind {
            EventKind::CastFailed { src, .. } => Some(*src) != player,
            EventKind::Heal { amount, .. } => *amount < 1.0,
            EventKind::CastStart { src, .. } => {
                Some(*src) != player && !run.sim.actors[*src].is_enemy()
            }
            _ => false,
        };
        if (e.is_notable() && !noise) || run.verbose_log {
            let line = run.sim.describe(e);
            run.log.push_back(line);
            while run.log.len() > LOG_LINES {
                run.log.pop_front();
            }
        }
        match &e.kind {
            EventKind::CastFailed { src, reason, what } if Some(*src) == player => {
                // Rate-limited: mashing a key repeats the same message without
                // flicker; a different failure replaces it immediately.
                let msg = format!("{what}: {reason}");
                let fresh = run
                    .error
                    .as_ref()
                    .is_none_or(|(m, t)| *m != msg || *t < ERROR_SECS - 0.4);
                if fresh {
                    run.error = Some((msg, ERROR_SECS));
                }
            }
            // Threat alerts, from the player's point of view: a tank hears about
            // any enemy turning on a non-tank, everyone else only about enemies
            // turning on themselves. At most one alert every 2.5 s.
            EventKind::TargetChange { enemy, target } => {
                if let Some(p) = player
                    && run.sim.actors[*enemy].is_engaged_enemy()
                    && run.alert_cooldown <= 0.0
                {
                    let tank = run.sim.is_tank(p);
                    let e = run.sim.actors[*enemy].name.clone();
                    let msg = if tank && !run.sim.is_tank(*target) {
                        let who = if *target == p {
                            "you".to_string()
                        } else {
                            run.sim.actors[*target].name.clone()
                        };
                        Some(format!("Aggro lost: {e} is attacking {who} - taunt!"))
                    } else if !tank && *target == p {
                        Some(format!("{e} is attacking you!"))
                    } else {
                        None
                    };
                    if let Some(msg) = msg {
                        info!("threat alert at {:.1}s: {msg}", run.sim.time);
                        run.alert = Some((msg, 2.5, Color::srgb(1.0, 0.55, 0.2)));
                        run.alert_cooldown = 2.5;
                    }
                }
            }
            EventKind::Taunt { src, dst } if Some(*src) == player => {
                let e = run.sim.actors[*dst].name.clone();
                info!("threat alert at {:.1}s: Taunted {e}", run.sim.time);
                run.alert = Some((format!("Taunted {e}"), 1.5, Color::srgb(0.5, 1.0, 0.5)));
            }
            EventKind::Interrupt { src, dst, what } if Some(*src) == player => {
                let name = run.sim.actors[*dst].name.clone();
                run.set_banner(
                    format!("You interrupted {name}'s {what}!"),
                    2.0,
                    Color::srgb(0.5, 0.8, 1.0),
                );
            }
            EventKind::PhaseChange {
                boss,
                phase,
                announce,
            } => {
                let name = run.sim.actors[*boss].name.clone();
                let text = if announce.is_empty() {
                    format!("PHASE {phase}: {name}")
                } else {
                    format!("PHASE {phase}: {announce}")
                };
                run.set_banner(text, 5.0, Color::srgb(1.0, 0.45, 0.1));
            }
            EventKind::Summon { src, what, count } => {
                let name = run.sim.actors[*src].name.clone();
                run.set_banner(
                    format!("{name}: {what}! ({count} adds) - switch targets"),
                    3.0,
                    Color::srgb(1.0, 0.7, 0.3),
                );
            }
            EventKind::Enrage { boss } => {
                let name = run.sim.actors[*boss].name.clone();
                run.set_banner(
                    format!("{name} is ENRAGED!"),
                    5.0,
                    Color::srgb(1.0, 0.1, 0.1),
                );
            }
            EventKind::CastStart { src, what, .. } => {
                // Warn about party-wide pulses.
                let a = &run.sim.actors[*src];
                if let (Some(es), Some(c)) = (&a.enemy, &a.cast)
                    && let dungeon_sim::sim::CastKind::Enemy(idx) = c.kind
                    && es.abilities[idx].cfg.kind == dungeon_sim::content::AbilityKind::Pulse
                {
                    let name = a.name.clone();
                    let interrupt = if c.interruptible {
                        " - INTERRUPT IT"
                    } else {
                        " - brace!"
                    };
                    run.set_banner(
                        format!("{name} is casting {what}{interrupt}"),
                        2.5,
                        Color::srgb(0.8, 0.5, 1.0),
                    );
                }
            }
            EventKind::TelegraphStart {
                center,
                radius,
                what,
                ..
            } => {
                if let Some(p) = player
                    && run.sim.actors[p].pos.dist(*center) <= radius + 0.5
                {
                    run.set_banner(
                        format!("{} UNDER YOU - MOVE!", what.to_uppercase()),
                        2.0,
                        Color::srgb(1.0, 0.2, 0.1),
                    );
                }
            }
            EventKind::Death { actor } if Some(*actor) == player => {
                run.set_banner(
                    "You have died. Your companions fight on...",
                    4.0,
                    Color::srgb(0.8, 0.2, 0.2),
                );
            }
            EventKind::EncounterCleared { encounter } => {
                let name = run.sim.encounters[*encounter].name.clone();
                run.set_banner(
                    format!("Encounter cleared: {name}"),
                    3.0,
                    Color::srgb(0.6, 1.0, 0.6),
                );
            }
            EventKind::Outcome { outcome } => {
                let (text, color) = match outcome {
                    RunOutcome::Victory => (
                        format!("VICTORY! {} falls.", run.sim.actors[run.sim.boss].name),
                        Color::srgb(1.0, 0.85, 0.3),
                    ),
                    RunOutcome::Wipe => (
                        "WIPE - the party has fallen.".to_string(),
                        Color::srgb(0.9, 0.2, 0.2),
                    ),
                    _ => (
                        "You leave the dungeon.".to_string(),
                        Color::srgb(0.8, 0.8, 0.8),
                    ),
                };
                run.set_banner(text, END_DELAY + 1.0, color);
                run.end_timer = Some(END_DELAY);
            }
            _ => {}
        }
    }
    run.events_seen = end;
}

fn sync_actors(
    run: Res<ActiveRun>,
    orbit: Res<OrbitCamera>,
    mut q: Query<(&ActorVisual, &mut Transform, &mut Visibility)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // In tight spaces the camera can be pulled right up to the character:
    // hide your own model rather than filling the screen with it.
    let too_close = orbit.current < 2.0;
    for (vis, mut t, mut visibility) in &mut q {
        let a = &run.sim.actors[vis.id];
        let despawned = a.enemy.as_ref().is_some_and(|e| e.despawned);
        let hidden_self = too_close && run.sim.player == Some(vis.id);
        *visibility = if despawned || hidden_self {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if a.alive {
            t.translation = run.world(vis.id, vis.center_y);
            t.rotation = Quat::from_rotation_y(a.facing);
        } else {
            t.translation = run.world(vis.id, vis.lie_y);
            t.rotation = Quat::from_rotation_y(a.facing) * Quat::from_rotation_x(FRAC_PI_2);
        }
        let is_boss = a.rank() == Some(EnemyRank::Boss);
        let phase = a.phase();
        t.scale = Vec3::splat(if is_boss && a.alive {
            1.0 + 0.1 * (phase - 1) as f32
        } else {
            1.0
        });
        let Some(mut m) = materials.get_mut(&vis.material) else {
            continue;
        };
        let want = if !a.alive {
            Color::srgb(0.15, 0.15, 0.15)
        } else if a.is_incapacitated() {
            Color::srgb(0.6, 0.4, 0.9)
        } else if a.mode() == Some(EnemyMode::Resetting) {
            vis.base_color.mix(&Color::WHITE, 0.5)
        } else {
            vis.base_color
        };
        if m.base_color != want {
            m.base_color = want;
        }
        if is_boss {
            let e = match (a.alive, phase) {
                (true, 2) => LinearRgba::rgb(2.5, 0.6, 0.05),
                (true, p) if p >= 3 => LinearRgba::rgb(2.5, 0.1, 0.4),
                _ => LinearRgba::BLACK,
            };
            if m.emissive != e {
                m.emissive = e;
            }
        }
    }
}

fn sync_telegraphs(
    mut commands: Commands,
    run: Res<ActiveRun>,
    mut q: Query<(Entity, &TelegraphVisual, &mut Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut known: Local<HashMap<u32, ()>>,
) {
    let live: HashMap<u32, &dungeon_sim::sim::Telegraph> =
        run.sim.telegraphs.iter().map(|t| (t.id, t)).collect();
    for (e, vis, mut t) in &mut q {
        match live.get(&vis.id) {
            Some(tg) => {
                let progress = (tg.elapsed() / tg.total).clamp(0.0, 1.0);
                let r = if vis.inner {
                    tg.radius * progress
                } else {
                    tg.radius
                };
                t.scale = Vec3::new(r.max(0.01), r.max(0.01), 1.0);
            }
            None => {
                commands.entity(e).despawn();
                known.remove(&vis.id);
            }
        }
    }
    for tg in &run.sim.telegraphs {
        if known.contains_key(&tg.id) {
            continue;
        }
        known.insert(tg.id, ());
        let disc = meshes.add(Circle::new(1.0));
        for (inner, color, y) in [
            (false, Color::srgba(1.0, 0.3, 0.05, 0.25), 0.05),
            (true, Color::srgba(1.0, 0.15, 0.0, 0.55), 0.06),
        ] {
            commands.spawn((
                Mesh3d(disc.clone()),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: color,
                    emissive: LinearRgba::rgb(1.5, 0.3, 0.0),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    ..default()
                })),
                Transform::from_translation(to_vec3(tg.center, tg.y + y))
                    .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
                    .with_scale(Vec3::new(0.01, 0.01, 1.0)),
                TelegraphVisual { id: tg.id, inner },
                DespawnOnExit(AppState::Dungeon),
            ));
        }
    }
}

/// Hazards glow bright when active and flicker shortly before turning on.
fn sync_hazards(
    time: Res<Time>,
    run: Res<ActiveRun>,
    q: Query<&HazardVisual>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for v in &q {
        let h = &run.sim.hazards[v.index];
        let cleared = run.sim.encounters[h.encounter].cleared;
        let soon = h.time_to_active(run.sim.time);
        let alpha = if cleared {
            0.0
        } else if h.active {
            0.55
        } else if soon < 1.0 {
            0.2 + 0.2 * (time.elapsed_secs() * 20.0).sin().abs()
        } else {
            0.12
        };
        if let Some(mut m) = materials.get_mut(&v.material) {
            m.base_color = Color::srgba(1.0, 0.35, 0.05, alpha);
            m.emissive = if h.active {
                LinearRgba::rgb(2.0, 0.5, 0.0)
            } else {
                LinearRgba::rgb(0.2, 0.05, 0.0)
            };
        }
    }
}

fn draw_gizmos(run: Res<ActiveRun>, mouseover: Res<crate::hud::Mouseover>, mut gizmos: Gizmos) {
    let flat = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
    // Who a friendly spell would land on right now (mouseover ally).
    if let Some(m) = mouseover.0 {
        let a = &run.sim.actors[m];
        gizmos.circle(
            Isometry3d::new(run.world(m, 0.06), flat),
            a.radius + 0.55,
            Color::srgb(0.45, 0.8, 1.0),
        );
    }
    if let Some(p) = run.sim.player {
        gizmos.circle(
            Isometry3d::new(run.world(p, 0.05), flat),
            0.8,
            Color::srgb(0.3, 1.0, 0.4),
        );
    }
    for tg in &run.sim.telegraphs {
        gizmos.circle(
            Isometry3d::new(to_vec3(tg.center, tg.y + 0.08), flat),
            tg.radius,
            Color::srgb(1.0, 0.2, 0.0),
        );
    }
    for h in &run.sim.hazards {
        if !run.sim.encounters[h.encounter].cleared {
            gizmos.circle(
                Isometry3d::new(to_vec3(h.pos.xz(), h.pos.y + 0.07), flat),
                h.cfg.radius,
                if h.active {
                    Color::srgb(1.0, 0.4, 0.0)
                } else {
                    Color::srgba(1.0, 0.5, 0.2, 0.35)
                },
            );
        }
    }
    // Frontal cleave cones on engaged enemies that have one.
    for &e in &run.sim.board.frontal {
        let a = &run.sim.actors[e];
        let pos = run.world(e, 0.1);
        let r = a.radius + 6.0;
        for k in -4..=4 {
            let ang = a.facing + k as f32 * std::f32::consts::FRAC_PI_8;
            let dir = Vec3::new(ang.sin(), 0.0, ang.cos());
            gizmos.line(pos, pos + dir * r, Color::srgba(1.0, 0.3, 0.2, 0.35));
        }
    }
    if let Some(p) = run.sim.player {
        let me = run.sim.actors[p].pos;
        for &e in &run.sim.enemies {
            let a = &run.sim.actors[e];
            if let Some(es) = &a.enemy
                && a.alive
                && es.mode == EnemyMode::Idle
                && a.pos.dist(me) < es.aggro_radius + 10.0
            {
                gizmos.circle(
                    Isometry3d::new(to_vec3(a.pos, a.y + 0.04), flat),
                    es.aggro_radius,
                    Color::srgba(1.0, 0.8, 0.2, 0.25),
                );
            }
        }
    }
}

fn finish_run(
    mut commands: Commands,
    time: Res<Time>,
    mut run: ResMut<ActiveRun>,
    mut data: ResMut<GameData>,
    mut next: ResMut<NextState<AppState>>,
) {
    let Some(t) = run.end_timer.as_mut() else {
        return;
    };
    *t -= time.delta_secs();
    if *t > 0.0 {
        return;
    }
    let sim = &run.sim;
    let summary = RunSummary::from_sim(sim);
    let content = sim.content.clone();
    let report = RunReport::from_sim(sim);
    let rewards = data.profile.grant_rewards(&content, &report);
    data.save();
    let log_path = std::path::Path::new("logs").join("last_run.log");
    let text: String = sim.events.iter().map(|e| sim.describe(e) + "\n").collect();
    let log_path = std::fs::create_dir_all("logs")
        .and_then(|_| std::fs::write(&log_path, text))
        .map(|_| log_path.display().to_string())
        .ok();
    let highlights: Vec<String> = sim
        .events
        .iter()
        .filter(|e| match &e.kind {
            EventKind::Death { actor } => {
                !sim.actors[*actor].is_enemy()
                    || sim.actors[*actor].rank() != Some(EnemyRank::Trash)
            }
            EventKind::PhaseChange { .. }
            | EventKind::EncounterCleared { .. }
            | EventKind::EncounterReset { .. }
            | EventKind::Enrage { .. }
            | EventKind::Revive { .. }
            | EventKind::Outcome { .. } => true,
            _ => false,
        })
        .map(|e| sim.describe(e))
        .collect();
    info!(
        "Run finished: {:?} in {:.1}s",
        summary.outcome, summary.duration
    );
    commands.insert_resource(LastResult {
        summary,
        rewards,
        dungeon_name: content.dungeon.name.clone(),
        difficulty_name: sim.difficulty.name.clone(),
        highlights,
        log_path,
    });
    run.end_timer = None;
    next.set(AppState::Results);
}

/// Fade walls that sit between the camera and the player so the action stays visible.
fn fade_occluders(
    run: Res<ActiveRun>,
    camera: Single<&GlobalTransform, With<MainCamera>>,
    walls: Query<&Occluder>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(p) = run.sim.player else { return };
    let cam = camera.translation();
    let points = [run.world(p, 0.3), run.world(p, 1.8)];
    for w in &walls {
        let blocks = points.iter().any(|&target| {
            let d = target - cam;
            let len = d.length();
            crate::camera::ray_box(cam, d / len, w.min, w.max).is_some_and(|t| t < len)
        });
        let alpha = if blocks { 0.22 } else { 1.0 };
        if let Some(m) = materials.get(&w.material)
            && (m.base_color.alpha() - alpha).abs() > 0.01
            && let Some(mut m) = materials.get_mut(&w.material)
        {
            m.base_color = run.wall_color.with_alpha(alpha);
        }
    }
}
