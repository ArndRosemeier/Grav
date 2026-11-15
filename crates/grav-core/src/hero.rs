//! The hero's body rules under attraction (P4 to P7).

use crate::G;
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// The movement numbers (from [`crate::Tuning`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Movement {
    /// Walking needs at least this press against the surface, in g (P4).
    pub walk_min_g: f32,
    /// Steepest slope against local down, degrees (P4).
    pub walk_max_slope_deg: f32,
    /// Below this the hero drifts, in g (P5).
    pub drift_below_g: f32,
    /// Constant walk speed, m/s (P6).
    pub walk_speed: f32,
    /// Jump height at 1 g, metres (P6): sets the constant jump impulse.
    pub jump_height_at_1g: f32,
    /// Impact speed below which a fall does no harm, m/s (P7).
    pub safe_impact: f32,
    /// Impact speed that kills, m/s (P7).
    pub lethal_impact: f32,
    /// Body turn rate to a new down, degrees per second (P2).
    pub turn_deg_per_s: f32,
    /// Camera up smoothing, seconds (P3).
    pub camera_smoothing: f32,
}

impl Default for Movement {
    fn default() -> Self {
        Self {
            walk_min_g: 0.15,
            walk_max_slope_deg: 45.0,
            drift_below_g: 0.05,
            walk_speed: 4.5,
            jump_height_at_1g: 1.2,
            safe_impact: 8.0,
            lethal_impact: 22.0,
            turn_deg_per_s: 180.0,
            camera_smoothing: 0.5,
        }
    }
}

/// P6: the constant jump speed that reaches `height_at_1g` under 1 g, so
/// the height scales with 1 / pull.
pub fn jump_speed(m: &Movement) -> f32 {
    (2.0 * m.jump_height_at_1g * G).sqrt()
}

/// P7: damage as a share of full health (1 = lethal) for an impact speed.
pub fn fall_damage(m: &Movement, impact_speed: f32) -> f32 {
    let span = (m.lethal_impact - m.safe_impact).max(1e-3);
    ((impact_speed - m.safe_impact) / span).clamp(0.0, 1.0)
}

/// P4: whether the field `accel` presses the hero onto a surface with
/// normal `normal` hard enough, at a gentle enough slope, to walk.
pub fn can_walk(m: &Movement, accel: Vec3, normal: Vec3) -> bool {
    let strength = accel.length();
    if strength < m.walk_min_g * G {
        return false;
    }
    let down = accel / strength;
    let press = -down.dot(normal.normalize_or_zero());
    press * strength >= m.walk_min_g * G && press >= m.walk_max_slope_deg.to_radians().cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jump_height_scales_with_one_over_pull() {
        let m = Movement::default();
        let v = jump_speed(&m);
        let h = |g: f32| v * v / (2.0 * g * G);
        assert!((h(1.0) - 1.2).abs() < 1e-4);
        assert!((h(0.4) - 3.0).abs() < 1e-3);
        assert!((h(1.8) - 0.667).abs() < 0.01);
    }

    #[test]
    fn falls_hurt_from_8_and_kill_at_22() {
        let m = Movement::default();
        assert_eq!(fall_damage(&m, 7.9), 0.0);
        assert!((fall_damage(&m, 15.0) - 0.5).abs() < 1e-6);
        assert_eq!(fall_damage(&m, 30.0), 1.0);
    }

    #[test]
    fn walking_needs_press_and_a_gentle_slope() {
        let m = Movement::default();
        // On a ceiling with down pointing up into it: walkable.
        assert!(can_walk(&m, Vec3::Y * G, Vec3::NEG_Y));
        // Too weak.
        assert!(!can_walk(&m, Vec3::NEG_Y * 0.1 * G, Vec3::Y));
        // A wall when down is straight down: too steep.
        assert!(!can_walk(&m, Vec3::NEG_Y * G, Vec3::X));
    }
}
