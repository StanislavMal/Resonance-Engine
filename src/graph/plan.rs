// src/graph/plan.rs (строки 1-20, добавить импорты)

use crate::archetype::ArchetypeId;
use crate::graph::{GraphError, GraphNode, MetaGraph};
use crate::resonator::DynResonator;
use petgraph::algo::toposort;
use petgraph::graph::NodeIndex;
use std::any::TypeId;
use std::collections::HashSet;
use std::sync::Arc;

/// Compiled execution plan (output of graph analysis)
#[derive(Clone)]
pub struct ExecutionPlan {
    /// Phases ordered by dependencies
    pub phases: Vec<ExecutionPhase>,
}

#[derive(Clone)]
pub struct ExecutionPhase {
    /// Systems in this phase (can run in parallel)
    pub systems: Vec<SystemExecution>,

    /// Read-only component access
    pub reads: Vec<TypeId>,

    /// Mutable component access
    pub writes: Vec<TypeId>,
}

#[derive(Clone)]
pub struct SystemExecution {
    pub archetype: ArchetypeId,
    pub resonator: Arc<DynResonator>,
    pub name: String,
}

impl MetaGraph {
    /// Analyze system dependencies → build execution plan
    pub fn compile_execution_plan(&mut self) -> Result<ExecutionPlan, GraphError> {
        if !self.is_dirty() {
            if let Some(plan) = self.execution_plan() {
                return Ok(plan.clone());
            }
        }

        // 1. Extract system subgraph
        let system_nodes: Vec<NodeIndex> = self
            .system_nodes()
            .values()
            .copied()
            .collect();

        if system_nodes.is_empty() {
            let plan = ExecutionPlan { phases: Vec::new() };
            self.set_execution_plan(plan.clone());
            return Ok(plan);
        }

        // 2. Topological sort (detect cycles)
        let sorted = match toposort(self.graph(), None) {
            Ok(sorted) => sorted,
            Err(cycle) => {
                let cycle_names = self.extract_cycle_names(cycle.node_id());
                return Err(GraphError::CyclicDependency(cycle_names));
            }
        };

        // 3. Filter to systems only
        let system_order: Vec<NodeIndex> = sorted
            .into_iter()
            .filter(|&n| matches!(self.graph()[n], GraphNode::System { .. }))
            .collect();

        // 4. Build phases (greedy coloring by write conflicts)
        let phases = self.build_phases(&system_order)?;

        let plan = ExecutionPlan { phases };
        self.set_execution_plan(plan.clone());

        Ok(plan)
    }

    fn extract_cycle_names(&self, start: NodeIndex) -> Vec<String> {
        let mut cycle = vec![self.node_name(start)];
        
        let mut visited = HashSet::new();
        let mut current = start;
        
        while visited.insert(current) {
            if let Some(next) = self.graph().neighbors(current).next() {
                cycle.push(self.node_name(next));
                current = next;
                if current == start {
                    break;
                }
            } else {
                break;
            }
        }
        
        cycle
    }

    fn node_name(&self, node: NodeIndex) -> String {
        match &self.graph()[node] {
            GraphNode::System { name, .. } => name.clone(),
            GraphNode::Entity { handle, .. } => format!("Entity({}:{})", handle.0.index, handle.0.generation),
            GraphNode::Component { name, .. } => format!("Component({})", name),
            GraphNode::Archetype { id, .. } => format!("Archetype({})", id.0),
        }
    }

    fn build_phases(&self, systems: &[NodeIndex]) -> Result<Vec<ExecutionPhase>, GraphError> {
        let mut phases = Vec::new();
        let mut remaining: Vec<NodeIndex> = systems.to_vec();

        while !remaining.is_empty() {
            let mut phase = ExecutionPhase {
                systems: Vec::new(),
                reads: Vec::new(),
                writes: Vec::new(),
            };

            let mut i = 0;
            while i < remaining.len() {
                let sys_node = remaining[i];

                if let GraphNode::System {
                    name,
                    archetype,
                    resonator,
                    reads,
                    writes,
                    ..
                } = &self.graph()[sys_node]
                {
                    // Check write conflicts
                    let has_conflict = writes.iter().any(|w| phase.writes.contains(w));

                    if !has_conflict {
                        // Can add to this phase
                        phase.systems.push(SystemExecution {
                            archetype: *archetype,
                            resonator: resonator.clone(),
                            name: name.clone(),
                        });
                        phase.reads.extend(reads);
                        phase.writes.extend(writes);

                        remaining.remove(i);
                        continue; // Don't increment i
                    }
                }

                i += 1;
            }

            if phase.systems.is_empty() {
                // Deadlock detection: no system could be added
                return Err(GraphError::ConflictingWrites {
                    system_a: "unknown".to_string(),
                    system_b: "unknown".to_string(),
                    component: "unknown".to_string(),
                });
            }

            phases.push(phase);
        }

        Ok(phases)
    }
}

impl ExecutionPlan {
    pub fn phase_count(&self) -> usize {
        self.phases.len()
    }

    pub fn total_systems(&self) -> usize {
        self.phases.iter().map(|p| p.systems.len()).sum()
    }

    pub fn max_parallelism(&self) -> usize {
        self.phases
            .iter()
            .map(|p| p.systems.len())
            .max()
            .unwrap_or(0)
    }
}