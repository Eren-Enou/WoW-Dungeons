//! Classic third-person MMO character and camera controls.
//!
//! * **W / S** move forward / backward along the character's facing.
//! * **A / D** turn the character; while the right mouse button is held they strafe.
//! * **Q / E** always strafe.
//! * **Right mouse drag** rotates the camera *and* steers the character (facing
//!   follows the camera).
//! * **Left mouse drag** rotates the camera only (free look); facing is unchanged.
//!   A left *click* (no drag) selects a target instead.
//! * **Both mouse buttons** run forward.
//! * **Auto-follow** (toggle with V or in the hub): while moving, the camera eases
//!   behind the character. Any manual camera input suppresses it for
//!   [`MANUAL_GRACE`] seconds.
//!
//! The rules live in [`update_controls`], a pure function over plain data so they
//! can be unit tested; the Bevy system only gathers input and applies the result.

use crate::camera::OrbitCamera;
use crate::common::GameData;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use std::f32::consts::{PI, TAU};

/// Keyboard turn rate (radians per second).
pub const TURN_RATE: f32 = 2.6;
/// Mouse look sensitivity (radians per pixel).
pub const YAW_SENS: f32 = 0.006;
pub const PITCH_SENS: f32 = 0.004;
/// Seconds of auto-follow suppression after manual camera input.
pub const MANUAL_GRACE: f32 = 1.2;
/// Pixels the left button must travel before a click becomes a drag.
pub const DRAG_THRESHOLD: f32 = 5.0;
pub const PITCH_MIN: f32 = 0.08;
pub const PITCH_MAX: f32 = 1.35;

/// One frame of player input, already decoded from keys and mouse.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ControlInput {
    pub forward: bool,
    pub back: bool,
    /// A / D.
    pub turn_left: bool,
    pub turn_right: bool,
    /// Q / E.
    pub strafe_left: bool,
    pub strafe_right: bool,
    /// Left button held *and* dragged past the click threshold.
    pub left_drag: bool,
    pub right_held: bool,
    /// Both mouse buttons held (run forward).
    pub both_buttons: bool,
    pub mouse_dx: f32,
    pub mouse_dy: f32,
    /// Arrow keys rotate / tilt the camera.
    pub cam_left: bool,
    pub cam_right: bool,
    pub cam_up: bool,
    pub cam_down: bool,
    pub dt: f32,
}

/// Persistent control state for the local character.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlState {
    /// Character heading (radians; same convention as `V2::heading` and camera yaw).
    pub facing: f32,
    pub yaw: f32,
    pub pitch: f32,
    /// Remaining auto-follow suppression after manual camera input.
    pub manual_timer: f32,
    pub follow: bool,
    /// Auto-follow responsiveness (1/seconds).
    pub follow_speed: f32,
}

impl Default for ControlState {
    fn default() -> Self {
        ControlState {
            facing: 0.0,
            yaw: 0.0,
            pitch: 0.55,
            manual_timer: 0.0,
            follow: true,
            follow_speed: 3.0,
        }
    }
}

/// What the character should do this frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ControlOutput {
    /// World-space movement direction (x, z), not normalized; zero = stand still.
    pub move_dir: Vec2,
    /// The player actively turned or steered this frame (don't auto-face targets).
    pub turned: bool,
}

/// Smallest signed angle from `from` to `to`, in (-PI, PI].
pub fn angle_diff(from: f32, to: f32) -> f32 {
    let mut d = (to - from) % TAU;
    if d > PI {
        d -= TAU;
    } else if d <= -PI {
        d += TAU;
    }
    d
}

pub fn forward_of(heading: f32) -> Vec2 {
    Vec2::new(heading.sin(), heading.cos())
}

/// Screen-right for a heading (matches `OrbitCamera::right`).
pub fn right_of(heading: f32) -> Vec2 {
    Vec2::new(-heading.cos(), heading.sin())
}

pub fn update_controls(s: &mut ControlState, i: &ControlInput) -> ControlOutput {
    let mut turned = false;
    // Camera rotation from the mouse (either button) and arrow keys.
    let mouse_look = i.left_drag || i.right_held;
    if mouse_look && (i.mouse_dx != 0.0 || i.mouse_dy != 0.0) {
        s.yaw -= i.mouse_dx * YAW_SENS;
        s.pitch = (s.pitch + i.mouse_dy * PITCH_SENS).clamp(PITCH_MIN, PITCH_MAX);
        s.manual_timer = MANUAL_GRACE;
    }
    if i.cam_left || i.cam_right {
        let d = (i.cam_left as i32 - i.cam_right as i32) as f32;
        s.yaw += d * 2.0 * i.dt;
        s.manual_timer = MANUAL_GRACE;
    }
    if i.cam_up || i.cam_down {
        let d = (i.cam_up as i32 - i.cam_down as i32) as f32;
        s.pitch = (s.pitch + d * i.dt).clamp(PITCH_MIN, PITCH_MAX);
    }
    // Right button steers: the character faces where the camera looks.
    if i.right_held {
        if angle_diff(s.facing, s.yaw).abs() > 1e-4 {
            turned = true;
        }
        s.facing = s.yaw;
    }
    // A / D: turn without the right button, strafe with it.
    let (strafe_l, strafe_r) = if i.right_held {
        (i.strafe_left || i.turn_left, i.strafe_right || i.turn_right)
    } else {
        if i.turn_left != i.turn_right {
            let d = if i.turn_left { 1.0 } else { -1.0 };
            s.facing += d * TURN_RATE * i.dt;
            turned = true;
        }
        (i.strafe_left, i.strafe_right)
    };
    let fwd = forward_of(s.facing);
    let right = right_of(s.facing);
    let mut dir = Vec2::ZERO;
    let forward = i.forward || i.both_buttons;
    if forward {
        dir += fwd;
    }
    if i.back {
        dir -= fwd;
    }
    if strafe_r {
        dir += right;
    }
    if strafe_l {
        dir -= right;
    }
    // Auto-follow: ease the camera behind the character while it moves or turns,
    // unless the player is controlling the camera (or did so very recently).
    let moving = dir.length_squared() > 0.0;
    let backpedal_only = i.back && !forward && !strafe_l && !strafe_r;
    s.manual_timer = (s.manual_timer - i.dt).max(0.0);
    if s.follow && !mouse_look && s.manual_timer <= 0.0 && (moving || turned) && !backpedal_only {
        let k = (s.follow_speed * i.dt).min(1.0);
        s.yaw += angle_diff(s.yaw, s.facing) * k;
    }
    // Keep angles bounded.
    s.facing = wrap(s.facing);
    s.yaw = wrap(s.yaw);
    ControlOutput {
        move_dir: dir,
        turned,
    }
}

fn wrap(a: f32) -> f32 {
    angle_diff(0.0, a)
}

// ------------------------------------------------------------------ Bevy --

/// Shared control state for the local character (hub and dungeon).
#[derive(Resource, Default)]
pub struct PlayerControl {
    pub state: ControlState,
    pub output: ControlOutput,
    /// Left button gesture tracking.
    pub left_press_pos: Option<Vec2>,
    pub left_dragged: bool,
    /// The left button went down over the UI (no look, no world click).
    pub left_on_ui: bool,
    /// A completed left *click* in the world this frame (screen position).
    pub world_click: Option<Vec2>,
    /// Set by HUD/hub systems when the cursor is over interactive UI.
    pub cursor_over_ui: bool,
}

/// UI panels that swallow the mouse while hovered: clicks don't select 3D
/// characters through them, left-drag doesn't look and the wheel doesn't zoom.
#[derive(Component, Default)]
#[require(Hovered)]
pub struct BlocksWorld;

pub struct ControlsPlugin;

impl Plugin for ControlsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerControl>().add_systems(
            Update,
            (detect_ui_hover, update_player_control)
                .chain()
                .in_set(ControlSet),
        );
    }
}

/// Runs before gameplay input systems that consume [`PlayerControl`].
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ControlSet;

fn detect_ui_hover(q: Query<&Hovered, With<BlocksWorld>>, mut pc: ResMut<PlayerControl>) {
    let over = q.iter().any(|h| h.get());
    if pc.cursor_over_ui != over {
        pc.cursor_over_ui = over;
    }
}

#[allow(clippy::too_many_arguments)]
fn update_player_control(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    time: Res<Time>,
    window: Single<&Window>,
    mut orbit: ResMut<OrbitCamera>,
    mut pc: ResMut<PlayerControl>,
    mut data: ResMut<GameData>,
) {
    let cursor = window.cursor_position();
    pc.world_click = None;
    // Left button gesture: click selects, drag looks.
    if mouse.just_pressed(MouseButton::Left) {
        pc.left_press_pos = cursor;
        pc.left_dragged = false;
        pc.left_on_ui = pc.cursor_over_ui;
    }
    if mouse.pressed(MouseButton::Left)
        && !pc.left_dragged
        && let (Some(start), Some(now)) = (pc.left_press_pos, cursor)
        && start.distance(now) > DRAG_THRESHOLD
    {
        pc.left_dragged = true;
    }
    if mouse.just_released(MouseButton::Left) {
        if !pc.left_dragged && !pc.left_on_ui {
            pc.world_click = cursor;
        }
        pc.left_press_pos = None;
        pc.left_dragged = false;
    }
    if keys.just_pressed(KeyCode::KeyV) {
        data.profile.settings.camera_follow = !data.profile.settings.camera_follow;
        let on = data.profile.settings.camera_follow;
        data.flash(format!(
            "Camera auto-follow {}",
            if on { "ON" } else { "OFF" }
        ));
        data.save();
    }
    let left_drag = mouse.pressed(MouseButton::Left) && pc.left_dragged && !pc.left_on_ui;
    let input = ControlInput {
        forward: keys.pressed(KeyCode::KeyW),
        back: keys.pressed(KeyCode::KeyS),
        turn_left: keys.pressed(KeyCode::KeyA),
        turn_right: keys.pressed(KeyCode::KeyD),
        strafe_left: keys.pressed(KeyCode::KeyQ),
        strafe_right: keys.pressed(KeyCode::KeyE),
        left_drag,
        right_held: mouse.pressed(MouseButton::Right),
        both_buttons: mouse.pressed(MouseButton::Left) && mouse.pressed(MouseButton::Right),
        mouse_dx: motion.delta.x,
        mouse_dy: motion.delta.y,
        cam_left: keys.pressed(KeyCode::ArrowLeft),
        cam_right: keys.pressed(KeyCode::ArrowRight),
        cam_up: keys.pressed(KeyCode::ArrowUp),
        cam_down: keys.pressed(KeyCode::ArrowDown),
        dt: time.delta_secs(),
    };
    let mut state = pc.state;
    state.yaw = orbit.yaw;
    state.pitch = orbit.pitch;
    state.follow = data.profile.settings.camera_follow;
    state.follow_speed = data.profile.settings.follow_speed;
    let out = update_controls(&mut state, &input);
    orbit.yaw = state.yaw;
    orbit.pitch = state.pitch;
    pc.state = state;
    pc.output = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> ControlInput {
        ControlInput {
            dt: 0.016,
            ..Default::default()
        }
    }

    fn run(s: &mut ControlState, i: ControlInput, frames: usize) -> ControlOutput {
        let mut out = ControlOutput::default();
        for _ in 0..frames {
            out = update_controls(s, &i);
        }
        out
    }

    #[test]
    fn w_moves_along_facing() {
        let mut s = ControlState {
            facing: 1.0,
            yaw: 1.0,
            ..Default::default()
        };
        let out = update_controls(
            &mut s,
            &ControlInput {
                forward: true,
                ..frame()
            },
        );
        let d = out.move_dir.normalize();
        assert!((d - forward_of(1.0)).length() < 1e-5);
    }

    #[test]
    fn right_drag_steers_the_character() {
        let mut s = ControlState::default();
        let i = ControlInput {
            right_held: true,
            mouse_dx: -50.0,
            ..frame()
        };
        let out = update_controls(&mut s, &i);
        assert!(out.turned);
        assert!((s.facing - s.yaw).abs() < 1e-5, "facing follows camera");
        assert!((s.yaw - 0.3).abs() < 1e-4);
    }

    #[test]
    fn left_drag_looks_without_changing_facing() {
        let mut s = ControlState::default();
        let i = ControlInput {
            left_drag: true,
            mouse_dx: 80.0,
            ..frame()
        };
        let out = update_controls(&mut s, &i);
        assert!(!out.turned);
        assert_eq!(s.facing, 0.0);
        assert!(s.yaw < -0.4, "camera rotated");
    }

    #[test]
    fn a_d_turn_without_right_button_and_strafe_with_it() {
        let mut s = ControlState::default();
        let out = update_controls(
            &mut s,
            &ControlInput {
                turn_left: true,
                ..frame()
            },
        );
        assert!(out.turned && s.facing > 0.0);
        assert_eq!(out.move_dir, Vec2::ZERO, "turning alone does not move");
        let mut s = ControlState::default();
        let out = update_controls(
            &mut s,
            &ControlInput {
                turn_left: true,
                right_held: true,
                ..frame()
            },
        );
        assert_eq!(s.facing, 0.0);
        assert!((out.move_dir.normalize() - (-right_of(0.0))).length() < 1e-5);
    }

    #[test]
    fn both_buttons_run_forward() {
        let mut s = ControlState::default();
        let out = update_controls(
            &mut s,
            &ControlInput {
                both_buttons: true,
                right_held: true,
                ..frame()
            },
        );
        assert!((out.move_dir.normalize() - forward_of(s.facing)).length() < 1e-5);
    }

    #[test]
    fn auto_follow_eases_camera_behind_moving_character() {
        let mut s = ControlState {
            facing: 1.5,
            yaw: 0.0,
            ..Default::default()
        };
        let i = ControlInput {
            forward: true,
            ..frame()
        };
        update_controls(&mut s, &i);
        assert!(s.yaw > 0.0 && s.yaw < 0.2, "smooth, not a snap: {}", s.yaw);
        run(&mut s, i, 200);
        assert!(
            angle_diff(s.yaw, s.facing).abs() < 0.01,
            "converges: {}",
            s.yaw
        );
    }

    #[test]
    fn auto_follow_waits_while_standing_and_can_be_disabled() {
        let mut s = ControlState {
            facing: 1.5,
            ..Default::default()
        };
        run(&mut s, frame(), 100);
        assert_eq!(s.yaw, 0.0, "no swing while standing still");
        let mut s = ControlState {
            facing: 1.5,
            follow: false,
            ..Default::default()
        };
        run(
            &mut s,
            ControlInput {
                forward: true,
                ..frame()
            },
            100,
        );
        assert_eq!(s.yaw, 0.0, "disabled");
    }

    #[test]
    fn manual_camera_input_takes_precedence_for_a_grace_period() {
        let mut s = ControlState {
            facing: 1.5,
            ..Default::default()
        };
        // Free-look to the side, then release and keep walking.
        update_controls(
            &mut s,
            &ControlInput {
                left_drag: true,
                mouse_dx: -100.0,
                ..frame()
            },
        );
        let looked = s.yaw;
        let walk = ControlInput {
            forward: true,
            ..frame()
        };
        run(&mut s, walk, 30); // ~0.5 s < grace
        assert_eq!(s.yaw, looked, "no auto-follow during the grace period");
        run(&mut s, walk, 200);
        assert!(
            angle_diff(s.yaw, s.facing).abs() < 0.05,
            "then follows again"
        );
    }

    #[test]
    fn backpedalling_does_not_spin_the_camera() {
        let mut s = ControlState {
            facing: 2.0,
            ..Default::default()
        };
        run(
            &mut s,
            ControlInput {
                back: true,
                ..frame()
            },
            100,
        );
        assert_eq!(s.yaw, 0.0);
    }

    #[test]
    fn angle_diff_wraps() {
        assert!((angle_diff(3.0, -3.0) - (TAU - 6.0)).abs() < 1e-5);
        assert!((angle_diff(0.1, 0.3) - 0.2).abs() < 1e-6);
    }
}
