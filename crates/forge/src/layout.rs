//! Topology first, geometry second: the main route, alternate routes, side
//! rooms and the shortcut are laid out as a graph on a grid of cells
//! (bounded, seeded search), then rooms get their shapes from the kit, the
//! route gets heights, and corridors get styles and offsets that fit the
//! rooms they join. `compose` turns the resulting `Plan` into walkable areas.

use crate::blueprint::{Blueprint, OptionalHome, RoutePurpose, Shortcut, SidePurpose};
use crate::kit::{FlankRating, Kit, Preference, RoomPref, ShapeSpec};
use dungeon_sim::math::V2;
use dungeon_sim::rng::Rng;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Corridors overlap the rooms they join by this much (a level overlap, or a
/// ramp end; both well under the 2 m ramp-overlap limit).
pub const OVERLAP: f32 = 1.5;
/// Steepest ramp the builder makes (rise/run); the navigation allows 0.6.
pub const RAMP_SLOPE: f32 = 0.3;
/// A drop must be clearly a drop and clear the lower room's 3.2 m walls.
pub const MIN_FALL: f32 = 3.6;
pub const MAX_FALL: f32 = 7.5;
/// Generation attempts before giving up with a diagnosis.
pub const MAX_ATTEMPTS: u32 = 512;
/// Shortest corridor between two rooms (edge to edge).
pub const MIN_GAP: f32 = 7.0;

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
        self.left().unit()
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

    pub fn index(self) -> usize {
        match self {
            Dir::N => 0,
            Dir::E => 1,
            Dir::S => 2,
            Dir::W => 3,
        }
    }

    pub fn between(a: (i32, i32), b: (i32, i32)) -> Option<Dir> {
        Dir::ALL
            .into_iter()
            .find(|d| (a.0 + d.step().0, a.1 + d.step().1) == b)
    }
}

/// Index of the corner between two perpendicular directions:
/// 0 = NE, 1 = NW, 2 = SW, 3 = SE.
pub fn corner(a: Dir, b: Dir) -> usize {
    let north = a == Dir::N || b == Dir::N;
    let east = a == Dir::E || b == Dir::E;
    match (north, east) {
        (true, true) => 0,
        (true, false) => 1,
        (false, false) => 2,
        (false, true) => 3,
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
    /// Side chamber (optional encounters, or empty for exploration).
    Side,
    /// Raised ledge at the top of a one-way drop.
    Overlook,
    /// A passage elbow that lengthens a winding route.
    Bend,
    /// A treasure cache's room (0.9).
    Vault,
    /// A landmark room (0.9): decoration and light, no reward.
    Vista,
}

/// Why a branch exists (0.9): a blueprint purpose, or a legacy/structural
/// kind for the audit and the viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum BranchKind {
    Detour,
    Flank,
    Safe,
    Optional,
    Cache,
    Vista,
    Quiet,
    /// The return-loop shortcut.
    Loop,
    /// The overlook and its one-way drop.
    Drop,
    /// A legacy alternate route or empty side room (no declared purpose).
    Unspecified,
}

impl BranchKind {
    pub fn label(self) -> &'static str {
        match self {
            BranchKind::Detour => "detour",
            BranchKind::Flank => "flank",
            BranchKind::Safe => "safe",
            BranchKind::Optional => "optional",
            BranchKind::Cache => "cache",
            BranchKind::Vista => "vista",
            BranchKind::Quiet => "quiet",
            BranchKind::Loop => "loop",
            BranchKind::Drop => "drop",
            BranchKind::Unspecified => "unspecified",
        }
    }

    /// Branches that leave the main route and come back to it.
    pub fn through(self) -> bool {
        matches!(
            self,
            BranchKind::Detour
                | BranchKind::Flank
                | BranchKind::Safe
                | BranchKind::Loop
                | BranchKind::Drop
                | BranchKind::Unspecified
        )
    }
}

/// Every [`BranchKind::label`].
pub const BRANCH_LABELS: &[&str] = &[
    "detour",
    "flank",
    "safe",
    "optional",
    "cache",
    "vista",
    "quiet",
    "loop",
    "drop",
    "unspecified",
];

/// A branch off the main route and what it is for (0.9).
#[derive(Debug, Clone, Serialize)]
pub struct Branch {
    pub kind: BranchKind,
    /// Main room it leaves from.
    pub from: usize,
    /// Main room it rejoins (through routes); `None` for dead ends.
    pub to: Option<usize>,
    /// Its rooms, in order from `from`.
    pub rooms: Vec<usize>,
    /// Its corridors, in order from `from`.
    pub corridors: Vec<usize>,
    /// The optional encounter it holds (index into the blueprint's `optional`).
    pub optional: Option<usize>,
    /// The room holding its treasure cache.
    pub cache: Option<usize>,
    /// Flank: the main room whose pack it approaches.
    pub target: Option<usize>,
    /// Safe: the main corridors (the short way) that carry route hazards.
    pub hazard_corridors: Vec<usize>,
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Shape {
    Plain,
    /// Pit half-size.
    Pit(f32),
    /// Pillar side and spacing.
    Pillars {
        size: f32,
        spacing: f32,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct Room {
    pub kind: RoomKind,
    pub route: Route,
    pub cell: (i32, i32),
    /// Cell centre in world space.
    pub center: V2,
    pub y: f32,
    /// Distance from the cell centre to the wall on each side (N, E, S, W).
    pub ext: [f32; 4],
    /// Corner cuts (NE, NW, SW, SE); 0 = square corner.
    pub cut: [f32; 4],
    pub shape: Shape,
    pub name: String,
    pub encounter: Option<EncounterRef>,
    /// Direction of travel when the party first enters (packs face it).
    pub entry: Option<Dir>,
    /// The branch it belongs to (0.9), if any.
    pub branch: Option<usize>,
    /// Holds a treasure cache (0.9).
    pub cache: bool,
}

impl Room {
    pub fn center(&self) -> V2 {
        self.center
    }

    pub fn ext(&self, d: Dir) -> f32 {
        self.ext[d.index()]
    }

    /// The smallest half-size (how much room there is at least).
    pub fn min_ext(&self) -> f32 {
        self.ext.iter().copied().fold(f32::INFINITY, f32::min)
    }

    /// The middle of the floor (cell centre shifted by uneven extents).
    pub fn mid(&self) -> V2 {
        self.center
            + V2::new(
                (self.ext(Dir::E) - self.ext(Dir::W)) / 2.0,
                (self.ext(Dir::N) - self.ext(Dir::S)) / 2.0,
            )
    }

    /// Where on side `side` a corridor may sit, as an interval along the
    /// lateral direction `v` (perpendicular to `side`), measured from the
    /// cell centre: the straight part of that wall, kept clear of the corner
    /// cuts and of corridors on the neighbouring sides.
    pub fn span(&self, side: Dir, v: Dir) -> (f32, f32) {
        let margin = |c: f32| if c >= 1.0 { 0.6 } else { 2.0 };
        let c_hi = self.cut[corner(side, v)];
        let c_lo = self.cut[corner(side, v.opposite())];
        (
            -(self.ext(v.opposite()) - c_lo - margin(c_lo)),
            self.ext(v) - c_hi - margin(c_hi),
        )
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
    /// Open deck over a chasm (lava or a void, per kit).
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
    /// Lateral offsets of the two ends from the cell centre line (along
    /// `dir.lateral()`).
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
    /// Distance between neighbouring cell centres (from the kit).
    pub cell: f32,
    pub rooms: Vec<Room>,
    pub corridors: Vec<Corridor>,
    /// Main-route room indices in order (entrance first, final arena last).
    pub main: Vec<usize>,
    /// Every branch off the main route and its purpose (0.9).
    pub branches: Vec<Branch>,
}

impl Plan {
    /// Edge-to-edge gap between two neighbouring rooms along `a -> b`.
    pub fn gap(&self, a: usize, b: usize) -> f32 {
        let d = Dir::between(self.rooms[a].cell, self.rooms[b].cell).expect("neighbours");
        self.cell - self.rooms[a].ext(d) - self.rooms[b].ext(d.opposite())
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
    pub counts: BTreeMap<String, u32>,
}

impl FailureTally {
    pub fn add(&mut self, what: impl Into<String>) {
        *self.counts.entry(what.into()).or_default() += 1;
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

fn range(rng: &mut Rng, r: [f32; 2]) -> f32 {
    if r[1] > r[0] {
        rng.range_f32(r[0], r[1])
    } else {
        r[0]
    }
}

/// A room shaped by the kit: base size, elongated along the direction of
/// travel `along`, each side jittered, corners cut.
fn new_room(
    rng: &mut Rng,
    kit: &Kit,
    kind: RoomKind,
    route: Route,
    cell: (i32, i32),
    along: Dir,
) -> Room {
    let spec = kit.room_spec(kind);
    let pitch = kit.components.cell;
    let max_half = pitch / 2.0 - 4.0;
    let base = range(rng, spec.size);
    let aspect = range(rng, spec.aspect);
    let mut ext = [0.0f32; 4];
    for d in Dir::ALL {
        let axial = d == along || d == along.opposite();
        let mut e = base * if axial { aspect } else { 1.0 };
        if spec.jitter > 0.0 {
            e *= 1.0 + rng.range_f32(-spec.jitter, spec.jitter);
        }
        ext[d.index()] = round_half(e.clamp(3.0, max_half));
    }
    let mut cut = [0.0f32; 4];
    if spec.shape == ShapeSpec::Plain && spec.cut[1] > 0.0 {
        for (k, (a, b)) in [
            (Dir::N, Dir::E),
            (Dir::N, Dir::W),
            (Dir::S, Dir::W),
            (Dir::S, Dir::E),
        ]
        .into_iter()
        .enumerate()
        {
            let small = ext[a.index()].min(ext[b.index()]);
            let c = range(rng, spec.cut) * small;
            cut[k] = round_half(c.min(small - 2.5).max(0.0));
        }
    }
    let shape = match spec.shape {
        ShapeSpec::Plain => Shape::Plain,
        ShapeSpec::Pit => Shape::Pit(round_half(range(rng, spec.pit))),
        ShapeSpec::Pillars => Shape::Pillars {
            size: spec.pillars.size,
            spacing: spec.pillars.spacing,
        },
    };
    Room {
        kind,
        route,
        cell,
        center: V2::new(cell.0 as f32 * pitch, cell.1 as f32 * pitch),
        y: 0.0,
        ext,
        cut,
        shape,
        name: String::new(),
        encounter: None,
        entry: None,
        branch: None,
        cache: false,
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
        let k = k.split(':').next().unwrap_or("");
        tips.push(match k {
            "main route boxed in" => "the main route is long; use fewer extra_rooms, passages or encounters",
            "alternate route" => "no room for a second way; use fewer alternate_routes or optional rooms",
            "optional room" => "no free cell beside the route for a side room; use fewer optional encounters or side_rooms",
            "side room" => "no free cell for an empty side chamber; use fewer side_rooms",
            "detour route" => "no free cells for a reward detour between neighbouring main rooms; use fewer routes or side chambers",
            "flank route" => "no free cells for a flank into an eligible pack's room (non-boss, not a patrol unless targeted); use another target or fewer routes",
            "safe route" => "no free cells for a safe way round a crossing; use fewer routes",
            "shortcut" => "no cell touches an earlier and a later room; try another seed or shortcut = \"none\"",
            "heights" => "the height changes do not fit the corridors; use fewer descents",
            "corridor fit" => "two rooms' walls don't line up for a straight corridor (ramp, bridge or drop); lower the kit's room jitter or use fewer descents and bridges",
            "space" => "an encounter found no spot in its room that satisfies its recipe's space needs; use bigger rooms in the kit or a smaller recipe",
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
    // ---- Main route kinds: entrance, encounters with extra rooms and
    // passage bends between.
    let extras =
        l.extra_rooms[0] + rng.index((l.extra_rooms[1] - l.extra_rooms[0] + 1) as usize) as u32;
    let e = bp.encounters.len();
    // Extra room before encounter k (k in 0..e); several may share a slot.
    let mut before: Vec<u32> = vec![0; e];
    if bp.purposeful() && l.shortcut == Shortcut::Drop {
        // 0.9: one stretch of empty rooms for the drop to skip (a drop past
        // a required fight would only send the party back for it).
        before[rng.index(e)] += extras;
    } else {
        for _ in 0..extras {
            before[rng.index(e)] += 1;
        }
    }
    let mut kinds: Vec<(RoomKind, Option<EncounterRef>)> = vec![(RoomKind::Entrance, None)];
    for (k, slot) in bp.encounters.iter().enumerate() {
        for _ in 0..before[k] {
            if rng.chance(l.passages) {
                kinds.push((RoomKind::Bend, None));
            }
            kinds.push((RoomKind::Chamber, None));
        }
        if rng.chance(l.passages) {
            kinds.push((RoomKind::Bend, None));
        }
        let pref = kit.recipes[&slot.recipe].room;
        kinds.push((room_kind_for(pref), Some(EncounterRef::Main(k))));
    }
    let n = kinds.len();

    // ---- Main route: a self-avoiding walk, straight or winding by blueprint.
    let straight_w = (1.0 + 8.0 * l.straightness) * 10.0;
    let turn_w = (3.0 * (1.0 - l.straightness) + 0.5) * 10.0;
    let mut used: BTreeSet<(i32, i32)> = BTreeSet::new();
    let mut cells = vec![(0, 0)];
    used.insert((0, 0));
    let mut heading = Dir::N;
    for i in 1..n {
        let cur = cells[i - 1];
        let mut options: Vec<(Dir, u32)> = Vec::new();
        for (d, w) in [
            (heading, straight_w),
            (heading.left(), turn_w),
            (heading.right(), turn_w),
        ] {
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
            // A bend must actually turn (otherwise it is just a corridor).
            if kinds[i - 1].0 == RoomKind::Bend && i >= 2 && d == heading {
                continue;
            }
            options.push((d, w.round() as u32));
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
        let entry = if i > 0 {
            Dir::between(cells[i - 1], cells[i])
        } else {
            None
        };
        // Elongate along the way in (or out, for the entrance).
        let along = entry
            .or_else(|| cells.get(1).and_then(|c| Dir::between(cells[0], *c)))
            .unwrap_or(Dir::N);
        let mut r = new_room(rng, kit, *kind, Route::Main(i), cells[i], along);
        r.encounter = *enc;
        r.entry = entry;
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
    let mut branches: Vec<Branch> = Vec::new();
    let mut alts: Vec<(usize, usize, Vec<usize>)> = Vec::new();

    let is_boss_room = |x: usize| matches!(kinds[x].1, Some(EncounterRef::Main(e)) if kit.recipe_has_boss(&bp.encounters[e].recipe));
    let required = |x: usize| matches!(kinds[x].1, Some(EncounterRef::Main(_)));
    // The shortcut (drop or loop). 0.9 blueprints place it before their
    // routes and side rooms, so the cells it needs are still free; legacy
    // blueprints place it last, as before (same layouts for the same seed).
    let mut drop_plan: Option<(usize, usize, usize)> = None; // (from main i, overlook, to main j)
    let mut loop_plan: Option<(usize, usize)> = None;
    macro_rules! shortcut {
        () => {
            match l.shortcut {
                Shortcut::None => {}
                Shortcut::Drop => {
                    let mut cands: Vec<(usize, usize, (i32, i32))> = Vec::new();
                    for i in 0..n {
                        // Skip at most two rooms, and never a boss.
                        // 0.9: skip two or three rooms (one skipped room saves no
                        // walking: the overlook adds a step of its own).
                        let span = if bp.purposeful() {
                            (i + 3)..n.saturating_sub(1).min(i + 5)
                        } else {
                            (i + 2)..n.saturating_sub(1).min(i + 4)
                        };
                        for j in span {
                            if skips_boss(bp, kit, &kinds, i, j) || kinds[j].0 == RoomKind::Bend {
                                continue;
                            }
                            // 0.9: a shortcut, not a way past required fights.
                            if bp.purposeful() && (i + 1..j).any(required) {
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
                    let along = Dir::between(cells[i], c).unwrap_or(Dir::N);
                    let mut r = new_room(rng, kit, RoomKind::Overlook, Route::Overlook, c, along);
                    r.entry = Some(along);
                    r.branch = Some(branches.len());
                    rooms.push(r);
                    let o = rooms.len() - 1;
                    corridors.push(link(i, o, EdgeRole::Shortcut, &rooms));
                    let mut drop = link(o, j, EdgeRole::Shortcut, &rooms);
                    drop.style = Style::Drop;
                    drop.width = 4.0;
                    corridors.push(drop);
                    drop_plan = Some((i, o, j));
                    branches.push(Branch {
                        kind: BranchKind::Drop,
                        from: i,
                        to: Some(j),
                        rooms: vec![o],
                        corridors: vec![corridors.len() - 2, corridors.len() - 1],
                        optional: None,
                        cache: None,
                        target: None,
                        hazard_corridors: Vec::new(),
                    });
                }
                Shortcut::Loop => {
                    let mut cands: Vec<(usize, usize)> = Vec::new();
                    for i in 0..n {
                        for j in (i + 3)..n.saturating_sub(1).min(i + 6) {
                            if !skips_boss(bp, kit, &kinds, i, j)
                                && kinds[j].0 != RoomKind::Bend
                                && kinds[i].0 != RoomKind::Bend
                            {
                                cands.push((i, j));
                            }
                        }
                    }
                    shuffle(rng, &mut cands);
                    // Neighbours join directly; otherwise a cloister walk through up
                    // to five free cells.
                    let direct = cands
                        .iter()
                        .copied()
                        .find(|&(i, j)| Dir::between(cells[i], cells[j]).is_some());
                    if let Some((i, j)) = direct {
                        corridors.push(link(j, i, EdgeRole::Loop, &rooms));
                        loop_plan = Some((i, j));
                        branches.push(Branch {
                            kind: BranchKind::Loop,
                            from: j,
                            to: Some(i),
                            rooms: Vec::new(),
                            corridors: vec![corridors.len() - 1],
                            optional: None,
                            cache: None,
                            target: None,
                            hazard_corridors: Vec::new(),
                        });
                    } else {
                        let mut found = None;
                        for (i, j) in cands {
                            if let Some(path) = free_path(&used, cells[j], cells[i], 5) {
                                found = Some((i, j, path));
                                break;
                            }
                        }
                        let Some((i, j, path)) = found else {
                            return Err("shortcut");
                        };
                        let mut ids = Vec::new();
                        let mut cors = Vec::new();
                        let mut prev = j;
                        let bi = branches.len();
                        for (k, c) in path.iter().enumerate() {
                            used.insert(*c);
                            let entry =
                                Dir::between(if k == 0 { cells[j] } else { path[k - 1] }, *c);
                            let mut r = new_room(
                                rng,
                                kit,
                                RoomKind::Bend,
                                Route::Alternate,
                                *c,
                                entry.unwrap_or(Dir::N),
                            );
                            r.entry = entry;
                            r.branch = Some(bi);
                            rooms.push(r);
                            let id = rooms.len() - 1;
                            corridors.push(link(prev, id, EdgeRole::Loop, &rooms));
                            cors.push(corridors.len() - 1);
                            ids.push(id);
                            prev = id;
                        }
                        corridors.push(link(prev, i, EdgeRole::Loop, &rooms));
                        cors.push(corridors.len() - 1);
                        branches.push(Branch {
                            kind: BranchKind::Loop,
                            from: j,
                            to: Some(i),
                            rooms: ids.clone(),
                            corridors: cors,
                            optional: None,
                            cache: None,
                            target: None,
                            hazard_corridors: Vec::new(),
                        });
                        // Heights along the walk are spread like an alternate route.
                        alts.push((j, i, ids));
                        loop_plan = None;
                    }
                }
            }
        };
    }
    if bp.purposeful() {
        shortcut!();
    }

    // ---- Purposeful routes (0.9). Each is built to deliver its purpose;
    // `check::branches` measures that it does.
    for (k, spec) in bp.routes.iter().enumerate() {
        let mut cands: Vec<(usize, usize)> = Vec::new();
        match spec.purpose {
            RoutePurpose::Detour | RoutePurpose::Safe => {
                for i in 0..n {
                    for j in [i + 1, i + 2] {
                        // Never into the final arena or a boss room, and
                        // never round a room that holds an encounter.
                        if j + 1 >= n || is_boss_room(j) || (i + 1..j).any(|x| kinds[x].1.is_some())
                        {
                            continue;
                        }
                        cands.push((i, j));
                    }
                }
            }
            RoutePurpose::Flank => {
                for (j, kind) in kinds.iter().enumerate().take(n.saturating_sub(1)).skip(1) {
                    let Some(EncounterRef::Main(e)) = kind.1 else {
                        continue;
                    };
                    let recipe = &bp.encounters[e].recipe;
                    if kit.recipe_has_boss(recipe) {
                        continue;
                    }
                    match &spec.target {
                        Some(t) if t != recipe => continue,
                        // Patrols walk across the room; flank them only on request.
                        None if kit.recipes[recipe].placement == Preference::Patrol => continue,
                        // A flank that measurably only costs, likewise (0.9.1).
                        None if kit.recipes[recipe].flank == Some(FlankRating::Poor) => continue,
                        _ => {}
                    }
                    for gap in 1..=3 {
                        let Some(i) = j.checked_sub(gap) else {
                            continue;
                        };
                        if (i + 1..j).any(required) {
                            continue;
                        }
                        cands.push((i, j));
                    }
                }
            }
        }
        cands.retain(|&(i, j)| !branches.iter().any(|b| b.from == i && b.to == Some(j)));
        // Detours and safe ways never open into a patrol's room: the patrol
        // walks its doorway and joins the fight in the branch (measured:
        // the Sentinel Patrol overpulled the Crawler Nest in 6 of 8 runs).
        if spec.purpose != RoutePurpose::Flank {
            let patrols = |x: usize| {
                matches!(kinds[x].1, Some(EncounterRef::Main(e))
                    if kit.recipes[&bp.encounters[e].recipe].placement == Preference::Patrol)
            };
            cands.retain(|&(i, j)| !patrols(i) && !patrols(j));
        }
        // Not in a stretch the drop skips: the party dropping past it would
        // never see the choice (a safe way round vents nobody walks past).
        if let Some((di, _, dj)) = drop_plan {
            cands.retain(|&(i, j)| (i..=j).all(|x| x <= di || x >= dj));
        }
        shuffle(rng, &mut cands);
        if spec.purpose == RoutePurpose::Flank {
            // Packs a flank measurably helps first, then packs it changes;
            // the order among equals stays seeded (0.9.1).
            let tier = |j: usize| match kinds[j].1 {
                Some(EncounterRef::Main(e)) => match kit.recipes[&bp.encounters[e].recipe].flank {
                    Some(FlankRating::Favoured) => 0,
                    Some(FlankRating::Different) | None => 1,
                    Some(FlankRating::Poor) => 2,
                },
                _ => 2,
            };
            cands.sort_by_key(|&(_, j)| tier(j));
        }
        let max_len = if spec.purpose == RoutePurpose::Flank {
            4
        } else {
            3
        };
        let mut found = None;
        for (i, j) in cands {
            if let Some(path) = free_path(&used, cells[i], cells[j], max_len) {
                found = Some((i, j, path));
                break;
            }
        }
        let Some((i, j, path)) = found else {
            return Err(match spec.purpose {
                RoutePurpose::Detour => "detour route",
                RoutePurpose::Flank => "flank route",
                RoutePurpose::Safe => "safe route",
            });
        };
        let bi = branches.len();
        let opt = bp.route_optional(k);
        let reward_at = path.len() / 2;
        let mut ids = Vec::new();
        let mut cors = Vec::new();
        let mut cache = None;
        let mut prev = i;
        for (t, c) in path.iter().enumerate() {
            used.insert(*c);
            let entry = Dir::between(if t == 0 { cells[i] } else { path[t - 1] }, *c);
            let reward = spec.purpose == RoutePurpose::Detour && t == reward_at;
            let kind = if reward {
                match opt.map(|o| kit.recipes[&bp.optional[o].recipe].room) {
                    Some(RoomPref::Hall) => RoomKind::Hall,
                    Some(_) => RoomKind::Chamber,
                    None => RoomKind::Vault,
                }
            } else {
                RoomKind::Bend
            };
            let mut r = new_room(
                rng,
                kit,
                kind,
                Route::Alternate,
                *c,
                entry.unwrap_or(Dir::N),
            );
            r.entry = entry;
            r.branch = Some(bi);
            if reward {
                r.encounter = opt.map(EncounterRef::Optional);
                r.cache = spec.cache;
            }
            rooms.push(r);
            let id = rooms.len() - 1;
            if reward && spec.cache {
                cache = Some(id);
            }
            corridors.push(link(prev, id, EdgeRole::Alternate, &rooms));
            cors.push(corridors.len() - 1);
            ids.push(id);
            prev = id;
        }
        corridors.push(link(prev, j, EdgeRole::Alternate, &rooms));
        cors.push(corridors.len() - 1);
        alts.push((i, j, ids.clone()));
        branches.push(Branch {
            kind: match spec.purpose {
                RoutePurpose::Detour => BranchKind::Detour,
                RoutePurpose::Flank => BranchKind::Flank,
                RoutePurpose::Safe => BranchKind::Safe,
            },
            from: i,
            to: Some(j),
            rooms: ids,
            corridors: cors,
            optional: if spec.purpose == RoutePurpose::Detour {
                opt
            } else {
                None
            },
            cache,
            target: (spec.purpose == RoutePurpose::Flank).then_some(j),
            // Main corridors i -> j (corridor e joins main rooms e and e+1).
            hazard_corridors: if spec.purpose == RoutePurpose::Safe {
                (i..j).collect()
            } else {
                Vec::new()
            },
        });
    }

    // ---- Legacy alternate routes: BFS through free cells between rooms i and j.
    for _ in 0..l.alternates() {
        let mut cands: Vec<(usize, usize)> = Vec::new();
        for i in 0..n {
            for gap in [2usize, 3] {
                let j = i + gap;
                // A second way around trash, never around a boss.
                if j <= n.saturating_sub(2)
                    && !skips_boss(bp, kit, &kinds, i, j)
                    && !alts.iter().any(|(a, b, _)| *a == i && *b == j)
                {
                    cands.push((i, j));
                }
            }
        }
        shuffle(rng, &mut cands);
        let mut found = None;
        for (i, j) in cands {
            if let Some(path) = free_path(&used, cells[i], cells[j], 3) {
                found = Some((i, j, path));
                break;
            }
        }
        let Some((i, j, path)) = found else {
            return Err("alternate route");
        };
        let mut ids = Vec::new();
        let mut prev = i;
        for (k, c) in path.iter().enumerate() {
            used.insert(*c);
            let entry = Dir::between(if k == 0 { cells[i] } else { path[k - 1] }, *c);
            let mut r = new_room(
                rng,
                kit,
                RoomKind::Chamber,
                Route::Alternate,
                *c,
                entry.unwrap_or(Dir::N),
            );
            r.entry = entry;
            rooms.push(r);
            let id = rooms.len() - 1;
            corridors.push(link(prev, id, EdgeRole::Alternate, &rooms));
            ids.push(id);
            prev = id;
        }
        let mut cors: Vec<usize> = (corridors.len() - ids.len()..corridors.len()).collect();
        corridors.push(link(prev, j, EdgeRole::Alternate, &rooms));
        cors.push(corridors.len() - 1);
        for &id in &ids {
            rooms[id].branch = Some(branches.len());
        }
        branches.push(Branch {
            kind: BranchKind::Unspecified,
            from: i,
            to: Some(j),
            rooms: ids.clone(),
            corridors: cors,
            optional: None,
            cache: None,
            target: None,
            hazard_corridors: Vec::new(),
        });
        alts.push((i, j, ids));
    }

    // ---- Side rooms. 0.9 blueprints say what each is for; legacy ones get
    // optional encounters, then empty chambers (purpose unspecified).
    struct Want {
        kind: BranchKind,
        optional: Option<usize>,
        depth: usize,
    }
    let mut wants: Vec<Want> = Vec::new();
    if bp.purposeful() {
        for (k, s) in bp.side.iter().enumerate() {
            wants.push(Want {
                kind: match s.purpose {
                    SidePurpose::Optional => BranchKind::Optional,
                    SidePurpose::Cache => BranchKind::Cache,
                    SidePurpose::Vista => BranchKind::Vista,
                    SidePurpose::Quiet => BranchKind::Quiet,
                },
                optional: bp.side_optional(k),
                depth: s.depth,
            });
        }
        for (o, h) in bp.optional_homes().iter().enumerate() {
            if *h == OptionalHome::Legacy {
                wants.push(Want {
                    kind: BranchKind::Optional,
                    optional: Some(o),
                    depth: 1,
                });
            }
        }
    } else {
        let empties =
            l.side_rooms[0] + rng.index((l.side_rooms[1] - l.side_rooms[0] + 1) as usize) as u32;
        for o in 0..bp.optional.len() {
            wants.push(Want {
                kind: BranchKind::Optional,
                optional: Some(o),
                depth: 1,
            });
        }
        for _ in 0..empties {
            wants.push(Want {
                kind: BranchKind::Unspecified,
                optional: None,
                depth: 1,
            });
        }
    }
    for want in wants {
        let mut hosts: Vec<usize> = (1..n.saturating_sub(1))
            .filter(|&h| kinds[h].0 != RoomKind::Bend)
            .collect();
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
                let mut chain = vec![(c, d)];
                if want.depth == 2 {
                    // An antechamber first, the chamber one cell further.
                    let mut d2s = Dir::ALL.to_vec();
                    shuffle(rng, &mut d2s);
                    let Some(d2) = d2s.into_iter().find(|dd| {
                        let c2 = add(c, *dd);
                        c2 != cells[h] && !used.contains(&c2)
                    }) else {
                        continue;
                    };
                    chain.push((add(c, d2), d2));
                }
                let bi = branches.len();
                let mut ids = Vec::new();
                let mut cors = Vec::new();
                let mut cache = None;
                let mut prev = h;
                for (t, &(cc, dd)) in chain.iter().enumerate() {
                    used.insert(cc);
                    let last = t + 1 == chain.len();
                    let kind = if !last {
                        RoomKind::Bend
                    } else {
                        match want.kind {
                            BranchKind::Cache => RoomKind::Vault,
                            BranchKind::Vista => RoomKind::Vista,
                            _ => match want
                                .optional
                                .map(|k| kit.recipes[&bp.optional[k].recipe].room)
                            {
                                None | Some(RoomPref::Chamber) => RoomKind::Side,
                                Some(other) => room_kind_for(other),
                            },
                        }
                    };
                    let mut r = new_room(rng, kit, kind, Route::Side, cc, dd);
                    if last {
                        r.encounter = want.optional.map(EncounterRef::Optional);
                        r.cache = want.kind == BranchKind::Cache;
                    }
                    r.entry = Some(dd);
                    r.branch = Some(bi);
                    rooms.push(r);
                    let id = rooms.len() - 1;
                    if last && want.kind == BranchKind::Cache {
                        cache = Some(id);
                    }
                    corridors.push(link(prev, id, EdgeRole::Side, &rooms));
                    cors.push(corridors.len() - 1);
                    ids.push(id);
                    prev = id;
                }
                branches.push(Branch {
                    kind: want.kind,
                    from: h,
                    to: None,
                    rooms: ids,
                    corridors: cors,
                    optional: want.optional,
                    cache,
                    target: None,
                    hazard_corridors: Vec::new(),
                });
                placed = true;
                break 'host;
            }
        }
        if !placed {
            return Err(if want.optional.is_some() {
                "optional room"
            } else {
                "side room"
            });
        }
    }

    // ---- Shortcut (legacy blueprints: after everything else).
    if !bp.purposeful() {
        shortcut!();
    }

    let mut plan = Plan {
        seed: 0,
        attempt: 0,
        cell: kit.components.cell,
        rooms,
        corridors,
        main,
        branches,
    };
    for k in 0..plan.corridors.len() {
        let c = &plan.corridors[k];
        if plan.gap(c.a, c.b) < MIN_GAP {
            return Err("corridor fit");
        }
    }

    // ---- Heights and corridor styles.
    assign_heights(&mut plan, bp, rng, &alts, drop_plan, loop_plan)?;
    assign_styles(&mut plan, bp, kit, rng)?;
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

fn assign_heights(
    plan: &mut Plan,
    bp: &Blueprint,
    rng: &mut Rng,
    alts: &[(usize, usize, Vec<usize>)],
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
    // Alternate routes: spread the height change over their corridors.
    for (i, j, ids) in alts {
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
    Ok(())
}

fn assign_styles(
    plan: &mut Plan,
    bp: &Blueprint,
    kit: &Kit,
    rng: &mut Rng,
) -> Result<(), &'static str> {
    let l = &bp.layout;
    let comp = &kit.components;
    // Bridges on flat main-route corridors between ordinary rooms.
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
        let (ra, rb) = (&plan.rooms[c.a], &plan.rooms[c.b]);
        let v = c.dir.left();
        // Where each end may sit, in the shared lateral coordinate.
        let (a_lo, a_hi) = ra.span(c.dir, v);
        let (b_lo, b_hi) = rb.span(c.dir.opposite(), v);
        let narrow = matches!(
            c.role,
            EdgeRole::Side | EdgeRole::Shortcut | EdgeRole::Alternate
        ) || ra.kind == RoomKind::Bend
            || rb.kind == RoomKind::Bend;
        let wanted = if c.style == Style::Drop {
            4.0
        } else if narrow {
            round_half(range(rng, comp.passage_width))
        } else {
            round_half(range(rng, comp.corridor_width))
        };
        // Widest corridor both ends can take.
        let fit = (a_hi - a_lo).min(b_hi - b_lo);
        let width = wanted.min((fit * 2.0).floor() / 2.0);
        if width < 2.5 {
            return Err("corridor fit");
        }
        let w2 = width / 2.0;
        let (a_lo, a_hi, b_lo, b_hi) = (a_lo + w2, a_hi - w2, b_lo + w2, b_hi - w2);
        // A common offset for corridors that must run straight.
        let (c_lo, c_hi) = (a_lo.max(b_lo), a_hi.min(b_hi));
        let common = (c_lo <= c_hi).then(|| {
            // Prefer the middle of the shared span.
            round_half(((c_lo + c_hi) / 2.0).clamp(c_lo, c_hi)).clamp(c_lo, c_hi)
        });
        let level = ra.y == rb.y;
        let mut style = if c.style == Style::Drop {
            Style::Drop
        } else if !level {
            Style::Ramp
        } else if bridges.contains(&k) {
            Style::Bridge
        } else if common.is_none() {
            Style::Angled
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
        let (mut off_a, mut off_b) = (common.unwrap_or(0.0), common.unwrap_or(0.0));
        match style {
            Style::Straight | Style::Ramp | Style::Bridge | Style::Drop => {
                if common.is_none() {
                    return Err("corridor fit");
                }
            }
            Style::Winding => {
                // The jog must clear the corridor's own width: ends on
                // opposite sides, at least width + 1.2 apart.
                let need = width + 1.2;
                let s = if rng.chance(0.5) { 1.0 } else { -1.0 };
                let (ta, tb) = if s > 0.0 { (a_lo, b_hi) } else { (a_hi, b_lo) };
                if (tb - ta).abs() >= need {
                    off_a = round_half(ta + (tb - ta) * 0.1).clamp(a_lo, a_hi);
                    off_b = round_half(tb - (tb - ta) * 0.1).clamp(b_lo, b_hi);
                    if (off_b - off_a).abs() < need {
                        style = Style::Straight;
                        off_a = common.unwrap_or(0.0);
                        off_b = off_a;
                    }
                } else {
                    style = Style::Straight;
                }
            }
            Style::Angled => {
                // Any two ends that fit; slant toward opposite sides.
                if a_lo > a_hi || b_lo > b_hi {
                    return Err("corridor fit");
                }
                let s = rng.chance(0.5);
                off_a = round_half(if s {
                    a_lo + (a_hi - a_lo) * 0.25
                } else {
                    a_hi - (a_hi - a_lo) * 0.25
                })
                .clamp(a_lo, a_hi);
                off_b = round_half(if s {
                    b_hi - (b_hi - b_lo) * 0.25
                } else {
                    b_lo + (b_hi - b_lo) * 0.25
                })
                .clamp(b_lo, b_hi);
            }
        }
        if style == Style::Straight && common.is_none() {
            return Err("corridor fit");
        }
        let cc = &mut plan.corridors[k];
        cc.style = style;
        cc.width = width;
        cc.off_a = off_a;
        cc.off_b = off_b;
        cc.stairs = style == Style::Ramp && rng.chance(0.4);
    }
    Ok(())
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
            RoomKind::Bend => &names.bend,
            RoomKind::Vault => &names.vault,
            RoomKind::Vista => &names.vista,
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
