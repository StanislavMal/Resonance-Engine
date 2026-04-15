// src/metrics.rs
//! Enhanced metrics collection

use crate::world::World;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct TickMetrics {
    pub total: Duration,
    pub resonator_execution: Duration,
    pub commit: Duration,
    pub despawn_processing: Duration,
    pub total_calls: u64,
    pub entities_processed: u64,
    pub dirty_count: u64,
    pub despawns: u64,
}

#[derive(Debug, Clone, Default)]
pub struct BenchmarkReport {
    pub ticks: Vec<TickMetrics>,
    pub archetype_info: Vec<crate::world::ArchetypeInfo>,
    pub alive_count: usize,
    pub memory_bytes: usize,
    pub archetype_count: usize,
}

impl BenchmarkReport {
    pub fn print(&self) {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║            RESONANCE ENGINE v11.0 — METRICS REPORT                 ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        if !self.ticks.is_empty() {
            let n = self.ticks.len();
            let totals: Vec<Duration> = self.ticks.iter().map(|t| t.total).collect();
            let avg = totals.iter().sum::<Duration>() / n as u32;
            let mut sorted = totals.clone();
            sorted.sort();
            let median = sorted[n / 2];
            let p95 = sorted[(n as f64 * 0.95) as usize];
            let p99 = sorted[((n as f64 * 0.99) as usize).min(n - 1)];

            println!("\n  ── TICK TIMING ({} ticks) ──", n);
            println!(
                "  Avg: {:>10} | Median: {:>10} | P95: {:>10} | P99: {:>10}",
                fmt(avg),
                fmt(median),
                fmt(p95),
                fmt(p99)
            );
            println!(
                "  Min: {:>10} | Max: {:>10}",
                fmt(sorted[0]),
                fmt(sorted[n - 1])
            );

            let avg_res: Duration = self
                .ticks
                .iter()
                .map(|t| t.resonator_execution)
                .sum::<Duration>()
                / n as u32;
            let avg_commit: Duration =
                self.ticks.iter().map(|t| t.commit).sum::<Duration>() / n as u32;
            let avg_despawn: Duration = self
                .ticks
                .iter()
                .map(|t| t.despawn_processing)
                .sum::<Duration>()
                / n as u32;
            let total_avg = avg.as_secs_f64();

            if total_avg > 0.0 {
                println!("\n  Phase Breakdown:");
                println!(
                    "    resonator_exec:  {:>10} ({:.1}%)",
                    fmt(avg_res),
                    avg_res.as_secs_f64() / total_avg * 100.0
                );
                println!(
                    "    commit:          {:>10} ({:.1}%)",
                    fmt(avg_commit),
                    avg_commit.as_secs_f64() / total_avg * 100.0
                );
                println!(
                    "    despawn:         {:>10} ({:.1}%)",
                    fmt(avg_despawn),
                    avg_despawn.as_secs_f64() / total_avg * 100.0
                );
            }

            let total_calls: u64 = self.ticks.iter().map(|t| t.total_calls).sum();
            let total_time: Duration = self.ticks.iter().map(|t| t.total).sum();
            if total_time.as_secs_f64() > 0.0 {
                println!(
                    "\n  Total calls: {} | Calls/sec: {:.0}",
                    total_calls,
                    total_calls as f64 / total_time.as_secs_f64()
                );
                println!(
                    "  Entities/sec: {:.0}",
                    self.alive_count as f64 / avg.as_secs_f64()
                );
            }
        }

        println!("\n  ── ARCHETYPES ({}) ──", self.archetype_count);
        for info in &self.archetype_info {
            println!(
                "    {:<20} {:>7} alive | {} floats/ent | {} resonators",
                info.name, info.alive, info.floats_per_entity, info.resonator_count
            );
        }

        println!("\n  ── MEMORY ──");
        println!(
            "  Total: {} | {}/entity | {} archetypes",
            fmt_bytes(self.memory_bytes),
            fmt_bytes(if self.alive_count > 0 {
                self.memory_bytes / self.alive_count
            } else {
                0
            }),
            self.archetype_count
        );

        println!("\n{}", "═".repeat(70));
    }
}

pub fn collect_benchmark(world: &mut World, warmup: usize, measure: usize) -> BenchmarkReport {
    for _ in 0..warmup {
        world.tick();
    }

    let mut ticks = Vec::with_capacity(measure);

    for _ in 0..measure {
        let tick_start = Instant::now();
        let result = world.tick();
        let total_time = tick_start.elapsed();

        ticks.push(TickMetrics {
            total: total_time,
            resonator_execution: total_time,
            commit: Duration::ZERO,
            despawn_processing: Duration::ZERO,
            total_calls: result.total_calls,
            entities_processed: result.entities_processed,
            dirty_count: result.dirty_count,
            despawns: result.despawn_requests,
        });
    }

    BenchmarkReport {
        ticks,
        archetype_info: world.archetype_info(),
        alive_count: world.alive_count(),
        memory_bytes: world.memory_bytes(),
        archetype_count: world.archetype_count(),
    }
}

fn fmt(d: Duration) -> String {
    if d.as_millis() > 0 {
        format!("{:.2}ms", d.as_secs_f64() * 1000.0)
    } else if d.as_micros() > 0 {
        format!("{:.1}µs", d.as_secs_f64() * 1_000_000.0)
    } else {
        format!("{}ns", d.as_nanos())
    }
}

fn fmt_bytes(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / 1048576.0)
    } else if bytes >= 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}