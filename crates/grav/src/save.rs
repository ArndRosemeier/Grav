//! Saving and loading a sandbox game (plan stage 5): where the hero
//! stands and faces, every stone (box state, program, open faces, clocks),
//! the throw range and the region's clock (heart weather).
//!
//! Not kept: what broke. Buildings, goods and people come back as the
//! world made them, as rubble cannot be stored yet.
//!
//! The file is `savegame.json` next to the game (git-ignored).

use grav_core::Stone;
use serde::{Deserialize, Serialize};

pub const SAVE_FILE: &str = "savegame.json";
/// Raised when the file's meaning changes; older files are refused.
const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SaveGame {
    pub version: u32,
    pub feet: [f32; 3],
    pub up: [f32; 3],
    pub heading: [f32; 3],
    pub stones: Vec<Stone>,
    pub selected: usize,
    pub range: f32,
    /// Game hours since the start (heart weather).
    pub hours: f64,
}

impl SaveGame {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        feet: grav_core::Vec3,
        up: grav_core::Vec3,
        heading: grav_core::Vec3,
        stones: Vec<Stone>,
        selected: usize,
        range: f32,
        hours: f64,
    ) -> Self {
        Self {
            version: VERSION,
            feet: feet.into(),
            up: up.into(),
            heading: heading.into(),
            stones,
            selected,
            range,
            hours,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let s: SaveGame = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if s.version != VERSION {
            return Err(format!(
                "saved by another version ({}, this one reads {VERSION})",
                s.version
            ));
        }
        let finite = |v: [f32; 3]| v.iter().all(|x| x.is_finite());
        if !finite(s.feet) || !finite(s.up) || !finite(s.heading) || !s.hours.is_finite() {
            return Err("the hero's place is not a finite point".into());
        }
        Ok(s)
    }

    pub fn write(&self) -> Result<(), String> {
        std::fs::write(SAVE_FILE, self.to_json()).map_err(|e| format!("{SAVE_FILE}: {e}"))
    }

    pub fn read() -> Result<Self, String> {
        let text = std::fs::read_to_string(SAVE_FILE).map_err(|e| format!("{SAVE_FILE}: {e}"))?;
        Self::from_json(&text)
    }

    pub fn exists() -> bool {
        std::path::Path::new(SAVE_FILE).exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grav_core::{BoxState, Tuning, Vec3};

    // A saved game must come back exactly, or the player loses their stones.
    #[test]
    fn a_saved_game_comes_back_as_it_was() {
        let spec = Tuning::default().start_stones[0].clone();
        let mut stone = Stone::new(1, spec.clone());
        stone.state = BoxState::Resting {
            at: Vec3::new(1.0, 2.0, 3.0),
        };
        stone.open[2] = true;
        stone.since_rest = 4.5;
        let s = SaveGame::new(
            Vec3::new(5.0, 6.0, 7.0),
            Vec3::Y,
            Vec3::Z,
            vec![stone, Stone::new(2, spec)],
            1,
            12.0,
            7.25,
        );
        assert_eq!(SaveGame::from_json(&s.to_json()).unwrap(), s);
        let old = s.to_json().replace("\"version\": 1", "\"version\": 0");
        assert!(SaveGame::from_json(&old).is_err());
    }
}
