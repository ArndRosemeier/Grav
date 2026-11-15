//! The dead stone as real rock: the spire, the outer wall, the island and
//! the far spires of the cavern.
//!
//! Every shape here is one function of position (`Rock`), and both the drawn
//! mesh and the solid body come from it, so what is drawn is what collides.
//! The town stands where the functions are gentle (within about a metre of
//! the plain cylinder, plane and sphere the rules were first tested on);
//! everywhere else the rock is rugged: folded strata, flutes, ledges, a
//! tapering tip (design doc "Look and style", L7 to L10).
//!
//! Meshes are built on worker threads and handed over ready to spawn.

use crate::space::{ISLAND_CENTER, ISLAND_RADIUS, SPIRE_END, SPIRE_RADIUS, SPIRE_START, WALL_Z};
use engine::prelude::*;
use engine::proc::Noise;
use std::f32::consts::TAU;

/// Where the spire's rock ends in a point, past `SPIRE_END`.
pub const SPIRE_TIP: f32 = SPIRE_END + 62.0;
/// Half the side of the outer wall that is built around the spire.
pub const WALL_HALF: f32 = 900.0;

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The rock's shape functions. Cheap to copy around threads.
#[derive(Clone)]
pub struct Rock {
    noise: Noise,
}

/// The one rock of this space, shared by the meshes, the frames things
/// stand on and the water's solids.
pub fn shared() -> &'static Rock {
    static ROCK: std::sync::OnceLock<Rock> = std::sync::OnceLock::new();
    ROCK.get_or_init(Rock::new)
}

impl Default for Rock {
    fn default() -> Self {
        Self::new()
    }
}

impl Rock {
    pub fn new() -> Self {
        Self {
            noise: Noise::new(7741),
        }
    }

    fn fbm(&self, p: Vec3, octaves: u32) -> f32 {
        // Perlin sums run in about -1..1: bring them to 0..1.
        (self.noise.fbm3(p, octaves, 2.03, 0.5) * 0.5 + 0.5).clamp(0.0, 1.0)
    }

    /// Ridged noise in 0..1: sharp crests, round hollows.
    fn ridged(&self, p: Vec3, octaves: u32) -> f32 {
        let mut sum = 0.0;
        let mut amp = 0.5;
        let mut freq = 1.0;
        let mut norm = 0.0;
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

    /// How much of the town's gentle ground applies at `z` along the spire
    /// (1 in the town, 0 in the wild rock).
    fn spire_town(z: f32) -> f32 {
        smooth(-112.0, -96.0, z) * (1.0 - smooth(42.0, 60.0, z))
    }

    /// The spire's radius at `theta` (radians, 0 = +y, towards +x) and `z`.
    pub fn spire_radius(&self, theta: f32, z: f32) -> f32 {
        // The plain body: a cylinder with a modest flare into the wall and a
        // long taper to the tip.
        let flare = {
            let t = ((-225.0 - z) / 25.0).clamp(0.0, 1.6);
            12.0 * t * t
        };
        let taper = {
            let t = ((z - 118.0) / (SPIRE_TIP - 118.0)).clamp(0.0, 1.0);
            1.0 - t.powf(1.7)
        };
        let base = SPIRE_RADIUS * taper + flare;
        let on = Vec3::new(theta.sin() * SPIRE_RADIUS, theta.cos() * SPIRE_RADIUS, z);
        let town = Self::spire_town(z);
        // Town ground: long, low swells, never below the plain cylinder.
        let swell = self.fbm(on * 0.035 + Vec3::splat(3.1), 3) * 0.5;
        // Wild rock: big masses, flutes along the axis, ledges of folded strata.
        let mass = (self.fbm(on * 0.016, 4) - 0.45) * 22.0;
        let crags = (self.ridged(on * 0.045 + Vec3::splat(40.0), 4) - 0.35) * 15.0;
        let flute_phase = theta * 11.0 + self.fbm(on * 0.02 + Vec3::splat(8.0), 2) * 5.0;
        let flutes = (flute_phase.sin().abs().powf(0.6) - 0.6) * 3.2;
        let strata = {
            let s = (z + self.fbm(on * 0.012 + Vec3::splat(77.0), 3) * 60.0) / 9.0;
            let f = s - s.floor();
            // A ledge: slow rise, sharp drop.
            (smooth(0.0, 0.78, f) - smooth(0.78, 1.0, f)) * 3.4
        };
        let wild = mass + crags + flutes + strata;
        // The taper keeps the tip a point: relief shrinks with the radius.
        let wild = wild * (0.25 + 0.75 * taper).min(base / 14.0 + 0.05);
        (base + town * swell + (1.0 - town) * wild).max(0.15)
    }

    /// A point on the spire's surface.
    pub fn spire_point(&self, theta: f32, z: f32) -> Vec3 {
        let r = self.spire_radius(theta, z);
        Vec3::new(theta.sin() * r, theta.cos() * r, z)
    }

    /// Is `p` inside the spire's rock?
    pub fn in_spire(&self, p: Vec3) -> bool {
        if p.z < SPIRE_START || p.z > SPIRE_TIP {
            return false;
        }
        let theta = p.x.atan2(p.y);
        p.truncate().length() < self.spire_radius(theta, p.z)
    }

    /// How far the outer wall's surface stands out from the plane
    /// `z = WALL_Z` at (x, y), towards +z.
    pub fn wall_height(&self, x: f32, y: f32) -> f32 {
        let on = Vec3::new(x, y, WALL_Z);
        // The dwarven street beside the spire root stays close to the plane.
        let street = (1.0 - smooth(100.0, 135.0, x.abs())) * (1.0 - smooth(16.0, 34.0, y.abs()));
        let swell = self.fbm(on * 0.03 + Vec3::splat(12.0), 3) * 0.8;
        let far = smooth(140.0, 520.0, x.hypot(y));
        let mass = (self.fbm(on * 0.004 + Vec3::splat(5.0), 3) - 0.42) * (50.0 + 160.0 * far);
        let buttress = (self.ridged(on * 0.011 + Vec3::splat(21.0), 3) - 0.3) * (44.0 + 60.0 * far);
        let strata = {
            let s = (x * 0.35 + y + self.fbm(on * 0.008 + Vec3::splat(55.0), 3) * 120.0) / 14.0;
            let f = s - s.floor();
            (smooth(0.0, 0.85, f) - smooth(0.85, 1.0, f)) * 7.0
        };
        let wild = mass + buttress + strata;
        street * swell + (1.0 - street) * wild.max(-8.0)
    }

    /// A point on the outer wall's surface.
    pub fn wall_point(&self, x: f32, y: f32) -> Vec3 {
        Vec3::new(x, y, WALL_Z + self.wall_height(x, y))
    }

    /// The island's radius in direction `dir` (unit) from its centre.
    pub fn island_radius(&self, dir: Vec3) -> f32 {
        let on = dir * ISLAND_RADIUS;
        // Gentle where the perch stands (up) and where the lake hangs (down).
        let perch = smooth(0.80, 0.93, dir.dot(Vec3::new(0.0, 1.0, 0.2).normalize()));
        let lake = smooth(0.72, 0.90, dir.dot(Vec3::NEG_Y));
        let gentle = perch.max(lake);
        let swell = self.fbm(on * 0.08 + Vec3::splat(31.0), 3) * 0.7;
        let mass = (self.fbm(on * 0.035 + Vec3::splat(90.0), 4) - 0.4) * 12.0;
        let crags = (self.ridged(on * 0.09 + Vec3::splat(14.0), 3) - 0.3) * 6.0;
        ISLAND_RADIUS + gentle * swell + (1.0 - gentle) * (mass + crags).max(-4.0)
    }

    /// A point on the island's surface.
    pub fn island_point(&self, dir: Vec3) -> Vec3 {
        ISLAND_CENTER + dir * self.island_radius(dir)
    }

    /// Is `p` inside the island's rock?
    pub fn in_island(&self, p: Vec3) -> bool {
        let d = p - ISLAND_CENTER;
        let len = d.length();
        len < 1.0e-3 || len < self.island_radius(d / len)
    }
}

/// A grid of surface points as a smooth mesh: `rows` x `cols` points from
/// `point(row, col)`, with normals from neighbouring points given by
/// `point` too (so chunks of one surface meet without a seam). `flip`
/// turns the faces the other way round.
fn grid_mesh(
    rows: std::ops::Range<i32>,
    cols: std::ops::Range<i32>,
    point: &(dyn Fn(i32, i32) -> Vec3 + Sync),
    flip: bool,
) -> EngineResult<Mesh> {
    let mut mesh = Mesh::new();
    let white = rgb(255, 255, 255);
    let (nr, nc) = (rows.len() as i32, cols.len() as i32);
    let mut ids = Vec::with_capacity((nr * nc) as usize);
    for r in rows.clone() {
        for c in cols.clone() {
            let p = point(r, c);
            let du = point(r + 1, c) - point(r - 1, c);
            let dv = point(r, c + 1) - point(r, c - 1);
            let mut n = du.cross(dv);
            if flip {
                n = -n;
            }
            let n = if n.length_squared() > 1.0e-12 {
                n.normalize()
            } else {
                Vec3::Z
            };
            let id = mesh.add_point(p)?;
            mesh.set_point_normal(id, n)?;
            mesh.set_point_color(id, white)?;
            ids.push(id);
        }
    }
    let at = |r: i32, c: i32| ids[(r * nc + c) as usize];
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
    Ok(mesh)
}

/// Which rock a piece belongs to (it decides the material's settings).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RockKind {
    Spire,
    Wall,
    Island,
    FarSpire,
}

/// One finished piece of rock, ready to spawn.
pub struct RockPiece {
    pub kind: RockKind,
    pub mesh: PreparedMesh,
    /// Solid (the hero can reach it) or scenery only.
    pub solid: bool,
}

const SPIRE_STEP: f32 = 1.25;
const SPIRE_AROUND: i32 = 288;

fn spire_chunks(rock: &Rock) -> Vec<Box<dyn FnOnce() -> EngineResult<RockPiece> + Send>> {
    let z0 = SPIRE_START - 2.0;
    let rows = ((SPIRE_TIP - z0) / SPIRE_STEP).ceil() as i32;
    let per_chunk = 46;
    let mut jobs: Vec<Box<dyn FnOnce() -> EngineResult<RockPiece> + Send>> = Vec::new();
    let mut start = 0;
    while start < rows {
        let end = (start + per_chunk).min(rows);
        let rock = rock.clone();
        jobs.push(Box::new(move || {
            let point = move |r: i32, c: i32| {
                let z = (z0 + r as f32 * SPIRE_STEP).min(SPIRE_TIP);
                let theta = c as f32 / SPIRE_AROUND as f32 * TAU;
                rock.spire_point(theta, z)
            };
            // Rows run along +z, columns around towards +x from +y, so
            // du x dv points out of the rock.
            let mesh = grid_mesh(start..end + 1, 0..SPIRE_AROUND + 1, &point, false)?;
            Ok(RockPiece {
                kind: RockKind::Spire,
                mesh: mesh.prepare(),
                solid: true,
            })
        }));
        start = end;
    }
    jobs
}

/// Wall grid coordinate: dense at the spire root, coarse far out.
fn wall_coord(i: i32, half_steps: i32) -> f32 {
    let u = i as f32 / half_steps as f32;
    WALL_HALF * (0.2 * u + 0.8 * u * u * u)
}

fn wall_chunks(rock: &Rock) -> Vec<Box<dyn FnOnce() -> EngineResult<RockPiece> + Send>> {
    let half_steps = 156;
    let per_chunk = 52;
    let mut jobs: Vec<Box<dyn FnOnce() -> EngineResult<RockPiece> + Send>> = Vec::new();
    let mut r0 = -half_steps;
    while r0 < half_steps {
        let r1 = (r0 + per_chunk).min(half_steps);
        let mut c0 = -half_steps;
        while c0 < half_steps {
            let c1 = (c0 + per_chunk).min(half_steps);
            let rock = rock.clone();
            jobs.push(Box::new(move || {
                let point = move |r: i32, c: i32| {
                    rock.wall_point(wall_coord(c, half_steps), wall_coord(r, half_steps))
                };
                let mesh = grid_mesh(r0..r1 + 1, c0..c1 + 1, &point, true)?;
                Ok(RockPiece {
                    kind: RockKind::Wall,
                    mesh: mesh.prepare(),
                    solid: true,
                })
            }));
            c0 = c1;
        }
        r0 = r1;
    }
    jobs
}

fn island_job(rock: &Rock) -> Box<dyn FnOnce() -> EngineResult<RockPiece> + Send> {
    let rock = rock.clone();
    Box::new(move || {
        let (rows, cols) = (96, 192);
        let point = move |r: i32, c: i32| {
            // Latitude from the +y pole to the -y pole, clamped just short of
            // the poles so every ring has a size.
            let lat = (r as f32 / rows as f32).clamp(0.004, 0.996) * std::f32::consts::PI;
            let lon = c as f32 / cols as f32 * TAU;
            let dir = Vec3::new(lat.sin() * lon.sin(), lat.cos(), lat.sin() * lon.cos());
            rock.island_point(dir)
        };
        let mesh = grid_mesh(0..rows + 1, 0..cols + 1, &point, false)?;
        Ok(RockPiece {
            kind: RockKind::Island,
            mesh: mesh.prepare(),
            solid: true,
        })
    })
}

/// A far spire of the cavern: scenery, in world space already.
#[derive(Clone, Copy, Debug)]
pub struct FarSpire {
    /// Where it leaves the wall.
    pub root: Vec3,
    /// The way it points (unit).
    pub axis: Vec3,
    pub length: f32,
    pub radius: f32,
    pub seed: u32,
}

/// The neighbouring spires that fill the view from this one.
pub fn far_spires(heart_dir: Vec3) -> Vec<FarSpire> {
    // (x, y) on the wall, length, radius. Laid out by hand so no two cross
    // and the view from the town street has depth on every side.
    let spots: [(f32, f32, f32, f32); 11] = [
        (-330.0, 180.0, 520.0, 62.0),
        (360.0, 250.0, 640.0, 75.0),
        (290.0, -310.0, 430.0, 55.0),
        (-420.0, -260.0, 700.0, 84.0),
        (-70.0, 520.0, 780.0, 90.0),
        (640.0, -40.0, 560.0, 70.0),
        (-690.0, 60.0, 600.0, 72.0),
        (120.0, -610.0, 520.0, 66.0),
        (-560.0, 560.0, 880.0, 100.0),
        (700.0, 620.0, 820.0, 96.0),
        (620.0, -600.0, 760.0, 88.0),
    ];
    spots
        .iter()
        .enumerate()
        .map(|(i, &(x, y, length, radius))| {
            // Spires lean inward, towards the heart, each a little its own way.
            let lean = Vec3::new(
                ((i * 37 % 11) as f32 / 11.0 - 0.5) * 0.5,
                ((i * 53 % 13) as f32 / 13.0 - 0.5) * 0.5,
                0.0,
            );
            let axis = (Vec3::Z * 0.75 + heart_dir * 0.45 + lean).normalize();
            FarSpire {
                root: Vec3::new(x, y, WALL_Z - 20.0),
                axis,
                length,
                radius,
                seed: 100 + i as u32,
            }
        })
        .collect()
}

fn far_spire_job(spire: FarSpire) -> Box<dyn FnOnce() -> EngineResult<RockPiece> + Send> {
    Box::new(move || {
        let noise = Noise::new(spire.seed);
        let fbm = move |p: Vec3, octaves: u32| noise.fbm3(p, octaves, 2.03, 0.5) * 0.5 + 0.5;
        let (rows, cols) = (150, 96);
        let side = spire.axis.any_orthonormal_vector();
        let up = spire.axis.cross(side);
        let point = move |r: i32, c: i32| {
            let t = (r as f32 / rows as f32).clamp(0.0, 1.0);
            let theta = c as f32 / cols as f32 * TAU;
            let ring = side * theta.cos() + up * theta.sin();
            let on = ring * spire.radius + spire.axis * (t * spire.length);
            let taper = 1.0 - t.powf(1.5);
            let flare = (1.0 - smooth(0.0, 0.12, t)) * 0.6;
            let mass = (fbm(on * 0.012, 4) - 0.45) * 0.55;
            let flutes = ((theta * 9.0 + fbm(on * 0.01, 2) * 6.0)
                .sin()
                .abs()
                .powf(0.6)
                - 0.6)
                * 0.09;
            let s = (t * spire.length + fbm(on * 0.008, 3) * 90.0) / 16.0;
            let f = s - s.floor();
            let strata = (smooth(0.0, 0.82, f) - smooth(0.82, 1.0, f)) * 0.07;
            let r = spire.radius
                * (taper + flare + (mass + flutes + strata) * (0.3 + 0.7 * taper)).max(0.01);
            spire.root + spire.axis * (t * spire.length) + ring * r
        };
        // Rows run along the axis, columns around from `side` to `up`
        // (axis x side): du x dv points inward, so turn it round.
        let mesh = grid_mesh(0..rows + 1, 0..cols + 1, &point, true)?;
        Ok(RockPiece {
            kind: RockKind::FarSpire,
            mesh: mesh.prepare(),
            solid: false,
        })
    })
}

/// Build all the rock on worker threads. Takes a few seconds; the caller
/// spawns the pieces.
pub fn build_all(rock: &Rock, heart_dir: Vec3) -> EngineResult<Vec<RockPiece>> {
    let mut jobs = spire_chunks(rock);
    jobs.extend(wall_chunks(rock));
    jobs.push(island_job(rock));
    for s in far_spires(heart_dir) {
        jobs.push(far_spire_job(s));
    }
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 12);
    let total = jobs.len();
    let queue = std::sync::Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>());
    let done = std::sync::Mutex::new(Vec::with_capacity(total));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let job = queue.lock().ok().and_then(|mut q| q.pop());
                let Some((index, job)) = job else { break };
                let piece = job();
                if let Ok(mut d) = done.lock() {
                    d.push((index, piece));
                }
            });
        }
    });
    let mut done = done
        .into_inner()
        .map_err(|_| EngineError::InvalidValue("a rock worker panicked".into()))?;
    if done.len() != total {
        return Err(EngineError::InvalidValue(
            "a rock worker did not finish".into(),
        ));
    }
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, p)| p).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_town_ground_is_gentle_and_never_below_the_plain_cylinder() {
        let rock = Rock::new();
        for zi in -90..=40 {
            for ai in 0..72 {
                let theta = ai as f32 / 72.0 * TAU;
                let r = rock.spire_radius(theta, zi as f32);
                assert!(
                    (SPIRE_RADIUS..SPIRE_RADIUS + 1.2).contains(&r),
                    "radius {r} at z {zi}"
                );
            }
        }
    }

    #[test]
    fn the_spire_ends_in_a_point() {
        let rock = Rock::new();
        assert!(rock.spire_radius(0.3, SPIRE_TIP) < 0.5);
        assert!(!rock.in_spire(Vec3::new(0.0, 0.0, SPIRE_TIP + 1.0)));
        assert!(rock.in_spire(Vec3::new(0.0, 10.0, 0.0)));
    }

    #[test]
    fn the_island_keeps_its_perch_and_lake_bed() {
        let rock = Rock::new();
        for dir in [Vec3::NEG_Y, Vec3::new(0.0, 1.0, 0.2).normalize()] {
            let r = rock.island_radius(dir);
            assert!((ISLAND_RADIUS..ISLAND_RADIUS + 0.8).contains(&r), "{r}");
        }
    }

    #[test]
    fn the_dwarven_street_on_the_wall_is_gentle() {
        let rock = Rock::new();
        for x in [-90.0, -70.0, 70.0, 90.0] {
            let h = rock.wall_height(x, 0.0);
            assert!((0.0..0.9).contains(&h), "{h}");
        }
    }
}
