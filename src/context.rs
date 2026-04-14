// src/context.rs
//! NodeContext — per-entity access during resonator execution
//!
//! Own fields: read from WRITE buffer (see own changes within tick)
//! External fields: read from READ buffer (deterministic snapshot)
//!
//! ## Tick lifecycle
//!
//! Within a single tick, the order of operations is:
//!
//! 1. `storage.begin_tick()` — mark tick as active
//! 2. For each archetype (parallel across archetypes):
//!    a. Collect alive entities
//!    b. For each entity, create NodeContext
//!    c. Execute ALL resonators on that entity, in order they were added
//!    d. If `despawn_self()` was called, entity is added to despawn queue
//!       (but entity remains accessible for the rest of THIS tick)
//! 3. `storage.commit()` — sync double-buffer
//! 4. Process despawn queue — entities actually removed
//! 5. `storage.end_tick()` — mark tick as finished
//!
//! Key consequence: `despawn_self()` is deferred. The entity's data stays
//! valid for the remainder of the current tick. Other entities reading it
//! via `read_external` in the same tick will still see valid data.

use crate::accessor::AttrOffset;
use crate::archetype::EntityId;
use crate::storage::{FieldIndex, FloatOffset, StoragePtr};

/// Opaque request to spawn an entity after the current tick.
/// Collected from resonators and processed by World after tick completes.
#[derive(Clone, Debug)]
pub struct SpawnRequest {
    pub archetype_name: String,
    pub overrides: Vec<(String, f64)>,
}

/// Opaque request to modify another entity after the current tick.
/// Collected from resonators and processed by World after tick completes.
#[derive(Clone, Debug)]
pub struct WriteRequest {
    pub target_offset: AttrOffset,
    pub value: f64,
}

/// Command buffer for deferred operations from within resonators.
///
/// Resonators cannot directly spawn entities or write to other entities.
/// Instead, they push commands into this buffer, which the World processes
/// after all resonators have finished executing.
///
/// Thread safety: each parallel chunk gets its own CommandBuffer.
/// After the tick, all buffers are merged.
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

    /// Merge another buffer into this one (used after parallel execution)
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

    /// Read own float field.
    ///
    /// Reads from WRITE buffer — you see your own writes from earlier
    /// resonators in the same tick.
    #[inline(always)]
    pub fn get(&self, field: FieldIndex) -> f64 {
        unsafe {
            let abs = self.base_offset + field.0 as usize;
            *self.ptr.write_floats.add(abs)
        }
    }

    /// Write float field with epsilon check.
    ///
    /// If the new value differs from the old value by less than epsilon
    /// (default 1e-9), the write is skipped and dirty flag is NOT set.
    /// This avoids unnecessary buffer copies in double-buffer mode.
    ///
    /// Use `set_unchecked` when you know the value is changing.
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

    /// Write float field, always marks dirty.
    ///
    /// Skips the epsilon comparison. Use when you know the value is changing,
    /// or when performance matters more than skipping no-op writes.
    ///
    /// This is NOT "unchecked" in the unsafe sense — there are still bounds
    /// checks in debug mode. The name means "no epsilon check".
    #[inline(always)]
    pub fn set_unchecked(&mut self, field: FieldIndex, value: f64) {
        unsafe {
            self.ptr.write_float(self.base_offset + field.0 as usize, value);
            self.dirty = true;
        }
    }

    /// Read, apply function, write back (with epsilon check).
    #[inline(always)]
    pub fn modify(&mut self, field: FieldIndex, f: impl FnOnce(f64) -> f64) {
        let old = self.get(field);
        self.set(field, f(old));
    }

    /// Add to current value (no epsilon check).
    #[inline(always)]
    pub fn add(&mut self, field: FieldIndex, amount: f64) {
        let old = self.get(field);
        self.set_unchecked(field, old + amount);
    }

    // ─── Entity identity ──────────────────────────────

    /// This entity's ID
    #[inline(always)]
    pub fn entity_id(&self) -> EntityId {
        self.entity_id
    }

    /// Base offset of this entity in the global float buffer.
    /// Useful for skipping self when iterating over pre-resolved offsets.
    #[inline(always)]
    pub fn base_offset(&self) -> usize {
        self.base_offset
    }

    // ─── Despawn ──────────────────────────────────────

    /// Request despawn at end of tick.
    ///
    /// The entity remains alive and accessible for the rest of the current tick.
    /// All remaining resonators for this entity will still execute.
    /// Other entities reading this entity via `read_external` will still see valid data.
    /// Actual removal happens after `storage.commit()`.
    #[inline(always)]
    pub fn despawn_self(&mut self) {
        self.despawn_requested = true;
    }

    #[inline(always)]
    pub fn wants_despawn(&self) -> bool {
        self.despawn_requested
    }

    // ─── Cross-entity reads ───────────────────────────

    /// Read another entity's attribute value.
    ///
    /// - In **Double buffer** mode: reads from previous tick's snapshot — deterministic.
    /// - In **Single buffer** mode: reads current (possibly modified) data.
    ///
    /// The offset must come from `world.absolute_offset::<Attr>(entity)` or
    /// `EntityAccessor::resolve::<Attr>(world, entity)`. Do not fabricate offsets.
    ///
    /// If the target entity was despawned, the data at that offset is still
    /// physically present but stale. For correctness, re-resolve offsets after
    /// any despawn cycle, or check validity before reading.
    #[inline(always)]
    pub fn read_external(&self, offset: AttrOffset) -> f64 {
        unsafe { self.ptr.read_float(offset.0) }
    }

    /// Read another entity's field using base offset + field index.
    ///
    /// Safer than raw offset: `FieldIndex` comes from schema resolution.
    #[inline(always)]
    pub fn read_external_field(&self, base_offset: usize, field: FieldIndex) -> f64 {
        unsafe { self.ptr.read_float(base_offset + field.0 as usize) }
    }

    // ─── Deferred commands ────────────────────────────

    /// Request to write a value to another entity's attribute after this tick.
    ///
    /// The write is deferred — it will be applied by World after all resonators
    /// have finished executing. This preserves determinism: all resonators in a
    /// tick see the same snapshot of data.
    ///
    /// # Example
    /// ```ignore
    /// // Pre-resolve target offset before tick:
    /// let target_hp = world.absolute_offset::<Health>(enemy).unwrap();
    ///
    /// // In resonator:
    /// ctx.defer_write(target_hp, current_target_hp - 10.0);
    /// ```
    #[inline]
    pub fn defer_write(&mut self, target_offset: AttrOffset, value: f64) {
        self.commands.write_requests.push(WriteRequest {
            target_offset,
            value,
        });
    }

    /// Request to spawn a new entity after this tick.
    ///
    /// The entity will be created using `spawn_runtime` with the given
    /// archetype name and attribute overrides.
    ///
    /// # Example
    /// ```ignore
    /// ctx.defer_spawn("Bullet", vec![
    ///     ("PosX".into(), my_x),
    ///     ("PosY".into(), my_y),
    ///     ("VelX".into(), aim_x * 10.0),
    /// ]);
    /// ```
    #[inline]
    pub fn defer_spawn(&mut self, archetype_name: &str, overrides: Vec<(String, f64)>) {
        self.commands.spawn_requests.push(SpawnRequest {
            archetype_name: archetype_name.to_string(),
            overrides,
        });
    }

    /// Request to despawn another entity after this tick.
    ///
    /// Unlike `despawn_self()`, this targets a different entity by its EntityId.
    #[inline]
    pub fn defer_despawn(&mut self, target: EntityId) {
        self.commands.despawn_requests.push(target);
    }

    /// Take the accumulated command buffer (called by scheduler after resonators)
    #[inline]
    pub fn take_commands(&mut self) -> CommandBuffer {
        std::mem::take(&mut self.commands)
    }
}