// src/graph/query.rs
//! Declarative graph queries — hierarchy traversal, pattern matching

use crate::entity::EntityHandle;
use crate::graph::{GraphEdge, GraphError, MetaGraph};
use std::collections::{HashSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraversalMode {
    /// Single hop (direct neighbors)
    OneHop,
    /// Breadth-first recursive traversal
    Recursive,
    /// Depth-first recursive traversal
    RecursiveDFS,
}

pub struct GraphQuery<'w> {
    meta_graph: &'w MetaGraph,
    start: Option<EntityHandle>,
    edges: Vec<GraphEdge>,
    filters: Vec<Box<dyn Fn(EntityHandle) -> bool + 'w>>,
    mode: TraversalMode,
}

impl<'w> GraphQuery<'w> {
    pub fn new(meta_graph: &'w MetaGraph) -> Self {
        Self {
            meta_graph,
            start: None,
            edges: Vec::new(),
            filters: Vec::new(),
            mode: TraversalMode::OneHop,
        }
    }

    pub fn start(mut self, entity: EntityHandle) -> Self {
        self.start = Some(entity);
        self
    }

    pub fn traverse(mut self, edge: GraphEdge, mode: TraversalMode) -> Self {
        self.edges.push(edge);
        self.mode = mode;
        self
    }

    pub fn filter<F>(mut self, f: F) -> Self
    where
        F: Fn(EntityHandle) -> bool + 'w,
    {
        self.filters.push(Box::new(f));
        self
    }

    pub fn execute(self) -> Result<Vec<EntityHandle>, GraphError> {
        let start = self.start.ok_or_else(|| {
            GraphError::InvalidTraversal("No start entity specified".to_string())
        })?;

        match self.mode {
            TraversalMode::OneHop => self.one_hop(start),
            TraversalMode::Recursive => self.bfs(start),
            TraversalMode::RecursiveDFS => self.dfs(start),
        }
    }

    fn one_hop(&self, start: EntityHandle) -> Result<Vec<EntityHandle>, GraphError> {
        // Use public API
        let children = self.meta_graph.query_children(start);
        
        // Filter by edge type and custom filters
        let filtered: Vec<EntityHandle> = children
            .into_iter()
            .filter(|&handle| self.filters.iter().all(|f| f(handle)))
            .collect();
        
        Ok(filtered)
    }

    fn bfs(&self, start: EntityHandle) -> Result<Vec<EntityHandle>, GraphError> {
        // Use public query_children repeatedly for BFS
        let mut result = Vec::new();
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();

        queue.push_back(start);
        visited.insert(start);

        while let Some(current) = queue.pop_front() {
            let children = self.meta_graph.query_children(current);
            
            for child in children {
                if visited.insert(child) {
                    if self.filters.iter().all(|f| f(child)) {
                        result.push(child);
                    }
                    queue.push_back(child);
                }
            }
        }

        Ok(result)
    }

    fn dfs(&self, start: EntityHandle) -> Result<Vec<EntityHandle>, GraphError> {
        let mut result = Vec::new();
        let mut stack = vec![start];
        let mut visited = HashSet::new();

        while let Some(current) = stack.pop() {
            if !visited.insert(current) {
                continue;
            }

            if current != start {
                if self.filters.iter().all(|f| f(current)) {
                    result.push(current);
                }
            }

            let children = self.meta_graph.query_children(current);
            for child in children {
                stack.push(child);
            }
        }

        Ok(result)
    }
}