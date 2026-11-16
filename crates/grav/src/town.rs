//! Buildings that come apart (W1 to W4).
//!
//! A building is one drawn shape among many of its kind until a stone's
//! field reaches it. Then it wakes: its place is taken out of the drawn
//! set and its pieces stand there instead, each bonded to its neighbours
//! and solid by its own shape. From then on the world's response
//! (response.rs) loads, warns and breaks them.

use crate::pieces::Shard;
use crate::response::Response;
use crate::space::{self, PieceRef};
use engine::prelude::*;
use grav_core::{BodyQuery, Field, Structure, Tuning};

/// One kind of building: its drawn set, its pieces, where each stands.
pub struct Kind {
    pub set: LodId,
    pub places: Vec<Place>,
    /// The whole building's solid shape at each place, while it sleeps.
    pub solids: Vec<Option<EntityId>>,
    pub shards: Vec<Shard>,
}

pub struct Town {
    kinds: Vec<Kind>,
    /// Per kind and place: has it woken.
    awake: Vec<Vec<bool>>,
    /// How far a kind's pieces reach from its place.
    reach: Vec<f32>,
    material: CustomMaterialId,
    pub woken: usize,
}

impl Town {
    pub fn new(kinds: Vec<Kind>, material: CustomMaterialId) -> Self {
        let awake = kinds.iter().map(|k| vec![false; k.places.len()]).collect();
        let reach = kinds
            .iter()
            .map(|k| {
                k.shards
                    .iter()
                    .map(|s| s.lo.abs().max(s.hi.abs()).length())
                    .fold(0.0, f32::max)
            })
            .collect();
        Self {
            kinds,
            awake,
            reach,
            material,
            woken: 0,
        }
    }

    /// How many buildings stand, and how many pieces they have in all.
    pub fn count(&self) -> (usize, usize) {
        let buildings = self.kinds.iter().map(|k| k.places.len()).sum();
        let pieces = self
            .kinds
            .iter()
            .map(|k| k.places.len() * k.shards.len())
            .sum();
        (buildings, pieces)
    }

    /// Each frame: wake every building an open stone's field reaches.
    /// `base` is the world's own field, which the buildings were built for.
    pub fn update(
        &mut self,
        world: &mut World,
        response: &mut Response,
        field: &Field,
        base: &Field,
        tuning: &Tuning,
    ) -> EngineResult<()> {
        for source in &field.sources {
            if source.opening <= 0.0 {
                continue;
            }
            for k in 0..self.kinds.len() {
                if self.kinds[k].shards.is_empty() {
                    continue;
                }
                let mut woke = false;
                for i in 0..self.kinds[k].places.len() {
                    let at = self.kinds[k].places[i].position;
                    if self.awake[k][i]
                        || at.distance(source.position) > source.radius + self.reach[k]
                    {
                        continue;
                    }
                    self.wake(world, response, base, tuning, k, i)?;
                    woke = true;
                }
                if woke {
                    let kind = &self.kinds[k];
                    let asleep: Vec<Place> = (0..kind.places.len())
                        .filter(|&i| !self.awake[k][i])
                        .map(|i| kind.places[i])
                        .collect();
                    world.set_lod_places(kind.set, &asleep)?;
                }
            }
        }
        Ok(())
    }

    /// Put building `i` of kind `k` into the world as its pieces.
    fn wake(
        &mut self,
        world: &mut World,
        response: &mut Response,
        base: &Field,
        tuning: &Tuning,
        k: usize,
        i: usize,
    ) -> EngineResult<()> {
        self.awake[k][i] = true;
        self.woken += 1;
        let kind = &mut self.kinds[k];
        if let Some(solid) = kind.solids[i].take() {
            world.despawn(solid);
        }
        let place = kind.places[i];
        let frame = place.to_matrix();
        let (_, rot, _) = frame.to_scale_rotation_translation();
        let mut structure = Structure::new();
        let mut refs = Vec::with_capacity(kind.shards.len());
        for shard in &kind.shards {
            let at = frame.transform_point3(shard.center);
            let design = base.acceleration(&BodyQuery::at(at));
            let piece = structure.add(
                shard.class,
                tuning.bond_g(shard.class),
                design,
                shard.anchored,
            );
            let entity = world.spawn(shard.mesh.clone());
            world.set_place(entity, space::place(at, rot))?;
            world.set_custom_material(entity, self.material)?;
            // Solid by its own shape; a sliver too small for that by its box.
            if world
                .add_body(entity, Body::fixed().shape(BodyShape::Mesh))
                .is_err()
            {
                world.add_body(entity, Body::fixed().shape(BodyShape::BoundingBox))?;
            }
            refs.push((entity, piece));
        }
        for (a, shard) in kind.shards.iter().enumerate() {
            for &b in &shard.neighbours {
                if a < b {
                    structure.bond(refs[a].1, refs[b].1);
                }
            }
        }
        let index = response.built.structures.len();
        let pieces = refs
            .into_iter()
            .map(|(entity, piece)| PieceRef {
                entity,
                structure: index,
                piece,
            })
            .collect();
        response.add(world, structure, pieces);
        Ok(())
    }
}
