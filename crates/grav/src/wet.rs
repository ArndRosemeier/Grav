//! Where the water is: worked out from the pull, like the rock.
//!
//! Water seeps out where it is dark and cool (the side of the rock turned
//! from the heart) and at springs; it runs downhill in the veins' pull,
//! which is down the potential (`Field::vein_potential`); it stands in
//! hollows. A pool grows until what dries off its surface matches what
//! runs in, or until it spills over its rim into the next hollow.
//!
//! The result is a water map on the same grid the rock is meshed on: how
//! much water passes each point, how deep it stands, and how wet the rock
//! is. The rock's material, the drawn water, and later the vegetation and
//! the settlements all read it.

use engine::prelude::*;
use engine::proc::Noise;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Condensation on rock turned fully from the heart, in water per square
/// metre (the unit all flow is counted in), and on rock facing it.
const DEW_SHADE: f32 = 1.0;
const DEW_LIGHT: f32 = 0.12;
/// A spring gives as much as this many square metres of shaded rock.
const SPRING: f32 = 40.0;
/// What dries off a square metre of standing water. Pools therefore cover
/// about a fifteenth of the ground that feeds them.
const DRYING: f32 = 15.0;
/// Flow (in square metres of shaded rock feeding it) at which a trickle
/// shows, and at which it is a full stream.
pub const TRICKLE: f32 = 220.0;
pub const STREAM: f32 = 2600.0;

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A grid of points on the rock, with what the water needs to know of each.
pub struct Ground {
    pub rows: usize,
    pub cols: usize,
    /// The columns close into a ring (around a spire).
    pub ring: bool,
    pub at: Vec<Vec3>,
    /// The veins' potential there: water runs to where it is lower.
    pub potential: Vec<f32>,
    /// The pull there in g, and the way up (against it).
    pub pull: Vec<f32>,
    pub up: Vec<Vec3>,
    /// Water that reaches this point is gone (it leaves this ground).
    pub drain: Vec<bool>,
}

/// The water on a `Ground`, per point.
pub struct Water {
    /// How much passes, in square metres of shaded rock feeding it.
    pub flow: Vec<f32>,
    /// How deep it stands, in metres.
    pub depth: Vec<f32>,
    /// How wet the rock is: up to 0.3 damp, about 0.5 on a bank, 0.8 and
    /// more where water runs, 1 under standing water.
    pub wet: Vec<f32>,
    /// Where the water's surface lies above the rock (below it, if
    /// negative), in metres: the pool's level carried on under its banks,
    /// so a drawn surface meets the rock along the true shoreline.
    pub surface: Vec<f32>,
}

#[derive(PartialEq)]
struct Level(f32, usize);
impl Eq for Level {}
impl PartialOrd for Level {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Level {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0).then(self.1.cmp(&other.1))
    }
}

impl Ground {
    pub fn new(rows: usize, cols: usize, ring: bool) -> Self {
        let n = rows * cols;
        Self {
            rows,
            cols,
            ring,
            at: vec![Vec3::ZERO; n],
            potential: vec![0.0; n],
            pull: vec![1.0; n],
            up: vec![Vec3::Y; n],
            drain: vec![false; n],
        }
    }

    pub fn index(&self, row: usize, col: usize) -> usize {
        row * self.cols + col % self.cols
    }

    fn neighbours(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        let (r, c) = ((i / self.cols) as i32, (i % self.cols) as i32);
        let (rows, cols) = (self.rows as i32, self.cols as i32);
        (-1..=1)
            .flat_map(move |dr| (-1..=1).map(move |dc| (dr, dc)))
            .filter_map(move |(dr, dc)| {
                if dr == 0 && dc == 0 {
                    return None;
                }
                let rr = r + dr;
                let mut cc = c + dc;
                if self.ring {
                    cc = cc.rem_euclid(cols);
                }
                (rr >= 0 && rr < rows && cc >= 0 && cc < cols).then(|| (rr * cols + cc) as usize)
            })
    }

    /// The ground each point stands for, in square metres.
    fn areas(&self) -> Vec<f32> {
        (0..self.at.len())
            .map(|i| {
                let (r, c) = (i / self.cols, i % self.cols);
                let r1 = (r + 1).min(self.rows - 1);
                let r0 = r1 - 1;
                let (c0, c1) = if self.ring {
                    (c, (c + 1) % self.cols)
                } else {
                    let c1 = (c + 1).min(self.cols - 1);
                    (c1 - 1, c1)
                };
                let along = self.at[r1 * self.cols + c].distance(self.at[r0 * self.cols + c]);
                let across = self.at[r * self.cols + c1].distance(self.at[r * self.cols + c0]);
                along * across
            })
            .collect()
    }

    /// Let the water run and stand. `heart` is the way to the heart (unit).
    pub fn water(&self, heart: Vec3) -> Water {
        let n = self.at.len();
        let noise = Noise::new(4242);
        let areas = self.areas();
        // What each point gives: dew in the shade, a few springs.
        let mut flow: Vec<f32> = (0..n)
            .map(|i| {
                let shade = smooth(0.35, -0.6, self.up[i].dot(heart));
                let dew = DEW_LIGHT + (DEW_SHADE - DEW_LIGHT) * shade;
                let p = self.at[i];
                let spot = noise.fbm3(p * 0.021 + Vec3::splat(88.0), 3, 2.03, 0.5) * 0.5 + 0.5;
                let spring = smooth(0.74, 0.8, spot) * SPRING;
                (dew + spring) * areas[i]
            })
            .collect();
        // Downhill: the neighbour the potential falls to most steeply.
        let none = usize::MAX;
        let down: Vec<usize> = (0..n)
            .map(|i| {
                if self.drain[i] {
                    return none;
                }
                let mut best = (0.0, none);
                for j in self.neighbours(i) {
                    let fall = (self.potential[i] - self.potential[j])
                        / self.at[i].distance(self.at[j]).max(1.0e-3);
                    if fall > best.0 {
                        best = (fall, j);
                    }
                }
                best.1
            })
            .collect();
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &b| self.potential[b].total_cmp(&self.potential[a]));
        for &i in &order {
            if down[i] != none {
                flow[down[i]] += flow[i];
            }
        }
        // Hollows fill, highest first, so what spills reaches the lower ones.
        let mut pool_of = vec![none; n];
        let mut level_of = vec![0.0_f32; n];
        for &pit in &order {
            if down[pit] != none || self.drain[pit] || pool_of[pit] != none {
                continue;
            }
            let inflow = flow[pit];
            let mut heap = BinaryHeap::new();
            let mut seen = std::collections::HashSet::new();
            heap.push(Reverse(Level(self.potential[pit], pit)));
            seen.insert(pit);
            let (mut level, mut surface) = (self.potential[pit], 0.0);
            let mut members = Vec::new();
            let mut spill = none;
            while let Some(Reverse(Level(phi, c))) = heap.pop() {
                if phi < level - 1.0e-4 || self.drain[c] || pool_of[c] != none {
                    // Over the rim: the rest runs on from here.
                    spill = c;
                    break;
                }
                level = phi;
                members.push(c);
                surface += areas[c];
                if surface * DRYING >= inflow {
                    break;
                }
                for j in self.neighbours(c) {
                    if seen.insert(j) {
                        heap.push(Reverse(Level(self.potential[j], j)));
                    }
                }
            }
            for &c in &members {
                pool_of[c] = pit;
                level_of[c] = level;
            }
            if spill != none {
                let over = (inflow - surface * DRYING).max(0.0);
                let mut c = spill;
                let mut steps = 0;
                while c != none && steps < n {
                    flow[c] += over;
                    c = down[c];
                    steps += 1;
                }
            }
        }
        let depth: Vec<f32> = (0..n)
            .map(|i| {
                if pool_of[i] == none {
                    0.0
                } else {
                    (level_of[i] - self.potential[i]).max(0.0) / self.pull[i].max(0.05)
                }
            })
            .collect();
        // Wet rock: under water, beside a stream, damp in the shade; and a
        // little of it spreads to the rock next to it.
        let own: Vec<f32> = (0..n)
            .map(|i| {
                let shade = smooth(0.35, -0.6, self.up[i].dot(heart));
                let p = self.at[i];
                let patch = noise.fbm3(p * 0.06 + Vec3::splat(31.0), 3, 2.03, 0.5) * 0.5 + 0.5;
                let damp = 0.3 * shade * smooth(0.35, 0.7, patch);
                let per_metre = flow[i] / areas[i].sqrt().max(0.3);
                let stream = smooth(TRICKLE * 0.5, TRICKLE, per_metre)
                    * (0.8 + 0.2 * smooth(TRICKLE, STREAM, per_metre));
                let pool = if depth[i] > 0.0 { 1.0 } else { 0.0 };
                damp.max(stream).max(pool)
            })
            .collect();
        // Water soaks into the banks: less with every step from it.
        let mut wet = own.clone();
        for _ in 0..3 {
            wet = (0..n)
                .map(|i| {
                    let near = self.neighbours(i).map(|j| wet[j]).fold(0.0, f32::max);
                    wet[i].max(near * 0.72)
                })
                .collect();
        }
        let surface = (0..n)
            .map(|i| {
                if pool_of[i] != none {
                    return depth[i];
                }
                let beside = self
                    .neighbours(i)
                    .filter(|&j| pool_of[j] != none)
                    .map(|j| (level_of[j] - self.potential[i]) / self.pull[i].max(0.05))
                    .fold(f32::MIN, f32::max);
                if beside > f32::MIN {
                    beside.clamp(-2.0, 0.0)
                } else if own[i] >= 0.8 {
                    0.0
                } else {
                    -0.3
                }
            })
            .collect();
        Water {
            flow,
            depth,
            wet,
            surface,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A square of ground lying in a 1 g pull along -y, with a height.
    fn ground(size: usize, height: impl Fn(f32, f32) -> f32) -> Ground {
        let mut g = Ground::new(size, size, false);
        for r in 0..size {
            for c in 0..size {
                let i = g.index(r, c);
                let h = height(c as f32, r as f32);
                g.at[i] = Vec3::new(c as f32, h, r as f32);
                g.potential[i] = h;
                g.drain[i] = r == 0 || c == 0 || r == size - 1 || c == size - 1;
            }
        }
        g
    }

    #[test]
    fn water_runs_down_a_slope_and_gathers() {
        // A valley along z that falls towards z = 0.
        let g = ground(60, |x, z| (x - 30.0).abs() * 0.3 + z * 0.1);
        let w = g.water(Vec3::NEG_Y);
        let low = w.flow[g.index(3, 30)];
        let high = w.flow[g.index(50, 30)];
        let side = w.flow[g.index(30, 10)];
        assert!(low > high && high > side, "{low} {high} {side}");
        assert!(w.depth.iter().all(|&d| d == 0.0), "nothing to stand in");
    }

    #[test]
    fn a_hollow_holds_a_pool_no_deeper_than_its_rim() {
        // A bowl 6 m deep in the middle of a gentle slope.
        let bowl = |x: f32, z: f32| {
            let d = ((x - 30.0).powi(2) + (z - 30.0).powi(2)).sqrt();
            z * 0.02 - 6.0 * (1.0 - (d / 12.0).min(1.0)).powi(2)
        };
        let g = ground(60, bowl);
        let w = g.water(Vec3::NEG_Y);
        let middle = w.depth[g.index(30, 30)];
        assert!(middle > 0.2 && middle <= 6.5, "{middle}");
        assert_eq!(w.depth[g.index(5, 5)], 0.0);
        assert!(w.wet[g.index(30, 30)] == 1.0);
    }
}
