// src/graph/mod.rs
//! Graph metadata layer — zero runtime overhead

pub mod meta;
pub mod query;
pub mod plan;

pub use meta::{GraphEdge, GraphNode, MetaGraph};
pub use plan::{ExecutionPhase, ExecutionPlan};
pub use query::{GraphQuery, TraversalMode};

/// Handle to a system node in the graph
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SystemHandle(pub petgraph::graph::NodeIndex);

/// Error types for graph operations
#[derive(Debug, Clone)]
pub enum GraphError {
    CyclicDependency(Vec<String>),
    SystemNotFound(String),
    ConflictingWrites { system_a: String, system_b: String, component: String },
    InvalidTraversal(String),
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::CyclicDependency(cycle) => {
                write!(f, "Dependency cycle detected: {}", cycle.join(" → "))
            }
            GraphError::SystemNotFound(name) => write!(f, "System '{}' not found", name),
            GraphError::ConflictingWrites { system_a, system_b, component } => {
                write!(
                    f,
                    "Systems '{}' and '{}' both write component '{}' without dependency",
                    system_a, system_b, component
                )
            }
            GraphError::InvalidTraversal(msg) => write!(f, "Invalid traversal: {}", msg),
        }
    }
}

impl std::error::Error for GraphError {}