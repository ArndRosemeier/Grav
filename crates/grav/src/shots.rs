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
    /// A long lens, for a close look at something far away.
    zoom: bool,
}

impl View {
    fn zoomed(mut self) -> Self {
        self.zoom = true;
        self
    }
}

pub struct Tour {
    views: Vec<View>,
    frame: u32,
    first: usize,
    /// Frames each view is held (`GRAV_TOUR_HOLD`, for views in which
    /// something happens).
    hold: u32,
    /// The views of what grows have been added (they are known only
    /// once it has grown).
    grown: bool,
    /// The game's own lens (field of view, near plane), kept for the views
    /// that do not zoom.
    lens: Option<(f32, f32)>,
}

impl Tour {
    pub fn from_env() -> Option<Self> {
        let first = std::env::var("GRAV_TOUR").ok()?.parse::<usize>().ok()?;
        Some(Self {
            views: views(),
            grown: false,
            frame: 0,
            first: first.saturating_sub(1),
            hold: std::env::var("GRAV_TOUR_HOLD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(HOLD)
                .max(1),
            lens: None,
        })
    }

    /// Set this frame's camera.
    /// An open stone the view on show wants in the world: where it is.
    /// (Where, strength in g, radius.)
    pub fn stone(&self) -> Option<(Vec3, f32, f32)> {
        let index = (self.first + (self.frame / self.hold) as usize) % self.views.len().max(1);
        let view = self.views.get(index)?;
        // GRAV_TOUR_STONE=<g>,<fraction>: another strength, and the stone
        // closes after that part of the view's time.
        let set = std::env::var("GRAV_TOUR_STONE").ok();
        let mut set = set.iter().flat_map(|s| s.split(',')).map(str::parse::<f32>);
        let g = set.next().and_then(Result::ok);
        let part = set.next().and_then(Result::ok).unwrap_or(1.0);
        if (self.frame % self.hold) as f32 >= self.hold as f32 * part {
            return None;
        }
        match view.name {
            GRASS_STONE => Some((grass_stone().0, 2.5, 12.0)),
            PEOPLE_STONE => Some((people_stone(), g.unwrap_or(3.0), 14.0)),
            TOWN_STONE => town_stone().map(|(at, ..)| (at, g.unwrap_or(3.0), 22.0)),
            _ => None,
        }
    }

    pub fn frame(&mut self, world: &mut World) {
        if !self.grown {
            self.grown = true;
            self.views.extend(growth_views());
        }
        let index = (self.first + (self.frame / self.hold) as usize) % self.views.len();
        let v = &self.views[index];
        let camera = world.camera();
        let lens = *self
            .lens
            .get_or_insert((camera.fov_y_degrees(), camera.near()));
        world.set_camera_lens(if v.zoom { 16.0 } else { lens.0 }, lens.1);
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
    let (wall, wall_rot) = space::wall_frame(180.0, 6.0);
    let wall_up = wall_rot * Vec3::Y;
    let (road, road_rot) = space::spire_frame(0.0, -120.0);
    let road_up = road_rot * Vec3::Y;
    let (spring, spring_rot) =
        space::spire_frame(crate::rock::SPRING_AT.0, crate::rock::SPRING_AT.1);
    let spring_up = spring_rot * Vec3::Y;
    let (isle, _) = space::island_frame(Vec3::new(0.6, 0.3, 0.7));
    vec![
        View {
            name: "street towards the heart",
            eye: street + up * 1.7,
            target: street + up * 4.0 + Vec3::Z * 60.0,
            up,
            zoom: false,
        },
        View {
            name: "rock close up, street",
            eye: street + up * 1.5 + Vec3::X * 2.0,
            target: street + Vec3::Z * 4.0,
            up,
            zoom: false,
        },
        View {
            name: "rock close up, wild side",
            eye: side + side_up * 3.0,
            target: side + Vec3::Z * 6.0 - side_up * 1.0,
            up: side_up,
            zoom: false,
        },
        View {
            name: "along the spire to the tip",
            eye: crag + crag_up * 14.0,
            target: Vec3::new(0.0, 0.0, rock_tip()),
            up: crag_up,
            zoom: false,
        },
        View {
            name: "vista: spire and cavern from outside",
            eye: Vec3::new(260.0, 190.0, 330.0),
            target: Vec3::new(0.0, 20.0, -40.0),
            up: Vec3::Y,
            zoom: false,
        },
        View {
            name: "vista: from the wall along the spire",
            eye: wall + wall_up * 12.0 + Vec3::Y * 60.0,
            target: Vec3::new(0.0, 60.0, 200.0),
            up: Vec3::Y,
            zoom: false,
        },
        View {
            name: "island",
            eye: isle + Vec3::new(30.0, 6.0, 40.0),
            target: space::ISLAND_CENTER,
            up: Vec3::Y,
            zoom: false,
        },
        View {
            name: "the heart",
            eye: street + up * 30.0,
            target: street + up * 30.0 + heart * 100.0,
            up,
            zoom: false,
        },
        View {
            name: "the heart, close",
            eye: street + up * 30.0,
            target: street + up * 30.0 + heart * 100.0,
            up,
            zoom: false,
        }
        .zoomed(),
        View {
            name: "street from above",
            eye: street + up * 150.0 + Vec3::Z * 40.0,
            target: street + Vec3::Z * 40.0,
            up: Vec3::Z,
            zoom: false,
        },
        View {
            name: "street in profile",
            eye: street + Vec3::X * 16.0 + up * 0.2 + Vec3::Z * 15.0,
            target: street + up * 0.2 + Vec3::Z * 15.0,
            up,
            zoom: false,
        },
        View {
            name: "road to the wall, from the town's end",
            eye: road + road_up * 1.7,
            target: road + road_up * 0.5 - Vec3::Z * 40.0,
            up: road_up,
            zoom: false,
        },
        View {
            name: "the flare, from above the road",
            eye: Vec3::new(40.0, 120.0, -150.0),
            target: Vec3::new(0.0, 80.0, -235.0),
            up: Vec3::Y,
            zoom: false,
        },
        View {
            name: "the flare, from the ring road",
            eye: Vec3::new(8.0, 137.0, -248.0) + Vec3::Z * 1.7,
            target: Vec3::new(-10.0, 60.0, -215.0),
            up: Vec3::Z,
            zoom: false,
        },
        View {
            name: "on the flare road, looking down to the wall",
            eye: Vec3::new(1.0, 84.0, -231.0),
            target: Vec3::new(-6.0, 130.0, -250.0),
            up: Vec3::new(0.0, 1.0, 1.8).normalize(),
            zoom: false,
        },
        View {
            name: "the flare and the wall, from the side",
            eye: Vec3::new(230.0, 150.0, -150.0),
            target: Vec3::new(0.0, 40.0, -235.0),
            up: Vec3::Z,
            zoom: false,
        },
        View {
            name: "wall close up",
            eye: wall + wall_up * 2.0,
            target: wall + Vec3::new(8.0, 3.0, 0.0),
            up: wall_up,
            zoom: false,
        },
        View {
            name: "the cavern, across from above the town",
            eye: street + up * 60.0,
            target: street + up * 60.0 + Vec3::new(1.0, 0.25, 0.35) * 100.0,
            up,
            zoom: false,
        },
        View {
            name: "the cavern, the other way",
            eye: street + up * 60.0,
            target: street + up * 60.0 + Vec3::new(-1.0, 0.5, 0.8) * 100.0,
            up,
            zoom: false,
        },
        View {
            name: "water: the town's spring",
            eye: spring + spring_up * 3.2 + Vec3::new(9.0, 0.0, -8.0),
            target: spring + spring_up * 0.3,
            up: spring_up,
            zoom: false,
        },
        View {
            name: "water: the spring and its brook from above",
            eye: spring + spring_up * 38.0,
            target: spring,
            up: Vec3::Z,
            zoom: false,
        },
        View {
            name: "water: a pool in the wild rock of the spire",
            eye: Vec3::new(39.0, 34.0, 114.0) + Vec3::new(0.75, 0.66, 0.0) * 24.0 - Vec3::Z * 16.0,
            target: Vec3::new(39.0, 34.0, 114.0),
            up: Vec3::new(0.75, 0.66, 0.0),
            zoom: false,
        },
        View {
            name: "water: a pool on the flare",
            eye: Vec3::new(58.0, 30.0, -219.0) + Vec3::new(0.5, 0.3, 0.6) * 20.0,
            target: Vec3::new(58.0, 30.0, -219.0),
            up: Vec3::new(0.6, 0.3, 0.7).normalize(),
            zoom: false,
        },
        View {
            name: "water: a pool on the wall by the dwarven street",
            eye: Vec3::new(196.0, 20.0, -226.0),
            target: Vec3::new(214.0, 42.0, -238.0),
            up: Vec3::Z,
            zoom: false,
        },
        View {
            name: "water: the spire's far side from the air",
            eye: Vec3::new(60.0, -150.0, -20.0),
            target: Vec3::new(0.0, -40.0, 40.0),
            up: Vec3::Z,
            zoom: false,
        },
    ]
}

/// Views of what grows: each landmark close up, then from above.
fn growth_views() -> Vec<View> {
    let mut views = Vec::new();
    for &(name, at, up, ahead) in crate::flora::landmarks() {
        let side = up.cross(ahead).normalize_or(up.any_orthonormal_vector());
        views.push(View {
            name,
            eye: at + up * 1.7 - ahead * 4.2 + side * 2.4,
            target: at + up * 1.0,
            up,
            zoom: false,
        });
        views.push(View {
            name,
            eye: at + up * 34.0 - ahead * 12.0,
            target: at,
            up,
            zoom: false,
        });
    }
    for &(name, at, up, front) in crate::settle::landmarks() {
        if name.starts_with("goods") || name.starts_with("light") {
            continue;
        }
        let side = up.cross(front).normalize_or(up.any_orthonormal_vector());
        views.push(View {
            name,
            eye: at + up * 1.7 + front * 7.5 + side * 3.5,
            target: at + up * 3.2,
            up,
            zoom: false,
        });
        views.push(View {
            name,
            eye: at + up * 16.0 + front * 34.0 - side * 22.0,
            target: at + up * 6.0,
            up,
            zoom: false,
        });
    }
    // The plumb grass: low over the ground, on a steep flank, and
    // around an open stone (which the tour sets for that view alone).
    let (lawn, lawn_rot) = space::spire_frame(15.0, -64.0);
    let (lawn_up, lawn_on) = (lawn_rot * Vec3::Y, lawn_rot * Vec3::Z);
    views.push(View {
        name: "grass: low over the ground",
        eye: lawn + lawn_up * 0.9 - lawn_on * 5.0,
        target: lawn + lawn_up * 0.8 + lawn_on * 6.0,
        up: lawn_up,
        zoom: false,
    });
    let (flank, flank_rot) = space::spire_frame(74.0, -40.0);
    let (flank_up, flank_on) = (flank_rot * Vec3::Y, flank_rot * Vec3::Z);
    views.push(View {
        name: "grass: on the flank",
        eye: flank + flank_up * 6.0 - flank_on * 12.0,
        target: flank + flank_up * 0.5 + flank_on * 4.0,
        up: flank_up,
        zoom: false,
    });
    let (stone, up) = grass_stone();
    let side = lawn_rot * Vec3::X;
    views.push(View {
        name: GRASS_STONE,
        eye: stone + up * 1.6 - lawn_on * 9.0 + side * 2.0,
        target: stone - up * 0.6,
        up,
        zoom: false,
    });
    views.push(View {
        name: GRASS_STONE,
        eye: stone + up * 13.0 - lawn_on * 5.0,
        target: stone - up * 1.2,
        up,
        zoom: false,
    });
    // A strong stone open by a tower: the town coming apart.
    if let Some((stone, at, up, front)) = town_stone() {
        let side = up.cross(front).normalize_or(up.any_orthonormal_vector());
        for (back, out, high) in [(27.0, 13.0, 6.0), (30.0, -16.0, 14.0)] {
            views.push(View {
                name: TOWN_STONE,
                eye: at + front * back + side * out + up * high,
                target: stone,
                up,
                zoom: false,
            });
        }
    }
    // Loose goods, close up.
    for &(name, at, up, front) in crate::settle::landmarks() {
        if !name.starts_with("goods") {
            continue;
        }
        let side = up.cross(front).normalize_or(up.any_orthonormal_vector());
        views.push(View {
            name,
            eye: at + up * 1.1 + front * 2.3 + side * 1.2,
            target: at + up * 0.35,
            up,
            zoom: false,
        });
    }
    // The people: standing in each street, looking along it.
    let rock = crate::rock::shared();
    let field = space::base_field();
    let ring = |metres: f32| {
        let angle = 50.0_f32.to_radians() + metres / crate::rock::RING_ROAD;
        rock.flare_at(angle, crate::rock::RING_ROAD)
    };
    let streets = [
        (
            "people: on the umbral road",
            Some(space::spire_frame(0.0, -34.0).0),
            Some(space::spire_frame(0.0, -16.0).0),
        ),
        (
            "people: in the dwarven lane",
            Some(space::wall_frame(186.0, 0.5).0),
            Some(space::wall_frame(204.0, 0.0).0),
        ),
        ("people: by the goblin camp", ring(-13.0), ring(3.0)),
    ];
    for (name, from, to) in streets {
        let (Some(from), Some(to)) = (from, to) else {
            continue;
        };
        let up = field.up_at(from, Vec3::Y);
        views.push(View {
            name,
            eye: from + up * 1.5,
            target: to + up * 1.0,
            up,
            zoom: false,
        });
    }
    // A strong stone over the umbral road: it takes people off their feet.
    let (from, rot) = space::spire_frame(3.0, -40.0);
    views.push(View {
        name: PEOPLE_STONE,
        eye: from + rot * Vec3::Y * 2.2,
        target: people_stone(),
        up: rot * Vec3::Y,
        zoom: false,
    });
    // The lights, close: the lamp and what it lights.
    for &(name, at, up, front) in crate::settle::landmarks() {
        if !name.starts_with("light") {
            continue;
        }
        let side = up.cross(front).normalize_or(up.any_orthonormal_vector());
        views.push(View {
            name,
            eye: at + up * 1.6 + front * 3.8 + side * 2.6,
            target: at + up * 1.8,
            up,
            zoom: false,
        });
    }
    views
}

const PEOPLE_STONE: &str = "people: a strong stone over the umbral road";

/// Where the tour's open stone hangs over the umbral road.
fn people_stone() -> Vec3 {
    let (at, rot) = space::spire_frame(0.0, -18.0);
    at + rot * Vec3::Y * 4.0
}

/// A fixed view for the measuring tour (meter.rs).
pub struct Spot {
    pub name: &'static str,
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
}

/// The views frame times are measured from: the street, the town from
/// above, the wide view, the dwarven wall, the cavern.
pub fn measure_views() -> Vec<Spot> {
    let wanted = [
        "street towards the heart",
        "street from above",
        "vista: from the wall along the spire",
        "wall close up",
        "the cavern, across from above the town",
    ];
    let all = views();
    wanted
        .iter()
        .filter_map(|name| all.iter().find(|v| v.name == *name))
        .map(|v| Spot {
            name: v.name,
            eye: v.eye,
            target: v.target,
            up: v.up,
        })
        .collect()
}

const TOWN_STONE: &str = "town: a strong stone open by a tower";

/// Where the tour's open stone hangs by the first tower, with the
/// tower's foot, up and front.
fn town_stone() -> Option<(Vec3, Vec3, Vec3, Vec3)> {
    let &(_, at, up, front) = crate::settle::landmarks().first()?;
    Some((at + up * 7.0 + front * 4.5, at, up, front))
}

const GRASS_STONE: &str = "grass: around an open stone";

/// Where the tour's open stone hangs for the grass views, and up there.
fn grass_stone() -> (Vec3, Vec3) {
    let (at, rot) = space::spire_frame(15.0, -64.0);
    let up = rot * Vec3::Y;
    (at + up * 1.3, up)
}

fn rock_tip() -> f32 {
    crate::rock::SPIRE_TIP
}
