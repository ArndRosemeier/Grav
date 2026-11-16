//! Plumb grass: thin long stalks that grow almost everywhere, a few to a
//! clump and the clumps far apart, and always stand straight against the
//! pull. It is the world's own plumb line: where down turns, the grass
//! shows it, and where a stone is open the grass around it points along
//! the new pull at once (Arnd, 2026-11-16).
//!
//! The clumps are kept in patches of ground. A patch near the eye is
//! drawn fine, further off light, far away not at all; a patch in reach
//! of an open stone has its stalks turned to the field every frame.

use crate::flora::{fine, hash, Form};
use crate::space;
use engine::prelude::*;
use grav_core::{BodyQuery, Field};
use std::collections::HashMap;

/// One clump: where it roots, how tall it is, how it is turned about its
/// own axis.
#[derive(Clone, Copy, Debug)]
pub struct Blade {
    pub at: Vec3,
    pub size: f32,
    pub turn: f32,
}

/// The side of a patch, in metres.
const PATCH: f32 = 48.0;
/// Patches nearer than this are drawn fine, then light up to `FAR`.
const NEAR: f32 = 60.0;
const FAR: f32 = 170.0;

struct Patch {
    center: Vec3,
    blades: Vec<Blade>,
    /// The places in the world's own pull, with no stone open.
    rest: Vec<Place>,
    fine: EntityId,
    light: EntityId,
    /// 0 drawn fine, 1 light, 2 not drawn; 3 not yet set.
    shown: u8,
    bent: bool,
}

pub struct Grass {
    patches: Vec<Patch>,
}

/// A clump of stalks along +y: each a thin straight wire, dark at the
/// foot and pale above, with a knot or two on it and a slender lit seed
/// at its tip.
pub fn clump(detail: u32) -> EngineResult<Mesh> {
    let mut f = Form::new();
    let stalks = fine(detail, 5, 3, 3);
    for s in 0..stalks {
        let k = 4000 + s as u32;
        let turn = (s as f32 + hash(k, 1)) / stalks as f32 * std::f32::consts::TAU;
        let out = Vec3::new(turn.cos(), 0.0, turn.sin());
        let foot = out * 0.05 * hash(k, 2) - Vec3::Y * 0.06;
        // Straight, and nearly parallel: the clump is a plumb line.
        let dir = (Vec3::Y + out * 0.035 * hash(k, 3)).normalize();
        let length = 0.75 + 0.75 * hash(k, 4);
        let thick = if detail == 0 { 0.009 } else { 0.017 };
        f.tube(
            &[foot, foot + dir * length * 0.5, foot + dir * length],
            fine(detail, 5, 3, 3),
            &|t| thick * (1.0 - 0.55 * t),
            &|t| {
                let c = Vec3::new(0.03, 0.06, 0.06)
                    .lerp(Vec3::new(0.5, 0.72, 0.58), (t * 1.4).min(1.0));
                (c, 0.12 * t)
            },
        )?;
        if detail == 0 {
            for knot in 0..2 {
                let t = 0.35 + 0.3 * knot as f32 + 0.1 * hash(k, 5 + knot);
                f.drop(
                    foot + dir * (length * t - 0.02),
                    dir,
                    0.016,
                    0.045,
                    6,
                    (Vec3::new(0.4, 0.62, 0.5), 0.15),
                )?;
            }
        }
        f.drop(
            foot + dir * (length - 0.01),
            dir,
            if detail == 0 { 0.017 } else { 0.026 },
            0.16,
            fine(detail, 8, 4, 4),
            (Vec3::new(0.8, 1.0, 0.86), 0.55),
        )?;
    }
    Ok(f.mesh)
}

/// Where a clump stands in `field`: rooted at its place, straight
/// against the pull there.
fn stand(blade: &Blade, field: &Field) -> Place {
    let pull = field.acceleration(&BodyQuery::at(blade.at));
    let up = (-pull).normalize_or(Vec3::Y);
    let rot = Quat::from_rotation_arc(Vec3::Y, up) * Quat::from_rotation_y(blade.turn);
    space::place(blade.at, rot).with_scale(blade.size)
}

impl Grass {
    /// Plant `blades`. `base` is the world's own field, with no stone open.
    pub fn new(
        world: &mut World,
        material: CustomMaterialId,
        blades: &[Blade],
        base: &Field,
    ) -> EngineResult<Self> {
        let fine_shape = world.spawn_instanced(clump(0)?);
        let light_shape = world.spawn_instanced(clump(1)?);
        for shape in [fine_shape, light_shape] {
            world.set_custom_material(shape, material)?;
            world.set_casts_shadow(shape, false)?;
        }
        let mut by_patch: HashMap<(i32, i32, i32), Vec<Blade>> = HashMap::new();
        for blade in blades {
            let q = (blade.at / PATCH).floor();
            by_patch
                .entry((q.x as i32, q.y as i32, q.z as i32))
                .or_default()
                .push(*blade);
        }
        let mut keys: Vec<_> = by_patch.keys().copied().collect();
        keys.sort();
        let mut patches = Vec::with_capacity(keys.len());
        for key in keys {
            let blades = by_patch.remove(&key).unwrap_or_default();
            let center = blades.iter().map(|b| b.at).sum::<Vec3>() / blades.len().max(1) as f32;
            let rest = blades.iter().map(|b| stand(b, base)).collect();
            patches.push(Patch {
                center,
                blades,
                rest,
                fine: world.spawn_instanced_like(fine_shape)?,
                light: world.spawn_instanced_like(light_shape)?,
                shown: 3,
                bent: false,
            });
        }
        Ok(Self { patches })
    }

    /// How many clumps stand.
    pub fn count(&self) -> usize {
        self.patches.iter().map(|p| p.blades.len()).sum()
    }

    /// Each frame: draw the patches near `eye`, and turn the stalks in
    /// reach of an open stone to `field`.
    pub fn update(&mut self, world: &mut World, field: &Field, eye: Vec3) -> EngineResult<()> {
        for patch in &mut self.patches {
            let d = patch.center.distance(eye);
            // A little slack, so a patch on the line does not flicker.
            let shown = match patch.shown {
                0 if d < NEAR + 6.0 => 0,
                1 if d > NEAR - 6.0 && d < FAR + 10.0 => 1,
                2 if d > FAR - 10.0 => 2,
                _ if d < NEAR => 0,
                _ if d < FAR => 1,
                _ => 2,
            };
            let touched = shown < 2
                && field.sources.iter().any(|s| {
                    s.opening > 0.0 && s.position.distance(patch.center) < s.radius + PATCH
                });
            if shown == patch.shown && !touched && !patch.bent {
                continue;
            }
            let bent: Vec<Place>;
            let places: &[Place] = if touched {
                bent = patch.blades.iter().map(|b| stand(b, field)).collect();
                &bent
            } else {
                &patch.rest
            };
            world.set_instances(patch.fine, if shown == 0 { places } else { &[] })?;
            world.set_instances(patch.light, if shown == 1 { places } else { &[] })?;
            patch.shown = shown;
            patch.bent = touched;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grass_stands_against_the_pull_and_turns_to_an_open_stone() {
        let base = space::base_field();
        // On top of the spire the pull is towards its axis: up is +y.
        let blade = Blade {
            at: Vec3::new(0.0, 50.5, 0.0),
            size: 1.0,
            turn: 0.3,
        };
        let up = |field: &Field| {
            let p = stand(&blade, field);
            let m = p.to_matrix();
            m.transform_vector3(Vec3::Y).normalize()
        };
        assert!(up(&base).dot(Vec3::Y) > 0.98, "{}", up(&base));
        // An attracting stone beside it, open: the grass leans away from it
        // (it points against the pull, and the pull is now partly sideways).
        let mut with = base.clone();
        with.sources.push(grav_core::Source::new(
            900,
            grav_core::Charge::Attracting,
            Vec3::new(4.0, 50.5, 0.0),
            3.0,
            15.0,
        ));
        assert!(up(&with).x < -0.3, "{}", up(&with));
    }

    #[test]
    fn a_clump_is_light() {
        assert!(clump(0).unwrap().face_count() < 900);
        assert!(clump(1).unwrap().face_count() < 120);
    }
}
