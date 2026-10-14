//! In-dungeon HUD: clickable party/raid frames, target and target-of-target
//! frames with threat, boss frame, action bar (cooldown, GCD, range and resource
//! states), resource readout, cast bar, combat log, run info and banners.
//!
//! Everything shown is read from the simulation (`ability_state`, `threat_view`,
//! `resource_rate`, ...) so the HUD can never disagree with what a key press does.
//! The HUD also owns the *mouseover* target used by friendly spells.

use crate::camera::MainCamera;
use crate::common::{AppState, text_font};
use crate::controls::{BlocksWorld, ControlSet, PlayerControl};
use crate::dungeon::{ActiveRun, pick_actor, player_input};
use crate::tooltip::Tooltip;
use bevy::picking::Pickable;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button as UiButton};
use dungeon_sim::class::ResourceKind;
use dungeon_sim::sim::{AbilityState, Actor, CastKind, PersonalThreat, TankThreat};
use dungeon_sim::{ActorId, CastError, EnemyMode, Sim};

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Mouseover>()
            .add_observer(on_hud_click)
            .add_systems(
                Update,
                update_mouseover
                    .after(ControlSet)
                    .before(player_input)
                    .run_if(in_state(AppState::Dungeon))
                    .run_if(resource_exists::<ActiveRun>),
            )
            .add_systems(
                Update,
                (
                    update_texts,
                    update_bars,
                    update_action_bar,
                    style_frames,
                    danger_vignette,
                )
                    .run_if(in_state(AppState::Dungeon))
                    .run_if(resource_exists::<ActiveRun>),
            );
    }
}

/// The friendly actor under the cursor (party frame first, then the
/// target-of-target frame, then the 3D world). Friendly spells cast on it
/// without changing the selected target.
#[derive(Resource, Default)]
pub struct Mouseover(pub Option<ActorId>);

/// Clickable HUD frames.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum HudClick {
    Party(usize),
    TargetOfTarget,
}

#[derive(Component, Clone, Copy)]
enum HudText {
    PartyName(usize),
    PartyStatus(usize),
    TargetName,
    TargetInfo,
    ThreatStatus,
    ThreatDetail,
    TotName,
    BossName,
    Resource,
    PlayerBuffs,
    PlayerDebuffs,
    CastLabel,
    Log,
    Info,
    Banner,
    Alert,
    Error,
}

#[derive(Component, Clone, Copy)]
enum HudBar {
    PartyHp(usize),
    PartyMana(usize),
    TargetHp,
    ThreatPull,
    TotHp,
    BossHp,
    PlayerResource,
    Cast,
}

/// One action-bar slot (root node).
#[derive(Component, Clone, Copy)]
struct ActionSlot(usize);

#[derive(Component, Clone, Copy)]
enum ActionPart {
    Name(usize),
    Status(usize),
    /// Dark curtain covering the remaining fraction of the ability's own cooldown.
    Cooldown(usize),
    /// Thin sweep along the top for the shared global cooldown.
    Gcd(usize),
}

/// Full-screen red border shown while the player stands in a telegraph.
#[derive(Component)]
struct DangerVignette;

/// Marks containers that hide when they have nothing to show.
#[derive(Component, Clone, Copy)]
enum HudPanel {
    Target,
    Threat,
    Tot,
    Boss,
    Cast,
}

const GOLD: Color = Color::srgb(1.0, 0.82, 0.3);
const HOVER_BLUE: Color = Color::srgb(0.45, 0.8, 1.0);
const DIM: Color = Color::srgb(0.55, 0.55, 0.55);
const RED_TEXT: Color = Color::srgb(1.0, 0.35, 0.3);

fn bar(width: f32, height: f32, fill: Color, kind: HudBar) -> impl Bundle {
    (
        Node {
            width: px(width),
            height: px(height),
            ..default()
        },
        BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.9)),
        children![(
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            BackgroundColor(fill),
            kind,
        )],
    )
}

pub fn spawn_hud(mut commands: Commands, run: Res<ActiveRun>) {
    let scoped = DespawnOnExit(AppState::Dungeon);
    let panel_bg = BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            border: UiRect::all(px(22)),
            justify_content: JustifyContent::Center,
            padding: UiRect::top(percent(18)),
            display: Display::None,
            ..default()
        },
        BorderColor::all(Color::srgba(1.0, 0.1, 0.0, 0.5)),
        DangerVignette,
        Pickable::IGNORE,
        scoped.clone(),
        children![(
            Text::new("MOVE OUT!"),
            text_font(34.0),
            TextColor(Color::srgb(1.0, 0.25, 0.1)),
            TextShadow::default(),
            Pickable::IGNORE,
        )],
    ));

    // Party frames (top-left); raid groups use compact two-column frames. The
    // whole frame is the click target.
    let raid = run.sim.party.len() > 5;
    let (frame_w, name_size, hp_h, status_size) = if raid {
        (150.0, 10.0, 8.0, 9.0)
    } else {
        (270.0, 13.0, 12.0, 11.0)
    };
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(10),
                top: px(10),
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                max_width: px(if raid {
                    2.0 * (frame_w + 8.0) + 22.0
                } else {
                    400.0
                }),
                column_gap: px(6),
                row_gap: px(if raid { 1.0 } else { 3.0 }),
                padding: UiRect::all(px(4)),
                ..default()
            },
            panel_bg,
            BlocksWorld,
            scoped.clone(),
        ))
        .with_children(|p| {
            for i in 0..run.sim.party.len() {
                p.spawn((
                    UiButton,
                    Hovered::default(),
                    HudClick::Party(i),
                    Tooltip::PartyMember(i),
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(1),
                        width: px(frame_w + 8.0),
                        padding: UiRect::all(px(2)),
                        border: UiRect::all(px(2)),
                        ..default()
                    },
                    BorderColor::all(Color::NONE),
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|f| {
                    f.spawn((Text::new(""), text_font(name_size), HudText::PartyName(i)));
                    f.spawn(bar(
                        frame_w,
                        hp_h,
                        Color::srgb(0.2, 0.75, 0.3),
                        HudBar::PartyHp(i),
                    ));
                    f.spawn(bar(
                        frame_w,
                        if raid { 3.0 } else { 4.0 },
                        Color::srgb(0.25, 0.45, 1.0),
                        HudBar::PartyMana(i),
                    ));
                    f.spawn((
                        Text::new(""),
                        text_font(status_size),
                        HudText::PartyStatus(i),
                    ));
                });
            }
        });

    // Target frame (top-centre) with threat lines.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: percent(50),
            margin: UiRect::left(px(-160)),
            top: px(10),
            width: px(320),
            flex_direction: FlexDirection::Column,
            row_gap: px(3),
            padding: UiRect::all(px(6)),
            ..default()
        },
        panel_bg,
        HudPanel::Target,
        BlocksWorld,
        Tooltip::Target,
        scoped.clone(),
        children![
            (Text::new(""), text_font(15.0), HudText::TargetName),
            bar(308.0, 14.0, Color::srgb(0.85, 0.2, 0.15), HudBar::TargetHp),
            (Text::new(""), text_font(12.0), HudText::TargetInfo),
            (
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(2),
                    ..default()
                },
                HudPanel::Threat,
                children![
                    (Text::new(""), text_font(13.0), HudText::ThreatStatus),
                    bar(308.0, 5.0, Color::srgb(0.9, 0.8, 0.2), HudBar::ThreatPull),
                    (
                        Text::new(""),
                        text_font(11.0),
                        TextColor(Color::srgb(0.8, 0.8, 0.8)),
                        HudText::ThreatDetail
                    ),
                ],
            ),
        ],
    ));

    // Target-of-target frame (right of the target frame); click to target it.
    commands.spawn((
        UiButton,
        Hovered::default(),
        HudClick::TargetOfTarget,
        Tooltip::text(
            "Target of target",
            "Who your target is targeting. Click to select them; T does the same from the keyboard. Friendly spells cast on them while you hover here.",
        ),
        Node {
            position_type: PositionType::Absolute,
            left: percent(50),
            margin: UiRect::left(px(168)),
            top: px(10),
            width: px(170),
            flex_direction: FlexDirection::Column,
            row_gap: px(2),
            padding: UiRect::all(px(4)),
            border: UiRect::all(px(2)),
            ..default()
        },
        BorderColor::all(Color::NONE),
        panel_bg,
        HudPanel::Tot,
        BlocksWorld,
        scoped.clone(),
        children![
            (Text::new(""), text_font(12.0), HudText::TotName),
            bar(158.0, 8.0, Color::srgb(0.2, 0.75, 0.3), HudBar::TotHp),
        ],
    ));

    // Boss frame (below the target frame).
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: percent(50),
            margin: UiRect::left(px(-210)),
            top: px(150),
            width: px(420),
            flex_direction: FlexDirection::Column,
            row_gap: px(3),
            padding: UiRect::all(px(6)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.15, 0.0, 0.1, 0.6)),
        HudPanel::Boss,
        BlocksWorld,
        scoped.clone(),
        children![
            (
                Text::new(""),
                text_font(15.0),
                TextColor(Color::srgb(1.0, 0.8, 0.5)),
                HudText::BossName
            ),
            bar(408.0, 16.0, Color::srgb(0.6, 0.15, 0.7), HudBar::BossHp),
        ],
    ));

    // Banner and error text (centre). Never intercepts clicks.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: percent(10),
            width: percent(80),
            top: percent(30),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(8),
            ..default()
        },
        Pickable::IGNORE,
        scoped.clone(),
        children![
            (
                Text::new(""),
                text_font(26.0),
                TextLayout::justify(Justify::Center),
                HudText::Banner,
                Pickable::IGNORE,
            ),
            (
                Text::new(""),
                text_font(17.0),
                TextShadow::default(),
                HudText::Alert,
                Pickable::IGNORE,
            ),
            (
                Text::new(""),
                text_font(18.0),
                TextColor(RED_TEXT),
                HudText::Error,
                Pickable::IGNORE,
            ),
        ],
    ));

    // Cast bar, resource readout and action bar (bottom-centre).
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: percent(50),
                margin: UiRect::left(px(-300)),
                bottom: px(14),
                width: px(600),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(5),
                ..default()
            },
            Pickable::IGNORE,
            scoped.clone(),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    ..default()
                },
                HudPanel::Cast,
                Pickable::IGNORE,
                children![
                    (Text::new(""), text_font(13.0), HudText::CastLabel),
                    bar(260.0, 10.0, Color::srgb(1.0, 0.8, 0.2), HudBar::Cast),
                ],
            ));
            // Your own buffs (green) and debuffs (red, orange if dispellable);
            // hover for details.
            let me = run
                .sim
                .player
                .and_then(|pl| run.sim.party.iter().position(|&m| m == pl))
                .unwrap_or(0);
            p.spawn((
                Node {
                    column_gap: px(14),
                    ..default()
                },
                Tooltip::PartyMember(me),
                children![
                    (
                        Text::new(""),
                        text_font(12.0),
                        TextColor(Color::srgb(0.55, 0.95, 0.55)),
                        HudText::PlayerBuffs
                    ),
                    (Text::new(""), text_font(12.0), HudText::PlayerDebuffs),
                ],
            ));
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(2),
                    ..default()
                },
                Pickable::IGNORE,
                children![
                    (
                        Text::new(""),
                        text_font(12.0),
                        TextColor(Color::srgb(0.85, 0.85, 0.9)),
                        HudText::Resource
                    ),
                    bar(
                        300.0,
                        6.0,
                        Color::srgb(0.25, 0.45, 1.0),
                        HudBar::PlayerResource
                    ),
                ],
            ));
            p.spawn((
                Node {
                    column_gap: px(8),
                    ..default()
                },
                BlocksWorld,
            ))
            .with_children(|row| {
                for slot in 0..5 {
                    row.spawn((
                        Node {
                            width: px(112),
                            height: px(64),
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::SpaceBetween,
                            padding: UiRect::all(px(4)),
                            border: UiRect::all(px(2)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.85)),
                        BorderColor::all(Color::srgb(0.5, 0.45, 0.3)),
                        ActionSlot(slot),
                        Tooltip::Ability(slot),
                    ))
                    .with_children(|s| {
                        s.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                left: px(0),
                                bottom: px(0),
                                width: percent(100),
                                height: percent(0),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.62)),
                            ActionPart::Cooldown(slot),
                        ));
                        s.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                left: px(0),
                                top: px(0),
                                height: px(3),
                                width: percent(0),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.75, 0.85, 1.0)),
                            ActionPart::Gcd(slot),
                        ));
                        s.spawn((Text::new(""), text_font(13.0), ActionPart::Name(slot)));
                        s.spawn((Text::new(""), text_font(11.0), ActionPart::Status(slot)));
                    });
                }
            });
        });

    // Combat log (bottom-left).
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(10),
            bottom: px(10),
            width: px(320),
            padding: UiRect::all(px(6)),
            ..default()
        },
        panel_bg,
        BlocksWorld,
        scoped.clone(),
        children![(
            Text::new(""),
            text_font(10.0),
            TextColor(Color::srgb(0.85, 0.85, 0.8)),
            HudText::Log
        )],
    ));

    // Run info (top-right).
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: px(10),
            top: px(10),
            width: px(270),
            padding: UiRect::all(px(6)),
            ..default()
        },
        panel_bg,
        BlocksWorld,
        scoped,
        children![(Text::new(""), text_font(11.0), HudText::Info)],
    ));
}

fn class_tag(a: &Actor) -> String {
    use dungeon_sim::Role;
    match a.class {
        Some(c) => {
            let role = match c.role() {
                Role::Tank => "Tank",
                Role::Healer => "Healer",
                Role::MeleeDps | Role::RangedDps => "DPS",
            };
            format!("{} {role}", c.label())
        }
        None => String::new(),
    }
}

fn target_of(sim: &Sim) -> Option<ActorId> {
    sim.player.and_then(|p| sim.actors[p].target)
}

/// The target's target, if the target is alive and has one.
fn target_of_target(sim: &Sim) -> Option<ActorId> {
    target_of(sim)
        .filter(|&t| sim.actors[t].alive)
        .and_then(|t| sim.actors[t].target)
}

fn is_friendly(sim: &Sim, id: ActorId) -> bool {
    sim.player
        .is_some_and(|p| sim.actors[id].alive && !sim.is_hostile(p, id))
}

/// Resolve which friendly the cursor is over: HUD frames first, then (only
/// when the cursor is not over the HUD) the nearest friendly character in 3D.
fn update_mouseover(
    run: Res<ActiveRun>,
    pc: Res<PlayerControl>,
    frames: Query<(&Hovered, &HudClick)>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut mouseover: ResMut<Mouseover>,
) {
    let sim = &run.sim;
    let mut found = None;
    for (hovered, click) in &frames {
        if !hovered.get() {
            continue;
        }
        let id = match *click {
            HudClick::Party(i) => sim.party.get(i).copied(),
            HudClick::TargetOfTarget => target_of_target(sim),
        };
        if let Some(id) = id.filter(|&id| is_friendly(sim, id)) {
            found = Some(id);
        }
    }
    if found.is_none()
        && !pc.cursor_over_ui
        && let Some(cursor) = window.cursor_position()
        && let Ok(ray) = camera.0.viewport_to_world(camera.1, cursor)
    {
        found = pick_actor(sim, ray, |a| is_friendly(sim, a.id));
    }
    if mouseover.0 != found {
        mouseover.0 = found;
    }
}

/// Clicking a party frame or the target-of-target frame selects that character.
fn on_hud_click(ev: On<Activate>, clicks: Query<&HudClick>, run: Option<ResMut<ActiveRun>>) {
    let (Ok(click), Some(mut run)) = (clicks.get(ev.entity), run) else {
        return;
    };
    let id = match *click {
        HudClick::Party(i) => run.sim.party.get(i).copied(),
        HudClick::TargetOfTarget => target_of_target(&run.sim),
    };
    if id.is_some() {
        run.sim.set_player_target(id);
    }
}

/// Frame borders: gold = selected target, blue = mouseover; dead and
/// incapacitated members get a tinted background.
fn style_frames(
    run: Res<ActiveRun>,
    mouseover: Res<Mouseover>,
    mut q: Query<(&HudClick, &Hovered, &mut BorderColor, &mut BackgroundColor)>,
) {
    let sim = &run.sim;
    let selected = target_of(sim);
    for (click, hovered, mut border, mut bg) in &mut q {
        let id = match *click {
            HudClick::Party(i) => sim.party.get(i).copied(),
            HudClick::TargetOfTarget => target_of_target(sim),
        };
        let Some(id) = id else {
            continue;
        };
        let a = &sim.actors[id];
        let b = if selected == Some(id) && matches!(click, HudClick::Party(_)) {
            GOLD
        } else if hovered.get() || mouseover.0 == Some(id) {
            HOVER_BLUE
        } else {
            Color::NONE
        };
        border.set_all(b);
        if matches!(click, HudClick::Party(_)) {
            bg.0 = if !a.alive {
                Color::srgba(0.45, 0.05, 0.05, 0.55)
            } else if a.is_incapacitated() {
                Color::srgba(0.4, 0.2, 0.55, 0.5)
            } else if hovered.get() {
                Color::srgba(1.0, 1.0, 1.0, 0.08)
            } else {
                Color::NONE
            };
        }
    }
}

/// Compact status line for a party member: dead / crowd control, debuffs
/// (dispellable ones marked), then helpful effects.
fn party_status(a: &Actor, compact: bool) -> (String, Color) {
    if !a.alive {
        return ("DEAD".into(), RED_TEXT);
    }
    let mut parts: Vec<String> = Vec::new();
    if !compact {
        parts.push(format!("{:.0}/{:.0}", a.hp.max(0.0), a.max_hp));
    }
    let cc = a.is_incapacitated() || a.is_silenced();
    let color = if cc {
        Color::srgb(0.85, 0.6, 1.0)
    } else if a.has_dispellable_debuff() {
        Color::srgb(1.0, 0.6, 0.2)
    } else if a.auras.iter().any(|x| x.harmful) {
        Color::srgb(1.0, 0.5, 0.45)
    } else if !a.auras.is_empty() {
        Color::srgb(0.6, 0.95, 0.6)
    } else {
        Color::srgb(0.8, 0.8, 0.8)
    };
    if a.is_incapacitated() {
        parts.push("STUNNED".into());
    } else if a.is_silenced() {
        parts.push("SILENCED".into());
    }
    for x in a
        .auras
        .iter()
        .filter(|x| x.harmful)
        .take(if compact { 1 } else { 3 })
    {
        let st = if x.stacks > 1 {
            format!("x{} ", x.stacks)
        } else {
            String::new()
        };
        let dispel = if x.dispellable { "*" } else { "" };
        parts.push(format!("{}{dispel} {st}{:.0}s", x.name, x.remaining));
    }
    if !compact {
        for x in a.auras.iter().filter(|x| !x.harmful).take(2) {
            parts.push(format!("{} {:.0}s", x.name, x.remaining));
        }
    }
    (parts.join(" | "), color)
}

/// Resource line under the cast bar: current/max, regeneration and the rule
/// that governs it (energy rate, five-second rule, rage gain/decay).
fn resource_line(sim: &Sim, p: ActorId) -> String {
    let a = &sim.actors[p];
    let rate = sim.resource_rate(p);
    let label = a.resource_kind.label();
    let mut chars = label.chars();
    let label: String = chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default();
    let head = format!("{label} {:.0}/{:.0}", a.resource, a.max_resource);
    match a.resource_kind {
        ResourceKind::Energy => {
            let to_full = (a.max_resource - a.resource).max(0.0);
            if to_full > 0.5 && rate > 0.0 {
                format!("{head}  +{rate:.0}/s, full in {:.1}s", to_full / rate)
            } else {
                format!("{head}  +{rate:.0}/s (full)")
            }
        }
        ResourceKind::Mana => {
            let fsr = sim.five_second_rule_left(p);
            if fsr > 0.0 {
                format!("{head}  +{rate:.1}/s - five-second rule: full regen in {fsr:.1}s")
            } else if sim.in_combat(p) {
                format!("{head}  +{rate:.1}/s (full regen - not casting)")
            } else {
                format!("{head}  +{rate:.1}/s (out of combat)")
            }
        }
        ResourceKind::Rage => {
            if rate < 0.0 {
                format!("{head}  decaying {:.0}/s out of combat", -rate)
            } else {
                format!("{head}  gained by dealing and taking damage")
            }
        }
    }
}

fn threat_lines(sim: &Sim, enemy: ActorId, viewer: ActorId) -> (String, String, Color) {
    let v = sim.threat_view(enemy, viewer);
    let name = |id: ActorId| sim.actors[id].name.clone();
    let on = v
        .attacking
        .map(|h| {
            let t = if v.taunted { " (taunted)" } else { "" };
            format!("on {}{t}", name(h))
        })
        .unwrap_or_default();
    let top = v
        .top_other
        .map(|(id, val)| format!(" | top other: {} {val:.0}", name(id)))
        .unwrap_or_default();
    let (status, color) = if sim.is_tank(viewer) {
        match v.tank {
            TankThreat::Idle => (String::new(), DIM),
            TankThreat::Secure => ("Threat: SECURE on you".into(), Color::srgb(0.4, 0.95, 0.4)),
            TankThreat::Contested => (
                format!(
                    "Threat: CONTESTED - {:.0}% of the way to a pull",
                    v.contest * 100.0
                ),
                Color::srgb(1.0, 0.85, 0.2),
            ),
            TankThreat::OtherTank => (
                format!(
                    "Threat: held by tank {}",
                    v.attacking.map(name).unwrap_or_default()
                ),
                Color::srgb(0.5, 0.8, 1.0),
            ),
            TankThreat::Lost => (
                format!(
                    "AGGRO LOST - on {}! Taunt",
                    v.attacking.map(name).unwrap_or_default()
                ),
                RED_TEXT,
            ),
        }
    } else {
        match v.personal {
            PersonalThreat::Idle => (String::new(), DIM),
            PersonalThreat::Attacking => ("ATTACKING YOU".into(), RED_TEXT),
            PersonalThreat::Warning => (
                format!("Threat {:.0}% of pull - ease off", v.pull_fraction * 100.0),
                Color::srgb(1.0, 0.85, 0.2),
            ),
            PersonalThreat::Safe => (
                format!("Threat {:.0}% of pull", v.pull_fraction * 100.0),
                Color::srgb(0.4, 0.95, 0.4),
            ),
        }
    };
    let pull = if v.pull_at > 0.0 {
        format!(" | pull at {:.0}", v.pull_at)
    } else {
        String::new()
    };
    let detail = format!("{on} | you {:.0}{pull}{top}", v.mine);
    (status, detail, color)
}

fn update_texts(run: Res<ActiveRun>, mut q: Query<(&mut Text, &mut TextColor, &HudText)>) {
    let sim = &run.sim;
    let player = sim.player;
    let target = target_of(sim);
    let compact = sim.party.len() > 5;
    for (mut text, mut color, kind) in &mut q {
        let s = match *kind {
            HudText::PartyName(i) => {
                let id = sim.party[i];
                let a = &sim.actors[id];
                let you = if Some(id) == player { " (you)" } else { "" };
                color.0 = if a.alive {
                    Color::WHITE
                } else {
                    Color::srgb(0.6, 0.3, 0.3)
                };
                if compact {
                    format!(
                        "{}{you} {}",
                        a.name,
                        a.class.map(|c| c.label()).unwrap_or("")
                    )
                } else {
                    format!("F{} {}{you} - {}", i + 1, a.name, class_tag(a))
                }
            }
            HudText::PartyStatus(i) => {
                let (s, c) = party_status(&sim.actors[sim.party[i]], compact);
                color.0 = c;
                s
            }
            HudText::TargetName => match target {
                Some(t) => {
                    let a = &sim.actors[t];
                    color.0 = if a.is_enemy() {
                        Color::srgb(1.0, 0.5, 0.4)
                    } else {
                        Color::srgb(0.5, 0.9, 1.0)
                    };
                    let state = match a.mode() {
                        Some(EnemyMode::Idle) => " (idle)",
                        Some(EnemyMode::Resetting) => " (evading)",
                        _ if !a.alive => " (dead)",
                        _ => "",
                    };
                    format!("{}{state}  {:.0}%", a.name, a.hp_frac() * 100.0)
                }
                None => String::new(),
            },
            HudText::TargetInfo => match target {
                Some(t) => {
                    let a = &sim.actors[t];
                    let mut parts = vec![format!("{:.0}/{:.0}", a.hp.max(0.0), a.max_hp)];
                    if let Some(c) = &a.cast {
                        parts.push(format!(
                            "casting {} ({:.1}s){}",
                            c.name,
                            c.remaining,
                            if c.interruptible {
                                " INTERRUPTIBLE"
                            } else {
                                ""
                            }
                        ));
                    }
                    for x in a.auras.iter().filter(|x| &*x.name != "Weakened Soul") {
                        let st = if x.stacks > 1 {
                            format!(" x{}", x.stacks)
                        } else {
                            String::new()
                        };
                        parts.push(format!("{}{st} {:.0}s", x.name, x.remaining));
                    }
                    if let Some(p) = player
                        && sim.actors[p].alive
                        && p != t
                    {
                        parts.push(format!(
                            "range {:.1}m",
                            (sim.actors[p].pos.dist(a.pos) - a.radius - 0.5).max(0.0)
                        ));
                    }
                    parts.join(" | ")
                }
                None => String::new(),
            },
            HudText::ThreatStatus | HudText::ThreatDetail => match (target, player) {
                (Some(t), Some(p)) if sim.actors[t].is_engaged_enemy() => {
                    let (status, detail, c) = threat_lines(sim, t, p);
                    if matches!(kind, HudText::ThreatStatus) {
                        color.0 = c;
                        status
                    } else {
                        detail
                    }
                }
                _ => String::new(),
            },
            HudText::TotName => match target_of_target(sim) {
                Some(tt) => {
                    let a = &sim.actors[tt];
                    color.0 = if Some(tt) == player {
                        RED_TEXT
                    } else if a.is_enemy() {
                        Color::srgb(1.0, 0.5, 0.4)
                    } else {
                        Color::srgb(0.5, 0.9, 1.0)
                    };
                    let you = if Some(tt) == player { " (you)" } else { "" };
                    format!("> {}{you} {:.0}%", a.name, a.hp_frac() * 100.0)
                }
                None => String::new(),
            },
            HudText::BossName => {
                let b = &sim.actors[sim.boss];
                let mut s = format!(
                    "{} - Phase {}  {:.0}%",
                    b.name,
                    sim.boss_phase(),
                    b.hp_frac() * 100.0
                );
                if let Some(c) = &b.cast {
                    s.push_str(&format!("  | casting {} ", c.name));
                }
                if let Some(t) = b.target {
                    s.push_str(&format!("  | on {}", sim.actors[t].name));
                }
                s
            }
            HudText::PlayerBuffs | HudText::PlayerDebuffs => match player {
                Some(p) => {
                    let harmful = matches!(kind, HudText::PlayerDebuffs);
                    let a = &sim.actors[p];
                    if harmful {
                        color.0 = if a.has_dispellable_debuff() {
                            Color::srgb(1.0, 0.6, 0.2)
                        } else {
                            RED_TEXT
                        };
                    }
                    let mut s = String::new();
                    for x in a.auras.iter().filter(|x| x.harmful == harmful) {
                        if !s.is_empty() {
                            s.push_str("  ");
                        }
                        s.push_str(&x.name);
                        if x.stacks > 1 {
                            s.push_str(&format!(" x{}", x.stacks));
                        }
                        s.push_str(&format!(" {:.0}s", x.remaining.max(0.0)));
                    }
                    s
                }
                None => String::new(),
            },
            HudText::Resource => match player {
                Some(p) if sim.actors[p].alive => resource_line(sim, p),
                _ => String::new(),
            },
            HudText::CastLabel => match player.and_then(|p| sim.actors[p].cast.as_ref()) {
                Some(c) => format!("{} {:.1}s", c.name, c.remaining.max(0.0)),
                None => String::new(),
            },
            HudText::Log => {
                let mut s = String::new();
                for (i, line) in run.log.iter().enumerate() {
                    if i > 0 {
                        s.push('\n');
                    }
                    s.push_str(line);
                }
                s
            }
            HudText::Info => {
                let mut s = format!(
                    "{}\ntime {:.0}s | seed {}\n",
                    sim.content.dungeon.name, sim.time, sim.seed
                );
                for e in &sim.encounters {
                    let mark = if e.cleared {
                        "[x]"
                    } else if e.mobs.iter().any(|&m| sim.actors[m].is_engaged_enemy()) {
                        "[!]"
                    } else {
                        "[ ]"
                    };
                    s.push_str(&format!("{mark} {}\n", e.id));
                }
                s.push_str(
                    "\nW/S move, A/D turn, Q/E strafe\n\
                     RMB steer, LMB look, both = run\n\
                     V camera follow, wheel zoom\n\
                     Click/Tab target, T assist, G kill target\n\
                     1-5 abilities - heals use mouseover\n\
                     F attack, F1-F5 party, Esc clear\n\
                     L verbose log, Backspace leave",
                );
                s
            }
            HudText::Banner => match &run.banner {
                Some((t, _, c)) => {
                    color.0 = *c;
                    t.clone()
                }
                None => String::new(),
            },
            HudText::Alert => match &run.alert {
                Some((t, _, c)) => {
                    color.0 = *c;
                    t.clone()
                }
                None => String::new(),
            },
            HudText::Error => run
                .error
                .as_ref()
                .map(|(e, _)| e.clone())
                .unwrap_or_default(),
        };
        if text.0 != s {
            text.0 = s;
        }
    }
}

fn update_bars(
    run: Res<ActiveRun>,
    mut bars: Query<(&mut Node, &mut BackgroundColor, &HudBar), Without<HudPanel>>,
    mut panels: Query<(&mut Node, &HudPanel), Without<HudBar>>,
) {
    let sim = &run.sim;
    let player = sim.player.map(|p| &sim.actors[p]);
    let target = target_of(sim);
    let threat = match (target, sim.player) {
        (Some(t), Some(p)) if sim.actors[t].is_engaged_enemy() => Some(sim.threat_view(t, p)),
        _ => None,
    };
    for (mut node, mut bg, kind) in &mut bars {
        let frac = match *kind {
            HudBar::PartyHp(i) => sim.actors[sim.party[i]].hp_frac(),
            HudBar::PartyMana(i) => {
                let a = &sim.actors[sim.party[i]];
                bg.0 = crate::common::resource_color(a.resource_kind);
                a.resource_frac()
            }
            HudBar::TargetHp => target.map(|t| sim.actors[t].hp_frac()).unwrap_or(0.0),
            HudBar::ThreatPull => match threat {
                Some(v) => {
                    // Holder: how close the nearest competitor is. Others: how
                    // close I am to pulling.
                    let f = if v.attacking == sim.player {
                        v.contest
                    } else {
                        v.pull_fraction
                    };
                    bg.0 = if f >= 1.0 {
                        Color::srgb(0.95, 0.25, 0.2)
                    } else if f >= dungeon_sim::sim::THREAT_WARNING {
                        Color::srgb(1.0, 0.8, 0.2)
                    } else {
                        Color::srgb(0.35, 0.85, 0.35)
                    };
                    f
                }
                None => 0.0,
            },
            HudBar::TotHp => target_of_target(sim)
                .map(|t| sim.actors[t].hp_frac())
                .unwrap_or(0.0),
            HudBar::BossHp => sim.actors[sim.boss].hp_frac(),
            HudBar::PlayerResource => match player {
                Some(a) => {
                    bg.0 = crate::common::resource_color(a.resource_kind);
                    a.resource_frac()
                }
                None => 0.0,
            },
            HudBar::Cast => player
                .and_then(|a| a.cast.as_ref())
                .map(|c| 1.0 - c.remaining / c.total)
                .unwrap_or(0.0),
        };
        let w = percent(100.0 * frac.clamp(0.0, 1.0));
        if node.width != w {
            node.width = w;
        }
    }
    for (mut node, kind) in &mut panels {
        let show = match kind {
            HudPanel::Target => target.is_some(),
            HudPanel::Threat => threat.is_some(),
            HudPanel::Tot => target_of_target(sim).is_some(),
            HudPanel::Boss => {
                let b = &sim.actors[sim.boss];
                b.alive && b.is_engaged_enemy()
            }
            HudPanel::Cast => player
                .and_then(|a| a.cast.as_ref())
                .is_some_and(|c| matches!(c.kind, CastKind::Ability(_))),
        };
        let d = if show { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
    }
}

/// Short status for one action slot plus its colour. The order matches what a
/// player most needs to know: cooldown, resource, range, then other blockers.
/// Not enough resource right now. Checked separately from `result`, because
/// validation reports the GCD first and the bar should still show the shortfall.
fn short_of_resource(st: &AbilityState) -> bool {
    st.resource_in != Some(0.0)
}

fn slot_status(st: &AbilityState, resource: &str) -> (String, Color) {
    if st.cooldown_left > 0.0 {
        return (format!("cooldown {:.1}s", st.cooldown_left), DIM);
    }
    if short_of_resource(st) {
        let wait = match st.resource_in {
            Some(t) => format!(" - {t:.1}s"),
            None => String::new(),
        };
        return (
            format!("need {:.0} {resource}{wait}", st.cost),
            Color::srgb(0.5, 0.65, 1.0),
        );
    }
    if st.in_range == Some(false) {
        return ("Out of range".into(), RED_TEXT);
    }
    let blocker = match st.result {
        Err(CastError::NoTarget) => Some("no target"),
        Err(CastError::InvalidTarget) => Some("invalid target"),
        Err(CastError::Silenced) => Some("silenced"),
        Err(CastError::Incapacitated) => Some("stunned"),
        Err(CastError::Moving) => Some("stop to cast"),
        Err(CastError::NothingToDispel) => Some("nothing to dispel"),
        Err(CastError::TargetWeakened) => Some("target weakened"),
        Err(CastError::Immune) => Some("immune"),
        _ => None,
    };
    if let Some(b) = blocker {
        return (b.into(), Color::srgb(1.0, 0.75, 0.4));
    }
    if st.cost > 0.0 {
        (format!("{:.0} {resource}", st.cost), Color::WHITE)
    } else {
        ("ready".into(), Color::WHITE)
    }
}

#[allow(clippy::type_complexity)]
fn update_action_bar(
    run: Res<ActiveRun>,
    mouseover: Res<Mouseover>,
    mut slots: Query<(&ActionSlot, &mut BackgroundColor, &mut BorderColor)>,
    mut parts: Query<
        (
            &ActionPart,
            &mut Node,
            Option<&mut Text>,
            Option<&mut TextColor>,
        ),
        Without<ActionSlot>,
    >,
) {
    let sim = &run.sim;
    let Some(p) = sim.player else {
        return;
    };
    let caster = &sim.actors[p];
    let n = caster.abilities.len();
    let mut states: [Option<AbilityState>; 5] = [None; 5];
    for (slot, st) in states.iter_mut().enumerate().take(n.min(5)) {
        let target = sim.player_cast_target(slot, mouseover.0);
        *st = Some(sim.ability_state(p, slot, target));
    }
    for (ActionSlot(slot), mut bg, mut border) in &mut slots {
        let (b, c) = match &states[*slot] {
            Some(st) if st.cooldown_left <= 0.0 && st.in_range == Some(false) => (
                Color::srgba(0.42, 0.06, 0.06, 0.9),
                Color::srgb(0.9, 0.25, 0.2),
            ),
            Some(st) if short_of_resource(st) => (
                Color::srgba(0.07, 0.09, 0.25, 0.9),
                Color::srgb(0.35, 0.45, 0.85),
            ),
            Some(st) if st.result.is_ok() || st.result == Err(CastError::OnGlobalCooldown) => (
                Color::srgba(0.1, 0.1, 0.15, 0.85),
                Color::srgb(0.75, 0.65, 0.35),
            ),
            Some(_) => (
                Color::srgba(0.1, 0.1, 0.15, 0.85),
                Color::srgb(0.35, 0.33, 0.28),
            ),
            None => (
                Color::srgba(0.05, 0.05, 0.05, 0.5),
                Color::srgb(0.2, 0.2, 0.2),
            ),
        };
        if bg.0 != b {
            bg.0 = b;
        }
        border.set_all(c);
    }
    let resource = caster.resource_kind.label();
    for (part, mut node, text, color) in &mut parts {
        match *part {
            ActionPart::Name(slot) => {
                let (Some(mut text), Some(mut color)) = (text, color) else {
                    continue;
                };
                let s = if slot < n {
                    format!("[{}] {}", slot + 1, caster.abilities[slot].def().name)
                } else {
                    String::new()
                };
                let usable = states[slot].is_some_and(|st| {
                    !short_of_resource(&st) && st.result.is_ok()
                        || matches!(
                            st.result,
                            Err(CastError::OnGlobalCooldown) | Err(CastError::Busy)
                        )
                });
                color.0 = if usable { Color::WHITE } else { DIM };
                if text.0 != s {
                    text.0 = s;
                }
            }
            ActionPart::Status(slot) => {
                let (Some(mut text), Some(mut color)) = (text, color) else {
                    continue;
                };
                let (s, c) = match &states[slot] {
                    Some(st) => slot_status(st, resource),
                    None => (String::new(), DIM),
                };
                color.0 = c;
                if text.0 != s {
                    text.0 = s;
                }
            }
            ActionPart::Cooldown(slot) => {
                let f = states[slot]
                    .filter(|st| st.cooldown_total > 0.0)
                    .map(|st| st.cooldown_left / st.cooldown_total)
                    .unwrap_or(0.0);
                let h = percent(100.0 * f.clamp(0.0, 1.0));
                if node.height != h {
                    node.height = h;
                }
            }
            ActionPart::Gcd(slot) => {
                // The GCD sweep only shows when it is what's holding the slot back.
                let f = states[slot]
                    .filter(|st| st.gcd_left > st.cooldown_left && st.gcd_total > 0.0)
                    .map(|st| st.gcd_left / st.gcd_total)
                    .unwrap_or(0.0);
                let w = percent(100.0 * f.clamp(0.0, 1.0));
                if node.width != w {
                    node.width = w;
                }
            }
        }
    }
}

fn danger_vignette(
    time: Res<Time>,
    run: Res<ActiveRun>,
    mut q: Single<(&mut Node, &mut BorderColor), With<DangerVignette>>,
) {
    let sim = &run.sim;
    let in_danger = sim.player.is_some_and(|p| {
        let a = &sim.actors[p];
        a.alive
            && sim
                .telegraphs
                .iter()
                .any(|t| a.pos.dist(t.center) <= t.radius + a.radius)
    });
    let (node, border) = &mut *q;
    node.display = if in_danger {
        Display::Flex
    } else {
        Display::None
    };
    if in_danger {
        let pulse = 0.35 + 0.3 * (time.elapsed_secs() * 10.0).sin().abs();
        border.set_all(Color::srgba(1.0, 0.1, 0.0, pulse));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(result: Result<(), CastError>) -> AbilityState {
        AbilityState {
            cooldown_left: 0.0,
            cooldown_total: 0.0,
            gcd_left: 0.0,
            gcd_total: 1.5,
            cost: 40.0,
            resource_in: Some(0.0),
            in_range: Some(true),
            result,
        }
    }

    #[test]
    fn slot_status_reports_the_most_useful_blocker_first() {
        let ready = state(Ok(()));
        assert_eq!(slot_status(&ready, "energy").0, "40 energy");
        // Cooldown beats everything else.
        let cd = AbilityState {
            cooldown_left: 3.2,
            cooldown_total: 8.0,
            in_range: Some(false),
            ..state(Err(CastError::NotReady))
        };
        assert_eq!(slot_status(&cd, "energy").0, "cooldown 3.2s");
        // Resource shortfall shows the wait until affordable.
        let poor = AbilityState {
            resource_in: Some(1.5),
            ..state(Err(CastError::NotEnoughResource))
        };
        assert_eq!(slot_status(&poor, "energy").0, "need 40 energy - 1.5s");
        let range = AbilityState {
            in_range: Some(false),
            ..state(Err(CastError::OutOfRange))
        };
        assert_eq!(slot_status(&range, "mana").0, "Out of range");
        // The GCD alone is not an error: the slot shows its cost.
        let gcd = AbilityState {
            gcd_left: 1.0,
            ..state(Err(CastError::OnGlobalCooldown))
        };
        assert_eq!(slot_status(&gcd, "rage").0, "40 rage");
        assert_eq!(
            slot_status(&state(Err(CastError::Moving)), "mana").0,
            "stop to cast"
        );
    }
}
