// src/gpu/buffer.rs
//! GPU buffer management with triple-buffering for CPU/GPU overlap

use crate::archetype::ArchetypeId;
use wgpu::{Buffer, BufferAddress, Device, Queue};

/// Single GPU buffer for archetype data
#[derive(Debug)]
pub struct GpuArchetypeBuffer {
    pub archetype_id: ArchetypeId,
    pub buffer: Buffer,
    pub capacity: usize,
    pub floats_per_entity: usize,
    /// Label for debugging
    pub label: String,
}

impl GpuArchetypeBuffer {
    /// Create a new GPU buffer for archetype data
    pub fn new(
        device: &Device,
        capacity: usize,
        floats_per_entity: usize,
        arch_id: ArchetypeId,
    ) -> Self {
        let size = (capacity * floats_per_entity * std::mem::size_of::<f64>()) as BufferAddress;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("ArchetypeBuffer_{}", arch_id.0)),
            size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        Self {
            archetype_id: arch_id,
            buffer,
            capacity,
            floats_per_entity,
            label: format!("ArchetypeBuffer_{}", arch_id.0),
        }
    }

    /// Resize the buffer to accommodate more entities
    pub fn resize(&mut self, device: &Device, new_capacity: usize) {
        if new_capacity <= self.capacity {
            return;
        }

        let size = (new_capacity * self.floats_per_entity * std::mem::size_of::<f64>()) as BufferAddress;
        self.buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&self.label),
            size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        self.capacity = new_capacity;
    }
}

/// Triple-buffered ring for CPU/GPU overlap
/// 
/// Slot 0: Staging (CPU writes here)
/// Slot 1: Read (GPU reads from here during compute)
/// Slot 2: Write (GPU writes results here)
pub struct GpuBufferRing {
    pub archetype_id: ArchetypeId,
    pub floats_per_entity: usize,
    pub capacity: usize,
    /// Three buffers for triple buffering
    pub buffers: [GpuArchetypeBuffer; 3],
    /// Current slot indices for each role
    pub staging_slot: usize,
    pub read_slot: usize,
    pub write_slot: usize,
    /// Flag indicating data needs sync from CPU
    pub dirty: bool,
    /// Frame counter for managing buffer rotation
    pub frame: u64,
}

impl GpuBufferRing {
    /// Create a new triple-buffered ring
    pub fn new(
        device: &Device,
        capacity: usize,
        floats_per_entity: usize,
        arch_id: ArchetypeId,
    ) -> Self {
        let mut buffers = Vec::with_capacity(3);
        for i in 0..3 {
            let mut buf = GpuArchetypeBuffer::new(device, capacity, floats_per_entity, arch_id);
            buf.label = format!("ArchetypeBuffer_{}_slot{}", arch_id.0, i);
            buffers.push(buf);
        }

        let [b0, b1, b2] = buffers.try_into().unwrap();
        
        Self {
            archetype_id: arch_id,
            floats_per_entity,
            capacity,
            buffers: [b0, b1, b2],
            staging_slot: 0,
            read_slot: 1,
            write_slot: 2,
            dirty: false,
            frame: 0,
        }
    }

    /// Resize all buffers in the ring
    pub fn resize(&mut self, device: &Device, new_capacity: usize, floats_per_entity: usize) {
        self.floats_per_entity = floats_per_entity;
        for buf in &mut self.buffers {
            buf.resize(device, new_capacity);
        }
        self.capacity = new_capacity;
    }

    /// Get the current staging buffer for CPU writes
    pub fn staging_buffer(&self) -> &GpuArchetypeBuffer {
        &self.buffers[self.staging_slot]
    }

    /// Get the current read buffer for GPU compute
    pub fn read_buffer(&self) -> &GpuArchetypeBuffer {
        &self.buffers[self.read_slot]
    }

    /// Get the current write buffer for GPU output
    pub fn write_buffer(&self) -> &GpuArchetypeBuffer {
        &self.buffers[self.write_slot]
    }

    /// Upload data from CPU to staging buffer
    pub fn upload_from_cpu<F>(&mut self, device: &Device, queue: &Queue, fill_data: F)
    where
        F: FnOnce(&mut [f64]),
    {
        let staging_buf = &self.buffers[self.staging_slot];
        let data_size = self.capacity * self.floats_per_entity * std::mem::size_of::<f64>();
        
        // Create temporary staging buffer for mapping
        let temp_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("TempStagingBuffer"),
            size: data_size as BufferAddress,
            usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: true,
        });

        {
            let mut slice = temp_buffer.slice(..).get_mapped_range_mut();
            let f64_slice: &mut [f64] = bytemuck::cast_slice_mut(&mut *slice);
            fill_data(f64_slice);
        }

        temp_buffer.unmap();

        // Copy from temp buffer to staging buffer
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("UploadEncoder"),
        });
        encoder.copy_buffer_to_buffer(
            &temp_buffer,
            0,
            &staging_buf.buffer,
            0,
            data_size as BufferAddress,
        );
        queue.submit(Some(encoder.finish()));
    }

    /// Rotate buffers for next frame (called after GPU work completes)
    pub fn rotate(&mut self) {
        self.frame += 1;
        // Rotate: write becomes read, read becomes staging, staging becomes write
        let old_write = self.write_slot;
        self.write_slot = self.staging_slot;
        self.staging_slot = self.read_slot;
        self.read_slot = old_write;
    }

    /// Get buffer binding info for shader use
    pub fn get_binding(&self, slot_type: BufferSlotType) -> (&Buffer, BufferAddress) {
        let buf = match slot_type {
            BufferSlotType::Staging => &self.buffers[self.staging_slot],
            BufferSlotType::Read => &self.buffers[self.read_slot],
            BufferSlotType::Write => &self.buffers[self.write_slot],
        };
        let size = (self.capacity * self.floats_per_entity * std::mem::size_of::<f64>()) as BufferAddress;
        (&buf.buffer, size)
    }
}

/// Type of buffer slot for binding
#[derive(Clone, Copy, Debug)]
pub enum BufferSlotType {
    Staging,
    Read,
    Write,
}
