//! Buildings that come apart (W1 to W4).
//!
//! A building is one drawn shape among many of its kind until a stone's
//! field reaches it. Then it wakes: its place is taken out of the drawn
//! set and its pieces stand there instead, each bonded to its neighbours
//! and solid by its own shape. From then on the world's response
//! (response.rs) loads, warns and breaks them. A building that came
//! through whole goes back to being one drawn shape a little after the
//! last field has left it.

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
    /// That solid shape, to put it back when a building sleeps again.
    pub solid: Option<Mesh>,
    pub shards: Vec<Shard>,
}

/// A building that stands as its pieces.
struct Woken {
    kind: usize,
    place: usize,
    /// Its structure in the world's response.
    structure: usize,
    /// Seconds since a field last reached it.
    calm: f32,
}

/// A whole building sleeps again this long after the last field left it.
const SLEEP_AFTER: f32 = 3.0;

pub struct Town {
    kinds: Vec<Kind>,
    /// Per kind and place: has it woken.
    awake: Vec<Vec<bool>>,
    /// How far a kind's pieces reach from its place.
    reach: Vec<f32>,
    material: CustomMaterialId,
    unseen: CustomMaterialId,
    standing: Vec<Woken>,
    /// How many buildings have woken, and how many went to sleep again.
    pub woken: usize,
    pub slept: usize,
}

impl Town {
    pub fn new(kinds: Vec<Kind>, material: CustomMaterialId, unseen: CustomMaterialId) -> Self {
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
            unseen,
            standing: Vec::new(),
            woken: 0,
            slept: 0,
        }
    }

    /// The building standing at `p` (within a metre): its kind and place.
    pub fn building_at(&self, p: Vec3) -> Option<(usize, usize)> {
        self.kinds.iter().enumerate().find_map(|(k, kind)| {
            kind.places
                .iter()
                .position(|q| q.position.distance(p) < 1.0)
                .map(|i| (k, i))
        })
    }

    /// Where building `i` of kind `k` stands.
    pub fn place_of(&self, k: usize, i: usize) -> Option<Vec3> {
        Some(self.kinds.get(k)?.places.get(i)?.position)
    }

    /// How many pieces building `b` has, and how many of them broke.
    pub fn broken_in(&self, response: &Response, b: (usize, usize)) -> (usize, usize) {
        let total = self.kinds[b.0].shards.len();
        let broken = self
            .standing
            .iter()
            .find(|w| (w.kind, w.place) == b)
            .map_or(0, |w| {
                response.built.structures[w.structure]
                    .pieces
                    .iter()
                    .filter(|p| p.stage == grav_core::PieceStage::Broken)
                    .count()
            });
        (total, broken)
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

    /// Each frame: wake every building an open stone's field reaches, and
    /// let whole ones sleep again when the fields have gone. `base` is the
    /// world's own field, which the buildings were built for.
    pub fn update(
        &mut self,
        world: &mut World,
        response: &mut Response,
        field: &Field,
        base: &Field,
        tuning: &Tuning,
        dt: f32,
    ) -> EngineResult<()> {
        self.sleep(world, response, field, dt)?;
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
                    self.draw(world, k)?;
                }
            }
        }
        Ok(())
    }

    /// Draw the sleeping buildings of kind `k` (the woken ones stand as
    /// their pieces).
    fn draw(&self, world: &mut World, k: usize) -> EngineResult<()> {
        let kind = &self.kinds[k];
        let asleep: Vec<Place> = (0..kind.places.len())
            .filter(|&i| !self.awake[k][i])
            .map(|i| kind.places[i])
            .collect();
        world.set_lod_places(kind.set, &asleep)
    }

    /// Buildings no field reaches any more, and of which nothing broke or
    /// strains, become one drawn shape again.
    fn sleep(
        &mut self,
        world: &mut World,
        response: &mut Response,
        field: &Field,
        dt: f32,
    ) -> EngineResult<()> {
        let mut changed = Vec::new();
        let mut standing = std::mem::take(&mut self.standing);
        standing.retain_mut(|w| {
            let at = self.kinds[w.kind].places[w.place].position;
            let reached = field.sources.iter().any(|s| {
                s.opening > 0.0 && at.distance(s.position) <= s.radius + self.reach[w.kind]
            });
            w.calm = if reached { 0.0 } else { w.calm + dt };
            if w.calm < SLEEP_AFTER || !response.at_rest(w.structure) {
                return true;
            }
            response.remove(world, w.structure);
            self.awake[w.kind][w.place] = false;
            self.slept += 1;
            changed.push((w.kind, w.place));
            false
        });
        self.standing = standing;
        for &(k, i) in &changed {
            if let Some(solid) = self.kinds[k].solid.clone() {
                let id = world.spawn(solid);
                world.set_place(id, self.kinds[k].places[i])?;
                world.set_custom_material(id, self.unseen)?;
                world.set_casts_shadow(id, false)?;
                world.add_body(id, Body::fixed().shape(BodyShape::Mesh))?;
                self.kinds[k].solids[i] = Some(id);
            }
        }
        changed.sort();
        changed.dedup_by_key(|c| c.0);
        for (k, _) in changed {
            self.draw(world, k)?;
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
        self.standing.push(Woken {
            kind: k,
            place: i,
            structure: index,
            calm: 0.0,
        });
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
