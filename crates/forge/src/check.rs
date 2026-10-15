//! Validation beyond "it loads": the navigation the game builds must match
//! the plan exactly (every intended connection, no accidental ones, drops
//! one-way), every room must be reachable, and encounters must have room to
//! be fought (staging space, no chain pulls, arenas big enough).

use crate::compose::Owner;
use crate::layout::{Plan, Style};
use dungeon_sim::content::{AbilityKind, Content, EnemyRank};
use dungeon_sim::math::{V2, V3};
use dungeon_sim::nav::{LinkKind, Nav};

#[derive(Debug, Default, Clone)]
pub struct CheckReport {
    /// Problems that make the dungeon invalid.
    pub errors: Vec<String>,
    /// Things worth a designer's look that don't block generation.
    pub warnings: Vec<String>,
    /// What was checked and passed (for the report).
    pub passed: Vec<String>,
}

impl CheckReport {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

fn describe(plan: &Plan, o: Owner) -> String {
    match o {
        Owner::Room(r) => format!("room '{}'", plan.rooms[r].name),
        Owner::Corridor(c) => {
            let k = &plan.corridors[c];
            format!(
                "corridor '{}' -> '{}'",
                plan.rooms[k.a].name, plan.rooms[k.b].name
            )
        }
    }
}

/// Structural checks against the navigation built by the game's loader.
pub fn structure(plan: &Plan, owners: &[Owner], content: &Content, nav: &Nav) -> CheckReport {
    let mut rep = CheckReport::default();
    if nav.surfaces.len() != owners.len() {
        rep.errors.push(format!(
            "internal: {} areas but {} owners",
            nav.surfaces.len(),
            owners.len()
        ));
        return rep;
    }
    let allowed = |a: Owner, b: Owner| -> bool {
        match (a, b) {
            (Owner::Room(x), Owner::Room(y)) => x == y,
            (Owner::Corridor(x), Owner::Corridor(y)) => x == y,
            (Owner::Room(r), Owner::Corridor(c)) | (Owner::Corridor(c), Owner::Room(r)) => {
                let k = &plan.corridors[c];
                // A drop walkway walks onto its overlook only; the far end is the drop.
                k.a == r || (k.b == r && k.walkable_both_ways())
            }
        }
    };
    let mut unintended = 0;
    for l in &nav.links {
        let (a, b) = (owners[l.from as usize], owners[l.to as usize]);
        match l.kind {
            LinkKind::Walk => {
                if !allowed(a, b) {
                    unintended += 1;
                    rep.errors.push(format!(
                        "unintended connection: area '{}' ({}) joins area '{}' ({}); the plan has no such corridor",
                        nav.surfaces[l.from as usize].name(),
                        describe(plan, a),
                        nav.surfaces[l.to as usize].name(),
                        describe(plan, b)
                    ));
                }
            }
            LinkKind::Drop => {
                let ok = matches!((a, b), (Owner::Corridor(c), Owner::Room(r))
                    if plan.corridors[c].style == Style::Drop && plan.corridors[c].b == r);
                if !ok {
                    rep.errors.push(format!(
                        "unexpected drop from '{}' to '{}'",
                        nav.surfaces[l.from as usize].name(),
                        nav.surfaces[l.to as usize].name()
                    ));
                }
            }
        }
    }
    if unintended == 0 {
        rep.passed
            .push("no walk link joins pieces the plan doesn't connect".into());
    }
    // Every corridor exists as planned.
    let linked = |from: Owner, to: Owner, kind: LinkKind| {
        nav.links.iter().any(|l| {
            l.kind == kind && owners[l.from as usize] == from && owners[l.to as usize] == to
        })
    };
    let mut missing = 0;
    for (k, c) in plan.corridors.iter().enumerate() {
        let me = Owner::Corridor(k);
        let ok = if c.style == Style::Drop {
            linked(Owner::Room(c.a), me, LinkKind::Walk)
                && linked(me, Owner::Room(c.b), LinkKind::Drop)
                && !linked(Owner::Room(c.b), me, LinkKind::Walk)
        } else {
            linked(Owner::Room(c.a), me, LinkKind::Walk)
                && linked(me, Owner::Room(c.a), LinkKind::Walk)
                && linked(Owner::Room(c.b), me, LinkKind::Walk)
                && linked(me, Owner::Room(c.b), LinkKind::Walk)
        };
        if !ok {
            missing += 1;
            rep.errors.push(format!(
                "{} ({:?}) is not connected as planned in the navigation",
                describe(plan, me),
                c.style
            ));
        }
    }
    if missing == 0 {
        rep.passed.push(format!(
            "all {} corridors connect as planned ({} one-way drops)",
            plan.corridors.len(),
            plan.corridors
                .iter()
                .filter(|c| c.style == Style::Drop)
                .count()
        ));
    }
    // Reachability from the entrance.
    let Ok((start, s0)) = Content::place(nav, content.dungeon.entrance, None) else {
        rep.errors.push("the entrance is not on a floor".into());
        return rep;
    };
    let mut unreachable = 0;
    for (i, r) in plan.rooms.iter().enumerate() {
        let Some(s) = owners.iter().position(|o| *o == Owner::Room(i)) else {
            continue;
        };
        let surf = &nav.surfaces[s];
        let c = surf.walk.centroid();
        let goal = V3::at(c, surf.height_at(c));
        if nav.path_points(start.xz(), s0, goal).is_none() {
            unreachable += 1;
            rep.errors.push(format!(
                "room '{}' cannot be reached from the entrance",
                r.name
            ));
        }
    }
    if unreachable == 0 {
        rep.passed.push(format!(
            "all {} rooms reachable from the entrance",
            plan.rooms.len()
        ));
    }
    // The final arena has a single way in.
    let fin = *plan.main.last().expect("main route");
    let ways = plan
        .corridors
        .iter()
        .filter(|c| c.a == fin || c.b == fin)
        .count();
    if ways == 1 {
        rep.passed
            .push("the final arena is reached only by the main route".into());
    } else {
        rep.errors.push(format!(
            "the final arena '{}' has {ways} ways in; it must be the end of the main route",
            plan.rooms[fin].name
        ));
    }
    rep
}

/// Encounter-space checks: staging room, chain pulls, arena size.
pub fn encounters(plan: &Plan, content: &Content, rooms_of: &[usize]) -> CheckReport {
    let mut rep = CheckReport::default();
    let t = &content.templates;
    let aggro = |key: &str| t.get(key).map(|x| x.aggro_radius).unwrap_or(10.0);
    let mut staging_ok = 0;
    for (e, enc) in content.encounters.iter().enumerate() {
        let room = &plan.rooms[rooms_of[e]];
        let Some(dir) = room.entry else { continue };
        // The middle of the corridor the party arrives through.
        let u = dir.unit();
        let gap_mid = room.center() - u * (room.half + 4.0);
        let worst = enc
            .mobs
            .iter()
            .map(|m| m.pos.dist(gap_mid) - aggro(&m.template))
            .fold(f32::INFINITY, f32::min);
        if worst < 1.0 {
            rep.errors.push(format!(
                "encounter '{}' in '{}': its pack would aggro the party waiting in the doorway ({worst:.1} m margin); use a bigger room for this recipe",
                enc.display_name(),
                room.name
            ));
        } else {
            staging_ok += 1;
        }
    }
    rep.passed.push(format!(
        "{staging_ok}/{} encounters leave a safe staging spot in their entry corridor",
        content.encounters.len()
    ));
    // Chain pulls: fighting one pack must not drag in another.
    let mut chains = 0;
    for (i, a) in content.encounters.iter().enumerate() {
        for b in content.encounters.iter().skip(i + 1) {
            for ma in &a.mobs {
                for mb in &b.mobs {
                    let dy = (ma.y.unwrap_or(0.0) - mb.y.unwrap_or(0.0)).abs();
                    let need = aggro(&mb.template).max(aggro(&ma.template)) + 6.0;
                    if dy < 3.0 && ma.pos.dist(mb.pos) < need {
                        chains += 1;
                        rep.errors.push(format!(
                            "encounters '{}' and '{}' are {:.1} m apart (need {need:.0}): pulling one pulls the other",
                            a.display_name(),
                            b.display_name(),
                            ma.pos.dist(mb.pos)
                        ));
                    }
                }
            }
        }
    }
    if chains == 0 {
        rep.passed
            .push("no encounter is close enough to chain-pull another".into());
    }
    // Arenas big enough for their bosses' mechanics.
    for (e, enc) in content.encounters.iter().enumerate() {
        let room = &plan.rooms[rooms_of[e]];
        for m in &enc.mobs {
            let Some(tp) = t.get(&m.template) else {
                continue;
            };
            if tp.rank != EnemyRank::Boss {
                continue;
            }
            let ring = tp
                .abilities
                .iter()
                .filter(|a| a.kind == AbilityKind::Ring)
                .map(|a| a.radius)
                .fold(0.0f32, f32::max);
            let cleave = tp
                .abilities
                .iter()
                .filter(|a| a.kind == AbilityKind::Cleave)
                .map(|a| a.radius)
                .fold(0.0f32, f32::max);
            let need = (2.5 * ring).max(1.6 * cleave);
            if room.half < need {
                rep.errors.push(format!(
                    "'{}' needs a room at least {need:.1} m from centre to wall for its rings and cleaves; '{}' has {:.1}",
                    tp.name, room.name, room.half
                ));
            } else {
                rep.passed.push(format!(
                    "'{}' has room for its mechanics ({:.1} m, needs {need:.1})",
                    tp.name, room.half
                ));
            }
        }
    }
    let _ = V2::ZERO;
    rep
}
