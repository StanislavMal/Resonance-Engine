// src/accessor.rs
//! EntityAccessor — zero-overhead cached access to entity attributes.
//!
//! ## How AttrOffset works
//!
//! AttrOffset is an ABSOLUTE index into the global SoA float buffer.
//! Different entities of the SAME archetype have DIFFERENT AttrOffset
//! values for the same attribute.
//!
//! ```text
//! Storage: [px₀, py₀, vx₀, px₁, py₁, vx₁, ...]
//!           ^0   ^1   ^2   ^3   ^4   ^5
//!
//! Entity 0: AttrOffset for PosX = 0
//! Entity 1: AttrOffset for PosX = 3
//! ```
//!
//! Safe to store across ticks. Does not hold raw pointers.

use crate::entity::EntityHandle;
use crate::typed_attrs::TypedAttr;
use crate::world::World;

/// Pre-resolved absolute offset into the float buffer for one attribute of one entity.
///
/// Stable as long as the entity is alive and no new build() reallocates storage.
/// After build() that adds new entities, check is_stale() or re-resolve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttrOffset(pub usize);

/// Cached per-entity accessor — resolve once, read/write across many ticks.
///
/// Does NOT hold raw pointers — safe to store freely.
/// Takes fresh pointer from `&World` / `&mut World` at each get/set call.
#[derive(Clone, Debug)]
pub struct EntityAccessor {
    /// layout_version at resolve time — detect stale accessor
    layout_version: u64,
    /// Absolute float offsets, indexed by slot (order of attrs passed to new())
    offsets: Box<[usize]>,
}

impl EntityAccessor {
    /// Resolve named attributes for `entity`.
    ///
    /// Returns `None` if entity is dead (including generation mismatch)
    /// or any attribute is missing from entity's schema.
    pub fn new(world: &World, entity: EntityHandle, attrs: &[&'static str]) -> Option<Self> {
        // Generation check first
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
            offsets.push(base + field.0 as usize);
        }
        Some(Self {
            layout_version: world.layout_version(),
            offsets: offsets.into_boxed_slice(),
        })
    }

    /// Typed single-attribute resolve. Returns an absolute offset.
    ///
    /// Returns `None` if entity is dead (including generation mismatch)
    /// or attribute is not in entity's schema.
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
        Some(AttrOffset(base + field.0 as usize))
    }

    /// True if world layout changed since this accessor was resolved.
    /// If stale, re-resolve with `EntityAccessor::new()`.
    #[inline]
    pub fn is_stale(&self, world: &World) -> bool {
        self.layout_version != world.layout_version()
    }

    /// Read attribute by slot index. Takes fresh pointer from world.
    #[inline(always)]
    pub fn get(&self, world: &World, slot: usize) -> f64 {
        world.storage.read_abs(self.offsets[slot])
    }

    /// Write attribute by slot index. Takes fresh pointer from world.
    #[inline(always)]
    pub fn set(&self, world: &mut World, slot: usize, value: f64) {
        world.storage.write_abs(self.offsets[slot], value);
    }

    /// Number of bound slots
    #[inline]
    pub fn slot_count(&self) -> usize {
        self.offsets.len()
    }

    /// Get the absolute offset for a specific slot
    #[inline]
    pub fn offset_at(&self, slot: usize) -> AttrOffset {
        AttrOffset(self.offsets[slot])
    }
}

/// Read via pre-resolved offset — single attribute fast path.
#[inline(always)]
pub fn read_at(world: &World, offset: AttrOffset) -> f64 {
    world.storage.read_abs(offset.0)
}

/// Write via pre-resolved offset — single attribute fast path.
#[inline(always)]
pub fn write_at(world: &mut World, offset: AttrOffset, value: f64) {
    world.storage.write_abs(offset.0, value);
}

/// Ergonomic per-entity reference that caches archetype + base offset.
///
/// Unlike `EntityAccessor` (which pre-resolves specific named attributes),
/// `EntityRef` caches only the base offset and archetype index, allowing you
/// to read/write any attribute by type without pre-resolving each one.
///
/// # Example
/// ```ignore
/// let body = world.entity_ref(handle).expect("alive");
/// let x = body.get::<PosX>(&world).unwrap();
/// let y = body.get::<PosY>(&world).unwrap();
/// body.set::<PosX>(&mut world, x + 1.0);
/// ```
#[derive(Clone, Debug)]
pub struct EntityRef {
    archetype_idx: usize,
    inner_idx: usize,
    base_offset: usize,
    layout_version: u64,
}

impl EntityRef {
    /// Create a new EntityRef. Returns None if entity is dead (including generation mismatch).
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
        })
    }

    /// Read a typed attribute. Returns None if attribute not in this entity's schema.
    #[inline]
    pub fn get<A: TypedAttr>(&self, world: &World) -> Option<f64> {
        let interned = world.interner.find(A::NAME)?;
        let arch = &world.archetypes[self.archetype_idx];
        let field = arch.schema.find_field(interned)?;
        Some(world.storage.read_abs(self.base_offset + field.0 as usize))
    }

    /// Write a typed attribute. Returns false if attribute not in schema.
    #[inline]
    pub fn set<A: TypedAttr>(&self, world: &mut World, value: f64) -> bool {
        let interned = match world.interner.find(A::NAME) {
            Some(id) => id,
            None => return false,
        };
        let field = match world.archetypes[self.archetype_idx]
            .schema
            .find_field(interned)
        {
            Some(f) => f,
            None => return false,
        };
        world
            .storage
            .write_abs(self.base_offset + field.0 as usize, value);
        true
    }

    /// Get the absolute offset for a typed attribute.
    #[inline]
    pub fn absolute_offset<A: TypedAttr>(&self, world: &World) -> Option<AttrOffset> {
        let interned = world.interner.find(A::NAME)?;
        let arch = &world.archetypes[self.archetype_idx];
        let field = arch.schema.find_field(interned)?;
        Some(AttrOffset(self.base_offset + field.0 as usize))
    }

    /// Check if this ref is stale (layout changed since creation).
    #[inline]
    pub fn is_stale(&self, world: &World) -> bool {
        self.layout_version != world.layout_version()
    }

    /// The cached base offset for this entity
    #[inline]
    pub fn base_offset(&self) -> usize {
        self.base_offset
    }

    /// The archetype index (for advanced schema queries)
    #[inline]
    pub fn archetype_idx(&self) -> usize {
        self.archetype_idx
    }
}