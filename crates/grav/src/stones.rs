//! The hero's stones: inventory, handling verbs, coating, and how they look.

use crate::space::place;
use engine::prelude::*;
use grav_core::{Charge, Coating, Field, Placement, Source, Stone, StoneSpec, Tuning};

/// A thrown stone in flight: it sticks where it lands.
struct Flight {
    stone: usize,
    from: Vec3,
    to: Vec3,
    axis: Vec3,
    on: Option<EntityId>,
    t: f32,
    duration: f32,
}

/// Where a set stone sits on a moving body: offset in the body's frame.
#[derive(Clone, Copy)]
struct Stuck {
    on: EntityId,
    local: Vec3,
    local_axis: Vec3,
}

pub struct Inventory {
    pub stones: Vec<Stone>,
    pub selected: usize,
    pub coating: Coating,
    visuals: Vec<EntityId>,
    lamps: Vec<Option<LampId>>,
    stuck: Vec<Option<Stuck>>,
    flights: Vec<Flight>,
    pub shell: EntityId,
    /// Last message for the HUD and how long it stays.
    pub message: Option<(String, f32)>,
}

/// Sheath travel per second while the key is held.
const SHEATH_RATE: f32 = 0.6;
/// How far the hero reaches to set, close or recover a stone.
pub const REACH: f32 = 3.0;

fn stone_color(c: Charge) -> Color {
    match c {
        Charge::Attracting => rgb(230, 170, 70),
        Charge::Repelling => rgb(110, 180, 255),
    }
}

impl Inventory {
    pub fn new(world: &mut World, tuning: &Tuning) -> EngineResult<Self> {
        let mut inv = Self {
            stones: Vec::new(),
            selected: 0,
            coating: Coating::new(tuning.start_coating),
            visuals: Vec::new(),
            lamps: Vec::new(),
            stuck: Vec::new(),
            flights: Vec::new(),
            shell: world.spawn(Shape::ball((0.0, 0.0, 0.0), 1.0, rgba(140, 190, 255, 28))?),
            message: None,
        };
        world.set_place(inv.shell, Place::new(0.0, -9999.0, 0.0).with_scale(0.001))?;
        for spec in &tuning.start_stones {
            inv.add(world, spec.clone())?;
        }
        Ok(inv)
    }

    pub fn add(&mut self, world: &mut World, spec: StoneSpec) -> EngineResult<()> {
        let id = self.stones.len() as u32 + 1;
        let color = stone_color(spec.charge);
        self.stones.push(Stone::new(id, spec));
        let e = world.spawn(Shape::ball((0.0, 0.0, 0.0), 0.15, color)?);
        world.set_place(e, Place::new(0.0, -9999.0, 0.0))?;
        self.visuals.push(e);
        self.lamps.push(None);
        self.stuck.push(None);
        Ok(())
    }

    pub fn say(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), 3.0));
    }

    pub fn current(&self) -> &Stone {
        &self.stones[self.selected]
    }

    /// Open the selected stone further; a packed stone is taken in hand.
    pub fn open(&mut self, hero: EntityId, amount: f32) {
        let s = &mut self.stones[self.selected];
        if s.placement == Placement::Packed {
            s.placement = Placement::Held { holder: hero.raw() };
        }
        let target = (s.opening + amount).min(1.0);
        s.open_to(target);
    }

    /// Close the selected stone part way, paying coating, if it is in reach.
    pub fn close(&mut self, amount: f32, hero_at: Vec3, world: &World) {
        let at = self.position(self.selected, world);
        let s = &self.stones[self.selected];
        let in_reach = matches!(s.placement, Placement::Held { .. })
            || at.is_some_and(|p| p.distance(hero_at) <= REACH);
        if !in_reach {
            self.say("Too far away to close it (walk to it, or recall with a spring sheath)");
            return;
        }
        let target = (s.opening - amount).max(0.0);
        let mut coating = self.coating;
        let result = self.stones[self.selected].close_to(target, &mut coating);
        self.coating = coating;
        if let Err(e) = result {
            self.say(format!("Not enough coating: {e}"));
        }
    }

    /// Take the selected stone in hand, or put a closed one back in the pack.
    pub fn hold_or_stow(&mut self, hero: EntityId) {
        let s = &mut self.stones[self.selected];
        match s.placement {
            Placement::Packed => s.placement = Placement::Held { holder: hero.raw() },
            Placement::Held { .. } if s.opening <= 0.0 => s.placement = Placement::Packed,
            Placement::Held { .. } => self.say("Close it before packing it"),
            _ => self.say("That stone is placed: recover it first"),
        }
    }

    /// Throw the held stone at `target` (where the aim ray hit).
    pub fn throw(&mut self, from: Vec3, target: Option<RayHit>) {
        let i = self.selected;
        if !matches!(self.stones[i].placement, Placement::Held { .. }) {
            self.say("Hold a stone to throw it");
            return;
        }
        let Some(hit) = target else {
            self.say("Nothing to stick to in throwing range");
            return;
        };
        let d = from.distance(hit.point);
        self.stones[i].placement = Placement::Loose {
            at: from,
            axis: hit.normal,
        };
        self.flights.push(Flight {
            stone: i,
            from,
            to: hit.point + hit.normal * 0.15,
            axis: hit.normal,
            on: hit.entity,
            t: 0.0,
            duration: (d / 25.0).clamp(0.1, 1.2),
        });
    }

    /// Fix the held stone to the surface in reach.
    pub fn set(&mut self, target: Option<RayHit>) {
        let i = self.selected;
        if !matches!(self.stones[i].placement, Placement::Held { .. }) {
            self.say("Hold a stone to set it");
            return;
        }
        match target {
            Some(hit) if hit.distance <= REACH => {
                self.stones[i].placement = Placement::Set {
                    at: hit.point + hit.normal * 0.15,
                    axis: hit.normal,
                    on: hit.entity.map(|e| e.raw()),
                };
                self.stuck[i] = None;
            }
            _ => self.say("No surface within reach"),
        }
    }

    /// Pick up the nearest placed stone in reach.
    pub fn recover(&mut self, hero: EntityId, hero_at: Vec3, world: &World) {
        let best = (0..self.stones.len())
            .filter(|&i| matches!(self.stones[i].placement, Placement::Set { .. }))
            .filter_map(|i| self.position(i, world).map(|p| (i, p.distance(hero_at))))
            .filter(|(_, d)| *d <= REACH)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match best {
            Some((i, _)) => {
                self.stones[i].placement = Placement::Held { holder: hero.raw() };
                self.stuck[i] = None;
                self.selected = i;
            }
            None => self.say("No placed stone within reach"),
        }
    }

    pub fn recall(&mut self) {
        if !self.stones[self.selected].recall() {
            self.say("Recall needs a loaded spring sheath");
        }
    }

    /// Where stone `i` is now (None when packed).
    pub fn position(&self, i: usize, world: &World) -> Option<Vec3> {
        match self.stones[i].placement {
            Placement::Packed | Placement::Held { .. } => {
                (!matches!(self.stones[i].placement, Placement::Packed))
                    .then(|| world.world_transform(self.visuals[i]).ok())
                    .flatten()
                    .map(|m| m.w_axis.truncate())
            }
            Placement::Set { at, .. } | Placement::Loose { at, .. } => Some(at),
        }
    }

    /// Advance flights, follow bodies stones are stuck to, run sheath clocks.
    pub fn tick(&mut self, world: &World, dt: f32) {
        for s in &mut self.stones {
            s.tick(dt);
        }
        let mut landed = Vec::new();
        for (k, f) in self.flights.iter_mut().enumerate() {
            f.t += dt;
            let u = (f.t / f.duration).min(1.0);
            // A gentle arc for the eye; the landing point is what counts.
            let at = f.from.lerp(f.to, u);
            self.stones[f.stone].placement = Placement::Loose { at, axis: f.axis };
            if u >= 1.0 {
                landed.push(k);
            }
        }
        for k in landed.into_iter().rev() {
            let f = self.flights.remove(k);
            self.stones[f.stone].placement = Placement::Set {
                at: f.to,
                axis: f.axis,
                on: f.on.map(|e| e.raw()),
            };
            self.stuck[f.stone] = None;
        }
        // Stones set on a body ride along with it.
        for i in 0..self.stones.len() {
            let Placement::Set {
                at,
                axis,
                on: Some(raw),
            } = self.stones[i].placement
            else {
                continue;
            };
            let on = EntityId::from_raw(raw);
            let Ok(m) = world.world_transform(on) else {
                continue;
            };
            let stuck = *self.stuck[i].get_or_insert_with(|| {
                let inv = m.inverse();
                Stuck {
                    on,
                    local: inv.transform_point3(at),
                    local_axis: inv.transform_vector3(axis),
                }
            });
            if stuck.on == on {
                self.stones[i].placement = Placement::Set {
                    at: m.transform_point3(stuck.local),
                    axis: m.transform_vector3(stuck.local_axis).normalize_or_zero(),
                    on: Some(raw),
                };
            }
        }
        if let Some((_, left)) = &mut self.message {
            *left -= dt;
            if *left <= 0.0 {
                self.message = None;
            }
        }
    }

    /// The sources the stones make now. `hand` is where a held stone is,
    /// `up` the hero's up (a held stone's axis).
    pub fn sources(&self, hero: EntityId, hand: Vec3, up: Vec3) -> Vec<Source> {
        self.stones
            .iter()
            .filter(|s| s.opening > 0.0)
            .filter_map(|s| match s.placement {
                Placement::Held { .. } => Some(s.source_at(hand, up, Some(hero.raw()))),
                _ => s.source(),
            })
            .collect()
    }

    /// Show stones, their lights and the selected stone's cutoff shell.
    pub fn draw(&mut self, world: &mut World, hand: Vec3, show_shell: bool) -> EngineResult<()> {
        for i in 0..self.stones.len() {
            let s = &self.stones[i];
            let at = match s.placement {
                Placement::Packed => None,
                Placement::Held { .. } => Some(hand),
                Placement::Set { at, .. } | Placement::Loose { at, .. } => Some(at),
            };
            let place = at.map_or(Place::new(0.0, -9999.0, 0.0), |p| place(p, Quat::IDENTITY));
            world.set_place(self.visuals[i], place)?;
            // An open stone glows in its charge's colour.
            let glow = at.filter(|_| s.opening > 0.0);
            match (glow, self.lamps[i]) {
                (Some(p), Some(id)) => {
                    let mut lamp = Lamp::point(
                        p,
                        stone_color(s.spec.charge),
                        1.0 + 4.0 * s.opening,
                        4.0 + 6.0 * s.opening,
                    );
                    lamp.position = p;
                    world.set_lamp(id, lamp)?;
                }
                (Some(p), None) => {
                    self.lamps[i] = Some(world.add_lamp(Lamp::point(
                        p,
                        stone_color(s.spec.charge),
                        1.0 + 4.0 * s.opening,
                        4.0 + 6.0 * s.opening,
                    ))?);
                }
                (None, Some(id)) => {
                    world.remove_lamp(id);
                    self.lamps[i] = None;
                }
                (None, None) => {}
            }
        }
        let s = &self.stones[self.selected];
        let at = match s.placement {
            Placement::Held { .. } => Some(hand),
            Placement::Set { at, .. } | Placement::Loose { at, .. } => Some(at),
            Placement::Packed => None,
        };
        match at.filter(|_| show_shell) {
            Some(p) => world.set_place(
                self.shell,
                place(p, Quat::IDENTITY).with_scale(s.spec.radius),
            )?,
            None => world.set_place(self.shell, Place::new(0.0, -9999.0, 0.0).with_scale(0.001))?,
        }
        Ok(())
    }

    /// Sheath change for one frame of holding the open or close key.
    pub fn sheath_step(dt: f32) -> f32 {
        SHEATH_RATE * dt
    }
}

/// The field with every stone's source added.
pub fn field_with(base: &Field, sources: Vec<Source>) -> Field {
    let mut f = base.clone();
    f.sources = sources;
    f
}
