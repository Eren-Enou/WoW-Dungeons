//! The hub: a small plaza with a portal and a recruitment board. The player walks
//! around and manages everything through a clickable, tabbed panel:
//! party, character sheet and equipment, dungeon & difficulty selection, class
//! reference and run history. Then they step into the portal.

use crate::camera::{CameraBlockers, CameraFocus, OrbitCamera};
use crate::common::{AppState, GameData, rarity_color, text_font};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::Pressed;
use bevy::ui_widgets::{Activate, Button as UiButton};
use dungeon_sim::ClassId;
use dungeon_sim::items::ItemSlot;
use dungeon_sim::profile::{PARTY_COMPANIONS, Wearer, xp_to_next};

pub struct HubPlugin;

impl Plugin for HubPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(HubUi::default())
            .add_observer(on_hub_activate)
            .add_systems(OnEnter(AppState::Hub), (setup_hub, spawn_hub_ui))
            .add_systems(
                Update,
                (
                    hub_movement,
                    hub_keys,
                    hub_portal,
                    refresh_companions,
                    rebuild_panel,
                    update_hub_text,
                    style_buttons,
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
const STASH_PAGE: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Party,
    Character,
    Dungeons,
    Classes,
    History,
}

impl Tab {
    const ALL: [Tab; 5] = [
        Tab::Party,
        Tab::Character,
        Tab::Dungeons,
        Tab::Classes,
        Tab::History,
    ];

    fn label(self) -> &'static str {
        match self {
            Tab::Party => "Party",
            Tab::Character => "Character",
            Tab::Dungeons => "Dungeons",
            Tab::Classes => "Classes",
            Tab::History => "History",
        }
    }
}

#[derive(Resource)]
pub struct HubUi {
    pub panel_open: bool,
    pub tab: Tab,
    pub wearer: Wearer,
    pub stash_page: usize,
    pub dirty: bool,
    pub companions_dirty: bool,
}

impl Default for HubUi {
    fn default() -> Self {
        HubUi {
            panel_open: true,
            tab: Tab::Party,
            wearer: Wearer::Player,
            stash_page: 0,
            dirty: true,
            companions_dirty: true,
        }
    }
}

#[derive(Component, Clone, Debug)]
pub enum HubAction {
    Tab(Tab),
    ToggleRecruit(usize),
    CycleProfile(usize),
    AutoFill,
    AutoEquip,
    ChangeClass,
    Inspect(Wearer),
    Equip(usize),
    Unequip(ItemSlot),
    StashPage(i32),
    SelectDungeon(String),
    SelectDifficulty(String),
    Reload,
}

#[derive(Component)]
struct HubButton;

#[derive(Component)]
struct HubPlayer;

#[derive(Component)]
struct HubCompanion;

#[derive(Component)]
struct Portal;

#[derive(Component)]
struct PanelRoot;

#[derive(Component)]
enum HubText {
    Info,
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
    mut blockers: ResMut<CameraBlockers>,
    data: Res<GameData>,
) {
    ui.companions_dirty = true;
    ui.dirty = true;
    orbit.yaw = 0.0;
    blockers.0.clear();
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
        let pos = Vec3::new(
            a.sin() * (HUB_RADIUS + 1.0),
            2.5,
            a.cos() * (HUB_RADIUS + 1.0),
        );
        blockers.0.push((
            pos - Vec3::new(0.6, 2.5, 0.6),
            pos + Vec3::new(0.6, 2.5, 0.6),
        ));
        commands.spawn((
            Mesh3d(pillar.clone()),
            MeshMaterial3d(stone.clone()),
            Transform::from_translation(pos),
            scoped.clone(),
        ));
    }
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
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(3.0, 2.0, 0.2))),
        MeshMaterial3d(materials.add(Color::srgb(0.45, 0.3, 0.15))),
        Transform::from_translation(BOARD_POS + Vec3::Y * 1.8)
            .with_rotation(Quat::from_rotation_y(1.2)),
        scoped.clone(),
    ));
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
            max_width: px(440),
            padding: UiRect::all(px(8)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        children![(Text::new(""), text_font(14.0), HubText::Info)],
        scoped.clone(),
    ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: px(10),
            top: px(10),
            width: px(640),
            max_height: percent(94),
            padding: UiRect::all(px(8)),
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.02, 0.06, 0.82)),
        PanelRoot,
        scoped.clone(),
    ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            width: px(560),
            bottom: px(30),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            Text::new(""),
            text_font(20.0),
            TextColor(Color::srgb(1.0, 0.85, 0.4)),
            TextShadow::default(),
            HubText::Message
        )],
        scoped,
    ));
}

// -------------------------------------------------------------- widgets --

fn label(p: &mut ChildSpawnerCommands, text: impl Into<String>, size: f32, color: Color) {
    p.spawn((Text::new(text.into()), text_font(size), TextColor(color)));
}

fn button(p: &mut ChildSpawnerCommands, text: impl Into<String>, action: HubAction, active: bool) {
    p.spawn((
        UiButton,
        Hovered::default(),
        HubButton,
        action,
        Node {
            padding: UiRect::axes(px(7), px(3)),
            border: UiRect::all(px(1)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BorderColor::all(if active {
            Color::srgb(1.0, 0.85, 0.4)
        } else {
            Color::srgb(0.4, 0.4, 0.5)
        }),
        BackgroundColor(Color::srgb(0.14, 0.14, 0.2)),
        children![(
            Text::new(text.into()),
            text_font(13.0),
            TextColor(if active {
                Color::srgb(1.0, 0.9, 0.5)
            } else {
                Color::WHITE
            })
        )],
    ));
}

fn row(p: &mut ChildSpawnerCommands, f: impl FnOnce(&mut ChildSpawnerCommands)) {
    p.spawn(Node {
        flex_direction: FlexDirection::Row,
        column_gap: px(6),
        align_items: AlignItems::Center,
        flex_wrap: FlexWrap::Wrap,
        ..default()
    })
    .with_children(f);
}

fn style_buttons(mut q: Query<(&Hovered, Has<Pressed>, &mut BackgroundColor), With<HubButton>>) {
    for (hovered, pressed, mut bg) in &mut q {
        bg.0 = if pressed {
            Color::srgb(0.35, 0.55, 0.3)
        } else if hovered.get() {
            Color::srgb(0.25, 0.25, 0.35)
        } else {
            Color::srgb(0.14, 0.14, 0.2)
        };
    }
}

// ------------------------------------------------------------- actions --

fn on_hub_activate(
    ev: On<Activate>,
    actions: Query<&HubAction>,
    mut data: ResMut<GameData>,
    mut ui: ResMut<HubUi>,
) {
    let Ok(action) = actions.get(ev.entity) else {
        return;
    };
    apply_action(action.clone(), &mut data, &mut ui);
}

pub fn apply_action(action: HubAction, data: &mut GameData, ui: &mut HubUi) {
    let mut save = true;
    match action {
        HubAction::Tab(t) => {
            ui.tab = t;
            save = false;
        }
        HubAction::ToggleRecruit(i) => {
            if !data.profile.toggle_recruit(i) {
                data.flash(format!(
                    "Party is full ({PARTY_COMPANIONS}) - dismiss someone first"
                ));
            }
            ui.companions_dirty = true;
        }
        HubAction::CycleProfile(i) => {
            if let Some(c) = data.profile.roster.get_mut(i) {
                c.profile = c.profile.next();
            }
        }
        HubAction::AutoFill => {
            data.profile.auto_fill_party();
            data.flash("Party auto-filled for your role");
            ui.companions_dirty = true;
        }
        HubAction::AutoEquip => {
            let changes = data.profile.auto_equip();
            if changes.is_empty() {
                data.flash("No upgrades in the stash");
            } else {
                data.flash(changes.join("; "));
            }
        }
        HubAction::ChangeClass => {
            let next = data.profile.player_class.next();
            data.profile.player_class = next;
            data.profile.auto_fill_party();
            data.flash(format!(
                "You are now a {} ({}). Party re-balanced.",
                next.label(),
                next.role().label()
            ));
            ui.companions_dirty = true;
        }
        HubAction::Inspect(w) => {
            ui.wearer = w;
            save = false;
        }
        HubAction::Equip(i) => {
            let name = data.profile.stash.get(i).map(|it| it.name.clone());
            match data.profile.equip(i, ui.wearer) {
                Ok(()) => data.flash(format!("Equipped {}", name.unwrap_or_default())),
                Err(e) => data.flash(e),
            }
        }
        HubAction::Unequip(slot) => {
            data.profile.unequip(ui.wearer, slot);
        }
        HubAction::StashPage(d) => {
            let pages = data.profile.stash.len().div_ceil(STASH_PAGE).max(1) as i32;
            ui.stash_page = (ui.stash_page as i32 + d).rem_euclid(pages) as usize;
            save = false;
        }
        HubAction::SelectDungeon(id) => {
            data.profile.selected_dungeon = id;
            if let Some(c) = data.selected_content()
                && c.difficulty(&data.profile.selected_difficulty).is_none()
            {
                let first = c.difficulties[0].id.clone();
                data.profile.selected_difficulty = first;
            }
        }
        HubAction::SelectDifficulty(id) => data.profile.selected_difficulty = id,
        HubAction::Reload => {
            if data.reload_content() {
                data.flash("Content reloaded OK");
            } else {
                data.flash("Content has errors - see the Dungeons tab");
            }
        }
    }
    ui.dirty = true;
    if save {
        data.save();
    }
}

// --------------------------------------------------------------- panel --

fn rebuild_panel(
    mut commands: Commands,
    mut ui: ResMut<HubUi>,
    data: Res<GameData>,
    root: Single<Entity, With<PanelRoot>>,
) {
    if !ui.dirty {
        return;
    }
    ui.dirty = false;
    let root = *root;
    commands.entity(root).despawn_related::<Children>();
    if !ui.panel_open {
        commands.entity(root).with_children(|p| {
            label(p, "Panel hidden (P)", 13.0, Color::WHITE);
        });
        return;
    }
    let ui = &*ui;
    commands.entity(root).with_children(|p| {
        row(p, |r| {
            for t in Tab::ALL {
                button(r, t.label(), HubAction::Tab(t), ui.tab == t);
            }
        });
        match ui.tab {
            Tab::Party => party_tab(p, &data),
            Tab::Character => character_tab(p, &data, ui),
            Tab::Dungeons => dungeons_tab(p, &data),
            Tab::Classes => classes_tab(p),
            Tab::History => history_tab(p, &data),
        }
    });
}

fn party_tab(p: &mut ChildSpawnerCommands, data: &GameData) {
    let pr = &data.profile;
    label(
        p,
        format!(
            "You: {} - level {} {} ({})",
            pr.player_name,
            pr.player_level,
            pr.player_class.label(),
            pr.player_class.role().label()
        ),
        15.0,
        Color::srgb(1.0, 0.9, 0.6),
    );
    row(p, |r| {
        button(r, "Change class", HubAction::ChangeClass, false);
        button(r, "Auto-fill party", HubAction::AutoFill, false);
        button(r, "Auto-equip upgrades", HubAction::AutoEquip, false);
    });
    label(
        p,
        format!(
            "Companions ({}/{} recruited). Click a name to inspect; profile = AI behaviour.",
            pr.party.len(),
            PARTY_COMPANIONS
        ),
        13.0,
        Color::srgb(0.8, 0.8, 0.8),
    );
    for (i, c) in pr.roster.iter().enumerate() {
        let recruited = pr.party.contains(&c.id);
        row(p, |r| {
            button(
                r,
                if recruited { "Dismiss" } else { "Recruit" },
                HubAction::ToggleRecruit(i),
                recruited,
            );
            r.spawn((
                Text::new(format!(
                    "{:<9} L{} {:<8} {:<10}",
                    c.name,
                    c.level,
                    c.class.label(),
                    c.class.role().label()
                )),
                text_font(13.0),
                TextColor(if recruited {
                    crate::hub::class_color(c.class)
                } else {
                    Color::srgb(0.6, 0.6, 0.6)
                }),
            ));
            button(
                r,
                format!("AI: {}", c.profile.label()),
                HubAction::CycleProfile(i),
                false,
            );
            button(
                r,
                "Inspect",
                HubAction::Inspect(Wearer::Companion(i)),
                false,
            );
            label(
                r,
                format!(
                    "runs {}  gear {}  familiarity {:.0}%",
                    c.runs_completed,
                    c.equipment.slots.len(),
                    100.0
                        * dungeon_sim::profile::familiarity(
                            c.clears.get(&pr.selected_dungeon).copied().unwrap_or(0)
                        )
                ),
                12.0,
                Color::srgb(0.7, 0.7, 0.7),
            );
        });
    }
    for w in pr.party_warnings() {
        label(p, format!("! {w}"), 13.0, Color::srgb(1.0, 0.6, 0.3));
    }
    label(
        p,
        "AI profiles: Conservative (safe threat, heals early) / Balanced / Aggressive (fast, risky, no CC) / Support (dispels & shields first) / Mechanics (fast reactions, wide margins).",
        12.0,
        Color::srgb(0.7, 0.75, 0.8),
    );
}

fn character_tab(p: &mut ChildSpawnerCommands, data: &GameData, ui: &HubUi) {
    let pr = &data.profile;
    row(p, |r| {
        button(
            r,
            pr.player_name.clone(),
            HubAction::Inspect(Wearer::Player),
            ui.wearer == Wearer::Player,
        );
        for (i, c) in pr.roster.iter().enumerate() {
            if pr.party.contains(&c.id) {
                button(
                    r,
                    c.name.clone(),
                    HubAction::Inspect(Wearer::Companion(i)),
                    ui.wearer == Wearer::Companion(i),
                );
            }
        }
    });
    let Some(sheet) = pr.sheet(ui.wearer) else {
        return;
    };
    let st = &sheet.stats;
    let xp = if sheet.xp_next > 0 {
        format!("{}/{} xp", sheet.xp, sheet.xp_next)
    } else {
        "max level".into()
    };
    label(
        p,
        format!(
            "{} - level {} {} ({})  {xp}",
            sheet.name,
            sheet.level,
            sheet.class.label(),
            sheet.class.role().label()
        ),
        15.0,
        class_color(sheet.class),
    );
    label(
        p,
        format!(
            "HP {:.0}   {} {:.0}   power +{:.0}%   armor {:.0} ({:.0}% vs equal level)\ncrit {:.1}%   hit {:.1}%   dodge {:.1}%   parry {:.1}%   block {:.1}%   resist {:.0}",
            sheet.max_hp,
            sheet.resource.label(),
            sheet.max_resource,
            sheet.power_pct,
            st.armor,
            sheet.armor_dr * 100.0,
            st.crit * 100.0,
            st.hit * 100.0,
            st.dodge * 100.0,
            st.parry * 100.0,
            st.block * 100.0,
            st.resist
        ),
        13.0,
        Color::WHITE,
    );
    let eq = pr.equipment(ui.wearer).cloned().unwrap_or_default();
    label(p, "Equipment", 14.0, Color::srgb(1.0, 0.9, 0.6));
    for slot in ItemSlot::ALL {
        row(p, |r| {
            label(
                r,
                format!("{:<10}", slot.label()),
                13.0,
                Color::srgb(0.7, 0.7, 0.7),
            );
            match eq.get(slot) {
                Some(item) => {
                    label(
                        r,
                        format!("{} - {}", item.name, item.stats.describe()),
                        13.0,
                        rarity_color(item.rarity),
                    );
                    button(r, "Unequip", HubAction::Unequip(slot), false);
                }
                None => label(r, "-", 13.0, Color::srgb(0.5, 0.5, 0.5)),
            }
        });
    }
    let role = sheet.class.role();
    let pages = pr.stash.len().div_ceil(STASH_PAGE).max(1);
    let page = ui.stash_page.min(pages - 1);
    row(p, |r| {
        label(
            r,
            format!(
                "Stash ({} items) page {}/{}",
                pr.stash.len(),
                page + 1,
                pages
            ),
            14.0,
            Color::srgb(1.0, 0.9, 0.6),
        );
        button(r, "<", HubAction::StashPage(-1), false);
        button(r, ">", HubAction::StashPage(1), false);
    });
    if pr.stash.is_empty() {
        label(
            p,
            "Empty - bosses and elites drop loot.",
            13.0,
            Color::srgb(0.6, 0.6, 0.6),
        );
    }
    for (i, item) in pr
        .stash
        .iter()
        .enumerate()
        .skip(page * STASH_PAGE)
        .take(STASH_PAGE)
    {
        let delta = pr.compare(item, ui.wearer);
        let suits = item.role.suits(role);
        row(p, |r| {
            button(r, "Equip", HubAction::Equip(i), false);
            label(r, item.describe(), 12.0, rarity_color(item.rarity));
            let (txt, col) = if !suits {
                (
                    format!("({} gear)", item.role.label()),
                    Color::srgb(0.8, 0.5, 0.3),
                )
            } else if delta > 0.5 {
                (format!("+{delta:.0} upgrade"), Color::srgb(0.3, 1.0, 0.3))
            } else if delta < -0.5 {
                (format!("{delta:.0}"), Color::srgb(1.0, 0.4, 0.4))
            } else {
                ("=".into(), Color::srgb(0.7, 0.7, 0.7))
            };
            label(r, txt, 12.0, col);
        });
    }
}

fn dungeons_tab(p: &mut ChildSpawnerCommands, data: &GameData) {
    let pr = &data.profile;
    row(p, |r| {
        label(
            r,
            format!("Content: {}", data.content_path),
            12.0,
            Color::srgb(0.7, 0.7, 0.7),
        );
        button(r, "Reload files (F5)", HubAction::Reload, false);
    });
    for d in &data.library.dungeons {
        let selected = d.dungeon.id == pr.selected_dungeon;
        row(p, |r| {
            button(
                r,
                if selected { "Selected" } else { "Select" },
                HubAction::SelectDungeon(d.dungeon.id.clone()),
                selected,
            );
            label(
                r,
                format!(
                    "{} (level {}+)",
                    d.dungeon.name, d.dungeon.recommended_level
                ),
                14.0,
                if selected {
                    Color::srgb(1.0, 0.9, 0.5)
                } else {
                    Color::WHITE
                },
            );
        });
        if selected {
            label(
                p,
                d.dungeon.description.clone(),
                12.0,
                Color::srgb(0.8, 0.8, 0.8),
            );
            row(p, |r| {
                label(r, "Difficulty:", 13.0, Color::WHITE);
                for df in &d.difficulties {
                    button(
                        r,
                        df.name.clone(),
                        HubAction::SelectDifficulty(df.id.clone()),
                        df.id == pr.selected_difficulty,
                    );
                }
            });
            if let Some(df) = d.difficulty(&pr.selected_difficulty) {
                label(
                    p,
                    format!(
                        "{}: enemy HP x{}, damage x{}, enemy level +{}, rewards x{}, loot rarity +{}",
                        df.name,
                        df.hp,
                        df.damage,
                        df.level_bonus,
                        df.reward_multiplier,
                        df.rarity_bonus
                    ),
                    12.0,
                    Color::srgb(0.8, 0.8, 0.6),
                );
            }
            for (i, e) in d.encounters.iter().enumerate() {
                let boss = d.encounter_has_boss(i);
                label(
                    p,
                    format!(
                        "  {} {}{}",
                        if boss { "[BOSS]" } else { "-" },
                        e.display_name(),
                        if e.optional { " (optional)" } else { "" }
                    ),
                    12.0,
                    if boss {
                        Color::srgb(1.0, 0.6, 0.4)
                    } else {
                        Color::srgb(0.75, 0.75, 0.75)
                    },
                );
            }
        }
    }
    for e in &data.content_errors {
        label(p, format!("ERROR: {e}"), 12.0, Color::srgb(1.0, 0.4, 0.3));
    }
}

fn classes_tab(p: &mut ChildSpawnerCommands) {
    for class in ClassId::ALL {
        label(
            p,
            format!(
                "{} - {} ({})",
                class.label(),
                class.role().label(),
                class.resource().label()
            ),
            14.0,
            class_color(class),
        );
        label(p, class.blurb(), 12.0, Color::srgb(0.85, 0.85, 0.85));
        let lines: Vec<String> = class
            .abilities()
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let d = a.def();
                format!(
                    "  {} {} - {}{}{}",
                    i + 1,
                    d.name,
                    d.description,
                    if d.cast_time > 0.0 {
                        format!(" [{:.1}s cast]", d.cast_time)
                    } else {
                        String::new()
                    },
                    if d.cooldown > 0.0 {
                        format!(" [{:.0}s cd]", d.cooldown)
                    } else {
                        String::new()
                    }
                )
            })
            .collect();
        label(p, lines.join("\n"), 11.0, Color::srgb(0.75, 0.8, 0.85));
    }
}

fn history_tab(p: &mut ChildSpawnerCommands, data: &GameData) {
    let pr = &data.profile;
    label(
        p,
        format!(
            "Runs won {} - failed {} - Vault Marks {}",
            pr.runs_won, pr.runs_wiped, pr.marks
        ),
        14.0,
        Color::srgb(1.0, 0.9, 0.6),
    );
    if pr.history.is_empty() {
        label(p, "No runs yet.", 13.0, Color::srgb(0.6, 0.6, 0.6));
    }
    for h in pr.history.iter().rev().take(14) {
        let name = data
            .library
            .get(&h.dungeon)
            .map(|c| c.dungeon.name.clone())
            .unwrap_or_else(|| h.dungeon.clone());
        let color = if h.outcome == "Victory" {
            Color::srgb(0.6, 1.0, 0.6)
        } else {
            Color::srgb(1.0, 0.6, 0.5)
        };
        label(
            p,
            format!(
                "{:<9} {} [{}] {:.0}s, {} deaths, +{} marks, +{} xp{}",
                h.outcome,
                name,
                h.difficulty,
                h.duration,
                h.deaths,
                h.marks,
                h.xp,
                if h.items.is_empty() {
                    String::new()
                } else {
                    format!(", loot: {}", h.items.join(", "))
                }
            ),
            12.0,
            color,
        );
    }
}

// ------------------------------------------------------------- systems --

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

fn hub_keys(keys: Res<ButtonInput<KeyCode>>, mut data: ResMut<GameData>, mut ui: ResMut<HubUi>) {
    if keys.just_pressed(KeyCode::KeyP) {
        ui.panel_open = !ui.panel_open;
        ui.dirty = true;
    }
    if keys.just_pressed(KeyCode::F5) {
        apply_action(HubAction::Reload, &mut data, &mut ui);
    }
    for (i, key) in [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
    ]
    .into_iter()
    .enumerate()
    {
        if keys.just_pressed(key) {
            ui.tab = Tab::ALL[i];
            ui.panel_open = true;
            ui.dirty = true;
        }
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
        data.reload_content();
        if data.selected_content().is_none() {
            data.flash("Cannot enter: the selected dungeon failed to load (see Dungeons tab)");
            return;
        }
        if data.profile.party.is_empty() {
            data.flash("Recruit at least one companion first (Party tab)");
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
                let xp = if p.player_level < dungeon_sim::class::MAX_LEVEL {
                    format!("{}/{}", p.player_xp, xp_to_next(p.player_level))
                } else {
                    "max".into()
                };
                let mut s = format!(
                    "DUNGEON SANDBOX - Hub of the Lantern Ward\n\
                     {} the {} - level {} (xp {xp})\n\
                     Vault Marks: {}   Runs won: {}   Failed: {}\n\n",
                    p.player_name,
                    p.player_class.label(),
                    p.player_level,
                    p.marks,
                    p.runs_won,
                    p.runs_wiped
                );
                match data.selected_content() {
                    Some(c) => {
                        let diff = c
                            .difficulty(&p.selected_difficulty)
                            .map(|d| d.name.clone())
                            .unwrap_or_default();
                        s.push_str(&format!(
                            "Next: {} [{}] (level {}+)\n  walk into the portal and press E\n\n",
                            c.dungeon.name, diff, c.dungeon.recommended_level
                        ));
                    }
                    None => s.push_str("No dungeon selected (see the Dungeons tab)\n\n"),
                }
                s.push_str(
                    "WASD move | RMB-drag / arrows: camera | wheel: zoom\n\
                     Panel: click buttons, or 1-5 switch tabs, P hide\n\
                     F5: reload content files",
                );
                if !data.content_errors.is_empty() {
                    s.push_str(&format!(
                        "\n\n{} CONTENT ERROR(S) - see the Dungeons tab",
                        data.content_errors.len()
                    ));
                }
                s
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

fn animate_portal(time: Res<Time>, mut q: Query<&mut Transform, With<Portal>>) {
    for mut t in &mut q {
        let s = 1.0 + (time.elapsed_secs() * 2.0).sin() * 0.04;
        t.scale = Vec3::splat(s);
    }
}
