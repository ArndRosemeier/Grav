//! Hands grav-core's field to the engine's physics as its gravity field.

use engine::prelude::*;
use grav_core::{BodyQuery, Field, LodestoneBody};
use std::collections::HashMap;
use std::sync::Arc;

/// A snapshot of the field plus which bodies are lodestone.
pub struct FieldLink {
    pub field: Field,
    pub lodestone: HashMap<u64, LodestoneBody>,
}

impl GravityField for FieldLink {
    fn acceleration(&self, probe: &FieldProbe) -> Vec3 {
        self.field.acceleration(&BodyQuery {
            id: probe.id,
            position: probe.position,
            mass: probe.mass,
            lodestone: self.lodestone.get(&probe.id).copied(),
        })
    }
}

/// Give physics a fresh snapshot (call when stones changed, or every frame).
pub fn install(world: &mut World, field: &Field, lodestone: HashMap<u64, LodestoneBody>) {
    world.set_gravity_field(Some(Arc::new(FieldLink {
        field: field.clone(),
        lodestone,
    })));
}
