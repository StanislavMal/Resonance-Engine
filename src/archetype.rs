// src/archetype.rs
//! Archetype system — shared schema, batch execution
//!
//! Одна HashMap на архетип вместо одной на сущность.
//! Batch execution: один vtable call на архетип, tight loop по данным.

use crate::interning::InternedStr;
use crate::resonator::{DynResonator, GpuDispatchConfig};
use crate::storage::{FieldIndex, FloatOffset};
use std::collections::HashMap;
use std::sync::Arc;

/// Уникальный ID архетипа
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArchetypeId(pub u32);

/// Уникальный ID сущности с generational index
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

/// Схема архетипа: какие атрибуты, в каком порядке
#[derive(Clone, Debug)]
pub struct ArchetypeSchema {
    /// Имя → позиция поля внутри архетипа
    pub field_map: HashMap<InternedStr, FieldIndex>,
    /// Обратный порядок: позиция → имя (для отладки)
    pub field_names: Vec<InternedStr>,
    /// Количество float полей на сущность
    pub floats_per_entity: u16,
    /// Количество int полей на сущность
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

    /// Добавить float атрибут, вернуть его FieldIndex
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

    /// Найти поле по имени
    #[inline]
    pub fn find_field(&self, name: InternedStr) -> Option<FieldIndex> {
        self.field_map.get(&name).copied()
    }

    /// Сигнатура архетипа (для дедупликации): сортированный набор имён
    pub fn signature(&self) -> Vec<InternedStr> {
        let mut sig: Vec<InternedStr> = self.field_map.keys().copied().collect();
        sig.sort_by_key(|s| s.0);
        sig
    }
}

/// Данные одного архетипа: все сущности одного типа
pub struct Archetype {
    pub id: ArchetypeId,
    pub schema: ArchetypeSchema,
    /// Entity IDs в порядке добавления
    pub entities: Vec<EntityId>,
    /// Float offsets: entities[i] начинается с float_offsets[i]
    pub float_offsets: Vec<FloatOffset>,
    /// Shared resonators для всех сущностей этого архетипа
    pub resonators: Vec<Arc<DynResonator>>,
    /// Имя архетипа (для отладки)
    pub name: String,
    /// Битовая маска живых сущностей
    alive: Vec<bool>,
    /// Free list внутри архетипа
    free_indices: Vec<usize>,
    /// Optional GPU resonator configuration for this archetype
    pub gpu_resonator: Option<GpuDispatchConfig>,
    /// Flag indicating GPU data needs sync from CPU
    pub needs_gpu_sync: bool,
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
            gpu_resonator: None,
            needs_gpu_sync: false,
        }
    }

    /// Добавить сущность, вернуть внутренний индекс
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

    /// Удалить сущность по внутреннему индексу
    pub fn remove_entity(&mut self, inner_idx: usize) -> bool {
        if inner_idx < self.alive.len() && self.alive[inner_idx] {
            self.alive[inner_idx] = false;
            self.free_indices.push(inner_idx);
            true
        } else {
            false
        }
    }

    /// Итератор по живым (inner_idx, entity_id, float_offset)
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

    /// Собрать вектор (float_offset, entity_id) только живых — для batch execution
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

/// Аллокатор EntityId с generational index
///
/// Использует битовый вектор для O(1) проверки alive вместо linear scan по free_list.
pub struct EntityAllocator {
    next_index: u32,
    generations: Vec<u32>,
    /// Битовый вектор: true = слот занят (alive), false = свободен
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

    /// Проверить что EntityId жив И generation совпадает — O(1)
    #[inline]
    pub fn is_alive(&self, id: EntityId) -> bool {
        let idx = id.index as usize;
        idx < self.generations.len()
            && self.generations[idx] == id.generation
            && self.alive_bits[idx]
    }

    /// Получить текущее поколение слота
    #[inline]
    pub fn generation_of(&self, index: u32) -> u32 {
        let idx = index as usize;
        if idx < self.generations.len() {
            self.generations[idx]
        } else {
            0
        }
    }

    /// Проверить что слот занят по индексу (без проверки generation) — O(1)
    #[inline]
    pub fn is_alive_index(&self, index: u32) -> bool {
        let idx = index as usize;
        idx < self.alive_bits.len() && self.alive_bits[idx]
    }
}