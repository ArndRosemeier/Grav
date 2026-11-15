//! A hand stone: what it is (spec), where it is and how open (state), and
//! the handling verbs (open, close, hold, set, throw, recall, recover).

use crate::coating::{Coating, CoatingError};
use crate::profile::{Profile, Selectivity, TimeDial};
use crate::source::{Charge, Source, SourceId};
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// What a stone is: its charge and its dressing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoneSpec {
    pub name: String,
    pub charge: Charge,
    /// Peak magnitude in g, fully open.
    pub max_g: f32,
    pub radius: f32,
    pub profile: Profile,
    #[serde(default)]
    pub time: TimeDial,
    #[serde(default)]
    pub selectivity: Selectivity,
    /// Adjuster slots (1 common, 2 guild, 3 rare loot; 0 for the shard).
    #[serde(default)]
    pub slots: u8,
    /// A spring sheath can close from a distance once per refill.
    #[serde(default)]
    pub spring_sheath: bool,
}

/// Where a stone is.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Placement {
    /// In the hero's pack: no field reaches out, whatever the sheath says.
    /// (A carried open stone is held, not packed.)
    Packed,
    /// In the hand of body `holder`: the field moves with it (P8).
    Held { holder: u64 },
    /// Fixed to a surface or object; `on` is the body it sticks to, if any.
    Set {
        at: Vec3,
        axis: Vec3,
        on: Option<u64>,
    },
    /// Flying or lying loose as a body of its own.
    Loose { at: Vec3, axis: Vec3 },
}

/// A stone the hero owns.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stone {
    pub id: SourceId,
    pub spec: StoneSpec,
    /// Sheath opening `0..=1`.
    pub opening: f32,
    pub placement: Placement,
    /// Seconds since it was opened from closed.
    pub open_seconds: f32,
    /// Whether the spring sheath still holds its one remote close.
    pub spring_loaded: bool,
}

impl Stone {
    pub fn new(id: u32, spec: StoneSpec) -> Self {
        let spring = spec.spring_sheath;
        Self {
            id: SourceId(id),
            spec,
            opening: 0.0,
            placement: Placement::Packed,
            open_seconds: 0.0,
            spring_loaded: spring,
        }
    }

    /// Open the sheath further (free, S7). Closing goes through [`Self::close_to`].
    pub fn open_to(&mut self, opening: f32) {
        let o = opening.clamp(0.0, 1.0);
        if o > self.opening {
            if self.opening <= 0.0 {
                self.open_seconds = 0.0;
            }
            self.opening = o;
        }
    }

    /// Close the sheath to `opening`, paying coating (S8, S9). Without
    /// enough the stone stays as it was (S10).
    pub fn close_to(&mut self, opening: f32, coating: &mut Coating) -> Result<f32, CoatingError> {
        let o = opening.clamp(0.0, 1.0);
        if o >= self.opening {
            return Ok(0.0);
        }
        let paid = coating.pay(self.spec.max_g, self.opening, o)?;
        self.opening = o;
        Ok(paid)
    }

    /// Close fully from a distance with the spring sheath's own coating.
    /// Returns false when the sheath has none or there is no spring.
    pub fn recall(&mut self) -> bool {
        if !self.spec.spring_sheath || !self.spring_loaded {
            return false;
        }
        self.spring_loaded = false;
        self.opening = 0.0;
        true
    }

    /// Refill the spring at a bench (paid there).
    pub fn refill_spring(&mut self) {
        self.spring_loaded = self.spec.spring_sheath;
    }

    pub fn tick(&mut self, dt: f32) {
        if self.opening > 0.0 {
            self.open_seconds += dt;
        }
    }

    /// The field this stone makes now, if any.
    pub fn source(&self) -> Option<Source> {
        if self.opening <= 0.0 {
            return None;
        }
        let (position, axis, holder) = match self.placement {
            Placement::Packed => return None,
            Placement::Held { .. } => return None, // the game passes the hand's pose
            Placement::Set { at, axis, .. } | Placement::Loose { at, axis } => (at, axis, None),
        };
        Some(self.source_at(position, axis, holder))
    }

    /// The field this stone makes at a given pose (a held stone: the hand).
    pub fn source_at(&self, position: Vec3, axis: Vec3, holder: Option<u64>) -> Source {
        Source {
            id: self.id,
            charge: self.spec.charge,
            position,
            axis: axis.try_normalize().unwrap_or(Vec3::Y),
            max_g: self.spec.max_g,
            opening: self.opening,
            radius: self.spec.radius,
            profile: self.spec.profile,
            time: self.spec.time,
            selectivity: self.spec.selectivity,
            open_seconds: self.open_seconds,
            holder,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn common() -> StoneSpec {
        StoneSpec {
            name: "common".into(),
            charge: Charge::Attracting,
            max_g: 1.5,
            radius: 15.0,
            profile: Profile::Core,
            time: TimeDial::Constant,
            selectivity: Selectivity::All,
            slots: 1,
            spring_sheath: false,
        }
    }

    #[test]
    fn open_then_close_costs_coating() {
        let mut s = Stone::new(1, common());
        let mut c = Coating::new(20.0);
        s.open_to(1.0);
        assert_eq!(s.opening, 1.0);
        assert!((s.close_to(0.0, &mut c).unwrap() - 1.5).abs() < 1e-6);
        assert_eq!(s.opening, 0.0);
        assert!((c.units - 18.5).abs() < 1e-6);
    }

    #[test]
    fn a_packed_or_closed_stone_makes_no_field() {
        let mut s = Stone::new(1, common());
        s.open_to(1.0);
        assert!(s.source().is_none(), "packed");
        s.placement = Placement::Set {
            at: Vec3::ZERO,
            axis: Vec3::Y,
            on: None,
        };
        assert!(s.source().is_some());
        let mut c = Coating::unlimited();
        s.close_to(0.0, &mut c).unwrap();
        assert!(s.source().is_none());
    }

    #[test]
    fn recall_needs_a_loaded_spring() {
        let mut spec = common();
        spec.spring_sheath = true;
        let mut s = Stone::new(1, spec);
        s.open_to(1.0);
        assert!(s.recall());
        assert_eq!(s.opening, 0.0);
        s.open_to(1.0);
        assert!(!s.recall(), "used up until refilled");
        s.refill_spring();
        assert!(s.recall());
    }
}
