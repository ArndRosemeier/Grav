//! Veins: natural attracting lodestone in dead stone, the "down" of a
//! district (A16, A17, R1 to R6).

use crate::G;
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
            dir * self.surface_g * self.health.clamp(0.0, 1.0) * share * G,
            share,
        )
    }

    /// Whether `p` can feel this vein at all (a cheap test before `pull`).
    pub fn may_reach(&self, p: Vec3) -> bool {
        self.nearest(p).is_some_and(|(_, d)| d < self.fade)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!((a - Vec3::NEG_Y * G).length() < 1e-4);
        let (a, s) = v.pull(Vec3::new(0.0, 85.0, 0.0));
        assert!((s - 0.5).abs() < 1e-5);
        assert!((a.y + 0.5 * G).abs() < 1e-4);
        assert_eq!(v.pull(Vec3::new(0.0, 120.0, 0.0)).0, Vec3::ZERO);
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
