//! Making the field readable: dust that drifts along it (always on), the
//! "stone sense" view of field arrows (held on a key), and the arrow that
//! shows which way the hero would fall (placing preview).

use crate::space::place;
use engine::prelude::*;
use grav_core::{BodyQuery, Field, G};

const DUST: usize = 700;
const DUST_BOX: f32 = 14.0;
const SENSE_HALF: i32 = 3;
const SENSE_STEP: f32 = 3.0;

pub struct Visuals {
    dust: EntityId,
    motes: Vec<(Vec3, Vec3)>,
    seed: u32,
    sense: EntityId,
    fall_arrow: EntityId,
}

/// Deterministic noise for respawning motes.
fn next(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*seed >> 8) as f32 / (1u32 << 24) as f32
}

fn hidden() -> Place {
    Place::new(0.0, -9999.0, 0.0).with_scale(0.001)
}

/// An arrow along +z, 1 m long, from the origin.
fn arrow_mesh(color: Color) -> EngineResult<Shape> {
    let mut m = Shape::box_at((0.0, 0.0, 0.42), (0.06, 0.06, 0.84), color)?;
    m.append_translated(
        &Shape::box_at((0.0, 0.0, 0.0), (0.16, 0.16, 0.16), color)?,
        Vec3::new(0.0, 0.0, 0.9),
    )?;
    Ok(m)
}

impl Visuals {
    pub fn new(world: &mut World, around: Vec3) -> EngineResult<Self> {
        let dust = world.spawn_instanced(Shape::box_at(
            (0.0, 0.0, 0.0),
            (0.035, 0.035, 0.035),
            rgb(210, 200, 180),
        )?);
        let sense = world.spawn_instanced(arrow_mesh(rgb(255, 255, 255))?);
        world.set_instances(sense, &[])?;
        let fall_arrow = world.spawn(arrow_mesh(rgb(255, 80, 200))?);
        world.set_place(fall_arrow, hidden())?;
        let mut v = Self {
            dust,
            motes: Vec::with_capacity(DUST),
            seed: 12345,
            sense,
            fall_arrow,
        };
        for _ in 0..DUST {
            let p = v.random_point(around);
            v.motes.push((p, Vec3::ZERO));
        }
        Ok(v)
    }

    fn random_point(&mut self, around: Vec3) -> Vec3 {
        let mut c = || (next(&mut self.seed) * 2.0 - 1.0) * DUST_BOX;
        around + Vec3::new(c(), c(), c())
    }

    /// Dust drifts with the field, with heavy air drag so it shows the
    /// direction more than the strength. Motes inside rock stay hidden by it.
    pub fn dust(
        &mut self,
        world: &mut World,
        field: &Field,
        around: Vec3,
        dt: f32,
    ) -> EngineResult<()> {
        let mut places = Vec::with_capacity(self.motes.len());
        for i in 0..self.motes.len() {
            let (mut p, mut v) = self.motes[i];
            if p.distance(around) > DUST_BOX * 1.2 {
                p = self.random_point(around);
                v = Vec3::ZERO;
            }
            let a = field.acceleration(&BodyQuery::at(p));
            v += a * dt;
            v *= (-3.0 * dt).exp();
            p += v * dt;
            self.motes[i] = (p, v);
            places.push(Place::new(p.x, p.y, p.z));
        }
        world.set_instances(self.dust, &places)
    }

    /// Stone sense: arrows on a grid around the hero, length and colour by
    /// strength (blue weak, yellow 1 g, red strong).
    pub fn sense(
        &mut self,
        world: &mut World,
        field: Option<&Field>,
        around: Vec3,
    ) -> EngineResult<()> {
        let Some(field) = field else {
            return world.set_instances(self.sense, &[]);
        };
        let snap = (around / SENSE_STEP).round() * SENSE_STEP;
        let mut places = Vec::new();
        for x in -SENSE_HALF..=SENSE_HALF {
            for y in -SENSE_HALF..=SENSE_HALF {
                for z in -SENSE_HALF..=SENSE_HALF {
                    let p = snap + Vec3::new(x as f32, y as f32, z as f32) * SENSE_STEP;
                    let a = field.acceleration(&BodyQuery::at(p));
                    let g = a.length() / G;
                    if g < 0.01 {
                        continue;
                    }
                    let rot = Quat::from_rotation_arc(Vec3::Z, a / a.length());
                    let t = (g / 2.0).clamp(0.0, 1.0);
                    let color = if t < 0.5 {
                        rgb(
                            (80.0 + 350.0 * t) as u8,
                            (140.0 + 200.0 * t) as u8,
                            (255.0 - 300.0 * t) as u8,
                        )
                    } else {
                        rgb(
                            255,
                            (240.0 - 360.0 * (t - 0.5)) as u8,
                            (105.0 - 160.0 * (t - 0.5)).max(0.0) as u8,
                        )
                    };
                    let len = (0.4 + g).min(2.5);
                    places.push(
                        place(p, rot)
                            .with_stretch(Vec3::new(1.0, 1.0, len))
                            .with_tint(color),
                    );
                }
            }
        }
        world.set_instances(self.sense, &places)
    }

    /// Throw preview: which way the hero would fall with the box open
    /// where it will hang. `None` hides it.
    pub fn preview(&mut self, world: &mut World, show: Option<(Vec3, Vec3)>) -> EngineResult<()> {
        match show {
            Some((hero_center, fall)) => {
                let g = fall.length() / G;
                if g > 0.01 {
                    let rot = Quat::from_rotation_arc(Vec3::Z, fall / fall.length());
                    world.set_place(
                        self.fall_arrow,
                        place(hero_center, rot).with_stretch(Vec3::new(
                            1.5,
                            1.5,
                            (0.8 + g * 1.5).min(4.0),
                        )),
                    )?;
                } else {
                    world.set_place(self.fall_arrow, hidden())?;
                }
            }
            None => {
                world.set_place(self.fall_arrow, hidden())?;
            }
        }
        Ok(())
    }
}
