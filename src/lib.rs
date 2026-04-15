// src/lib.rs
//! Resonance Engine v11.0 — High-performance deterministic simulation framework
//!
//! ## Key Features
//! - Deterministic execution with double-buffering
//! - Relations system for entity hierarchies
//! - Full serialization support
//! - Hot reload capability (struct-based resonators)
//! - Built-in profiling and spatial queries
//!
//! ## Quick Start
//! ```no_run
//! use resonance_engine::*;
//!
//! define_attrs!(PosX, VelX);
//!
//! struct Physics {
//!     px: BoundField<PosX>,
//!     vx: BoundField<VelX>,
//! }
//!
//! impl Resonator for Physics {
//!     fn apply(&self, ctx: &mut NodeContext) {
//!         let new_x = self.px.get(ctx) + self.vx.get(ctx);
//!         self.px.set_unchecked(ctx, new_x);
//!     }
//! }
//!
//! impl ResonatorFactory for Physics {
//!     fn create(map: &FieldMap) -> Self {
//!         Self {
//!             px: map.bind::<PosX>(),
//!             vx: map.bind::<VelX>(),
//!         }
//!     }
//! }
//!
//! fn main() {
//!     let mut world = World::new();
//!     world.register_resonator::<Physics>("Particle");
//!     
//!     for _ in 0..1000 {
//!         world.entity("Particle")
//!             .attr_typed::<PosX>(0.0)
//!             .attr_typed::<VelX>(1.0)
//!             .done();
//!     }
//!     
//!     world.build();
//!     
//!     for _ in 0..60 {
//!         world.tick();
//!     }
//! }
//! ```

pub mod accessor;
pub mod archetype;
pub mod context;
pub mod entity;
pub mod events;
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