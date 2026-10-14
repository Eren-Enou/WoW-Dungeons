//! Dungeon Sandbox: a third-person dungeon prototype rendered with Bevy 0.20.
//!
//! All gameplay rules live in the `dungeon_sim` crate. This crate renders the
//! simulation, translates input into sim commands and provides the hub, HUD and
//! results screens.

mod camera;
mod common;
mod demo;
mod dungeon;
mod hub;
mod hud;
mod results;

use bevy::prelude::*;
use bevy::window::WindowResolution;
use common::{AppState, GameData};

fn main() {
    // `--smoke-test N`: run for N seconds then exit (used to verify the window starts).
    let smoke_seconds: Option<f32> = std::env::args()
        .skip_while(|a| a != "--smoke-test")
        .nth(1)
        .and_then(|s| s.parse().ok());

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Dungeon Sandbox".into(),
            resolution: WindowResolution::new(1280, 760),
            ..default()
        }),
        ..default()
    }))
    .insert_resource(ClearColor(Color::srgb(0.05, 0.05, 0.08)))
    .insert_resource(GameData::load())
    .init_state::<AppState>()
    .add_plugins((
        camera::CameraPlugin,
        hub::HubPlugin,
        dungeon::DungeonPlugin,
        hud::HudPlugin,
        results::ResultsPlugin,
        demo::DemoPlugin(demo::parse_args()),
    ));
    if let Some(secs) = smoke_seconds {
        app.insert_resource(common::SmokeTest {
            remaining: secs,
            frames: 0,
        })
        .add_systems(Update, common::smoke_test_exit);
    }
    app.run();
}
