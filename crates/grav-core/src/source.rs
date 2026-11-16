//! One open stone as the field sees it: where it is and what it does.

use crate::one_g;
use crate::profile::{Profile, Selectivity, TimeDial};
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

/// Whether a lodestone attracts or repels (S6: fixed by origin).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Charge {
    /// Veins and stones cut from them.
    Attracting,
    /// The heart and its shards.
    Repelling,
}

impl Charge {
    /// +1 pulls, -1 pushes.
    pub fn sign(self) -> f32 {
        match self {
            Charge::Attracting => 1.0,
            Charge::Repelling => -1.0,
        }
    }

    pub fn opposite(self) -> Charge {
        match self {
            Charge::Attracting => Charge::Repelling,
            Charge::Repelling => Charge::Attracting,
        }
    }
}

/// A stone's identity, kept by the game (and later sent over the network).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourceId(pub u32);

/// A stone producing a field right now.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub id: SourceId,
    pub charge: Charge,
    pub position: Vec3,
    /// Unit axis for plane (its normal) and line (its direction) profiles.
    pub axis: Vec3,
    /// Peak magnitude in g with the sheath fully open (always >= 0; the
    /// charge gives the sign).
    pub max_g: f32,
    /// How far the sheath is open, `0..=1` (S3).
    pub opening: f32,
    /// Hard cutoff radius R in metres (A9).
    pub radius: f32,
    pub profile: Profile,
    pub time: TimeDial,
    pub selectivity: Selectivity,
    /// Seconds since the sheath was last opened from closed (for ramps).
    pub open_seconds: f32,
    /// The body holding this stone in its hand, which the stone does not
    /// act on (P8). `None` when it is set, loose or thrown.
    pub holder: Option<u64>,
    /// Which sides the field spills out of (S14): bit `i` for
    /// [`crate::stone::Face::ALL`]`[i]`, all six for a round field. A side
    /// covers the space where its face's axis is the largest part of the
    /// offset, so six sides together make the round field.
    #[serde(default = "all_faces")]
    pub faces: u8,
    /// The box's orientation: its faces' axes (local +z front, +y up, +x
    /// left) in the world.
    #[serde(default)]
    pub frame: Quat,
}

/// Every side open.
pub const ALL_FACES: u8 = 0b11_1111;

fn all_faces() -> u8 {
    ALL_FACES
}

/// The side of a box (index into [`crate::stone::Face::ALL`]) that a local
/// offset falls on.
pub fn face_of(local: Vec3) -> usize {
    let a = local.abs();
    if a.z >= a.x && a.z >= a.y {
        if local.z >= 0.0 {
            0
        } else {
            1
        }
    } else if a.x >= a.y {
        if local.x >= 0.0 {
            2
        } else {
            3
        }
    } else if local.y >= 0.0 {
        4
    } else {
        5
    }
}

impl Source {
    /// A point source with the core profile, fully open.
    pub fn new(id: u32, charge: Charge, position: Vec3, max_g: f32, radius: f32) -> Self {
        Self {
            id: SourceId(id),
            charge,
            position,
            axis: Vec3::Y,
            max_g,
            opening: 1.0,
            radius,
            profile: Profile::Core,
            time: TimeDial::Constant,
            selectivity: Selectivity::All,
            open_seconds: 0.0,
            holder: None,
            faces: ALL_FACES,
            frame: Quat::IDENTITY,
        }
    }

    pub fn with_profile(mut self, profile: Profile, axis: Vec3) -> Self {
        self.profile = profile;
        self.axis = axis.try_normalize().unwrap_or(Vec3::Y);
        self
    }

    /// Signed magnitude in m/s² right now: positive pulls, negative pushes.
    pub fn magnitude(&self) -> f32 {
        self.charge.sign()
            * self.max_g.max(0.0)
            * self.opening.clamp(0.0, 1.0)
            * self.time.factor(self.open_seconds)
            * one_g()
    }

    /// Whether this source produces any field at all.
    pub fn is_active(&self) -> bool {
        self.opening > 0.0 && self.max_g > 0.0 && self.radius > 0.0
    }

    /// The field this source gives a body at `p` with `mass`, in m/s²,
    /// before charge rules for lodestone bodies.
    pub fn acceleration(&self, p: Vec3, mass: f32) -> Vec3 {
        if !self.is_active() || !self.selectivity.accepts(mass) {
            return Vec3::ZERO;
        }
        if self.faces != ALL_FACES {
            let local = self.frame.inverse() * (p - self.position);
            if self.faces & (1 << face_of(local)) == 0 {
                return Vec3::ZERO;
            }
        }
        self.profile.pull(p - self.position, self.axis, self.radius) * self.magnitude()
    }

    /// Whether `p` is inside this source's reach (its bounding sphere: a
    /// plane's disc and a line's cylinder fit in a sphere of R·√2).
    pub fn may_reach(&self, p: Vec3) -> bool {
        let reach = match self.profile {
            Profile::Plane | Profile::Line => self.radius * std::f32::consts::SQRT_2,
            _ => self.radius,
        };
        p.distance_squared(self.position) < reach * reach
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_half_open_stone_gives_half_the_pull() {
        let mut s = Source::new(1, Charge::Attracting, Vec3::ZERO, 1.5, 15.0);
        let full = s.acceleration(Vec3::new(3.0, 0.0, 0.0), 80.0);
        s.opening = 0.5;
        let half = s.acceleration(Vec3::new(3.0, 0.0, 0.0), 80.0);
        assert!((half * 2.0 - full).length() < 1e-5);
        // Core at x = 0.2: 1.5 g * 0.8, towards the stone.
        assert!((full.x + 1.5 * 0.8 * one_g()).abs() < 1e-4);
    }

    #[test]
    fn a_repelling_shard_pushes() {
        let s = Source::new(2, Charge::Repelling, Vec3::ZERO, 1.0, 10.0);
        let a = s.acceleration(Vec3::new(0.0, 5.0, 0.0), 1.0);
        assert!(a.y > 0.0);
        assert!((a.y - 0.5 * one_g()).abs() < 1e-4);
    }

    #[test]
    fn one_open_face_pulls_only_on_its_side() {
        let mut s = Source::new(4, Charge::Attracting, Vec3::ZERO, 1.0, 10.0);
        s.faces = 1; // the front: local +z
        s.frame = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2); // front faces +x
        assert!(s.acceleration(Vec3::new(4.0, 1.0, 0.0), 1.0).x < -0.1);
        assert_eq!(s.acceleration(Vec3::new(-4.0, 0.0, 0.0), 1.0), Vec3::ZERO);
        assert_eq!(s.acceleration(Vec3::new(1.0, 4.0, 0.0), 1.0), Vec3::ZERO);
        // All six sides make the round field again.
        s.faces = ALL_FACES;
        assert!(s.acceleration(Vec3::new(1.0, 4.0, 0.0), 1.0).y < -0.1);
    }

    #[test]
    fn mass_selective_stones_ignore_other_masses() {
        let mut s = Source::new(3, Charge::Attracting, Vec3::ZERO, 2.0, 20.0);
        s.selectivity = Selectivity::MassBand {
            min: 500.0,
            max: f32::INFINITY,
        };
        assert_eq!(s.acceleration(Vec3::X * 5.0, 80.0), Vec3::ZERO);
        assert!(s.acceleration(Vec3::X * 5.0, 900.0).length() > 0.0);
    }
}
