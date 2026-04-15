// src/graph/meta.rs
//! Metadata graph — topology without execution data

use crate::archetype::ArchetypeId;
use crate::entity::EntityHandle;
use crate::graph::{ExecutionPlan, GraphError, SystemHandle};
use crate::resonator::DynResonator;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;  // ← ADD THIS
use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;

/// Node types in the metadata graph
#[derive(Clone)]  // ← REMOVE Debug (Arc<DynResonator> не Debug)
pub enum GraphNode {
    /// Entity node (references archetype slot)
    Entity {
        handle: EntityHandle,
        archetype: ArchetypeId,
    },

    /// Component type node
    Component {
        type_id: TypeId,
        name: &'static str,
    },

    /// System/Resonator node
    System {
        name: String,
        priority: i32,
        archetype: ArchetypeId,
        resonator: Arc<DynResonator>,
        reads: Vec<TypeId>,
        writes: Vec<TypeId>,
    },

    /// Archetype node
    Archetype {
        id: ArchetypeId,
        schema: Vec<TypeId>,
    },
}

// Manual Debug impl (skip resonator field)
impl std::fmt::Debug for GraphNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphNode::Entity { handle, archetype } => f
                .debug_struct("Entity")
                .field("handle", handle)
                .field("archetype", archetype)
                .finish(),
            GraphNode::Component { type_id, name } => f
                .debug_struct("Component")
                .field("type_id", type_id)
                .field("name", name)
                .finish(),
            GraphNode::System {
                name,
                priority,
                archetype,
                reads,
                writes,
                ..
            } => f
                .debug_struct("System")
                .field("name", name)
                .field("priority", priority)
                .field("archetype", archetype)
                .field("reads", reads)
                .field("writes", writes)
                .finish_non_exhaustive(),
            GraphNode::Archetype { id, schema } => f
                .debug_struct("Archetype")
                .field("id", id)
                .field("schema", schema)
                .finish(),
        }
    }
}

/// Edge types (relations)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GraphEdge {
    /// Entity HAS Component
    Has,

    /// Entity IS CHILD OF Entity
    ChildOf,

    /// System READS Component
    Reads,

    /// System WRITES Component
    Writes,

    /// System DEPENDS ON System
    DependsOn,

    /// Custom user-defined relation
    Custom(&'static str),
}

/// Metadata graph (separate from execution!)
#[derive(Clone)]
pub struct MetaGraph {
    /// Petgraph for topology analysis
    graph: DiGraph<GraphNode, GraphEdge>,

    /// Fast lookups
    entity_nodes: HashMap<EntityHandle, NodeIndex>,
    component_nodes: HashMap<TypeId, NodeIndex>,
    system_nodes: HashMap<String, NodeIndex>,

    /// Cached execution plan
    execution_plan: Option<ExecutionPlan>,
    dirty: bool,
}

impl MetaGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            entity_nodes: HashMap::new(),
            component_nodes: HashMap::new(),
            system_nodes: HashMap::new(),
            execution_plan: None,
            dirty: false,
        }
    }

    /// Add entity node (O(1), just metadata)
    pub fn add_entity(&mut self, handle: EntityHandle, archetype: ArchetypeId) -> NodeIndex {
        let node = self.graph.add_node(GraphNode::Entity { handle, archetype });
        self.entity_nodes.insert(handle, node);
        self.dirty = true;
        node
    }

    /// Add system node
    pub fn add_system_node(
        &mut self,
        name: String,
        archetype: ArchetypeId,
        resonator: Arc<DynResonator>,
        reads: Vec<TypeId>,
        writes: Vec<TypeId>,
    ) -> SystemHandle {
        let node = self.graph.add_node(GraphNode::System {
            name: name.clone(),
            priority: 0,
            archetype,
            resonator,
            reads: reads.clone(),
            writes: writes.clone(),
        });

        self.system_nodes.insert(name, node);
        self.dirty = true;

        SystemHandle(node)
    }

    /// Add relation (O(1), just an edge)
    pub fn add_relation(
        &mut self,
        subject: EntityHandle,
        relation: GraphEdge,
        object: EntityHandle,
    ) -> Result<(), GraphError> {
        let from = self
            .entity_nodes
            .get(&subject)
            .ok_or_else(|| GraphError::InvalidTraversal(format!("Entity {:?} not in graph", subject)))?;
        let to = self
            .entity_nodes
            .get(&object)
            .ok_or_else(|| GraphError::InvalidTraversal(format!("Entity {:?} not in graph", object)))?;

        self.graph.add_edge(*from, *to, relation);
        Ok(())
    }

    /// Add dependency between systems
    pub fn add_dependency(&mut self, from: &str, to: &str) -> Result<(), GraphError> {
        let from_node = self
            .system_nodes
            .get(from)
            .ok_or_else(|| GraphError::SystemNotFound(from.to_string()))?;
        let to_node = self
            .system_nodes
            .get(to)
            .ok_or_else(|| GraphError::SystemNotFound(to.to_string()))?;

        self.graph
            .add_edge(*from_node, *to_node, GraphEdge::DependsOn);
        self.dirty = true;
        Ok(())
    }

    /// Query via graph traversal (returns entity handles, not data)
    pub fn query_children(&self, parent: EntityHandle) -> Vec<EntityHandle> {
        let parent_node = match self.entity_nodes.get(&parent) {
            Some(n) => *n,
            None => return Vec::new(),
        };

        self.graph
            .edges(parent_node)
            .filter(|e| *e.weight() == GraphEdge::ChildOf)
            .filter_map(|e| {
                if let GraphNode::Entity { handle, .. } = self.graph[e.target()] {
                    Some(handle)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Check if graph needs recompilation
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark as clean after compilation
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Get cached execution plan
    pub fn execution_plan(&self) -> Option<&ExecutionPlan> {
        self.execution_plan.as_ref()
    }

    /// Set execution plan
    pub fn set_execution_plan(&mut self, plan: ExecutionPlan) {
        self.execution_plan = Some(plan);
        self.dirty = false;
    }

    /// Access internal graph (for compilation)
    pub fn graph(&self) -> &DiGraph<GraphNode, GraphEdge> {
        &self.graph
    }

    /// Get system nodes
    pub fn system_nodes(&self) -> &HashMap<String, NodeIndex> {
        &self.system_nodes
    }

    /// Remove entity from graph
    pub fn remove_entity(&mut self, handle: EntityHandle) {
        if let Some(node) = self.entity_nodes.remove(&handle) {
            self.graph.remove_node(node);
            self.dirty = true;
        }
    }

    /// Export to DOT format for visualization
    pub fn export_dot(&self, path: &str) -> std::io::Result<()> {
        let mut output = String::from("digraph Simulation {\n");
        output.push_str("  rankdir=LR;\n");
        output.push_str("  node [shape=box];\n\n");

        // Nodes
        for node_idx in self.graph.node_indices() {
            let node = &self.graph[node_idx];
            match node {
                GraphNode::Entity { handle, .. } => {
                    output.push_str(&format!(
                        "  E{} [label=\"Entity {}:{}\", color=green];\n",
                        node_idx.index(),
                        handle.0.index,
                        handle.0.generation
                    ));
                }
                GraphNode::System { name, .. } => {
                    output.push_str(&format!(
                        "  S{} [label=\"{}\", shape=ellipse, color=blue];\n",
                        node_idx.index(),
                        name
                    ));
                }
                GraphNode::Component { name, .. } => {
                    output.push_str(&format!(
                        "  C{} [label=\"{}\", shape=diamond, color=orange];\n",
                        node_idx.index(),
                        name
                    ));
                }
                GraphNode::Archetype { id, .. } => {
                    output.push_str(&format!(
                        "  A{} [label=\"Archetype {}\", shape=hexagon, color=purple];\n",
                        node_idx.index(),
                        id.0
                    ));
                }
            }
        }

        output.push_str("\n");

        // Edges
        for edge in self.graph.edge_references() {
            let style = match edge.weight() {
                GraphEdge::Has => "solid",
                GraphEdge::ChildOf => "bold",
                GraphEdge::Reads => "dashed, color=gray",
                GraphEdge::Writes => "solid, color=red",
                GraphEdge::DependsOn => "dotted, color=blue",
                GraphEdge::Custom(name) => {
                    output.push_str(&format!(
                        "  N{} -> N{} [label=\"{}\"];\n",
                        edge.source().index(),
                        edge.target().index(),
                        name
                    ));
                    continue;
                }
            };

            output.push_str(&format!(
                "  N{} -> N{} [style=\"{}\"];\n",
                edge.source().index(),
                edge.target().index(),
                style
            ));
        }

        output.push_str("}\n");
        std::fs::write(path, output)
    }
}

impl Default for MetaGraph {
    fn default() -> Self {
        Self::new()
    }
}