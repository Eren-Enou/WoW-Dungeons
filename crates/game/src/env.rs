//! The dungeon environment: everything that draws the *place* (floors,
//! ramps, stairs, walls, lava, decoration, props, lights, fog, the entrance
//! portal and boss daises), built from a dungeon's `Content` and the `Nav`
//! made from it.
//!
//! Both the game (`AppState::Dungeon`) and the viewer (`AppState::Viewer`)
//! call [`spawn_environment`], so what the viewer shows is exactly what the
//! game draws. Nothing here reads or changes simulation state.

use crate::camera::CameraBlockers;
use crate::common::{AppState, rgb, to_vec3};
use bevy::prelude::*;
use dungeon_sim::V2;
use dungeon_sim::content::{Content, EnemyRank};
use dungeon_sim::nav::Nav;
use std::f32::consts::FRAC_PI_2;

/// A wall that fades out when it hides the player from the camera (game only).
#[derive(Component)]
pub struct Occluder {
    pub material: Handle<StandardMaterial>,
    pub min: Vec3,
    pub max: Vec3,
}

/// One modular piece as placed (for the viewer's inspector and overlays).
#[derive(Debug, Clone)]
pub struct ModuleInstance {
    /// `wall`, `corner`, `door`.
    pub kind: &'static str,
    pub model: String,
    /// The model file was missing: procedural geometry was drawn instead.
    pub fallback: bool,
    pub transform: Transform,
    /// World-space bounds.
    pub min: Vec3,
    pub max: Vec3,
    /// Room (index into the structure) it belongs to, if known.
    pub room: Option<usize>,
}

/// Every modular piece of the current environment.
#[derive(Resource, Default, Debug, Clone)]
pub struct ModuleInstances(pub Vec<ModuleInstance>);

/// Where the entrance is, with its floor height resolved.
pub fn entrance_point(content: &Content, nav: &Nav) -> Vec3 {
    let entrance = content.dungeon.entrance;
    let y = content.dungeon.entrance_y.unwrap_or_else(|| {
        nav.locate(entrance, 0.0)
            .map(|s| nav.height(s, entrance))
            .unwrap_or(0.0)
    });
    to_vec3(entrance, y)
}

/// Spawn the whole environment for `content` under `scope` (despawned when
/// that state exits). Sets ambient light and the camera's fog. Returns
/// nothing: camera blockers are written into `blockers`.
#[allow(clippy::too_many_arguments)]
pub fn spawn_environment(
    commands: &mut Commands,
    content: &Content,
    nav: &Nav,
    scope: AppState,
    camera: Entity,
    blockers: &mut CameraBlockers,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    asset_server: &AssetServer,
) {
    let scoped = DespawnOnExit(scope);
    let theme = content.dungeon.theme;
    commands.insert_resource(GlobalAmbientLight {
        color: theme
            .ambient
            .map(rgb)
            .unwrap_or(Color::srgb(0.6, 0.65, 0.8)),
        brightness: 220.0,
        ..default()
    });
    if let Some(fog) = theme.fog {
        commands.entity(camera).insert(bevy::pbr::DistanceFog {
            color: rgb(fog),
            falloff: bevy::pbr::FogFalloff::Linear {
                start: 18.0,
                end: 75.0,
            },
            ..default()
        });
    } else {
        commands.entity(camera).remove::<bevy::pbr::DistanceFog>();
    }
    commands.spawn((
        DirectionalLight {
            illuminance: 1800.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(10.0, 30.0, -20.0).looking_at(Vec3::ZERO, Vec3::Y),
        scoped.clone(),
    ));
    spawn_level_geometry(commands, content, nav, scope, blockers, meshes, materials);
    let props = crate::cave::spawn_props(commands, content, scope, asset_server);
    blockers.0.extend(props);
    // Exit portal at the entrance.
    commands.spawn((
        Mesh3d(meshes.add(Torus::new(1.2, 1.6))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.1, 0.5),
            emissive: LinearRgba::rgb(1.5, 0.5, 3.0),
            ..default()
        })),
        Transform::from_translation(entrance_point(content, nav) + Vec3::new(0.0, 1.8, -5.0))
            .with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
        scoped.clone(),
    ));
    // A dais under every boss, where it spawns.
    for e in &content.encounters {
        for m in &e.mobs {
            let Some(t) = content.templates.get(&m.template) else {
                continue;
            };
            if t.rank != EnemyRank::Boss {
                continue;
            }
            let (p, _) = nav.project(m.pos, m.y.unwrap_or(0.0));
            commands.spawn((
                Mesh3d(meshes.add(Cylinder::new(t.radius + 3.0, 0.15))),
                MeshMaterial3d(materials.add(Color::srgb(0.2, 0.12, 0.25))),
                Transform::from_translation(Vec3::new(p.x, p.y + 0.02, p.z)),
                scoped.clone(),
            ));
        }
    }
}

/// Build floors, ramps, stairs, bridges, rock supports, walls, drop markers,
/// decoration and lava from the simulation's `Nav` and content, so what you
/// see is what the sim walks on.
pub fn spawn_level_geometry(
    commands: &mut Commands,
    content: &Content,
    nav: &Nav,
    scope: AppState,
    blockers: &mut CameraBlockers,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    use crate::cave::{prism_mesh, wall_chunks};
    use dungeon_sim::nav::{Axis, LinkKind, SLAB};
    use dungeon_sim::poly::Poly;
    let scoped = DespawnOnExit(scope);
    let dungeon = &content.dungeon;
    let theme = dungeon.theme;
    let [fr, fg, fb] = theme.floor;
    // Floors get lighter with height so levels read at a glance.
    let (lo, hi) = nav
        .surfaces
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), s| {
            (lo.min(s.min_y()), hi.max(s.max_y()))
        });
    let shade = |base: [f32; 3], y: f32| {
        let t = if hi > lo { (y - lo) / (hi - lo) } else { 0.0 };
        Color::srgb(base[0] + 0.12 * t, base[1] + 0.1 * t, base[2] + 0.07 * t)
    };
    let rock = materials.add(StandardMaterial {
        base_color: Color::srgb(fr * 0.55, fg * 0.55, fb * 0.6),
        perceptual_roughness: 0.95,
        ..default()
    });
    blockers.0.clear();
    let auto_lights = dungeon.lights.is_empty();
    for (i, s) in nav.surfaces.iter().enumerate() {
        let tint = dungeon
            .areas
            .get(i)
            .and_then(|a| a.tint)
            .unwrap_or(theme.floor);
        let alt = if i % 2 == 0 { 0.0 } else { 0.02 };
        let top_mat = materials.add(StandardMaterial {
            base_color: if s.def.open_under {
                Color::srgb(0.42, 0.3, 0.2)
            } else {
                shade([tint[0] + alt, tint[1] + alt, tint[2]], s.max_y())
            },
            perceptual_roughness: 0.9,
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        let bottom = if s.def.open_under {
            s.min_y() - SLAB
        } else {
            nav.base_y
        };
        let shape = &s.def.shape;
        let b = s.bounds;
        let rect_stairs = s.def.stairs && s.def.slope.is_none() && shape.is_axis_rect();
        if rect_stairs {
            // Steps of at most 0.5 m along an axis ramp.
            let len = match s.def.axis {
                Axis::Z => b.size().z,
                Axis::X => b.size().x,
            };
            let n = ((len / 0.5).ceil() as usize).max(1);
            let step = len / n as f32;
            for k in 0..n {
                let (a0, a1) = (k as f32 * step, (k + 1) as f32 * step);
                let (min, max) = match s.def.axis {
                    Axis::Z => (
                        V2::new(b.min.x, b.min.z + a0),
                        V2::new(b.max.x, b.min.z + a1),
                    ),
                    Axis::X => (
                        V2::new(b.min.x + a0, b.min.z),
                        V2::new(b.min.x + a1, b.max.z),
                    ),
                };
                let h = s.height_at((min + max) * 0.5);
                let piece = Poly::from_rect(dungeon_sim::nav::Rect::new(min, max));
                commands.spawn((
                    Mesh3d(meshes.add(prism_mesh(&piece, |_| h, bottom))),
                    MeshMaterial3d(top_mat.clone()),
                    Transform::IDENTITY,
                    scoped.clone(),
                ));
                blockers.0.push((
                    Vec3::new(min.x, bottom, min.z),
                    Vec3::new(max.x, h - 0.1, max.z),
                ));
            }
        } else {
            // A prism whose top follows the surface (flat floor, ramp, deck).
            let z = -(i as f32) * 0.001;
            commands.spawn((
                Mesh3d(meshes.add(prism_mesh(shape, |p| s.height_at(p) + z, bottom))),
                MeshMaterial3d(top_mat.clone()),
                Transform::IDENTITY,
                scoped.clone(),
            ));
            if !s.def.open_under && bottom < s.min_y() - 0.25 {
                // Rock under raised floors reads as rock, not floor.
                commands.spawn((
                    Mesh3d(meshes.add(prism_mesh(shape, |p| s.height_at(p) - 0.25, bottom))),
                    MeshMaterial3d(rock.clone()),
                    Transform::from_scale(Vec3::new(1.0, 1.0, 1.0)),
                    scoped.clone(),
                ));
            }
            // Camera blockers: ramps in 1 m slices along their bounds, floors whole.
            if s.is_ramp() {
                let n = (b.size().x.max(b.size().z).ceil() as usize).max(1);
                for k in 0..n {
                    let f0 = k as f32 / n as f32;
                    let f1 = (k + 1) as f32 / n as f32;
                    let along_x = b.size().x >= b.size().z;
                    let (min, max) = if along_x {
                        (
                            V2::new(b.min.x + b.size().x * f0, b.min.z),
                            V2::new(b.min.x + b.size().x * f1, b.max.z),
                        )
                    } else {
                        (
                            V2::new(b.min.x, b.min.z + b.size().z * f0),
                            V2::new(b.max.x, b.min.z + b.size().z * f1),
                        )
                    };
                    let low = s
                        .height_at(min)
                        .min(s.height_at(max))
                        .min(s.height_at(V2::new(min.x, max.z)))
                        .min(s.height_at(V2::new(max.x, min.z)));
                    blockers.0.push((
                        Vec3::new(min.x, bottom, min.z),
                        Vec3::new(max.x, low - 0.15, max.z),
                    ));
                }
            } else {
                blockers.0.push((
                    Vec3::new(b.min.x, bottom, b.min.z),
                    Vec3::new(b.max.x, s.max_y() - 0.05, b.max.z),
                ));
            }
        }
        if auto_lights {
            let c = shape.centroid();
            commands.spawn((
                PointLight {
                    color: rgb(theme.light),
                    intensity: 600_000.0,
                    range: 35.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_xyz(c.x, s.max_y() + 7.0, c.z),
                scoped.clone(),
            ));
        }
    }
    // Walls: smooth slabs or jagged rock, at any angle, never over the floor.
    for w in &nav.walls {
        for c in wall_chunks(w, theme.rough_walls) {
            blockers.0.push((c.min, c.max));
            let material = materials.add(StandardMaterial {
                base_color: rgb(theme.wall),
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 0.95,
                ..default()
            });
            commands.spawn((
                Mesh3d(meshes.add(Cuboid::new(c.size.x, c.size.y, c.size.z))),
                MeshMaterial3d(material.clone()),
                Occluder {
                    material,
                    min: c.min,
                    max: c.max,
                },
                Transform::from_translation(c.center).with_rotation(c.rotation),
                scoped.clone(),
            ));
        }
    }
    // One-way drops: an orange strip on the edge you can step off.
    let drop_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.55, 0.1, 0.55),
        emissive: LinearRgba::rgb(0.6, 0.25, 0.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    for l in nav.links.iter().filter(|l| l.kind == LinkKind::Drop) {
        let y = l.entry.y;
        commands.spawn((
            Mesh3d(meshes.add(prism_mesh(&l.portal, |_| y + 0.04, y + 0.01))),
            MeshMaterial3d(drop_mat.clone()),
            Transform::IDENTITY,
            scoped.clone(),
        ));
    }
    crate::cave::spawn_lava(commands, content, scope, meshes, materials);
    let decor = crate::cave::spawn_decor(commands, content, scope, meshes, materials);
    blockers.0.extend(decor);
    for l in &dungeon.lights {
        commands.spawn((
            PointLight {
                color: rgb(l.color),
                intensity: 500_000.0 * l.intensity,
                range: l.range,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(l.pos.x, l.y, l.pos.z),
            scoped.clone(),
        ));
    }
}
