// src/serialization.rs
//! World serialization/deserialization

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SchemaSnapshot {
    pub version: u64,
    pub entities: Vec<EntitySnapshot>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EntitySnapshot {
    pub id: u32,
    pub generation: u32,
    pub archetype_name: String,
    pub attributes: HashMap<String, f64>,
}

impl SchemaSnapshot {
    pub fn new(version: u64) -> Self {
        Self {
            version,
            entities: Vec::new(),
        }
    }
    
    pub fn add_entity(&mut self, snapshot: EntitySnapshot) {
        self.entities.push(snapshot);
    }
    
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }
    
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
    
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
    
    pub fn save(&self, path: &str) -> Result<(), std::io::Error> {
        let json = self.to_json()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }
    
    pub fn load(path: &str) -> Result<Self, std::io::Error> {
        let json = std::fs::read_to_string(path)?;
        Self::from_json(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

impl EntitySnapshot {
    pub fn new(id: u32, generation: u32, archetype_name: String) -> Self {
        Self {
            id,
            generation,
            archetype_name,
            attributes: HashMap::new(),
        }
    }
    
    pub fn add_attribute(&mut self, name: String, value: f64) {
        self.attributes.insert(name, value);
    }
    
    pub fn get_attribute(&self, name: &str) -> Option<f64> {
        self.attributes.get(name).copied()
    }
}