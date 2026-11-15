//! Coating: the cost of using stones (S7 to S13).

use serde::{Deserialize, Serialize};

/// The hero's coating supply.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Coating {
    pub units: f32,
    /// Sandbox and puzzle modes may run without a limit (S13).
    pub unlimited: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CoatingError {
    /// Not enough coating to close that far (S10): the stone keeps acting.
    NotEnough { needed: f32, have: f32 },
}

impl std::fmt::Display for CoatingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoatingError::NotEnough { needed, have } => {
                write!(f, "closing needs {needed:.2} coating, {have:.2} left")
            }
        }
    }
}

impl std::error::Error for CoatingError {}

impl Coating {
    pub fn new(units: f32) -> Self {
        Self {
            units,
            unlimited: false,
        }
    }

    pub fn unlimited() -> Self {
        Self {
            units: f32::INFINITY,
            unlimited: true,
        }
    }

    /// S9: covering a fraction again costs the stone's maximum magnitude in g
    /// times that fraction. Opening (S7) is free.
    pub fn cost(max_g: f32, from_opening: f32, to_opening: f32) -> f32 {
        let covered = (from_opening.clamp(0.0, 1.0) - to_opening.clamp(0.0, 1.0)).max(0.0);
        max_g.max(0.0) * covered
    }

    /// Pay for moving a sheath from `from` to `to`. Returns what was paid.
    pub fn pay(&mut self, max_g: f32, from: f32, to: f32) -> Result<f32, CoatingError> {
        let needed = Self::cost(max_g, from, to);
        if self.unlimited || needed <= 0.0 {
            return Ok(needed);
        }
        // A little slack so rounding never strands the last unit.
        if needed > self.units + 1e-4 {
            return Err(CoatingError::NotEnough {
                needed,
                have: self.units,
            });
        }
        self.units = (self.units - needed).max(0.0);
        Ok(needed)
    }

    /// The furthest a sheath at `from` can be closed with what is left.
    pub fn closable_to(&self, max_g: f32, from: f32) -> f32 {
        if self.unlimited || max_g <= 0.0 {
            return 0.0;
        }
        (from - self.units / max_g).clamp(0.0, from)
    }

    pub fn add(&mut self, units: f32) {
        if !self.unlimited {
            self.units += units.max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_costs_and_opening_is_free() {
        let mut c = Coating::new(20.0);
        assert_eq!(c.pay(1.5, 0.0, 1.0).unwrap(), 0.0);
        assert!((c.pay(1.5, 1.0, 0.0).unwrap() - 1.5).abs() < 1e-6);
        assert!((c.units - 18.5).abs() < 1e-6);
        // Part way: 1.5 g × 0.4 covered.
        assert!((c.pay(1.5, 0.6, 0.2).unwrap() - 0.6).abs() < 1e-6);
    }

    #[test]
    fn without_enough_coating_the_stone_stays_open() {
        let mut c = Coating::new(0.5);
        assert!(matches!(
            c.pay(6.0, 1.0, 0.0),
            Err(CoatingError::NotEnough { .. })
        ));
        assert_eq!(c.units, 0.5);
        assert!((c.closable_to(6.0, 1.0) - (1.0 - 0.5 / 6.0)).abs() < 1e-6);
        assert!(Coating::unlimited().pay(6.0, 1.0, 0.0).is_ok());
    }
}
