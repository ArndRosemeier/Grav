//! Heart weather in the game: the region's clock, what grav-core's
//! weather says for it now, and what that does to the light and air and
//! to the field. Flares rumble and the heart flares with them.
//!
//! `GRAV_HOUR=<h>` starts the clock at that game hour; `GRAV_LIGHT=<0..1>`
//! and `GRAV_FLARE=<g>` hold the light and a flare where they are, for
//! screenshots.

use crate::look::Look;
use engine::prelude::*;
use grav_core::{Field, Sky, Weather, WeatherTuning};

/// The test space's region: its weather's seed.
const REGION_SEED: u64 = 0x5EED_0001;

pub struct HeartWeather {
    weather: Weather,
    /// Game hours since the start.
    pub hours: f64,
    /// The weather now.
    pub now: Sky,
    /// A flare began this frame (it may shed a shard).
    pub flare_began: bool,
    hold_light: Option<f32>,
    hold_flare: Option<f32>,
    /// What the look was last set to (light, flare), so it is set again
    /// only when it has changed.
    shown: Option<(f32, f32)>,
    rumble: Option<Sound>,
    rumbling: Option<SoundId>,
}

fn env<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok().and_then(|s| s.parse().ok())
}

impl HeartWeather {
    pub fn new(tuning: &WeatherTuning) -> Self {
        let weather = Weather::new(REGION_SEED, tuning.clone());
        let hours = env::<f64>("GRAV_HOUR").unwrap_or(0.0);
        let now = weather.at(hours);
        Self {
            weather,
            hours,
            now,
            flare_began: false,
            hold_light: env("GRAV_LIGHT"),
            hold_flare: env("GRAV_FLARE"),
            shown: None,
            rumble: Sound::load("assets/sounds/flare_rumble.ogg").ok(),
            rumbling: None,
        }
    }

    /// Move the clock on by `dt` real seconds.
    pub fn update(&mut self, dt: f32) {
        let was = self.now.flare > 0.05;
        self.hours += (dt / self.weather.tuning.hour_seconds.max(1.0e-3)) as f64;
        self.now = self.weather.at(self.hours);
        if let Some(light) = self.hold_light {
            self.now.light = light.clamp(0.0, 1.0);
            self.now.bright = light >= 0.5;
        }
        if let Some(g) = self.hold_flare {
            self.now.flare_g = g.max(0.0);
            self.now.flare = if g > 0.0 { 1.0 } else { 0.0 };
        }
        self.flare_began = !was && self.now.flare > 0.05;
    }

    /// The flare's push goes into this frame's field (not the field the
    /// world was built for: a flare loads what stands, H11).
    pub fn push(&self, field: &mut Field) {
        if let Some(h) = field.heart.as_mut() {
            h.flare_g = self.now.flare_g;
        }
    }

    /// Light, air and sound for the weather now.
    pub fn show(&mut self, world: &mut World, look: &Look) -> EngineResult<()> {
        let state = (self.now.light, self.now.flare);
        let changed = self
            .shown
            .is_none_or(|(l, f)| (l - state.0).abs() > 0.004 || (f - state.1).abs() > 0.01);
        if changed {
            look.weather(world, state.0, state.1)?;
            self.shown = Some(state);
        }
        // A flare rumbles while it lasts.
        match (self.now.flare > 0.05, self.rumbling) {
            (true, None) => {
                if let Some(sound) = &self.rumble {
                    self.rumbling = Some(world.play_sound(
                        sound,
                        PlaySound::ambient().looped().volume(0.8).fade_in(4.0),
                    ));
                }
            }
            (false, Some(id)) => {
                world.stop_sound(id, 4.0);
                self.rumbling = None;
            }
            _ => {}
        }
        Ok(())
    }
}
