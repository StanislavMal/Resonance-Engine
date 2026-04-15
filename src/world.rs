// src/world.rs

use crate::accessor::{AttrOffset, EntityRef};
use crate::archetype::*;
use crate::context::CommandBuffer;
use crate::entity::{EntityBuilder, EntityHandle, PendingEntity};
use crate::graph::MetaGraph;
use crate::interning::{InternedStr, StringInterner};
use crate::query::QueryBuilder;
use crate::relations::{Relation, RelationGraph};
use crate::resonator::{DynResonator, FieldMap, ResonatorFactory};
use crate::scheduler::{self, SchedulerConfig, TickResult};
use crate::serialization::{EntitySnapshot, SchemaSnapshot};
use crate::storage::{FieldIndex, Storage};  // ← ADD FieldIndex HERE
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

struct PrefabRegistration {
    #[allow(dead_code)]  // Used for validation
    archetype_name: String,
    attr_names: Vec<String>,
    attr_defaults: Vec<f64>,
    archetype_idx: Option<usize>,
}

/// Registry for resonator factories (struct-based)
struct ResonatorRegistry {
    factories: HashMap<&'static str, Box<dyn Fn(&FieldMap) -> Arc<DynResonator> + Send + Sync>>,
}

impl ResonatorRegistry {
    fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }
    
    fn register<R: ResonatorFactory>(&mut self, name: &'static str) {
        self.factories.insert(
            name,
            Box::new(|map: &FieldMap| Arc::new(R::create(map)) as Arc<DynResonator>),
        );
    }
    
    fn create(&self, name: &str, map: &FieldMap) -> Option<Arc<DynResonator>> {
        self.factories.get(name).map(|factory| factory(map))
    }
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
    prefab_registry: HashMap<String, PrefabRegistration>,
    pub(crate) relations: RelationGraph,
    resonator_registry: ResonatorRegistry,
    
    // NEW: Make public for external access
    pub meta_graph: MetaGraph,  // ← CHANGE FROM pub(crate) TO pub
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
            relations: RelationGraph::new(),
            resonator_registry: ResonatorRegistry::new(),
            meta_graph: MetaGraph::new(),  // NEW
        }
    }

    // ─── Resonator Registration ───────────────────────

    /// Register a struct-based resonator type
    pub fn register_resonator<R: ResonatorFactory>(&mut self, type_name: &'static str) {
        self.resonator_registry.register::<R>(type_name);
    }

    // ─── Entity Lifecycle ─────────────────────────────

    #[inline]
    pub fn is_alive(&self, entity: EntityHandle) -> bool {
        self.allocator.is_alive(entity.0)
    }

    // ─── Relations ────────────────────────────────────

    pub fn add_relation<R: Relation>(&mut self, source: EntityHandle, target: EntityHandle) {
        if self.is_alive(source) && self.is_alive(target) {
            self.relations.add::<R>(source.0, target.0);
        }
    }

    pub fn get_relation<R: Relation>(&self, source: EntityHandle) -> Option<EntityHandle> {
        self.relations
            .get::<R>(source.0)
            .filter(|&eid| self.allocator.is_alive(eid))
            .map(EntityHandle)
    }

    pub fn get_all_relations<R: Relation>(&self, source: EntityHandle) -> Vec<EntityHandle> {
        self.relations
            .get_all::<R>(source.0)
            .into_iter()
            .filter(|&eid| self.allocator.is_alive(eid))
            .map(EntityHandle)
            .collect()
    }
    
    pub fn get_reverse_relations<R: Relation>(&self, target: EntityHandle) -> Vec<EntityHandle> {
        self.relations
            .get_reverse::<R>(target.0)
            .into_iter()
            .filter(|&eid| self.allocator.is_alive(eid))
            .map(EntityHandle)
            .collect()
    }

    pub fn remove_relation<R: Relation>(&mut self, source: EntityHandle, target: EntityHandle) {
        self.relations.remove::<R>(source.0, target.0);
    }
    
    pub fn remove_all_relations<R: Relation>(&mut self, source: EntityHandle) {
        self.relations.remove_all::<R>(source.0);
    }
    
    pub fn has_relation<R: Relation>(&self, source: EntityHandle) -> bool {
        self.relations.has::<R>(source.0)
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

    // ─── Prefab Registration ──────────────────────────

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
            resonator_types: Vec::new(),
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
            let arch_id = self.archetypes[arch_idx].id;
            let mut resonators_built = !self.archetypes[arch_idx].resonators.is_empty();

            for pe in entities {
                let floats_per = self.archetypes[arch_idx].schema.floats_per_entity as usize;
                let float_offset = self.storage.alloc_floats(floats_per);
                let inner_idx = self.archetypes[arch_idx].add_entity(pe.entity_id, float_offset);

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

                // NEW: Add to graph
                let handle = EntityHandle(pe.entity_id);
                self.meta_graph.add_entity(handle, arch_id);

                if !resonators_built && !pe.resonator_types.is_empty() {
                    let field_map = FieldMap::new(
                        self.archetypes[arch_idx].schema.field_map.clone(),
                        self.interner.clone_for_read(),
                    );
                    
                    for type_name in &pe.resonator_types {
                        if let Some(resonator) = self.resonator_registry.create(type_name, &field_map) {
                            self.archetypes[arch_idx].resonators.push(resonator);
                        } else {
                            eprintln!("Warning: Resonator type '{}' not registered", type_name);
                        }
                    }
                    resonators_built = true;
                }
            }
        }

        self.rebuild_attr_index();
        self.resolve_prefab_archetypes();
        self.built = true;
        self.layout_version += 1;
        self.entities_dirty = true;
        
        self.refresh_entity_cache();
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
                let inner_idx = self.archetypes[arch_idx].add_entity(pe.entity_id, float_offset);

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

    // ─── Tick (UNCHANGED execution, graph metadata only) ─────────

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

        total_result.despawn_requests = all_despawns.len() as u64;
        for entity_id in all_despawns {
            self.despawn_internal(entity_id);
        }

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

        let unassigned: Vec<usize> = (0..self.archetypes.len())
            .filter(|idx| !self.archetype_phases.contains_key(idx))
            .collect();

        if unassigned.len() >= 2 {
            let results: Vec<_> = unassigned
                .par_iter()
                .map(|&arch_idx| {
                    scheduler::execute_archetype(&self.archetypes[arch_idx], sp, &self.config)
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
                let batch_result =
                    scheduler::execute_archetype(&self.archetypes[arch_idx], sp, &self.config);
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

    fn process_commands(&mut self, commands: CommandBuffer) {
        // Validate and apply deferred writes
        for write in commands.write_requests {
            if write.target_offset.validate(self) {
                self.storage.write_abs(write.target_offset.offset, write.value);
            }
        }

        for entity_id in commands.despawn_requests {
            self.despawn_internal(entity_id);
        }

        if !commands.spawn_requests.is_empty() {
            for spawn in commands.spawn_requests {
                self.spawn_deferred(&spawn.archetype_name, &spawn.overrides);
            }
            if !self.pending_entities.is_empty() {
                self.spawn_runtime();
            }
        }
    }

    fn spawn_deferred(&mut self, archetype_name: &str, overrides: &[(String, f64)]) {
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
            self.relations.remove_entity(entity_id);
            self.meta_graph.remove_entity(EntityHandle(entity_id));  // NEW
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

    fn write_by_id(&mut self, entity: EntityHandle, attr_id: InternedStr, value: f64) -> bool {
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

    // ─── Query ────────────────────────────────────────

    pub fn query(&self) -> QueryBuilder<'_> {
        QueryBuilder::new(self)
    }

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

    // ─── Serialization ────────────────────────────────

    pub fn snapshot(&self) -> SchemaSnapshot {
        let mut snapshot = SchemaSnapshot::new(self.layout_version);

        for arch in &self.archetypes {
            for (_, eid, offset) in arch.alive_iter() {
                let mut entity_snap = EntitySnapshot::new(
                    eid.index,
                    eid.generation,
                    arch.name.clone(),
                );

                for (&attr_id, &field_idx) in &arch.schema.field_map {
                    let name = self.interner.resolve(attr_id).to_string();
                    let value = self.storage.read_float(offset, field_idx);
                    entity_snap.add_attribute(name, value);
                }

                snapshot.add_entity(entity_snap);
            }
        }

        snapshot
    }

    pub fn restore(&mut self, snapshot: SchemaSnapshot) -> Result<(), String> {
        // Full state reset
        self.archetypes.clear();
        self.archetype_signatures.clear();
        self.allocator = EntityAllocator::new();
        self.entity_locations.clear();
        self.pending_entities.clear();
        self.relations = RelationGraph::new();
        self.meta_graph = MetaGraph::new();  // NEW
        self.attr_index.clear();
        self.cached_entities.clear();
        self.entities_dirty = true;
        self.next_archetype_id = 0;
        
        // Recreate entities from snapshot (without resonators)
        for entity_snap in snapshot.entities {
            let mut builder = self.entity(&entity_snap.archetype_name);

            for (attr_name, value) in entity_snap.attributes {
                builder = builder.attr(&attr_name, value);
            }

            builder.done();
        }

        Ok(())
    }

    pub fn save_snapshot(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let snapshot = self.snapshot();
        snapshot.save(path)?;
        Ok(())
    }

    pub fn load_snapshot(&mut self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let snapshot = SchemaSnapshot::load(path)?;
        self.restore(snapshot)?;
        Ok(())
    }

    // ─── Misc ─────────────────────────────────────────

    pub fn memory_bytes(&self) -> usize {
        self.storage.memory_bytes()
    }

    pub fn total_memory_bytes(&self) -> usize {
        self.storage.total_memory_bytes()
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

    pub fn has_tag<A: TypedAttr>(&self, entity: EntityHandle) -> bool {
        self.read_typed::<A>(entity).map(|v| v == 1.0).unwrap_or(false)
    }

    pub fn has_attr<A: TypedAttr>(&self, entity: EntityHandle) -> bool {
        self.read_typed::<A>(entity).is_some()
    }

    pub fn snapshot_json(&self) -> String {
        self.snapshot().to_json().unwrap_or_else(|e| {
            format!("{{\"error\": \"{}\"}}", e)
        })
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
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