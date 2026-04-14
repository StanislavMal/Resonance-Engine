// src/interning.rs
//! String interner — O(1) lookup by InternedStr

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InternedStr(pub u32);

#[derive(Clone)]
pub struct StringInterner {
    map: HashMap<String, InternedStr>,
    strings: Vec<String>,
}

impl StringInterner {
    pub fn new() -> Self {
        Self { map: HashMap::new(), strings: Vec::new() }
    }

    pub fn intern(&mut self, s: &str) -> InternedStr {
        if let Some(&id) = self.map.get(s) {
            return id;
        }
        let id = InternedStr(self.strings.len() as u32);
        self.strings.push(s.to_string());
        self.map.insert(s.to_string(), id);
        id
    }

    pub fn find(&self, s: &str) -> Option<InternedStr> {
        self.map.get(s).copied()
    }

    pub fn resolve(&self, id: InternedStr) -> &str {
        &self.strings[id.0 as usize]
    }

    pub fn clone_for_read(&self) -> Self {
        self.clone()
    }
}