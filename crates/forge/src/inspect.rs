//! A readable summary of any dungeon file (handwritten or generated):
//! areas by kind, heights, drops, encounters and the route length.

use dungeon_sim::content::{Content, EnemyRank};
use dungeon_sim::math::V3;
use std::fmt::Write;

pub fn summary(c: &Content) -> String {
    let d = &c.dungeon;
    let nav = c.nav();
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{} ({}) - level {}, group {}",
        d.name, d.id, d.recommended_level, d.recommended_group
    );
    let polys = d.areas.iter().filter(|a| a.points.is_some()).count();
    let ramps = d.areas.iter().filter(|a| a.y_to.is_some()).count();
    let decks = d
        .areas
        .iter()
        .filter(|a| a.under == dungeon_sim::content::UnderConfig::Open)
        .count();
    let (lo, hi) = nav
        .surfaces
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), s| {
            (lo.min(s.min_y()), hi.max(s.max_y()))
        });
    let _ = writeln!(
        s,
        "areas: {} ({} polygons, {} ramps, {} open decks); heights {:.1} .. {:.1} m",
        d.areas.len(),
        polys,
        ramps,
        decks,
        lo,
        hi
    );
    let drops = nav
        .links
        .iter()
        .filter(|l| l.kind == dungeon_sim::nav::LinkKind::Drop)
        .count();
    let _ = writeln!(
        s,
        "navigation: {} surfaces, {} links ({} one-way drops), {} wall segments",
        nav.surfaces.len(),
        nav.links.len(),
        drops,
        nav.walls.len()
    );
    let _ = writeln!(
        s,
        "scenery: {} lava, {} decor, {} lights",
        d.lava.len(),
        d.decor.len(),
        d.lights.len()
    );
    let start = Content::place(&nav, d.entrance, d.entrance_y).ok();
    let _ = writeln!(s, "encounters ({}):", c.encounters.len());
    let mut walked = 0.0f32;
    let mut from = start;
    for e in &c.encounters {
        let mut ranks = [0u32; 3];
        for m in &e.mobs {
            if let Some(t) = c.templates.get(&m.template) {
                ranks[match t.rank {
                    EnemyRank::Trash => 0,
                    EnemyRank::Elite => 1,
                    EnemyRank::Boss => 2,
                }] += 1;
            }
        }
        let first = &e.mobs[0];
        let here = Content::place(&nav, first.pos, first.y).ok();
        let area = here
            .map(|(_, s)| nav.surfaces[s as usize].name().to_string())
            .unwrap_or_else(|| "?".into());
        // Route length from the previous encounter (or the entrance).
        let leg = match (from, here) {
            (Some((a, sa)), Some((b, _))) => nav.path_points(a.xz(), sa, b).map(|pts| {
                let mut len = 0.0;
                let mut p: V3 = a;
                for q in pts {
                    len += p.dist(q);
                    p = q;
                }
                len
            }),
            _ => None,
        };
        if let Some(l) = leg {
            walked += l;
        }
        let _ = writeln!(
            s,
            "  {:<28} {:<9} {} trash, {} elite, {} boss{}  in '{}' at y {:.1}  ({} m walk)",
            e.display_name(),
            if e.optional { "optional" } else { "required" },
            ranks[0],
            ranks[1],
            ranks[2],
            if e.hazards.is_empty() {
                String::new()
            } else {
                format!(", {} hazards", e.hazards.len())
            },
            area,
            here.map(|(p, _)| p.y).unwrap_or(0.0),
            leg.map(|l| format!("{l:.0}")).unwrap_or_else(|| "?".into())
        );
        if here.is_some() {
            from = here;
        }
    }
    let _ = writeln!(s, "walking route through every encounter: {walked:.0} m");
    s
}
