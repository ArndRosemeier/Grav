//! Water you can move: 3D fluid boxes (engine `Fluid`) that follow the
//! field, stones included. For now the town's spring basin; other standing
//! water is drawn from the water map (`wet.rs`).

use crate::space;
use engine::prelude::*;

/// Water further than this from the hero sleeps.
const AWAKE_DISTANCE: f32 = 120.0;

/// A body of simulated water in a basin of the rock.
pub struct Basin {
    pub fluid: FluidId,
    /// The basin's middle on the rock surface, and its frame.
    pub at: Vec3,
    pub rot: Quat,
}

pub struct Waters {
    pub pool: Basin,
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
    Ok(Waters { pool })
}

impl Waters {
    /// Wake water near the hero, put far water to sleep.
    pub fn update(&self, world: &mut World, hero_at: Vec3) -> EngineResult<()> {
        let near = self.pool.at.distance(hero_at) < AWAKE_DISTANCE;
        world.set_fluid_awake(self.pool.fluid, near)
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
    fn the_pool_has_down_into_the_spire() {
        let field = space::base_field();
        let (at, rot) = space::spire_frame(crate::rock::SPRING_AT.0, crate::rock::SPRING_AT.1);
        let a = field.acceleration(&BodyQuery::at(at + rot * Vec3::Y * 0.5));
        assert!(a.normalize().dot(-(rot * Vec3::Y)) > 0.95, "{a}");
    }
}
