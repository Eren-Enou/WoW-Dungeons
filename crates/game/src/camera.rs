//! Third-person orbit camera that follows the entity marked `CameraFocus`.
//!
//! Right mouse drag (or the arrow keys) rotates, the mouse wheel zooms.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(OrbitCamera::default())
            .add_systems(Startup, spawn_camera)
            .add_systems(
                PostUpdate,
                (orbit_input, follow)
                    .chain()
                    .before(TransformSystems::Propagate),
            );
    }
}

#[derive(Component)]
pub struct MainCamera;

#[derive(Component)]
pub struct CameraFocus;

#[derive(Resource)]
pub struct OrbitCamera {
    /// Direction the camera looks (radians about +Y; 0 looks toward +Z).
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        OrbitCamera {
            yaw: 0.0,
            pitch: 0.55,
            distance: 14.0,
        }
    }
}

impl OrbitCamera {
    /// Horizontal forward direction (x, z).
    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.yaw.sin(), 0.0, self.yaw.cos())
    }

    /// Horizontal right direction as seen on screen.
    pub fn right(&self) -> Vec3 {
        Vec3::new(-self.yaw.cos(), 0.0, self.yaw.sin())
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 8.0, -14.0).looking_at(Vec3::ZERO, Vec3::Y),
        MainCamera,
    ));
}

fn orbit_input(
    mut orbit: ResMut<OrbitCamera>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
) {
    if mouse.pressed(MouseButton::Right) {
        orbit.yaw -= motion.delta.x * 0.006;
        orbit.pitch = (orbit.pitch + motion.delta.y * 0.004).clamp(0.08, 1.35);
    }
    let dt = time.delta_secs();
    if keys.pressed(KeyCode::ArrowLeft) {
        orbit.yaw += 2.0 * dt;
    }
    if keys.pressed(KeyCode::ArrowRight) {
        orbit.yaw -= 2.0 * dt;
    }
    if keys.pressed(KeyCode::ArrowUp) {
        orbit.pitch = (orbit.pitch + dt).min(1.35);
    }
    if keys.pressed(KeyCode::ArrowDown) {
        orbit.pitch = (orbit.pitch - dt).max(0.08);
    }
    if scroll.delta.y != 0.0 {
        orbit.distance = (orbit.distance - scroll.delta.y.signum() * 1.5).clamp(5.0, 32.0);
    }
}

fn follow(
    orbit: Res<OrbitCamera>,
    focus: Query<&Transform, (With<CameraFocus>, Without<MainCamera>)>,
    mut cam: Single<&mut Transform, With<MainCamera>>,
) {
    let Some(target) = focus.iter().next() else {
        return;
    };
    let center = target.translation + Vec3::Y * 1.5;
    let back = -orbit.forward() * orbit.distance * orbit.pitch.cos();
    let up = Vec3::Y * orbit.distance * orbit.pitch.sin();
    **cam = Transform::from_translation(center + back + up).looking_at(center, Vec3::Y);
}
