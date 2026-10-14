//! Navigation over a walkable floor made of overlapping axis-aligned rectangles.
//!
//! Shortcut (documented in PROGRESS.md): instead of a navmesh, each rectangle is a
//! graph node and overlapping rectangles are connected. To route from A to B we
//! BFS the rectangle graph and steer toward the centre of the overlap between the
//! current rectangle and the next one. Because rectangles are convex, the straight
//! line to that overlap centre is always walkable. Actors do not collide with each
//! other and walls do not block line of sight.

use crate::math::V2;
use std::collections::VecDeque;

/// Distance actors keep from walls.
pub const WALL_MARGIN: f32 = 0.4;

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

    fn shrunk(&self, m: f32) -> Rect {
        Rect::new(
            V2::new(self.min.x + m, self.min.z + m),
            V2::new(self.max.x - m, self.max.z - m),
        )
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

    fn intersection(&self, o: &Rect) -> Option<Rect> {
        let min = V2::new(self.min.x.max(o.min.x), self.min.z.max(o.min.z));
        let max = V2::new(self.max.x.min(o.max.x), self.max.z.min(o.max.z));
        (min.x <= max.x && min.z <= max.z).then_some(Rect::new(min, max))
    }
}

#[derive(Clone, Debug)]
pub struct Nav {
    /// Raw rectangles as authored (used for rendering).
    pub areas: Vec<Rect>,
    /// Rectangles shrunk by the wall margin (used for movement).
    walk: Vec<Rect>,
    adj: Vec<Vec<usize>>,
}

impl Nav {
    pub fn new(areas: Vec<Rect>) -> Self {
        let walk: Vec<Rect> = areas.iter().map(|r| r.shrunk(WALL_MARGIN)).collect();
        let mut adj = vec![Vec::new(); walk.len()];
        for i in 0..walk.len() {
            for j in (i + 1)..walk.len() {
                if walk[i].intersection(&walk[j]).is_some() {
                    adj[i].push(j);
                    adj[j].push(i);
                }
            }
        }
        Nav { areas, walk, adj }
    }

    pub fn contains(&self, p: V2) -> bool {
        self.walk.iter().any(|r| r.contains(p))
    }

    pub fn is_connected(&self) -> bool {
        if self.walk.is_empty() {
            return true;
        }
        let mut seen = vec![false; self.walk.len()];
        let mut q = VecDeque::from([0usize]);
        seen[0] = true;
        while let Some(i) = q.pop_front() {
            for &j in &self.adj[i] {
                if !seen[j] {
                    seen[j] = true;
                    q.push_back(j);
                }
            }
        }
        seen.into_iter().all(|s| s)
    }

    /// Wall segments (start, end) along the outline of the union of the authored
    /// rectangles: each rectangle edge minus the parts lying inside another rectangle.
    pub fn wall_segments(&self) -> Vec<(V2, V2)> {
        let mut out = Vec::new();
        for (i, r) in self.areas.iter().enumerate() {
            // Horizontal edges (constant z) and vertical edges (constant x).
            for &z in &[r.min.z, r.max.z] {
                let cuts: Vec<(f32, f32)> = self
                    .areas
                    .iter()
                    .enumerate()
                    .filter(|(j, o)| *j != i && o.min.z < z && z < o.max.z)
                    .map(|(_, o)| (o.min.x, o.max.x))
                    .collect();
                for (a, b) in subtract_intervals((r.min.x, r.max.x), &cuts) {
                    out.push((V2::new(a, z), V2::new(b, z)));
                }
            }
            for &x in &[r.min.x, r.max.x] {
                let cuts: Vec<(f32, f32)> = self
                    .areas
                    .iter()
                    .enumerate()
                    .filter(|(j, o)| *j != i && o.min.x < x && x < o.max.x)
                    .map(|(_, o)| (o.min.z, o.max.z))
                    .collect();
                for (a, b) in subtract_intervals((r.min.z, r.max.z), &cuts) {
                    out.push((V2::new(x, a), V2::new(x, b)));
                }
            }
        }
        out
    }

    /// Nearest walkable point to `p`.
    pub fn project(&self, p: V2) -> V2 {
        if self.contains(p) {
            return p;
        }
        let mut best = p;
        let mut best_d = f32::INFINITY;
        for r in &self.walk {
            let c = r.clamp(p);
            let d = c.dist(p);
            if d < best_d {
                best_d = d;
                best = c;
            }
        }
        best
    }

    fn containing(&self, p: V2) -> Vec<usize> {
        (0..self.walk.len())
            .filter(|&i| self.walk[i].contains(p))
            .collect()
    }

    /// Next point to steer toward when travelling from `from` to `to`.
    /// Returns `to` (projected onto the floor) when it is directly reachable.
    pub fn next_waypoint(&self, from: V2, to: V2) -> V2 {
        let to = self.project(to);
        let from = self.project(from);
        let starts = self.containing(from);
        let goals = self.containing(to);
        if starts.iter().any(|s| goals.contains(s)) {
            return to;
        }
        // Multi-source BFS over the rectangle graph.
        let mut prev = vec![usize::MAX; self.walk.len()];
        let mut seen = vec![false; self.walk.len()];
        let mut q = VecDeque::new();
        for &s in &starts {
            seen[s] = true;
            q.push_back(s);
        }
        let mut found = None;
        while let Some(i) = q.pop_front() {
            if goals.contains(&i) {
                found = Some(i);
                break;
            }
            for &j in &self.adj[i] {
                if !seen[j] {
                    seen[j] = true;
                    prev[j] = i;
                    q.push_back(j);
                }
            }
        }
        let Some(mut node) = found else {
            return to;
        };
        // Walk back to the first rectangle after a start rectangle.
        while prev[node] != usize::MAX && prev[prev[node]] != usize::MAX {
            node = prev[node];
        }
        let first = prev[node];
        if first == usize::MAX {
            return to;
        }
        match self.walk[first].intersection(&self.walk[node]) {
            Some(overlap) => overlap.center(),
            None => to,
        }
    }

    /// Move from `from` by `delta`, sliding along walls if blocked.
    pub fn try_move(&self, from: V2, delta: V2) -> V2 {
        let target = from + delta;
        if self.contains(target) {
            return target;
        }
        let slide_x = V2::new(from.x + delta.x, from.z);
        if self.contains(slide_x) {
            return slide_x;
        }
        let slide_z = V2::new(from.x, from.z + delta.z);
        if self.contains(slide_z) {
            return slide_z;
        }
        if self.contains(from) {
            from
        } else {
            self.project(from)
        }
    }
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

    fn l_shape() -> Nav {
        // A vertical corridor and a horizontal corridor joined at the top.
        Nav::new(vec![
            Rect::new(V2::new(0.0, 0.0), V2::new(4.0, 20.0)),
            Rect::new(V2::new(0.0, 16.0), V2::new(30.0, 20.0)),
        ])
    }

    #[test]
    fn direct_when_same_rect() {
        let nav = l_shape();
        let w = nav.next_waypoint(V2::new(2.0, 1.0), V2::new(2.0, 10.0));
        assert_eq!(w, V2::new(2.0, 10.0));
    }

    #[test]
    fn routes_through_overlap() {
        let nav = l_shape();
        let w = nav.next_waypoint(V2::new(2.0, 1.0), V2::new(28.0, 18.0));
        // Overlap of the two shrunk rects is around x in [0.4,3.6], z in [16.4,19.6].
        assert!(w.z > 16.0 && w.x < 4.0, "{w:?}");
    }

    #[test]
    fn walls_leave_openings_where_rects_overlap() {
        let nav = l_shape();
        let walls = nav.wall_segments();
        // The top edge of the vertical corridor (z = 20) is shared with the horizontal
        // corridor's top edge, but the vertical corridor's top edge lies on the boundary
        // so it stays; no wall may cross the interior of the junction.
        for (a, b) in &walls {
            let mid = (*a + *b) * 0.5;
            let inside_other = nav
                .areas
                .iter()
                .filter(|r| {
                    mid.x > r.min.x + 1e-3
                        && mid.x < r.max.x - 1e-3
                        && mid.z > r.min.z + 1e-3
                        && mid.z < r.max.z - 1e-3
                })
                .count();
            assert_eq!(inside_other, 0, "wall {a:?}-{b:?} cuts through floor");
        }
        assert!(!walls.is_empty());
    }

    #[test]
    fn walls_block_movement() {
        let nav = l_shape();
        let p = nav.try_move(V2::new(2.0, 5.0), V2::new(10.0, 0.0));
        assert!(nav.contains(p));
        assert!(p.x < 4.0);
    }

    #[test]
    fn reaches_goal_by_following_waypoints() {
        let nav = l_shape();
        let mut p = V2::new(2.0, 1.0);
        let goal = V2::new(28.0, 18.0);
        for _ in 0..1000 {
            let w = nav.next_waypoint(p, goal);
            let d = w - p;
            let step = d.normalize_or_zero() * d.len().min(0.5);
            p = nav.try_move(p, step);
        }
        assert!(p.dist(goal) < 0.1, "{p:?}");
    }
}
