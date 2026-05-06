// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::{Arc, RwLock};

use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    Encoding, ISA, ISARequirements, Instruction, ParetoFrontier3D, Property, float_to_bits,
};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum PropertyGroupKey {
    Bool(bool),
    Int(i64),
    Float(u64),
    Str(String),
}

impl From<&Property> for PropertyGroupKey {
    fn from(value: &Property) -> Self {
        match value {
            Property::Bool(v) => Self::Bool(*v),
            Property::Int(v) => Self::Int(*v),
            Property::Float(v) => Self::Float(float_to_bits(*v)),
            Property::Str(v) => Self::Str(v.clone()),
        }
    }
}

pub struct ProvenanceGraph {
    nodes: Vec<ProvenanceNode>,
    // A consecutive list of child node indices for each node, where the
    // children of node i are located at children[offset..offset+num_children]
    // in the children vector.
    children: Vec<usize>,
    // Per-instruction-ID index of Pareto-optimal node indices.
    // Built by `build_pareto_index()` after all nodes have been added.
    pareto_index: FxHashMap<u64, Vec<usize>>,
}

impl Default for ProvenanceGraph {
    fn default() -> Self {
        // Initialize with a dummy node at index 0 to simplify indexing logic
        // (so that 0 can be used as a "null" provenance)
        let empty = ProvenanceNode::default();
        ProvenanceGraph {
            nodes: vec![empty],
            children: Vec::new(),
            pareto_index: FxHashMap::default(),
        }
    }
}

/// Thin wrapper for 3D Pareto comparison of instructions at arity 1.
struct InstructionParetoItem {
    node_index: usize,
    space: u64,
    time: u64,
    error: f64,
}

impl crate::ParetoItem3D for InstructionParetoItem {
    type Objective1 = u64;
    type Objective2 = u64;
    type Objective3 = f64;

    fn objective1(&self) -> u64 {
        self.space
    }
    fn objective2(&self) -> u64 {
        self.time
    }
    fn objective3(&self) -> f64 {
        self.error
    }
}

impl ProvenanceGraph {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(
        &mut self,
        mut instruction: Instruction,
        transform_id: u64,
        children: &[usize],
    ) -> usize {
        let node_index = self.nodes.len();
        instruction.source = node_index;
        let offset = self.children.len();
        let num_children = children.len();
        self.children.extend_from_slice(children);
        self.nodes.push(ProvenanceNode {
            instruction,
            transform_id,
            offset,
            num_children,
        });
        node_index
    }

    #[must_use]
    pub fn instruction(&self, node_index: usize) -> &Instruction {
        &self.nodes[node_index].instruction
    }

    #[must_use]
    pub fn transform_id(&self, node_index: usize) -> u64 {
        self.nodes[node_index].transform_id
    }

    #[must_use]
    pub fn children(&self, node_index: usize) -> &[usize] {
        let node = &self.nodes[node_index];
        &self.children[node.offset..node.offset + node.num_children]
    }

    #[must_use]
    pub fn num_nodes(&self) -> usize {
        self.nodes.len() - 1
    }

    #[must_use]
    pub fn num_edges(&self) -> usize {
        self.children.len()
    }

    /// Builds the per-instruction-ID Pareto index.
    ///
    /// For each instruction ID in the graph, collects all nodes and retains
    /// only the Pareto-optimal subset with respect to (space, time, `error_rate`)
    /// evaluated at arity 1. Instructions with different encodings or
    /// properties are never in competition.
    ///
    /// Must be called after all nodes have been added.
    pub fn build_pareto_index(&mut self) {
        // Group node indices by (instruction_id, encoding, properties)
        let mut groups: FxHashMap<u64, Vec<usize>> = FxHashMap::default();
        for idx in 1..self.nodes.len() {
            let instr = &self.nodes[idx].instruction;
            groups.entry(instr.id).or_default().push(idx);
        }

        let mut pareto_index = FxHashMap::default();
        for (id, node_indices) in groups {
            // Sub-partition by encoding and property keys to avoid comparing
            // incompatible instructions (Risk R2 mitigation)
            #[allow(clippy::type_complexity)]
            let mut sub_groups: FxHashMap<
                (Encoding, Vec<(u64, PropertyGroupKey)>),
                Vec<usize>,
            > = FxHashMap::default();
            for &idx in &node_indices {
                let instr = &self.nodes[idx].instruction;
                let mut prop_vec: Vec<(u64, PropertyGroupKey)> = instr
                    .properties
                    .as_ref()
                    .map(|p| {
                        let mut v: Vec<_> = p
                            .iter()
                            .map(|(&k, v)| (k, PropertyGroupKey::from(v)))
                            .collect();
                        v.sort_unstable();
                        v
                    })
                    .unwrap_or_default();
                prop_vec.sort_unstable();
                sub_groups
                    .entry((instr.encoding, prop_vec))
                    .or_default()
                    .push(idx);
            }

            let mut pareto_nodes = Vec::new();
            for (_key, indices) in sub_groups {
                let items: Vec<InstructionParetoItem> = indices
                    .iter()
                    .filter_map(|&idx| {
                        let instr = &self.nodes[idx].instruction;
                        let space = instr.space(Some(1))?;
                        let time = instr.time(Some(1))?;
                        let error = instr.error_rate(Some(1))?;
                        Some(InstructionParetoItem {
                            node_index: idx,
                            space,
                            time,
                            error,
                        })
                    })
                    .collect();

                let frontier: ParetoFrontier3D<InstructionParetoItem> = items.into_iter().collect();
                pareto_nodes.extend(frontier.into_iter().map(|item| item.node_index));
            }

            pareto_index.insert(id, pareto_nodes);
        }

        self.pareto_index = pareto_index;
    }

    /// Returns the Pareto-optimal node indices for a given instruction ID.
    #[must_use]
    pub fn pareto_nodes(&self, instruction_id: u64) -> Option<&[usize]> {
        self.pareto_index.get(&instruction_id).map(Vec::as_slice)
    }

    /// Returns all instruction IDs that have Pareto-optimal entries.
    #[must_use]
    pub fn pareto_instruction_ids(&self) -> Vec<u64> {
        self.pareto_index.keys().copied().collect()
    }

    /// Returns the raw node count (including the sentinel at index 0).
    #[must_use]
    pub fn raw_node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the total number of ISAs that can be formed from Pareto-optimal
    /// nodes.
    ///
    /// Requires [`build_pareto_index`](Self::build_pareto_index) to have
    /// been called.
    #[must_use]
    pub fn total_isa_count(&self) -> usize {
        self.pareto_index.values().map(Vec::len).product()
    }

    /// Returns ISAs formed from Pareto-optimal nodes that satisfy the given
    /// requirements.
    ///
    /// For each constraint, selects matching Pareto-optimal nodes. Produces
    /// the Cartesian product of per-constraint match sets, each augmented
    /// with one representative node per unconstrained instruction ID (so
    /// that returned ISAs contain entries for all instruction types in the
    /// graph).
    ///
    /// When `min_node_idx` is `Some(n)`, only Pareto nodes with index ≥ n
    /// are considered for constrained groups.  Unconstrained "extra" nodes
    /// are not filtered since they serve only as default placeholders.
    ///
    /// Requires [`build_pareto_index`](Self::build_pareto_index) to have
    /// been called.
    #[must_use]
    pub fn query_satisfying(
        &self,
        graph_arc: &Arc<RwLock<ProvenanceGraph>>,
        requirements: &ISARequirements,
        min_node_idx: Option<usize>,
    ) -> Vec<ISA> {
        let min_idx = min_node_idx.unwrap_or(0);

        let mut constrained_groups: Vec<Vec<(u64, usize)>> = Vec::new();
        let mut constrained_ids: FxHashSet<u64> = FxHashSet::default();

        for constraint in requirements.constraints.values() {
            constrained_ids.insert(constraint.id());

            // When a node range is specified, scan ALL nodes in the range
            // instead of using the global Pareto index.  The global index
            // may have pruned nodes from this range as duplicates of
            // earlier, equivalent nodes outside the range.
            let matching: Vec<(u64, usize)> = if min_idx > 0 {
                let mut m: Vec<(u64, usize)> = (min_idx..self.nodes.len())
                    .filter(|&node_idx| {
                        let instr = &self.nodes[node_idx].instruction;
                        instr.id == constraint.id() && constraint.is_satisfied_by(instr)
                    })
                    .map(|node_idx| (constraint.id(), node_idx))
                    .collect();

                // Fall back to the full graph for passthrough instructions
                // that the source does not modify (e.g. architecture base
                // gates that a wrapper leaves unchanged).
                if m.is_empty() {
                    m = (1..min_idx)
                        .filter(|&node_idx| {
                            let instr = &self.nodes[node_idx].instruction;
                            instr.id == constraint.id() && constraint.is_satisfied_by(instr)
                        })
                        .map(|node_idx| (constraint.id(), node_idx))
                        .collect();
                }
                m
            } else {
                let Some(pareto) = self.pareto_index.get(&constraint.id()) else {
                    return Vec::new();
                };
                pareto
                    .iter()
                    .filter(|&&node_idx| constraint.is_satisfied_by(self.instruction(node_idx)))
                    .map(|&node_idx| (constraint.id(), node_idx))
                    .collect()
            };

            if matching.is_empty() {
                return Vec::new();
            }
            constrained_groups.push(matching);
        }

        // One representative node per unconstrained instruction ID.
        // When a Pareto index is available, use it; otherwise scan all
        // nodes (this path is used during populate() before the index
        // is built).
        let extra_nodes: Vec<(u64, usize)> = if self.pareto_index.is_empty() {
            let mut seen: FxHashMap<u64, usize> = FxHashMap::default();
            for idx in 1..self.nodes.len() {
                let id = self.nodes[idx].instruction.id;
                if !constrained_ids.contains(&id) {
                    seen.entry(id).or_insert(idx);
                }
            }
            seen.into_iter().collect()
        } else {
            self.pareto_index
                .iter()
                .filter(|(id, _)| !constrained_ids.contains(id))
                .filter_map(|(&id, nodes)| nodes.first().map(|&n| (id, n)))
                .collect()
        };

        // Cartesian product of constrained groups
        let mut combinations: Vec<Vec<(u64, usize)>> = vec![Vec::new()];
        for group in &constrained_groups {
            let mut next = Vec::with_capacity(combinations.len() * group.len());
            for combo in &combinations {
                for &item in group {
                    let mut extended = combo.clone();
                    extended.push(item);
                    next.push(extended);
                }
            }
            combinations = next;
        }

        // Build ISAs from selections
        combinations
            .into_iter()
            .map(|mut combo| {
                combo.extend(extra_nodes.iter().copied());
                let mut isa = ISA::with_graph(Arc::clone(graph_arc));
                for (id, node_idx) in combo {
                    isa.add_node(id, node_idx);
                }
                isa
            })
            .collect()
    }
}

struct ProvenanceNode {
    instruction: Instruction,
    transform_id: u64,
    offset: usize,
    num_children: usize,
}

impl Default for ProvenanceNode {
    fn default() -> Self {
        ProvenanceNode {
            instruction: Instruction::fixed_arity(0, Encoding::Physical, 0, 0, None, None, 0.0),
            transform_id: 0,
            offset: 0,
            num_children: 0,
        }
    }
}
