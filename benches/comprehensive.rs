// benches/comprehensive.rs

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use resonance_engine::*;
use std::hint::black_box as bb;

// ═══════════════════════════════════════════════════════════════
//  ATTRIBUTES
// ═══════════════════════════════════════════════════════════════

define_attrs!(PosX, PosY, PosZ, VelX, VelY, VelZ);
define_attrs!(Health, Energy, Damage, Level);  // ← ADD Level HERE
define_tags!(Enemy, Player, Unit);  // ← ADD Unit HERE

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

define_relation!(Parent);  // ← ADD THIS BEFORE bench_graph_queries

fn bench_graph_queries(c: &mut Criterion) {
    let mut group = c.benchmark_group("graph/queries");
    group.sample_size(50);

    for &count in &[100, 1_000, 10_000] {
        // Setup hierarchy
        group.bench_with_input(
            BenchmarkId::new("hierarchy_creation", count),
            &count,
            |b, &count| {
                b.iter(|| {
                    let mut world = World::new();
                    
                    let root = world.entity("Root").done();
                    
                    let mut parents = vec![root];
                    for _ in 0..count {
                        let child = world.entity("Child").done();
                        parents.push(child);
                    }
                    
                    world.build();
                    
                    // Add relations
                    for i in 1..parents.len() {
                        world.add_relation::<Parent>(parents[i], parents[0]);
                    }
                    
                    bb(world.alive_count());
                });
            },
        );

        // Benchmark: relation lookup
        group.bench_with_input(
            BenchmarkId::new("relation_lookup", count),
            &count,
            |b, &count| {
                let mut world = World::new();
                
                let root = world.entity("Root").done();
                
                let mut children = Vec::new();
                for _ in 0..count {
                    let child = world.entity("Child").done();
                    children.push(child);
                }
                
                world.build();
                
                for &child in &children {
                    world.add_relation::<Parent>(child, root);
                }

                b.iter(|| {
                    let result = world.get_reverse_relations::<Parent>(root);
                    bb(result.len());
                });
            },
        );

        // Benchmark: BFS traversal (simplified version)
        group.bench_with_input(
            BenchmarkId::new("bfs_traversal", count),
            &count,
            |b, &count| {
                let mut world = World::new();
                
                let root = world.entity("Root").done();
                
                let mut all_children = Vec::new();
                for _ in 0..count.min(1000) {
                    let child = world.entity("Node").done();
                    all_children.push(child);
                }
                
                world.build();
                
                // Simple flat hierarchy
                for &child in &all_children {
                    world.add_relation::<Parent>(child, root);
                }

                b.iter(|| {
                    let mut visited = std::collections::HashSet::new();
                    let mut queue = std::collections::VecDeque::new();
                    queue.push_back(root);
                    visited.insert(root);
                    
                    let mut result = Vec::new();
                    while let Some(current) = queue.pop_front() {
                        let children = world.get_reverse_relations::<Parent>(current);
                        for child in children {
                            if visited.insert(child) {
                                result.push(child);
                                queue.push_back(child);
                            }
                        }
                    }
                    
                    bb(result.len());
                });
            },
        );
    }

    group.finish();
}

// ═══════════════════════════════════════════════════════════════
//  BENCHMARK 11: Graph vs Traditional Queries
// ═══════════════════════════════════════════════════════════════

fn bench_graph_vs_traditional(c: &mut Criterion) {
    let mut group = c.benchmark_group("comparison/graph_vs_traditional");
    group.sample_size(50);

    let count = 10_000;

    // Setup world with hierarchy
    let mut world = World::new();
    
    let root = world.entity("Root").tag_typed::<Unit>().done();
    
    let mut all_children = Vec::new();
    for i in 0..count {
        let child = world
            .entity("Child")
            .tag_typed::<Unit>()
            .attr_typed::<Level>(1.0)
            .attr_typed::<PosX>(i as f64)
            .done();
        all_children.push(child);
    }
    
    world.build();
    
    for &child in &all_children {
        world.add_relation::<Parent>(child, root);
    }

    // Benchmark: Traditional tag query
    group.bench_function("traditional_tag_query", |b| {
        b.iter(|| {
            let result = world.query().with::<Unit>().execute();
            bb(result.len());
        });
    });

    // Benchmark: Graph relation query
    group.bench_function("graph_relation_query", |b| {
        b.iter(|| {
            let result = world.get_reverse_relations::<Parent>(root);
            bb(result.len());
        });
    });

    // Benchmark: Traditional filtered query
    group.bench_function("traditional_filtered", |b| {
        b.iter(|| {
            let result = world
                .query()
                .with::<Unit>()
                .filter(|e, w| w.read_typed::<Level>(e).unwrap_or(0.0) == 1.0)
                .execute();
            bb(result.len());
        });
    });

    group.finish();
}

// ═══════════════════════════════════════════════════════════════
//  CRITERION CONFIG (SINGLE DEFINITION)
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
    // bench_relations,
    // bench_buffer_modes,
    bench_graph_queries,
    bench_graph_vs_traditional,
);

criterion_main!(benches);