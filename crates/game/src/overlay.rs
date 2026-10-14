//! Screen-space overlays anchored to world positions: enemy/ally nameplates
//! (name, health bar, cast bar) and floating combat text.

use crate::camera::MainCamera;
use crate::common::{AppState, text_font, to_vec3};
use crate::dungeon::ActiveRun;
use bevy::prelude::*;
use dungeon_sim::{ActorId, EnemyMode, EventKind};

pub struct OverlayPlugin;

impl Plugin for OverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (spawn_floating_text, update_floating_text, update_nameplates)
                .chain()
                .after(crate::dungeon::process_events)
                .run_if(in_state(AppState::Dungeon))
                .run_if(resource_exists::<ActiveRun>),
        );
    }
}

const PLATE_WIDTH: f32 = 110.0;

#[derive(Component)]
pub struct Nameplate {
    id: ActorId,
}

#[derive(Component)]
struct PlateName(ActorId);

#[derive(Component)]
struct PlateHp(ActorId);

#[derive(Component)]
struct PlateCast(ActorId);

#[derive(Component)]
struct FloatingText {
    world: Vec3,
    age: f32,
    life: f32,
    rise: f32,
    drift: f32,
}

/// Spawn one nameplate per actor except the player (called on dungeon enter).
pub fn spawn_nameplates(mut commands: Commands, run: Res<ActiveRun>) {
    for a in &run.sim.actors {
        if Some(a.id) == run.sim.player {
            continue;
        }
        let id = a.id;
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: px(PLATE_WIDTH),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
            Nameplate { id },
            DespawnOnExit(AppState::Dungeon),
            children![
                (Text::new(a.name.clone()), text_font(11.0), PlateName(id)),
                (
                    Node {
                        width: px(PLATE_WIDTH - 20.0),
                        height: px(6),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.8)),
                    children![(
                        Node {
                            width: percent(100),
                            height: percent(100),
                            ..default()
                        },
                        BackgroundColor(if a.is_enemy() {
                            Color::srgb(0.85, 0.2, 0.15)
                        } else {
                            Color::srgb(0.2, 0.8, 0.3)
                        }),
                        PlateHp(id),
                    )],
                ),
                (
                    Text::new(""),
                    text_font(10.0),
                    TextColor(Color::srgb(1.0, 0.8, 0.3)),
                    PlateCast(id)
                ),
            ],
        ));
    }
}

fn project(camera: &Camera, cam_gt: &GlobalTransform, world: Vec3) -> Option<Vec2> {
    // Reject points behind the camera.
    let to = world - cam_gt.translation();
    if to.dot(*cam_gt.forward()) <= 0.1 {
        return None;
    }
    camera.world_to_viewport(cam_gt, world).ok()
}

#[allow(clippy::type_complexity)]
fn update_nameplates(
    run: Res<ActiveRun>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut plates: Query<(&Nameplate, &mut Node), (Without<PlateHp>,)>,
    mut names: Query<(&PlateName, &mut TextColor, &mut Text), Without<PlateCast>>,
    mut hps: Query<(&PlateHp, &mut Node), Without<Nameplate>>,
    mut casts: Query<(&PlateCast, &mut Text), Without<PlateName>>,
) {
    let (cam, cam_gt) = *camera;
    let sim = &run.sim;
    let target = sim.player.and_then(|p| sim.actors[p].target);
    let player_pos = sim.player.map(|p| sim.actors[p].pos);
    for (plate, mut node) in &mut plates {
        let a = &sim.actors[plate.id];
        let height = if a.is_enemy() {
            a.radius * 3.4 + 0.6
        } else {
            2.5
        };
        let world = to_vec3(run.interp_pos(plate.id), height);
        let near = player_pos.is_none_or(|p| p.dist(a.pos) < 55.0);
        let visible = a.alive && near;
        match project(cam, cam_gt, world).filter(|_| visible) {
            Some(screen) => {
                node.display = Display::Flex;
                node.left = px(screen.x - PLATE_WIDTH * 0.5);
                node.top = px(screen.y - 30.0);
            }
            None => node.display = Display::None,
        }
    }
    for (n, mut color, mut text) in &mut names {
        let a = &sim.actors[n.0];
        let selected = target == Some(n.0);
        color.0 = match (a.is_enemy(), a.mode()) {
            (true, Some(EnemyMode::Engaged)) => Color::srgb(1.0, 0.35, 0.3),
            (true, Some(EnemyMode::Resetting)) => Color::srgb(0.7, 0.7, 0.7),
            (true, _) => Color::srgb(1.0, 0.9, 0.4),
            _ => Color::srgb(0.5, 1.0, 0.6),
        };
        if selected {
            color.0 = Color::WHITE;
        }
        let want = if selected {
            format!("> {} <", a.name)
        } else {
            a.name.clone()
        };
        if text.0 != want {
            text.0 = want;
        }
    }
    for (h, mut node) in &mut hps {
        node.width = percent(100.0 * sim.actors[h.0].hp_frac());
    }
    for (c, mut text) in &mut casts {
        let a = &sim.actors[c.0];
        let want = match a.cast {
            Some(cast) => format!("{} {:.1}s", cast.name, cast.remaining.max(0.0)),
            None => String::new(),
        };
        if text.0 != want {
            text.0 = want;
        }
    }
}

fn spawn_floating_text(mut commands: Commands, run: Res<ActiveRun>) {
    let sim = &run.sim;
    let player = sim.player;
    for (i, e) in run.fresh.iter().enumerate() {
        let jitter = ((sim.tick as usize + i) % 7) as f32 * 0.25 - 0.75;
        let (who, text, color, size) = match &e.kind {
            EventKind::Damage {
                src,
                dst,
                amount,
                crit,
                ..
            } => {
                let involves_player = Some(*src) == player || Some(*dst) == player;
                let color = if sim.actors[*dst].is_enemy() {
                    if Some(*src) == player {
                        Color::srgb(1.0, 0.95, 0.4)
                    } else {
                        Color::srgb(0.85, 0.85, 0.85)
                    }
                } else {
                    Color::srgb(1.0, 0.3, 0.25)
                };
                let size = if *crit {
                    22.0
                } else if involves_player {
                    16.0
                } else {
                    12.0
                };
                let s = if *crit {
                    format!("{amount:.0}!")
                } else {
                    format!("{amount:.0}")
                };
                (*dst, s, color, size)
            }
            EventKind::Heal { dst, amount, .. } if *amount >= 1.0 => (
                *dst,
                format!("+{amount:.0}"),
                Color::srgb(0.3, 1.0, 0.4),
                if Some(*dst) == player { 15.0 } else { 12.0 },
            ),
            EventKind::Interrupted { src, .. } => (
                *src,
                "Interrupted".to_string(),
                Color::srgb(0.6, 0.8, 1.0),
                14.0,
            ),
            EventKind::Taunt { dst, .. } => (
                *dst,
                "Taunted".to_string(),
                Color::srgb(1.0, 0.5, 0.2),
                13.0,
            ),
            EventKind::Death { actor } => {
                (*actor, "Dies".to_string(), Color::srgb(0.7, 0.7, 0.7), 14.0)
            }
            EventKind::Revive { actor } => (
                *actor,
                "Rallies!".to_string(),
                Color::srgb(0.6, 1.0, 0.9),
                14.0,
            ),
            _ => continue,
        };
        let a = &sim.actors[who];
        let height = if a.is_enemy() { a.radius * 3.4 } else { 2.2 };
        commands.spawn((
            Text::new(text),
            text_font(size),
            TextColor(color),
            TextShadow::default(),
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            FloatingText {
                world: to_vec3(a.pos, height),
                age: 0.0,
                life: 1.1,
                rise: 1.6,
                drift: jitter,
            },
            DespawnOnExit(AppState::Dungeon),
        ));
    }
}

fn update_floating_text(
    mut commands: Commands,
    time: Res<Time>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut q: Query<(Entity, &mut FloatingText, &mut Node, &mut TextColor)>,
) {
    let (cam, cam_gt) = *camera;
    for (e, mut ft, mut node, mut color) in &mut q {
        ft.age += time.delta_secs();
        if ft.age >= ft.life {
            commands.entity(e).despawn();
            continue;
        }
        let t = ft.age / ft.life;
        let world = ft.world + Vec3::Y * ft.rise * t;
        match project(cam, cam_gt, world) {
            Some(p) => {
                node.display = Display::Flex;
                node.left = px(p.x - 12.0 + ft.drift * 18.0);
                node.top = px(p.y - 10.0);
            }
            None => node.display = Display::None,
        }
        color.0 = color.0.with_alpha(1.0 - t * t);
    }
}
