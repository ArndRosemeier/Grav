//! Heart weather (design doc, "Heart weather"; H11, H12): what the heart
//! does to one region over time. The pulses are local, so each region has
//! its own weather from its own seed: bright phases (its "day", while a
//! bright patch on the heart faces it), dim phases (its night) and flares,
//! surges of outward push. Weather cycles; it never advances a clock of
//! the story.
//!
//! Everything is worked out from the seed and the game time, so the same
//! region has the same weather on every machine, and the guild's almanac
//! (H12) is the same function asked about the hours ahead.

use serde::{Deserialize, Serialize};

/// Heart weather's numbers (the table's durations and strengths).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WeatherTuning {
    /// Real seconds per hour of game time.
    pub hour_seconds: f32,
    /// How long a bright phase lasts, hours (2 to 10).
    pub bright_hours: (f32, f32),
    /// How long a dim phase lasts, hours (2 to 20).
    pub dim_hours: (f32, f32),
    /// How long the light takes to turn between phases, hours.
    pub turn_hours: f32,
    /// The chance that a flare starts in any one hour.
    pub flare_chance: f32,
    /// How long a flare lasts, hours (minutes to hours).
    pub flare_hours: (f32, f32),
    /// The extra outward push of a flare in the open cavern, g (0.1 to 0.5).
    pub flare_push_g: (f32, f32),
    /// How long a flare takes to rise to full push, hours.
    pub flare_rise_hours: f32,
}

impl Default for WeatherTuning {
    fn default() -> Self {
        Self {
            hour_seconds: 150.0,
            bright_hours: (2.0, 10.0),
            dim_hours: (2.0, 20.0),
            turn_hours: 0.5,
            flare_chance: 0.12,
            flare_hours: (0.15, 2.0),
            flare_push_g: (0.1, 0.5),
            flare_rise_hours: 0.05,
        }
    }
}

/// The weather of one region at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sky {
    /// How much of the heart's light reaches the region, 0 (dim) to 1
    /// (bright); between the two while it turns.
    pub light: f32,
    /// In a bright phase (its day).
    pub bright: bool,
    /// Hours until the phase changes.
    pub hours_to_turn: f32,
    /// The flare's extra outward push now, g (0 when there is none).
    pub flare_g: f32,
    /// How far the flare has risen, 0 to 1 (for its look).
    pub flare: f32,
}

/// A flare in the schedule.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flare {
    /// When it starts and ends, game hours.
    pub start: f64,
    pub end: f64,
    /// Its full extra push, g.
    pub push_g: f32,
}

/// One region's weather.
#[derive(Clone, Debug, PartialEq)]
pub struct Weather {
    seed: u64,
    pub tuning: WeatherTuning,
}

/// A number from 0 to 1 for (`seed`, `n`, `salt`), the same everywhere.
fn unit(seed: u64, n: u64, salt: u64) -> f32 {
    let mut x =
        seed ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt.wrapping_mul(0xD1B5_4A32_D192_ED03);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

fn lerp(range: (f32, f32), t: f32) -> f32 {
    range.0 + (range.1 - range.0) * t
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Weather {
    pub fn new(seed: u64, tuning: WeatherTuning) -> Self {
        Self { seed, tuning }
    }

    /// How long phase `k` lasts, hours: even phases are bright, odd dim.
    fn phase_hours(&self, k: u64) -> f32 {
        let range = if k.is_multiple_of(2) {
            self.tuning.bright_hours
        } else {
            self.tuning.dim_hours
        };
        lerp(range, unit(self.seed, k, 1)).max(self.tuning.turn_hours * 2.0)
    }

    /// Which phase `hours` falls in, and how far into it. The game starts
    /// a third of the way into a bright phase.
    fn phase_at(&self, hours: f64) -> (u64, f64, f64) {
        let mut t = hours.max(0.0) + self.phase_hours(0) as f64 / 3.0;
        let mut k = 0;
        loop {
            let d = self.phase_hours(k) as f64;
            if t < d {
                return (k, t, d);
            }
            t -= d;
            k += 1;
        }
    }

    /// The flare that may start in hour `n`.
    fn flare_in(&self, n: u64) -> Option<Flare> {
        if unit(self.seed, n, 2) >= self.tuning.flare_chance {
            return None;
        }
        let start = n as f64 + unit(self.seed, n, 3) as f64;
        let hours = lerp(self.tuning.flare_hours, unit(self.seed, n, 4));
        Some(Flare {
            start,
            end: start + hours as f64,
            push_g: lerp(self.tuning.flare_push_g, unit(self.seed, n, 5)),
        })
    }

    /// The flares that touch the hours from `from` to `to`.
    pub fn flares(&self, from: f64, to: f64) -> Vec<Flare> {
        let longest = self.tuning.flare_hours.1.ceil() as f64 + 1.0;
        let first = (from - longest).max(0.0).floor() as u64;
        let last = to.max(0.0).floor() as u64;
        (first..=last)
            .filter_map(|n| self.flare_in(n))
            .filter(|f| f.end > from && f.start < to)
            .collect()
    }

    /// The weather at `hours` of game time.
    pub fn at(&self, hours: f64) -> Sky {
        let (k, x, d) = self.phase_at(hours);
        let level = |k: u64| if k.is_multiple_of(2) { 1.0 } else { 0.0 };
        let half = self.tuning.turn_hours as f64 * 0.5;
        let here = level(k);
        let mut light = here;
        if x < half && k > 0 {
            light =
                level(k - 1) + (here - level(k - 1)) * smooth(-half as f32, half as f32, x as f32);
        }
        if d - x < half {
            light =
                here + (level(k + 1) - here) * smooth(-half as f32, half as f32, (x - d) as f32);
        }
        let mut flare_g: f32 = 0.0;
        let mut flare: f32 = 0.0;
        let rise = self.tuning.flare_rise_hours.max(1.0e-3) as f64;
        for f in self.flares(hours, hours) {
            let up = smooth(0.0, 1.0, ((hours - f.start) / rise) as f32);
            let down = 1.0
                - smooth(
                    0.0,
                    1.0,
                    ((hours - (f.end - 2.0 * rise)) / (2.0 * rise)) as f32,
                );
            let env = up * down;
            if f.push_g * env > flare_g {
                flare_g = f.push_g * env;
                flare = env;
            }
        }
        Sky {
            light,
            bright: k.is_multiple_of(2),
            hours_to_turn: (d - x) as f32,
            flare_g,
            flare,
        }
    }

    /// The almanac (H12): the weather every `step` hours from `from` for
    /// `ahead` hours.
    pub fn forecast(&self, from: f64, ahead: f64, step: f64) -> Vec<(f64, Sky)> {
        let n = (ahead / step.max(1.0e-3)).ceil() as usize;
        (0..=n)
            .map(|i| {
                let t = from + i as f64 * step;
                (t, self.at(t))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weather() -> Weather {
        Weather::new(7, WeatherTuning::default())
    }

    #[test]
    #[ignore = "full"]
    fn days_and_nights_take_their_turns_within_their_lengths() {
        let w = weather();
        let t = &w.tuning;
        // Walk 400 hours in steps of a minute and time each phase.
        let mut last = w.at(0.0);
        assert!(last.bright, "the game starts by day");
        let mut since = 0.0;
        let mut phases = 0;
        let step = 1.0 / 60.0;
        for i in 1..24_000 {
            let s = w.at(i as f64 * step);
            assert!((0.0..=1.0).contains(&s.light));
            if s.bright != last.bright {
                if phases > 0 {
                    let (lo, hi) = if last.bright {
                        t.bright_hours
                    } else {
                        t.dim_hours
                    };
                    assert!(since >= lo - 0.02 && since <= hi + 0.02, "{since} h");
                }
                phases += 1;
                since = 0.0;
            }
            since += step as f32;
            last = s;
        }
        assert!(phases > 20, "{phases}");
    }

    #[test]
    #[ignore = "full"]
    fn flares_come_now_and_then_within_their_strength() {
        let w = weather();
        let flares = w.flares(0.0, 1000.0);
        let share = flares.len() as f32 / 1000.0;
        assert!((0.06..0.2).contains(&share), "{share}");
        for f in &flares {
            assert!((0.1..=0.5).contains(&f.push_g));
            let mid = w.at((f.start + f.end) / 2.0);
            assert!(mid.flare_g > 0.0 && mid.flare_g <= 0.5);
        }
        // Between flares there is no push.
        let calm = (0..1000)
            .map(|h| h as f64 + 0.5)
            .find(|&h| w.flares(h, h).is_empty())
            .unwrap();
        assert_eq!(w.at(calm).flare_g, 0.0);
    }

    #[test]
    #[ignore = "full"]
    fn the_same_seed_gives_the_same_weather_and_the_almanac_agrees() {
        let (a, b) = (weather(), weather());
        for h in [0.0, 3.3, 17.9, 250.25] {
            assert_eq!(a.at(h), b.at(h));
        }
        let other = Weather::new(8, WeatherTuning::default());
        assert!((0..200).any(|h| a.at(h as f64).bright != other.at(h as f64).bright));
        let days = a.forecast(10.0, 6.0, 0.5);
        assert_eq!(days.len(), 13);
        assert_eq!(days[4].1, a.at(12.0));
    }
}
