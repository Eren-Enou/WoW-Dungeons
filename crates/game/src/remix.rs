//! Seeded dungeon entry (0.9): for dungeons made by Dungeon Forge, the hub
//! offers the Classic (shipped) layout, a Remix (a new random seed) or a
//! specific Seed.
//!
//! The game does not link the generator: it runs the `dungeon-forge`
//! executable (beside the game, or under `target/` in development) on the
//! blueprint the shipped file names, writes the result into a managed
//! folder (`generated/` beside the save file) and loads it through the same
//! validator as every other dungeon. The dungeon keeps its id, so progress,
//! loot and companions' familiarity carry over; the save is never touched.
//! A seed that cannot be generated is reported as such: nothing else is
//! substituted for it.

use dungeon_sim::content::Content;
use std::path::{Path, PathBuf};

/// Which layout the next run of the selected dungeon uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    Classic,
    Remix,
    Seed,
}

/// The layout chosen for one dungeon.
#[derive(Debug, Clone)]
pub struct LayoutChoice {
    pub dungeon: String,
    pub mode: LayoutMode,
    pub seed: Option<u64>,
    /// The generated dungeon (remix and seed), loaded and validated.
    pub content: Option<Content>,
    pub file: Option<PathBuf>,
    /// Why the requested seed could not be used.
    pub error: Option<String>,
}

impl LayoutChoice {
    pub fn classic(dungeon: &str) -> Self {
        LayoutChoice {
            dungeon: dungeon.to_string(),
            mode: LayoutMode::Classic,
            seed: None,
            content: None,
            file: None,
            error: None,
        }
    }

    /// "classic layout" or "remix seed 4821" / "seed 7", for the hub, the
    /// HUD, the results screen and the run log.
    pub fn label(&self) -> String {
        match (self.mode, self.seed, &self.content) {
            (LayoutMode::Classic, ..) | (_, None, _) | (_, _, None) => "classic layout".into(),
            (LayoutMode::Remix, Some(s), Some(c)) => format!(
                "remix, seed {s} (generator v{})",
                c.dungeon
                    .generated
                    .as_ref()
                    .map(|g| g.generator)
                    .unwrap_or(0)
            ),
            (LayoutMode::Seed, Some(s), Some(c)) => format!(
                "seed {s} (generator v{})",
                c.dungeon
                    .generated
                    .as_ref()
                    .map(|g| g.generator)
                    .unwrap_or(0)
            ),
        }
    }
}

/// Dungeons that can be remixed: generated, with a blueprint we can find.
pub fn remixable(content: &Content) -> Option<PathBuf> {
    let info = content.dungeon.generated.as_ref()?;
    if info.blueprint_path.is_empty() {
        return None;
    }
    let rel = Path::new(&info.blueprint_path);
    let mut candidates = vec![rel.to_path_buf()];
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        candidates.push(dir.join(rel));
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Where generated layouts are written: `generated/` beside the save file.
pub fn generated_dir(save_path: &Path) -> PathBuf {
    save_path
        .parent()
        .map(|d| d.join("generated"))
        .unwrap_or_else(|| PathBuf::from("generated"))
}

/// A seed for "Remix": different each time, reproducible once shown.
pub fn random_seed() -> u64 {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    // Mix the clock so consecutive presses differ; keep seeds short to type.
    let mut h = t ^ 0x9E37_79B9_7F4A_7C15;
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    1 + h % 99_999
}

/// Generate `blueprint` at `seed` with the external forge and load it.
pub fn generate(
    blueprint: &Path,
    seed: u64,
    out_dir: &Path,
    dungeon_id: &str,
) -> Result<(Content, PathBuf), String> {
    let exe = crate::viewer::forge_exe().ok_or_else(|| {
        "dungeon-forge was not found beside the game (or under target/); it is needed to make new layouts".to_string()
    })?;
    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("could not create {}: {e}", out_dir.display()))?;
    let out = out_dir.join(format!("{dungeon_id}_s{seed}.toml"));
    let status = std::process::Command::new(&exe)
        .arg("generate")
        .arg(blueprint)
        .args(["--seed", &seed.to_string(), "--quiet", "--out"])
        .arg(&out)
        .output()
        .map_err(|e| format!("could not run {}: {e}", exe.display()))?;
    if !status.status.success() {
        let err = String::from_utf8_lossy(&status.stderr);
        let first = err
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("no message")
            .trim_start_matches("error: ");
        // The forge lists every cause; the hub shows the headline.
        let first = first.split(" (").next().unwrap_or(first);
        return Err(format!("seed {seed} could not be generated: {first}"));
    }
    let content = Content::load(&out).map_err(|e| format!("seed {seed}: {e}"))?;
    if content.dungeon.id != dungeon_id {
        return Err(format!(
            "seed {seed}: the blueprint makes '{}', not '{dungeon_id}'",
            content.dungeon.id
        ));
    }
    Ok((content, out))
}

// ---------------------------------------------------------- self-test --

use crate::common::{AppState, GameData};
use crate::hub::{HubAction, HubUi};
use bevy::prelude::*;

/// `--remix-test`: drive the hub's layout controls with real key presses,
/// play a seeded layout and come back. Exits 0 when every check passes.
#[derive(Resource, Default)]
pub struct RemixTest {
    step: u32,
    t: f32,
    ok: bool,
    log: Vec<String>,
    file_text: String,
    content_files: usize,
}

impl RemixTest {
    fn check(&mut self, ok: bool, what: String) {
        self.log
            .push(format!("{} {what}", if ok { "OK  " } else { "FAIL" }));
        self.ok &= ok;
    }
}

pub fn remix_test_plugin(app: &mut App) {
    if std::env::args().any(|a| a == "--remix-test") {
        app.insert_resource(RemixTest {
            ok: true,
            ..Default::default()
        })
        .add_systems(Update, remix_test.before(crate::controls::ControlSet));
    }
}

fn toml_files(dir: &str) -> usize {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
                .count()
        })
        .unwrap_or(0)
}

#[allow(clippy::too_many_arguments)]
fn remix_test(
    time: Res<Time>,
    mut test: ResMut<RemixTest>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut data: ResMut<GameData>,
    mut ui: ResMut<HubUi>,
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
    run: Option<Res<crate::dungeon::ActiveRun>>,
    mut exit: MessageWriter<AppExit>,
) {
    test.t += time.delta_secs();
    for k in [
        KeyCode::Digit1,
        KeyCode::Digit0,
        KeyCode::Enter,
        KeyCode::Backspace,
    ] {
        if keys.pressed(k) {
            keys.release(k);
        }
    }
    let at = |s: f32, t: &RemixTest| t.t >= s;
    let step = test.step;
    match step {
        0 if at(1.0, &test) && *state.get() == AppState::Hub => {
            test.content_files = toml_files(&data.content_path);
            crate::hub::apply_action(
                HubAction::SelectDungeon("hollow_reliquary".into()),
                &mut data,
                &mut ui,
            );
            ui.tab = crate::hub::Tab::Dungeons;
            ui.seed_input.clear();
            crate::hub::apply_action(HubAction::LayoutSeedFocus, &mut data, &mut ui);
            test.step += 1;
        }
        1 if at(1.3, &test) => {
            keys.press(KeyCode::Digit1);
            test.step += 1;
        }
        2 if at(1.6, &test) => {
            keys.press(KeyCode::Digit0);
            test.step += 1;
        }
        3 if at(1.9, &test) => {
            let typed = ui.seed_input.clone();
            test.check(typed == "10", format!("typed seed reads '{typed}'"));
            keys.press(KeyCode::Enter);
            test.step += 1;
        }
        4 if at(2.6, &test) => {
            let l = data.selected_layout().cloned();
            let file = l.as_ref().and_then(|l| l.file.clone());
            let in_generated = file
                .as_ref()
                .is_some_and(|f| f.parent() == Some(generated_dir(&data.save_path).as_path()));
            test.check(
                l.as_ref()
                    .is_some_and(|l| l.content.is_some() && l.seed == Some(10)),
                format!(
                    "Enter: seed 10 built ({})",
                    l.as_ref().map(|l| l.label()).unwrap_or_default()
                ),
            );
            test.check(
                in_generated,
                format!("written to the managed folder: {file:?}"),
            );
            test.file_text = file
                .as_ref()
                .and_then(|f| std::fs::read_to_string(f).ok())
                .unwrap_or_default();
            let classic_rooms = data.selected_content().map(|c| c.dungeon.rooms.len());
            let seeded_rooms = data.run_content().map(|c| c.dungeon.rooms.len());
            let same_areas = data.selected_content().map(|c| c.dungeon.areas.len())
                == data.run_content().map(|c| c.dungeon.areas.len())
                && classic_rooms == seeded_rooms;
            test.check(
                !same_areas,
                format!("the seeded layout differs from the classic one ({classic_rooms:?} vs {seeded_rooms:?} rooms)"),
            );
            // The same seed again builds the same file.
            crate::hub::apply_action(HubAction::LayoutSeedGo, &mut data, &mut ui);
            test.step += 1;
        }
        5 if at(3.2, &test) => {
            let again = data
                .selected_layout()
                .and_then(|l| l.file.as_ref())
                .and_then(|f| std::fs::read_to_string(f).ok())
                .unwrap_or_default();
            let same = !again.is_empty() && again == test.file_text;
            test.check(
                same,
                "seed 10 again: byte-identical file (reproducible)".into(),
            );
            // A seed that cannot be built: say so, keep the classic layout.
            crate::hub::apply_action(
                HubAction::SelectDungeon("emberforge_vaults".into()),
                &mut data,
                &mut ui,
            );
            ui.seed_input = "3".into();
            crate::hub::apply_action(HubAction::LayoutSeedGo, &mut data, &mut ui);
            let l = data.selected_layout().cloned();
            let classic = data
                .run_content()
                .map(|c| c.dungeon.generated.as_ref().map(|g| g.seed));
            test.check(
                l.as_ref().is_some_and(|l| {
                    l.content.is_none() && l.error.as_deref().is_some_and(|e| e.contains("seed 3"))
                }),
                format!(
                    "Emberforge seed 3 (cannot be built): {:?}",
                    l.as_ref().and_then(|l| l.error.clone())
                ),
            );
            test.check(
                classic == Some(Some(1)),
                format!("...and the classic layout stays selected (seed {classic:?})"),
            );
            crate::hub::apply_action(HubAction::LayoutRemix, &mut data, &mut ui);
            let l = data.selected_layout().cloned();
            test.check(
                l.as_ref()
                    .is_some_and(|l| l.content.is_some() && l.mode == LayoutMode::Remix)
                    || l.as_ref().is_some_and(|l| l.error.is_some()),
                format!(
                    "Remix: {}",
                    l.as_ref()
                        .map(|l| l.error.clone().unwrap_or_else(|| l.label()))
                        .unwrap_or_default()
                ),
            );
            // Play seed 10 of the Reliquary.
            crate::hub::apply_action(
                HubAction::SelectDungeon("hollow_reliquary".into()),
                &mut data,
                &mut ui,
            );
            ui.seed_input = "10".into();
            crate::hub::apply_action(HubAction::LayoutSeedGo, &mut data, &mut ui);
            next.set(AppState::Dungeon);
            test.step += 1;
        }
        6 if at(5.5, &test) && *state.get() == AppState::Dungeon => {
            let label = run.as_ref().map(|r| r.layout.clone()).unwrap_or_default();
            let seed = run
                .as_ref()
                .and_then(|r| r.sim.content.dungeon.generated.as_ref().map(|g| g.seed));
            test.check(
                label == "seed 10 (generator v4)" && seed == Some(10),
                format!("in the dungeon: '{label}', content seed {seed:?}"),
            );
            let caches = run.as_ref().map(|r| r.sim.caches.len()).unwrap_or(0);
            test.check(caches > 0, format!("{caches} treasure cache(s) in the run"));
            keys.press(KeyCode::Backspace);
            test.step += 1;
        }
        7 if *state.get() == AppState::Results => {
            keys.press(KeyCode::Enter);
            test.step += 1;
        }
        8 if *state.get() == AppState::Hub => {
            let (p, err) = dungeon_sim::profile::Profile::load_or_default(&data.save_path);
            test.check(
                err.is_none() && data.save_path.is_file(),
                format!(
                    "back in the hub; the save loads ({} runs recorded, problem: {err:?})",
                    p.runs_won + p.runs_wiped
                ),
            );
            let files = toml_files(&data.content_path);
            let same_count = files == test.content_files;
            test.check(
                same_count,
                format!("authored content untouched ({files} dungeon files, as before)"),
            );
            println!("REMIX TEST REPORT");
            for l in &test.log {
                println!("{l}");
            }
            println!("REMIX TEST {}", if test.ok { "OK" } else { "FAILED" });
            exit.write(if test.ok {
                AppExit::Success
            } else {
                AppExit::error()
            });
            test.step += 1;
        }
        _ => {}
    }
    if test.t > 90.0 && test.step < 9 {
        println!("REMIX TEST: timed out at step {}", test.step);
        for l in &test.log {
            println!("{l}");
        }
        exit.write(AppExit::error());
        test.step = 99;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_layouts_live_beside_the_save_not_in_it() {
        let save = Path::new("some/dir/profile.json");
        assert_eq!(generated_dir(save), Path::new("some/dir/generated"));
        let a = random_seed();
        assert!((1..=99_999).contains(&a));
    }

    #[test]
    fn shipped_forge_dungeons_are_remixable_and_handwritten_ones_are_not() {
        let dir = format!(
            "{}/../../assets/content/dungeons",
            env!("CARGO_MANIFEST_DIR")
        );
        let load = |f: &str| Content::load(format!("{dir}/{f}")).unwrap();
        // The blueprint path is relative to the repository root (the working
        // directory of the game); tests run from the crate directory.
        let c = load("08_hollow_reliquary.toml");
        let info = c.dungeon.generated.as_ref().unwrap();
        assert!(info.blueprint_path.ends_with("hollow_reliquary.toml"));
        assert!(load("01_sunken_vault.toml").dungeon.generated.is_none());
        assert!(remixable(&load("01_sunken_vault.toml")).is_none());
        let mut l = LayoutChoice::classic("hollow_reliquary");
        assert_eq!(l.label(), "classic layout");
        l.mode = LayoutMode::Seed;
        l.seed = Some(7);
        l.content = Some(c);
        assert_eq!(l.label(), "seed 7 (generator v4)");
    }
}
