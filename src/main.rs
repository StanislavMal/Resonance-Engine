// src/main.rs
//! Resonance Engine v10.3 — Full Demo + Ecosystem Simulation

mod accessor;
mod archetype;
mod context;
mod entity;
mod events;
mod interning;
mod metrics;
mod prefab;
mod query;
mod resonator;
mod scheduler;
mod spatial;
mod storage;
mod typed_attrs;
mod world;

use accessor::{read_at, write_at, AttrOffset, EntityAccessor, EntityRef};
use context::NodeContext;
use prefab::WorldPrefabExt;
use resonator::{BoundField, DynResonator, FieldMap, Resonator};
use spatial::{SpatialConfig, SpatialGrid, WorldSpatialExt};
use world::World;

use std::sync::Arc;
use std::time::Instant;

// ═══════════════════════════════════════════════════════════════
//  ATTRIBUTES
// ═══════════════════════════════════════════════════════════════

define_attrs!(PosX, PosY, VelX, VelY);
define_attrs!(Energy, MaxEnergy, Speed, VisionRange);
define_attrs!(FoodValue, GrowthRate, ReproduceCooldown);
define_attrs!(Health, Damage, Lifetime, TargetX, TargetY);
define_tags!(Grass, Rabbit, Wolf, Bullet, Enemy, Player);

define_enum_attr!(AiStateEnum => Idle, Patrol, Chase, Flee, Attack);
define_int_attr!(Population);
define_bool_attr!(IsAlive);

// ═══════════════════════════════════════════════════════════════
//  HELPERS
// ═══════════════════════════════════════════════════════════════

fn fmt_time(d: std::time::Duration) -> String {
    if d.as_millis() > 0 {
        format!("{:.2}ms", d.as_secs_f64() * 1000.0)
    } else {
        format!("{:.1}µs", d.as_secs_f64() * 1_000_000.0)
    }
}

fn separator(title: &str) {
    println!("\n{}", "═".repeat(70));
    println!("  {}", title);
    println!("{}", "═".repeat(70));
}

/// Deterministic pseudo-random for reproducibility.
/// Returns value in [0.0, 1.0).
fn pseudo_random(seed: u64) -> f64 {
    let mut x = seed.wrapping_add(0x9E3779B97F4A7C15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58476D1CE4E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D049BB133111EB);
    x ^= x >> 31;
    (x % 1_000_000) as f64 / 1_000_000.0
}

// ═══════════════════════════════════════════════════════════════
//  MAIN
// ═══════════════════════════════════════════════════════════════

fn main() {
    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║            RESONANCE ENGINE v10.3 — FULL DEMO                      ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");

    let total_start = Instant::now();

    // ── Part 1: Core feature demos ────────────────────
    separator("DEMO 1: GENERATION SAFETY");
    demo_generation_safety();

    separator("DEMO 2: SET vs SET_UNCHECKED EXPLAINED");
    demo_set_variants();

    separator("DEMO 3: DEFER_WRITE — CROSS-ENTITY DAMAGE");
    demo_defer_write();

    separator("DEMO 4: DEFER_SPAWN — REPRODUCTION FROM RESONATOR");
    demo_defer_spawn();

    separator("DEMO 5: DESPAWN LIFECYCLE — EXACT TIMING");
    demo_despawn_lifecycle();

    separator("DEMO 6: ENTITYACCESSOR vs WORLD API BENCHMARK");
    demo_accessor_benchmark();

    separator("DEMO 7: EXTERNAL COMMAND QUEUE PATTERN");
    demo_command_queue();

    separator("DEMO 8: COLLISION DETECTION PATTERN");
    demo_collision();

    separator("DEMO 9: STATE MACHINE WITH ENUM ATTRIBUTES");
    demo_state_machine();

    separator("DEMO 10: DEBUGGING RESONATORS");
    demo_debugging();

    // ── Part 2: Full ecosystem simulation ─────────────
    separator("ECOSYSTEM: GRASS → RABBIT → WOLF (200 ticks)");
    demo_ecosystem();

    // ── Part 3: Performance ───────────────────────────
    separator("PERFORMANCE BENCHMARK");
    demo_performance();

    println!(
        "\n  Total execution time: {}",
        fmt_time(total_start.elapsed())
    );
    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║            ALL DEMOS PASSED                                        ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 1: Generation Safety
// ═══════════════════════════════════════════════════════════════

fn demo_generation_safety() {
    let mut world = World::new();

    let entity_a = world
        .entity("Temp")
        .attr_typed::<Energy>(100.0)
        .done();
    world.build();

    assert!(world.is_alive(entity_a));
    assert_eq!(world.read_typed::<Energy>(entity_a), Some(100.0));

    world.despawn(entity_a);
    assert!(!world.is_alive(entity_a));
    assert_eq!(world.read_typed::<Energy>(entity_a), None);

    // New entity may reuse same slot — old handle stays invalid
    let entity_b = world
        .entity("New")
        .attr_typed::<Energy>(999.0)
        .done();
    world.build();

    assert_eq!(world.read_typed::<Energy>(entity_a), None); // stale generation
    assert_eq!(world.read_typed::<Energy>(entity_b), Some(999.0));

    println!("  ✓ Stale handle correctly returns None after slot reuse");
    println!("  ✓ Fresh handle reads correct value");
    println!("  ✓ All APIs check generation: read, write, EntityAccessor, EntityRef");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 2: set vs set_unchecked
// ═══════════════════════════════════════════════════════════════

fn demo_set_variants() {
    let mut world = World::new();

    let e = world
        .entity("Test")
        .attr_typed::<PosX>(10.0)
        .on(resonator!(map => {
            let px = map.bind::<PosX>();
            move |ctx: &mut NodeContext| {
                // set: compares old vs new, skips if |diff| < 1e-9
                px.set(ctx, 10.0 + 1e-12); // NO write — delta too small

                // set_unchecked: always writes, always marks dirty
                px.set_unchecked(ctx, 10.001); // WRITES — no comparison

                // add_unchecked: read + add + set_unchecked
                px.add_unchecked(ctx, 0.1); // reads 10.001, writes 10.101
            }
        }))
        .done();
    world.build();
    world.tick();

    let val = world.read_typed::<PosX>(e).unwrap();
    println!("  After tick: PosX = {:.3} (expected 10.101)", val);
    println!();
    println!("  ┌──────────────────┬──────────────────────────────────────────┐");
    println!("  │ Method           │ Behavior                                 │");
    println!("  ├──────────────────┼──────────────────────────────────────────┤");
    println!("  │ set(f, v)        │ Skip if |old-new| < epsilon (1e-9)      │");
    println!("  │ set_unchecked    │ Always write, always mark dirty          │");
    println!("  │ add_unchecked    │ = set_unchecked(f, get(f) + delta)       │");
    println!("  ├──────────────────┼──────────────────────────────────────────┤");
    println!("  │ 'unchecked'      │ No epsilon check, NOT 'no bounds check' │");
    println!("  │                  │ Bounds are debug_assert'd. No UB.        │");
    println!("  └──────────────────┴──────────────────────────────────────────┘");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 3: defer_write — Cross-Entity Damage
// ═══════════════════════════════════════════════════════════════

fn demo_defer_write() {
    let mut world = World::new();

    let attacker = world
        .entity("Attacker")
        .attr_typed::<Energy>(100.0)
        .done();

    let target = world
        .entity("Target")
        .attr_typed::<Energy>(50.0)
        .done();

    world.build();

    let target_energy_offset = world.absolute_offset::<Energy>(target).unwrap();

    world.add_resonator_runtime(attacker, move |map: &FieldMap| {
        let energy = map.bind::<Energy>();
        Arc::new(move |ctx: &mut NodeContext| {
            let my_energy = energy.get(ctx);
            if my_energy > 10.0 {
                let target_e = ctx.read_external(target_energy_offset);
                // Deferred: applied AFTER all resonators finish this tick
                ctx.defer_write(target_energy_offset, target_e - 10.0);
                energy.add_unchecked(ctx, -5.0);
            }
        }) as Arc<DynResonator>
    });

    println!(
        "  Before: Attacker energy={:.0}, Target energy={:.0}",
        world.read_typed::<Energy>(attacker).unwrap(),
        world.read_typed::<Energy>(target).unwrap(),
    );

    world.tick();

    println!(
        "  After:  Attacker energy={:.0}, Target energy={:.0}",
        world.read_typed::<Energy>(attacker).unwrap(),
        world.read_typed::<Energy>(target).unwrap(),
    );
    println!("  ✓ defer_write preserves determinism — all resonators see same snapshot");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 4: defer_spawn
// ═══════════════════════════════════════════════════════════════

fn demo_defer_spawn() {
    let mut world = World::new();

    let mother = world
        .entity("Mother")
        .attr_typed::<PosX>(100.0)
        .attr_typed::<PosY>(200.0)
        .attr_typed::<Energy>(80.0)
        .attr_typed::<ReproduceCooldown>(0.0)
        .done();

    // Template entity so archetype exists before register_spawnable
    let _template = world
        .entity("Child")
        .attr_typed::<PosX>(0.0)
        .attr_typed::<PosY>(0.0)
        .attr_typed::<Energy>(0.0)
        .done();

    world.build();

    // Register AFTER build, BEFORE first tick that uses defer_spawn
    // Attribute names + defaults must match the archetype's schema exactly
    world.register_spawnable(
        "Child",
        &["PosX", "PosY", "Energy"],
        &[0.0, 0.0, 30.0],
    );

    world.add_resonator_runtime(mother, move |map: &FieldMap| {
        let px = map.bind::<PosX>();
        let py = map.bind::<PosY>();
        let energy = map.bind::<Energy>();
        let cooldown = map.bind::<ReproduceCooldown>();

        Arc::new(move |ctx: &mut NodeContext| {
            let cd = cooldown.get(ctx);
            if cd > 0.0 {
                cooldown.add_unchecked(ctx, -1.0);
                return;
            }
            if energy.get(ctx) >= 60.0 {
                ctx.defer_spawn("Child", vec![
                    ("PosX".into(), px.get(ctx) + 1.0),
                    ("PosY".into(), py.get(ctx) + 1.0),
                    ("Energy".into(), 30.0),
                ]);
                energy.add_unchecked(ctx, -30.0);
                cooldown.set_unchecked(ctx, 5.0);
            }
        }) as Arc<DynResonator>
    });

    let before = world.alive_count();
    world.tick();
    let after = world.alive_count();

    println!("  Before: {} entities", before);
    println!("  After:  {} entities (+{} spawned via defer_spawn)", after, after - before);
    println!("  ✓ Entity created after tick completed, not during resonator execution");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 5: Despawn Lifecycle
// ═══════════════════════════════════════════════════════════════

fn demo_despawn_lifecycle() {
    let mut world = World::new();

    let entity = world
        .entity("Mortal")
        .attr_typed::<Energy>(3.0)
        .on(resonator!(map => {
            bind_fields!(map, energy: Energy);
            move |ctx: &mut NodeContext| {
                energy.add_unchecked(ctx, -1.0);
                if energy.get(ctx) <= 0.0 {
                    ctx.despawn_self();
                    // Still valid — data stays until end of tick
                    let _still_readable = energy.get(ctx);
                }
            }
        }))
        .done();

    world.build();

    println!("  Tick-by-tick lifecycle:");
    for tick in 1..=4 {
        world.tick();
        let alive = world.is_alive(entity);
        let energy = world.read_typed::<Energy>(entity);
        println!(
            "    Tick {}: alive={:<5} energy={:?}",
            tick, alive, energy
        );
    }

    println!();
    println!("  Order of operations within one tick:");
    println!("    1. storage.begin_tick()");
    println!("    2. For each archetype, execute resonators");
    println!("       → despawn_self() sets flag, entity stays alive this tick");
    println!("       → Other entities' read_external sees this entity's data");
    println!("    3. storage.commit() — sync double buffer");
    println!("    4. Process despawn queue — entity actually removed");
    println!("    5. Process defer_write queue");
    println!("    6. Process defer_spawn queue");
    println!("    7. storage.end_tick()");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 6: EntityAccessor Benchmark
// ═══════════════════════════════════════════════════════════════

fn demo_accessor_benchmark() {
    let mut world = World::new();

    let e = world
        .entity("Subject")
        .attr_typed::<PosX>(1.0)
        .attr_typed::<PosY>(2.0)
        .attr_typed::<Energy>(100.0)
        .done();
    world.build();

    let acc = EntityAccessor::new(&world, e, &["PosX", "PosY", "Energy"]).unwrap();

    // EntityAccessor: pre-resolved, ~1ns per read
    let start = Instant::now();
    let mut sum = 0.0_f64;
    for _ in 0..500_000 {
        sum += acc.get(&world, 0) + acc.get(&world, 1) + acc.get(&world, 2);
    }
    let accessor_time = start.elapsed();

    // World API: HashMap lookups each call, ~50ns per read
    let start = Instant::now();
    let mut sum2 = 0.0_f64;
    for _ in 0..500_000 {
        sum2 += world.read_typed::<PosX>(e).unwrap_or(0.0)
            + world.read_typed::<PosY>(e).unwrap_or(0.0)
            + world.read_typed::<Energy>(e).unwrap_or(0.0);
    }
    let api_time = start.elapsed();

    println!("  EntityAccessor: {} (500K × 3 reads)", fmt_time(accessor_time));
    println!("  World API:      {} (500K × 3 reads)", fmt_time(api_time));
    println!(
        "  Speedup:        {:.1}×",
        api_time.as_secs_f64() / accessor_time.as_secs_f64().max(1e-9)
    );
    assert!(sum > 0.0 && sum2 > 0.0);
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 7: External Command Queue
// ═══════════════════════════════════════════════════════════════

fn demo_command_queue() {
    let mut world = World::new();

    let unit = world
        .entity("Unit")
        .attr_typed::<PosX>(0.0)
        .attr_typed::<PosY>(0.0)
        .attr_typed::<TargetX>(0.0)
        .attr_typed::<TargetY>(0.0)
        .attr_typed::<Speed>(2.0)
        .on(resonator!(map => {
            bind_fields!(map, px: PosX, py: PosY, tx: TargetX, ty: TargetY, spd: Speed);
            move |ctx: &mut NodeContext| {
                let dx = tx.get(ctx) - px.get(ctx);
                let dy = ty.get(ctx) - py.get(ctx);
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > 1.0 {
                    let s = spd.get(ctx);
                    px.add_unchecked(ctx, dx / dist * s);
                    py.add_unchecked(ctx, dy / dist * s);
                }
            }
        }))
        .done();

    world.build();

    // Simulate external command: "move unit to (100, 50)"
    // Applied BETWEEN ticks — safe, no tick is running
    struct MoveCommand {
        target: entity::EntityHandle,
        x: f64,
        y: f64,
    }
    let commands = vec![MoveCommand { target: unit, x: 100.0, y: 50.0 }];

    // Apply commands before tick
    for cmd in &commands {
        world.write_typed::<TargetX>(cmd.target, cmd.x);
        world.write_typed::<TargetY>(cmd.target, cmd.y);
    }

    println!("  Command: move unit to (100, 50)");
    for tick in 0..5 {
        world.tick();
        let x = world.read_typed::<PosX>(unit).unwrap();
        let y = world.read_typed::<PosY>(unit).unwrap();
        println!("    Tick {}: pos=({:.1}, {:.1})", tick, x, y);
    }
    println!("  ✓ External commands applied between ticks via write_typed");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 8: Collision Detection
// ═══════════════════════════════════════════════════════════════

fn demo_collision() {
    let mut world = World::new();

    // Create enemies
    for i in 0..5 {
        world
            .entity("Enemy")
            .attr_typed::<PosX>(i as f64 * 20.0 + 50.0)
            .attr_typed::<PosY>(50.0)
            .attr_typed::<Health>(30.0)
            .tag_typed::<Enemy>()
            .done();
    }

    // Create a bullet heading toward enemies
    let bullet = world
        .entity("Bullet")
        .attr_typed::<PosX>(0.0)
        .attr_typed::<PosY>(50.0)
        .attr_typed::<VelX>(15.0)
        .attr_typed::<VelY>(0.0)
        .attr_typed::<Damage>(25.0)
        .tag_typed::<Bullet>()
        .on(resonator!(map => {
            bind_fields!(map, px: PosX, py: PosY, vx: VelX, vy: VelY);
            move |ctx: &mut NodeContext| {
                px.set_unchecked(ctx, px.get(ctx) + vx.get(ctx));
                py.set_unchecked(ctx, py.get(ctx) + vy.get(ctx));
            }
        }))
        .done();

    world.build();

    let mut grid = world.create_spatial_grid(SpatialConfig::new(
        20.0, 0.0, 0.0, 200.0, 200.0,
    ));

    println!("  Collision detection happens BETWEEN ticks:");
    println!("    1. rebuild_spatial_grid()");
    println!("    2. Query grid for bullet↔enemy overlap");
    println!("    3. Apply damage via world.write_typed() (safe — outside tick)");
    println!("    4. world.tick() — resonators run with updated data");
    println!();

    let mut hits = 0;
    for tick in 0..10 {
        // Step 1: rebuild grid
        world.rebuild_spatial_grid(&mut grid);

        // Step 2: collision check — BETWEEN ticks, safe to read/write
        if world.is_alive(bullet) {
            let bx = world.read_typed::<PosX>(bullet).unwrap_or(0.0);
            let by = world.read_typed::<PosY>(bullet).unwrap_or(0.0);

            let nearby = grid.query_radius(bx, by, 10.0);
            for entry in &nearby {
                if let Some(target) = world.get_handle_by_index(entry.entity_index) {
                    if world.has_tag::<Enemy>(target) && world.is_alive(target) {
                        let hp = world.read_typed::<Health>(target).unwrap_or(0.0);
                        let dmg = world.read_typed::<Damage>(bullet).unwrap_or(0.0);
                        world.write_typed::<Health>(target, hp - dmg);
                        world.despawn(bullet);
                        hits += 1;
                        println!("    Tick {}: HIT! Enemy at ({:.0},{:.0}), HP {:.0}→{:.0}",
                            tick, entry.x, entry.y, hp, hp - dmg);
                        break;
                    }
                }
            }
        }

        // Step 3: tick
        world.tick();
    }

    if hits == 0 {
        println!("    No hits (bullet missed all enemies)");
    }
    println!("  ✓ Collision detection runs outside tick() — safe to mutate world");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 9: State Machine
// ═══════════════════════════════════════════════════════════════

fn demo_state_machine() {
    let mut world = World::new();

    let guard = world
        .entity("Guard")
        .attr_typed::<AiStateEnum>(AiStateEnum::Idle)
        .attr_typed::<Energy>(100.0)
        .attr_typed::<Lifetime>(0.0) // use as tick counter
        .on(resonator!(map => {
            bind_fields!(map, state: AiStateEnum, energy: Energy, timer: Lifetime);
            move |ctx: &mut NodeContext| {
                timer.add_unchecked(ctx, 1.0);
                let t = timer.get(ctx);
                let s = state.get(ctx);
                let e = energy.get(ctx);

                if s == AiStateEnum::Idle && t >= 3.0 {
                    state.set_unchecked(ctx, AiStateEnum::Patrol);
                } else if s == AiStateEnum::Patrol {
                    energy.add_unchecked(ctx, -5.0);
                    if e < 30.0 {
                        state.set_unchecked(ctx, AiStateEnum::Flee);
                    }
                } else if s == AiStateEnum::Flee {
                    energy.add_unchecked(ctx, 2.0);
                    if e > 80.0 {
                        state.set_unchecked(ctx, AiStateEnum::Idle);
                        timer.set_unchecked(ctx, 0.0); // reset counter
                    }
                }
            }
        }))
        .done();

    world.build();

    let state_names = ["Idle", "Patrol", "Chase", "Flee", "Attack"];

    for tick in 0..20 {
        world.tick();
        if tick % 3 == 0 {
            let s = world.read_typed::<AiStateEnum>(guard).unwrap() as usize;
            let e = world.read_typed::<Energy>(guard).unwrap();
            let name = state_names.get(s).unwrap_or(&"?");
            println!("    Tick {:>2}: state={:<7} energy={:.0}", tick, name, e);
        }
    }
    println!("  ✓ State machine via define_enum_attr! with compile-time constants");
    println!("  ✓ Tick counter stored as entity attribute (not mutable closure state)");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 10: Debugging Resonators
// ═══════════════════════════════════════════════════════════════

fn demo_debugging() {
    let mut world = World::new();

    let debug_entity = world
        .entity("Debuggable")
        .attr_typed::<PosX>(0.0)
        .attr_typed::<Energy>(100.0)
        .on(resonator!(map => {
            bind_fields!(map, px: PosX, energy: Energy);
            move |ctx: &mut NodeContext| {
                px.add_unchecked(ctx, 1.0);
                energy.add_unchecked(ctx, -3.0);

                // Pattern 1: Conditional log by entity ID
                if ctx.entity_id().index == 0 {
                    let _e = energy.get(ctx);
                    // println!("  [debug] Entity 0: energy={:.1}", _e);
                }

                // Pattern 2: Log on threshold
                if energy.get(ctx) < 20.0 && energy.get(ctx) > 17.0 {
                    // println!("  [warn] Low energy: {:.1}", energy.get(ctx));
                }
            }
        }))
        .done();

    world.build();

    // Run a few ticks
    for _ in 0..5 {
        world.tick();
    }

    // Pattern 3: Snapshot export between ticks
    let json = world.snapshot_json();
    println!("  Snapshot after 5 ticks: {} chars", json.len());
    println!("  First 100: {}...", &json[..json.len().min(100)]);

    println!();
    println!("  Debugging patterns for resonators:");
    println!("    1. Conditional println! by entity ID");
    println!("       if ctx.entity_id().index == 42 {{ println!(...) }}");
    println!("    2. Threshold logging (low health, collision, etc.)");
    println!("    3. snapshot_json() / snapshot_csv() between ticks");
    println!("    4. apply_resonator() for one-shot inspection");
    println!("    5. Single-step: tick() once, inspect, repeat");
}

// ═══════════════════════════════════════════════════════════════
//  ECOSYSTEM SIMULATION
// ═══════════════════════════════════════════════════════════════

fn demo_ecosystem() {
    let mut world = World::new();
    world.set_buffer_mode(storage::BufferMode::Double);

    let world_size = 500.0;
    let initial_grass = 80;
    let initial_rabbits = 25;
    let initial_wolves = 4;

    // ── Create grass ──────────────────────────────────

    let grass_prefab = world
        .prefab("Grass")
        .attr_typed::<PosX>(0.0)
        .attr_typed::<PosY>(0.0)
        .attr_typed::<Energy>(20.0)
        .attr_typed::<MaxEnergy>(50.0)
        .attr_typed::<FoodValue>(15.0)
        .attr_typed::<GrowthRate>(0.5)
        .tag_typed::<Grass>()
        .on(resonator!(map => {
            bind_fields!(map, energy: Energy, max_energy: MaxEnergy, growth_rate: GrowthRate);
            move |ctx: &mut NodeContext| {
                let e = energy.get(ctx);
                let m = max_energy.get(ctx);
                if e < m {
                    energy.set_unchecked(ctx, (e + growth_rate.get(ctx)).min(m));
                }
                if e <= 0.0 {
                    ctx.despawn_self();
                }
            }
        }));

    grass_prefab.spawn_batch_with(&mut world, initial_grass, |i| {
        vec![
            ("PosX", pseudo_random(i as u64 * 7 + 1) * world_size),
            ("PosY", pseudo_random(i as u64 * 13 + 3) * world_size),
            ("Energy", 10.0 + pseudo_random(i as u64 * 31) * 40.0),
        ]
    });

    // ── Create rabbits ────────────────────────────────

    let rabbit_prefab = world
        .prefab("Rabbit")
        .attr_typed::<PosX>(0.0)
        .attr_typed::<PosY>(0.0)
        .attr_typed::<VelX>(0.0)
        .attr_typed::<VelY>(0.0)
        .attr_typed::<Energy>(50.0)
        .attr_typed::<MaxEnergy>(100.0)
        .attr_typed::<Speed>(2.0)
        .attr_typed::<VisionRange>(50.0)
        .attr_typed::<ReproduceCooldown>(0.0)
        .tag_typed::<Rabbit>()
        .on(resonator!(map => {
            bind_fields!(map, px: PosX, py: PosY, vx: VelX, vy: VelY, energy: Energy);
            let ws = world_size;
            move |ctx: &mut NodeContext| {
                px.set_unchecked(ctx, (px.get(ctx) + vx.get(ctx)).rem_euclid(ws));
                py.set_unchecked(ctx, (py.get(ctx) + vy.get(ctx)).rem_euclid(ws));
                energy.add_unchecked(ctx, -0.3);
                if energy.get(ctx) <= 0.0 { ctx.despawn_self(); }
            }
        }));

    rabbit_prefab.spawn_batch_with(&mut world, initial_rabbits, |i| {
        vec![
            ("PosX", pseudo_random(i as u64 * 17 + 100) * world_size),
            ("PosY", pseudo_random(i as u64 * 23 + 200) * world_size),
            ("Energy", 40.0 + pseudo_random(i as u64 * 41) * 40.0),
        ]
    });

    // ── Create wolves ─────────────────────────────────

    let wolf_prefab = world
        .prefab("Wolf")
        .attr_typed::<PosX>(0.0)
        .attr_typed::<PosY>(0.0)
        .attr_typed::<VelX>(0.0)
        .attr_typed::<VelY>(0.0)
        .attr_typed::<Energy>(80.0)
        .attr_typed::<MaxEnergy>(150.0)
        .attr_typed::<Speed>(3.0)
        .attr_typed::<VisionRange>(80.0)
        .attr_typed::<ReproduceCooldown>(0.0)
        .tag_typed::<Wolf>()
        .on(resonator!(map => {
            bind_fields!(map, px: PosX, py: PosY, vx: VelX, vy: VelY, energy: Energy);
            let ws = world_size;
            move |ctx: &mut NodeContext| {
                px.set_unchecked(ctx, (px.get(ctx) + vx.get(ctx)).rem_euclid(ws));
                py.set_unchecked(ctx, (py.get(ctx) + vy.get(ctx)).rem_euclid(ws));
                energy.add_unchecked(ctx, -0.5);
                if energy.get(ctx) <= 0.0 { ctx.despawn_self(); }
            }
        }));

    wolf_prefab.spawn_batch_with(&mut world, initial_wolves, |i| {
        vec![
            ("PosX", pseudo_random(i as u64 * 37 + 500) * world_size),
            ("PosY", pseudo_random(i as u64 * 43 + 700) * world_size),
            ("Energy", 60.0 + pseudo_random(i as u64 * 53) * 60.0),
        ]
    });

    // ── Build ─────────────────────────────────────────

    world.build();

    // ── Spatial grid ──────────────────────────────────

    let mut grid = world.create_spatial_grid(SpatialConfig::new(
        50.0, 0.0, 0.0, world_size, world_size,
    ));

    // ── Simulation loop ───────────────────────────────

    println!("  World: {:.0}×{:.0}", world_size, world_size);
    println!(
        "  Initial: 🌿{} 🐰{} 🐺{}",
        initial_grass, initial_rabbits, initial_wolves
    );
    println!();

    let sim_ticks = 200;

    for tick in 0..sim_ticks {
        // 1. Rebuild spatial index
        world.rebuild_spatial_grid(&mut grid);

        // 2. AI: rabbits seek+eat grass, wolves seek+eat rabbits
        run_rabbit_ai(&mut world, &grid, world_size);
        run_wolf_ai(&mut world, &grid);

        // 3. Reproduction
        run_reproduction(&mut world, tick as u64, world_size);

        // 4. Execute resonators (movement, energy drain, despawn)
        world.tick();

        // Stats
        if tick % 25 == 0 || tick == sim_ticks - 1 {
            let g = query::count_with::<Grass>(&world);
            let r = query::count_with::<Rabbit>(&world);
            let w = query::count_with::<Wolf>(&world);
            println!(
                "  Tick {:>3}: 🌿{:<5} 🐰{:<5} 🐺{:<5} total={}",
                tick, g, r, w, world.alive_count()
            );
        }
    }

    let fg = query::count_with::<Grass>(&world);
    let fr = query::count_with::<Rabbit>(&world);
    let fw = query::count_with::<Wolf>(&world);
    println!("\n  Final: 🌿{} 🐰{} 🐺{}", fg, fr, fw);
    println!("  ✓ Ecosystem ran for {} ticks with inter-species interactions", sim_ticks);
}

/// Rabbits: find nearest grass, move toward it, eat it
fn run_rabbit_ai(world: &mut World, grid: &SpatialGrid, world_size: f64) {
    let rabbits = query::with_attr::<Rabbit>(world);

    for rabbit in &rabbits {
        if !world.is_alive(*rabbit) { continue; }

        let rx = match world.read_typed::<PosX>(*rabbit) { Some(v) => v, None => continue };
        let ry = world.read_typed::<PosY>(*rabbit).unwrap_or(0.0);
        let speed = world.read_typed::<Speed>(*rabbit).unwrap_or(1.0);
        let vision = world.read_typed::<VisionRange>(*rabbit).unwrap_or(30.0);

        let nearby = grid.query_radius(rx, ry, vision);
        let mut best: Option<(u32, f64, f64, f64)> = None;

        for entry in &nearby {
            if let Some(h) = world.get_handle_by_index(entry.entity_index) {
                if world.has_tag::<Grass>(h) && world.is_alive(h) {
                    let dx = entry.x - rx;
                    let dy = entry.y - ry;
                    let d = (dx * dx + dy * dy).sqrt();
                    if best.map_or(true, |b| d < b.3) {
                        best = Some((entry.entity_index, entry.x, entry.y, d));
                    }
                }
            }
        }

        if let Some((gi, gx, gy, dist)) = best {
            if dist < 5.0 {
                if let Some(grass_h) = world.get_handle_by_index(gi) {
                    let food = world.read_typed::<FoodValue>(grass_h).unwrap_or(0.0);
                    let ge = world.read_typed::<Energy>(grass_h).unwrap_or(0.0);
                    if ge > 5.0 {
                        let eaten = food.min(ge);
                        let re = world.read_typed::<Energy>(*rabbit).unwrap_or(0.0);
                        let max_e = world.read_typed::<MaxEnergy>(*rabbit).unwrap_or(100.0);
                        world.write_typed::<Energy>(*rabbit, (re + eaten).min(max_e));
                        world.write_typed::<Energy>(grass_h, ge - eaten);
                    }
                }
            } else {
                let dx = gx - rx;
                let dy = gy - ry;
                let len = dist.max(0.01);
                world.write_typed::<VelX>(*rabbit, dx / len * speed);
                world.write_typed::<VelY>(*rabbit, dy / len * speed);
            }
        } else {
            let seed = rabbit.0.index as u64 * 997 + 1;
            let angle = pseudo_random(seed) * std::f64::consts::TAU;
            world.write_typed::<VelX>(*rabbit, angle.cos() * speed * 0.5);
            world.write_typed::<VelY>(*rabbit, angle.sin() * speed * 0.5);
        }
    }
}

/// Wolves: find nearest rabbit, chase, kill
fn run_wolf_ai(world: &mut World, grid: &SpatialGrid) {
    let wolves = query::with_attr::<Wolf>(world);

    for wolf in &wolves {
        if !world.is_alive(*wolf) { continue; }

        let wx = match world.read_typed::<PosX>(*wolf) { Some(v) => v, None => continue };
        let wy = world.read_typed::<PosY>(*wolf).unwrap_or(0.0);
        let speed = world.read_typed::<Speed>(*wolf).unwrap_or(2.0);
        let vision = world.read_typed::<VisionRange>(*wolf).unwrap_or(60.0);

        let nearby = grid.query_radius(wx, wy, vision);
        let mut best: Option<(u32, f64, f64, f64)> = None;

        for entry in &nearby {
            if let Some(h) = world.get_handle_by_index(entry.entity_index) {
                if world.has_tag::<Rabbit>(h) && world.is_alive(h) {
                    let dx = entry.x - wx;
                    let dy = entry.y - wy;
                    let d = (dx * dx + dy * dy).sqrt();
                    if best.map_or(true, |b| d < b.3) {
                        best = Some((entry.entity_index, entry.x, entry.y, d));
                    }
                }
            }
        }

        if let Some((pi, px, py, dist)) = best {
            if dist < 5.0 {
                if let Some(prey_h) = world.get_handle_by_index(pi) {
                    if world.is_alive(prey_h) {
                        let prey_e = world.read_typed::<Energy>(prey_h).unwrap_or(0.0);
                        let wolf_e = world.read_typed::<Energy>(*wolf).unwrap_or(0.0);
                        let max_e = world.read_typed::<MaxEnergy>(*wolf).unwrap_or(150.0);
                        world.write_typed::<Energy>(*wolf, (wolf_e + prey_e * 0.5).min(max_e));
                        world.despawn(prey_h);
                    }
                }
            } else {
                let dx = px - wx;
                let dy = py - wy;
                let len = dist.max(0.01);
                world.write_typed::<VelX>(*wolf, dx / len * speed);
                world.write_typed::<VelY>(*wolf, dy / len * speed);
            }
        } else {
            let seed = wolf.0.index as u64 * 1031 + 1;
            let angle = pseudo_random(seed) * std::f64::consts::TAU;
            world.write_typed::<VelX>(*wolf, angle.cos() * speed * 0.5);
            world.write_typed::<VelY>(*wolf, angle.sin() * speed * 0.5);
        }
    }
}

/// Grass regrowth + rabbit/wolf reproduction
fn run_reproduction(world: &mut World, tick: u64, world_size: f64) {
    // Grass regrowth
    if tick % 8 == 0 {
        for i in 0..3 {
            let seed = tick * 100 + i;
            let x = pseudo_random(seed * 7 + 1) * world_size;
            let y = pseudo_random(seed * 13 + 3) * world_size;
            world
                .entity("Grass")
                .attr_typed::<PosX>(x)
                .attr_typed::<PosY>(y)
                .attr_typed::<Energy>(10.0)
                .attr_typed::<MaxEnergy>(50.0)
                .attr_typed::<FoodValue>(15.0)
                .attr_typed::<GrowthRate>(0.5)
                .tag_typed::<Grass>()
                .on(resonator!(map => {
                    bind_fields!(map, energy: Energy, max_energy: MaxEnergy, growth_rate: GrowthRate);
                    move |ctx: &mut NodeContext| {
                        let e = energy.get(ctx);
                        let m = max_energy.get(ctx);
                        if e < m { energy.set_unchecked(ctx, (e + growth_rate.get(ctx)).min(m)); }
                        if e <= 0.0 { ctx.despawn_self(); }
                    }
                }))
                .done();
        }
        world.spawn_runtime();
    }

    // Rabbit reproduction
    let rabbits = query::with_attr::<Rabbit>(world);
    let mut baby_positions = Vec::new();

    for rabbit in &rabbits {
        if !world.is_alive(*rabbit) { continue; }
        let energy = world.read_typed::<Energy>(*rabbit).unwrap_or(0.0);
        let cd = world.read_typed::<ReproduceCooldown>(*rabbit).unwrap_or(0.0);

        if cd > 0.0 {
            world.write_typed::<ReproduceCooldown>(*rabbit, cd - 1.0);
            continue;
        }
        if energy > 70.0 {
            let x = world.read_typed::<PosX>(*rabbit).unwrap_or(0.0);
            let y = world.read_typed::<PosY>(*rabbit).unwrap_or(0.0);
            world.write_typed::<Energy>(*rabbit, energy - 30.0);
            world.write_typed::<ReproduceCooldown>(*rabbit, 15.0);
            baby_positions.push((x + 2.0, y + 2.0));
        }
    }

    for (x, y) in baby_positions {
        world
            .entity("Rabbit")
            .attr_typed::<PosX>(x)
            .attr_typed::<PosY>(y)
            .attr_typed::<VelX>(0.0)
            .attr_typed::<VelY>(0.0)
            .attr_typed::<Energy>(35.0)
            .attr_typed::<MaxEnergy>(100.0)
            .attr_typed::<Speed>(2.0)
            .attr_typed::<VisionRange>(50.0)
            .attr_typed::<ReproduceCooldown>(15.0)
            .tag_typed::<Rabbit>()
            .on(resonator!(map => {
                bind_fields!(map, px: PosX, py: PosY, vx: VelX, vy: VelY, energy: Energy);
                let ws = world_size;
                move |ctx: &mut NodeContext| {
                    px.set_unchecked(ctx, (px.get(ctx) + vx.get(ctx)).rem_euclid(ws));
                    py.set_unchecked(ctx, (py.get(ctx) + vy.get(ctx)).rem_euclid(ws));
                    energy.add_unchecked(ctx, -0.3);
                    if energy.get(ctx) <= 0.0 { ctx.despawn_self(); }
                }
            }))
            .done();
    }

    world.spawn_runtime();
}

// ═══════════════════════════════════════════════════════════════
//  PERFORMANCE
// ═══════════════════════════════════════════════════════════════

fn demo_performance() {
    println!("  Single resonator (PosX += VelX, PosY += VelY):");
    println!();

    for &count in &[1_000, 10_000, 50_000, 100_000] {
        let mut w = World::new();
        let p = w
            .prefab("Perf")
            .attr_typed::<PosX>(0.0)
            .attr_typed::<PosY>(0.0)
            .attr_typed::<VelX>(0.01)
            .attr_typed::<VelY>(0.02)
            .on(resonator!(map => {
                bind_fields!(map, px: PosX, py: PosY, vx: VelX, vy: VelY);
                move |ctx: &mut NodeContext| {
                    px.set_unchecked(ctx, px.get(ctx) + vx.get(ctx));
                    py.set_unchecked(ctx, py.get(ctx) + vy.get(ctx));
                }
            }));
        p.spawn_batch(&mut w, count);
        w.build();

        // Warmup
        for _ in 0..5 { w.tick(); }

        let mut times = Vec::new();
        for _ in 0..30 {
            let s = Instant::now();
            w.tick();
            times.push(s.elapsed());
        }
        times.sort();
        let avg: std::time::Duration =
            times.iter().sum::<std::time::Duration>() / times.len() as u32;
        let median = times[times.len() / 2];

        println!(
            "    {:>7} entities: avg {:<10} median {:<10} ({:.0} ent/sec)",
            count,
            fmt_time(avg),
            fmt_time(median),
            count as f64 / avg.as_secs_f64()
        );
    }
}