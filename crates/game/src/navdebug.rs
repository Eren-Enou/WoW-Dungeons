//! Navigation debug overlay (toggle with N; Shift+N adds collision boundaries).
//!
//! Draws, from the simulation's own `Nav` and actor state:
//! * every walkable surface outlined and coloured by height (blue low, yellow
//!   high), labelled with its name and elevation;
//! * walk portals (green rectangles) and one-way drops (orange arrows);
//! * each AI party member's planned path (thin line), current waypoint (dot)
//!   and goal (ring); the selected actor's path is drawn white;
//! * unreachable goals as red crosses, stuck/recovering actors with a red ring;
//! * with Shift+N, wall faces and the shrunk "walk" rectangles actors' centres
//!   are confined to.
//!
//! Everything is cheap gizmo lines plus a few world-anchored text labels.

use crate::camera::MainCamera;
use crate::common::{AppState, text_font, to_vec3};
use crate::dungeon::ActiveRun;
use bevy::picking::Pickable;
use bevy::prelude::*;
use dungeon_sim::V2;
use dungeon_sim::math::V3;
use dungeon_sim::nav::{LinkKind, Rect};

pub struct NavDebugPlugin;

impl Plugin for NavDebugPlugin {
    fn build(&self, app: &mut App) {
        // `--nav-debug` starts with the overlay on (automated screenshots).
        let on = std::env::args().any(|a| a == "--nav-debug");
        app.insert_resource(NavDebug {
            on,
            collision: false,
        })
        .add_systems(
            Update,
            (toggle, draw, labels)
                .chain()
                .run_if(in_state(AppState::Dungeon))
                .run_if(resource_exists::<ActiveRun>),
        );
    }
}

#[derive(Resource)]
pub struct NavDebug {
    pub on: bool,
    pub collision: bool,
}

#[derive(Component)]
struct SurfaceLabel {
    index: usize,
}

fn toggle(keys: Res<ButtonInput<KeyCode>>, mut dbg: ResMut<NavDebug>) {
    if keys.just_pressed(KeyCode::KeyN) {
        if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
            dbg.collision = !dbg.collision;
            dbg.on |= dbg.collision;
        } else {
            dbg.on = !dbg.on;
        }
    }
}

fn height_color(y: f32, lo: f32, hi: f32) -> Color {
    let t = if hi > lo { (y - lo) / (hi - lo) } else { 0.0 };
    Color::srgb(0.2 + 0.8 * t, 0.5 + 0.4 * t, 1.0 - 0.8 * t)
}

/// Outline a rectangle following a surface's height (or a fixed height).
fn outline(gizmos: &mut Gizmos, r: Rect, h: impl Fn(V2) -> f32, color: Color) {
    let c = [
        r.min,
        V2::new(r.max.x, r.min.z),
        r.max,
        V2::new(r.min.x, r.max.z),
    ];
    for k in 0..4 {
        let (a, b) = (c[k], c[(k + 1) % 4]);
        gizmos.line(to_vec3(a, h(a) + 0.06), to_vec3(b, h(b) + 0.06), color);
    }
}

fn v3(p: V3) -> Vec3 {
    Vec3::new(p.x, p.y, p.z)
}

fn draw(dbg: Res<NavDebug>, run: Res<ActiveRun>, mut gizmos: Gizmos) {
    if !dbg.on {
        return;
    }
    let sim = &run.sim;
    let nav = &sim.nav;
    let (lo, hi) = nav
        .surfaces
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), s| {
            (lo.min(s.min_y()), hi.max(s.max_y()))
        });
    for s in &nav.surfaces {
        let color = if s.def.unreachable {
            Color::srgb(0.6, 0.2, 0.2)
        } else {
            height_color((s.min_y() + s.max_y()) * 0.5, lo, hi)
        };
        outline(&mut gizmos, s.def.rect, |p| s.height_at(p), color);
        if dbg.collision {
            outline(
                &mut gizmos,
                s.walk,
                |p| s.height_at(p) + 0.02,
                color.with_alpha(0.45),
            );
        }
    }
    for l in &nav.links {
        match l.kind {
            LinkKind::Walk => {
                if l.from < l.to {
                    outline(
                        &mut gizmos,
                        l.portal,
                        |_| l.entry.y + 0.05,
                        Color::srgb(0.2, 1.0, 0.3),
                    );
                }
            }
            LinkKind::Drop => {
                let a = v3(l.entry) + Vec3::Y * 0.3;
                let b = v3(l.exit) + Vec3::Y * 0.3;
                gizmos.arrow(a, b, Color::srgb(1.0, 0.55, 0.1));
                outline(
                    &mut gizmos,
                    l.portal,
                    |_| l.entry.y + 0.08,
                    Color::srgb(1.0, 0.55, 0.1),
                );
            }
        }
    }
    if dbg.collision {
        for w in &nav.walls {
            let a0 = to_vec3(w.a, w.bottom0);
            let b0 = to_vec3(w.b, w.bottom1);
            let up = Vec3::Y * w.height;
            let c = Color::srgba(1.0, 0.3, 0.3, 0.5);
            gizmos.line(a0 + up, b0 + up, c);
            gizmos.line(a0, a0 + up, c);
            gizmos.line(b0, b0 + up, c);
        }
    }
    // Paths of AI party members (and the selected actor, whoever it is).
    let selected = sim.player.and_then(|p| sim.actors[p].target);
    for id in 0..sim.actors.len() {
        let a = &sim.actors[id];
        // Party members (a human player has no goal, so nothing is drawn) and
        // whoever is selected.
        if !a.alive || !(!a.is_enemy() || selected == Some(id)) {
            continue;
        }
        let here = run.world(id, 0.15);
        if a.nav.stalled > 0.0 || a.nav.recover > 0.0 {
            gizmos.circle(
                Isometry3d::new(here, Quat::from_rotation_arc(Vec3::Z, Vec3::Y)),
                a.radius + 0.4,
                Color::srgb(1.0, 0.1, 0.1),
            );
        }
        let Some(goal) = a.move_goal else {
            continue;
        };
        let color = if selected == Some(id) {
            Color::WHITE
        } else {
            Color::srgba(0.4, 0.9, 1.0, 0.8)
        };
        match nav.path_points(a.pos, a.surface, goal) {
            Some(points) => {
                let mut prev = here;
                for p in points.iter().skip(1) {
                    let q = v3(*p) + Vec3::Y * 0.15;
                    gizmos.line(prev, q, color);
                    prev = q;
                }
                let wp = nav.next_waypoint(a.pos, a.surface, goal);
                gizmos.sphere(
                    Isometry3d::from_translation(to_vec3(wp.point, a.y + 0.3)),
                    0.12,
                    color,
                );
                gizmos.circle(
                    Isometry3d::new(
                        v3(goal) + Vec3::Y * 0.1,
                        Quat::from_rotation_arc(Vec3::Z, Vec3::Y),
                    ),
                    0.35,
                    color,
                );
            }
            None => {
                // Unreachable goal: a red cross where it was wanted.
                let g = v3(goal) + Vec3::Y * 0.2;
                let red = Color::srgb(1.0, 0.1, 0.1);
                gizmos.line(
                    g + Vec3::new(-0.5, 0.0, -0.5),
                    g + Vec3::new(0.5, 0.0, 0.5),
                    red,
                );
                gizmos.line(
                    g + Vec3::new(-0.5, 0.0, 0.5),
                    g + Vec3::new(0.5, 0.0, -0.5),
                    red,
                );
                gizmos.line(here, g, red.with_alpha(0.4));
            }
        }
    }
}

/// Surface name + elevation labels, anchored to each surface's centre.
#[allow(clippy::type_complexity)]
fn labels(
    mut commands: Commands,
    dbg: Res<NavDebug>,
    run: Res<ActiveRun>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut q: Query<(Entity, &SurfaceLabel, &mut Node)>,
) {
    let nav = &run.sim.nav;
    if !dbg.on {
        for (e, _, _) in &q {
            commands.entity(e).despawn();
        }
        return;
    }
    if q.is_empty() {
        for (i, s) in nav.surfaces.iter().enumerate() {
            let y = if s.is_ramp() {
                format!("y {:.1}..{:.1}", s.def.y, s.def.y_to.unwrap_or(s.def.y))
            } else {
                format!("y {:.1}", s.def.y)
            };
            commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                Text::new(format!("[{i}] {}\n{y}", s.name())),
                text_font(10.0),
                TextColor(Color::srgb(0.85, 0.95, 1.0)),
                TextShadow::default(),
                Pickable::IGNORE,
                SurfaceLabel { index: i },
                DespawnOnExit(AppState::Dungeon),
            ));
        }
        return;
    }
    let (cam, cam_gt) = *camera;
    for (_, label, mut node) in &mut q {
        let s = &nav.surfaces[label.index];
        let c = s.def.rect.center();
        let world = to_vec3(c, s.height_at(c) + 0.3);
        let in_front = (world - cam_gt.translation()).dot(*cam_gt.forward()) > 0.5;
        match cam
            .world_to_viewport(cam_gt, world)
            .ok()
            .filter(|_| in_front)
        {
            Some(p) => {
                node.display = Display::Flex;
                node.left = px(p.x - 40.0);
                node.top = px(p.y - 10.0);
            }
            None => node.display = Display::None,
        }
    }
}
