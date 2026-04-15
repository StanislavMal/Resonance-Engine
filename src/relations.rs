// src/relations.rs
//! Relations system for entity hierarchies and associations

use crate::archetype::EntityId;
use std::collections::HashMap;

pub trait Relation: 'static {
    const NAME: &'static str;
}

#[macro_export]
macro_rules! define_relation {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name;
        
        impl $crate::relations::Relation for $name {
            const NAME: &'static str = stringify!($name);
        }
    };
}

#[derive(Clone, Default)]
pub struct RelationGraph {
    forward: HashMap<EntityId, Vec<(u32, EntityId)>>,
    backward: HashMap<EntityId, Vec<(u32, EntityId)>>,
    relation_ids: HashMap<&'static str, u32>,
    next_id: u32,
}

impl RelationGraph {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn add<R: Relation>(&mut self, source: EntityId, target: EntityId) {
        let rel_id = self.get_or_create_relation_id(R::NAME);
        self.forward.entry(source).or_default().push((rel_id, target));
        self.backward.entry(target).or_default().push((rel_id, source));
    }
    
    pub fn get<R: Relation>(&self, source: EntityId) -> Option<EntityId> {
        let rel_id = self.relation_ids.get(R::NAME)?;
        self.forward.get(&source)?
            .iter()
            .find(|(id, _)| id == rel_id)
            .map(|(_, target)| *target)
    }
    
    pub fn get_all<R: Relation>(&self, source: EntityId) -> Vec<EntityId> {
        let rel_id = match self.relation_ids.get(R::NAME) {
            Some(id) => *id,
            None => return Vec::new(),
        };
        
        self.forward.get(&source)
            .map(|rels| {
                rels.iter()
                    .filter(|(id, _)| *id == rel_id)
                    .map(|(_, target)| *target)
                    .collect()
            })
            .unwrap_or_default()
    }
    
    pub fn get_reverse<R: Relation>(&self, target: EntityId) -> Vec<EntityId> {
        let rel_id = match self.relation_ids.get(R::NAME) {
            Some(id) => *id,
            None => return Vec::new(),
        };
        
        self.backward.get(&target)
            .map(|rels| {
                rels.iter()
                    .filter(|(id, _)| *id == rel_id)
                    .map(|(_, source)| *source)
                    .collect()
            })
            .unwrap_or_default()
    }
    
    pub fn remove<R: Relation>(&mut self, source: EntityId, target: EntityId) {
        let rel_id = match self.relation_ids.get(R::NAME) {
            Some(id) => *id,
            None => return,
        };
        
        if let Some(rels) = self.forward.get_mut(&source) {
            rels.retain(|(id, t)| !(*id == rel_id && *t == target));
        }
        if let Some(rels) = self.backward.get_mut(&target) {
            rels.retain(|(id, s)| !(*id == rel_id && *s == source));
        }
    }
    
    pub fn remove_all<R: Relation>(&mut self, source: EntityId) {
        let rel_id = match self.relation_ids.get(R::NAME) {
            Some(id) => *id,
            None => return,
        };
        
        if let Some(rels) = self.forward.get_mut(&source) {
            let targets: Vec<_> = rels.iter()
                .filter(|(id, _)| *id == rel_id)
                .map(|(_, t)| *t)
                .collect();
            
            rels.retain(|(id, _)| *id != rel_id);
            
            for target in targets {
                if let Some(back) = self.backward.get_mut(&target) {
                    back.retain(|(id, s)| !(*id == rel_id && *s == source));
                }
            }
        }
    }
    
    pub fn remove_entity(&mut self, entity: EntityId) {
        if let Some(forward) = self.forward.remove(&entity) {
            for (rel_id, target) in forward {
                if let Some(back) = self.backward.get_mut(&target) {
                    back.retain(|(id, s)| !(*id == rel_id && *s == entity));
                }
            }
        }
        
        if let Some(backward) = self.backward.remove(&entity) {
            for (rel_id, source) in backward {
                if let Some(fwd) = self.forward.get_mut(&source) {
                    fwd.retain(|(id, t)| !(*id == rel_id && *t == entity));
                }
            }
        }
    }
    
    pub fn has<R: Relation>(&self, source: EntityId) -> bool {
        self.get::<R>(source).is_some()
    }
    
    pub fn count<R: Relation>(&self, source: EntityId) -> usize {
        self.get_all::<R>(source).len()
    }
    
    fn get_or_create_relation_id(&mut self, name: &'static str) -> u32 {
        if let Some(&id) = self.relation_ids.get(name) {
            return id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.relation_ids.insert(name, id);
        id
    }
}