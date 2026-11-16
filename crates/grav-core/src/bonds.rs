//! Bonded building pieces: load, warnings, breaking and passing load on
//! (W1 to W4).
//!
//! A [`Structure`] is a graph of pieces. Each piece has a bond strength in
//! g, the field it was built for, the pieces it rests against and whether
//! it is anchored to dead stone. Every fixed step [`Structure::step`] is
//! given the field at each piece and answers which pieces warn and which
//! break loose. Breaking is plain data, so a server and its clients agree
//! on every collapse; the falling pieces themselves are physics bodies the
//! game makes.

use crate::one_g;
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// The kinds of bonded pieces with their bond strengths (W1 table).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BondClass {
    /// Roof tiles, shutters, signs, market stalls.
    Light,
    /// Timber beams, floors, light walls.
    Timber,
    /// Mortared masonry.
    Masonry,
    /// Dwarven heavy-district masonry.
    Dwarven,
    /// Guild-built and anchored to dead stone.
    Guild,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PieceId(pub u32);

/// What a piece shows (W3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum PieceStage {
    /// Below 60 % of its bond: nothing visible.
    #[default]
    Quiet,
    /// 60 to 100 %: dust, creaking, cracks.
    Warning,
    /// Broken loose: now a loose body.
    Broken,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Piece {
    pub class: BondClass,
    /// Bond strength in g (from the class, see [`crate::Tuning`]).
    pub bond_g: f32,
    /// The field this piece was built for, m/s² (its district's down).
    pub design: Vec3,
    /// Bonded neighbours.
    pub neighbours: Vec<PieceId>,
    /// Held by dead stone (a foundation): it can still break, but its
    /// neighbours count as supported through it.
    pub anchored: bool,
    pub stage: PieceStage,
    /// Load passed on by neighbours that broke, in g (W4).
    pub carried_g: f32,
    /// The last load relative to the bond (0..; 1 breaks).
    pub ratio: f32,
}

/// What one step changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StructureStep {
    /// Pieces that broke loose this step, by load or by losing support.
    pub broken: Vec<PieceId>,
    /// Pieces that started warning this step.
    pub warning: Vec<PieceId>,
    /// Pieces that calmed down again.
    pub quiet: Vec<PieceId>,
}

/// A bonded structure (one building, a bridge, a wall).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Structure {
    pub pieces: Vec<Piece>,
    /// Load share at which a piece starts to warn (W3: 0.6).
    pub warn_at: f32,
}

impl Structure {
    pub fn new() -> Self {
        Self {
            pieces: Vec::new(),
            warn_at: 0.6,
        }
    }

    /// Add a piece; returns its id.
    pub fn add(&mut self, class: BondClass, bond_g: f32, design: Vec3, anchored: bool) -> PieceId {
        self.pieces.push(Piece {
            class,
            bond_g,
            design,
            neighbours: Vec::new(),
            anchored,
            stage: PieceStage::Quiet,
            carried_g: 0.0,
            ratio: 0.0,
        });
        PieceId(self.pieces.len() as u32 - 1)
    }

    /// Bond two pieces to each other.
    pub fn bond(&mut self, a: PieceId, b: PieceId) {
        if a == b {
            return;
        }
        for (x, y) in [(a, b), (b, a)] {
            let n = &mut self.pieces[x.0 as usize].neighbours;
            if !n.contains(&y) {
                n.push(y);
            }
        }
    }

    pub fn piece(&self, id: PieceId) -> &Piece {
        &self.pieces[id.0 as usize]
    }

    pub fn is_broken(&self, id: PieceId) -> bool {
        self.piece(id).stage == PieceStage::Broken
    }

    pub fn intact_count(&self) -> usize {
        self.pieces
            .iter()
            .filter(|p| p.stage != PieceStage::Broken)
            .count()
    }

    /// W2: the load on a piece in g: how far the field now differs from
    /// the one it was built for, plus what loose things resting on it and
    /// broken neighbours put on it.
    pub fn load_g(&self, id: PieceId, field: Vec3, resting_g: f32) -> f32 {
        let p = self.piece(id);
        (field - p.design).length() / one_g() + resting_g.max(0.0) + p.carried_g
    }

    /// One fixed step. `field(id)` is the acceleration at the piece now;
    /// `resting(id)` is the extra load in g from loose bodies resting on it.
    /// Breaks are decided on the loads at the start of the step and passed
    /// on for the next one, so a collapse spreads over a few steps (W4).
    pub fn step(
        &mut self,
        mut field: impl FnMut(PieceId) -> Vec3,
        mut resting: impl FnMut(PieceId) -> f32,
    ) -> StructureStep {
        let mut out = StructureStep::default();
        let n = self.pieces.len();
        let mut breaking = Vec::new();
        for i in 0..n {
            if self.pieces[i].stage == PieceStage::Broken {
                continue;
            }
            let id = PieceId(i as u32);
            let load = self.load_g(id, field(id), resting(id));
            let p = &mut self.pieces[i];
            p.ratio = load / p.bond_g.max(1e-3);
            let stage = if p.ratio > 1.0 {
                PieceStage::Broken
            } else if p.ratio >= self.warn_at {
                PieceStage::Warning
            } else {
                PieceStage::Quiet
            };
            match (p.stage, stage) {
                (_, PieceStage::Broken) => breaking.push((id, load)),
                (PieceStage::Quiet, PieceStage::Warning) => out.warning.push(id),
                (PieceStage::Warning, PieceStage::Quiet) => out.quiet.push(id),
                _ => {}
            }
            if stage != PieceStage::Broken {
                p.stage = stage;
            }
        }
        // W4: a piece that breaks passes its load to its intact neighbours.
        for &(id, _) in &breaking {
            self.pieces[id.0 as usize].stage = PieceStage::Broken;
        }
        for &(id, load) in &breaking {
            let intact: Vec<PieceId> = self
                .piece(id)
                .neighbours
                .iter()
                .copied()
                .filter(|&nb| !self.is_broken(nb))
                .collect();
            if intact.is_empty() {
                continue;
            }
            let share = load / intact.len() as f32;
            for nb in intact {
                self.pieces[nb.0 as usize].carried_g += share;
            }
        }
        out.broken.extend(breaking.iter().map(|(id, _)| *id));
        // Pieces no longer connected to an anchor fall too.
        out.broken.extend(self.drop_unsupported());
        out
    }

    /// Break every intact piece with no path of intact bonds to an anchor.
    fn drop_unsupported(&mut self) -> Vec<PieceId> {
        let n = self.pieces.len();
        let mut supported = vec![false; n];
        let mut queue: Vec<usize> = (0..n)
            .filter(|&i| self.pieces[i].anchored && self.pieces[i].stage != PieceStage::Broken)
            .collect();
        for &i in &queue {
            supported[i] = true;
        }
        while let Some(i) = queue.pop() {
            for nb in &self.pieces[i].neighbours {
                let j = nb.0 as usize;
                if !supported[j] && self.pieces[j].stage != PieceStage::Broken {
                    supported[j] = true;
                    queue.push(j);
                }
            }
        }
        let mut out = Vec::new();
        for (i, p) in self.pieces.iter_mut().enumerate() {
            if p.stage != PieceStage::Broken && !supported[i] {
                p.stage = PieceStage::Broken;
                out.push(PieceId(i as u32));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A column of `n` pieces on an anchored foundation, built for 1 g down.
    fn column(n: usize, bond_g: f32) -> (Structure, Vec<PieceId>) {
        let mut s = Structure::new();
        let down = Vec3::NEG_Y * one_g();
        let mut ids = vec![s.add(BondClass::Masonry, bond_g, down, true)];
        for _ in 1..n {
            let id = s.add(BondClass::Masonry, bond_g, down, false);
            s.bond(*ids.last().unwrap(), id);
            ids.push(id);
        }
        (s, ids)
    }

    #[test]
    fn the_field_it_was_built_for_loads_nothing() {
        let (mut s, _) = column(4, 1.5);
        let r = s.step(|_| Vec3::NEG_Y * one_g(), |_| 0.0);
        assert!(r.broken.is_empty() && r.warning.is_empty());
    }

    #[test]
    fn pieces_warn_then_break() {
        let (mut s, ids) = column(3, 1.0);
        let top = ids[2];
        // 0.7 g sideways on the top piece: warning.
        let r = s.step(
            |id| {
                Vec3::NEG_Y * one_g()
                    + if id == top {
                        Vec3::X * 0.7 * one_g()
                    } else {
                        Vec3::ZERO
                    }
            },
            |_| 0.0,
        );
        assert_eq!(r.warning, vec![top]);
        assert!(r.broken.is_empty());
        // 1.2 g: it breaks.
        let r = s.step(
            |id| {
                Vec3::NEG_Y * one_g()
                    + if id == top {
                        Vec3::X * 1.2 * one_g()
                    } else {
                        Vec3::ZERO
                    }
            },
            |_| 0.0,
        );
        assert_eq!(r.broken, vec![top]);
        assert!(s.is_broken(top));
    }

    #[test]
    fn losing_the_support_drops_everything_above() {
        let (mut s, ids) = column(5, 1.0);
        let r = s.step(
            |id| {
                Vec3::NEG_Y * one_g()
                    + if id == ids[1] {
                        Vec3::X * 2.0 * one_g()
                    } else {
                        Vec3::ZERO
                    }
            },
            |_| 0.0,
        );
        let mut broken = r.broken.clone();
        broken.sort();
        assert_eq!(broken, vec![ids[1], ids[2], ids[3], ids[4]]);
        assert!(!s.is_broken(ids[0]));
    }

    #[test]
    fn load_passes_on_and_spreads_over_steps() {
        // A row of five anchored wall pieces bonded side by side.
        let mut s = Structure::new();
        let down = Vec3::NEG_Y * one_g();
        let ids: Vec<PieceId> = (0..5)
            .map(|_| s.add(BondClass::Timber, 0.8, down, true))
            .collect();
        for w in ids.windows(2) {
            s.bond(w[0], w[1]);
        }
        // A stone next to the middle piece: 0.9 g extra there, 0.5 g next to it.
        let field = |id: PieceId| {
            let k = (id.0 as i32 - 2).abs();
            down + Vec3::X * one_g() * [0.9, 0.5, 0.1][k as usize]
        };
        let r1 = s.step(field, |_| 0.0);
        assert_eq!(r1.broken, vec![ids[2]]);
        // Its 0.9 g go half to each neighbour: 0.5 + 0.45 > 0.8, they follow.
        let r2 = s.step(field, |_| 0.0);
        let mut b = r2.broken.clone();
        b.sort();
        assert_eq!(b, vec![ids[1], ids[3]]);
        // The outer ones carry 0.1 + 0.95: they go on the next step.
        let r3 = s.step(field, |_| 0.0);
        assert_eq!(r3.broken.len(), 2);
        assert_eq!(s.intact_count(), 0);
    }

    #[test]
    fn resting_loose_bodies_add_load() {
        let (mut s, ids) = column(2, 1.0);
        let r = s.step(
            |_| Vec3::NEG_Y * one_g(),
            |id| if id == ids[1] { 1.1 } else { 0.0 },
        );
        assert_eq!(r.broken, vec![ids[1]]);
    }
}
