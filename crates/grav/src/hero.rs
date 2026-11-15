//! The hero: a character standing against the field (P1 to P8), its
//! camera and its body.

use crate::space::place;
use engine::glam::{Mat3, Vec2};
use engine::prelude::*;
use grav_core::{can_walk, fall_damage, jump_speed, Movement, G};

pub struct Hero {
    pub id: EntityId,
    body: EntityId,
    /// Facing, kept across the hero's up as down turns.
    pub heading: Vec3,
    /// The camera's up, smoothed towards the body's up (P3).
    pub cam_up: Vec3,
    pub health: f32,
    was_grounded: bool,
    last_fall: Vec3,
    /// Hero state for the HUD.
    pub pull_g: f32,
    pub drifting: bool,
    pub walking_allowed: bool,
}

/// Inputs the hero reads this frame (from keys, or from the demo script).
#[derive(Clone, Copy, Debug, Default)]
pub struct HeroInput {
    /// x strafe right, y forward, each -1..1.
    pub walk: Vec2,
    /// Degrees to turn right this frame.
    pub turn: f32,
    pub jump: bool,
}

impl Hero {
    pub fn spawn(world: &mut World, feet: Vec3, up: Vec3) -> EngineResult<Self> {
        // A dwarf-sized body: 1.4 m.
        let id = world.spawn_pivot(Place::new(feet.x, feet.y, feet.z));
        world.add_character(
            id,
            Character::default()
                .size(1.4, 0.32)
                .mass(70.0)
                .up(CharacterUp::field()),
        )?;
        let mut mesh = Shape::box_at((0.0, 0.62, 0.0), (0.56, 0.95, 0.36), rgb(150, 70, 48))?;
        mesh.append_translated(
            &Shape::ball((0.0, 0.0, 0.0), 0.2, rgb(222, 180, 150))?,
            Vec3::new(0.0, 1.22, 0.0),
        )?;
        mesh.append_translated(
            &Shape::box_at((0.0, 0.0, 0.0), (0.3, 0.12, 0.2), rgb(90, 60, 40))?,
            Vec3::new(0.0, 1.05, 0.2),
        )?;
        let body = world.spawn(mesh);
        // Face along the spire, towards the town.
        let heading = (Vec3::Z - up * up.z).normalize_or(up.any_orthonormal_vector());
        Ok(Self {
            id,
            body,
            heading,
            cam_up: up,
            health: 100.0,
            was_grounded: false,
            last_fall: Vec3::ZERO,
            pull_g: 0.0,
            drifting: false,
            walking_allowed: false,
        })
    }

    pub fn state(&self, world: &World) -> CharacterState {
        world.character(self.id).unwrap_or_default()
    }

    pub fn feet(&self, world: &World) -> Vec3 {
        world
            .world_transform(self.id)
            .map(|m| m.w_axis.truncate())
            .unwrap_or_default()
    }

    /// The right hand, where a held stone is.
    pub fn hand(&self, world: &World) -> Vec3 {
        let s = self.state(world);
        let up = if s.up.length_squared() > 0.5 {
            s.up
        } else {
            self.cam_up
        };
        let right = self.heading.cross(up).normalize_or_zero();
        self.feet(world) + up * 0.95 + self.heading * 0.35 + right * 0.3
    }

    pub fn head(&self, world: &World) -> Vec3 {
        self.feet(world) + self.cam_up * 1.25
    }

    /// One frame: steer, walk, jump or push off, take fall damage, place
    /// the camera.
    pub fn update(
        &mut self,
        world: &mut World,
        input: HeroInput,
        m: &Movement,
        dt: f32,
    ) -> EngineResult<()> {
        let s = self.state(world);
        let up = if s.up.length_squared() > 0.5 {
            s.up
        } else {
            Vec3::Y
        };
        // P3: the camera's up follows the body's, smoothed over ~0.5 s.
        let k = 1.0 - (-dt * 3.0 / m.camera_smoothing.max(0.05)).exp();
        self.cam_up = self.cam_up.lerp(up, k).normalize_or(up);
        // Keep facing across the camera's up, and turn about it.
        let mut h = self.heading - self.cam_up * self.heading.dot(self.cam_up);
        if h.length_squared() < 1e-6 {
            h = self.cam_up.any_orthonormal_vector();
        }
        h = Quat::from_axis_angle(self.cam_up, -input.turn.to_radians()) * h.normalize();
        self.heading = h;
        let right = self.heading.cross(self.cam_up);

        self.pull_g = s.gravity.length() / G;
        self.drifting = self.pull_g < m.drift_below_g;
        // P4: walk only where the field presses the hero onto the surface.
        self.walking_allowed = s.grounded && can_walk(m, s.gravity, up);
        let dir = self.heading * input.walk.y + right * input.walk.x;
        let walk = if self.walking_allowed {
            dir.clamp_length_max(1.0) * m.walk_speed
        } else {
            Vec3::ZERO
        };
        world.walk(self.id, walk)?;
        if input.jump {
            if self.walking_allowed {
                // P6: a constant jump impulse.
                world.jump(self.id, jump_speed(m))?;
            } else if self.drifting {
                // P5: push off whatever is close, along the facing.
                let feet = self.feet(world);
                let near = world
                    .cast_sphere_ignoring(feet + self.cam_up * 0.7, -self.cam_up, 0.4, 1.5, self.id)
                    .is_some();
                if near {
                    world.push_character(self.id, self.heading * 3.0 + self.cam_up * 1.5)?;
                }
            }
        }
        // P7: damage from the impact speed into the surface on landing.
        if s.grounded && !self.was_grounded {
            let impact = self.last_fall.dot(-up).max(0.0);
            let d = fall_damage(m, impact);
            if d > 0.0 {
                self.health = (self.health - d * 100.0).max(0.0);
                world.float_text(
                    self.head(world),
                    format!("-{:.0}", d * 100.0),
                    rgb(255, 90, 70),
                );
            }
        }
        self.was_grounded = s.grounded;
        if !s.grounded {
            self.last_fall = s.fall;
        }
        // The body stands along its own up and faces the heading.
        let z =
            (self.heading - up * self.heading.dot(up)).normalize_or(up.any_orthonormal_vector());
        let x = up.cross(z);
        let rot = Quat::from_mat3(&Mat3::from_cols(x, up, z));
        let feet = self.feet(world);
        world.set_place(self.body, place(feet, rot))?;
        world.look_follow_entity_up(self.id, self.cam_up, self.heading, 5.5, 2.2)?;
        Ok(())
    }
}
