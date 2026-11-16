//! The hero: a character standing against the field (P1 to P8), its
//! camera and its body.

use crate::space::place;
use engine::glam::Vec2;
use engine::prelude::*;
use grav_core::{
    can_walk, climb_slope_deg, fall_damage, jump_speed, slide_step, walk_accel, walk_speed_in,
    Movement, G,
};

/// The ground speed the Actors gait clips are made for (a 1.75 m person), m/s.
const WALK_CLIP_SPEED: f32 = 1.4;
const RUN_CLIP_SPEED: f32 = 4.5;

pub struct Hero {
    pub id: EntityId,
    body: EntityId,
    /// The clip the body plays, when it is an animated dwarf.
    clip: Option<&'static str>,
    /// How fast the legs carry this body compared with a 1.75 m person.
    leg_scale: f32,
    /// Facing, kept across the hero's up as down turns.
    pub heading: Vec3,
    /// Where the body faces: it turns to the direction he walks in.
    facing: Vec3,
    /// The camera's up, smoothed towards the body's up (P3).
    pub cam_up: Vec3,
    /// How far the camera looks up (down when negative), degrees.
    pub look_pitch: f32,
    pub health: f32,
    was_grounded: bool,
    last_fall: Vec3,
    /// Hero state for the HUD.
    pub pull_g: f32,
    pub drifting: bool,
    pub walking_allowed: bool,
    pub grounded: bool,
    /// Degrees between the pull and straight into the ground underfoot.
    pub pull_tilt_deg: f32,
    /// His own walking velocity along the ground: it builds up and dies
    /// away by his weight (P11).
    walk_v: Vec3,
    /// The velocity the pull drags him along the ground with (P9).
    pub slide: Vec3,
    /// The pull a moment ago (smoothed over `jolt_window`), to notice a
    /// sudden change (P12).
    calm_pull: Option<Vec3>,
    last_pull: Vec3,
    /// Seconds of stumbling left (P12).
    pub stumble: f32,
}

/// Inputs the hero reads this frame (from keys, or from the demo script).
#[derive(Clone, Copy, Debug, Default)]
pub struct HeroInput {
    /// x strafe right, y forward, each -1..1.
    pub walk: Vec2,
    /// Degrees to turn right this frame.
    pub turn: f32,
    /// Degrees to look up this frame.
    pub pitch: f32,
    pub jump: bool,
    pub sprint: bool,
}

impl Hero {
    pub fn spawn(world: &mut World, feet: Vec3, up: Vec3, m: &Movement) -> EngineResult<Self> {
        // A dwarf-sized body: 1.4 m.
        let id = world.spawn_pivot(Place::new(feet.x, feet.y, feet.z));
        world.add_character(
            id,
            Character::default()
                .size(1.4, 0.32)
                .mass(70.0)
                .up(CharacterUp::Field {
                    turn_deg_per_s: m.turn_deg_per_s,
                    drift_below: m.drift_below_g * G,
                }),
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
        let heading = grav_core::along_ground(Vec3::Z, up);
        Ok(Self {
            id,
            body,
            clip: None,
            leg_scale: 1.0,
            heading,
            facing: heading,
            cam_up: up,
            look_pitch: 0.0,
            health: 100.0,
            was_grounded: false,
            last_fall: Vec3::ZERO,
            pull_g: 0.0,
            drifting: false,
            walking_allowed: false,
            grounded: false,
            pull_tilt_deg: 0.0,
            walk_v: Vec3::ZERO,
            slide: Vec3::ZERO,
            calm_pull: None,
            last_pull: Vec3::ZERO,
            stumble: 0.0,
        })
    }

    /// Draw the hero as an animated model (the Actors dwarf) instead of
    /// the block figure.
    pub fn set_animated_body(&mut self, world: &mut World, body: EntityId, height: f32) {
        self.leg_scale = (height / 1.75).clamp(0.5, 1.5);
        world.despawn(self.body);
        self.body = body;
        self.clip = Some("idle");
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
        // P4: in light pull he climbs steeper, in heavy pull less.
        world.set_character_max_slope(self.id, climb_slope_deg(m, self.pull_g))?;
        self.drifting = self.pull_g < m.drift_below_g;
        // The ground under the feet: what the pull presses him onto.
        let feet = self.feet(world);
        let ground = world
            .cast_ray_ignoring(feet + up * 0.5, -up, 1.2, self.id)
            .map(|hit| hit.normal)
            .filter(|n| n.dot(up) > 0.2)
            .unwrap_or(up);
        let along = |v: Vec3| v - ground * v.dot(ground);
        self.grounded = s.grounded;
        self.pull_tilt_deg = (-s.gravity).angle_between(ground).to_degrees();
        // P4: walk only where the field presses the hero onto the surface.
        self.walking_allowed = s.grounded && can_walk(m, s.gravity, ground);

        // P12: a sudden change of pull yanks him and makes him stumble.
        let calm = self.calm_pull.get_or_insert(s.gravity);
        let jolted = (s.gravity - *calm).length() > m.stumble_change_g * G;
        *calm = calm.lerp(s.gravity, (dt / m.jolt_window.max(dt)).min(1.0));
        if jolted && s.grounded {
            self.stumble = m.stumble_seconds;
            self.slide += along(s.gravity - self.last_pull) * m.jolt_seconds;
        }
        self.last_pull = s.gravity;
        self.stumble = (self.stumble - dt).max(0.0);
        let control = if self.stumble > 0.0 {
            m.stumble_control
        } else {
            1.0
        };

        if s.grounded {
            // P9: the pull along the ground drags him only on slopes his
            // grip cannot hold, and friction stops a slide on rock quickly.
            self.slide = slide_step(m, s.gravity, ground, along(self.slide), dt);
            // P10, P11: his own walk: slower when heavy or against the
            // pull, and it builds up and stops by his weight.
            let dir =
                along(self.heading * input.walk.y + right * input.walk.x).clamp_length_max(1.0);
            let want = if self.walking_allowed && dir.length_squared() > 1e-6 {
                dir * walk_speed_in(m, s.gravity, ground, dir.normalize())
                    * control
                    * if input.sprint { m.sprint_factor } else { 1.0 }
            } else {
                Vec3::ZERO
            };
            let step = walk_accel(m, s.gravity, ground) * control * dt;
            let gap = want - along(self.walk_v);
            self.walk_v = along(self.walk_v) + gap.clamp_length_max(step);
        } else if self.was_grounded {
            // Leaving the ground: his speed goes with him.
            world.push_character(self.id, self.walk_v + self.slide)?;
            self.walk_v = Vec3::ZERO;
            self.slide = Vec3::ZERO;
        }
        world.walk(self.id, self.walk_v + self.slide)?;
        if input.jump {
            if self.walking_allowed {
                // P6: a constant jump impulse.
                world.jump(self.id, jump_speed(m))?;
            } else if self.drifting {
                // P5: push off whatever is close, along the facing.
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
        // The body stands along its own up and turns to where he walks
        // (the camera keeps the heading), so the legs agree with the motion.
        let across = |v: Vec3| v - up * v.dot(up);
        let moving = across(self.walk_v);
        let want = if moving.length() > 0.3 {
            moving.normalize()
        } else {
            across(self.facing).normalize_or(across(self.heading).normalize_or_zero())
        };
        let from = across(self.facing).normalize_or(want);
        let angle = from.angle_between(want);
        let step = (540.0f32.to_radians() * dt).min(angle);
        self.facing = if angle > 1e-4 {
            // Turn about up, the short way round.
            let sign = if from.cross(want).dot(up) >= 0.0 {
                1.0
            } else {
                -1.0
            };
            Quat::from_axis_angle(up, sign * step) * from
        } else {
            want
        };
        let rot = grav_core::upright(up, self.facing);
        world.set_place(self.body, place(feet, rot))?;
        if let Some(clip) = self.clip {
            // The legs show his own walking, not a slide: idle, walk or
            // run by the speed he walks at, played as fast as he moves so
            // the feet do not skate.
            let speed = if s.grounded {
                self.walk_v.length()
            } else {
                0.0
            };
            let (want, clip_speed) = if speed < 0.25 {
                ("idle", 0.0)
            } else if speed < WALK_CLIP_SPEED * 1.5 * self.leg_scale {
                ("walk", WALK_CLIP_SPEED)
            } else {
                ("run", RUN_CLIP_SPEED)
            };
            if want != clip {
                // A pack without a run clip walks faster instead.
                if world.play_animation(self.body, want).is_err() {
                    world.play_animation(self.body, "walk")?;
                }
                self.clip = Some(want);
            }
            let rate = if clip_speed > 0.0 {
                (speed / (clip_speed * self.leg_scale)).clamp(0.4, 2.0)
            } else {
                1.0
            };
            world.set_animation_speed(self.body, rate)?;
        }
        // The camera circles the head: behind him, raised or lowered by the
        // look pitch, pulled in where rock is in the way.
        self.look_pitch = (self.look_pitch + input.pitch).clamp(-60.0, 70.0);
        let (sin, cos) = self.look_pitch.to_radians().sin_cos();
        let look = self.heading * cos + self.cam_up * sin;
        let target = feet + self.cam_up * 1.9;
        let back = -look * 5.5;
        let room = world
            .cast_sphere_ignoring(target, back, 0.3, 5.5, self.id)
            .map_or(1.0, |hit| (hit.distance / 5.5).clamp(0.05, 1.0));
        world.look_at_up(target + back * room, target, self.cam_up);
        Ok(())
    }
}
