//! The dungeon viewer: a dungeon's environment without a simulation.
//!
//! `dungeon-sandbox --viewer [ID | path.toml]` (or V in the hub) loads a
//! dungeon file, builds its `Nav` and calls the same environment builder the
//! game uses (`env.rs`), so what you see is what the game draws. No `Sim` is
//! created, nothing is fought, and the profile is never written.
//!
//! On top of the environment it draws optional layers, from data only:
//! 1. navigation: surfaces, portals, drops, unreachable regions
//!    (`navdebug.rs`);
//! 2. structure: rooms, corridors, doorways, the main route;
//! 3. encounters: spawns by rank, aggro radii, patrols, hazards;
//! 4. modules and anchors: every modular piece and decoration anchor.
//!
//! The camera flies freely (or walks at eye height, or orbits a room), jumps
//! to the entrance or any room, and a click selects a room, module, anchor or
//! mob for a read-only inspector. See DUNGEON_VIEWER_GUIDE.md.

use crate::camera::{CameraBlockers, MainCamera};
use crate::common::{AppState, GameData, text_font};
use crate::controls::{BlocksWorld, PlayerControl};
use crate::env::{ModuleInstances, entrance_point};
use crate::navdebug::NavDebug;
use crate::structure::Structure;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::picking::Pickable;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::ui_widgets::{Activate, Button as UiButton};
use dungeon_sim::V2;
use dungeon_sim::content::{Content, EnemyRank};
use dungeon_sim::nav::Nav;
use std::path::{Path, PathBuf};

pub struct ViewerPlugin(pub ViewerRequest);

impl Plugin for ViewerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0.clone())
            .init_resource::<ViewerCam>()
            .init_resource::<ViewerLayers>()
            .init_resource::<FrameStats>()
            .add_observer(on_viewer_activate)
            .add_systems(Update, start_in_viewer.run_if(in_state(AppState::Hub)))
            .add_systems(OnEnter(AppState::Viewer), enter_viewer)
            .add_systems(OnExit(AppState::Viewer), exit_viewer)
            .add_systems(
                Update,
                (
                    viewer_keys,
                    move_camera,
                    pick,
                    draw_layers,
                    place_labels,
                    update_panels,
                    frame_stats,
                    tour,
                )
                    .chain()
                    .after(crate::controls::ControlSet)
                    .run_if(in_state(AppState::Viewer))
                    .run_if(resource_exists::<ViewerScene>),
            );
    }
}

/// What to open. Set from the command line or by the hub.
#[derive(Resource, Clone, Default)]
pub struct ViewerRequest {
    /// Dungeon id or file path; `None` = the hub's selected dungeon.
    pub target: Option<String>,
    /// Blueprint and seed to generate (with the `dungeon-forge` executable).
    pub blueprint: Option<String>,
    pub seed: Option<u64>,
    /// Start in the viewer (the command line asked for it).
    pub at_start: bool,
    /// Screenshot every room, write a contact sheet and statistics, exit.
    pub tour: bool,
    /// Layers to switch on at start (`nav,structure,encounters,modules`).
    pub layers: Vec<String>,
    /// Room to start at (id or name).
    pub room: Option<String>,
}

impl ViewerRequest {
    pub fn from_args() -> ViewerRequest {
        let args: Vec<String> = std::env::args().collect();
        let value = |flag: &str| {
            args.iter()
                .position(|a| a == flag)
                .and_then(|i| args.get(i + 1))
                .filter(|v| !v.starts_with("--"))
                .cloned()
        };
        let at_start = args.iter().any(|a| a == "--viewer");
        ViewerRequest {
            target: value("--viewer"),
            blueprint: value("--blueprint"),
            seed: value("--seed").and_then(|s| s.parse().ok()),
            at_start: at_start || args.iter().any(|a| a == "--blueprint"),
            tour: args.iter().any(|a| a == "--viewer-tour"),
            layers: value("--layers")
                .map(|l| l.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default(),
            room: value("--room"),
        }
    }
}

/// The dungeon being viewed.
#[derive(Resource)]
pub struct ViewerScene {
    pub content: Content,
    pub nav: Nav,
    pub structure: Structure,
    /// Surfaces the entrance can't reach (content bug when not intended).
    pub cut_off: Vec<bool>,
    /// File it came from (for reload), if any.
    pub path: Option<PathBuf>,
    /// How it was found ("file ...", "library id ...").
    pub source: String,
    pub error: Option<String>,
    /// Rooms in tour / Tab order: main route first, then the rest.
    pub order: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CamMode {
    #[default]
    Fly,
    /// Eye height over walkable floor; can't leave it.
    Walk,
    /// Around a point (the current room or the selection).
    Orbit,
}

#[derive(Resource)]
pub struct ViewerCam {
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    /// Fly speed, metres per second.
    pub speed: f32,
    pub mode: CamMode,
    pub orbit_center: Vec3,
    pub orbit_distance: f32,
    /// Index into `ViewerScene::order` of the last room jumped to.
    pub room_cursor: Option<usize>,
    /// Keep the camera across reloads of the same dungeon.
    pub dungeon: String,
}

impl Default for ViewerCam {
    fn default() -> Self {
        ViewerCam {
            pos: Vec3::new(0.0, 12.0, -14.0),
            yaw: std::f32::consts::PI,
            pitch: -0.5,
            speed: 12.0,
            mode: CamMode::Fly,
            orbit_center: Vec3::ZERO,
            orbit_distance: 25.0,
            room_cursor: None,
            dungeon: String::new(),
        }
    }
}

impl ViewerCam {
    fn rotation(&self) -> Quat {
        Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0)
    }

    fn forward(&self) -> Vec3 {
        self.rotation() * -Vec3::Z
    }

    /// Point the camera from `from` at `at`.
    pub fn look(&mut self, from: Vec3, at: Vec3) {
        self.pos = from;
        let d = (at - from).normalize_or(Vec3::NEG_Z);
        self.yaw = (-d.x).atan2(-d.z);
        self.pitch = d.y.clamp(-1.0, 1.0).asin();
    }

    /// An overview of a box: from the south-west, above, looking at its middle.
    pub fn frame(&mut self, center: Vec3, size: Vec3) {
        let s = size.x.max(size.z).max(8.0);
        let from = center + Vec3::new(-0.35 * s - 3.0, 0.55 * s + 7.0, -0.75 * s - 6.0);
        self.look(from, center + Vec3::Y * 0.5);
        self.orbit_center = center;
        self.orbit_distance = from.distance(center);
    }
}

#[derive(Resource)]
pub struct ViewerLayers {
    pub ui: bool,
    /// The info panel (top left) and status line.
    pub info: bool,
    pub help: bool,
    pub structure: bool,
    pub encounters: bool,
    pub modules: bool,
    pub rooms_panel: bool,
}

impl Default for ViewerLayers {
    fn default() -> Self {
        ViewerLayers {
            ui: true,
            info: true,
            help: true,
            structure: false,
            encounters: false,
            modules: false,
            rooms_panel: false,
        }
    }
}

/// What a click selected.
#[derive(Resource, Clone, Debug)]
pub enum Selection {
    Room(usize),
    Module(usize),
    Anchor(usize),
    Mob { encounter: usize, mob: usize },
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum PanelText {
    Info,
    Help,
    Status,
}

#[derive(Component)]
struct RoomsPanel;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Layer {
    Structure,
    Encounters,
}

/// A label pinned to a world position, shown with its layer.
#[derive(Component)]
struct WorldLabel {
    at: Vec3,
    layer: Layer,
}

#[derive(Component, Clone)]
enum ViewerAction {
    GoRoom(usize),
}

#[derive(Resource, Default)]
pub struct FrameStats {
    /// Recent frame times, seconds.
    pub samples: Vec<f32>,
    pub visible_meshes: usize,
    pub meshes: usize,
    pub entities: usize,
    pub message: Option<(String, f32)>,
}

impl FrameStats {
    fn mean_ms(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        1000.0 * self.samples.iter().sum::<f32>() / self.samples.len() as f32
    }

    fn p95_ms(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let mut v = self.samples.clone();
        v.sort_by(f32::total_cmp);
        1000.0 * v[((v.len() as f32 * 0.95) as usize).min(v.len() - 1)]
    }
}

/// `--viewer` on the command line: leave the hub on the first frame (the
/// camera and other startup entities exist by then).
fn start_in_viewer(mut req: ResMut<ViewerRequest>, mut next: ResMut<NextState<AppState>>) {
    if req.at_start {
        req.at_start = false;
        next.set(AppState::Viewer);
    }
}

// ------------------------------------------------------------- loading --

/// The `dungeon-forge` executable next to this one (packaged build) or in
/// the cargo target folder (development).
fn forge_exe() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "dungeon-forge.exe"
    } else {
        "dungeon-forge"
    };
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        candidates.push(dir.join(name));
    }
    candidates.push(PathBuf::from("target/release").join(name));
    candidates.push(PathBuf::from("target/debug").join(name));
    candidates.into_iter().find(|p| p.is_file())
}

/// Generate `blueprint` at `seed` into `forge_out/viewer/` and return the file.
pub fn generate_layout(blueprint: &str, seed: u64) -> Result<PathBuf, String> {
    let exe = forge_exe().ok_or_else(|| {
        "dungeon-forge executable not found next to the game or in target/; build it with `cargo build --release -p dungeon_forge`".to_string()
    })?;
    let stem = Path::new(blueprint)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "layout".into());
    let out = PathBuf::from(format!("forge_out/viewer/{stem}_s{seed}.toml"));
    let status = std::process::Command::new(&exe)
        .args([
            "generate",
            blueprint,
            "--seed",
            &seed.to_string(),
            "--tagged",
            "--quiet",
            "--out",
        ])
        .arg(&out)
        .output()
        .map_err(|e| format!("could not run {}: {e}", exe.display()))?;
    if !status.status.success() {
        return Err(format!(
            "dungeon-forge failed for {blueprint} seed {seed}: {}",
            String::from_utf8_lossy(&status.stderr).trim()
        ));
    }
    Ok(out)
}

fn load_target(
    req: &ViewerRequest,
    data: &mut GameData,
) -> Result<(Content, Option<PathBuf>, String), String> {
    if let Some(bp) = &req.blueprint {
        let seed = req.seed.unwrap_or(1);
        let path = generate_layout(bp, seed)?;
        let c = Content::load(&path).map_err(|e| e.to_string())?;
        return Ok((
            c,
            Some(path.clone()),
            format!("generated from {bp} seed {seed}"),
        ));
    }
    match &req.target {
        Some(t) if t.ends_with(".toml") || Path::new(t).is_file() => {
            let c = Content::load(t).map_err(|e| e.to_string())?;
            Ok((c, Some(PathBuf::from(t)), format!("file {t}")))
        }
        Some(id) => {
            data.reload_content();
            data.library
                .get(id)
                .cloned()
                .map(|c| (c, None, format!("library id '{id}'")))
                .ok_or_else(|| {
                    format!(
                        "no dungeon with id '{id}' in {} (ids: {})",
                        data.content_path,
                        data.library
                            .dungeons
                            .iter()
                            .map(|d| d.dungeon.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
        }
        None => {
            data.reload_content();
            let id = data.profile.selected_dungeon.clone();
            data.selected_content()
                .cloned()
                .map(|c| (c, None, format!("library id '{id}' (selected in the hub)")))
                .ok_or_else(|| "no dungeon content loaded".to_string())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn enter_viewer(
    mut commands: Commands,
    mut req: ResMut<ViewerRequest>,
    mut data: ResMut<GameData>,
    mut cam: ResMut<ViewerCam>,
    mut layers: ResMut<ViewerLayers>,
    mut navdbg: ResMut<NavDebug>,
    camera: Single<Entity, With<MainCamera>>,
    mut blockers: ResMut<CameraBlockers>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    let (content, path, source, error) = match load_target(&req, &mut data) {
        Ok((c, p, s)) => (c, p, s, None),
        Err(e) => {
            error!("viewer: {e}");
            // Show something rather than nothing: the first library dungeon.
            match data.library.dungeons.first().cloned() {
                Some(c) => {
                    let id = c.dungeon.id.clone();
                    (c, None, format!("library id '{id}' (fallback)"), Some(e))
                }
                None => {
                    error!("viewer: no content at all; returning to the hub");
                    commands.insert_resource(NextState::Pending(AppState::Hub));
                    return;
                }
            }
        }
    };
    // Later reloads re-read the same file (a generated seed became a file).
    if let Some(p) = &path {
        req.target = Some(p.display().to_string());
        req.blueprint = None;
    }
    let nav = content.nav();
    let structure = Structure::build(&content, &nav);
    let entrance = entrance_point(&content, &nav);
    let cut_off = crate::navdebug::disconnected(&nav, entrance);
    blockers.0.clear();
    crate::env::spawn_environment(
        &mut commands,
        &content,
        &nav,
        AppState::Viewer,
        *camera,
        &mut blockers,
        &mut meshes,
        &mut materials,
        &asset_server,
    );
    // Main route by order, then everything else in file order.
    let mut order: Vec<usize> = (0..structure.rooms.len()).collect();
    order.sort_by_key(|&i| {
        let r = &structure.rooms[i];
        (r.order.is_none(), r.order.unwrap_or(0), r.corridor, i)
    });
    // Camera: keep it when reloading the same dungeon, else the entrance.
    if cam.dungeon != content.dungeon.id {
        cam.dungeon = content.dungeon.id.clone();
        cam.mode = CamMode::Fly;
        cam.room_cursor = None;
        let first = order.iter().copied().find(|&i| i != 0).unwrap_or(0);
        let target = structure
            .rooms
            .get(first)
            .map(|r| r.center())
            .unwrap_or(entrance + Vec3::Z * 10.0);
        let back = (entrance - target).with_y(0.0).normalize_or(Vec3::NEG_Z);
        cam.look(entrance + back * 10.0 + Vec3::Y * 9.0, entrance + Vec3::Y);
    }
    if let Some(name) = &req.room
        && let Some(k) = structure
            .rooms
            .iter()
            .position(|r| r.id == *name || r.name.eq_ignore_ascii_case(name))
    {
        let r = &structure.rooms[k];
        cam.frame(r.center(), r.size());
        cam.room_cursor = order.iter().position(|&i| i == k);
    }
    for l in &req.layers {
        match l.as_str() {
            "nav" => navdbg.on = true,
            "collision" => {
                navdbg.on = true;
                navdbg.collision = true;
            }
            "structure" => layers.structure = true,
            "encounters" => layers.encounters = true,
            "modules" => layers.modules = true,
            "noui" => layers.ui = false,
            "noinfo" => layers.info = false,
            "panels" => {}
            _ => warn!("viewer: unknown layer '{l}'"),
        }
    }
    if req.tour {
        // Clean shots: layers and labels yes, panels no (unless asked).
        layers.help = false;
        layers.info = req.layers.iter().any(|l| l == "panels");
    }
    spawn_panels(&mut commands, &structure, &order, &content, &nav);
    info!(
        "Viewer: '{}' ({}), {} rooms, {} surfaces",
        content.dungeon.name,
        source,
        structure.rooms.len(),
        nav.surfaces.len()
    );
    commands.insert_resource(ViewerScene {
        content,
        nav,
        structure,
        cut_off,
        path,
        source,
        error,
        order,
    });
}

fn exit_viewer(mut commands: Commands, mut blockers: ResMut<CameraBlockers>) {
    commands.remove_resource::<ViewerScene>();
    commands.remove_resource::<Selection>();
    blockers.0.clear();
}

fn spawn_panels(
    commands: &mut Commands,
    structure: &Structure,
    order: &[usize],
    content: &Content,
    nav: &Nav,
) {
    let scoped = DespawnOnExit(AppState::Viewer);
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(10),
            top: px(10),
            max_width: px(470),
            padding: UiRect::all(px(8)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.62)),
        BlocksWorld,
        children![(Text::new(""), text_font(13.0), PanelText::Info)],
        scoped.clone(),
    ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(10),
            bottom: px(10),
            max_width: px(560),
            padding: UiRect::all(px(8)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        BlocksWorld,
        children![(
            Text::new(HELP),
            text_font(12.0),
            TextColor(Color::srgb(0.85, 0.85, 0.85)),
            PanelText::Help
        )],
        scoped.clone(),
    ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: px(10),
            bottom: px(10),
            padding: UiRect::all(px(6)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
        children![(
            Text::new(""),
            text_font(12.0),
            TextColor(Color::srgb(0.75, 0.9, 1.0)),
            PanelText::Status
        )],
        Pickable::IGNORE,
        scoped.clone(),
    ));
    // Room list (L): one button per room, in route order.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: px(10),
                top: px(10),
                width: px(300),
                max_height: percent(80),
                padding: UiRect::all(px(6)),
                flex_direction: FlexDirection::Column,
                row_gap: px(2),
                overflow: Overflow::scroll_y(),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.02, 0.06, 0.85)),
            BlocksWorld,
            RoomsPanel,
            scoped.clone(),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("Rooms (click to go there)"),
                text_font(13.0),
                TextColor(Color::srgb(1.0, 0.9, 0.5)),
            ));
            for &i in order {
                let r = &structure.rooms[i];
                let tag = if r.corridor { "  " } else { "" };
                p.spawn((
                    UiButton,
                    Hovered::default(),
                    ViewerAction::GoRoom(i),
                    Node {
                        padding: UiRect::axes(px(5), px(1)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.12, 0.12, 0.18)),
                    children![(
                        Text::new(format!("{tag}{} [{}]", r.name, r.role)),
                        text_font(11.0),
                        TextColor(if r.corridor {
                            Color::srgb(0.7, 0.7, 0.75)
                        } else {
                            Color::WHITE
                        })
                    )],
                ));
            }
        });
    // World labels: rooms (structure layer) and encounters.
    for r in &structure.rooms {
        if r.corridor {
            continue;
        }
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                ..default()
            },
            Text::new(format!(
                "{} {}\n[{}{}]",
                r.id,
                r.name,
                r.role,
                if r.route.is_empty() {
                    String::new()
                } else {
                    format!(", {}", r.route)
                }
            )),
            text_font(12.0),
            TextColor(Color::srgb(1.0, 0.95, 0.7)),
            TextShadow::default(),
            Pickable::IGNORE,
            WorldLabel {
                at: r.center() + Vec3::Y * 3.0,
                layer: Layer::Structure,
            },
            scoped.clone(),
        ));
    }
    for e in &content.encounters {
        let Some(m) = e.mobs.first() else { continue };
        let (p, _) = nav.project(m.pos, m.y.unwrap_or(0.0));
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                ..default()
            },
            Text::new(format!(
                "{}{}\n{} mobs",
                e.display_name(),
                if e.optional { " (optional)" } else { "" },
                e.mobs.len()
            )),
            text_font(12.0),
            TextColor(if e.optional {
                Color::srgb(0.5, 1.0, 0.6)
            } else {
                Color::srgb(1.0, 0.6, 0.6)
            }),
            TextShadow::default(),
            Pickable::IGNORE,
            WorldLabel {
                at: Vec3::new(p.x, p.y + 4.5, p.z),
                layer: Layer::Encounters,
            },
            scoped.clone(),
        ));
    }
}

const HELP: &str = "VIEWER  hold right mouse: look | WASD move | E/Space up, Q/C down | Shift fast, Ctrl slow | wheel: speed (orbit: distance)
G walk at eye height | O orbit current room | F free flight | Home entrance | Tab / Shift+Tab next / previous room | L room list
1 navigation  2 structure  3 encounters  4 modules & anchors  5 wall collision | click: inspect, Esc: clear
F5 reload file | [ ] previous / next seed (generated) | F12 screenshot | H hide all | F1 this help | Backspace: back to the hub";

fn on_viewer_activate(
    ev: On<Activate>,
    actions: Query<&ViewerAction>,
    scene: Option<Res<ViewerScene>>,
    mut cam: ResMut<ViewerCam>,
) {
    let (Ok(action), Some(scene)) = (actions.get(ev.entity), scene) else {
        return;
    };
    match action {
        ViewerAction::GoRoom(i) => go_room(&scene, &mut cam, *i),
    }
}

fn go_room(scene: &ViewerScene, cam: &mut ViewerCam, i: usize) {
    let r = &scene.structure.rooms[i];
    cam.frame(r.center(), r.size());
    cam.room_cursor = scene.order.iter().position(|&k| k == i);
    if cam.mode == CamMode::Walk {
        cam.mode = CamMode::Fly;
    }
}

// ------------------------------------------------------------ controls --

#[allow(clippy::too_many_arguments)]
fn viewer_keys(
    keys: Res<ButtonInput<KeyCode>>,
    scene: Res<ViewerScene>,
    mut req: ResMut<ViewerRequest>,
    mut cam: ResMut<ViewerCam>,
    mut layers: ResMut<ViewerLayers>,
    mut navdbg: ResMut<NavDebug>,
    mut stats: ResMut<FrameStats>,
    mut next: ResMut<NextState<AppState>>,
    mut commands: Commands,
    selection: Option<Res<Selection>>,
    mut shots: Local<u32>,
) {
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if keys.just_pressed(KeyCode::Backspace) {
        next.set(AppState::Hub);
        return;
    }
    if keys.just_pressed(KeyCode::Escape) && selection.is_some() {
        commands.remove_resource::<Selection>();
    }
    if keys.just_pressed(KeyCode::F5) {
        // Re-enter the state: despawn everything, re-read the file.
        next.set(AppState::Viewer);
        return;
    }
    for (key, delta) in [(KeyCode::BracketLeft, -1i64), (KeyCode::BracketRight, 1)] {
        if !keys.just_pressed(key) {
            continue;
        }
        let Some(g) = &scene.content.dungeon.generated else {
            stats.message = Some((
                "[ ] need a generated dungeon (with [dungeon.generated])".into(),
                4.0,
            ));
            continue;
        };
        if g.blueprint_path.is_empty() {
            stats.message = Some(("this file doesn't record its blueprint path".into(), 4.0));
            continue;
        }
        let seed = (g.seed as i64 + delta).max(1) as u64;
        req.blueprint = Some(g.blueprint_path.clone());
        req.seed = Some(seed);
        // A new layout: start at its entrance.
        cam.dungeon.clear();
        next.set(AppState::Viewer);
        return;
    }
    if keys.just_pressed(KeyCode::F12) {
        *shots += 1;
        let _ = std::fs::create_dir_all("screenshots");
        let path = format!(
            "screenshots/viewer_{}_{:02}.png",
            scene.content.dungeon.id, *shots
        );
        stats.message = Some((format!("saved {path}"), 3.0));
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
    let toggles = [
        (KeyCode::F1, 0),
        (KeyCode::KeyH, 1),
        (KeyCode::Digit1, 2),
        (KeyCode::KeyN, 2),
        (KeyCode::Digit2, 3),
        (KeyCode::Digit3, 4),
        (KeyCode::Digit4, 5),
        (KeyCode::Digit5, 6),
        (KeyCode::KeyL, 7),
    ];
    for (key, what) in toggles {
        if !keys.just_pressed(key) {
            continue;
        }
        match what {
            0 => layers.help = !layers.help,
            1 => layers.ui = !layers.ui,
            2 => navdbg.on = !navdbg.on,
            3 => layers.structure = !layers.structure,
            4 => layers.encounters = !layers.encounters,
            5 => layers.modules = !layers.modules,
            6 => {
                navdbg.collision = !navdbg.collision;
                navdbg.on |= navdbg.collision;
            }
            _ => layers.rooms_panel = !layers.rooms_panel,
        }
    }
    if keys.just_pressed(KeyCode::Home) || keys.just_pressed(KeyCode::KeyR) {
        let e = entrance_point(&scene.content, &scene.nav);
        let toward = scene
            .order
            .iter()
            .filter_map(|&i| scene.structure.rooms.get(i))
            .find(|r| r.order == Some(1) || (r.order.is_none() && !r.corridor))
            .map(|r| r.center())
            .unwrap_or(e + Vec3::Z * 10.0);
        let back = (e - toward).with_y(0.0).normalize_or(Vec3::NEG_Z);
        cam.look(e + back * 10.0 + Vec3::Y * 9.0, e + Vec3::Y);
        cam.mode = CamMode::Fly;
    }
    if keys.just_pressed(KeyCode::Tab)
        || keys.just_pressed(KeyCode::PageDown)
        || keys.just_pressed(KeyCode::PageUp)
    {
        let n = scene.order.len();
        if n > 0 {
            let back = shift || keys.just_pressed(KeyCode::PageUp);
            let k = match (cam.room_cursor, back) {
                (None, false) => 0,
                (None, true) => n - 1,
                (Some(k), false) => (k + 1) % n,
                (Some(k), true) => (k + n - 1) % n,
            };
            let i = scene.order[k];
            go_room(&scene, &mut cam, i);
        }
    }
    if keys.just_pressed(KeyCode::KeyG) {
        cam.mode = if cam.mode == CamMode::Walk {
            CamMode::Fly
        } else {
            CamMode::Walk
        };
    }
    if keys.just_pressed(KeyCode::KeyF) {
        cam.mode = CamMode::Fly;
    }
    if keys.just_pressed(KeyCode::KeyO) {
        if cam.mode == CamMode::Orbit {
            cam.mode = CamMode::Fly;
        } else {
            // Around the selection, else the room in front, else ahead.
            let center = selection
                .as_deref()
                .and_then(|s| selection_point(&scene, s))
                .or_else(|| {
                    scene
                        .structure
                        .room_at(&scene.nav, cam.pos + cam.forward() * 12.0 - Vec3::Y * 4.0)
                        .map(|i| scene.structure.rooms[i].center())
                })
                .unwrap_or(cam.pos + cam.forward() * 15.0);
            cam.orbit_center = center;
            cam.orbit_distance = cam.pos.distance(center).clamp(4.0, 120.0);
            cam.mode = CamMode::Orbit;
        }
    }
}

/// Eye height in walk mode.
const EYE: f32 = 1.7;

#[allow(clippy::too_many_arguments)]
fn move_camera(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    pc: Res<PlayerControl>,
    scene: Res<ViewerScene>,
    req: Res<ViewerRequest>,
    mut cam: ResMut<ViewerCam>,
    mut tf: Single<&mut Transform, With<MainCamera>>,
) {
    let dt = time.delta_secs().min(0.1);
    if !req.tour {
        if mouse.pressed(MouseButton::Right) {
            cam.yaw -= motion.delta.x * 0.004;
            cam.pitch = (cam.pitch - motion.delta.y * 0.004).clamp(-1.5, 1.5);
        }
        if scroll.delta.y != 0.0 && !pc.cursor_over_ui {
            let k = 1.2f32.powf(scroll.delta.y.signum());
            if cam.mode == CamMode::Orbit {
                cam.orbit_distance = (cam.orbit_distance / k).clamp(3.0, 200.0);
            } else {
                cam.speed = (cam.speed * k).clamp(1.0, 120.0);
            }
        }
    }
    let k = |key: KeyCode| if keys.pressed(key) { 1.0 } else { 0.0 };
    let mut factor = 1.0;
    if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
        factor *= 3.0;
    }
    if keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight) {
        factor *= 0.25;
    }
    let fwd_in = k(KeyCode::KeyW) - k(KeyCode::KeyS);
    let side_in = k(KeyCode::KeyD) - k(KeyCode::KeyA);
    let up_in = (k(KeyCode::KeyE) + k(KeyCode::Space)) - (k(KeyCode::KeyQ) + k(KeyCode::KeyC));
    let rot = cam.rotation();
    let right = rot * Vec3::X;
    match cam.mode {
        CamMode::Fly => {
            let v = cam.forward() * fwd_in + right * side_in + Vec3::Y * up_in;
            let step = v.normalize_or_zero() * cam.speed * factor * dt;
            cam.pos += step;
        }
        CamMode::Walk => {
            let f = cam.forward().with_y(0.0).normalize_or_zero();
            let r = right.with_y(0.0).normalize_or_zero();
            let v = (f * fwd_in + r * side_in).normalize_or_zero() * 7.0 * factor * dt;
            let want = cam.pos + v;
            // Stay on walkable floor: project onto the navigation surfaces.
            let (p, _) = scene.nav.project(V2::new(want.x, want.z), cam.pos.y - EYE);
            cam.pos = Vec3::new(p.x, p.y + EYE, p.z);
        }
        CamMode::Orbit => {
            // Keys turn and raise the orbit; the mouse (right button) too.
            cam.yaw += side_in * 1.2 * dt;
            cam.pitch = (cam.pitch - up_in * 0.8 * dt).clamp(-1.5, 0.2);
            cam.orbit_distance = (cam.orbit_distance * (1.0 - fwd_in * 0.8 * dt)).clamp(3.0, 200.0);
            let d = cam.forward();
            cam.pos = cam.orbit_center - d * cam.orbit_distance;
        }
    }
    **tf = Transform::from_translation(cam.pos).with_rotation(cam.rotation());
}

// ------------------------------------------------------------- picking --

fn selection_point(scene: &ViewerScene, s: &Selection) -> Option<Vec3> {
    match s {
        Selection::Room(i) => scene.structure.rooms.get(*i).map(|r| r.center()),
        Selection::Anchor(i) => scene
            .content
            .dungeon
            .anchors
            .get(*i)
            .map(|a| Vec3::new(a.pos.x, a.y, a.pos.z)),
        Selection::Mob { encounter, mob } => {
            let m = scene.content.encounters.get(*encounter)?.mobs.get(*mob)?;
            let (p, _) = scene.nav.project(m.pos, m.y.unwrap_or(0.0));
            Some(Vec3::new(p.x, p.y, p.z))
        }
        Selection::Module(_) => None,
    }
}

fn ray_sphere(ray: Ray3d, c: Vec3, r: f32) -> Option<f32> {
    let oc = ray.origin - c;
    let d = *ray.direction;
    let b = oc.dot(d);
    let disc = b * b - (oc.length_squared() - r * r);
    if disc < 0.0 {
        return None;
    }
    let t = -b - disc.sqrt();
    (t > 0.0).then_some(t)
}

#[allow(clippy::too_many_arguments)]
fn pick(
    mouse: Res<ButtonInput<MouseButton>>,
    pc: Res<PlayerControl>,
    scene: Res<ViewerScene>,
    layers: Res<ViewerLayers>,
    modules: Option<Res<ModuleInstances>>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut commands: Commands,
) {
    if !mouse.just_pressed(MouseButton::Left) || pc.cursor_over_ui {
        return;
    }
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let (cam, gt) = *camera;
    let Ok(ray) = cam.viewport_to_world(gt, cursor) else {
        return;
    };
    match pick_at(&scene, ray, &layers, modules.as_deref()) {
        Some(s) => commands.insert_resource(s),
        None => commands.remove_resource::<Selection>(),
    }
}

/// What a ray from the camera hits first: a mob (encounter layer), an
/// anchor (modules layer), a modular piece, else the room whose floor it
/// meets.
pub fn pick_at(
    scene: &ViewerScene,
    ray: Ray3d,
    layers: &ViewerLayers,
    modules: Option<&ModuleInstances>,
) -> Option<Selection> {
    let mut best: Option<(f32, Selection)> = None;
    let mut consider = |t: f32, s: Selection| {
        if best.as_ref().is_none_or(|(bt, _)| t < *bt) {
            best = Some((t, s));
        }
    };
    if layers.encounters {
        for (ei, e) in scene.content.encounters.iter().enumerate() {
            for (mi, m) in e.mobs.iter().enumerate() {
                let (p, _) = scene.nav.project(m.pos, m.y.unwrap_or(0.0));
                let r = scene
                    .content
                    .templates
                    .get(&m.template)
                    .map(|t| t.radius)
                    .unwrap_or(0.5)
                    .max(0.6);
                if let Some(t) = ray_sphere(ray, Vec3::new(p.x, p.y + 1.0, p.z), r + 0.3) {
                    consider(
                        t,
                        Selection::Mob {
                            encounter: ei,
                            mob: mi,
                        },
                    );
                }
            }
        }
    }
    if layers.modules {
        for (i, a) in scene.content.dungeon.anchors.iter().enumerate() {
            if let Some(t) = ray_sphere(ray, Vec3::new(a.pos.x, a.y + 0.5, a.pos.z), 0.6) {
                consider(t, Selection::Anchor(i));
            }
        }
    }
    if let Some(m) = &modules {
        for (i, inst) in m.0.iter().enumerate() {
            if let Some(t) = ray_box(ray, inst.min, inst.max) {
                consider(t, Selection::Module(i));
            }
        }
    }
    // Floors: march the ray until it passes below a surface it is over.
    let mut t = 0.0;
    while t < 250.0 {
        let p = ray.get_point(t);
        if let Some(s) = scene.nav.locate(V2::new(p.x, p.z), p.y + 0.5) {
            let sf = scene.nav.surface(s);
            if p.y <= sf.height_at(V2::new(p.x, p.z)) + 0.05 {
                consider(t, Selection::Room(scene.structure.surface_room[s as usize]));
                break;
            }
        }
        t += 0.25;
    }
    best.map(|(_, s)| s)
}

fn ray_box(ray: Ray3d, min: Vec3, max: Vec3) -> Option<f32> {
    crate::camera::ray_box(ray.origin, *ray.direction, min, max)
}

// ------------------------------------------------------------- layers --

fn rank_color(rank: Option<EnemyRank>) -> Color {
    match rank {
        Some(EnemyRank::Boss) => Color::srgb(1.0, 0.3, 0.85),
        Some(EnemyRank::Elite) => Color::srgb(1.0, 0.8, 0.2),
        _ => Color::srgb(0.95, 0.95, 0.95),
    }
}

fn flat_circle(gizmos: &mut Gizmos, c: Vec3, r: f32, color: Color) {
    gizmos.circle(
        Isometry3d::new(c, Quat::from_rotation_arc(Vec3::Z, Vec3::Y)),
        r,
        color,
    );
}

fn route_color(route: &str, role: &str) -> Color {
    if role == "arena" {
        return Color::srgb(1.0, 0.3, 0.3);
    }
    match route {
        "main" => Color::srgb(0.35, 0.7, 1.0),
        "alternate" => Color::srgb(0.3, 1.0, 0.9),
        "side" => Color::srgb(0.4, 1.0, 0.4),
        "overlook" | "shortcut" => Color::srgb(1.0, 0.6, 0.2),
        "loop" => Color::srgb(0.8, 0.5, 1.0),
        _ => Color::srgb(0.8, 0.8, 0.8),
    }
}

fn draw_layers(
    scene: Res<ViewerScene>,
    layers: Res<ViewerLayers>,
    selection: Option<Res<Selection>>,
    modules: Option<Res<ModuleInstances>>,
    mut gizmos: Gizmos,
) {
    let st = &scene.structure;
    if layers.structure {
        for r in &st.rooms {
            let y = r.min.y + 0.15;
            let c = route_color(&r.route, &r.role);
            let color = if r.corridor { c.with_alpha(0.45) } else { c };
            let (a, b) = (r.min, r.max);
            let pts = [
                Vec3::new(a.x, y, a.z),
                Vec3::new(b.x, y, a.z),
                Vec3::new(b.x, y, b.z),
                Vec3::new(a.x, y, b.z),
            ];
            for k in 0..4 {
                gizmos.line(pts[k], pts[(k + 1) % 4], color);
            }
        }
        for d in &st.doorways {
            let h = d.along * (d.width * 0.5);
            let y = Vec3::Y * 0.3;
            gizmos.line(
                d.center - h + y,
                d.center + h + y,
                Color::srgb(1.0, 0.95, 0.2),
            );
            gizmos.line(
                d.center - h + y,
                d.center - h + Vec3::Y * 2.5,
                Color::srgb(1.0, 0.95, 0.2),
            );
            gizmos.line(
                d.center + h + y,
                d.center + h + Vec3::Y * 2.5,
                Color::srgb(1.0, 0.95, 0.2),
            );
        }
        // Main route through the rooms in order.
        let mut main: Vec<&crate::structure::RoomView> =
            st.rooms.iter().filter(|r| r.order.is_some()).collect();
        main.sort_by_key(|r| r.order);
        for w in main.windows(2) {
            gizmos.arrow(
                w[0].center() + Vec3::Y * 1.0,
                w[1].center() + Vec3::Y * 1.0,
                Color::srgb(0.5, 0.85, 1.0),
            );
        }
    }
    if layers.encounters {
        for e in &scene.content.encounters {
            for m in &e.mobs {
                let t = scene.content.templates.get(&m.template);
                let (p, _) = scene.nav.project(m.pos, m.y.unwrap_or(0.0));
                let at = Vec3::new(p.x, p.y, p.z);
                let color = rank_color(t.map(|t| t.rank));
                let r = t.map(|t| t.radius).unwrap_or(0.5).max(0.4);
                gizmos.sphere(Isometry3d::from_translation(at + Vec3::Y * 1.0), r, color);
                gizmos.line(at, at + Vec3::Y * 2.0, color);
                if let Some(t) = t {
                    let ring = if e.optional {
                        Color::srgba(0.4, 1.0, 0.5, 0.35)
                    } else {
                        Color::srgba(1.0, 0.4, 0.4, 0.35)
                    };
                    flat_circle(&mut gizmos, at + Vec3::Y * 0.1, t.aggro_radius, ring);
                }
                if !m.patrol.is_empty() {
                    let mut prev = at + Vec3::Y * 0.2;
                    for q in &m.patrol {
                        let (pp, _) = scene.nav.project(*q, p.y);
                        let next = Vec3::new(pp.x, pp.y + 0.2, pp.z);
                        gizmos.line(prev, next, Color::srgb(0.9, 0.9, 1.0));
                        flat_circle(&mut gizmos, next, 0.3, Color::srgb(0.9, 0.9, 1.0));
                        prev = next;
                    }
                }
            }
            for h in &e.hazards {
                let (p, _) = scene.nav.project(h.pos, h.y.unwrap_or(0.0));
                flat_circle(
                    &mut gizmos,
                    Vec3::new(p.x, p.y + 0.12, p.z),
                    h.radius,
                    Color::srgb(1.0, 0.5, 0.1),
                );
            }
        }
    }
    if layers.modules {
        for a in &scene.content.dungeon.anchors {
            let at = Vec3::new(a.pos.x, a.y, a.pos.z);
            let color = if a.used.is_empty() {
                Color::srgba(0.6, 0.6, 0.6, 0.7)
            } else {
                Color::srgb(0.3, 1.0, 0.6)
            };
            gizmos.sphere(Isometry3d::from_translation(at + Vec3::Y * 0.5), 0.3, color);
            let yaw = a.yaw.to_radians();
            let dir = Vec3::new(yaw.sin(), 0.0, yaw.cos());
            gizmos.arrow(at + Vec3::Y * 0.5, at + Vec3::Y * 0.5 + dir * 1.2, color);
        }
        if let Some(m) = &modules {
            for inst in &m.0 {
                let color = if inst.fallback {
                    Color::srgba(1.0, 0.3, 0.3, 0.6)
                } else {
                    Color::srgba(0.4, 0.8, 1.0, 0.35)
                };
                gizmos.cube(
                    Transform::from_translation((inst.min + inst.max) * 0.5)
                        .with_scale(inst.max - inst.min),
                    color,
                );
            }
        }
    }
    // The selection, always.
    if let Some(sel) = selection.as_deref() {
        let white = Color::WHITE;
        match sel {
            Selection::Room(i) => {
                if let Some(r) = st.rooms.get(*i) {
                    for &s in &r.surfaces {
                        let sf = scene.nav.surface(s as _);
                        for (a, b) in sf.def.shape.edges() {
                            gizmos.line(
                                Vec3::new(a.x, sf.height_at(a) + 0.2, a.z),
                                Vec3::new(b.x, sf.height_at(b) + 0.2, b.z),
                                white,
                            );
                        }
                    }
                }
            }
            Selection::Module(i) => {
                if let Some(inst) = modules.as_ref().and_then(|m| m.0.get(*i)) {
                    gizmos.cube(
                        Transform::from_translation((inst.min + inst.max) * 0.5)
                            .with_scale(inst.max - inst.min + Vec3::splat(0.05)),
                        white,
                    );
                }
            }
            other => {
                if let Some(p) = selection_point(&scene, other) {
                    flat_circle(&mut gizmos, p + Vec3::Y * 0.15, 1.2, white);
                }
            }
        }
    }
}

fn place_labels(
    layers: Res<ViewerLayers>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut q: Query<(&WorldLabel, &mut Node)>,
) {
    let (cam, gt) = *camera;
    for (label, mut node) in &mut q {
        let on = layers.ui
            && match label.layer {
                Layer::Structure => layers.structure,
                Layer::Encounters => layers.encounters,
            };
        let in_front = (label.at - gt.translation()).dot(*gt.forward()) > 0.5
            && label.at.distance(gt.translation()) < 110.0;
        match cam
            .world_to_viewport(gt, label.at)
            .ok()
            .filter(|_| on && in_front)
        {
            Some(p) => {
                node.display = Display::Flex;
                node.left = px(p.x - 50.0);
                node.top = px(p.y - 12.0);
            }
            None => node.display = Display::None,
        }
    }
}

// --------------------------------------------------------------- panels --

#[allow(clippy::too_many_arguments)]
fn update_panels(
    time: Res<Time>,
    scene: Res<ViewerScene>,
    cam: Res<ViewerCam>,
    layers: Res<ViewerLayers>,
    navdbg: Res<NavDebug>,
    selection: Option<Res<Selection>>,
    modules: Option<Res<ModuleInstances>>,
    mut stats: ResMut<FrameStats>,
    mut texts: Query<(&PanelText, &mut Text)>,
    mut nodes: Query<(&mut Node, Option<&Children>), Without<RoomsPanel>>,
    mut rooms_panel: Single<&mut Node, With<RoomsPanel>>,
) {
    let c = &scene.content;
    let d = &c.dungeon;
    let here = scene.structure.room_at(&scene.nav, cam.pos);
    let below = crate::structure::floor_below(&scene.nav, cam.pos).map(|s| &scene.nav.surfaces[s]);
    let enemies: usize = c.encounters.iter().map(|e| e.mobs.len()).sum();
    let optional = c.encounters.iter().filter(|e| e.optional).count();
    let rooms = scene.structure.rooms.len() - scene.structure.corridors();
    let mut info = format!("{} ({})\n{}\n", d.name, d.id, scene.source);
    if let Some(g) = &d.generated {
        info.push_str(&format!(
            "Dungeon Forge: blueprint {} seed {} | kit {} | generator v{}\n",
            g.blueprint, g.seed, g.kit, g.generator
        ));
    } else {
        info.push_str("handwritten (rooms derived from area names)\n");
    }
    info.push_str(&format!(
        "{rooms} rooms, {} corridors, {} doorways | {} areas | {} encounters ({optional} optional), {enemies} enemies\n",
        scene.structure.corridors(),
        scene.structure.doorways.len(),
        d.areas.len(),
        c.encounters.len(),
    ));
    let cut = scene.cut_off.iter().filter(|b| **b).count();
    if cut > 0 {
        info.push_str(&format!(
            "WARNING: {cut} area(s) not reachable from the entrance (magenta in layer 1)\n"
        ));
    }
    if let Some(m) = &modules {
        let fb = m.0.iter().filter(|i| i.fallback).count();
        info.push_str(&format!(
            "modules: {} pieces{}\n",
            m.0.len(),
            if fb > 0 {
                format!(", {fb} drawn as fallback")
            } else {
                String::new()
            }
        ));
    }
    if let Some(p) = &scene.path {
        info.push_str(&format!("F5 reloads {}\n", p.display()));
    }
    if let Some(e) = &scene.error {
        info.push_str(&format!("LOAD ERROR: {e}\n"));
    }
    info.push_str(&format!(
        "\ncamera ({:.1}, {:.1}, {:.1}) | {:?} | speed {:.0} m/s\n",
        cam.pos.x, cam.pos.y, cam.pos.z, cam.mode, cam.speed
    ));
    match (here, below) {
        (Some(r), Some(s)) => {
            let r = &scene.structure.rooms[r];
            info.push_str(&format!(
                "in {} {} [{}{}]\narea '{}' floor y {:.1}{}\n",
                r.id,
                r.name,
                r.role,
                if r.route.is_empty() {
                    String::new()
                } else {
                    format!(", {}", r.route)
                },
                s.name(),
                s.height_at(V2::new(cam.pos.x, cam.pos.z)),
                r.encounter
                    .as_ref()
                    .map(|e| format!(" | encounter {e}"))
                    .unwrap_or_default()
            ));
        }
        _ => info.push_str("(not above a walkable area)\n"),
    }
    let on = |b: bool| if b { "on" } else { "off" };
    info.push_str(&format!(
        "layers: 1 nav {} | 2 structure {} | 3 encounters {} | 4 modules {}\n",
        on(navdbg.on),
        on(layers.structure),
        on(layers.encounters),
        on(layers.modules)
    ));
    if let Some(sel) = selection.as_deref() {
        info.push('\n');
        info.push_str(&describe_selection(&scene, sel, modules.as_deref()));
    }
    if let Some((msg, t)) = &mut stats.message {
        *t -= time.delta_secs();
        info.push_str(&format!("\n{msg}"));
        if *t <= 0.0 {
            stats.message = None;
        }
    }
    let status = format!(
        "{:.2} ms/frame (p95 {:.2}) | {} of {} meshes visible | {} entities",
        stats.mean_ms(),
        stats.p95_ms(),
        stats.visible_meshes,
        stats.meshes,
        stats.entities
    );
    for (which, mut text) in &mut texts {
        match which {
            PanelText::Info => text.0.clone_from(&info),
            PanelText::Status => text.0.clone_from(&status),
            PanelText::Help => {}
        }
    }
    // Show or hide whole panels: the info panel's parent node etc.
    for (mut node, children) in &mut nodes {
        let Some(children) = children else { continue };
        for child in children.iter() {
            if let Ok((which, _)) = texts.get(child) {
                let show = layers.ui
                    && match which {
                        PanelText::Help => layers.help,
                        _ => layers.info,
                    };
                node.display = if show { Display::Flex } else { Display::None };
            }
        }
    }
    rooms_panel.display = if layers.ui && layers.rooms_panel {
        Display::Flex
    } else {
        Display::None
    };
}

fn describe_selection(
    scene: &ViewerScene,
    sel: &Selection,
    modules: Option<&ModuleInstances>,
) -> String {
    match sel {
        Selection::Room(i) => {
            let r = &scene.structure.rooms[*i];
            let areas: Vec<&str> = r
                .surfaces
                .iter()
                .map(|&s| scene.nav.surfaces[s].name())
                .collect();
            let doors = scene
                .structure
                .doorways
                .iter()
                .filter(|d| d.rooms.0 == *i || d.rooms.1 == *i)
                .count();
            format!(
                "SELECTED room {} '{}'\nrole {} | route {} | order {}\nsize {:.1} x {:.1} m, floor y {:.1}..{:.1}\n{} area(s): {}\n{doors} doorway(s){} | atmosphere '{}'{}\n",
                r.id,
                r.name,
                r.role,
                if r.route.is_empty() { "-" } else { &r.route },
                r.order.map(|o| o.to_string()).unwrap_or_else(|| "-".into()),
                r.size().x,
                r.size().z,
                r.min.y,
                r.max.y,
                areas.len(),
                areas.join(", "),
                r.encounter
                    .as_ref()
                    .map(|e| format!(" | encounter {e}"))
                    .unwrap_or_default(),
                r.atmosphere_key(),
                if r.declared {
                    ""
                } else {
                    " | (derived from area names)"
                }
            )
        }
        Selection::Anchor(i) => {
            let a = &scene.content.dungeon.anchors[*i];
            format!(
                "SELECTED anchor #{i} kind '{}'\nat ({:.1}, {:.1}, {:.1}) facing {:.0} deg | room '{}' | clearance {:.1} m\nused: {}\n",
                a.kind,
                a.pos.x,
                a.y,
                a.pos.z,
                a.yaw,
                a.room,
                a.clearance,
                if a.used.is_empty() {
                    "nothing"
                } else {
                    &a.used
                }
            )
        }
        Selection::Mob { encounter, mob } => {
            let e = &scene.content.encounters[*encounter];
            let m = &e.mobs[*mob];
            let t = scene.content.templates.get(&m.template);
            format!(
                "SELECTED mob '{}' ({}) in encounter {} '{}'{}\nat ({:.1}, {:.1}) | rank {:?} | aggro {:.0} m | patrol points {}\n",
                t.map(|t| t.name.as_str()).unwrap_or("?"),
                m.template,
                e.id,
                e.display_name(),
                if e.optional { " (optional)" } else { "" },
                m.pos.x,
                m.pos.z,
                t.map(|t| t.rank),
                t.map(|t| t.aggro_radius).unwrap_or(0.0),
                m.patrol.len()
            )
        }
        Selection::Module(i) => match modules.and_then(|m| m.0.get(*i)) {
            Some(inst) => {
                let (yaw, _, _) = inst.transform.rotation.to_euler(EulerRot::YXZ);
                format!(
                    "SELECTED module #{i} '{}'\nasset {}{}\nposition ({:.2}, {:.2}, {:.2}) | yaw {:.0} deg | scale ({:.2}, {:.2}, {:.2})\nbounds {:.2} x {:.2} x {:.2} m | room {}\n",
                    inst.kind,
                    inst.model,
                    if inst.fallback {
                        " (MISSING: procedural fallback drawn)"
                    } else {
                        ""
                    },
                    inst.transform.translation.x,
                    inst.transform.translation.y,
                    inst.transform.translation.z,
                    yaw.to_degrees(),
                    inst.transform.scale.x,
                    inst.transform.scale.y,
                    inst.transform.scale.z,
                    inst.max.x - inst.min.x,
                    inst.max.y - inst.min.y,
                    inst.max.z - inst.min.z,
                    inst.room
                        .and_then(|r| scene.structure.rooms.get(r))
                        .map(|r| format!("{} '{}'", r.id, r.name))
                        .unwrap_or_else(|| "-".into())
                )
            }
            None => "SELECTED module (gone)\n".into(),
        },
    }
}

fn frame_stats(
    time: Res<Time>,
    mut stats: ResMut<FrameStats>,
    meshes: Query<&ViewVisibility, With<Mesh3d>>,
    all: Query<Entity>,
) {
    stats.samples.push(time.delta_secs());
    if stats.samples.len() > 120 {
        stats.samples.remove(0);
    }
    stats.meshes = meshes.iter().count();
    stats.visible_meshes = meshes.iter().filter(|v| v.get()).count();
    stats.entities = all.iter().count();
}

// ----------------------------------------------------------------- tour --

#[derive(Clone)]
struct Shot {
    room: usize,
    walk: bool,
    file: String,
}

struct ShotResult {
    shot: Shot,
    mean_ms: f32,
    p95_ms: f32,
    visible: usize,
    meshes: usize,
}

#[derive(Default)]
struct TourState {
    shots: Vec<Shot>,
    k: usize,
    t: f32,
    started: bool,
    samples: Vec<f32>,
    results: Vec<ShotResult>,
    dir: String,
}

fn safe(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// `--viewer-tour`: an overview of every room and corridor plus an eye-height
/// view inside every room, then `index.html` (a contact sheet) and
/// `stats.csv` (frame times and mesh counts per shot) under
/// `screenshots/viewer/<dungeon>/`. Exits when done.
#[allow(clippy::too_many_arguments)]
fn tour(
    time: Res<Time>,
    req: Res<ViewerRequest>,
    scene: Res<ViewerScene>,
    stats: Res<FrameStats>,
    mut cam: ResMut<ViewerCam>,
    mut st: Local<TourState>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    if !req.tour {
        return;
    }
    if !st.started {
        st.started = true;
        st.dir = format!("screenshots/viewer/{}", safe(&scene.content.dungeon.id));
        let _ = std::fs::create_dir_all(&st.dir);
        for (n, &i) in scene.order.iter().enumerate() {
            let r = &scene.structure.rooms[i];
            let base = format!("{n:02}_{}", safe(&r.name));
            st.shots.push(Shot {
                room: i,
                walk: false,
                file: format!("{base}.png"),
            });
            if !r.corridor {
                st.shots.push(Shot {
                    room: i,
                    walk: true,
                    file: format!("{base}_eye.png"),
                });
            }
        }
    }
    if st.k >= st.shots.len() {
        st.t += time.delta_secs();
        if st.t > 1.5 {
            write_tour_report(&scene, &st);
            info!("viewer tour: {} shots in {}", st.results.len(), st.dir);
            exit.write(AppExit::Success);
        }
        return;
    }
    let shot = st.shots[st.k].clone();
    if st.t == 0.0 {
        let r = &scene.structure.rooms[shot.room];
        if shot.walk {
            // Stand at the room's way in, looking across it.
            let c = r.center();
            let door = scene
                .structure
                .doorways
                .iter()
                .find(|d| d.rooms.0 == shot.room || d.rooms.1 == shot.room)
                .map(|d| d.center)
                .unwrap_or(c - Vec3::Z * r.size().z * 0.4);
            let from = door + (c - door).with_y(0.0).normalize_or_zero() * 2.0;
            let (p, _) = scene.nav.project(V2::new(from.x, from.z), c.y);
            let eye = Vec3::new(p.x, p.y + EYE, p.z);
            cam.look(eye, Vec3::new(c.x, p.y + EYE * 0.8, c.z));
            cam.mode = CamMode::Fly;
        } else {
            cam.frame(r.center(), r.size());
            cam.mode = CamMode::Fly;
        }
        st.samples.clear();
    }
    st.t += time.delta_secs();
    // Let models load on the first shot; then settle briefly and measure.
    let settle = if st.k == 0 { 3.0 } else { 0.6 };
    if st.t > settle - 0.4 {
        st.samples.push(time.delta_secs());
    }
    if st.t > settle {
        let path = format!("{}/{}", st.dir, shot.file);
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
        let n = st.samples.len().max(1) as f32;
        let mean = 1000.0 * st.samples.iter().sum::<f32>() / n;
        let mut v = st.samples.clone();
        v.sort_by(f32::total_cmp);
        let p95 = v
            .get(((v.len() as f32 * 0.95) as usize).min(v.len().saturating_sub(1)))
            .map(|x| x * 1000.0)
            .unwrap_or(0.0);
        st.results.push(ShotResult {
            shot,
            mean_ms: mean,
            p95_ms: p95,
            visible: stats.visible_meshes,
            meshes: stats.meshes,
        });
        st.k += 1;
        st.t = 0.0;
    }
}

fn write_tour_report(scene: &ViewerScene, st: &TourState) {
    let d = &scene.content.dungeon;
    let mut html = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>{} - viewer tour</title>\n<style>body{{background:#111;color:#ddd;font:13px sans-serif}}div.s{{display:inline-block;margin:4px;width:322px;vertical-align:top}}img{{width:320px;border:1px solid #444}}</style>\n<h2>{} ({})</h2><p>{}</p>\n",
        d.name, d.name, d.id, scene.source
    );
    let mut csv =
        String::from("file,room_id,room,role,route,view,mean_ms,p95_ms,visible_meshes,meshes\n");
    for r in &st.results {
        let room = &scene.structure.rooms[r.shot.room];
        let view = if r.shot.walk { "eye" } else { "overview" };
        html.push_str(&format!(
            "<div class=s><a href=\"{f}\"><img src=\"{f}\"></a><br>{} {} [{}] {view}<br>{:.2} ms, {} / {} meshes visible</div>\n",
            room.id,
            room.name,
            room.role,
            r.mean_ms,
            r.visible,
            r.meshes,
            f = r.shot.file
        ));
        csv.push_str(&format!(
            "{},{},\"{}\",{},{},{view},{:.3},{:.3},{},{}\n",
            r.shot.file,
            room.id,
            room.name,
            room.role,
            room.route,
            r.mean_ms,
            r.p95_ms,
            r.visible,
            r.meshes
        ));
    }
    let _ = std::fs::write(format!("{}/index.html", st.dir), html);
    let _ = std::fs::write(format!("{}/stats.csv", st.dir), csv);
}

// ------------------------------------------------------------ self test --

/// `--viewer-test`: drive the viewer through its real key handling (entrance,
/// room jump, flight, walk mode, layers, screenshot, reload, back to the hub)
/// and print a report; exit code 0 when every check held.
#[derive(Resource, Default)]
pub struct ViewerTest {
    pub on: bool,
    t: f32,
    step: usize,
    log: Vec<String>,
    ok: bool,
    mark: Vec3,
    reloads: u32,
}

impl ViewerTest {
    fn check(&mut self, ok: bool, what: String) {
        self.log
            .push(format!("{} {what}", if ok { "OK  " } else { "FAIL" }));
        self.ok &= ok;
    }
}

pub fn viewer_test_plugin(app: &mut App) {
    let on = std::env::args().any(|a| a == "--viewer-test");
    app.insert_resource(ViewerTest {
        on,
        ok: true,
        ..Default::default()
    })
    .add_systems(Update, viewer_test.before(crate::controls::ControlSet))
    .add_systems(OnEnter(AppState::Viewer), |mut t: ResMut<ViewerTest>| {
        t.reloads += 1
    });
}

#[allow(clippy::too_many_arguments)]
fn viewer_test(
    time: Res<Time>,
    mut test: ResMut<ViewerTest>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    state: Res<State<AppState>>,
    scene: Option<Res<ViewerScene>>,
    cam: Res<ViewerCam>,
    layers: Res<ViewerLayers>,
    navdbg: Res<NavDebug>,
    saves: Res<GameData>,
    mut exit: MessageWriter<AppExit>,
) {
    if !test.on {
        return;
    }
    // Taps last one frame.
    for k in [
        KeyCode::KeyR,
        KeyCode::Tab,
        KeyCode::KeyG,
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::F12,
        KeyCode::F5,
        KeyCode::Backspace,
        KeyCode::KeyF,
    ] {
        if keys.just_pressed(k) {
            keys.release(k);
        }
    }
    test.t += time.delta_secs();
    let at = |s: f32| test.t >= s;
    let floor_y = |p: Vec3| {
        scene.as_ref().and_then(|sc| {
            crate::structure::floor_below(&sc.nav, p)
                .map(|s| sc.nav.surfaces[s].height_at(V2::new(p.x, p.z)))
        })
    };
    match test.step {
        0 if at(3.0) => {
            let ok = *state.get() == AppState::Viewer && scene.is_some();
            test.check(ok, format!("viewer entered, scene loaded: {ok}"));
            keys.press(KeyCode::KeyR);
            test.step += 1;
        }
        1 if at(3.5) => {
            let sc = scene.as_ref().unwrap();
            let e = entrance_point(&sc.content, &sc.nav);
            let d = cam.pos.distance(e);
            test.check(
                d < 20.0,
                format!("R/Home: camera {d:.1} m from the entrance"),
            );
            keys.press(KeyCode::Tab);
            test.step += 1;
        }
        2 if at(4.0) => {
            let sc = scene.as_ref().unwrap();
            let k = cam.room_cursor;
            let r = k.map(|k| &sc.structure.rooms[sc.order[k]]);
            let ok = r.is_some_and(|r| cam.pos.distance(r.center()) < 80.0);
            test.check(ok, format!("Tab: jumped to room {:?}", r.map(|r| &r.name)));
            test.mark = cam.pos;
            keys.press(KeyCode::KeyW);
            test.step += 1;
        }
        3 if at(5.0) => {
            keys.release(KeyCode::KeyW);
            let d = cam.pos.distance(test.mark);
            test.check(d > 5.0, format!("W (fly) for 1 s moved {d:.1} m"));
            keys.press(KeyCode::KeyG);
            test.step += 1;
        }
        4 if at(5.5) => {
            let fy = floor_y(cam.pos);
            let ok =
                cam.mode == CamMode::Walk && fy.is_some_and(|f| (cam.pos.y - f - EYE).abs() < 0.2);
            test.check(
                ok,
                format!(
                    "G: walk mode {:?}, eye {:.2} m above the floor",
                    cam.mode,
                    fy.map(|f| cam.pos.y - f).unwrap_or(-1.0)
                ),
            );
            test.mark = cam.pos;
            keys.press(KeyCode::KeyW);
            test.step += 1;
        }
        5 if at(7.0) => {
            keys.release(KeyCode::KeyW);
            let fy = floor_y(cam.pos);
            let ok = fy.is_some_and(|f| (cam.pos.y - f - EYE).abs() < 0.2);
            let moved = cam.pos.distance(test.mark);
            test.check(
                ok && moved > 2.0,
                format!("walking 1.5 s moved {moved:.1} m and stayed on walkable floor: {ok}"),
            );
            for k in [
                KeyCode::Digit1,
                KeyCode::Digit2,
                KeyCode::Digit3,
                KeyCode::Digit4,
                KeyCode::KeyF,
            ] {
                keys.press(k);
            }
            test.step += 1;
        }
        6 if at(7.5) => {
            let ok = navdbg.on && layers.structure && layers.encounters && layers.modules;
            test.check(ok, format!("1-4: all four layers on: {ok}"));
            keys.press(KeyCode::F12);
            test.step += 1;
        }
        7 if at(8.5) => {
            test.mark = cam.pos;
            keys.press(KeyCode::F5);
            test.step += 1;
        }
        8 if at(10.5) => {
            let reloads = test.reloads;
            let ok = reloads >= 2 && scene.is_some() && cam.pos.distance(test.mark) < 0.01;
            test.check(
                ok,
                format!("F5: reloaded ({reloads} entries), camera kept: {ok}"),
            );
            keys.press(KeyCode::Backspace);
            test.step += 1;
        }
        9 if at(11.5) => {
            let ok = *state.get() == AppState::Hub;
            test.check(ok, format!("Backspace: back in the hub: {ok}"));
            let saved = saves.save_path.is_file();
            test.log.push(format!(
                "save file present afterwards: {saved} (the viewer never writes it)"
            ));
            println!("VIEWER TEST REPORT");
            for l in &test.log {
                println!("{l}");
            }
            println!("VIEWER TEST {}", if test.ok { "OK" } else { "FAILED" });
            test.step += 1;
            exit.write(if test.ok {
                AppExit::Success
            } else {
                AppExit::error()
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(rel: &str) -> ViewerScene {
        let path = format!("{}/../../assets/content/{rel}", env!("CARGO_MANIFEST_DIR"));
        let content = Content::load(&path).unwrap();
        let nav = content.nav();
        let structure = Structure::build(&content, &nav);
        let entrance = entrance_point(&content, &nav);
        let cut_off = crate::navdebug::disconnected(&nav, entrance);
        let order = (0..structure.rooms.len()).collect();
        ViewerScene {
            content,
            nav,
            structure,
            cut_off,
            path: None,
            source: rel.into(),
            error: None,
            order,
        }
    }

    fn down(at: Vec3) -> Ray3d {
        Ray3d::new(at + Vec3::Y * 30.0, Dir3::NEG_Y)
    }

    #[test]
    fn clicks_select_rooms_modules_and_mobs() {
        let sc = scene("dungeons/08_hollow_reliquary.toml");
        let layers = ViewerLayers {
            encounters: true,
            ..Default::default()
        };
        // Straight down onto the middle of the entrance room's floor.
        let k = sc
            .structure
            .rooms
            .iter()
            .position(|r| r.role == "entrance")
            .unwrap();
        let c = sc.structure.rooms[k].center();
        match pick_at(&sc, down(c), &ViewerLayers::default(), None) {
            Some(Selection::Room(i)) => assert_eq!(i, k),
            other => panic!("{other:?}"),
        }
        // A wall piece is hit before the floor behind it.
        let m = crate::modules::plan_modules_with(&sc.content, &sc.nav, &sc.structure, &|_| true);
        let w = m.0.iter().position(|i| i.kind == "wall").unwrap();
        let mid = (m.0[w].min + m.0[w].max) * 0.5;
        match pick_at(&sc, down(mid), &ViewerLayers::default(), Some(&m)) {
            Some(Selection::Module(i)) => assert_eq!(m.0[i].kind, "wall"),
            other => panic!("{other:?}"),
        }
        // A mob, with the encounter layer on.
        let mob = &sc.content.encounters[0].mobs[0];
        let (p, _) = sc.nav.project(mob.pos, mob.y.unwrap_or(0.0));
        match pick_at(&sc, down(Vec3::new(p.x, p.y, p.z)), &layers, None) {
            Some(Selection::Mob { encounter: 0, .. }) => {}
            other => panic!("{other:?}"),
        }
        let text = describe_selection(&sc, &Selection::Module(w), Some(&m));
        assert!(
            text.contains("crypt_wall") && text.contains("scale"),
            "{text}"
        );
    }

    #[test]
    fn handwritten_dungeons_get_derived_rooms_and_no_cut_off_regions() {
        for rel in [
            "dungeons/01_sunken_vault.toml",
            "dungeons/05_cavern_traverse.toml",
            "dungeons/06_cinderdeep_hollows.toml",
        ] {
            let sc = scene(rel);
            assert!(sc.structure.rooms.iter().all(|r| !r.declared));
            assert!(!sc.structure.doorways.is_empty(), "{rel}");
            let n: usize = sc.structure.rooms.iter().map(|r| r.surfaces.len()).sum();
            assert_eq!(n, sc.nav.surfaces.len(), "{rel}: every area in one room");
            assert!(!sc.cut_off.iter().any(|b| *b), "{rel}: disconnected areas");
        }
    }
}
