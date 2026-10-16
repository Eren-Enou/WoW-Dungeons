//! The logical structure of a dungeon as the renderer and tools see it:
//! rooms (and corridors) made of navigation surfaces, and the doorways
//! between them.
//!
//! Generated dungeons carry `[[dungeon.rooms]]` written by Dungeon Forge.
//! Handwritten ones don't; their rooms are derived from area names (the part
//! before " (", so "Hall (aisle 2)" belongs to "Hall"), and an area whose
//! name joins two others with " - " is a corridor. Either way every surface
//! belongs to exactly one room.

use bevy::prelude::*;
use dungeon_sim::V2;
use dungeon_sim::content::Content;
use dungeon_sim::nav::{LinkKind, Nav};

#[derive(Debug, Clone)]
pub struct RoomView {
    pub id: String,
    pub name: String,
    pub role: String,
    pub route: String,
    pub order: Option<u32>,
    /// Surface (= area) indices.
    pub surfaces: Vec<usize>,
    pub min: Vec3,
    pub max: Vec3,
    pub encounter: Option<String>,
    pub atmosphere: Option<String>,
    pub corridor: bool,
    /// Whether the dungeon file declared this room (false = derived).
    pub declared: bool,
    /// Why this room exists (see [`Purpose`]).
    pub purpose: Purpose,
    /// Only one way in or out.
    pub dead_end: bool,
    /// Rooms (not counting corridors) between it and the main route.
    pub off_main: Option<u32>,
}

/// Why a room or corridor exists, from the generator's own route data.
/// Shared by the 3D structure layer, the 2D map, the inspector and the
/// branch audit (0.8.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// On the main route.
    Main,
    /// A second way between two main-route rooms.
    Alternate,
    /// A side chamber holding an optional encounter.
    Optional,
    /// A side chamber with nothing in it (exploration, pacing).
    Empty,
    /// A raised room above a one-way drop (a shortcut down).
    Overlook,
    /// A corridor that is a shortcut (overlook walkway).
    Shortcut,
    /// A corridor (or the rooms along it) returning to an earlier main room.
    Loop,
    /// An ordinary corridor.
    Passage,
    /// Handwritten dungeon: no route data.
    Unknown,
}

impl Purpose {
    pub fn label(self) -> &'static str {
        match self {
            Purpose::Main => "main route",
            Purpose::Alternate => "alternate route",
            Purpose::Optional => "optional encounter chamber",
            Purpose::Empty => "empty side chamber",
            Purpose::Overlook => "overlook (one-way drop shortcut)",
            Purpose::Shortcut => "shortcut",
            Purpose::Loop => "return loop",
            Purpose::Passage => "passage",
            Purpose::Unknown => "room (no route data)",
        }
    }

    /// Colour used for this purpose on the map and in the structure layer.
    pub fn color(self) -> [f32; 3] {
        match self {
            Purpose::Main => [0.35, 0.7, 1.0],
            Purpose::Alternate => [0.3, 1.0, 0.9],
            Purpose::Optional => [0.4, 1.0, 0.4],
            Purpose::Empty => [0.7, 0.6, 0.85],
            Purpose::Overlook | Purpose::Shortcut => [1.0, 0.6, 0.2],
            Purpose::Loop => [0.85, 0.5, 1.0],
            Purpose::Passage => [0.6, 0.6, 0.65],
            Purpose::Unknown => [0.8, 0.8, 0.8],
        }
    }
}

fn purpose_of(r: &RoomView, optional: bool) -> Purpose {
    if !r.declared {
        return if r.corridor {
            Purpose::Passage
        } else {
            Purpose::Unknown
        };
    }
    if r.corridor {
        return match r.route.as_str() {
            "shortcut" => Purpose::Shortcut,
            "loop" => Purpose::Loop,
            _ => Purpose::Passage,
        };
    }
    match r.route.as_str() {
        "main" => Purpose::Main,
        "alternate" => Purpose::Alternate,
        "overlook" => Purpose::Overlook,
        "side" if r.encounter.is_some() && optional => Purpose::Optional,
        "side" if r.encounter.is_some() => Purpose::Main,
        "side" => Purpose::Empty,
        _ => Purpose::Unknown,
    }
}

impl RoomView {
    /// Floor centre: middle of the bounds at the lowest floor height.
    pub fn center(&self) -> Vec3 {
        Vec3::new(
            (self.min.x + self.max.x) * 0.5,
            self.min.y,
            (self.min.z + self.max.z) * 0.5,
        )
    }

    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    /// The preset this room's lighting uses: its own, else its role's.
    pub fn atmosphere_key(&self) -> &str {
        self.atmosphere.as_deref().unwrap_or(&self.role)
    }
}

/// An opening between two rooms: a walk portal between surfaces of
/// different rooms.
#[derive(Debug, Clone, Copy)]
pub struct Doorway {
    pub rooms: (usize, usize),
    pub center: Vec3,
    /// Unit vector along the opening (across the passage).
    pub along: Vec3,
    /// Width of the opening.
    pub width: f32,
    /// Unit vector through the opening (along the passage).
    pub through: Vec3,
    /// Half the portal's depth along `through`.
    pub half_depth: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Structure {
    pub rooms: Vec<RoomView>,
    /// Room of every surface.
    pub surface_room: Vec<usize>,
    pub doorways: Vec<Doorway>,
}

/// Room name of an area: "Hall (aisle 2)" -> "Hall".
pub fn room_of(area: &str) -> &str {
    area.split(" (").next().unwrap_or(area)
}

impl Structure {
    pub fn build(content: &Content, nav: &Nav) -> Structure {
        let n = nav.surfaces.len();
        let mut rooms: Vec<RoomView> = Vec::new();
        let mut surface_room = vec![usize::MAX; n];
        let index_of = |name: &str| nav.surfaces.iter().position(|s| s.name() == name);
        for r in &content.dungeon.rooms {
            let surfaces: Vec<usize> = r.areas.iter().filter_map(|a| index_of(a)).collect();
            let k = rooms.len();
            for &s in &surfaces {
                surface_room[s] = k;
            }
            rooms.push(RoomView {
                id: r.id.clone(),
                name: r.name.clone(),
                role: r.role.clone(),
                route: r.route.clone(),
                order: r.order,
                surfaces,
                min: Vec3::ZERO,
                max: Vec3::ZERO,
                encounter: r.encounter.clone().filter(|e| !e.is_empty()),
                atmosphere: r.atmosphere.clone(),
                purpose: Purpose::Unknown,
                dead_end: false,
                off_main: None,
                corridor: matches!(
                    r.role.as_str(),
                    "corridor" | "passage" | "bridge" | "stair" | "ramp" | "ledge"
                ) && !r.connects.is_empty(),
                declared: true,
            });
        }
        // Surfaces no declared room covers: group by name.
        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            if surface_room[i] != usize::MAX {
                continue;
            }
            let name = room_of(nav.surfaces[i].name()).to_string();
            let k = match rooms.iter().position(|r| !r.declared && r.name == name) {
                Some(k) => k,
                None => {
                    let corridor = name.contains(" - ");
                    rooms.push(RoomView {
                        id: format!("a{}", rooms.len()),
                        name: name.clone(),
                        role: if corridor { "corridor" } else { "room" }.into(),
                        route: String::new(),
                        order: None,
                        surfaces: Vec::new(),
                        min: Vec3::ZERO,
                        max: Vec3::ZERO,
                        encounter: None,
                        atmosphere: None,
                        corridor,
                        declared: false,
                        purpose: Purpose::Unknown,
                        dead_end: false,
                        off_main: None,
                    });
                    rooms.len() - 1
                }
            };
            rooms[k].surfaces.push(i);
            surface_room[i] = k;
        }
        // Encounters of derived rooms: the room under the pack's first mob.
        for e in &content.encounters {
            if let Some(m) = e.mobs.first() {
                let (p, s) = nav.project(m.pos, m.y.unwrap_or(0.0));
                let _ = p;
                let k = surface_room[s as usize];
                if !rooms[k].declared && rooms[k].encounter.is_none() {
                    rooms[k].encounter = Some(e.id.clone());
                }
            }
        }
        for r in rooms.iter_mut() {
            let mut lo = Vec3::splat(f32::INFINITY);
            let mut hi = Vec3::splat(f32::NEG_INFINITY);
            for &s in &r.surfaces {
                let sf = &nav.surfaces[s];
                lo = lo.min(Vec3::new(sf.bounds.min.x, sf.min_y(), sf.bounds.min.z));
                hi = hi.max(Vec3::new(sf.bounds.max.x, sf.max_y(), sf.bounds.max.z));
            }
            if r.surfaces.is_empty() {
                lo = Vec3::ZERO;
                hi = Vec3::ZERO;
            }
            r.min = lo;
            r.max = hi;
        }
        // Doorways: walk portals between different rooms. One opening can be
        // split over several portals (a corridor meeting two floor strips of
        // a pit or pillared room): merge touching portals of the same pair.
        let mut boxes: Vec<((usize, usize), Vec2, Vec2, f32)> = Vec::new();
        for l in &nav.links {
            if l.kind != LinkKind::Walk {
                continue;
            }
            let (a, b) = (surface_room[l.from as usize], surface_room[l.to as usize]);
            if a >= b || l.portal.points().len() < 2 {
                continue;
            }
            let bb = l.portal.bounds();
            let (lo, hi) = (Vec2::new(bb.min.x, bb.min.z), Vec2::new(bb.max.x, bb.max.z));
            match boxes.iter_mut().find(|(pair, blo, bhi, y)| {
                *pair == (a, b)
                    && (l.entry.y - *y).abs() < 0.5
                    && lo.x <= bhi.x + 1.0
                    && hi.x >= blo.x - 1.0
                    && lo.y <= bhi.y + 1.0
                    && hi.y >= blo.y - 1.0
            }) {
                Some((_, blo, bhi, _)) => {
                    *blo = blo.min(lo);
                    *bhi = bhi.max(hi);
                }
                None => boxes.push(((a, b), lo, hi, l.entry.y)),
            }
        }
        // Merge until stable (a box can grow to touch an earlier one).
        let touches = |a: &((usize, usize), Vec2, Vec2, f32),
                       b: &((usize, usize), Vec2, Vec2, f32)| {
            a.0 == b.0
                && (a.3 - b.3).abs() < 0.5
                && a.1.x <= b.2.x + 1.0
                && a.2.x >= b.1.x - 1.0
                && a.1.y <= b.2.y + 1.0
                && a.2.y >= b.1.y - 1.0
        };
        loop {
            let mut merged = false;
            'outer: for i in 0..boxes.len() {
                for j in i + 1..boxes.len() {
                    if touches(&boxes[i], &boxes[j]) {
                        let b = boxes.remove(j);
                        boxes[i].1 = boxes[i].1.min(b.1);
                        boxes[i].2 = boxes[i].2.max(b.2);
                        merged = true;
                        break 'outer;
                    }
                }
            }
            if !merged {
                break;
            }
        }
        let doorways: Vec<Doorway> = boxes
            .into_iter()
            .map(|(rooms, lo, hi, y)| {
                let size = hi - lo;
                let (along, width, through, depth) = if size.x >= size.y {
                    (Vec3::X, size.x, Vec3::Z, size.y)
                } else {
                    (Vec3::Z, size.y, Vec3::X, size.x)
                };
                let c = (lo + hi) * 0.5;
                Doorway {
                    rooms,
                    center: Vec3::new(c.x, y, c.y),
                    along,
                    width,
                    through,
                    half_depth: depth * 0.5,
                }
            })
            .collect();
        // Room graph: doorways, plus one-way drops (from -> to).
        let n = rooms.len();
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
        for d in &doorways {
            adj[d.rooms.0].push(d.rooms.1);
            adj[d.rooms.1].push(d.rooms.0);
        }
        for l in &nav.links {
            if l.kind == LinkKind::Drop {
                let (a, b) = (surface_room[l.from as usize], surface_room[l.to as usize]);
                if a != b {
                    adj[a].push(b);
                    adj[b].push(a);
                }
            }
        }
        for a in adj.iter_mut() {
            a.sort_unstable();
            a.dedup();
        }
        let optional: std::collections::BTreeSet<&str> = content
            .encounters
            .iter()
            .filter(|e| e.optional)
            .map(|e| e.id.as_str())
            .collect();
        for r in rooms.iter_mut() {
            let opt = r.encounter.as_deref().is_some_and(|e| optional.contains(e));
            r.purpose = purpose_of(r, opt);
        }
        // The forge tags the rooms of a return loop (the crypt's cloister
        // walk) "alternate"; they sit only between loop corridors.
        for i in 0..n {
            let loop_only = !rooms[i].corridor
                && rooms[i].purpose == Purpose::Alternate
                && adj[i].iter().any(|&j| rooms[j].corridor)
                && adj[i]
                    .iter()
                    .filter(|&&j| rooms[j].corridor)
                    .all(|&j| rooms[j].purpose == Purpose::Loop);
            if loop_only {
                rooms[i].purpose = Purpose::Loop;
            }
        }
        for (i, r) in rooms.iter_mut().enumerate() {
            r.dead_end = !r.corridor && adj[i].len() == 1;
        }
        // Distance from the main route, counting rooms (corridors are free).
        if rooms.iter().any(|r| r.purpose == Purpose::Main) {
            let mut dist: Vec<Option<u32>> = vec![None; n];
            let mut queue = std::collections::VecDeque::new();
            for (i, r) in rooms.iter().enumerate() {
                if r.purpose == Purpose::Main || (r.corridor && r.route == "main") {
                    dist[i] = Some(0);
                    queue.push_back(i);
                }
            }
            // 0-1 BFS: corridors cost nothing, rooms one.
            while let Some(i) = queue.pop_front() {
                let di = dist[i].unwrap_or(0);
                for &j in &adj[i] {
                    let step = u32::from(!rooms[j].corridor);
                    if dist[j].is_none_or(|d| di + step < d) {
                        dist[j] = Some(di + step);
                        if step == 0 {
                            queue.push_front(j);
                        } else {
                            queue.push_back(j);
                        }
                    }
                }
            }
            for (i, r) in rooms.iter_mut().enumerate() {
                r.off_main = dist[i];
            }
        }
        Structure {
            rooms,
            surface_room,
            doorways,
        }
    }

    /// The room whose floor is under a point: the highest surface below it
    /// that contains it horizontally.
    pub fn room_at(&self, nav: &Nav, p: Vec3) -> Option<usize> {
        let s = floor_below(nav, p)?;
        self.surface_room.get(s).copied()
    }

    pub fn corridors(&self) -> usize {
        self.rooms.iter().filter(|r| r.corridor).count()
    }
}

/// The highest walkable surface under `p` (within 60 m below it).
pub fn floor_below(nav: &Nav, p: Vec3) -> Option<usize> {
    let q = V2::new(p.x, p.z);
    nav.surfaces
        .iter()
        .enumerate()
        .filter(|(_, s)| s.def.shape.contains(q))
        .map(|(i, s)| (i, s.height_at(q)))
        .filter(|(_, h)| *h <= p.y + 0.5 && *h > p.y - 60.0)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(file: &str) -> Content {
        let path = format!(
            "{}/../../assets/content/dungeons/{file}",
            env!("CARGO_MANIFEST_DIR")
        );
        Content::load(&path).unwrap()
    }

    fn purpose(st: &Structure, id: &str) -> Purpose {
        st.rooms.iter().find(|r| r.id == id).unwrap().purpose
    }

    #[test]
    fn generated_rooms_are_classified_by_purpose() {
        let c = load("07_emberforge_vaults.toml");
        let st = Structure::build(&c, &c.nav());
        assert_eq!(purpose(&st, "r0"), Purpose::Main);
        assert_eq!(purpose(&st, "r9"), Purpose::Main);
        assert_eq!(purpose(&st, "r10"), Purpose::Alternate);
        assert_eq!(purpose(&st, "r12"), Purpose::Optional); // Coal Bunker
        assert_eq!(purpose(&st, "r13"), Purpose::Empty); // Shuttered Kiln
        assert_eq!(purpose(&st, "r14"), Purpose::Overlook);
        let kiln = st.rooms.iter().find(|r| r.id == "r13").unwrap();
        assert!(kiln.dead_end);
        assert_eq!(kiln.off_main, Some(1));
        assert_eq!(
            st.rooms.iter().find(|r| r.id == "r3").unwrap().off_main,
            Some(0)
        );

        let mut c = load("08_hollow_reliquary.toml");
        let st = Structure::build(&c, &c.nav());
        assert_eq!(purpose(&st, "r13"), Purpose::Optional);
        assert_eq!(purpose(&st, "r15"), Purpose::Empty);
        assert_eq!(purpose(&st, "c16"), Purpose::Loop);
        assert_eq!(purpose(&st, "r10"), Purpose::Alternate);
        // A room between loop corridors only belongs to the loop.
        let around: Vec<String> = c
            .dungeon
            .rooms
            .iter()
            .filter(|r| r.connects.iter().any(|x| x == "r10"))
            .map(|r| r.id.clone())
            .collect();
        assert!(!around.is_empty());
        for r in c.dungeon.rooms.iter_mut() {
            if around.contains(&r.id) {
                r.route = "loop".into();
            }
        }
        let st = Structure::build(&c, &c.nav());
        assert_eq!(purpose(&st, "r10"), Purpose::Loop);
    }

    #[test]
    fn handwritten_rooms_have_no_route_purpose() {
        let c = load("01_sunken_vault.toml");
        let st = Structure::build(&c, &c.nav());
        assert!(
            st.rooms
                .iter()
                .all(|r| matches!(r.purpose, Purpose::Unknown | Purpose::Passage))
        );
    }
}
