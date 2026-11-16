//! World response: bonded pieces warn, break loose and pass on load
//! (W1 to W4); broken pieces become bodies that follow the field and clump
//! (W5, W6); far debris is thinned (W7).
//!
//! A piece under strain shows it before it goes: dust trickles from it
//! along the pull and it knocks; when it breaks there is a cloud of its
//! own stuff, chips and the sound of it.

use crate::flora::hash;
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
    /// Counts the signs given (dust, sounds), to vary them.
    signs: u32,
    /// Seconds until the next trickle of dust from the straining pieces.
    trickle: f32,
    /// The sounds of straining and breaking, light stuff and heavy
    /// (assets/sounds; loaded when first wanted).
    sounds: Option<Vec<Vec<Sound>>>,
}

/// The sounds by what happens and to what: `strain_light_0.ogg` and so
/// on, three of each. One that is missing is left out.
fn load_sounds() -> Vec<Vec<Sound>> {
    ["strain_light", "strain_heavy", "break_light", "break_heavy"]
        .iter()
        .map(|name| {
            (0..3)
                .filter_map(|n| Sound::load(format!("assets/sounds/{name}_{n}.ogg")).ok())
                .collect()
        })
        .collect()
}

/// The colour of a kind of piece's dust.
fn dust_of(class: BondClass) -> Color {
    match class {
        BondClass::Light => rgb(150, 170, 172),
        BondClass::Timber => rgb(120, 128, 136),
        BondClass::Masonry => rgb(104, 104, 108),
        _ => rgb(70, 68, 70),
    }
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
            signs: 0,
            trickle: 0.0,
            sounds: None,
        }
    }

    /// A structure that has come into the world later (a building woken
    /// by a field): its pieces stand where their entities are now.
    pub fn add(&mut self, world: &World, structure: grav_core::Structure, pieces: Vec<PieceRef>) {
        for p in &pieces {
            let m = world.world_transform(p.entity).unwrap_or_default();
            let (_, r, t) = m.to_scale_rotation_translation();
            self.rest.push((t, r));
        }
        self.built.structures.push(structure);
        self.built.pieces.extend(pieces);
    }

    /// True when every piece of structure `s` stands and is calm.
    pub fn at_rest(&self, s: usize) -> bool {
        self.built.structures[s]
            .pieces
            .iter()
            .all(|p| p.stage == PieceStage::Quiet)
    }

    /// Take structure `s` out of the world again (a building that goes
    /// back to being one drawn shape). Its place in the list stays, empty.
    pub fn remove(&mut self, world: &mut World, s: usize) {
        let mut new_index = Vec::with_capacity(self.built.pieces.len());
        let mut kept = 0;
        for p in &self.built.pieces {
            if p.structure == s {
                world.despawn(p.entity);
                new_index.push(None);
            } else {
                new_index.push(Some(kept));
                kept += 1;
            }
        }
        let mut k = 0;
        self.rest.retain(|_| {
            k += 1;
            new_index[k - 1].is_some()
        });
        self.built.pieces.retain(|p| p.structure != s);
        self.previewed = std::mem::take(&mut self.previewed)
            .into_iter()
            .filter_map(|k| new_index[k])
            .collect();
        self.built.structures[s] = grav_core::Structure::new();
    }

    /// Dust from piece `k`, `size` metres across, falling the way the pull
    /// goes there; with `sound`, the knock or crumble of it.
    fn sign(
        &mut self,
        world: &mut World,
        field: &Field,
        k: usize,
        size: f32,
        sound: bool,
    ) -> EngineResult<()> {
        self.signs = self.signs.wrapping_add(1);
        let p = self.piece_ref(k);
        let class = self.built.structures[p.structure].piece(p.piece).class;
        let at = self.rest[k].0;
        let pull = field.acceleration(&BodyQuery::at(at));
        let down = pull.normalize_or(Vec3::NEG_Y);
        let breaking = size > 1.0;
        // Thin: a haze the pieces show through, not a cloud in front of them.
        let stuff = dust_of(class);
        let thin = |a: f32| Color { a, ..stuff };
        let dust = ParticleEmitter::dust(at, size, stuff)
            .with_color(thin(if breaking { 0.2 } else { 0.28 }))
            .with_secondary_color(thin(0.0))
            .with_size(if breaking { 0.5 } else { 0.22 })
            .with_lifetime(if breaking { 3.0 } else { 1.8 })
            .with_velocity(down * 0.4)
            .with_acceleration(pull * 0.06)
            .with_seed(self.signs);
        world.particles_mut().burst(dust)?;
        if breaking {
            let chips = ParticleEmitter::debris(at, dust_of(class), 6, 1.6)
                .with_velocity(down * 0.8)
                .with_acceleration(pull)
                .with_seed(self.signs);
            world.particles_mut().burst(chips)?;
        }
        if sound {
            let light = matches!(class, BondClass::Light | BondClass::Timber);
            let set = usize::from(breaking) * 2 + usize::from(!light);
            let sounds = self.sounds.get_or_insert_with(load_sounds);
            let n = sounds[set].len();
            // Our own sounds; the engine's stock ones where a file is missing.
            let sound = if n > 0 {
                sounds[set][(hash(self.signs, 3) * n as f32) as usize % n].clone()
            } else {
                let stock = match (breaking, light) {
                    (_, true) => Stock::KnockSoft,
                    (true, false) => Stock::Crumble,
                    (false, false) => Stock::KnockHard,
                };
                Sound::stock(stock, (hash(self.signs, 3) * 3.0) as usize % 3)
            };
            world.play_sound(
                &sound,
                PlaySound::at(at)
                    .volume(if breaking { 0.9 } else { 0.45 })
                    .pitch(0.9 + 0.2 * hash(self.signs, 4)),
            );
        }
        Ok(())
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
        // Dust from what strains: at once from a piece that starts to, and
        // every little while from a few of those that still do.
        let mut dusty: Vec<usize> = tint.iter().filter(|t| t.1).map(|t| t.0).take(6).collect();
        self.trickle -= dt;
        if self.trickle <= 0.0 {
            self.trickle = 0.35;
            let straining: Vec<usize> = (0..self.built.pieces.len())
                .filter(|&k| {
                    let p = self.built.pieces[k];
                    self.built.structures[p.structure].piece(p.piece).stage == PieceStage::Warning
                })
                .collect();
            for n in 0..straining.len().min(3) {
                let pick = hash(self.signs.wrapping_add(n as u32), 7) * straining.len() as f32;
                dusty.push(straining[(pick as usize).min(straining.len() - 1)]);
            }
        }
        for (n, &k) in dusty.iter().enumerate() {
            self.sign(world, field, k, 0.45, n == 0 && tint.iter().any(|t| t.1))?;
        }
        for (n, &k) in broke.iter().take(10).enumerate() {
            self.sign(world, field, k, 1.3, n < 2)?;
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
            // Solid by its own shape (what is drawn is what collides); a
            // sliver too small for that by its box.
            let body = Body::dynamic().density(density(class)).ccd(true);
            if world.add_body(p.entity, body.clone()).is_err() {
                world.add_body(p.entity, body.shape(BodyShape::BoundingBox))?;
            }
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

#[cfg(test)]
mod tests {
    use engine::prelude::*;

    #[test]
    fn every_sound_of_straining_and_breaking_is_there_and_plays() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/sounds");
        for name in ["strain_light", "strain_heavy", "break_light", "break_heavy"] {
            for n in 0..3 {
                let sound = Sound::load(format!("{dir}/{name}_{n}.ogg")).unwrap();
                assert!(
                    sound.duration() > 0.5 && sound.duration() < 9.0,
                    "{name}_{n}"
                );
            }
        }
    }
}
