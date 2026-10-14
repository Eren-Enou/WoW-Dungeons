//! Third-person orbit camera that follows the entity marked `CameraFocus`.
//!
//! Right mouse drag (or the arrow keys) rotates, the mouse wheel zooms. The camera
//! is pulled in front of any `CameraBlockers` box between it and the focus, and
//! its distance is smoothed (fast in, slow out) so it never clips through walls.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(OrbitCamera::default())
            .init_resource::<CameraBlockers>()
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

/// Axis-aligned boxes (min, max) the camera must not pass through.
#[derive(Resource, Default)]
pub struct CameraBlockers(pub Vec<(Vec3, Vec3)>);

#[derive(Resource)]
pub struct OrbitCamera {
    /// Direction the camera looks (radians about +Y; 0 looks toward +Z).
    pub yaw: f32,
    pub pitch: f32,
    /// Desired distance (set by zoom).
    pub distance: f32,
    /// Actual distance after collision and smoothing.
    pub current: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        OrbitCamera {
            yaw: 0.0,
            pitch: 0.55,
            distance: 14.0,
            current: 14.0,
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

    fn offset_dir(&self) -> Vec3 {
        (-self.forward() * self.pitch.cos() + Vec3::Y * self.pitch.sin()).normalize()
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
        orbit.distance = (orbit.distance - scroll.delta.y.signum() * 1.5).clamp(4.0, 32.0);
    }
}

/// Distance along `dir` (unit) from `origin` to the first box hit, if any (slab test).
pub fn ray_box(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let mut t0 = 0.0f32;
    let mut t1 = f32::INFINITY;
    for i in 0..3 {
        let (o, d, lo, hi) = (origin[i], dir[i], min[i], max[i]);
        if d.abs() < 1e-6 {
            if o < lo || o > hi {
                return None;
            }
        } else {
            let (mut a, mut b) = ((lo - o) / d, (hi - o) / d);
            if a > b {
                std::mem::swap(&mut a, &mut b);
            }
            t0 = t0.max(a);
            t1 = t1.min(b);
            if t0 > t1 {
                return None;
            }
        }
    }
    Some(t0)
}

fn follow(
    time: Res<Time>,
    mut orbit: ResMut<OrbitCamera>,
    blockers: Res<CameraBlockers>,
    focus: Query<&Transform, (With<CameraFocus>, Without<MainCamera>)>,
    mut cam: Single<&mut Transform, With<MainCamera>>,
) {
    let Some(target) = focus.iter().next() else {
        return;
    };
    let center = target.translation + Vec3::Y * 1.5;
    let dir = orbit.offset_dir();
    let mut allowed = orbit.distance;
    for (min, max) in &blockers.0 {
        if let Some(t) = ray_box(center, dir, *min, *max) {
            allowed = allowed.min((t - 0.4).max(1.5));
        }
    }
    // Pull in instantly when blocked, ease back out.
    let rate = if allowed < orbit.current { 30.0 } else { 4.0 };
    let k = (rate * time.delta_secs()).min(1.0);
    orbit.current += (allowed - orbit.current) * k;
    **cam = Transform::from_translation(center + dir * orbit.current).looking_at(center, Vec3::Y);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_hits_box_in_front() {
        let t = ray_box(
            Vec3::ZERO,
            Vec3::X,
            Vec3::new(5.0, -1.0, -1.0),
            Vec3::new(6.0, 1.0, 1.0),
        );
        assert_eq!(t, Some(5.0));
    }

    #[test]
    fn ray_misses_box_beside() {
        let t = ray_box(
            Vec3::ZERO,
            Vec3::X,
            Vec3::new(5.0, 2.0, -1.0),
            Vec3::new(6.0, 3.0, 1.0),
        );
        assert_eq!(t, None);
    }
}
