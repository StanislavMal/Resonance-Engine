// src/prefab.rs
//! Prefab system — template-based batch entity creation
//!
//! Resonator factories stored as Fn (not FnOnce) so they can be called
//! multiple times for batch spawning. Factories produce Arc<DynResonator>
//! which is shared across all entities of same archetype.

use crate::entity::EntityHandle;
use crate::resonator::{DynResonator, FieldMap, Resonator};
use crate::typed_attrs::TypedAttr;
use crate::world::World;
use std::sync::Arc;

struct PrefabAttr {
    name: &'static str,
    default_value: f64,
}

pub struct Prefab {
    name: &'static str,
    attrs: Vec<PrefabAttr>,
    /// Factories that produce resonators given field bindings
    resonator_factories: Vec<Arc<dyn Fn(&FieldMap) -> Arc<DynResonator> + Send + Sync>>,
}

impl Prefab {
    pub fn new(name: &'static str) -> Self {
        Self { name, attrs: Vec::new(), resonator_factories: Vec::new() }
    }

    pub fn attr(mut self, name: &'static str, default: f64) -> Self {
        self.attrs.push(PrefabAttr { name, default_value: default });
        self
    }

    pub fn attr_typed<A: TypedAttr>(self, default: f64) -> Self { self.attr(A::NAME, default) }
    pub fn tag(self, name: &'static str) -> Self { self.attr(name, 1.0) }
    pub fn tag_typed<A: TypedAttr>(self) -> Self { self.tag(A::NAME) }

    pub fn on<F, R>(mut self, factory: F) -> Self
    where
        F: Fn(&FieldMap) -> R + Send + Sync + 'static,
        R: Resonator,
    {
        self.resonator_factories.push(Arc::new(move |map: &FieldMap| {
            Arc::new(factory(map)) as Arc<DynResonator>
        }));
        self
    }

    pub fn spawn(&self, world: &mut World) -> EntityHandle {
        self.spawn_with(world, &[])
    }

    pub fn spawn_with(&self, world: &mut World, overrides: &[(&str, f64)]) -> EntityHandle {
        let mut builder = world.entity(self.name);

        for attr in &self.attrs {
            let value = overrides.iter()
                .find(|(n, _)| *n == attr.name)
                .map(|(_, v)| *v)
                .unwrap_or(attr.default_value);
            builder = builder.attr(attr.name, value);
        }

        // Clone factory Arcs for the FnOnce boundary of EntityBuilder::on
        for factory_arc in &self.resonator_factories {
            let f = Arc::clone(factory_arc);
            builder = builder.on(move |map: &FieldMap| {
                // Call the Arc<Fn> to get the resonator
                // But we need to return a Resonator impl, not Arc<DynResonator>
                // Wrap in a struct that delegates
                SharedResonator { inner: f(map) }
            });
        }

        builder.done()
    }

    pub fn spawn_batch(&self, world: &mut World, count: usize) -> Vec<EntityHandle> {
        (0..count).map(|_| self.spawn(world)).collect()
    }

    pub fn spawn_batch_with<F>(
        &self,
        world: &mut World,
        count: usize,
        mut configure: F,
    ) -> Vec<EntityHandle>
    where
        F: FnMut(usize) -> Vec<(&'static str, f64)>,
    {
        (0..count).map(|i| self.spawn_with(world, &configure(i))).collect()
    }
}

/// Wrapper to make Arc<DynResonator> implement Resonator
struct SharedResonator {
    inner: Arc<DynResonator>,
}

impl Resonator for SharedResonator {
    fn apply(&self, ctx: &mut crate::context::NodeContext) {
        self.inner.apply(ctx);
    }
}

pub trait WorldPrefabExt {
    fn prefab(&self, name: &'static str) -> Prefab;
}

impl WorldPrefabExt for World {
    fn prefab(&self, name: &'static str) -> Prefab {
        Prefab::new(name)
    }
}