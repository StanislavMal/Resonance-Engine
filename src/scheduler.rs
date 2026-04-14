// src/scheduler.rs
//! Phase-based scheduler with parallel archetype execution

use crate::archetype::{Archetype, EntityId};
use crate::context::{CommandBuffer, NodeContext};
use crate::resonator::DynResonator;
use crate::storage::{FloatOffset, StoragePtr};
use rayon::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Result of one tick
#[derive(Debug, Clone, Copy, Default)]
pub struct TickResult {
    pub total_calls: u64,
    pub entities_processed: u64,
    pub dirty_count: u64,
    pub despawn_requests: u64,
}

/// Result of hybrid CPU/GPU tick
#[derive(Debug, Clone, Default)]
pub struct HybridTickResult {
    pub cpu_result: TickResult,
    pub gpu_commands_submitted: usize,
    pub gpu_entities_processed: u64,
}

/// Scheduler config
pub struct SchedulerConfig {
    pub epsilon: f64,
    pub parallel_threshold: usize,
    pub min_chunk_size: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            epsilon: 1e-9,
            parallel_threshold: 1000,
            min_chunk_size: 64,
        }
    }
}

/// Result of batch execution for one archetype
pub struct ArchetypeBatchResult {
    pub calls: u64,
    pub processed: u64,
    pub dirty: u64,
    pub despawn_list: Vec<EntityId>,
    pub commands: CommandBuffer,
}

/// Execute all resonators of one archetype — batch
pub fn execute_archetype(
    archetype: &Archetype,
    sp: StoragePtr,
    config: &SchedulerConfig,
) -> ArchetypeBatchResult {
    let batch = archetype.collect_alive_batch();
    let resonators = &archetype.resonators;

    if resonators.is_empty() || batch.is_empty() {
        return ArchetypeBatchResult {
            calls: 0,
            processed: 0,
            dirty: 0,
            despawn_list: Vec::new(),
            commands: CommandBuffer::new(),
        };
    }

    if batch.len() >= config.parallel_threshold {
        execute_parallel(&batch, resonators, sp, config)
    } else {
        execute_sequential(&batch, resonators, sp, config)
    }
}

fn execute_sequential(
    batch: &[(FloatOffset, EntityId)],
    resonators: &[Arc<DynResonator>],
    sp: StoragePtr,
    config: &SchedulerConfig,
) -> ArchetypeBatchResult {
    let mut calls = 0u64;
    let mut dirty = 0u64;
    let mut despawn_list = Vec::new();
    let mut commands = CommandBuffer::new();

    for &(offset, entity_id) in batch {
        let mut ctx = NodeContext::new(sp, offset, entity_id, config.epsilon);

        for res in resonators {
            res.apply(&mut ctx);
            calls += 1;
        }

        if ctx.dirty {
            dirty += 1;
        }
        if ctx.wants_despawn() {
            despawn_list.push(entity_id);
        }
        commands.merge(ctx.take_commands());
    }

    ArchetypeBatchResult {
        calls,
        processed: batch.len() as u64,
        dirty,
        despawn_list,
        commands,
    }
}

fn execute_parallel(
    batch: &[(FloatOffset, EntityId)],
    resonators: &[Arc<DynResonator>],
    sp: StoragePtr,
    config: &SchedulerConfig,
) -> ArchetypeBatchResult {
    let total_calls = AtomicU64::new(0);
    let total_dirty = AtomicU64::new(0);

    let chunk_size = config
        .min_chunk_size
        .max(batch.len() / rayon::current_num_threads());

    let chunk_results: Vec<(Vec<EntityId>, CommandBuffer)> = batch
        .par_chunks(chunk_size)
        .map(|chunk| {
            let mut local_calls = 0u64;
            let mut local_dirty = 0u64;
            let mut local_despawns = Vec::new();
            let mut local_commands = CommandBuffer::new();

            for &(offset, entity_id) in chunk {
                let mut ctx = NodeContext::new(sp, offset, entity_id, config.epsilon);

                for res in resonators {
                    res.apply(&mut ctx);
                    local_calls += 1;
                }

                if ctx.dirty {
                    local_dirty += 1;
                }
                if ctx.wants_despawn() {
                    local_despawns.push(entity_id);
                }
                local_commands.merge(ctx.take_commands());
            }

            total_calls.fetch_add(local_calls, Ordering::Relaxed);
            total_dirty.fetch_add(local_dirty, Ordering::Relaxed);
            (local_despawns, local_commands)
        })
        .collect();

    let mut despawn_list = Vec::new();
    let mut commands = CommandBuffer::new();
    for (chunk_despawns, chunk_commands) in chunk_results {
        despawn_list.extend(chunk_despawns);
        commands.merge(chunk_commands);
    }

    ArchetypeBatchResult {
        calls: total_calls.load(Ordering::Relaxed),
        processed: batch.len() as u64,
        dirty: total_dirty.load(Ordering::Relaxed),
        despawn_list,
        commands,
    }
}