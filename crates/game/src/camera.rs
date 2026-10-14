//! Third-person orbit camera that follows the entity marked `CameraFocus`.
//!
//! Rotation and steering live in `controls.rs`; this module handles zoom, follow
//! and collision. The camera is pulled in front of any `CameraBlockers` box
//! between it and the focus (probing with five parallel rays so thin gaps and
//! corners don't flicker), pulls in fast and eases back out with a small
//! deadband so it never clips through walls or jitters near them.

use bevy::input::mouse::AccumulatedMouseScroll;
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
    scroll: Res<AccumulatedMouseScroll>,
    pc: Res<crate::controls::PlayerControl>,
) {
    if scroll.delta.y != 0.0 && !pc.cursor_over_ui {
        orbit.distance = (orbit.distance - scroll.delta.y.signum() * 1.5).clamp(4.0, 32.0);
    }
}

/// How far the camera may sit from `center` along `dir` without entering a box.
/// Probes the centre ray plus four rays offset sideways/vertically by the camera's
/// "radius", so the camera keeps clear of edges instead of grazing them.
pub fn collision_distance(center: Vec3, dir: Vec3, desired: f32, boxes: &[(Vec3, Vec3)]) -> f32 {
    const RADIUS: f32 = 0.35;
    let side = dir.cross(Vec3::Y).normalize_or_zero();
    let up = side.cross(dir).normalize_or_zero();
    let mut allowed = desired;
    for offset in [
        Vec3::ZERO,
        side * RADIUS,
        -side * RADIUS,
        up * RADIUS,
        -up * RADIUS,
    ] {
        for (min, max) in boxes {
            if let Some(t) = ray_box(center + offset, dir, *min, *max) {
                allowed = allowed.min((t - 0.4).max(1.5));
            }
        }
    }
    allowed
}

/// Pull in immediately, ease back out, and ignore tiny outward changes (deadband)
/// so the distance doesn't oscillate at corners.
pub fn smooth_distance(current: f32, allowed: f32, dt: f32) -> f32 {
    if allowed < current {
        return current + (allowed - current) * (30.0 * dt).min(1.0);
    }
    if allowed - current < 0.25 {
        return current;
    }
    current + (allowed - current) * (4.0 * dt).min(1.0)
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
    let allowed = collision_distance(center, dir, orbit.distance, &blockers.0);
    orbit.current = smooth_distance(orbit.current, allowed, time.delta_secs());
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
    fn camera_keeps_clear_of_walls_and_does_not_jitter() {
        // A wall 6 m behind the focus.
        let wall = (Vec3::new(-5.0, -5.0, -6.5), Vec3::new(5.0, 5.0, -6.0));
        let d = collision_distance(Vec3::ZERO, -Vec3::Z, 14.0, &[wall]);
        assert!((1.5..6.0).contains(&d), "{d}");
        // A wall just beside the ray is still respected thanks to the side probes.
        let beside = (Vec3::new(0.2, -5.0, -9.0), Vec3::new(3.0, 5.0, -8.0));
        assert!(collision_distance(Vec3::ZERO, -Vec3::Z, 14.0, &[beside]) < 9.0);
        // Pull-in is immediate-ish; tiny outward changes are ignored.
        assert!(smooth_distance(10.0, 4.0, 0.1) < 5.0);
        assert_eq!(smooth_distance(4.0, 4.1, 0.1), 4.0);
        assert!(smooth_distance(4.0, 10.0, 0.1) > 4.0);
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
