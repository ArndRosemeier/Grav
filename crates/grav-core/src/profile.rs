//! The six fixed source profiles (A8 to A10) and the dials on top of them.

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// A source's distance formula, `p(x)` with `x = distance / R`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    /// `1 - x`: strongest at the centre, nothing at R.
    Core,
    /// `x`: free near the stone, strongest just inside R.
    Leash,
    /// `2x - 1`: pulls beyond R/2, pushes inside; bodies hover at R/2.
    Shell,
    /// `1`: the same pull everywhere inside R, a portable "down".
    Uniform,
    /// `1` towards a plane through the stone (normal = the stone's axis),
    /// inside a disc of radius R and thickness 2R.
    Plane,
    /// `1` towards an axis through the stone, inside a cylinder of radius R
    /// and length 2R.
    Line,
}

/// Share of R inside which every profile ramps to zero at the centre (A10).
pub const INNER_RAMP: f32 = 0.05;

impl Profile {
    /// `p(x)` for a point profile at `x = d / R` in `0..1`, before the inner
    /// ramp.
    fn shape(self, x: f32) -> f32 {
        match self {
            Profile::Core => 1.0 - x,
            Profile::Leash => x,
            Profile::Shell => 2.0 * x - 1.0,
            Profile::Uniform | Profile::Plane | Profile::Line => 1.0,
        }
    }

    /// `p(x)` with the cutoff (A9) and the inner ramp (A10): zero at and
    /// beyond `x = 1`, and linear from zero at the centre to `p(0.05)`.
    pub fn value(self, x: f32) -> f32 {
        if !(x.is_finite() && x >= 0.0) || x >= 1.0 {
            return 0.0;
        }
        if x < INNER_RAMP {
            return self.shape(INNER_RAMP) * x / INNER_RAMP;
        }
        self.shape(x)
    }

    /// Acceleration in units of the source's magnitude at `offset` from a
    /// source with axis `axis` (unit; used by plane and line) and radius `r`.
    /// The result points towards the source (multiply by a negative
    /// magnitude to push).
    pub fn pull(self, offset: Vec3, axis: Vec3, r: f32) -> Vec3 {
        if r.is_nan() || r <= 0.0 {
            return Vec3::ZERO;
        }
        match self {
            Profile::Plane => {
                // Height above the plane, and distance along it.
                let h = offset.dot(axis);
                let along = (offset - axis * h).length();
                if along >= r || h.abs() >= r {
                    return Vec3::ZERO;
                }
                -axis * h.signum() * self.value(h.abs() / r)
            }
            Profile::Line => {
                let a = offset.dot(axis);
                let across = offset - axis * a;
                let d = across.length();
                if a.abs() >= r || d >= r || d < 1e-9 {
                    return Vec3::ZERO;
                }
                -across / d * self.value(d / r)
            }
            _ => {
                let d = offset.length();
                if d >= r || d < 1e-9 {
                    return Vec3::ZERO;
                }
                -offset / d * self.value(d / r)
            }
        }
    }
}

/// How a source's magnitude changes over time.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum TimeDial {
    #[default]
    Constant,
    /// On for `on` seconds, off for `off` seconds, starting on.
    Pulse { on: f32, off: f32 },
    /// Magnitude follows a sine between zero and full, period 1 to 20 s.
    Rhythm { period: f32 },
    /// Rises from zero to full over `seconds` (1 to 30) after opening.
    Ramp { seconds: f32 },
}

impl TimeDial {
    /// Factor `0..=1` at `t` seconds since the stone was opened.
    pub fn factor(self, t: f32) -> f32 {
        let t = t.max(0.0);
        match self {
            TimeDial::Constant => 1.0,
            TimeDial::Pulse { on, off } => {
                let period = (on + off).max(1e-3);
                if t.rem_euclid(period) < on {
                    1.0
                } else {
                    0.0
                }
            }
            TimeDial::Rhythm { period } => {
                let p = period.max(1e-3);
                0.5 - 0.5 * (std::f32::consts::TAU * t / p).cos()
            }
            TimeDial::Ramp { seconds } => (t / seconds.max(1e-3)).min(1.0),
        }
    }
}

/// Which masses respond to a source.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Selectivity {
    #[default]
    All,
    /// Only bodies with `min <= mass < max` kg.
    MassBand { min: f32, max: f32 },
}

impl Selectivity {
    pub fn accepts(self, mass: f32) -> bool {
        match self {
            Selectivity::All => true,
            Selectivity::MassBand { min, max } => mass >= min && mass < max,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_follow_the_table() {
        assert!((Profile::Core.value(0.5) - 0.5).abs() < 1e-6);
        assert!((Profile::Leash.value(0.9) - 0.9).abs() < 1e-6);
        assert!((Profile::Shell.value(0.25) + 0.5).abs() < 1e-6);
        assert!((Profile::Shell.value(0.5)).abs() < 1e-6);
        assert_eq!(Profile::Uniform.value(0.7), 1.0);
        for p in [
            Profile::Core,
            Profile::Leash,
            Profile::Shell,
            Profile::Uniform,
        ] {
            assert_eq!(p.value(1.0), 0.0, "{p:?} cuts off at R");
            assert_eq!(p.value(3.0), 0.0);
            assert_eq!(p.value(0.0), 0.0, "{p:?} ramps to zero at the centre");
            // Linear ramp inside 0.05 R.
            let edge = p.value(INNER_RAMP);
            assert!((p.value(INNER_RAMP * 0.5) - edge * 0.5).abs() < 1e-6);
        }
    }

    #[test]
    fn point_profiles_pull_towards_the_stone() {
        let a = Profile::Core.pull(Vec3::new(5.0, 0.0, 0.0), Vec3::Y, 10.0);
        assert!((a - Vec3::new(-0.5, 0.0, 0.0)).length() < 1e-6);
        // Shell pushes inside R/2.
        let a = Profile::Shell.pull(Vec3::new(2.5, 0.0, 0.0), Vec3::Y, 10.0);
        assert!((a - Vec3::new(0.5, 0.0, 0.0)).length() < 1e-6);
        assert_eq!(
            Profile::Core.pull(Vec3::new(11.0, 0.0, 0.0), Vec3::Y, 10.0),
            Vec3::ZERO
        );
    }

    #[test]
    fn plane_and_line_pull_across() {
        // Above a horizontal plane: pulled straight down, also off-centre.
        let a = Profile::Plane.pull(Vec3::new(3.0, 2.0, 1.0), Vec3::Y, 10.0);
        assert!((a - Vec3::NEG_Y).length() < 1e-6);
        let a = Profile::Plane.pull(Vec3::new(3.0, -2.0, 1.0), Vec3::Y, 10.0);
        assert!((a - Vec3::Y).length() < 1e-6);
        // Outside the disc or the slab: nothing.
        assert_eq!(
            Profile::Plane.pull(Vec3::new(11.0, 2.0, 0.0), Vec3::Y, 10.0),
            Vec3::ZERO
        );
        assert_eq!(
            Profile::Plane.pull(Vec3::new(0.0, 10.5, 0.0), Vec3::Y, 10.0),
            Vec3::ZERO
        );
        // Line along y: pulled towards the axis.
        let a = Profile::Line.pull(Vec3::new(0.0, 4.0, 3.0), Vec3::Y, 10.0);
        assert!((a - Vec3::NEG_Z).length() < 1e-6);
        assert_eq!(
            Profile::Line.pull(Vec3::new(0.0, 12.0, 3.0), Vec3::Y, 10.0),
            Vec3::ZERO
        );
    }

    #[test]
    fn dials() {
        assert_eq!(TimeDial::Pulse { on: 1.0, off: 2.0 }.factor(0.5), 1.0);
        assert_eq!(TimeDial::Pulse { on: 1.0, off: 2.0 }.factor(1.5), 0.0);
        assert_eq!(TimeDial::Pulse { on: 1.0, off: 2.0 }.factor(3.2), 1.0);
        assert!((TimeDial::Rhythm { period: 4.0 }.factor(2.0) - 1.0).abs() < 1e-6);
        assert!(TimeDial::Rhythm { period: 4.0 }.factor(0.0).abs() < 1e-6);
        assert!((TimeDial::Ramp { seconds: 10.0 }.factor(5.0) - 0.5).abs() < 1e-6);
        assert!(Selectivity::MassBand {
            min: 0.0,
            max: 50.0
        }
        .accepts(20.0));
        assert!(!Selectivity::MassBand {
            min: 0.0,
            max: 50.0
        }
        .accepts(80.0));
    }
}
