// src/lib.rs
//! Resonance Engine v11.1 — Graph metadata layer
//!
//! ## New in v11.1
//! - Graph-based system dependencies (auto-parallelism)
//! - Hierarchical entity queries (BFS/DFS)
//! - Execution plan compilation (cycle detection)
//! - DOT export for visualization
//!
//! ## Architecture
//! ```text
//! Graph Layer (metadata):
//!   - Entities as nodes
//!   - Relations as edges
//!   - Systems with dependencies
//!   ↓ Compile to ExecutionPlan
//! SoA Layer (runtime):
//!   - Unchanged performance (114M entities/sec)
//!   - Uses cached plan (0% overhead)
//! ```

pub mod accessor;
pub mod archetype;
pub mod context;
pub mod entity;
pub mod events;
pub mod graph;  // NEW
pub mod interning;
pub mod metrics;
pub mod prefab;
pub mod profiler;
pub mod query;
pub mod relations;
pub mod resonator;
pub mod scheduler;
pub mod serialization;
pub mod spatial;
pub mod storage;
pub mod typed_attrs;
pub mod world;

#[cfg(feature = "hot-reload")]
pub mod hot_reload;

// ─── Core Re-exports ─────────────────────────────
pub use accessor::{read_at, write_at, AttrOffset, EntityAccessor, EntityRef};
pub use archetype::EntityId;
pub use context::{CommandBuffer, NodeContext, SpawnRequest, WriteRequest};
pub use entity::EntityHandle;
pub use profiler::Profiler;
pub use relations::{Relation, RelationGraph};
pub use resonator::{BoundField, DynResonator, FieldMap, Resonator, ResonatorFactory, ResonatorExt};
pub use serialization::{EntitySnapshot, SchemaSnapshot};
pub use storage::{BufferMode, FieldIndex};
pub use typed_attrs::{EnumAttr, TypedAttr};
pub use world::{BuildWarning, World};

// ─── Graph Re-exports (NEW) ───────────────────────
pub use graph::{
    ExecutionPhase, ExecutionPlan, GraphEdge, GraphError, GraphNode, GraphQuery, MetaGraph,
    SystemHandle, TraversalMode,
};

// ─── Extension Traits ─────────────────────────────
pub use prefab::WorldPrefabExt;
pub use spatial::WorldSpatialExt;

// ─── Spatial types ────────────────────────────────
pub use spatial::{SpatialConfig, SpatialEntry, SpatialGrid};

// ─── Metrics ──────────────────────────────────────
pub use metrics::{collect_benchmark, BenchmarkReport};

// ─── Query ────────────────────────────────────────
pub use query::QueryBuilder;

// ─── Hot Reload ───────────────────────────────────
#[cfg(feature = "hot-reload")]
pub use hot_reload::HotReloadSystem;