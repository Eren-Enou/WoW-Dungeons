//! Procedural cave geometry and atmosphere (0.5).
//!
//! Everything here is presentation built from the simulation's `Nav` and the
//! content's decoration lists. Navigation never reads any of it:
//! * floors, ramps and decks are prisms over each surface's convex footprint,
//!   with a sloped top following the surface's height;
//! * walls can be drawn as jagged rock: deterministic jittered chunks that are
//!   only ever pushed *outward* from the walkable side, so rock never covers
//!   floor you can walk on;
//! * pillars, stalagmites, boulders and crystals (validated to stand outside
//!   walkable space), lava with a slow emissive pulse, drifting embers.

use crate::common::{AppState, rgb};
use crate::dungeon::ActiveRun;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use dungeon_sim::V2;
use dungeon_sim::content::DecorKind;
use dungeon_sim::nav::Wall;
use dungeon_sim::poly::Poly;

pub struct CavePlugin;

impl Plugin for CavePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (pulse_lava, drift_embers)
                .run_if(in_state(AppState::Dungeon))
                .run_if(resource_exists::<ActiveRun>),
        );
    }
}

/// Deterministic pseudo-random value in 0..1 from a position and a salt.
pub fn hash01(x: f32, z: f32, salt: u32) -> f32 {
    let mut h = (x * 73.13).to_bits()
        ^ (z * 19.71).to_bits().rotate_left(13)
        ^ salt.wrapping_mul(0x9E37_79B9);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    (h & 0x00ff_ffff) as f32 / 16_777_216.0
}

/// A prism over a convex footprint: top at `top(p)` for each vertex, bottom at
/// `bottom`. Side walls and top; no bottom face (never seen).
pub fn prism_mesh(poly: &Poly, top: impl Fn(V2) -> f32, bottom: f32) -> Mesh {
    let pts = poly.points();
    let n = pts.len();
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    // Top face (fan). The footprint is counter-clockwise in (x, z) maths terms,
    // which is clockwise seen from above in Bevy's right-handed Y-up space, so
    // triangles are emitted reversed to face up.
    let tops: Vec<Vec3> = pts.iter().map(|p| Vec3::new(p.x, top(*p), p.z)).collect();
    let normal = if n >= 3 {
        let nrm = (tops[2] - tops[0])
            .cross(tops[1] - tops[0])
            .normalize_or_zero();
        if nrm.y < 0.0 { -nrm } else { nrm }
    } else {
        Vec3::Y
    };
    for t in &tops {
        positions.push(t.to_array());
        normals.push(normal.to_array());
    }
    for k in 1..n.saturating_sub(1) {
        indices.extend_from_slice(&[0, (k + 1) as u32, k as u32]);
    }
    // Sides.
    for k in 0..n {
        let (a, b) = (pts[k], pts[(k + 1) % n]);
        let e = b - a;
        let out = Vec3::new(e.z, 0.0, -e.x).normalize_or_zero();
        let base = positions.len() as u32;
        for v in [
            Vec3::new(a.x, bottom, a.z),
            Vec3::new(b.x, bottom, b.z),
            Vec3::new(b.x, top(b), b.z),
            Vec3::new(a.x, top(a), a.z),
        ] {
            positions.push(v.to_array());
            normals.push(out.to_array());
        }
        indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_indices(Indices::U32(indices))
}

/// One piece of a wall: a box placed and rotated along the wall.
pub struct WallChunk {
    pub center: Vec3,
    pub size: Vec3,
    pub rotation: Quat,
    /// Axis-aligned bounds (camera collision and fading).
    pub min: Vec3,
    pub max: Vec3,
}

fn chunk(center: Vec3, size: Vec3, yaw: f32, tilt: Quat) -> WallChunk {
    let rotation = Quat::from_rotation_y(yaw) * tilt;
    // Bounds of the rotated box.
    let half = size * 0.5;
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for sx in [-1.0, 1.0] {
        for sy in [-1.0, 1.0] {
            for sz in [-1.0, 1.0] {
                let p = center + rotation * (half * Vec3::new(sx, sy, sz));
                min = min.min(p);
                max = max.max(p);
            }
        }
    }
    WallChunk {
        center,
        size,
        rotation,
        min,
        max,
    }
}

/// Wall pieces along one wall face. Smooth walls are one slab per metre or
/// so (any angle); rough walls are jittered rocks that only grow outward.
pub fn wall_chunks(w: &Wall, rough: bool) -> Vec<WallChunk> {
    let len = w.a.dist(w.b);
    if len < 0.05 {
        return Vec::new();
    }
    let dir = (w.b - w.a) * (1.0 / len);
    let yaw = (-dir.z).atan2(dir.x);
    let sloped = (w.bottom0 - w.bottom1).abs() > 0.05;
    let piece = if rough {
        1.3
    } else if sloped {
        1.0
    } else {
        len.min(4.0)
    };
    let count = (len / piece).ceil().max(1.0) as usize;
    let mut out = Vec::with_capacity(count);
    for k in 0..count {
        let f0 = k as f32 / count as f32;
        let f1 = (k + 1) as f32 / count as f32;
        let a = w.a + (w.b - w.a) * f0;
        let b = w.a + (w.b - w.a) * f1;
        let mid = (a + b) * 0.5;
        let plen = a.dist(b);
        let bottom = w.bottom0 + (w.bottom1 - w.bottom0) * (f0 + f1) * 0.5;
        if rough {
            let r1 = hash01(mid.x, mid.z, 1);
            let r2 = hash01(mid.x, mid.z, 2);
            let r3 = hash01(mid.x, mid.z, 3);
            let thick = 0.6 + r1 * 0.9;
            let height = w.height * (0.8 + r2 * 0.6);
            // Inner face exactly on the wall line (or further out): never inside.
            let offset = thick * 0.5 + r3 * 0.35;
            let c = mid + w.out * offset;
            let tilt =
                Quat::from_rotation_x((r2 - 0.5) * 0.25) * Quat::from_rotation_z((r1 - 0.5) * 0.2);
            out.push(chunk(
                Vec3::new(c.x, bottom - 0.3 + height * 0.5, c.z),
                Vec3::new(plen * (1.1 + r3 * 0.3), height + 0.6, thick),
                yaw,
                tilt,
            ));
            // An occasional boulder at the foot of the wall, also outside.
            if r1 > 0.72 {
                let s = 0.7 + r2 * 0.8;
                let c2 = mid + w.out * (s * 0.5 + 0.1);
                out.push(chunk(
                    Vec3::new(c2.x, bottom + s * 0.35, c2.z),
                    Vec3::new(s * 1.3, s, s),
                    yaw + r3,
                    Quat::IDENTITY,
                ));
            }
        } else {
            let c = mid + w.out * 0.2;
            out.push(chunk(
                Vec3::new(c.x, bottom + w.height * 0.5, c.z),
                Vec3::new(plen + 0.4, w.height, 0.4),
                yaw,
                Quat::IDENTITY,
            ));
        }
    }
    out
}

/// Pulsing lava material handles.
#[derive(Component)]
pub struct LavaGlow {
    pub material: Handle<StandardMaterial>,
    pub phase: f32,
}

#[derive(Component)]
pub struct Ember {
    pub base: Vec3,
    pub rise: f32,
    pub speed: f32,
    pub phase: f32,
}

fn pulse_lava(
    time: Res<Time>,
    q: Query<&LavaGlow>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let t = time.elapsed_secs();
    for g in &q {
        if let Some(mut m) = materials.get_mut(&g.material) {
            let k = 1.5 + 0.45 * (t * 0.9 + g.phase).sin() + 0.2 * (t * 2.3 + g.phase * 1.7).sin();
            m.emissive = LinearRgba::rgb(k, k * 0.3, k * 0.04);
        }
    }
}

fn drift_embers(time: Res<Time>, mut q: Query<(&Ember, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (e, mut tr) in &mut q {
        let f = ((t * e.speed + e.phase) % e.rise) / e.rise;
        let sway = (t * 1.3 + e.phase * 5.0).sin() * 0.4;
        tr.translation = e.base + Vec3::new(sway, f * e.rise, sway * 0.6);
        tr.scale = Vec3::splat((1.0 - f) * 0.9 + 0.1);
    }
}

/// Spawn decoration for the run (pillars, stalagmites, boulders, crystals).
/// Returns their camera-blocking bounds.
pub fn spawn_decor(
    commands: &mut Commands,
    run: &ActiveRun,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Vec<(Vec3, Vec3)> {
    let scoped = DespawnOnExit(AppState::Dungeon);
    let theme = run.sim.content.dungeon.theme;
    let rock = materials.add(StandardMaterial {
        base_color: rgb(theme.wall).darker(0.05),
        perceptual_roughness: 0.95,
        ..default()
    });
    let mut blockers = Vec::new();
    for (i, d) in run.sim.content.dungeon.decor.iter().enumerate() {
        let r = d.radius;
        let h = d.height;
        let jitter = hash01(d.pos.x, d.pos.z, i as u32);
        let base = Vec3::new(d.pos.x, d.y, d.pos.z);
        match d.kind {
            DecorKind::Pillar => {
                // A column of three slightly offset drums.
                for k in 0..3 {
                    let f = k as f32 / 3.0;
                    let rr = r * (1.0 - 0.15 * f + 0.1 * hash01(d.pos.x, d.pos.z, 10 + k));
                    commands.spawn((
                        Mesh3d(meshes.add(Cylinder::new(rr, h / 3.0 + 0.2))),
                        MeshMaterial3d(rock.clone()),
                        Transform::from_translation(
                            base + Vec3::new(0.0, h * (f + 1.0 / 6.0), 0.0),
                        )
                        .with_rotation(Quat::from_rotation_y(jitter * 6.0 + k as f32)),
                        scoped.clone(),
                    ));
                }
            }
            DecorKind::Stalagmite => {
                commands.spawn((
                    Mesh3d(meshes.add(Cone::new(r, h))),
                    MeshMaterial3d(rock.clone()),
                    Transform::from_translation(base + Vec3::Y * (h * 0.5))
                        .with_rotation(Quat::from_rotation_z((jitter - 0.5) * 0.2)),
                    scoped.clone(),
                ));
            }
            DecorKind::Boulder => {
                commands.spawn((
                    Mesh3d(meshes.add(Sphere::new(1.0).mesh().ico(1).expect("ico sphere"))),
                    MeshMaterial3d(rock.clone()),
                    Transform::from_translation(base + Vec3::Y * (h * 0.4))
                        .with_scale(Vec3::new(
                            r * (1.0 + jitter * 0.3),
                            h * 0.6,
                            r * (1.2 - jitter * 0.3),
                        ))
                        .with_rotation(Quat::from_rotation_y(jitter * 6.0)),
                    scoped.clone(),
                ));
            }
            DecorKind::Crystal => {
                let c = d.color.unwrap_or([0.6, 0.4, 1.0]);
                let mat = materials.add(StandardMaterial {
                    base_color: rgb(c),
                    emissive: LinearRgba::rgb(c[0] * 2.0, c[1] * 2.0, c[2] * 2.0),
                    ..default()
                });
                for k in 0..3 {
                    let a = jitter * 6.0 + k as f32 * 2.1;
                    let off = Vec3::new(a.cos(), 0.0, a.sin()) * (r * 0.4);
                    let hh = h * (0.6 + 0.4 * hash01(d.pos.x, d.pos.z, 20 + k));
                    commands.spawn((
                        Mesh3d(meshes.add(Cuboid::new(r * 0.45, hh, r * 0.45))),
                        MeshMaterial3d(mat.clone()),
                        Transform::from_translation(base + off + Vec3::Y * (hh * 0.45))
                            .with_rotation(
                                Quat::from_rotation_y(a)
                                    * Quat::from_rotation_z(0.25 * (k as f32 - 1.0)),
                            ),
                        scoped.clone(),
                    ));
                }
                commands.spawn((
                    PointLight {
                        color: rgb(c),
                        intensity: 120_000.0,
                        range: 9.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_translation(base + Vec3::Y * (h * 0.7)),
                    scoped.clone(),
                ));
            }
        }
        blockers.push((base - Vec3::new(r, 0.0, r), base + Vec3::new(r, h, r)));
    }
    blockers
}

/// Lava pools/rivers with a pulsing glow and a few drifting embers.
pub fn spawn_lava(
    commands: &mut Commands,
    run: &ActiveRun,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let scoped = DespawnOnExit(AppState::Dungeon);
    let ember_mesh = meshes.add(Sphere::new(0.06).mesh().ico(0).expect("ico sphere"));
    let ember_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.6, 0.2),
        emissive: LinearRgba::rgb(6.0, 2.0, 0.3),
        unlit: true,
        ..default()
    });
    let explicit_lights = !run.sim.content.dungeon.lights.is_empty();
    let crust_mesh = meshes.add(Cuboid::new(1.0, 0.06, 1.0));
    let crust_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.06, 0.04, 0.035),
        perceptual_roughness: 1.0,
        ..default()
    });
    for (i, l) in run.sim.content.dungeon.lava.iter().enumerate() {
        let Ok(shape) = l.shape() else { continue };
        let mat = materials.add(StandardMaterial {
            base_color: Color::srgb(0.35, 0.07, 0.01),
            emissive: LinearRgba::rgb(1.5, 0.45, 0.06),
            perceptual_roughness: 0.35,
            ..default()
        });
        commands.spawn((
            Mesh3d(meshes.add(prism_mesh(&shape, |_| l.y, l.y - 0.3))),
            MeshMaterial3d(mat.clone()),
            Transform::IDENTITY,
            LavaGlow {
                material: mat,
                phase: i as f32 * 1.37,
            },
            scoped.clone(),
        ));
        let c = shape.centroid();
        // Dark crust floating on the molten surface: reads as liquid rock,
        // never as floor (it sits in the lava, below every walkable edge).
        let bb = shape.bounds();
        let plates = ((shape.area() / 7.0) as usize).clamp(2, 160);
        let mut placed = 0;
        let mut k = 100u32;
        while placed < plates && k < 100 + plates as u32 * 8 {
            k += 1;
            let p = V2::new(
                bb.min.x + hash01(c.x, c.z, k) * (bb.max.x - bb.min.x),
                bb.min.z + hash01(c.z, c.x, k + 11) * (bb.max.z - bb.min.z),
            );
            if !shape.contains(p) {
                continue;
            }
            placed += 1;
            let (r1, r2, r3) = (
                hash01(p.x, p.z, 1),
                hash01(p.x, p.z, 2),
                hash01(p.x, p.z, 3),
            );
            commands.spawn((
                Mesh3d(crust_mesh.clone()),
                MeshMaterial3d(crust_mat.clone()),
                Transform::from_xyz(p.x, l.y + 0.02, p.z)
                    .with_rotation(Quat::from_rotation_y(r3 * 6.28))
                    .with_scale(Vec3::new(0.7 + r1 * 1.8, 1.0, 0.5 + r2 * 1.4)),
                scoped.clone(),
            ));
        }
        if !explicit_lights {
            commands.spawn((
                PointLight {
                    color: Color::srgb(1.0, 0.45, 0.1),
                    intensity: 400_000.0,
                    range: 14.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_xyz(c.x, l.y + 1.5, c.z),
                scoped.clone(),
            ));
        }
        // Embers: one per ~12 m², capped, scattered deterministically.
        let b = shape.bounds();
        let count = ((shape.area() / 12.0) as usize).clamp(2, 24);
        let mut placed = 0;
        let mut k = 0u32;
        while placed < count && k < 400 {
            k += 1;
            let p = V2::new(
                b.min.x + hash01(c.x, c.z, k) * (b.max.x - b.min.x),
                b.min.z + hash01(c.z, c.x, k + 7) * (b.max.z - b.min.z),
            );
            if !shape.contains(p) {
                continue;
            }
            placed += 1;
            commands.spawn((
                Mesh3d(ember_mesh.clone()),
                MeshMaterial3d(ember_mat.clone()),
                Transform::from_xyz(p.x, l.y, p.z),
                Ember {
                    base: Vec3::new(p.x, l.y + 0.1, p.z),
                    rise: 3.0 + hash01(p.x, p.z, 3) * 4.0,
                    speed: 0.6 + hash01(p.x, p.z, 4) * 0.8,
                    phase: hash01(p.x, p.z, 5) * 10.0,
                },
                scoped.clone(),
            ));
        }
    }
}
