// src/entity.rs
//! EntityHandle, EntityBuilder — ergonomic entity construction

use crate::archetype::{ArchetypeSchema, EntityId};
use crate::interning::InternedStr;
use crate::storage::FieldIndex;
use crate::typed_attrs::TypedAttr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntityHandle(pub EntityId);

impl EntityHandle {
    pub fn id(&self) -> EntityId {
        self.0
    }
}

pub(crate) struct PendingEntity {
    pub entity_id: EntityId,
    pub name: InternedStr,
    pub schema: ArchetypeSchema,
    pub defaults: Vec<(FieldIndex, f64)>,
    pub resonator_types: Vec<String>,
}

pub struct EntityBuilder<'w> {
    pub(crate) world: &'w mut crate::world::World,
    pub(crate) entity_id: EntityId,
    pub(crate) name: InternedStr,
    pub(crate) schema: ArchetypeSchema,
    pub(crate) defaults: Vec<(FieldIndex, f64)>,
    pub(crate) resonator_types: Vec<String>,
}

impl<'w> EntityBuilder<'w> {
    pub fn attr(mut self, name: &str, value: f64) -> Self {
        let id = self.world.interner.intern(name);
        let field = self.schema.add_float(id);
        self.defaults.push((field, value));
        self
    }

    pub fn attr_typed<A: TypedAttr>(self, value: f64) -> Self {
        self.attr(A::NAME, value)
    }

    pub fn tag(self, name: &str) -> Self {
        self.attr(name, 1.0)
    }

    pub fn tag_typed<A: TypedAttr>(self) -> Self {
        self.tag(A::NAME)
    }

    /// Register a resonator type for this entity
    pub fn resonator_type(mut self, type_name: &str) -> Self {
        self.resonator_types.push(type_name.to_string());
        self
    }

    pub fn done(self) -> EntityHandle {
        let handle = EntityHandle(self.entity_id);
        let pending = PendingEntity {
            entity_id: self.entity_id,
            name: self.name,
            schema: self.schema,
            defaults: self.defaults,
            resonator_types: self.resonator_types,
        };
        self.world.pending_entities.push(pending);
        handle
    }
}