# Resonance Engine v10.3

A data-oriented simulation framework in Rust.

Stores entity data in flat arrays. Groups entities by attribute set.
Runs update functions in tight, cache-friendly loops.

Works for: physics sims, agent models, ecosystems, games, economics, science.

---

## 30-Second Example

```rust
use resonance_engine::*;

define_attrs!(PosX, VelX);

fn main() {
    let mut world = World::new();
    
    let p = world.prefab("Particle")
        .attr_typed::<PosX>(0.0)
        .attr_typed::<VelX>(1.0)
        .on(resonator!(map => {
            bind_fields!(map, px: PosX, vx: VelX);
            move |ctx: &mut NodeContext| {
                px.set_unchecked(ctx, px.get(ctx) + vx.get(ctx));
            }
        }));

    p.spawn_batch(&mut world, 10_000);
    world.build();

    for _ in 0..60 { world.tick(); }
}
```

---

## Core Concepts

**Entity** — an ID with f64 attributes. No behavior.

**Attribute** — a named f64 value. Declared at compile time for type safety:
```rust
define_attrs!(Health, PosX, VelX);
define_tags!(Player, Enemy);  // tags = attributes with value 1.0
```

**Archetype** — entities with the same attribute set, stored together. Automatic.

**Resonator** — a function that runs every tick for every entity in an archetype.

**World** — owns all data, runs all ticks.

---

## Lifecycle

```
1. world.entity("Name").attr_typed::<A>(v).on(resonator).done()
2. world.build()
3. loop {
       // external logic here (AI, collision, commands)
       world.tick();
   }
```

`build()` finalizes pending entities into archetypes.
After `build()`, add more entities with `spawn_runtime()` if archetype exists.

---

## Data Access — Five Levels

| Level | API | Speed | Use when |
|-------|-----|-------|----------|
| 1 | `BoundField<A>` | ~1 ns | Inside resonators |
| 2 | `AttrOffset` + `read_at` | ~1 ns | External hot loop, 1 attr |
| 3 | `EntityAccessor` | ~1 ns | External hot loop, N attrs |
| 4 | `EntityRef` | ~5 ns | External convenience |
| 5 | `world.read_typed::<A>()` | ~50 ns | Init, UI, debug |

### Level 1: BoundField (inside resonators)
```rust
.on(resonator!(map => {
    bind_fields!(map, hp: Health);
    move |ctx: &mut NodeContext| {
        hp.set_unchecked(ctx, hp.get(ctx) - 1.0);
    }
}))
```

### Level 3: EntityAccessor (external hot loop)
```rust
let acc = EntityAccessor::new(&world, e, &["PosX", "PosY"]).unwrap();
for _ in 0..1_000_000 {
    let x = acc.get(&world, 0);
}
```

### Level 5: World API (simple)
```rust
let hp = world.read_typed::<Health>(entity);  // Option<f64>
world.write_typed::<Health>(entity, 50.0);    // bool
```

---

## set vs set_unchecked

| Method | Behavior | Use when |
|--------|----------|----------|
| `set(field, value)` | Skips write if `|old - new| < 1e-9`. Only marks dirty on change. | Values that rarely change |
| `set_unchecked(field, value)` | Always writes. Always marks dirty. | Values that always change (positions) |
| `add_unchecked(field, delta)` | `= set_unchecked(f, get(f) + delta)` | Accumulators |

**"Unchecked" = no epsilon comparison.** NOT "no bounds check". Bounds are `debug_assert!`'d. No UB.

---

## Reading Other Entities

Resonators can read other entities' data via pre-resolved offsets.

```rust
// Before tick:
let target_hp = world.absolute_offset::<Health>(target).unwrap();

// In resonator:
let hp = ctx.read_external(target_hp);  // reads from snapshot (double buffer)
```

**Rules:**
- Read-only. Cannot write to other entities this way.
- Double buffer: reads previous tick's snapshot → deterministic.
- Single buffer: reads current data → may include this tick's writes.
- Offsets come from `absolute_offset` or `EntityAccessor::resolve`. Never fabricate.

**Stale offsets:** if target was despawned, the offset points to memory with the
last written values (not garbage, not UB). But the data is logically invalid.
Re-resolve offsets after despawn cycles, or check `is_alive()` before the tick.

---

## Writing to Other Entities: defer_write

Resonators cannot directly mutate other entities. They push deferred commands:

```rust
ctx.defer_write(target_hp_offset, new_value);
```

Applied after all resonators finish. All resonators see the same snapshot.

---

## Spawning from Resonators: defer_spawn

```rust
// Once, before simulation:
world.register_spawnable("Bullet", &["PosX", "VelX", "Damage"], &[0.0, 0.0, 10.0]);

// In resonator:
ctx.defer_spawn("Bullet", vec![("PosX".into(), my_x), ("VelX".into(), 10.0)]);
```

`register_spawnable` does NOT create an archetype. You must first create at
least one entity of that type via `world.entity().done()` + `build()`. The
attribute list must match the archetype's schema exactly.

Entity is created after the tick.

---

## Despawn Lifecycle

```
Tick execution order:
1. storage.begin_tick()
2. Execute resonators for all archetypes
   → despawn_self() sets flag — entity stays alive until step 4
   → Other entities see this entity's data via read_external
   → All remaining resonators for this entity still execute
3. storage.commit() — sync double buffer
4. Process despawn queue — entities removed
5. Process defer_write queue
6. Process defer_spawn queue
7. storage.end_tick()
```

Key: `despawn_self()` is deferred. Data valid through the entire tick.

---

## Entity Handle Safety

Handles have generational indices. Old handles become invalid after despawn + slot reuse:

```rust
world.despawn(a);
let b = world.entity("B").attr_typed::<Health>(999.0).done();
world.build();
world.read_typed::<Health>(a)  // → None (stale generation)
world.is_alive(a)              // → false
```

All APIs check generation: `read_typed`, `write_typed`, `EntityAccessor`, `EntityRef`, etc.

---

## Prefabs

```rust
let bullet = world.prefab("Bullet")
    .attr_typed::<PosX>(0.0)
    .attr_typed::<Damage>(10.0)
    .on(resonator!(map => { ... }));

bullet.spawn_batch(&mut world, 1000);
```

---

## Buffer Modes

| Mode | Speed | Determinism | When |
|------|-------|-------------|------|
| `Single` | Fastest | No | Entities don't read each other |
| `Double` | ~1.5× memory | Full | N-body, agent interactions |

```rust
world.set_buffer_mode(BufferMode::Double);
```

---

## Phases

Control execution order:

```rust
world.add_phase("forces");
world.add_phase("integrate");
world.assign_phase("ForceArch", "forces");     // after build
world.assign_phase("PhysicsArch", "integrate");
```

Within a phase: parallel (rayon). Between phases: barrier.

**Limitation:** phases are per-archetype, not per-resonator. If an archetype has
resonators for different logical phases, split into separate archetypes or handle
ordering outside `tick()`.

**For complex multi-phase systems** — consider running logic outside resonators:

```rust
loop {
    compute_forces(&mut world);   // external, uses world.write_typed
    world.tick();                 // resonators do integration
}
```

---

## Spatial Grid

```rust
let mut grid = world.create_spatial_grid(SpatialConfig::new(50.0, 0.0, 0.0, 1000.0, 1000.0));

// Each tick:
world.rebuild_spatial_grid(&mut grid);  // MUST call before queries
let nearby = grid.query_radius(x, y, radius);
```

If you forget `rebuild_spatial_grid()`, queries return stale positions from the
last rebuild. No crash — just old data. Grid does not auto-update.

---

## Thread Safety

**Engine guarantees:**
- Different archetypes in same phase run in parallel safely
- `read_external` reads read-only snapshot (double buffer) — no races
- `defer_*` commands collected per-thread, merged after parallel work

**You must guarantee:**
- No `&mut World` or shared mutable state captured in resonators
- No `world.write_typed()` during tick (debug assert fires)
- Resonator closures are `Send + Sync`

**Forbidden:** `Mutex`/`RwLock` in resonators (deadlock risk with rayon).

---

## Common Patterns

### Cross-Archetype Interaction (Predator eats Prey)

```rust
// Between ticks — safe to read/write world:
fn run_wolf_ai(world: &mut World, grid: &SpatialGrid) {
    let wolves = query::with_attr::<Wolf>(world);
    for wolf in &wolves {
        let (wx, wy) = /* read wolf position */;
        let nearby = grid.query_radius(wx, wy, vision);
        for entry in &nearby {
            if let Some(prey) = world.get_handle_by_index(entry.entity_index) {
                if world.has_tag::<Rabbit>(prey) {
                    world.write_typed::<Energy>(*wolf, /* gain */);
                    world.despawn(prey);
                    break;
                }
            }
        }
    }
}

// Main loop:
loop {
    world.rebuild_spatial_grid(&mut grid);
    run_wolf_ai(&mut world, &grid);
    world.tick();
}
```

### External Command Queue (UI / Network)

```rust
// Collect commands between frames (UI thread, network, etc.)
let commands: Vec<(EntityHandle, f64, f64)> = /* from input */;

// Apply BETWEEN ticks — safe:
for (unit, x, y) in &commands {
    world.write_typed::<TargetX>(*unit, *x);
    world.write_typed::<TargetY>(*unit, *y);
}
world.tick();
```

### Collision Detection

```rust
// Between ticks:
world.rebuild_spatial_grid(&mut grid);
for bullet in &bullets {
    let (bx, by) = /* read position */;
    let nearby = grid.query_radius(bx, by, hit_radius);
    for entry in &nearby {
        if let Some(enemy) = world.get_handle_by_index(entry.entity_index) {
            if world.has_tag::<Enemy>(enemy) {
                world.write_typed::<Health>(enemy, hp - damage);
                world.despawn(*bullet);
                break;
            }
        }
    }
}
world.tick();
```

### State Machine

```rust
define_enum_attr!(State => Idle, Patrol, Chase, Flee);

.on(resonator!(map => {
    bind_fields!(map, state: State, energy: Energy);
    move |ctx: &mut NodeContext| {
        let s = state.get(ctx);
        if s == State::Idle { /* ... */ state.set_unchecked(ctx, State::Patrol); }
        else if s == State::Patrol { /* ... */ }
    }
}))
```

### Delayed Actions (Timer)

```rust
define_attrs!(CastTimer);

.on(resonator!(map => {
    bind_fields!(map, timer: CastTimer);
    move |ctx: &mut NodeContext| {
        if timer.get(ctx) > 0.0 {
            timer.add_unchecked(ctx, -1.0);
            if timer.get(ctx) <= 0.0 {
                // Timer expired — apply effect
                ctx.defer_write(target_offset, damage_value);
            }
        }
    }
}))
```

### Parent-Child Hierarchy

```rust
// Store parent's base offset as an attribute
define_attrs!(ParentBase);

// On child creation:
let parent_base = world.entity_ref(parent).unwrap().base_offset();
child.attr_typed::<ParentBase>(parent_base as f64);

// In child's resonator:
let parent_px_field = map.field_index("PosX").unwrap();
// ...
let base = parent_base_field.get(ctx) as usize;
let parent_x = ctx.read_external_field(base, parent_px_field);
px.set_unchecked(ctx, parent_x + offset_x);
```

`base_offset()` is the entity's absolute index in the global float buffer.
`read_external_field(base, field_index)` reads `base + field_index` from the
read buffer. If parent despawns, data at that offset is stale (last values, no UB).

---

## Debugging Resonators

```rust
// 1. Conditional log by entity ID
if ctx.entity_id().index == 42 {
    println!("Entity 42: hp={:.1}", hp.get(ctx));
}

// 2. Threshold log
if energy.get(ctx) < 10.0 {
    println!("Low energy: entity {}", ctx.entity_id());
}

// 3. Snapshot between ticks
if tick % 10 == 0 {
    std::fs::write("debug.json", world.snapshot_json()).ok();
}

// 4. One-shot inspection
world.apply_resonator(entity, &inspector_resonator);

// 5. Single-step: call tick() once, inspect, repeat
```

---

## Error Handling

| Operation | Invalid input | Result |
|-----------|--------------|--------|
| `read_typed(dead_entity)` | Stale generation | `None` |
| `write_typed(dead_entity, v)` | Stale generation | `false`, no-op |
| `EntityAccessor::new(dead)` | Stale generation | `None` |
| `absolute_offset(dead)` | Stale generation | `None` |
| `acc.get(world, 999)` | Out of bounds | **Panic** |
| `spawn_batch(0)` | Zero count | Empty vec, no-op |
| `despawn(already_dead)` | Already despawned | No-op |
| `write_abs during tick` | Tick active | **Debug assert** |
| `read_external(stale_offset)` | Despawned target | Stale data (last values, no UB) |
| `defer_spawn("Unknown", ...)` | Unregistered | `eprintln` warning, no-op |

---

## Architecture Limitations

| Limitation | Why | Workaround |
|-----------|-----|------------|
| No archetype migration | SoA requires fixed schemas | Include all attrs upfront, or despawn+recreate |
| No multi-attribute queries | Single-attr filter | Chain queries or manual iteration |
| f64-only attributes | Uniform arrays | `define_int_attr!`/`define_bool_attr!` wrappers |
| Phases per-archetype | Simple scheduler | Split archetypes or external logic |

---

## FAQ

**Q: Can I call `world.tick()` from inside a resonator?**
No. Borrow checker prevents it. Resonators operate on `NodeContext`, not `World`.

**Q: Can I capture `&World` in a resonator?**
No. `&mut World` is borrowed by the tick loop.

**Q: How to communicate between archetypes?**
Use `defer_write` in resonators, or handle logic outside `tick()` (between ticks).

**Q: Multiple World instances?**
Yes. Completely independent. Useful for prediction/rollback.

**Q: Save/load state?**
`snapshot_json()` for debug. For production: iterate archetypes, export manually.
No built-in deserialization yet.

**Q: Why no dynamic add/remove components?**
It breaks SoA memory layout — the core perf optimization. Other ECS support it
via archetype migration, but it's expensive. Resonance chose speed + simplicity.

**Q: What if I need complex multi-phase pipelines?**
Run logic outside resonators between ticks. Use `tick()` only for per-entity
updates that benefit from batch execution.

---

## Performance

Single thread, release mode, LTO:

| Entities | Attrs | Avg tick | Throughput |
|----------|-------|---------|------------|
| 1,000 | 4 | ~5 µs | ~200M/sec |
| 10,000 | 4 | ~40 µs | ~250M/sec |
| 50,000 | 4 | ~300 µs | ~170M/sec |
| 100,000 | 4 | ~550 µs | ~180M/sec |
```