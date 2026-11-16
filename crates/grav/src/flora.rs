//! What grows on the rock. Nothing here is scattered by noise: every plant
//! stands where the rules of its kind put it, and the rules are about
//! water, the heart's light and the other plants. That is where the
//! patterns come from.
//!
//! - Mats (drawn by the rock's material): three kinds, each a colony that
//!   spreads from where it took hold, fastest over the ground it likes.
//!   Where two colonies meet neither gives way, and a thin gold crust grows
//!   on the line between them: the rock is a map of territories.
//! - Lantern caps stand in rings that widen with age from where the first
//!   one stood, and leave the ground inside spent. Where two rings meet,
//!   both end.
//! - Spindle trees keep their distance from each other, clear the ground
//!   under them and keep a ring of caps at the edge of the clearing.
//! - Heart fans face the heart and stand on lit rock, never in another
//!   fan's shadow, so they stand in staggered ranks. In the lane of shade
//!   behind each fan a file of pale bells grows, pointing away from the
//!   heart.
//! - Glass reeds line the water. Tufts stand in the mats and take their
//!   colour.

use crate::rock::Waters;
use crate::space;
use crate::wet::{Ground, Water};
use engine::glam::Mat3;
use engine::prelude::*;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::f32::consts::TAU;

pub(crate) fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A number in 0..1 from `n` and a key.
pub(crate) fn hash(n: u32, k: u32) -> f32 {
    let mut h = n.wrapping_mul(0x9e37_79b9) ^ k.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    (h & 0xff_ffff) as f32 / 16_777_215.0
}

/// The kinds of plant that stand on the rock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Reed,
    Cap,
    Fan,
    Bell,
    Tree,
    Tuft,
}

const KINDS: [Kind; 6] = [
    Kind::Reed,
    Kind::Cap,
    Kind::Fan,
    Kind::Bell,
    Kind::Tree,
    Kind::Tuft,
];
const VARIANTS: u32 = 3;

/// One plant: where it stands, how it is turned (+y up, +z to the heart),
/// its size and its colour.
#[derive(Clone, Copy, Debug)]
pub struct Plant {
    pub kind: Kind,
    pub variant: u32,
    pub at: Vec3,
    pub rot: Quat,
    pub size: f32,
    pub tint: Vec3,
}

/// Everything that grows in the test space.
pub struct Growth {
    /// The mats on the spire's and the wall's ground, per grid point:
    /// colour times how thick it lies.
    pub spire_cover: Vec<Vec3>,
    pub wall_cover: Vec<Vec3>,
    pub plants: Vec<Plant>,
    /// The plumb grass (see grass.rs).
    pub blades: Vec<crate::grass::Blade>,
}

/// The mats' colours (the brightest part is 1; the rock's material sets
/// how dark they really are).
const TEAL: Vec3 = Vec3::new(0.06, 1.0, 0.82);
const CRIMSON: Vec3 = Vec3::new(1.0, 0.05, 0.16);
const VIOLET: Vec3 = Vec3::new(0.5, 0.14, 1.0);
const GOLD: Vec3 = Vec3::new(1.0, 0.9, 0.06);

/// What the plants need to know of each point of a ground.
struct Soil<'a> {
    ground: &'a Ground,
    water: &'a Water,
    normal: Vec<Vec3>,
    /// How much of the heart's light falls on it, 0..1.
    light: Vec<f32>,
    /// The size of a grid cell there, in metres.
    cell: Vec<f32>,
    /// Metres to the nearest road.
    path: Vec<f32>,
    heart: Vec3,
}

impl<'a> Soil<'a> {
    fn new(
        ground: &'a Ground,
        water: &'a Water,
        heart: Vec3,
        flip: bool,
        path: &dyn Fn(Vec3) -> f32,
    ) -> Self {
        let n = ground.at.len();
        let mut soil = Self {
            ground,
            water,
            normal: vec![Vec3::Z; n],
            light: vec![0.0; n],
            cell: vec![1.0; n],
            path: vec![0.0; n],
            heart,
        };
        for r in 0..ground.rows {
            for c in 0..ground.cols {
                let i = ground.index(r, c);
                let at = |dr: i32, dc: i32| ground.at[soil.step(r, c, dr, dc).unwrap_or(i)];
                let du = at(1, 0) - at(-1, 0);
                let dv = at(0, 1) - at(0, -1);
                let mut normal = du.cross(dv).normalize_or(ground.up[i]);
                if flip {
                    normal = -normal;
                }
                let p = ground.at[i];
                // The spire stands in the heart's light: behind it the
                // rock is in its shadow.
                let shadow = {
                    let t = (-(p.x * heart.x + p.y * heart.y)
                        / (heart.x * heart.x + heart.y * heart.y).max(1.0e-6))
                    .max(0.0);
                    let q = p + heart * t;
                    let beside = q.truncate().length();
                    if t > 4.0 && q.z > space::WALL_Z && q.z < space::SPIRE_END + 40.0 {
                        1.0 - smooth(36.0, 52.0, beside)
                    } else {
                        0.0
                    }
                };
                soil.normal[i] = normal;
                soil.light[i] = smooth(0.0, 0.7, normal.dot(heart)) * (1.0 - shadow);
                soil.cell[i] = (du.length() * dv.length() * 0.25).sqrt().max(0.2);
                soil.path[i] = path(p);
            }
        }
        soil
    }

    /// The grid point `dr`, `dc` from (`r`, `c`), if there is one.
    fn step(&self, r: usize, c: usize, dr: i32, dc: i32) -> Option<usize> {
        let g = self.ground;
        let r = r as i32 + dr;
        let mut c = c as i32 + dc;
        if g.ring {
            c = c.rem_euclid(g.cols as i32);
        }
        (r >= 0 && r < g.rows as i32 && c >= 0 && c < g.cols as i32)
            .then(|| g.index(r as usize, c as usize))
    }

    fn row_col(&self, i: usize) -> (usize, usize) {
        (i / self.ground.cols, i % self.ground.cols)
    }

    /// The direction to the heart along the ground at `i`.
    fn to_heart(&self, i: usize) -> Vec3 {
        let n = self.ground.up[i];
        (self.heart - n * self.heart.dot(n)).normalize_or(n.any_orthonormal_vector())
    }

    /// Open ground a plant can root in: not under water, not on a road.
    fn open(&self, i: usize, off_path: f32) -> bool {
        self.water.depth[i] < 0.04 && self.path[i] > off_path && !self.ground.drain[i]
    }

    /// How well mat `kind` (0 teal, 1 crimson, 2 violet) does at `i`.
    fn suits(&self, kind: u8, i: usize) -> f32 {
        let (wet, light) = (self.water.wet[i], self.light[i]);
        if self.water.depth[i] > 0.04 || self.ground.drain[i] {
            return 0.0;
        }
        let dry = 1.0 - smooth(0.82, 0.95, wet);
        let off = smooth(2.5, 5.0, self.path[i]);
        let s = match kind {
            0 => smooth(0.08, 0.3, wet) * (1.0 - 0.55 * light),
            1 => smooth(0.2, 0.6, light) * (1.0 - smooth(0.28, 0.5, wet)),
            _ => (1.0 - smooth(0.12, 0.45, light)) * (1.0 - smooth(0.4, 0.7, wet)) * 0.8,
        };
        s * dry * off
    }

    /// A plant's frame at `i`: +y up (leaning to the heart by `lean`),
    /// +z to the heart along the ground, turned by `turn` about up.
    fn frame(&self, i: usize, lean: f32, turn: f32) -> Quat {
        let ahead = self.to_heart(i);
        let up = (self.ground.up[i] + ahead * lean).normalize();
        let z = (ahead - up * ahead.dot(up)).normalize_or(up.any_orthonormal_vector());
        let x = up.cross(z);
        Quat::from_mat3(&Mat3::from_cols(x, up, z)) * Quat::from_rotation_y(turn)
    }

    /// A point of the ground near `i`, off the grid point by part of a cell.
    fn near(&self, i: usize, seed: u32) -> Vec3 {
        let (r, c) = self.row_col(i);
        let (a, b) = (hash(seed, 901) - 0.5, hash(seed, 902) - 0.5);
        let p = self.ground.at[i];
        let along = self
            .step(r, c, if a > 0.0 { 1 } else { -1 }, 0)
            .map_or(p, |j| self.ground.at[j]);
        let across = self
            .step(r, c, 0, if b > 0.0 { 1 } else { -1 })
            .map_or(p, |j| self.ground.at[j]);
        p + (along - p) * a.abs() * 0.9 + (across - p) * b.abs() * 0.9
    }
}

/// Points in space, found again by where they are.
struct Near {
    size: f32,
    cells: HashMap<(i32, i32, i32), Vec<(Vec3, u32)>>,
}

impl Near {
    fn new(size: f32) -> Self {
        Self {
            size,
            cells: HashMap::new(),
        }
    }
    fn key(&self, p: Vec3) -> (i32, i32, i32) {
        let q = (p / self.size).floor();
        (q.x as i32, q.y as i32, q.z as i32)
    }
    fn add(&mut self, p: Vec3, tag: u32) {
        self.cells.entry(self.key(p)).or_default().push((p, tag));
    }
    /// Every stored point within one cell of `p`.
    fn around(&self, p: Vec3) -> impl Iterator<Item = (Vec3, u32)> + '_ {
        let (x, y, z) = self.key(p);
        (-1..=1).flat_map(move |dx| {
            (-1..=1).flat_map(move |dy| {
                (-1..=1).flat_map(move |dz| {
                    self.cells
                        .get(&(x + dx, y + dy, z + dz))
                        .into_iter()
                        .flatten()
                        .copied()
                })
            })
        })
    }
}

#[derive(PartialEq)]
struct Cost(f32);
impl Eq for Cost {}
impl PartialOrd for Cost {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Cost {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// The mats: colonies spreading from where they took hold until they run
/// out of reach or meet another. Gives each point's colour times amount.
fn mats(soil: &Soil, seed: u32) -> Vec<Vec3> {
    let n = soil.ground.at.len();
    let none = u32::MAX;
    let mut owner = vec![none; n];
    let mut reached = vec![f32::MAX; n];
    // Colonies: kind, how far it can spread, its own shade.
    let mut colonies: Vec<(u8, f32, Vec3)> = Vec::new();
    let mut heap = BinaryHeap::new();
    for (i, start) in reached.iter_mut().enumerate() {
        let suit = [soil.suits(0, i), soil.suits(1, i), soil.suits(2, i)];
        let best = suit[0].max(suit[1]).max(suit[2]);
        let key = seed.wrapping_add(i as u32);
        if hash(key, 1) > soil.cell[i] * soil.cell[i] / 190.0 * best {
            continue;
        }
        // Which kind takes hold: by how well each does here.
        let pick = hash(key, 2) * (suit[0] + suit[1] + suit[2]);
        let kind = if pick < suit[0] {
            0
        } else if pick < suit[0] + suit[1] {
            1
        } else {
            2
        };
        let reach = 3.0 + 30.0 * hash(key, 3).powi(2);
        let base = [TEAL, CRIMSON, VIOLET][kind as usize];
        let other = [VIOLET, VIOLET, TEAL][kind as usize];
        let shade = base.lerp(other, 0.3 * hash(key, 4)) * (0.7 + 0.3 * hash(key, 5));
        colonies.push((kind, reach, shade));
        *start = 0.0;
        heap.push((Reverse(Cost(0.0)), i, colonies.len() as u32 - 1));
    }
    while let Some((Reverse(Cost(cost)), i, colony)) = heap.pop() {
        if owner[i] != none || cost > reached[i] {
            continue;
        }
        owner[i] = colony;
        let (kind, reach, _) = colonies[colony as usize];
        let (r, c) = soil.row_col(i);
        for (dr, dc) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            let Some(j) = soil.step(r, c, dr, dc) else {
                continue;
            };
            if owner[j] != none {
                continue;
            }
            let suit = soil.suits(kind, j);
            if suit < 0.04 {
                continue;
            }
            let next = cost + soil.ground.at[i].distance(soil.ground.at[j]) / (0.12 + suit);
            if next < reach && next < reached[j] {
                reached[j] = next;
                heap.push((Reverse(Cost(next)), j, colony));
            }
        }
    }
    let mut cover = vec![Vec3::ZERO; n];
    for (i, grown) in cover.iter_mut().enumerate() {
        if owner[i] == none {
            continue;
        }
        let (kind, reach, shade) = colonies[owner[i] as usize];
        let amount = soil.suits(kind, i).sqrt() * (1.0 - (reached[i] / reach).powi(3));
        // The line where two colonies meet.
        let (r, c) = soil.row_col(i);
        let meets = [(1, 0), (0, 1)].iter().any(|&(dr, dc)| {
            soil.step(r, c, dr, dc)
                .is_some_and(|j| owner[j] != none && owner[j] != owner[i])
        });
        // (Only where the grid is fine enough to draw a line.)
        *grown = if meets && soil.cell[i] < 2.2 && amount > 0.3 {
            GOLD * 0.8
        } else {
            shade * amount.clamp(0.0, 1.0)
        };
    }
    cover
}

/// A ring of lantern caps, or the ring a tree keeps around its clearing.
struct Ring {
    center: usize,
    radius: f32,
    tint: Vec3,
    tree: bool,
}

/// Grow everything on one ground. `far` thins what stands far from the
/// spire's axis (the wall is a kilometre wide).
fn grow_on(soil: &Soil, cover: &mut [Vec3], seed: u32, plants: &mut Vec<Plant>) {
    let g = soil.ground;
    let n = g.at.len();
    let key = |i: usize, k: u32| hash(seed.wrapping_add(i as u32), k);
    // Far out on the wall there is less of everything small.
    let thin = |i: usize| 1.0 - smooth(260.0, 700.0, g.at[i].truncate().length());

    // Spindle trees: by water on ground that is not steep, the eldest
    // first, none within another's reach.
    let mut rings: Vec<Ring> = Vec::new();
    let mut trees = Near::new(30.0);
    let mut order: Vec<usize> = (0..n)
        .filter(|&i| {
            let wet = soil.water.wet[i];
            soil.open(i, 9.0)
                && wet > 0.22
                && wet < 0.8
                && soil.normal[i].dot(g.up[i]) > 0.8
                && key(i, 11) < 0.25
        })
        .collect();
    order.sort_by(|&a, &b| key(a, 12).total_cmp(&key(b, 12)));
    for i in order {
        let p = g.at[i];
        let reach = 13.0 + 13.0 * key(i, 13);
        if trees.around(p).any(|(q, _)| q.distance(p) < reach) {
            continue;
        }
        trees.add(p, i as u32);
        let size = 0.7 + 0.6 * key(i, 14);
        plants.push(Plant {
            kind: Kind::Tree,
            variant: (key(i, 15) * VARIANTS as f32) as u32 % VARIANTS,
            at: p,
            rot: soil.frame(i, 0.12, key(i, 16) * TAU),
            size,
            tint: Vec3::ONE.lerp(Vec3::new(1.0, 0.7, 0.9), key(i, 17)),
        });
        rings.push(Ring {
            center: i,
            radius: 3.6 * size + 1.2,
            tint: Vec3::new(0.75, 1.0, 0.9),
            tree: true,
        });
    }

    // Lantern caps: rings in the shade on damp rock, each as wide as it
    // is old.
    for i in 0..n {
        let wet = soil.water.wet[i];
        if !soil.open(i, 6.0) || soil.light[i] > 0.3 || !(0.06..=0.8).contains(&wet) {
            continue;
        }
        if key(i, 21) > soil.cell[i] * soil.cell[i] / 260.0 * thin(i) {
            continue;
        }
        let tints = [
            Vec3::ONE,
            Vec3::new(0.5, 0.62, 1.0),
            Vec3::new(1.0, 0.5, 0.72),
            Vec3::new(0.62, 1.0, 0.7),
        ];
        rings.push(Ring {
            center: i,
            radius: 1.6 + 10.0 * key(i, 22).powi(2),
            tint: tints[(key(i, 23) * 4.0) as usize % 4],
            tree: false,
        });
    }
    let mut centers = Near::new(26.0);
    for (k, ring) in rings.iter().enumerate() {
        centers.add(g.at[ring.center], k as u32);
    }
    for (k, ring) in rings.iter().enumerate() {
        let (r0, c0) = soil.row_col(ring.center);
        let center = g.at[ring.center];
        let reach = ((ring.radius + 1.5) / soil.cell[ring.center]).ceil() as i32 + 1;
        // The points on the ring, in order around it.
        let mut on: Vec<(f32, usize)> = Vec::new();
        let ahead = soil.to_heart(ring.center);
        let side = g.up[ring.center].cross(ahead);
        for dr in -reach..=reach {
            for dc in -reach..=reach {
                let Some(j) = soil.step(r0, c0, dr, dc) else {
                    continue;
                };
                let v = g.at[j] - center;
                let d = v.length();
                // Inside, the ground is spent (under a tree: kept clear).
                if d < ring.radius - 0.5 {
                    cover[j] *= 0.12 + 0.5 * smooth(0.0, 1.0, d / ring.radius).powi(3);
                } else if d < ring.radius + 1.4 && !ring.tree {
                    cover[j] = (cover[j] * 1.6).min(Vec3::ONE);
                }
                if (d - ring.radius).abs() < soil.cell[j] * 0.6 {
                    on.push((v.dot(side).atan2(v.dot(ahead)), j));
                }
            }
        }
        on.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut last: Option<Vec3> = None;
        for (_, j) in on {
            let p = g.at[j];
            let gap = if ring.tree {
                1.5
            } else {
                0.8 + 0.1 * ring.radius
            };
            if last.is_some_and(|q| q.distance(p) < gap) {
                continue;
            }
            if !soil.open(j, 3.5) || (!ring.tree && soil.light[j] > 0.45) {
                continue;
            }
            // Where it has run into another ring, it ends.
            let met = centers.around(p).any(|(q, other)| {
                other as usize != k && q.distance(p) < rings[other as usize].radius - 0.4
            });
            if met || key(j, 24) < 0.12 {
                continue;
            }
            last = Some(p);
            plants.push(Plant {
                kind: Kind::Cap,
                variant: (key(j, 25) * VARIANTS as f32) as u32 % VARIANTS,
                at: p,
                rot: soil.frame(j, 0.0, key(j, 26) * TAU),
                size: if ring.tree {
                    0.35 + 0.3 * key(j, 27)
                } else {
                    0.45 + 0.1 * ring.radius + 0.5 * key(j, 27).powi(2)
                },
                tint: ring.tint,
            });
        }
    }

    // Heart fans: lit, dryish rock; never in the shadow lane of a fan
    // that stood there first. Bells in the lane behind each.
    let mut ground = Near::new(3.0);
    for i in 0..n {
        if soil.cell[i] < 3.0 {
            ground.add(g.at[i], i as u32);
        }
    }
    let mut fans = Near::new(12.0);
    let mut order: Vec<usize> = (0..n)
        .filter(|&i| {
            soil.open(i, 5.0)
                && soil.light[i] > 0.5
                && soil.water.wet[i] < 0.45
                && key(i, 31) < soil.cell[i] * soil.cell[i] / 80.0 * (0.2 + 0.8 * thin(i))
        })
        .collect();
    order.sort_by(|&a, &b| key(a, 32).total_cmp(&key(b, 32)));
    for i in order {
        let p = g.at[i];
        let ahead = soil.to_heart(i);
        let size = 0.7 + 0.9 * key(i, 33).powi(2);
        let blocked = fans.around(p).any(|(q, _)| {
            let v = p - q;
            let along = v.dot(ahead);
            let beside = (v - ahead * along).length();
            v.length() < 2.6 || (beside < 1.7 && along.abs() < 10.0)
        });
        if blocked {
            continue;
        }
        fans.add(p, i as u32);
        plants.push(Plant {
            kind: Kind::Fan,
            variant: (key(i, 34) * VARIANTS as f32) as u32 % VARIANTS,
            at: p,
            rot: soil.frame(i, 0.18, (key(i, 35) - 0.5) * 0.5),
            size,
            tint: Vec3::ONE.lerp(Vec3::new(1.0, 0.55, 1.0), key(i, 36)),
        });
        // The file of bells in its shade, smaller the further back.
        let mut back = 0.9 * size;
        for b in 0..6 {
            back += (0.8 + 0.28 * b as f32) * size;
            let want = p - ahead * back;
            let Some((q, j)) = ground
                .around(want)
                .min_by(|a, b| a.0.distance(want).total_cmp(&b.0.distance(want)))
            else {
                break;
            };
            let j = j as usize;
            if q.distance(want) > 2.0 || !soil.open(j, 3.0) {
                break;
            }
            plants.push(Plant {
                kind: Kind::Bell,
                variant: (key(j, 37) * VARIANTS as f32) as u32 % VARIANTS,
                at: q,
                rot: soil.frame(j, 0.0, key(j, 38) * TAU),
                size: size * (1.0 - 0.12 * b as f32),
                tint: Vec3::ONE,
            });
        }
    }

    // Glass reeds on the banks, and tufts in the mats.
    for (i, &mat) in cover.iter().enumerate() {
        let wet = soil.water.wet[i];
        let area = soil.cell[i] * soil.cell[i];
        let bank = smooth(0.36, 0.48, wet) * (1.0 - smooth(0.78, 0.9, wet));
        if soil.open(i, 3.5) && key(i, 41) < bank * area * 0.5 * thin(i) {
            let own = mat / mat.max_element().max(0.05);
            plants.push(Plant {
                kind: Kind::Reed,
                variant: (key(i, 42) * VARIANTS as f32) as u32 % VARIANTS,
                at: soil.near(i, seed.wrapping_add(i as u32)),
                rot: soil.frame(i, 0.1, key(i, 43) * TAU),
                size: 0.6 + 0.9 * key(i, 44),
                tint: Vec3::ONE.lerp(own, 0.45 * mat.max_element().min(1.0)),
            });
        }
        let thick = mat.max_element();
        if thick > 0.3
            && soil.open(i, 3.0)
            && key(i, 45) < (thick - 0.2) * area * 0.2 * thin(i).powi(2)
        {
            plants.push(Plant {
                kind: Kind::Tuft,
                variant: (key(i, 46) * VARIANTS as f32) as u32 % VARIANTS,
                at: soil.near(i, seed.wrapping_add(i as u32) ^ 0x51),
                rot: soil.frame(i, 0.0, key(i, 47) * TAU),
                size: 0.6 + 1.1 * key(i, 48).powi(2),
                tint: (mat / thick).lerp(Vec3::ONE, 0.25),
            });
        }
    }
}

/// Smooth noise in 0..1 with features `size` metres across.
fn drift(p: Vec3, size: f32, k: u32) -> f32 {
    let q = p / size;
    let base = q.floor();
    let w = q - base;
    let w = w * w * (Vec3::splat(3.0) - 2.0 * w);
    let at = |dx: f32, dy: f32, dz: f32| {
        let c = base + Vec3::new(dx, dy, dz);
        let n = (c.x as i32 as u32).wrapping_mul(0x8da6_b343)
            ^ (c.y as i32 as u32).wrapping_mul(0xd816_3841)
            ^ (c.z as i32 as u32).wrapping_mul(0xcb1a_b31f);
        hash(n, k)
    };
    let mix = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x0 = mix(at(0.0, 0.0, 0.0), at(1.0, 0.0, 0.0), w.x);
    let x1 = mix(at(0.0, 1.0, 0.0), at(1.0, 1.0, 0.0), w.x);
    let x2 = mix(at(0.0, 0.0, 1.0), at(1.0, 0.0, 1.0), w.x);
    let x3 = mix(at(0.0, 1.0, 1.0), at(1.0, 1.0, 1.0), w.x);
    mix(mix(x0, x1, w.y), mix(x2, x3, w.y), w.z)
}

/// The plumb grass: clumps on nearly all ground, in light and shade, on
/// flats and on steep rock, a few metres apart; only water, roads and
/// what is built are bare of it. It lies in loose drifts, thicker here
/// and thinner there, and stands taller where the ground is damp.
fn plumb_grass(soil: &Soil, seed: u32, blades: &mut Vec<crate::grass::Blade>) {
    for (i, &at) in soil.ground.at.iter().enumerate() {
        // Only where the hero can come: the far ground is drawn too
        // coarsely to root a stalk on.
        if soil.cell[i] > 3.2 || !soil.open(i, 1.6) {
            continue;
        }
        let area = soil.cell[i] * soil.cell[i];
        let thick = 0.35 + 1.5 * drift(at, 26.0, 71).powi(2);
        let wanted = area / 4.5 * thick;
        let key = seed.wrapping_add(i as u32);
        let count = wanted.floor() as u32 + u32::from(hash(key, 81) < wanted.fract());
        for n in 0..count {
            let k = key.wrapping_mul(31).wrapping_add(n);
            blades.push(crate::grass::Blade {
                at: soil.near(i, k),
                size: 0.65 + 0.5 * hash(k, 82) + 0.35 * smooth(0.1, 0.5, soil.water.wet[i]),
                turn: hash(k, 83) * TAU,
            });
        }
    }
}

/// Everything that grows on the spire and the wall. `heart` is the
/// direction to the heart; `taken` is what is built (where, and how far
/// around it is kept clear); `path` gives the metres from a point to the
/// nearest road.
pub fn grow(
    waters: &Waters,
    heart: Vec3,
    taken: &[(Vec3, f32)],
    path: &dyn Fn(Vec3) -> f32,
) -> Growth {
    // What is built counts as road: nothing grows on it or right by it.
    let mut built = Near::new(16.0);
    for (i, &(p, _)) in taken.iter().enumerate() {
        built.add(p, i as u32);
    }
    let path = |p: Vec3| {
        built
            .around(p)
            .map(|(q, i)| q.distance(p) - taken[i as usize].1 + 3.0)
            .fold(path(p), f32::min)
    };
    let path: &dyn Fn(Vec3) -> f32 = &path;
    let spire = Soil::new(&waters.spire, &waters.spire_water, heart, false, path);
    let wall = Soil::new(&waters.wall, &waters.wall_water, heart, true, path);
    let mut spire_cover = mats(&spire, 1000);
    let mut wall_cover = mats(&wall, 900_000);
    let mut plants = Vec::new();
    grow_on(&spire, &mut spire_cover, 2000, &mut plants);
    grow_on(&wall, &mut wall_cover, 800_000, &mut plants);
    let mut blades = Vec::new();
    plumb_grass(&spire, 5000, &mut blades);
    plumb_grass(&wall, 700_000, &mut blades);
    if std::env::var("GRAV_FLORA_COUNT").is_ok() {
        eprintln!("flora: grass {}", blades.len());
        for kind in KINDS {
            let n = plants.iter().filter(|p| p.kind == kind).count();
            eprintln!("flora: {kind:?} {n}");
        }
    }
    // Landmarks for the screenshot tour: one of each kind, near where
    // the hero starts and out on the wall.
    let mut marks = Vec::new();
    let spots = [
        ("spire", Vec3::new(14.0, 48.0, -78.0)),
        ("wall", Vec3::new(150.0, 120.0, space::WALL_Z)),
    ];
    let names = [
        (Kind::Tree, "growth: a spindle tree"),
        (Kind::Cap, "growth: lantern caps"),
        (Kind::Fan, "growth: heart fans"),
        (Kind::Reed, "growth: glass reeds"),
        (Kind::Tuft, "growth: tufts in a mat"),
    ];
    for (_, spot) in spots {
        for (kind, name) in names {
            let nearest = plants
                .iter()
                .filter(|p| p.kind == kind)
                .min_by(|a, b| a.at.distance(spot).total_cmp(&b.at.distance(spot)));
            if let Some(p) = nearest {
                marks.push((name, p.at, p.rot * Vec3::Y, p.rot * Vec3::Z));
            }
        }
    }
    let _ = LANDMARKS.set(marks);
    Growth {
        spire_cover,
        wall_cover,
        plants,
        blades,
    }
}

/// A place worth a look: its name, where, its up, the way to the heart.
type Mark = (&'static str, Vec3, Vec3, Vec3);
static LANDMARKS: std::sync::OnceLock<Vec<Mark>> = std::sync::OnceLock::new();

/// Plants to look at in the screenshot tour (known once `grow` has run).
pub fn landmarks() -> &'static [Mark] {
    LANDMARKS.get().map_or(&[], |m| m.as_slice())
}

// ---------------------------------------------------------------------
// The plants' shapes.

/// A shape being built: points with normal, colour, glow.
pub(crate) struct Form {
    pub(crate) mesh: Mesh,
    ids: Vec<PointId>,
    at: Vec<Vec3>,
    normal: Vec<Vec3>,
    /// What the next points are made of (the material reads it): 0 a
    /// waxy stem, 1 bark, 2 a skin of cells (caps, plates).
    pub(crate) surface: f32,
}

/// Colour and glow of a point.
pub(crate) type Paint = (Vec3, f32);

impl Form {
    pub(crate) fn new() -> Self {
        Self {
            mesh: Mesh::new(),
            ids: Vec::new(),
            at: Vec::new(),
            normal: Vec::new(),
            surface: 0.0,
        }
    }

    pub(crate) fn point(&mut self, p: Vec3, normal: Vec3, paint: Paint) -> EngineResult<usize> {
        let id = self.mesh.add_point(p)?;
        let normal = normal.normalize_or(Vec3::Y);
        self.mesh.set_point_normal(id, normal)?;
        let c = paint.0.clamp(Vec3::ZERO, Vec3::ONE);
        self.mesh
            .set_point_color(id, Color::rgb01_unchecked(c.x, c.y, c.z))?;
        self.mesh.set_point_uv(id, [paint.1, self.surface])?;
        self.ids.push(id);
        self.at.push(p);
        self.normal.push(normal);
        Ok(self.ids.len() - 1)
    }

    /// A triangle, wound so that it faces the way its points' normals do.
    pub(crate) fn tri(&mut self, a: usize, b: usize, c: usize) -> EngineResult<()> {
        let face = (self.at[b] - self.at[a]).cross(self.at[c] - self.at[a]);
        if face.length_squared() < 1.0e-14 {
            return Ok(());
        }
        let out = self.normal[a] + self.normal[b] + self.normal[c];
        if face.dot(out) >= 0.0 {
            self.mesh
                .add_triangle(self.ids[a], self.ids[b], self.ids[c])
        } else {
            self.mesh
                .add_triangle(self.ids[a], self.ids[c], self.ids[b])
        }
    }

    /// Rings of points joined into a skin; each ring has the same count.
    pub(crate) fn skin(&mut self, rings: &[Vec<usize>]) -> EngineResult<()> {
        for pair in rings.windows(2) {
            let n = pair[0].len();
            for j in 0..n {
                let k = (j + 1) % n;
                self.tri(pair[0][j], pair[0][k], pair[1][k])?;
                self.tri(pair[0][j], pair[1][k], pair[1][j])?;
            }
        }
        Ok(())
    }

    /// A round stem along `path`, `radius(t)` thick and painted by
    /// `paint(t)` for t from 0 at its foot to 1, closed at the tip.
    pub(crate) fn tube(
        &mut self,
        path: &[Vec3],
        sides: usize,
        radius: &dyn Fn(f32) -> f32,
        paint: &dyn Fn(f32) -> Paint,
    ) -> EngineResult<()> {
        let last = path.len() - 1;
        let mut across = (path[1] - path[0]).any_orthonormal_vector();
        let mut rings = Vec::with_capacity(path.len());
        for (i, &p) in path.iter().enumerate() {
            let along = (path[(i + 1).min(last)] - path[i.saturating_sub(1)]).normalize_or(Vec3::Y);
            across =
                (across - along * across.dot(along)).normalize_or(along.any_orthonormal_vector());
            let other = along.cross(across);
            let t = i as f32 / last as f32;
            let mut ring = Vec::with_capacity(sides);
            for j in 0..sides {
                let a = j as f32 / sides as f32 * TAU;
                let out = across * a.cos() + other * a.sin();
                ring.push(self.point(p + out * radius(t), out, paint(t))?);
            }
            rings.push(ring);
        }
        self.skin(&rings)?;
        let along = (path[last] - path[last - 1]).normalize_or(Vec3::Y);
        let tip = self.point(path[last] + along * radius(1.0), along, paint(1.0))?;
        let ring = &rings[last];
        for j in 0..sides {
            self.tri(ring[j], ring[(j + 1) % sides], tip)?;
        }
        Ok(())
    }

    /// A turned shape about the axis `up` through `base`: `profile` gives
    /// (radius, height) from one end to the other; `wave(k, angle)`
    /// scales the radius of ring `k` at an angle; `paint(k, angle)`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lathe(
        &mut self,
        base: Vec3,
        up: Vec3,
        profile: &[(f32, f32)],
        sides: usize,
        wave: &dyn Fn(usize, f32) -> f32,
        paint: &dyn Fn(usize, f32) -> Paint,
    ) -> EngineResult<()> {
        let a = up.any_orthonormal_vector();
        let b = up.cross(a);
        let last = profile.len() - 1;
        // Which way the profile runs: from the top outwards and down (and
        // back in underneath), or from the foot up. Its normals point out
        // of the body either way.
        let turn: f32 = (0..=last)
            .map(|k| {
                let (a, b) = (profile[k], profile[(k + 1) % (last + 1)]);
                a.0 * b.1 - b.0 * a.1
            })
            .sum();
        let side = if turn > 0.0 { -1.0 } else { 1.0 };
        let mut rings = Vec::with_capacity(profile.len());
        for k in 0..=last {
            let (r, h) = profile[k];
            let (r0, h0) = profile[k.saturating_sub(1)];
            let (r1, h1) = profile[(k + 1).min(last)];
            // Out of the surface, for a profile that runs from the top
            // outwards and down, then back in underneath.
            let (nr, nh) = ((h0 - h1) * side, (r1 - r0) * side);
            let mut ring = Vec::with_capacity(sides);
            for j in 0..sides {
                let angle = j as f32 / sides as f32 * TAU;
                let out = a * angle.cos() + b * angle.sin();
                let p = base + out * r * wave(k, angle) + up * h;
                ring.push(self.point(p, out * nr + up * nh, paint(k, angle))?);
            }
            rings.push(ring);
        }
        self.skin(&rings)
    }

    /// A flat face with points of its own (so its edges are hard),
    /// facing away from `inside`.
    pub(crate) fn face(
        &mut self,
        corners: &[Vec3],
        inside: Vec3,
        paint: Paint,
    ) -> EngineResult<()> {
        let mut normal = (corners[1] - corners[0]).cross(corners[2] - corners[0]);
        let middle = corners.iter().copied().sum::<Vec3>() / corners.len() as f32;
        if normal.dot(middle - inside) < 0.0 {
            normal = -normal;
        }
        let mut ids = Vec::with_capacity(corners.len());
        for &c in corners {
            ids.push(self.point(c, normal, paint)?);
        }
        for i in 1..ids.len() - 1 {
            self.tri(ids[0], ids[i], ids[i + 1])?;
        }
        Ok(())
    }

    /// A block: four corners below (in order around) and the four above.
    pub(crate) fn block(
        &mut self,
        low: [Vec3; 4],
        high: [Vec3; 4],
        paint: Paint,
    ) -> EngineResult<()> {
        let inside = (low.iter().copied().sum::<Vec3>() + high.iter().copied().sum::<Vec3>()) / 8.0;
        self.face(&low, inside, paint)?;
        self.face(&high, inside, paint)?;
        for i in 0..4 {
            let j = (i + 1) % 4;
            self.face(&[low[i], low[j], high[j], high[i]], inside, paint)?;
        }
        Ok(())
    }

    /// A box around `center`, `half` its size along the axes `frame`
    /// (right, up, out).
    pub(crate) fn cuboid(
        &mut self,
        center: Vec3,
        frame: [Vec3; 3],
        half: Vec3,
        paint: Paint,
    ) -> EngineResult<()> {
        let [x, y, z] = [frame[0] * half.x, frame[1] * half.y, frame[2] * half.z];
        self.block(
            [
                center - x - y - z,
                center + x - y - z,
                center + x - y + z,
                center - x - y + z,
            ],
            [
                center - x + y - z,
                center + x + y - z,
                center + x + y + z,
                center - x + y + z,
            ],
            paint,
        )
    }

    /// A block upright on y: `half0` (x, z) wide at `y0`, `half1` at `y1`.
    pub(crate) fn frustum(
        &mut self,
        middle: Vec3,
        y0: f32,
        half0: (f32, f32),
        y1: f32,
        half1: (f32, f32),
        paint: Paint,
    ) -> EngineResult<()> {
        let ring = |y: f32, h: (f32, f32)| {
            [
                middle + Vec3::new(-h.0, y, -h.1),
                middle + Vec3::new(h.0, y, -h.1),
                middle + Vec3::new(h.0, y, h.1),
                middle + Vec3::new(-h.0, y, h.1),
            ]
        };
        self.block(ring(y0, half0), ring(y1, half1), paint)
    }

    /// A drop: a round body `radius` wide and `length` long from `at`
    /// along `dir`.
    pub(crate) fn drop(
        &mut self,
        at: Vec3,
        dir: Vec3,
        radius: f32,
        length: f32,
        sides: usize,
        paint: Paint,
    ) -> EngineResult<()> {
        let steps = (sides / 2).clamp(3, 8);
        let profile: Vec<(f32, f32)> = (0..=steps)
            .map(|k| {
                let t = k as f32 / steps as f32;
                // Fuller towards the far end.
                let r = (t * std::f32::consts::PI).sin().max(0.0).powf(0.8) * (0.7 + 0.3 * t);
                (r * radius, length * (1.0 - t))
            })
            .collect();
        self.lathe(at, dir, &profile, sides, &|_, _| 1.0, &|_, _| paint)
    }
}

/// A path that rises `height` from `foot`, bowing out along `lean` and
/// nodding over at the top by `nod`.
pub(crate) fn arc(foot: Vec3, lean: Vec3, height: f32, nod: f32, steps: usize) -> Vec<Vec3> {
    (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            foot + Vec3::Y * height * (t - nod * t * t * t * 0.5) + lean * height * t * t
        })
        .collect()
}

fn mix(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a.lerp(b, t.clamp(0.0, 1.0))
}

/// How finely a shape is made: 0 for close up, 2 for far away.
pub(crate) fn fine(detail: u32, near: usize, mid: usize, far: usize) -> usize {
    [near, mid, far][detail.min(2) as usize]
}

/// Glass reeds: a clump of thin stems, each ending in a lit bead.
fn reed(seed: u32, detail: u32) -> EngineResult<Mesh> {
    let mut f = Form::new();
    let stems = fine(detail, 9, 7, 4);
    for s in 0..stems {
        let k = seed * 131 + s as u32;
        let turn = (s as f32 + hash(k, 1)) / stems as f32 * TAU;
        let out = Vec3::new(turn.cos(), 0.0, turn.sin());
        let foot = out * 0.16 * hash(k, 2) - Vec3::Y * 0.08;
        let height = 0.9 + 1.1 * hash(k, 3);
        let path = arc(
            foot,
            out * (0.08 + 0.2 * hash(k, 4)),
            height,
            0.25,
            fine(detail, 8, 4, 2),
        );
        let thick = 0.02 + 0.012 * hash(k, 5);
        f.tube(
            &path,
            fine(detail, 8, 5, 3),
            &|t| thick * (1.0 - 0.6 * t),
            &|t| {
                let c = mix(
                    Vec3::new(0.01, 0.06, 0.06),
                    Vec3::new(0.04, 0.42, 0.36),
                    t * 1.3,
                );
                (c, 0.25 * t * t)
            },
        )?;
        let tip = path[path.len() - 1];
        let along = (tip - path[path.len() - 2]).normalize();
        f.drop(
            tip - along * 0.02,
            along,
            0.026 + 0.012 * hash(k, 6),
            0.11 + 0.07 * hash(k, 7),
            fine(detail, 10, 6, 4),
            (Vec3::new(0.2, 0.95, 0.8), 0.6),
        )?;
    }
    Ok(f.mesh)
}

/// A lantern cap: a pale stalk and a wide cap, lit from under its rim.
fn cap(seed: u32, detail: u32) -> EngineResult<Mesh> {
    let mut f = Form::new();
    let caps = if detail < 2 { 1 + seed % 3 } else { 1 };
    for c in 0..caps {
        let k = seed * 173 + c;
        let small = if c == 0 { 1.0 } else { 0.35 + 0.3 * hash(k, 1) };
        let turn = hash(k, 2) * TAU;
        let out = Vec3::new(turn.cos(), 0.0, turn.sin());
        let foot = if c == 0 {
            Vec3::ZERO
        } else {
            out * (0.28 + 0.2 * hash(k, 3))
        } - Vec3::Y * 0.08;
        let height = (0.75 + 0.5 * hash(k, 4)) * small;
        let lean = out * 0.12 * (hash(k, 5) + if c == 0 { 0.0 } else { 0.8 });
        let path = arc(foot, lean, height, 0.0, fine(detail, 7, 4, 2));
        let thick = 0.06 * small.sqrt();
        f.surface = 0.0;
        f.tube(
            &path,
            fine(detail, 24, 10, 5),
            &|t| thick * (1.5 - 1.1 * smooth(0.0, 0.3, t) + 0.5 * smooth(0.75, 1.0, t)),
            &|t| {
                (
                    mix(Vec3::new(0.28, 0.22, 0.33), Vec3::new(0.6, 0.5, 0.62), t),
                    0.1 * t,
                )
            },
        )?;
        let top = path[path.len() - 1];
        let axis = (top - path[path.len() - 2] + Vec3::Y * 0.5).normalize();
        let wide = (0.42 + 0.22 * hash(k, 6)) * small;
        let tall = wide * (0.38 + 0.3 * hash(k, 7));
        // From the crown out over the dome to the rim, then back in
        // underneath to the stalk.
        let profile = [
            (0.0, tall),
            (0.22 * wide, tall * 0.97),
            (0.5 * wide, tall * 0.8),
            (0.78 * wide, tall * 0.48),
            (0.95 * wide, tall * 0.14),
            (wide, -0.04 * wide),
            (0.93 * wide, -0.09 * wide),
            (0.6 * wide, 0.02),
            (0.25 * wide, 0.05),
            (thick * 1.2, 0.0),
        ];
        let lobes = 5.0 + (hash(k, 8) * 4.0).floor();
        let phase = hash(k, 9) * TAU;
        let sides = fine(detail, 32, 14, 7);
        f.surface = 2.0;
        f.lathe(
            top - axis * tall * 0.2,
            axis,
            &profile,
            sides,
            &|ring, angle| {
                let rim = smooth(2.0, 5.0, ring as f32) * (1.0 - smooth(6.0, 8.0, ring as f32));
                1.0 + 0.07 * rim * (angle * lobes + phase).cos()
            },
            &|ring, angle| {
                if ring >= 6 {
                    // Gills: lit, in spokes.
                    let spoke = 0.6 + 0.4 * (angle * 21.0).cos();
                    (Vec3::new(1.0, 0.36, 0.62) * spoke, 0.9 * spoke)
                } else {
                    let spot = smooth(
                        0.55,
                        0.8,
                        hash(k + (angle * 2.9) as u32 * 7 + ring as u32, 10),
                    );
                    let skin = mix(
                        Vec3::new(0.5, 0.03, 0.3),
                        Vec3::new(0.2, 0.02, 0.32),
                        ring as f32 / 5.0,
                    );
                    (
                        mix(skin, Vec3::new(0.95, 0.7, 0.85), spot * 0.8),
                        0.05 + 0.3 * spot + if ring == 5 { 0.5 } else { 0.0 },
                    )
                }
            },
        )?;
    }
    Ok(f.mesh)
}

/// A heart fan: a flat lattice of branches that faces the heart (+z).
fn fan(seed: u32, detail: u32) -> EngineResult<Mesh> {
    let mut f = Form::new();
    let depth = fine(detail, 6, 4, 3) as u32;
    // Branches still to grow: foot, angle in the fan's plane, length,
    // thickness, how far along the fan (0 foot, 1 tips), generation.
    let mut todo = vec![(
        Vec3::new(0.0, -0.08, 0.0),
        0.0_f32,
        0.62,
        0.05,
        0u32,
        seed * 977,
    )];
    while let Some((foot, angle, length, thick, gen, k)) = todo.pop() {
        let steps = fine(detail, 4, 2, 1);
        let bend = (hash(k, 1) - 0.5) * 0.5;
        let path: Vec<Vec3> = (0..=steps)
            .map(|i| {
                let t = i as f32 / steps as f32;
                let a = angle + bend * t;
                foot + Vec3::new(a.sin(), a.cos(), 0.12 * (hash(k, 2) - 0.5) * t) * length * t
            })
            .collect();
        let end = path[steps];
        let (t0, t1) = (
            gen as f32 / (depth + 1) as f32,
            (gen + 1) as f32 / (depth + 1) as f32,
        );
        let thin = thick * 0.66;
        let last = gen == depth;
        f.tube(
            &path,
            fine(detail, 7, 5, 3),
            &|t| thick + (thin - thick) * t,
            &|t| {
                let u = t0 + (t1 - t0) * t;
                let c = if u < 0.5 {
                    mix(
                        Vec3::new(0.1, 0.008, 0.03),
                        Vec3::new(0.62, 0.03, 0.09),
                        u * 2.0,
                    )
                } else {
                    mix(
                        Vec3::new(0.62, 0.03, 0.09),
                        Vec3::new(1.0, 0.32, 0.5),
                        u * 2.0 - 1.0,
                    )
                };
                (c, 0.55 * u * u * u)
            },
        )?;
        if last {
            continue;
        }
        let kids = if hash(k, 3) < 0.3 && gen > 0 { 3 } else { 2 };
        for kid in 0..kids {
            let spread = (kid as f32 / (kids - 1) as f32 - 0.5) * (0.75 + 0.5 * hash(k, 4 + kid));
            let turned = (angle + bend + spread).clamp(-1.25, 1.25);
            todo.push((
                end,
                turned,
                length * (0.68 + 0.2 * hash(k, 7 + kid)),
                thin,
                gen + 1,
                k.wrapping_mul(31).wrapping_add(kid + 1),
            ));
        }
    }
    Ok(f.mesh)
}

/// Bells: a few thin stalks that arch over, each hanging a lit bulb.
fn bell(seed: u32, detail: u32) -> EngineResult<Mesh> {
    let mut f = Form::new();
    let stalks = fine(detail, 4, 3, 2);
    for s in 0..stalks {
        let k = seed * 211 + s as u32;
        let turn = (s as f32 + hash(k, 1)) / stalks as f32 * TAU;
        let out = Vec3::new(turn.cos(), 0.0, turn.sin());
        let height = 0.3 + 0.3 * hash(k, 2);
        let path = arc(
            out * 0.03 - Vec3::Y * 0.05,
            out * (0.3 + 0.3 * hash(k, 3)),
            height,
            1.1,
            fine(detail, 7, 4, 2),
        );
        f.tube(
            &path,
            fine(detail, 6, 4, 3),
            &|t| 0.012 * (1.0 - 0.5 * t),
            &|t| {
                (
                    mix(Vec3::new(0.05, 0.12, 0.03), Vec3::new(0.35, 0.6, 0.15), t),
                    0.2 * t,
                )
            },
        )?;
        let tip = path[path.len() - 1];
        f.drop(
            tip + Vec3::Y * 0.01,
            -Vec3::Y,
            0.05 + 0.025 * hash(k, 4),
            0.11 + 0.05 * hash(k, 5),
            fine(detail, 12, 7, 4),
            (Vec3::new(0.8, 1.0, 0.42), 1.5),
        )?;
    }
    Ok(f.mesh)
}

/// A spindle tree: a trunk that winds up towards the heart, with a stair
/// of wide plates turning around it and a head of lit pods.
fn tree(seed: u32, detail: u32) -> EngineResult<Mesh> {
    let mut f = Form::new();
    let k = seed * 307;
    let height = 7.5 + 4.0 * hash(k, 1);
    let turns = 1.2 + 1.2 * hash(k, 2);
    let sway = 0.35 + 0.4 * hash(k, 3);
    let hand = if hash(k, 4) < 0.5 { 1.0 } else { -1.0 };
    let steps = fine(detail, 30, 12, 5);
    let spine = |t: f32| {
        let a = t * turns * TAU * hand;
        let r = sway * (t * 3.0).min(1.0) * (1.0 - 0.6 * t);
        Vec3::new(a.cos() * r, height * t - 0.25, a.sin() * r + 0.9 * t * t)
    };
    let path: Vec<Vec3> = (0..=steps)
        .map(|i| spine(i as f32 / steps as f32))
        .collect();
    let girth = 0.3 + 0.12 * hash(k, 5);
    f.surface = 1.0;
    f.tube(
        &path,
        fine(detail, 26, 12, 6),
        &|t| girth * (1.9 - 0.9 * smooth(0.0, 0.12, t)) * (1.0 - 0.72 * t),
        &|t| {
            // Pale rings where plates once sat.
            let band = smooth(0.8, 0.95, (t * 46.0 + hash(k, 6)).fract());
            (
                mix(
                    Vec3::new(0.06, 0.025, 0.08),
                    Vec3::new(0.34, 0.2, 0.4),
                    band * 0.8,
                ),
                0.04 * band,
            )
        },
    )?;
    if detail >= 3 {
        return Ok(f.mesh);
    }
    // The plates: one after the other up the trunk, each turned on from
    // the last by the same part of a turn, smaller towards the top.
    let plates = fine(detail, 13, 9, 5);
    for i in 0..plates {
        let t = 0.26 + 0.66 * i as f32 / plates as f32;
        let at = spine(t);
        let a = i as f32 * 2.399 * hand + hash(k, 7) * TAU;
        let out = Vec3::new(a.cos(), 0.0, a.sin());
        let reach = (2.6 - 1.7 * t) * (0.8 + 0.4 * hash(k + i as u32, 8));
        let arm: Vec<Vec3> = (0..=fine(detail, 6, 3, 2))
            .map(|j| {
                let u = j as f32 / fine(detail, 6, 3, 2) as f32;
                at + out * reach * u + Vec3::Y * reach * 0.3 * (u - u * u * 0.6)
            })
            .collect();
        let thick = girth * 0.3 * (1.0 - 0.5 * t);
        f.surface = 1.0;
        f.tube(
            &arm,
            fine(detail, 10, 6, 3),
            &|u| thick * (1.0 - 0.55 * u),
            &|_| (Vec3::new(0.09, 0.035, 0.12), 0.0),
        )?;
        let wide = reach * (0.42 + 0.14 * hash(k + i as u32, 9));
        let hub = arm[arm.len() - 1];
        let tilt = (Vec3::Y + out * 0.25).normalize();
        let profile = [
            (0.0, 0.07 * wide),
            (0.3 * wide, 0.08 * wide),
            (0.7 * wide, 0.1 * wide),
            (0.96 * wide, 0.16 * wide),
            (wide, 0.13 * wide),
            (0.9 * wide, 0.06 * wide),
            (0.5 * wide, 0.0),
            (0.0, -0.03 * wide),
        ];
        let lobes = 3.0 + (hash(k + i as u32, 10) * 3.0).floor();
        let phase = hash(k + i as u32, 11) * TAU;
        f.surface = 2.0;
        f.lathe(
            hub,
            tilt,
            &profile,
            fine(detail, 28, 12, 6),
            &|ring, angle| 1.0 + 0.1 * (angle * lobes + phase).cos() * (ring as f32 / 4.0).min(1.0),
            &|ring, angle| match ring {
                0..=2 => {
                    let vein = smooth(0.7, 1.0, (angle * 9.0 + phase).cos());
                    (
                        mix(
                            Vec3::new(0.02, 0.26, 0.28),
                            Vec3::new(0.2, 0.75, 0.6),
                            vein * ring as f32 * 0.5,
                        ),
                        0.12 * vein * ring as f32,
                    )
                }
                3 | 4 => (Vec3::new(0.4, 1.0, 0.86), 1.0),
                _ => (Vec3::new(0.55, 0.05, 0.4), 0.3),
            },
        )?;
    }
    // The head: pods on short stalks.
    let top = spine(1.0);
    for i in 0..fine(detail, 9, 6, 3) {
        let kk = k + 50 + i as u32;
        let a = hash(kk, 12) * TAU;
        let dir = Vec3::new(a.cos() * 0.8, 0.5 + hash(kk, 13), a.sin() * 0.8).normalize();
        let length = 0.5 + 0.9 * hash(kk, 14);
        f.surface = 1.0;
        f.tube(
            &[
                top - Vec3::Y * 0.2,
                top + dir * length * 0.5,
                top + dir * length,
            ],
            fine(detail, 6, 4, 3),
            &|u| 0.035 * (1.0 - 0.5 * u),
            &|_| (Vec3::new(0.09, 0.035, 0.12), 0.0),
        )?;
        f.surface = 0.0;
        f.drop(
            top + dir * length,
            dir,
            0.12 + 0.08 * hash(kk, 15),
            0.36 + 0.2 * hash(kk, 16),
            fine(detail, 12, 7, 4),
            (Vec3::new(0.9, 1.0, 0.55), 1.4),
        )?;
    }
    Ok(f.mesh)
}

/// A tuft: short blunt fingers. Pale, so that it takes its mat's colour.
fn tuft(seed: u32, detail: u32) -> EngineResult<Mesh> {
    let mut f = Form::new();
    let fingers = fine(detail, 11, 7, 4);
    for s in 0..fingers {
        let k = seed * 419 + s as u32;
        let turn = (s as f32 + hash(k, 1)) / fingers as f32 * TAU;
        let out = Vec3::new(turn.cos(), 0.0, turn.sin());
        let foot = out * 0.14 * hash(k, 2).sqrt() - Vec3::Y * 0.04;
        let height = 0.12 + 0.26 * hash(k, 3);
        let path = arc(
            foot,
            out * (0.15 + 0.4 * hash(k, 4)),
            height,
            0.3,
            fine(detail, 4, 2, 1),
        );
        let thick = 0.016 + 0.012 * hash(k, 5);
        f.tube(
            &path,
            fine(detail, 7, 4, 3),
            &|t| thick * (1.0 - 0.3 * t),
            &|t| {
                (
                    mix(Vec3::splat(0.16), Vec3::splat(0.95), t * t),
                    0.3 * t * t * t,
                )
            },
        )?;
    }
    Ok(f.mesh)
}

/// One shape of one kind of plant, at its levels of detail, with the
/// places where it stands.
pub struct Stand {
    pub levels: Vec<(PreparedMesh, f32)>,
    pub places: Vec<Place>,
    pub shadow: bool,
    pub stuff: Stuff,
    /// Its plain solid shape, for collision (none: one walks through).
    pub solid: Option<Mesh>,
}

/// What a stand is made of (which material draws it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stuff {
    Flora,
    Built,
}

type Job = Box<dyn FnOnce() -> EngineResult<Stand> + Send>;
/// Makes a plant's shape from a seed and a level of detail.
type Shaper = fn(u32, u32) -> EngineResult<Mesh>;

/// The plants as jobs for worker threads: one per kind and shape.
pub fn stands(plants: &[Plant]) -> Vec<Job> {
    let mut jobs: Vec<Job> = Vec::new();
    for kind in KINDS {
        for variant in 0..VARIANTS {
            let places: Vec<Place> = plants
                .iter()
                .filter(|p| p.kind == kind && p.variant == variant)
                .map(|p| {
                    space::place(p.at, p.rot)
                        .with_scale(p.size)
                        .with_tint(Color::rgb01_unchecked(p.tint.x, p.tint.y, p.tint.z))
                })
                .collect();
            if places.is_empty() {
                continue;
            }
            jobs.push(Box::new(move || {
                let seed = variant + 1;
                let (shape, reach): (Shaper, [f32; 3]) = match kind {
                    Kind::Reed => (reed, [22.0, 60.0, 260.0]),
                    Kind::Cap => (cap, [26.0, 80.0, 420.0]),
                    Kind::Fan => (fan, [30.0, 95.0, 480.0]),
                    Kind::Bell => (bell, [16.0, 45.0, 170.0]),
                    Kind::Tree => (tree, [70.0, 220.0, 1800.0]),
                    Kind::Tuft => (tuft, [12.0, 30.0, 75.0]),
                };
                let mut levels = Vec::new();
                for (detail, to) in reach.into_iter().enumerate() {
                    levels.push((shape(seed, detail as u32)?.prepare(), to));
                }
                Ok(Stand {
                    levels,
                    places,
                    shadow: matches!(kind, Kind::Tree | Kind::Cap | Kind::Fan),
                    stuff: Stuff::Flora,
                    // A tree's trunk is solid.
                    solid: if kind == Kind::Tree {
                        Some(tree(seed, 3)?)
                    } else {
                        None
                    },
                })
            }));
        }
    }
    jobs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_plant_shape_builds_at_every_level() {
        let shapes: [Shaper; 6] = [reed, cap, fan, bell, tree, tuft];
        for shape in shapes {
            let mut last = usize::MAX;
            for detail in 0..3 {
                let mesh = shape(1, detail).unwrap();
                assert!(mesh.face_count() > 0);
                // Each level is lighter than the one before.
                assert!(mesh.face_count() < last);
                last = mesh.face_count();
            }
        }
    }

    #[test]
    fn a_turned_shape_faces_outwards_whichever_way_its_profile_runs() {
        for profile in [
            vec![(1.0, 0.0), (1.2, 1.0), (0.8, 2.0), (0.0, 3.0)],
            vec![(0.0, 3.0), (0.8, 2.0), (1.2, 1.0), (1.0, 0.0)],
        ] {
            let mut f = Form::new();
            f.lathe(Vec3::ZERO, Vec3::Y, &profile, 12, &|_, _| 1.0, &|_, _| {
                (Vec3::ONE, 0.0)
            })
            .unwrap();
            for (at, normal) in f.at.iter().zip(&f.normal) {
                let out = Vec3::new(at.x, 0.0, at.z);
                if out.length() > 0.5 {
                    assert!(normal.dot(out) > 0.0, "{at} {normal}");
                }
            }
        }
    }

    #[test]
    fn a_close_up_tree_has_a_few_thousand_triangles() {
        let n = tree(1, 0).unwrap().face_count();
        assert!((3000..40_000).contains(&n), "{n}");
        assert!(tree(1, 2).unwrap().face_count() < 900);
    }
}
