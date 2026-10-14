//! Shared state, resources and small helpers.

use bevy::prelude::*;
use dungeon_sim::content::{Content, ContentLibrary};
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
    pub library: ContentLibrary,
    pub content_errors: Vec<String>,
    /// Transient message shown in the hub (text, seconds left).
    pub message: Option<(String, f32)>,
    /// Working copy for the in-game encounter editor.
    pub editor: Option<Content>,
}

impl GameData {
    pub fn load() -> Self {
        let save_path = Profile::default_path();
        let (profile, err) = Profile::load_or_default(&save_path);
        let mut data = GameData {
            profile,
            save_path,
            content_path: ContentLibrary::default_path(),
            library: ContentLibrary::default(),
            content_errors: Vec::new(),
            message: err.map(|e| (format!("Save problem: {e}"), 10.0)),
            editor: None,
        };
        data.reload_content();
        for e in &data.content_errors {
            error!("{e}");
        }
        data
    }

    /// Re-read every dungeon file. Returns true if there were no errors.
    pub fn reload_content(&mut self) -> bool {
        self.library = ContentLibrary::load(&self.content_path);
        self.content_errors = self.library.errors.iter().map(|e| e.to_string()).collect();
        for d in &self.library.dungeons {
            info!(
                "Loaded dungeon '{}' ({} encounters, hp x{})",
                d.dungeon.name,
                d.encounters.len(),
                d.dungeon.hp_multiplier
            );
        }
        // Keep the selection valid.
        if self.library.get(&self.profile.selected_dungeon).is_none()
            && let Some(first) = self.library.dungeons.first()
        {
            self.profile.selected_dungeon = first.dungeon.id.clone();
        }
        if let Some(c) = self.selected_content()
            && c.difficulty(&self.profile.selected_difficulty).is_none()
        {
            self.profile.selected_difficulty = c.difficulties[0].id.clone();
        }
        self.content_errors.is_empty()
    }

    /// Validate and write the editor's working copy as a dungeon file in the
    /// content directory, reload, and select it. Returns the written path.
    pub fn save_editor(&mut self) -> Result<String, String> {
        let Some(c) = &self.editor else {
            return Err("nothing to save".into());
        };
        let c = c.as_custom();
        c.validate("editor").map_err(|e| e.to_string())?;
        let text = c.to_toml()?;
        let dir = std::path::Path::new(&self.content_path);
        let dir = if dir.is_dir() {
            dir.to_path_buf()
        } else {
            dir.parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("."))
        };
        let path = dir.join(format!("90_{}.toml", c.dungeon.id));
        let header = "# Written by the in-game encounter editor. Edit freely; F5 reloads.\n";
        std::fs::write(&path, format!("{header}{text}")).map_err(|e| e.to_string())?;
        self.reload_content();
        self.profile.selected_dungeon = c.dungeon.id.clone();
        self.editor = Some(c);
        Ok(path.display().to_string())
    }

    pub fn selected_content(&self) -> Option<&Content> {
        self.library.get(&self.profile.selected_dungeon)
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
    pub difficulty_name: String,
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

pub fn rgb(c: [f32; 3]) -> Color {
    Color::srgb(c[0], c[1], c[2])
}

pub fn rarity_color(r: dungeon_sim::items::Rarity) -> Color {
    use dungeon_sim::items::Rarity::*;
    match r {
        Common => Color::srgb(0.85, 0.85, 0.85),
        Uncommon => Color::srgb(0.3, 0.95, 0.3),
        Rare => Color::srgb(0.35, 0.6, 1.0),
        Epic => Color::srgb(0.75, 0.4, 1.0),
    }
}

pub fn resource_color(r: dungeon_sim::class::ResourceKind) -> Color {
    use dungeon_sim::class::ResourceKind::*;
    match r {
        Mana => Color::srgb(0.25, 0.45, 1.0),
        Rage => Color::srgb(0.85, 0.2, 0.2),
        Energy => Color::srgb(0.95, 0.85, 0.2),
    }
}
