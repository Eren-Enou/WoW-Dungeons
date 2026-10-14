//! Dungeon state: owns the running `Sim`, maps input to sim commands at a fixed
//! timestep, and renders actors, walls, telegraphs and health bars.

use crate::camera::{CameraFocus, MainCamera, OrbitCamera};
use crate::common::{AppState, GameData, LastResult, to_vec3};
use crate::demo::DemoConfig;
use crate::hub::class_color;
use bevy::prelude::*;
use dungeon_sim::content::EnemyRank;
use dungeon_sim::{ActorId, DT, EnemyMode, EventKind, RunOutcome, RunSummary, Sim, Team, V2};
use std::collections::{HashMap, VecDeque};
use std::f32::consts::FRAC_PI_2;

pub struct DungeonPlugin;

impl Plugin for DungeonPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(AppState::Dungeon),
            (start_run, spawn_world, spawn_actors, crate::hud::spawn_hud).chain(),
        )
        .add_systems(OnExit(AppState::Dungeon), |mut c: Commands| {
            c.remove_resource::<ActiveRun>()
        })
        .add_systems(
            Update,
            (
                player_input,
                step_sim,
                process_events,
                sync_actors,
                sync_bars,
                sync_telegraphs,
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
const LOG_LINES: usize = 14;

#[derive(Resource)]
pub struct ActiveRun {
    pub sim: Sim,
    pub acc: f32,
    pub prev_pos: Vec<V2>,
    pub events_seen: usize,
    pub log: VecDeque<String>,
    pub verbose_log: bool,
    pub banner: Option<(String, f32, Color)>,
    pub error: Option<(String, f32)>,
    pub end_timer: Option<f32>,
}

impl ActiveRun {
    pub fn alpha(&self) -> f32 {
        (self.acc / DT).clamp(0.0, 1.0)
    }

    pub fn interp_pos(&self, id: ActorId) -> V2 {
        let a = self.prev_pos[id];
        let b = self.sim.actors[id].pos;
        a + (b - a) * self.alpha()
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

#[derive(Component)]
struct BarRoot {
    id: ActorId,
    height: f32,
}

#[derive(Component)]
struct BarFill {
    id: ActorId,
}

#[derive(Component)]
struct TelegraphVisual {
    id: u32,
    inner: bool,
}

fn start_run(
    mut commands: Commands,
    mut data: ResMut<GameData>,
    mut orbit: ResMut<OrbitCamera>,
    demo: Option<Res<DemoConfig>>,
) {
    let content = data
        .content
        .clone()
        .expect("hub only enters with valid content");
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(1)
        % 1_000_000;
    let party = data.profile.party_specs(true);
    let mut sim = Sim::new(&content, &party, seed);
    if demo.as_ref().is_some_and(|d| d.autoplay)
        && let Some(p) = sim.player
    {
        // Demo: the AI drives the player's slot and the tank leads the way.
        sim.actors[p].controller = dungeon_sim::sim::Controller::Ai;
        sim.autopilot = true;
        if let Some(t) = sim.tank() {
            sim.leader = t;
        }
    }
    info!(
        "Entering '{}' with seed {seed} and {} party members",
        content.dungeon.name,
        party.len()
    );
    orbit.yaw = 0.0;
    let prev_pos = sim.actors.iter().map(|a| a.pos).collect();
    let mut run = ActiveRun {
        sim,
        acc: 0.0,
        prev_pos,
        events_seen: 0,
        log: VecDeque::new(),
        verbose_log: false,
        banner: None,
        error: None,
        end_timer: None,
    };
    run.set_banner(
        format!(
            "{} - clear 3 packs, then defeat the Hollow King",
            content.dungeon.name
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
    let nav = &run.sim.nav;
    for (i, r) in nav.areas.iter().enumerate() {
        let size = r.max - r.min;
        let c = r.center();
        let tint = if i % 2 == 0 { 0.0 } else { 0.03 };
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(size.x, 0.2, size.z))),
            MeshMaterial3d(materials.add(Color::srgb(0.22 + tint, 0.24 + tint, 0.27))),
            Transform::from_xyz(c.x, -0.1 - i as f32 * 0.001, c.z),
            scoped.clone(),
        ));
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.75, 0.45),
                intensity: 600_000.0,
                range: 35.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(c.x, 7.0, c.z),
            scoped.clone(),
        ));
    }
    let wall_mat = materials.add(Color::srgb(0.32, 0.3, 0.34));
    for (a, b) in nav.wall_segments() {
        let len = a.dist(b);
        let mid = (a + b) * 0.5;
        let horizontal = (a.z - b.z).abs() < 1e-3;
        let (sx, sz) = if horizontal {
            (len + 0.4, 0.4)
        } else {
            (0.4, len + 0.4)
        };
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(sx, 1.2, sz))),
            MeshMaterial3d(wall_mat.clone()),
            Transform::from_xyz(mid.x, 0.6, mid.z),
            scoped.clone(),
        ));
    }
    // Exit portal at the entrance, and a throne behind the boss.
    let entrance = run.sim.content.dungeon.entrance;
    commands.spawn((
        Mesh3d(meshes.add(Torus::new(1.2, 1.6))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.1, 0.5),
            emissive: LinearRgba::rgb(1.5, 0.5, 3.0),
            ..default()
        })),
        Transform::from_translation(to_vec3(entrance, 1.8) + Vec3::new(0.0, 0.0, -5.0))
            .with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
        scoped.clone(),
    ));
    let boss_home = run.sim.actors[run.sim.boss].pos;
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(5.0, 6.0, 2.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.18, 0.1, 0.22))),
        Transform::from_translation(to_vec3(boss_home, 3.0) + Vec3::new(0.0, 0.0, 6.0)),
        scoped,
    ));
}

fn spawn_actors(
    mut commands: Commands,
    run: Res<ActiveRun>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let scoped = DespawnOnExit(AppState::Dungeon);
    let bar_bg = meshes.add(Cuboid::new(1.6, 0.18, 0.05));
    let bar_bg_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.05, 0.05, 0.05),
        unlit: true,
        ..default()
    });
    let nose = meshes.add(Cuboid::new(0.3, 0.3, 0.5));
    let nose_mat = materials.add(Color::srgb(0.9, 0.8, 0.3));
    for a in &run.sim.actors {
        let (mesh, color, center_y, lie_y, height): (Mesh3d, Color, f32, f32, f32) =
            match (&a.enemy, a.class) {
                (None, Some(class)) => (
                    Mesh3d(meshes.add(Capsule3d::new(0.5, 1.0))),
                    class_color(class),
                    1.0,
                    0.5,
                    2.0,
                ),
                (Some(e), _) => {
                    let r = a.radius;
                    match e.rank {
                        EnemyRank::Trash => (
                            Mesh3d(meshes.add(Capsule3d::new(r, r * 1.2))),
                            Color::srgb(0.2, 0.35, 0.25),
                            r * 1.6,
                            r,
                            r * 3.2,
                        ),
                        EnemyRank::Elite => (
                            Mesh3d(meshes.add(Cuboid::new(r * 2.0, r * 2.4, r * 1.6))),
                            Color::srgb(0.45, 0.35, 0.25),
                            r * 1.2,
                            r * 0.8,
                            r * 2.4,
                        ),
                        EnemyRank::Boss => (
                            Mesh3d(meshes.add(Capsule3d::new(r, r * 1.8))),
                            Color::srgb(0.3, 0.15, 0.4),
                            r * 1.9,
                            r,
                            r * 3.8,
                        ),
                    }
                }
                _ => continue,
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
            Transform::from_translation(to_vec3(a.pos, center_y))
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
        if a.enemy.as_ref().is_some_and(|e| e.rank == EnemyRank::Boss) {
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
        // Floating health bar.
        let fill_color = match a.team {
            Team::Party => Color::srgb(0.2, 0.85, 0.3),
            Team::Enemy => Color::srgb(0.9, 0.2, 0.15),
        };
        commands
            .spawn((
                Transform::from_translation(to_vec3(a.pos, height + 0.5)),
                Visibility::default(),
                BarRoot {
                    id: a.id,
                    height: height + 0.5,
                },
                scoped.clone(),
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(bar_bg.clone()),
                    MeshMaterial3d(bar_bg_mat.clone()),
                    Transform::default(),
                ));
                p.spawn((
                    Mesh3d(meshes.add(Cuboid::new(1.5, 0.12, 0.06))),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: fill_color,
                        unlit: true,
                        ..default()
                    })),
                    Transform::from_xyz(0.0, 0.0, 0.0),
                    BarFill { id: a.id },
                ));
            });
    }
}

pub fn player_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    orbit: Res<OrbitCamera>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut run: ResMut<ActiveRun>,
) {
    let Some(player) = run.sim.player else {
        return;
    };
    let mut dir = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        dir += orbit.forward();
    }
    if keys.pressed(KeyCode::KeyS) {
        dir -= orbit.forward();
    }
    if keys.pressed(KeyCode::KeyD) {
        dir += orbit.right();
    }
    if keys.pressed(KeyCode::KeyA) {
        dir -= orbit.right();
    }
    let alive = run.sim.actors[player].alive;
    let mv = if alive && run.end_timer.is_none() {
        V2::new(dir.x, dir.z)
    } else {
        V2::ZERO
    };
    run.sim.set_player_move(mv);
    if mv.len_sq() > 0.0 {
        run.sim.set_player_facing(mv.heading());
    } else if let Some(t) = run.sim.actors[player].target {
        let d = run.sim.actors[t].pos - run.sim.actors[player].pos;
        if d.len_sq() > 0.01 && run.sim.is_hostile(player, t) {
            run.sim.set_player_facing(d.heading());
        }
    }

    for (key, slot) in [
        (KeyCode::Digit1, 0),
        (KeyCode::Digit2, 1),
        (KeyCode::Digit3, 2),
    ] {
        if keys.just_pressed(key) {
            run.sim.queue_player_cast(slot);
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
    // F1-F5: target party members.
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
        let me = run.sim.actors[player].pos;
        let mut hostiles: Vec<(f32, ActorId)> = run
            .sim
            .enemies
            .iter()
            .copied()
            .filter(|&e| {
                let a = &run.sim.actors[e];
                a.alive && a.mode() != Some(EnemyMode::Resetting) && a.pos.dist(me) < 45.0
            })
            .map(|e| (run.sim.actors[e].pos.dist(me), e))
            .collect();
        hostiles.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        if !hostiles.is_empty() {
            let current = run.sim.actors[player].target;
            let idx = current
                .and_then(|c| hostiles.iter().position(|(_, e)| *e == c))
                .map(|i| (i + 1) % hostiles.len())
                .unwrap_or(0);
            run.sim.set_player_target(Some(hostiles[idx].1));
        }
    }
    if mouse.just_pressed(MouseButton::Left)
        && let Some(cursor) = window.cursor_position()
        && let Ok(ray) = camera.0.viewport_to_world(camera.1, cursor)
    {
        let mut best: Option<(f32, ActorId)> = None;
        for a in run.sim.actors.iter().filter(|a| a.alive) {
            let center = to_vec3(a.pos, if a.is_enemy() { a.radius * 1.6 } else { 1.0 });
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
        if let Some((_, id)) = best {
            run.sim.set_player_target(Some(id));
        }
    }
}

fn step_sim(time: Res<Time>, mut run: ResMut<ActiveRun>, demo: Option<Res<DemoConfig>>) {
    let speed = demo.map(|d| d.speed).unwrap_or(1.0);
    run.acc += time.delta_secs().min(0.25) * speed;
    let max_steps = (8.0 * speed).ceil() as usize;
    let mut steps = 0;
    while run.acc >= DT && steps < max_steps {
        let positions: Vec<V2> = run.sim.actors.iter().map(|a| a.pos).collect();
        run.prev_pos = positions;
        run.sim.step();
        run.acc -= DT;
        steps += 1;
    }
    if steps == max_steps {
        run.acc = 0.0;
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
}

/// Turn new sim events into log lines, banners and error messages.
fn process_events(mut run: ResMut<ActiveRun>) {
    let start = run.events_seen;
    let end = run.sim.events.len();
    if start == end {
        return;
    }
    let player = run.sim.player;
    let events: Vec<_> = run.sim.events[start..end].to_vec();
    for e in &events {
        if e.is_notable() || run.verbose_log {
            let line = run.sim.describe(e);
            run.log.push_back(line);
            while run.log.len() > LOG_LINES {
                run.log.pop_front();
            }
        }
        match &e.kind {
            EventKind::CastFailed { src, reason, .. } if Some(*src) == player => {
                run.error = Some((reason.to_string(), 1.5));
            }
            EventKind::PhaseChange { boss, phase } => {
                let name = run.sim.actors[*boss].name.clone();
                run.set_banner(
                    format!("PHASE {phase}: {name} erupts in cinders! Watch the ground!"),
                    5.0,
                    Color::srgb(1.0, 0.45, 0.1),
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
            EventKind::TelegraphStart { center, radius, .. } => {
                if let Some(p) = player
                    && run.sim.actors[p].pos.dist(*center) <= radius + 0.5
                {
                    run.set_banner(
                        "CINDER RING UNDER YOU - MOVE!",
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
                let id = run.sim.encounters[*encounter].id.clone();
                run.set_banner(
                    format!("Encounter cleared: {id}"),
                    3.0,
                    Color::srgb(0.6, 1.0, 0.6),
                );
            }
            EventKind::Outcome { outcome } => {
                let (text, color) = match outcome {
                    RunOutcome::Victory => (
                        "VICTORY! The Hollow King falls.",
                        Color::srgb(1.0, 0.85, 0.3),
                    ),
                    RunOutcome::Wipe => {
                        ("WIPE - the party has fallen.", Color::srgb(0.9, 0.2, 0.2))
                    }
                    _ => ("You leave the dungeon.", Color::srgb(0.8, 0.8, 0.8)),
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
    mut q: Query<(&ActorVisual, &mut Transform)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let phase2 = run.sim.boss_phase() == 2;
    for (vis, mut t) in &mut q {
        let a = &run.sim.actors[vis.id];
        let pos = run.interp_pos(vis.id);
        if a.alive {
            t.translation = to_vec3(pos, vis.center_y);
            t.rotation = Quat::from_rotation_y(a.facing);
        } else {
            t.translation = to_vec3(pos, vis.lie_y);
            t.rotation = Quat::from_rotation_y(a.facing) * Quat::from_rotation_x(FRAC_PI_2);
        }
        let is_boss = vis.id == run.sim.boss;
        t.scale = Vec3::splat(if is_boss && phase2 && a.alive {
            1.2
        } else {
            1.0
        });
        let Some(mut m) = materials.get_mut(&vis.material) else {
            continue;
        };
        let want = if !a.alive {
            Color::srgb(0.15, 0.15, 0.15)
        } else if a.mode() == Some(EnemyMode::Resetting) {
            vis.base_color.mix(&Color::WHITE, 0.5)
        } else {
            vis.base_color
        };
        if m.base_color != want {
            m.base_color = want;
        }
        if is_boss {
            let e = if phase2 && a.alive {
                LinearRgba::rgb(2.5, 0.6, 0.05)
            } else {
                LinearRgba::BLACK
            };
            if m.emissive != e {
                m.emissive = e;
            }
        }
    }
}

fn sync_bars(
    run: Res<ActiveRun>,
    orbit: Res<OrbitCamera>,
    mut roots: Query<(&BarRoot, &mut Transform, &mut Visibility), Without<BarFill>>,
    mut fills: Query<(&BarFill, &mut Transform), Without<BarRoot>>,
) {
    let facing = Quat::from_rotation_y(orbit.yaw);
    for (bar, mut t, mut vis) in &mut roots {
        let a = &run.sim.actors[bar.id];
        let scale = if bar.id == run.sim.boss { 1.8 } else { 1.0 };
        t.translation = to_vec3(
            run.interp_pos(bar.id),
            bar.height
                * if bar.id == run.sim.boss && run.sim.boss_phase() == 2 {
                    1.2
                } else {
                    1.0
                },
        );
        t.rotation = facing;
        t.scale = Vec3::splat(scale);
        *vis = if a.alive {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (fill, mut t) in &mut fills {
        let f = run.sim.actors[fill.id].hp_frac().clamp(0.001, 1.0);
        t.scale.x = f;
        // Local +X is screen-left (the bar faces back toward the camera), so anchor there.
        t.translation.x = (1.0 - f) * 0.75;
        t.translation.z = -0.03;
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
            (false, Color::srgba(1.0, 0.3, 0.05, 0.25), 0.03),
            (true, Color::srgba(1.0, 0.15, 0.0, 0.55), 0.04),
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
                Transform::from_translation(to_vec3(tg.center, y))
                    .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
                    .with_scale(Vec3::new(0.01, 0.01, 1.0)),
                TelegraphVisual { id: tg.id, inner },
                DespawnOnExit(AppState::Dungeon),
            ));
        }
    }
}

fn draw_gizmos(run: Res<ActiveRun>, mut gizmos: Gizmos) {
    let flat = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
    if let Some(p) = run.sim.player {
        let me = &run.sim.actors[p];
        gizmos.circle(
            Isometry3d::new(to_vec3(run.interp_pos(p), 0.05), flat),
            0.8,
            Color::srgb(0.3, 1.0, 0.4),
        );
        if let Some(t) = me.target {
            let a = &run.sim.actors[t];
            let color = if a.is_enemy() {
                Color::srgb(1.0, 0.25, 0.2)
            } else {
                Color::srgb(0.3, 0.8, 1.0)
            };
            let pos = to_vec3(run.interp_pos(t), 0.06);
            gizmos.circle(Isometry3d::new(pos, flat), a.radius + 0.5, color);
            gizmos.circle(Isometry3d::new(pos, flat), a.radius + 0.6, color);
        }
    }
    for tg in &run.sim.telegraphs {
        gizmos.circle(
            Isometry3d::new(to_vec3(tg.center, 0.08), flat),
            tg.radius,
            Color::srgb(1.0, 0.2, 0.0),
        );
    }
    // Aggro radius hint for idle enemies near the player.
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
                    Isometry3d::new(to_vec3(a.pos, 0.04), flat),
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
    let rewards = data.profile.grant_rewards(&content, sim.outcome, sim.seed);
    data.save();
    // Persist the full combat log for inspection.
    let log_path = std::path::Path::new("logs").join("last_run.log");
    let text: String = sim.events.iter().map(|e| sim.describe(e) + "\n").collect();
    let log_path = std::fs::create_dir_all("logs")
        .and_then(|_| std::fs::write(&log_path, text))
        .map(|_| log_path.display().to_string())
        .ok();
    let highlights: Vec<String> = sim
        .events
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                EventKind::Death { .. }
                    | EventKind::PhaseChange { .. }
                    | EventKind::EncounterCleared { .. }
                    | EventKind::EncounterReset { .. }
                    | EventKind::Enrage { .. }
                    | EventKind::TelegraphHit { .. }
                    | EventKind::Outcome { .. }
            )
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
        highlights,
        log_path,
    });
    run.end_timer = None;
    next.set(AppState::Results);
}
