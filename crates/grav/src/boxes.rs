//! The hero's stones in their coated boxes (S14 to S20): the belt, the
//! throw with its preview, the box bench, and how boxes look in the world.
//!
//! A closed box is weightless and silent; thrown, it flies straight and air
//! stops it. Once it hangs still, its clockwork opens the faces it was set
//! to open, and the pull spills out of those sides only. Its drop timer
//! bares a little plain mass: the box comes down and the hero picks it up
//! where it lies.

use crate::space::{hidden, place};
use engine::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Sense, Stroke, StrokeKind};
use engine::glam::Mat3;
use engine::prelude::*;
use grav_core::{BoxState, Charge, Face, Field, Flight, Program, Source, Stone, Tuning};

/// How far a lid swings open, degrees.
const LID_OPEN_DEG: f32 = 115.0;
/// How long a lid takes to swing open, seconds.
const LID_SWING_SECONDS: f32 = 0.35;
/// How far the preview's side fans reach, m.
const FAN_REACH: f32 = 1.1;
/// Thickness of the box's frame bars and of its lids, m.
const BAR: f32 = 0.035;
const LID: f32 = 0.018;

/// The reserved charge colours (L1): amber pulls, blue pushes.
pub fn charge_color(c: Charge) -> Color {
    match c {
        Charge::Attracting => rgb(240, 170, 70),
        Charge::Repelling => rgb(110, 180, 255),
    }
}

fn charge_ui(c: Charge) -> Color32 {
    match c {
        Charge::Attracting => Color32::from_rgb(240, 170, 70),
        Charge::Repelling => Color32::from_rgb(110, 180, 255),
    }
}

/// Linear emission for the stone inside.
fn charge_glow(c: Charge) -> [f32; 4] {
    match c {
        Charge::Attracting => [1.0, 0.55, 0.15, 4.0],
        Charge::Repelling => [0.3, 0.6, 1.0, 4.0],
    }
}

/// The stone glows; veins of light crawl over it.
const CORE: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let p = m.object_pos * 40.0;
    let veins = abs(sin(p.x + sin(p.y * 1.7 + m.time) * 1.3) * sin(p.z * 1.3 - m.time * 0.7));
    let glow = 0.35 + 0.65 * smoothstep(0.75, 1.0, 1.0 - veins);
    o.color = vec4<f32>(m.params[0].xyz * 0.4, 1.0);
    o.emission = m.params[0].xyz * m.params[0].w * glow;
    o.roughness = 0.25;
    return o;
}
"#;

/// A face's frame in the box: x along its hinge, y out of the box, z across
/// the face from the hinge.
fn face_basis(face: Face) -> (Vec3, Vec3, Vec3) {
    let n = face.normal();
    let b = if n.y.abs() > 0.5 { Vec3::Z } else { Vec3::Y };
    (n.cross(b), n, b)
}

/// Where a lid's hinge is in the box, and how it turns when `open` (0..1).
fn lid_place(face: Face, half: f32, open: f32) -> Place {
    let (t, n, b) = face_basis(face);
    let size = 2.0 * half - BAR;
    let hinge = n * half - b * (size * 0.5);
    let swing = Quat::from_rotation_x(-(LID_OPEN_DEG * open).to_radians());
    let rot = Quat::from_mat3(&Mat3::from_cols(t, n, b)) * swing;
    place(hinge, rot)
}

/// The box's frame: twelve bars with caps at the corners.
fn frame_mesh(half: f32) -> EngineResult<Shape> {
    let bronze = rgb(96, 76, 52);
    let long = 2.0 * half;
    let mut mesh = Mesh::new();
    for a in [-half, half] {
        for c in [-half, half] {
            mesh.append_translated(
                &Shape::box_at((0.0, 0.0, 0.0), (long, BAR, BAR), bronze)?,
                Vec3::new(0.0, a, c),
            )?;
            mesh.append_translated(
                &Shape::box_at((0.0, 0.0, 0.0), (BAR, long, BAR), bronze)?,
                Vec3::new(a, 0.0, c),
            )?;
            mesh.append_translated(
                &Shape::box_at((0.0, 0.0, 0.0), (BAR, BAR, long), bronze)?,
                Vec3::new(a, c, 0.0),
            )?;
        }
    }
    for x in [-half, half] {
        for y in [-half, half] {
            for z in [-half, half] {
                mesh.append_translated(
                    &Shape::ball((0.0, 0.0, 0.0), BAR * 0.85, rgb(120, 96, 62))?,
                    Vec3::new(x, y, z),
                )?;
            }
        }
    }
    Ok(mesh)
}

/// A lid: a coated plate with its hinge edge at the origin.
fn lid_mesh(half: f32) -> EngineResult<Shape> {
    let size = 2.0 * half - BAR;
    let mut m = Shape::box_at(
        (0.0, -LID * 0.5, size * 0.5),
        (size, LID, size),
        rgb(158, 140, 108),
    )?;
    // A brass catch on the free edge.
    m.append_translated(
        &Shape::box_at((0.0, 0.0, 0.0), (0.06, 0.012, 0.02), rgb(170, 130, 70))?,
        Vec3::new(0.0, 0.004, size - 0.02),
    )?;
    Ok(m)
}

/// A see-through fan over one side: the space that side's field fills.
fn fan_mesh(face: Face, color: Color) -> EngineResult<Shape> {
    let (t, n, b) = face_basis(face);
    let mut m = Mesh::new();
    let apex = m.add_point(Vec3::ZERO)?;
    let corners: Vec<_> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .into_iter()
        .map(|(u, v)| m.add_point((n + t * u + b * v) * FAN_REACH))
        .collect::<EngineResult<_>>()?;
    for k in 0..4 {
        let (a, c) = (corners[k], corners[(k + 1) % 4]);
        // Both sides, so it shows from inside and out.
        m.add_triangle(apex, a, c)?;
        m.add_triangle(apex, c, a)?;
    }
    m.paint_all(color);
    Ok(m)
}

struct BoxLook {
    root: EntityId,
    lids: [EntityId; 6],
    swing: [f32; 6],
    lamp: Option<LampId>,
    has_body: bool,
}

struct Ghost {
    root: EntityId,
    fans: [[EntityId; 6]; 2],
    path: EntityId,
}

/// What the throw would do, worked out each frame while a box is out.
#[derive(Clone, Copy, Debug)]
pub struct Aim {
    pub from: Vec3,
    pub dir: Vec3,
    pub speed: f32,
    /// Where the box would come to rest.
    pub rest: Vec3,
    pub frame: Quat,
}

pub struct Boxes {
    pub stones: Vec<Stone>,
    looks: Vec<BoxLook>,
    pub selected: usize,
    /// The selected box is in the hero's hand, ready to throw.
    pub out: bool,
    /// How far the next throw carries, m (+ and - change it; it stays
    /// set between throws).
    pub range: f32,
    /// The box bench is open.
    pub bench: bool,
    bench_face: usize,
    ghost: Ghost,
    flight: Flight,
    /// Last message for the HUD and how long it stays.
    pub message: Option<(String, f32)>,
    /// Throws since the start (challenges count the stones used).
    pub thrown: u32,
}

impl Boxes {
    pub fn new(world: &mut World, tuning: &Tuning) -> EngineResult<Self> {
        let flight = tuning.flight;
        let half = flight.half_size;
        let mut materials = Vec::new();
        for c in [Charge::Attracting, Charge::Repelling] {
            materials.push(
                world.add_material(
                    CustomMaterial::new("box stone", CORE).param(0, charge_glow(c)),
                )?,
            );
        }
        let mut stones = Vec::new();
        let mut looks = Vec::new();
        for (k, spec) in tuning.start_stones.iter().enumerate() {
            let stone = Stone::new(k as u32 + 1, spec.clone());
            let root = world.spawn(frame_mesh(half)?);
            world.set_place(root, hidden())?;
            let core = world.spawn(Shape::ball((0.0, 0.0, 0.0), half * 0.55, rgb(80, 70, 60))?);
            let material = materials[usize::from(spec.charge == Charge::Repelling)];
            world.set_custom_material(core, material)?;
            world.attach(core, root, Place::new(0.0, 0.0, 0.0))?;
            let mut lids = [root; 6];
            for (i, face) in Face::ALL.into_iter().enumerate() {
                let lid = world.spawn(lid_mesh(half)?);
                world.attach(lid, root, lid_place(face, half, 0.0))?;
                lids[i] = lid;
            }
            stones.push(stone);
            looks.push(BoxLook {
                root,
                lids,
                swing: [0.0; 6],
                lamp: None,
                has_body: false,
            });
        }
        // The preview: a glass box, fans over the sides it will open, and
        // dots along its flight.
        let root = world.spawn(Shape::box_at(
            (0.0, 0.0, 0.0),
            Vec3::splat(2.0 * half),
            rgba(255, 245, 225, 70),
        )?);
        world.set_casts_shadow(root, false)?;
        world.set_place(root, hidden())?;
        let mut fans = [[root; 6]; 2];
        for (c, charge) in [Charge::Attracting, Charge::Repelling]
            .into_iter()
            .enumerate()
        {
            let tint = match charge {
                Charge::Attracting => rgba(240, 170, 70, 26),
                Charge::Repelling => rgba(110, 180, 255, 26),
            };
            for (i, face) in Face::ALL.into_iter().enumerate() {
                let fan = world.spawn(fan_mesh(face, tint)?);
                world.set_casts_shadow(fan, false)?;
                world.attach(fan, root, hidden())?;
                fans[c][i] = fan;
            }
        }
        let path = world.spawn_instanced(Shape::ball(
            (0.0, 0.0, 0.0),
            0.035,
            rgba(255, 236, 200, 170),
        )?);
        world.set_instances(path, &[])?;
        Ok(Self {
            stones,
            looks,
            selected: 0,
            out: false,
            range: 10.0,
            bench: false,
            bench_face: 0,
            ghost: Ghost { root, fans, path },
            flight,
            message: None,
            thrown: 0,
        })
    }

    pub fn say(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), 3.0));
    }

    pub fn current(&self) -> &Stone {
        &self.stones[self.selected]
    }

    /// Number key `k`: pick that box and take it in hand; the same key
    /// again puts it away.
    pub fn pick(&mut self, k: usize) {
        if k >= self.stones.len() {
            return;
        }
        if k == self.selected && self.out {
            self.out = false;
        } else {
            self.selected = k;
            self.out = self.stones[k].is_ready();
            if !self.out {
                self.say(match self.stones[k].state {
                    BoxState::Dropped { .. } => "That box is down: walk to it to pick it up",
                    _ => "That box is out in the world: wait for it to come down",
                });
            }
        }
    }

    /// The throw for the box in hand along `dir` at `speed`: where it
    /// would stop. `up` and `heading` are the hero's (the box's frame).
    pub fn aim(
        &self,
        world: &World,
        hero: EntityId,
        from: Vec3,
        dir: Vec3,
        speed: f32,
        (up, heading): (Vec3, Vec3),
    ) -> Aim {
        let range = self.flight.range(speed);
        let half = self.flight.half_size;
        let hit = world.cast_ray_ignoring(from, dir, range + half, hero);
        let rest = match hit {
            Some(h) => from + dir * (h.distance - half).max(0.0),
            None => from + dir * range,
        };
        let across = dir - up * dir.dot(up);
        let front = across
            .try_normalize()
            .unwrap_or_else(|| grav_core::along_ground(heading, up));
        let frame = grav_core::facing(up, front);
        Aim {
            from,
            dir,
            speed,
            rest,
            frame,
        }
    }

    /// Throw the box in hand as `aim` says.
    pub fn throw(&mut self, aim: &Aim) {
        let i = self.selected;
        if !self.out || !self.stones[i].is_ready() {
            return;
        }
        self.stones[i].throw(aim.from, aim.dir * aim.speed, aim.frame);
        self.out = false;
        self.thrown += 1;
    }

    /// Change the throw's range by `by` metres, within the flight's limits.
    pub fn change_range(&mut self, by: f32) {
        self.range = (self.range + by).clamp(self.flight.min_range, self.flight.max_range);
    }

    pub fn flight(&self) -> &Flight {
        &self.flight
    }

    /// Fly thrown boxes, run clocks, let dropped ones fall, pick them up.
    pub fn tick(
        &mut self,
        world: &mut World,
        hero: EntityId,
        hero_at: Vec3,
        dt: f32,
    ) -> EngineResult<()> {
        let half = self.flight.half_size;
        for i in 0..self.stones.len() {
            if let Some((a, b)) = self.stones[i].fly(&self.flight, dt) {
                let d = b - a;
                let len = d.length();
                if len > 1e-6 {
                    let dir = d / len;
                    if let Some(hit) = world.cast_ray_ignoring(a, dir, len + half, hero) {
                        self.stones[i].rest_at(a + dir * (hit.distance - half).max(0.0));
                    }
                }
            }
            self.stones[i].tick(dt);
            let look = &mut self.looks[i];
            match self.stones[i].state {
                BoxState::Dropped { at } => {
                    if !look.has_body {
                        // Plain mass bared: from now on physics carries it.
                        world.set_place(look.root, place(at, self.stones[i].frame))?;
                        world.add_body(
                            look.root,
                            Body::dynamic().shape(BodyShape::Box(Vec3::splat(half))),
                        )?;
                        look.has_body = true;
                    }
                    let m = world.world_transform(look.root)?;
                    let (_, rot, pos) = m.to_scale_rotation_translation();
                    self.stones[i].moved_to(pos);
                    self.stones[i].frame = rot;
                    if pos.distance(hero_at) < self.flight.pickup_reach {
                        self.stones[i].pick_up();
                        world.remove_body(look.root);
                        look.has_body = false;
                        look.swing = [0.0; 6];
                        let name = self.stones[i].spec.name.clone();
                        self.say(format!("Picked up: {name}"));
                    }
                }
                _ if look.has_body => {
                    world.remove_body(look.root);
                    look.has_body = false;
                }
                _ => {}
            }
        }
        if let Some((_, left)) = &mut self.message {
            *left -= dt;
            if *left <= 0.0 {
                self.message = None;
            }
        }
        Ok(())
    }

    /// The fields the boxes make now.
    pub fn sources(&self) -> Vec<Source> {
        self.stones
            .iter()
            .zip(&self.looks)
            .filter_map(|(s, l)| s.source(l.has_body.then(|| l.root.raw())))
            .collect()
    }

    /// Where box `i` is in the world (None while the hero has it).
    pub fn position(&self, i: usize) -> Option<Vec3> {
        self.stones[i].position()
    }

    /// Place boxes, swing their lids, light open ones, show the preview.
    pub fn draw(
        &mut self,
        world: &mut World,
        hand: Vec3,
        hand_frame: Quat,
        aim: Option<&Aim>,
        dt: f32,
    ) -> EngineResult<()> {
        let half = self.flight.half_size;
        for i in 0..self.stones.len() {
            let s = &self.stones[i];
            let look = &mut self.looks[i];
            let shown = match s.state {
                BoxState::Ready if self.out && i == self.selected => Some((hand, hand_frame)),
                BoxState::Ready => None,
                BoxState::Flying { at, .. } | BoxState::Resting { at } => Some((at, s.frame)),
                BoxState::Dropped { .. } => None, // physics places it
            };
            if !look.has_body {
                world.set_place(
                    look.root,
                    shown.map_or_else(hidden, |(at, rot)| place(at, rot)),
                )?;
            }
            for (k, face) in Face::ALL.into_iter().enumerate() {
                let goal = if s.open[k] { 1.0 } else { 0.0 };
                let step = dt / LID_SWING_SECONDS;
                let w = &mut look.swing[k];
                *w = if goal > *w {
                    (*w + step).min(goal)
                } else {
                    goal
                };
                let eased = *w * *w * (3.0 - 2.0 * *w);
                world.set_place(look.lids[k], lid_place(face, half, eased))?;
            }
            // An open box lights its surroundings in its charge's colour.
            let open = s.open.iter().filter(|o| **o).count();
            let at = s.position().filter(|_| open > 0);
            match (at, look.lamp) {
                (Some(p), Some(id)) => {
                    let lamp = Lamp::point(p, charge_color(s.spec.charge), 1.5 + open as f32, 7.0);
                    world.set_lamp(id, lamp)?;
                }
                (Some(p), None) => {
                    look.lamp = Some(world.add_lamp(Lamp::point(
                        p,
                        charge_color(s.spec.charge),
                        1.5 + open as f32,
                        7.0,
                    ))?);
                }
                (None, Some(id)) => {
                    world.remove_lamp(id);
                    look.lamp = None;
                }
                (None, None) => {}
            }
        }
        // The preview.
        let s = &self.stones[self.selected];
        let charge = usize::from(s.spec.charge == Charge::Repelling);
        let mask = s.programmed_mask();
        match aim {
            Some(a) => {
                world.set_place(self.ghost.root, place(a.rest, a.frame))?;
                for (c, fans) in self.ghost.fans.iter().enumerate() {
                    for (k, fan) in fans.iter().enumerate() {
                        let on = c == charge && mask & (1 << k) != 0;
                        world.set_place(
                            *fan,
                            if on {
                                Place::new(0.0, 0.0, 0.0)
                            } else {
                                hidden()
                            },
                        )?;
                    }
                }
                let length = a.from.distance(a.rest);
                let n = (length / 0.8).floor().min(60.0) as usize;
                let dots: Vec<Place> = (1..=n)
                    .map(|k| {
                        let p = a.from + a.dir * (k as f32 * 0.8);
                        Place::new(p.x, p.y, p.z)
                    })
                    .collect();
                world.set_instances(self.ghost.path, &dots)?;
            }
            None => {
                world.set_place(self.ghost.root, hidden())?;
                world.set_instances(self.ghost.path, &[])?;
            }
        }
        Ok(())
    }

    /// The field as it would be with the box in hand resting where `aim`
    /// says, its programmed faces open (for the preview).
    pub fn preview_field(&self, field: &Field, aim: &Aim) -> Field {
        let s = self.current();
        let mut with = field.clone();
        with.sources.retain(|x| x.id != s.id);
        let mask = s.programmed_mask();
        if mask != 0 {
            with.sources
                .push(s.source_at(aim.rest, aim.frame, mask, None));
        }
        with
    }
}

// ---- The HUD: belt, countdowns in the world, and the box bench ----

fn panel() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(18, 16, 14, 215))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(10))
}

/// The unfolded box: up on top; left, front, right, back across; down
/// below. Returns each face's square.
fn net_rects(origin: Pos2, cell: f32) -> [egui::Rect; 6] {
    let at = |col: f32, row: f32| {
        egui::Rect::from_min_size(
            origin + egui::vec2(col * cell, row * cell),
            egui::vec2(cell - 2.0, cell - 2.0),
        )
    };
    // In Face::ALL order: front, back, left, right, up, down.
    [
        at(1.0, 1.0),
        at(3.0, 1.0),
        at(0.0, 1.0),
        at(2.0, 1.0),
        at(1.0, 0.0),
        at(1.0, 2.0),
    ]
}

fn paint_net(
    painter: &egui::Painter,
    origin: Pos2,
    cell: f32,
    stone: &Stone,
    selected: Option<usize>,
) {
    let col = charge_ui(stone.spec.charge);
    for (k, r) in net_rects(origin, cell).into_iter().enumerate() {
        let armed = stone.program.faces[k].is_some();
        let fill = if stone.open[k] {
            col
        } else if armed {
            col.gamma_multiply(0.45)
        } else {
            Color32::from_rgb(52, 46, 40)
        };
        painter.rect_filled(r, CornerRadius::same(2), fill);
        if selected == Some(k) {
            painter.rect_stroke(
                r,
                CornerRadius::same(2),
                Stroke::new(2.0_f32, Color32::WHITE),
                StrokeKind::Outside,
            );
        }
    }
}

fn status(stone: &Stone, out: bool, hero_at: Vec3) -> String {
    match stone.state {
        BoxState::Ready if out => "in hand".into(),
        BoxState::Ready => "ready".into(),
        BoxState::Flying { .. } => "flying".into(),
        BoxState::Resting { .. } => match stone.drop_countdown() {
            Some(t) => format!("hangs, down in {t:.0} s"),
            None => "hangs".into(),
        },
        BoxState::Dropped { at } => format!("down, {:.0} m away", at.distance(hero_at)),
    }
}

impl Boxes {
    /// The belt at the bottom of the screen: one card per box.
    pub fn belt(&self, ctx: &egui::Context, hero_at: Vec3) {
        egui::Area::new(egui::Id::new("belt"))
            .anchor(Align2::CENTER_BOTTOM, [0.0, -34.0])
            .interactable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (k, s) in self.stones.iter().enumerate() {
                        let selected = k == self.selected;
                        let stroke = if selected {
                            Stroke::new(2.0_f32, charge_ui(s.spec.charge))
                        } else {
                            Stroke::new(1.0_f32, Color32::from_gray(70))
                        };
                        panel().stroke(stroke).show(ui, |ui| {
                            ui.set_min_width(150.0);
                            ui.horizontal(|ui| {
                                let (r, _) =
                                    ui.allocate_exact_size(egui::vec2(48.0, 36.0), Sense::hover());
                                paint_net(ui.painter(), r.min, 12.0, s, None);
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{}  {}", k + 1, s.spec.name))
                                            .color(Color32::from_gray(235))
                                            .strong(),
                                    );
                                    ui.label(
                                        egui::RichText::new(status(
                                            s,
                                            self.out && selected,
                                            hero_at,
                                        ))
                                        .color(Color32::from_gray(170))
                                        .size(12.0),
                                    );
                                });
                            });
                        });
                    }
                });
            });
    }

    /// Countdown rings over boxes hanging in the world.
    pub fn countdowns(&self, ctx: &egui::Context, world: &World, width: f32, height: f32) {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("box rings"),
        ));
        for s in &self.stones {
            let BoxState::Resting { at } = s.state else {
                continue;
            };
            let Some(p) = world.screen_point(at, width, height) else {
                continue;
            };
            let c = Pos2::new(p.x, p.y);
            let col = charge_ui(s.spec.charge);
            let left = s.drop_countdown().unwrap_or(0.0);
            let share = (left / s.program.drop_after.max(0.1)).clamp(0.0, 1.0);
            let r = 26.0;
            painter.circle_stroke(c, r, Stroke::new(3.0_f32, Color32::from_black_alpha(140)));
            let steps = 48;
            let pts: Vec<Pos2> = (0..=((steps as f32 * share) as usize))
                .map(|k| {
                    let a = -std::f32::consts::FRAC_PI_2
                        + k as f32 / steps as f32 * std::f32::consts::TAU;
                    c + egui::vec2(a.cos(), a.sin()) * r
                })
                .collect();
            if pts.len() > 1 {
                painter.add(egui::Shape::line(pts, Stroke::new(3.0_f32, col)));
            }
            let next = (0..6)
                .filter_map(|i| s.face_countdown(i))
                .fold(None, |m: Option<f32>, t| Some(m.map_or(t, |m| m.min(t))));
            let text = match next {
                Some(t) => format!("opens {t:.1}"),
                None => format!("{left:.0}"),
            };
            painter.text(
                c + egui::vec2(0.0, r + 12.0),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(14.0),
                Color32::from_gray(235),
            );
        }
    }

    /// The box bench (Tab): program the selected box's faces and its drop.
    pub fn bench_window(&mut self, ctx: &egui::Context) {
        if !self.bench {
            return;
        }
        let i = self.selected;
        let ready = self.stones[i].is_ready();
        let mut open = self.bench;
        let mut face = self.bench_face;
        let flight_range = self.flight.max_range;
        let s = &mut self.stones[i];
        let col = charge_ui(s.spec.charge);
        egui::Window::new(format!("Box bench: {}", s.spec.name))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, -40.0])
            .frame(panel().stroke(Stroke::new(1.5_f32, col)))
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} stone, {:.1} g, reach {:.0} m",
                        match s.spec.charge {
                            Charge::Attracting => "Attracting",
                            Charge::Repelling => "Repelling",
                        },
                        s.spec.max_g,
                        s.spec.radius
                    ))
                    .color(col),
                );
                ui.label(
                    egui::RichText::new("Front faces where you throw, up is your up. Clocks start when the box comes to rest.")
                        .size(12.0)
                        .color(Color32::from_gray(170)),
                );
                ui.add_space(6.0);
                ui.add_enabled_ui(ready, |ui| {
                    ui.horizontal(|ui| {
                        // The net: click a face to set it.
                        let cell = 46.0;
                        let (r, resp) = ui.allocate_exact_size(egui::vec2(cell * 4.0, cell * 3.0), Sense::click());
                        let rects = net_rects(r.min, cell);
                        if resp.clicked() {
                            if let Some(pos) = resp.interact_pointer_pos() {
                                if let Some(k) = rects.iter().position(|q| q.contains(pos)) {
                                    face = k;
                                }
                            }
                        }
                        let painter = ui.painter();
                        paint_net(painter, r.min, cell, s, Some(face));
                        for (k, q) in rects.iter().enumerate() {
                            let label = match s.program.faces[k] {
                                Some(t) => format!("{}\n{t:.1} s", Face::ALL[k].name()),
                                None => format!("{}\nshut", Face::ALL[k].name()),
                            };
                            painter.text(q.center(), Align2::CENTER_CENTER, label, FontId::proportional(11.0), Color32::from_gray(240));
                        }
                        ui.vertical(|ui| {
                            ui.set_width(230.0);
                            let f = Face::ALL[face];
                            ui.label(egui::RichText::new(format!("{} face", f.name())).strong());
                            let mut armed = s.program.faces[face].is_some();
                            if ui.checkbox(&mut armed, "Opens").changed() {
                                s.program.faces[face] = armed.then_some(1.0);
                            }
                            if let Some(t) = &mut s.program.faces[face] {
                                ui.add(egui::Slider::new(t, 0.0..=30.0).step_by(0.5).suffix(" s").text("after rest"));
                            }
                            ui.add_space(8.0);
                            ui.label(egui::RichText::new("Drop").strong());
                            ui.add(
                                egui::Slider::new(&mut s.program.drop_after, 2.0..=120.0)
                                    .step_by(1.0)
                                    .suffix(" s")
                                    .text("after rest"),
                            );
                            ui.label(
                                egui::RichText::new("Then it comes down: walk to it to pick it up.")
                                    .size(12.0)
                                    .color(Color32::from_gray(170)),
                            );
                        });
                    });
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        let drop = s.program.drop_after;
                        let presets: [(&str, Program); 5] = [
                            ("All round", Program { faces: [Some(1.0); 6], drop_after: drop }),
                            ("Front only", Program::only(&[Face::Front], 1.0, drop)),
                            ("Down only", Program::only(&[Face::Down], 1.0, drop)),
                            ("Up only", Program::only(&[Face::Up], 1.0, drop)),
                            ("Back only", Program::only(&[Face::Back], 1.0, drop)),
                        ];
                        for (name, p) in presets {
                            if ui.button(name).clicked() {
                                s.program = p;
                            }
                        }
                    });
                });
                if !ready {
                    ui.label(
                        egui::RichText::new("This box is out in the world. Program it when you have it back.")
                            .color(Color32::from_rgb(255, 200, 140)),
                    );
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!(
                        "With a box in hand, + and - set the range (up to {flight_range:.0} m), a left click throws, and its number key again puts it away. Tab closes the bench."
                    ))
                    .size(12.0)
                    .color(Color32::from_gray(170)),
                );
            });
        self.bench = open;
        self.bench_face = face;
    }
}

impl Boxes {
    /// The range and a small hint under the crosshair while a box is in
    /// hand.
    pub fn throw_hint(&self, ctx: &egui::Context) {
        if !self.out || self.bench {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("throw hint"),
        ));
        let c = ctx.screen_rect().center();
        painter.text(
            c + egui::vec2(0.0, 64.0),
            Align2::CENTER_CENTER,
            format!("{:.0} m", self.range),
            FontId::proportional(20.0),
            charge_ui(self.current().spec.charge),
        );
        painter.text(
            c + egui::vec2(0.0, 86.0),
            Align2::CENTER_CENTER,
            "+ / \u{2212} range    click throws",
            FontId::proportional(12.0),
            Color32::from_gray(200),
        );
    }
}
