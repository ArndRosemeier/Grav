//! Water: 3D fluid boxes (engine `Fluid`) that follow the field, stones
//! included. A pool on the spire near the start, and a hanging lake held
//! against the underside of the island by its vein.
//!
//! Each body of water sits in a stone basin built in a surface frame
//! (local +y away from the rock), so the same code makes a pool on top of
//! the spire and a lake upside down under the island.

use crate::space::{self, place};
use engine::glam::Vec2;
use engine::prelude::*;

/// Rim thickness of a basin.
const RIM: f32 = 0.6;
/// Water further than this from the hero sleeps.
const AWAKE_DISTANCE: f32 = 120.0;

/// A stone basin of water on a surface.
pub struct Basin {
    pub fluid: FluidId,
    /// The basin's middle on the rock surface, and its frame.
    pub at: Vec3,
    pub rot: Quat,
}

pub struct Waters {
    pub pool: Basin,
    pub lake: Basin,
}

/// Plan of one basin.
struct Plan {
    at: Vec3,
    rot: Quat,
    /// Inner half size across the surface (local x, z).
    half: Vec2,
    /// Rim height above the surface.
    rim: f32,
    /// Water depth to fill.
    depth: f32,
    cell: f32,
    color: Color,
}

fn build_basin(world: &mut World, plan: &Plan, rock: impl Fn(Vec3) -> bool) -> EngineResult<Basin> {
    let (at, rot) = (plan.at, plan.rot);
    let inv = rot.inverse();
    let local = move |p: Vec3| inv * (p - at);
    let outer = plan.half + Vec2::splat(RIM);
    // Rim walls, sunk 2 m into the rock so a curved surface leaves no gap.
    let wall_height = plan.rim + 2.0;
    let wall_mid = plan.rim - wall_height * 0.5;
    let walls = [
        (
            Vec3::new(plan.half.x + RIM * 0.5, wall_mid, 0.0),
            Vec3::new(RIM, wall_height, outer.y * 2.0),
        ),
        (
            Vec3::new(-plan.half.x - RIM * 0.5, wall_mid, 0.0),
            Vec3::new(RIM, wall_height, outer.y * 2.0),
        ),
        (
            Vec3::new(0.0, wall_mid, plan.half.y + RIM * 0.5),
            Vec3::new(plan.half.x * 2.0, wall_height, RIM),
        ),
        (
            Vec3::new(0.0, wall_mid, -plan.half.y - RIM * 0.5),
            Vec3::new(plan.half.x * 2.0, wall_height, RIM),
        ),
    ];
    for (centre, size) in walls {
        let e = world.spawn(Shape::box_at((0.0, 0.0, 0.0), size, rgb(104, 98, 106))?);
        world.set_place(e, place(at + rot * centre, rot))?;
        world.add_body(e, Body::fixed().shape(BodyShape::Box(size * 0.5)))?;
    }
    let in_rim = move |l: Vec3| {
        l.y < plan.rim
            && l.x.abs() < outer.x
            && l.z.abs() < outer.y
            && (l.x.abs() >= plan.half.x || l.z.abs() >= plan.half.y)
    };
    // The box: every corner of the basin plus head room, in world axes.
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for x in [-outer.x, outer.x] {
        for y in [-1.5, plan.rim + 3.0] {
            for z in [-outer.y, outer.y] {
                let p = at + rot * Vec3::new(x, y, z);
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
    }
    let mut fluid = Fluid::new(
        FluidSettings::new(lo, hi - lo, plan.cell)
            .color(plan.color)
            .mesh_every(2),
    )?;
    fluid.mark_solid(|p| rock(p) || in_rim(local(p)));
    let (half, depth) = (plan.half, plan.depth);
    fluid.fill(|p| {
        let l = local(p);
        l.x.abs() < half.x && l.z.abs() < half.y && l.y < depth && l.y > -1.5
    });
    let id = world.add_fluid(fluid)?;
    Ok(Basin { fluid: id, at, rot })
}

pub fn build(world: &mut World) -> EngineResult<Waters> {
    // A pool on the spire a little to the side of where the hero starts.
    let (at, rot) = space::spire_frame(16.0, -78.0);
    // The basin's floor is the real rock (what is drawn is what holds water).
    let spire_rock = |p: Vec3| crate::rock::shared().in_spire(p);
    let pool = build_basin(
        world,
        &Plan {
            at,
            rot,
            half: Vec2::new(3.5, 3.5),
            rim: 1.2,
            depth: 0.9,
            cell: 0.3,
            color: rgba(45, 110, 150, 190),
        },
        spire_rock,
    )?;
    // The hanging lake: under the island, held up by its vein.
    let (at, rot) = space::island_frame(Vec3::NEG_Y);
    let island_rock = |p: Vec3| crate::rock::shared().in_island(p);
    let lake = build_basin(
        world,
        &Plan {
            at,
            rot,
            half: Vec2::new(4.0, 4.0),
            rim: 1.6,
            depth: 1.2,
            cell: 0.35,
            color: rgba(70, 120, 170, 180),
        },
        island_rock,
    )?;
    Ok(Waters { pool, lake })
}

impl Waters {
    /// Wake water near the hero, put far water to sleep.
    pub fn update(&self, world: &mut World, hero_at: Vec3) -> EngineResult<()> {
        for b in [&self.pool, &self.lake] {
            world.set_fluid_awake(b.fluid, b.at.distance(hero_at) < AWAKE_DISTANCE)?;
        }
        Ok(())
    }

    pub fn volume(&self, world: &World) -> f32 {
        [&self.pool, &self.lake]
            .iter()
            .filter_map(|b| world.fluid(b.fluid))
            .map(|f| f.volume())
            .sum()
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
        let (at, rot) = space::spire_frame(16.0, -78.0);
        let a = field.acceleration(&BodyQuery::at(at + rot * Vec3::Y * 0.5));
        assert!(a.normalize().dot(-(rot * Vec3::Y)) > 0.95, "{a}");
    }

    #[test]
    fn the_hanging_lake_stays_in_its_basin() {
        let mut world = World::new();
        world
            .set_physics_settings(PhysicsSettings {
                ground: PhysicsGround::None,
                ..PhysicsSettings::default()
            })
            .unwrap();
        let waters = build(&mut world).unwrap();
        crate::link::install(&mut world, &space::base_field(), Default::default());
        let start = world.fluid(waters.lake.fluid).unwrap().volume();
        for _ in 0..180 {
            world.tick_physics(1.0 / 60.0);
        }
        let lake = world.fluid(waters.lake.fluid).unwrap();
        assert!(
            lake.volume() > start * 0.97,
            "lost {} of {start}",
            start - lake.volume()
        );
        // Its water stays against the rock, inside the rim (1.6 m high).
        let rot = waters.lake.rot;
        let deepest = lake
            .particles()
            .map(|p| (rot.inverse() * (p - waters.lake.at)).y)
            .fold(f32::MIN, f32::max);
        assert!(deepest < 1.6, "water hangs {deepest} m below the rock");
    }
}
