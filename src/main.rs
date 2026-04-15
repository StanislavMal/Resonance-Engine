// src/main.rs
//! Resonance Engine v11.0 — Complete demonstration
//!
//! Showcases all major features:
//! - Struct-based resonators (hot-reload ready)
//! - Relations system
//! - Advanced queries
//! - Serialization
//! - Profiling
//! - Ecosystem simulation

use resonance_engine::*;
use std::time::Instant;

// ═══════════════════════════════════════════════════════════════
//  ATTRIBUTES & RELATIONS
// ═══════════════════════════════════════════════════════════════

define_attrs!(PosX, PosY, VelX, VelY);
define_attrs!(Energy, MaxEnergy, Health, Damage);
define_attrs!(Speed, VisionRange, ReproduceCooldown, FoodValue, GrowthRate);
define_tags!(Grass, Rabbit, Wolf, Player, Enemy);
define_relation!(Parent);
define_relation!(Target);

define_enum_attr!(AiState => Idle, Patrol, Chase, Flee);

// ═══════════════════════════════════════════════════════════════
//  RESONATORS (Struct-based for hot reload)
// ═══════════════════════════════════════════════════════════════

define_resonator!(
    PhysicsResonator {
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
    EnergyDrainResonator {
        energy: Energy
    }
    |this, ctx| {
        this.energy.add_unchecked(ctx, -0.5);
        if this.energy.get(ctx) <= 0.0 {
            ctx.despawn_self();
        }
    }
);

define_resonator!(
    GrassGrowthResonator {
        energy: Energy,
        max_energy: MaxEnergy,
        growth_rate: GrowthRate
    }
    |this, ctx| {
        let e = this.energy.get(ctx);
        let m = this.max_energy.get(ctx);
        if e < m {
            let new_e = (e + this.growth_rate.get(ctx)).min(m);
            this.energy.set_unchecked(ctx, new_e);
        }
        if e <= 0.0 {
            ctx.despawn_self();
        }
    }
);

// ═══════════════════════════════════════════════════════════════
//  MAIN
// ═══════════════════════════════════════════════════════════════

fn main() {
    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║            RESONANCE ENGINE v11.0 — COMPLETE DEMO                   ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝\n");

    let total_start = Instant::now();

    separator("PART 1: CORE FEATURES");
    demo_struct_resonators();
    demo_relations();
    demo_advanced_queries();
    demo_serialization();
    demo_profiling();

    separator("PART 2: ECOSYSTEM SIMULATION");
    demo_ecosystem();

    separator("PART 3: PERFORMANCE");
    demo_performance();

    println!("\n  Total execution time: {}", fmt_time(total_start.elapsed()));
    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║            ✅ ALL DEMOS PASSED                                       ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 1: Struct-based Resonators
// ═══════════════════════════════════════════════════════════════

fn demo_struct_resonators() {
    println!("\n🔹 DEMO 1: Struct-based Resonators (Hot-reload ready)\n");

    let mut world = World::new();
    
    // Register resonator types
    world.register_resonator::<PhysicsResonator>("Physics");
    world.register_resonator::<EnergyDrainResonator>("EnergyDrain");

    // Create entities
    for i in 0..5 {
        world
            .entity("Particle")
            .attr_typed::<PosX>(i as f64 * 10.0)
            .attr_typed::<PosY>(0.0)
            .attr_typed::<VelX>(1.0)
            .attr_typed::<VelY>(0.5)
            .attr_typed::<Energy>(10.0)
            .resonator_type("Physics")
            .resonator_type("EnergyDrain")
            .done();
    }

    world.build();

    println!("  Created {} particles", world.alive_count());
    
    for tick in 0..5 {
        world.tick();
        let entities: Vec<_> = world.query().with::<Energy>().execute();
        
        let avg_energy: f64 = entities
            .iter()
            .filter_map(|e| world.read_typed::<Energy>(*e))
            .sum::<f64>()
            / entities.len().max(1) as f64;
        
        println!("  Tick {}: avg energy = {:.1}", tick, avg_energy);
    }

    println!("\n  ✅ Struct-based resonators work!");
    println!("  💡 These can be hot-reloaded from dynamic libraries");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 2: Relations
// ═══════════════════════════════════════════════════════════════

fn demo_relations() {
    println!("\n🔹 DEMO 2: Relations System\n");

    let mut world = World::new();

    let parent = world
        .entity("Parent")
        .attr_typed::<PosX>(100.0)
        .attr_typed::<PosY>(50.0)
        .done();

    let child1 = world.entity("Child").attr_typed::<PosX>(0.0).attr_typed::<PosY>(0.0).done();
    let child2 = world.entity("Child").attr_typed::<PosX>(0.0).attr_typed::<PosY>(0.0).done();

    world.build();

    // Add relations
    world.add_relation::<Parent>(child1, parent);
    world.add_relation::<Parent>(child2, parent);

    println!("  Created hierarchy: 1 parent, 2 children");

    // Query relations
    if let Some(parent_handle) = world.get_relation::<Parent>(child1) {
        let px = world.read_typed::<PosX>(parent_handle).unwrap();
        println!("  Child1's parent at position: {:.0}", px);
    }

    let children = world.get_reverse_relations::<Parent>(parent);
    println!("  Parent has {} children", children.len());

    // Remove relation
    world.remove_relation::<Parent>(child1, parent);
    let remaining = world.get_reverse_relations::<Parent>(parent).len();
    println!("  After removing one: {} children remain", remaining);

    println!("\n  ✅ Relations system working!");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 3: Advanced Queries
// ═══════════════════════════════════════════════════════════════

fn demo_advanced_queries() {
    println!("\n🔹 DEMO 3: Advanced Queries\n");

    let mut world = World::new();

    // Create mixed entities
    for i in 0..10 {
        let mut builder = world
            .entity("Unit")
            .attr_typed::<Health>(50.0 + i as f64 * 10.0)
            .attr_typed::<Energy>(100.0 - i as f64 * 5.0);

        if i % 2 == 0 {
            builder = builder.tag_typed::<Enemy>();
        } else {
            builder = builder.tag_typed::<Player>();
        }

        builder.done();
    }

    world.build();

    // Query 1: Strong enemies
    let strong_enemies = world
        .query()
        .with::<Enemy>()
        .with::<Health>()
        .filter(|e, w| w.read_typed::<Health>(e).unwrap_or(0.0) > 80.0)
        .execute();

    println!("  Strong enemies (health > 80): {}", strong_enemies.len());

    // Query 2: Tired players
    let tired_players = world
        .query()
        .with::<Player>()
        .without::<Enemy>()
        .filter(|e, w| w.read_typed::<Energy>(e).unwrap_or(100.0) < 60.0)
        .count();

    println!("  Tired players (energy < 60): {}", tired_players);

    // Query 3: First enemy
    let first = world.query().with::<Enemy>().first();
    println!("  First enemy: {:?}", first.is_some());

    println!("\n  ✅ Advanced queries working!");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 4: Serialization
// ═══════════════════════════════════════════════════════════════

fn demo_serialization() {
    println!("\n🔹 DEMO 4: Serialization & Persistence\n");

    let mut world = World::new();

    for i in 0..3 {
        world
            .entity("Saved")
            .attr_typed::<PosX>(i as f64 * 10.0)
            .attr_typed::<Energy>(100.0 - i as f64 * 10.0)
            .done();
    }

    world.build();

    // Save snapshot
    let snapshot = world.snapshot();
    println!("  Saved {} entities to snapshot", snapshot.entity_count());

    // Modify world
    for e in world.entities().to_vec() {
        world.write_typed::<Energy>(e, 999.0);
    }
    println!("  Modified all entities (energy = 999)");

    // Restore
    world.restore(snapshot.clone()).unwrap();
    world.build();

    let first = world.entities()[0];
    let energy = world.read_typed::<Energy>(first).unwrap();
    println!("  Restored: first entity energy = {:.0}", energy);

    // File I/O
    snapshot.save("test_snapshot.json").unwrap();
    let loaded = SchemaSnapshot::load("test_snapshot.json").unwrap();
    println!("  Loaded {} entities from disk", loaded.entity_count());

    std::fs::remove_file("test_snapshot.json").ok();

    println!("\n  ✅ Serialization working!");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 5: Profiling
// ═══════════════════════════════════════════════════════════════

fn demo_profiling() {
    println!("\n🔹 DEMO 5: Built-in Profiler\n");

    let mut world = World::new();
    world.register_resonator::<PhysicsResonator>("Physics");

    for _ in 0..1000 {
        world
            .entity("Particle")
            .attr_typed::<PosX>(0.0)
            .attr_typed::<PosY>(0.0)
            .attr_typed::<VelX>(1.0)
            .attr_typed::<VelY>(1.0)
            .resonator_type("Physics")
            .done();
    }

    world.build();

    let mut profiler = Profiler::new();

    for _ in 0..10 {
        profile!(profiler, "tick", { world.tick(); });
    }

    println!("{}", profiler.report());
    println!("  ✅ Profiler working!");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 6: Ecosystem
// ═══════════════════════════════════════════════════════════════

fn demo_ecosystem() {
    println!("\n🔹 ECOSYSTEM: Grass → Rabbit → Wolf\n");

    let mut world = World::new();
    world.set_buffer_mode(BufferMode::Double);

    // Register resonators
    world.register_resonator::<PhysicsResonator>("Physics");
    world.register_resonator::<GrassGrowthResonator>("GrassGrowth");
    world.register_resonator::<EnergyDrainResonator>("EnergyDrain");

    let world_size = 500.0;

    // Create grass
    for i in 0..50 {
        world
            .entity("Grass")
            .attr_typed::<PosX>(pseudo_random(i * 7 + 1) * world_size)
            .attr_typed::<PosY>(pseudo_random(i * 13 + 3) * world_size)
            .attr_typed::<Energy>(20.0)
            .attr_typed::<MaxEnergy>(50.0)
            .attr_typed::<FoodValue>(15.0)
            .attr_typed::<GrowthRate>(0.5)
            .tag_typed::<Grass>()
            .resonator_type("GrassGrowth")
            .done();
    }

    // Create rabbits
    for i in 0..15 {
        world
            .entity("Rabbit")
            .attr_typed::<PosX>(pseudo_random(i * 17 + 100) * world_size)
            .attr_typed::<PosY>(pseudo_random(i * 23 + 200) * world_size)
            .attr_typed::<VelX>(0.0)
            .attr_typed::<VelY>(0.0)
            .attr_typed::<Energy>(50.0)
            .attr_typed::<Speed>(2.0)
            .tag_typed::<Rabbit>()
            .resonator_type("Physics")
            .resonator_type("EnergyDrain")
            .done();
    }

    // Create wolves
    for i in 0..3 {
        world
            .entity("Wolf")
            .attr_typed::<PosX>(pseudo_random(i * 37 + 500) * world_size)
            .attr_typed::<PosY>(pseudo_random(i * 43 + 700) * world_size)
            .attr_typed::<VelX>(0.0)
            .attr_typed::<VelY>(0.0)
            .attr_typed::<Energy>(80.0)
            .attr_typed::<Speed>(3.0)
            .tag_typed::<Wolf>()
            .resonator_type("Physics")
            .resonator_type("EnergyDrain")
            .done();
    }

    world.build();

    println!("  Initial: 🌿50 🐰15 🐺3");

    let mut grid = world.create_spatial_grid(SpatialConfig::new(
        50.0, 0.0, 0.0, world_size, world_size
    ));

    // Simulate
    for tick in 0..100 {
        world.rebuild_spatial_grid(&mut grid);
        
        // Simple AI (just random movement for demo)
        for entity in world.query().with::<Rabbit>().execute() {
            let angle = pseudo_random(entity.0.index as u64 + tick) * std::f64::consts::TAU;
            world.write_typed::<VelX>(entity, angle.cos() * 2.0);
            world.write_typed::<VelY>(entity, angle.sin() * 2.0);
        }
        
        world.tick();

        if tick % 20 == 0 {
            let g = query::count_with::<Grass>(&world);
            let r = query::count_with::<Rabbit>(&world);
            let w = query::count_with::<Wolf>(&world);
            println!("  Tick {:>3}: 🌿{:<4} 🐰{:<4} 🐺{:<4}", tick, g, r, w);
        }
    }

    let fg = query::count_with::<Grass>(&world);
    let fr = query::count_with::<Rabbit>(&world);
    let fw = query::count_with::<Wolf>(&world);
    println!("\n  Final: 🌿{} 🐰{} 🐺{}", fg, fr, fw);
    println!("  ✅ Ecosystem simulation complete!");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 7: Performance
// ═══════════════════════════════════════════════════════════════

fn demo_performance() {
    println!("\n🔹 PERFORMANCE: Batch integration (PosX += VelX, PosY += VelY)\n");

    for &count in &[1_000, 10_000, 50_000, 100_000] {
        let mut world = World::new();
        world.register_resonator::<PhysicsResonator>("Physics");

        for _ in 0..count {
            world
                .entity("Particle")
                .attr_typed::<PosX>(0.0)
                .attr_typed::<PosY>(0.0)
                .attr_typed::<VelX>(0.01)
                .attr_typed::<VelY>(0.02)
                .resonator_type("Physics")
                .done();
        }

        world.build();

        // Warmup
        for _ in 0..5 {
            world.tick();
        }

        // Benchmark
        let mut times = Vec::new();
        for _ in 0..30 {
            let start = Instant::now();
            world.tick();
            times.push(start.elapsed());
        }

        times.sort();
        let avg: std::time::Duration = times.iter().sum::<std::time::Duration>() / times.len() as u32;
        let median = times[times.len() / 2];

        println!(
            "  {:>7} entities: avg {:<10} median {:<10} ({:.0} M ent/sec)",
            format_num(count),
            fmt_time(avg),
            fmt_time(median),
            count as f64 / avg.as_secs_f64() / 1_000_000.0
        );
    }

    println!("\n  ✅ Performance benchmark complete!");
}

// ═══════════════════════════════════════════════════════════════
//  HELPERS
// ═══════════════════════════════════════════════════════════════

fn separator(title: &str) {
    println!("\n{}", "═".repeat(70));
    println!("  {}", title);
    println!("{}", "═".repeat(70));
}

fn fmt_time(d: std::time::Duration) -> String {
    if d.as_millis() > 0 {
        format!("{:.2}ms", d.as_secs_f64() * 1000.0)
    } else {
        format!("{:.1}µs", d.as_secs_f64() * 1_000_000.0)
    }
}

fn format_num(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{}M", n / 1_000_000)
    } else if n >= 1_000 {
        format!("{}K", n / 1_000)
    } else {
        n.to_string()
    }
}

fn pseudo_random(seed: u64) -> f64 {
    let mut x = seed.wrapping_add(0x9E3779B97F4A7C15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58476D1CE4E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D049BB133111EB);
    x ^= x >> 31;
    (x % 1_000_000) as f64 / 1_000_000.0
}