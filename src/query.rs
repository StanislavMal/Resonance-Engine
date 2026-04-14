// src/query.rs
//! Typed queries — archetype-aware, no HashMap lookups in hot path

use crate::entity::EntityHandle;
use crate::typed_attrs::TypedAttr;
use crate::world::World;

/// All entities with a specific typed attribute
pub fn with_attr<A: TypedAttr>(world: &World) -> Vec<EntityHandle> {
    let attr_id = match world.interner().find(A::NAME) {
        Some(id) => id,
        None => return Vec::new(),
    };

    let mut result = Vec::new();
    if let Some(locations) = world.attr_index.get(&attr_id) {
        for &(arch_idx, _) in locations {
            let arch = &world.archetypes[arch_idx];
            for (_, eid, _) in arch.alive_iter() {
                if world.is_alive(EntityHandle(eid)) {
                    result.push(EntityHandle(eid));
                }
            }
        }
    }
    result
}

/// All entities with attribute value matching predicate
pub fn where_attr<A: TypedAttr>(
    world: &World,
    pred: impl Fn(f64) -> bool,
) -> Vec<EntityHandle> {
    let attr_id = match world.interner().find(A::NAME) {
        Some(id) => id,
        None => return Vec::new(),
    };

    let mut result = Vec::new();
    if let Some(locations) = world.attr_index.get(&attr_id) {
        for &(arch_idx, field_idx) in locations {
            let arch = &world.archetypes[arch_idx];
            for (_, eid, offset) in arch.alive_iter() {
                let val = world.storage.read_float(offset, field_idx);
                if pred(val) {
                    result.push(EntityHandle(eid));
                }
            }
        }
    }
    result
}

/// Count entities with attribute
pub fn count_with<A: TypedAttr>(world: &World) -> usize {
    let attr_id = match world.interner().find(A::NAME) {
        Some(id) => id,
        None => return 0,
    };
    world
        .attr_index
        .get(&attr_id)
        .map(|locs| {
            locs.iter()
                .map(|&(ai, _)| world.archetypes[ai].alive_count())
                .sum()
        })
        .unwrap_or(0)
}