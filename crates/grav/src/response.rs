//! World response: bonded pieces warn, break loose and pass on load
//! (W1 to W4); broken pieces become bodies that follow the field and clump
//! (W5, W6); far debris is thinned (W7).

use crate::space::{place, Built, PieceRef};
use engine::prelude::*;
use grav_core::{BodyQuery, BondClass, Field, PieceStage};

pub struct Response {
    pub built: Built,
    /// Centre and rotation of every piece while it is bonded.
    rest: Vec<(Vec3, Quat)>,
    /// Broken pieces and how long ago they broke.
    debris: Vec<(EntityId, f32)>,
    pub broken_total: usize,
    pub warning_now: usize,
    /// Pieces tinted by the placing preview.
    previewed: Vec<usize>,
}

/// Debris further than this from the hero is thinned after a while.
const THIN_DISTANCE: f32 = 150.0;
const THIN_AFTER: f32 = 30.0;

fn density(class: BondClass) -> f32 {
    match class {
        BondClass::Light => 0.7,
        BondClass::Timber => 0.6,
        _ => 2.2,
    }
}

impl Response {
    pub fn new(world: &World, built: Built) -> Self {
        let rest = built
            .pieces
            .iter()
            .map(|p| {
                let m = world.world_transform(p.entity).unwrap_or_default();
                let (_, r, t) = m.to_scale_rotation_translation();
                (t, r)
            })
            .collect();
        Self {
            built,
            rest,
            debris: Vec::new(),
            broken_total: 0,
            warning_now: 0,
            previewed: Vec::new(),
        }
    }

    fn piece_ref(&self, k: usize) -> PieceRef {
        self.built.pieces[k]
    }

    /// One step of every structure in `field`.
    pub fn step(
        &mut self,
        world: &mut World,
        field: &Field,
        hero_at: Vec3,
        dt: f32,
    ) -> EngineResult<()> {
        let mut index = std::collections::HashMap::new();
        for (k, p) in self.built.pieces.iter().enumerate() {
            index.insert((p.structure, p.piece), k);
        }
        let mut broke = Vec::new();
        let mut tint = Vec::new();
        for s in 0..self.built.structures.len() {
            let rest = &self.rest;
            let pieces = &self.built.pieces;
            let at = |id: grav_core::PieceId| rest[index[&(s, id)]].0;
            let r = self.built.structures[s]
                .step(|id| field.acceleration(&BodyQuery::at(at(id))), |_| 0.0);
            for id in r.broken {
                broke.push(index[&(s, id)]);
            }
            for id in r.warning {
                tint.push((index[&(s, id)], true));
            }
            for id in r.quiet {
                tint.push((index[&(s, id)], false));
            }
            let _ = pieces;
        }
        for (k, warn) in tint {
            let p = self.piece_ref(k);
            let (at, rot) = self.rest[k];
            let t = if warn {
                rgb(255, 170, 120)
            } else {
                Color::WHITE
            };
            world.set_place(p.entity, place(at, rot).with_tint(t))?;
        }
        for k in broke {
            let p = self.piece_ref(k);
            let class = self.built.structures[p.structure].piece(p.piece).class;
            let (at, rot) = self.rest[k];
            world.set_place(p.entity, place(at, rot).with_tint(rgb(235, 225, 215)))?;
            world.remove_body(p.entity);
            world.add_body(
                p.entity,
                Body::dynamic()
                    .density(density(class))
                    .shape(BodyShape::Box(p.half))
                    .ccd(true),
            )?;
            self.debris.push((p.entity, 0.0));
            self.broken_total += 1;
        }
        self.warning_now = self
            .built
            .structures
            .iter()
            .flat_map(|s| s.pieces.iter())
            .filter(|p| p.stage == PieceStage::Warning)
            .count();
        // W7: thin debris far from the hero.
        let mut keep = Vec::with_capacity(self.debris.len());
        for (e, age) in self.debris.drain(..) {
            let age = age + dt;
            let far = world
                .world_transform(e)
                .map(|m| m.w_axis.truncate().distance(hero_at) > THIN_DISTANCE)
                .unwrap_or(true);
            if far && age > THIN_AFTER {
                world.despawn(e);
            } else {
                keep.push((e, age));
            }
        }
        self.debris = keep;
        Ok(())
    }

    /// Preview: tint the pieces that would break in `field` (call with
    /// `None` to clear). Returns how many would break.
    pub fn preview(&mut self, world: &mut World, field: Option<&Field>) -> EngineResult<usize> {
        for k in std::mem::take(&mut self.previewed) {
            let p = self.piece_ref(k);
            if !self.built.structures[p.structure].is_broken(p.piece) {
                let warn =
                    self.built.structures[p.structure].piece(p.piece).stage == PieceStage::Warning;
                let (at, rot) = self.rest[k];
                let t = if warn {
                    rgb(255, 170, 120)
                } else {
                    Color::WHITE
                };
                world.set_place(p.entity, place(at, rot).with_tint(t))?;
            }
        }
        let Some(field) = field else { return Ok(0) };
        for k in 0..self.built.pieces.len() {
            let p = self.piece_ref(k);
            let s = &self.built.structures[p.structure];
            if s.is_broken(p.piece) {
                continue;
            }
            let (at, rot) = self.rest[k];
            let a = field.acceleration(&BodyQuery::at(at));
            let ratio = s.load_g(p.piece, a, 0.0) / s.piece(p.piece).bond_g.max(1e-3);
            if ratio > 1.0 {
                world.set_place(p.entity, place(at, rot).with_tint(rgb(255, 60, 60)))?;
                self.previewed.push(k);
            }
        }
        Ok(self.previewed.len())
    }
}
