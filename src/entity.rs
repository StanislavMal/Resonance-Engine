// src/entity.rs
//! EntityHandle, EntityBuilder — ergonomic entity construction

use crate::archetype::{ArchetypeId, ArchetypeSchema, EntityId};
use crate::interning::InternedStr;
use crate::resonator::{DynResonator, FieldMap};
use crate::storage::FieldIndex;
use crate::typed_attrs::TypedAttr;
use std::collections::HashMap;
use std::sync::Arc;

/// External handle to an entity
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntityHandle(pub EntityId);

impl EntityHandle {
    pub fn id(&self) -> EntityId { self.0 }
}

/// Pending entity data before build
pub(crate) struct PendingEntity {
    pub entity_id: EntityId,
    pub name: InternedStr,
    pub schema: ArchetypeSchema,
    pub defaults: Vec<(FieldIndex, f64)>,
    pub resonator_factories: Vec<Box<dyn FnOnce(&FieldMap) -> Arc<DynResonator>>>,
}

/// Builder for creating entities with fluent API
pub struct EntityBuilder<'w> {
    pub(crate) world: &'w mut crate::world::World,
    pub(crate) entity_id: EntityId,
    pub(crate) name: InternedStr,
    pub(crate) schema: ArchetypeSchema,
    pub(crate) defaults: Vec<(FieldIndex, f64)>,
    pub(crate) resonator_factories: Vec<Box<dyn FnOnce(&FieldMap) -> Arc<DynResonator>>>,
}

impl<'w> EntityBuilder<'w> {
    /// Add a float attribute with initial value
    pub fn attr(mut self, name: &str, value: f64) -> Self {
        let id = self.world.interner.intern(name);
        let field = self.schema.add_float(id);
        self.defaults.push((field, value));
        self
    }

    /// Add typed attribute
    pub fn attr_typed<A: TypedAttr>(self, value: f64) -> Self {
        self.attr(A::NAME, value)
    }

    /// Add tag (attribute with value 1.0)
    pub fn tag(self, name: &str) -> Self {
        self.attr(name, 1.0)
    }

    /// Add typed tag
    pub fn tag_typed<A: TypedAttr>(self) -> Self {
        self.tag(A::NAME)
    }

    /// Add resonator via factory closure
    pub fn on<F, R>(mut self, factory: F) -> Self
    where
        F: FnOnce(&FieldMap) -> R + 'static,
        R: crate::resonator::Resonator,
    {
        self.resonator_factories.push(Box::new(move |map| {
            Arc::new(factory(map)) as Arc<DynResonator>
        }));
        self
    }

    /// Finalize and register entity
    pub fn done(self) -> EntityHandle {
        let handle = EntityHandle(self.entity_id);
        let pending = PendingEntity {
            entity_id: self.entity_id,
            name: self.name,
            schema: self.schema,
            defaults: self.defaults,
            resonator_factories: self.resonator_factories,
        };
        self.world.pending_entities.push(pending);
        handle
    }
}