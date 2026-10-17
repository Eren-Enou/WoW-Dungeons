//! Branch purposes, measured (0.9).
//!
//! Every branch the layout built declares what it is for. This module
//! checks, on the real navigation, that it delivers it:
//!
//! * **detour**: holds a reward (an optional encounter or a cache), bypasses
//!   no required encounter, rejoins the main route;
//! * **flank**: enters a required pack's room through another door, at least
//!   60° off the pack's front, from a spot outside every member's aggro;
//! * **safe**: hazard-free, at least 15% longer than the short way, which
//!   carries the kit's route hazards;
//! * **optional / cache**: the encounter or cache is really there;
//! * **drop**: the rooms it skips can be walked back to from the landing;
//! * every required room can reach the final boss.
//!
//! Lengths are walking lengths on the navigation, room to room through
//! floor centres (`Nav::path_points`), not straight lines. Errors reject the
//! layout attempt; facts and warnings go into `[[dungeon.branches]]`.

use crate::check::CheckReport;
use crate::compose::Composed;
use crate::kit::Kit;
use crate::layout::{BranchKind, EncounterRef, Plan};
use dungeon_sim::content::{BranchConfig, Content};
use dungeon_sim::math::{V2, V3};
use dungeon_sim::nav::Nav;

/// A flank must come in at least this far off the pack's front (degrees).
pub const FLANK_MIN_ANGLE: f32 = 60.0;
/// A safe route must be at least this much longer than the short way.
pub const SAFE_MIN_RATIO: f32 = 1.15;
/// Flank staging keeps this margin beyond every member's aggro radius.
pub const FLANK_MARGIN: f32 = 1.0;
/// A flank longer than this many times the main approach is a detour.
pub const FLANK_MAX_RATIO: f32 = 3.0;
/// A flank is a side passage: at least this long against the main approach,
/// so the shortest way to the pack stays the main door (no accidental flank).
pub const FLANK_MIN_RATIO: f32 = 1.1;
/// A detour costs walking: at least this long against the main route, so
/// the shortest way never runs through its reward (no accidental pulls).
pub const DETOUR_MIN_RATIO: f32 = 1.15;
/// A safe way longer than this is no longer a reasonable alternative.
pub const SAFE_MAX_RATIO: f32 = 3.5;
/// A one-way drop must save at least this much walking (0.9 blueprints).
pub const DROP_MAX_RATIO: f32 = 0.95;

/// What the generator measured about one branch.
#[derive(Debug, Clone, Default)]
pub struct Measured {
    pub facts: Vec<String>,
    pub warnings: Vec<String>,
    pub bypasses: Vec<String>,
    pub rewards: Vec<String>,
    /// Walking length of the branch between its two main rooms (through routes).
    pub branch_len: Option<f32>,
    /// Walking length of the main route between the same two rooms.
    pub main_len: Option<f32>,
    /// Flank: degrees between the pack's front and the flank door.
    pub angle: Option<f32>,
    /// Did it deliver its declared purpose?
    pub delivered: bool,
}

/// A point on room `r`'s floor (its first piece's centre) at floor height.
pub fn room_point(plan: &Plan, comp: &Composed, r: usize) -> V3 {
    let p = comp.floors[r]
        .first()
        .map(|f| f.centroid())
        .unwrap_or(plan.rooms[r].center());
    V3::at(p, plan.rooms[r].y)
}

/// Walking length from `a` to `b` on the navigation.
pub fn walk(nav: &Nav, a: V3, b: V3) -> Option<f32> {
    let s = nav.locate(a.xz(), a.y + 0.5)?;
    let pts = nav.path_points(a.xz(), s, b)?;
    Some(pts.windows(2).map(|w| w[0].dist(w[1])).sum())
}

/// Walking length through a sequence of rooms (leg by leg).
fn walk_rooms(nav: &Nav, plan: &Plan, comp: &Composed, rooms: &[usize]) -> Option<f32> {
    rooms
        .windows(2)
        .map(|w| {
            walk(
                nav,
                room_point(plan, comp, w[0]),
                room_point(plan, comp, w[1]),
            )
        })
        .sum()
}

fn main_pos(plan: &Plan, r: usize) -> Option<usize> {
    plan.main.iter().position(|&m| m == r)
}

fn room_name(plan: &Plan, r: usize) -> String {
    format!("r{r} '{}'", plan.rooms[r].name)
}

/// The encounter id staged in room `r`.
fn encounter_in(content: &Content, enc_rooms: &[usize], r: usize) -> Option<usize> {
    enc_rooms
        .iter()
        .position(|&er| er == r)
        .filter(|&e| e < content.encounters.len())
}

#[allow(clippy::too_many_arguments)]
pub fn measure(
    plan: &Plan,
    comp: &Composed,
    kit: &Kit,
    content: &Content,
    nav: &Nav,
    enc_rooms: &[usize],
    anchors: &[(V2, V2)],
    cache_rooms: &[usize],
    strict: bool,
) -> (CheckReport, Vec<Measured>) {
    let mut rep = CheckReport::default();
    let mut out = Vec::new();
    let aggro = |key: &str| {
        content
            .templates
            .get(key)
            .map(|t| t.aggro_radius)
            .unwrap_or(10.0)
    };
    let required_in = |a: usize, b: usize| -> Vec<String> {
        // Required encounters in main rooms strictly between positions a and b.
        let (lo, hi) = (a.min(b), a.max(b));
        plan.main[lo + 1..hi]
            .iter()
            .filter(|&&m| matches!(plan.rooms[m].encounter, Some(EncounterRef::Main(_))))
            .filter_map(|&m| encounter_in(content, enc_rooms, m))
            .map(|e| content.encounters[e].id.clone())
            .collect()
    };
    for (bi, b) in plan.branches.iter().enumerate() {
        let mut m = Measured::default();
        let label = format!(
            "branch b{bi} ({}, from {})",
            b.kind.label(),
            room_name(plan, b.from)
        );
        // Rewards held anywhere in the branch.
        for &r in &b.rooms {
            if let Some(e) = encounter_in(content, enc_rooms, r)
                && content.encounters[e].optional
            {
                m.rewards.push(content.encounters[e].id.clone());
            }
            for (k, &cr) in cache_rooms.iter().enumerate() {
                if cr == r {
                    m.rewards.push(content.dungeon.caches[k].id.clone());
                }
            }
        }
        // Through routes: lengths against the main route, and what they go round.
        if let Some(to) = b.to {
            let (Some(pf), Some(pt)) = (main_pos(plan, b.from), main_pos(plan, to)) else {
                rep.errors
                    .push(format!("{label}: does not join two main-route rooms"));
                out.push(m);
                continue;
            };
            let (lo, hi) = (pf.min(pt), pf.max(pt));
            let main_rooms: Vec<usize> = plan.main[lo..=hi].to_vec();
            let mut chain = vec![b.from];
            chain.extend(&b.rooms);
            chain.push(to);
            m.main_len = walk_rooms(nav, plan, comp, &main_rooms);
            m.branch_len = if b.kind == BranchKind::Drop {
                // Walk to the overlook, then the drop's own path down.
                let o = b.rooms[0];
                walk(
                    nav,
                    room_point(plan, comp, b.from),
                    room_point(plan, comp, o),
                )
                .and_then(|up| {
                    walk(nav, room_point(plan, comp, o), room_point(plan, comp, to)).map(|d| up + d)
                })
            } else {
                walk_rooms(nav, plan, comp, &chain)
            };
            m.bypasses = required_in(pf, pt);
            match (m.branch_len, m.main_len) {
                (Some(bl), Some(ml)) => m.facts.push(format!(
                    "walk {bl:.0} m via the branch vs {ml:.0} m by the main route (x{:.2})",
                    bl / ml.max(1.0)
                )),
                _ => {
                    if b.kind != BranchKind::Loop {
                        rep.errors
                            .push(format!("{label}: cannot be walked end to end"));
                    }
                }
            }
        }
        let ratio = match (m.branch_len, m.main_len) {
            (Some(bl), Some(ml)) => Some(bl / ml.max(1.0)),
            _ => None,
        };
        match b.kind {
            BranchKind::Detour => {
                if m.rewards.is_empty() {
                    rep.errors
                        .push(format!("{label}: a detour with no reward in it"));
                }
                if !m.bypasses.is_empty() {
                    rep.errors.push(format!(
                        "{label}: goes round required encounter(s) {}",
                        m.bypasses.join(", ")
                    ));
                }
                let costs = ratio.is_some_and(|r| r >= DETOUR_MIN_RATIO);
                if !costs {
                    rep.errors.push(format!(
                        "{label}: the detour is x{:.2} the main route (need at least x{DETOUR_MIN_RATIO}): its reward would lie on the shortest way",
                        ratio.unwrap_or(0.0)
                    ));
                }
                m.delivered = !m.rewards.is_empty() && m.bypasses.is_empty() && costs;
                if m.delivered {
                    m.facts.push(format!("holds {}", m.rewards.join(" and ")));
                }
            }
            BranchKind::Flank => {
                let Some(j) = b.target else {
                    rep.errors
                        .push(format!("{label}: a flank without a target"));
                    out.push(m);
                    continue;
                };
                let Some(e) = encounter_in(content, enc_rooms, j) else {
                    rep.errors
                        .push(format!("{label}: its target room holds no encounter"));
                    out.push(m);
                    continue;
                };
                let enc = &content.encounters[e];
                let (anchor, front) = anchors[e];
                let Some((stage, door)) = crate::populate::flank_staging(plan, bi) else {
                    out.push(m);
                    continue;
                };
                let d = (door - anchor).normalize_or_zero();
                let angle = (front.x * d.x + front.z * d.z)
                    .clamp(-1.0, 1.0)
                    .acos()
                    .to_degrees();
                m.angle = Some(angle);
                let margin = enc
                    .mobs
                    .iter()
                    .flat_map(|mb| {
                        std::iter::once(mb.pos)
                            .chain(mb.patrol.iter().copied())
                            .map(move |p| p.dist(stage) - aggro(&mb.template))
                    })
                    .fold(f32::INFINITY, f32::min);
                let main_door = crate::populate::staging_point(plan, j);
                let near = |from: V2| {
                    enc.mobs
                        .iter()
                        .map(|mb| mb.pos.dist(from))
                        .fold(f32::INFINITY, f32::min)
                };
                let far = |from: V2| {
                    enc.mobs
                        .iter()
                        .map(|mb| mb.pos.dist(from))
                        .fold(0.0, f32::max)
                };
                m.facts.push(format!(
                    "enters '{}' {angle:.0}° off the pack's front ({})",
                    enc.display_name(),
                    if angle >= 120.0 { "rear" } else { "side" }
                ));
                m.facts.push(format!(
                    "from the flank door the nearest member is {:.1} m and the farthest {:.1} m away; from the main door {:.1} m and {:.1} m",
                    near(door),
                    far(door),
                    near(main_door),
                    far(main_door)
                ));
                m.facts
                    .push(format!("flank staging is {margin:.1} m outside aggro"));
                // The kit's measured rating of flanking this recipe (0.9.1).
                if let Some(r) = enc
                    .id
                    .split_once('_')
                    .and_then(|(_, k)| kit.recipes.get(k))
                    .and_then(|r| r.flank)
                {
                    m.facts
                        .push(format!("the kit rates flanking this pack {}", r.label()));
                    if r == crate::kit::FlankRating::Poor {
                        m.warnings.push(
                            "the kit rates flanking this pack poor; the blueprint named it".into(),
                        );
                    }
                }
                if angle < FLANK_MIN_ANGLE {
                    rep.errors.push(format!(
                        "{label}: enters only {angle:.0}° off the pack's front (need {FLANK_MIN_ANGLE:.0}°): not a different approach"
                    ));
                }
                if margin < FLANK_MARGIN {
                    rep.errors.push(format!(
                        "{label}: arriving by the flank pulls '{}' ({margin:.1} m margin, need {FLANK_MARGIN})",
                        enc.display_name()
                    ));
                }
                if !m.bypasses.is_empty() {
                    rep.errors.push(format!(
                        "{label}: goes round required encounter(s) {}",
                        m.bypasses.join(", ")
                    ));
                }
                let short = ratio.is_some_and(|r| (FLANK_MIN_RATIO..=FLANK_MAX_RATIO).contains(&r));
                if !short {
                    rep.errors.push(format!(
                        "{label}: the flank is x{:.2} the main approach (need x{FLANK_MIN_RATIO} to x{FLANK_MAX_RATIO}): either the main door's shortcut or a detour, not an approach",
                        ratio.unwrap_or(f32::INFINITY)
                    ));
                }
                m.delivered = angle >= FLANK_MIN_ANGLE
                    && margin >= FLANK_MARGIN
                    && m.bypasses.is_empty()
                    && short;
            }
            BranchKind::Safe => {
                let hz = kit.remix.route_hazard.as_ref();
                let name = hz.map(|h| h.name.as_str()).unwrap_or("");
                let vents: Vec<(V2, f32)> = content
                    .encounters
                    .iter()
                    .flat_map(|e| e.hazards.iter())
                    .filter(|h| h.name == name)
                    .map(|h| (h.pos, h.radius))
                    .collect();
                let on_short = vents
                    .iter()
                    .filter(|(p, _)| {
                        b.hazard_corridors.iter().any(|&k| {
                            comp.areas
                                .iter()
                                .zip(&comp.owners)
                                .filter(|(_, o)| **o == crate::compose::Owner::Corridor(k))
                                .filter_map(|(a, _)| crate::compose::area_poly(a))
                                .any(|poly| poly.contains(*p))
                        })
                    })
                    .count();
                // Any hazard (of any encounter) reaching the safe way's floor?
                let safe_floor: Vec<_> = comp
                    .areas
                    .iter()
                    .zip(&comp.owners)
                    .filter(|(_, o)| match o {
                        crate::compose::Owner::Room(r) => b.rooms.contains(r),
                        crate::compose::Owner::Corridor(k) => b.corridors.contains(k),
                    })
                    .filter_map(|(a, _)| crate::compose::area_poly(a))
                    .collect();
                let exposed = content
                    .encounters
                    .iter()
                    .flat_map(|e| e.hazards.iter())
                    .filter(|h| {
                        safe_floor.iter().any(|poly| {
                            poly.contains(h.pos) || h.pos.dist(poly.clamp(h.pos)) < h.radius
                        })
                    })
                    .count();
                m.facts.push(format!(
                    "the short way crosses {on_short} {name}(s); the long way is {}",
                    if exposed == 0 {
                        "hazard-free".to_string()
                    } else {
                        format!("exposed to {exposed} hazard(s)")
                    }
                ));
                if on_short == 0 {
                    rep.errors.push(format!(
                        "{label}: the short way carries no {name}: nothing to be safe from"
                    ));
                }
                if exposed > 0 {
                    rep.errors.push(format!(
                        "{label}: the safe way is exposed to {exposed} hazard(s)"
                    ));
                }
                if let Some(r) = ratio
                    && r > SAFE_MAX_RATIO
                {
                    rep.errors.push(format!(
                        "{label}: the safe way is x{r:.2} the short way (at most x{SAFE_MAX_RATIO}): too long to be a real alternative"
                    ));
                }
                if let Some(r) = ratio
                    && r < SAFE_MIN_RATIO
                {
                    rep.errors.push(format!(
                        "{label}: the safe way is only x{r:.2} the short way (need x{SAFE_MIN_RATIO}): no trade-off"
                    ));
                }
                m.delivered = on_short > 0
                    && exposed == 0
                    && ratio.is_some_and(|r| (SAFE_MIN_RATIO..=SAFE_MAX_RATIO).contains(&r));
            }
            BranchKind::Optional | BranchKind::Cache => {
                if m.rewards.is_empty() {
                    rep.errors.push(format!("{label}: holds nothing"));
                }
                m.delivered = !m.rewards.is_empty();
                if m.delivered {
                    m.facts.push(format!(
                        "holds {}, {} room(s) deep",
                        m.rewards.join(" and "),
                        b.rooms.len()
                    ));
                }
            }
            BranchKind::Vista | BranchKind::Quiet => {
                m.delivered = true;
                m.facts.push(format!(
                    "{} room ({} deep): atmosphere, no reward",
                    if b.kind == BranchKind::Vista {
                        "landmark"
                    } else {
                        "deliberately quiet"
                    },
                    b.rooms.len()
                ));
            }
            BranchKind::Drop => {
                // Every room the drop skips can be walked back to.
                if let (Some(pf), Some(to)) = (main_pos(plan, b.from), b.to)
                    && let Some(pt) = main_pos(plan, to)
                {
                    let landing = room_point(plan, comp, to);
                    let stranded: Vec<String> = plan.main[pf + 1..pt]
                        .iter()
                        .filter(|&&r| walk(nav, landing, room_point(plan, comp, r)).is_none())
                        .map(|&r| room_name(plan, r))
                        .collect();
                    if !stranded.is_empty() {
                        rep.errors.push(format!(
                            "{label}: after the drop, {} cannot be walked back to",
                            stranded.join(", ")
                        ));
                    }
                    if !m.bypasses.is_empty() {
                        m.warnings.push(format!(
                            "the drop lands past required {}: walk back for them",
                            m.bypasses.join(", ")
                        ));
                    }
                }
                let saves = ratio.is_some_and(|r| r <= DROP_MAX_RATIO);
                m.delivered = saves && m.bypasses.is_empty();
                if saves {
                    m.facts
                        .push("a one-way shortcut down: shorter than the main route".into());
                } else if strict {
                    rep.errors.push(format!(
                        "{label}: the drop saves no walking (x{:.2}, need at most x{DROP_MAX_RATIO})",
                        ratio.unwrap_or(f32::INFINITY)
                    ));
                }
            }
            BranchKind::Loop => {
                // A way back, not a way round.
                m.bypasses.clear();
                m.delivered = true;
                m.facts
                    .push("a walkable way back to an earlier room".into());
            }
            BranchKind::Unspecified => {
                m.warnings.push(if b.to.is_some() {
                    "alternate route with no declared purpose (legacy layout.alternate_routes)"
                        .into()
                } else {
                    "empty dead end with no declared purpose (legacy layout.side_rooms)".into()
                });
                if !m.bypasses.is_empty() {
                    m.warnings.push(format!(
                        "goes round required {}, which must be cleared anyway (backtracking)",
                        m.bypasses.join(", ")
                    ));
                }
            }
        }
        out.push(m);
    }
    // Progression: the final boss can be reached from every required room.
    let fin = *plan.main.last().expect("main route");
    let goal = room_point(plan, comp, fin);
    let mut stuck = Vec::new();
    for &r in &plan.main {
        if matches!(plan.rooms[r].encounter, Some(EncounterRef::Main(_)))
            && r != fin
            && walk(nav, room_point(plan, comp, r), goal).is_none()
        {
            stuck.push(room_name(plan, r));
        }
    }
    if stuck.is_empty() {
        rep.passed
            .push("the final boss can be reached from every required room".into());
    } else {
        rep.errors.push(format!(
            "the final boss cannot be reached from {}",
            stuck.join(", ")
        ));
    }
    let delivered = out.iter().filter(|m| m.delivered).count();
    rep.passed.push(format!(
        "{delivered}/{} branches deliver their purpose",
        out.len()
    ));
    (rep, out)
}

/// `[[dungeon.branches]]` from the plan and the measurements.
pub fn branch_configs(plan: &Plan, measured: &[Measured]) -> Vec<BranchConfig> {
    plan.branches
        .iter()
        .zip(measured)
        .enumerate()
        .map(|(k, (b, m))| {
            let mut rooms = Vec::new();
            // Corridors and rooms interleaved, from the main room outward.
            for (t, &c) in b.corridors.iter().enumerate() {
                rooms.push(format!("c{c}"));
                if let Some(&r) = b.rooms.get(t) {
                    rooms.push(format!("r{r}"));
                }
            }
            BranchConfig {
                id: format!("b{k}"),
                purpose: b.kind.label().into(),
                from: format!("r{}", b.from),
                to: b.to.map(|t| format!("r{t}")),
                rooms,
                target: b.target.map(|t| format!("r{t}")),
                rewards: m.rewards.clone(),
                bypasses: m.bypasses.clone(),
                facts: m.facts.clone(),
                warnings: m.warnings.clone(),
            }
        })
        .collect()
}
