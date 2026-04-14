// src/gpu/mod.rs
//! GPU compute backend using wgpu
//!
//! Provides GpuContext for managing GPU resources and executing compute shaders.
//! Supports triple-buffering for overlap between CPU and GPU execution.

pub mod context;
pub mod buffer;
pub mod shader;
pub mod executor;

pub use context::GpuContext;
pub use buffer::{GpuArchetypeBuffer, GpuBufferRing};
pub use shader::{GpuShader, PublicGpuDispatchConfig as GpuDispatchConfig};
pub use executor::GpuExecutor;
