// src/gpu/shader.rs
//! GPU shader management and dispatch configuration

use crate::resonator::GpuDispatchConfig;
pub use crate::resonator::GpuDispatchConfig as PublicGpuDispatchConfig;
use wgpu::{ComputePipeline, Device, ShaderModule};
use std::collections::HashMap;
use std::sync::Arc;

/// Compiled GPU shader with pipeline
pub struct GpuShader {
    pub name: String,
    pub module: ShaderModule,
    pub pipeline: Arc<ComputePipeline>,
    pub config: GpuDispatchConfig,
}

impl GpuShader {
    /// Create a new GPU shader from WGSL source
    pub fn new(
        device: &Device,
        name: &str,
        wgsl_source: &str,
        config: GpuDispatchConfig,
    ) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&format!("Shader_{}", name)),
            source: wgpu::ShaderSource::Wgsl(wgsl_source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("PipelineLayout_{}", name)),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(&format!("Pipeline_{}", name)),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some(config.entry_point),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            name: name.to_string(),
            module,
            pipeline: Arc::new(pipeline),
            config,
        }
    }

    /// Calculate number of workgroups needed for given entity count
    pub fn calculate_workgroups(&self, entity_count: usize) -> (u32, u32, u32) {
        let (wx, wy, wz) = self.config.workgroup_size;
        let total_threads = wx * wy * wz;
        let workgroups = ((entity_count as u32) + total_threads - 1) / total_threads;
        (workgroups, 1, 1)
    }
}

/// Built-in shaders provided by the engine
pub mod builtins {

    /// Particle physics update shader
    pub const PARTICLE_PHYSICS_WGSL: &str = r#"
@group(0) @binding(0)
var<storage, read> positions_in: array<f32>;

@group(0) @binding(1)
var<storage, read_write> positions_out: array<f32>;

@group(0) @binding(2)
var<storage, read> velocities: array<f32>;

@group(0) @binding(3)
var<uniform> params: vec2<f32>; // dt, gravity

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let idx = id.x;
    if (idx >= arrayLength(&positions_in)) {
        return;
    }

    let pos = positions_in[idx];
    let vel = velocities[idx];
    let dt = params.x;
    let gravity = params.y;

    // Simple Euler integration
    let new_pos = pos + vel * dt;
    let new_vel = vel + vec2<f32>(0.0, -gravity) * dt;

    positions_out[idx] = new_pos;
}
"#;

    /// Generic attribute transform shader
    pub const ATTRIBUTE_TRANSFORM_WGSL: &str = r#"
@group(0) @binding(0)
var<storage, read> input_data: array<f32>;

@group(0) @binding(1)
var<storage, read_write> output_data: array<f32>;

@group(0) @binding(2)
var<uniform> transform_params: array<f32>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let idx = id.x;
    if (idx >= arrayLength(&input_data)) {
        return;
    }

    // Generic transform - actual operation depends on bound data
    output_data[idx] = input_data[idx] * transform_params[0] + transform_params[1];
}
"#;
}

/// Shader registry for managing compiled shaders
pub struct ShaderRegistry {
    shaders: HashMap<String, Arc<GpuShader>>,
}

impl ShaderRegistry {
    pub fn new() -> Self {
        Self {
            shaders: HashMap::new(),
        }
    }

    /// Get or compile a shader
    pub fn get_or_compile(
        &mut self,
        device: &Device,
        name: &str,
        wgsl_source: &str,
        config: GpuDispatchConfig,
    ) -> Arc<GpuShader> {
        if let Some(shader) = self.shaders.get(name) {
            return Arc::clone(shader);
        }

        let shader = Arc::new(GpuShader::new(device, name, wgsl_source, config));
        self.shaders.insert(name.to_string(), Arc::clone(&shader));
        shader
    }

    /// Get a compiled shader by name
    pub fn get(&self, name: &str) -> Option<Arc<GpuShader>> {
        self.shaders.get(name).map(Arc::clone)
    }
}
