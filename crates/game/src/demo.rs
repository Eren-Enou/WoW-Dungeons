//! Developer automation for verifying the graphical build without a human.
//!
//! * `--demo`: enter the portal automatically and let the AI play the player's
//!   slot (leader autopilot), then return to the hub.
//! * `--speed X`: run the simulation X times faster (demo only).
//! * `--screenshots S`: save a screenshot to `screenshots/` every S seconds.
//! * `--smoke-test N`: exit after N seconds (see `common::smoke_test_exit`).

use crate::common::{AppState, GameData};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

#[derive(Resource, Clone)]
pub struct DemoConfig {
    pub autoplay: bool,
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
        speed: value("--speed").unwrap_or(1.0).clamp(0.1, 10.0),
        screenshot_every: value("--screenshots"),
    }
}

pub struct DemoPlugin(pub DemoConfig);

impl Plugin for DemoPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0.clone())
            .init_resource::<DemoState>()
            .add_systems(Update, (demo_driver, screenshots));
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
    if !cfg.autoplay {
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
