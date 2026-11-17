//! Every number to tune, as data (design doc: "keep them as data, not as
//! constants in code"). `Tuning::default()` holds the design's first values;
//! the game loads `tuning.json` over them when it exists.

use crate::bonds::BondClass;
use crate::hero::Movement;
use crate::profile::Profile;
use crate::source::Charge;
use crate::stone::{Flight, StoneSpec};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tuning {
    pub movement: Movement,
    /// Bond strength in g per class (W1 table).
    pub bond_light: f32,
    pub bond_timber: f32,
    pub bond_masonry: f32,
    pub bond_dwarven: f32,
    pub bond_guild: f32,
    /// The hero's starting stones, each in its box.
    pub start_stones: Vec<StoneSpec>,
    /// How a thrown box flies (S17).
    pub flight: Flight,
    /// Hand stones' limits (sandbox palette): magnitude and radius.
    pub hand_max_g: f32,
    pub hand_radius: (f32, f32),
    pub loot_max_g: f32,
    pub loot_max_radius: f32,
    /// Heart weather's lengths and strengths.
    pub weather: crate::weather::WeatherTuning,
}

impl Default for Tuning {
    fn default() -> Self {
        let common = StoneSpec {
            name: "Common stone".into(),
            charge: Charge::Attracting,
            max_g: 1.5,
            radius: 15.0,
            profile: Profile::Core,
            time: Default::default(),
            selectivity: Default::default(),
            slots: 1,
        };
        let shard = StoneSpec {
            name: "Repelling shard".into(),
            charge: Charge::Repelling,
            max_g: 1.0,
            radius: 10.0,
            profile: Profile::Core,
            time: Default::default(),
            selectivity: Default::default(),
            slots: 0,
        };
        Self {
            movement: Movement::default(),
            // A common stone (1.5 g, 15 m) breaks light work within about
            // 3 m and timber within 2 m; masonry only right on it (Arnd,
            // 2026-11-16: a stone anywhere near a building collapsed it,
            // with 0.3 and 0.8 a whole street went). A stone gathering
            // goods in front of a shack must not bring the shack down.
            bond_light: 1.2,
            bond_timber: 1.3,
            bond_masonry: 1.5,
            bond_dwarven: 3.0,
            bond_guild: 5.0,
            start_stones: vec![common.clone(), common, shard],
            flight: Flight::default(),
            hand_max_g: 3.0,
            hand_radius: (5.0, 40.0),
            loot_max_g: 6.0,
            loot_max_radius: 80.0,
            weather: Default::default(),
        }
    }
}

impl Tuning {
    pub fn bond_g(&self, class: BondClass) -> f32 {
        match class {
            BondClass::Light => self.bond_light,
            BondClass::Timber => self.bond_timber,
            BondClass::Masonry => self.bond_masonry,
            BondClass::Dwarven => self.bond_dwarven,
            BondClass::Guild => self.bond_guild,
        }
    }

    /// Defaults with whatever `json` sets on top.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("tuning serialises")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "full"]
    fn the_start_kit_matches_the_design() {
        let t = Tuning::default();
        assert_eq!(t.start_stones.len(), 3);
        assert_eq!(t.start_stones[2].charge, Charge::Repelling);
        assert_eq!(t.bond_g(BondClass::Dwarven), 3.0);
    }

    #[test]
    fn json_overrides_only_what_it_names() {
        let t = Tuning::from_json(r#"{ "bond_light": 0.5 }"#).unwrap();
        assert_eq!(t.bond_light, 0.5);
        assert_eq!(t.bond_masonry, 1.5);
        let back = Tuning::from_json(&t.to_json()).unwrap();
        assert_eq!(back, t);
    }
}
