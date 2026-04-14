// src/world.rs
//! World — main simulation container

use crate::accessor::{AttrOffset, EntityRef};
use crate::archetype::*;
use crate::context::{CommandBuffer, NodeContext};
use crate::entity::{EntityBuilder, EntityHandle, PendingEntity};
use crate::gpu::{GpuContext, GpuExecutor};
use crate::gpu::shader::builtins;
use crate::interning::{InternedStr, StringInterner};
use crate::resonator::{DynResonator, FieldMap, Resonator};
use crate::scheduler::{self, SchedulerConfig, TickResult};
use crate::storage::{FieldIndex, Storage};
use crate::typed_attrs::TypedAttr;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) struct EntityLoc {
    pub(crate) archetype_idx: usize,
    pub(crate) inner_idx: usize,
}

#[derive(Clone, Debug)]
pub struct Phase {
    pub name: String,
    pub order: u32,
}

#[derive(Debug, Clone)]
pub enum BuildWarning {
    EntityWithoutResonators {
        archetype_name: String,
        entity_count: usize,
    },
    EmptyArchetype {
        name: String,
    },
}

/// Registered prefab schema for deferred spawning from resonators
struct PrefabRegistration {
    archetype_name: String,
    attr_names: Vec<String>,
    attr_defaults: Vec<f64>,
    /// Index into archetype_signatures (resolved after first build)
    archetype_idx: Option<usize>,
}

pub struct World {
    pub(crate) interner: StringInterner,
    pub(crate) storage: Storage,
    pub(crate) archetypes: Vec<Archetype>,
    pub(crate) allocator: EntityAllocator,
    pub(crate) pending_entities: Vec<PendingEntity>,
    pub(crate) config: SchedulerConfig,

    pub(crate) entity_locations: Vec<Option<EntityLoc>>,
    archetype_signatures: HashMap<Vec<InternedStr>, usize>,
    next_archetype_id: u32,
    built: bool,
    layout_version: u64,
    cached_entities: Vec<EntityHandle>,
    entities_dirty: bool,

    pub attr_index: HashMap<InternedStr, Vec<(usize, FieldIndex)>>,

    phases: Vec<Phase>,
    archetype_phases: HashMap<usize, String>,

    /// Registered prefab schemas for defer_spawn
    prefab_registry: HashMap<String, PrefabRegistration>,
}

impl World {
    pub fn new() -> Self {
        Self {
            interner: StringInterner::new(),
            storage: Storage::new(),
            archetypes: Vec::new(),
            allocator: EntityAllocator::new(),
            pending_entities: Vec::new(),
            config: SchedulerConfig::default(),
            entity_locations: Vec::new(),
            archetype_signatures: HashMap::new(),
            next_archetype_id: 0,
            built: false,
            layout_version: 1,
            cached_entities: Vec::new(),
            entities_dirty: true,
            attr_index: HashMap::new(),
            phases: Vec::new(),
            archetype_phases: HashMap::new(),
            prefab_registry: HashMap::new(),
        }
    }

    // ─── Entity Lifecycle ─────────────────────────────

    /// Check if entity is alive with full generation validation — O(1).
    ///
    /// Returns false if entity was despawned or handle has stale generation.
    #[inline]
    pub fn is_alive(&self, entity: EntityHandle) -> bool {
        self.allocator.is_alive(entity.0)
    }

    // ─── Phase Management ──────────────────────────────

    pub fn add_phase(&mut self, name: &str) {
        let order = self.phases.len() as u32;
        self.phases.push(Phase {
            name: name.to_string(),
            order,
        });
    }

    pub fn assign_phase(&mut self, archetype_name: &str, phase_name: &str) {
        for (idx, arch) in self.archetypes.iter().enumerate() {
            if arch.name == archetype_name {
                self.archetype_phases.insert(idx, phase_name.to_string());
            }
        }
    }

    // ─── Prefab Registration (for defer_spawn) ────────

    /// Register a prefab schema so that `defer_spawn("Name", overrides)` works.
    ///
    /// Must be called before the first tick that uses defer_spawn with this name.
    /// The prefab must have been previously created and built at least once so
    /// its archetype exists.
    pub fn register_spawnable(&mut self, name: &str, attr_names: &[&str], defaults: &[f64]) {
        assert_eq!(
            attr_names.len(),
            defaults.len(),
            "register_spawnable: attr_names and defaults must have same length"
        );
        let sig: Vec<InternedStr> = attr_names
            .iter()
            .map(|n| self.interner.intern(n))
            .collect();
        let mut sorted_sig = sig.clone();
        sorted_sig.sort_by_key(|s| s.0);
        let arch_idx = self.archetype_signatures.get(&sorted_sig).copied();

        self.prefab_registry.insert(
            name.to_string(),
            PrefabRegistration {
                archetype_name: name.to_string(),
                attr_names: attr_names.iter().map(|s| s.to_string()).collect(),
                attr_defaults: defaults.to_vec(),
                archetype_idx: arch_idx,
            },
        );
    }

    // ─── Entity Creation ──────────────────────────────

    pub fn entity(&mut self, name: &str) -> EntityBuilder<'_> {
        let entity_id = self.allocator.allocate();
        let name_id = self.interner.intern(name);
        EntityBuilder {
            world: self,
            entity_id,
            name: name_id,
            schema: ArchetypeSchema::new(),
            defaults: Vec::new(),
            resonator_factories: Vec::new(),
            gpu_config: None,
        }
    }

    // ─── Build ────────────────────────────────────────

    pub fn build(&mut self) {
        let pending = std::mem::take(&mut self.pending_entities);

        let mut by_signature: HashMap<Vec<InternedStr>, Vec<PendingEntity>> = HashMap::new();
        for pe in pending {
            let sig = pe.schema.signature();
            by_signature.entry(sig).or_default().push(pe);
        }

        for (sig, entities) in by_signature {
            let arch_idx = self.get_or_create_archetype(&sig, &entities[0]);
            let mut resonators_built = !self.archetypes[arch_idx].resonators.is_empty();

            for pe in entities {
                let floats_per = self.archetypes[arch_idx].schema.floats_per_entity as usize;
                let float_offset = self.storage.alloc_floats(floats_per);
                let inner_idx =
                    self.archetypes[arch_idx].add_entity(pe.entity_id, float_offset);

                let eid_idx = pe.entity_id.index as usize;
                if self.entity_locations.len() <= eid_idx {
                    self.entity_locations.resize_with(eid_idx + 1, || None);
                }
                self.entity_locations[eid_idx] = Some(EntityLoc {
                    archetype_idx: arch_idx,
                    inner_idx,
                });

                for (field, value) in &pe.defaults {
                    self.storage.init_float(float_offset, *field, *value);
                }

                if !resonators_built && !pe.resonator_factories.is_empty() {
                    let field_map = FieldMap::new(
                        self.archetypes[arch_idx].schema.field_map.clone(),
                        self.interner.clone_for_read(),
                    );
                    let mut resonators = Vec::new();
                    for factory in pe.resonator_factories {
                        resonators.push(factory(&field_map));
                    }
                    self.archetypes[arch_idx].resonators = resonators;
                    resonators_built = true;
                }

                // Transfer GPU config from pending entity to archetype
                if let Some(gpu_config) = pe.gpu_config {
                    self.archetypes[arch_idx].gpu_resonator = Some(gpu_config);
                    self.archetypes[arch_idx].needs_gpu_sync = true;
                }
            }
        }

        self.rebuild_attr_index();
        self.resolve_prefab_archetypes();
        self.built = true;
        self.layout_version += 1;
        self.entities_dirty = true;
    }

    pub fn build_with_validation(&mut self) -> Vec<BuildWarning> {
        self.build();
        let mut warnings = Vec::new();
        for arch in &self.archetypes {
            let alive = arch.alive_count();
            if alive > 0 && arch.resonators.is_empty() {
                warnings.push(BuildWarning::EntityWithoutResonators {
                    archetype_name: arch.name.clone(),
                    entity_count: alive,
                });
            }
            if alive == 0 {
                warnings.push(BuildWarning::EmptyArchetype {
                    name: arch.name.clone(),
                });
            }
        }
        warnings
    }

    pub fn spawn_runtime(&mut self) {
        let pending = std::mem::take(&mut self.pending_entities);
        if pending.is_empty() {
            return;
        }

        for pe in pending {
            let sig = pe.schema.signature();
            if let Some(&arch_idx) = self.archetype_signatures.get(&sig) {
                let floats_per = self.archetypes[arch_idx].schema.floats_per_entity as usize;
                let float_offset = self.storage.alloc_floats(floats_per);
                let inner_idx =
                    self.archetypes[arch_idx].add_entity(pe.entity_id, float_offset);

                let eid_idx = pe.entity_id.index as usize;
                if self.entity_locations.len() <= eid_idx {
                    self.entity_locations.resize_with(eid_idx + 1, || None);
                }
                self.entity_locations[eid_idx] = Some(EntityLoc {
                    archetype_idx: arch_idx,
                    inner_idx,
                });

                for (field, value) in &pe.defaults {
                    self.storage.init_float(float_offset, *field, *value);
                }

                // Transfer GPU config for runtime-spawned entities
                if let Some(gpu_config) = pe.gpu_config {
                    self.archetypes[arch_idx].gpu_resonator = Some(gpu_config);
                    self.archetypes[arch_idx].needs_gpu_sync = true;
                }
            } else {
                self.pending_entities.push(pe);
            }
        }

        if !self.pending_entities.is_empty() {
            self.build();
        } else {
            self.entities_dirty = true;
        }
    }

    fn get_or_create_archetype(
        &mut self,
        sig: &[InternedStr],
        template: &PendingEntity,
    ) -> usize {
        if let Some(&idx) = self.archetype_signatures.get(sig) {
            return idx;
        }
        let arch_id = ArchetypeId(self.next_archetype_id);
        self.next_archetype_id += 1;
        let name = self.interner.resolve(template.name).to_string();
        let archetype = Archetype::new(arch_id, template.schema.clone(), name);
        let idx = self.archetypes.len();
        self.archetypes.push(archetype);
        self.archetype_signatures.insert(sig.to_vec(), idx);
        idx
    }

    fn rebuild_attr_index(&mut self) {
        self.attr_index.clear();
        for (arch_idx, arch) in self.archetypes.iter().enumerate() {
            for (&attr_id, &field_idx) in &arch.schema.field_map {
                self.attr_index
                    .entry(attr_id)
                    .or_default()
                    .push((arch_idx, field_idx));
            }
        }
    }

    fn resolve_prefab_archetypes(&mut self) {
        for reg in self.prefab_registry.values_mut() {
            if reg.archetype_idx.is_some() {
                continue;
            }
            let sig: Vec<InternedStr> = reg
                .attr_names
                .iter()
                .filter_map(|n| self.interner.find(n))
                .collect();
            let mut sorted = sig.clone();
            sorted.sort_by_key(|s| s.0);
            reg.archetype_idx = self.archetype_signatures.get(&sorted).copied();
        }
    }

    // ─── Tick ─────────────────────────────────────────

    pub fn tick(&mut self) -> TickResult {
        self.storage.begin_tick();
        let result = if self.phases.is_empty() {
            self.tick_all()
        } else {
            self.tick_phased()
        };
        self.storage.end_tick();
        result
    }

    fn tick_all(&mut self) -> TickResult {
        let sp = self.storage.raw_ptrs();
        let mut total_result = TickResult::default();
        let mut all_despawns: Vec<EntityId> = Vec::new();
        let mut all_commands = CommandBuffer::new();

        for archetype in &self.archetypes {
            let batch_result = scheduler::execute_archetype(archetype, sp, &self.config);
            total_result.total_calls += batch_result.calls;
            total_result.entities_processed += batch_result.processed;
            total_result.dirty_count += batch_result.dirty;
            all_despawns.extend(batch_result.despawn_list);
            all_commands.merge(batch_result.commands);
        }

        self.storage.commit();

        // Process despawns
        total_result.despawn_requests = all_despawns.len() as u64;
        for entity_id in all_despawns {
            self.despawn_internal(entity_id);
        }

        // Process deferred commands
        self.process_commands(all_commands);

        total_result
    }

    fn tick_phased(&mut self) -> TickResult {
        let sp = self.storage.raw_ptrs();
        let mut total_result = TickResult::default();
        let mut all_despawns: Vec<EntityId> = Vec::new();
        let mut all_commands = CommandBuffer::new();

        for phase in &self.phases {
            let phase_name = &phase.name;
            let phase_archetypes: Vec<usize> = self
                .archetype_phases
                .iter()
                .filter(|(_, name)| name.as_str() == phase_name.as_str())
                .map(|(&idx, _)| idx)
                .collect();

            if phase_archetypes.len() >= 2 {
                let results: Vec<_> = phase_archetypes
                    .par_iter()
                    .map(|&arch_idx| {
                        scheduler::execute_archetype(
                            &self.archetypes[arch_idx],
                            sp,
                            &self.config,
                        )
                    })
                    .collect();
                for batch_result in results {
                    total_result.total_calls += batch_result.calls;
                    total_result.entities_processed += batch_result.processed;
                    total_result.dirty_count += batch_result.dirty;
                    all_despawns.extend(batch_result.despawn_list);
                    all_commands.merge(batch_result.commands);
                }
            } else {
                for &arch_idx in &phase_archetypes {
                    let batch_result = scheduler::execute_archetype(
                        &self.archetypes[arch_idx],
                        sp,
                        &self.config,
                    );
                    total_result.total_calls += batch_result.calls;
                    total_result.entities_processed += batch_result.processed;
                    total_result.dirty_count += batch_result.dirty;
                    all_despawns.extend(batch_result.despawn_list);
                    all_commands.merge(batch_result.commands);
                }
            }
        }

        // Unassigned archetypes
        let unassigned: Vec<usize> = (0..self.archetypes.len())
            .filter(|idx| !self.archetype_phases.contains_key(idx))
            .collect();

        if unassigned.len() >= 2 {
            let results: Vec<_> = unassigned
                .par_iter()
                .map(|&arch_idx| {
                    scheduler::execute_archetype(
                        &self.archetypes[arch_idx],
                        sp,
                        &self.config,
                    )
                })
                .collect();
            for batch_result in results {
                total_result.total_calls += batch_result.calls;
                total_result.entities_processed += batch_result.processed;
                total_result.dirty_count += batch_result.dirty;
                all_despawns.extend(batch_result.despawn_list);
                all_commands.merge(batch_result.commands);
            }
        } else {
            for &arch_idx in &unassigned {
                let batch_result = scheduler::execute_archetype(
                    &self.archetypes[arch_idx],
                    sp,
                    &self.config,
                );
                total_result.total_calls += batch_result.calls;
                total_result.entities_processed += batch_result.processed;
                total_result.dirty_count += batch_result.dirty;
                all_despawns.extend(batch_result.despawn_list);
                all_commands.merge(batch_result.commands);
            }
        }

        self.storage.commit();

        total_result.despawn_requests = all_despawns.len() as u64;
        for entity_id in all_despawns {
            self.despawn_internal(entity_id);
        }

        self.process_commands(all_commands);

        total_result
    }

    /// Process deferred commands from resonators
    fn process_commands(&mut self, commands: CommandBuffer) {
        // 1. Deferred writes
        for write in commands.write_requests {
            self.storage.write_abs(write.target_offset.0, write.value);
        }

        // 2. Deferred despawns (of other entities)
        for entity_id in commands.despawn_requests {
            self.despawn_internal(entity_id);
        }

        // 3. Deferred spawns
        if !commands.spawn_requests.is_empty() {
            for spawn in commands.spawn_requests {
                self.spawn_deferred(&spawn.archetype_name, &spawn.overrides);
            }
            // Process any newly added pending entities
            if !self.pending_entities.is_empty() {
                self.spawn_runtime();
            }
        }
    }

    /// Spawn an entity from a registered prefab with overrides
    fn spawn_deferred(&mut self, archetype_name: &str, overrides: &[(String, f64)]) {
        // Look up registered prefab
        let reg = match self.prefab_registry.get(archetype_name) {
            Some(r) => r,
            None => {
                eprintln!(
                    "Warning: defer_spawn for unregistered archetype '{}'. \
                     Call world.register_spawnable() first.",
                    archetype_name
                );
                return;
            }
        };

        let arch_idx = match reg.archetype_idx {
            Some(idx) => idx,
            None => {
                eprintln!(
                    "Warning: archetype '{}' not yet built. \
                     Ensure at least one entity of this type exists before defer_spawn.",
                    archetype_name
                );
                return;
            }
        };

        let entity_id = self.allocator.allocate();
        let floats_per = self.archetypes[arch_idx].schema.floats_per_entity as usize;
        let float_offset = self.storage.alloc_floats(floats_per);
        let inner_idx = self.archetypes[arch_idx].add_entity(entity_id, float_offset);

        let eid_idx = entity_id.index as usize;
        if self.entity_locations.len() <= eid_idx {
            self.entity_locations.resize_with(eid_idx + 1, || None);
        }
        self.entity_locations[eid_idx] = Some(EntityLoc {
            archetype_idx: arch_idx,
            inner_idx,
        });

        // Apply defaults
        for (i, attr_name) in reg.attr_names.iter().enumerate() {
            if let Some(interned) = self.interner.find(attr_name) {
                if let Some(field) = self.archetypes[arch_idx].schema.find_field(interned) {
                    let value = overrides
                        .iter()
                        .find(|(n, _)| n == attr_name)
                        .map(|(_, v)| *v)
                        .unwrap_or(reg.attr_defaults[i]);
                    self.storage.init_float(float_offset, field, value);
                }
            }
        }

        self.entities_dirty = true;
    }

    fn despawn_internal(&mut self, entity_id: EntityId) {
        let eid_idx = entity_id.index as usize;
        if eid_idx >= self.entity_locations.len() {
            return;
        }
        if let Some(loc) = self.entity_locations[eid_idx].take() {
            if loc.archetype_idx < self.archetypes.len() {
                self.archetypes[loc.archetype_idx].remove_entity(loc.inner_idx);
            }
            self.allocator.deallocate(entity_id);
            self.entities_dirty = true;
        }
    }

    // ─── Read/Write API ───────────────────────────────

    pub fn read(&self, entity: EntityHandle, attr_name: &str) -> Option<f64> {
        let attr_id = self.interner.find(attr_name)?;
        self.read_by_id(entity, attr_id)
    }

    pub fn read_typed<A: TypedAttr>(&self, entity: EntityHandle) -> Option<f64> {
        let attr_id = self.interner.find(A::NAME)?;
        self.read_by_id(entity, attr_id)
    }

    fn read_by_id(&self, entity: EntityHandle, attr_id: InternedStr) -> Option<f64> {
        if !self.allocator.is_alive(entity.0) {
            return None;
        }
        let eid_idx = entity.0.index as usize;
        let loc = self.entity_locations.get(eid_idx)?.as_ref()?;
        let arch = &self.archetypes[loc.archetype_idx];
        if !arch.is_alive(loc.inner_idx) {
            return None;
        }
        let field = arch.schema.find_field(attr_id)?;
        let offset = arch.float_offsets[loc.inner_idx];
        Some(self.storage.read_float(offset, field))
    }

    pub fn write(&mut self, entity: EntityHandle, attr_name: &str, value: f64) -> bool {
        let attr_id = match self.interner.find(attr_name) {
            Some(id) => id,
            None => return false,
        };
        self.write_by_id(entity, attr_id, value)
    }

    pub fn write_typed<A: TypedAttr>(&mut self, entity: EntityHandle, value: f64) -> bool {
        let attr_id = match self.interner.find(A::NAME) {
            Some(id) => id,
            None => return false,
        };
        self.write_by_id(entity, attr_id, value)
    }

    fn write_by_id(
        &mut self,
        entity: EntityHandle,
        attr_id: InternedStr,
        value: f64,
    ) -> bool {
        if !self.allocator.is_alive(entity.0) {
            return false;
        }
        let eid_idx = entity.0.index as usize;
        let loc = match self.entity_locations.get(eid_idx).and_then(|l| l.as_ref()) {
            Some(l) => l,
            None => return false,
        };
        let arch = &self.archetypes[loc.archetype_idx];
        if !arch.is_alive(loc.inner_idx) {
            return false;
        }
        let field = match arch.schema.find_field(attr_id) {
            Some(f) => f,
            None => return false,
        };
        let offset = arch.float_offsets[loc.inner_idx];
        self.storage
            .write_abs(offset.0 as usize + field.0 as usize, value);
        true
    }

    pub fn read_batch_2<A: TypedAttr, B: TypedAttr>(
        &self,
        entities: &[EntityHandle],
    ) -> Vec<(Option<f64>, Option<f64>)> {
        let attr_a = self.interner.find(A::NAME);
        let attr_b = self.interner.find(B::NAME);
        entities
            .iter()
            .map(|entity| {
                if !self.allocator.is_alive(entity.0) {
                    return (None, None);
                }
                let eid_idx = entity.0.index as usize;
                let loc = match self.entity_locations.get(eid_idx).and_then(|l| l.as_ref()) {
                    Some(l) => l,
                    None => return (None, None),
                };
                let arch = &self.archetypes[loc.archetype_idx];
                if !arch.is_alive(loc.inner_idx) {
                    return (None, None);
                }
                let offset = arch.float_offsets[loc.inner_idx];
                let va = attr_a
                    .and_then(|id| arch.schema.find_field(id))
                    .map(|field| self.storage.read_float(offset, field));
                let vb = attr_b
                    .and_then(|id| arch.schema.find_field(id))
                    .map(|field| self.storage.read_float(offset, field));
                (va, vb)
            })
            .collect()
    }

    pub fn read_batch_3<A: TypedAttr, B: TypedAttr, C: TypedAttr>(
        &self,
        entities: &[EntityHandle],
    ) -> Vec<(Option<f64>, Option<f64>, Option<f64>)> {
        let attr_a = self.interner.find(A::NAME);
        let attr_b = self.interner.find(B::NAME);
        let attr_c = self.interner.find(C::NAME);
        entities
            .iter()
            .map(|entity| {
                if !self.allocator.is_alive(entity.0) {
                    return (None, None, None);
                }
                let eid_idx = entity.0.index as usize;
                let loc = match self.entity_locations.get(eid_idx).and_then(|l| l.as_ref()) {
                    Some(l) => l,
                    None => return (None, None, None),
                };
                let arch = &self.archetypes[loc.archetype_idx];
                if !arch.is_alive(loc.inner_idx) {
                    return (None, None, None);
                }
                let offset = arch.float_offsets[loc.inner_idx];
                let va = attr_a
                    .and_then(|id| arch.schema.find_field(id))
                    .map(|field| self.storage.read_float(offset, field));
                let vb = attr_b
                    .and_then(|id| arch.schema.find_field(id))
                    .map(|field| self.storage.read_float(offset, field));
                let vc = attr_c
                    .and_then(|id| arch.schema.find_field(id))
                    .map(|field| self.storage.read_float(offset, field));
                (va, vb, vc)
            })
            .collect()
    }

    pub fn despawn(&mut self, entity: EntityHandle) {
        if self.allocator.is_alive(entity.0) {
            self.despawn_internal(entity.0);
        }
    }

    // ─── EntityRef ────────────────────────────────────

    pub fn entity_ref(&self, entity: EntityHandle) -> Option<EntityRef> {
        EntityRef::new(self, entity)
    }

    pub fn absolute_offset<A: TypedAttr>(&self, entity: EntityHandle) -> Option<AttrOffset> {
        crate::accessor::EntityAccessor::resolve::<A>(self, entity)
    }

    // ─── Queries ──────────────────────────────────────

    pub fn entities_iter(&self) -> impl Iterator<Item = EntityHandle> + '_ {
        self.archetypes
            .iter()
            .flat_map(|arch| arch.alive_iter().map(|(_, eid, _)| EntityHandle(eid)))
    }

    pub fn refresh_entity_cache(&mut self) {
        if self.entities_dirty {
            self.cached_entities = self.entities_iter().collect();
            self.entities_dirty = false;
        }
    }

    pub fn entities(&self) -> &[EntityHandle] {
        &self.cached_entities
    }

    pub fn alive_count(&self) -> usize {
        self.archetypes.iter().map(|a| a.alive_count()).sum()
    }

    pub fn memory_bytes(&self) -> usize {
        self.storage.memory_bytes()
    }

    pub fn total_memory_bytes(&self) -> usize {
        self.storage.total_memory_bytes()
    }

    pub fn attr_values<A: TypedAttr>(&self) -> Vec<f64> {
        let attr_id = match self.interner.find(A::NAME) {
            Some(id) => id,
            None => return Vec::new(),
        };
        let mut values = Vec::new();
        if let Some(locations) = self.attr_index.get(&attr_id) {
            for &(arch_idx, field_idx) in locations {
                let arch = &self.archetypes[arch_idx];
                for (_, _, offset) in arch.alive_iter() {
                    values.push(self.storage.read_float(offset, field_idx));
                }
            }
        }
        values
    }

    pub fn apply_resonator<R: Resonator>(
        &mut self,
        entity: EntityHandle,
        resonator: &R,
    ) {
        if !self.allocator.is_alive(entity.0) {
            return;
        }
        let eid_idx = entity.0.index as usize;
        if let Some(Some(loc)) = self.entity_locations.get(eid_idx) {
            let arch = &self.archetypes[loc.archetype_idx];
            if arch.is_alive(loc.inner_idx) {
                let offset = arch.float_offsets[loc.inner_idx];
                let sp = self.storage.raw_ptrs();
                let mut ctx = NodeContext::new(sp, offset, entity.0, self.config.epsilon);
                resonator.apply(&mut ctx);
            }
        }
    }

    pub fn add_resonator_runtime<R>(
        &mut self,
        entity: EntityHandle,
        resonator_factory: R,
    ) where
        R: FnOnce(&FieldMap) -> Arc<DynResonator> + 'static,
    {
        if !self.allocator.is_alive(entity.0) {
            return;
        }
        let eid_idx = entity.0.index as usize;
        if let Some(Some(loc)) = self.entity_locations.get(eid_idx) {
            let arch_idx = loc.archetype_idx;
            if arch_idx < self.archetypes.len() {
                let field_map = FieldMap::new(
                    self.archetypes[arch_idx].schema.field_map.clone(),
                    self.interner.clone_for_read(),
                );
                let resonator = resonator_factory(&field_map);
                self.archetypes[arch_idx].resonators.push(resonator);
                self.entities_dirty = true;
            }
        }
    }

    pub fn set_buffer_mode(&mut self, mode: crate::storage::BufferMode) {
        self.storage.set_mode(mode);
    }

    pub fn buffer_mode(&self) -> crate::storage::BufferMode {
        self.storage.mode()
    }

    pub fn archetype_count(&self) -> usize {
        self.archetypes.len()
    }

    pub fn archetype_info(&self) -> Vec<ArchetypeInfo> {
        self.archetypes
            .iter()
            .map(|a| ArchetypeInfo {
                name: a.name.clone(),
                alive: a.alive_count(),
                total_slots: a.total_slots(),
                floats_per_entity: a.schema.floats_per_entity,
                resonator_count: a.resonators.len(),
            })
            .collect()
    }

    pub fn layout_version(&self) -> u64 {
        self.layout_version
    }
    pub fn interner(&self) -> &StringInterner {
        &self.interner
    }
    pub fn interner_mut(&mut self) -> &mut StringInterner {
        &mut self.interner
    }

    // ─── Handle Resolution ───────────────────────────

    pub fn get_handle_by_index(&self, index: u32) -> Option<EntityHandle> {
        if !self.allocator.is_alive_index(index) {
            return None;
        }
        let idx = index as usize;
        if let Some(Some(loc)) = self.entity_locations.get(idx) {
            let arch = &self.archetypes[loc.archetype_idx];
            if arch.is_alive(loc.inner_idx) {
                let generation = self.allocator.generation_of(index);
                return Some(EntityHandle(EntityId::new(index, generation)));
            }
        }
        None
    }

    pub fn resolve_spatial_entries(
        &self,
        entries: &[crate::spatial::SpatialEntry],
    ) -> Vec<(EntityHandle, f64, f64)> {
        let mut result = Vec::with_capacity(entries.len());
        for entry in entries {
            if let Some(handle) = self.get_handle_by_index(entry.entity_index) {
                result.push((handle, entry.x, entry.y));
            }
        }
        result
    }

    // ─── Tag / Attr Checking ──────────────────────────

    pub fn has_tag<A: TypedAttr>(&self, entity: EntityHandle) -> bool {
        self.read_typed::<A>(entity)
            .map(|v| v == 1.0)
            .unwrap_or(false)
    }

    pub fn has_attr<A: TypedAttr>(&self, entity: EntityHandle) -> bool {
        self.read_typed::<A>(entity).is_some()
    }

    pub fn has_attr_named(&self, entity: EntityHandle, name: &str) -> bool {
        self.read(entity, name).is_some()
    }

    // ─── Snapshot / Debug ─────────────────────────────

    pub fn snapshot_json(&self) -> String {
        let mut result = String::from("[\n");
        let mut first = true;
        for arch in &self.archetypes {
            for (_, eid, offset) in arch.alive_iter() {
                if !first {
                    result.push_str(",\n");
                }
                first = false;
                result.push_str(&format!(
                    "  {{\"id\":{},\"arch\":\"{}\",\"attrs\":{{",
                    eid.index,
                    arch.name.replace('\"', "\\\"")
                ));
                let mut first_attr = true;
                for (&attr_id, &field_idx) in &arch.schema.field_map {
                    if !first_attr {
                        result.push(',');
                    }
                    first_attr = false;
                    let name = self.interner.resolve(attr_id).replace('\"', "\\\"");
                    let val = self.storage.read_float(offset, field_idx);
                    result.push_str(&format!("\"{}\":{}", name, val));
                }
                result.push_str("}}");
            }
        }
        result.push_str("\n]");
        result
    }

    pub fn snapshot_csv(&self, attr_names: &[&str]) -> String {
        let mut result = String::from("entity_id");
        for name in attr_names {
            result.push(',');
            result.push_str(name);
        }
        result.push('\n');

        let attr_ids: Vec<Option<InternedStr>> =
            attr_names.iter().map(|n| self.interner.find(n)).collect();

        for arch in &self.archetypes {
            for (_, eid, offset) in arch.alive_iter() {
                result.push_str(&format!("{}", eid.index));
                for attr_id_opt in &attr_ids {
                    result.push(',');
                    if let Some(attr_id) = attr_id_opt {
                        if let Some(field) = arch.schema.find_field(*attr_id) {
                            let val = self.storage.read_float(offset, field);
                            result.push_str(&format!("{:.6}", val));
                        }
                    }
                }
                result.push('\n');
            }
        }
        result
    }

    // ─── Hybrid GPU/CPU Tick ──────────────────────────

    /// Execute a tick with hybrid CPU/GPU processing
    /// 
    /// This method overlaps CPU and GPU work:
    /// 1. Sync dirty CPU data to GPU buffers
    /// 2. Dispatch GPU compute shaders for GPU-enabled archetypes
    /// 3. Execute CPU resonators for remaining archetypes in parallel
    /// 4. Return without waiting for GPU completion (async overlap)
    pub fn tick_hybrid(&mut self, gpu_ctx: &mut GpuContext) -> scheduler::HybridTickResult {
        use crate::gpu::shader::GpuShader;

        self.storage.begin_tick();
        
        let mut gpu_commands_submitted = 0usize;
        let mut gpu_entities_processed = 0u64;

        // Step 1: Collect indices for sync (avoid borrow conflict)
        let sync_indices: Vec<usize> = self.archetypes
            .iter()
            .enumerate()
            .filter(|(_, arch)| arch.needs_gpu_sync && arch.gpu_resonator.is_some())
            .map(|(idx, _)| idx)
            .collect();
        
        // Now sync using collected indices
        for arch_idx in sync_indices {
            gpu_ctx.sync_archetype(self, arch_idx);
            self.archetypes[arch_idx].needs_gpu_sync = false;
        }

        // Step 2: Collect GPU and CPU tasks
        let mut gpu_executor = GpuExecutor::new();
        let mut cpu_archetypes: Vec<usize> = Vec::new();

        // Create shader registry for this frame
        let mut shader_registry = crate::gpu::shader::ShaderRegistry::new();

        for (arch_idx, archetype) in self.archetypes.iter().enumerate() {
            if let Some(ref gpu_config) = archetype.gpu_resonator {
                // This archetype has GPU execution enabled
                let entity_count = archetype.alive_count();
                if entity_count > 0 {
                    // Get or compile the shader
                    let wgsl_source = match gpu_config.entry_point {
                        "main" => builtins::PARTICLE_PHYSICS_WGSL,
                        _ => builtins::ATTRIBUTE_TRANSFORM_WGSL,
                    };
                    
                    let shader = shader_registry.get_or_compile(
                        &gpu_ctx.device,
                        "particle_physics",
                        wgsl_source,
                        gpu_config.clone(),
                    );

                    gpu_executor.queue_dispatch(
                        archetype.id,
                        shader,
                        entity_count,
                    );
                    gpu_entities_processed += entity_count as u64;
                }
            } else {
                // CPU-only archetype
                cpu_archetypes.push(arch_idx);
            }
        }

        // Step 3: Execute GPU dispatches
        if !gpu_executor.pending_count() == 0 {
            let mut encoder = gpu_ctx.create_command_encoder();
            let _results = gpu_executor.execute(gpu_ctx, &mut encoder);
            gpu_commands_submitted = gpu_executor.pending_count();
            gpu_ctx.submit_commands(encoder);
        }

        // Step 4: Execute CPU resonators in parallel
        let mut cpu_result = TickResult::default();
        let sp = self.storage.raw_ptrs();
        let mut all_despawns: Vec<EntityId> = Vec::new();
        let mut all_commands = CommandBuffer::new();

        if cpu_archetypes.len() >= 2 {
            let results: Vec<_> = cpu_archetypes
                .par_iter()
                .map(|&arch_idx| {
                    scheduler::execute_archetype(&self.archetypes[arch_idx], sp, &self.config)
                })
                .collect();
            
            for batch_result in results {
                cpu_result.total_calls += batch_result.calls;
                cpu_result.entities_processed += batch_result.processed;
                cpu_result.dirty_count += batch_result.dirty;
                all_despawns.extend(batch_result.despawn_list);
                all_commands.merge(batch_result.commands);
            }
        } else {
            for &arch_idx in &cpu_archetypes {
                let batch_result = scheduler::execute_archetype(
                    &self.archetypes[arch_idx],
                    sp,
                    &self.config,
                );
                cpu_result.total_calls += batch_result.calls;
                cpu_result.entities_processed += batch_result.processed;
                cpu_result.dirty_count += batch_result.dirty;
                all_despawns.extend(batch_result.despawn_list);
                all_commands.merge(batch_result.commands);
            }
        }

        self.storage.commit();

        // Process despawns and commands
        cpu_result.despawn_requests = all_despawns.len() as u64;
        for entity_id in all_despawns {
            self.despawn_internal(entity_id);
        }
        self.process_commands(all_commands);

        self.storage.end_tick();

        scheduler::HybridTickResult {
            cpu_result,
            gpu_commands_submitted,
            gpu_entities_processed,
        }
    }

    /// Sync all archetype data to GPU buffers (call before first tick_hybrid)
    pub fn sync_all_to_gpu(&mut self, gpu_ctx: &mut GpuContext) {
        let sync_indices: Vec<usize> = self.archetypes
            .iter()
            .enumerate()
            .filter(|(_, arch)| arch.gpu_resonator.is_some() && arch.alive_count() > 0)
            .map(|(idx, _)| idx)
            .collect();
        
        for arch_idx in sync_indices {
            gpu_ctx.sync_archetype(self, arch_idx);
            self.archetypes[arch_idx].needs_gpu_sync = false;
        }
    }
}

#[derive(Debug, Clone)]
pub struct ArchetypeInfo {
    pub name: String,
    pub alive: usize,
    pub total_slots: usize,
    pub floats_per_entity: u16,
    pub resonator_count: usize,
}