//! Topology first, geometry second: the main route, alternate route, side
//! rooms and shortcut are laid out as a graph on a grid of cells (bounded,
//! seeded search), then given heights and corridor styles. `compose` turns
//! the resulting `Plan` into walkable areas.

use crate::blueprint::{Blueprint, Shortcut};
use crate::kit::{Kit, RoomPref};
use dungeon_sim::math::V2;
use dungeon_sim::rng::Rng;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Distance between neighbouring cell centres (m).
pub const CELL: f32 = 36.0;
/// Corridors overlap the rooms they join by this much (a level overlap, or a
/// ramp end; both well under the 2 m ramp-overlap limit).
pub const OVERLAP: f32 = 1.5;
/// Steepest ramp the builder makes (rise/run); the navigation allows 0.6.
pub const RAMP_SLOPE: f32 = 0.3;
/// A drop must be clearly a drop and clear the lower room's 3.2 m walls.
pub const MIN_FALL: f32 = 3.6;
pub const MAX_FALL: f32 = 7.5;
/// Generation attempts before giving up with a diagnosis.
pub const MAX_ATTEMPTS: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Dir {
    N,
    E,
    S,
    W,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::N, Dir::E, Dir::S, Dir::W];

    pub fn step(self) -> (i32, i32) {
        match self {
            Dir::N => (0, 1),
            Dir::E => (1, 0),
            Dir::S => (0, -1),
            Dir::W => (-1, 0),
        }
    }

    /// Unit vector in world x/z (+z is north).
    pub fn unit(self) -> V2 {
        let (x, z) = self.step();
        V2::new(x as f32, z as f32)
    }

    /// Unit vector 90° to the left of `self` (the corridor's lateral axis).
    pub fn lateral(self) -> V2 {
        let u = self.unit();
        V2::new(-u.z, u.x)
    }

    pub fn opposite(self) -> Dir {
        match self {
            Dir::N => Dir::S,
            Dir::E => Dir::W,
            Dir::S => Dir::N,
            Dir::W => Dir::E,
        }
    }

    pub fn left(self) -> Dir {
        match self {
            Dir::N => Dir::W,
            Dir::W => Dir::S,
            Dir::S => Dir::E,
            Dir::E => Dir::N,
        }
    }

    pub fn right(self) -> Dir {
        self.left().opposite()
    }

    pub fn between(a: (i32, i32), b: (i32, i32)) -> Option<Dir> {
        Dir::ALL
            .into_iter()
            .find(|d| (a.0 + d.step().0, a.1 + d.step().1) == b)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RoomKind {
    Entrance,
    /// Small room (junctions, single elites).
    Chamber,
    /// Large room (packs).
    Hall,
    /// Boss arena.
    Arena,
    /// Ring of floor around a lava pit.
    Pit,
    /// Optional side chamber.
    Side,
    /// Raised ledge at the top of a one-way drop.
    Overlook,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Route {
    /// Position on the main route (0 = entrance).
    Main(usize),
    Alternate,
    Side,
    Overlook,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EncounterRef {
    /// Index into the blueprint's `encounters`.
    Main(usize),
    /// Index into the blueprint's `optional`.
    Optional(usize),
}

#[derive(Debug, Clone, Serialize)]
pub struct Room {
    pub kind: RoomKind,
    pub route: Route,
    pub cell: (i32, i32),
    pub y: f32,
    /// Half-size (apothem) of the footprint.
    pub half: f32,
    /// Corner cut (0 = square; octagons cut the corners).
    pub chamfer: f32,
    /// Half-size of the central lava pit (pit rooms only).
    pub pit: f32,
    pub name: String,
    pub encounter: Option<EncounterRef>,
    /// Direction of travel when the party first enters (packs face it).
    pub entry: Option<Dir>,
}

impl Room {
    pub fn center(&self) -> V2 {
        V2::new(self.cell.0 as f32 * CELL, self.cell.1 as f32 * CELL)
    }

    /// How far a corridor may sit sideways from the room's side midpoint
    /// (its centre line) and still enter through a straight wall without
    /// touching the corridors on the neighbouring sides.
    pub fn lateral_limit(&self) -> f32 {
        if self.chamfer > 0.0 {
            self.half - self.chamfer - 0.6
        } else {
            self.half - 2.0
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Style {
    Straight,
    /// Three pieces: out, sideways, on (an S-bend).
    Winding,
    /// One slanted piece (a convex parallelogram).
    Angled,
    Ramp,
    /// Open deck over a lava chasm.
    Bridge,
    /// Raised walkway ending at the lower room's edge; one-way drop.
    Drop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EdgeRole {
    Main,
    Alternate,
    Side,
    /// Room → overlook (walk) and overlook → lower room (drop).
    Shortcut,
    /// Walkable return between non-consecutive main rooms.
    Loop,
}

#[derive(Debug, Clone, Serialize)]
pub struct Corridor {
    pub a: usize,
    pub b: usize,
    /// Direction from `a` to `b`.
    pub dir: Dir,
    pub style: Style,
    pub role: EdgeRole,
    pub width: f32,
    /// Lateral offsets of the two ends from the rooms' side midpoints.
    pub off_a: f32,
    pub off_b: f32,
    /// Draw a ramp as steps.
    pub stairs: bool,
}

impl Corridor {
    pub fn walkable_both_ways(&self) -> bool {
        self.style != Style::Drop
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub seed: u64,
    pub attempt: u32,
    pub rooms: Vec<Room>,
    pub corridors: Vec<Corridor>,
    /// Main-route room indices in order (entrance first, final arena last).
    pub main: Vec<usize>,
}

impl Plan {
    /// Edge-to-edge gap between two neighbouring rooms.
    pub fn gap(&self, a: usize, b: usize) -> f32 {
        CELL - self.rooms[a].half - self.rooms[b].half
    }

    /// Largest height change one ramp corridor between `a` and `b` can take.
    pub fn max_rise(&self, a: usize, b: usize) -> f32 {
        // Floor to 0.5 m so heights stay round numbers.
        ((self.gap(a, b) + 2.0 * OVERLAP) * RAMP_SLOPE * 2.0).floor() / 2.0
    }
}

/// Why attempts failed (for the diagnosis when all of them do).
#[derive(Debug, Default, Clone)]
pub struct FailureTally {
    pub counts: BTreeMap<&'static str, u32>,
}

impl FailureTally {
    pub fn add(&mut self, what: &'static str) {
        *self.counts.entry(what).or_default() += 1;
    }
}

fn room_kind_for(pref: RoomPref) -> RoomKind {
    match pref {
        RoomPref::Chamber => RoomKind::Chamber,
        RoomPref::Hall => RoomKind::Hall,
        RoomPref::Arena => RoomKind::Arena,
        RoomPref::Pit => RoomKind::Pit,
    }
}

fn shuffle<T>(rng: &mut Rng, v: &mut [T]) {
    for i in (1..v.len()).rev() {
        let j = rng.index(i + 1);
        v.swap(i, j);
    }
}

fn round_half(v: f32) -> f32 {
    (v * 2.0).round() / 2.0
}

fn new_room(rng: &mut Rng, kind: RoomKind, route: Route, cell: (i32, i32)) -> Room {
    let (half, chamfer_f, pit) = match kind {
        RoomKind::Entrance => (round_half(rng.range_f32(7.0, 8.5)), 0.0, 0.0),
        RoomKind::Chamber => (
            round_half(rng.range_f32(8.0, 9.5)),
            if rng.chance(0.5) { 0.3 } else { 0.0 },
            0.0,
        ),
        RoomKind::Hall => (round_half(rng.range_f32(11.0, 12.5)), 0.3, 0.0),
        RoomKind::Arena => (round_half(rng.range_f32(13.0, 14.0)), 0.45, 0.0),
        RoomKind::Pit => {
            let h = round_half(rng.range_f32(11.0, 12.5));
            (h, 0.0, round_half(rng.range_f32(3.5, 4.5)))
        }
        RoomKind::Side => (round_half(rng.range_f32(7.0, 8.5)), 0.0, 0.0),
        RoomKind::Overlook => (5.0, 0.0, 0.0),
    };
    Room {
        kind,
        route,
        cell,
        y: 0.0,
        half,
        chamfer: (half * chamfer_f * 2.0).round() / 2.0,
        pit,
        name: String::new(),
        encounter: None,
        entry: None,
    }
}

fn add(c: (i32, i32), d: Dir) -> (i32, i32) {
    (c.0 + d.step().0, c.1 + d.step().1)
}

/// One layout attempt. Each attempt has its own sub-seed, so a failed attempt
/// never changes what the next one does.
pub fn plan_attempt(
    bp: &Blueprint,
    kit: &Kit,
    seed: u64,
    attempt: u32,
) -> Result<Plan, &'static str> {
    let mut rng =
        Rng::new(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ ((attempt as u64) << 32 | 0xF0F0));
    let mut p = try_plan(bp, kit, &mut rng)?;
    p.seed = seed;
    p.attempt = attempt;
    Ok(p)
}

/// Suggestions for the causes that made attempts fail.
pub fn advice(t: &FailureTally) -> String {
    let mut tips = Vec::new();
    for k in t.counts.keys() {
        tips.push(match *k {
            "main route boxed in" => "the main route is long; use fewer extra_rooms or encounters",
            "alternate route" => "no room for a second way; set alternate_route = false or use fewer optional rooms",
            "optional room" => "no free cell beside the route for a side room; use fewer optional encounters",
            "shortcut" => "no cell touches an earlier and a later room; try another seed or shortcut = \"none\"",
            "heights" => "the height changes do not fit the corridors; use fewer descents",
            "validation" => "the composed dungeon failed validation (see the first error); this is a generator bug worth reporting with the blueprint and seed",
            _ => "try another seed",
        });
    }
    tips.dedup();
    if tips.is_empty() {
        String::new()
    } else {
        format!("Suggestions: {}.", tips.join("; "))
    }
}

fn try_plan(bp: &Blueprint, kit: &Kit, rng: &mut Rng) -> Result<Plan, &'static str> {
    let l = &bp.layout;
    // ---- Main route kinds: entrance, encounters with extra rooms between.
    let extras =
        l.extra_rooms[0] + rng.index((l.extra_rooms[1] - l.extra_rooms[0] + 1) as usize) as u32;
    let e = bp.encounters.len();
    // Extra room before encounter k (k in 0..e); several may share a slot.
    let mut before: Vec<u32> = vec![0; e];
    for _ in 0..extras {
        before[rng.index(e)] += 1;
    }
    let mut kinds: Vec<(RoomKind, Option<EncounterRef>)> = vec![(RoomKind::Entrance, None)];
    for (k, slot) in bp.encounters.iter().enumerate() {
        for _ in 0..before[k] {
            kinds.push((RoomKind::Chamber, None));
        }
        let pref = kit.recipes[&slot.recipe].room;
        kinds.push((room_kind_for(pref), Some(EncounterRef::Main(k))));
    }
    let n = kinds.len();

    // ---- Main route: a self-avoiding walk with a forward bias.
    let mut used: BTreeSet<(i32, i32)> = BTreeSet::new();
    let mut cells = vec![(0, 0)];
    used.insert((0, 0));
    let mut heading = Dir::N;
    for i in 1..n {
        let cur = cells[i - 1];
        let mut options: Vec<(Dir, u32)> = Vec::new();
        for (d, w) in [(heading, 3), (heading.left(), 2), (heading.right(), 2)] {
            let c = add(cur, d);
            if used.contains(&c) {
                continue;
            }
            // Don't walk into a dead end (unless this is the last room).
            let free_after = Dir::ALL
                .iter()
                .filter(|dd| {
                    let cc = add(c, **dd);
                    cc != cur && !used.contains(&cc)
                })
                .count();
            if i + 1 < n && free_after == 0 {
                continue;
            }
            options.push((d, w));
        }
        if options.is_empty() {
            return Err("main route boxed in");
        }
        let total: u32 = options.iter().map(|o| o.1).sum();
        let mut pick = rng.index(total as usize) as u32;
        let mut dir = options[0].0;
        for (d, w) in &options {
            if pick < *w {
                dir = *d;
                break;
            }
            pick -= w;
        }
        let c = add(cur, dir);
        cells.push(c);
        used.insert(c);
        heading = dir;
    }

    let mut rooms: Vec<Room> = Vec::new();
    for (i, (kind, enc)) in kinds.iter().enumerate() {
        let mut r = new_room(rng, *kind, Route::Main(i), cells[i]);
        r.encounter = *enc;
        if i > 0 {
            r.entry = Dir::between(cells[i - 1], cells[i]);
        }
        rooms.push(r);
    }
    let main: Vec<usize> = (0..n).collect();
    let mut corridors: Vec<Corridor> = Vec::new();
    let link = |a: usize, b: usize, role: EdgeRole, rooms: &[Room]| -> Corridor {
        Corridor {
            a,
            b,
            dir: Dir::between(rooms[a].cell, rooms[b].cell).expect("neighbouring cells"),
            style: Style::Straight,
            role,
            width: 5.0,
            off_a: 0.0,
            off_b: 0.0,
            stairs: false,
        }
    };
    for i in 1..n {
        corridors.push(link(i - 1, i, EdgeRole::Main, &rooms));
    }

    // ---- Alternate route: BFS through free cells between rooms i and j.
    let mut alt: Option<(usize, usize, Vec<usize>)> = None;
    if l.alternate_route {
        let mut cands: Vec<(usize, usize)> = Vec::new();
        for i in 0..n {
            for gap in [2usize, 3] {
                let j = i + gap;
                // A second way around trash, never around a boss.
                if j <= n.saturating_sub(2) && !skips_boss(bp, kit, &kinds, i, j) {
                    cands.push((i, j));
                }
            }
        }
        shuffle(rng, &mut cands);
        for (i, j) in cands {
            if let Some(path) = free_path(&used, cells[i], cells[j], 3) {
                let mut ids = Vec::new();
                let mut prev = i;
                for (k, c) in path.iter().enumerate() {
                    used.insert(*c);
                    let mut r = new_room(rng, RoomKind::Chamber, Route::Alternate, *c);
                    r.entry = Dir::between(if k == 0 { cells[i] } else { path[k - 1] }, *c);
                    rooms.push(r);
                    let id = rooms.len() - 1;
                    corridors.push(link(prev, id, EdgeRole::Alternate, &rooms));
                    ids.push(id);
                    prev = id;
                }
                corridors.push(link(prev, j, EdgeRole::Alternate, &rooms));
                alt = Some((i, j, ids));
                break;
            }
        }
        if alt.is_none() {
            return Err("alternate route");
        }
    }

    // ---- Optional side rooms off the main route (not the final arena).
    for (k, slot) in bp.optional.iter().enumerate() {
        let mut hosts: Vec<usize> = (1..n.saturating_sub(1)).collect();
        if hosts.is_empty() {
            hosts.push(0);
        }
        shuffle(rng, &mut hosts);
        let mut placed = false;
        'host: for h in hosts {
            let mut dirs = Dir::ALL.to_vec();
            shuffle(rng, &mut dirs);
            for d in dirs {
                let c = add(cells[h], d);
                if used.contains(&c) {
                    continue;
                }
                used.insert(c);
                let pref = kit.recipes[&slot.recipe].room;
                let kind = match pref {
                    RoomPref::Chamber => RoomKind::Side,
                    other => room_kind_for(other),
                };
                let mut r = new_room(rng, kind, Route::Side, c);
                r.encounter = Some(EncounterRef::Optional(k));
                r.entry = Some(d);
                rooms.push(r);
                let id = rooms.len() - 1;
                corridors.push(link(h, id, EdgeRole::Side, &rooms));
                placed = true;
                break 'host;
            }
        }
        if !placed {
            return Err("optional room");
        }
    }

    // ---- Shortcut.
    let mut drop_plan: Option<(usize, usize, usize)> = None; // (from main i, overlook, to main j)
    let mut loop_plan: Option<(usize, usize)> = None;
    match l.shortcut {
        Shortcut::None => {}
        Shortcut::Drop => {
            let mut cands: Vec<(usize, usize, (i32, i32))> = Vec::new();
            for i in 0..n {
                // Skip at most two rooms, and never a boss.
                for j in (i + 2)..n.saturating_sub(1).min(i + 4) {
                    if skips_boss(bp, kit, &kinds, i, j) {
                        continue;
                    }
                    for d in Dir::ALL {
                        let c = add(cells[i], d);
                        if !used.contains(&c) && Dir::between(c, cells[j]).is_some() {
                            cands.push((i, j, c));
                        }
                    }
                }
            }
            if cands.is_empty() {
                return Err("shortcut");
            }
            let (i, j, c) = cands[rng.index(cands.len())];
            used.insert(c);
            let mut r = new_room(rng, RoomKind::Overlook, Route::Overlook, c);
            r.entry = Dir::between(cells[i], c);
            rooms.push(r);
            let o = rooms.len() - 1;
            corridors.push(link(i, o, EdgeRole::Shortcut, &rooms));
            let mut drop = link(o, j, EdgeRole::Shortcut, &rooms);
            drop.style = Style::Drop;
            drop.width = 4.0;
            corridors.push(drop);
            drop_plan = Some((i, o, j));
        }
        Shortcut::Loop => {
            let mut cands: Vec<(usize, usize)> = Vec::new();
            for i in 0..n {
                for j in (i + 3)..n.saturating_sub(1) {
                    if Dir::between(cells[i], cells[j]).is_some()
                        && !skips_boss(bp, kit, &kinds, i, j)
                    {
                        cands.push((i, j));
                    }
                }
            }
            if cands.is_empty() {
                return Err("shortcut");
            }
            let (i, j) = cands[rng.index(cands.len())];
            corridors.push(link(j, i, EdgeRole::Loop, &rooms));
            loop_plan = Some((i, j));
        }
    }

    let mut plan = Plan {
        seed: 0,
        attempt: 0,
        rooms,
        corridors,
        main,
    };

    // ---- Heights and corridor styles.
    assign_heights(&mut plan, bp, rng, alt.as_ref(), drop_plan, loop_plan)?;
    assign_styles(&mut plan, bp, rng);
    name_rooms(&mut plan, kit, rng);
    Ok(plan)
}

/// Does going straight from main room `i` to `j` bypass a boss room?
fn skips_boss(
    bp: &Blueprint,
    kit: &Kit,
    kinds: &[(RoomKind, Option<EncounterRef>)],
    i: usize,
    j: usize,
) -> bool {
    kinds[i + 1..j].iter().any(|(_, e)| {
        matches!(e, Some(EncounterRef::Main(k)) if kit.recipe_has_boss(&bp.encounters[*k].recipe))
    })
}

/// Shortest path (1..=max cells) of free cells from a neighbour of `from` to
/// a neighbour of `to`. Deterministic: neighbours in fixed order.
fn free_path(
    used: &BTreeSet<(i32, i32)>,
    from: (i32, i32),
    to: (i32, i32),
    max: usize,
) -> Option<Vec<(i32, i32)>> {
    let mut prev: BTreeMap<(i32, i32), Option<(i32, i32)>> = BTreeMap::new();
    let mut q = VecDeque::new();
    for d in Dir::ALL {
        let c = add(from, d);
        if !used.contains(&c) {
            prev.insert(c, None);
            q.push_back((c, 1usize));
        }
    }
    while let Some((c, len)) = q.pop_front() {
        if Dir::between(c, to).is_some() {
            let mut path = vec![c];
            let mut cur = c;
            while let Some(Some(p)) = prev.get(&cur) {
                path.push(*p);
                cur = *p;
            }
            path.reverse();
            return Some(path);
        }
        if len == max {
            continue;
        }
        for d in Dir::ALL {
            let nc = add(c, d);
            if used.contains(&nc) || prev.contains_key(&nc) || nc == from || nc == to {
                continue;
            }
            prev.insert(nc, Some(c));
            q.push_back((nc, len + 1));
        }
    }
    None
}

fn corridor_between(plan: &Plan, a: usize, b: usize) -> Option<usize> {
    plan.corridors
        .iter()
        .position(|c| (c.a == a && c.b == b) || (c.a == b && c.b == a))
}

fn assign_heights(
    plan: &mut Plan,
    bp: &Blueprint,
    rng: &mut Rng,
    alt: Option<&(usize, usize, Vec<usize>)>,
    drop_plan: Option<(usize, usize, usize)>,
    loop_plan: Option<(usize, usize)>,
) -> Result<(), &'static str> {
    let n = plan.main.len();
    // Main-route descents on distinct edges.
    let mut edges: Vec<usize> = (1..n).collect(); // edge into main room k
    shuffle(rng, &mut edges);
    let descents: BTreeSet<usize> = edges
        .iter()
        .copied()
        .take(bp.layout.descents as usize)
        .collect();
    let mut y = 0.0f32;
    plan.rooms[0].y = 0.0;
    for k in 1..n {
        if descents.contains(&k) {
            let max = plan.max_rise(k - 1, k);
            let dh = round_half(rng.range_f32(2.5, 3.5)).min(max);
            if dh < 2.0 {
                return Err("heights");
            }
            y -= dh;
        }
        plan.rooms[k].y = y;
    }
    // Alternate route: spread the height change over its corridors.
    if let Some((i, j, ids)) = alt {
        let (yi, yj) = (plan.rooms[*i].y, plan.rooms[*j].y);
        let steps = ids.len() + 1;
        for (t, &id) in ids.iter().enumerate() {
            plan.rooms[id].y = round_half(yi + (yj - yi) * (t + 1) as f32 / steps as f32);
        }
        let chain: Vec<usize> = std::iter::once(*i)
            .chain(ids.iter().copied())
            .chain(std::iter::once(*j))
            .collect();
        for w in chain.windows(2) {
            if (plan.rooms[w[0]].y - plan.rooms[w[1]].y).abs() > plan.max_rise(w[0], w[1]) {
                return Err("heights");
            }
        }
    }
    // Side rooms sit level with their host.
    for c in plan.corridors.clone() {
        if c.role == EdgeRole::Side {
            plan.rooms[c.b].y = plan.rooms[c.a].y;
        }
    }
    // Overlook: raised so the fall into the later room is a clear drop.
    if let Some((i, o, j)) = drop_plan {
        let d = plan.rooms[i].y - plan.rooms[j].y;
        let up = if d >= MIN_FALL {
            0.0
        } else {
            (MIN_FALL - d) * 2.0
        };
        let up = (up.ceil() / 2.0).max(0.0);
        let yo = plan.rooms[i].y + up;
        let fall = yo - plan.rooms[j].y;
        if !(MIN_FALL..=MAX_FALL).contains(&fall) || up > plan.max_rise(i, o) {
            return Err("heights");
        }
        plan.rooms[o].y = yo;
    }
    if let Some((i, j)) = loop_plan
        && (plan.rooms[i].y - plan.rooms[j].y).abs() > plan.max_rise(i, j)
    {
        return Err("heights");
    }
    let _ = corridor_between;
    Ok(())
}

fn assign_styles(plan: &mut Plan, bp: &Blueprint, rng: &mut Rng) {
    let l = &bp.layout;
    // Bridges on flat main-route corridors.
    let mut flat_main: Vec<usize> = plan
        .corridors
        .iter()
        .enumerate()
        .filter(|(_, c)| c.role == EdgeRole::Main && plan.rooms[c.a].y == plan.rooms[c.b].y)
        .map(|(k, _)| k)
        .collect();
    shuffle(rng, &mut flat_main);
    let bridges: BTreeSet<usize> = flat_main.into_iter().take(l.bridges as usize).collect();
    for k in 0..plan.corridors.len() {
        let c = plan.corridors[k].clone();
        if c.style == Style::Drop {
            continue;
        }
        let (ra, rb) = (&plan.rooms[c.a], &plan.rooms[c.b]);
        let level = ra.y == rb.y;
        let width = match c.role {
            EdgeRole::Main | EdgeRole::Loop => round_half(rng.range_f32(5.0, 6.0)),
            EdgeRole::Alternate => round_half(rng.range_f32(4.5, 5.5)),
            EdgeRole::Side | EdgeRole::Shortcut => 4.5,
        };
        let limit = ra.lateral_limit().min(rb.lateral_limit());
        let room_for_offset = limit - width / 2.0;
        let mut style = if !level {
            Style::Ramp
        } else if bridges.contains(&k) {
            Style::Bridge
        } else if matches!(
            c.role,
            EdgeRole::Main | EdgeRole::Alternate | EdgeRole::Loop
        ) && rng.chance(l.winding)
        {
            if rng.chance(0.5) {
                Style::Winding
            } else {
                Style::Angled
            }
        } else {
            Style::Straight
        };
        let (mut off_a, mut off_b) = (0.0, 0.0);
        match style {
            Style::Winding => {
                // The jog must clear the corridor's own width.
                let need = width / 2.0 + 0.6;
                if room_for_offset >= need {
                    let k = round_half(rng.range_f32(need, room_for_offset));
                    let s = if rng.chance(0.5) { 1.0 } else { -1.0 };
                    off_a = -k * s;
                    off_b = k * s;
                } else {
                    style = Style::Straight;
                }
            }
            Style::Angled => {
                if room_for_offset >= 1.5 {
                    let k = round_half(rng.range_f32(1.5, room_for_offset));
                    let s = if rng.chance(0.5) { 1.0 } else { -1.0 };
                    off_a = -k * s;
                    off_b = k * s;
                } else {
                    style = Style::Straight;
                }
            }
            _ => {}
        }
        let cc = &mut plan.corridors[k];
        cc.style = style;
        cc.width = width;
        cc.off_a = off_a;
        cc.off_b = off_b;
        cc.stairs = style == Style::Ramp && rng.chance(0.4);
    }
}

fn name_rooms(plan: &mut Plan, kit: &Kit, rng: &mut Rng) {
    let names = &kit.palette.names;
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for r in plan.rooms.iter_mut() {
        let pool = match r.kind {
            RoomKind::Entrance => &names.entrance,
            RoomKind::Chamber => &names.chamber,
            RoomKind::Hall => &names.hall,
            RoomKind::Arena => &names.arena,
            RoomKind::Pit => &names.pit,
            RoomKind::Side => &names.side,
            RoomKind::Overlook => &names.overlook,
        };
        let mut free: Vec<&String> = pool.iter().filter(|n| !taken.contains(*n)).collect();
        let name = if free.is_empty() {
            let base = &pool[rng.index(pool.len())];
            (2..)
                .map(|k| format!("{base} {k}"))
                .find(|n| !taken.contains(n))
                .expect("unbounded")
        } else {
            let i = rng.index(free.len());
            free.swap_remove(i).clone()
        };
        taken.insert(name.clone());
        r.name = name;
    }
}
