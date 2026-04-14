// src/gpu/executor.rs
//! GPU executor for running compute shaders on archetype data

use crate::archetype::ArchetypeId;
use crate::resonator::GpuDispatchConfig;
use wgpu::CommandEncoder;
use std::sync::Arc;

use super::context::GpuContext;
use super::shader::GpuShader;

/// Result of GPU execution
#[derive(Debug, Clone)]
pub struct GpuExecutionResult {
    pub archetype_id: ArchetypeId,
    pub entities_processed: u32,
    pub workgroups_dispatched: (u32, u32, u32),
}

/// Executor for GPU compute tasks
pub struct GpuExecutor {
    pending_dispatches: Vec<PendingDispatch>,
}

struct PendingDispatch {
    archetype_id: ArchetypeId,
    shader: Arc<GpuShader>,
    entity_count: usize,
}

impl GpuExecutor {
    pub fn new() -> Self {
        Self {
            pending_dispatches: Vec::new(),
        }
    }

    /// Queue a compute dispatch for an archetype
    pub fn queue_dispatch(
        &mut self,
        archetype_id: ArchetypeId,
        shader: Arc<GpuShader>,
        entity_count: usize,
    ) {
        self.pending_dispatches.push(PendingDispatch {
            archetype_id,
            shader,
            entity_count,
        });
    }

    /// Execute all queued dispatches using the provided command encoder
    pub fn execute(
        &mut self,
        gpu_ctx: &GpuContext,
        encoder: &mut CommandEncoder,
    ) -> Vec<GpuExecutionResult> {
        let mut results = Vec::with_capacity(self.pending_dispatches.len());

        for dispatch in self.pending_dispatches.drain(..) {
            let ring = match gpu_ctx.get_buffer_ring(dispatch.archetype_id) {
                Some(r) => r,
                None => continue, // Skip if buffer not registered
            };

            let entity_count = dispatch.entity_count.min(ring.capacity);
            let workgroups = dispatch.shader.calculate_workgroups(entity_count);

            // Create compute pass
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(&format!("Compute_{}", dispatch.shader.name)),
                timestamp_writes: None,
            });

            // Set pipeline
            compute_pass.set_pipeline(&dispatch.shader.pipeline);

            // TODO: Set up bind groups for buffer bindings
            // This requires proper bind group layout configuration
            
            // Dispatch workgroups
            compute_pass.dispatch_workgroups(workgroups.0, workgroups.1, workgroups.2);

            results.push(GpuExecutionResult {
                archetype_id: dispatch.archetype_id,
                entities_processed: entity_count as u32,
                workgroups_dispatched: workgroups,
            });
        }

        results
    }

    /// Clear pending dispatches without executing
    pub fn clear(&mut self) {
        self.pending_dispatches.clear();
    }

    /// Get number of pending dispatches
    pub fn pending_count(&self) -> usize {
        self.pending_dispatches.len()
    }
}

/// Extension trait for hybrid CPU/GPU execution
pub trait GpuResonatorExt {
    /// Returns GPU shader info if this resonator can run on GPU
    fn gpu_shader(&self) -> Option<(&'static str, GpuDispatchConfig)> {
        None
    }

    /// Returns true if this resonator should prefer GPU execution
    fn prefers_gpu(&self) -> bool {
        self.gpu_shader().is_some()
    }
}

/// Marker trait for GPU-executable resonators
pub trait ResonatorGpu: Send + Sync {
    /// Get the WGSL shader source for this resonator
    fn shader_source(&self) -> &'static str;
    
    /// Get dispatch configuration
    fn dispatch_config(&self) -> GpuDispatchConfig;
}
