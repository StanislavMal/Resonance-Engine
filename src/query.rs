// src/query.rs (FIX lifetime warning)

use crate::entity::EntityHandle;
use crate::interning::InternedStr;
use crate::typed_attrs::TypedAttr;
use crate::world::World;

pub fn with_attr<A: TypedAttr>(world: &World) -> Vec<EntityHandle> {
    let attr_id = match world.interner().find(A::NAME) {
        Some(id) => id,
        None => return Vec::new(),
    };

    let mut result = Vec::new();
    if let Some(locations) = world.attr_index.get(&attr_id) {
        for &(arch_idx, _) in locations {
            let arch = &world.archetypes[arch_idx];
            for (_, eid, _) in arch.alive_iter() {
                if world.is_alive(EntityHandle(eid)) {
                    result.push(EntityHandle(eid));
                }
            }
        }
    }
    result
}

pub fn where_attr<A: TypedAttr>(
    world: &World,
    pred: impl Fn(f64) -> bool,
) -> Vec<EntityHandle> {
    let attr_id = match world.interner().find(A::NAME) {
        Some(id) => id,
        None => return Vec::new(),
    };

    let mut result = Vec::new();
    if let Some(locations) = world.attr_index.get(&attr_id) {
        for &(arch_idx, field_idx) in locations {
            let arch = &world.archetypes[arch_idx];
            for (_, eid, offset) in arch.alive_iter() {
                let val = world.storage.read_float(offset, field_idx);
                if pred(val) {
                    result.push(EntityHandle(eid));
                }
            }
        }
    }
    result
}

pub fn count_with<A: TypedAttr>(world: &World) -> usize {
    let attr_id = match world.interner().find(A::NAME) {
        Some(id) => id,
        None => return 0,
    };
    world
        .attr_index
        .get(&attr_id)
        .map(|locs| {
            locs.iter()
                .map(|&(ai, _)| world.archetypes[ai].alive_count())
                .sum()
        })
        .unwrap_or(0)
}

pub struct QueryBuilder<'w> {
    world: &'w World,
    include: Vec<InternedStr>,
    exclude: Vec<InternedStr>,
    filters: Vec<Box<dyn Fn(EntityHandle, &World) -> bool + 'w>>,
}

impl<'w> QueryBuilder<'w> {
    pub fn new(world: &'w World) -> Self {
        Self {
            world,
            include: Vec::new(),
            exclude: Vec::new(),
            filters: Vec::new(),
        }
    }
    
    pub fn with<A: TypedAttr>(mut self) -> Self {
        if let Some(id) = self.world.interner.find(A::NAME) {
            self.include.push(id);
        }
        self
    }
    
    pub fn without<A: TypedAttr>(mut self) -> Self {
        if let Some(id) = self.world.interner.find(A::NAME) {
            self.exclude.push(id);
        }
        self
    }
    
    pub fn filter<F>(mut self, f: F) -> Self
    where
        F: Fn(EntityHandle, &World) -> bool + 'w,
    {
        self.filters.push(Box::new(f));
        self
    }
    
    pub fn execute(self) -> Vec<EntityHandle> {
        let mut result = Vec::new();
        
        for arch_idx in 0..self.world.archetypes.len() {
            let arch = &self.world.archetypes[arch_idx];
            
            let has_all_required = self.include.iter().all(|&attr_id| {
                arch.schema.field_map.contains_key(&attr_id)
            });
            if !has_all_required {
                continue;
            }
            
            let has_excluded = self.exclude.iter().any(|&attr_id| {
                arch.schema.field_map.contains_key(&attr_id)
            });
            if has_excluded {
                continue;
            }
            
            for (_, eid, _) in arch.alive_iter() {
                let handle = EntityHandle(eid);
                
                let passes_filters = self.filters.iter().all(|f| f(handle, self.world));
                if passes_filters {
                    result.push(handle);
                }
            }
        }
        
        result
    }
    
    pub fn count(self) -> usize {
        let mut count = 0;
        
        for arch_idx in 0..self.world.archetypes.len() {
            let arch = &self.world.archetypes[arch_idx];
            
            let has_all_required = self.include.iter().all(|&attr_id| {
                arch.schema.field_map.contains_key(&attr_id)
            });
            if !has_all_required {
                continue;
            }
            
            let has_excluded = self.exclude.iter().any(|&attr_id| {
                arch.schema.field_map.contains_key(&attr_id)
            });
            if has_excluded {
                continue;
            }
            
            for (_, eid, _) in arch.alive_iter() {
                let handle = EntityHandle(eid);
                let passes_filters = self.filters.iter().all(|f| f(handle, self.world));
                if passes_filters {
                    count += 1;
                }
            }
        }
        
        count
    }
    
    pub fn first(self) -> Option<EntityHandle> {
        for arch_idx in 0..self.world.archetypes.len() {
            let arch = &self.world.archetypes[arch_idx];
            
            let has_all_required = self.include.iter().all(|&attr_id| {
                arch.schema.field_map.contains_key(&attr_id)
            });
            if !has_all_required {
                continue;
            }
            
            let has_excluded = self.exclude.iter().any(|&attr_id| {
                arch.schema.field_map.contains_key(&attr_id)
            });
            if has_excluded {
                continue;
            }
            
            for (_, eid, _) in arch.alive_iter() {
                let handle = EntityHandle(eid);
                let passes_filters = self.filters.iter().all(|f| f(handle, self.world));
                if passes_filters {
                    return Some(handle);
                }
            }
        }
        
        None
    }
}