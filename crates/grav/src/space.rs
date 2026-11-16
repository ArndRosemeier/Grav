//! The test space: one spire of dead stone growing out of the outer wall
//! and a floating island (design doc, "The core mechanics milestone").
//! Nothing is built on it yet: the first stand-in houses were torn down
//! and real buildings are to come.
//!
//! Districts in this space:
//! - the spire's surface: middle band, 1 g towards its vein, so down turns
//!   as you walk around it and the far side has the opposite down (R1);
//! - the outer wall's face: heavy band, 1.8 g;
//! - the spire root, where both meet: a seam (R4);
//! - the island above the tip: light band, 0.4 g;
//! - the open cavern between: only the heart's push (R3).

use crate::flora::Stuff;
use crate::look::Look;
use crate::rock;
use engine::glam::{EulerRot, Mat3};
use engine::prelude::*;
use grav_core::{Field, Heart, PieceId, Structure, Vein, VeinShape};

/// The spire's axis runs along +z through the origin.
pub const SPIRE_RADIUS: f32 = 50.0;
pub const SPIRE_END: f32 = 250.0;
/// The outer wall's face is the plane z = WALL_Z, facing +z.
pub const WALL_Z: f32 = -250.0;
/// How deep the wall's vein lies under its face. Rock settles where the
/// veins' summed potential is equal (`rock.rs`): at this depth the wall's
/// plane is on the same level as the spire's surface, so one settled
/// surface runs from the town down the spire's flare and out onto the wall.
/// (Spire: 20 m above its vein at 1 g; wall: 1.8 g, so 88 / 1.8 m.)
pub const WALL_VEIN_DEPTH: f32 = 48.89;
pub const ISLAND_CENTER: Vec3 = Vec3::new(0.0, 210.0, 170.0);
pub const ISLAND_RADIUS: f32 = 25.0;

/// The heart's centre: six kilometres inward along the way the spire grows,
/// well above it and a little to one side, so its light rakes down the
/// town's street and the island's shadow falls beside the town, not on it.
pub const HEART_CENTER: Vec3 = Vec3::new(1500.0, 2700.0, 5200.0);

/// Veins and heart for this space.
pub fn base_field() -> Field {
    let spire = Vein::new(
        "spire",
        vec![VeinShape::Capsule {
            a: Vec3::new(0.0, 0.0, -230.0),
            b: Vec3::new(0.0, 0.0, 220.0),
            radius: 30.0,
        }],
        1.0,
    );
    let mut wall = Vein::new(
        "outer wall",
        vec![VeinShape::Box {
            center: Vec3::new(0.0, 0.0, WALL_Z - WALL_VEIN_DEPTH - 15.0),
            rotation: Quat::IDENTITY,
            half: Vec3::new(400.0, 400.0, 15.0),
        }],
        1.8,
    );
    wall.hold = 70.0;
    let mut island = Vein::new(
        "island",
        vec![VeinShape::Sphere {
            center: ISLAND_CENTER,
            radius: 10.0,
        }],
        0.4,
    );
    island.hold = 40.0;
    island.fade = 60.0;
    Field {
        veins: vec![spire, wall, island],
        heart: Some(Heart {
            center: HEART_CENTER,
            stripped_radius: 3000.0,
            wall_radius: 9000.0,
            push_at_stripped: 0.3,
            push_at_wall: 0.02,
            push_scale: 1.0,
        }),
        sources: Vec::new(),
    }
}

/// A place from a position and a rotation.
pub fn place(at: Vec3, rot: Quat) -> Place {
    let (yaw, pitch, roll) = rot.to_euler(EulerRot::YXZ);
    Place::new(at.x, at.y, at.z)
        .with_yaw_deg(yaw.to_degrees())
        .with_pitch_deg(pitch.to_degrees())
        .with_roll_deg(roll.to_degrees())
}

/// The dead stone: spire, outer wall, island and the far spires, each with
/// its solid shape from the same mesh that is drawn.
/// What `build_rock` put into the world, kept so parts of it can be
/// taken away and put back (for measuring what each costs).
pub struct Scenery {
    pub blades: Vec<crate::grass::Blade>,
    pub plants: Vec<(LodId, Vec<Place>)>,
    pub built: Vec<(LodId, Vec<Place>)>,
    pub cavern: Vec<(LodId, Vec<Place>)>,
    /// The buildings by kind, for the town that comes apart (town.rs).
    pub town: Vec<crate::town::Kind>,
    /// The loose goods by kind (goods.rs).
    pub goods: Vec<crate::goods::Kind>,
}

impl Scenery {
    /// Draw one of the parts or not.
    pub fn show(
        &self,
        world: &mut World,
        part: &[(LodId, Vec<Place>)],
        on: bool,
    ) -> EngineResult<()> {
        for (set, places) in part {
            world.set_lod_places(*set, if on { places } else { &[] })?;
        }
        Ok(())
    }
}

pub fn build_rock(world: &mut World, look: &Look) -> EngineResult<Scenery> {
    let heart_dir = HEART_CENTER.normalize();
    let (pieces, stands, blades) = rock::build_all(rock::shared(), heart_dir)?;
    let mut scenery = Scenery {
        blades,
        plants: Vec::new(),
        built: Vec::new(),
        cavern: Vec::new(),
        town: Vec::new(),
        goods: Vec::new(),
    };
    // What grows on it: each shape of each kind once, drawn wherever one
    // stands, lighter with distance.
    for stand in stands {
        let material = match stand.stuff {
            Stuff::Flora => look.flora,
            Stuff::Built => look.built,
        };
        let set = world.spawn_lod_prepared(stand.levels)?;
        world.set_lod_places(set, &stand.places)?;
        for (i, level) in world.lod_levels(set)?.into_iter().enumerate() {
            world.set_custom_material(level, material)?;
            world.set_casts_shadow(level, stand.shadow && i < 2)?;
        }
        // What is drawn is what collides: the same shape, plain, unseen,
        // once for each place (only where the hero can come).
        let part = match stand.stuff {
            Stuff::Flora => &mut scenery.plants,
            Stuff::Built => &mut scenery.built,
        };
        part.push((set, stand.places.clone()));
        let mut solids = vec![None; stand.places.len()];
        if let Some(solid) = &stand.solid {
            for (i, place) in stand.places.iter().enumerate() {
                if place.position.truncate().length() > 420.0 {
                    continue;
                }
                let id = world.spawn(solid.clone());
                world.set_place(id, *place)?;
                world.set_custom_material(id, look.unseen)?;
                world.set_casts_shadow(id, false)?;
                world.add_body(id, Body::fixed().shape(BodyShape::Mesh))?;
                solids[i] = Some(id);
            }
        }
        if let Some(mesh) = stand.loose {
            scenery.goods.push(crate::goods::Kind {
                set,
                places: stand.places,
                solids,
                mesh,
            });
        } else if !stand.pieces.is_empty() {
            scenery.town.push(crate::town::Kind {
                set,
                places: stand.places,
                solids,
                solid: stand.solid,
                shards: stand.pieces,
            });
        }
    }
    for piece in pieces {
        let id = world.spawn_prepared(piece.mesh);
        world.set_custom_material(id, look.rock_material(piece.kind))?;
        if piece.solid {
            world.add_body(id, Body::fixed().shape(BodyShape::Mesh))?;
        }
        if piece.kind == rock::RockKind::Water {
            world.set_casts_shadow(id, false)?;
        }
    }
    // The rest of the cavern: scenery in level-of-detail sets.
    let count = std::env::var("GRAV_CAVERN_COUNT").is_ok();
    // Triangles drawn from the hero's start, by level (a diagnostic).
    let mut drawn = [0usize; 4];
    for piece in crate::cavern::Cavern::new().build()? {
        let near = piece.near;
        if count {
            let (eye, _) = spire_frame(0.0, -75.0);
            for place in &piece.places {
                let d = place.position.distance(eye);
                let level = piece.levels.iter().position(|(_, to)| d < *to);
                if let Some(level) = level {
                    let from_coarsest = piece.levels.len() - 1 - level;
                    drawn[from_coarsest.min(3)] += piece.levels[level].0.triangle_count();
                }
            }
        }
        let set = world.spawn_lod_prepared(piece.levels)?;
        world.set_lod_places(set, &piece.places)?;
        for level in world.lod_levels(set)? {
            let material = if near {
                look.rock_material(rock::RockKind::FarSpire)
            } else {
                look.cavern_material()
            };
            world.set_custom_material(level, material)?;
            world.set_casts_shadow(level, false)?;
        }
        scenery.cavern.push((set, piece.places));
    }
    if count {
        eprintln!("cavern triangles from the start, coarsest level first: {drawn:?}");
    }
    Ok(scenery)
}

/// A frame standing on the spire's surface at `angle_deg` around the axis
/// (0 = top) and `z` along it: (foot point, rotation whose +y is up).
pub fn spire_frame(angle_deg: f32, z: f32) -> (Vec3, Quat) {
    let a = angle_deg.to_radians();
    let up = Vec3::new(a.sin(), a.cos(), 0.0);
    let at = rock::shared().spire_point(a, z);
    // Local +z stays along the spire, local +y is up.
    let x = up.cross(Vec3::Z);
    let rot = Quat::from_mat3(&Mat3::from_cols(x, up, Vec3::Z));
    (at, rot)
}

/// A frame on the outer wall's face at (x, y): +y points away from the wall.
pub fn wall_frame(x: f32, y: f32) -> (Vec3, Quat) {
    let rot = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    (rock::shared().wall_point(x, y), rot)
}

/// A frame on the island's surface in direction `dir` from its centre.
pub fn island_frame(dir: Vec3) -> (Vec3, Quat) {
    let up = dir.normalize();
    (
        rock::shared().island_point(up),
        Quat::from_rotation_arc(Vec3::Y, up),
    )
}

/// One bonded piece: its entity, its structure and id there.
#[derive(Clone, Copy, Debug)]
pub struct PieceRef {
    pub entity: EntityId,
    pub structure: usize,
    pub piece: PieceId,
}

/// What stands built in the space, for the world's response to stones:
/// bonded structures, their pieces and loose things.
#[derive(Default)]
pub struct Built {
    pub structures: Vec<Structure>,
    pub pieces: Vec<PieceRef>,
}
