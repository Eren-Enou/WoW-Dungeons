//! Dungeon state: owns the running `Sim`, maps input to sim commands at a fixed
//! timestep, and renders actors, walls, hazards and telegraphs.

use crate::camera::{CameraBlockers, CameraFocus, MainCamera, OrbitCamera};
use crate::combat_feed::{CombatFeed, FeedLine, FightTracker};
use crate::common::{AppState, GameData, LastResult, rgb, to_vec3};
use crate::controls::{ControlSet, PlayerControl};
use crate::demo::DemoConfig;
use crate::env::Occluder;
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
                sync_caches,
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
/// Lines kept above the action bar, and how long each stays (0.9.2).
pub const FEED_LINES: usize = 4;
pub const FEED_SECS: f32 = 4.0;

#[derive(Resource)]
pub struct ActiveRun {
    pub sim: Sim,
    /// Which layout this run plays (0.9): "classic layout" or the seed.
    pub layout: String,
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
    /// Your recent actions above the action bar, with their age (0.9.2).
    pub feed: VecDeque<(FeedLine, f32)>,
    pub feed_state: CombatFeed,
    /// Your interrupts used, as counted so far (the feed needs the new ones).
    pub interrupts_seen: u32,
    /// Your numbers for the current or last fight, and whether they show (K).
    pub fight: FightTracker,
    pub show_fight: bool,
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

#[derive(Component)]
struct TelegraphVisual {
    id: u32,
    inner: bool,
}

/// Opened caches show their open model (or vanish) and lose their glint.
fn sync_caches(
    run: Res<ActiveRun>,
    mut caches: Query<(Entity, &mut crate::env::CacheVisual)>,
    glints: Query<(Entity, &crate::env::CacheGlint)>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
) {
    for (e, mut v) in &mut caches {
        let open = run
            .sim
            .caches
            .get(v.index)
            .is_some_and(|c| c.opened_at.is_some());
        if !open || v.opened {
            continue;
        }
        v.opened = true;
        match &v.open_model {
            Some(m) => {
                commands.entity(e).insert(WorldAssetRoot(
                    asset_server.load(bevy::gltf::GltfAssetLabel::Scene(0).from_asset(m.clone())),
                ));
            }
            None => commands.entity(e).despawn(),
        }
        for (g, glint) in &glints {
            if glint.0 == v.index {
                commands.entity(g).despawn();
            }
        }
    }
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
        .run_content()
        .cloned()
        .expect("hub only enters with valid content");
    let layout = data.layout_label();
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
        route: Vec::new(),
        slot_follow_only: false,
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
        "Entering '{}' [{}], {layout}, with seed {seed} and {} party members",
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
        layout,
        feed: VecDeque::new(),
        feed_state: CombatFeed::default(),
        interrupts_seen: 0,
        fight: FightTracker::default(),
        show_fight: true,
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
    crate::env::spawn_environment(
        &mut commands,
        &run.sim.content,
        &run.sim.nav,
        AppState::Dungeon,
        *camera,
        &mut blockers,
        &mut meshes,
        &mut materials,
        &asset_server,
    );
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
    if keys.just_pressed(KeyCode::KeyK) {
        run.show_fight = !run.show_fight;
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
    for (_, age) in run.feed.iter_mut() {
        *age += dt;
    }
    while run.feed.front().is_some_and(|(_, age)| *age >= FEED_SECS) {
        run.feed.pop_front();
    }
}

/// Turn new sim events into log lines, banners and error messages.
pub fn process_events(mut run: ResMut<ActiveRun>) {
    let start = run.events_seen;
    let end = run.sim.events.len();
    run.fresh.clear();
    let run = &mut *run;
    run.fight.update(&run.sim);
    // Your recent actions (0.9.2). An interrupt with nothing to stop may leave
    // no event, so the attempts come from the metrics.
    let attempts = run
        .sim
        .player
        .map(|p| run.sim.metrics.actors[p].interrupts_attempted)
        .unwrap_or(0);
    let new_attempts = attempts.saturating_sub(run.interrupts_seen);
    run.interrupts_seen = attempts;
    if start < end || new_attempts > 0 {
        let lines = run
            .feed_state
            .lines(&run.sim, &run.sim.events[start..end], new_attempts);
        for l in lines {
            run.feed.push_back((l, 0.0));
        }
        while run.feed.len() > FEED_LINES {
            run.feed.pop_front();
        }
    }
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
            EventKind::CacheOpened { cache, .. } => {
                let name = run.sim.caches[*cache].name.to_string();
                run.set_banner(
                    format!("Opened: {name} (its loot is yours at the end of the run)"),
                    3.0,
                    Color::srgb(1.0, 0.85, 0.4),
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
    let header = format!(
        "# {} - {} - sim seed {}\n",
        sim.content.dungeon.name, run.layout, sim.seed
    );
    let text: String = std::iter::once(header)
        .chain(sim.events.iter().map(|e| sim.describe(e) + "\n"))
        .collect();
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
            | EventKind::CacheOpened { .. }
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
    let rewards_sources: Vec<String> = rewards
        .sources
        .iter()
        .map(|id| content.source_name(id))
        .collect();
    commands.insert_resource(LastResult {
        summary,
        rewards,
        dungeon_name: if run.layout == "classic layout" {
            content.dungeon.name.clone()
        } else {
            format!("{} ({})", content.dungeon.name, run.layout)
        },
        difficulty_name: sim.difficulty.name.clone(),
        highlights,
        log_path,
        item_sources: rewards_sources,
        treasure: sim
            .caches
            .iter()
            .map(|c| (c.name.to_string(), c.opened_at.is_some()))
            .collect(),
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
