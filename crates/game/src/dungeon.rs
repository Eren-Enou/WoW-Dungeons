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
            |mut c: Commands, mut blockers: ResMut<CameraBlockers>| {
                c.remove_resource::<ActiveRun>();
                blockers.0.clear();
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
    run.set_banner(
        format!(
            "{} - {} encounters, final boss: {boss_name}",
            content.dungeon.name, required
        ),
        5.0,
        Color::srgb(0.9, 0.85, 0.6),
    );
    data.message = None;
    commands.insert_resource(run);
}

fn spawn_world(
    mut commands: Commands,
    run: Res<ActiveRun>,
    mut blockers: ResMut<CameraBlockers>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let scoped = DespawnOnExit(AppState::Dungeon);
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.6, 0.65, 0.8),
        brightness: 220.0,
        ..default()
    });
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

/// Build floors, ramps, stairs, bridges, rock supports, walls, drop markers and
/// lava from the simulation's `Nav`, so what you see is what the sim walks on.
fn spawn_level_geometry(
    commands: &mut Commands,
    run: &ActiveRun,
    blockers: &mut CameraBlockers,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    use dungeon_sim::nav::{Axis, LinkKind, SLAB};
    let scoped = DespawnOnExit(AppState::Dungeon);
    let nav = &run.sim.nav;
    let theme = run.sim.content.dungeon.theme;
    let [fr, fg, fb] = theme.floor;
    // Floors get lighter with height so levels read at a glance.
    let (lo, hi) = nav
        .surfaces
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), s| {
            (lo.min(s.min_y()), hi.max(s.max_y()))
        });
    let shade = |y: f32| {
        let t = if hi > lo { (y - lo) / (hi - lo) } else { 0.0 };
        Color::srgb(fr + 0.14 * t, fg + 0.12 * t, fb + 0.08 * t)
    };
    let rock = materials.add(Color::srgb(fr * 0.55, fg * 0.55, fb * 0.6));
    let mut boxes: Vec<(Vec3, Vec3, Handle<StandardMaterial>, bool)> = Vec::new();
    blockers.0.clear();
    for (i, s) in nav.surfaces.iter().enumerate() {
        let r = s.def.rect;
        let size = r.size();
        let c = r.center();
        let tint = if i % 2 == 0 { 0.0 } else { 0.025 };
        let top_mat = materials.add(if s.def.open_under {
            Color::srgb(0.42, 0.3, 0.2)
        } else {
            let base = shade(s.max_y());
            base.mix(&Color::srgb(fr + tint, fg + tint, fb), 0.3)
        });
        let base = if s.def.open_under {
            None
        } else {
            Some(nav.base_y)
        };
        if !s.is_ramp() {
            let y = s.def.y;
            let bottom = base.unwrap_or(y - SLAB);
            // Top slab (slight per-area offset avoids z-fighting on overlaps).
            let top_bottom = y - 0.2;
            boxes.push((
                Vec3::new(r.min.x, top_bottom - i as f32 * 0.001, r.min.z),
                Vec3::new(r.max.x, y - i as f32 * 0.001, r.max.z),
                top_mat.clone(),
                false,
            ));
            if bottom < top_bottom - 0.01 {
                boxes.push((
                    Vec3::new(r.min.x, bottom, r.min.z),
                    Vec3::new(r.max.x, top_bottom, r.max.z),
                    if s.def.open_under {
                        top_mat.clone()
                    } else {
                        rock.clone()
                    },
                    false,
                ));
            }
            blockers.0.push((
                Vec3::new(r.min.x, bottom, r.min.z),
                Vec3::new(r.max.x, y - 0.05, r.max.z),
            ));
        } else {
            // Ramps and stairs: steps of at most 0.5 m along the slope; a ramp
            // also gets a smooth sloped deck on top.
            let len = match s.def.axis {
                Axis::Z => size.z,
                Axis::X => size.x,
            };
            let n = ((len / 0.5).ceil() as usize).max(1);
            let step = len / n as f32;
            for k in 0..n {
                let a0 = k as f32 * step;
                let a1 = a0 + step;
                let (min, max) = match s.def.axis {
                    Axis::Z => (
                        V2::new(r.min.x, r.min.z + a0),
                        V2::new(r.max.x, r.min.z + a1),
                    ),
                    Axis::X => (
                        V2::new(r.min.x + a0, r.min.z),
                        V2::new(r.min.x + a1, r.max.z),
                    ),
                };
                let mid = (min + max) * 0.5;
                let h = s.height_at(mid);
                let top = if s.def.stairs { h } else { h - 0.15 };
                let bottom = base.unwrap_or(top - SLAB);
                boxes.push((
                    Vec3::new(min.x, bottom, min.z),
                    Vec3::new(max.x, top, max.z),
                    if s.def.stairs {
                        top_mat.clone()
                    } else {
                        rock.clone()
                    },
                    false,
                ));
                blockers.0.push((
                    Vec3::new(min.x, bottom, min.z),
                    Vec3::new(max.x, top - 0.1, max.z),
                ));
            }
            if !s.def.stairs {
                let y0 = s.height_at(r.min);
                let y1 = s.height_at(r.max);
                let rise = y1 - y0;
                let (deck, rot) = match s.def.axis {
                    Axis::Z => {
                        let l = (len * len + rise * rise).sqrt();
                        (
                            Cuboid::new(size.x, 0.12, l),
                            Quat::from_rotation_x(-(rise / len).atan()),
                        )
                    }
                    Axis::X => {
                        let l = (len * len + rise * rise).sqrt();
                        (
                            Cuboid::new(l, 0.12, size.z),
                            Quat::from_rotation_z((rise / len).atan()),
                        )
                    }
                };
                commands.spawn((
                    Mesh3d(meshes.add(deck)),
                    MeshMaterial3d(top_mat.clone()),
                    Transform::from_xyz(c.x, (y0 + y1) * 0.5 - 0.04, c.z).with_rotation(rot),
                    scoped.clone(),
                ));
            }
        }
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
    for (min, max, mat, _) in boxes {
        let size = max - min;
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
            MeshMaterial3d(mat),
            Transform::from_translation((min + max) * 0.5),
            scoped.clone(),
        ));
    }
    // Walls follow the floor they stand on (in short pieces along ramps).
    for w in &nav.walls {
        let len = w.a.dist(w.b);
        let pieces = if (w.bottom0 - w.bottom1).abs() > 0.05 {
            (len / 1.0).ceil().max(1.0) as usize
        } else {
            1
        };
        for k in 0..pieces {
            let f0 = k as f32 / pieces as f32;
            let f1 = (k + 1) as f32 / pieces as f32;
            let a = w.a + (w.b - w.a) * f0;
            let b = w.a + (w.b - w.a) * f1;
            let bottom = w.bottom0 + (w.bottom1 - w.bottom0) * (f0 + f1) * 0.5;
            let plen = a.dist(b);
            let mid = (a + b) * 0.5;
            let horizontal = (a.z - b.z).abs() < 1e-3;
            let (sx, sz) = if horizontal {
                (plen + 0.4, 0.4)
            } else {
                (0.4, plen + 0.4)
            };
            let half = Vec3::new(sx * 0.5, w.height * 0.5, sz * 0.5);
            let center = Vec3::new(mid.x, bottom + w.height * 0.5, mid.z);
            blockers.0.push((center - half, center + half));
            let material = materials.add(StandardMaterial {
                base_color: run.wall_color,
                alpha_mode: AlphaMode::Blend,
                ..default()
            });
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(sx, w.height, sz))),
                MeshMaterial3d(material.clone()),
                Occluder {
                    material,
                    min: center - half,
                    max: center + half,
                },
                Transform::from_translation(center),
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
        let p = l.portal;
        let size = p.size();
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(size.x.max(0.2), 0.04, size.z.max(0.2)))),
            MeshMaterial3d(drop_mat.clone()),
            Transform::from_xyz(p.center().x, l.entry.y + 0.03, p.center().z),
            scoped.clone(),
        ));
    }
    // Lava pools (decorative; not walkable).
    let lava = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.35, 0.05),
        emissive: LinearRgba::rgb(4.0, 1.0, 0.1),
        ..default()
    });
    for l in &run.sim.content.dungeon.lava {
        let size = l.max - l.min;
        let c = (l.min + l.max) * 0.5;
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(size.x, 0.1, size.z))),
            MeshMaterial3d(lava.clone()),
            Transform::from_xyz(c.x, l.y - 0.05, c.z),
            scoped.clone(),
        ));
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.45, 0.1),
                intensity: 400_000.0,
                range: 14.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(c.x, l.y + 1.5, c.z),
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
