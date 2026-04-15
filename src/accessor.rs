// src/accessor.rs
//! EntityAccessor — zero-overhead cached access with validation

use crate::entity::EntityHandle;
use crate::typed_attrs::TypedAttr;
use crate::world::World;

/// Pre-resolved absolute offset with generation validation
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttrOffset {
    pub offset: usize,
    pub generation: u32,
    pub entity_index: u32,
}

impl AttrOffset {
    #[inline]
    pub fn new(offset: usize, generation: u32, entity_index: u32) -> Self {
        Self { offset, generation, entity_index }
    }
    
    /// Validate that entity is still alive with same generation
    #[inline]
    pub fn validate(&self, world: &World) -> bool {
        world.allocator.is_alive(crate::archetype::EntityId {
            index: self.entity_index,
            generation: self.generation,
        })
    }
}

/// Cached per-entity accessor
#[derive(Clone, Debug)]
pub struct EntityAccessor {
    layout_version: u64,
    offsets: Box<[AttrOffset]>,
}

impl EntityAccessor {
    pub fn new(world: &World, entity: EntityHandle, attrs: &[&'static str]) -> Option<Self> {
        if !world.is_alive(entity) {
            return None;
        }
        let eid_idx = entity.0.index as usize;
        let loc = world.entity_locations.get(eid_idx)?.as_ref()?;
        let arch = &world.archetypes[loc.archetype_idx];
        let base = arch.float_offsets[loc.inner_idx].0 as usize;
        
        let mut offsets = Vec::with_capacity(attrs.len());
        for &name in attrs {
            let interned = world.interner.find(name)?;
            let field = arch.schema.find_field(interned)?;
            offsets.push(AttrOffset::new(
                base + field.0 as usize,
                entity.0.generation,
                entity.0.index,
            ));
        }
        
        Some(Self {
            layout_version: world.layout_version(),
            offsets: offsets.into_boxed_slice(),
        })
    }

    pub fn resolve<A: TypedAttr>(world: &World, entity: EntityHandle) -> Option<AttrOffset> {
        if !world.is_alive(entity) {
            return None;
        }
        let eid_idx = entity.0.index as usize;
        let loc = world.entity_locations.get(eid_idx)?.as_ref()?;
        let arch = &world.archetypes[loc.archetype_idx];
        let base = arch.float_offsets[loc.inner_idx].0 as usize;
        let interned = world.interner.find(A::NAME)?;
        let field = arch.schema.find_field(interned)?;
        
        Some(AttrOffset::new(
            base + field.0 as usize,
            entity.0.generation,
            entity.0.index,
        ))
    }

    #[inline]
    pub fn is_stale(&self, world: &World) -> bool {
        self.layout_version != world.layout_version()
    }

    #[inline(always)]
    pub fn get(&self, world: &World, slot: usize) -> f64 {
        world.storage.read_abs(self.offsets[slot].offset)
    }

    #[inline(always)]
    pub fn set(&self, world: &mut World, slot: usize, value: f64) {
        world.storage.write_abs(self.offsets[slot].offset, value);
    }

    #[inline]
    pub fn slot_count(&self) -> usize {
        self.offsets.len()
    }

    #[inline]
    pub fn offset_at(&self, slot: usize) -> AttrOffset {
        self.offsets[slot]
    }
    
    /// Validate all offsets
    pub fn validate(&self, world: &World) -> bool {
        self.offsets.iter().all(|o| o.validate(world))
    }
}

#[inline(always)]
pub fn read_at(world: &World, offset: AttrOffset) -> f64 {
    world.storage.read_abs(offset.offset)
}

#[inline(always)]
pub fn write_at(world: &mut World, offset: AttrOffset, value: f64) {
    world.storage.write_abs(offset.offset, value);
}

#[derive(Clone, Debug)]
pub struct EntityRef {
    archetype_idx: usize,
    #[allow(dead_code)]  // Used for validation
    inner_idx: usize,
    base_offset: usize,
    layout_version: u64,
    generation: u32,
    entity_index: u32,
}

impl EntityRef {
    pub fn new(world: &World, entity: EntityHandle) -> Option<Self> {
        if !world.is_alive(entity) {
            return None;
        }
        let eid_idx = entity.0.index as usize;
        let loc = world.entity_locations.get(eid_idx)?.as_ref()?;
        let arch = &world.archetypes[loc.archetype_idx];
        if !arch.is_alive(loc.inner_idx) {
            return None;
        }
        let base = arch.float_offsets[loc.inner_idx].0 as usize;
        
        Some(Self {
            archetype_idx: loc.archetype_idx,
            inner_idx: loc.inner_idx,
            base_offset: base,
            layout_version: world.layout_version(),
            generation: entity.0.generation,
            entity_index: entity.0.index,
        })
    }

    #[inline]
    pub fn get<A: TypedAttr>(&self, world: &World) -> Option<f64> {
        let interned = world.interner.find(A::NAME)?;
        let arch = &world.archetypes[self.archetype_idx];
        let field = arch.schema.find_field(interned)?;
        Some(world.storage.read_abs(self.base_offset + field.0 as usize))
    }

    #[inline]
    pub fn set<A: TypedAttr>(&self, world: &mut World, value: f64) -> bool {
        let interned = match world.interner.find(A::NAME) {
            Some(id) => id,
            None => return false,
        };
        let field = match world.archetypes[self.archetype_idx].schema.find_field(interned) {
            Some(f) => f,
            None => return false,
        };
        world.storage.write_abs(self.base_offset + field.0 as usize, value);
        true
    }

    #[inline]
    pub fn absolute_offset<A: TypedAttr>(&self, world: &World) -> Option<AttrOffset> {
        let interned = world.interner.find(A::NAME)?;
        let arch = &world.archetypes[self.archetype_idx];
        let field = arch.schema.find_field(interned)?;
        
        Some(AttrOffset::new(
            self.base_offset + field.0 as usize,
            self.generation,
            self.entity_index,
        ))
    }

    #[inline]
    pub fn is_stale(&self, world: &World) -> bool {
        self.layout_version != world.layout_version()
    }

    #[inline]
    pub fn base_offset(&self) -> usize {
        self.base_offset
    }

    #[inline]
    pub fn archetype_idx(&self) -> usize {
        self.archetype_idx
    }
}