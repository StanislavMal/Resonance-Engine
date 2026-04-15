// src/archetype.rs
//! Archetype system — shared schema, batch execution

use crate::interning::InternedStr;
use crate::resonator::DynResonator;
use crate::storage::{FieldIndex, FloatOffset};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArchetypeId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId {
    pub index: u32,
    pub generation: u32,
}

impl EntityId {
    pub fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "E({}:g{})", self.index, self.generation)
    }
}

#[derive(Clone, Debug)]
pub struct ArchetypeSchema {
    pub field_map: HashMap<InternedStr, FieldIndex>,
    pub field_names: Vec<InternedStr>,
    pub floats_per_entity: u16,
    pub ints_per_entity: u16,
}

impl ArchetypeSchema {
    pub fn new() -> Self {
        Self {
            field_map: HashMap::new(),
            field_names: Vec::new(),
            floats_per_entity: 0,
            ints_per_entity: 0,
        }
    }

    pub fn add_float(&mut self, name: InternedStr) -> FieldIndex {
        if let Some(&idx) = self.field_map.get(&name) {
            return idx;
        }
        let idx = FieldIndex(self.floats_per_entity);
        self.field_map.insert(name, idx);
        self.field_names.push(name);
        self.floats_per_entity += 1;
        idx
    }

    #[inline]
    pub fn find_field(&self, name: InternedStr) -> Option<FieldIndex> {
        self.field_map.get(&name).copied()
    }

    pub fn signature(&self) -> Vec<InternedStr> {
        let mut sig: Vec<InternedStr> = self.field_map.keys().copied().collect();
        sig.sort_by_key(|s| s.0);
        sig
    }
}

impl Default for ArchetypeSchema {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Archetype {
    pub id: ArchetypeId,
    pub schema: ArchetypeSchema,
    pub entities: Vec<EntityId>,
    pub float_offsets: Vec<FloatOffset>,
    pub resonators: Vec<Arc<DynResonator>>,
    pub name: String,
    alive: Vec<bool>,
    free_indices: Vec<usize>,
}

impl Archetype {
    pub fn new(id: ArchetypeId, schema: ArchetypeSchema, name: String) -> Self {
        Self {
            id,
            schema,
            entities: Vec::new(),
            float_offsets: Vec::new(),
            resonators: Vec::new(),
            name,
            alive: Vec::new(),
            free_indices: Vec::new(),
        }
    }

    pub fn add_entity(&mut self, entity_id: EntityId, float_offset: FloatOffset) -> usize {
        if let Some(reuse_idx) = self.free_indices.pop() {
            self.entities[reuse_idx] = entity_id;
            self.float_offsets[reuse_idx] = float_offset;
            self.alive[reuse_idx] = true;
            reuse_idx
        } else {
            let idx = self.entities.len();
            self.entities.push(entity_id);
            self.float_offsets.push(float_offset);
            self.alive.push(true);
            idx
        }
    }

    pub fn remove_entity(&mut self, inner_idx: usize) -> bool {
        if inner_idx < self.alive.len() && self.alive[inner_idx] {
            self.alive[inner_idx] = false;
            self.free_indices.push(inner_idx);
            true
        } else {
            false
        }
    }

    pub fn alive_iter(&self) -> impl Iterator<Item = (usize, EntityId, FloatOffset)> + '_ {
        self.entities
            .iter()
            .enumerate()
            .zip(self.float_offsets.iter())
            .zip(self.alive.iter())
            .filter(|(_, alive)| **alive)
            .map(|(((idx, &eid), &offset), _)| (idx, eid, offset))
    }

    pub fn alive_count(&self) -> usize {
        self.alive.iter().filter(|&&a| a).count()
    }

    pub fn total_slots(&self) -> usize {
        self.entities.len()
    }

    #[inline]
    pub fn is_alive(&self, inner_idx: usize) -> bool {
        inner_idx < self.alive.len() && self.alive[inner_idx]
    }

    pub fn collect_alive_batch(&self) -> Vec<(FloatOffset, EntityId)> {
        let mut batch = Vec::with_capacity(self.alive_count());
        for i in 0..self.entities.len() {
            if self.alive[i] {
                batch.push((self.float_offsets[i], self.entities[i]));
            }
        }
        batch
    }
}

pub struct EntityAllocator {
    next_index: u32,
    generations: Vec<u32>,
    alive_bits: Vec<bool>,
    free_list: Vec<u32>,
}

impl EntityAllocator {
    pub fn new() -> Self {
        Self {
            next_index: 0,
            generations: Vec::new(),
            alive_bits: Vec::new(),
            free_list: Vec::new(),
        }
    }

    pub fn allocate(&mut self) -> EntityId {
        if let Some(index) = self.free_list.pop() {
            self.alive_bits[index as usize] = true;
            EntityId::new(index, self.generations[index as usize])
        } else {
            let index = self.next_index;
            self.next_index += 1;
            let idx = index as usize;
            if self.generations.len() <= idx {
                self.generations.resize(idx + 1, 0);
                self.alive_bits.resize(idx + 1, false);
            }
            self.alive_bits[idx] = true;
            EntityId::new(index, 0)
        }
    }

    pub fn deallocate(&mut self, id: EntityId) {
        let idx = id.index as usize;
        if idx < self.generations.len() {
            self.generations[idx] += 1;
            self.alive_bits[idx] = false;
            self.free_list.push(id.index);
        }
    }

    #[inline]
    pub fn is_alive(&self, id: EntityId) -> bool {
        let idx = id.index as usize;
        idx < self.generations.len()
            && self.generations[idx] == id.generation
            && self.alive_bits[idx]
    }

    #[inline]
    pub fn generation_of(&self, index: u32) -> u32 {
        let idx = index as usize;
        if idx < self.generations.len() {
            self.generations[idx]
        } else {
            0
        }
    }

    #[inline]
    pub fn is_alive_index(&self, index: u32) -> bool {
        let idx = index as usize;
        idx < self.alive_bits.len() && self.alive_bits[idx]
    }
}

impl Default for EntityAllocator {
    fn default() -> Self {
        Self::new()
    }
}