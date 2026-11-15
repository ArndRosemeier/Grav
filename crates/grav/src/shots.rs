//! A camera tour for screenshots: with `GRAV_TOUR` set, the camera steps
//! through fixed views of the world instead of following the hero, a new
//! one every `HOLD` frames, so a verify run with `--shots` gives a set of
//! pictures to look at before pushing (CLAUDE.md, "Look before you push").
//!
//! `GRAV_TOUR=1` starts at the first view; `GRAV_TOUR=<n>` starts at view n.

use crate::space;
use engine::prelude::*;

/// Frames each view is held (the picture settles in the first few).
pub const HOLD: u32 = 12;

struct View {
    name: &'static str,
    eye: Vec3,
    target: Vec3,
    up: Vec3,
}

pub struct Tour {
    views: Vec<View>,
    frame: u32,
    first: usize,
}

impl Tour {
    pub fn from_env() -> Option<Self> {
        let first = std::env::var("GRAV_TOUR").ok()?.parse::<usize>().ok()?;
        Some(Self {
            views: views(),
            frame: 0,
            first: first.saturating_sub(1),
        })
    }

    /// Set this frame's camera.
    pub fn frame(&mut self, world: &mut World) {
        let index = (self.first + (self.frame / HOLD) as usize) % self.views.len();
        let v = &self.views[index];
        world.look_at_up(v.eye, v.target, v.up);
        world.report("tour_view", v.name);
        self.frame += 1;
    }
}

fn views() -> Vec<View> {
    let heart = space::HEART_CENTER.normalize();
    let (street, street_rot) = space::spire_frame(0.0, -70.0);
    let up = street_rot * Vec3::Y;
    let (side, side_rot) = space::spire_frame(55.0, 80.0);
    let side_up = side_rot * Vec3::Y;
    let (crag, crag_rot) = space::spire_frame(-30.0, 100.0);
    let crag_up = crag_rot * Vec3::Y;
    let (wall, wall_rot) = space::wall_frame(60.0, 6.0);
    let wall_up = wall_rot * Vec3::Y;
    let (isle, _) = space::island_frame(Vec3::new(0.6, 0.3, 0.7));
    vec![
        View {
            name: "street towards the heart",
            eye: street + up * 1.7,
            target: street + up * 4.0 + Vec3::Z * 60.0,
            up,
        },
        View {
            name: "rock close up, street",
            eye: street + up * 1.5 + Vec3::X * 2.0,
            target: street + Vec3::Z * 4.0,
            up,
        },
        View {
            name: "rock close up, wild side",
            eye: side + side_up * 3.0,
            target: side + Vec3::Z * 6.0 - side_up * 1.0,
            up: side_up,
        },
        View {
            name: "along the spire to the tip",
            eye: crag + crag_up * 14.0,
            target: Vec3::new(0.0, 0.0, rock_tip()),
            up: crag_up,
        },
        View {
            name: "vista: spire and cavern from outside",
            eye: Vec3::new(260.0, 190.0, 330.0),
            target: Vec3::new(0.0, 20.0, -40.0),
            up: Vec3::Y,
        },
        View {
            name: "vista: from the wall along the spire",
            eye: wall + wall_up * 12.0 + Vec3::Y * 60.0,
            target: Vec3::new(0.0, 60.0, 200.0),
            up: Vec3::Y,
        },
        View {
            name: "island",
            eye: isle + Vec3::new(30.0, 6.0, 40.0),
            target: space::ISLAND_CENTER,
            up: Vec3::Y,
        },
        View {
            name: "the heart",
            eye: street + up * 30.0,
            target: street + up * 30.0 + heart * 100.0,
            up,
        },
        View {
            name: "street from above",
            eye: street + up * 150.0 + Vec3::Z * 40.0,
            target: street + Vec3::Z * 40.0,
            up: Vec3::Z,
        },
        View {
            name: "street in profile",
            eye: street + Vec3::X * 16.0 + up * 0.2 + Vec3::Z * 15.0,
            target: street + up * 0.2 + Vec3::Z * 15.0,
            up,
        },
        View {
            name: "wall close up",
            eye: wall + wall_up * 2.0,
            target: wall + Vec3::new(8.0, 3.0, 0.0),
            up: wall_up,
        },
    ]
}

fn rock_tip() -> f32 {
    crate::rock::SPIRE_TIP
}
