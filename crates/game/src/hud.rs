//! In-dungeon HUD: party frames, target frame, boss frame, action bar, cast bar,
//! combat log, run info and banners. Pure presentation of `ActiveRun` state.

use crate::common::{AppState, text_font};
use crate::dungeon::ActiveRun;
use bevy::prelude::*;
use dungeon_sim::sim::{Actor, CastKind};
use dungeon_sim::{ActorId, EnemyMode};

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (update_texts, update_bars, danger_vignette)
                .run_if(in_state(AppState::Dungeon))
                .run_if(resource_exists::<ActiveRun>),
        );
    }
}

#[derive(Component, Clone, Copy)]
enum HudText {
    PartyName(usize),
    TargetName,
    TargetInfo,
    BossName,
    Action(usize),
    CastLabel,
    Log,
    Info,
    Banner,
    Error,
}

#[derive(Component, Clone, Copy)]
enum HudBar {
    PartyHp(usize),
    PartyMana(usize),
    TargetHp,
    BossHp,
    Cast,
    Cooldown(usize),
}

/// Full-screen red border shown while the player stands in a telegraph.
#[derive(Component)]
struct DangerVignette;

/// Marks containers that hide when they have nothing to show.
#[derive(Component, Clone, Copy)]
enum HudPanel {
    Target,
    Boss,
    Cast,
}

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
        scoped.clone(),
        children![(
            Text::new("MOVE OUT!"),
            text_font(34.0),
            TextColor(Color::srgb(1.0, 0.25, 0.1)),
            TextShadow::default(),
        )],
    ));

    // Party frames (top-left).
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(10),
                top: px(10),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                padding: UiRect::all(px(6)),
                ..default()
            },
            panel_bg,
            scoped.clone(),
        ))
        .with_children(|p| {
            for i in 0..run.sim.party.len() {
                p.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(2),
                    ..default()
                })
                .with_children(|f| {
                    f.spawn((Text::new(""), text_font(13.0), HudText::PartyName(i)));
                    f.spawn(bar(
                        200.0,
                        12.0,
                        Color::srgb(0.2, 0.75, 0.3),
                        HudBar::PartyHp(i),
                    ));
                    f.spawn(bar(
                        200.0,
                        5.0,
                        Color::srgb(0.25, 0.45, 1.0),
                        HudBar::PartyMana(i),
                    ));
                });
            }
        });

    // Target frame (top-centre).
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
        scoped.clone(),
        children![
            (Text::new(""), text_font(15.0), HudText::TargetName),
            bar(308.0, 14.0, Color::srgb(0.85, 0.2, 0.15), HudBar::TargetHp),
            (Text::new(""), text_font(12.0), HudText::TargetInfo),
        ],
    ));

    // Boss frame (below target frame).
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: percent(50),
            margin: UiRect::left(px(-210)),
            top: px(110),
            width: px(420),
            flex_direction: FlexDirection::Column,
            row_gap: px(3),
            padding: UiRect::all(px(6)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.15, 0.0, 0.1, 0.6)),
        HudPanel::Boss,
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

    // Banner and error text (centre).
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: percent(10),
            width: percent(80),
            top: percent(28),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(8),
            ..default()
        },
        scoped.clone(),
        children![
            (
                Text::new(""),
                text_font(26.0),
                TextLayout::justify(Justify::Center),
                HudText::Banner
            ),
            (
                Text::new(""),
                text_font(18.0),
                TextColor(Color::srgb(1.0, 0.3, 0.3)),
                HudText::Error
            ),
        ],
    ));

    // Cast bar + action bar (bottom-centre).
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
                row_gap: px(6),
                ..default()
            },
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
                children![
                    (Text::new(""), text_font(13.0), HudText::CastLabel),
                    bar(260.0, 10.0, Color::srgb(1.0, 0.8, 0.2), HudBar::Cast),
                ],
            ));
            p.spawn(Node {
                column_gap: px(8),
                ..default()
            })
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
                        children![
                            (Text::new(""), text_font(13.0), HudText::Action(slot)),
                            bar(
                                100.0,
                                5.0,
                                Color::srgb(0.6, 0.6, 0.6),
                                HudBar::Cooldown(slot)
                            ),
                        ],
                    ));
                }
            });
        });

    // Combat log (bottom-left).
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(10),
            bottom: px(10),
            width: px(470),
            padding: UiRect::all(px(6)),
            ..default()
        },
        panel_bg,
        scoped.clone(),
        children![(
            Text::new(""),
            text_font(11.0),
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
            width: px(300),
            padding: UiRect::all(px(6)),
            ..default()
        },
        panel_bg,
        scoped,
        children![(Text::new(""), text_font(12.0), HudText::Info)],
    ));
}

fn class_tag(a: &Actor) -> String {
    match a.class {
        Some(c) => format!("{} {}", c.label(), c.role().label()),
        None => String::new(),
    }
}

fn target_of(run: &ActiveRun) -> Option<ActorId> {
    run.sim.player.and_then(|p| run.sim.actors[p].target)
}

fn update_texts(run: Res<ActiveRun>, mut q: Query<(&mut Text, &mut TextColor, &HudText)>) {
    let sim = &run.sim;
    let player = sim.player;
    for (mut text, mut color, kind) in &mut q {
        let s = match *kind {
            HudText::PartyName(i) => {
                let id = sim.party[i];
                let a = &sim.actors[id];
                let you = if Some(id) == player { " (you)" } else { "" };
                let targeted = if target_of(&run) == Some(id) {
                    " <"
                } else {
                    ""
                };
                color.0 = if a.alive {
                    Color::WHITE
                } else {
                    Color::srgb(0.6, 0.3, 0.3)
                };
                if a.alive {
                    format!(
                        "F{} {}{you} - {}  {:.0}/{:.0}{targeted}",
                        i + 1,
                        a.name,
                        class_tag(a),
                        a.hp,
                        a.max_hp
                    )
                } else {
                    format!("F{} {}{you} - DEAD", i + 1, a.name)
                }
            }
            HudText::TargetName => match target_of(&run) {
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
                        _ => "",
                    };
                    format!(
                        "{}{state}  {:.0}/{:.0} ({:.0}%)",
                        a.name,
                        a.hp,
                        a.max_hp,
                        a.hp_frac() * 100.0
                    )
                }
                None => String::new(),
            },
            HudText::TargetInfo => match target_of(&run) {
                Some(t) => {
                    let a = &sim.actors[t];
                    let mut parts = Vec::new();
                    if let Some(tt) = a.target {
                        parts.push(format!("targeting: {}", sim.actors[tt].name));
                    }
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
                    let auras: Vec<String> = a
                        .auras
                        .iter()
                        .filter(|x| &*x.name != "Weakened Soul")
                        .map(|x| {
                            let st = if x.stacks > 1 {
                                format!(" x{}", x.stacks)
                            } else {
                                String::new()
                            };
                            format!("{}{st} {:.0}s", x.name, x.remaining)
                        })
                        .collect();
                    if !auras.is_empty() {
                        parts.push(auras.join(", "));
                    }
                    if a.is_enemy()
                        && let Some(p) = player
                    {
                        let mine = sim.threat(t, p);
                        let top = a
                            .enemy
                            .as_ref()
                            .map(|e| e.threat.iter().map(|x| x.value).fold(0.0, f32::max))
                            .unwrap_or(0.0);
                        if top > 0.0 {
                            parts.push(format!("your threat: {:.0}%", 100.0 * mine / top));
                        }
                    }
                    if let Some(p) = player
                        && sim.actors[p].alive
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
            HudText::Action(slot) => match player.map(|p| &sim.actors[p]) {
                Some(a) if slot < a.abilities.len() => {
                    let def = a.abilities[slot].def();
                    let cd = a.cooldowns[slot];
                    let ready = a.ability_ready(slot);
                    color.0 = if ready {
                        Color::WHITE
                    } else {
                        Color::srgb(0.55, 0.55, 0.55)
                    };
                    let status = if cd > 0.0 {
                        format!("{cd:.1}s")
                    } else if a.resource < def.cost {
                        format!("{:.0} {} (low)", def.cost, a.resource_kind.label())
                    } else if def.cost > 0.0 {
                        format!("{:.0} {}", def.cost, a.resource_kind.label())
                    } else {
                        "ready".into()
                    };
                    format!("[{}] {}\n{status}", slot + 1, def.name)
                }
                _ => String::new(),
            },
            HudText::CastLabel => match player.and_then(|p| sim.actors[p].cast.as_ref()) {
                Some(c) => format!("{} {:.1}s", c.name, c.remaining.max(0.0)),
                None => String::new(),
            },
            HudText::Log => run.log.iter().cloned().collect::<Vec<_>>().join("\n"),
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
                    "\nWASD move | RMB/arrows camera | Tab/click target\n\
                     1-5 abilities | Q/E strafe | G kill target | F attack | F1-F5 party | Esc clear\n\
                     L verbose log | Backspace leave dungeon",
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
    for (mut node, mut bg, kind) in &mut bars {
        let frac = match *kind {
            HudBar::PartyHp(i) => sim.actors[sim.party[i]].hp_frac(),
            HudBar::PartyMana(i) => {
                let a = &sim.actors[sim.party[i]];
                bg.0 = crate::common::resource_color(a.resource_kind);
                a.resource_frac()
            }
            HudBar::TargetHp => target_of(&run)
                .map(|t| sim.actors[t].hp_frac())
                .unwrap_or(0.0),
            HudBar::BossHp => sim.actors[sim.boss].hp_frac(),
            HudBar::Cast => player
                .and_then(|a| a.cast.as_ref())
                .map(|c| 1.0 - c.remaining / c.total)
                .unwrap_or(0.0),
            HudBar::Cooldown(slot) => player
                .filter(|a| slot < a.abilities.len())
                .map(|a| {
                    let cd = a.abilities[slot].def().cooldown;
                    if cd > 0.0 {
                        1.0 - a.cooldowns[slot] / cd
                    } else {
                        1.0 - a.gcd
                    }
                })
                .unwrap_or(0.0),
        };
        node.width = percent(100.0 * frac.clamp(0.0, 1.0));
    }
    for (mut node, kind) in &mut panels {
        let show = match kind {
            HudPanel::Target => target_of(&run).is_some(),
            HudPanel::Boss => {
                let b = &sim.actors[sim.boss];
                b.alive && b.is_engaged_enemy()
            }
            HudPanel::Cast => player
                .and_then(|a| a.cast.as_ref())
                .is_some_and(|c| matches!(c.kind, CastKind::Ability(_))),
        };
        node.display = if show { Display::Flex } else { Display::None };
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
