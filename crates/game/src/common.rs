//! Shared state, resources and small helpers.

use bevy::prelude::*;
use dungeon_sim::content::Content;
use dungeon_sim::profile::{Profile, RunRewards};
use dungeon_sim::{RunSummary, V2};
use std::path::PathBuf;

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    Hub,
    Dungeon,
    Results,
}

/// Persistent data and loaded content.
#[derive(Resource)]
pub struct GameData {
    pub profile: Profile,
    pub save_path: PathBuf,
    pub content_path: String,
    pub content: Option<Content>,
    pub content_error: Option<String>,
    /// Transient message shown in the hub (text, seconds left).
    pub message: Option<(String, f32)>,
}

impl GameData {
    pub fn load() -> Self {
        let save_path = Profile::default_path();
        let (profile, err) = Profile::load_or_default(&save_path);
        let mut data = GameData {
            profile,
            save_path,
            content_path: Content::default_path(),
            content: None,
            content_error: None,
            message: err.map(|e| (format!("Save problem: {e}"), 10.0)),
        };
        data.reload_content();
        if let Some(e) = &data.content_error {
            error!("{e}");
        }
        data
    }

    /// Re-read the content file from disk. Keeps the error for display if invalid.
    pub fn reload_content(&mut self) -> bool {
        match Content::load(&self.content_path) {
            Ok(c) => {
                info!(
                    "Loaded content '{}' (hp x{}, ring every {}s)",
                    c.dungeon.name, c.dungeon.hp_multiplier, c.boss.ring_interval
                );
                self.content = Some(c);
                self.content_error = None;
                true
            }
            Err(e) => {
                self.content = None;
                self.content_error = Some(e.to_string());
                false
            }
        }
    }

    pub fn save(&mut self) {
        if let Err(e) = self.profile.save(&self.save_path) {
            error!("{e}");
            self.message = Some((format!("Could not save: {e}"), 8.0));
        }
    }

    pub fn flash(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), 4.0));
    }
}

/// Outcome of the last run, shown on the results screen.
#[derive(Resource)]
pub struct LastResult {
    pub summary: RunSummary,
    pub rewards: RunRewards,
    pub dungeon_name: String,
    pub highlights: Vec<String>,
    pub log_path: Option<String>,
}

#[derive(Resource)]
pub struct SmokeTest {
    pub remaining: f32,
    pub frames: u64,
}

pub fn smoke_test_exit(
    time: Res<Time>,
    mut smoke: ResMut<SmokeTest>,
    mut exit: MessageWriter<AppExit>,
) {
    smoke.frames += 1;
    smoke.remaining -= time.delta_secs();
    if smoke.remaining <= 0.0 {
        info!("smoke test finished after {} frames", smoke.frames);
        exit.write(AppExit::Success);
    }
}

pub fn to_vec3(v: V2, y: f32) -> Vec3 {
    Vec3::new(v.x, y, v.z)
}

pub fn text_font(size: f32) -> TextFont {
    TextFont {
        font_size: bevy::text::FontSize::Px(size),
        ..default()
    }
}
