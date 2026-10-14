//! The hub: a small plaza with a portal and a recruitment board. The player walks
//! around, manages the party and gear, then steps into the portal.

use crate::camera::{CameraFocus, OrbitCamera};
use crate::common::{AppState, GameData, text_font};
use bevy::prelude::*;
use dungeon_sim::ClassId;
use dungeon_sim::profile::{PARTY_COMPANIONS, Wearer};

pub struct HubPlugin;

impl Plugin for HubPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(HubUi::default())
            .add_systems(OnEnter(AppState::Hub), (setup_hub, spawn_hub_ui))
            .add_systems(
                Update,
                (
                    hub_movement,
                    hub_keys,
                    hub_portal,
                    refresh_companions,
                    update_hub_text,
                    animate_portal,
                )
                    .chain()
                    .run_if(in_state(AppState::Hub)),
            );
    }
}

const HUB_RADIUS: f32 = 22.0;
const PORTAL_POS: Vec3 = Vec3::new(0.0, 0.0, 16.0);
const BOARD_POS: Vec3 = Vec3::new(-10.0, 0.0, 4.0);

#[derive(Resource)]
pub struct HubUi {
    pub panel_open: bool,
    pub stash_sel: usize,
    pub companions_dirty: bool,
}

impl Default for HubUi {
    fn default() -> Self {
        HubUi {
            panel_open: true,
            stash_sel: 0,
            companions_dirty: true,
        }
    }
}

#[derive(Component)]
struct HubPlayer;

#[derive(Component)]
struct HubCompanion;

#[derive(Component)]
struct Portal;

#[derive(Component)]
enum HubText {
    Info,
    Panel,
    Message,
}

pub fn class_color(class: ClassId) -> Color {
    match class {
        ClassId::Warden => Color::srgb(0.25, 0.45, 0.95),
        ClassId::Mender => Color::srgb(0.95, 0.95, 0.85),
        ClassId::Striker => Color::srgb(0.85, 0.35, 0.2),
        ClassId::Arcanist => Color::srgb(0.6, 0.3, 0.9),
    }
}

fn setup_hub(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ui: ResMut<HubUi>,
    mut orbit: ResMut<OrbitCamera>,
    data: Res<GameData>,
) {
    ui.companions_dirty = true;
    orbit.yaw = 0.0;
    let scoped = DespawnOnExit(AppState::Hub);
    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 400.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            illuminance: 9000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(8.0, 20.0, -6.0).looking_at(Vec3::ZERO, Vec3::Y),
        scoped.clone(),
    ));
    // Plaza floor and a ring of pillars.
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(HUB_RADIUS + 2.0, 0.2))),
        MeshMaterial3d(materials.add(Color::srgb(0.32, 0.36, 0.3))),
        Transform::from_xyz(0.0, -0.1, 0.0),
        scoped.clone(),
    ));
    let pillar = meshes.add(Cylinder::new(0.6, 5.0));
    let stone = materials.add(Color::srgb(0.55, 0.52, 0.48));
    for i in 0..12 {
        let a = i as f32 / 12.0 * std::f32::consts::TAU;
        commands.spawn((
            Mesh3d(pillar.clone()),
            MeshMaterial3d(stone.clone()),
            Transform::from_xyz(
                a.sin() * (HUB_RADIUS + 1.0),
                2.5,
                a.cos() * (HUB_RADIUS + 1.0),
            ),
            scoped.clone(),
        ));
    }
    // Portal: a glowing upright ring with a shimmering disc.
    commands.spawn((
        Mesh3d(meshes.add(Torus::new(1.9, 2.4))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.1, 0.5),
            emissive: LinearRgba::rgb(2.0, 0.6, 4.0),
            ..default()
        })),
        Transform::from_translation(PORTAL_POS + Vec3::Y * 2.6)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        scoped.clone(),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Circle::new(1.9))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.5, 0.3, 1.0, 0.55),
            emissive: LinearRgba::rgb(0.8, 0.3, 1.6),
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_translation(PORTAL_POS + Vec3::Y * 2.6),
        Portal,
        scoped.clone(),
    ));
    commands.spawn((
        PointLight {
            color: Color::srgb(0.7, 0.4, 1.0),
            intensity: 400_000.0,
            range: 20.0,
            ..default()
        },
        Transform::from_translation(PORTAL_POS + Vec3::new(0.0, 3.0, -2.0)),
        scoped.clone(),
    ));
    // Recruitment board.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(3.0, 2.0, 0.2))),
        MeshMaterial3d(materials.add(Color::srgb(0.45, 0.3, 0.15))),
        Transform::from_translation(BOARD_POS + Vec3::Y * 1.8)
            .with_rotation(Quat::from_rotation_y(1.2)),
        scoped.clone(),
    ));
    // The player.
    let class = data.profile.player_class;
    commands
        .spawn((
            Mesh3d(meshes.add(Capsule3d::new(0.5, 1.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: class_color(class),
                emissive: LinearRgba::rgb(0.15, 0.12, 0.02),
                ..default()
            })),
            Transform::from_xyz(0.0, 1.0, 0.0),
            HubPlayer,
            CameraFocus,
            scoped.clone(),
        ))
        .with_child((
            Mesh3d(meshes.add(Cuboid::new(0.3, 0.3, 0.5))),
            MeshMaterial3d(materials.add(Color::srgb(0.9, 0.8, 0.3))),
            Transform::from_xyz(0.0, 0.4, 0.55),
        ));
}

fn spawn_hub_ui(mut commands: Commands) {
    let scoped = DespawnOnExit(AppState::Hub);
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            top: px(10),
            max_width: px(560),
            padding: UiRect::all(px(8)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        children![(Text::new(""), text_font(15.0), HubText::Info)],
        scoped.clone(),
    ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: px(12),
            top: px(10),
            width: px(560),
            padding: UiRect::all(px(8)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.05, 0.7)),
        children![(Text::new(""), text_font(14.0), HubText::Panel)],
        scoped.clone(),
    ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: percent(25),
            width: percent(50),
            bottom: px(40),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            Text::new(""),
            text_font(20.0),
            TextColor(Color::srgb(1.0, 0.85, 0.4)),
            TextLayout::justify(Justify::Center),
            HubText::Message
        )],
        scoped,
    ));
}

fn hub_movement(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    orbit: Res<OrbitCamera>,
    mut player: Single<&mut Transform, With<HubPlayer>>,
) {
    let mut dir = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        dir += orbit.forward();
    }
    if keys.pressed(KeyCode::KeyS) {
        dir -= orbit.forward();
    }
    if keys.pressed(KeyCode::KeyD) {
        dir += orbit.right();
    }
    if keys.pressed(KeyCode::KeyA) {
        dir -= orbit.right();
    }
    if dir.length_squared() > 0.0 {
        let dir = dir.normalize();
        let mut p = player.translation + dir * 7.0 * time.delta_secs();
        let flat = Vec2::new(p.x, p.z);
        if flat.length() > HUB_RADIUS {
            let c = flat.normalize() * HUB_RADIUS;
            p.x = c.x;
            p.z = c.y;
        }
        player.translation = p;
        player.rotation = Quat::from_rotation_y(dir.x.atan2(dir.z));
    }
}

fn digit_pressed(keys: &ButtonInput<KeyCode>) -> Option<usize> {
    const DIGITS: [KeyCode; 8] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
    ];
    DIGITS.iter().position(|k| keys.just_pressed(*k))
}

fn hub_keys(keys: Res<ButtonInput<KeyCode>>, mut data: ResMut<GameData>, mut ui: ResMut<HubUi>) {
    if keys.just_pressed(KeyCode::KeyP) {
        ui.panel_open = !ui.panel_open;
    }
    if keys.just_pressed(KeyCode::F5) {
        if data.reload_content() {
            data.flash("Content reloaded OK");
        } else {
            data.flash("Content has errors - see panel");
        }
    }
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let mut changed = false;
    if keys.just_pressed(KeyCode::KeyC) {
        let next = data.profile.player_class.next();
        data.profile.player_class = next;
        data.profile.auto_fill_party();
        let msg = format!(
            "You are now a {} ({}). Party re-balanced.",
            next.label(),
            next.role().label()
        );
        data.flash(msg);
        changed = true;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        data.profile.auto_fill_party();
        data.flash("Party auto-filled for your role");
        changed = true;
    }
    if let Some(i) = digit_pressed(&keys) {
        if shift {
            // Equip the selected stash item on companion i.
            if data.profile.stash.is_empty() {
                data.flash("Stash is empty - clear the dungeon for loot");
            } else {
                let sel = ui.stash_sel.min(data.profile.stash.len() - 1);
                let item_name = data.profile.stash[sel].name.clone();
                match data.profile.equip(sel, Wearer::Companion(i)) {
                    Ok(()) => {
                        let who = data.profile.roster[i].name.clone();
                        data.flash(format!("{who} equips {item_name}"));
                        changed = true;
                    }
                    Err(e) => data.flash(e),
                }
            }
        } else if i < data.profile.roster.len() {
            if data.profile.toggle_recruit(i) {
                changed = true;
            } else {
                data.flash(format!(
                    "Party is full ({PARTY_COMPANIONS}) - dismiss someone first"
                ));
            }
        }
    }
    if keys.just_pressed(KeyCode::BracketRight) || keys.just_pressed(KeyCode::Tab) {
        ui.stash_sel = (ui.stash_sel + 1) % data.profile.stash.len().max(1);
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        let n = data.profile.stash.len().max(1);
        ui.stash_sel = (ui.stash_sel + n - 1) % n;
    }
    if keys.just_pressed(KeyCode::KeyG) && !data.profile.stash.is_empty() {
        let sel = ui.stash_sel.min(data.profile.stash.len() - 1);
        let item_name = data.profile.stash[sel].name.clone();
        if data.profile.equip(sel, Wearer::Player).is_ok() {
            data.flash(format!("You equip {item_name}"));
            changed = true;
        }
    }
    if changed {
        ui.stash_sel = ui.stash_sel.min(data.profile.stash.len().saturating_sub(1));
        ui.companions_dirty = true;
        data.save();
    }
}

fn hub_portal(
    keys: Res<ButtonInput<KeyCode>>,
    player: Single<&Transform, With<HubPlayer>>,
    mut data: ResMut<GameData>,
    mut next: ResMut<NextState<AppState>>,
) {
    let near = player.translation.with_y(0.0).distance(PORTAL_POS) < 4.0;
    if near && keys.just_pressed(KeyCode::KeyE) {
        // Always pick up the latest content edits when entering.
        if !data.reload_content() {
            data.flash("Cannot enter: content file has errors (see panel)");
            return;
        }
        if data.profile.party.is_empty() {
            data.flash("Recruit at least one companion first (keys 1-8)");
            return;
        }
        next.set(AppState::Dungeon);
    }
}

fn refresh_companions(
    mut commands: Commands,
    mut ui: ResMut<HubUi>,
    data: Res<GameData>,
    existing: Query<Entity, With<HubCompanion>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !ui.companions_dirty {
        return;
    }
    ui.companions_dirty = false;
    for e in &existing {
        commands.entity(e).despawn();
    }
    let capsule = meshes.add(Capsule3d::new(0.5, 1.0));
    let mut recruited = 0;
    let mut idle = 0;
    for c in &data.profile.roster {
        let in_party = data.profile.party.contains(&c.id);
        let pos = if in_party {
            recruited += 1;
            PORTAL_POS + Vec3::new(-4.5 + recruited as f32 * 1.8, 1.0, -5.0)
        } else {
            idle += 1;
            BOARD_POS + Vec3::new(1.5 + (idle % 2) as f32 * 1.4, 1.0, -2.0 + idle as f32 * 1.3)
        };
        let mut color = class_color(c.class);
        if !in_party {
            color = color.mix(&Color::srgb(0.3, 0.3, 0.3), 0.5);
        }
        commands.spawn((
            Mesh3d(capsule.clone()),
            MeshMaterial3d(materials.add(color)),
            Transform::from_translation(pos),
            HubCompanion,
            DespawnOnExit(AppState::Hub),
        ));
    }
}

fn update_hub_text(
    time: Res<Time>,
    mut data: ResMut<GameData>,
    ui: Res<HubUi>,
    player: Single<&Transform, With<HubPlayer>>,
    mut texts: Query<(&mut Text, &HubText)>,
) {
    if let Some((_, t)) = data.message.as_mut() {
        *t -= time.delta_secs();
        if *t <= 0.0 {
            data.message = None;
        }
    }
    let p = &data.profile;
    let near_portal = player.translation.with_y(0.0).distance(PORTAL_POS) < 4.0;
    for (mut text, kind) in &mut texts {
        let s = match kind {
            HubText::Info => {
                let mut s = format!(
                    "DUNGEON SANDBOX - Hub of the Lantern Ward\n\
                     Vault Marks: {}   Runs won: {}   Wipes: {}\n\n\
                     WASD move | RMB-drag / arrows: camera | wheel: zoom\n\
                     P: toggle party panel | E at the portal: enter dungeon\n\
                     F5: reload content file\n\n",
                    p.marks, p.runs_won, p.runs_wiped
                );
                match (&data.content, &data.content_error) {
                    (Some(c), _) => s.push_str(&format!(
                        "Content: '{}' OK\n  enemy HP x{}, damage x{}, Cinder Ring every {}s ({}s warning)\n  file: {}",
                        c.dungeon.name,
                        c.dungeon.hp_multiplier,
                        c.dungeon.damage_multiplier,
                        c.boss.ring_interval,
                        c.boss.ring_telegraph,
                        data.content_path
                    )),
                    (None, Some(e)) => s.push_str(&format!("CONTENT ERROR (fix the file, then F5):\n{e}")),
                    _ => {}
                }
                s
            }
            HubText::Panel => {
                if !ui.panel_open {
                    "Party panel hidden (P)".to_string()
                } else {
                    party_panel_text(&data, ui.stash_sel)
                }
            }
            HubText::Message => {
                let mut s = data
                    .message
                    .as_ref()
                    .map(|(m, _)| m.clone())
                    .unwrap_or_default();
                if near_portal && s.is_empty() {
                    s = "Press E to enter the portal".into();
                }
                s
            }
        };
        if text.0 != s {
            text.0 = s;
        }
    }
}

fn party_panel_text(data: &GameData, stash_sel: usize) -> String {
    let p = &data.profile;
    let gear = |e: &dungeon_sim::profile::Equipment| {
        let w = e.weapon.as_ref().map(|i| i.name.as_str()).unwrap_or("-");
        let a = e.armor.as_ref().map(|i| i.name.as_str()).unwrap_or("-");
        format!("weapon: {w} | armor: {a}")
    };
    let mut s = format!(
        "YOU: {} the {} ({})  [C] change class\n  {}  (+{} power, +{} stamina)\n\n\
         COMPANIONS  [1-8] recruit/dismiss  [R] auto-fill  ({}/{} recruited)\n",
        p.player_name,
        p.player_class.label(),
        p.player_class.role().label(),
        gear(&p.player_equipment),
        p.player_equipment.power(),
        p.player_equipment.stamina(),
        p.party.len(),
        PARTY_COMPANIONS
    );
    for (i, c) in p.roster.iter().enumerate() {
        let mark = if p.party.contains(&c.id) {
            "[x]"
        } else {
            "[ ]"
        };
        s.push_str(&format!(
            " {mark} {} {:<9} {:<8} {:<10} runs {}  {}\n",
            i + 1,
            c.name,
            c.class.label(),
            c.class.role().label(),
            c.runs_completed,
            if c.equipment.items().next().is_some() {
                gear(&c.equipment)
            } else {
                String::new()
            }
        ));
    }
    for w in p.party_warnings() {
        s.push_str(&format!(" ! {w}\n"));
    }
    s.push_str("\nSTASH  [ / ] or Tab select | G equip on you | Shift+1-8 equip on companion\n");
    if p.stash.is_empty() {
        s.push_str("  (empty - defeat the Hollow King for loot)\n");
    }
    for (i, item) in p.stash.iter().enumerate() {
        let cursor = if i == stash_sel.min(p.stash.len().saturating_sub(1)) {
            ">"
        } else {
            " "
        };
        s.push_str(&format!(" {cursor} {}\n", item.describe()));
    }
    s
}

fn animate_portal(time: Res<Time>, mut q: Query<&mut Transform, With<Portal>>) {
    for mut t in &mut q {
        let s = 1.0 + (time.elapsed_secs() * 2.0).sin() * 0.04;
        t.scale = Vec3::splat(s);
    }
}
