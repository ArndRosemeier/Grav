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
    /// Steepest slope walked up and stood on at 1 g, degrees against the
    /// pull (P4). Boots on rock: steep.
    pub climb_slope_deg: f32,
    /// The slope limit's tangent scales with pull^-this: light pull lets
    /// him climb steeper, heavy pull less (P4).
    pub climb_slope_exp: f32,
    /// The slope limit stays within these, degrees.
    pub climb_slope_min_deg: f32,
    pub climb_slope_max_deg: f32,
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
    /// Once sliding, friction brakes with this share of the grip that
    /// holds a standing hero (rock on boots: a slide stops quickly) (P9).
    pub slide_friction: f32,
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
            climb_slope_deg: 50.0,
            climb_slope_exp: 0.5,
            climb_slope_min_deg: 30.0,
            climb_slope_max_deg: 72.0,
            drift_below_g: 0.05,
            walk_speed: 3.6,
            jump_height_at_1g: 1.2,
            safe_impact: 8.0,
            lethal_impact: 22.0,
            turn_deg_per_s: 90.0,
            camera_smoothing: 0.5,
            sprint_factor: 1.5,
            slide_friction: 0.8,
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

/// P4: the steepest slope (degrees against the pull) the hero walks up
/// and stands on under a pull of `pull_g`: steeper in light pull, less in
/// heavy.
pub fn climb_slope_deg(m: &Movement, pull_g: f32) -> f32 {
    let tan = m.climb_slope_deg.to_radians().tan() * pull_g.max(1e-3).powf(-m.climb_slope_exp);
    tan.atan()
        .to_degrees()
        .clamp(m.climb_slope_min_deg, m.climb_slope_max_deg)
}

/// P9: the grip of his feet under `accel`: the pull along the ground he
/// braces against, as a share of the press onto it. It holds exactly up to
/// the slope he can climb.
pub fn grip(m: &Movement, accel: Vec3) -> f32 {
    climb_slope_deg(m, accel.length() / G).to_radians().tan()
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
    press * strength >= m.walk_min_g * G
        && press >= climb_slope_deg(m, strength / G).to_radians().cos()
}

/// The pull split against a surface with normal `normal`: how hard it
/// presses onto it (m/s², negative when it pulls away) and its part along
/// the surface.
pub fn press_and_drag(accel: Vec3, normal: Vec3) -> (f32, Vec3) {
    let n = normal.normalize_or_zero();
    let press = -accel.dot(n);
    (press, accel + n * press)
}

/// P9: the slide velocity along the ground after `dt` seconds, from `v`.
/// A standing hero stays put while his grip holds the pull along the
/// ground; once he slides, the drag speeds him and friction brakes him, so
/// a slide on rock dies away within moments where the ground levels off.
pub fn slide_step(m: &Movement, accel: Vec3, normal: Vec3, v: Vec3, dt: f32) -> Vec3 {
    let (press, drag) = press_and_drag(accel, normal);
    let press = press.max(0.0);
    let hold = grip(m, accel) * press;
    if v.length() < 1e-3 && drag.length() <= hold {
        return Vec3::ZERO;
    }
    let brake = hold * m.slide_friction * dt;
    let moved = v + drag * dt;
    let speed = moved.length();
    if speed <= brake {
        Vec3::ZERO
    } else {
        (moved * ((speed - brake) / speed)).clamp_length_max(m.slide_max)
    }
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
    fn steep_rock_holds_and_light_pull_climbs_steeper() {
        let m = Movement::default();
        assert!((climb_slope_deg(&m, 1.0) - 50.0).abs() < 1e-3);
        assert!(climb_slope_deg(&m, 0.3) > 62.0);
        assert!(climb_slope_deg(&m, 2.0) < 42.0);
        let slope = |deg: f32| {
            let a = deg.to_radians();
            Vec3::new(a.sin(), a.cos(), 0.0)
        };
        // 1 g straight down on a 45 degree slope: walkable; 55: not.
        assert!(can_walk(&m, Vec3::NEG_Y * G, slope(45.0)));
        assert!(!can_walk(&m, Vec3::NEG_Y * G, slope(55.0)));
        // The same 55 degree slope in 0.3 g: walkable.
        assert!(can_walk(&m, Vec3::NEG_Y * 0.3 * G, slope(55.0)));
    }

    #[test]
    fn a_slide_needs_a_slope_beyond_the_grip_and_dies_on_level_rock() {
        let m = Movement::default();
        let down = Vec3::NEG_Y * G;
        let slope = |deg: f32| {
            let a = deg.to_radians();
            Vec3::new(-a.sin(), a.cos(), 0.0)
        };
        let dt = 1.0 / 60.0;
        // Standing on 40 degrees: he holds.
        assert_eq!(
            slide_step(&m, down, slope(40.0), Vec3::ZERO, dt),
            Vec3::ZERO
        );
        // On 60 degrees he slides down (the slope falls towards -x).
        let mut v = Vec3::ZERO;
        for _ in 0..30 {
            v = slide_step(&m, down, slope(60.0), v, dt);
        }
        assert!(v.x < -0.5 && v.length() > 1.0, "{v}");
        // Out onto level rock at 5 m/s: stopped within a second, and a
        // shallow dip never keeps him rocking.
        let mut v = Vec3::X * 5.0;
        for _ in 0..60 {
            v = slide_step(&m, down, Vec3::Y, v, dt);
        }
        assert_eq!(v, Vec3::ZERO);
        let mut v = Vec3::X * 3.0;
        for i in 0..120 {
            let side = if v.x > 0.0 { 15.0 } else { -15.0 };
            v = slide_step(&m, down, slope(side), v, dt);
            if i > 60 {
                assert_eq!(v, Vec3::ZERO);
            }
        }
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
