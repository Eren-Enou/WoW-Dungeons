//! One tooltip framework for the hub and the dungeon HUD.
//!
//! Any UI node with a [`Tooltip`] shows a single shared panel after a short
//! hover. The text comes from `dungeon_sim::describe` (generated from the real
//! definitions) or is resolved live (party members, the target's auras). The
//! panel follows the cursor, flips/clamps to stay on screen, sits above all UI
//! and is never pickable, so it can't steal clicks.

use crate::common::{GameData, text_font};
use crate::dungeon::ActiveRun;
use crate::hub::{HubAction, HubUi};
use bevy::picking::Pickable;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::{ComputedNode, UiGlobalTransform};
use dungeon_sim::describe::{self, Tip};
use dungeon_sim::{ActorId, Sim};

pub struct TooltipPlugin;

impl Plugin for TooltipPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_panel)
            .add_systems(PostUpdate, show_tooltip.before(bevy::ui::UiSystems::Layout));
    }
}

/// Seconds the cursor must rest on something before its tooltip appears.
const DELAY: f32 = 0.2;
const MAX_WIDTH: f32 = 340.0;

#[derive(Component, Clone)]
#[require(Hovered)]
pub enum Tooltip {
    Text(Tip),
    /// The player's ability in this action-bar slot.
    Ability(usize),
    /// Party member `i` (live health, resource and auras).
    PartyMember(usize),
    /// The selected target (live auras, threat colours).
    Target,
    /// Resolved from the entity's `HubAction`.
    Hub,
}

impl Tooltip {
    pub fn text(title: impl Into<String>, body: impl Into<String>) -> Tooltip {
        Tooltip::Text(Tip::new(title, body))
    }
}

#[derive(Component)]
pub struct TooltipPanel;

#[derive(Component)]
pub struct TooltipTitle;

#[derive(Component)]
struct TooltipBody;

fn spawn_panel(mut commands: Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: px(MAX_WIDTH),
            flex_direction: FlexDirection::Column,
            row_gap: px(3),
            padding: UiRect::all(px(7)),
            border: UiRect::all(px(1)),
            display: Display::None,
            ..default()
        },
        BackgroundColor(Color::srgba(0.03, 0.03, 0.07, 0.95)),
        BorderColor::all(Color::srgb(0.6, 0.55, 0.35)),
        GlobalZIndex(10_000),
        Pickable::IGNORE,
        TooltipPanel,
        children![
            (
                Text::new(""),
                text_font(14.0),
                TextColor(Color::srgb(1.0, 0.88, 0.5)),
                TooltipTitle,
                Pickable::IGNORE,
            ),
            (
                Text::new(""),
                text_font(12.0),
                TextColor(Color::srgb(0.9, 0.9, 0.9)),
                TooltipBody,
                Pickable::IGNORE,
            ),
        ],
    ));
}

fn party_member_tip(sim: &Sim, id: ActorId) -> Tip {
    let a = &sim.actors[id];
    let class = a
        .class
        .map(|c| format!(" - {} {}", c.label(), c.role().label()))
        .unwrap_or_default();
    let mut lines = vec![format!(
        "Health {:.0}/{:.0} | {} {:.0}/{:.0}",
        a.hp.max(0.0),
        a.max_hp,
        a.resource_kind.label(),
        a.resource,
        a.max_resource
    )];
    if !a.alive {
        lines.push("Dead - fallen members rally 8 s after the party leaves combat.".into());
    }
    aura_lines(sim, id, &mut lines);
    lines.push("Click to select. Friendly spells go to whoever is under the mouse.".into());
    Tip::new(format!("{}{class}", a.name), lines.join("\n"))
}

fn aura_lines(sim: &Sim, id: ActorId, lines: &mut Vec<String>) {
    for x in &sim.actors[id].auras {
        let src = sim
            .actors
            .get(x.source)
            .map(|s| s.name.as_str())
            .unwrap_or("");
        let t = describe::aura(x, src);
        let mut body = t.body.lines();
        let effect = body.next().unwrap_or_default();
        let left = body.next().unwrap_or_default();
        let dispel = if x.harmful && x.dispellable {
            " [dispellable]"
        } else {
            ""
        };
        let stacks = if x.stacks > 1 {
            format!(" x{}", x.stacks)
        } else {
            String::new()
        };
        lines.push(format!(
            "{} {}{stacks}{dispel}: {effect} ({left})",
            if x.harmful { "-" } else { "+" },
            t.title
        ));
    }
}

fn target_tip(sim: &Sim) -> Option<Tip> {
    let p = sim.player?;
    let t = sim.actors[p].target?;
    let a = &sim.actors[t];
    if !a.is_enemy() {
        return Some(party_member_tip(sim, t));
    }
    let mut lines = vec![format!(
        "Level {} {}{}",
        a.stats.level,
        a.rank().map(|r| format!("{r:?}")).unwrap_or_default(),
        if a.enemy.as_ref().is_some_and(|e| e.cc_immune) {
            " - immune to crowd control"
        } else {
            ""
        }
    )];
    aura_lines(sim, t, &mut lines);
    lines.push(if sim.is_tank(p) {
        "Plate colours (tank view): green = secure on you, yellow = someone is close to pulling, blue = another tank, red = attacking a non-tank (taunt!)".into()
    } else {
        "Plate colours: yellow = you are close to pulling threat (110% in melee, 130% at range), red = it is attacking you.".into()
    });
    Some(Tip::new(a.name.clone(), lines.join("\n")))
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn show_tooltip(
    time: Res<Time>,
    window: Single<&Window>,
    sources: Query<(
        Entity,
        &Tooltip,
        &Hovered,
        &ComputedNode,
        &UiGlobalTransform,
        Option<&HubAction>,
    )>,
    run: Option<Res<ActiveRun>>,
    data: Res<GameData>,
    hub: Res<HubUi>,
    mut panel: Single<(&mut Node, &ComputedNode), With<TooltipPanel>>,
    mut title: Single<&mut Text, With<TooltipTitle>>,
    mut body: Single<&mut Text, (With<TooltipBody>, Without<TooltipTitle>)>,
    mut hovered_for: Local<(Option<Entity>, f32)>,
) {
    // The innermost hovered source wins (smallest node).
    let best = sources
        .iter()
        .filter(|(_, _, h, n, _, _)| h.get() && !n.is_empty())
        .min_by(|a, b| {
            let area = |n: &ComputedNode| n.size().x * n.size().y;
            area(a.3).total_cmp(&area(b.3))
        });
    let entity = best.map(|b| b.0);
    if hovered_for.0 != entity {
        *hovered_for = (entity, 0.0);
    } else {
        hovered_for.1 += time.delta_secs();
    }
    let tip = match best {
        Some((_, tip, _, _, _, action)) if hovered_for.1 >= DELAY => match tip {
            Tooltip::Text(t) => Some(t.clone()),
            Tooltip::Hub => action.and_then(|a| crate::hub::hub_tooltip(a, &data, &hub)),
            Tooltip::Ability(slot) => run.as_ref().and_then(|r| {
                let p = r.sim.player?;
                let a = &r.sim.actors[p];
                let def = a.abilities.get(*slot)?.def();
                Some(describe::ability(&def, a.power_mult, a.resource_kind))
            }),
            Tooltip::PartyMember(i) => run
                .as_ref()
                .and_then(|r| r.sim.party.get(*i).map(|&id| party_member_tip(&r.sim, id))),
            Tooltip::Target => run.as_ref().and_then(|r| target_tip(&r.sim)),
        },
        _ => None,
    };
    let (node, computed) = &mut *panel;
    let Some(tip) = tip else {
        if node.display != Display::None {
            node.display = Display::None;
        }
        return;
    };
    if title.0 != tip.title {
        title.0 = tip.title;
    }
    if body.0 != tip.body {
        body.0 = tip.body;
    }
    node.display = Display::Flex;
    // Place beside the cursor, flipping to the other side near the edges.
    // Without a cursor (e.g. synthetic pointer input) anchor on the element.
    let Some(cursor) = window
        .cursor_position()
        .or_else(|| best.map(|(_, _, _, n, gt, _)| gt.translation * n.inverse_scale_factor()))
    else {
        return;
    };
    let size = computed.size() * computed.inverse_scale_factor();
    let (w, h) = (window.width(), window.height());
    let mut x = cursor.x + 18.0;
    if x + size.x > w - 4.0 {
        x = cursor.x - size.x - 12.0;
    }
    let mut y = cursor.y + 18.0;
    if y + size.y > h - 4.0 {
        y = cursor.y - size.y - 12.0;
    }
    node.left = px(x.clamp(4.0, (w - size.x - 4.0).max(4.0)));
    node.top = px(y.clamp(4.0, (h - size.y - 4.0).max(4.0)));
}
