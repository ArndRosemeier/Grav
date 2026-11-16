//! A building as bonded pieces (W1 to W4): the shape that is drawn, cut
//! into the pieces it breaks into.
//!
//! The cut works on the finished shape, so the pieces together are the
//! building as it stands (what is drawn is what breaks). The shape's
//! triangles are sorted into cells of space and, within a cell, by what
//! they are made of. Masonry is a skin in the drawn shape; a piece of it
//! gets its thickness here: an inner face and broken edges all round.

use crate::flora::Form;
use engine::prelude::*;
use grav_core::BondClass;
use std::collections::{BTreeMap, HashMap};

/// One piece of a building, in the building's own frame.
pub struct Shard {
    /// Its shape around its own centre.
    pub mesh: Mesh,
    pub center: Vec3,
    /// Its bounds in the building's frame.
    pub lo: Vec3,
    pub hi: Vec3,
    pub class: BondClass,
    /// The pieces it is bonded to (indices into the building's pieces).
    pub neighbours: Vec<usize>,
    /// Held by the ground it stands on.
    pub anchored: bool,
}

/// How space is cut into cells.
#[derive(Clone, Copy)]
pub enum Cells {
    /// Rings around the y axis: bands `0` metres high in `1` sectors.
    Round(f32, u32),
    /// Blocks of this size.
    Square(Vec3),
    /// No cut: one piece of each kind of surface.
    One,
}

impl Cells {
    fn of(self, p: Vec3) -> (i32, i32, i32) {
        match self {
            Cells::Round(band, sectors) => {
                let turn = p.x.atan2(p.z).rem_euclid(std::f32::consts::TAU);
                let sector = (turn / std::f32::consts::TAU * sectors as f32) as i32;
                (
                    (p.y / band).floor() as i32,
                    sector.min(sectors as i32 - 1),
                    0,
                )
            }
            Cells::One => (0, 0, 0),
            Cells::Square(size) => {
                let q = (p / size).floor();
                (q.x as i32, q.y as i32, q.z as i32)
            }
        }
    }
}

/// What a kind of surface is as a piece: how strongly it is bonded and,
/// for masonry skins, how thick it is.
pub type Stuff = (BondClass, Option<f32>);

/// Where a building is held.
#[derive(Clone, Copy, PartialEq)]
pub enum Hold {
    /// By the ground under it.
    Foot,
    /// At both ends along z (a bridge).
    Ends,
}

/// A cell of space and a kind of surface: what makes one piece.
type Group = ((i32, i32, i32), i32);

/// Cut `form` into pieces. `stuff` says what each kind of surface is.
pub fn shatter(
    form: &Form,
    cells: Cells,
    hold: Hold,
    stuff: &dyn Fn(f32) -> Stuff,
) -> EngineResult<Vec<Shard>> {
    // Triangles by cell and kind (in a fixed order: same shape, same pieces).
    let mut groups: BTreeMap<Group, Vec<[usize; 3]>> = BTreeMap::new();
    for &t in &form.tris {
        let mid = (form.at[t[0]] + form.at[t[1]] + form.at[t[2]]) / 3.0;
        let kind = form.kind[t[0]].round() as i32;
        groups.entry((cells.of(mid), kind)).or_default().push(t);
    }
    let mut shards = Vec::with_capacity(groups.len());
    for ((_, kind), tris) in groups {
        let (class, thick) = stuff(kind as f32);
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for t in &tris {
            for &i in t {
                lo = lo.min(form.at[i]);
                hi = hi.max(form.at[i]);
            }
        }
        let center = (lo + hi) * 0.5;
        let mut f = Form::new();
        f.surface = kind as f32;
        let mut outer: HashMap<usize, usize> = HashMap::new();
        for t in &tris {
            let mut own = [0; 3];
            for (k, &i) in t.iter().enumerate() {
                own[k] = match outer.get(&i) {
                    Some(&j) => j,
                    None => {
                        let j = f.point(form.at[i] - center, form.normal[i], form.paint[i])?;
                        outer.insert(i, j);
                        j
                    }
                };
            }
            f.tri(own[0], own[1], own[2])?;
        }
        if let Some(thick) = thick {
            thicken(form, &tris, center, thick, &mut f)?;
            lo = lo.min(lo - Vec3::splat(thick * 0.5));
            hi = hi.max(hi + Vec3::splat(thick * 0.5));
        }
        if f.tris.is_empty() {
            continue;
        }
        shards.push(Shard {
            mesh: f.mesh,
            center,
            lo,
            hi,
            class,
            neighbours: Vec::new(),
            anchored: false,
        });
    }
    // Bonds: pieces that touch. Anchors: the pieces at the foot or ends.
    let margin = Vec3::splat(0.12);
    for a in 0..shards.len() {
        for b in a + 1..shards.len() {
            let apart = (shards[a].lo - margin).cmpgt(shards[b].hi + margin)
                | (shards[b].lo - margin).cmpgt(shards[a].hi + margin);
            if !apart.any() {
                shards[a].neighbours.push(b);
                shards[b].neighbours.push(a);
            }
        }
    }
    let low = shards
        .iter()
        .map(|s| s.lo)
        .fold(Vec3::splat(f32::MAX), Vec3::min);
    let high = shards
        .iter()
        .map(|s| s.hi)
        .fold(Vec3::splat(f32::MIN), Vec3::max);
    for s in &mut shards {
        s.anchored = match hold {
            Hold::Foot => s.lo.y < low.y + 0.9,
            Hold::Ends => s.lo.z < low.z + 0.8 || s.hi.z > high.z - 0.8,
        };
    }
    Ok(shards)
}

/// Give a piece of skin its thickness: an inner face, and a broken edge
/// wherever the piece ends.
fn thicken(
    form: &Form,
    tris: &[[usize; 3]],
    center: Vec3,
    thick: f32,
    f: &mut Form,
) -> EngineResult<()> {
    let dim = |paint: (Vec3, f32)| (paint.0 * 0.5, 0.0);
    let inner_at = |i: usize| form.at[i] - form.normal[i] * thick - center;
    let mut inner: HashMap<usize, usize> = HashMap::new();
    // Edges by where their ends are (points are doubled at seams): how
    // often each is used, and one use with its direction and face normal.
    let spot = |p: Vec3| {
        let q = (p * 500.0).round();
        (q.x as i64, q.y as i64, q.z as i64)
    };
    type Spot = (i64, i64, i64);
    let mut edges: BTreeMap<(Spot, Spot), (u32, usize, usize, Vec3)> = BTreeMap::new();
    for t in tris {
        let mut own = [0; 3];
        for (k, &i) in t.iter().enumerate() {
            own[k] = match inner.get(&i) {
                Some(&j) => j,
                None => {
                    let j = f.point(inner_at(i), -form.normal[i], dim(form.paint[i]))?;
                    inner.insert(i, j);
                    j
                }
            };
        }
        f.tri(own[0], own[1], own[2])?;
        let face = (form.at[t[1]] - form.at[t[0]])
            .cross(form.at[t[2]] - form.at[t[0]])
            .normalize_or(form.normal[t[0]]);
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let (sa, sb) = (spot(form.at[a]), spot(form.at[b]));
            let key = if sa <= sb { (sa, sb) } else { (sb, sa) };
            edges.entry(key).or_insert((0, a, b, face)).0 += 1;
        }
    }
    for (count, a, b, face) in edges.into_values() {
        if count != 1 {
            continue;
        }
        let side = (form.at[b] - form.at[a]).cross(face).normalize_or(face);
        let (pa, pb) = (form.paint[a], form.paint[b]);
        let rough = |paint: (Vec3, f32)| (paint.0 * 0.72, 0.0);
        let q = [
            f.point(form.at[a] - center, side, rough(pa))?,
            f.point(form.at[b] - center, side, rough(pb))?,
            f.point(inner_at(b), side, dim(pb))?,
            f.point(inner_at(a), side, dim(pa))?,
        ];
        f.tri(q[0], q[1], q[2])?;
        f.tri(q[0], q[2], q[3])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain round tower skin, 8 m tall.
    fn tube() -> Form {
        let mut f = Form::new();
        let profile: Vec<(f32, f32)> = (0..=16).map(|i| (2.0, i as f32 * 0.5)).collect();
        f.lathe(Vec3::ZERO, Vec3::Y, &profile, 24, &|_, _| 1.0, &|_, _| {
            (Vec3::splat(0.5), 0.0)
        })
        .unwrap();
        f
    }

    #[test]
    fn a_skin_is_cut_into_thick_pieces_that_hold_each_other() {
        let form = tube();
        let shards = shatter(&form, Cells::Round(2.0, 6), Hold::Foot, &|_| {
            (BondClass::Masonry, Some(0.3))
        })
        .unwrap();
        // 4 bands of 6 sectors (the closed ends fall into the same cells).
        assert_eq!(shards.len(), 24, "{}", shards.len());
        let outer: usize = form.tris.len();
        let all: usize = shards.iter().map(|s| s.mesh.face_count()).sum();
        // Outer and inner face, plus the broken edges.
        assert!(all > outer * 2, "{all} faces from {outer}");
        assert_eq!(shards.iter().filter(|s| s.anchored).count(), 6);
        for (i, s) in shards.iter().enumerate() {
            assert!(s.neighbours.len() >= 3, "piece {i}: {:?}", s.neighbours);
            assert!(!s.neighbours.contains(&i));
            // Around its own centre.
            let (lo, hi) = s.mesh.bounds().unwrap();
            assert!((lo + hi).length() < 0.8, "piece {i} is off centre");
        }
    }
}
