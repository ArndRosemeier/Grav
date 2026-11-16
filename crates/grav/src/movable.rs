//! Everything a pull can move (W5, F5): one rule for when a pull takes
//! hold of a thing, and one way of moving once it has.
//!
//! A movable thing holds still (drawn in a set with others of its kind,
//! or walking its road) until the pull of stones on it, measured against
//! the world's own pull there, is more than its grip. Then it is let go:
//! it becomes a body in the engine's physics, and physics moves every
//! body by the same field (link.rs). Goods, rubble and people are all
//! movable; a new kind of thing implements [`Movable`] and is moved like
//! the rest.

use engine::prelude::*;
use grav_core::{BodyQuery, Field};

/// A kind of thing a pull can move, as many of it as there are.
pub trait Movable {
    /// How many there are.
    fn count(&self) -> usize;
    /// Where one is while it holds still (none: it is a body already, or
    /// gone).
    fn holds_at(&self, i: usize) -> Option<Vec3>;
    /// How much pull of stones it withstands, as a share of the world's
    /// own pull where it is.
    fn grip(&self) -> f32;
    /// A pull has taken hold of it: it becomes a body.
    fn let_go(&mut self, world: &mut World, i: usize) -> EngineResult<()>;
    /// After a round of letting go (to draw what still holds, once).
    fn settle(&mut self, _world: &mut World) -> EngineResult<()> {
        Ok(())
    }
}

/// True when the stones' pull at `at` is more than `grip` allows.
/// `base` is the world's own field, `field` the same with the stones.
pub fn held(field: &Field, base: &Field, at: Vec3, grip: f32) -> bool {
    let own = base.acceleration(&BodyQuery::at(at));
    let now = field.acceleration(&BodyQuery::at(at));
    (now - own).length() > grip * own.length().max(1.0)
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
    let grip = things.grip();
    let mut taken = 0;
    for i in 0..things.count() {
        let Some(at) = things.holds_at(i) else {
            continue;
        };
        // Only where a stone reaches is there anything to weigh.
        if open
            .iter()
            .any(|&(stone, reach)| stone.distance(at) < reach)
            && held(field, base, at, grip)
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
    let body = Body::dynamic().density(density).ccd(true);
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
        assert!(!held(&base, &base, at, 0.01));
        // A strong stone near: what grips little is taken, what grips
        // much is not.
        assert!(held(&with, &base, at, 0.05));
        assert!(!held(&with, &base, at, 50.0));
    }
}
