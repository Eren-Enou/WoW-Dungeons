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
