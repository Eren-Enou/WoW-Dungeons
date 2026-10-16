//! The viewer's 2D dungeon map (0.8.1, key M).
//!
//! A top-down, orthographic picture of the whole dungeon, drawn on the CPU
//! into a texture from the same data the viewer uses (`Content`, `Nav`,
//! `Structure`). It is independent of 3D lighting and fog, and needs no
//! forge at runtime; it follows the conventions of `dungeon-forge`'s SVG
//! maps. It shows:
//! * floors coloured by height (warm low, cool high), with lava;
//! * room and corridor outlines coloured by purpose (main, alternate,
//!   optional chamber, empty side chamber, shortcut, loop);
//! * the main route, one-way drops, doorways, the entrance;
//! * spawns by rank (boss, elite, trash) and optional packs;
//! * labels with room id, name, role and floor height;
//! * the viewer camera's position and heading.
//!
//! Wheel zooms at the cursor and dragging pans. A click on a room jumps the
//! 3D camera there and closes the map; F fits the dungeon to the screen.
//! The map is rebuilt with the scene, so it follows seed switches and
//! reloads.

use crate::common::{AppState, text_font};
use crate::controls::BlocksWorld;
use crate::structure::Structure;
use crate::viewer::{ViewerCam, ViewerScene};
use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use dungeon_sim::V2;
use dungeon_sim::content::{Content, EnemyRank};
use dungeon_sim::nav::{LinkKind, Nav};
use dungeon_sim::poly::Poly;

/// Pixels per metre (capped so the texture stays at most `MAX_PX` wide).
const PPM: f32 = 6.0;
const MAX_PX: f32 = 2048.0;
/// Margin round the dungeon, metres.
const PAD: f32 = 6.0;

/// A finished map picture and where it sits in the world.
pub struct MapPicture {
    pub image: Image,
    /// World x of the left edge and z of the top edge (north up).
    pub min_x: f32,
    pub max_z: f32,
    pub ppm: f32,
    pub width: u32,
    pub height: u32,
}

impl MapPicture {
    /// World (x, z) to image pixel.
    pub fn to_px(&self, p: V2) -> Vec2 {
        Vec2::new((p.x - self.min_x) * self.ppm, (self.max_z - p.z) * self.ppm)
    }

    /// Image pixel to world (x, z).
    pub fn to_world(&self, px: Vec2) -> V2 {
        V2::new(self.min_x + px.x / self.ppm, self.max_z - px.y / self.ppm)
    }
}

struct Canvas {
    w: i32,
    h: i32,
    data: Vec<u8>,
}

impl Canvas {
    fn put(&mut self, x: i32, y: i32, c: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        let i = ((y * self.w + x) * 4) as usize;
        let a = c[3] as f32 / 255.0;
        for (d, &s) in self.data[i..i + 3].iter_mut().zip(&c[..3]) {
            let old = *d as f32;
            *d = (old + (s as f32 - old) * a).round() as u8;
        }
        self.data[i + 3] = 255;
    }

    fn dot(&mut self, p: Vec2, r: f32, c: [u8; 4]) {
        let ri = r.ceil() as i32;
        for dy in -ri..=ri {
            for dx in -ri..=ri {
                if (dx * dx + dy * dy) as f32 <= r * r {
                    self.put(p.x as i32 + dx, p.y as i32 + dy, c);
                }
            }
        }
    }

    fn line(&mut self, a: Vec2, b: Vec2, width: f32, c: [u8; 4], dash: Option<f32>) {
        let len = a.distance(b).max(0.01);
        let steps = (len * 2.0).ceil() as i32;
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            if let Some(d) = dash
                && ((t * len / d) as i32) % 2 == 1
            {
                continue;
            }
            self.dot(a + (b - a) * t, width * 0.5, c);
        }
    }
}

fn rgba(c: [f32; 3], a: f32) -> [u8; 4] {
    [
        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
        (a.clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

/// Floor colour by depth (0 = highest .. 1 = lowest): light grey-blue high,
/// dark slate low, so the purpose outlines and lava stand out.
fn height_color(t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    let l = |a: f32, b: f32| (a + (b - a) * t) / 255.0;
    [l(160.0, 58.0), l(168.0, 64.0), l(186.0, 86.0)]
}

fn fill_poly(cv: &mut Canvas, pic: &MapPicture, poly: &Poly, c: [u8; 4]) {
    let b = poly.bounds();
    let p0 = pic.to_px(V2::new(b.min.x, b.max.z));
    let p1 = pic.to_px(V2::new(b.max.x, b.min.z));
    for y in p0.y.floor() as i32..=p1.y.ceil() as i32 {
        for x in p0.x.floor() as i32..=p1.x.ceil() as i32 {
            let w = pic.to_world(Vec2::new(x as f32 + 0.5, y as f32 + 0.5));
            if poly.contains(w) {
                cv.put(x, y, c);
            }
        }
    }
}

/// Draw the map of a dungeon.
pub fn draw(content: &Content, nav: &Nav, st: &Structure) -> MapPicture {
    let mut lo = V2::new(f32::INFINITY, f32::INFINITY);
    let mut hi = V2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for s in &nav.surfaces {
        lo = V2::new(lo.x.min(s.bounds.min.x), lo.z.min(s.bounds.min.z));
        hi = V2::new(hi.x.max(s.bounds.max.x), hi.z.max(s.bounds.max.z));
    }
    if !lo.x.is_finite() {
        lo = V2::new(-10.0, -10.0);
        hi = V2::new(10.0, 10.0);
    }
    let (w_m, h_m) = (hi.x - lo.x + 2.0 * PAD, hi.z - lo.z + 2.0 * PAD);
    let ppm = PPM.min(MAX_PX / w_m.max(h_m));
    let (w, h) = ((w_m * ppm).ceil() as u32, (h_m * ppm).ceil() as u32);
    let mut pic = MapPicture {
        image: Image::default(),
        min_x: lo.x - PAD,
        max_z: hi.z + PAD,
        ppm,
        width: w,
        height: h,
    };
    let mut cv = Canvas {
        w: w as i32,
        h: h as i32,
        data: [12u8, 11, 16, 255].repeat((w * h) as usize),
    };
    let (ymin, ymax) = nav
        .surfaces
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), s| {
            (a.min(s.min_y()), b.max(s.max_y()))
        });
    let depth = |y: f32| {
        if ymax - ymin < 0.01 {
            0.0
        } else {
            (ymax - y) / (ymax - ymin)
        }
    };
    for l in &content.dungeon.lava {
        if let Ok(p) = l.shape() {
            fill_poly(&mut cv, &pic, &p, rgba([0.77, 0.26, 0.06], 0.9));
        }
    }
    // Floors, lowest first so decks draw over what they span.
    let mut order: Vec<usize> = (0..nav.surfaces.len()).collect();
    order.sort_by(|&a, &b| nav.surfaces[a].max_y().total_cmp(&nav.surfaces[b].max_y()));
    for &i in &order {
        let s = &nav.surfaces[i];
        let mut c = height_color(depth((s.min_y() + s.max_y()) * 0.5));
        let room = &st.rooms[st.surface_room[i]];
        if room.corridor {
            c = [c[0] * 0.8, c[1] * 0.8, c[2] * 0.8];
        }
        fill_poly(&mut cv, &pic, &s.def.shape, rgba(c, 1.0));
    }
    // Room outlines in their purpose colour: surface edges not shared with
    // another surface of the same room (no seams between floor strips).
    for &i in &order {
        let s = &nav.surfaces[i];
        let ri = st.surface_room[i];
        let room = &st.rooms[ri];
        let col = rgba(room.purpose.color(), 1.0);
        let width = if room.corridor { 1.5 } else { 2.5 };
        for (a, b) in s.def.shape.edges() {
            let mid = (a + b) * 0.5;
            let e = b - a;
            let out = V2::new(e.z, -e.x).normalize_or_zero();
            let probe = |sgn: f32| mid + out * (0.5 * sgn);
            let shared = room.surfaces.iter().any(|&j| {
                j != i
                    && (nav.surfaces[j].def.shape.contains(probe(1.0))
                        || nav.surfaces[j].def.shape.contains(probe(-1.0)))
                    && (nav.surfaces[j].max_y() - s.max_y()).abs() < 1.0
            });
            if !shared {
                cv.line(pic.to_px(a), pic.to_px(b), width, col, None);
            }
        }
    }
    // Doorways.
    for d in &st.doorways {
        let c = V2::new(d.center.x, d.center.z);
        let h = V2::new(d.along.x, d.along.z) * (d.width * 0.5);
        cv.line(
            pic.to_px(c - h),
            pic.to_px(c + h),
            2.0,
            [255, 236, 80, 255],
            None,
        );
    }
    // One-way drops.
    for l in nav.links.iter().filter(|l| l.kind == LinkKind::Drop) {
        let (a, b) = (pic.to_px(l.entry.xz()), pic.to_px(l.exit.xz()));
        cv.line(a, b, 2.5, [255, 160, 40, 255], None);
        cv.dot(b, 4.0, [255, 160, 40, 255]);
    }
    // Main route through the room centres, in order.
    let mut main: Vec<&crate::structure::RoomView> =
        st.rooms.iter().filter(|r| r.order.is_some()).collect();
    main.sort_by_key(|r| r.order);
    for w2 in main.windows(2) {
        let (a, b) = (w2[0].center(), w2[1].center());
        cv.line(
            pic.to_px(V2::new(a.x, a.z)),
            pic.to_px(V2::new(b.x, b.z)),
            2.0,
            [130, 220, 255, 230],
            Some(3.0),
        );
    }
    // Spawns.
    for e in &content.encounters {
        let mut centre = V2::ZERO;
        for m in &e.mobs {
            centre += m.pos * (1.0 / e.mobs.len() as f32);
            let rank = content.templates.get(&m.template).map(|t| t.rank);
            let (r, c) = match rank {
                Some(EnemyRank::Boss) => (6.0, [255, 80, 215, 255]),
                Some(EnemyRank::Elite) => (4.0, [255, 205, 50, 255]),
                _ => (2.5, [240, 240, 240, 255]),
            };
            cv.dot(pic.to_px(m.pos), r + 1.0, [0, 0, 0, 255]);
            cv.dot(pic.to_px(m.pos), r, c);
        }
        if e.optional {
            let c = pic.to_px(centre);
            let r = 5.0 * ppm;
            for k in 0..48 {
                let a = k as f32 / 48.0 * std::f32::consts::TAU;
                cv.dot(
                    c + Vec2::new(a.cos(), a.sin()) * r,
                    1.2,
                    [90, 255, 130, 255],
                );
            }
        }
    }
    let e = content.dungeon.entrance;
    cv.dot(pic.to_px(e), 6.0, [0, 0, 0, 255]);
    cv.dot(pic.to_px(e), 5.0, [60, 255, 110, 255]);
    pic.image = Image::new(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        cv.data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    pic
}

// ------------------------------------------------------------- the panel --

/// The map on screen.
#[derive(Resource)]
pub struct MapView {
    pub open: bool,
    pub zoom: f32,
    /// Screen position of the image's top-left corner.
    pub pan: Vec2,
    pub min_x: f32,
    pub max_z: f32,
    pub ppm: f32,
    pub size: Vec2,
    /// Fit on the next layout pass.
    fit: bool,
    /// Mouse down position and pan at that moment.
    drag: Option<(Vec2, Vec2)>,
    moved: bool,
}

impl MapView {
    /// Fit the whole dungeon on screen at the next layout pass.
    pub fn fit_now(&mut self) {
        self.fit = true;
    }

    pub fn world_to_screen(&self, p: V2) -> Vec2 {
        self.pan
            + Vec2::new((p.x - self.min_x) * self.ppm, (self.max_z - p.z) * self.ppm) * self.zoom
    }

    pub fn screen_to_world(&self, s: Vec2) -> V2 {
        let px = (s - self.pan) / self.zoom;
        V2::new(self.min_x + px.x / self.ppm, self.max_z - px.y / self.ppm)
    }
}

#[derive(Component)]
pub struct MapRoot;

#[derive(Component)]
pub struct MapImage;

#[derive(Component)]
pub struct MapLabel {
    at: V2,
    short: String,
    long: String,
}

#[derive(Component)]
pub struct MapMarker(u8);

/// Build the map picture and its panel (hidden) for the scene.
pub fn spawn_map(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    content: &Content,
    nav: &Nav,
    st: &Structure,
    open: bool,
) {
    let pic = draw(content, nav, st);
    let size = Vec2::new(pic.width as f32, pic.height as f32);
    let (min_x, max_z, ppm) = (pic.min_x, pic.max_z, pic.ppm);
    let handle = images.add(pic.image);
    let scoped = DespawnOnExit(AppState::Viewer);
    commands.insert_resource(MapView {
        open,
        zoom: 1.0,
        pan: Vec2::ZERO,
        min_x,
        max_z,
        ppm,
        size,
        fit: true,
        drag: None,
        moved: false,
    });
    let legend = "MAP (M close, F fit, wheel zoom (names, then details), drag pan, click a room to go there)\n\
        floors: light = high, dark = low (red: lava) | outlines: blue main, cyan alternate, green optional chamber,\n\
        violet empty side chamber, orange overlook/shortcut, pink loop, grey passage\n\
        yellow: doorways | dashed: main route | orange arrow: one-way drop | green dot: entrance\n\
        spawns: magenta boss, yellow elite, white trash; green ring: optional pack | red square: you";
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: Val::Vw(100.0),
                height: Val::Vh(100.0),
                overflow: Overflow::clip(),
                display: if open { Display::Flex } else { Display::None },
                ..default()
            },
            BackgroundColor(Color::srgb(0.02, 0.02, 0.03)),
            BlocksWorld,
            MapRoot,
            GlobalZIndex(5),
            scoped.clone(),
        ))
        .with_children(|p| {
            p.spawn((
                ImageNode::new(handle),
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                Pickable::IGNORE,
                MapImage,
            ));
            for r in &st.rooms {
                if r.corridor {
                    continue;
                }
                let c = r.center();
                let col = r.purpose.color();
                let enc = r
                    .encounter
                    .as_ref()
                    .and_then(|id| content.encounters.iter().find(|e| &e.id == id))
                    .map(|e| {
                        format!(
                            "\n{}{}",
                            e.display_name(),
                            if e.optional { " (optional)" } else { "" }
                        )
                    })
                    .unwrap_or_default();
                let short = format!("{} {}", r.id, r.name);
                let long = format!(
                    "{} {}\n{} | {} | y {:.1}{enc}",
                    r.id,
                    r.name,
                    r.role,
                    r.purpose.label(),
                    r.min.y
                );
                p.spawn((
                    Text::new(short.clone()),
                    text_font(11.0),
                    TextColor(Color::srgb(col[0], col[1], col[2])),
                    Node {
                        position_type: PositionType::Absolute,
                        padding: UiRect::axes(px(2), px(0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.62)),
                    Pickable::IGNORE,
                    MapLabel {
                        at: V2::new(c.x, c.z),
                        short,
                        long,
                    },
                ));
            }
            // Camera marker: a square and three dots showing the heading.
            for k in 0..4u8 {
                let s = if k == 0 { 10.0 } else { 4.0 };
                p.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: px(s),
                        height: px(s),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(1.0, 0.15, 0.15)),
                    Pickable::IGNORE,
                    MapMarker(k),
                ));
            }
            p.spawn((
                Text::new(legend),
                text_font(12.0),
                TextColor(Color::srgb(0.9, 0.9, 0.9)),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(10),
                    top: px(8),
                    padding: UiRect::all(px(6)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
                Pickable::IGNORE,
            ));
        });
}

/// Keys and mouse while the map is open. Returns whether it is open (the
/// 3D camera and picking stand still meanwhile).
#[allow(clippy::too_many_arguments)]
pub fn map_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    window: Single<&Window>,
    scene: Res<ViewerScene>,
    mut cam: ResMut<ViewerCam>,
    mut map: ResMut<MapView>,
    mut root: Query<&mut Node, With<MapRoot>>,
) {
    if keys.just_pressed(KeyCode::KeyM) {
        map.open = !map.open;
        map.drag = None;
    }
    if map.open && keys.just_pressed(KeyCode::Escape) {
        map.open = false;
    }
    if let Ok(mut n) = root.single_mut() {
        n.display = if map.open {
            Display::Flex
        } else {
            Display::None
        };
    }
    if !map.open {
        return;
    }
    if keys.just_pressed(KeyCode::KeyF) {
        map.fit = true;
    }
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    if scroll.delta.y != 0.0 {
        // Zoom about the cursor.
        let before = map.screen_to_world(cursor);
        map.zoom = (map.zoom * 1.25f32.powf(scroll.delta.y.signum())).clamp(0.1, 12.0);
        let after = map.world_to_screen(before);
        map.pan += cursor - after;
    }
    let pressed = mouse.just_pressed(MouseButton::Left) || mouse.just_pressed(MouseButton::Right);
    if pressed {
        map.drag = Some((cursor, map.pan));
        map.moved = false;
    }
    if let Some((start, pan0)) = map.drag {
        if cursor.distance(start) > 4.0 {
            map.moved = true;
        }
        if map.moved {
            map.pan = pan0 + (cursor - start);
        }
        let released =
            mouse.just_released(MouseButton::Left) || mouse.just_released(MouseButton::Right);
        if released {
            map.drag = None;
            if !map.moved && mouse.just_released(MouseButton::Left) {
                let w = map.screen_to_world(cursor);
                if let Some(i) = room_under(&scene, w) {
                    crate::viewer::go_room(&scene, &mut cam, i);
                    map.open = false;
                }
            }
        }
    }
}

/// The room whose floor contains a map point (the highest floor there).
pub fn room_under(scene: &ViewerScene, w: V2) -> Option<usize> {
    scene
        .nav
        .surfaces
        .iter()
        .enumerate()
        .filter(|(_, s)| s.def.shape.contains(w))
        .max_by(|a, b| a.1.max_y().total_cmp(&b.1.max_y()))
        .map(|(i, _)| scene.structure.surface_room[i])
}

/// Position the picture, labels and camera marker.
#[allow(clippy::type_complexity)]
pub fn map_layout(
    window: Single<&Window>,
    cam: Res<ViewerCam>,
    mut map: ResMut<MapView>,
    mut image: Query<&mut Node, (With<MapImage>, Without<MapLabel>, Without<MapMarker>)>,
    mut labels: Query<(&MapLabel, &mut Node, &mut Text), (Without<MapImage>, Without<MapMarker>)>,
    mut markers: Query<(&MapMarker, &mut Node), (Without<MapImage>, Without<MapLabel>)>,
) {
    if !map.open {
        return;
    }
    let win = Vec2::new(window.width(), window.height());
    if map.fit {
        map.fit = false;
        map.zoom = (win.x * 0.94 / map.size.x).min(win.y * 0.86 / map.size.y);
        map.pan = (win - map.size * map.zoom) * 0.5 + Vec2::new(0.0, win.y * 0.05);
    }
    if let Ok(mut n) = image.single_mut() {
        n.left = px(map.pan.x);
        n.top = px(map.pan.y);
        n.width = px(map.size.x * map.zoom);
        n.height = px(map.size.y * map.zoom);
    }
    // Short labels at a glance, full ones once zoomed in.
    let scale = map.zoom * map.ppm;
    for (l, mut n, mut t) in &mut labels {
        let p = map.world_to_screen(l.at);
        n.display = if scale >= 1.2 {
            Display::Flex
        } else {
            Display::None
        };
        // Ids only when fitted (names would overlap), names once zoomed in,
        // everything close up.
        let want = if scale >= 6.0 {
            l.long.as_str()
        } else if scale >= 2.5 {
            l.short.as_str()
        } else {
            l.short.split(' ').next().unwrap_or_default()
        };
        if t.0 != want {
            t.0 = want.to_string();
        }
        n.left = px(p.x - 30.0);
        n.top = px(p.y - 8.0);
    }
    let here = V2::new(cam.pos.x, cam.pos.z);
    let f = cam_forward(&cam);
    for (m, mut n) in &mut markers {
        let p = if m.0 == 0 {
            map.world_to_screen(here) - Vec2::splat(5.0)
        } else {
            map.world_to_screen(here) + Vec2::new(f.x, -f.y) * (8.0 + 7.0 * m.0 as f32)
                - Vec2::splat(2.0)
        };
        n.left = px(p.x);
        n.top = px(p.y);
    }
}

/// The camera's horizontal heading as (x, z).
fn cam_forward(cam: &ViewerCam) -> Vec2 {
    let f = Quat::from_euler(EulerRot::YXZ, cam.yaw, cam.pitch, 0.0) * -Vec3::Z;
    Vec2::new(f.x, f.z).normalize_or_zero()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_coordinates_round_trip_and_floors_are_drawn() {
        let path = format!(
            "{}/../../assets/content/dungeons/08_hollow_reliquary.toml",
            env!("CARGO_MANIFEST_DIR")
        );
        let c = Content::load(&path).unwrap();
        let nav = c.nav();
        let st = Structure::build(&c, &nav);
        let pic = draw(&c, &nav, &st);
        assert!(pic.width <= 2049 && pic.height <= 2049);
        let p = V2::new(12.5, -3.25);
        let q = pic.to_world(pic.to_px(p));
        assert!(q.dist(p) < 1e-3);
        // The middle of every room's first floor is not background.
        let data = pic.image.data.as_ref().unwrap();
        for r in st.rooms.iter().filter(|r| !r.corridor) {
            let s = &nav.surfaces[r.surfaces[0]];
            let px = pic.to_px(s.def.shape.centroid());
            let i = ((px.y as u32 * pic.width + px.x as u32) * 4) as usize;
            assert_ne!(&data[i..i + 3], &[12, 11, 16], "{} not drawn", r.name);
        }
        // Screen <-> world on the panel.
        let view = MapView {
            open: true,
            zoom: 0.7,
            pan: Vec2::new(30.0, 40.0),
            min_x: pic.min_x,
            max_z: pic.max_z,
            ppm: pic.ppm,
            size: Vec2::ONE,
            fit: false,
            drag: None,
            moved: false,
        };
        assert!(view.screen_to_world(view.world_to_screen(p)).dist(p) < 1e-3);
    }
}
