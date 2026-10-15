//! Layered 2.5D navigation (0.4). See NAVIGATION_0_4.md.
//!
//! The walkable world is a set of *surfaces*: convex axis-aligned footprints
//! with a height function (flat, or a linear ramp along x or z). Surfaces only
//! connect through validated *links*:
//!
//! * **walk** links are created automatically where two surfaces overlap *and*
//!   their heights agree within `max_step` (for a ramp, only the strip of the
//!   overlap where that holds becomes the portal). Overlap alone never connects.
//! * **drop** links are authored, one-way, from a zone on a higher surface's
//!   edge down to a lower surface.
//!
//! Routing runs over directed links (portals). All-pairs shortest paths are
//! precomputed when the level loads; a query only combines the start surface's
//! exits with the goal surface's entries, so it never allocates.
//!
//! Line of sight tests a segment against wall faces and against every
//! surface's solid volume (a slab for bridges, rock down to the lowest floor
//! otherwise).

use crate::math::{V2, V3};

/// Distance actors keep from the edges of a surface.
pub const WALL_MARGIN: f32 = 0.4;
/// Default wall height above a surface.
pub const WALL_HEIGHT: f32 = 3.2;
/// Thickness of an `open` surface (bridge deck).
pub const SLAB: f32 = 0.4;
/// A ramp may overlap the floor it joins by at most this much (its foot/head).
pub const RAMP_OVERLAP_MAX: f32 = 2.0;
/// Mid-route waypoints closer than this are skipped (callers treat anything
/// nearer than about 0.05 m as "arrived").
pub const WAYPOINT_SKIP: f32 = 0.1;
/// Extra route cost for taking a drop (prefer walking when similar).
pub const DROP_PENALTY: f32 = 2.0;

pub type SurfaceId = u16;
pub type LinkId = u16;
/// First and last link of a route.
type Hops = Option<(LinkId, LinkId)>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: V2,
    pub max: V2,
}

impl Rect {
    pub fn new(min: V2, max: V2) -> Self {
        Rect { min, max }
    }

    pub fn center(&self) -> V2 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> V2 {
        self.max - self.min
    }

    pub fn shrunk(&self, m: f32) -> Rect {
        Rect::new(
            V2::new(self.min.x + m, self.min.z + m),
            V2::new(self.max.x - m, self.max.z - m),
        )
    }

    pub fn is_valid(&self) -> bool {
        self.min.is_finite()
            && self.max.is_finite()
            && self.min.x < self.max.x
            && self.min.z < self.max.z
    }

    pub fn contains(&self, p: V2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.z >= self.min.z && p.z <= self.max.z
    }

    pub fn clamp(&self, p: V2) -> V2 {
        V2::new(
            p.x.clamp(self.min.x, self.max.x),
            p.z.clamp(self.min.z, self.max.z),
        )
    }

    pub fn intersection(&self, o: &Rect) -> Option<Rect> {
        let min = V2::new(self.min.x.max(o.min.x), self.min.z.max(o.min.z));
        let max = V2::new(self.max.x.min(o.max.x), self.max.z.min(o.max.z));
        (min.x <= max.x && min.z <= max.z).then_some(Rect::new(min, max))
    }

    fn corners(&self) -> [V2; 4] {
        [
            self.min,
            V2::new(self.max.x, self.min.z),
            self.max,
            V2::new(self.min.x, self.max.z),
        ]
    }

    /// Parameter interval `[t0, t1]` (within 0..1) where segment `a`→`b` is
    /// inside the rectangle (Liang–Barsky clipping).
    pub fn clip(&self, a: V2, b: V2) -> Option<(f32, f32)> {
        let d = b - a;
        let (mut t0, mut t1) = (0.0f32, 1.0f32);
        for (p, q) in [
            (-d.x, a.x - self.min.x),
            (d.x, self.max.x - a.x),
            (-d.z, a.z - self.min.z),
            (d.z, self.max.z - a.z),
        ] {
            if p.abs() < 1e-9 {
                if q < 0.0 {
                    return None;
                }
            } else {
                let r = q / p;
                if p < 0.0 {
                    t0 = t0.max(r);
                } else {
                    t1 = t1.min(r);
                }
            }
        }
        (t0 <= t1).then_some((t0, t1))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Axis {
    X,
    #[default]
    Z,
}

/// A walkable surface as authored.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceDef {
    pub name: String,
    pub rect: Rect,
    /// Height (at the `min` edge for a ramp).
    pub y: f32,
    /// Ramp: height at the `max` edge along `axis`.
    pub y_to: Option<f32>,
    pub axis: Axis,
    /// Draw a ramp as steps (navigation is identical).
    pub stairs: bool,
    pub walls: bool,
    pub wall_height: f32,
    /// Thin deck you can walk and see under (bridges); otherwise solid rock
    /// down to the lowest floor.
    pub open_under: bool,
    /// Intentionally unreachable from the entrance (not a validation error).
    pub unreachable: bool,
}

impl SurfaceDef {
    pub fn flat(name: &str, min: V2, max: V2, y: f32) -> SurfaceDef {
        SurfaceDef {
            name: name.into(),
            rect: Rect::new(min, max),
            y,
            y_to: None,
            axis: Axis::Z,
            stairs: false,
            walls: true,
            wall_height: WALL_HEIGHT,
            open_under: false,
            unreachable: false,
        }
    }

    pub fn ramp(name: &str, min: V2, max: V2, axis: Axis, y: f32, y_to: f32) -> SurfaceDef {
        SurfaceDef {
            y_to: Some(y_to),
            axis,
            ..SurfaceDef::flat(name, min, max, y)
        }
    }

    pub fn open(mut self) -> SurfaceDef {
        self.open_under = true;
        self.walls = false;
        self
    }
}

/// A surface ready for queries.
#[derive(Clone, Debug)]
pub struct Surface {
    pub def: SurfaceDef,
    /// Footprint shrunk by the wall margin (where actor centres may be).
    pub walk: Rect,
    /// Height plane: `c0 + cx·x + cz·z` inside the footprint.
    c0: f32,
    cx: f32,
    cz: f32,
}

impl Surface {
    fn new(def: SurfaceDef) -> Surface {
        let (mut c0, mut cx, mut cz) = (def.y, 0.0, 0.0);
        if let Some(to) = def.y_to {
            let r = def.rect;
            match def.axis {
                Axis::Z => {
                    let k = (to - def.y) / (r.max.z - r.min.z);
                    cz = k;
                    c0 = def.y - k * r.min.z;
                }
                Axis::X => {
                    let k = (to - def.y) / (r.max.x - r.min.x);
                    cx = k;
                    c0 = def.y - k * r.min.x;
                }
            }
        }
        Surface {
            walk: def.rect.shrunk(WALL_MARGIN),
            def,
            c0,
            cx,
            cz,
        }
    }

    pub fn name(&self) -> &str {
        &self.def.name
    }

    /// Height at horizontal point `p` (clamped to the footprint).
    pub fn height_at(&self, p: V2) -> f32 {
        let p = self.def.rect.clamp(p);
        self.c0 + self.cx * p.x + self.cz * p.z
    }

    pub fn is_ramp(&self) -> bool {
        self.def.y_to.is_some()
    }

    pub fn min_y(&self) -> f32 {
        self.def.y.min(self.def.y_to.unwrap_or(self.def.y))
    }

    pub fn max_y(&self) -> f32 {
        self.def.y.max(self.def.y_to.unwrap_or(self.def.y))
    }

    /// Rise over run.
    pub fn slope(&self) -> f32 {
        (self.cx * self.cx + self.cz * self.cz).sqrt()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    /// Bidirectional (created as two directed links) where heights agree.
    Walk,
    /// One-way: step off an edge and fall.
    Drop,
}

/// An authored link (drops; walk links may be declared to assert they exist).
#[derive(Clone, Debug, PartialEq)]
pub struct LinkDef {
    pub from: String,
    pub to: String,
    pub kind: LinkKind,
    /// Drop zone: straddles the upper surface's edge and the landing area.
    pub zone: Rect,
}

/// A directed link between two surfaces.
#[derive(Clone, Debug)]
pub struct Link {
    pub from: SurfaceId,
    pub to: SurfaceId,
    pub kind: LinkKind,
    /// Walk: the region inside both surfaces. Drop: the take-off area on `from`.
    pub portal: Rect,
    /// Drop: the authored zone (empty rect for walk links).
    pub zone: Rect,
    /// Where the traversal starts (on `from`) and ends (on `to`).
    pub entry: V3,
    pub exit: V3,
    pub cost: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavParams {
    /// Largest height difference crossed by walking.
    pub max_step: f32,
    /// Steepest walkable ramp (rise over run).
    pub max_slope: f32,
    /// Highest allowed drop.
    pub max_drop: f32,
    /// Clearance needed to walk under a surface.
    pub headroom: f32,
}

impl Default for NavParams {
    fn default() -> Self {
        NavParams {
            max_step: 0.5,
            max_slope: 0.6,
            max_drop: 8.0,
            headroom: 2.5,
        }
    }
}

/// A wall face along a surface edge, from the surface (`bottom0`..`bottom1`
/// along the segment) up `height` metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wall {
    pub a: V2,
    pub b: V2,
    pub bottom0: f32,
    pub bottom1: f32,
    pub height: f32,
    pub surface: SurfaceId,
}

/// Result of a steering query.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Waypoint {
    /// Horizontal point to steer toward.
    pub point: V2,
    /// The link being traversed, if any.
    pub link: Option<LinkId>,
    /// False when the goal cannot be reached from here (point = current position).
    pub reachable: bool,
}

#[derive(Clone, Debug)]
pub struct Nav {
    pub surfaces: Vec<Surface>,
    pub links: Vec<Link>,
    pub walls: Vec<Wall>,
    pub params: NavParams,
    /// Bottom of solid rock (lowest floor minus a slab).
    pub base_y: f32,
    out: Vec<Vec<LinkId>>,
    inc: Vec<Vec<LinkId>>,
    /// Cost from the *end* of link i to the end of link j (row-major).
    dist: Vec<f32>,
    /// Next link after i on the best route to j.
    next: Vec<LinkId>,
    /// Surface-to-surface reachability.
    reach: Vec<bool>,
}

/// Linear height difference `d(p) = a(p) - b(p)` as (c0, cx, cz).
fn height_diff(a: &Surface, b: &Surface) -> (f32, f32, f32) {
    (a.c0 - b.c0, a.cx - b.cx, a.cz - b.cz)
}

/// Sub-rectangle of `r` where `|c0 + cx·x + cz·z| <= tol`.
fn level_region(r: Rect, (c0, cx, cz): (f32, f32, f32), tol: f32) -> Option<Rect> {
    let eps = 1e-6;
    let solve = |c: f32, k: f32, lo: f32, hi: f32| -> Option<(f32, f32)> {
        let (mut a, mut b) = ((-tol - c) / k, (tol - c) / k);
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        let (a, b) = (a.max(lo), b.min(hi));
        (a <= b).then_some((a, b))
    };
    if cx.abs() < eps && cz.abs() < eps {
        return (c0.abs() <= tol).then_some(r);
    }
    if cx.abs() < eps {
        let (z0, z1) = solve(c0, cz, r.min.z, r.max.z)?;
        return Some(Rect::new(V2::new(r.min.x, z0), V2::new(r.max.x, z1)));
    }
    if cz.abs() < eps {
        let (x0, x1) = solve(c0, cx, r.min.x, r.max.x)?;
        return Some(Rect::new(V2::new(x0, r.min.z), V2::new(x1, r.max.z)));
    }
    // Two slopes in different directions: only accept a fully level overlap.
    r.corners()
        .iter()
        .all(|p| (c0 + cx * p.x + cz * p.z).abs() <= tol)
        .then_some(r)
}

/// Does segment `a`→`b` touch rectangle `r`?
fn segment_touches(r: &Rect, a: V2, b: V2) -> bool {
    r.clip(a, b).is_some()
}

impl Nav {
    /// Flat legacy layout: every rectangle at height 0 (how 0.3 maps load).
    pub fn flat(rects: &[Rect]) -> Nav {
        let defs = rects
            .iter()
            .enumerate()
            .map(|(i, r)| SurfaceDef::flat(&format!("area{i}"), r.min, r.max, 0.0))
            .collect();
        Nav::build(defs, Vec::new(), NavParams::default()).expect("flat layouts are valid")
    }

    /// Validate and preprocess a level. Errors are complete, human-readable
    /// sentences naming the areas involved.
    pub fn build(
        defs: Vec<SurfaceDef>,
        link_defs: Vec<LinkDef>,
        params: NavParams,
    ) -> Result<Nav, Vec<String>> {
        let mut errors = Vec::new();
        if defs.is_empty() {
            errors.push("there must be at least one walkable area".into());
            return Err(errors);
        }
        if defs.len() > u16::MAX as usize / 2 {
            errors.push("too many walkable areas".into());
            return Err(errors);
        }
        for (i, d) in defs.iter().enumerate() {
            let what = format!("area '{}'", d.name);
            if !d.rect.is_valid() {
                errors.push(format!(
                    "{what}: min {:?} must be smaller than max {:?} on both axes",
                    <[f32; 2]>::from(d.rect.min),
                    <[f32; 2]>::from(d.rect.max)
                ));
                continue;
            }
            let s = d.rect.size();
            if s.x < 2.0 * WALL_MARGIN + 0.2 || s.z < 2.0 * WALL_MARGIN + 0.2 {
                errors.push(format!(
                    "{what} is too narrow ({:.1} x {:.1} m); areas must be at least 1.0 m wide",
                    s.x, s.z
                ));
            }
            if !d.y.is_finite()
                || d.y.abs() > 500.0
                || d.y_to.is_some_and(|t| !t.is_finite() || t.abs() > 500.0)
            {
                errors.push(format!("{what}: heights must be finite and within ±500 m"));
            }
            if !d.wall_height.is_finite() || !(0.0..=50.0).contains(&d.wall_height) {
                errors.push(format!("{what}: wall_height must be between 0 and 50"));
            }
            if defs[..i].iter().any(|o| o.name == d.name) {
                errors.push(format!(
                    "{what}: the name is used more than once; area names must be unique"
                ));
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        let surfaces: Vec<Surface> = defs.into_iter().map(Surface::new).collect();
        for s in &surfaces {
            if s.slope() > params.max_slope + 1e-4 {
                errors.push(format!(
                    "area '{}' is a ramp with slope {:.2} (rise/run); the maximum walkable slope is {:.2} - make it longer or lower",
                    s.name(),
                    s.slope(),
                    params.max_slope
                ));
            }
        }

        // Automatic walk links and overlap validation.
        let mut links: Vec<Link> = Vec::new();
        for i in 0..surfaces.len() {
            for j in (i + 1)..surfaces.len() {
                let (a, b) = (&surfaces[i], &surfaces[j]);
                let diff = height_diff(a, b);
                let portal = a
                    .walk
                    .intersection(&b.walk)
                    .and_then(|ov| level_region(ov, diff, params.max_step))
                    .filter(|p| p.size().x >= 0.02 && p.size().z >= 0.02);
                if let Some(raw) = a.def.rect.intersection(&b.def.rect)
                    && raw.size().x > 0.01
                    && raw.size().z > 0.01
                {
                    let d: Vec<f32> = raw
                        .corners()
                        .iter()
                        .map(|p| diff.0 + diff.1 * p.x + diff.2 * p.z)
                        .collect();
                    let all_level = d.iter().all(|v| v.abs() <= params.max_step);
                    let a_above = d.iter().all(|&v| v >= params.headroom);
                    let b_above = d.iter().all(|&v| -v >= params.headroom);
                    if all_level {
                        // Same floor level.
                    } else if a_above || b_above {
                        let (upper, lower) = if a_above { (a, b) } else { (b, a) };
                        if !upper.def.open_under {
                            errors.push(format!(
                                "area '{}' lies above area '{}' but is solid underneath; set under = \"open\" on '{}' (a bridge or walkway) or move the areas apart",
                                upper.name(),
                                lower.name(),
                                upper.name()
                            ));
                        }
                    } else if portal.is_some() {
                        // A ramp's foot or head may overlap the floor it joins a little.
                        let extent = if diff.1.abs() > diff.2.abs() {
                            raw.size().x
                        } else {
                            raw.size().z
                        };
                        if extent > RAMP_OVERLAP_MAX + 1e-3 {
                            errors.push(format!(
                                "areas '{}' and '{}' overlap over {:.1} m but are only level with each other over part of it; a ramp may overlap the floor it joins by at most {RAMP_OVERLAP_MAX} m - shorten one of them",
                                a.name(),
                                b.name(),
                                extent
                            ));
                        }
                    } else {
                        let lo = d.iter().fold(f32::INFINITY, |m, v| m.min(v.abs()));
                        let hi = d.iter().fold(0.0f32, |m, v| m.max(v.abs()));
                        errors.push(format!(
                            "areas '{}' and '{}' overlap but are {lo:.1}-{hi:.1} m apart vertically: too far to step ({} m) and too close to walk under ({} m); move them apart or make them level",
                            a.name(),
                            b.name(),
                            params.max_step,
                            params.headroom
                        ));
                    }
                }
                if let Some(p) = portal {
                    let c = p.center();
                    for (from, to) in [(i, j), (j, i)] {
                        let (sf, st) = (&surfaces[from], &surfaces[to]);
                        links.push(Link {
                            from: from as SurfaceId,
                            to: to as SurfaceId,
                            kind: LinkKind::Walk,
                            portal: p,
                            zone: p,
                            entry: V3::at(c, sf.height_at(c)),
                            exit: V3::at(c, st.height_at(c)),
                            cost: 0.0,
                        });
                    }
                }
            }
        }

        // Authored links.
        let id_of = |name: &str| surfaces.iter().position(|s| s.def.name == name);
        for (k, l) in link_defs.iter().enumerate() {
            let what = format!("links[{k}] ('{}' -> '{}')", l.from, l.to);
            let (Some(f), Some(t)) = (id_of(&l.from), id_of(&l.to)) else {
                let known: Vec<&str> = surfaces.iter().map(|s| s.name()).collect();
                errors.push(format!(
                    "{what}: unknown area name; known areas: {}",
                    known.join(", ")
                ));
                continue;
            };
            match l.kind {
                LinkKind::Walk => {
                    if !links.iter().any(|x| {
                        x.kind == LinkKind::Walk && x.from as usize == f && x.to as usize == t
                    }) {
                        errors.push(format!(
                            "{what}: declared as a walk link but the areas don't share a level overlap; they must overlap by at least {:.1} m where their heights agree within {} m",
                            2.0 * WALL_MARGIN,
                            params.max_step
                        ));
                    }
                }
                LinkKind::Drop => {
                    if !l.zone.is_valid() {
                        errors.push(format!(
                            "{what}: zone min must be smaller than max on both axes"
                        ));
                        continue;
                    }
                    let (sf, st) = (&surfaces[f], &surfaces[t]);
                    let Some(takeoff) = l.zone.intersection(&sf.walk) else {
                        errors.push(format!(
                            "{what}: the drop zone does not cover any walkable part of '{}'; it must straddle that area's edge",
                            l.from
                        ));
                        continue;
                    };
                    let Some(landing) = l.zone.intersection(&st.walk) else {
                        errors.push(format!(
                            "{what}: the drop zone does not reach any walkable part of '{}' (the landing area)",
                            l.to
                        ));
                        continue;
                    };
                    let entry = V3::at(takeoff.center(), sf.height_at(takeoff.center()));
                    let exit_p = landing.center();
                    let exit = V3::at(exit_p, st.height_at(exit_p));
                    let fall = entry.y - exit.y;
                    if fall <= params.max_step || fall > params.max_drop {
                        errors.push(format!(
                            "{what}: drop height is {fall:.1} m; it must be more than {} m (otherwise it is a step) and at most {} m",
                            params.max_step, params.max_drop
                        ));
                        continue;
                    }
                    // The gap between the edges must be small enough to step across.
                    let gap = takeoff.clamp(exit_p).dist(landing.clamp(takeoff.center()));
                    if gap > 3.0 {
                        errors.push(format!(
                            "{what}: the landing area is {gap:.1} m away from the edge; a drop zone must cover the edge of '{}' and the floor right next to or below it",
                            l.from
                        ));
                        continue;
                    }
                    links.push(Link {
                        from: f as SurfaceId,
                        to: t as SurfaceId,
                        kind: LinkKind::Drop,
                        portal: takeoff,
                        zone: l.zone,
                        entry,
                        exit,
                        cost: entry.xz().dist(exit.xz()) + fall * 0.5 + DROP_PENALTY,
                    });
                }
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        let base_y = surfaces
            .iter()
            .map(|s| s.min_y())
            .fold(f32::INFINITY, f32::min)
            - 0.5;
        let mut nav = Nav {
            surfaces,
            links,
            walls: Vec::new(),
            params,
            base_y,
            out: Vec::new(),
            inc: Vec::new(),
            dist: Vec::new(),
            next: Vec::new(),
            reach: Vec::new(),
        };
        nav.precompute();
        nav.walls = nav.build_walls();
        Ok(nav)
    }

    fn precompute(&mut self) {
        let ns = self.surfaces.len();
        let n = self.links.len();
        self.out = vec![Vec::new(); ns];
        self.inc = vec![Vec::new(); ns];
        for (k, l) in self.links.iter().enumerate() {
            self.out[l.from as usize].push(k as LinkId);
            self.inc[l.to as usize].push(k as LinkId);
        }
        let inf = f32::INFINITY;
        let mut dist = vec![inf; n * n];
        let mut next = vec![LinkId::MAX; n * n];
        for i in 0..n {
            dist[i * n + i] = 0.0;
            next[i * n + i] = i as LinkId;
            let li = &self.links[i];
            for &j in &self.out[li.to as usize] {
                let j = j as usize;
                if j == i {
                    continue;
                }
                let lj = &self.links[j];
                // Don't immediately walk back through the same portal.
                if lj.kind == LinkKind::Walk
                    && li.kind == LinkKind::Walk
                    && lj.to == li.from
                    && lj.portal == li.portal
                {
                    continue;
                }
                let c = li.exit.dist(lj.entry) + lj.cost;
                if c < dist[i * n + j] {
                    dist[i * n + j] = c;
                    next[i * n + j] = j as LinkId;
                }
            }
        }
        for k in 0..n {
            for i in 0..n {
                let ik = dist[i * n + k];
                if ik == inf {
                    continue;
                }
                for j in 0..n {
                    let c = ik + dist[k * n + j];
                    if c < dist[i * n + j] - 1e-5 {
                        dist[i * n + j] = c;
                        next[i * n + j] = next[i * n + k];
                    }
                }
            }
        }
        self.dist = dist;
        self.next = next;
        let mut reach = vec![false; ns * ns];
        for s in 0..ns {
            reach[s * ns + s] = true;
            for g in 0..ns {
                if s == g {
                    continue;
                }
                reach[s * ns + g] = self.out[s].iter().any(|&i| {
                    self.inc[g]
                        .iter()
                        .any(|&j| self.dist[i as usize * n + j as usize] < inf)
                });
            }
        }
        self.reach = reach;
    }

    /// Wall faces: each walled surface's outline minus the parts shared with a
    /// surface it is connected to, and minus drop zones leaving it.
    fn build_walls(&self) -> Vec<Wall> {
        let mut out = Vec::new();
        for (i, s) in self.surfaces.iter().enumerate() {
            if !s.def.walls {
                continue;
            }
            let r = s.def.rect;
            let linked: Vec<&Rect> = self
                .links
                .iter()
                .filter(|l| l.kind == LinkKind::Walk && l.from as usize == i)
                .map(|l| &self.surfaces[l.to as usize].def.rect)
                .collect();
            let drops: Vec<&Rect> = self
                .links
                .iter()
                .filter(|l| l.kind == LinkKind::Drop && l.from as usize == i)
                .map(|l| &l.zone)
                .collect();
            let mut push = |a: V2, b: V2| {
                out.push(Wall {
                    a,
                    b,
                    bottom0: s.height_at(a),
                    bottom1: s.height_at(b),
                    height: s.def.wall_height,
                    surface: i as SurfaceId,
                });
            };
            for &z in &[r.min.z, r.max.z] {
                let cuts: Vec<(f32, f32)> = linked
                    .iter()
                    .filter(|o| o.min.z < z && z < o.max.z)
                    .map(|o| (o.min.x, o.max.x))
                    .chain(
                        drops
                            .iter()
                            .filter(|o| o.min.z <= z && z <= o.max.z)
                            .map(|o| (o.min.x, o.max.x)),
                    )
                    .collect();
                for (a, b) in subtract_intervals((r.min.x, r.max.x), &cuts) {
                    push(V2::new(a, z), V2::new(b, z));
                }
            }
            for &x in &[r.min.x, r.max.x] {
                let cuts: Vec<(f32, f32)> = linked
                    .iter()
                    .filter(|o| o.min.x < x && x < o.max.x)
                    .map(|o| (o.min.z, o.max.z))
                    .chain(
                        drops
                            .iter()
                            .filter(|o| o.min.x <= x && x <= o.max.x)
                            .map(|o| (o.min.z, o.max.z)),
                    )
                    .collect();
                for (a, b) in subtract_intervals((r.min.z, r.max.z), &cuts) {
                    push(V2::new(x, a), V2::new(x, b));
                }
            }
        }
        out
    }

    pub fn surface(&self, id: SurfaceId) -> &Surface {
        &self.surfaces[id as usize]
    }

    pub fn find(&self, name: &str) -> Option<SurfaceId> {
        self.surfaces
            .iter()
            .position(|s| s.def.name == name)
            .map(|i| i as SurfaceId)
    }

    pub fn height(&self, s: SurfaceId, p: V2) -> f32 {
        self.surfaces[s as usize].height_at(p)
    }

    /// Can surface `g` be reached from surface `s` by walking and dropping?
    pub fn reachable(&self, s: SurfaceId, g: SurfaceId) -> bool {
        let ns = self.surfaces.len();
        self.reach[s as usize * ns + g as usize]
    }

    /// The surface under horizontal point `p` whose height is closest to `y`.
    pub fn locate(&self, p: V2, y: f32) -> Option<SurfaceId> {
        let mut best: Option<(f32, usize)> = None;
        for (i, s) in self.surfaces.iter().enumerate() {
            if s.walk.contains(p) {
                let d = (s.height_at(p) - y).abs();
                if best.is_none_or(|(bd, _)| d < bd - 1e-4) {
                    best = Some((d, i));
                }
            }
        }
        best.map(|(_, i)| i as SurfaceId)
    }

    /// Nearest walkable point to `p` at about elevation `y`: horizontal distance
    /// plus a heavy penalty on height difference, so a point beside a bridge snaps
    /// onto the bridge rather than onto the floor far below.
    pub fn project(&self, p: V2, y: f32) -> (V3, SurfaceId) {
        let mut best = (f32::INFINITY, V3::at(p, y), 0usize);
        for (i, s) in self.surfaces.iter().enumerate() {
            let q = s.walk.clamp(p);
            let h = s.height_at(q);
            let score = q.dist(p) + 4.0 * (h - y).abs();
            if score < best.0 - 1e-5 {
                best = (score, V3::at(q, h), i);
            }
        }
        (best.1, best.2 as SurfaceId)
    }

    /// Best route cost from `from` (on `s`) to `goal` (on `g`) and its first link.
    fn route(&self, s: SurfaceId, from: V3, g: SurfaceId, goal: V3) -> Option<(f32, Hops)> {
        if s == g {
            return Some((from.dist(goal), None));
        }
        let n = self.links.len();
        let mut best: Option<(f32, LinkId, LinkId)> = None;
        for &i in &self.out[s as usize] {
            let li = &self.links[i as usize];
            let head = from.dist(li.entry) + li.cost;
            for &j in &self.inc[g as usize] {
                let mid = self.dist[i as usize * n + j as usize];
                if !mid.is_finite() {
                    continue;
                }
                let c = head + mid + self.links[j as usize].exit.dist(goal);
                if best.is_none_or(|(bc, _, _)| c < bc - 1e-5) {
                    best = Some((c, i, j));
                }
            }
        }
        best.map(|(c, i, j)| (c, Some((i, j))))
    }

    /// Where to steer next from `from` (standing on `s`) toward `goal`
    /// (resolved onto the surface nearest its elevation).
    pub fn next_waypoint(&self, from: V2, s: SurfaceId, goal: V3) -> Waypoint {
        let (gp, g) = self.project(goal.xz(), goal.y);
        // Standing in a portal counts as standing on either side of it; the far
        // side wins ties so an actor inside a portal keeps moving through it.
        let mut best: Option<(f32, SurfaceId, Hops)> = None;
        let mut consider = |sid: SurfaceId| {
            let from3 = V3::at(from, self.height(sid, from));
            if let Some((c, first)) = self.route(sid, from3, g, gp)
                && best.is_none_or(|(bc, _, _)| c < bc - 1e-3)
            {
                best = Some((c, sid, first));
            }
        };
        for &k in &self.out[s as usize] {
            let l = &self.links[k as usize];
            if l.kind == LinkKind::Walk && l.portal.contains(from) {
                consider(l.to);
            }
        }
        consider(s);
        let Some((_, _, first)) = best else {
            return Waypoint {
                point: from,
                link: None,
                reachable: false,
            };
        };
        let Some((i, j)) = first else {
            return Waypoint {
                point: gp.xz(),
                link: None,
                reachable: true,
            };
        };
        let li = &self.links[i as usize];
        // The point after this link: the goal, or the next link's entry.
        let after = if i == j {
            gp.xz()
        } else {
            let n = self.links.len();
            let k = self.next[i as usize * n + j as usize];
            self.links[k as usize].entry.xz()
        };
        let point = match li.kind {
            LinkKind::Walk => {
                // Aim at the part of the portal nearest to where we go next,
                // keeping a little clearance from its sides. Already there:
                // keep going (never return the current position mid-route).
                let m = (li.portal.size().x.min(li.portal.size().z) * 0.5).min(0.3);
                let aim = li.portal.shrunk(m).clamp(after);
                if aim.dist(from) < WAYPOINT_SKIP {
                    after
                } else {
                    aim
                }
            }
            LinkKind::Drop => {
                let edge = li.portal.clamp(li.exit.xz());
                if li.portal.contains(from) || edge.dist(from) < WAYPOINT_SKIP {
                    // At the edge: step off toward the landing spot.
                    li.exit.xz()
                } else {
                    edge
                }
            }
        };
        Waypoint {
            point,
            link: Some(i),
            reachable: true,
        }
    }

    /// Full route as points (debug drawing and tests; allocates).
    pub fn path_points(&self, from: V2, s: SurfaceId, goal: V3) -> Option<Vec<V3>> {
        let (gp, g) = self.project(goal.xz(), goal.y);
        let from3 = V3::at(from, self.height(s, from));
        let (_, first) = self.route(s, from3, g, gp)?;
        let mut pts = vec![from3];
        if let Some((mut i, j)) = first {
            let n = self.links.len();
            loop {
                let l = &self.links[i as usize];
                pts.push(l.entry);
                if l.kind == LinkKind::Drop {
                    pts.push(l.exit);
                }
                if i == j {
                    break;
                }
                i = self.next[i as usize * n + j as usize];
            }
        }
        pts.push(gp);
        Some(pts)
    }

    /// Move from `from` on surface `s` by `delta`. Leaves the surface only
    /// through a link; otherwise slides along the edge. Returns the new
    /// position, surface and whether the actor stepped off a drop.
    pub fn try_move(&self, from: V2, s: SurfaceId, delta: V2) -> (V2, SurfaceId, bool) {
        let target = from + delta;
        for t in [
            target,
            V2::new(from.x + delta.x, from.z),
            V2::new(from.x, from.z + delta.z),
        ] {
            if let Some(r) = self.attempt(from, s, t) {
                return r;
            }
        }
        let surf = &self.surfaces[s as usize];
        if surf.walk.contains(from) {
            (from, s, false)
        } else {
            (surf.walk.clamp(from), s, false)
        }
    }

    fn attempt(&self, from: V2, s: SurfaceId, target: V2) -> Option<(V2, SurfaceId, bool)> {
        if target == from {
            return None;
        }
        if self.surfaces[s as usize].walk.contains(target) {
            return Some((target, s, false));
        }
        for &k in &self.out[s as usize] {
            let l = &self.links[k as usize];
            let to = &self.surfaces[l.to as usize];
            match l.kind {
                LinkKind::Walk => {
                    if to.walk.contains(target) && segment_touches(&l.portal, from, target) {
                        return Some((target, l.to, false));
                    }
                }
                LinkKind::Drop => {
                    if l.portal.contains(from) && l.zone.contains(target) {
                        let land = to.walk.clamp(target);
                        if land.dist(target) <= 2.0 * WALL_MARGIN + 0.3 && l.zone.contains(land) {
                            return Some((land, l.to, true));
                        }
                    }
                }
            }
        }
        None
    }

    /// Is the straight segment between two world points free of walls and
    /// solid surface volumes?
    pub fn line_of_sight(&self, a: V3, b: V3) -> bool {
        let (a2, b2) = (a.xz(), b.xz());
        // Cheap reject: only geometry whose box overlaps the segment's box.
        let (lo_x, hi_x) = (a2.x.min(b2.x), a2.x.max(b2.x));
        let (lo_z, hi_z) = (a2.z.min(b2.z), a2.z.max(b2.z));
        let outside =
            |min: V2, max: V2| max.x < lo_x || min.x > hi_x || max.z < lo_z || min.z > hi_z;
        for w in &self.walls {
            if outside(
                V2::new(w.a.x.min(w.b.x), w.a.z.min(w.b.z)),
                V2::new(w.a.x.max(w.b.x), w.a.z.max(w.b.z)),
            ) {
                continue;
            }
            if let Some((t, u)) = seg_intersect(a2, b2, w.a, w.b) {
                let y = a.y + (b.y - a.y) * t;
                let bottom = w.bottom0 + (w.bottom1 - w.bottom0) * u;
                if y >= bottom - 0.01 && y <= bottom + w.height {
                    return false;
                }
            }
        }
        for s in &self.surfaces {
            // Solid volumes entirely below both ends can't be in the way.
            if s.max_y() < a.y.min(b.y) - 0.05 || outside(s.def.rect.min, s.def.rect.max) {
                continue;
            }
            let Some((t0, t1)) = s.def.rect.clip(a2, b2) else {
                continue;
            };
            if t1 - t0 < 1e-5 {
                continue;
            }
            // Above the top: y_seg(t) - top(t) > 0; above the base: y_seg(t) - base > 0.
            let top = |t: f32| {
                let p = a2 + (b2 - a2) * t;
                s.c0 + s.cx * p.x + s.cz * p.z
            };
            let yseg = |t: f32| a.y + (b.y - a.y) * t;
            let base = if s.def.open_under {
                s.min_y() - SLAB
            } else {
                self.base_y
            };
            // f(t) = top - y (inside when > 0), g(t) = y - base (inside when > 0).
            let (f0, f1) = (top(t0) - yseg(t0), top(t1) - yseg(t1));
            let (g0, g1) = (yseg(t0) - base, yseg(t1) - base);
            if interval_positive_overlap(t0, t1, f0, f1, g0, g1) {
                return false;
            }
        }
        true
    }
}

/// For linear f and g on [t0, t1] (given by endpoint values), is there a t
/// where both are > a small epsilon?
fn interval_positive_overlap(t0: f32, t1: f32, f0: f32, f1: f32, g0: f32, g1: f32) -> bool {
    const EPS: f32 = 0.02;
    let pos = |v0: f32, v1: f32| -> Option<(f32, f32)> {
        // Sub-interval (in 0..1 of the parameter range) where v > EPS.
        let (a, b) = (v0 - EPS, v1 - EPS);
        match (a > 0.0, b > 0.0) {
            (true, true) => Some((0.0, 1.0)),
            (false, false) => None,
            (true, false) => Some((0.0, a / (a - b))),
            (false, true) => Some((a / (a - b), 1.0)),
        }
    };
    let (Some(fr), Some(gr)) = (pos(f0, f1), pos(g0, g1)) else {
        return false;
    };
    let lo = fr.0.max(gr.0);
    let hi = fr.1.min(gr.1);
    (hi - lo) * (t1 - t0) > 1e-4
}

/// Proper intersection of segments p→q and r→s: parameters (t on p→q, u on r→s).
fn seg_intersect(p: V2, q: V2, r: V2, s: V2) -> Option<(f32, f32)> {
    let d1 = q - p;
    let d2 = s - r;
    let den = d1.x * d2.z - d1.z * d2.x;
    if den.abs() < 1e-9 {
        return None;
    }
    let w = r - p;
    let t = (w.x * d2.z - w.z * d2.x) / den;
    let u = (w.x * d1.z - w.z * d1.x) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some((t, u))
}

/// `span` minus the union of `cuts`, dropping slivers shorter than 1cm.
fn subtract_intervals(span: (f32, f32), cuts: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut pieces = vec![span];
    for &(c0, c1) in cuts {
        let mut next = Vec::new();
        for (a, b) in pieces {
            if c1 <= a || c0 >= b {
                next.push((a, b));
                continue;
            }
            if c0 > a {
                next.push((a, c0));
            }
            if c1 < b {
                next.push((c1, b));
            }
        }
        pieces = next;
    }
    pieces.retain(|(a, b)| b - a > 0.01);
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, z: f32) -> V2 {
        V2::new(x, z)
    }

    /// Walk from `from` to `goal` the way the sim does; returns the final
    /// position, surface, the visited surfaces and the largest per-step jump in
    /// elevation (to catch snapping).
    fn walk(nav: &Nav, from: V3, goal: V3, steps: usize) -> (V2, SurfaceId, Vec<SurfaceId>) {
        let (p0, mut s) = nav.project(from.xz(), from.y);
        let mut p = p0.xz();
        let mut seen = vec![s];
        for _ in 0..steps {
            let w = nav.next_waypoint(p, s, goal);
            if !w.reachable {
                break;
            }
            let d = w.point - p;
            if d.len() < 1e-3 {
                break;
            }
            let step = d.normalize_or_zero() * d.len().min(0.25);
            let (np, ns, _) = nav.try_move(p, s, step);
            p = np;
            if ns != s {
                s = ns;
                seen.push(s);
            }
        }
        (p, s, seen)
    }

    /// Lower floor at 0, a ramp up along +z to an upper floor at 4.
    fn ramp_level() -> Nav {
        Nav::build(
            vec![
                SurfaceDef::flat("low", v(0.0, 0.0), v(10.0, 10.0), 0.0),
                // Ramp 10 m long rising 4 m (slope 0.4), overlapping each floor by 1 m.
                SurfaceDef::ramp("ramp", v(3.0, 9.0), v(7.0, 21.0), Axis::Z, -0.4, 4.4),
                SurfaceDef::flat("high", v(0.0, 20.0), v(10.0, 30.0), 4.0),
            ],
            vec![],
            NavParams::default(),
        )
        .unwrap()
    }

    #[test]
    fn overlapping_floors_at_different_heights_do_not_connect() {
        // A bridge deck 4 m above a floor, same footprint region.
        let nav = Nav::build(
            vec![
                SurfaceDef::flat("floor", v(0.0, 0.0), v(20.0, 6.0), 0.0),
                SurfaceDef::flat("bridge", v(8.0, -5.0), v(12.0, 11.0), 4.0).open(),
            ],
            vec![],
            NavParams::default(),
        )
        .unwrap();
        assert!(nav.links.is_empty());
        assert!(!nav.reachable(0, 1));
        // Walking along the floor under the bridge stays on the floor.
        let (p, s, seen) = walk(&nav, V3::new(1.0, 0.0, 3.0), V3::new(19.0, 0.0, 3.0), 400);
        assert_eq!(seen, vec![0]);
        assert_eq!(s, 0);
        assert!(p.dist(v(19.0, 3.0)) < 0.1);
    }

    #[test]
    fn solid_surface_above_another_is_rejected_with_a_fix() {
        let err = Nav::build(
            vec![
                SurfaceDef::flat("floor", v(0.0, 0.0), v(20.0, 6.0), 0.0),
                SurfaceDef::flat("ledge", v(8.0, -5.0), v(12.0, 11.0), 4.0),
            ],
            vec![],
            NavParams::default(),
        )
        .unwrap_err();
        assert!(err[0].contains("under = \"open\""), "{err:?}");
    }

    #[test]
    fn floors_too_close_vertically_are_rejected() {
        let err = Nav::build(
            vec![
                SurfaceDef::flat("a", v(0.0, 0.0), v(10.0, 10.0), 0.0),
                SurfaceDef::flat("b", v(5.0, 5.0), v(15.0, 15.0), 1.5).open(),
            ],
            vec![],
            NavParams::default(),
        )
        .unwrap_err();
        assert!(err[0].contains("too far to step"), "{err:?}");
    }

    #[test]
    fn ramp_connects_two_floors_in_both_directions_with_smooth_height() {
        let nav = ramp_level();
        assert!(nav.reachable(0, 2) && nav.reachable(2, 0));
        let up = walk(&nav, V3::new(5.0, 0.0, 2.0), V3::new(5.0, 4.0, 27.0), 600);
        assert_eq!(up.1, 2, "reached the upper floor");
        assert_eq!(up.2, vec![0, 1, 2]);
        let down = walk(&nav, V3::new(5.0, 4.0, 27.0), V3::new(5.0, 0.0, 2.0), 600);
        assert_eq!(down.1, 0);
        assert!(down.0.dist(v(5.0, 2.0)) < 0.1);
        // Height along the ramp interpolates.
        let r = nav.find("ramp").unwrap();
        assert!((nav.height(r, v(5.0, 15.0)) - 2.0).abs() < 0.01);
        // Heights where surfaces hand over agree within a step.
        for l in &nav.links {
            assert!((l.entry.y - l.exit.y).abs() <= nav.params.max_step + 1e-4);
        }
    }

    #[test]
    fn too_steep_ramp_is_rejected() {
        let err = Nav::build(
            vec![SurfaceDef::ramp(
                "cliff",
                v(0.0, 0.0),
                v(4.0, 4.0),
                Axis::Z,
                0.0,
                4.0,
            )],
            vec![],
            NavParams::default(),
        )
        .unwrap_err();
        assert!(err[0].contains("maximum walkable slope"), "{err:?}");
    }

    fn drop_level() -> Nav {
        Nav::build(
            vec![
                SurfaceDef::flat("ledge", v(0.0, 0.0), v(10.0, 10.0), 4.0),
                SurfaceDef::flat("pit", v(0.0, 10.0), v(10.0, 20.0), 0.0),
            ],
            vec![LinkDef {
                from: "ledge".into(),
                to: "pit".into(),
                kind: LinkKind::Drop,
                zone: Rect::new(v(3.0, 8.5), v(7.0, 11.5)),
            }],
            NavParams::default(),
        )
        .unwrap()
    }

    #[test]
    fn one_way_drop_goes_down_but_not_up() {
        let nav = drop_level();
        assert!(nav.reachable(0, 1));
        assert!(!nav.reachable(1, 0));
        let (p, s, seen) = walk(&nav, V3::new(5.0, 4.0, 2.0), V3::new(5.0, 0.0, 18.0), 400);
        assert_eq!(s, 1);
        assert_eq!(seen, vec![0, 1]);
        assert!(p.dist(v(5.0, 18.0)) < 0.1);
        // From the pit, the ledge is unreachable: the waypoint says so.
        let w = nav.next_waypoint(v(5.0, 15.0), 1, V3::new(5.0, 4.0, 2.0));
        assert!(!w.reachable);
        // Walking off the ledge outside the zone is blocked by the edge.
        let (p, s, dropped) = nav.try_move(v(1.0, 9.6), 0, v(0.0, 0.3));
        assert_eq!(s, 0);
        assert!(!dropped);
        assert!(p.z <= 9.6 + 1e-4);
    }

    #[test]
    fn unreachable_platform_is_reported() {
        let nav = Nav::build(
            vec![
                SurfaceDef::flat("floor", v(0.0, 0.0), v(20.0, 20.0), 0.0),
                SurfaceDef::flat("pillar_top", v(30.0, 0.0), v(34.0, 4.0), 6.0),
            ],
            vec![],
            NavParams::default(),
        )
        .unwrap();
        let w = nav.next_waypoint(v(5.0, 5.0), 0, V3::new(32.0, 6.0, 2.0));
        assert!(!w.reachable);
        assert_eq!(w.point, v(5.0, 5.0));
        assert!(
            nav.path_points(v(5.0, 5.0), 0, V3::new(32.0, 6.0, 2.0))
                .is_none()
        );
    }

    #[test]
    fn picks_the_shorter_of_two_routes() {
        // A ring of corridors around a solid block; the goal is closer via the east side.
        let nav = Nav::flat(&[
            Rect::new(v(0.0, 0.0), v(30.0, 4.0)),   // south
            Rect::new(v(0.0, 0.0), v(4.0, 30.0)),   // west
            Rect::new(v(26.0, 0.0), v(30.0, 30.0)), // east
            Rect::new(v(0.0, 26.0), v(30.0, 30.0)), // north
        ]);
        let goal = V3::new(28.0, 0.0, 20.0);
        let path = nav.path_points(v(20.0, 2.0), 0, goal).unwrap();
        assert!(path.iter().all(|p| p.x > 10.0), "went east: {path:?}");
        let (p, _, seen) = walk(&nav, V3::new(20.0, 0.0, 2.0), goal, 400);
        assert!(p.dist(goal.xz()) < 0.1);
        assert!(!seen.contains(&1), "never entered the west corridor");
    }

    #[test]
    fn winding_corridor_is_followed_without_leaving_the_floor() {
        // A zig-zag of five corridors.
        let rects = [
            Rect::new(v(0.0, 0.0), v(3.0, 12.0)),
            Rect::new(v(0.0, 9.0), v(15.0, 12.0)),
            Rect::new(v(12.0, 9.0), v(15.0, 24.0)),
            Rect::new(v(0.0, 21.0), v(15.0, 24.0)),
            Rect::new(v(0.0, 21.0), v(3.0, 36.0)),
        ];
        let nav = Nav::flat(&rects);
        let (mut p, mut s) = (v(1.5, 1.0), 0);
        let goal = V3::new(1.5, 0.0, 35.0);
        let mut last_step = V2::ZERO;
        let mut reversals = 0;
        for _ in 0..2000 {
            let w = nav.next_waypoint(p, s, goal);
            let d = w.point - p;
            let step = d.normalize_or_zero() * d.len().min(0.3);
            let (np, ns, _) = nav.try_move(p, s, step);
            assert!(
                nav.surfaces[ns as usize].walk.contains(np),
                "left the floor at {np:?}"
            );
            let moved = np - p;
            if moved.len() > 1e-3 {
                let a = moved.normalize_or_zero();
                if a.x * last_step.x + a.z * last_step.z < -0.5 {
                    reversals += 1;
                }
                last_step = a;
            }
            p = np;
            s = ns;
        }
        assert!(p.dist(goal.xz()) < 0.1, "{p:?}");
        assert_eq!(reversals, 0, "never turns back on itself (no oscillation)");
    }

    #[test]
    fn crossing_region_boundaries_is_stable() {
        // Thin overlap: a 0.3 m step must still cross into the next corridor.
        let nav = Nav::flat(&[
            Rect::new(v(0.0, 0.0), v(4.0, 10.0)),
            Rect::new(v(0.0, 9.15), v(4.0, 20.0)),
        ]);
        let (p, s, seen) = walk(&nav, V3::new(2.0, 0.0, 1.0), V3::new(2.0, 0.0, 19.0), 400);
        assert_eq!(s, 1);
        assert_eq!(seen, vec![0, 1], "one hand-over, no flip-flopping");
        assert!(p.dist(v(2.0, 19.0)) < 0.1);
    }

    #[test]
    fn walls_block_movement_and_sight() {
        let nav = Nav::flat(&[
            Rect::new(v(0.0, 0.0), v(4.0, 20.0)),
            Rect::new(v(0.0, 16.0), v(30.0, 20.0)),
        ]);
        let (p, s, _) = nav.try_move(v(2.0, 5.0), 0, v(10.0, 0.0));
        assert_eq!(s, 0);
        assert!(p.x < 4.0);
        // Around the corner of the L: no sight; along the corridor: sight.
        assert!(!nav.line_of_sight(V3::new(2.0, 1.5, 2.0), V3::new(25.0, 1.0, 18.0)));
        assert!(nav.line_of_sight(V3::new(2.0, 1.5, 2.0), V3::new(2.0, 1.0, 18.0)));
        assert!(nav.line_of_sight(V3::new(2.0, 1.5, 18.0), V3::new(25.0, 1.0, 18.0)));
    }

    #[test]
    fn floors_block_sight_between_levels() {
        let nav = Nav::build(
            vec![
                SurfaceDef::flat("floor", v(0.0, 0.0), v(20.0, 6.0), 0.0),
                SurfaceDef::flat("bridge", v(8.0, -5.0), v(12.0, 11.0), 4.0).open(),
            ],
            vec![],
            NavParams::default(),
        )
        .unwrap();
        // Directly under the bridge looking up at someone on it: blocked.
        assert!(!nav.line_of_sight(V3::new(10.0, 1.5, 3.0), V3::new(10.0, 5.0, 3.5)));
        // From the floor well beside the bridge up to its far end: clear.
        assert!(nav.line_of_sight(V3::new(1.0, 1.5, 3.0), V3::new(10.0, 5.0, 5.5)));
        // ...but not someone on the bridge outside the corridor: its wall is in the way.
        assert!(!nav.line_of_sight(V3::new(1.0, 1.5, 3.0), V3::new(10.0, 5.0, 10.0)));
        // Under the deck, along the floor: clear.
        assert!(nav.line_of_sight(V3::new(1.0, 1.5, 3.0), V3::new(19.0, 1.0, 3.0)));
    }

    #[test]
    fn solid_ledge_hides_the_floor_behind_it() {
        let nav = ramp_level();
        // From the low floor's far corner, someone deep on the high floor is
        // hidden by the rock under the high floor's edge.
        assert!(!nav.line_of_sight(V3::new(5.0, 1.5, 1.0), V3::new(5.0, 5.0, 28.0)));
        // Standing at the high floor's edge, the floor below is visible.
        assert!(nav.line_of_sight(V3::new(5.0, 5.5, 20.5), V3::new(5.0, 1.0, 5.0)));
    }

    #[test]
    fn walls_have_openings_at_connections_and_drop_zones() {
        let nav = drop_level();
        // The ledge has no wall where the drop zone crosses its edge.
        let blocked = nav.walls.iter().any(|w| {
            w.surface == 0
                && (w.a.z - 10.0).abs() < 1e-3
                && w.a.x.min(w.b.x) < 5.0
                && w.a.x.max(w.b.x) > 5.0
        });
        assert!(!blocked);
        let nav = ramp_level();
        // Walls follow the ramp's slope.
        let w = nav
            .walls
            .iter()
            .find(|w| w.surface == 1 && (w.a.x - 3.0).abs() < 1e-3)
            .unwrap();
        assert!((w.bottom0 - w.bottom1).abs() > 3.0);
    }

    #[test]
    fn projection_prefers_the_nearby_level() {
        let nav = Nav::build(
            vec![
                SurfaceDef::flat("floor", v(0.0, 0.0), v(20.0, 6.0), 0.0),
                SurfaceDef::flat("bridge", v(8.0, -5.0), v(12.0, 11.0), 4.0).open(),
            ],
            vec![],
            NavParams::default(),
        )
        .unwrap();
        // Beside the bridge but over the floor, at bridge height: snaps onto the bridge.
        let (p, s) = nav.project(v(14.0, 3.0), 4.0);
        assert_eq!(s, 1);
        assert!((p.y - 4.0).abs() < 1e-4);
        // Same point at floor height: the floor.
        assert_eq!(nav.project(v(14.0, 3.0), 0.0).1, 0);
        // Under the bridge: locate picks by elevation.
        assert_eq!(nav.locate(v(10.0, 3.0), 0.2), Some(0));
        assert_eq!(nav.locate(v(10.0, 3.0), 3.9), Some(1));
    }

    #[test]
    fn invalid_links_produce_actionable_errors() {
        let err = Nav::build(
            vec![
                SurfaceDef::flat("ledge", v(0.0, 0.0), v(10.0, 10.0), 4.0),
                SurfaceDef::flat("pit", v(0.0, 10.0), v(10.0, 20.0), 0.0),
            ],
            vec![
                LinkDef {
                    from: "ledge".into(),
                    to: "pitt".into(),
                    kind: LinkKind::Drop,
                    zone: Rect::new(v(3.0, 8.5), v(7.0, 11.5)),
                },
                LinkDef {
                    from: "ledge".into(),
                    to: "pit".into(),
                    kind: LinkKind::Walk,
                    zone: Rect::new(v(3.0, 8.5), v(7.0, 11.5)),
                },
            ],
            NavParams::default(),
        )
        .unwrap_err();
        assert!(
            err.iter().any(|e| e.contains("known areas: ledge, pit")),
            "{err:?}"
        );
        assert!(
            err.iter().any(|e| e.contains("declared as a walk link")),
            "{err:?}"
        );
    }

    #[test]
    fn routing_is_deterministic() {
        let nav = ramp_level();
        let a = nav.path_points(v(5.0, 2.0), 0, V3::new(5.0, 4.0, 27.0));
        let b = ramp_level().path_points(v(5.0, 2.0), 0, V3::new(5.0, 4.0, 27.0));
        assert_eq!(a, b);
    }
}
