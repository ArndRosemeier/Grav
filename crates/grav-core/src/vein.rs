//! Veins: natural attracting lodestone in dead stone, the "down" of a
//! district (A16, A17, R1 to R6).

use crate::one_g;
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

/// One piece of a vein's surface. A vein is a union of pieces.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum VeinShape {
    Sphere {
        center: Vec3,
        radius: f32,
    },
    /// A tube from `a` to `b`.
    Capsule {
        a: Vec3,
        b: Vec3,
        radius: f32,
    },
    /// A tube from `a` to `b` whose radius changes evenly from `radius_a`
    /// to `radius_b` (a cone with round ends): the vein in a tapering
    /// spire.
    Cone {
        a: Vec3,
        b: Vec3,
        radius_a: f32,
        radius_b: f32,
    },
    /// An oriented box with half extents `half`.
    Box {
        center: Vec3,
        rotation: Quat,
        half: Vec3,
    },
}

impl VeinShape {
    /// Nearest point on this piece's surface (or inside it, the point
    /// itself) and the distance outside it (0 inside).
    pub fn nearest(&self, p: Vec3) -> (Vec3, f32) {
        match *self {
            VeinShape::Sphere { center, radius } => {
                let d = p - center;
                let len = d.length();
                if len <= radius {
                    (p, 0.0)
                } else {
                    (center + d / len * radius, len - radius)
                }
            }
            VeinShape::Capsule { a, b, radius } => {
                let ab = b - a;
                let t = if ab.length_squared() > 1e-12 {
                    ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let c = a + ab * t;
                let d = p - c;
                let len = d.length();
                if len <= radius {
                    (p, 0.0)
                } else {
                    (c + d / len * radius, len - radius)
                }
            }
            VeinShape::Cone {
                a,
                b,
                radius_a,
                radius_b,
            } => cone_nearest(p, a, b, radius_a, radius_b),
            VeinShape::Box {
                center,
                rotation,
                half,
            } => {
                let local = rotation.inverse() * (p - center);
                let clamped = local.clamp(-half, half);
                let q = center + rotation * clamped;
                (q, (local - clamped).length())
            }
        }
    }
}

/// Nearest surface point of a round cone and the distance outside it.
fn cone_nearest(p: Vec3, a: Vec3, b: Vec3, ra: f32, rb: f32) -> (Vec3, f32) {
    let ab = b - a;
    let h = ab.length();
    let sphere = |c: Vec3, r: f32| {
        let d = p - c;
        let len = d.length();
        if len <= r {
            (p, 0.0)
        } else {
            (c + d / len * r, len - r)
        }
    };
    if h < 1e-6 || (ra - rb).abs() >= h {
        // One end's ball holds the other: the bigger ball is the shape.
        return if ra >= rb {
            sphere(a, ra)
        } else {
            sphere(b, rb)
        };
    }
    let axis = ab / h;
    // In the plane through the axis: x along it, y away from it.
    let x = (p - a).dot(axis);
    let off = p - a - axis * x;
    let y = off.length();
    let out = if y > 1e-6 {
        off / y
    } else {
        axis.any_orthonormal_vector()
    };
    // The side line touches both end balls; (cos, sin) is its outward
    // normal in that plane.
    let sin = (ra - rb) / h;
    let cos = (1.0 - sin * sin).sqrt();
    let k = -sin * y + cos * x;
    if k < 0.0 {
        return sphere(a, ra);
    }
    if k > cos * h {
        return sphere(b, rb);
    }
    let d = cos * y + sin * x - ra;
    if d <= 0.0 {
        return (p, 0.0);
    }
    let normal = out * cos + axis * sin;
    (p - normal * d, d)
}

/// A vein and its hold zone.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vein {
    pub name: String,
    pub shapes: Vec<VeinShape>,
    /// Pull at the surface in g.
    pub surface_g: f32,
    /// Constant pull up to this far from the surface (A16: 70 m).
    pub hold: f32,
    /// Fades linearly to zero at this distance (A16: 100 m).
    pub fade: f32,
    /// 1 full, 0 dead (R6); scales the pull.
    pub health: f32,
    /// Inhabited veins cap flares (H11).
    pub inhabited: bool,
}

impl Vein {
    pub fn new(name: impl Into<String>, shapes: Vec<VeinShape>, surface_g: f32) -> Self {
        Self {
            name: name.into(),
            shapes,
            surface_g,
            hold: 70.0,
            fade: 100.0,
            health: 1.0,
            inhabited: true,
        }
    }

    /// Nearest surface point over all pieces and the distance to it.
    pub fn nearest(&self, p: Vec3) -> Option<(Vec3, f32)> {
        self.shapes
            .iter()
            .map(|s| s.nearest(p))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// How much of full pull reaches a point `d` metres from the surface:
    /// 1 inside the hold distance, fading linearly to 0 at the fade distance.
    pub fn strength_at(&self, d: f32) -> f32 {
        if d <= self.hold {
            1.0
        } else if d >= self.fade || self.fade <= self.hold {
            0.0
        } else {
            1.0 - (d - self.hold) / (self.fade - self.hold)
        }
    }

    /// The vein's pull at `p` in m/s² and its share of full strength
    /// (0..1, used for shielding the heart's push, A7).
    pub fn pull(&self, p: Vec3) -> (Vec3, f32) {
        let Some((q, d)) = self.nearest(p) else {
            return (Vec3::ZERO, 0.0);
        };
        let share = self.strength_at(d);
        if share <= 0.0 || d < 1e-4 {
            // On or inside the surface there is no direction: bodies never
            // get there (veins sit in dead stone), so give no pull.
            return (Vec3::ZERO, share);
        }
        let dir = (q - p) / d;
        (
            dir * self.surface_g * self.health.clamp(0.0, 1.0) * share * one_g(),
            share,
        )
    }

    /// The potential of this vein's pull `d` metres from its surface, in
    /// g times metres: the pull is its downhill slope. It rises by the pull
    /// per metre through the hold zone, levels off across the fade band and
    /// is flat beyond it.
    pub fn potential_at(&self, d: f32) -> f32 {
        let g = self.surface_g * self.health.clamp(0.0, 1.0);
        let d = d.max(0.0);
        if d <= self.hold || self.fade <= self.hold {
            return g * d.min(self.hold);
        }
        let band = self.fade - self.hold;
        let x = (d - self.hold).min(band);
        g * (self.hold + x - x * x / (2.0 * band))
    }

    /// The potential of this vein's pull at `p` (see `potential_at`).
    /// Loose matter comes to rest on surfaces where the summed potential of
    /// all veins is equal, so settled rock follows those surfaces.
    pub fn potential(&self, p: Vec3) -> f32 {
        self.nearest(p).map_or(0.0, |(_, d)| self.potential_at(d))
    }

    /// Whether `p` can feel this vein at all (a cheap test before `pull`).
    pub fn may_reach(&self, p: Vec3) -> bool {
        self.nearest(p).is_some_and(|(_, d)| d < self.fade)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cone_vein_pulls_square_to_its_side() {
        let cone = VeinShape::Cone {
            a: Vec3::ZERO,
            b: Vec3::new(0.0, 0.0, 100.0),
            radius_a: 30.0,
            radius_b: 10.0,
        };
        // Beside the middle (radius 20 there), 10 m out from the side.
        let slope = (20.0f32 / 100.0).atan();
        let normal = Vec3::new(0.0, slope.cos(), slope.sin());
        let side = Vec3::new(0.0, 20.0, 50.0);
        let p = side + normal * 10.0;
        let (q, d) = cone.nearest(p);
        // (The side touches the end balls, so it stands a little proud.)
        assert!((d - 9.6).abs() < 0.1, "{d}");
        assert!(((p - q).normalize() - normal).length() < 0.01, "{q}");
        // Inside, and past each end.
        assert_eq!(cone.nearest(Vec3::new(0.0, 5.0, 50.0)).1, 0.0);
        let (q, d) = cone.nearest(Vec3::new(0.0, 0.0, 130.0));
        assert!((d - 20.0).abs() < 1e-3 && (q.z - 110.0).abs() < 1e-3);
        let (_, d) = cone.nearest(Vec3::new(0.0, 0.0, -50.0));
        assert!((d - 20.0).abs() < 1e-3);
    }

    fn slab() -> Vein {
        // A flat vein under y = 0.
        Vein::new(
            "floor",
            vec![VeinShape::Box {
                center: Vec3::new(0.0, -5.0, 0.0),
                rotation: Quat::IDENTITY,
                half: Vec3::new(500.0, 5.0, 500.0),
            }],
            1.0,
        )
    }

    #[test]
    fn a_vein_pulls_constant_then_fades() {
        let v = slab();
        let (a, s) = v.pull(Vec3::new(0.0, 10.0, 0.0));
        assert_eq!(s, 1.0);
        assert!((a - Vec3::NEG_Y * one_g()).length() < 1e-4);
        let (a, s) = v.pull(Vec3::new(0.0, 85.0, 0.0));
        assert!((s - 0.5).abs() < 1e-5);
        assert!((a.y + 0.5 * one_g()).abs() < 1e-4);
        assert_eq!(v.pull(Vec3::new(0.0, 120.0, 0.0)).0, Vec3::ZERO);
    }

    #[test]
    fn the_pull_is_the_downhill_slope_of_the_potential() {
        let v = slab();
        for y in [5.0_f32, 40.0, 72.0, 85.0, 97.0, 130.0] {
            let h = 0.05;
            let slope = (v.potential(Vec3::new(0.0, y + h, 0.0))
                - v.potential(Vec3::new(0.0, y - h, 0.0)))
                / (2.0 * h);
            let pull = v.pull(Vec3::new(0.0, y, 0.0)).0.y / one_g();
            assert!((slope + pull).abs() < 2e-3, "at {y}: {slope} and {pull}");
        }
    }

    #[test]
    fn a_dead_vein_holds_nothing() {
        let mut v = slab();
        v.health = 0.0;
        assert_eq!(v.pull(Vec3::new(0.0, 10.0, 0.0)).0, Vec3::ZERO);
    }

    #[test]
    fn down_turns_around_a_capsule_spire_vein() {
        let v = Vein::new(
            "spire",
            vec![VeinShape::Capsule {
                a: Vec3::new(0.0, -100.0, 0.0),
                b: Vec3::new(0.0, 100.0, 0.0),
                radius: 20.0,
            }],
            1.0,
        );
        let east = v.pull(Vec3::new(30.0, 0.0, 0.0)).0;
        let west = v.pull(Vec3::new(-30.0, 0.0, 0.0)).0;
        assert!(east.x < 0.0 && west.x > 0.0, "opposite downs (R1)");
    }
}
