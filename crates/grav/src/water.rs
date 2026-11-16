//! Water you can move: 3D fluid boxes (engine `Fluid`) that follow the
//! field, stones included: the town's spring basin and the lake that
//! hangs under the island. Other standing water is drawn from the water
//! map (`wet.rs`).

use crate::space;
use engine::prelude::*;

/// Water further than this from the hero sleeps.
const AWAKE_DISTANCE: f32 = 120.0;
/// How long water goes on moving after the last thing stirred it, and how
/// far beyond the basin's rim the hero counts as stirring it.
const SETTLE_SECONDS: f32 = 6.0;
const SETTLE_MARGIN: f32 = 3.0;

/// The simulated water's surface: the fluid's mesh is lumpy where its
/// particles lie, and Engine's reflections take the world's +y as up, so
/// a lake that hangs upside down mirrors sky in blotches. This calms the
/// surface towards the basin's own up (params[0].xyz) and stirs it with
/// a fine ripple instead.
const SURFACE: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let up = normalize(m.params[0].xyz);
    let n = normalize(m.normal);
    let face = up;
    let p = m.world_pos;
    let t = m.time;
    let ripple = vec3<f32>(
        sin(p.x * 3.1 + t * 1.3) + sin(p.z * 4.7 - t * 0.9),
        0.0,
        sin(p.z * 2.9 + t * 1.1) + sin(p.x * 5.3 + t * 0.7),
    ) * 0.035;
    o.normal = normalize(mix(n, face, 0.85) + (ripple - face * dot(ripple, face)));
    o.roughness = 0.1;
    return o;
}
"#;

/// Give a basin's water its surface (`SURFACE`), calmed towards `up`.
fn calm_surface(world: &mut World, fluid: FluidId, name: &str, up: Vec3) -> EngineResult<()> {
    let material = world.add_material(
        CustomMaterial::new(name, SURFACE)
            .param(0, [up.x, up.y, up.z, 0.0])
            .see_through(),
    )?;
    if let Some(e) = world.fluid_entity(fluid) {
        world.set_custom_material(e, material)?;
    }
    Ok(())
}

/// A body of simulated water in a basin of the rock.
pub struct Basin {
    pub fluid: FluidId,
    /// The basin's middle on the rock surface, and its frame.
    pub at: Vec3,
    pub rot: Quat,
    /// How far from `at` the water reaches.
    pub reach: f32,
    /// Seconds since something last stirred it.
    calm: f32,
}

pub struct Waters {
    pub pool: Basin,
    /// The hanging lake, once it has settled (on a worker thread: it takes
    /// a few seconds).
    pub lake: Option<Basin>,
    lake_job: Option<std::thread::JoinHandle<EngineResult<Fluid>>>,
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
    Ok(Basin {
        fluid: id,
        at,
        rot,
        reach: SPRING_BASIN_RADIUS,
        calm: 0.0,
    })
}

/// The lake under the island: its vein holds the water up in a bowl cut
/// into the underside, against the heart's push (A-rules: a hanging lake).
/// It is filled to a hand under the lip and left to find its level
/// before the game starts, so it hangs as the field has it, not as a
/// sphere. Runs on a worker thread.
fn settle_lake() -> EngineResult<Fluid> {
    use crate::rock::{LAKE_ANGLE, LAKE_DEPTH};
    use space::{ISLAND_CENTER, ISLAND_RADIUS};
    let across = lake_across();
    let lo = ISLAND_CENTER + Vec3::new(-across, -ISLAND_RADIUS - 1.5, -across);
    let hi = ISLAND_CENTER + Vec3::new(across, -ISLAND_RADIUS + LAKE_DEPTH + 1.0, across);
    let mut fluid = Fluid::new(
        FluidSettings::new(lo, hi - lo, 0.4)
            .color(rgba(18, 52, 66, 200))
            .mesh_every(2),
    )?;
    let rock = crate::rock::shared();
    fluid.mark_solid(|p| rock.in_island(p));
    let level = ISLAND_RADIUS - 0.25;
    fluid.fill(|p| {
        let d = p - ISLAND_CENTER;
        d.length() < level
            && d.normalize_or_zero().dot(Vec3::NEG_Y) > (LAKE_ANGLE * 0.95).cos()
            && !rock.in_island(p)
    });
    // Let it settle in the field it lives in, until it is still.
    let field = space::base_field();
    let pull = |p: Vec3| field.acceleration(&grav_core::BodyQuery::at(p));
    for _ in 0..LAKE_SETTLE_STEPS {
        fluid.step(1.0 / 30.0, pull);
    }
    Ok(fluid)
}

/// How far the lake's box reaches from its middle, each way.
fn lake_across() -> f32 {
    space::ISLAND_RADIUS * crate::rock::LAKE_ANGLE.sin() * 1.25
}

/// Steps of 1/30 s the lake is given to come to rest before it is shown.
const LAKE_SETTLE_STEPS: usize = 240;

pub fn build(world: &mut World) -> EngineResult<Waters> {
    // The spring basin in the town, a little to the side of where the
    // hero starts.
    let pool = build_spring(world)?;
    let lake_job = Some(std::thread::spawn(settle_lake));
    Ok(Waters {
        pool,
        lake: None,
        lake_job,
    })
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
        if std::env::var("GRAV_NO_LAKE").is_err()
            && self.lake_job.as_ref().is_some_and(|j| j.is_finished())
        {
            if let Some(job) = self.lake_job.take() {
                let fluid = job.join().map_err(|_| {
                    EngineError::InvalidValue("the hanging lake's worker panicked".into())
                })??;
                let (at, rot) = space::island_frame(Vec3::NEG_Y);
                let fluid = world.add_fluid(fluid)?;
                calm_surface(world, fluid, "hanging lake water", rot * Vec3::Y)?;
                self.lake = Some(Basin {
                    fluid,
                    at,
                    rot,
                    reach: lake_across(),
                    calm: SETTLE_SECONDS,
                });
            }
        }
        for basin in std::iter::once(&mut self.pool).chain(self.lake.as_mut()) {
            let reach = basin.reach + SETTLE_MARGIN;
            let at = basin.at;
            let stirred = at.distance(hero_at) < reach
                || field
                    .sources
                    .iter()
                    .any(|s| s.opening > 0.0 && s.position.distance(at) < s.radius + reach);
            basin.calm = if stirred { 0.0 } else { basin.calm + dt };
            let awake = at.distance(hero_at) < AWAKE_DISTANCE
                && basin.calm < SETTLE_SECONDS
                && std::env::var("GRAV_WATER_ASLEEP").is_err();
            world.set_fluid_awake(basin.fluid, awake)?;
        }
        Ok(())
    }

    /// The spring's water, in cubic metres.
    pub fn volume(&self, world: &World) -> f32 {
        world.fluid(self.pool.fluid).map_or(0.0, |f| f.volume())
    }

    /// The hanging lake's water, in cubic metres.
    pub fn lake_volume(&self, world: &World) -> f32 {
        self.lake
            .as_ref()
            .and_then(|l| world.fluid(l.fluid))
            .map_or(0.0, |f| f.volume())
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
