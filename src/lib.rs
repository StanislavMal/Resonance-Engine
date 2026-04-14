// src/lib.rs
//! Resonance Engine v10.3 — High-performance simulation framework

pub mod accessor;
pub mod archetype;
pub mod context;
pub mod entity;
pub mod events;
pub mod interning;
pub mod metrics;
pub mod prefab;
pub mod query;
pub mod resonator;
pub mod scheduler;
pub mod spatial;
pub mod storage;
pub mod typed_attrs;
pub mod world;

// ─── Core Re-exports ─────────────────────────────
pub use accessor::{read_at, write_at, AttrOffset, EntityAccessor, EntityRef};
pub use archetype::EntityId;
pub use context::{CommandBuffer, NodeContext, SpawnRequest, WriteRequest};
pub use entity::EntityHandle;
pub use resonator::{BoundField, DynResonator, FieldMap, Resonator, ResonatorExt};
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