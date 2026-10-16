//! Layer C: encounters from recipes, placed by the placement scorer
//! (`placement.rs`) using the room's real floor, its exits, the navigation
//! (for patrol routes and sightlines) and the packs already placed; then
//! hazards, loot sources and rewards.

use crate::blueprint::Blueprint;
use crate::compose::Composed;
use crate::kit::{HazardPlace, Kit};
use crate::layout::{BranchKind, EdgeRole, EncounterRef, Plan, Room};
use crate::placement::{self, FLOOR_MARGIN, Placed, RoomView};
use dungeon_sim::content::{CacheConfig, EncounterConfig, HazardConfig, LootEntry, MobSpawn};
use dungeon_sim::math::V2;
use dungeon_sim::nav::Nav;
use dungeon_sim::poly::Poly;

pub struct Populated {
    pub encounters: Vec<EncounterConfig>,
    /// Room index holding each encounter.
    pub rooms: Vec<usize>,
    /// Where each encounter's pack is centred, and which way it faces.
    pub anchors: Vec<(V2, V2)>,
    pub loot: Vec<LootEntry>,
    /// Treasure caches (0.9) and the room of each.
    pub caches: Vec<CacheConfig>,
    pub cache_rooms: Vec<usize>,
}

fn r2(v: V2) -> V2 {
    V2::new((v.x * 100.0).round() / 100.0, (v.z * 100.0).round() / 100.0)
}

/// Pull `p` toward `toward` until it stands on the floor with margin.
fn onto_floor(p: V2, toward: V2, floors: &[Poly]) -> V2 {
    let inset: Vec<Poly> = floors
        .iter()
        .filter_map(|f| f.inset(FLOOR_MARGIN + 0.2))
        .collect();
    let on = |q: V2| inset.iter().any(|f| f.contains(q));
    if on(p) {
        return p;
    }
    for k in 1..=20 {
        let q = p + (toward - p) * (k as f32 / 20.0);
        if on(q) {
            return q;
        }
    }
    inset
        .iter()
        .map(|f| f.clamp(p))
        .min_by(|a, b| a.dist(p).total_cmp(&b.dist(p)))
        .unwrap_or(p)
}

/// Where corridor `k` meets room `r` (world), and the direction out of the room.
pub fn mouth(plan: &Plan, k: usize, r: usize) -> (V2, V2) {
    let c = &plan.corridors[k];
    let room: &Room = &plan.rooms[r];
    let v = c.dir.lateral();
    if c.a == r {
        let u = c.dir.unit();
        (room.center() + u * room.ext(c.dir) + v * c.off_a, u)
    } else {
        let u = c.dir.opposite().unit();
        (
            room.center() + u * room.ext(c.dir.opposite()) + v * c.off_b,
            u,
        )
    }
}

/// Where the party waits before pulling the encounter in room `r`.
pub fn staging_point(plan: &Plan, r: usize) -> V2 {
    let dummy: Vec<Poly> = Vec::new();
    view_with(plan, &dummy, r).staging
}

/// Where a party arriving by flank `b` waits before the pull: 4 m back up
/// the flank's last corridor from the target room's door.
pub fn flank_staging(plan: &Plan, b: usize) -> Option<(V2, V2)> {
    let br = &plan.branches[b];
    let j = br.target?;
    let k = *br.corridors.last()?;
    let (m, out) = mouth(plan, k, j);
    Some((m + out * 4.0, m))
}

/// The room as placement sees it: floor, entry mouth, other mouths, staging.
fn view<'a>(plan: &'a Plan, comp: &'a Composed, r: usize) -> RoomView<'a> {
    view_with(plan, &comp.floors[r], r)
}

fn view_with<'a>(plan: &'a Plan, floors: &'a [Poly], r: usize) -> RoomView<'a> {
    let touching: Vec<usize> = (0..plan.corridors.len())
        .filter(|&k| plan.corridors[k].a == r || plan.corridors[k].b == r)
        .collect();
    // The way in: the corridor that brings the party here first.
    let entry = touching
        .iter()
        .copied()
        .find(|&k| {
            let c = &plan.corridors[k];
            c.b == r
                && matches!(
                    c.role,
                    EdgeRole::Main | EdgeRole::Side | EdgeRole::Alternate
                )
        })
        .or_else(|| touching.first().copied());
    let (entry_mouth, out_dir) = entry
        .map(|k| mouth(plan, k, r))
        .unwrap_or((plan.rooms[r].center(), V2::new(0.0, -1.0)));
    RoomView {
        room: &plan.rooms[r],
        floors,
        entry_mouth,
        other_mouths: touching
            .iter()
            .filter(|&&k| Some(k) != entry)
            .map(|&k| mouth(plan, k, r).0)
            .collect(),
        staging: entry_mouth + out_dir * 4.0,
        // A flank into this room must also stage out of the pack's aggro.
        extra_staging: (0..plan.branches.len())
            .filter(|&b| plan.branches[b].target == Some(r))
            .filter_map(|b| flank_staging(plan, b).map(|(s, _)| s))
            .collect(),
    }
}

pub fn populate(
    bp: &Blueprint,
    kit: &Kit,
    plan: &Plan,
    comp: &Composed,
    nav: &Nav,
) -> Result<Populated, String> {
    let mut encounters = Vec::new();
    let mut rooms = Vec::new();
    let mut anchors = Vec::new();
    // Main encounters in route order, each optional one right after the
    // encounter of the main room its branch leaves from.
    let mut order: Vec<(usize, EncounterRef)> = Vec::new();
    for &m in &plan.main {
        if let Some(e) = plan.rooms[m].encounter {
            order.push((m, e));
        }
        for b in plan.branches.iter().filter(|b| b.from == m) {
            for &r in &b.rooms {
                if let Some(e @ EncounterRef::Optional(_)) = plan.rooms[r].encounter {
                    order.push((r, e));
                }
            }
        }
    }
    // The final boss must be the last required encounter: move optional
    // fights that would come after it in front of it.
    if let Some(final_pos) = order
        .iter()
        .rposition(|(_, e)| matches!(e, EncounterRef::Main(_)))
    {
        let tail: Vec<(usize, EncounterRef)> = order.drain(final_pos + 1..).collect();
        let fin = order.pop().expect("final");
        order.extend(tail);
        order.push(fin);
    }
    let aggro = |key: &str| {
        kit.templates
            .get(key)
            .map(|t| t.aggro_radius)
            .unwrap_or(10.0)
    };
    let mut placed: Vec<Placed> = Vec::new();
    for (room, eref) in order {
        let (slot, id, optional) = match eref {
            EncounterRef::Main(k) => (
                &bp.encounters[k],
                format!("e{}_{}", k + 1, bp.encounters[k].recipe),
                false,
            ),
            EncounterRef::Optional(k) => (
                &bp.optional[k],
                format!("o{}_{}", k + 1, bp.optional[k].recipe),
                true,
            ),
        };
        let recipe = &kit.recipes[&slot.recipe];
        let r = &plan.rooms[room];
        let rv = view(plan, comp, room);
        if recipe.space.pillars && comp.pillars[room].is_empty() {
            return Err(format!("{} (no pillars in '{}')", slot.recipe, r.name));
        }
        let choice = placement::place(nav, &rv, recipe, &aggro, &placed, &[])
            .ok_or_else(|| format!("{} in {:?} '{}'", slot.recipe, r.kind, r.name))?;
        let mobs: Vec<MobSpawn> = choice
            .spots
            .iter()
            .map(|s| MobSpawn {
                template: s.template.clone(),
                pos: r2(s.pos),
                y: Some(r.y),
                patrol: s.patrol.iter().map(|p| r2(*p)).collect(),
            })
            .collect();
        for s in &choice.spots {
            let a = aggro(&s.template);
            placed.push(Placed {
                pos: s.pos,
                y: r.y,
                aggro: a,
            });
            for p in &s.patrol {
                placed.push(Placed {
                    pos: *p,
                    y: r.y,
                    aggro: a,
                });
            }
        }
        // Hazards: on the approach (between the door and the pack) or either
        // side of the pack, kept off the pack itself.
        let f = choice.toward_entry;
        let v = V2::new(-f.z, f.x);
        let mut hazards = Vec::new();
        for h in &recipe.hazards {
            let base = match h.place {
                HazardPlace::Approach => choice.anchor + (rv.entry_mouth - choice.anchor) * 0.45,
                HazardPlace::Flanks => choice.anchor,
            };
            let across = match h.place {
                HazardPlace::Approach => 3.0 + h.radius,
                HazardPlace::Flanks => 4.5 + h.radius,
            };
            for (k, side) in [1.0f32, -1.0].into_iter().enumerate() {
                let mut p = onto_floor(base + v * (side * across), base, rv.floors);
                // Never on top of a pack member.
                if choice.spots.iter().any(|s| s.pos.dist(p) < h.radius + 1.0) {
                    p = onto_floor(base + v * (side * (across + 3.0)), base, rv.floors);
                }
                hazards.push(HazardConfig {
                    name: h.name.clone(),
                    pos: r2(p),
                    y: Some(r.y),
                    radius: h.radius,
                    damage: h.damage,
                    school: h.school,
                    on: h.on,
                    off: h.off,
                    // Alternate the two vents so there is always a safe side.
                    offset: if k == 1 { (h.on + h.off) / 2.0 } else { 0.0 },
                });
            }
        }
        // A single boss names its encounter; a council keeps the recipe's name.
        let bosses: Vec<&String> = recipe
            .mobs
            .iter()
            .filter(|m| {
                kit.templates.get(&m.template).map(|t| t.rank)
                    == Some(dungeon_sim::content::EnemyRank::Boss)
            })
            .map(|m| &kit.templates[&m.template].name)
            .collect();
        let boss_name = (bosses.len() == 1).then(|| bosses[0].clone());
        let name = slot
            .name
            .clone()
            .or(boss_name)
            .unwrap_or_else(|| recipe.name.clone());
        encounters.push(EncounterConfig {
            id,
            name,
            optional,
            mobs,
            hazards,
        });
        rooms.push(room);
        anchors.push((choice.anchor, f));
    }

    // Treasure caches: at the far side of their room from the way in, clear
    // of any pack standing there.
    let mut caches = Vec::new();
    let mut cache_rooms = Vec::new();
    if let Some(spec) = &kit.remix.cache {
        for (ri, r) in plan.rooms.iter().enumerate().filter(|(_, r)| r.cache) {
            let rv = view(plan, comp, ri);
            let mobs: Vec<V2> = encounters
                .iter()
                .zip(&rooms)
                .filter(|(_, er)| **er == ri)
                .flat_map(|(e, _)| e.mobs.iter().map(|m| m.pos))
                .collect();
            let spot = placement::lattice(&rv, FLOOR_MARGIN + spec.prop.radius + 0.3)
                .into_iter()
                .filter(|p| mobs.iter().all(|m| m.dist(*p) >= 3.0))
                .filter(|p| rv.other_mouths.iter().all(|m| m.dist(*p) >= 3.0))
                .max_by(|a, b| {
                    a.dist(rv.entry_mouth)
                        .total_cmp(&b.dist(rv.entry_mouth))
                        .then(b.x.total_cmp(&a.x))
                        .then(b.z.total_cmp(&a.z))
                })
                .ok_or_else(|| format!("cache in '{}' (no floor clear of the pack)", r.name))?;
            let k = caches.len() + 1;
            let face = rv.entry_mouth - spot;
            caches.push(CacheConfig {
                id: format!("c{k}_cache"),
                name: spec.name.clone(),
                pos: r2(spot),
                y: Some(r.y),
                radius: spec.radius,
                model: Some(spec.prop.model.clone()),
                yaw: ((-face.x).atan2(-face.z).to_degrees() * 10.0).round() / 10.0,
            });
            cache_rooms.push(ri);
        }
    }

    // Route hazards on the short way of each safe route: vents along the
    // main corridors, owned by the next required encounter (they die with
    // it), alternating so there is always a moment to cross.
    if let Some(h) = &kit.remix.route_hazard {
        for b in plan.branches.iter().filter(|b| b.kind == BranchKind::Safe) {
            let Some(to) = b.to else { continue };
            let pos_to = plan.main.iter().position(|&m| m == to).unwrap_or(0);
            let Some(owner) = plan.main[pos_to..].iter().find_map(|&m| {
                (matches!(plan.rooms[m].encounter, Some(EncounterRef::Main(_))))
                    .then(|| rooms.iter().position(|&er| er == m))
                    .flatten()
            }) else {
                continue;
            };
            // Keep vents off the spot where the party waits for the pull.
            let staging = staging_point(
                plan,
                plan.main[pos_to..]
                    .iter()
                    .copied()
                    .find(|&m| plan.rooms[m].encounter.is_some())
                    .unwrap_or(to),
            );
            // Each vent covers its corridor wall to wall (no edge to sidle
            // past); vents keep a safe pause spot between them.
            let mut vents: Vec<(V2, f32, f32)> = Vec::new();
            for &k in &b.hazard_corridors {
                let c = &plan.corridors[k];
                let radius = h.radius.max(c.width * 0.5 + 0.4);
                let spacing = h.spacing.max(2.0 * radius + 2.5);
                let (ma, _) = mouth(plan, k, c.a);
                let (mb, _) = mouth(plan, k, c.b);
                let pieces: Vec<Poly> = comp
                    .areas
                    .iter()
                    .zip(&comp.owners)
                    .filter(|(_, o)| **o == crate::compose::Owner::Corridor(k))
                    .filter_map(|(a, _)| crate::compose::area_poly(a))
                    .collect();
                let len = ma.dist(mb).max(1.0);
                let steps = (len / 0.5).ceil() as usize;
                let mut cands: Vec<V2> = (0..=steps)
                    .map(|t| ma + (mb - ma) * (t as f32 / steps as f32))
                    .collect();
                cands.extend(pieces.iter().map(|p| p.centroid()));
                let inset: Vec<Poly> = pieces.iter().filter_map(|p| p.inset(0.8)).collect();
                for p in cands {
                    if !inset.iter().any(|q| q.contains(p))
                        || p.dist(staging) < radius + 3.0
                        || p.dist(ma) < radius + 1.0
                        || p.dist(mb) < radius + 1.0
                        || vents
                            .iter()
                            .any(|(v, _, r)| v.dist(p) < spacing.max(r + radius + 2.5))
                    {
                        continue;
                    }
                    let Some(s) = nav.locate(p, plan.rooms[c.a].y.max(plan.rooms[c.b].y) + 1.0)
                    else {
                        continue;
                    };
                    vents.push((p, nav.height(s, p), radius));
                    if vents.len() >= 6 {
                        break;
                    }
                }
            }
            for (n, (p, y, radius)) in vents.into_iter().enumerate() {
                encounters[owner].hazards.push(HazardConfig {
                    name: h.name.clone(),
                    pos: r2(p),
                    y: Some((y * 100.0).round() / 100.0),
                    radius: (radius * 10.0).round() / 10.0,
                    damage: h.damage,
                    school: h.school,
                    on: h.on,
                    off: h.off,
                    offset: if n % 2 == 1 {
                        (h.on + h.off) / 2.0
                    } else {
                        0.0
                    },
                });
            }
        }
    }

    // Loot: kit sources are roles; map them to encounter ids here.
    let final_id = encounters
        .iter()
        .rfind(|e| !e.optional)
        .map(|e| e.id.clone())
        .expect("a final encounter");
    let has_boss = |e: &EncounterConfig| {
        e.mobs.iter().any(|m| {
            kit.templates.get(&m.template).map(|t| t.rank)
                == Some(dungeon_sim::content::EnemyRank::Boss)
        })
    };
    let mid_boss = encounters
        .iter()
        .find(|e| !e.optional && e.id != final_id && has_boss(e))
        .map(|e| e.id.clone());
    let optionals: Vec<String> = encounters
        .iter()
        .filter(|e| e.optional)
        .map(|e| e.id.clone())
        .collect();
    let first_pack = encounters
        .iter()
        .find(|e| !e.optional && !has_boss(e))
        .map(|e| e.id.clone());
    let mut loot = Vec::new();
    for l in &kit.loot {
        let base = format!("{}_{}", bp.blueprint.id, l.id);
        // Every optional encounter and every cache gets the pool (0.9); the
        // first keeps the plain id, so items already in a stash still match.
        let fanned = |ids: &[String], loot: &mut Vec<LootEntry>| {
            for (k, src) in ids.iter().enumerate() {
                loot.push(LootEntry {
                    id: if k == 0 {
                        base.clone()
                    } else {
                        format!("{base}_{}", src.split('_').next().unwrap_or("x"))
                    },
                    source: Some(src.clone()),
                    ..l.clone()
                });
            }
        };
        match l.source.as_deref() {
            Some("optional") if !optionals.is_empty() => fanned(&optionals, &mut loot),
            Some("cache") => {
                let ids: Vec<String> = caches.iter().map(|c| c.id.clone()).collect();
                fanned(&ids, &mut loot);
            }
            other => {
                let source = match other {
                    Some("boss") => mid_boss.clone(),
                    Some("pack") => first_pack.clone(),
                    _ => None,
                }
                .unwrap_or_else(|| final_id.clone());
                loot.push(LootEntry {
                    id: base,
                    source: Some(source),
                    ..l.clone()
                });
            }
        }
    }
    Ok(Populated {
        encounters,
        rooms,
        anchors,
        loot,
        caches,
        cache_rooms,
    })
}
