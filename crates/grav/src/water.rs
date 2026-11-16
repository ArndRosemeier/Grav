//! Water you can move: 3D fluid boxes (engine `Fluid`) that follow the
//! field, stones included. For now the town's spring basin; other standing
//! water is drawn from the water map (`wet.rs`).

use crate::space;
use engine::prelude::*;

/// Water further than this from the hero sleeps.
const AWAKE_DISTANCE: f32 = 120.0;
/// How long water goes on moving after the last thing stirred it, and how
/// far beyond the basin's rim the hero counts as stirring it.
const SETTLE_SECONDS: f32 = 6.0;
const SETTLE_MARGIN: f32 = 3.0;

/// A body of simulated water in a basin of the rock.
pub struct Basin {
    pub fluid: FluidId,
    /// The basin's middle on the rock surface, and its frame.
    pub at: Vec3,
    pub rot: Quat,
}

pub struct Waters {
    pub pool: Basin,
    /// Seconds since something last stirred the pool.
    calm: f32,
}

/// The town's spring: its basin is a hollow in the spire's own rock
/// (`rock.rs`), filled to the level of the ground around it.
fn build_spring(world: &mut World) -> EngineResult<Basin> {
    use crate::rock::{SPRING_AT, SPRING_BASIN_RADIUS};
    let (at, rot) = space::spire_frame(SPRING_AT.0, SPRING_AT.1);
    let reach = SPRING_BASIN_RADIUS + 2.0;
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for x in [-reach, reach] {
        for y in [-1.0, 5.5] {
            for z in [-reach, reach] {
                let p = at + rot * Vec3::new(x, y, z);
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
    }
    let mut fluid = Fluid::new(
        FluidSettings::new(lo, hi - lo, 0.3)
            .color(rgba(16, 44, 58, 205))
            .mesh_every(2),
    )?;
    let rock = crate::rock::shared();
    fluid.mark_solid(|p| rock.in_spire(p));
    // Up to a hand below the ground around the basin: the spire's settled
    // surface is where the water's own level lies.
    let level = space::SPIRE_RADIUS - 0.15;
    fluid.fill(|p| {
        p.truncate().length() < level
            && p.distance(at) < SPRING_BASIN_RADIUS + 1.0
            && !rock.in_spire(p)
    });
    let id = world.add_fluid(fluid)?;
    Ok(Basin { fluid: id, at, rot })
}

pub fn build(world: &mut World) -> EngineResult<Waters> {
    // The spring basin in the town, a little to the side of where the
    // hero starts.
    let pool = build_spring(world)?;
    Ok(Waters { pool, calm: 0.0 })
}

impl Waters {
    /// Water is simulated while something stirs it and for a while
    /// after, until it has come to rest: the hero at its edge or in it,
    /// or an open stone whose field reaches it. Still water sleeps (the
    /// simulation is the most costly thing in a frame: about 9 ms on
    /// Arnd's desktop, measured 2026-11-16).
    pub fn update(
        &mut self,
        world: &mut World,
        hero_at: Vec3,
        field: &grav_core::Field,
        dt: f32,
    ) -> EngineResult<()> {
        let reach = crate::rock::SPRING_BASIN_RADIUS + SETTLE_MARGIN;
        let at = self.pool.at;
        let stirred = at.distance(hero_at) < reach
            || field
                .sources
                .iter()
                .any(|s| s.opening > 0.0 && s.position.distance(at) < s.radius + reach);
        self.calm = if stirred { 0.0 } else { self.calm + dt };
        let awake = at.distance(hero_at) < AWAKE_DISTANCE
            && self.calm < SETTLE_SECONDS
            && std::env::var("GRAV_WATER_ASLEEP").is_err();
        world.set_fluid_awake(self.pool.fluid, awake)
    }

    pub fn volume(&self, world: &World) -> f32 {
        world.fluid(self.pool.fluid).map_or(0.0, |f| f.volume())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grav_core::BodyQuery;

    #[test]
    #[ignore = "full"]
    fn the_island_holds_its_lake_up() {
        // Under the island its vein outweighs the heart's push: water there
        // falls up, into the basin.
        let field = space::base_field();
        let (at, rot) = space::island_frame(Vec3::NEG_Y);
        let p = at + rot * Vec3::new(0.0, 0.5, 0.0);
        let a = field.acceleration(&BodyQuery::at(p));
        let toward_rock = -(rot * Vec3::Y);
        assert!(a.dot(toward_rock) > 0.2 * grav_core::G, "{a}");
    }

    #[test]
    #[ignore = "full"]
    fn the_pool_has_down_into_the_spire() {
        let field = space::base_field();
        let (at, rot) = space::spire_frame(crate::rock::SPRING_AT.0, crate::rock::SPRING_AT.1);
        let a = field.acceleration(&BodyQuery::at(at + rot * Vec3::Y * 0.5));
        assert!(a.normalize().dot(-(rot * Vec3::Y)) > 0.95, "{a}");
    }
}
