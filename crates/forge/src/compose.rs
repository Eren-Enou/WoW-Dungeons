//! Layer B: room and corridor components. Each component is defined once in
//! a local frame — `s` along the corridor (from room `a` toward room `b`),
//! `t` to its left — and placed by the corridor's direction (a 90° rotation),
//! the cell centre and the floor height. Rotations by 90° keep rectangles
//! axis-aligned, so they stay on the navigation's exact rectangle path.
//!
//! Room shapes come from the kit: plain polygons with per-side extents and
//! corner cuts, rings of floor round a lava pit, and pillared halls whose
//! pillars are walled holes in the floor (they block movement and sight).

use crate::kit::{Chasm, Kit};
use crate::layout::{Corridor, Dir, OVERLAP, Plan, Room, RoomKind, Route, Shape, Style};
use dungeon_sim::content::{
    AreaConfig, AxisConfig, DecorConfig, DecorKind, LavaConfig, LightConfig, LinkConfig,
    LinkKindConfig, PropConfig, UnderConfig,
};
use dungeon_sim::math::V2;
use dungeon_sim::nav::Rect;
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
    pub props: Vec<PropConfig>,
    pub entrance: V2,
    /// Walkable pieces of each room (world space), for placing mobs.
    pub floors: Vec<Vec<Poly>>,
    /// Pillar holes of each room (world space), for placement and maps.
    pub pillars: Vec<Vec<Rect>>,
    /// Decoration anchors and what was placed at them.
    pub anchors: Vec<dungeon_sim::content::AnchorConfig>,
}

/// Chasm depth under bridges.
const CHASM: f32 = 6.0;
/// Lava sits this far below a pit room's floor.
const PIT_DEPTH: f32 = 1.2;
/// Pieces round a pit or a pillar overlap this much. Walk areas are inset
/// 0.4 m on every edge, so a 1.5 m overlap left a 0.7 m portal that
/// companions turning the corner oscillated across (0.6); 3 m leaves 2.2 m.
const STRIP_OVERLAP: f32 = 3.0;
/// Pillars keep at least this much floor between them and the room's walls
/// (room for corridors joining and for walking round).
const PILLAR_WALL_GAP: f32 = 4.0;

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

/// An area's walkable outline (rectangle or polygon).
pub fn area_poly(a: &AreaConfig) -> Option<Poly> {
    match (&a.points, a.min, a.max) {
        (Some(p), _, _) => Poly::new(p).ok(),
        (None, Some(lo), Some(hi)) => Some(Poly::from_rect(Rect::new(lo, hi))),
        _ => None,
    }
}

fn r2(v: V2) -> V2 {
    // Round to centimetres: stable, readable output.
    V2::new((v.x * 100.0).round() / 100.0, (v.z * 100.0).round() / 100.0)
}

fn has_cuts(r: &Room) -> bool {
    r.cut.iter().any(|c| *c > 0.0)
}

/// Room footprint (one convex polygon) in world space, counter-clockwise.
pub fn room_shape(r: &Room) -> Vec<V2> {
    let c = r.center();
    let (n, e, s, w) = (r.ext(Dir::N), r.ext(Dir::E), r.ext(Dir::S), r.ext(Dir::W));
    let [ne, nw, sw, se] = r.cut;
    let local: Vec<(f32, f32)> = if has_cuts(r) {
        vec![
            (e, -(s - se)),
            (e, n - ne),
            (e - ne, n),
            (-(w - nw), n),
            (-w, n - nw),
            (-w, -(s - sw)),
            (-(w - sw), -s),
            (e - se, -s),
        ]
    } else {
        vec![(e, -s), (e, n), (-w, n), (-w, -s)]
    };
    let mut pts: Vec<V2> = local
        .into_iter()
        .map(|(x, z)| r2(c + V2::new(x, z)))
        .collect();
    pts.dedup_by(|a, b| a.dist(*b) < 1e-3);
    pts
}

/// Pillar centres along one axis between `lo` and `hi` (cell-local),
/// centred, `spacing` apart, keeping PILLAR_WALL_GAP from both walls.
fn pillar_axis(lo: f32, hi: f32, size: f32, spacing: f32) -> Vec<f32> {
    let a = lo + PILLAR_WALL_GAP + size / 2.0;
    let b = hi - PILLAR_WALL_GAP - size / 2.0;
    if b < a {
        return Vec::new();
    }
    let count = ((b - a) / spacing).floor() as usize + 1;
    let mid = (a + b) / 2.0;
    (0..count)
        .map(|k| mid + (k as f32 - (count as f32 - 1.0) / 2.0) * spacing)
        .collect()
}

/// Pillar holes of a room (cell-local rectangles), empty if none fit.
pub fn pillar_holes(r: &Room) -> Vec<Rect> {
    let Shape::Pillars { size, spacing } = r.shape else {
        return Vec::new();
    };
    let xs = pillar_axis(-r.ext(Dir::W), r.ext(Dir::E), size, spacing);
    let zs = pillar_axis(-r.ext(Dir::S), r.ext(Dir::N), size, spacing);
    let mut out = Vec::new();
    for &x in &xs {
        for &z in &zs {
            out.push(Rect::new(
                V2::new(x - size / 2.0, z - size / 2.0),
                V2::new(x + size / 2.0, z + size / 2.0),
            ));
        }
    }
    out
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

fn push_rect(
    out: &mut Composed,
    i: usize,
    name: String,
    c: V2,
    (a, b): (V2, V2),
    y: f32,
    tint: Option<[f32; 3]>,
) {
    let mut area = rect_area(name, r2(c + a), r2(c + b), y);
    area.tint = tint;
    out.floors[i].push(Poly::from_rect(Rect::new(
        area.min.expect("rect"),
        area.max.expect("rect"),
    )));
    out.areas.push(area);
    out.owners.push(Owner::Room(i));
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
        props: Vec::new(),
        entrance: V2::ZERO,
        floors: vec![Vec::new(); plan.rooms.len()],
        pillars: vec![Vec::new(); plan.rooms.len()],
        anchors: Vec::new(),
    };

    // ---- Rooms.
    for (i, r) in plan.rooms.iter().enumerate() {
        let tint = Some(tint_for(kit, r.y, deepest));
        let c = r.center();
        let (n, e, s, w) = (r.ext(Dir::N), r.ext(Dir::E), r.ext(Dir::S), r.ext(Dir::W));
        let holes = pillar_holes(r);
        match r.shape {
            Shape::Pit(p) => {
                let pieces = [
                    ("west", V2::new(-w, -s), V2::new(-p, n)),
                    ("east", V2::new(p, -s), V2::new(e, n)),
                    (
                        "north",
                        V2::new(-p - STRIP_OVERLAP, p),
                        V2::new(p + STRIP_OVERLAP, n),
                    ),
                    (
                        "south",
                        V2::new(-p - STRIP_OVERLAP, -s),
                        V2::new(p + STRIP_OVERLAP, -p),
                    ),
                ];
                for (side, a, b) in pieces {
                    push_rect(
                        &mut out,
                        i,
                        format!("{} ({side})", r.name),
                        c,
                        (a, b),
                        r.y,
                        tint,
                    );
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
            }
            Shape::Pillars { size, .. } if !holes.is_empty() => {
                // Bands of floor between pillar columns (full depth), and
                // pieces between the pillars of each column overlapping the
                // bands either side by STRIP_OVERLAP.
                let mut xs: Vec<f32> = holes.iter().map(|h| h.min.x).collect();
                xs.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
                let mut zs: Vec<f32> = holes.iter().map(|h| h.min.z).collect();
                zs.sort_by(f32::total_cmp);
                zs.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
                let mut x_edges = vec![-w];
                for &x in &xs {
                    x_edges.push(x);
                    x_edges.push(x + size);
                }
                x_edges.push(e);
                for (k, band) in x_edges.chunks(2).enumerate() {
                    push_rect(
                        &mut out,
                        i,
                        format!("{} (aisle {})", r.name, k + 1),
                        c,
                        (V2::new(band[0], -s), V2::new(band[1], n)),
                        r.y,
                        tint,
                    );
                }
                let mut z_edges = vec![-s];
                for &z in &zs {
                    z_edges.push(z);
                    z_edges.push(z + size);
                }
                z_edges.push(n);
                for (k, &x) in xs.iter().enumerate() {
                    let (x0, x1) = (
                        (x - STRIP_OVERLAP).max(-w),
                        (x + size + STRIP_OVERLAP).min(e),
                    );
                    for (j, gap) in z_edges.chunks(2).enumerate() {
                        push_rect(
                            &mut out,
                            i,
                            format!("{} (row {} gap {})", r.name, k + 1, j + 1),
                            c,
                            (V2::new(x0, gap[0]), V2::new(x1, gap[1])),
                            r.y,
                            tint,
                        );
                    }
                }
                // The pillars themselves: holes, decorated by the kit's
                // rules (`decorate.rs`).
                for h in &holes {
                    out.pillars[i].push(Rect::new(c + h.min, c + h.max));
                }
            }
            _ => {
                let pts = room_shape(r);
                let mut area = if has_cuts(r) {
                    poly_area(r.name.clone(), pts.clone(), r.y)
                } else {
                    rect_area(
                        r.name.clone(),
                        r2(c + V2::new(-w, -s)),
                        r2(c + V2::new(e, n)),
                        r.y,
                    )
                };
                area.tint = tint;
                out.floors[i].push(Poly::new(&pts).expect("room shapes are convex"));
                out.areas.push(area);
                out.owners.push(Owner::Room(i));
            }
        }
        // Light over every room: cool near the entrance, warm deep down.
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
            RoomKind::Bend => (
                0.6,
                lerp3(kit.palette.light_shallow, kit.palette.light_deep, t),
            ),
            _ => (
                1.0,
                lerp3(kit.palette.light_shallow, kit.palette.light_deep, t),
            ),
        };
        let reach = r.ext.iter().copied().fold(0.0f32, f32::max);
        out.lights.push(LightConfig {
            pos: r2(r.mid()),
            y: r.y + 6.5,
            color,
            intensity: intensity * kit.palette.light_intensity,
            range: (reach * 2.4).round(),
        });
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
        .map(|c| c.dir)
        .unwrap_or(Dir::N);
    out.entrance = r2(ent.center() - exit.unit() * (ent.ext(exit.opposite()) - 2.5));
    // ---- Decoration at anchors (kit rules).
    out.anchors = crate::decorate::decorate(plan, kit, &mut out);
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
    let sa = ra.ext(cor.dir);
    let sb = plan.cell - rb.ext(cor.dir.opposite());
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
                let mid = (sa + sb) / 2.0;
                match kit.components.chasm {
                    Chasm::Lava => {
                        let lw = w2 + 4.0;
                        let (p, q) = (at(sa, cor.off_a - lw), at(sb, cor.off_a + lw));
                        out.lava.push(LavaConfig {
                            min: Some(V2::new(p.x.min(q.x), p.z.min(q.z))),
                            max: Some(V2::new(p.x.max(q.x), p.z.max(q.z))),
                            points: None,
                            y: ra.y - CHASM,
                        });
                        // Rock stacks rising from the lava either side of the bridge.
                        for side in [-1.0, 1.0] {
                            out.decor.push(DecorConfig {
                                kind: DecorKind::Stalagmite,
                                pos: at(mid, cor.off_a + side * (w2 + 2.5)),
                                y: ra.y - CHASM,
                                radius: 1.0,
                                height: CHASM - 0.5,
                                color: None,
                            });
                        }
                        out.lights.push(LightConfig {
                            pos: at(mid, cor.off_a),
                            y: ra.y - CHASM + 1.5,
                            color: kit.palette.light_deep,
                            intensity: 1.2 * kit.palette.light_intensity,
                            range: 16.0,
                        });
                    }
                    Chasm::Void => {
                        // Nothing below: a faint cold light over the deck.
                        out.lights.push(LightConfig {
                            pos: at(mid, cor.off_a),
                            y: ra.y + 4.0,
                            color: kit.palette.light_shallow,
                            intensity: 0.5 * kit.palette.light_intensity,
                            range: 14.0,
                        });
                    }
                }
            }
            push(out, a);
        }
        Style::Ramp => {
            let mut a = rect_area(
                format!("{base} {}", if cor.stairs { "stair" } else { "ramp" }),
                at(sa - OVERLAP, cor.off_a - w2),
                at(sb + OVERLAP, cor.off_a + w2),
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

/// Name of the lower room's area under `probe` (pit and pillared rooms have
/// several pieces).
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
