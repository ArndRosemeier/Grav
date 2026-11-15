//! The whole cavern around the test space: the outer wall all the way
//! round, the spires that grow in from it, and the islands and rubble that
//! float between them and the heart (design doc "The world": one giant
//! hollow, very spiky inside).
//!
//! All of it is scenery, kilometres away, so every piece is a level-of-
//! detail set: a patch of wall or a spire is drawn fine only where the
//! camera can be near it and coarser with distance. The meshes are built on
//! worker threads. The near wall, spire and island of the test space are
//! `rock.rs`; here the wall dips behind them and takes over at their edge.

use crate::rock;
use crate::space::{HEART_CENTER, ISLAND_CENTER, WALL_Z};
use engine::prelude::*;
use engine::proc::Noise;
use std::f32::consts::{PI, TAU};

/// Nothing but lodestone lasts nearer the heart than this (its spines
/// reach about 3.3 km).
const CLEAR_OF_HEART: f32 = 3600.0;
/// How far behind the test space's own wall the cavern wall lies there.
pub const BEHIND_HOME_WALL: f32 = 16.0;
/// Every last level draws out to here: past the far side of the cavern.
const EVERYWHERE: f32 = 60_000.0;

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hash(n: u32, k: u32) -> f32 {
    let mut h = n.wrapping_mul(0x9e37_79b9) ^ k.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^= h >> 15;
    (h & 0xff_ffff) as f32 / 16_777_215.0
}

/// One finished piece of the cavern: its levels (finest first, each with
/// the distance it draws to) and where its instances stand.
pub struct Piece {
    pub levels: Vec<(PreparedMesh, f32)>,
    pub places: Vec<Place>,
}

type Job = Box<dyn FnOnce() -> EngineResult<Piece> + Send>;

/// A surface as a mesh around `origin`: `rows` x `cols` quads from
/// `point(s, t)` with s and t in 0..1 (the function is also asked a little
/// outside that range, for the normals). `rock(p)` is the way into the
/// rock at p; faces look the other way. With a `skirt`, the border hangs a
/// curtain that deep into the rock, which hides the cracks between
/// neighbours drawn at different levels.
struct Surface<'a> {
    rows: usize,
    cols: usize,
    origin: Vec3,
    point: &'a dyn Fn(f32, f32) -> Vec3,
    rock: &'a dyn Fn(Vec3) -> Vec3,
    uv: &'a dyn Fn(f32, f32) -> [f32; 2],
    skirt: f32,
}

impl Surface<'_> {
    fn build(&self) -> EngineResult<PreparedMesh> {
        let mut mesh = Mesh::new();
        let white = rgb(255, 255, 255);
        let ring = usize::from(self.skirt > 0.0);
        let (nr, nc) = (self.rows + 1 + 2 * ring, self.cols + 1 + 2 * ring);
        let (es, et) = (0.5 / self.rows as f32, 0.5 / self.cols as f32);
        let mut ids = Vec::with_capacity(nr * nc);
        let mut turned = 0i32;
        for r in 0..nr {
            for c in 0..nc {
                let on_skirt = ring == 1 && (r == 0 || c == 0 || r == nr - 1 || c == nc - 1);
                let s = (r.saturating_sub(ring).min(self.rows)) as f32 / self.rows as f32;
                let t = (c.saturating_sub(ring).min(self.cols)) as f32 / self.cols as f32;
                let mut p = (self.point)(s, t);
                let du = (self.point)(s + es, t) - (self.point)(s - es, t);
                let dv = (self.point)(s, t + et) - (self.point)(s, t - et);
                let into = (self.rock)(p);
                let mut n = du.cross(dv);
                turned += if n.dot(into) > 0.0 { 1 } else { -1 };
                n = if n.length_squared() > 1.0e-10 {
                    let n = n.normalize();
                    if n.dot(into) > 0.0 {
                        -n
                    } else {
                        n
                    }
                } else {
                    -into
                };
                if on_skirt {
                    p += into * self.skirt;
                }
                let id = mesh.add_point(p - self.origin)?;
                mesh.set_point_normal(id, n)?;
                mesh.set_point_color(id, white)?;
                mesh.set_point_uv(id, (self.uv)(s, t))?;
                ids.push(id);
            }
        }
        // du x dv is the face normal of (a, b, c) below; turn the faces
        // round when that points into the rock.
        let flip = turned > 0;
        let at = |r: usize, c: usize| ids[r * nc + c];
        for r in 0..nr - 1 {
            for c in 0..nc - 1 {
                let (a, b, cc, d) = (at(r, c), at(r + 1, c), at(r + 1, c + 1), at(r, c + 1));
                if flip {
                    mesh.add_triangle(a, cc, b)?;
                    mesh.add_triangle(a, d, cc)?;
                } else {
                    mesh.add_triangle(a, b, cc)?;
                    mesh.add_triangle(a, cc, d)?;
                }
            }
        }
        Ok(mesh.prepare())
    }
}

/// The cavern's shape. Cheap to copy around threads.
#[derive(Clone)]
pub struct Cavern {
    noise: Noise,
    center: Vec3,
    radius: f32,
    /// From the heart towards the test space (unit).
    home: Vec3,
}

/// A spire growing in from the wall.
#[derive(Clone, Copy, Debug)]
struct Spire {
    root: Vec3,
    axis: Vec3,
    length: f32,
    radius: f32,
    seed: u32,
}

impl Cavern {
    pub fn new() -> Self {
        let home_wall = Vec3::new(0.0, 0.0, WALL_Z);
        Self {
            noise: Noise::new(9127),
            center: HEART_CENTER,
            radius: HEART_CENTER.distance(home_wall),
            home: (home_wall - HEART_CENTER).normalize(),
        }
    }

    fn fbm(&self, p: Vec3, octaves: u32) -> f32 {
        (self.noise.fbm3(p, octaves, 2.03, 0.5) * 0.5 + 0.5).clamp(0.0, 1.0)
    }

    fn ridged(&self, p: Vec3, octaves: u32) -> f32 {
        let (mut sum, mut amp, mut freq, mut norm) = (0.0, 0.5, 1.0, 0.0);
        for i in 0..octaves {
            let n = self.noise.sample3(p * freq + Vec3::splat(i as f32 * 19.7));
            let r = 1.0 - n.abs().min(1.0);
            sum += r * r * amp;
            norm += amp;
            amp *= 0.5;
            freq *= 2.1;
        }
        sum / norm
    }

    /// The wall's surface in direction `dir` (unit) from the heart.
    pub fn wall_point(&self, dir: Vec3) -> Vec3 {
        self.wall(dir).0
    }

    /// The wall's surface in direction `dir`, and how far that spot
    /// stands into the hollow, 0 (deep in a bay) to 1 (out on a buttress).
    fn wall(&self, dir: Vec3) -> (Vec3, f32) {
        let on = dir * self.radius;
        let plain = self.center + on;
        // Around the test space the wall lies flat behind that space's own
        // wall, and leaves it over a kilometre or so.
        let beside = plain.x.abs().max(plain.y.abs());
        let (home, calm) = if dir.dot(self.home) > 0.3 {
            (
                1.0 - smooth(800.0, 2300.0, beside),
                1.0 - smooth(860.0, 1500.0, beside),
            )
        } else {
            (0.0, 0.0)
        };
        // Vaults and bays kilometres wide, buttresses, ribs, ledges: all of
        // it into the hollow or back from it.
        let vaults = (self.fbm(on / 3100.0, 4) - 0.5) * 1500.0;
        let buttresses = (self.ridged(on / 900.0 + Vec3::splat(13.0), 4) - 0.35) * 420.0;
        let ribs = (self.ridged(on / 210.0 + Vec3::splat(47.0), 3) - 0.3) * 70.0;
        let ledges = {
            let s = self.fbm(on / 1300.0 + Vec3::splat(71.0), 3) * 14.0;
            let f = s - s.floor();
            (smooth(0.0, 0.8, f) - smooth(0.8, 1.0, f)) * 60.0
        };
        let inward = (vaults + buttresses + ribs + ledges) * (1.0 - calm);
        let flat = Vec3::new(plain.x, plain.y, WALL_Z - BEHIND_HOME_WALL);
        let at = plain.lerp(flat, home) - dir * inward;
        (at, ((inward + 500.0) / 1300.0).clamp(0.0, 1.0))
    }

    /// The wall as patches of a cube laid on the sphere, `SIDE` x `SIDE`
    /// on each face.
    fn wall_jobs(&self) -> Vec<Job> {
        const SIDE: usize = 8;
        let faces = [
            (Vec3::X, Vec3::Y, Vec3::Z),
            (Vec3::NEG_X, Vec3::Z, Vec3::Y),
            (Vec3::Y, Vec3::Z, Vec3::X),
            (Vec3::NEG_Y, Vec3::X, Vec3::Z),
            (Vec3::Z, Vec3::X, Vec3::Y),
            (Vec3::NEG_Z, Vec3::Y, Vec3::X),
        ];
        let mut jobs: Vec<Job> = Vec::new();
        for (out, a, b) in faces {
            for i in 0..SIDE {
                for j in 0..SIDE {
                    let cavern = self.clone();
                    jobs.push(Box::new(move || {
                        let dir = |s: f32, t: f32| {
                            // Equal angles, so patches are near equal in size.
                            let u = ((i as f32 + s) / SIDE as f32 * 2.0 - 1.0) * PI / 4.0;
                            let v = ((j as f32 + t) / SIDE as f32 * 2.0 - 1.0) * PI / 4.0;
                            (out + a * u.tan() + b * v.tan()).normalize()
                        };
                        let point = |s: f32, t: f32| cavern.wall_point(dir(s, t));
                        let origin = point(0.5, 0.5);
                        let center = cavern.center;
                        let rock = move |p: Vec3| (p - center).normalize();
                        // Only patches the test space can come near get the
                        // fine level.
                        let near = origin.length() < 5200.0;
                        let mut steps = vec![(40, 7500.0, 70.0), (14, EVERYWHERE, 120.0)];
                        if near {
                            steps.insert(0, (128, 3200.0, 25.0));
                        }
                        let mut levels = Vec::new();
                        for (n, to, skirt) in steps {
                            let mesh = Surface {
                                rows: n,
                                cols: n,
                                origin,
                                point: &point,
                                rock: &rock,
                                uv: &|s, t| [cavern.wall(dir(s, t)).1, 0.0],
                                skirt,
                            }
                            .build()?;
                            levels.push((mesh, to));
                        }
                        Ok(Piece {
                            levels,
                            places: vec![Place::new(origin.x, origin.y, origin.z)],
                        })
                    }));
                }
            }
        }
        jobs
    }

    /// The spires of the cavern, spread over the whole wall.
    fn spires(&self) -> Vec<Spire> {
        const COUNT: u32 = 210;
        let golden = PI * (3.0 - 5.0_f32.sqrt());
        let mut spires = Vec::new();
        for i in 0..COUNT {
            // An even spread, each nudged its own way.
            let y = 1.0 - 2.0 * (i as f32 + 0.5) / COUNT as f32;
            let r = (1.0 - y * y).max(0.0).sqrt();
            let turn = i as f32 * golden;
            let nudge = Vec3::new(hash(i, 1) - 0.5, hash(i, 2) - 0.5, hash(i, 3) - 0.5) * 0.16;
            let dir = (Vec3::new(r * turn.cos(), y, r * turn.sin()) + nudge).normalize();
            let foot = self.wall_point(dir);
            // Spires lean inward, each a little its own way.
            let lean = Vec3::new(hash(i, 4) - 0.5, hash(i, 5) - 0.5, hash(i, 6) - 0.5) * 0.55;
            let axis = ((self.center - foot).normalize() + lean).normalize();
            let size = hash(i, 7);
            let room = foot.distance(self.center) - CLEAR_OF_HEART;
            let length = (700.0 + 2500.0 * size * size).min(room * 0.92);
            let radius = length * (0.075 + 0.05 * hash(i, 8));
            // None through the test space, or rooted in its own wall.
            let to_home = -foot;
            let along = to_home.dot(axis).clamp(0.0, length);
            let miss = (foot + axis * along).length();
            if miss < 900.0 + radius * 2.0 || foot.length() < 1900.0 {
                continue;
            }
            spires.push(Spire {
                root: foot - axis * (radius * 1.6),
                axis,
                length: length + radius * 1.6,
                radius,
                seed: 500 + i,
            });
        }
        spires
    }

    fn spire_job(spire: Spire) -> Job {
        Box::new(move || {
            let noise = Noise::new(spire.seed);
            let fbm = |p: Vec3, octaves: u32| noise.fbm3(p, octaves, 2.03, 0.5) * 0.5 + 0.5;
            let side = spire.axis.any_orthonormal_vector();
            let up = spire.axis.cross(side);
            // Features keep their size in metres on every spire.
            let k = spire.radius / 100.0;
            let point = |s: f32, t: f32| {
                let s = s.clamp(0.0, 1.0);
                let theta = t * TAU;
                let ring = side * theta.cos() + up * theta.sin();
                let on = (ring * spire.radius + spire.axis * (s * spire.length)) / k.max(0.4);
                let taper = 1.0 - s.powf(1.45);
                let flare = (1.0 - smooth(0.0, 0.2, s)).powi(2) * 1.3;
                let mass = (fbm(on * 0.006, 4) - 0.45) * 0.6;
                let flutes = ((theta * 8.0 + fbm(on * 0.005, 2) * 6.0)
                    .sin()
                    .abs()
                    .powf(0.6)
                    - 0.6)
                    * 0.1;
                let shelf = (s * spire.length / k.max(0.4) + fbm(on * 0.004, 3) * 240.0) / 55.0;
                let f = shelf - shelf.floor();
                let shelves = (smooth(0.0, 0.8, f) - smooth(0.8, 1.0, f)) * 0.11;
                let rough = (mass + flutes + shelves) * (0.25 + 0.75 * taper);
                let r = spire.radius * (taper + flare + rough).max(0.0);
                spire.root + spire.axis * (s * spire.length) + ring * r
            };
            let origin = spire.root + spire.axis * (spire.length * 0.5);
            let rock = |p: Vec3| {
                let along = (p - spire.root).dot(spire.axis);
                let off = spire.root + spire.axis * along - p;
                if off.length_squared() > 1.0e-6 {
                    off.normalize()
                } else {
                    -spire.axis
                }
            };
            let near = origin.length() < 5600.0;
            let mut steps = vec![(64, 40, 8500.0), (22, 14, EVERYWHERE)];
            if near {
                steps.insert(0, (190, 112, 3600.0));
            }
            let mut levels = Vec::new();
            for (rows, cols, to) in steps {
                let mesh = Surface {
                    rows,
                    cols,
                    origin,
                    point: &point,
                    rock: &rock,
                    uv: &|s, _| [s, 1.0],
                    skirt: 0.0,
                }
                .build()?;
                levels.push((mesh, to));
            }
            Ok(Piece {
                levels,
                places: vec![Place::new(origin.x, origin.y, origin.z)],
            })
        })
    }

    /// Islands and rubble between the spires and the heart: a handful of
    /// shapes, each drawn many times over at its own size and turn.
    fn island_jobs(&self, spires: &[Spire]) -> Vec<Job> {
        const SHAPES: u32 = 5;
        let clear = |p: Vec3, size: f32| {
            p.length() > 1100.0 + size
                && p.distance(ISLAND_CENTER) > 500.0 + size
                && p.distance(self.center) > CLEAR_OF_HEART + size
                && p.distance(self.center) < self.radius - 900.0
                && spires.iter().all(|s| {
                    let along = (p - s.root).dot(s.axis).clamp(0.0, s.length);
                    (s.root + s.axis * along).distance(p) > s.radius * 1.6 + size * 1.5
                })
        };
        let mut places: Vec<Vec<Place>> = vec![Vec::new(); SHAPES as usize];
        let mut put = |n: u32, at: Vec3, size: f32| {
            let place = Place::new(at.x, at.y, at.z)
                .with_yaw_deg(hash(n, 31) * 360.0)
                .with_pitch_deg(hash(n, 32) * 360.0)
                .with_roll_deg(hash(n, 33) * 360.0)
                .with_scale(size);
            places[(n % SHAPES) as usize].push(place);
        };
        let direction = |n: u32| {
            let y = hash(n, 21) * 2.0 - 1.0;
            let r = (1.0 - y * y).max(0.0).sqrt();
            let turn = hash(n, 22) * TAU;
            Vec3::new(r * turn.cos(), y, r * turn.sin())
        };
        for n in 0..150 {
            let size = 45.0 + 260.0 * hash(n, 23).powi(3);
            let from_heart = CLEAR_OF_HEART + 300.0 + 2000.0 * hash(n, 24);
            let at = self.center + direction(n) * from_heart;
            if !clear(at, size) {
                continue;
            }
            put(n, at, size);
            // Rubble held around some of them.
            if hash(n, 25) < 0.45 {
                for m in 0..(6 + (hash(n, 26) * 14.0) as u32) {
                    let id = 10_000 + n * 40 + m;
                    let small = size * (0.05 + 0.2 * hash(id, 27));
                    let out = direction(id) * size * (1.5 + 2.5 * hash(id, 28));
                    if clear(at + out, small) {
                        put(id, at + out, small);
                    }
                }
            }
        }
        (0..SHAPES)
            .zip(places)
            .map(|(shape, places)| -> Job {
                Box::new(move || {
                    let noise = Noise::new(800 + shape);
                    let stretch = Vec3::new(
                        0.8 + 0.5 * hash(shape, 41),
                        0.6 + 0.4 * hash(shape, 42),
                        0.9 + 0.6 * hash(shape, 43),
                    );
                    let point = |s: f32, t: f32| {
                        let lat = s.clamp(0.003, 0.997) * PI;
                        let lon = t * TAU;
                        let dir =
                            Vec3::new(lat.sin() * lon.sin(), lat.cos(), lat.sin() * lon.cos());
                        let mass = noise.fbm3(dir * 1.3, 4, 2.03, 0.5) * 0.28;
                        let crags = (1.0
                            - noise.fbm3(dir * 3.1 + Vec3::splat(9.0), 3, 2.1, 0.5).abs())
                        .powi(2)
                            * 0.22;
                        dir * stretch * (0.72 + mass + crags)
                    };
                    let rock = |p: Vec3| -p.normalize_or_zero();
                    let mut levels = Vec::new();
                    for (rows, cols, to) in
                        [(64, 128, 2600.0), (22, 44, 6500.0), (9, 18, EVERYWHERE)]
                    {
                        let mesh = Surface {
                            rows,
                            cols,
                            origin: Vec3::ZERO,
                            point: &point,
                            rock: &rock,
                            uv: &|_, _| [0.0, 2.0],
                            skirt: 0.0,
                        }
                        .build()?;
                        levels.push((mesh, to));
                    }
                    Ok(Piece { levels, places })
                })
            })
            .collect()
    }

    /// Build the whole cavern on worker threads.
    pub fn build(&self) -> EngineResult<Vec<Piece>> {
        let spires = self.spires();
        let mut jobs = self.wall_jobs();
        jobs.extend(self.island_jobs(&spires));
        jobs.extend(spires.into_iter().map(Self::spire_job));
        rock::on_workers(jobs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cavern_wall_lies_behind_the_test_space_wall() {
        let cavern = Cavern::new();
        for (x, y) in [(0.0, 0.0), (500.0, -300.0), (-700.0, 600.0)] {
            let dir = (Vec3::new(x, y, WALL_Z) - cavern.center).normalize();
            let p = cavern.wall_point(dir);
            assert!((p.z - (WALL_Z - BEHIND_HOME_WALL)).abs() < 0.5, "{p}");
        }
    }

    #[test]
    fn no_far_spire_or_island_comes_near_the_test_space() {
        let cavern = Cavern::new();
        let spires = cavern.spires();
        assert!(spires.len() > 150, "{}", spires.len());
        for s in &spires {
            let along = (-s.root).dot(s.axis).clamp(0.0, s.length);
            assert!((s.root + s.axis * along).length() > 900.0);
            let tip = s.root + s.axis * s.length;
            assert!(tip.distance(cavern.center) > CLEAR_OF_HEART - 1.0);
        }
    }
}
