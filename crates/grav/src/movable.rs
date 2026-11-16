//! Everything a pull can move (W5, F5): one rule for when a pull takes
//! hold of a thing, and one way of moving once it has.
//!
//! Everything that stands on rock feels the same forces: the field's pull,
//! and friction holding it against the part of that pull along the ground
//! (the hero adds his walking on top, which ends when his feet leave the
//! ground). The footing rule is grav-core's `footing_holds`, the same
//! one the hero's feet use (`slide_step`).
//!
//! A movable thing holds still (drawn in a set with others of its kind,
//! or walking its road) while friction holds it: the pull presses it onto
//! its ground and the pull along the ground is no more than its grip
//! times that press. Once that fails it is let go: it becomes a body in
//! the engine's physics, which moves every body by the same field
//! (link.rs) with the same friction. Goods, rubble and people are all
//! movable; a new kind of thing implements [`Movable`] and is moved like
//! the rest.

use engine::prelude::*;
use grav_core::{footing_holds, one_g, BodyQuery, Field, ROCK_FRICTION};

/// A kind of thing a pull can move, as many of it as there are.
pub trait Movable {
    /// How many there are.
    fn count(&self) -> usize;
    /// Where one is while it holds still (none: it is a body already, or
    /// gone).
    fn holds_at(&self, i: usize) -> Option<Vec3>;
    /// Its friction on rock under the pull `pull` (m/s²): the pull along
    /// the ground it withstands, as a share of the press onto it.
    fn grip(&self, pull: Vec3) -> f32;
    /// A pull has taken hold of it: it becomes a body.
    fn let_go(&mut self, world: &mut World, i: usize) -> EngineResult<()>;
    /// After a round of letting go (to draw what still holds, once).
    fn settle(&mut self, _world: &mut World) -> EngineResult<()> {
        Ok(())
    }
}

/// True when its footing no longer holds a thing standing at `at`: the
/// pull no longer presses it onto its ground, or pulls along the ground
/// harder than `grip` times the press. The ground under a standing thing
/// is settled rock, square to the world's own pull (`base`); `field` is
/// the same with the stones. `grip` gives the friction for a pull.
pub fn held(field: &Field, base: &Field, at: Vec3, grip: impl Fn(Vec3) -> f32) -> bool {
    let own = base.acceleration(&BodyQuery::at(at));
    let now = field.acceleration(&BodyQuery::at(at));
    if own.length() < 1e-3 {
        // No ground to speak of (nothing stands where nothing pulls).
        return now.length() > 0.05 * one_g();
    }
    !footing_holds(grip(now), now, -own.normalize())
}

/// The one rule: let go of everything in `things` that a pull has taken
/// hold of. Returns how many.
pub fn take_hold(
    things: &mut dyn Movable,
    world: &mut World,
    field: &Field,
    base: &Field,
) -> EngineResult<usize> {
    let open: Vec<(Vec3, f32)> = field
        .sources
        .iter()
        .filter(|s| s.opening > 0.0)
        .map(|s| (s.position, s.radius + 2.0))
        .collect();
    if open.is_empty() {
        return Ok(0);
    }
    let mut taken = 0;
    for i in 0..things.count() {
        let Some(at) = things.holds_at(i) else {
            continue;
        };
        // Only where a stone reaches is there anything to weigh.
        if open
            .iter()
            .any(|&(stone, reach)| stone.distance(at) < reach)
            && held(field, base, at, |pull| things.grip(pull))
        {
            things.let_go(world, i)?;
            taken += 1;
        }
    }
    if taken > 0 {
        things.settle(world)?;
    }
    Ok(taken)
}

/// Make `entity` a body that the field moves: solid by its own shape
/// (what is drawn is what collides), a sliver too small for that by its
/// box.
pub fn loose_body(world: &mut World, entity: EntityId, density: f32) -> EngineResult<()> {
    let body = Body::dynamic()
        .density(density)
        .friction(ROCK_FRICTION)
        .ccd(true);
    if world.add_body(entity, body.clone()).is_err() {
        world.add_body(entity, body.shape(BodyShape::BoundingBox))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space;

    #[test]
    fn a_pull_takes_hold_by_its_strength_against_the_grip() {
        let base = space::base_field();
        let at = Vec3::new(0.0, 50.5, 0.0);
        let mut with = base.clone();
        with.sources.push(grav_core::Source::new(
            900,
            grav_core::Charge::Attracting,
            at + Vec3::new(6.0, 3.0, 0.0),
            3.0,
            15.0,
        ));
        // No stone: nothing is held, whatever its grip.
        assert!(!held(&base, &base, at, |_| 0.01));
        // A strong stone near, pulling sideways and a little up: what
        // grips little slides off, what grips very much stays.
        assert!(held(&with, &base, at, |_| 0.6));
        assert!(!held(&with, &base, at, |_| 50.0));
        // A stone straight underfoot only presses harder: nothing slides.
        let mut under = base.clone();
        under.sources.push(grav_core::Source::new(
            901,
            grav_core::Charge::Attracting,
            at - Vec3::Y * 4.0,
            3.0,
            15.0,
        ));
        assert!(!held(&under, &base, at, |_| 0.05));
    }
}
