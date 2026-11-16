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

use crate::flora::smooth;
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
    /// Near the test space: drawn with the near rock's material and light.
    pub near: bool,
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
    rock: &'a dyn Fn(f32, f32, Vec3) -> Vec3,
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
                let into = (self.rock)(s, t, p);
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

/// How many steps a spire's spine is kept in.
const SPINE: usize = 48;

/// A spire grown in from the wall along its vein. Veins wander, fork and
/// swell, so no spire is a straight cone: its spine bends and kinks, its
/// girth bulges and narrows, its section is lobed, and some have lost
/// their tips.
#[derive(Clone, Debug)]
struct Spire {
    /// The spine from root to tip: position and the two directions across.
    spine: Vec<(Vec3, Vec3, Vec3)>,
    length: f32,
    radius: f32,
    /// How fast it narrows (1 a cone, more a tower that narrows late).
    taper: f32,
    /// Where along it the tip has broken off (1: it has not).
    broken_at: f32,
    seed: u32,
}

impl Spire {
    /// Grow a spire from `root` setting out along `axis`, bending towards
    /// `heart` and wandering on the way.
    fn grow(
        noise: &Noise,
        root: Vec3,
        axis: Vec3,
        length: f32,
        radius: f32,
        heart: Vec3,
        seed: u32,
    ) -> Self {
        let side0 = axis.any_orthonormal_vector();
        let up0 = axis.cross(side0);
        let key = seed as f32 * 7.31;
        let wander = 0.35 + 0.75 * hash(seed, 51);
        // One kink, where the vein once changed its mind.
        let kink_at = 0.25 + 0.5 * hash(seed, 52);
        let kink_turn = hash(seed, 53) * TAU;
        let kink =
            (side0 * kink_turn.cos() + up0 * kink_turn.sin()) * (0.5 * hash(seed, 54).powi(2));
        let mut spine = Vec::with_capacity(SPINE + 1);
        let (mut at, mut side) = (root, side0);
        for i in 0..=SPINE {
            let u = i as f32 / SPINE as f32;
            let a = noise.fbm3(Vec3::new(u * 1.7, key, 0.3), 2, 2.0, 0.5);
            let b = noise.fbm3(Vec3::new(u * 1.7, 0.7, key), 2, 2.0, 0.5);
            let free = smooth(0.05, 0.35, u);
            let to_heart = (heart - at).normalize_or_zero() * 0.3 * u;
            let turn =
                (side0 * a + up0 * b) * wander * free + kink * smooth(kink_at, kink_at + 0.08, u);
            let dir = (axis + turn + to_heart).normalize();
            side = (side - dir * side.dot(dir)).normalize_or(side0);
            spine.push((at, side, dir.cross(side)));
            at += dir * (length / SPINE as f32);
        }
        let broken_at = if hash(seed, 55) < 0.3 {
            0.55 + 0.35 * hash(seed, 56)
        } else {
            1.0
        };
        Self {
            spine,
            length,
            radius,
            taper: 0.9 + 1.6 * hash(seed, 57).powi(2),
            broken_at,
            seed,
        }
    }

    /// The spine at `u` (0 root, 1 tip): position, the two directions
    /// across, and the direction along.
    fn at(&self, u: f32) -> (Vec3, Vec3, Vec3, Vec3) {
        let f = u.clamp(0.0, 1.0) * SPINE as f32;
        let i = (f as usize).min(SPINE - 1);
        let t = f - i as f32;
        let (a, b) = (self.spine[i], self.spine[i + 1]);
        let side = a.1.lerp(b.1, t).normalize();
        let up = a.2.lerp(b.2, t).normalize();
        (a.0.lerp(b.0, t), side, up, side.cross(up))
    }

    /// Where the spire ends: its tip, or where it broke.
    fn end(&self) -> f32 {
        (self.broken_at + 0.03).min(1.0)
    }

    /// Its plain girth at `u`, without lobes or roughness, in metres.
    fn girth(&self, noise: &Noise, u: f32) -> f32 {
        let key = self.seed as f32 * 3.17;
        let narrowing = 1.0 - u.clamp(0.0, 1.0).powf(self.taper);
        let bulge = 1.0 + 0.38 * noise.fbm3(Vec3::new(u * 4.5, key, 9.0), 2, 2.0, 0.5);
        let stump = if self.broken_at < 1.0 {
            1.0 - smooth(self.broken_at, self.broken_at + 0.03, u)
        } else {
            1.0
        };
        self.radius * (narrowing * bulge).max(0.0) * stump
    }

    /// The surface at `s` along (0 root to 1 at its end) and `t` around.
    fn point(&self, noise: &Noise, s: f32, t: f32) -> Vec3 {
        let u = s.clamp(0.0, 1.0) * self.end();
        let (at, side, up, _) = self.at(u);
        let theta = t * TAU;
        let ring = side * theta.cos() + up * theta.sin();
        let key = Vec3::splat(self.seed as f32 * 1.93);
        // No spire is round. One side of it is a cliff and the other a long
        // slope, and which side that is turns slowly along it.
        let facing = hash(self.seed, 61) * TAU
            + 2.4 * noise.fbm3(Vec3::new(u * 1.3, 4.0, key.x), 2, 2.0, 0.5);
        let lee = (theta - facing).cos();
        let lean = 0.25 + 0.55 * hash(self.seed, 62);
        let flank = 1.0 + lean * if lee > 0.0 { lee.powf(1.5) } else { 0.45 * lee };
        // The root: a headwall, then a break in the slope, then an apron of
        // fallen rock running out onto the wall, far longer on the long
        // side than under the cliff.
        let reach = 0.14 + 0.24 * hash(self.seed, 63);
        let brk = 0.2 + 0.2 * hash(self.seed, 64);
        let v = (u / reach).min(1.0);
        let fall = if v < brk {
            1.0 - 0.66 * v / brk
        } else {
            0.34 * (1.0 - (v - brk) / (1.0 - brk)).powf(1.25)
        };
        let root = fall * (1.0 + 0.9 * hash(self.seed, 65)) * (1.0 + 0.8 * lee.max(-0.6));
        // Side ridges: a few crests that run up it from the root, each
        // steep on one side, each ending at its own height.
        let mut ridges = 0.0_f32;
        for i in 0..2 + (hash(self.seed, 66) * 3.0) as u32 {
            let k = 70 + i * 6;
            let turn = hash(self.seed, k) * TAU + (hash(self.seed, k + 1) - 0.5) * 2.5 * u;
            let top = 0.25 + 0.6 * hash(self.seed, k + 2);
            if u >= top {
                continue;
            }
            let off =
                ((theta - turn + PI).rem_euclid(TAU) - PI) * if i % 2 == 0 { 1.0 } else { -1.0 };
            let half = if off > 0.0 { 0.22 } else { 0.7 } * (0.6 + 0.8 * hash(self.seed, k + 3));
            let d = (off.abs() / half).min(1.0);
            let across = if d < 0.3 {
                1.0 - 0.66 * d / 0.3
            } else {
                0.34 * (1.0 - (d - 0.3) / 0.7).powf(1.25)
            };
            let crest = (1.0 - u / top).powf(0.7 + hash(self.seed, k + 4));
            ridges = ridges.max(across * crest * (0.5 + 0.9 * hash(self.seed, k + 5)));
        }
        // Lobes on top of that, most at the root.
        let lobes = noise.fbm3(
            Vec3::new(theta.cos(), theta.sin(), u * 2.2) * 1.2 + key,
            3,
            2.0,
            0.5,
        );
        let section = flank * (1.0 + (0.25 + 0.3 * fall) * lobes);
        let girth = self.girth(noise, u) + self.radius * (root + ridges);
        // Broken rock on it, in some stretches hardly any, in others a lot.
        let on = ring * self.radius + Vec3::new(0.0, 0.0, u * self.length) + key * 40.0;
        let busy = smooth(
            -0.25,
            0.35,
            noise.fbm3(on / (self.radius * 6.0) + key, 2, 2.0, 0.5),
        );
        let rough = rock::shared().broken(on, self.radius / 50.0) * (0.2 + 1.3 * busy);
        let shrink = (girth / self.radius).clamp(0.0, 1.0);
        at + ring * (girth * section + rough * (0.25 + 0.75 * shrink) * shrink.sqrt()).max(0.0)
    }

    /// The nearest the spine comes to `p`, less its girth there.
    fn clearance(&self, noise: &Noise, p: Vec3) -> f32 {
        (0..=SPINE)
            .map(|i| {
                let u = i as f32 / SPINE as f32;
                self.spine[i].0.distance(p) - self.girth(noise, u) - self.radius * 0.3
            })
            .fold(f32::MAX, f32::min)
    }
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
                1.0 - smooth(860.0, 1060.0, beside),
            )
        } else {
            (0.0, 0.0)
        };
        // Vaults and bays kilometres wide, buttresses, ribs, ledges: all of
        // it into the hollow or back from it.
        // Eroded relief: ranges and basins whose slopes differ everywhere.
        let vaults =
            rock::Rock::eroded(rock::Rock::drained(on / 4600.0 + Vec3::splat(21.3), 0.3), 8)
                * 460.0;
        // On them the rock is broken as rock breaks, at the size of the
        // wall: benches and cliffs hundreds of metres high, great blocks.
        let broken = rock::shared().broken(on, 12.0) * 1.2;
        // Plains: away from the great veins, where no spire grew, the wall
        // lies wide and level in its vein's pull. Settled ground: this is
        // where water stands and where people can build.
        let vein = self
            .noise
            .fbm3(dir * 2.1 + Vec3::splat(31.0), 3, 2.0, 0.5)
            .abs()
            * 4.0;
        let patch = self.fbm(on / 2400.0 + Vec3::splat(77.0), 2);
        let level = smooth(0.9, 1.5, vein) * smooth(0.35, 0.55, patch);
        let inward = (vaults * (1.0 - 0.75 * level) + broken * (1.0 - 0.94 * level)) * (1.0 - calm);
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
                        let rock = move |_: f32, _: f32, p: Vec3| (p - center).normalize();
                        // Only patches the test space can come near get the
                        // fine level.
                        let near = origin.length() < 5200.0;
                        let mut steps = vec![(40, 4500.0, 70.0), (14, EVERYWHERE, 120.0)];
                        if near {
                            steps.insert(0, (128, 1900.0, 25.0));
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
                            near: false,
                        })
                    }));
                }
            }
        }
        jobs
    }

    /// The spires of the cavern. They stand where the wall's great veins
    /// run: in ridges and clusters, a few giants among many small ones,
    /// with wide bays of bare wall between; some fork.
    fn spires(&self) -> Vec<Spire> {
        const TRIES: u32 = 1100;
        struct Seedling {
            dir: Vec3,
            length: f32,
            id: u32,
        }
        let mut seedlings = Vec::new();
        for i in 0..TRIES {
            let y = hash(i, 1) * 2.0 - 1.0;
            let r = (1.0 - y * y).max(0.0).sqrt();
            let turn = hash(i, 2) * TAU;
            let dir = Vec3::new(r * turn.cos(), y, r * turn.sin());
            // The great veins: lines over the wall where a slow noise
            // crosses zero. Spires crowd along them, the biggest on them.
            let n = self.noise.fbm3(dir * 2.1 + Vec3::splat(31.0), 3, 2.0, 0.5);
            let vein = 1.0 - (n.abs() * 4.0).min(1.0);
            if vein < 0.25 + 0.5 * hash(i, 3) {
                continue;
            }
            let size = hash(i, 7);
            let length = (380.0 + 3000.0 * size.powf(2.6)) * (0.55 + 0.6 * vein);
            seedlings.push(Seedling { dir, length, id: i });
        }
        // The big ones take their ground first; a smaller one may stand
        // close beside them, but not inside their roots.
        seedlings.sort_by(|a, b| b.length.total_cmp(&a.length));
        let home_axis = |p: Vec3| {
            let z = p.z.clamp(WALL_Z, 330.0);
            p.distance(Vec3::new(0.0, 0.0, z))
        };
        let mut spires: Vec<Spire> = Vec::new();
        let mut feet: Vec<(Vec3, f32)> = Vec::new();
        let plant = |spires: &mut Vec<Spire>, spire: Spire| -> bool {
            let ok = (0..=SPINE).all(|i| {
                let u = i as f32 / SPINE as f32;
                let (p, girth) = (spire.spine[i].0, spire.girth(&self.noise, u) + spire.radius);
                p.distance(self.center) > CLEAR_OF_HEART
                    && home_axis(p) > 230.0 + girth * 1.4
                    && p.distance(ISLAND_CENTER) > 120.0 + girth * 1.4
            });
            if ok {
                spires.push(spire);
            }
            ok
        };
        for seedling in seedlings {
            let i = seedling.id;
            let foot = self.wall_point(seedling.dir);
            let room = foot.distance(self.center) - CLEAR_OF_HEART;
            let length = seedling.length.min(room * 0.9);
            let radius = length * (0.06 + 0.07 * hash(i, 8));
            if feet
                .iter()
                .any(|&(f, r)| f.distance(foot) < (r + radius) * 1.7)
            {
                continue;
            }
            // Spires set out inward, each leaning its own way.
            let lean = Vec3::new(hash(i, 4) - 0.5, hash(i, 5) - 0.5, hash(i, 6) - 0.5) * 0.8;
            let axis = ((self.center - foot).normalize() + lean).normalize();
            let root = foot - axis * (radius * 1.2);
            let spire = Spire::grow(
                &self.noise,
                root,
                axis,
                length + radius * 1.2,
                radius,
                self.center,
                500 + i,
            );
            let parent = spire.clone();
            if !plant(&mut spires, spire) {
                continue;
            }
            feet.push((foot, radius));
            // Forks: a branch leaves the trunk part way up.
            let forks = match hash(i, 61) {
                h if h < 0.62 => 0,
                h if h < 0.9 => 1,
                _ => 2,
            };
            for k in 0..forks {
                let id = 40_000 + i * 4 + k;
                let u = 0.22 + 0.45 * hash(id, 62);
                let (at, side, up, along) = parent.at(u * parent.end());
                let turn = hash(id, 63) * TAU;
                let out = side * turn.cos() + up * turn.sin();
                let axis = (along + out * (0.45 + 0.6 * hash(id, 64))).normalize();
                let girth = parent.girth(&self.noise, u * parent.end());
                let radius = girth * (0.55 + 0.3 * hash(id, 65));
                let length = parent.length * (1.0 - u) * (0.5 + 0.6 * hash(id, 66));
                if radius < 12.0 || length < radius * 4.0 {
                    continue;
                }
                let branch = Spire::grow(
                    &self.noise,
                    at - axis * radius * 0.4,
                    axis,
                    length,
                    radius,
                    self.center,
                    id,
                );
                plant(&mut spires, branch);
            }
        }
        spires
    }

    fn spire_job(&self, spire: Spire) -> Job {
        let noise = self.noise.clone();
        Box::new(move || {
            let point = |s: f32, t: f32| spire.point(&noise, s, t);
            let origin = spire.at(0.5 * spire.end()).0;
            let rock = |s: f32, t: f32, _: Vec3| {
                let (_, side, up, _) = spire.at(s.clamp(0.0, 1.0) * spire.end());
                let theta = t * TAU;
                -(side * theta.cos() + up * theta.sin())
            };
            // Fine only where the test space can come near; the nearest
            // are also lit and textured as the near rock is.
            let reach = spire.clearance(&noise, Vec3::ZERO);
            let near = reach < 1500.0;
            let mut steps = vec![(64, 40, 5000.0), (22, 14, EVERYWHERE)];
            if reach < 5200.0 {
                steps.insert(0, (200, 120, 2800.0));
            }
            if near {
                steps.insert(0, (420, 220, 1300.0));
            }
            let mut levels = Vec::new();
            for (rows, cols, to) in steps {
                let mesh = Surface {
                    rows,
                    cols,
                    origin,
                    point: &point,
                    rock: &rock,
                    uv: &|s, _| if near { [0.0, 0.0] } else { [s, 1.0] },
                    skirt: 0.0,
                }
                .build()?;
                levels.push((mesh, to));
            }
            Ok(Piece {
                levels,
                places: vec![Place::new(origin.x, origin.y, origin.z)],
                near,
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
                && spires
                    .iter()
                    .all(|s| s.clearance(&self.noise, p) > s.radius * 0.5 + size * 1.5)
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
                    let rock = |_: f32, _: f32, p: Vec3| -p.normalize_or_zero();
                    let mut levels = Vec::new();
                    for (rows, cols, to) in
                        [(64, 128, 1500.0), (22, 44, 3800.0), (9, 18, EVERYWHERE)]
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
                    Ok(Piece {
                        levels,
                        places,
                        near: false,
                    })
                })
            })
            .collect()
    }

    /// Build the whole cavern on worker threads.
    pub fn build(&self) -> EngineResult<Vec<Piece>> {
        let spires = self.spires();
        let mut jobs = self.wall_jobs();
        jobs.extend(self.island_jobs(&spires));
        jobs.extend(spires.into_iter().map(|s| self.spire_job(s)));
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
    fn spires_keep_clear_of_the_test_space_and_the_heart() {
        let cavern = Cavern::new();
        let spires = cavern.spires();
        println!("{} spires", spires.len());
        assert!(spires.len() > 150, "{}", spires.len());
        for s in &spires {
            for (p, _, _) in &s.spine {
                assert!(p.distance(cavern.center) > CLEAR_OF_HEART - 1.0);
                let z = p.z.clamp(WALL_Z, 330.0);
                assert!(p.distance(Vec3::new(0.0, 0.0, z)) > 230.0, "{p}");
            }
        }
    }
}
