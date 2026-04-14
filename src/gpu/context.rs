// src/gpu/context.rs
//! GpuContext - manages wgpu Device, Queue and archetype buffers

use crate::archetype::ArchetypeId;
use crate::world::World;
use crate::storage::FloatOffset;
use wgpu::{CommandEncoder, Device, Queue};
use std::sync::Arc;

use super::buffer::GpuBufferRing;

/// GPU context holding device, queue and managed buffers
pub struct GpuContext {
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    /// Triple-buffered storage for each archetype
    pub buffer_rings: Vec<GpuBufferRing>,
    /// Mapping from ArchetypeId to buffer ring index
    archetype_to_ring: std::collections::HashMap<ArchetypeId, usize>,
}

impl GpuContext {
    /// Initialize GPU context asynchronously
    pub async fn new() -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("Failed to find a suitable GPU adapter");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    label: Some("Resonance Engine Device"),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .expect("Failed to create wgpu device");

        Self {
            device: Arc::new(device),
            queue: Arc::new(queue),
            buffer_rings: Vec::new(),
            archetype_to_ring: std::collections::HashMap::new(),
        }
    }

    /// Register or update a buffer ring for an archetype
    pub fn register_archetype(
        &mut self,
        arch_id: ArchetypeId,
        floats_per_entity: usize,
        initial_capacity: usize,
    ) {
        if let Some(&ring_idx) = self.archetype_to_ring.get(&arch_id) {
            // Update existing ring capacity if needed
            if self.buffer_rings[ring_idx].capacity < initial_capacity {
                self.buffer_rings[ring_idx].resize(
                    &self.device,
                    initial_capacity,
                    floats_per_entity,
                );
            }
        } else {
            let ring = GpuBufferRing::new(
                &self.device,
                initial_capacity,
                floats_per_entity,
                arch_id,
            );
            let ring_idx = self.buffer_rings.len();
            self.buffer_rings.push(ring);
            self.archetype_to_ring.insert(arch_id, ring_idx);
        }
    }

    /// Sync CPU archetype data to GPU staging buffer
    pub fn sync_archetype(&mut self, world: &mut World, arch_idx: usize) {
        let archetype = &world.archetypes[arch_idx];
        let arch_id = archetype.id;
        
        let ring_idx = match self.archetype_to_ring.get(&arch_id) {
            Some(&idx) => idx,
            None => return, // Not registered for GPU
        };

        let ring = &mut self.buffer_rings[ring_idx];
        if !ring.dirty {
            return; // No sync needed
        }

        // Get CPU data from world storage
        let floats_per_entity = archetype.schema.floats_per_entity as usize;
        let entity_count = archetype.alive_count();
        
        // Collect float offsets for alive entities
        let offsets_data: Vec<FloatOffset> = archetype.alive_iter().map(|(_, _, off)| off).collect();
        
        // Read from staging slot and upload to GPU
        ring.upload_from_cpu(&self.device, &self.queue, |staging_slice| {
            let mut offset = 0usize;
            for &float_offset in &offsets_data {
                for field_idx in 0..floats_per_entity {
                    let abs_offset = float_offset.0 as usize + field_idx;
                    if abs_offset < world.storage.float_count() {
                        staging_slice[offset] = world.storage.read_abs(abs_offset);
                    }
                    offset += 1;
                }
            }
        });

        ring.dirty = false;
    }

    /// Create a command encoder for GPU work
    pub fn create_command_encoder(&self) -> CommandEncoder {
        self.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Resonance Compute Encoder"),
            })
    }

    /// Submit commands to GPU queue
    pub fn submit_commands(&self, encoder: CommandEncoder) {
        let command_buffer = encoder.finish();
        self.queue.submit(Some(command_buffer));
    }

    /// Get buffer ring for an archetype
    pub fn get_buffer_ring(&self, arch_id: ArchetypeId) -> Option<&GpuBufferRing> {
        self.archetype_to_ring
            .get(&arch_id)
            .map(|&idx| &self.buffer_rings[idx])
    }

    /// Mark an archetype's GPU buffer as dirty (needs sync)
    pub fn mark_dirty(&mut self, arch_id: ArchetypeId) {
        if let Some(&ring_idx) = self.archetype_to_ring.get(&arch_id) {
            self.buffer_rings[ring_idx].dirty = true;
        }
    }
}
