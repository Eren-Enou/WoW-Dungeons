//! Encounter placement: where in its room a recipe's pack stands.
//!
//! Candidate anchors are sampled on a fixed lattice over the room's floor
//! (so pits, pillars and cut corners are excluded automatically). For each
//! candidate the recipe's formation is laid out facing the party's entry and
//! checked; failing candidates are rejected, the rest scored, and the best
//! wins. Everything is deterministic: fixed lattice order, strict
//! comparisons.
//!
//! Rejections (hard rules):
//! * a member would stand off the floor (with margin) or on a hazard;
//! * the pack would aggro the party waiting in the entry corridor;
//! * the pack would be in aggro reach of an already placed encounter;
//! * the recipe needs a ranged lane and no floor spot 12-18 m away sees it;
//! * a patrol route isn't a walkable route, or passes the staging spot or
//!   another pack.
//!
//! Scores (soft preferences): the recipe's `placement` (far from the entry,
//! in front of another exit, in the middle), plus free floor around the
//! anchor (room to dodge rings and reposition).

use crate::kit::{Formation, Placement, Preference, Recipe};
use crate::layout::{Dir, Room};
use dungeon_sim::math::{V2, V3};
use dungeon_sim::nav::Nav;
use dungeon_sim::poly::Poly;

/// Keep mobs and hazards this far inside the floor.
pub const FLOOR_MARGIN: f32 = 1.0;
/// Lattice step for candidate anchors (m).
const STEP: f32 = 1.5;
/// Pack members must leave this margin beyond their aggro radius to the spot
/// where the party waits.
const STAGING_MARGIN: f32 = 1.5;
/// Packs keep their largest aggro radius plus this from other packs.
const PACK_GAP: f32 = 6.0;

/// A mob's position and, for patrols, its route.
#[derive(Debug, Clone)]
pub struct Spot {
    pub template: String,
    pub pos: V2,
    pub patrol: Vec<V2>,
}

/// Everything placement needs to know about one room.
pub struct RoomView<'a> {
    pub room: &'a Room,
    pub floors: &'a [Poly],
    /// Corridor mouths on this room's walls (world), the entry one first.
    pub entry_mouth: V2,
    pub other_mouths: Vec<V2>,
    /// Where the party waits before the pull (in the entry corridor).
    pub staging: V2,
}

/// A pack already placed (for spacing).
pub struct Placed {
    pub pos: V2,
    pub y: f32,
    pub aggro: f32,
}

pub struct Choice {
    pub spots: Vec<Spot>,
    pub anchor: V2,
    /// Facing toward the party's approach (unit).
    pub toward_entry: V2,
}

fn inset_floors(floors: &[Poly], margin: f32) -> Vec<Poly> {
    floors.iter().filter_map(|f| f.inset(margin)).collect()
}

fn on(inset: &[Poly], p: V2) -> bool {
    inset.iter().any(|f| f.contains(p))
}

/// Lattice points on the floor of a room, in a fixed order.
pub fn lattice(view: &RoomView, margin: f32) -> Vec<V2> {
    let inset = inset_floors(view.floors, margin);
    let r = view.room;
    let c = r.center();
    let mut pts = Vec::new();
    let (x0, x1) = (c.x - r.ext(Dir::W), c.x + r.ext(Dir::E));
    let (z0, z1) = (c.z - r.ext(Dir::S), c.z + r.ext(Dir::N));
    let mut z = z0 + STEP / 2.0;
    while z < z1 {
        let mut x = x0 + STEP / 2.0;
        while x < x1 {
            let p = V2::new(x, z);
            if on(&inset, p) {
                pts.push(p);
            }
            x += STEP;
        }
        z += STEP;
    }
    pts
}

fn spread(i: u32, n: u32, gap: f32) -> f32 {
    (i as f32 - (n as f32 - 1.0) / 2.0) * gap
}

/// The formation laid out round `anchor`; `f` points toward the entry.
fn formation(recipe: &Recipe, anchor: V2, f: V2) -> Vec<(String, V2)> {
    let v = V2::new(-f.z, f.x);
    let mut out = Vec::new();
    if recipe.formation == Formation::Ring {
        let total: u32 = recipe.mobs.iter().map(|m| m.count).sum();
        let radius = 2.2 + 0.45 * total.saturating_sub(2) as f32;
        let mut k = 0;
        for m in &recipe.mobs {
            for _ in 0..m.count {
                // Half a step off the entry axis: no member points straight
                // at the door (packs fit small rooms without aggroing it).
                let a = std::f32::consts::TAU * (k as f32 + 0.5) / total.max(1) as f32;
                let dir = f * a.cos() + v * a.sin();
                let p = if total == 1 {
                    anchor
                } else {
                    anchor + dir * radius
                };
                out.push((m.template.clone(), p));
                k += 1;
            }
        }
        return out;
    }
    let count = |p: Placement| {
        recipe
            .mobs
            .iter()
            .filter(|m| m.place == p)
            .map(|m| m.count)
            .sum::<u32>()
    };
    let (n_front, n_back, n_center) = (
        count(Placement::Front),
        count(Placement::Back),
        count(Placement::Center),
    );
    let (front_d, front_gap, back_d, back_gap) = match recipe.formation {
        Formation::Line | Formation::Ring => (1.8, 2.4, 1.8, 2.4),
        Formation::Wedge => (1.4, 2.6, 2.4, 2.8),
        Formation::Screen => (2.2, 1.9, 3.6, 2.8),
    };
    let (mut fi, mut bi, mut ci, mut li) = (0u32, 0u32, 0u32, 0u32);
    for m in &recipe.mobs {
        for _ in 0..m.count {
            let p = match m.place {
                Placement::Front => {
                    let t = spread(fi, n_front, front_gap);
                    // A wedge's point stands further forward.
                    let extra = if recipe.formation == Formation::Wedge && n_front > 1 {
                        1.2 * (1.0 - (t.abs() / (front_gap * n_front as f32)).min(1.0))
                    } else {
                        0.0
                    };
                    fi += 1;
                    anchor + f * (front_d + extra) + v * t
                }
                Placement::Back => {
                    let t = spread(bi, n_back, back_gap);
                    bi += 1;
                    anchor - f * back_d + v * t
                }
                Placement::Center => {
                    let t = spread(ci, n_center, 2.4);
                    ci += 1;
                    anchor + v * t
                }
                Placement::Flank => {
                    let side = if li % 2 == 0 { 1.0 } else { -1.0 };
                    let dist = 3.5 + 2.0 * (li / 2) as f32;
                    li += 1;
                    anchor + v * (side * dist)
                }
            };
            out.push((m.template.clone(), p));
        }
    }
    out
}

/// Does any floor spot 12-18 m from `anchor` see it (eye height both ends)?
fn has_ranged_lane(nav: &Nav, view: &RoomView, lattice: &[V2], anchor: V2) -> bool {
    let y = view.room.y + 1.5;
    lattice.iter().any(|p| {
        let d = p.dist(anchor);
        (12.0..=18.0).contains(&d) && nav.line_of_sight(V3::at(anchor, y), V3::at(*p, y))
    }) || {
        // Small rooms: the lane may run out into a corridor; accept a spot in
        // the entry corridor if it can see the pack.
        let s = view.staging;
        let d = s.dist(anchor);
        (10.0..=20.0).contains(&d) && nav.line_of_sight(V3::at(anchor, y), V3::at(s, y))
    }
}

/// Choose where `recipe`'s pack stands in this room, or `None` if no spot
/// satisfies its hard rules. `aggro` gives a template's aggro radius.
#[allow(clippy::too_many_arguments)]
pub fn place(
    nav: &Nav,
    view: &RoomView,
    recipe: &Recipe,
    aggro: &dyn Fn(&str) -> f32,
    others: &[Placed],
    hazards_at: &[(V2, f32)],
) -> Option<Choice> {
    let room = view.room;
    if room.min_ext() < recipe.space.min_size {
        return None;
    }
    let lat = lattice(view, FLOOR_MARGIN);
    let inset = inset_floors(view.floors, FLOOR_MARGIN);
    let entry_dir = room.entry.map(|d| d.unit()).unwrap_or(V2::new(0.0, 1.0));
    // Members face the party: "toward the entry" is against the direction of travel.
    let f = entry_dir * -1.0;
    let max_aggro = recipe
        .mobs
        .iter()
        .map(|m| aggro(&m.template))
        .fold(0.0f32, f32::max);
    let clear_of_others = |p: V2| {
        others.iter().all(|o| {
            (o.y - room.y).abs() >= 3.0 || p.dist(o.pos) >= o.aggro.max(max_aggro) + PACK_GAP
        })
    };
    let staged = |p: V2, template: &str| p.dist(view.staging) - aggro(template) >= STAGING_MARGIN;
    let off_hazards = |p: V2| hazards_at.iter().all(|(h, r)| p.dist(*h) > r + 1.0);
    // Free floor round a point (room to move), counted on the lattice.
    let roominess = |p: V2| lat.iter().filter(|q| q.dist(p) <= 7.0).count() as f32;

    if recipe.placement == Preference::Patrol {
        return place_patrol(
            nav,
            view,
            recipe,
            &lat,
            &inset,
            f,
            &staged,
            &clear_of_others,
            &off_hazards,
        );
    }

    let mut best: Option<(f32, Choice)> = None;
    for &anchor in &lat {
        let members = formation(recipe, anchor, f);
        if !members
            .iter()
            .all(|(t, p)| on(&inset, *p) && staged(*p, t) && clear_of_others(*p) && off_hazards(*p))
        {
            continue;
        }
        // Not in a corridor mouth (leave the doorways passable).
        if members
            .iter()
            .any(|(_, p)| view.other_mouths.iter().any(|m| p.dist(*m) < 2.5))
        {
            continue;
        }
        let pref = match recipe.placement {
            Preference::Far | Preference::Patrol => anchor.dist(view.entry_mouth),
            Preference::Center => -anchor.dist(room.mid()) * 2.0,
            Preference::GuardExit => match view
                .other_mouths
                .iter()
                .map(|m| anchor.dist(*m))
                .min_by(f32::total_cmp)
            {
                Some(d) => -d * 2.0,
                None => anchor.dist(view.entry_mouth),
            },
        };
        let score = pref + 0.12 * roominess(anchor);
        if best.as_ref().is_some_and(|(b, _)| score <= *b + 1e-4) {
            continue;
        }
        // The expensive check last.
        if recipe.space.ranged_lane && !has_ranged_lane(nav, view, &lat, anchor) {
            continue;
        }
        best = Some((
            score,
            Choice {
                spots: members
                    .into_iter()
                    .map(|(template, pos)| Spot {
                        template,
                        pos,
                        patrol: Vec::new(),
                    })
                    .collect(),
                anchor,
                toward_entry: f,
            },
        ));
    }
    best.map(|(_, c)| c)
}

/// A patrol walks back and forth across the room, perpendicular to the way
/// in, in single file.
#[allow(clippy::too_many_arguments)]
fn place_patrol(
    nav: &Nav,
    view: &RoomView,
    recipe: &Recipe,
    lat: &[V2],
    inset: &[Poly],
    f: V2,
    staged: &dyn Fn(V2, &str) -> bool,
    clear_of_others: &dyn Fn(V2) -> bool,
    off_hazards: &dyn Fn(V2) -> bool,
) -> Option<Choice> {
    let room = view.room;
    let v = V2::new(-f.z, f.x);
    let want = recipe.patrol.map(|p| p.length).unwrap_or(12.0);
    let members: Vec<String> = recipe
        .mobs
        .iter()
        .flat_map(|m| std::iter::repeat_n(m.template.clone(), m.count as usize))
        .collect();
    let mut best: Option<(f32, Choice)> = None;
    for &mid in lat {
        // Longest route along `v` through `mid` that stays on the floor.
        let mut half = want / 2.0;
        while half >= 3.0 {
            let (a, b) = (mid - v * half, mid + v * half);
            if (0..=10).all(|k| on(inset, a + (b - a) * (k as f32 / 10.0))) {
                break;
            }
            half -= 1.0;
        }
        if half < 3.0 || 2.0 * half < 1.8 * (members.len() as f32 - 1.0) + 4.0 {
            continue;
        }
        let (a, b) = (mid - v * half, mid + v * half);
        // Single file behind the leader, along the route.
        let file: Vec<V2> = (0..members.len())
            .map(|k| a + v * (1.8 * k as f32).min(half))
            .collect();
        let samples: Vec<V2> = (0..=20).map(|k| a + (b - a) * (k as f32 / 20.0)).collect();
        if !samples.iter().all(|p| {
            members.iter().all(|t| staged(*p, t)) && clear_of_others(*p) && off_hazards(*p)
        }) {
            continue;
        }
        if !file.iter().all(|p| on(inset, *p)) {
            continue;
        }
        let score = mid.dist(view.entry_mouth) * 0.5 + half * 2.0;
        if best.as_ref().is_some_and(|(s, _)| score <= *s + 1e-4) {
            continue;
        }
        // Each leg must be a real route on the navigation.
        let y = room.y;
        let walkable = nav
            .locate(a, y)
            .is_some_and(|s| nav.path_points(a, s, V3::at(b, y)).is_some());
        if !walkable {
            continue;
        }
        // Each member turns at its own place in the file, so the column keeps
        // its spacing at both ends of the route.
        let n = members.len();
        let spots = members
            .iter()
            .zip(&file)
            .enumerate()
            .map(|(k, (t, p))| Spot {
                template: t.clone(),
                pos: *p,
                patrol: vec![b - v * (1.8 * (n - 1 - k) as f32).min(half)],
            })
            .collect();
        best = Some((
            score,
            Choice {
                spots,
                anchor: mid,
                toward_entry: f,
            },
        ));
    }
    best.map(|(_, c)| c)
}
