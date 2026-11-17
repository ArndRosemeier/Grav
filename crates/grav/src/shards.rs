//! Shard fall (heart weather): flares shed splinters of the heart, which
//! come down as glowing blue crystals and push everything away until
//! their charge is spent (design doc, "Heart weather"; L1: blue repels).
//!
//! A shard falls along the local down from high above, with a trail of
//! sparks, and strikes with a flash, a bang and a cloud of grit. On the
//! ground it is a repelling source like a dropped repelling stone: it
//! pushes the hero, goods, rubble and people. It fades over its last
//! half minute and crumbles.
//!
//! Challenges also set shards down by hand (`place`).

use crate::flora::hash;
use engine::prelude::*;
use grav_core::{Charge, Profile, Source, SourceId};

/// Seconds a shard is charged once down.
const LIFE: f32 = 180.0;
/// Over its last this many seconds it fades.
const FADE: f32 = 30.0;
/// Seconds a falling shard takes from where it is first seen.
const FALL: f32 = 2.2;
/// How high above the ground it is first seen, m.
const FALL_HEIGHT: f32 = 70.0;
/// A shard's push, g, and reach, m.
const SHARD_G: f32 = 2.2;
const SHARD_REACH: f32 = 11.0;

const CRYSTAL: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let power = m.params[0].x;
    let along = m.uv.y;
    let facing = max(dot(normalize(m.normal), normalize(m.view_dir)), 0.0);
    let rim = pow(1.0 - facing, 3.0);
    let beat = 0.62 + 0.38 * sin(m.time * 2.1 + m.uv.x * 17.0 + along * 5.0);
    // Cracks of brighter light running through the crystal.
    let q = m.object_pos * 9.0;
    let crack = smoothstep(0.93, 1.0, abs(sin(q.x * 1.7 + q.y * 0.6 + sin(q.z * 1.3 + m.time * 0.4) * 1.5)));
    o.color = vec4<f32>(0.32, 0.46, 0.66, 1.0);
    o.roughness = 0.06;
    o.metallic = 0.0;
    let core = vec3<f32>(0.25, 0.5, 1.0) * (0.15 + 0.85 * along * along) * beat;
    o.emission = (core * 1.6 + vec3<f32>(0.55, 0.75, 1.0) * (rim * 0.9 + crack * 2.4)) * power;
    return o;
}
"#;

struct Shard {
    root: EntityId,
    lamp: LampId,
    /// Where it lies (or will, while it falls).
    at: Vec3,
    up: Vec3,
    /// Seconds since it was first seen; negative ones are still falling.
    age: f32,
    id: u32,
    struck: bool,
    /// Seconds its charge takes to wake once down (0: at once).
    wake: f32,
}

pub struct Shards {
    list: Vec<Shard>,
    material: CustomMaterialId,
    mesh: Mesh,
    next_id: u32,
    /// How many have fallen since the start.
    pub fallen: u32,
}

/// A crystal of splinters: seven blades from one root, along `+y`, the
/// tips glowing (`uv.y` 0 at the root, 1 at a tip).
fn crystal() -> EngineResult<Mesh> {
    let mut m = Mesh::new();
    for b in 0..7u32 {
        let k = 9100 + b * 31;
        let lean = if b == 0 { 0.0 } else { 0.35 + 0.5 * hash(k, 1) };
        let turn = b as f32 * 2.3 + hash(k, 2);
        let dir = Vec3::new(lean * turn.cos(), 1.0, lean * turn.sin()).normalize();
        let long = if b == 0 { 1.6 } else { 0.7 + 0.7 * hash(k, 3) };
        let thick = long * (0.12 + 0.05 * hash(k, 4));
        let side = dir.any_orthonormal_vector();
        let other = dir.cross(side);
        let sides = 6;
        let ring = |r: f32, along: f32| -> Vec<Vec3> {
            (0..sides)
                .map(|i| {
                    let a = i as f32 / sides as f32 * std::f32::consts::TAU + 0.3 * hash(k, 5);
                    dir * along + (side * a.cos() + other * a.sin()) * r
                })
                .collect()
        };
        let foot = ring(thick * 0.8, -0.15);
        let body = ring(thick, long * 0.72);
        let u = b as f32 / 7.0;
        let mut ids = Vec::new();
        for (ps, v) in [(&foot, 0.0), (&body, 0.72)] {
            let mut row = Vec::new();
            for p in ps.iter() {
                let id = m.add_point(*p)?;
                m.set_point_uv(id, [u, v])?;
                row.push(id);
            }
            ids.push(row);
        }
        let tip = m.add_point(dir * long)?;
        m.set_point_uv(tip, [u, 1.0])?;
        for i in 0..sides {
            let j = (i + 1) % sides;
            m.add_quad(ids[0][i], ids[0][j], ids[1][j], ids[1][i])?;
            m.add_triangle(ids[1][i], ids[1][j], tip)?;
        }
    }
    Ok(m)
}

impl Shards {
    pub fn new(world: &mut World) -> EngineResult<Self> {
        let material = world.add_material(
            CustomMaterial::new("heart_shard", CRYSTAL).param(0, [1.0, 0.0, 0.0, 0.0]),
        )?;
        Ok(Self {
            list: Vec::new(),
            material,
            mesh: crystal()?,
            next_id: 0,
            fallen: 0,
        })
    }

    /// A shard comes down at `at` (on the ground) whose up is `up`.
    pub fn fall(&mut self, world: &mut World, at: Vec3, up: Vec3) -> EngineResult<()> {
        self.add(world, at, up, -FALL)
    }

    /// A shard already lying at `at` (a challenge's), its charge waking
    /// over `wake` seconds.
    pub fn place(&mut self, world: &mut World, at: Vec3, up: Vec3, wake: f32) -> EngineResult<()> {
        self.add(world, at, up, 0.0)?;
        if let Some(s) = self.list.last_mut() {
            s.wake = wake.max(0.0);
        }
        Ok(())
    }

    fn add(&mut self, world: &mut World, at: Vec3, up: Vec3, age: f32) -> EngineResult<()> {
        let root = world.spawn(self.mesh.clone());
        world.set_custom_material(root, self.material)?;
        let lamp = world.add_lamp(
            Lamp::point(at + up * 0.8, rgb(150, 190, 255), 9.0, 14.0)
                .shadows(true)
                .flicker(0.12),
        )?;
        self.next_id += 1;
        self.fallen += 1;
        let mut s = Shard {
            root,
            lamp,
            at,
            up,
            age,
            id: self.next_id,
            struck: age >= 0.0,
            wake: 0.0,
        };
        Self::pose(world, &mut s)?;
        self.list.push(s);
        Ok(())
    }

    /// Where a shard is drawn: high above while falling, then in the
    /// ground, leaning a little.
    fn pose(world: &mut World, s: &mut Shard) -> EngineResult<()> {
        let k = 9500 + s.id * 7;
        let fall = (-s.age / FALL).clamp(0.0, 1.0);
        let at = s.at + s.up * FALL_HEIGHT * fall * fall;
        let lean = Quat::from_axis_angle(s.up.any_orthonormal_vector(), 0.25 * (hash(k, 1) - 0.5));
        let rot = lean * grav_core::facing(s.up, s.up.any_orthonormal_vector());
        let place = crate::space::place(at - s.up * 0.25, rot).with_scale(1.7 + 0.6 * hash(k, 2));
        world.set_place(s.root, place)
    }

    /// Each frame: falling ones fall and strike, lying ones fade and
    /// crumble.
    pub fn update(&mut self, world: &mut World, dt: f32) -> EngineResult<()> {
        let mut gone = Vec::new();
        for (i, s) in self.list.iter_mut().enumerate() {
            s.age += dt;
            if s.age < 0.0 {
                Self::pose(world, s)?;
                let p = world.world_transform(s.root)?.w_axis.truncate();
                world
                    .particles_mut()
                    .start(ParticleEmitter::sparks(p, s.up, 3))?;
                world.set_lamp_position(s.lamp, p + s.up * 0.8)?;
                continue;
            }
            if !s.struck {
                s.struck = true;
                Self::pose(world, s)?;
                world.set_lamp_position(s.lamp, s.at + s.up * 0.8)?;
                world.particles_mut().start(ParticleEmitter::debris(
                    s.at,
                    rgb(120, 125, 140),
                    24,
                    3.0,
                ))?;
                world
                    .particles_mut()
                    .start(ParticleEmitter::sparks(s.at, s.up, 40))?;
                world.play_sound(
                    &Sound::stock(Stock::Explosion, s.id as usize),
                    PlaySound::at(s.at),
                );
            }
            if s.age > LIFE {
                world.despawn(s.root);
                world.remove_lamp(s.lamp);
                world.particles_mut().start(ParticleEmitter::debris(
                    s.at,
                    rgb(150, 170, 200),
                    10,
                    1.2,
                ))?;
                gone.push(i);
            }
        }
        for i in gone.into_iter().rev() {
            self.list.swap_remove(i);
        }
        // One material for all: its glow follows the brightest shard.
        let power = self
            .list
            .iter()
            .map(|s| Self::charge(s.age, s.wake))
            .fold(0.0, f32::max);
        world.set_material_param(self.material, 0, [power.max(0.15), 0.0, 0.0, 0.0])?;
        Ok(())
    }

    /// How charged a shard is at `age`, 0..1.
    fn charge(age: f32, wake: f32) -> f32 {
        if age < 0.0 {
            1.0
        } else {
            let waking = if wake > 0.0 {
                (age / wake).clamp(0.0, 1.0)
            } else {
                1.0
            };
            ((LIFE - age) / FADE).clamp(0.0, 1.0) * waking
        }
    }

    /// The fields of the shards on the ground.
    pub fn sources(&self) -> Vec<Source> {
        self.list
            .iter()
            .filter(|s| s.age >= 0.0)
            .map(|s| Source {
                id: SourceId(90_000 + s.id),
                charge: Charge::Repelling,
                position: s.at + s.up * 0.5,
                axis: s.up,
                max_g: SHARD_G,
                opening: Self::charge(s.age, s.wake),
                radius: SHARD_REACH,
                profile: Profile::Core,
                time: Default::default(),
                selectivity: Default::default(),
                open_seconds: s.age,
                holder: None,
                faces: grav_core::ALL_FACES,
                frame: Quat::IDENTITY,
            })
            .collect()
    }

    /// Take every shard away (a challenge starting again).
    pub fn clear(&mut self, world: &mut World) {
        for s in self.list.drain(..) {
            world.despawn(s.root);
            world.remove_lamp(s.lamp);
        }
    }

    pub fn count(&self) -> usize {
        self.list.len()
    }
}
