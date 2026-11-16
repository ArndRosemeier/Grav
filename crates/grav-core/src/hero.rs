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
    /// Sprinting multiplies the walking speed by this.
    pub sprint_factor: f32,
    /// Feeling the pull (P9 to P12).
    /// Grip of the feet: the pull along the ground the hero can brace
    /// against, as a share of the press onto it (P9).
    pub grip: f32,
    /// Top sliding speed, m/s (P9).
    pub slide_max: f32,
    /// How much pull along the walking direction speeds or slows the walk:
    /// speed changes by this share per g (P10).
    pub walk_pull_gain: f32,
    /// Walk speed scales with pull^-this (heavy is slow, light is fast), and
    /// stays within `weight_speed_min..max` of `walk_speed` (P11).
    pub weight_speed_exp: f32,
    pub weight_speed_min: f32,
    pub weight_speed_max: f32,
    /// How fast the hero starts, stops and turns his walk at 1 g, m/s²;
    /// scales with the press onto the ground (P11).
    pub move_accel: f32,
    /// A change of pull of at least this much (in g) within
    /// `jolt_window` seconds makes the hero stumble (P12).
    pub stumble_change_g: f32,
    pub jolt_window: f32,
    /// How long a stumble lasts, and the share of control left in it (P12).
    pub stumble_seconds: f32,
    pub stumble_control: f32,
    /// The yank of a jolt: the change of pull acts on the hero for this
    /// long at once, seconds (P12).
    pub jolt_seconds: f32,
}

impl Default for Movement {
    fn default() -> Self {
        Self {
            walk_min_g: 0.15,
            walk_max_slope_deg: 45.0,
            drift_below_g: 0.05,
            walk_speed: 3.6,
            jump_height_at_1g: 1.2,
            safe_impact: 8.0,
            lethal_impact: 22.0,
            turn_deg_per_s: 90.0,
            camera_smoothing: 0.5,
            sprint_factor: 1.5,
            grip: 0.3,
            slide_max: 14.0,
            walk_pull_gain: 1.0,
            weight_speed_exp: 0.5,
            weight_speed_min: 0.6,
            weight_speed_max: 1.5,
            move_accel: 22.0,
            stumble_change_g: 0.35,
            jolt_window: 0.25,
            stumble_seconds: 0.8,
            stumble_control: 0.25,
            jolt_seconds: 0.15,
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

/// The pull split against a surface with normal `normal`: how hard it
/// presses onto it (m/s², negative when it pulls away) and its part along
/// the surface.
pub fn press_and_drag(accel: Vec3, normal: Vec3) -> (f32, Vec3) {
    let n = normal.normalize_or_zero();
    let press = -accel.dot(n);
    (press, accel + n * press)
}

/// P9: the acceleration that drags a standing hero along the ground: the
/// pull along the surface beyond what his grip holds. Zero while he can
/// brace against it.
pub fn slide_accel(m: &Movement, accel: Vec3, normal: Vec3) -> Vec3 {
    let (press, drag) = press_and_drag(accel, normal);
    let hold = m.grip * press.max(0.0);
    let d = drag.length();
    if d <= hold || d < 1e-6 {
        Vec3::ZERO
    } else {
        drag * ((d - hold) / d)
    }
}

/// P9: how fast grip brakes a slide once the drag is gone, m/s².
pub fn slide_brake(m: &Movement, accel: Vec3, normal: Vec3) -> f32 {
    let (press, drag) = press_and_drag(accel, normal);
    (m.grip * press.max(0.0) - drag.length()).max(0.0)
}

/// P10 and P11: the walking speed in direction `dir` (a unit vector along
/// the ground): slower in heavy pull and against the pull, faster in light
/// pull and with it.
pub fn walk_speed_in(m: &Movement, accel: Vec3, normal: Vec3, dir: Vec3) -> f32 {
    let (press, drag) = press_and_drag(accel, normal);
    let weight = (press.max(1e-3) / G)
        .powf(-m.weight_speed_exp)
        .clamp(m.weight_speed_min, m.weight_speed_max);
    let along = (1.0 + m.walk_pull_gain * drag.dot(dir) / G).clamp(0.25, 1.75);
    m.walk_speed * weight * along
}

/// P11: how fast the walk velocity may change, m/s²: planted in heavy
/// pull, floaty in light pull.
pub fn walk_accel(m: &Movement, accel: Vec3, normal: Vec3) -> f32 {
    let (press, _) = press_and_drag(accel, normal);
    m.move_accel * (press.max(0.0) / G).clamp(0.1, 3.0)
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

    #[test]
    fn a_sideways_pull_drags_once_it_beats_the_grip() {
        let m = Movement::default();
        let n = Vec3::Y;
        // Straight down: nothing drags.
        assert_eq!(slide_accel(&m, Vec3::NEG_Y * G, n), Vec3::ZERO);
        // A 10 degree lean (tan = 0.18 < grip 0.3): he braces.
        let lean = |deg: f32| {
            let a = deg.to_radians();
            Vec3::new(a.sin(), -a.cos(), 0.0) * G
        };
        assert_eq!(slide_accel(&m, lean(10.0), n), Vec3::ZERO);
        assert!(slide_brake(&m, lean(10.0), n) > 0.5);
        // 30 degrees (tan = 0.58): he slides towards +x.
        let s = slide_accel(&m, lean(30.0), n);
        assert!(s.x > 1.5 && s.y.abs() < 1e-5, "{s}");
        // A pull straight along the ground drags with all of it.
        let s = slide_accel(&m, Vec3::X * G, n);
        assert!((s.x - G).abs() < 1e-4);
    }

    #[test]
    fn heavy_is_slow_light_is_fast_and_the_pull_helps_or_hinders() {
        let m = Movement::default();
        let n = Vec3::Y;
        let at = |g: f32| walk_speed_in(&m, Vec3::NEG_Y * g * G, n, Vec3::X);
        assert!((at(1.0) - m.walk_speed).abs() < 1e-4);
        assert!(at(1.8) < at(1.0) * 0.8 && at(0.4) > at(1.0) * 1.4);
        let leaning = Vec3::new(0.2, -0.98, 0.0) * G;
        let with = walk_speed_in(&m, leaning, n, Vec3::X);
        let against = walk_speed_in(&m, leaning, n, Vec3::NEG_X);
        assert!(with > m.walk_speed * 1.15 && against < m.walk_speed * 0.85);
        assert!(
            walk_accel(&m, Vec3::NEG_Y * 0.4 * G, n) < walk_accel(&m, Vec3::NEG_Y * G, n) * 0.5
        );
    }
}
