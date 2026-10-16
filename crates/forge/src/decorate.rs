//! Decoration anchors and kit decoration rules (0.8).
//!
//! The composer knows where decoration *could* go without covering floor:
//! * pillar holes and their four faces;
//! * the walls of each room (at mount height, above heads);
//! * both sides of every doorway (`branch_door` instead of `door_side` where a
//!   main-route room opens into a branch, so kits can mark the way to a
//!   detour, a flank, a cache or a landmark; 0.9);
//! * cut corners;
//! * the outside of room corners.
//!
//! Each of those is an anchor: position, facing, room, clearance, and the
//! direction to push a footprint so it stays outside walkable space.
//!
//! A kit's `[[decorate]]` rules say what to put at which anchors: a model
//! prop or primitive decor, optionally with a light, in rooms of given roles,
//! every n-th anchor, with a chance. Choices are deterministic from the seed.
//! Every placement at floor level is checked against the walkable areas, the
//! same rule the loader enforces; one that would cover floor is skipped.
//! Wall- and pillar-mounted pieces hang at 2.6 m, above head height.
//!
//! Without rules, the kit's 0.6/0.7 palette fields (`arena_prop`,
//! `pillar_prop`, `components.corner_decor`) become equivalent rules.

use crate::compose::Composed;
use crate::kit::{DecorateRule, Kit, LightSpec};
use crate::layout::{Dir, Plan, RoomKind};
use dungeon_sim::content::{
    AnchorConfig, AreaConfig, DecorConfig, DecorKind, LightConfig, PropConfig,
};
use dungeon_sim::math::V2;
use dungeon_sim::nav::Rect;
use dungeon_sim::poly::Poly;

/// Height of wall- and pillar-mounted anchors above the floor.
pub const MOUNT: f32 = 2.6;
/// Wall anchors are spread this far apart (at most).
const WALL_SPACING: f32 = 7.0;
/// Warm candle light (the 0.7 pillar candles).
const CANDLE: [f32; 3] = [1.0, 0.68, 0.38];

#[derive(Debug, Clone)]
pub struct Anchor {
    pub kind: &'static str,
    pub pos: V2,
    pub y: f32,
    /// Degrees; a model's -z faces this way (into the room).
    pub yaw: f32,
    pub room: usize,
    pub clearance: f32,
    /// A footprint of radius r is placed at `pos + push * r`.
    pub push: V2,
    pub used: String,
    /// `branch_door`: the purpose of the branch the doorway leads into.
    pub branch: Option<&'static str>,
}

fn r2(v: V2) -> V2 {
    V2::new((v.x * 100.0).round() / 100.0, (v.z * 100.0).round() / 100.0)
}

/// Yaw (degrees) that turns a model's -z toward `dir`.
fn yaw_toward(dir: V2) -> f32 {
    let d = (-dir.x).atan2(-dir.z).to_degrees();
    (d * 10.0).round() / 10.0
}

/// Corridor mouths on room `r`: (point on the wall line, side, half width,
/// corridor index).
fn mouths(plan: &Plan, r: usize) -> Vec<(V2, Dir, f32, usize)> {
    let room = &plan.rooms[r];
    plan.corridors
        .iter()
        .enumerate()
        .filter_map(|(k, c)| {
            let v = c.dir.lateral();
            if c.a == r {
                Some((
                    room.center() + c.dir.unit() * room.ext(c.dir) + v * c.off_a,
                    c.dir,
                    c.width / 2.0,
                    k,
                ))
            } else if c.b == r {
                let d = c.dir.opposite();
                Some((
                    room.center() + d.unit() * room.ext(d) + v * c.off_b,
                    d,
                    c.width / 2.0,
                    k,
                ))
            } else {
                None
            }
        })
        .collect()
}

/// The purpose of the branch corridor `k` belongs to, when it opens off a
/// main-route room (the doorway a player sees from the main route).
fn branch_of(plan: &Plan, room: usize, k: usize) -> Option<&'static str> {
    if !plan.main.contains(&room) {
        return None;
    }
    plan.branches
        .iter()
        .find(|b| b.corridors.contains(&k))
        .map(|b| b.kind.label())
}

/// All anchors of the plan, rooms in order.
pub fn anchors(plan: &Plan, pillars: &[Vec<Rect>]) -> Vec<Anchor> {
    let mut out = Vec::new();
    for (i, r) in plan.rooms.iter().enumerate() {
        let c = r.center();
        let (n, e, s, w) = (r.ext(Dir::N), r.ext(Dir::E), r.ext(Dir::S), r.ext(Dir::W));
        let [cut_ne, cut_nw, cut_sw, cut_se] = r.cut;
        // Pillars: centre (fill) and faces (mounts).
        for h in &pillars[i] {
            let hc = (h.min + h.max) * 0.5;
            let half = (h.max.x - h.min.x) * 0.5;
            // A landmark room's hole is a plinth for its landmark (0.9).
            if r.kind == RoomKind::Vista {
                out.push(Anchor {
                    kind: "plinth",
                    pos: r2(hc),
                    y: r.y,
                    yaw: 0.0,
                    room: i,
                    clearance: half,
                    push: V2::ZERO,
                    used: String::new(),
                    branch: None,
                });
                continue;
            }
            out.push(Anchor {
                kind: "pillar",
                pos: r2(hc),
                y: r.y,
                yaw: 0.0,
                room: i,
                clearance: half,
                push: V2::ZERO,
                used: String::new(),
                branch: None,
            });
            for nrm in [
                V2::new(1.0, 0.0),
                V2::new(0.0, 1.0),
                V2::new(-1.0, 0.0),
                V2::new(0.0, -1.0),
            ] {
                out.push(Anchor {
                    kind: "pillar_face",
                    pos: r2(hc + nrm * half),
                    y: r.y + MOUNT,
                    yaw: yaw_toward(nrm),
                    room: i,
                    clearance: 0.6,
                    push: V2::ZERO,
                    used: String::new(),
                    branch: None,
                });
            }
        }
        // Passage bends get no wall or door decoration, except the markers
        // on a doorway into a branch (0.9).
        let bend = r.kind == RoomKind::Bend;
        if bend && !plan.branches.iter().any(|b| b.from == i || b.to == Some(i)) {
            continue;
        }
        let ms = mouths(plan, i);
        // Walls, side by side: (side, start, end) of the side's straight part.
        let sides = [
            (
                Dir::N,
                V2::new(c.x - w + cut_nw, c.z + n),
                V2::new(c.x + e - cut_ne, c.z + n),
            ),
            (
                Dir::E,
                V2::new(c.x + e, c.z - s + cut_se),
                V2::new(c.x + e, c.z + n - cut_ne),
            ),
            (
                Dir::S,
                V2::new(c.x - w + cut_sw, c.z - s),
                V2::new(c.x + e - cut_se, c.z - s),
            ),
            (
                Dir::W,
                V2::new(c.x - w, c.z - s + cut_sw),
                V2::new(c.x - w, c.z + n - cut_nw),
            ),
        ];
        for (side, a, b) in sides {
            let len = a.dist(b);
            if len < 3.0 {
                continue;
            }
            let along = (b - a) * (1.0 / len);
            let into = side.unit() * -1.0;
            let count = (len / WALL_SPACING).floor().max(1.0) as usize;
            for k in 0..if bend { 0 } else { count } {
                let p = a + along * (len * (k as f32 + 0.5) / count as f32);
                // Not at a doorway, not in a corner.
                let near_door = ms
                    .iter()
                    .any(|(m, d, hw, _)| *d == side && m.dist(p) < hw + 1.6);
                let t = a.dist(p).min(b.dist(p));
                if near_door || t < 1.5 {
                    continue;
                }
                out.push(Anchor {
                    kind: "wall",
                    pos: r2(p),
                    y: r.y + MOUNT,
                    yaw: yaw_toward(into),
                    room: i,
                    clearance: 0.6,
                    push: V2::ZERO,
                    used: String::new(),
                    branch: None,
                });
            }
            // Both sides of every doorway on this side.
            for (m, d, hw, k) in &ms {
                if *d != side {
                    continue;
                }
                let branch = branch_of(plan, i, *k);
                if bend && branch.is_none() {
                    continue;
                }
                for sgn in [-1.0f32, 1.0] {
                    let p = *m + along * (sgn * (hw + 0.9));
                    let t = (p.x - a.x) * along.x + (p.z - a.z) * along.z;
                    if t < 0.8 || t > len - 0.8 {
                        continue;
                    }
                    out.push(Anchor {
                        kind: if branch.is_some() {
                            "branch_door"
                        } else {
                            "door_side"
                        },
                        pos: r2(p),
                        y: r.y + MOUNT,
                        yaw: yaw_toward(into),
                        room: i,
                        clearance: 0.6,
                        push: V2::ZERO,
                        used: String::new(),
                        branch,
                    });
                }
            }
        }
        if bend {
            continue;
        }
        // Corners: cut corners (inside the room's box, outside its floor)
        // and the outside of every corner (behind the walls).
        for (k, (sx, sz, ex, ez)) in [
            (1.0f32, 1.0f32, Dir::E, Dir::N),
            (-1.0, 1.0, Dir::W, Dir::N),
            (-1.0, -1.0, Dir::W, Dir::S),
            (1.0, -1.0, Dir::E, Dir::S),
        ]
        .into_iter()
        .enumerate()
        {
            let cut = r.cut[k];
            if cut >= 1.5 {
                out.push(Anchor {
                    kind: "cut_corner",
                    pos: r2(c + V2::new(
                        sx * (r.ext(ex) - cut / 3.0),
                        sz * (r.ext(ez) - cut / 3.0),
                    )),
                    y: r.y,
                    yaw: 45.0 + 90.0 * k as f32,
                    room: i,
                    clearance: cut / 6.0,
                    push: V2::ZERO,
                    used: String::new(),
                    branch: None,
                });
            }
            out.push(Anchor {
                kind: "outer_corner",
                pos: r2(c + V2::new(sx * (r.ext(ex) + 1.0), sz * (r.ext(ez) + 1.0))),
                y: r.y,
                yaw: 0.0,
                room: i,
                clearance: 3.0,
                push: V2::new(sx, sz),
                used: String::new(),
                branch: None,
            });
        }
    }
    out
}

/// The palette fields of 0.6/0.7 as rules (kits without `[[decorate]]`).
pub fn legacy_rules(kit: &Kit) -> Vec<DecorateRule> {
    let mut rules = Vec::new();
    let rule = |anchor: &str| DecorateRule {
        anchor: anchor.into(),
        rooms: Vec::new(),
        branches: Vec::new(),
        every: 1,
        offset: 0,
        chance: 1.0,
        prop: None,
        decor: Vec::new(),
        fit: false,
        height: None,
        light: None,
    };
    if let Some(spec) = &kit.palette.pillar_prop {
        rules.push(DecorateRule {
            every: 2,
            offset: 1,
            prop: Some(spec.clone()),
            light: Some(LightSpec {
                color: CANDLE,
                intensity: 0.45,
                range: 9.0,
                height: spec.height + 0.5,
                every: 1,
            }),
            ..rule("pillar")
        });
    }
    rules.push(DecorateRule {
        decor: vec![DecorKind::Pillar],
        fit: true,
        height: Some(7.0),
        ..rule("pillar")
    });
    if let Some(spec) = &kit.palette.arena_prop {
        rules.push(DecorateRule {
            rooms: vec!["arena".into()],
            prop: Some(spec.clone()),
            ..rule("cut_corner")
        });
    }
    if kit.components.corner_decor {
        rules.push(DecorateRule {
            decor: kit.palette.decor.clone(),
            ..rule("outer_corner")
        });
    }
    rules
}

fn role(kind: RoomKind) -> &'static str {
    match kind {
        RoomKind::Entrance => "entrance",
        RoomKind::Chamber => "chamber",
        RoomKind::Hall => "hall",
        RoomKind::Arena => "arena",
        RoomKind::Pit => "pit",
        RoomKind::Side => "side",
        RoomKind::Overlook => "overlook",
        RoomKind::Bend => "passage",
        RoomKind::Vault => "vault",
        RoomKind::Vista => "vista",
    }
}

pub fn room_role(kind: RoomKind) -> &'static str {
    role(kind)
}

/// Default size of primitive decoration.
fn decor_size(kind: DecorKind) -> (f32, f32) {
    match kind {
        DecorKind::Pillar => (1.3, 8.0),
        DecorKind::Stalagmite => (1.1, 6.0),
        DecorKind::Boulder => (1.4, 2.5),
        DecorKind::Crystal => (0.9, 4.0),
    }
}

fn hash(seed: u64, a: u64, b: u64, c: u64) -> f32 {
    let mut h = seed
        ^ a.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ b.wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ c.wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

fn area_polys(areas: &[AreaConfig]) -> Vec<(Poly, f32, f32)> {
    areas
        .iter()
        .filter_map(|a| {
            let poly = match (&a.points, a.min, a.max) {
                (Some(p), _, _) => Poly::new(p).ok()?,
                (None, Some(lo), Some(hi)) => Poly::from_rect(Rect::new(lo, hi)),
                _ => return None,
            };
            let (y0, y1) = (
                a.y.min(a.y_to.unwrap_or(a.y)),
                a.y.max(a.y_to.unwrap_or(a.y)),
            );
            Some((poly, y0, y1))
        })
        .collect()
}

/// Would a footprint at floor level cover walkable floor?
fn covers_floor(floors: &[(Poly, f32, f32)], p: V2, y: f32, r: f32) -> bool {
    floors.iter().any(|(poly, y0, y1)| {
        y < y1 + 2.2 && y > y0 - 2.5 && (poly.contains(p) || p.dist(poly.clamp(p)) < r + 0.05)
    })
}

/// Apply the kit's rules (or the legacy ones) to the anchors and add the
/// decoration, props and lights to `out`. Returns the anchors for the file.
pub fn decorate(plan: &Plan, kit: &Kit, out: &mut Composed) -> Vec<AnchorConfig> {
    let mut all = anchors(plan, &out.pillars);
    let rules = if kit.decorate.is_empty() {
        legacy_rules(kit)
    } else {
        kit.decorate.clone()
    };
    let floors = area_polys(&out.areas);
    for (ri, rule) in rules.iter().enumerate() {
        // Per room, count this kind's anchors in order.
        let mut count: Vec<u32> = vec![0; plan.rooms.len()];
        let mut placed = 0u32;
        #[allow(clippy::needless_range_loop)]
        for ai in 0..all.len() {
            let a = &all[ai];
            if a.kind != rule.anchor {
                continue;
            }
            let room = &plan.rooms[a.room];
            let j = count[a.room];
            count[a.room] += 1;
            if !a.used.is_empty()
                || (!rule.rooms.is_empty() && !rule.rooms.iter().any(|r| r == role(room.kind)))
                || (!rule.branches.is_empty()
                    && !a
                        .branch
                        .is_some_and(|b| rule.branches.iter().any(|x| x == b)))
                || !(j + rule.offset).is_multiple_of(rule.every)
                || hash(plan.seed, ri as u64, a.room as u64, j as u64) >= rule.chance
            {
                continue;
            }
            let mounted = a.y > room.y + 2.2;
            let (what, radius, height) = if let Some(spec) = &rule.prop {
                let r = if a.kind == "cut_corner" {
                    spec.radius.min(a.clearance)
                } else {
                    spec.radius
                };
                (format!("prop {}", spec.model), r, spec.height)
            } else {
                let kind = rule.decor[(a.room + j as usize) % rule.decor.len()];
                let (r0, h0) = decor_size(kind);
                let r = if rule.fit { a.clearance - 0.15 } else { r0 };
                (
                    format!("decor {kind:?}").to_lowercase(),
                    r,
                    rule.height.unwrap_or(h0),
                )
            };
            if radius < 0.1 {
                continue;
            }
            let pos = r2(a.pos + V2::new(a.push.x * radius, a.push.z * radius));
            if !mounted && covers_floor(&floors, pos, a.y, radius) {
                continue;
            }
            if let Some(spec) = &rule.prop {
                out.props.push(PropConfig {
                    model: spec.model.clone(),
                    pos,
                    y: a.y,
                    yaw: a.yaw,
                    scale: spec.scale,
                    radius,
                    height,
                    fallback: spec.fallback,
                });
            } else {
                let kind = rule.decor[(a.room + j as usize) % rule.decor.len()];
                out.decor.push(DecorConfig {
                    kind,
                    pos,
                    y: a.y,
                    radius,
                    height,
                    color: (kind == DecorKind::Crystal).then_some(kit.palette.light_deep),
                });
            }
            if let Some(l) = &rule.light
                && placed.is_multiple_of(l.every)
            {
                out.lights.push(LightConfig {
                    pos,
                    y: a.y + l.height,
                    color: l.color,
                    intensity: ((l.intensity * kit.palette.light_intensity) * 1000.0).round()
                        / 1000.0,
                    range: l.range,
                });
            }
            placed += 1;
            all[ai].used = what;
        }
    }
    all.into_iter()
        .map(|a| AnchorConfig {
            kind: a.kind.to_string(),
            pos: a.pos,
            y: a.y,
            yaw: a.yaw,
            room: plan.rooms[a.room].name.clone(),
            clearance: a.clearance,
            used: a.used,
        })
        .collect()
}
