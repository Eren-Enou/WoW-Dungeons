//! Layer B: room and corridor components. Each component is defined once in
//! a local frame — `s` along the corridor (from room `a` toward room `b`),
//! `t` to its left — and placed by the corridor's direction (a 90° rotation),
//! the room centre and the floor height. Rotations by 90° keep rectangles
//! axis-aligned, so they stay on the navigation's exact rectangle path.

use crate::kit::Kit;
use crate::layout::{CELL, Corridor, OVERLAP, Plan, Room, RoomKind, Route, Style};
use dungeon_sim::content::{
    AreaConfig, AxisConfig, DecorConfig, DecorKind, LavaConfig, LightConfig, LinkConfig,
    LinkKindConfig, UnderConfig,
};
use dungeon_sim::math::V2;
use dungeon_sim::poly::Poly;
use serde::Serialize;

/// Which component an area belongs to (for the structural check).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Owner {
    Room(usize),
    Corridor(usize),
}

pub struct Composed {
    pub areas: Vec<AreaConfig>,
    pub owners: Vec<Owner>,
    pub links: Vec<LinkConfig>,
    pub lava: Vec<LavaConfig>,
    pub decor: Vec<DecorConfig>,
    pub lights: Vec<LightConfig>,
    pub entrance: V2,
    /// Walkable pieces of each room (world space), for placing mobs.
    pub floors: Vec<Vec<Poly>>,
}

/// Chasm depth under bridges.
const CHASM: f32 = 6.0;
/// The strips round a pit overlap this much at the corners. Walk areas are
/// inset 0.4 m on every edge, so a 1.5 m overlap left a 0.7 m portal that
/// companions turning the corner oscillated across; 3 m leaves 2.2 m.
const STRIP_OVERLAP: f32 = 3.0;
/// Lava sits this far below a pit room's floor.
const PIT_DEPTH: f32 = 1.2;

fn rect_area(name: String, a: V2, b: V2, y: f32) -> AreaConfig {
    AreaConfig {
        name,
        min: Some(V2::new(a.x.min(b.x), a.z.min(b.z))),
        max: Some(V2::new(a.x.max(b.x), a.z.max(b.z))),
        points: None,
        slope_from: None,
        slope_to: None,
        tint: None,
        y,
        y_to: None,
        axis: AxisConfig::Z,
        stairs: false,
        walls: true,
        wall_height: None,
        under: UnderConfig::Solid,
        unreachable: false,
    }
}

fn poly_area(name: String, pts: Vec<V2>, y: f32) -> AreaConfig {
    AreaConfig {
        points: Some(pts),
        min: None,
        max: None,
        ..rect_area(name, V2::ZERO, V2::new(1.0, 1.0), y)
    }
}

fn r2(v: V2) -> V2 {
    // Round to centimetres: stable, readable output.
    V2::new((v.x * 100.0).round() / 100.0, (v.z * 100.0).round() / 100.0)
}

/// Room footprint in world space.
pub fn room_shape(r: &Room) -> Vec<V2> {
    let c = r.center();
    let (h, k) = (r.half, r.chamfer);
    let local: Vec<(f32, f32)> = if k > 0.0 {
        vec![
            (h, -(h - k)),
            (h, h - k),
            (h - k, h),
            (-(h - k), h),
            (-h, h - k),
            (-h, -(h - k)),
            (-(h - k), -h),
            (h - k, -h),
        ]
    } else {
        vec![(h, -h), (h, h), (-h, h), (-h, -h)]
    };
    local
        .into_iter()
        .map(|(x, z)| r2(c + V2::new(x, z)))
        .collect()
}

fn tint_for(kit: &Kit, y: f32, deepest: f32) -> [f32; 3] {
    let t = if deepest < -0.01 {
        (y / deepest).clamp(0.0, 1.0)
    } else {
        0.0
    };
    lerp3(kit.palette.tint_shallow, kit.palette.tint_deep, t)
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let f = |i: usize| ((a[i] + (b[i] - a[i]) * t) * 1000.0).round() / 1000.0;
    [f(0), f(1), f(2)]
}

pub fn compose(plan: &Plan, kit: &Kit) -> Composed {
    let deepest = plan.rooms.iter().map(|r| r.y).fold(0.0f32, f32::min);
    let mut out = Composed {
        areas: Vec::new(),
        owners: Vec::new(),
        links: Vec::new(),
        lava: Vec::new(),
        decor: Vec::new(),
        lights: Vec::new(),
        entrance: V2::ZERO,
        floors: vec![Vec::new(); plan.rooms.len()],
    };

    // ---- Rooms.
    for (i, r) in plan.rooms.iter().enumerate() {
        let tint = Some(tint_for(kit, r.y, deepest));
        let c = r.center();
        if r.kind == RoomKind::Pit {
            let (h, p) = (r.half, r.pit);
            let pieces = [
                ("west", V2::new(-h, -h), V2::new(-p, h)),
                ("east", V2::new(p, -h), V2::new(h, h)),
                (
                    "north",
                    V2::new(-p - STRIP_OVERLAP, p),
                    V2::new(p + STRIP_OVERLAP, h),
                ),
                (
                    "south",
                    V2::new(-p - STRIP_OVERLAP, -h),
                    V2::new(p + STRIP_OVERLAP, -p),
                ),
            ];
            for (side, a, b) in pieces {
                let mut area = rect_area(format!("{} ({side})", r.name), r2(c + a), r2(c + b), r.y);
                area.tint = tint;
                out.floors[i].push(Poly::from_rect(dungeon_sim::nav::Rect::new(
                    area.min.expect("rect"),
                    area.max.expect("rect"),
                )));
                out.areas.push(area);
                out.owners.push(Owner::Room(i));
            }
            out.lava.push(LavaConfig {
                min: Some(r2(c - V2::new(p, p))),
                max: Some(r2(c + V2::new(p, p))),
                points: None,
                y: r.y - PIT_DEPTH,
            });
            // A molten column in the pit: a landmark that stands in lava.
            out.decor.push(DecorConfig {
                kind: DecorKind::Pillar,
                pos: c,
                y: r.y - PIT_DEPTH,
                radius: (p * 0.25).clamp(0.6, 1.1),
                height: 9.0,
                color: None,
            });
        } else {
            let pts = room_shape(r);
            let mut area = if r.chamfer > 0.0 {
                poly_area(r.name.clone(), pts.clone(), r.y)
            } else {
                rect_area(r.name.clone(), pts[3], pts[1], r.y)
            };
            area.tint = tint;
            out.floors[i].push(Poly::new(&pts).expect("room shapes are convex"));
            out.areas.push(area);
            out.owners.push(Owner::Room(i));
        }
        // Light over every room: cool near the entrance, hot deep down.
        let t = if deepest < -0.01 {
            (r.y / deepest).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (intensity, color) = match r.kind {
            RoomKind::Arena => (
                1.5,
                lerp3(
                    kit.palette.light_shallow,
                    kit.palette.light_deep,
                    t.max(0.7),
                ),
            ),
            RoomKind::Pit => (1.3, kit.palette.light_deep),
            RoomKind::Entrance => (1.1, kit.palette.light_shallow),
            _ => (
                1.0,
                lerp3(kit.palette.light_shallow, kit.palette.light_deep, t),
            ),
        };
        out.lights.push(LightConfig {
            pos: c,
            y: r.y + 6.5,
            color,
            intensity,
            range: (r.half * 2.4).round(),
        });
        // Rock columns just outside the room's corners, poking above the walls.
        let mix = &kit.palette.decor;
        for (k, (sx, sz)) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
            .into_iter()
            .enumerate()
        {
            let kind = mix[(i + k) % mix.len()];
            let (radius, height) = match kind {
                DecorKind::Pillar => (1.3, 8.0),
                DecorKind::Stalagmite => (1.1, 6.0),
                DecorKind::Boulder => (1.4, 2.5),
                DecorKind::Crystal => (0.9, 4.0),
            };
            let d = r.half + 1.0 + radius;
            out.decor.push(DecorConfig {
                kind,
                pos: r2(c + V2::new(sx * d, sz * d)),
                y: r.y,
                radius,
                height,
                color: if kind == DecorKind::Crystal {
                    Some(kit.palette.light_deep)
                } else {
                    None
                },
            });
        }
    }

    // ---- Corridors.
    for (k, cor) in plan.corridors.iter().enumerate() {
        compose_corridor(plan, kit, k, cor, deepest, &mut out);
    }

    // ---- Entrance: the far side of the entrance room from its exit.
    let ent = &plan.rooms[plan.main[0]];
    let exit = plan
        .corridors
        .iter()
        .find(|c| c.a == plan.main[0] && matches!(plan.rooms[c.b].route, Route::Main(1)))
        .map(|c| c.dir.unit())
        .unwrap_or(V2::new(0.0, 1.0));
    out.entrance = r2(ent.center() - exit * (ent.half - 2.5));
    out
}

fn compose_corridor(
    plan: &Plan,
    kit: &Kit,
    k: usize,
    cor: &Corridor,
    deepest: f32,
    out: &mut Composed,
) {
    let (ra, rb) = (&plan.rooms[cor.a], &plan.rooms[cor.b]);
    let u = cor.dir.unit();
    let v = cor.dir.lateral();
    let ca = ra.center();
    let at = |s: f32, t: f32| r2(ca + u * s + v * t);
    let sa = ra.half;
    let sb = CELL - rb.half;
    let w2 = cor.width / 2.0;
    let base = format!("{} - {}", ra.name, rb.name);
    let tint = Some(tint_for(kit, ra.y.min(rb.y), deepest));
    let push = |out: &mut Composed, mut a: AreaConfig| {
        if a.tint.is_none() {
            a.tint = tint;
        }
        out.areas.push(a);
        out.owners.push(Owner::Corridor(k));
    };
    match cor.style {
        Style::Straight | Style::Bridge => {
            let mut a = rect_area(
                if cor.style == Style::Bridge {
                    format!("{base} bridge")
                } else {
                    format!("{base} passage")
                },
                at(sa - OVERLAP, cor.off_a - w2),
                at(sb + OVERLAP, cor.off_a + w2),
                ra.y,
            );
            if cor.style == Style::Bridge {
                a.under = UnderConfig::Open;
                a.walls = false;
                let lw = w2 + 4.0;
                let (p, q) = (at(sa, -lw), at(sb, lw));
                out.lava.push(LavaConfig {
                    min: Some(V2::new(p.x.min(q.x), p.z.min(q.z))),
                    max: Some(V2::new(p.x.max(q.x), p.z.max(q.z))),
                    points: None,
                    y: ra.y - CHASM,
                });
                // Rock stacks rising from the lava either side of the bridge.
                let mid = (sa + sb) / 2.0;
                for side in [-1.0, 1.0] {
                    out.decor.push(DecorConfig {
                        kind: DecorKind::Stalagmite,
                        pos: at(mid, side * (w2 + 2.5)),
                        y: ra.y - CHASM,
                        radius: 1.0,
                        height: CHASM - 0.5,
                        color: None,
                    });
                }
                out.lights.push(LightConfig {
                    pos: at(mid, 0.0),
                    y: ra.y - CHASM + 1.5,
                    color: kit.palette.light_deep,
                    intensity: 1.2,
                    range: 16.0,
                });
            }
            push(out, a);
        }
        Style::Ramp => {
            let mut a = rect_area(
                format!("{base} {}", if cor.stairs { "stair" } else { "ramp" }),
                at(sa - OVERLAP, -w2),
                at(sb + OVERLAP, w2),
                0.0,
            );
            // Height at the `min` edge, `y_to` at the `max` edge, along the axis.
            let forward = u.x + u.z > 0.0;
            let (y_min, y_max) = if forward { (ra.y, rb.y) } else { (rb.y, ra.y) };
            a.y = y_min;
            a.y_to = Some(y_max);
            a.axis = if u.x != 0.0 {
                AxisConfig::X
            } else {
                AxisConfig::Z
            };
            a.stairs = cor.stairs;
            push(out, a);
        }
        Style::Winding => {
            let m = (sa + sb) / 2.0;
            let (oa, ob) = (cor.off_a, cor.off_b);
            push(
                out,
                rect_area(
                    format!("{base} bend (1)"),
                    at(sa - OVERLAP, oa - w2),
                    at(m + w2, oa + w2),
                    ra.y,
                ),
            );
            push(
                out,
                rect_area(
                    format!("{base} bend (2)"),
                    at(m - w2, oa.min(ob) - w2),
                    at(m + w2, oa.max(ob) + w2),
                    ra.y,
                ),
            );
            push(
                out,
                rect_area(
                    format!("{base} bend (3)"),
                    at(m - w2, ob - w2),
                    at(sb + OVERLAP, ob + w2),
                    ra.y,
                ),
            );
        }
        Style::Angled => {
            let pts = vec![
                at(sa - OVERLAP, cor.off_a - w2),
                at(sb + OVERLAP, cor.off_b - w2),
                at(sb + OVERLAP, cor.off_b + w2),
                at(sa - OVERLAP, cor.off_a + w2),
            ];
            push(out, poly_area(format!("{base} slant"), pts, ra.y));
        }
        Style::Drop => {
            // Raised walkway at the overlook's height ending flush with the
            // lower room's edge; the drop zone straddles that edge.
            let mut a = rect_area(
                format!("{base} ledge"),
                at(sa - OVERLAP, cor.off_a - w2),
                at(sb, cor.off_a + w2),
                ra.y,
            );
            a.walls = false;
            let ledge_name = a.name.clone();
            push(out, a);
            let probe = ca + u * (sb + 0.6) + v * cor.off_a;
            let landing = landing_area(plan, cor.b, probe, out);
            let (p, q) = (
                at(sb - 1.2, cor.off_a - w2 + 0.3),
                at(sb + 1.2, cor.off_a + w2 - 0.3),
            );
            out.links.push(LinkConfig {
                from: ledge_name,
                to: landing,
                kind: LinkKindConfig::Drop,
                min: Some(V2::new(p.x.min(q.x), p.z.min(q.z))),
                max: Some(V2::new(p.x.max(q.x), p.z.max(q.z))),
            });
        }
    }
}

/// Name of the lower room's area under `probe` (pit rooms have four pieces).
fn landing_area(plan: &Plan, room: usize, probe: V2, out: &Composed) -> String {
    out.areas
        .iter()
        .zip(&out.owners)
        .find(|(a, o)| {
            **o == Owner::Room(room)
                && match (a.min, a.max, &a.points) {
                    (Some(lo), Some(hi), _) => {
                        probe.x >= lo.x && probe.x <= hi.x && probe.z >= lo.z && probe.z <= hi.z
                    }
                    (_, _, Some(p)) => Poly::new(p).is_ok_and(|p| p.contains(probe)),
                    _ => false,
                }
        })
        .map(|(a, _)| a.name.clone())
        .unwrap_or_else(|| plan.rooms[room].name.clone())
}
