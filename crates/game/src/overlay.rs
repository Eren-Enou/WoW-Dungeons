//! Screen-space overlays anchored to world positions: enemy/ally nameplates
//! (name, health bar, cast bar) and floating combat text.

use crate::camera::MainCamera;
use crate::combat_feed::own_effect;
use crate::common::{AppState, text_font, to_vec3};
use crate::dungeon::ActiveRun;
use bevy::picking::Pickable;
use bevy::prelude::*;
use dungeon_sim::class::Effect;
use dungeon_sim::sim::{PersonalThreat, TankThreat};
use dungeon_sim::stats::HitOutcome;
use dungeon_sim::{ActorId, EnemyMode, EventKind, Sim};

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

const PLATE_WIDTH: f32 = 170.0;

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

/// Health-bar frame of an enemy plate, coloured by threat.
#[derive(Component)]
struct PlateThreat(ActorId);

/// Nameplate colour for an enemy, from the player's threat perspective.
/// Tanks: green = secure, yellow = contested, blue = another tank holds it,
/// red = attacking a non-tank. Others: grey-red = safe, yellow = close to
/// pulling, red = attacking you. Idle enemies are yellow-gold, evading grey.
pub fn threat_color(sim: &Sim, enemy: ActorId) -> Color {
    let a = &sim.actors[enemy];
    match a.mode() {
        Some(EnemyMode::Resetting) => return Color::srgb(0.7, 0.7, 0.7),
        Some(EnemyMode::Engaged) => {}
        _ => return Color::srgb(1.0, 0.9, 0.4),
    }
    let Some(p) = sim.player else {
        return Color::srgb(1.0, 0.35, 0.3);
    };
    let v = sim.threat_view(enemy, p);
    if sim.is_tank(p) {
        match v.tank {
            TankThreat::Secure => Color::srgb(0.35, 0.95, 0.35),
            TankThreat::Contested => Color::srgb(1.0, 0.85, 0.15),
            TankThreat::OtherTank => Color::srgb(0.45, 0.75, 1.0),
            TankThreat::Lost => Color::srgb(1.0, 0.2, 0.15),
            TankThreat::Idle => Color::srgb(1.0, 0.9, 0.4),
        }
    } else {
        match v.personal {
            PersonalThreat::Safe | PersonalThreat::Idle => Color::srgb(0.9, 0.55, 0.5),
            PersonalThreat::Warning => Color::srgb(1.0, 0.85, 0.15),
            PersonalThreat::Attacking => Color::srgb(1.0, 0.2, 0.15),
        }
    }
}

#[derive(Component)]
struct FloatingText {
    world: Vec3,
    age: f32,
    life: f32,
    rise: f32,
    drift: f32,
}

/// Spawn a nameplate (name, health bar, cast bar) for one actor.
pub fn spawn_nameplate(commands: &mut Commands, a: &dungeon_sim::sim::Actor) {
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
        Pickable::IGNORE,
        DespawnOnExit(AppState::Dungeon),
        children![
            (
                Text::new(a.name.clone()),
                text_font(11.0),
                TextShadow::default(),
                PlateName(id),
                Pickable::IGNORE
            ),
            (
                Node {
                    width: px(100.0),
                    height: px(if a.is_enemy() { 8.0 } else { 6.0 }),
                    border: UiRect::all(px(if a.is_enemy() { 1.0 } else { 0.0 })),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.8)),
                BorderColor::all(Color::NONE),
                PlateThreat(id),
                Pickable::IGNORE,
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
                    Pickable::IGNORE,
                )],
            ),
            (
                Text::new(""),
                text_font(10.0),
                TextColor(Color::srgb(1.0, 0.8, 0.3)),
                PlateCast(id),
                Pickable::IGNORE
            ),
        ],
    ));
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
    mut frames: Query<(&PlateThreat, &mut BorderColor)>,
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
        let world = run.world(plate.id, height);
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
        color.0 = if a.is_enemy() {
            threat_color(sim, n.0)
        } else {
            Color::srgb(0.5, 1.0, 0.6)
        };
        let want = if selected {
            format!("> {} <", a.name)
        } else {
            a.name.clone()
        };
        if text.0 != want {
            text.0 = want;
        }
    }
    for (f, mut border) in &mut frames {
        let c = if target == Some(f.0) {
            Color::WHITE
        } else if sim.actors[f.0].is_engaged_enemy() {
            threat_color(sim, f.0)
        } else {
            Color::NONE
        };
        border.set_all(c);
    }
    for (h, mut node) in &mut hps {
        node.width = percent(100.0 * sim.actors[h.0].hp_frac());
    }
    for (c, mut text) in &mut casts {
        let a = &sim.actors[c.0];
        // Cast bars only on enemies: that's what you might need to interrupt.
        let mut want = match a.cast.as_ref().filter(|_| a.is_enemy()) {
            Some(cast) => format!(
                "{} {:.1}s{}",
                cast.name,
                cast.remaining.max(0.0),
                if cast.interruptible { " (kick!)" } else { "" }
            ),
            None => String::new(),
        };
        // Show notable auras (debuffs on allies, control on enemies).
        let auras: Vec<String> = a
            .auras
            .iter()
            .filter(|x| x.harmful && &*x.name != "Taunted" && &*x.name != "Weakened Soul")
            .map(|x| {
                if x.stacks > 1 {
                    format!("{} x{}", x.name, x.stacks)
                } else {
                    x.name.to_string()
                }
            })
            .collect();
        if !auras.is_empty() {
            if !want.is_empty() {
                want.push('\n');
            }
            want.push_str(&auras.join(", "));
        }
        if text.0 != want {
            text.0 = want;
        }
    }
}

/// Floating text, ranked (0.9.2): your own outcomes are largest and live
/// longest; interrupts name the spell stopped; companions' hits and ticks are
/// small and pale, and in a 10-player raid their ordinary auto-attacks and
/// heals are not floated at all (they stay in the log and metrics).
fn spawn_floating_text(mut commands: Commands, run: Res<ActiveRun>) {
    let sim = &run.sim;
    let player = sim.player;
    let raid = sim.party.len() > 5;
    let mine = |src: ActorId| Some(src) == player;
    // Your over-time ticks are smaller than your direct hits.
    let my_tick = |src: ActorId, what: &str| {
        mine(src)
            && player.is_some_and(|p| {
                matches!(
                    own_effect(sim, p, what),
                    Some(Effect::Dot { .. } | Effect::Hot { .. })
                )
            })
    };
    let abs = |absorbed: f32| {
        if absorbed > 0.0 {
            format!(" ({absorbed:.0} abs)")
        } else {
            String::new()
        }
    };
    for (i, e) in run.fresh.iter().enumerate() {
        let jitter = ((sim.tick as usize + i) % 7) as f32 * 0.25 - 0.75;
        let (who, text, color, size, life) = match &e.kind {
            EventKind::Damage {
                src,
                dst,
                what,
                amount,
                outcome,
                absorbed,
                ..
            } => {
                let crit = *outcome == HitOutcome::Crit;
                let involves_player = mine(*src) || Some(*dst) == player;
                if outcome.avoided() || *outcome == HitOutcome::Absorb {
                    if !involves_player && (raid || !sim.actors[*dst].is_enemy()) {
                        continue;
                    }
                    let word = match outcome {
                        HitOutcome::Absorb => format!("Absorbed {absorbed:.0}"),
                        o => {
                            let l = o.label();
                            let mut c = l.chars();
                            c.next()
                                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                                .unwrap_or_default()
                        }
                    };
                    let size = if involves_player { 15.0 } else { 11.0 };
                    (*dst, word, Color::srgb(0.75, 0.75, 1.0), size, 1.1)
                } else if mine(*src) {
                    // Tier 1: your damage.
                    let (color, size) = if crit {
                        (Color::srgb(1.0, 0.7, 0.15), 24.0)
                    } else if my_tick(*src, what) {
                        (Color::srgb(0.95, 0.85, 0.45), 13.0)
                    } else {
                        (Color::srgb(1.0, 0.95, 0.4), 17.0)
                    };
                    let bang = if crit { "!" } else { "" };
                    let s = format!("{amount:.0}{bang}{}", abs(*absorbed));
                    (*dst, s, color, size, 1.3)
                } else if Some(*dst) == player {
                    // Damage you take.
                    let size = if crit { 20.0 } else { 16.0 };
                    let s = format!("-{amount:.0}{}", abs(*absorbed));
                    (*dst, s, Color::srgb(1.0, 0.3, 0.25), size, 1.2)
                } else if sim.actors[*dst].is_enemy() {
                    // Tier 4: companions' hits and ticks.
                    if raid && !crit {
                        continue;
                    }
                    let s = if crit {
                        format!("{amount:.0}!")
                    } else {
                        format!("{amount:.0}")
                    };
                    let size = if crit { 14.0 } else { 11.0 };
                    (*dst, s, Color::srgba(0.8, 0.8, 0.8, 0.8), size, 0.9)
                } else {
                    // Companions hurt.
                    let size = if crit { 14.0 } else { 12.0 };
                    let s = format!("{amount:.0}");
                    (*dst, s, Color::srgb(1.0, 0.3, 0.25), size, 1.0)
                }
            }
            // What was stopped: large for yours (tier 1), pale for companions'.
            EventKind::Interrupt { src, dst, what } => {
                let s = format!("Interrupted: {what}");
                if mine(*src) {
                    (*dst, s, Color::srgb(0.45, 0.85, 1.0), 20.0, 1.6)
                } else {
                    (*dst, s, Color::srgb(0.6, 0.75, 0.9), 14.0, 1.3)
                }
            }
            EventKind::Dispel { dst, what, .. } => (
                *dst,
                format!("-{what}"),
                Color::srgb(0.9, 0.9, 0.5),
                13.0,
                1.1,
            ),
            EventKind::CcBroken { dst, what } => (
                *dst,
                format!("{what} broken"),
                Color::srgb(1.0, 0.6, 0.2),
                14.0,
                1.2,
            ),
            EventKind::Heal {
                src,
                dst,
                what,
                amount,
                crit,
                ..
            } if *amount >= 1.0 => {
                let bang = if *crit { "!" } else { "" };
                let s = format!("+{amount:.0}{bang}");
                if mine(*src) {
                    let size = if *crit {
                        20.0
                    } else if my_tick(*src, what) {
                        13.0
                    } else {
                        16.0
                    };
                    (*dst, s, Color::srgb(0.35, 1.0, 0.45), size, 1.3)
                } else if Some(*dst) == player {
                    (*dst, s, Color::srgb(0.3, 1.0, 0.4), 15.0, 1.1)
                } else {
                    if raid && !*crit {
                        continue;
                    }
                    (*dst, s, Color::srgba(0.3, 0.85, 0.4, 0.8), 11.0, 0.9)
                }
            }
            // Your shields, defensives and crowd control, where they land.
            EventKind::AuraApplied { src, dst, what, .. }
                if mine(*src)
                    && player.is_some_and(|p| {
                        matches!(
                            own_effect(sim, p, what),
                            Some(
                                Effect::Absorb { .. }
                                    | Effect::DamageReduction { .. }
                                    | Effect::Incapacitate { .. }
                            )
                        )
                    }) =>
            {
                (
                    *dst,
                    what.to_string(),
                    Color::srgb(0.75, 0.85, 1.0),
                    15.0,
                    1.3,
                )
            }
            EventKind::Interrupted { src, .. } => (
                *src,
                "Interrupted".to_string(),
                Color::srgb(0.6, 0.8, 1.0),
                14.0,
                1.1,
            ),
            EventKind::Taunt { src, dst } => (
                *dst,
                "Taunted".to_string(),
                Color::srgb(1.0, 0.5, 0.2),
                if mine(*src) { 16.0 } else { 13.0 },
                1.1,
            ),
            EventKind::Death { actor } => (
                *actor,
                "Dies".to_string(),
                Color::srgb(0.7, 0.7, 0.7),
                14.0,
                1.1,
            ),
            EventKind::Revive { actor } => (
                *actor,
                "Rallies!".to_string(),
                Color::srgb(0.6, 1.0, 0.9),
                14.0,
                1.1,
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
            Pickable::IGNORE,
            FloatingText {
                world: to_vec3(a.pos, a.y + height),
                age: 0.0,
                life,
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
