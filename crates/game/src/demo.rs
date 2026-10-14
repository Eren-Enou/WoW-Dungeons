//! Developer automation for verifying the graphical build without a human.
//!
//! * `--demo`: enter the portal automatically and let the AI play the player's
//!   slot (leader autopilot), then return to the hub.
//! * `--speed X`: run the simulation X times faster (demo only).
//! * `--screenshots S`: save a screenshot to `screenshots/` every S seconds.
//! * `--smoke-test N`: exit after N seconds (see `common::smoke_test_exit`).
//! * `--input-test`: enter the dungeon and drive the *real* keyboard input
//!   systems with a scripted sequence (strafe, walk, Tab, 1/2/3), then print a
//!   report of what the sim registered and exit.

use crate::common::{AppState, GameData};
use crate::dungeon::ActiveRun;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use dungeon_sim::EventKind;

#[derive(Resource, Clone)]
pub struct DemoConfig {
    pub autoplay: bool,
    pub input_test: bool,
    pub speed: f32,
    pub screenshot_every: Option<f32>,
}

#[derive(Resource, Default)]
struct DemoState {
    state_time: f32,
    shot_timer: f32,
    shots: u32,
    runs_started: u32,
}

pub fn parse_args() -> DemoConfig {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse::<f32>().ok())
    };
    DemoConfig {
        autoplay: args.iter().any(|a| a == "--demo"),
        input_test: args.iter().any(|a| a == "--input-test"),
        speed: value("--speed").unwrap_or(1.0).clamp(0.1, 10.0),
        screenshot_every: value("--screenshots"),
    }
}

pub struct DemoPlugin(pub DemoConfig);

impl Plugin for DemoPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0.clone())
            .init_resource::<DemoState>()
            .add_systems(Update, (demo_driver, screenshots, demo_camera))
            .add_systems(
                Update,
                scripted_input
                    .before(crate::dungeon::player_input)
                    .run_if(in_state(AppState::Dungeon))
                    .run_if(resource_exists::<ActiveRun>),
            );
    }
}

fn demo_driver(
    time: Res<Time>,
    cfg: Res<DemoConfig>,
    state: Res<State<AppState>>,
    mut demo: ResMut<DemoState>,
    mut data: ResMut<GameData>,
    mut next: ResMut<NextState<AppState>>,
    mut last: Local<Option<AppState>>,
) {
    if !cfg.autoplay && !cfg.input_test {
        return;
    }
    let current = *state.get();
    if *last != Some(current) {
        *last = Some(current);
        demo.state_time = 0.0;
    }
    demo.state_time += time.delta_secs();
    match current {
        AppState::Hub if demo.state_time > 3.0 && demo.runs_started == 0 => {
            if data.reload_content() && !data.profile.party.is_empty() {
                demo.runs_started += 1;
                next.set(AppState::Dungeon);
            }
        }
        AppState::Results if demo.state_time > 5.0 => next.set(AppState::Hub),
        _ => {}
    }
}

fn screenshots(
    mut commands: Commands,
    time: Res<Time>,
    cfg: Res<DemoConfig>,
    mut demo: ResMut<DemoState>,
) {
    let Some(every) = cfg.screenshot_every else {
        return;
    };
    demo.shot_timer += time.delta_secs();
    if demo.shot_timer >= every {
        demo.shot_timer = 0.0;
        demo.shots += 1;
        let path = format!("screenshots/shot_{:03}.png", demo.shots);
        let _ = std::fs::create_dir_all("screenshots");
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
}

#[derive(Default)]
struct ScriptState {
    t: f32,
    start: Option<dungeon_sim::V2>,
    tab_pressed: u32,
}

/// Scripted keyboard sequence for `--input-test`, keyed on seconds since entering.
fn scripted_input(
    time: Res<Time>,
    cfg: Res<DemoConfig>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    run: Res<ActiveRun>,
    mut script: Local<ScriptState>,
    mut exit: MessageWriter<AppExit>,
) {
    let ScriptState {
        t,
        start,
        tab_pressed,
    } = &mut *script;
    if !cfg.input_test {
        return;
    }
    let Some(p) = run.sim.player else { return };
    let before = *t;
    *t += time.delta_secs();
    let now = *t;
    let crossed = |at: f32| before < at && now >= at;
    if start.is_none() {
        *start = Some(run.sim.actors[p].pos);
    }
    // Taps last one frame: release them before (maybe) pressing again.
    for key in [
        KeyCode::Tab,
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
    ] {
        if keys.pressed(key) {
            keys.release(key);
        }
    }
    // Camera yaw is 0: W walks +Z (north); A strafes toward +X.
    let hold = |keys: &mut ButtonInput<KeyCode>, key: KeyCode, from: f32, to: f32| {
        if now >= from && now < to {
            keys.press(key);
        } else if keys.pressed(key) && now >= to && before < to + 0.1 {
            keys.release(key);
        }
    };
    hold(&mut keys, KeyCode::KeyA, 0.5, 0.93);
    hold(&mut keys, KeyCode::KeyW, 1.0, 5.8);
    if crossed(6.5) {
        keys.press(KeyCode::Tab);
        *tab_pressed += 1;
    }
    for (i, at) in [8.0f32, 9.3, 10.6, 12.6, 13.9, 15.2, 17.2, 18.5, 19.8]
        .iter()
        .enumerate()
    {
        if crossed(*at) {
            keys.press([KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3][i % 3]);
        }
    }
    if crossed(22.0) {
        let a = &run.sim.actors[p];
        let moved = start.map(|s| s.dist(a.pos)).unwrap_or(0.0);
        let casts = run.sim.metrics.actors[p].casts;
        let failures: Vec<String> = run
            .sim
            .events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::CastFailed { src, .. } if src == p))
            .map(|e| run.sim.describe(e))
            .collect();
        let engaged = run.sim.engaged_enemies().len();
        let report = format!(
            "INPUT TEST REPORT
class: {:?}
moved: {moved:.1} m (from {:?} to {:?})
tab presses: {}
current target: {:?}
successful player casts: {casts}
player damage done: {:.0}
failed casts: {}
{}
engaged enemies: {engaged}
",
            a.class,
            start.map(|s| (s.x, s.z)),
            (a.pos.x, a.pos.z),
            *tab_pressed,
            a.target.map(|t| run.sim.actors[t].name.clone()),
            run.sim.metrics.actors[p].damage_done,
            failures.len(),
            failures.join(
                "
"
            ),
        );
        info!("{report}");
        let _ = std::fs::create_dir_all("logs");
        let _ = std::fs::write("logs/input_test.txt", report);
        exit.write(AppExit::Success);
    }
}

/// In `--demo`, swing the camera behind the player's heading so screenshots show the action.
fn demo_camera(
    time: Res<Time>,
    cfg: Res<DemoConfig>,
    run: Option<Res<ActiveRun>>,
    mut orbit: ResMut<crate::camera::OrbitCamera>,
) {
    let (true, Some(run)) = (cfg.autoplay, run) else {
        return;
    };
    let Some(p) = run.sim.player else { return };
    let target = run.sim.actors[p].facing;
    let mut diff = target - orbit.yaw;
    while diff > std::f32::consts::PI {
        diff -= std::f32::consts::TAU;
    }
    while diff < -std::f32::consts::PI {
        diff += std::f32::consts::TAU;
    }
    orbit.yaw += diff * (1.5 * time.delta_secs()).min(1.0);
}
