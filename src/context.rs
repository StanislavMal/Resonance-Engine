// src/context.rs
//! NodeContext — per-entity access during resonator execution with safe external reads

use crate::accessor::AttrOffset;
use crate::archetype::EntityId;
use crate::storage::{FieldIndex, FloatOffset, StoragePtr};
use crate::world::World;

#[derive(Clone, Debug)]
pub struct SpawnRequest {
    pub archetype_name: String,
    pub overrides: Vec<(String, f64)>,
}

#[derive(Clone, Debug)]
pub struct WriteRequest {
    pub target_offset: AttrOffset,
    pub value: f64,
}

#[derive(Clone, Debug, Default)]
pub struct CommandBuffer {
    pub spawn_requests: Vec<SpawnRequest>,
    pub write_requests: Vec<WriteRequest>,
    pub despawn_requests: Vec<EntityId>,
}

impl CommandBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.spawn_requests.is_empty()
            && self.write_requests.is_empty()
            && self.despawn_requests.is_empty()
    }

    pub fn merge(&mut self, other: CommandBuffer) {
        self.spawn_requests.extend(other.spawn_requests);
        self.write_requests.extend(other.write_requests);
        self.despawn_requests.extend(other.despawn_requests);
    }
}

pub struct NodeContext {
    ptr: StoragePtr,
    base_offset: usize,
    entity_id: EntityId,
    epsilon: f64,
    pub dirty: bool,
    despawn_requested: bool,
    commands: CommandBuffer,
}

impl NodeContext {
    #[inline(always)]
    pub fn new(
        ptr: StoragePtr,
        float_offset: FloatOffset,
        entity_id: EntityId,
        epsilon: f64,
    ) -> Self {
        Self {
            ptr,
            base_offset: float_offset.0 as usize,
            entity_id,
            epsilon,
            dirty: false,
            despawn_requested: false,
            commands: CommandBuffer::new(),
        }
    }

    // ─── Own field access ─────────────────────────────

    #[inline(always)]
    pub fn get(&self, field: FieldIndex) -> f64 {
        unsafe {
            let abs = self.base_offset + field.0 as usize;
            *self.ptr.write_floats.add(abs)
        }
    }

    #[inline(always)]
    pub fn set(&mut self, field: FieldIndex, value: f64) {
        unsafe {
            let abs = self.base_offset + field.0 as usize;
            let old = *self.ptr.write_floats.add(abs);
            if (old - value).abs() > self.epsilon {
                self.ptr.write_float(abs, value);
                self.dirty = true;
            }
        }
    }

    #[inline(always)]
    pub fn set_unchecked(&mut self, field: FieldIndex, value: f64) {
        unsafe {
            self.ptr.write_float(self.base_offset + field.0 as usize, value);
            self.dirty = true;
        }
    }

    #[inline(always)]
    pub fn modify(&mut self, field: FieldIndex, f: impl FnOnce(f64) -> f64) {
        let old = self.get(field);
        self.set(field, f(old));
    }

    #[inline(always)]
    pub fn add(&mut self, field: FieldIndex, amount: f64) {
        let old = self.get(field);
        self.set_unchecked(field, old + amount);
    }

    // ─── Entity identity ──────────────────────────────

    #[inline(always)]
    pub fn entity_id(&self) -> EntityId {
        self.entity_id
    }

    #[inline(always)]
    pub fn base_offset(&self) -> usize {
        self.base_offset
    }

    // ─── Despawn ──────────────────────────────────────

    #[inline(always)]
    pub fn despawn_self(&mut self) {
        self.despawn_requested = true;
    }

    #[inline(always)]
    pub fn wants_despawn(&self) -> bool {
        self.despawn_requested
    }

    // ─── Cross-entity reads (NOW SAFE) ────────────────

    /// Read another entity's attribute value with validation
    ///
    /// Returns None if target entity was despawned or generation mismatched
    #[inline(always)]
    pub fn read_external(&self, offset: AttrOffset, world: &World) -> Option<f64> {
        // Validate generation before read
        if !offset.validate(world) {
            return None;
        }
        Some(unsafe { self.ptr.read_float(offset.offset) })
    }

    /// Read without validation (use only if you know entity is alive)
    #[inline(always)]
    pub unsafe fn read_external_unchecked(&self, offset: AttrOffset) -> f64 {
        self.ptr.read_float(offset.offset)
    }

    #[inline(always)]
    pub fn read_external_field(&self, base_offset: usize, field: FieldIndex) -> f64 {
        unsafe { self.ptr.read_float(base_offset + field.0 as usize) }
    }

    // ─── Deferred commands ────────────────────────────

    #[inline]
    pub fn defer_write(&mut self, target_offset: AttrOffset, value: f64) {
        self.commands.write_requests.push(WriteRequest {
            target_offset,
            value,
        });
    }

    #[inline]
    pub fn defer_spawn(&mut self, archetype_name: &str, overrides: Vec<(String, f64)>) {
        self.commands.spawn_requests.push(SpawnRequest {
            archetype_name: archetype_name.to_string(),
            overrides,
        });
    }

    #[inline]
    pub fn defer_despawn(&mut self, target: EntityId) {
        self.commands.despawn_requests.push(target);
    }

    #[inline]
    pub fn take_commands(&mut self) -> CommandBuffer {
        std::mem::take(&mut self.commands)
    }
}