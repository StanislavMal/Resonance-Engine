// src/entity.rs
//! EntityHandle, EntityBuilder — ergonomic entity construction

use crate::archetype::{ArchetypeId, ArchetypeSchema, EntityId};
use crate::interning::InternedStr;
use crate::resonator::{DynResonator, FieldMap, GpuDispatchConfig};
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
    /// Optional GPU resonator configuration for hybrid execution
    pub gpu_config: Option<GpuDispatchConfig>,
}

/// Builder for creating entities with fluent API
pub struct EntityBuilder<'w> {
    pub(crate) world: &'w mut crate::world::World,
    pub(crate) entity_id: EntityId,
    pub(crate) name: InternedStr,
    pub(crate) schema: ArchetypeSchema,
    pub(crate) defaults: Vec<(FieldIndex, f64)>,
    pub(crate) resonator_factories: Vec<Box<dyn FnOnce(&FieldMap) -> Arc<DynResonator>>>,
    pub(crate) gpu_config: Option<GpuDispatchConfig>,
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

    /// Enable GPU execution for this archetype with the specified shader
    /// 
    /// This marks the archetype for GPU compute processing during tick_hybrid().
    /// The entry_point should match a function in the WGSL shader.
    pub fn with_gpu_resonator(mut self, entry_point: &'static str) -> Self {
        self.gpu_config = Some(GpuDispatchConfig {
            entry_point,
            workgroup_size: (64, 1, 1),
        });
        self
    }

    /// Create multiple entities of the same archetype (instancing)
    pub fn count(self, count: usize) -> Vec<EntityHandle> {
        let mut handles = Vec::with_capacity(count);
        let gpu_config = self.gpu_config.clone();
        
        for _ in 0..count {
            // Clone the builder state for each entity
            let entity_id = self.world.allocator.allocate();
            let pending = PendingEntity {
                entity_id,
                name: self.name,
                schema: self.schema.clone(),
                defaults: self.defaults.clone(),
                resonator_factories: vec![], // Resonators are shared, not cloned
                gpu_config: gpu_config.clone(),
            };
            handles.push(EntityHandle(entity_id));
            self.world.pending_entities.push(pending);
        }
        
        // Add resonators only once (they're shared across the archetype)
        if !self.resonator_factories.is_empty() {
            let factories: Vec<_> = self.resonator_factories.into_iter().collect();
            let pending = PendingEntity {
                entity_id: self.entity_id,
                name: self.name,
                schema: self.schema,
                defaults: self.defaults,
                resonator_factories: factories,
                gpu_config: self.gpu_config,
            };
            self.world.pending_entities.push(pending);
        }
        
        handles
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
            gpu_config: self.gpu_config,
        };
        self.world.pending_entities.push(pending);
        handle
    }
}