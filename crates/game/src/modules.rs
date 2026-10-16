//! Modular environment art: GLB pieces assembled along the dungeon's real
//! geometry (`[dungeon.modules]`, written by Dungeon Forge from a kit).
//!
//! A fixed-size room model can't fit rooms of every size, so pieces follow
//! the boundaries navigation already has:
//! * `wall`: each wall face is split into whole pieces, each stretched to
//!   fit (at most about ±25%) and scaled to the wall's height;
//! * `corner`: a post at every wall joint;
//! * `door`: a frame wherever a corridor opens into a room, sized to the
//!   opening, its posts outside the opening and its lintel above head height;
//! * `pillar`: fills each `pillar` anchor (a hole in the walkable floor),
//!   scaled to the hole, so cover looks exactly as big as it is.
//!
//! Every piece's local frame: x along the wall (or across the opening), y up
//! from the floor, z away from walkable space (inner face at z = 0). Floors
//! stay the procedural prisms: they are the walkable truth.
//!
//! Visual only: navigation, line of sight and the simulation never read any
//! of this. A missing model file falls back to the procedural geometry for
//! that piece kind, and the instance is recorded as a fallback.

use crate::cave::hash01;
use crate::common::AppState;
use crate::env::{ModuleInstance, ModuleInstances};
use crate::structure::Structure;
use bevy::prelude::*;
use dungeon_sim::V2;
use dungeon_sim::content::{Content, ModuleConfig};
use dungeon_sim::nav::{Nav, WALL_MARGIN, Wall};

/// Walls lower than this (lava lips) stay procedural.
const MIN_WALL: f32 = 1.0;
/// Pieces are stretched at most this much before another is added.
const MAX_STRETCH: f32 = 0.25;

/// Is the module's model (or `model`) present under `assets/`?
pub fn available(model: &str) -> bool {
    crate::cave::prop_model_available(model)
}

/// A pillar anchor's square (centre, half size) when the pillar module is used.
pub fn pillar_squares(content: &Content) -> Vec<(V2, f32, f32)> {
    pillar_squares_with(content, &available)
}

fn pillar_squares_with(content: &Content, exists: &dyn Fn(&str) -> bool) -> Vec<(V2, f32, f32)> {
    let Some(m) = content
        .dungeon
        .modules
        .as_ref()
        .and_then(|m| m.pillar.as_ref())
    else {
        return Vec::new();
    };
    if !exists(&m.model) {
        return Vec::new();
    }
    content
        .dungeon
        .anchors
        .iter()
        .filter(|a| a.kind == "pillar")
        .map(|a| (a.pos, a.clearance, a.y))
        .collect()
}

/// Is this wall the side of a pillar the pillar module replaces?
pub fn wall_in_pillar(w: &Wall, squares: &[(V2, f32, f32)]) -> bool {
    let mid = (w.a + w.b) * 0.5;
    squares.iter().any(|(c, h, y)| {
        (mid.x - c.x).abs() <= h + 0.15
            && (mid.z - c.z).abs() <= h + 0.15
            && (w.bottom0 - y).abs() < 1.0
    })
}

/// Is `p` (at floor height `y`) deeper than `tol` inside walkable floor?
pub fn deep_in_floor(nav: &Nav, p: Vec3, tol: f32) -> bool {
    let q = V2::new(p.x, p.z);
    nav.surfaces.iter().any(|s| {
        s.def.shape.contains(q)
            && (s.height_at(q) - p.y).abs() < 1.0
            && s.def.shape.edges().all(|(a, b)| {
                let ab = b - a;
                let t = (((q.x - a.x) * ab.x + (q.z - a.z) * ab.z) / ab.len_sq().max(1e-6))
                    .clamp(0.0, 1.0);
                (a + ab * t).dist(q) > tol
            })
    })
}

/// Will the wall module draw this wall (instead of procedural slabs)?
pub fn wall_uses_module(content: &Content, w: &Wall) -> bool {
    let Some(m) = content
        .dungeon
        .modules
        .as_ref()
        .and_then(|m| m.wall.as_ref())
    else {
        return false;
    };
    available(&m.model) && wall_eligible(w)
}

fn wall_eligible(w: &Wall) -> bool {
    w.height >= MIN_WALL && (w.bottom0 - w.bottom1).abs() < 0.05 && w.a.dist(w.b) >= 0.3
}

/// Rotation whose local +x runs along `x`, +y up and +z along `z`
/// (horizontal unit vectors); `x` is flipped when needed to stay
/// right-handed (pieces are symmetric along x).
fn frame(x: Vec3, z: Vec3) -> Quat {
    // Right-handed: x × y = z.
    let x = if x.cross(Vec3::Y).dot(z) > 0.0 { x } else { -x };
    Quat::from_mat3(&Mat3::from_cols(x, Vec3::Y, z))
}

/// World bounds of the local box `[-sx/2, sx/2] x [0, sy] x [z0, z1]`.
fn bounds(t: &Transform, size: Vec3, z0: f32, z1: f32) -> (Vec3, Vec3) {
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for x in [-0.5, 0.5] {
        for y in [0.0, 1.0] {
            for z in [z0, z1] {
                let p = t.transform_point(Vec3::new(x * size.x, y * size.y, z));
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
    }
    (lo, hi)
}

fn pick_model<'a>(
    m: &'a ModuleConfig,
    at: Vec3,
    salt: u32,
    exists: &dyn Fn(&str) -> bool,
) -> &'a str {
    if !m.variants.is_empty() && hash01(at.x, at.z, salt) < m.variant_chance {
        let k = (hash01(at.z, at.x, salt + 1) * m.variants.len() as f32) as usize;
        let v = &m.variants[k.min(m.variants.len() - 1)];
        if exists(v) {
            return v;
        }
    }
    &m.model
}

#[allow(clippy::too_many_arguments)]
fn place(
    out: &mut ModuleInstances,
    kind: &'static str,
    model: &str,
    fallback: bool,
    t: Transform,
    size: Vec3,
    room: Option<usize>,
) {
    // Nominal model box scaled by the transform. Walls and doors start at
    // the wall line (trims reach 0.15 m in front); posts are centred.
    let unit = Transform {
        scale: Vec3::ONE,
        ..t
    };
    let s = size * t.scale;
    let (z0, z1) = match kind {
        "corner" | "pillar" => (-s.z * 0.5, s.z * 0.5),
        _ => (-0.15 * t.scale.z, s.z),
    };
    let (min, max) = bounds(&unit, s, z0, z1);
    out.0.push(ModuleInstance {
        kind,
        model: model.to_string(),
        fallback,
        transform: t,
        min,
        max,
        room,
    });
}

/// Spawn all modular pieces of `content` (see [`plan_modules`]). Walls that
/// get wall pieces are skipped by the procedural wall builder (see
/// [`wall_uses_module`]).
pub fn spawn_modules(
    commands: &mut Commands,
    content: &Content,
    nav: &Nav,
    structure: &Structure,
    scope: AppState,
    asset_server: &AssetServer,
) -> ModuleInstances {
    let out = plan_modules(content, nav, structure);
    for inst in out.0.iter().filter(|i| !i.fallback) {
        commands.spawn((
            WorldAssetRoot(
                asset_server
                    .load(bevy::gltf::GltfAssetLabel::Scene(0).from_asset(inst.model.clone())),
            ),
            inst.transform,
            DespawnOnExit(scope),
        ));
    }
    out
}

/// Where every modular piece of `content` goes (pure: no rendering).
pub fn plan_modules(content: &Content, nav: &Nav, structure: &Structure) -> ModuleInstances {
    plan_modules_with(content, nav, structure, &available)
}

/// [`plan_modules`] with an explicit "does this model file exist" check.
pub fn plan_modules_with(
    content: &Content,
    nav: &Nav,
    structure: &Structure,
    exists: &dyn Fn(&str) -> bool,
) -> ModuleInstances {
    let mut out = ModuleInstances::default();
    let Some(set) = &content.dungeon.modules else {
        return out;
    };
    let room_of = |p: Vec3| structure.room_at(nav, p + Vec3::Y * 0.5);
    let squares = pillar_squares_with(content, exists);

    // ---- Walls.
    if let Some(m) = &set.wall {
        let ok = exists(&m.model);
        if !ok {
            warn!(
                "module 'assets/{}' not found; walls are drawn procedurally",
                m.model
            );
        }
        let len0 = m.size[0];
        for (wi, w) in nav.walls.iter().enumerate() {
            if !wall_eligible(w) || wall_in_pillar(w, &squares) {
                continue;
            }
            let len = w.a.dist(w.b);
            let mut n = (len / len0).round().max(1.0) as usize;
            while n > 1 && len / n as f32 / len0 < 1.0 - MAX_STRETCH {
                n -= 1;
            }
            while len / (n as f32) / len0 > 1.0 + MAX_STRETCH {
                n += 1;
            }
            let dir = (w.b - w.a) * (1.0 / len);
            let x = Vec3::new(dir.x, 0.0, dir.z);
            let z = Vec3::new(w.out.x, 0.0, w.out.z);
            let rot = frame(x, z);
            let piece = len / n as f32;
            for k in 0..n {
                let mid = w.a + dir * (piece * (k as f32 + 0.5));
                let at = Vec3::new(mid.x, w.bottom0, mid.z);
                let t = Transform::from_translation(at)
                    .with_rotation(rot)
                    .with_scale(Vec3::new(piece / len0, w.height / m.size[1], 1.0));
                let model = pick_model(m, at, wi as u32 * 7 + k as u32, exists);
                place(
                    &mut out,
                    "wall",
                    model,
                    !ok,
                    t,
                    Vec3::from(m.size),
                    room_of(at - z * 1.0),
                );
            }
        }
    }

    // ---- Corners: a post where a wall turns or ends (not where navigation
    // merely split a straight wall into pieces).
    if let Some(m) = &set.corner {
        let ok = exists(&m.model);
        if ok {
            let walls: Vec<&Wall> = nav
                .walls
                .iter()
                .filter(|w| wall_eligible(w) && !wall_in_pillar(w, &squares))
                .collect();
            let straight_through = |at: Vec3| {
                let ends: Vec<&&Wall> = walls
                    .iter()
                    .filter(|w| {
                        (Vec3::new(w.a.x, w.bottom0, w.a.z).distance(at) < 0.3)
                            || (Vec3::new(w.b.x, w.bottom1, w.b.z).distance(at) < 0.3)
                    })
                    .collect();
                ends.len() == 2 && {
                    let d = |w: &Wall| (w.b - w.a).normalize_or_zero();
                    let (a, b) = (d(ends[0]), d(ends[1]));
                    (a.x * b.z - a.z * b.x).abs() < 0.05
                        && (ends[0].out.x * ends[1].out.x + ends[0].out.z * ends[1].out.z) > 0.9
                }
            };
            let mut seen: Vec<Vec3> = Vec::new();
            for w in walls.iter() {
                for (p, y) in [(w.a, w.bottom0), (w.b, w.bottom1)] {
                    let at = Vec3::new(p.x, y, p.z);
                    if seen.iter().any(|s| s.distance(at) < 0.3) {
                        continue;
                    }
                    seen.push(at);
                    // Not where a wall ends in an opening (the door frame
                    // stands there) or anywhere else on walkable floor.
                    if straight_through(at) || deep_in_floor(nav, at, 0.2) {
                        continue;
                    }
                    let dir = (w.b - w.a).normalize_or_zero();
                    let rot = frame(
                        Vec3::new(dir.x, 0.0, dir.z),
                        Vec3::new(w.out.x, 0.0, w.out.z),
                    );
                    let t = Transform::from_translation(at)
                        .with_rotation(rot)
                        .with_scale(Vec3::new(1.0, (w.height + 0.3) / m.size[1], 1.0));
                    place(
                        &mut out,
                        "corner",
                        &m.model,
                        false,
                        t,
                        Vec3::from(m.size),
                        room_of(at),
                    );
                }
            }
        } else {
            warn!("module 'assets/{}' not found; no corner posts", m.model);
        }
    }

    // ---- Door frames where a corridor opens into a room.
    if let Some(m) = &set.door {
        let ok = exists(&m.model);
        if ok {
            for d in &structure.doorways {
                let (ra, rb) = (&structure.rooms[d.rooms.0], &structure.rooms[d.rooms.1]);
                let (room, room_i) = match (ra.corridor, rb.corridor) {
                    (false, true) => (ra, d.rooms.0),
                    (true, false) => (rb, d.rooms.1),
                    _ => continue,
                };
                // The portal end at the room's wall: away from the room's middle.
                let c = room.center();
                let end1 = d.center + d.through * d.half_depth;
                let end2 = d.center - d.through * d.half_depth;
                let flat = |v: Vec3| Vec2::new(v.x, v.z);
                let (end, toward) = if flat(end1).distance(flat(c)) >= flat(end2).distance(flat(c))
                {
                    (end1, d.through)
                } else {
                    (end2, -d.through)
                };
                // Walk areas stop WALL_MARGIN short of the real outline.
                let mut at = end + toward * WALL_MARGIN;
                let mut width = d.width + 2.0 * WALL_MARGIN;
                // The real opening: the corridor's floor across the doorway,
                // just beyond the room's wall (a portal can cover only part
                // of a mouth that meets several floor strips).
                let corridor = if room_i == d.rooms.0 {
                    d.rooms.1
                } else {
                    d.rooms.0
                };
                let probe = at + toward * 0.3;
                let q = V2::new(probe.x, probe.z);
                let far = V2::new(d.along.x, d.along.z) * 25.0;
                for &si in &structure.rooms[corridor].surfaces {
                    let shape = &nav.surfaces[si].def.shape;
                    if !shape.contains(q) {
                        continue;
                    }
                    if let Some((t0, t1)) = shape.clip(q - far, q + far) {
                        let len = 50.0;
                        width = (t1 - t0) * len;
                        let mid = -25.0 + len * (t0 + t1) * 0.5;
                        at += d.along * mid;
                    }
                    break;
                }
                let t = Transform::from_translation(at)
                    .with_rotation(frame(d.along, toward))
                    .with_scale(Vec3::new(width / m.size[0], 1.0, 1.0));
                place(
                    &mut out,
                    "door",
                    &m.model,
                    false,
                    t,
                    Vec3::from(m.size),
                    Some(room_i),
                );
            }
        } else {
            warn!("module 'assets/{}' not found; no door frames", m.model);
        }
    }

    // ---- Pillars (already resolved: the model exists when squares exist).
    if let Some(m) = &set.pillar {
        for (c, h, y) in &squares {
            let at = Vec3::new(c.x, *y, c.z);
            let t = Transform::from_translation(at).with_scale(Vec3::new(
                2.0 * h / m.size[0],
                1.0,
                2.0 * h / m.size[2],
            ));
            place(
                &mut out,
                "pillar",
                &m.model,
                false,
                t,
                Vec3::from(m.size),
                room_of(at + Vec3::new(h + 1.0, 0.0, 0.0)),
            );
        }
        if squares.is_empty() && !exists(&m.model) {
            warn!(
                "module 'assets/{}' not found; pillars are drawn as walls",
                m.model
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_frame_is_a_rotation_mapping_x_and_z() {
        for (x, z) in [
            (Vec3::X, Vec3::Z),
            (Vec3::X, -Vec3::Z),
            (Vec3::Z, Vec3::X),
            (Vec3::new(0.6, 0.0, 0.8), Vec3::new(-0.8, 0.0, 0.6)),
            (Vec3::new(0.6, 0.0, 0.8), Vec3::new(0.8, 0.0, -0.6)),
        ] {
            let q = frame(x, z);
            assert!((q.length() - 1.0).abs() < 1e-4);
            // +z (away from the floor) always maps to `z`; x maps to ±x.
            assert!((q * Vec3::Z).distance(z) < 1e-4, "{x:?} {z:?}");
            assert!((q * Vec3::Y).distance(Vec3::Y) < 1e-4);
            assert!((q * Vec3::X).cross(x).length() < 1e-4);
        }
    }
}

#[cfg(test)]
mod placement_tests {
    //! The modular art must never misrepresent where you can walk: run over
    //! every shipped generated dungeon (all have `[dungeon.modules]`).
    use super::*;
    use dungeon_sim::nav::Nav;

    const FILES: &[&str] = &[
        "dungeons/07_emberforge_vaults.toml",
        "dungeons/08_hollow_reliquary.toml",
        "variants/forge/emberforge_vaults_s6.toml",
        "variants/forge/emberforge_vaults_s8.toml",
        "variants/forge/hollow_reliquary_s5.toml",
        "variants/forge/hollow_reliquary_s10.toml",
    ];

    fn load(rel: &str) -> (Content, Nav, Structure) {
        let path = format!("{}/../../assets/content/{rel}", env!("CARGO_MANIFEST_DIR"));
        let c = Content::load(&path).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let nav = c.nav();
        let st = Structure::build(&c, &nav);
        (c, nav, st)
    }

    fn walkable(nav: &Nav, p: Vec3) -> bool {
        let q = V2::new(p.x, p.z);
        nav.surfaces
            .iter()
            .any(|s| s.def.shape.contains(q) && (s.height_at(q) - p.y).abs() < 1.0)
    }

    #[test]
    fn modular_art_keeps_off_the_floor_and_out_of_doorways() {
        for rel in FILES {
            let (c, nav, st) = load(rel);
            let m = plan_modules_with(&c, &nav, &st, &|_| true);
            let set = c
                .dungeon
                .modules
                .as_ref()
                .expect("generated dungeons have modules");
            let kinds = |k: &str| m.0.iter().filter(|i| i.kind == k).count();
            assert!(kinds("wall") > 50, "{rel}: {} wall pieces", kinds("wall"));
            assert!(kinds("door") >= 5, "{rel}: {} door frames", kinds("door"));
            for inst in &m.0 {
                let t = inst.transform;
                let local = |x: f32, y: f32, z: f32| {
                    t.transform_point(Vec3::new(x, y, z) / t.scale.max(Vec3::splat(1e-6)) * t.scale)
                };
                match inst.kind {
                    "wall" => {
                        // Walls are navigation's own walls (they block line
                        // of sight): every piece's inner face lies on one,
                        // its body on the side away from that wall's floor.
                        let w = set.wall.as_ref().unwrap();
                        let o = t.translation;
                        let on_wall = nav.walls.iter().any(|nw| {
                            let ab = nw.b - nw.a;
                            let q = V2::new(o.x, o.z);
                            let k = (((q.x - nw.a.x) * ab.x + (q.z - nw.a.z) * ab.z)
                                / ab.len_sq().max(1e-6))
                            .clamp(0.0, 1.0);
                            (nw.a + ab * k).dist(q) < 0.05 && (nw.bottom0 - o.y).abs() < 0.05 && {
                                let back = t.transform_point(Vec3::new(0.0, 0.0, w.size[2]));
                                let d = V2::new(back.x - o.x, back.z - o.z);
                                d.x * nw.out.x + d.z * nw.out.z > 0.0
                            }
                        });
                        assert!(
                            on_wall,
                            "{rel}: wall piece at {o} is not on a navigation wall"
                        );
                    }
                    "door" => {
                        let d = set.door.as_ref().unwrap();
                        let half = d.size[0] / 2.0;
                        // The opening is walkable ...
                        for x in [-half + 0.6, 0.0, half - 0.6] {
                            let p = t.transform_point(Vec3::new(x, 0.0, 0.0));
                            assert!(walkable(&nav, p), "{rel}: door opening not walkable at {p}");
                        }
                        // ... and the posts beside it are not floor.
                        for x in [-half - 0.25, half + 0.25] {
                            for z in [-0.1, 0.4] {
                                let p = t.transform_point(Vec3::new(x, 0.5, z));
                                assert!(
                                    !deep_in_floor(&nav, p, 0.25),
                                    "{rel}: door post on the floor at {p} (room {:?}, scale {}, rot {:?}, at {})",
                                    inst.room.map(|r| &st.rooms[r].name),
                                    t.scale,
                                    t.rotation.to_euler(EulerRot::YXZ),
                                    t.translation
                                );
                            }
                        }
                    }
                    "pillar" => {
                        // Fills its hole: the centre is not floor, the hole's
                        // corners are covered by the scaled model.
                        let p = local(0.0, 0.0, 0.0);
                        assert!(!walkable(&nav, p), "{rel}: pillar over floor at {p}");
                        let size = inst.max - inst.min;
                        assert!(
                            size.x > 1.5 && size.z > 1.5 && size.y >= 6.9,
                            "{rel}: pillar {size}"
                        );
                    }
                    _ => {
                        let p = (inst.min + inst.max) * 0.5;
                        let p = Vec3::new(p.x, inst.min.y, p.z);
                        assert!(
                            !deep_in_floor(&nav, p, 0.3),
                            "{rel}: {} on the floor at {p}",
                            inst.kind
                        );
                    }
                }
            }
            // Every pillar hole is drawn as a pillar.
            let holes = c
                .dungeon
                .anchors
                .iter()
                .filter(|a| a.kind == "pillar")
                .count();
            assert_eq!(
                kinds("pillar"),
                if set.pillar.is_some() { holes } else { 0 },
                "{rel}"
            );
            // Wall-mounted decoration hangs above heads.
            for p in &c.dungeon.props {
                if p.model.contains("sconce") || p.model.contains("lantern") {
                    let (fp, _) = nav.project(p.pos, p.y - 2.6);
                    assert!(p.y >= fp.y + 2.2, "{rel}: {} at head height", p.model);
                }
            }
        }
    }

    #[test]
    fn missing_models_fall_back_and_old_dungeons_have_no_modules() {
        let (c, nav, st) = load("dungeons/08_hollow_reliquary.toml");
        let m = plan_modules_with(&c, &nav, &st, &|_| false);
        assert!(m.0.iter().filter(|i| i.kind == "wall").all(|i| i.fallback));
        // No model: no door frames, corners or pillar pieces (the procedural
        // walls are drawn instead, pillars included).
        assert!(m.0.iter().all(|i| i.kind == "wall"));
        let (c, nav, st) = load("dungeons/06_cinderdeep_hollows.toml");
        assert!(c.dungeon.modules.is_none());
        assert!(plan_modules(&c, &nav, &st).0.is_empty());
    }
}
