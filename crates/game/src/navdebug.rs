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
//! * surfaces the entrance can't reach (and that aren't marked
//!   `unreachable`) in magenta: a disconnected region is a content bug.
//!
//! Everything is cheap gizmo lines plus a few world-anchored text labels.
//! The surface, link and wall layers only need a `Nav`, so the dungeon viewer
//! (`viewer.rs`) draws them too; AI paths need the running simulation.

use crate::camera::MainCamera;
use crate::common::{AppState, text_font, to_vec3};
use crate::dungeon::ActiveRun;
use crate::viewer::ViewerScene;
use bevy::picking::Pickable;
use bevy::prelude::*;
use dungeon_sim::V2;
use dungeon_sim::math::V3;
use dungeon_sim::nav::LinkKind;
use dungeon_sim::nav::Nav;
use dungeon_sim::poly::Poly;

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
        )
        .add_systems(
            Update,
            (draw_viewer, labels_viewer)
                .chain()
                .run_if(in_state(AppState::Viewer))
                .run_if(resource_exists::<ViewerScene>),
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

/// Outline a polygon following a surface's height (or a fixed height).
fn outline(gizmos: &mut Gizmos, p: &Poly, h: impl Fn(V2) -> f32, color: Color) {
    for (a, b) in p.edges() {
        gizmos.line(to_vec3(a, h(a) + 0.06), to_vec3(b, h(b) + 0.06), color);
    }
}

fn v3(p: V3) -> Vec3 {
    Vec3::new(p.x, p.y, p.z)
}

/// Surfaces the entrance surface can't reach on foot (and that aren't marked
/// intentionally unreachable).
pub fn disconnected(nav: &Nav, entrance: Vec3) -> Vec<bool> {
    let start = nav.locate(V2::new(entrance.x, entrance.z), entrance.y);
    (0..nav.surfaces.len())
        .map(|i| {
            let s = &nav.surfaces[i];
            !s.def.unreachable
                && start.is_some_and(|a| a as usize != i && !nav.reachable(a, i as _))
        })
        .collect()
}

/// Surfaces, portals, drops and (optionally) walls of a `Nav`.
pub fn draw_nav(nav: &Nav, collision: bool, cut_off: &[bool], gizmos: &mut Gizmos) {
    let (lo, hi) = nav
        .surfaces
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), s| {
            (lo.min(s.min_y()), hi.max(s.max_y()))
        });
    for (i, s) in nav.surfaces.iter().enumerate() {
        let color = if cut_off.get(i).copied().unwrap_or(false) {
            Color::srgb(1.0, 0.1, 0.9)
        } else if s.def.unreachable {
            Color::srgb(0.6, 0.2, 0.2)
        } else {
            height_color((s.min_y() + s.max_y()) * 0.5, lo, hi)
        };
        outline(gizmos, &s.def.shape, |p| s.height_at(p), color);
        if collision {
            outline(
                gizmos,
                &s.walk,
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
                        gizmos,
                        &l.portal,
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
                    gizmos,
                    &l.portal,
                    |_| l.entry.y + 0.08,
                    Color::srgb(1.0, 0.55, 0.1),
                );
            }
        }
    }
    if collision {
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
}

fn draw(dbg: Res<NavDebug>, run: Res<ActiveRun>, mut gizmos: Gizmos) {
    if !dbg.on {
        return;
    }
    let sim = &run.sim;
    let nav = &sim.nav;
    draw_nav(nav, dbg.collision, &[], &mut gizmos);
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

fn draw_viewer(dbg: Res<NavDebug>, scene: Res<ViewerScene>, mut gizmos: Gizmos) {
    if dbg.on {
        draw_nav(&scene.nav, dbg.collision, &scene.cut_off, &mut gizmos);
    }
}

fn labels(
    commands: Commands,
    dbg: Res<NavDebug>,
    run: Res<ActiveRun>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    q: Query<(Entity, &SurfaceLabel, &mut Node)>,
) {
    place_labels(
        commands,
        &dbg,
        &run.sim.nav,
        &[],
        AppState::Dungeon,
        *camera,
        q,
    );
}

fn labels_viewer(
    commands: Commands,
    dbg: Res<NavDebug>,
    scene: Res<ViewerScene>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    q: Query<(Entity, &SurfaceLabel, &mut Node)>,
) {
    place_labels(
        commands,
        &dbg,
        &scene.nav,
        &scene.cut_off,
        AppState::Viewer,
        *camera,
        q,
    );
}

/// Surface name + elevation labels, anchored to each surface's centre.
fn place_labels(
    mut commands: Commands,
    dbg: &NavDebug,
    nav: &Nav,
    cut_off: &[bool],
    scope: AppState,
    camera: (&Camera, &GlobalTransform),
    mut q: Query<(Entity, &SurfaceLabel, &mut Node)>,
) {
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
            let warn = if cut_off.get(i).copied().unwrap_or(false) {
                "\nNOT REACHABLE from the entrance"
            } else {
                ""
            };
            commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                Text::new(format!("[{i}] {}\n{y}{warn}", s.name())),
                text_font(10.0),
                TextColor(Color::srgb(0.85, 0.95, 1.0)),
                TextShadow::default(),
                Pickable::IGNORE,
                SurfaceLabel { index: i },
                DespawnOnExit(scope),
            ));
        }
        return;
    }
    let (cam, cam_gt) = camera;
    for (_, label, mut node) in &mut q {
        let s = &nav.surfaces[label.index];
        let c = s.def.shape.centroid();
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
