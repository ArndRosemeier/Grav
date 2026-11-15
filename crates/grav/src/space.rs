//! The test space: one spire of dead stone growing out of the outer wall,
//! a floating island, and a few streets of bonded houses (design doc,
//! "The core mechanics milestone").
//!
//! Districts in this space:
//! - the spire's surface: middle band, 1 g towards its vein, so down turns
//!   as you walk around it and the far side has the opposite down (R1);
//! - the outer wall's face: heavy band, 1.8 g;
//! - the spire root, where both meet: a seam (R4);
//! - the island above the tip: light band, 0.4 g;
//! - the open cavern between: only the heart's push (R3).

use crate::look::Look;
use crate::rock;
use engine::glam::{EulerRot, Mat3};
use engine::prelude::*;
use grav_core::{BondClass, Field, Heart, PieceId, Structure, Tuning, Vein, VeinShape};

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
pub fn build_rock(world: &mut World, look: &Look) -> EngineResult<()> {
    let heart_dir = HEART_CENTER.normalize();
    for piece in rock::build_all(rock::shared(), heart_dir)? {
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
    for piece in crate::cavern::Cavern::new().build()? {
        let set = world.spawn_lod_prepared(piece.levels)?;
        world.set_lod_places(set, &piece.places)?;
        for level in world.lod_levels(set)? {
            world.set_custom_material(level, look.cavern_material())?;
            world.set_casts_shadow(level, false)?;
        }
    }
    Ok(())
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
    pub half: Vec3,
}

/// What the builder made that the game keeps.
#[derive(Default)]
pub struct Built {
    pub structures: Vec<Structure>,
    pub pieces: Vec<PieceRef>,
    pub loose: Vec<EntityId>,
}

struct Builder<'a> {
    world: &'a mut World,
    tuning: &'a Tuning,
    field: &'a Field,
    built: &'a mut Built,
}

impl Builder<'_> {
    /// A box piece of a structure, at `local` (centre) in the frame.
    #[allow(clippy::too_many_arguments)]
    fn piece(
        &mut self,
        frame: (Vec3, Quat),
        local: Vec3,
        size: Vec3,
        class: BondClass,
        anchored: bool,
        color: Color,
    ) -> EngineResult<PieceId> {
        let (origin, rot) = frame;
        let at = origin + rot * local;
        let e = self
            .world
            .spawn(Shape::box_at((0.0, 0.0, 0.0), size, color)?);
        self.world.set_place(e, place(at, rot))?;
        let half = size * 0.5;
        self.world
            .add_body(e, Body::fixed().shape(BodyShape::Box(half)))?;
        let s = self.built.structures.len() - 1;
        let design = self.field.acceleration(&grav_core::BodyQuery::at(at));
        let id = self.built.structures[s].add(class, self.tuning.bond_g(class), design, anchored);
        self.built.pieces.push(PieceRef {
            entity: e,
            structure: s,
            piece: id,
            half,
        });
        Ok(id)
    }

    /// A small two-storey house: masonry walls in blocks, timber floors,
    /// a tiled roof. `heavy` builds it dwarven (low, thick, strong).
    fn house(&mut self, frame: (Vec3, Quat), heavy: bool) -> EngineResult<()> {
        self.built.structures.push(Structure::new());
        let (w, d) = (6.0_f32, 5.0_f32);
        let block = 1.5_f32;
        let thick = if heavy { 0.7 } else { 0.4 };
        let storeys = if heavy { 1 } else { 2 };
        let rows_per_storey = 2;
        let wall_class = if heavy {
            BondClass::Dwarven
        } else {
            BondClass::Masonry
        };
        let stone = if heavy {
            rgb(120, 104, 92)
        } else {
            rgb(176, 160, 136)
        };
        let rows = storeys * rows_per_storey;
        // Walls: a grid of blocks around the footprint, row by row.
        let nx = (w / block) as i32;
        let nz = (d / block) as i32;
        let mut grid: Vec<Vec<PieceId>> = Vec::new();
        for row in 0..rows {
            let y = block * (row as f32 + 0.5) - 0.05;
            let mut ring = Vec::new();
            let door = |side: i32, i: i32| side == 0 && i == nx / 2 && row == 0;
            for side in 0..4 {
                let (n, along_x) = if side % 2 == 0 {
                    (nx, true)
                } else {
                    (nz, false)
                };
                for i in 0..n {
                    if door(side, i) {
                        continue;
                    }
                    let t = -0.5 * (n as f32) * block + block * (i as f32 + 0.5);
                    let (local, size) = if along_x {
                        let z = if side == 0 { -d * 0.5 } else { d * 0.5 };
                        (Vec3::new(t, y, z), Vec3::new(block, block, thick))
                    } else {
                        let x = if side == 1 { w * 0.5 } else { -w * 0.5 };
                        (Vec3::new(x, y, t), Vec3::new(thick, block, block))
                    };
                    let id = self.piece(frame, local, size * 0.98, wall_class, row == 0, stone)?;
                    ring.push(id);
                }
            }
            let s = self.built.structures.len() - 1;
            for k in 0..ring.len() {
                self.built.structures[s].bond(ring[k], ring[(k + 1) % ring.len()]);
            }
            if let Some(below) = grid.last() {
                for (k, &id) in ring.iter().enumerate() {
                    if let Some(&b) = below.get(k) {
                        self.built.structures[s].bond(id, b);
                    }
                }
            }
            grid.push(ring);
        }
        let top_y = block * rows as f32;
        // Timber floors between storeys.
        let s = self.built.structures.len() - 1;
        for storey in 1..storeys {
            let y = block * (storey * rows_per_storey) as f32;
            let floor = self.piece(
                frame,
                Vec3::new(0.0, y, 0.0),
                Vec3::new(w - thick, 0.2, d - thick),
                BondClass::Timber,
                false,
                rgb(130, 96, 64),
            )?;
            let row = &grid[(storey * rows_per_storey) as usize - 1];
            for &id in row.iter().step_by(2) {
                self.built.structures[s].bond(floor, id);
            }
        }
        // Roof: timber beams, then light tiles on them.
        let top = grid.last().cloned().unwrap_or_default();
        let mut beams = Vec::new();
        for i in 0..3 {
            let x = -w * 0.33 + i as f32 * w * 0.33;
            let beam = self.piece(
                frame,
                Vec3::new(x, top_y + 0.15, 0.0),
                Vec3::new(0.25, 0.3, d + 0.4),
                BondClass::Timber,
                false,
                rgb(110, 80, 52),
            )?;
            for &id in top.iter().step_by(3) {
                self.built.structures[s].bond(beam, id);
            }
            beams.push(beam);
        }
        let tiles_x = 4;
        let tiles_z = 3;
        let tile = Vec3::new(w / tiles_x as f32 + 0.3, 0.12, d / tiles_z as f32 + 0.3);
        for i in 0..tiles_x {
            for j in 0..tiles_z {
                let x = -w * 0.5 + tile.x * (i as f32 + 0.5) - 0.15;
                let z = -d * 0.5 + tile.z * (j as f32 + 0.5) - 0.15;
                let id = self.piece(
                    frame,
                    Vec3::new(x, top_y + 0.36, z),
                    tile * 0.97,
                    BondClass::Light,
                    false,
                    rgb(150, 70, 52),
                )?;
                let beam =
                    beams[((i as usize) * beams.len() / tiles_x as usize).min(beams.len() - 1)];
                self.built.structures[s].bond(id, beam);
            }
        }
        Ok(())
    }

    /// A market stall: a light frame and awning.
    fn stall(&mut self, frame: (Vec3, Quat)) -> EngineResult<()> {
        self.built.structures.push(Structure::new());
        let s = self.built.structures.len() - 1;
        let wood = rgb(140, 104, 70);
        let mut posts = Vec::new();
        for (x, z) in [(-1.0, -0.7), (1.0, -0.7), (-1.0, 0.7), (1.0, 0.7)] {
            posts.push(self.piece(
                frame,
                Vec3::new(x, 1.0, z),
                Vec3::new(0.12, 2.0, 0.12),
                BondClass::Light,
                true,
                wood,
            )?);
        }
        let awning = self.piece(
            frame,
            Vec3::new(0.0, 2.05, 0.0),
            Vec3::new(2.4, 0.08, 1.8),
            BondClass::Light,
            false,
            rgb(60, 110, 150),
        )?;
        for p in posts {
            self.built.structures[s].bond(awning, p);
        }
        Ok(())
    }

    /// Loose goods: crates and barrels lying around (they follow the field at once).
    fn crate_at(&mut self, frame: (Vec3, Quat), local: Vec3, size: f32) -> EngineResult<()> {
        let (origin, rot) = frame;
        let at = origin + rot * (local + Vec3::Y * (size * 0.5 + 0.02));
        let e = self.world.spawn(Shape::box_at(
            (0.0, 0.0, 0.0),
            Vec3::splat(size),
            rgb(150, 112, 70),
        )?);
        self.world.set_place(e, place(at, rot))?;
        self.world.add_body(
            e,
            Body::dynamic()
                .density(0.6)
                .shape(BodyShape::Box(Vec3::splat(size * 0.5))),
        )?;
        self.built.loose.push(e);
        Ok(())
    }
}

/// Build the houses, stalls and loose goods. The pieces are built for the
/// field they stand in (`field` without stones).
pub fn build_town(world: &mut World, tuning: &Tuning, field: &Field) -> EngineResult<Built> {
    let mut built = Built::default();
    let mut b = Builder {
        world,
        tuning,
        field,
        built: &mut built,
    };
    // A street along the top of the spire, and houses down its sides.
    for (i, z) in [-60.0, -40.0, -20.0, 0.0, 20.0].iter().enumerate() {
        let angle = if i % 2 == 0 { -9.0 } else { 9.0 };
        b.house(spire_frame(angle, *z), false)?;
    }
    b.house(spire_frame(70.0, -10.0), false)?;
    b.house(spire_frame(180.0, -10.0), false)?;
    // Stalls and goods in the street.
    for z in [-50.0, -30.0, -10.0, 10.0] {
        b.stall(spire_frame(0.0, z))?;
        let f = spire_frame(0.0, z + 4.0);
        b.crate_at(f, Vec3::new(1.5, 0.0, 0.0), 0.6)?;
        b.crate_at(f, Vec3::new(-1.2, 0.0, 1.0), 0.5)?;
        b.crate_at(f, Vec3::new(-1.0, 0.0, -1.2), 0.8)?;
    }
    // Dwarven houses on the outer wall, near the spire root.
    for x in [-190.0, -170.0, 170.0, 190.0] {
        b.house(wall_frame(x, 0.0), true)?;
    }
    // An umbral perch on the island.
    b.house(island_frame(Vec3::new(0.0, 1.0, 0.2)), false)?;
    b.crate_at(island_frame(Vec3::new(0.3, 1.0, -0.3)), Vec3::ZERO, 0.7)?;
    Ok(built)
}
