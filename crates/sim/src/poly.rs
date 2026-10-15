//! Convex polygons on the horizontal plane (x, z), used as navigation
//! footprints, portals and decoration outlines.
//!
//! Points are stored counter-clockwise (positive shoelace area with
//! `cross(a, b) = a.x * b.z - a.z * b.x`), so the interior is on the left of
//! every edge. All operations are small loops over a handful of points and
//! never allocate except when building a new polygon.

use crate::math::V2;
use crate::nav::Rect;

fn cross(a: V2, b: V2) -> f32 {
    a.x * b.z - a.z * b.x
}

fn dot(a: V2, b: V2) -> f32 {
    a.x * b.x + a.z * b.z
}

#[derive(Clone, Debug, PartialEq)]
pub struct Poly {
    pts: Vec<V2>,
    /// Set when the polygon is an axis-aligned rectangle: the queries then use
    /// exact rectangle maths (0.4 levels navigate bit-for-bit as before).
    rect: Option<Rect>,
}

impl Poly {
    /// Build a convex polygon from points in either winding order. Collinear
    /// and duplicate points are dropped. Errors are complete sentences.
    pub fn new(points: &[V2]) -> Result<Poly, String> {
        if points.iter().any(|p| !p.is_finite()) {
            return Err("points must be finite numbers".into());
        }
        let mut pts: Vec<V2> = Vec::with_capacity(points.len());
        for &p in points {
            if pts.last().is_none_or(|q: &V2| q.dist(p) > 1e-4) {
                pts.push(p);
            }
        }
        while pts.len() > 1 && pts[0].dist(*pts.last().expect("non-empty")) <= 1e-4 {
            pts.pop();
        }
        if pts.len() < 3 {
            return Err("a polygon needs at least 3 distinct points".into());
        }
        if signed_area(&pts) < 0.0 {
            pts.reverse();
        }
        // Drop collinear points.
        let mut k = 0;
        while k < pts.len() && pts.len() > 3 {
            let n = pts.len();
            let (a, b, c) = (pts[(k + n - 1) % n], pts[k], pts[(k + 1) % n]);
            if cross(b - a, c - b).abs() <= 1e-5 * (b - a).len().max(1.0) * (c - b).len().max(1.0) {
                pts.remove(k);
            } else {
                k += 1;
            }
        }
        let n = pts.len();
        for k in 0..n {
            let (a, b, c) = (pts[k], pts[(k + 1) % n], pts[(k + 2) % n]);
            if cross(b - a, c - b) < -1e-5 {
                return Err(format!(
                    "the polygon is not convex at [{:.1}, {:.1}]; split it into convex pieces that overlap where they meet",
                    b.x, b.z
                ));
            }
        }
        let poly = Poly::from_pts(pts);
        if poly.area() < 1e-3 {
            return Err("the polygon has (almost) no area".into());
        }
        Ok(poly)
    }

    pub fn from_rect(r: Rect) -> Poly {
        Poly {
            pts: vec![
                r.min,
                V2::new(r.max.x, r.min.z),
                r.max,
                V2::new(r.min.x, r.max.z),
            ],
            rect: Some(r),
        }
    }

    fn from_pts(pts: Vec<V2>) -> Poly {
        let axis = pts.len() == 4 && {
            let n = pts.len();
            (0..n).all(|k| {
                let (a, b) = (pts[k], pts[(k + 1) % n]);
                a.x == b.x || a.z == b.z
            })
        };
        let rect = axis.then(|| {
            let mut min = pts[0];
            let mut max = pts[0];
            for p in &pts {
                min = V2::new(min.x.min(p.x), min.z.min(p.z));
                max = V2::new(max.x.max(p.x), max.z.max(p.z));
            }
            Rect::new(min, max)
        });
        Poly { pts, rect }
    }

    /// Four points with axis-parallel edges.
    pub fn is_axis_rect(&self) -> bool {
        self.rect.is_some()
    }

    pub fn points(&self) -> &[V2] {
        &self.pts
    }

    /// Edges as (start, end), counter-clockwise.
    pub fn edges(&self) -> impl Iterator<Item = (V2, V2)> + '_ {
        let n = self.pts.len();
        (0..n).map(move |k| (self.pts[k], self.pts[(k + 1) % n]))
    }

    pub fn area(&self) -> f32 {
        signed_area(&self.pts)
    }

    pub fn centroid(&self) -> V2 {
        if self.is_axis_rect() {
            return self.bounds().center();
        }
        let mut a = 0.0;
        let (mut cx, mut cz) = (0.0, 0.0);
        for (p, q) in self.edges() {
            let c = cross(p, q);
            a += c;
            cx += (p.x + q.x) * c;
            cz += (p.z + q.z) * c;
        }
        if a.abs() < 1e-9 {
            let n = self.pts.len() as f32;
            let s = self.pts.iter().fold(V2::ZERO, |s, &p| s + p);
            return s * (1.0 / n);
        }
        V2::new(cx / (3.0 * a), cz / (3.0 * a))
    }

    pub fn bounds(&self) -> Rect {
        if let Some(r) = self.rect {
            return r;
        }
        let mut min = self.pts[0];
        let mut max = self.pts[0];
        for p in &self.pts {
            min = V2::new(min.x.min(p.x), min.z.min(p.z));
            max = V2::new(max.x.max(p.x), max.z.max(p.z));
        }
        Rect::new(min, max)
    }

    /// Width along `dir` (unit vector).
    pub fn extent_along(&self, dir: V2) -> f32 {
        let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
        for &p in &self.pts {
            let d = dot(p, dir);
            lo = lo.min(d);
            hi = hi.max(d);
        }
        hi - lo
    }

    pub fn contains(&self, p: V2) -> bool {
        match self.rect {
            Some(r) => r.contains(p),
            None => self.contains_tol(p, 1e-4),
        }
    }

    /// Inside, allowing points up to `tol` metres outside an edge.
    pub fn contains_tol(&self, p: V2, tol: f32) -> bool {
        self.edges().all(|(a, b)| {
            let e = b - a;
            cross(e, p - a) >= -tol * e.len()
        })
    }

    /// Nearest point of the polygon (itself when inside).
    pub fn clamp(&self, p: V2) -> V2 {
        if let Some(r) = self.rect {
            return r.clamp(p);
        }
        if self.contains(p) {
            return p;
        }
        let mut best = self.pts[0];
        let mut best_d = f32::INFINITY;
        for (a, b) in self.edges() {
            let e = b - a;
            let t = (dot(p - a, e) / dot(e, e).max(1e-12)).clamp(0.0, 1.0);
            let q = a + e * t;
            let d = q.dist(p);
            if d < best_d {
                best_d = d;
                best = q;
            }
        }
        best
    }

    /// Parameter interval `[t0, t1]` (within 0..1) where segment `a`→`b` is
    /// inside the polygon (Cyrus–Beck).
    pub fn clip(&self, a: V2, b: V2) -> Option<(f32, f32)> {
        if let Some(r) = self.rect {
            return r.clip(a, b);
        }
        let d = b - a;
        let (mut t0, mut t1) = (0.0f32, 1.0f32);
        for (p, q) in self.edges() {
            let e = q - p;
            let num = cross(e, a - p);
            let den = cross(e, d);
            if den.abs() < 1e-9 {
                if num < -1e-6 {
                    return None;
                }
            } else {
                let t = -num / den;
                if den > 0.0 {
                    t0 = t0.max(t);
                } else {
                    t1 = t1.min(t);
                }
                if t0 > t1 {
                    return None;
                }
            }
        }
        (t0 <= t1).then_some((t0, t1))
    }

    /// Keep the part where the affine function `f(p) = c + n·p` is >= 0.
    pub fn clip_halfplane(&self, n: V2, c: f32) -> Option<Poly> {
        let f = |p: V2| c + dot(n, p);
        let mut out = Vec::with_capacity(self.pts.len() + 1);
        for (p, q) in self.edges() {
            let (fp, fq) = (f(p), f(q));
            if fp >= 0.0 {
                out.push(p);
            }
            if (fp >= 0.0) != (fq >= 0.0) {
                let t = fp / (fp - fq);
                out.push(p + (q - p) * t);
            }
        }
        let mut dedup: Vec<V2> = Vec::with_capacity(out.len());
        for p in out {
            if dedup.last().is_none_or(|q: &V2| q.dist(p) > 1e-6) {
                dedup.push(p);
            }
        }
        while dedup.len() > 1 && dedup[0].dist(*dedup.last().expect("non-empty")) <= 1e-6 {
            dedup.pop();
        }
        (dedup.len() >= 3 && signed_area(&dedup) > 1e-7).then(|| Poly::from_pts(dedup))
    }

    /// The overlap of two convex polygons.
    pub fn intersect(&self, other: &Poly) -> Option<Poly> {
        if self.is_axis_rect() && other.is_axis_rect() {
            let r = self.bounds().intersection(&other.bounds())?;
            return (r.min.x < r.max.x && r.min.z < r.max.z).then(|| Poly::from_rect(r));
        }
        let mut cur = self.clone();
        for (a, b) in other.edges() {
            let e = b - a;
            // Interior of `other`: cross(e, p - a) >= 0  ⇔  -cross(e, a) + cross(e, p) >= 0.
            let n = V2::new(-e.z, e.x);
            cur = cur.clip_halfplane(n, -dot(n, a))?;
        }
        Some(cur)
    }

    /// The polygon shrunk by `m` metres on every side (empty if too thin).
    pub fn inset(&self, m: f32) -> Option<Poly> {
        // Axis-aligned rectangles (all legacy maps) take the exact path, so 0.4
        // levels navigate bit-for-bit as before.
        if self.is_axis_rect() {
            let r = self.bounds().shrunk(m);
            return (r.min.x < r.max.x && r.min.z < r.max.z).then(|| Poly::from_rect(r));
        }
        let mut cur = self.clone();
        for (a, b) in self.edges() {
            let e = b - a;
            let n = V2::new(-e.z, e.x) * (1.0 / e.len().max(1e-9));
            cur = cur.clip_halfplane(n, -dot(n, a) - m)?;
        }
        Some(cur)
    }
}

fn signed_area(pts: &[V2]) -> f32 {
    let n = pts.len();
    let mut s = 0.0;
    for k in 0..n {
        s += cross(pts[k], pts[(k + 1) % n]);
    }
    s * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, z: f32) -> V2 {
        V2::new(x, z)
    }

    fn diamond() -> Poly {
        Poly::new(&[v(0.0, -5.0), v(5.0, 0.0), v(0.0, 5.0), v(-5.0, 0.0)]).unwrap()
    }

    #[test]
    fn builds_counter_clockwise_and_rejects_bad_shapes() {
        // Clockwise input is reversed; area is positive either way.
        let cw = Poly::new(&[v(0.0, 0.0), v(0.0, 4.0), v(4.0, 4.0), v(4.0, 0.0)]).unwrap();
        assert!((cw.area() - 16.0).abs() < 1e-4);
        // Collinear points are dropped.
        let p = Poly::new(&[
            v(0.0, 0.0),
            v(2.0, 0.0),
            v(4.0, 0.0),
            v(4.0, 4.0),
            v(0.0, 4.0),
        ])
        .unwrap();
        assert_eq!(p.points().len(), 4);
        let err = Poly::new(&[
            v(0.0, 0.0),
            v(4.0, 0.0),
            v(1.0, 1.0),
            v(4.0, 4.0),
            v(0.0, 4.0),
        ])
        .unwrap_err();
        assert!(err.contains("not convex"), "{err}");
        assert!(Poly::new(&[v(0.0, 0.0), v(1.0, 1.0)]).is_err());
    }

    #[test]
    fn rectangles_behave_like_rects() {
        let r = Rect::new(v(0.0, 0.0), v(10.0, 4.0));
        let p = Poly::from_rect(r);
        assert!(p.contains(v(5.0, 2.0)) && !p.contains(v(11.0, 2.0)));
        assert_eq!(p.clamp(v(12.0, 9.0)), v(10.0, 4.0));
        let inset = p.inset(0.4).unwrap();
        assert_eq!(inset.bounds(), r.shrunk(0.4));
        assert_eq!(
            p.clip(v(-5.0, 2.0), v(15.0, 2.0)),
            r.clip(v(-5.0, 2.0), v(15.0, 2.0))
        );
    }

    #[test]
    fn diagonal_shapes_contain_clamp_clip_and_intersect() {
        let d = diamond();
        assert!(d.contains(v(0.0, 0.0)) && d.contains(v(2.4, 2.4)));
        assert!(!d.contains(v(4.0, 4.0)));
        let c = d.clamp(v(10.0, 10.0));
        assert!((c.x - 2.5).abs() < 1e-4 && (c.z - 2.5).abs() < 1e-4);
        let (t0, t1) = d.clip(v(-10.0, 0.0), v(10.0, 0.0)).unwrap();
        assert!((t0 - 0.25).abs() < 1e-4 && (t1 - 0.75).abs() < 1e-4);
        assert!(d.clip(v(4.0, 4.0), v(10.0, 4.0)).is_none());
        let sq = Poly::from_rect(Rect::new(v(0.0, 0.0), v(10.0, 10.0)));
        let ov = d.intersect(&sq).unwrap();
        assert!((ov.area() - 12.5).abs() < 1e-3, "{}", ov.area());
        let far = Poly::from_rect(Rect::new(v(20.0, 20.0), v(30.0, 30.0)));
        assert!(d.intersect(&far).is_none());
    }

    #[test]
    fn inset_shrinks_every_edge_and_vanishes_when_too_thin() {
        let d = diamond();
        let i = d.inset(1.0).unwrap();
        for &p in i.points() {
            assert!(d.contains(p));
            // Every inset point is at least 1 m from every original edge.
            for (a, b) in d.edges() {
                let e = b - a;
                assert!(cross(e, p - a) / e.len() >= 1.0 - 1e-3);
            }
        }
        let thin = Poly::from_rect(Rect::new(v(0.0, 0.0), v(10.0, 0.6)));
        assert!(thin.inset(0.4).is_none());
        let c = d.centroid();
        assert!(c.len() < 1e-4);
    }
}
