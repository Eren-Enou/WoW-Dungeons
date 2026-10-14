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
//! * `--tooltip-test`: hover a stat on the Character tab and a vendor item with
//!   synthetic pointer moves; check the tooltip text and that it stays on screen.
//! * `--frame-test`: play a Mender, target an enemy, then use synthetic mouse
//!   events to hover a party frame and press 1 (mouseover heal), click another
//!   frame, and report whether the heal, selection and click-through rules held.

use crate::common::{AppState, GameData};
use crate::dungeon::ActiveRun;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use dungeon_sim::EventKind;

#[derive(Resource, Clone)]
pub struct DemoConfig {
    pub autoplay: bool,
    pub input_test: bool,
    /// Cycle through every hub tab taking screenshots, then exit.
    pub hub_tour: bool,
    /// Click the History tab with synthetic pointer events and report the result.
    pub click_test: bool,
    /// Hover/click party frames in the dungeon with synthetic pointer events.
    pub frame_test: bool,
    /// Hover hub elements and check the tooltip panel.
    pub tooltip_test: bool,
    /// Edit a boss through the editor actions, save, and verify the reload.
    pub editor_test: bool,
    pub speed: f32,
    pub screenshot_every: Option<f32>,
    /// Demo overrides for the selected dungeon / difficulty / party level.
    pub dungeon: Option<String>,
    pub difficulty: Option<String>,
    pub level: Option<u32>,
    pub group: Option<u32>,
    /// Player class for demo runs (`--class warden`).
    pub class: Option<String>,
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
    let text = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    DemoConfig {
        autoplay: args.iter().any(|a| a == "--demo"),
        input_test: args.iter().any(|a| a == "--input-test"),
        hub_tour: args.iter().any(|a| a == "--hub-tour"),
        click_test: args.iter().any(|a| a == "--click-test"),
        frame_test: args.iter().any(|a| a == "--frame-test"),
        tooltip_test: args.iter().any(|a| a == "--tooltip-test"),
        editor_test: args.iter().any(|a| a == "--editor-test"),
        speed: value("--speed").unwrap_or(1.0).clamp(0.1, 10.0),
        screenshot_every: value("--screenshots"),
        dungeon: text("--dungeon"),
        difficulty: text("--difficulty"),
        level: value("--level").map(|l| l as u32),
        group: value("--group").map(|g| g as u32),
        class: text("--class"),
    }
}

pub struct DemoPlugin(pub DemoConfig);

impl Plugin for DemoPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0.clone())
            .init_resource::<DemoState>()
            .add_systems(
                Update,
                (
                    demo_driver,
                    screenshots,
                    demo_camera,
                    hub_tour,
                    click_test,
                    editor_test,
                    tooltip_test,
                ),
            )
            .add_systems(
                Update,
                frame_test
                    .before(crate::controls::ControlSet)
                    .run_if(in_state(AppState::Dungeon))
                    .run_if(resource_exists::<ActiveRun>),
            )
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
    if !cfg.autoplay && !cfg.input_test && !cfg.frame_test {
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
            if let Some(d) = &cfg.dungeon {
                data.profile.selected_dungeon = d.clone();
            }
            if let Some(d) = &cfg.difficulty {
                data.profile.selected_difficulty = d.clone();
            }
            if let Some(g) = cfg.group {
                data.profile.set_group_size(g);
            }
            if let Some(name) = &cfg.class
                && let Some(c) = dungeon_sim::class::ClassId::ALL
                    .into_iter()
                    .find(|c| c.label().eq_ignore_ascii_case(name))
            {
                data.profile.player_class = c;
                data.profile.auto_fill_party();
            }
            if cfg.frame_test {
                data.profile.player_class = dungeon_sim::class::ClassId::Mender;
                data.profile.auto_fill_party();
            }
            if let Some(l) = cfg.level {
                data.profile.player_level = l;
                for c in data.profile.roster.iter_mut() {
                    c.level = l;
                }
            }
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
#[allow(clippy::too_many_arguments)]
fn scripted_input(
    time: Res<Time>,
    cfg: Res<DemoConfig>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    run: Res<ActiveRun>,
    pc: Res<crate::controls::PlayerControl>,
    orbit: Res<crate::camera::OrbitCamera>,
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
    // Facing is 0: W walks +Z (north); Q strafes left, toward +X.
    let hold = |keys: &mut ButtonInput<KeyCode>, key: KeyCode, from: f32, to: f32| {
        if now >= from && now < to {
            keys.press(key);
        } else if keys.pressed(key) && now >= to && before < to + 0.1 {
            keys.release(key);
        }
    };
    hold(&mut keys, KeyCode::KeyQ, 0.5, 0.93);
    hold(&mut keys, KeyCode::KeyW, 1.0, 5.8);
    // Turn right while walking: the camera should swing behind (auto-follow).
    hold(&mut keys, KeyCode::KeyD, 4.4, 4.8);
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
        let report = [
            "INPUT TEST REPORT".to_string(),
            format!("class: {:?}", a.class),
            format!(
                "moved: {moved:.1} m (from {:?} to {:?})",
                start.map(|s| (s.x, s.z)),
                (a.pos.x, a.pos.z)
            ),
            format!("tab presses: {}", *tab_pressed),
            format!(
                "current target: {:?}",
                a.target.map(|t| run.sim.actors[t].name.clone())
            ),
            format!("successful player casts: {casts}"),
            format!(
                "player damage done: {:.0}",
                run.sim.metrics.actors[p].damage_done
            ),
            format!("failed casts: {}", failures.len()),
            failures.join("\n"),
            format!("engaged enemies: {engaged}"),
            format!(
                "facing {:.2} rad, camera yaw {:.2} rad, follow gap {:.2} rad",
                pc.state.facing,
                orbit.yaw,
                crate::controls::angle_diff(orbit.yaw, pc.state.facing).abs()
            ),
        ]
        .join("\n");
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

/// `--hub-tour`: show each hub tab for 1.5 s (screenshot each), inspecting a
/// companion on the Character tab, then exit.
#[allow(clippy::too_many_arguments)]
fn hub_tour(
    mut commands: Commands,
    time: Res<Time>,
    cfg: Res<DemoConfig>,
    state: Res<State<AppState>>,
    mut data: ResMut<GameData>,
    mut ui: ResMut<crate::hub::HubUi>,
    mut t: Local<f32>,
    mut step: Local<usize>,
    mut exit: MessageWriter<AppExit>,
) {
    if !cfg.hub_tour || *state.get() != AppState::Hub {
        return;
    }
    *t += time.delta_secs();
    if *t < 1.5 {
        return;
    }
    *t = 0.0;
    use crate::hub::Tab;
    let plan = [
        Tab::Party,
        Tab::Character,
        Tab::Character,
        Tab::Dungeons,
        Tab::Classes,
        Tab::History,
        Tab::Sandbox,
        Tab::Vendor,
        Tab::Editor,
    ];
    if *step >= plan.len() {
        exit.write(AppExit::Success);
        return;
    }
    if *step == 2 {
        // Second Character shot: inspect the first recruited companion.
        let first = data.profile.party.first().cloned();
        if let Some(i) = first.and_then(|id| data.profile.roster.iter().position(|c| c.id == id)) {
            ui.wearer = dungeon_sim::profile::Wearer::Companion(i);
        }
    }
    if *step == 8 {
        crate::hub::apply_action(crate::hub::HubAction::EditStart, &mut data, &mut ui);
    }
    if *step == 3 {
        // Exercise the same action path a button click uses.
        let other = data
            .library
            .dungeons
            .iter()
            .map(|d| d.dungeon.id.clone())
            .find(|id| *id != data.profile.selected_dungeon);
        if let Some(id) = other {
            crate::hub::apply_action(crate::hub::HubAction::SelectDungeon(id), &mut data, &mut ui);
        }
    }
    ui.tab = plan[*step];
    ui.dirty = true;
    let path = format!("screenshots/tour_{}.png", *step);
    let _ = std::fs::create_dir_all("screenshots");
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
    *step += 1;
}

/// `--click-test`: drive Bevy's picking pipeline with synthetic mouse events
/// (move, press, release) over the hub's "History" tab button, then check that
/// the button's `Activate` observer switched the tab.
#[allow(clippy::too_many_arguments)]
fn click_test(
    time: Res<Time>,
    cfg: Res<DemoConfig>,
    state: Res<State<AppState>>,
    ui: Res<crate::hub::HubUi>,
    buttons: Query<(&crate::hub::HubAction, &bevy::ui::UiGlobalTransform)>,
    window: Single<(Entity, &Window)>,
    mut pointer: MessageWriter<bevy::picking::pointer::PointerInput>,
    mut t: Local<f32>,
    mut step: Local<u32>,
    mut exit: MessageWriter<AppExit>,
) {
    use bevy::camera::RenderTarget;
    use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput};
    use bevy::window::WindowRef;
    if !cfg.click_test || *state.get() != AppState::Hub {
        return;
    }
    *t += time.delta_secs();
    if *t < 2.0 {
        return;
    }
    let (win_entity, win) = *window;
    let Some((_, gt)) = buttons
        .iter()
        .find(|(a, _)| matches!(a, crate::hub::HubAction::Tab(crate::hub::Tab::History)))
    else {
        return;
    };
    let position = gt.translation / win.scale_factor();
    let Some(target) =
        RenderTarget::Window(WindowRef::Entity(win_entity)).normalize(Some(win_entity))
    else {
        return;
    };
    let location = Location { target, position };
    let action = match *step {
        0 => Some(PointerAction::Move { delta: Vec2::ZERO }),
        1 => Some(PointerAction::Press(PointerButton::Primary)),
        2 => Some(PointerAction::Release(PointerButton::Primary)),
        _ => None,
    };
    if let Some(action) = action {
        pointer.write(PointerInput::new(PointerId::Mouse, location, action));
        *step += 1;
        return;
    }
    *step += 1;
    if *step > 30 {
        let ok = ui.tab == crate::hub::Tab::History;
        let msg = format!(
            "CLICK TEST {}: clicked History tab at {position:?}, current tab {:?}",
            if ok { "OK" } else { "FAILED" },
            ui.tab
        );
        info!("{msg}");
        let _ = std::fs::create_dir_all("logs");
        let _ = std::fs::write("logs/click_test.txt", msg);
        exit.write(AppExit::Success);
    }
}

#[derive(Default)]
struct FrameTest {
    t: f32,
    step: u32,
    enemy: Option<dungeon_sim::ActorId>,
    log: Vec<String>,
    ok: bool,
}

/// `--frame-test`: see the module docs. Runs before the control systems so the
/// synthetic pointer events and key presses are seen the same frame.
#[allow(clippy::too_many_arguments)]
fn frame_test(
    time: Res<Time>,
    cfg: Res<DemoConfig>,
    mut run: ResMut<ActiveRun>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    frames: Query<(&crate::hud::HudClick, &bevy::ui::UiGlobalTransform)>,
    slots: Query<(&crate::tooltip::Tooltip, &bevy::ui::UiGlobalTransform)>,
    mouseover: Res<crate::hud::Mouseover>,
    pc: Res<crate::controls::PlayerControl>,
    window: Single<(Entity, &Window)>,
    mut pointer: MessageWriter<bevy::picking::pointer::PointerInput>,
    mut commands: Commands,
    mut st: Local<FrameTest>,
    mut exit: MessageWriter<AppExit>,
) {
    use crate::hud::HudClick;
    use bevy::camera::RenderTarget;
    use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput};
    use bevy::window::WindowRef;
    if !cfg.frame_test {
        return;
    }
    if keys.pressed(KeyCode::Digit1) {
        keys.release(KeyCode::Digit1);
    }
    st.t += time.delta_secs();
    let Some(p) = run.sim.player else { return };
    let party = run.sim.party.clone();
    let (win_entity, win) = *window;
    let Some(target) =
        RenderTarget::Window(WindowRef::Entity(win_entity)).normalize(Some(win_entity))
    else {
        return;
    };
    let slot_pos = slots
        .iter()
        .find(|(t, _)| matches!(t, crate::tooltip::Tooltip::Ability(0)))
        .map(|(_, gt)| gt.translation / win.scale_factor());
    let frame_pos = |i: usize| {
        frames
            .iter()
            .find(|(c, _)| **c == HudClick::Party(i))
            .map(|(_, gt)| gt.translation / win.scale_factor())
    };
    let mut send = |pos: Vec2, action: PointerAction| {
        pointer.write(PointerInput::new(
            PointerId::Mouse,
            Location {
                target: target.clone(),
                position: pos,
            },
            action,
        ));
    };
    // The healer must be in range: put the hurt ally next to the player.
    let ally = party.iter().copied().find(|&a| a != p).unwrap_or(p);
    let other = party
        .iter()
        .copied()
        .find(|&a| a != p && a != ally)
        .unwrap_or(p);
    let (Some(ally_i), Some(other_i)) = (
        party.iter().position(|&a| a == ally),
        party.iter().position(|&a| a == other),
    ) else {
        return;
    };
    match st.step {
        0 if st.t > 2.0 => {
            let enemy = run
                .sim
                .enemies
                .iter()
                .copied()
                .find(|&e| run.sim.actors[e].alive);
            st.enemy = enemy;
            run.sim.set_player_target(enemy);
            let ppos = run.sim.actors[p].pos;
            let a = &mut run.sim.actors[ally];
            a.pos = ppos + dungeon_sim::V2::new(2.0, 0.0);
            a.hp = a.max_hp * 0.4;
            st.step = 1;
        }
        1 => {
            if let Some(pos) = frame_pos(ally_i) {
                send(pos, PointerAction::Move { delta: Vec2::ZERO });
                st.step = 2;
                st.t = 0.0;
            }
        }
        2 if st.t > 0.3 => {
            st.log.push(format!(
                "hover frame {ally_i}: mouseover {:?} (expected {:?}), cursor_over_ui {}",
                mouseover.0,
                Some(ally),
                pc.cursor_over_ui
            ));
            st.ok = mouseover.0 == Some(ally) && pc.cursor_over_ui;
            keys.press(KeyCode::Digit1);
            st.step = 3;
            st.t = 0.0;
        }
        3 if st.t > 3.5 => {
            let healed = run.sim.events.iter().any(|e| {
                matches!(e.kind, dungeon_sim::EventKind::Heal { src, dst, .. } if src == p && dst == ally)
            });
            let kept = run.sim.actors[p].target == st.enemy;
            st.log.push(format!(
                "pressed 1 (Mend) with an enemy selected: healed hovered ally {healed}, selection kept on enemy {kept}"
            ));
            st.ok &= healed && kept;
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("screenshots/frame_test.png"));
            if let Some(pos) = frame_pos(other_i) {
                send(pos, PointerAction::Move { delta: Vec2::ZERO });
                send(pos, PointerAction::Press(PointerButton::Primary));
            }
            st.step = 4;
            st.t = 0.0;
        }
        4 if st.t > 0.1 => {
            if let Some(pos) = frame_pos(other_i) {
                send(pos, PointerAction::Release(PointerButton::Primary));
            }
            st.step = 5;
            st.t = 0.0;
        }
        5 if st.t > 0.5 => {
            let sel = run.sim.actors[p].target;
            st.log.push(format!(
                "clicked frame {other_i}: selected {:?} (expected {:?}); world click consumed by UI: {}",
                sel.map(|t| run.sim.actors[t].name.clone()),
                run.sim.actors[other].name,
                pc.world_click.is_none()
            ));
            st.ok &= sel == Some(other);
            if let Some(pos) = slot_pos {
                send(pos, PointerAction::Move { delta: Vec2::ZERO });
            }
            st.step = 50;
            st.t = 0.0;
        }
        50 if st.t > 0.8 => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("screenshots/frame_test_tooltip.png"));
            let msg = format!(
                "FRAME TEST {}\n{}",
                if st.ok { "OK" } else { "FAILED" },
                st.log.join("\n")
            );
            info!("{msg}");
            let _ = std::fs::create_dir_all("logs");
            let _ = std::fs::write("logs/frame_test.txt", msg);
            st.step = 6;
            st.t = 0.0;
        }
        6 if st.t > 1.0 => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

#[derive(Default)]
struct TooltipTest {
    t: f32,
    step: u32,
    log: Vec<String>,
    ok: bool,
}

/// `--tooltip-test`: see the module docs.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn tooltip_test(
    time: Res<Time>,
    cfg: Res<DemoConfig>,
    state: Res<State<AppState>>,
    mut data: ResMut<GameData>,
    mut ui: ResMut<crate::hub::HubUi>,
    sources: Query<(
        &crate::tooltip::Tooltip,
        &bevy::ui::UiGlobalTransform,
        Option<&crate::hub::HubAction>,
    )>,
    panel: Query<
        (&Node, &bevy::ui::ComputedNode, &bevy::ui::UiGlobalTransform),
        With<crate::tooltip::TooltipPanel>,
    >,
    title: Query<&Text, With<crate::tooltip::TooltipTitle>>,
    window: Single<(Entity, &Window)>,
    mut pointer: MessageWriter<bevy::picking::pointer::PointerInput>,
    mut commands: Commands,
    mut st: Local<TooltipTest>,
    mut exit: MessageWriter<AppExit>,
) {
    use crate::hub::{HubAction, Tab, apply_action};
    use crate::tooltip::Tooltip;
    use bevy::camera::RenderTarget;
    use bevy::picking::pointer::{Location, PointerAction, PointerId, PointerInput};
    use bevy::window::WindowRef;
    if !cfg.tooltip_test || *state.get() != AppState::Hub {
        return;
    }
    st.t += time.delta_secs();
    let (win_entity, win) = *window;
    let Some(target) =
        RenderTarget::Window(WindowRef::Entity(win_entity)).normalize(Some(win_entity))
    else {
        return;
    };
    let mut hover = |pos: Vec2| {
        pointer.write(PointerInput::new(
            PointerId::Mouse,
            Location {
                target: target.clone(),
                position: pos,
            },
            PointerAction::Move { delta: Vec2::ZERO },
        ));
    };
    let check = |st: &mut TooltipTest, what: &str, want: &str| {
        let shown = panel.single().ok();
        let title = title.single().map(|t| t.0.clone()).unwrap_or_default();
        let (visible, inside) = match shown {
            Some((node, cn, gt)) => {
                let half = cn.size() * cn.inverse_scale_factor() * 0.5;
                let c = gt.translation / win.scale_factor();
                (
                    node.display != Display::None,
                    c.x - half.x >= 0.0
                        && c.y - half.y >= 0.0
                        && c.x + half.x <= win.width()
                        && c.y + half.y <= win.height(),
                )
            }
            None => (false, false),
        };
        let ok = visible && inside && title.contains(want);
        st.log.push(format!(
            "{what}: visible {visible}, on screen {inside}, title {title:?} {}",
            if ok { "OK" } else { "FAILED" }
        ));
        st.ok &= ok;
    };
    match st.step {
        0 if st.t > 2.0 => {
            st.ok = true;
            apply_action(HubAction::Tab(Tab::Character), &mut data, &mut ui);
            st.step = 1;
            st.t = 0.0;
        }
        1 if st.t > 0.5 => {
            let pos = sources.iter().find_map(|(tip, gt, _)| match tip {
                Tooltip::Text(t) if t.title == "armor" => Some(gt.translation / win.scale_factor()),
                _ => None,
            });
            if let Some(pos) = pos {
                hover(pos);
                st.step = 2;
                st.t = 0.0;
            }
        }
        2 if st.t > 0.8 => {
            check(&mut st, "armor stat", "armor");
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("screenshots/tooltip_stat.png"));
            apply_action(HubAction::Tab(Tab::Vendor), &mut data, &mut ui);
            st.step = 3;
            st.t = 0.0;
        }
        3 if st.t > 0.5 => {
            let pos = sources.iter().find_map(|(_, gt, a)| match a {
                Some(HubAction::Buy(_)) => Some(gt.translation / win.scale_factor()),
                _ => None,
            });
            if let Some(pos) = pos {
                hover(pos);
                st.step = 4;
                st.t = 0.0;
            }
        }
        4 if st.t > 0.8 => {
            check(&mut st, "vendor item", "");
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk("screenshots/tooltip_vendor.png"));
            st.step = 5;
            st.t = 0.0;
        }
        5 if st.t > 0.5 => {
            let msg = format!(
                "TOOLTIP TEST {}\n{}",
                if st.ok { "OK" } else { "FAILED" },
                st.log.join("\n")
            );
            info!("{msg}");
            let _ = std::fs::create_dir_all("logs");
            let _ = std::fs::write("logs/tooltip_test.txt", msg);
            exit.write(AppExit::Success);
            st.step = 6;
        }
        _ => {}
    }
}

/// `--editor-test`: drive the encounter editor through its hub actions (start a
/// copy, shorten the first boss ability's interval, raise boss HP, save) and check
/// the saved dungeon is in the reloaded library. Run with `DUNGEON_CONTENT`
/// pointing at a scratch copy of the content directory.
fn editor_test(
    time: Res<Time>,
    cfg: Res<DemoConfig>,
    state: Res<State<AppState>>,
    mut data: ResMut<GameData>,
    mut ui: ResMut<crate::hub::HubUi>,
    mut t: Local<f32>,
    mut exit: MessageWriter<AppExit>,
) {
    use crate::hub::{HubAction, apply_action};
    use dungeon_sim::content::ContentEdit;
    if !cfg.editor_test || *state.get() != AppState::Hub {
        return;
    }
    *t += time.delta_secs();
    if *t < 2.0 {
        return;
    }
    let original = data.profile.selected_dungeon.clone();
    apply_action(HubAction::EditStart, &mut data, &mut ui);
    apply_action(
        HubAction::Edit(ContentEdit::Interval(0, -3.0)),
        &mut data,
        &mut ui,
    );
    apply_action(HubAction::Edit(ContentEdit::Hp(1.1)), &mut data, &mut ui);
    let before = data
        .editor
        .as_ref()
        .map(|c| c.editable_templates()[0].clone())
        .unwrap_or_default();
    apply_action(HubAction::EditSave, &mut data, &mut ui);
    let custom = format!("{original}_custom");
    let loaded = data.library.get(&custom).map(|c| {
        let tpl = &c.templates[&before];
        (tpl.hp, tpl.abilities.first().map(|a| a.interval))
    });
    let ok = loaded.is_some() && data.profile.selected_dungeon == custom;
    let msg = format!(
        "EDITOR TEST {}: saved '{custom}' (template '{before}': hp/interval {loaded:?}); selected '{}'; message {:?}; content errors {}",
        if ok { "OK" } else { "FAILED" },
        data.profile.selected_dungeon,
        data.message.as_ref().map(|m| m.0.clone()),
        data.content_errors.len()
    );
    info!("{msg}");
    let _ = std::fs::create_dir_all("logs");
    let _ = std::fs::write("logs/editor_test.txt", msg);
    exit.write(AppExit::Success);
}
