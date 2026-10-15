//! Layer C: encounters from recipes, placed relative to the direction the
//! party enters each room; loot sources and rewards.

use crate::blueprint::Blueprint;
use crate::compose::Composed;
use crate::kit::{HazardPlace, Kit, Placement};
use crate::layout::{EdgeRole, EncounterRef, Plan, RoomKind};
use dungeon_sim::content::{EncounterConfig, HazardConfig, LootEntry, MobSpawn};
use dungeon_sim::math::V2;
use dungeon_sim::poly::Poly;

/// Keep mobs and hazards this far inside a room's floor.
const FLOOR_MARGIN: f32 = 1.2;
/// Sideways spacing between mobs in a row.
const SPACING: f32 = 2.4;

pub struct Populated {
    pub encounters: Vec<EncounterConfig>,
    /// Room index holding each encounter.
    pub rooms: Vec<usize>,
    pub loot: Vec<LootEntry>,
}

fn r2(v: V2) -> V2 {
    V2::new((v.x * 100.0).round() / 100.0, (v.z * 100.0).round() / 100.0)
}

/// Pull `p` toward the room centre until it stands on the floor with margin.
fn onto_floor(p: V2, center: V2, floors: &[Poly]) -> V2 {
    let inset: Vec<Poly> = floors
        .iter()
        .filter_map(|f| f.inset(FLOOR_MARGIN))
        .collect();
    let on = |q: V2| inset.iter().any(|f| f.contains(q));
    if on(p) {
        return p;
    }
    // Walk toward the centre, then slide sideways around a pit.
    for k in 1..=20 {
        let q = p + (center - p) * (k as f32 / 20.0);
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

pub fn populate(bp: &Blueprint, kit: &Kit, plan: &Plan, comp: &Composed) -> Populated {
    let mut encounters = Vec::new();
    let mut rooms = Vec::new();
    // Main encounters in route order, each optional one right after the
    // encounter of the room its side corridor branches from.
    let mut order: Vec<(usize, EncounterRef)> = Vec::new();
    for &m in &plan.main {
        if let Some(e) = plan.rooms[m].encounter {
            order.push((m, e));
        }
        for c in &plan.corridors {
            if c.role == EdgeRole::Side
                && c.a == m
                && let Some(e) = plan.rooms[c.b].encounter
            {
                order.push((c.b, e));
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
        let c = r.center();
        let u = r.entry.map(|d| d.unit()).unwrap_or(V2::new(0.0, 1.0));
        let v = V2::new(-u.z, u.x);
        // Pack anchor: the far side of the room (the party stages at the door).
        let anchor = match r.kind {
            RoomKind::Pit => c + u * ((r.half + r.pit) / 2.0),
            RoomKind::Arena => c + u * (0.15 * r.half),
            _ => c + u * (0.3 * r.half),
        };
        let row_gap = if r.kind == RoomKind::Pit { 1.5 } else { 1.8 };
        let mut mobs = Vec::new();
        let place_row = |base: V2, n: u32, template: &str, mobs: &mut Vec<MobSpawn>| {
            for i in 0..n {
                let t = (i as f32 - (n as f32 - 1.0) / 2.0) * SPACING;
                let p = onto_floor(base + v * t, c, &comp.floors[room]);
                mobs.push(MobSpawn {
                    template: template.to_string(),
                    pos: r2(p),
                    y: Some(r.y),
                });
            }
        };
        for m in &recipe.mobs {
            match m.place {
                Placement::Front => {
                    place_row(anchor - u * row_gap, m.count, &m.template, &mut mobs)
                }
                Placement::Back => place_row(anchor + u * row_gap, m.count, &m.template, &mut mobs),
                Placement::Center => place_row(anchor, m.count, &m.template, &mut mobs),
                Placement::Flank => {
                    for i in 0..m.count {
                        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
                        let dist = 3.5 + 2.0 * (i / 2) as f32;
                        let p = onto_floor(anchor + v * (side * dist), c, &comp.floors[room]);
                        mobs.push(MobSpawn {
                            template: m.template.clone(),
                            pos: r2(p),
                            y: Some(r.y),
                        });
                    }
                }
            }
        }
        let mut hazards = Vec::new();
        for h in &recipe.hazards {
            let (along, across) = match h.place {
                HazardPlace::Approach => (-0.25 * r.half, 0.45 * r.half),
                HazardPlace::Flanks => (0.0, 0.5 * r.half),
            };
            let base = if h.place == HazardPlace::Flanks {
                anchor
            } else {
                c
            };
            for (k, side) in [1.0f32, -1.0].into_iter().enumerate() {
                let p = onto_floor(
                    base + u * along + v * (side * across),
                    c,
                    &comp.floors[room],
                );
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
        let boss_name = recipe
            .mobs
            .iter()
            .find(|m| {
                kit.templates.get(&m.template).map(|t| t.rank)
                    == Some(dungeon_sim::content::EnemyRank::Boss)
            })
            .map(|m| kit.templates[&m.template].name.clone());
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
    let first_optional = encounters.iter().find(|e| e.optional).map(|e| e.id.clone());
    let first_pack = encounters
        .iter()
        .find(|e| !e.optional && !has_boss(e))
        .map(|e| e.id.clone());
    let loot = kit
        .loot
        .iter()
        .map(|l| {
            let source = match l.source.as_deref() {
                Some("boss") => mid_boss.clone(),
                Some("optional") => first_optional.clone(),
                Some("pack") => first_pack.clone(),
                _ => None,
            }
            .unwrap_or_else(|| final_id.clone());
            LootEntry {
                id: format!("{}_{}", bp.blueprint.id, l.id),
                source: Some(source),
                ..l.clone()
            }
        })
        .collect();
    Populated {
        encounters,
        rooms,
        loot,
    }
}
