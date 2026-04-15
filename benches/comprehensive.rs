// benches/comprehensive.rs
//! Comprehensive benchmark suite for Resonance Engine v11.0
//!
//! Run with: cargo bench --bench comprehensive

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use resonance_engine::*;
use std::hint::black_box as bb;

// ═══════════════════════════════════════════════════════════════
//  ATTRIBUTES
// ═══════════════════════════════════════════════════════════════

define_attrs!(PosX, PosY, PosZ, VelX, VelY, VelZ);
define_attrs!(Health, Energy, Damage);
define_tags!(Enemy, Player);

// ═══════════════════════════════════════════════════════════════
//  RESONATORS
// ═══════════════════════════════════════════════════════════════

define_resonator!(
    Physics2D {
        px: PosX,
        py: PosY,
        vx: VelX,
        vy: VelY
    }
    |this, ctx| {
        this.px.add_unchecked(ctx, this.vx.get(ctx));
        this.py.add_unchecked(ctx, this.vy.get(ctx));
    }
);

define_resonator!(
    Physics3D {
        px: PosX,
        py: PosY,
        pz: PosZ,
        vx: VelX,
        vy: VelY,
        vz: VelZ
    }
    |this, ctx| {
        this.px.add_unchecked(ctx, this.vx.get(ctx));
        this.py.add_unchecked(ctx, this.vy.get(ctx));
        this.pz.add_unchecked(ctx, this.vz.get(ctx));
    }
);

define_resonator!(
    EnergyDrain {
        energy: Energy
    }
    |this, ctx| {
        this.energy.add_unchecked(ctx, -0.1);
        if this.energy.get(ctx) <= 0.0 {
            ctx.despawn_self();
        }
    }
);

// ═══════════════════════════════════════════════════════════════
//  BENCHMARK 1: Simple Iteration
// ═══════════════════════════════════════════════════════════════

fn bench_simple_iteration(c: &mut Criterion) {
    let mut group = c.benchmark_group("iteration/simple_2d");
    group.sample_size(50);

    for &count in &[1_000, 10_000, 50_000, 100_000, 500_000] {
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &count| {
            let mut world = World::new();
            world.register_resonator::<Physics2D>("Physics2D");

            for _ in 0..count {
                world
                    .entity("Particle")
                    .attr_typed::<PosX>(0.0)
                    .attr_typed::<PosY>(0.0)
                    .attr_typed::<VelX>(0.01)
                    .attr_typed::<VelY>(0.02)
                    .resonator_type("Physics2D")
                    .done();
            }

            world.build();

            b.iter(|| {
                world.tick();
            });
        });
    }

    group.finish();
}

// ═══════════════════════════════════════════════════════════════
//  BENCHMARK 2: 3D Physics
// ═══════════════════════════════════════════════════════════════

fn bench_physics_3d(c: &mut Criterion) {
    let mut group = c.benchmark_group("iteration/physics_3d");
    group.sample_size(50);

    for &count in &[1_000, 10_000, 50_000, 100_000] {
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &count| {
            let mut world = World::new();
            world.register_resonator::<Physics3D>("Physics3D");

            for _ in 0..count {
                world
                    .entity("Particle")
                    .attr_typed::<PosX>(0.0)
                    .attr_typed::<PosY>(0.0)
                    .attr_typed::<PosZ>(0.0)
                    .attr_typed::<VelX>(0.01)
                    .attr_typed::<VelY>(0.02)
                    .attr_typed::<VelZ>(0.03)
                    .resonator_type("Physics3D")
                    .done();
            }

            world.build();

            b.iter(|| {
                world.tick();
            });
        });
    }

    group.finish();
}

// ═══════════════════════════════════════════════════════════════
//  BENCHMARK 3: Multiple Resonators
// ═══════════════════════════════════════════════════════════════

fn bench_multiple_resonators(c: &mut Criterion) {
    let mut group = c.benchmark_group("iteration/multi_resonator");
    group.sample_size(50);

    for &count in &[1_000, 10_000, 50_000, 100_000] {
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &count| {
            let mut world = World::new();
            world.register_resonator::<Physics2D>("Physics2D");
            world.register_resonator::<EnergyDrain>("EnergyDrain");

            for _ in 0..count {
                world
                    .entity("Entity")
                    .attr_typed::<PosX>(0.0)
                    .attr_typed::<PosY>(0.0)
                    .attr_typed::<VelX>(0.01)
                    .attr_typed::<VelY>(0.02)
                    .attr_typed::<Energy>(100.0)
                    .resonator_type("Physics2D")
                    .resonator_type("EnergyDrain")
                    .done();
            }

            world.build();

            b.iter(|| {
                world.tick();
            });
        });
    }

    group.finish();
}

// ═══════════════════════════════════════════════════════════════
//  BENCHMARK 4: Read/Write API
// ═══════════════════════════════════════════════════════════════

fn bench_read_write_api(c: &mut Criterion) {
    let mut group = c.benchmark_group("api/read_write");
    group.sample_size(100);

    // Setup
    let mut world = World::new();
    let entity = world
        .entity("Test")
        .attr_typed::<PosX>(0.0)
        .attr_typed::<PosY>(0.0)
        .attr_typed::<Energy>(100.0)
        .done();
    world.build();

    // Benchmark: World API (slow)
    group.bench_function("world_api", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                let x = world.read_typed::<PosX>(entity).unwrap();
                let y = world.read_typed::<PosY>(entity).unwrap();
                let e = world.read_typed::<Energy>(entity).unwrap();
                bb(x + y + e);
            }
        });
    });

    // Benchmark: EntityAccessor (fast)
    let acc = EntityAccessor::new(&world, entity, &["PosX", "PosY", "Energy"]).unwrap();
    group.bench_function("entity_accessor", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                let x = acc.get(&world, 0);
                let y = acc.get(&world, 1);
                let e = acc.get(&world, 2);
                bb(x + y + e);
            }
        });
    });

    // Benchmark: EntityRef (medium)
    let eref = world.entity_ref(entity).unwrap();
    group.bench_function("entity_ref", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                let x = eref.get::<PosX>(&world).unwrap();
                let y = eref.get::<PosY>(&world).unwrap();
                let e = eref.get::<Energy>(&world).unwrap();
                bb(x + y + e);
            }
        });
    });

    group.finish();
}

// ═══════════════════════════════════════════════════════════════
//  BENCHMARK 5: Spatial Queries
// ═══════════════════════════════════════════════════════════════

fn bench_spatial_queries(c: &mut Criterion) {
    let mut group = c.benchmark_group("spatial/queries");
    group.sample_size(50);

    for &count in &[1_000, 10_000, 50_000] {
        group.bench_with_input(
            BenchmarkId::new("radius_query", count),
            &count,
            |b, &count| {
                let mut world = World::new();

                // Create entities in grid
                let grid_size = (count as f64).sqrt() as usize;
                for i in 0..count {
                    let x = (i % grid_size) as f64 * 10.0;
                    let y = (i / grid_size) as f64 * 10.0;
                    world
                        .entity("Entity")
                        .attr_typed::<PosX>(x)
                        .attr_typed::<PosY>(y)
                        .done();
                }

                world.build();

                let mut grid = world.create_spatial_grid(SpatialConfig::new(
                    50.0,
                    0.0,
                    0.0,
                    grid_size as f64 * 10.0,
                    grid_size as f64 * 10.0,
                ));
                world.rebuild_spatial_grid(&mut grid);

                b.iter(|| {
                    let result = grid.query_radius(500.0, 500.0, 100.0);
                    bb(result.len());
                });
            },
        );
    }

    group.finish();
}

// ═══════════════════════════════════════════════════════════════
//  BENCHMARK 6: Query System
// ═══════════════════════════════════════════════════════════════

fn bench_query_system(c: &mut Criterion) {
    let mut group = c.benchmark_group("query/filter");
    group.sample_size(50);

    let mut world = World::new();

    // Create mixed entities
    for i in 0..10_000 {
        let mut builder = world
            .entity("Unit")
            .attr_typed::<Health>(50.0 + (i % 100) as f64)
            .attr_typed::<Energy>(100.0 - (i % 50) as f64);

        if i % 2 == 0 {
            builder = builder.tag_typed::<Enemy>();
        } else {
            builder = builder.tag_typed::<Player>();
        }

        builder.done();
    }

    world.build();

    // Simple query
    group.bench_function("simple", |b| {
        b.iter(|| {
            let result = world.query().with::<Enemy>().execute();
            bb(result.len());
        });
    });

    // Filtered query
    group.bench_function("filtered", |b| {
        b.iter(|| {
            let result = world
                .query()
                .with::<Enemy>()
                .filter(|e, w| w.read_typed::<Health>(e).unwrap_or(0.0) > 80.0)
                .execute();
            bb(result.len());
        });
    });

    // Count query
    group.bench_function("count", |b| {
        b.iter(|| {
            let count = world
                .query()
                .with::<Player>()
                .filter(|e, w| w.read_typed::<Energy>(e).unwrap_or(0.0) < 60.0)
                .count();
            bb(count);
        });
    });

    group.finish();
}

// ═══════════════════════════════════════════════════════════════
//  BENCHMARK 7: Serialization
// ═══════════════════════════════════════════════════════════════

fn bench_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialization");
    group.sample_size(20);

    for &count in &[100, 1_000, 10_000] {
        // Snapshot creation
        group.bench_with_input(
            BenchmarkId::new("snapshot", count),
            &count,
            |b, &count| {
                let mut world = World::new();

                for i in 0..count {
                    world
                        .entity("Entity")
                        .attr_typed::<PosX>(i as f64)
                        .attr_typed::<Energy>(100.0)
                        .done();
                }

                world.build();

                b.iter(|| {
                    let snapshot = world.snapshot();
                    bb(snapshot.entity_count());
                });
            },
        );

        // Restore
        group.bench_with_input(
            BenchmarkId::new("restore", count),
            &count,
            |b, &count| {
                let mut world = World::new();

                for i in 0..count {
                    world
                        .entity("Entity")
                        .attr_typed::<PosX>(i as f64)
                        .attr_typed::<Energy>(100.0)
                        .done();
                }

                world.build();
                let snapshot = world.snapshot();

                b.iter(|| {
                    world.restore(snapshot.clone()).unwrap();
                    world.build();
                });
            },
        );
    }

    group.finish();
}

// ═══════════════════════════════════════════════════════════════
//  BENCHMARK 8: Relations
// ═══════════════════════════════════════════════════════════════

define_relation!(Parent);

fn bench_relations(c: &mut Criterion) {
    let mut group = c.benchmark_group("relations");
    group.sample_size(50);

    let mut world = World::new();

    // Create hierarchy: 1 root + 1000 children
    let root = world.entity("Root").attr_typed::<Energy>(100.0).done();
    let mut children = Vec::new();

    for _ in 0..1_000 {
        let child = world.entity("Child").attr_typed::<Energy>(50.0).done();
        children.push(child);
    }

    world.build();

    // Add relations
    for child in &children {
        world.add_relation::<Parent>(*child, root);
    }

    // Benchmark: get relation
    group.bench_function("get", |b| {
        b.iter(|| {
            for child in &children {
                let parent = world.get_relation::<Parent>(*child);
                bb(parent);
            }
        });
    });

    // Benchmark: get reverse
    group.bench_function("get_reverse", |b| {
        b.iter(|| {
            let children = world.get_reverse_relations::<Parent>(root);
            bb(children.len());
        });
    });

    group.finish();
}

// ═══════════════════════════════════════════════════════════════
//  BENCHMARK 9: Buffer Modes
// ═══════════════════════════════════════════════════════════════

fn bench_buffer_modes(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_mode");
    group.sample_size(50);

    let count = 50_000;

    // Single buffer
    group.bench_function("single", |b| {
        let mut world = World::new();
        world.set_buffer_mode(BufferMode::Single);
        world.register_resonator::<Physics2D>("Physics2D");

        for _ in 0..count {
            world
                .entity("Particle")
                .attr_typed::<PosX>(0.0)
                .attr_typed::<PosY>(0.0)
                .attr_typed::<VelX>(0.01)
                .attr_typed::<VelY>(0.02)
                .resonator_type("Physics2D")
                .done();
        }

        world.build();

        b.iter(|| {
            world.tick();
        });
    });

    // Double buffer
    group.bench_function("double", |b| {
        let mut world = World::new();
        world.set_buffer_mode(BufferMode::Double);
        world.register_resonator::<Physics2D>("Physics2D");

        for _ in 0..count {
            world
                .entity("Particle")
                .attr_typed::<PosX>(0.0)
                .attr_typed::<PosY>(0.0)
                .attr_typed::<VelX>(0.01)
                .attr_typed::<VelY>(0.02)
                .resonator_type("Physics2D")
                .done();
        }

        world.build();

        b.iter(|| {
            world.tick();
        });
    });

    group.finish();
}

// ═══════════════════════════════════════════════════════════════
//  CRITERION CONFIG
// ═══════════════════════════════════════════════════════════════

criterion_group!(
    benches,
    bench_simple_iteration,
    bench_physics_3d,
    bench_multiple_resonators,
    bench_read_write_api,
    bench_spatial_queries,
    bench_query_system,
    bench_serialization,
    bench_relations,
    bench_buffer_modes,
);

criterion_main!(benches);