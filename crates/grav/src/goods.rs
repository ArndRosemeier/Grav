//! Loose goods in the streets (W5): jars by the umbral towers, casks by
//! the dwarven holds, crates and sacks by the goblin shacks.
//!
//! They stand as drawn shapes among many of their kind until a pull takes
//! hold of them (movable.rs). Then each becomes a body of its own that
//! follows the pull, and stays one.

use crate::flora::{fine, hash, solid, Form};
use crate::movable::{self, Movable};
use crate::settle::{CLOTH, FOAM, IRON, LATTICE, PANE, WOOD};
use engine::prelude::*;
use std::f32::consts::{PI, TAU};

/// A hoop or band round the y axis at height `y`: `r` its inner radius.
fn band(
    f: &mut Form,
    y: f32,
    r: f32,
    proud: f32,
    half: f32,
    sides: usize,
    paint: Vec3,
) -> EngineResult<()> {
    f.lathe(
        Vec3::ZERO,
        Vec3::Y,
        &[
            (r - 0.004, y - half),
            (r + proud, y - half * 0.8),
            (r + proud, y + half * 0.8),
            (r - 0.004, y + half),
        ],
        sides,
        &|_, _| 1.0,
        &|_, _| (paint, 0.0),
    )
}

/// An umbral jar of foamed stone in two metal bands, stopped with a lit
/// bead: squat (0) or tall (1).
pub fn jar(variant: u32, detail: u32) -> EngineResult<Form> {
    let mut f = Form::new();
    let (h, r) = if variant % 2 == 1 {
        (0.95, 0.2)
    } else {
        (0.58, 0.25)
    };
    let foam = Vec3::new(0.56, 0.55, 0.5).lerp(Vec3::new(0.46, 0.54, 0.58), 0.5 * variant as f32);
    let sides = fine(detail, 24, 12, 7);
    // Foot, belly, shoulder, neck, lip, and down into the mouth.
    let full = [
        (0.0, 0.0),
        (0.56, 0.0),
        (0.74, 0.04),
        (0.93, 0.2),
        (1.0, 0.42),
        (0.94, 0.62),
        (0.72, 0.8),
        (0.46, 0.88),
        (0.42, 0.93),
        (0.52, 0.985),
        (0.47, 1.0),
        (0.36, 0.99),
        (0.0, 0.95),
    ];
    let plain = [
        (0.0, 0.0),
        (0.6, 0.0),
        (1.0, 0.4),
        (0.9, 0.66),
        (0.45, 0.9),
        (0.5, 1.0),
        (0.0, 1.0),
    ];
    let profile: Vec<(f32, f32)> = if detail == 0 { &full[..] } else { &plain[..] }
        .iter()
        .map(|&(a, b)| (a * r, b * h))
        .collect();
    f.surface = FOAM;
    f.lathe(
        Vec3::ZERO,
        Vec3::Y,
        &profile,
        sides,
        &|_, _| 1.0,
        &|ring, _| {
            (
                foam * (0.82 + 0.18 * (ring as f32 / profile.len() as f32)),
                0.0,
            )
        },
    )?;
    if solid(detail) {
        return Ok(f);
    }
    if detail < 2 {
        let metal = Vec3::new(0.1, 0.19, 0.16);
        f.surface = LATTICE;
        band(&mut f, 0.42 * h, r, 0.012, 0.016, sides, metal)?;
        band(&mut f, 0.66 * h, 0.9 * r, 0.012, 0.014, sides, metal)?;
    }
    f.surface = PANE;
    f.drop(
        Vec3::Y * 0.93 * h,
        Vec3::Y,
        0.3 * r,
        0.5 * r,
        fine(detail, 12, 6, 4),
        (Vec3::new(0.55, 1.0, 0.8), 1.6),
    )?;
    Ok(f)
}

/// A dwarven cask of dark staves in iron hoops: a barrel (0) or a keg (1).
pub fn cask(variant: u32, detail: u32) -> EngineResult<Form> {
    let mut f = Form::new();
    let (h, r) = if variant % 2 == 1 {
        (0.62, 0.25)
    } else {
        (0.96, 0.36)
    };
    let sides = fine(detail, 28, 14, 8);
    let rings = fine(detail, 9, 5, 3);
    let girth = |t: f32| r * (0.84 + 0.16 * (t * PI).sin());
    let mut profile = vec![
        (0.0, 0.035),
        (girth(0.0) - 0.03, 0.035),
        (girth(0.0) - 0.03, 0.0),
    ];
    for i in 0..=rings {
        let t = i as f32 / rings as f32;
        profile.push((girth(t), t * h));
    }
    profile.extend([
        (girth(1.0) - 0.03, h),
        (girth(1.0) - 0.03, h - 0.035),
        (0.0, h - 0.035),
    ]);
    f.surface = WOOD;
    let staves = 14.0;
    f.lathe(
        Vec3::ZERO,
        Vec3::Y,
        &profile,
        sides,
        &|_, _| 1.0,
        &|_, angle| {
            // Each stave its own shade.
            let stave = (angle / TAU * staves) as u32;
            (
                Vec3::new(0.13, 0.08, 0.05) * (0.75 + 0.5 * hash(stave + variant * 31, 9)),
                0.0,
            )
        },
    )?;
    if solid(detail) || detail == 2 {
        return Ok(f);
    }
    f.surface = IRON;
    let iron = Vec3::new(0.07, 0.07, 0.08);
    for t in [0.1, 0.3, 0.7, 0.9] {
        band(&mut f, t * h, girth(t), 0.012, 0.028, sides, iron)?;
    }
    Ok(f)
}

/// A goblin crate nailed up from planks: square (0) or long (1).
pub fn crate_box(variant: u32, detail: u32) -> EngineResult<Form> {
    let mut f = Form::new();
    let half = if variant % 2 == 1 {
        Vec3::new(0.46, 0.24, 0.27)
    } else {
        Vec3::new(0.31, 0.3, 0.31)
    };
    let frame = [Vec3::X, Vec3::Y, Vec3::Z];
    let middle = Vec3::Y * half.y;
    let woods = [
        Vec3::new(0.2, 0.13, 0.08),
        Vec3::new(0.27, 0.2, 0.13),
        Vec3::new(0.12, 0.09, 0.07),
    ];
    f.surface = WOOD;
    if detail >= 2 {
        f.cuboid(middle, frame, half, (woods[0], 0.0))?;
        return Ok(f);
    }
    if detail == 1 {
        // From further off: the box whole, with its battens.
        f.cuboid(
            middle,
            frame,
            half - Vec3::splat(0.005),
            (woods[1] * 0.9, 0.0),
        )?;
    } else {
        // The dark inside, seen through the gaps.
        f.cuboid(
            middle,
            frame,
            half - Vec3::splat(0.022),
            (Vec3::splat(0.015), 0.0),
        )?;
    }
    let thick = 0.011;
    for axis in (0..3).filter(|_| detail == 0) {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        // Planks run along the longer of the face's two sides.
        let (along, across) = if half[u] >= half[v] { (u, v) } else { (v, u) };
        let planks = (half[across] / 0.095).round().max(2.0) as usize;
        for sign in [-1.0_f32, 1.0] {
            for k in 0..planks {
                let key = variant * 97 + (axis * 40 + k) as u32 + if sign > 0.0 { 500 } else { 0 };
                let mut center = middle;
                center[axis] += sign * (half[axis] - thick);
                center[across] += ((k as f32 + 0.5) / planks as f32 * 2.0 - 1.0) * half[across];
                // By eye: no plank quite as long or as straight as the next.
                center[along] += 0.012 * (hash(key, 1) - 0.5);
                let mut size = Vec3::ZERO;
                size[axis] = thick;
                size[across] = half[across] / planks as f32 * (0.86 + 0.08 * hash(key, 2));
                size[along] = half[along] * (0.985 + 0.03 * hash(key, 3));
                let shade = woods[(hash(key, 4) * 3.0) as usize % 3] * (0.8 + 0.4 * hash(key, 5));
                f.cuboid(center, frame, size, (shade, 0.0))?;
            }
        }
    }
    // Battens up the corners, a little proud.
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let center = middle + Vec3::new(sx * (half.x - 0.022), 0.0, sz * (half.z - 0.022));
        f.cuboid(
            center,
            frame,
            Vec3::new(0.034, half.y + 0.008, 0.034),
            (woods[2] * 0.9, 0.0),
        )?;
    }
    Ok(f)
}

/// A goblin sack, full and tied at the neck.
pub fn sack(variant: u32, detail: u32) -> EngineResult<Form> {
    let mut f = Form::new();
    let (h, r) = (0.62 + 0.14 * (variant % 2) as f32, 0.27);
    let cloth = [Vec3::new(0.25, 0.2, 0.07), Vec3::new(0.2, 0.17, 0.13)][variant as usize % 2];
    let sides = fine(detail, 22, 11, 6);
    let full = [
        (0.0, 0.0),
        (0.62, 0.0),
        (0.9, 0.06),
        (1.0, 0.2),
        (1.0, 0.42),
        (0.86, 0.62),
        (0.5, 0.78),
        (0.27, 0.84),
        (0.22, 0.87),
        (0.3, 0.92),
        (0.4, 1.0),
        (0.0, 0.93),
    ];
    let plain = [
        (0.0, 0.0),
        (0.8, 0.0),
        (1.0, 0.3),
        (0.8, 0.65),
        (0.25, 0.86),
        (0.0, 1.0),
    ];
    let profile: Vec<(f32, f32)> = if detail == 0 { &full[..] } else { &plain[..] }
        .iter()
        .map(|&(a, b)| (a * r, b * h))
        .collect();
    let last = profile.len() as f32;
    let seed = variant as f32 * 1.7;
    f.surface = CLOTH;
    f.lathe(
        Vec3::ZERO,
        Vec3::Y,
        &profile,
        sides,
        &|ring, angle| {
            // Lumps of what is in it below, pleats where it is tied.
            let up = ring as f32 / last;
            let lumps = 0.07 * (angle * 3.0 + ring as f32 * 1.3 + seed).sin()
                + 0.04 * (angle * 5.0 - ring as f32 + seed).cos();
            let pleats = 0.12 * (angle * 9.0).cos();
            1.0 + lumps * (1.0 - up) + pleats * (up * up)
        },
        &|ring, angle| {
            let fold = 0.85 + 0.15 * (angle * 9.0).cos() * (ring as f32 / last);
            (cloth * fold, 0.0)
        },
    )?;
    if solid(detail) || detail == 2 {
        return Ok(f);
    }
    // The cord round the neck.
    band(
        &mut f,
        0.86 * h,
        0.24 * r,
        0.02,
        0.014,
        sides,
        Vec3::new(0.3, 0.26, 0.16),
    )?;
    Ok(f)
}

/// One kind of goods: its drawn set, where each stands, its shape.
pub struct Kind {
    pub set: LodId,
    pub places: Vec<Place>,
    /// The solid shape at each place, while it stands still.
    pub solids: Vec<Option<EntityId>>,
    pub mesh: Mesh,
}

pub struct Goods {
    kinds: Vec<Kind>,
    /// Per kind and place: is it loose (a body of its own).
    loose: Vec<Vec<bool>>,
    /// Every one of them: its kind and its place among that kind.
    index: Vec<(usize, usize)>,
    /// Kinds of which some came loose since they were last drawn.
    changed: Vec<bool>,
    material: CustomMaterialId,
    pub loose_total: usize,
    /// The body of each that came loose (by its place in `index`).
    bodies: Vec<Option<EntityId>>,
}

impl Goods {
    pub fn new(kinds: Vec<Kind>, material: CustomMaterialId) -> Self {
        let loose = kinds.iter().map(|k| vec![false; k.places.len()]).collect();
        let index = kinds
            .iter()
            .enumerate()
            .flat_map(|(k, kind)| (0..kind.places.len()).map(move |n| (k, n)))
            .collect();
        Self {
            changed: vec![false; kinds.len()],
            kinds,
            loose,
            index,
            material,
            loose_total: 0,
            bodies: Vec::new(),
        }
        .sized()
    }

    fn sized(mut self) -> Self {
        self.bodies = vec![None; self.index.len()];
        self
    }

    /// The goods standing at `p` (within half a metre).
    pub fn item_at(&self, p: Vec3) -> Option<usize> {
        self.index
            .iter()
            .position(|&(k, n)| self.kinds[k].places[n].position.distance(p) < 0.5)
    }

    /// Where goods `i` are now: where they stand, or where their body is.
    pub fn where_is(&self, world: &World, i: usize) -> Option<Vec3> {
        let (k, n) = *self.index.get(i)?;
        match self.bodies[i] {
            Some(body) => world
                .world_transform(body)
                .ok()
                .map(|m| m.w_axis.truncate()),
            None => Some(self.kinds[k].places[n].position),
        }
    }

    /// Where the goods are that came loose and lie still.
    pub fn resting_loose(&self, world: &World) -> Vec<Vec3> {
        self.bodies
            .iter()
            .flatten()
            .filter(|&&b| world.body_velocity(b).is_none_or(|v| v.length() < 0.5))
            .filter_map(|&b| world.world_transform(b).ok())
            .map(|m| m.w_axis.truncate())
            .collect()
    }

    /// Have goods `i` come loose.
    pub fn is_loose(&self, i: usize) -> bool {
        self.bodies.get(i).is_some_and(|b| b.is_some())
    }
}

impl Movable for Goods {
    fn count(&self) -> usize {
        self.index.len()
    }

    fn holds_at(&self, i: usize) -> Option<Vec3> {
        let (k, n) = self.index[i];
        (!self.loose[k][n]).then(|| self.kinds[k].places[n].position)
    }

    /// Goods only stand there: the friction of a crate on rock, the same
    /// as their body has once loose.
    fn grip(&self, _pull: Vec3) -> f32 {
        grav_core::ROCK_FRICTION
    }

    fn let_go(&mut self, world: &mut World, i: usize) -> EngineResult<()> {
        let (k, n) = self.index[i];
        self.loose[k][n] = true;
        self.loose_total += 1;
        self.changed[k] = true;
        if let Some(solid) = self.kinds[k].solids[n].take() {
            world.despawn(solid);
        }
        let id = world.spawn(self.kinds[k].mesh.clone());
        self.bodies[i] = Some(id);
        world.set_place(id, self.kinds[k].places[n])?;
        world.set_custom_material(id, self.material)?;
        movable::loose_body(world, id, 0.6)
    }

    /// Draw what still stands of the kinds that lost some.
    fn settle(&mut self, world: &mut World) -> EngineResult<()> {
        for k in 0..self.kinds.len() {
            if !std::mem::take(&mut self.changed[k]) {
                continue;
            }
            let kind = &self.kinds[k];
            let still: Vec<Place> = (0..kind.places.len())
                .filter(|&n| !self.loose[k][n])
                .map(|n| kind.places[n])
                .collect();
            world.set_lod_places(kind.set, &still)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "full"]
    fn goods_are_small_things_with_a_close_up_shape() {
        for (name, shape) in [
            ("jar", jar as fn(u32, u32) -> EngineResult<Form>),
            ("cask", cask),
            ("crate", crate_box),
            ("sack", sack),
        ] {
            for variant in 0..2 {
                let f = shape(variant, 0).unwrap();
                let (lo, hi) = f.mesh.bounds().unwrap();
                assert!(lo.y > -0.02 && hi.y < 1.1, "{name} {variant}: {lo} {hi}");
                let n = f.mesh.face_count();
                assert!((250..2500).contains(&n), "{name} {variant}: {n} faces");
            }
        }
    }
}
