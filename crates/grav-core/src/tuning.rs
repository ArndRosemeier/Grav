//! Every number to tune, as data (design doc: "keep them as data, not as
//! constants in code"). `Tuning::default()` holds the design's first values;
//! the game loads `tuning.json` over them when it exists.

use crate::bonds::BondClass;
use crate::hero::Movement;
use crate::profile::Profile;
use crate::source::Charge;
use crate::stone::StoneSpec;
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
    /// Starting coating units.
    pub start_coating: f32,
    /// The hero's starting stones.
    pub start_stones: Vec<StoneSpec>,
    /// Throw range in metres.
    pub throw_range: f32,
    /// Hand stones' limits (sandbox palette): magnitude and radius.
    pub hand_max_g: f32,
    pub hand_radius: (f32, f32),
    pub loot_max_g: f32,
    pub loot_max_radius: f32,
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
            spring_sheath: false,
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
            spring_sheath: false,
        };
        Self {
            movement: Movement::default(),
            bond_light: 0.3,
            bond_timber: 0.8,
            bond_masonry: 1.5,
            bond_dwarven: 3.0,
            bond_guild: 5.0,
            start_coating: 20.0,
            start_stones: vec![common.clone(), common, shard],
            throw_range: 30.0,
            hand_max_g: 3.0,
            hand_radius: (5.0, 40.0),
            loot_max_g: 6.0,
            loot_max_radius: 80.0,
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
    fn the_start_kit_matches_the_design() {
        let t = Tuning::default();
        assert_eq!(t.start_coating, 20.0);
        assert_eq!(t.start_stones.len(), 3);
        assert_eq!(t.start_stones[2].charge, Charge::Repelling);
        assert_eq!(t.bond_g(BondClass::Dwarven), 3.0);
    }

    #[test]
    fn json_overrides_only_what_it_names() {
        let t = Tuning::from_json(r#"{ "start_coating": 50.0 }"#).unwrap();
        assert_eq!(t.start_coating, 50.0);
        assert_eq!(t.bond_masonry, 1.5);
        let back = Tuning::from_json(&t.to_json()).unwrap();
        assert_eq!(back, t);
    }
}
