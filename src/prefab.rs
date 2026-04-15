// src/prefab.rs
//! Prefab system with struct-based resonators

use crate::entity::EntityHandle;
use crate::resonator::{DynResonator, FieldMap, Resonator, ResonatorFactory};
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
    resonator_types: Vec<&'static str>,
}

impl Prefab {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            attrs: Vec::new(),
            resonator_types: Vec::new(),
        }
    }

    pub fn attr(mut self, name: &'static str, default: f64) -> Self {
        self.attrs.push(PrefabAttr {
            name,
            default_value: default,
        });
        self
    }

    pub fn attr_typed<A: TypedAttr>(self, default: f64) -> Self {
        self.attr(A::NAME, default)
    }

    pub fn tag(self, name: &'static str) -> Self {
        self.attr(name, 1.0)
    }

    pub fn tag_typed<A: TypedAttr>(self) -> Self {
        self.tag(A::NAME)
    }

    /// Register a resonator type (must be registered in World first)
    pub fn with_resonator(mut self, type_name: &'static str) -> Self {
        self.resonator_types.push(type_name);
        self
    }

    pub fn spawn(&self, world: &mut World) -> EntityHandle {
        self.spawn_with(world, &[])
    }

    pub fn spawn_with(&self, world: &mut World, overrides: &[(&str, f64)]) -> EntityHandle {
        let mut builder = world.entity(self.name);

        for attr in &self.attrs {
            let value = overrides
                .iter()
                .find(|(n, _)| *n == attr.name)
                .map(|(_, v)| *v)
                .unwrap_or(attr.default_value);
            builder = builder.attr(attr.name, value);
        }

        // Add resonator type markers
        for resonator_type in &self.resonator_types {
            builder = builder.resonator_type(resonator_type);
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
        (0..count)
            .map(|i| self.spawn_with(world, &configure(i)))
            .collect()
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