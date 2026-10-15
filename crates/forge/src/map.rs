//! Top-down overview map (SVG) of any dungeon file: floors coloured by
//! height, lava, decor, one-way drops, packs and bosses, the entrance, and —
//! for generated dungeons — room names and the main route.

use crate::layout::Plan;
use dungeon_sim::content::{Content, EnemyRank};
use dungeon_sim::math::V2;
use dungeon_sim::nav::LinkKind;
use std::fmt::Write;

fn height_color(t: f32) -> String {
    // High = cool grey-blue, low = hot orange-red.
    let t = t.clamp(0.0, 1.0);
    let lerp = |a: f32, b: f32| (a + (b - a) * t) as u8;
    format!(
        "rgb({},{},{})",
        lerp(120.0, 200.0),
        lerp(140.0, 90.0),
        lerp(185.0, 60.0)
    )
}

pub fn svg(c: &Content, plan: Option<&Plan>) -> String {
    let nav = c.nav();
    let mut lo = V2::new(f32::INFINITY, f32::INFINITY);
    let mut hi = V2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for s in &nav.surfaces {
        lo = V2::new(lo.x.min(s.bounds.min.x), lo.z.min(s.bounds.min.z));
        hi = V2::new(hi.x.max(s.bounds.max.x), hi.z.max(s.bounds.max.z));
    }
    let pad = 8.0;
    let (w, h) = (hi.x - lo.x + 2.0 * pad, hi.z - lo.z + 2.0 * pad);
    let (ymin, ymax) = nav
        .surfaces
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), s| {
            (a.min(s.min_y()), b.max(s.max_y()))
        });
    let depth = |y: f32| {
        if ymax - ymin < 0.01 {
            0.0
        } else {
            (ymax - y) / (ymax - ymin)
        }
    };
    // World x right, z up (north at the top).
    let px = |p: V2| (p.x - lo.x + pad, hi.z - p.z + pad);
    let pts = |ps: &[V2]| {
        ps.iter()
            .map(|p| {
                let (x, y) = px(*p);
                format!("{x:.2},{y:.2}")
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let scale = (900.0 / w.max(h + 14.0)).clamp(2.0, 12.0);
    let mut s = String::new();
    let _ = writeln!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:.1} {h2:.1}" width="{pw:.0}" height="{ph:.0}" font-family="sans-serif">"#,
        h2 = h + 14.0,
        pw = w * scale,
        ph = (h + 14.0) * scale
    );
    let _ = writeln!(
        s,
        r##"<rect x="0" y="0" width="{w:.1}" height="{:.1}" fill="#14110f"/>"##,
        h + 14.0
    );
    for l in &c.dungeon.lava {
        if let Ok(p) = l.shape() {
            let _ = writeln!(
                s,
                r##"<polygon points="{}" fill="#c4420f" opacity="0.85"/>"##,
                pts(p.points())
            );
        }
    }
    // Floors, lowest first so decks draw over what they span.
    let mut order: Vec<usize> = (0..nav.surfaces.len()).collect();
    order.sort_by(|&a, &b| nav.surfaces[a].max_y().total_cmp(&nav.surfaces[b].max_y()));
    for i in order {
        let sf = &nav.surfaces[i];
        let mid = (sf.min_y() + sf.max_y()) / 2.0;
        let open = sf.def.open_under;
        let _ = writeln!(
            s,
            r##"<polygon points="{}" fill="{}" stroke="{}" stroke-width="0.25"{}><title>{} (y {:.1}{})</title></polygon>"##,
            pts(sf.def.shape.points()),
            height_color(depth(mid)),
            if open { "#ffe08a" } else { "#2a2420" },
            if open {
                r#" stroke-dasharray="1,0.6""#
            } else {
                ""
            },
            sf.name(),
            sf.min_y(),
            if sf.is_ramp() {
                format!(" to {:.1}", sf.max_y())
            } else {
                String::new()
            }
        );
    }
    for d in &c.dungeon.decor {
        let (x, y) = px(d.pos);
        let _ = writeln!(
            s,
            r##"<circle cx="{x:.2}" cy="{y:.2}" r="{:.2}" fill="#4a403a"/>"##,
            d.radius
        );
    }
    for l in &nav.links {
        if l.kind != LinkKind::Drop {
            continue;
        }
        let (x1, y1) = px(l.entry.xz());
        let (x2, y2) = px(l.exit.xz());
        let _ = writeln!(
            s,
            r##"<line x1="{x1:.2}" y1="{y1:.2}" x2="{x2:.2}" y2="{y2:.2}" stroke="#ffd23f" stroke-width="1.2"/><circle cx="{x2:.2}" cy="{y2:.2}" r="1.2" fill="#ffd23f"/>"##
        );
    }
    if let Some(plan) = plan {
        // Main route through the room centres.
        let route: Vec<V2> = plan.main.iter().map(|&r| plan.rooms[r].center()).collect();
        let _ = writeln!(
            s,
            r##"<polyline points="{}" fill="none" stroke="#7fe0ff" stroke-width="0.8" stroke-dasharray="2,1.5" opacity="0.8"/>"##,
            pts(&route)
        );
        for r in &plan.rooms {
            let (x, y) = px(r.center() + V2::new(0.0, r.half - 2.5));
            let _ = writeln!(
                s,
                r##"<text x="{x:.2}" y="{y:.2}" font-size="2.2" fill="#f4ede4" text-anchor="middle">{} ({:+.1})</text>"##,
                xml(&r.name),
                r.y
            );
        }
    }
    for e in &c.encounters {
        for m in &e.mobs {
            let rank = c.templates.get(&m.template).map(|t| t.rank);
            let (fill, r) = match rank {
                Some(EnemyRank::Boss) => ("#ff4fd8", 1.8),
                Some(EnemyRank::Elite) => ("#ffcc33", 1.1),
                _ => ("#f2f2f2", 0.8),
            };
            let (x, y) = px(m.pos);
            let _ = writeln!(
                s,
                r##"<circle cx="{x:.2}" cy="{y:.2}" r="{r}" fill="{fill}" stroke="{}" stroke-width="0.3"><title>{}</title></circle>"##,
                if e.optional { "#55ff88" } else { "#000" },
                xml(e.display_name())
            );
        }
        for hz in &e.hazards {
            let (x, y) = px(hz.pos);
            let _ = writeln!(
                s,
                r##"<circle cx="{x:.2}" cy="{y:.2}" r="{:.2}" fill="none" stroke="#ff7a1a" stroke-width="0.4"/>"##,
                hz.radius
            );
        }
    }
    let (ex, ey) = px(c.dungeon.entrance);
    let _ = writeln!(
        s,
        r##"<circle cx="{ex:.2}" cy="{ey:.2}" r="1.6" fill="#3cff6e"/>"##
    );
    // Legend.
    let ly = h + 4.0;
    let _ = writeln!(
        s,
        r##"<text x="2" y="{ly:.1}" font-size="2.4" fill="#ddd">{} - high {:.1} m</text>"##,
        xml(&c.dungeon.name),
        ymax
    );
    for k in 0..10 {
        let _ = writeln!(
            s,
            r#"<rect x="{:.1}" y="{:.1}" width="4" height="2.5" fill="{}"/>"#,
            2.0 + k as f32 * 4.0,
            ly + 2.0,
            height_color(k as f32 / 9.0)
        );
    }
    let _ = writeln!(
        s,
        r##"<text x="44" y="{:.1}" font-size="2.2" fill="#ddd">low {:.1} m | green: entrance | white/yellow/pink: trash/elite/boss (green ring: optional) | yellow line: one-way drop | dashed: open deck | blue dashes: main route</text>"##,
        ly + 4.3,
        ymin
    );
    s.push_str("</svg>\n");
    s
}

fn xml(t: &str) -> String {
    t.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
