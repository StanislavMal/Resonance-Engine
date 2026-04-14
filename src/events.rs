// src/events.rs
//! Typed event channels — lock-free, no HashMap, no dyn Any

use std::collections::VecDeque;

pub trait Event: Send + Sync + Clone + 'static {}

/// Typed event channel
pub struct EventChannel<E: Event> {
    buffer: VecDeque<E>,
}

impl<E: Event> EventChannel<E> {
    pub fn new() -> Self { Self { buffer: VecDeque::new() } }
    pub fn emit(&mut self, event: E) { self.buffer.push_back(event); }
    pub fn drain(&mut self) -> impl Iterator<Item = E> + '_ { self.buffer.drain(..) }
    pub fn is_empty(&self) -> bool { self.buffer.is_empty() }
    pub fn len(&self) -> usize { self.buffer.len() }
    pub fn iter(&self) -> impl Iterator<Item = &E> { self.buffer.iter() }
    pub fn clear(&mut self) { self.buffer.clear(); }
}

// Common events
#[derive(Debug, Clone)]
pub struct DespawnEvent { pub entity_id: crate::archetype::EntityId }
impl Event for DespawnEvent {}

#[derive(Debug, Clone)]
pub struct SpawnEvent { pub entity_id: crate::archetype::EntityId }
impl Event for SpawnEvent {}

#[derive(Debug, Clone)]
pub struct CollisionEvent {
    pub entity_a: crate::archetype::EntityId,
    pub entity_b: crate::archetype::EntityId,
    pub overlap: f64,
}
impl Event for CollisionEvent {}