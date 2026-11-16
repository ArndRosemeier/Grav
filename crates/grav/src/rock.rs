//! The dead stone as real rock: the spire, the outer wall, the island and
//! the far spires of the cavern.
//!
//! Every shape here is one function of position (`Rock`), and both the drawn
//! mesh and the solid body come from it, so what is drawn is what collides.
//!
//! The veins come first. Rock that has had ages to settle under their pull
//! lies where the veins' summed potential is equal (`Field::vein_potential`),
//! so on it down is square to the ground everywhere: that settled surface is
//! where the paths and towns are. Around one vein it is the plain cylinder
//! or plane; where the spire's vein meets the wall's it is a wide flare, the
//! natural road from one district into the next. Off the paths the rock is
//! wild: folded strata, flutes, ledges, a tapering tip (design doc "Look and
//! style", L7 to L10).
//!
//! Meshes are built on worker threads and handed over ready to spawn.

use crate::flora::Growth;
use crate::space::{self, ISLAND_CENTER, ISLAND_RADIUS, SPIRE_END, SPIRE_RADIUS, WALL_Z};
use crate::wet::{Ground, Water};
use engine::prelude::*;
use engine::proc::Noise;
use std::f32::consts::TAU;
use std::sync::Arc;

/// Where the spire's rock ends in a point, past `SPIRE_END`.
pub const SPIRE_TIP: f32 = SPIRE_END + 62.0;
/// Half the side of the outer wall that is built around the spire.
pub const WALL_HALF: f32 = 900.0;

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Where the spire's rows stop running along the axis and start following
/// the flare into the wall. The flare begins a little below this.
const FLARE_TOP: f32 = -196.0;
/// The settled surface is tabled from the wall up to here along the spire.
const TABLE_TOP: f32 = -150.0;
const TABLE_STEP: f32 = 0.25;
/// The flare's skirt lies on the wall out to this far from the axis, then
/// sinks under the wall's own rock.
const SKIRT_FLAT: f32 = 150.0;
const SKIRT_END: f32 = 165.0;
/// The town's spring: where on the spire (degrees round from the top, z
/// along it), and the size of the basin it has worn into the rock.
pub const SPRING_AT: (f32, f32) = (16.0, -78.0);
pub const SPRING_BASIN_RADIUS: f32 = 6.5;
const SPRING_RADIUS_AT: f32 = SPIRE_RADIUS;
const SPRING_BASIN_DEPTH: f32 = 1.7;
/// What the spring gives, in the water map's unit (square metres of shaded
/// rock): enough to keep its basin full and a brook running from it.
const SPRING_FLOW: f32 = 3200.0;

/// The ring road around the flare's foot, this far from the axis.
pub const RING_ROAD: f32 = 137.0;

fn linear(a: f32, b: f32, x: f32) -> f32 {
    ((x - a) / (b - a)).clamp(0.0, 1.0)
}

/// The settled surface where the spire's vein meets the wall's: found once
/// from the field, then read from tables.
struct Settled {
    /// The surface's distance from the axis at heights from `WALL_Z` up to
    /// `TABLE_TOP`, every `TABLE_STEP`.
    radius: Vec<f32>,
    /// The flare's outline from `FLARE_TOP` down and out onto the wall, as
    /// (distance from the axis, z), one point every `SPIRE_STEP` along it.
    outline: Vec<(f32, f32)>,
    /// The way out of the rock at each outline point, as (outward, z).
    normals: Vec<(f32, f32)>,
}

impl Settled {
    fn new() -> Self {
        let field = space::base_field();
        // The spire is round and the wall flat here, so one slice shows it
        // all; taken on the side away from the island.
        let phi = |rho: f32, z: f32| field.vein_potential(Vec3::new(0.0, -rho, z));
        // The town's ground is the level everything else is measured by.
        let level = phi(SPIRE_RADIUS, 0.0);
        let radius_at = |z: f32| {
            let (mut lo, mut hi) = (SPIRE_RADIUS - 15.0, 400.0);
            for _ in 0..40 {
                let mid = 0.5 * (lo + hi);
                if phi(mid, z) < level {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            lo
        };
        let height_at = |rho: f32| {
            let (mut lo, mut hi) = (WALL_Z - 20.0, FLARE_TOP);
            for _ in 0..40 {
                let mid = 0.5 * (lo + hi);
                if phi(rho, mid) < level {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            lo
        };
        let count = ((TABLE_TOP - WALL_Z) / TABLE_STEP).round() as usize + 1;
        let radius = (0..count)
            .map(|i| radius_at(WALL_Z + i as f32 * TABLE_STEP))
            .collect();
        // The outline: down the spire while it is steep, then out along the
        // wall, so neither run ever has to follow a slope it cannot.
        let mut line = Vec::new();
        let mut z = FLARE_TOP;
        let turn = loop {
            let rho = radius_at(z);
            if rho > SPIRE_RADIUS + 10.0 {
                break rho;
            }
            line.push((rho, z));
            z -= 0.1;
        };
        let mut rho = turn;
        while rho <= SKIRT_END {
            let sink = 3.0 * linear(SKIRT_FLAT, SKIRT_END, rho);
            line.push((rho, height_at(rho) - sink));
            rho += 0.1;
        }
        // One point every SPIRE_STEP along the line.
        let mut outline = vec![line[0]];
        let mut walked = 0.0;
        let mut next = SPIRE_STEP;
        for pair in line.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let len = (b.0 - a.0).hypot(b.1 - a.1);
            while walked + len >= next {
                let t = (next - walked) / len;
                outline.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
                next += SPIRE_STEP;
            }
            walked += len;
        }
        let last = outline.len() - 1;
        let normals = (0..=last)
            .map(|i| {
                let (a, b) = (outline[i.saturating_sub(1)], outline[(i + 1).min(last)]);
                // The line runs outward and down: its left side is out.
                let (dr, dz) = (b.0 - a.0, b.1 - a.1);
                let len = dr.hypot(dz).max(1.0e-6);
                (-dz / len, dr / len)
            })
            .collect();
        Self {
            radius,
            outline,
            normals,
        }
    }

    /// The settled surface's distance from the axis at `z`.
    fn radius(&self, z: f32) -> f32 {
        if z >= TABLE_TOP {
            return SPIRE_RADIUS;
        }
        let f = ((z - WALL_Z) / TABLE_STEP).max(0.0);
        let i = (f as usize).min(self.radius.len() - 2);
        let t = (f - i as f32).min(1.0);
        self.radius[i] + (self.radius[i + 1] - self.radius[i]) * t
    }
}

/// The rock's shape functions. Cheap to copy around threads.
#[derive(Clone)]
pub struct Rock {
    noise: Noise,
    settled: Arc<Settled>,
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
            settled: Arc::new(Settled::new()),
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

    /// Cells of broken rock at `p` (in cell units): the nearest cell's own
    /// number in -1..1, and how far `p` is from the cell's edge.
    fn cells(p: Vec3) -> (f32, f32) {
        fn hash(x: i32, y: i32, z: i32, k: u32) -> f32 {
            let mut h = (x as u32).wrapping_mul(0x8da6_b343)
                ^ (y as u32).wrapping_mul(0xd816_3841)
                ^ (z as u32).wrapping_mul(0xcb1a_b31f)
                ^ k.wrapping_mul(0x9e37_79b9);
            h ^= h >> 15;
            h = h.wrapping_mul(0x2c1b_3c6d);
            h ^= h >> 12;
            h = h.wrapping_mul(0x297a_2d39);
            h ^= h >> 15;
            (h & 0xffff) as f32 / 65535.0
        }
        let base = p.floor();
        let (bx, by, bz) = (base.x as i32, base.y as i32, base.z as i32);
        let (mut d1, mut d2, mut own) = (f32::MAX, f32::MAX, 0.0);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let (x, y, z) = (bx + dx, by + dy, bz + dz);
                    let at = Vec3::new(
                        x as f32 + hash(x, y, z, 1),
                        y as f32 + hash(x, y, z, 2),
                        z as f32 + hash(x, y, z, 3),
                    );
                    let d = at.distance(p);
                    if d < d1 {
                        d2 = d1;
                        d1 = d;
                        own = hash(x, y, z, 4) * 2.0 - 1.0;
                    } else if d < d2 {
                        d2 = d;
                    }
                }
            }
        }
        (own, d2 - d1)
    }

    /// Settled rock is still rock: how far the ground at `p` stands out
    /// from the settled surface. `rough` runs from 0 on a road (cracked
    /// slabs a foot can cross, low swells) to 1 beside it (tilted blocks,
    /// boulders, outcrops); `broken` adds the ledges and stacks of ground
    /// nobody keeps clear.
    fn relief(&self, p: Vec3, rough: f32, broken: f32) -> f32 {
        let (slab, edge) = Self::cells(p / 8.0);
        let (block, block_edge) = Self::cells(p / 21.0 + Vec3::splat(9.3));
        let slabs = slab * (0.1 + 0.75 * rough);
        let cracks = -(1.0 - smooth(0.0, 0.1, edge)) * (0.08 + 0.5 * rough);
        let blocks = (block * 1.6 - (1.0 - smooth(0.0, 0.07, block_edge)) * 1.2) * rough;
        let swells = (self.fbm(p * 0.05 + Vec3::splat(3.1), 3) - 0.5) * (0.5 + 2.6 * rough);
        let outcrops = smooth(0.5, 0.8, self.ridged(p * 0.07 + Vec3::splat(61.0), 3)) * 4.5 * rough;
        let mut out = slabs + cracks + blocks + swells + outcrops;
        if broken > 0.0 {
            let stacks = (self.ridged(p * 0.028 + Vec3::splat(27.0), 4) - 0.3) * 13.0;
            let s = self.fbm(p * 0.02 + Vec3::splat(71.0), 3) * 9.0;
            let f = s - s.floor();
            let ledges = (smooth(0.0, 0.8, f) - smooth(0.8, 1.0, f)) * 3.0;
            out += (stacks + ledges) * broken;
        }
        out
    }

    /// Wild, broken rock at `p`: how far it stands out, in metres, at
    /// `scale` 1 (about 20 m either way; larger scales make every feature
    /// that much larger, for the far scenery).
    pub fn broken(&self, p: Vec3, scale: f32) -> f32 {
        let p = p / scale;
        // The mass of it is eroded relief: its detail fades where it is
        // already steep, so slopes, and how fast slopes change, differ
        // from place to place.
        let mass = Self::eroded(Self::drained(p * 0.0045, 0.35), 7) * 9.0;
        // Quiet stretches and shattered ones: the same rock is not broken
        // equally everywhere.
        let busy = smooth(
            -0.3,
            0.35,
            self.noise.fbm3(p * 0.0052 + Vec3::splat(88.0), 2, 2.0, 0.5),
        );
        // Cut into benches: flat treads, steep risers, each bench its own
        // height so no two cliffs are alike.
        let lay = self.fbm(p * 0.014, 4);
        let steps = lay * 8.0 + self.fbm(p * 0.05 + Vec3::splat(5.0), 2) * 0.8;
        let tread = steps.floor();
        let riser = smooth(0.72, 0.98, steps - tread);
        let benches = ((tread + riser) / 8.0 - 0.5) * 30.0 * (0.12 + 0.88 * busy);
        // Joints: blocks at three sizes, each standing a little proud or
        // set back, with the joint between them cut in.
        let (big, big_edge) = Self::cells(p / 23.0 + Vec3::splat(3.3));
        let (mid, mid_edge) = Self::cells(p / 9.0 + Vec3::splat(17.1));
        let (small, small_edge) = Self::cells(p / 3.6 + Vec3::splat(41.7));
        let blocks = (big * 4.2 + mid * 1.7 - (1.0 - smooth(0.0, 0.06, big_edge)) * 2.2)
            * (0.1 + 0.9 * busy)
            + (small * 0.55
                - (1.0 - smooth(0.0, 0.08, mid_edge)) * 0.9
                - (1.0 - smooth(0.0, 0.1, small_edge)) * 0.3)
                * (0.4 + 0.6 * busy);
        // Crags where the rock is hardest.
        let crags = smooth(0.45, 0.85, self.ridged(p * 0.03 + Vec3::splat(40.0), 4)) * 9.0 * busy;
        (mass + benches + blocks + crags + Self::gouges(p)) * scale
    }

    /// A number in 0..1 for the lattice point `x`, `y`, `z` and key `k`.
    fn lattice(x: i32, y: i32, z: i32, k: u32) -> f32 {
        let mut h = (x as u32).wrapping_mul(0x8da6_b343)
            ^ (y as u32).wrapping_mul(0xd816_3841)
            ^ (z as u32).wrapping_mul(0xcb1a_b31f)
            ^ k.wrapping_mul(0x9e37_79b9);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2c1b_3c6d);
        h ^= h >> 12;
        h = h.wrapping_mul(0x297a_2d39);
        h ^= h >> 15;
        (h & 0xffff) as f32 / 65535.0
    }

    /// Value noise in -1..1 at `p` with its slope (Quilez, "more noise").
    fn value_and_slope(p: Vec3) -> (f32, Vec3) {
        let base = p.floor();
        let w = p - base;
        let u = w * w * w * (w * (w * 6.0 - 15.0) + 10.0);
        let du = 30.0 * w * w * (w * (w - 2.0) + 1.0);
        let (x, y, z) = (base.x as i32, base.y as i32, base.z as i32);
        let at = |dx: i32, dy: i32, dz: i32| Self::lattice(x + dx, y + dy, z + dz, 77);
        let (a, b, c, d) = (at(0, 0, 0), at(1, 0, 0), at(0, 1, 0), at(1, 1, 0));
        let (e, f, g, h) = (at(0, 0, 1), at(1, 0, 1), at(0, 1, 1), at(1, 1, 1));
        let (k0, k1, k2, k3) = (a, b - a, c - a, e - a);
        let (k4, k5, k6) = (a - b - c + d, a - c - e + g, a - b - e + f);
        let k7 = -a + b + c - d + e - f - g + h;
        let value = k0
            + k1 * u.x
            + k2 * u.y
            + k3 * u.z
            + k4 * u.x * u.y
            + k5 * u.y * u.z
            + k6 * u.z * u.x
            + k7 * u.x * u.y * u.z;
        let slope = Vec3::new(
            k1 + k4 * u.y + k6 * u.z + k7 * u.y * u.z,
            k2 + k5 * u.z + k4 * u.x + k7 * u.z * u.x,
            k3 + k6 * u.x + k5 * u.y + k7 * u.x * u.y,
        );
        (-1.0 + 2.0 * value, 2.0 * du * slope)
    }

    /// Eroded relief at `p`, about -2..2 (the mountain formula of
    /// OrrunWithEngine, after Quilez, in three dimensions): each finer
    /// layer is weakened where the layers before it are already steep.
    /// Steep faces stay clean, crests and floors carry the detail, and no
    /// two slopes are alike.
    pub fn eroded(p: Vec3, octaves: u32) -> f32 {
        let (mut sum, mut amp, mut steep, mut p) = (0.0, 1.0, Vec3::ZERO, p);
        for _ in 0..octaves {
            let (n, slope) = Self::value_and_slope(p);
            steep += slope;
            sum += amp * n / (1.0 + steep.length_squared());
            amp *= 0.5;
            // Turn and double, so the layers do not line up.
            p = Vec3::new(
                0.8 * p.y + 0.6 * p.z,
                -0.8 * p.x + 0.36 * p.y - 0.48 * p.z,
                -0.6 * p.x - 0.48 * p.y + 0.64 * p.z,
            ) * 2.0;
        }
        sum
    }

    /// `p` pushed along the slope of a slow second field, so that crests
    /// bunch up and drain instead of repeating at one spacing.
    pub fn drained(p: Vec3, amount: f32) -> Vec3 {
        let (_, a) = Self::value_and_slope(p * 0.9 + Vec3::splat(13.7));
        let q = p + a * amount;
        let (_, b) = Self::value_and_slope(q * 2.3 + Vec3::splat(57.1));
        q + b * amount * 0.38
    }

    /// Gouges: here and there a single ravine cut into the rock, of its
    /// own length, width and depth, with a raised shoulder on either
    /// side. Each is placed on its own; none repeats.
    fn gouges(p: Vec3) -> f32 {
        const CELL: f32 = 95.0;
        let q = p / CELL;
        let base = q.floor();
        let (bx, by, bz) = (base.x as i32, base.y as i32, base.z as i32);
        let mut out = 0.0_f32;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let (x, y, z) = (bx + dx, by + dy, bz + dz);
                    let r = |k: u32| Self::lattice(x, y, z, 200 + k);
                    if r(0) > 0.3 {
                        continue;
                    }
                    let at = Vec3::new(x as f32 + r(1), y as f32 + r(2), z as f32 + r(3)) * CELL;
                    let along = Vec3::new(r(4) - 0.5, r(5) - 0.5, r(6) - 0.5).normalize_or(Vec3::X);
                    let half_length = 28.0 + 50.0 * r(7);
                    let half_width = 3.0 + 7.0 * r(8) * r(8);
                    let depth = 2.5 + 8.0 * r(9);
                    let rel = p - at;
                    let l = rel.dot(along);
                    if l.abs() >= half_length {
                        continue;
                    }
                    let fade = 1.0 - smooth(0.55, 1.0, l.abs() / half_length);
                    let across = (rel - along * l).length();
                    let cut = (1.0 - (across / half_width).min(1.0)).powf(1.35);
                    let v = ((across - half_width) / (half_width * 1.2)).clamp(0.0, 1.0);
                    let shoulder = (4.0 * v * (1.0 - v)).powi(2);
                    let g = (-depth * cut + depth * 0.42 * shoulder) * fade;
                    if g.abs() > out.abs() {
                        out = g;
                    }
                }
            }
        }
        out
    }

    /// How far (metres along the ground) the point at `theta`, `rho` from
    /// the axis is from the nearest road: the road along the top of the
    /// spire, the two that leave it down the flare towards the dwarven
    /// street, and the ring road around the flare's foot.
    fn road_distance(theta: f32, rho: f32) -> f32 {
        use std::f32::consts::{FRAC_PI_2, PI};
        let turn = |to: f32| ((theta - to).rem_euclid(TAU) + PI).rem_euclid(TAU) - PI;
        let top = turn(0.0).abs() * rho;
        // The side roads start part way down the flare.
        let late = 60.0 * (1.0 - smooth(60.0, 78.0, rho));
        let side = turn(FRAC_PI_2).abs().min(turn(-FRAC_PI_2).abs()) * rho + late;
        let ring = (rho - RING_ROAD).abs();
        top.min(side).min(ring)
    }

    /// Metres from `p` (on the spire, the flare or the wall) to the
    /// nearest road.
    pub fn path_distance(p: Vec3) -> f32 {
        let rho = p.truncate().length();
        let theta = p.x.atan2(p.y);
        if p.z > FLARE_TOP {
            return theta.abs() * rho;
        }
        let lane = if p.x.abs() < 235.0 {
            p.y.abs()
        } else {
            f32::MAX
        };
        Self::road_distance(theta, rho).min(lane)
    }

    /// How much of the town's gentle ground applies at `z` along the spire
    /// (1 in the town, 0 in the wild rock).
    fn spire_town(z: f32) -> f32 {
        smooth(-112.0, -96.0, z) * (1.0 - smooth(42.0, 60.0, z))
    }

    /// The spire's radius at `theta` (radians, 0 = +y, towards +x) and `z`.
    /// Below `FLARE_TOP` it is the settled flare (`flare_point` draws it).
    pub fn spire_radius(&self, theta: f32, z: f32) -> f32 {
        if z < FLARE_TOP {
            return self.settled.radius(z);
        }
        // The plain body: the settled surface around the vein, with a long
        // taper to the tip where the rock has not settled.
        let taper = {
            let t = ((z - 118.0) / (SPIRE_TIP - 118.0)).clamp(0.0, 1.0);
            1.0 - t.powf(1.7)
        };
        let base = self.settled.radius(z) * taper;
        let on = Vec3::new(theta.sin() * SPIRE_RADIUS, theta.cos() * SPIRE_RADIUS, z);
        let town = Self::spire_town(z);
        // Town ground: long, low swells, never below the plain cylinder.
        let swell = self.fbm(on * 0.035 + Vec3::splat(3.1), 3) * 0.5;
        // Wild rock, as rock breaks: great masses cut into benches with
        // cliffs between them, the whole of it jointed into blocks that
        // stand proud or have fallen out, and sharp crags on top.
        let wild = self.broken(on, 1.0);
        let shrink = (0.25 + 0.75 * taper).min(base / 14.0 + 0.05);
        // Outside the town: the road along the top with rocky ground beside
        // it, wild rock further round, and towards the wall the flare's
        // broken ground instead of the wild rock.
        let d = Self::road_distance(theta, SPIRE_RADIUS);
        let flare = 1.0 - smooth(FLARE_TOP + 2.0, FLARE_TOP + 24.0, z);
        let rocky = self.relief(on, smooth(4.0, 15.0, d), smooth(15.0, 45.0, d) * flare);
        let wildness = smooth(15.0, 32.0, d) * (1.0 - flare);
        let outside = (rocky * (1.0 - wildness) + wild * wildness) * shrink;
        // The spring's basin: a round hollow worn into the town's ground.
        let basin = {
            let turn = (theta - SPRING_AT.0.to_radians() + std::f32::consts::PI).rem_euclid(TAU)
                - std::f32::consts::PI;
            let d = (turn * SPRING_RADIUS_AT).hypot(z - SPRING_AT.1) / SPRING_BASIN_RADIUS;
            let bowl = (1.0 - d * d).max(0.0);
            // A lip a hand high around it, where the water has left stone.
            let lip = 0.12 * (1.0 - ((d - 1.05) / 0.2).powi(2)).max(0.0);
            SPRING_BASIN_DEPTH * bowl * bowl - lip
        };
        (base + town * swell + (1.0 - town) * outside - basin).max(0.15)
    }

    /// A point on the spire's surface.
    pub fn spire_point(&self, theta: f32, z: f32) -> Vec3 {
        let r = self.spire_radius(theta, z);
        Vec3::new(theta.sin() * r, theta.cos() * r, z)
    }

    /// A point on the flare where the spire runs into the wall: `row`
    /// counts steps of `SPIRE_STEP` along it from `FLARE_TOP` (row 0)
    /// down and out onto the wall.
    pub fn flare_point(&self, theta: f32, row: usize) -> Vec3 {
        if row == 0 {
            return self.spire_point(theta, FLARE_TOP);
        }
        let row = row.min(self.settled.outline.len() - 1);
        let (rho, z) = self.settled.outline[row];
        let (nr, nz) = self.settled.normals[row];
        let out = self.flare_relief(theta, rho, z);
        let rho = rho + nr * out;
        Vec3::new(theta.sin() * rho, theta.cos() * rho, z + nz * out)
    }

    /// The flare's surface at `theta` around, `rho` from the axis.
    pub fn flare_at(&self, theta: f32, rho: f32) -> Option<Vec3> {
        let row = (1..self.settled.outline.len()).min_by(|&a, &b| {
            (self.settled.outline[a].0 - rho)
                .abs()
                .total_cmp(&(self.settled.outline[b].0 - rho).abs())
        })?;
        ((self.settled.outline[row].0 - rho).abs() < 3.0).then(|| self.flare_point(theta, row))
    }

    /// The rocky ground on the flare at its settled point (`rho`, `z`).
    fn flare_relief(&self, theta: f32, rho: f32, z: f32) -> f32 {
        let on = Vec3::new(theta.sin() * rho, theta.cos() * rho, z);
        let d = Self::road_distance(theta, rho);
        // Where the skirt sinks under the wall it is plain.
        let skirt = 1.0 - linear(SKIRT_FLAT - 4.0, SKIRT_FLAT + 4.0, rho);
        self.relief(on, smooth(4.0, 15.0, d), smooth(15.0, 45.0, d)) * skirt
    }

    /// How many rows the flare has (`flare_point`).
    pub fn flare_rows(&self) -> usize {
        self.settled.outline.len()
    }

    /// Is `p` inside the spire's rock (the flare into the wall included)?
    pub fn in_spire(&self, p: Vec3) -> bool {
        if p.z < WALL_Z || p.z > SPIRE_TIP {
            return false;
        }
        let theta = p.x.atan2(p.y);
        let rho = p.truncate().length();
        if p.z < FLARE_TOP {
            // Under the flare: measure from the nearest point of its outline.
            let line = &self.settled.outline;
            let (mut best, mut near) = (0, f32::MAX);
            for (i, &(r, z)) in line.iter().enumerate() {
                let d = (rho - r).powi(2) + (p.z - z).powi(2);
                if d < near {
                    near = d;
                    best = i;
                }
            }
            let (r, z) = line[best];
            let (nr, nz) = self.settled.normals[best];
            let above = (rho - r) * nr + (p.z - z) * nz;
            return above < self.flare_relief(theta, r, z);
        }
        rho < self.spire_radius(theta, p.z)
    }

    /// How far the outer wall's surface stands out from the plane
    /// `z = WALL_Z` at (x, y), towards +z.
    pub fn wall_height(&self, x: f32, y: f32) -> f32 {
        let on = Vec3::new(x, y, WALL_Z);
        let rho = x.hypot(y);
        // Settled ground: a ring around the spire's flare, and the dwarven
        // street leading away from it. The plane is the settled surface of
        // the wall's vein.
        let ring = 1.0 - smooth(175.0, 215.0, rho);
        let lane = (1.0 - smooth(235.0, 270.0, x.abs())) * (1.0 - smooth(16.0, 34.0, y.abs()));
        let street = ring.max(lane);
        // Settled, but rock: slabs on the dwarven street, rougher beside it.
        let off_street = y.abs() + 40.0 * smooth(235.0, 270.0, x.abs());
        let swell = self.relief(on, smooth(5.0, 16.0, off_street), 0.0);
        // Under the flare's skirt the wall lies low, out of sight.
        let under = -3.0 * (1.0 - linear(146.0, 158.0, rho));
        let far = smooth(140.0, 520.0, rho);
        let mass = (self.fbm(on * 0.004 + Vec3::splat(5.0), 3) - 0.42) * (50.0 + 160.0 * far);
        // Broken rock at the wall's size: benches, cliffs and blocks.
        let wild = mass + self.broken(on, 3.0) * (0.6 + 0.8 * far);
        let height = under + street * swell + (1.0 - street) * wild.max(-8.0);
        // At its edge this wall sinks under the cavern's wall, which takes over.
        let rim = smooth(740.0, 880.0, x.abs().max(y.abs()));
        height + (-(crate::cavern::BEHIND_HOME_WALL + 10.0) - height) * rim
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
    uv: &(dyn Fn(i32, i32) -> [f32; 2] + Sync),
    cover: &(dyn Fn(i32, i32) -> Vec3 + Sync),
    flip: bool,
) -> EngineResult<Mesh> {
    let mut mesh = Mesh::new();
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
            // The colour carries what grows there (flora.rs): black is
            // bare rock.
            let grown = cover(r, c).clamp(Vec3::ZERO, Vec3::ONE);
            mesh.set_point_color(id, Color::rgb01_unchecked(grown.x, grown.y, grown.z))?;
            mesh.set_point_uv(id, uv(r, c))?;
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
    /// Standing and running water on the rock.
    Water,
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

impl Rock {
    /// The spire's mesh rows: row 0 is at FLARE_TOP, rows above run along
    /// the axis, rows below follow the flare down and out onto the wall.
    /// First and last row.
    fn spire_rows(&self) -> (i32, i32) {
        (
            1 - self.flare_rows() as i32,
            ((SPIRE_TIP - FLARE_TOP) / SPIRE_STEP).ceil() as i32,
        )
    }

    /// The spire's mesh point at `row` and column `col` around.
    fn spire_grid_point(&self, row: i32, col: i32) -> Vec3 {
        let theta = col as f32 / SPIRE_AROUND as f32 * TAU;
        if row < 0 {
            return self.flare_point(theta, (-row) as usize);
        }
        let z = (FLARE_TOP + row as f32 * SPIRE_STEP).min(SPIRE_TIP);
        self.spire_point(theta, z)
    }

    /// The wall's mesh point at grid (`row`, `col`), each from
    /// `-WALL_HALF_STEPS` to `WALL_HALF_STEPS`.
    fn wall_grid_point(&self, row: i32, col: i32) -> Vec3 {
        self.wall_point(
            wall_coord(col, WALL_HALF_STEPS),
            wall_coord(row, WALL_HALF_STEPS),
        )
    }

    /// The water on the spire (flare included) and on the wall.
    pub fn waters(&self, heart: Vec3) -> Waters {
        let field = space::base_field();
        let fill = |ground: &mut Ground, i: usize, p: Vec3, out: Vec3| {
            ground.at[i] = p;
            ground.potential[i] = field.vein_potential(p);
            let pull = field.acceleration(&grav_core::BodyQuery::at(p + out * 0.05));
            let g = pull.length() / grav_core::G;
            ground.pull[i] = g;
            ground.up[i] = if g > 0.02 { -pull.normalize() } else { out };
        };
        let (first, last) = self.spire_rows();
        let mut spire = Ground::new((last - first + 1) as usize, SPIRE_AROUND as usize, true);
        for r in first..=last {
            for c in 0..SPIRE_AROUND {
                let i = spire.index((r - first) as usize, c as usize);
                let p = self.spire_grid_point(r, c);
                let out = Vec3::new(p.x, p.y, 0.0).normalize_or_zero();
                fill(&mut spire, i, p, out);
                // Off the skirt's end the water is the wall's.
                spire.drain[i] = r == first;
            }
        }
        let side = (2 * WALL_HALF_STEPS + 1) as usize;
        let mut wall = Ground::new(side, side, false);
        for r in -WALL_HALF_STEPS..=WALL_HALF_STEPS {
            for c in -WALL_HALF_STEPS..=WALL_HALF_STEPS {
                let i = wall.index(
                    (r + WALL_HALF_STEPS) as usize,
                    (c + WALL_HALF_STEPS) as usize,
                );
                let p = self.wall_grid_point(r, c);
                fill(&mut wall, i, p, Vec3::Z);
                // Under the flare the wall is out of sight; at its edge the
                // cavern's wall takes over.
                let edge = r.abs() == WALL_HALF_STEPS || c.abs() == WALL_HALF_STEPS;
                wall.drain[i] = edge || p.truncate().length() < 156.0;
            }
        }
        // The town's spring rises in the middle of its basin.
        let spring_at = self.spire_point(SPRING_AT.0.to_radians(), SPRING_AT.1);
        let nearest = (0..spire.at.len())
            .min_by(|&a, &b| {
                spire.at[a]
                    .distance_squared(spring_at)
                    .total_cmp(&spire.at[b].distance_squared(spring_at))
            })
            .unwrap_or(0);
        spire.spring[nearest] = SPRING_FLOW;
        let mut spire_water = spire.water(heart);
        // The basin's water is the simulated kind (water.rs), not drawn here.
        for i in 0..spire.at.len() {
            if spire.at[i].distance(spring_at) < SPRING_BASIN_RADIUS + 1.6 {
                spire_water.surface[i] = -0.3;
            }
        }
        let wall_water = wall.water(heart);
        Waters {
            spire,
            spire_water,
            wall,
            wall_water,
        }
    }
}

/// The water of this space's rock (`Rock::waters`).
pub struct Waters {
    pub spire: Ground,
    pub spire_water: Water,
    pub wall: Ground,
    pub wall_water: Water,
}

type Job = Box<dyn FnOnce() -> EngineResult<RockPiece> + Send>;

fn spire_chunks(rock: &Rock, waters: &Arc<Waters>, growth: &Arc<Growth>) -> Vec<Job> {
    let (first, rows) = rock.spire_rows();
    let per_chunk = 46;
    let mut jobs: Vec<Job> = Vec::new();
    let mut start = first;
    while start < rows {
        let end = (start + per_chunk).min(rows);
        let rock = rock.clone();
        let waters = waters.clone();
        let growth = growth.clone();
        jobs.push(Box::new(move || {
            let point = |r: i32, c: i32| rock.spire_grid_point(r, c);
            let cover = |r: i32, c: i32| {
                let r = r.clamp(first, rows);
                growth.spire_cover[waters
                    .spire
                    .index((r - first) as usize, c.rem_euclid(SPIRE_AROUND) as usize)]
            };
            let uv = |r: i32, c: i32| {
                let r = r.clamp(first, rows);
                let i = waters
                    .spire
                    .index((r - first) as usize, c.rem_euclid(SPIRE_AROUND) as usize);
                [waters.spire_water.wet[i], waters.spire_water.depth[i]]
            };
            // Rows run along +z, columns around towards +x from +y, so
            // du x dv points out of the rock.
            let mesh = grid_mesh(
                start..end + 1,
                0..SPIRE_AROUND + 1,
                &point,
                &uv,
                &cover,
                false,
            )?;
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

/// The water that shows: a sheet just over the rock wherever it stands or
/// runs, dipping under the rock where it does not.
fn water_jobs(waters: &Arc<Waters>) -> Vec<Job> {
    let mut jobs: Vec<Job> = Vec::new();
    for on_wall in [false, true] {
        let rows = if on_wall {
            waters.wall.rows
        } else {
            waters.spire.rows
        };
        let mut start = 0;
        while start + 1 < rows {
            let end = (start + 64).min(rows - 1);
            let waters = waters.clone();
            jobs.push(Box::new(move || {
                let (ground, water) = if on_wall {
                    (&waters.wall, &waters.wall_water)
                } else {
                    (&waters.spire, &waters.spire_water)
                };
                let cols = ground.cols;
                let last_col = if ground.ring { cols } else { cols - 1 };
                let shows = |i: usize| water.surface[i] >= 0.0;
                let mut mesh = Mesh::new();
                let mut ids = std::collections::HashMap::new();
                let mut vertex = |mesh: &mut Mesh, i: usize| -> EngineResult<PointId> {
                    if let Some(&id) = ids.get(&i) {
                        return Ok(id);
                    }
                    let lift = water.surface[i] + 0.05;
                    let id = mesh.add_point(ground.at[i] + ground.up[i] * lift)?;
                    mesh.set_point_normal(id, ground.up[i])?;
                    mesh.set_point_color(id, rgb(255, 255, 255))?;
                    let flow = (water.flow[i] / crate::wet::STREAM).min(1.0);
                    mesh.set_point_uv(id, [water.depth[i], flow])?;
                    ids.insert(i, id);
                    Ok(id)
                };
                for r in start..end {
                    for c in 0..last_col {
                        let corners = [
                            ground.index(r, c),
                            ground.index(r + 1, c),
                            ground.index(r + 1, c + 1),
                            ground.index(r, c + 1),
                        ];
                        if !corners.iter().any(|&i| shows(i)) {
                            continue;
                        }
                        let [a, b, cc, d] = corners;
                        let (a, b, cc, d) = (
                            vertex(&mut mesh, a)?,
                            vertex(&mut mesh, b)?,
                            vertex(&mut mesh, cc)?,
                            vertex(&mut mesh, d)?,
                        );
                        if on_wall {
                            mesh.add_triangle(a, cc, b)?;
                            mesh.add_triangle(a, d, cc)?;
                        } else {
                            mesh.add_triangle(a, b, cc)?;
                            mesh.add_triangle(a, cc, d)?;
                        }
                    }
                }
                Ok(RockPiece {
                    kind: RockKind::Water,
                    mesh: mesh.prepare(),
                    solid: false,
                })
            }));
            start = end;
        }
    }
    jobs
}

/// Wall grid coordinate: dense at the spire root, coarse far out.
fn wall_coord(i: i32, half_steps: i32) -> f32 {
    let u = i as f32 / half_steps as f32;
    WALL_HALF * (0.3 * u + 0.7 * u * u * u)
}

/// The wall's grid runs this many steps each way from the spire's axis.
const WALL_HALF_STEPS: i32 = 216;

fn wall_chunks(rock: &Rock, waters: &Arc<Waters>, growth: &Arc<Growth>) -> Vec<Job> {
    let half_steps = WALL_HALF_STEPS;
    let per_chunk = 54;
    let mut jobs: Vec<Box<dyn FnOnce() -> EngineResult<RockPiece> + Send>> = Vec::new();
    let mut r0 = -half_steps;
    while r0 < half_steps {
        let r1 = (r0 + per_chunk).min(half_steps);
        let mut c0 = -half_steps;
        while c0 < half_steps {
            let c1 = (c0 + per_chunk).min(half_steps);
            let rock = rock.clone();
            let waters = waters.clone();
            let growth = growth.clone();
            jobs.push(Box::new(move || {
                let point = |r: i32, c: i32| rock.wall_grid_point(r, c);
                let cover = |r: i32, c: i32| {
                    growth.wall_cover[waters.wall.index(
                        (r.clamp(-half_steps, half_steps) + half_steps) as usize,
                        (c.clamp(-half_steps, half_steps) + half_steps) as usize,
                    )]
                };
                let uv = |r: i32, c: i32| {
                    let i = waters.wall.index(
                        (r.clamp(-half_steps, half_steps) + half_steps) as usize,
                        (c.clamp(-half_steps, half_steps) + half_steps) as usize,
                    );
                    [waters.wall_water.wet[i], waters.wall_water.depth[i]]
                };
                let mesh = grid_mesh(r0..r1 + 1, c0..c1 + 1, &point, &uv, &cover, true)?;
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
        let mesh = grid_mesh(
            0..rows + 1,
            0..cols + 1,
            &point,
            &|_, _| [0.0, 0.0],
            &|_, _| Vec3::ZERO,
            false,
        )?;
        Ok(RockPiece {
            kind: RockKind::Island,
            mesh: mesh.prepare(),
            solid: true,
        })
    })
}

/// What `build_all` makes: the rock, what stands on it, the grass.
pub type Grown = (
    Vec<RockPiece>,
    Vec<crate::flora::Stand>,
    Vec<crate::grass::Blade>,
);

/// Build all the rock on worker threads, and grow what grows on it
/// (the rock's mesh carries the mats' colours). Takes a few seconds; the
/// caller spawns the pieces and the plants, and plants the grass.
pub fn build_all(rock: &Rock, heart_dir: Vec3) -> EngineResult<Grown> {
    let waters = Arc::new(rock.waters(heart_dir));
    // People first: nothing grows where they have built.
    let sites = crate::settle::plan(rock, &waters);
    let taken: Vec<(Vec3, f32)> = sites.iter().map(|s| (s.at, s.clear)).collect();
    let growth = Arc::new(crate::flora::grow(&waters, heart_dir, &taken, &|p| {
        Rock::path_distance(p)
    }));
    let mut jobs = spire_chunks(rock, &waters, &growth);
    jobs.extend(wall_chunks(rock, &waters, &growth));
    jobs.extend(water_jobs(&waters));
    jobs.push(island_job(rock));
    let mut stand_jobs = crate::flora::stands(&growth.plants);
    stand_jobs.extend(crate::settle::stands(&sites));
    let stands = on_workers(stand_jobs)?;
    Ok((on_workers(jobs)?, stands, growth.blades.clone()))
}

/// Run jobs on worker threads and give their results in the jobs' order.
pub fn on_workers<T: Send>(
    jobs: Vec<Box<dyn FnOnce() -> EngineResult<T> + Send>>,
) -> EngineResult<Vec<T>> {
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 16);
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
        .map_err(|_| EngineError::InvalidValue("a worker panicked".into()))?;
    if done.len() != total {
        return Err(EngineError::InvalidValue("a worker did not finish".into()));
    }
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, p)| p).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn where_the_water_is() {
        let rock = Rock::new();
        let w = rock.waters(space::HEART_CENTER.normalize());
        for (name, g, wa) in [
            ("spire", &w.spire, &w.spire_water),
            ("wall", &w.wall, &w.wall_water),
        ] {
            let wet = wa.depth.iter().filter(|&&d| d > 0.02).count();
            let stream = wa.wet.iter().filter(|&&x| x > 0.93).count();
            println!(
                "{name}: {} points, {wet} under water, {stream} shown",
                g.at.len()
            );
            let mut deep: Vec<usize> = (0..g.at.len()).collect();
            deep.sort_by(|&a, &b| wa.depth[b].total_cmp(&wa.depth[a]));
            let mut shown: Vec<Vec3> = Vec::new();
            for i in deep {
                if shown.iter().all(|p| p.distance(g.at[i]) > 40.0) {
                    println!(
                        "  depth {:.1} at {:.0} flow {:.0}",
                        wa.depth[i], g.at[i], wa.flow[i]
                    );
                    shown.push(g.at[i]);
                    if shown.len() > 14 {
                        break;
                    }
                }
            }
        }
    }

    fn near_spring(theta: f32, z: f32) -> bool {
        let turn = (theta - SPRING_AT.0.to_radians() + std::f32::consts::PI).rem_euclid(TAU)
            - std::f32::consts::PI;
        (turn * SPIRE_RADIUS).hypot(z - SPRING_AT.1) < SPRING_BASIN_RADIUS * 1.4
    }

    #[test]
    fn the_spring_basin_is_a_hollow_in_the_town() {
        let rock = Rock::new();
        let r = rock.spire_radius(SPRING_AT.0.to_radians(), SPRING_AT.1);
        assert!((SPIRE_RADIUS - 1.8..SPIRE_RADIUS - 1.0).contains(&r), "{r}");
        let w = rock.waters(space::HEART_CENTER.normalize());
        let at = rock.spire_point(SPRING_AT.0.to_radians(), SPRING_AT.1);
        let deepest = (0..w.spire.at.len())
            .filter(|&i| w.spire.at[i].distance(at) < 3.0)
            .map(|i| w.spire_water.depth[i])
            .fold(0.0, f32::max);
        assert!(deepest > 1.0, "the basin holds {deepest} m of water");
    }

    #[test]
    fn the_town_ground_is_gentle_and_never_below_the_plain_cylinder() {
        let rock = Rock::new();
        for zi in -90..=40 {
            for ai in 0..72 {
                let theta = ai as f32 / 72.0 * TAU;
                let r = rock.spire_radius(theta, zi as f32);
                assert!(
                    (SPIRE_RADIUS..SPIRE_RADIUS + 1.2).contains(&r)
                        || near_spring(theta, zi as f32),
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
        for x in [-190.0, -170.0, 170.0, 190.0] {
            let h = rock.wall_height(x, 0.0);
            assert!((-0.7..0.7).contains(&h), "{h}");
        }
    }

    #[test]
    fn the_wall_far_from_the_spire_is_settled_at_its_plane() {
        // The wall's vein lies as deep as it must for the plane z = WALL_Z
        // to be on the same level as the town's ground.
        let field = space::base_field();
        let town = field.vein_potential(Vec3::new(0.0, -SPIRE_RADIUS, 0.0));
        let wall = field.vein_potential(Vec3::new(-300.0, -300.0, WALL_Z));
        assert!((town - wall).abs() < 0.02, "{town} and {wall}");
    }

    #[test]
    fn the_flare_joins_spire_and_wall() {
        let rock = Rock::new();
        let top = rock.flare_point(0.0, 0);
        assert!((top - rock.spire_point(0.0, FLARE_TOP)).length() < 0.02);
        let next = rock.flare_point(0.0, 1);
        assert!(
            (top - next).length() < SPIRE_STEP + 0.6,
            "{top} then {next}"
        );
        let last = rock.flare_point(0.0, rock.flare_rows() - 1);
        assert!(last.y > SKIRT_FLAT && last.z < WALL_Z - 2.0, "{last}");
        // It only ever runs outward and down.
        for pair in rock.settled.outline.windows(2) {
            assert!(pair[1].0 >= pair[0].0 - 1e-3 && pair[1].1 <= pair[0].1 + 1e-3);
        }
    }

    #[test]
    fn down_is_square_to_the_road_from_the_town_to_the_wall() {
        // Walk the road along the top of the spire, down the flare and out
        // onto the wall: the pull never leans more than a few degrees from
        // the ground, far inside what the hero's feet can hold.
        let rock = Rock::new();
        let field = space::base_field();
        let mut road = Vec::new();
        let mut z = -90.0;
        while z > FLARE_TOP {
            road.push(rock.spire_point(0.0, z));
            z -= SPIRE_STEP;
        }
        for row in 0..rock.flare_rows() {
            let p = rock.flare_point(0.0, row);
            if p.y < RING_ROAD {
                road.push(p);
            }
        }
        let mut worst: f32 = 0.0;
        for pair in road.windows(2) {
            let along = (pair[1] - pair[0]).normalize();
            let up = Vec3::X.cross(along).normalize();
            let up = if up.dot(Vec3::new(0.0, 1.0, 1.0)) < 0.0 {
                -up
            } else {
                up
            };
            let at = (pair[0] + pair[1]) * 0.5 + up;
            let pull = field.acceleration(&grav_core::BodyQuery::at(at));
            assert!(pull.length() > 5.0, "no pull at {at}");
            let lean = pull.normalize().dot(along).abs().asin().to_degrees();
            worst = worst.max(lean);
            assert!(lean < 12.0, "the pull leans {lean} degrees at {at}");
        }
        assert!(worst > 0.0);
    }
}
