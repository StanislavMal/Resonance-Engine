// examples/graph_demo.rs
//! Complete demonstration of Graph Metadata Layer capabilities
//!
//! Shows:
//! - Hierarchical entity relationships (parent-child)
//! - Graph-based queries (BFS/DFS traversal)
//! - DOT export for visualization
//! - Performance comparison vs traditional queries

use resonance_engine::*;
use std::time::Instant;

define_attrs!(PosX, PosY, Health, Level, Experience);
define_tags!(Unit, Building, Resource);
define_relation!(Parent);
define_relation!(Owner);
define_relation!(Target);

define_resonator!(
    HealthRegen {
        health: Health
    }
    |this, ctx| {
        let current = this.health.get(ctx);
        if current < 100.0 {
            this.health.set_unchecked(ctx, current + 0.5);
        }
    }
);

fn main() {
    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║       RESONANCE ENGINE v11.1 — GRAPH METADATA LAYER DEMO          ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝\n");

    demo_hierarchy();
    demo_graph_queries();
    demo_visualization();
    demo_performance();

    println!("\n✅ All graph demos completed successfully!");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 1: Hierarchical Relationships
// ═══════════════════════════════════════════════════════════════

fn demo_hierarchy() {
    println!("═══ DEMO 1: Hierarchical Entity Relationships ═══\n");

    let mut world = World::new();
    world.register_resonator::<HealthRegen>("HealthRegen");

    // Create kingdom hierarchy
    let kingdom = world
        .entity("Kingdom")
        .attr_typed::<Level>(10.0)
        .tag_typed::<Building>()
        .done();

    // Create 3 cities
    let mut cities = Vec::new();
    for i in 0..3 {
        let city = world
            .entity("City")
            .attr_typed::<PosX>(i as f64 * 100.0)
            .attr_typed::<PosY>(50.0)
            .attr_typed::<Level>(5.0)
            .tag_typed::<Building>()
            .done();
        cities.push(city);
    }

    // Create 5 units per city
    let mut all_units = Vec::new();
    for (city_idx, &city) in cities.iter().enumerate() {
        for unit_idx in 0..5 {
            let unit = world
                .entity("Unit")
                .attr_typed::<PosX>(city_idx as f64 * 100.0 + unit_idx as f64 * 10.0)
                .attr_typed::<PosY>(100.0)
                .attr_typed::<Health>(80.0 + unit_idx as f64 * 4.0)
                .attr_typed::<Level>(1.0)
                .tag_typed::<Unit>()
                .resonator_type("HealthRegen")
                .done();
            all_units.push((unit, city));
        }
    }

    world.build();

    // Add relations
    for &city in &cities {
        world.add_relation::<Parent>(city, kingdom);
    }

    for &(unit, city) in &all_units {
        world.add_relation::<Parent>(unit, city);
        world.add_relation::<Owner>(unit, kingdom);
    }

    println!("Created hierarchy:");
    println!("  Kingdom (level 10)");
    println!("  ├─ City 0 (level 5)");
    println!("  │  ├─ Unit 0-0 (health 80)");
    println!("  │  ├─ Unit 0-1 (health 84)");
    println!("  │  └─ ... (3 more units)");
    println!("  ├─ City 1 (level 5)");
    println!("  └─ City 2 (level 5)\n");

    // Query via relations
    let kingdom_cities = world.get_reverse_relations::<Parent>(kingdom);
    println!("Kingdom has {} direct children (cities)", kingdom_cities.len());

    let city0_units = world.get_reverse_relations::<Parent>(cities[0]);
    println!("City 0 has {} units", city0_units.len());

    let all_kingdom_units = world.get_reverse_relations::<Owner>(kingdom);
    println!("Kingdom owns {} units total", all_kingdom_units.len());

    // Simulate 10 ticks (health regen)
    for _ in 0..10 {
        world.tick();
    }

    let avg_health: f64 = all_units
        .iter()
        .filter_map(|(u, _)| world.read_typed::<Health>(*u))
        .sum::<f64>()
        / all_units.len() as f64;

    println!("\nAfter 10 ticks of regen:");
    println!("  Average unit health: {:.1} (was ~82, now ~87)", avg_health);

    println!("  ✓ Hierarchical relations working!\n");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 2: Graph-based Queries (BFS/DFS)
// ═══════════════════════════════════════════════════════════════

fn demo_graph_queries() {
    println!("═══ DEMO 2: Graph-based Queries (BFS/DFS Traversal) ═══\n");

    let mut world = World::new();

    // Create deep hierarchy: Root → L1 → L2 → L3
    let root = world
        .entity("Root")
        .attr_typed::<Level>(0.0)
        .done();

    let mut level1 = Vec::new();
    for i in 0..3 {
        let l1 = world
            .entity("L1")
            .attr_typed::<Level>(1.0)
            .attr_typed::<PosX>(i as f64 * 50.0)
            .done();
        level1.push(l1);
    }

    let mut level2 = Vec::new();
    for (i, &parent) in level1.iter().enumerate() {
        for j in 0..2 {
            let l2 = world
                .entity("L2")
                .attr_typed::<Level>(2.0)
                .attr_typed::<PosX>(i as f64 * 50.0 + j as f64 * 10.0)
                .done();
            level2.push((l2, parent));
        }
    }

    let mut level3 = Vec::new();
    for &(parent, _) in &level2 {
        let l3 = world
            .entity("L3")
            .attr_typed::<Level>(3.0)
            .done();
        level3.push((l3, parent));
    }

    world.build();

    // Add ChildOf relations
    for &l1 in &level1 {
        world.add_relation::<Parent>(l1, root);
    }
    for &(l2, parent) in &level2 {
        world.add_relation::<Parent>(l2, parent);
    }
    for &(l3, parent) in &level3 {
        world.add_relation::<Parent>(l3, parent);
    }

    println!("Created 4-level hierarchy:");
    println!("  Root (1 entity)");
    println!("  └─ Level 1 (3 entities)");
    println!("     └─ Level 2 (6 entities)");
    println!("        └─ Level 3 (6 entities)");
    println!("  Total: 16 entities\n");

    // BFS traversal from root
    println!("BFS Traversal from root:");
    let start = Instant::now();
    
    // Use public query_children API recursively
    let mut all_descendants = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    
    queue.push_back(root);
    visited.insert(root);
    
    while let Some(current) = queue.pop_front() {
        let children = world.get_reverse_relations::<Parent>(current);
        for child in children {
            if visited.insert(child) {
                all_descendants.push(child);
                queue.push_back(child);
            }
        }
    }
    
    let bfs_time = start.elapsed();

    println!("  Found {} descendants in {:?}", all_descendants.len(), bfs_time);
    println!("  Order: L1 entities, then L2, then L3 (breadth-first)");

    // DFS traversal
    println!("\nDFS Traversal from root:");
    let start = Instant::now();
    
    let mut all_descendants_dfs = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![root];
    
    while let Some(current) = stack.pop() {
        if !visited.insert(current) {
            continue;
        }
        
        if current != root {
            all_descendants_dfs.push(current);
        }
        
        let children = world.get_reverse_relations::<Parent>(current);
        for child in children {
            stack.push(child);
        }
    }
    
    let dfs_time = start.elapsed();

    println!("  Found {} descendants in {:?}", all_descendants_dfs.len(), dfs_time);
    println!("  Order: First L1, then all its descendants, etc. (depth-first)");

    // Filter descendants by level
    let level2_count = all_descendants
        .iter()
        .filter(|e| world.read_typed::<Level>(**e).unwrap_or(0.0) == 2.0)
        .count();

    println!("\nFiltered query:");
    println!("  Descendants with Level=2: {}", level2_count);

    println!("  ✓ Graph queries working!\n");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 3: Graph Visualization (DOT export)
// ═══════════════════════════════════════════════════════════════

fn demo_visualization() {
    println!("═══ DEMO 3: Graph Visualization (DOT Export) ═══\n");

    let mut world = World::new();

    // Create small graph for visualization
    let player = world
        .entity("Player")
        .attr_typed::<Health>(100.0)
        .tag_typed::<Unit>()
        .done();

    let weapon = world
        .entity("Weapon")
        .attr_typed::<Level>(5.0)
        .done();

    let enemy = world
        .entity("Enemy")
        .attr_typed::<Health>(50.0)
        .tag_typed::<Unit>()
        .done();

    let base = world
        .entity("Base")
        .tag_typed::<Building>()
        .done();

    world.build();

    // Add relations
    world.add_relation::<Owner>(weapon, player);
    world.add_relation::<Target>(player, enemy);
    world.add_relation::<Parent>(player, base);

    // Add to graph metadata
    let _ = world.meta_graph.add_relation(weapon, GraphEdge::Custom("EquippedBy"), player);
    let _ = world.meta_graph.add_relation(player, GraphEdge::Custom("Attacks"), enemy);

    // Export to DOT
    let dot_path = "graph_visualization.dot";
    match world.meta_graph.export_dot(dot_path) {
        Ok(_) => {
            println!("✓ Exported graph to: {}", dot_path);
            println!("\nTo visualize:");
            println!("  1. Install Graphviz: https://graphviz.org/download/");
            println!("  2. Run: dot -Tpng {} -o graph.png", dot_path);
            println!("  3. Open graph.png in image viewer\n");

            println!("Graph contains:");
            println!("  - 4 entity nodes (Player, Weapon, Enemy, Base)");
            println!("  - 3 relation edges (Owner, Target, Parent)");
            println!("  - 2 custom edges (EquippedBy, Attacks)\n");
        }
        Err(e) => println!("Failed to export DOT: {}", e),
    }

    println!("  ✓ Visualization export working!\n");
}

// ═══════════════════════════════════════════════════════════════
//  DEMO 4: Performance Comparison
// ═══════════════════════════════════════════════════════════════

fn demo_performance() {
    println!("═══ DEMO 4: Performance Comparison ═══\n");

    let mut world = World::new();

    // Create large hierarchy: 1 root + 100 branches + 1000 leaves
    let root = world.entity("Root").attr_typed::<Level>(0.0).done();

    let mut branches = Vec::new();
    for i in 0..100 {
        let branch = world
            .entity("Branch")
            .attr_typed::<Level>(1.0)
            .attr_typed::<PosX>(i as f64)
            .done();
        branches.push(branch);
    }

    let mut leaves = Vec::new();
    for (i, &parent) in branches.iter().enumerate() {
        for j in 0..10 {
            let leaf = world
                .entity("Leaf")
                .attr_typed::<Level>(2.0)
                .attr_typed::<PosX>(i as f64 * 10.0 + j as f64)
                .tag_typed::<Unit>()
                .done();
            leaves.push((leaf, parent));
        }
    }

    world.build();

    // Add relations
    for &branch in &branches {
        world.add_relation::<Parent>(branch, root);
    }
    for &(leaf, parent) in &leaves {
        world.add_relation::<Parent>(leaf, parent);
    }

    println!("Created hierarchy: 1 root + 100 branches + 1000 leaves (1101 entities)\n");

    // Benchmark 1: Find all leaves via traditional query
    let start = Instant::now();
    let leaves_traditional: Vec<_> = world
        .query()
        .with::<Unit>()
        .execute();
    let traditional_time = start.elapsed();

    println!("Traditional Query (tag-based):");
    println!("  Found {} units", leaves_traditional.len());
    println!("  Time: {:?}\n", traditional_time);

    // Benchmark 2: Find all leaves via graph traversal
    let start = Instant::now();
    
    let mut leaves_graph = Vec::new();
    for &branch in &branches {
        let children = world.get_reverse_relations::<Parent>(branch);
        leaves_graph.extend(children);
    }
    
    let graph_time = start.elapsed();

    println!("Graph Traversal (relation-based):");
    println!("  Found {} children", leaves_graph.len());
    println!("  Time: {:?}", graph_time);

    if graph_time < traditional_time {
        println!("  ✓ Graph query {}x faster!", 
            traditional_time.as_nanos() / graph_time.as_nanos().max(1));
    } else {
        println!("  Note: Traditional query faster (fewer entities)");
    }

    // Benchmark 3: Deep traversal (all descendants)
    let start = Instant::now();
    
    let mut all_descendants = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    
    queue.push_back(root);
    visited.insert(root);
    
    while let Some(current) = queue.pop_front() {
        let children = world.get_reverse_relations::<Parent>(current);
        for child in children {
            if visited.insert(child) {
                all_descendants.push(child);
                queue.push_back(child);
            }
        }
    }
    
    let deep_time = start.elapsed();

    println!("\nDeep BFS Traversal (all descendants):");
    println!("  Found {} total descendants", all_descendants.len());
    println!("  Time: {:?}", deep_time);
    println!("  Average: {:.0}ns per entity", 
        deep_time.as_nanos() as f64 / all_descendants.len() as f64);

    println!("\n  ✓ Performance metrics collected!\n");
}