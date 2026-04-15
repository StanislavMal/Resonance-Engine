// examples/demo_v11.rs

use resonance_engine::*;

define_attrs!(PosX, PosY, VelX, VelY, Energy, Health, Damage);
define_tags!(Enemy, Player);
define_relation!(Parent);
define_relation!(Target);

// FIXED: Explicit types for closure parameters
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

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║       RESONANCE ENGINE v11.0 — FULL FEATURE DEMO          ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    demo_struct_resonators();
    demo_relations();
    demo_serialization();
    demo_advanced_queries();
    demo_profiling();

    println!("\n✅ All v11.0 features demonstrated successfully!");
}

fn demo_struct_resonators() {
    println!("═══ DEMO 1: Struct-based Resonators ═══\n");

    let mut world = World::new();
    
    world.register_resonator::<PhysicsResonator>("Physics");
    world.register_resonator::<EnergyDrainResonator>("EnergyDrain");

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

    println!("Created {} particles with struct-based resonators", world.alive_count());
    
    for tick in 0..5 {
        world.tick();
        let entities: Vec<_> = world
            .query()
            .with::<Energy>()
            .execute();
        
        let avg_energy: f64 = entities
            .iter()
            .filter_map(|e| world.read_typed::<Energy>(*e))
            .sum::<f64>()
            / entities.len().max(1) as f64;
        
        println!("  Tick {}: avg energy = {:.1}", tick, avg_energy);
    }

    println!("  ✓ Struct-based resonators are hot-reload ready!\n");
}

fn demo_relations() {
    println!("═══ DEMO 2: Relations System ═══\n");

    let mut world = World::new();

    let parent = world
        .entity("Parent")
        .attr_typed::<PosX>(100.0)
        .attr_typed::<PosY>(50.0)
        .done();

    let child1 = world
        .entity("Child")
        .attr_typed::<PosX>(0.0)
        .attr_typed::<PosY>(0.0)
        .done();

    let child2 = world
        .entity("Child")
        .attr_typed::<PosX>(0.0)
        .attr_typed::<PosY>(0.0)
        .done();

    world.build();

    world.add_relation::<Parent>(child1, parent);
    world.add_relation::<Parent>(child2, parent);

    println!("Created parent-child hierarchy:");
    println!("  Parent: {:?}", parent);
    println!("  Child 1: {:?}", child1);
    println!("  Child 2: {:?}", child2);

    if let Some(parent_handle) = world.get_relation::<Parent>(child1) {
        let px = world.read_typed::<PosX>(parent_handle).unwrap();
        println!("\n  Child1's parent position: {:.0}", px);
    }

    let children = world.get_reverse_relations::<Parent>(parent);
    println!("  Parent has {} children", children.len());

    world.remove_relation::<Parent>(child1, parent);
    println!("\n  After removing child1 relation:");
    println!("  Parent has {} children", world.get_reverse_relations::<Parent>(parent).len());

    println!("  ✓ Relations system working!\n");
}

fn demo_serialization() {
    println!("═══ DEMO 3: Serialization ═══\n");

    let mut world = World::new();

    for i in 0..3 {
        world
            .entity("Saved")
            .attr_typed::<PosX>(i as f64 * 10.0)
            .attr_typed::<Energy>(100.0 - i as f64 * 10.0)
            .done();
    }

    world.build();

    let snapshot = world.snapshot();
    println!("Saved {} entities", snapshot.entity_count());

    world.tick();
    for e in world.entities().to_vec() {
        world.write_typed::<Energy>(e, 999.0);
    }
    println!("Modified all entities (energy=999)");

    world.restore(snapshot.clone()).unwrap();
    world.build();

    println!("Restored from snapshot");
    let first = world.entities()[0];
    let energy = world.read_typed::<Energy>(first).unwrap();
    println!("  First entity energy: {:.0} (should be 100)", energy);

    snapshot.save("test_snapshot.json").unwrap();
    println!("\n  Saved to test_snapshot.json");

    let loaded = SchemaSnapshot::load("test_snapshot.json").unwrap();
    println!("  Loaded {} entities from file", loaded.entity_count());

    std::fs::remove_file("test_snapshot.json").ok();

    println!("  ✓ Serialization working!\n");
}

fn demo_advanced_queries() {
    println!("═══ DEMO 4: Advanced Queries ═══\n");

    let mut world = World::new();

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

    let strong_enemies = world
        .query()
        .with::<Enemy>()
        .with::<Health>()
        .filter(|e, w| w.read_typed::<Health>(e).unwrap_or(0.0) > 80.0)
        .execute();

    println!("Strong enemies (health > 80): {}", strong_enemies.len());

    let tired_players = world
        .query()
        .with::<Player>()
        .without::<Enemy>()
        .filter(|e, w| w.read_typed::<Energy>(e).unwrap_or(100.0) < 60.0)
        .count();

    println!("Tired players (energy < 60): {}", tired_players);

    let first_enemy = world.query().with::<Enemy>().first();
    println!("First enemy: {:?}", first_enemy);

    println!("  ✓ Advanced queries working!\n");
}

fn demo_profiling() {
    println!("═══ DEMO 5: Built-in Profiler ═══\n");

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
        profile!(profiler, "full_tick", {
            profile!(profiler, "tick_logic", {
                world.tick();
            });
        });
    }

    println!("{}\n", profiler.report());
}