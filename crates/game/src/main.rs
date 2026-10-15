//! Dungeon Sandbox: a third-person dungeon prototype rendered with Bevy 0.20.
//!
//! All gameplay rules live in the `dungeon_sim` crate. This crate renders the
//! simulation, translates input into sim commands and provides the hub, HUD and
//! results screens.

mod camera;
mod cave;
mod common;
mod controls;
mod demo;
mod dungeon;
mod env;
mod fx;
mod hub;
mod hud;
mod navdebug;
mod overlay;
mod results;
mod structure;
mod tooltip;
mod viewer;

use bevy::prelude::*;
use bevy::window::WindowResolution;
use common::{AppState, GameData};

/// Content and saves are found relative to the working directory. Started
/// from a shortcut or another folder, fall back to the folder holding the
/// executable when that is where `assets/` is (the packaged layout).
fn find_game_folder() {
    let here = std::path::Path::new(dungeon_sim::content::DEFAULT_CONTENT_DIR);
    if here.is_dir() {
        return;
    }
    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    else {
        return;
    };
    if exe_dir.join(here).is_dir() && std::env::set_current_dir(&exe_dir).is_ok() {
        eprintln!("using game folder {}", exe_dir.display());
    }
}

fn main() {
    find_game_folder();
    // `--smoke-test N`: run for N seconds then exit (used to verify the window starts).
    let smoke_seconds: Option<f32> = std::env::args()
        .skip_while(|a| a != "--smoke-test")
        .nth(1)
        .and_then(|s| s.parse().ok());

    // `--start-level N`: raise the player and every companion to level N in
    // the loaded save (for playtests of higher-level dungeons). Never lowers.
    let mut data = GameData::load();
    if let Some(level) = std::env::args()
        .skip_while(|a| a != "--start-level")
        .nth(1)
        .and_then(|s| s.parse::<u32>().ok())
    {
        let level = level.clamp(1, dungeon_sim::class::MAX_LEVEL);
        let p = &mut data.profile;
        p.player_level = p.player_level.max(level);
        for c in p.roster.iter_mut() {
            c.level = c.level.max(level);
        }
        data.save();
        eprintln!("--start-level: party raised to level {level}");
    }

    let viewer_request = viewer::ViewerRequest::from_args();
    // `--no-vsync`: present as fast as possible (frame-time measurements).
    let present_mode = if std::env::args().any(|a| a == "--no-vsync") {
        bevy::window::PresentMode::AutoNoVsync
    } else {
        bevy::window::PresentMode::AutoVsync
    };
    let mut app = App::new();
    // Models load from <game folder>/assets (the working directory was set
    // to the game folder above), the same place content is read from.
    let asset_root = std::env::current_dir()
        .map(|d| d.join("assets"))
        .unwrap_or_else(|_| "assets".into());
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Dungeon Sandbox".into(),
                    resolution: WindowResolution::new(1280, 760),
                    present_mode,
                    ..default()
                }),
                ..default()
            })
            .set(bevy::asset::AssetPlugin {
                file_path: asset_root.to_string_lossy().into_owned(),
                ..default()
            }),
    )
    .insert_resource(ClearColor(Color::srgb(0.05, 0.05, 0.08)))
    .insert_resource(data)
    .init_state::<AppState>()
    .add_plugins((
        camera::CameraPlugin,
        cave::CavePlugin,
        controls::ControlsPlugin,
        tooltip::TooltipPlugin,
        hub::HubPlugin,
        dungeon::DungeonPlugin,
        hud::HudPlugin,
        navdebug::NavDebugPlugin,
        results::ResultsPlugin,
        overlay::OverlayPlugin,
        fx::FxPlugin,
        demo::DemoPlugin(demo::parse_args()),
        viewer::ViewerPlugin(viewer_request),
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
