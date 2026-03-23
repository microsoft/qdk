// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    fmt::Display,
    ops::Add,
    sync::{Arc, RwLock, RwLockReadGuard},
};

use num_traits::FromPrimitive;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use crate::{ParetoFrontier3D, trace::instruction_ids::instruction_name};

pub mod property_keys;

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub struct ISA {
    graph: Arc<RwLock<ProvenanceGraph>>,
    nodes: FxHashMap<u64, usize>,
}

impl Default for ISA {
    fn default() -> Self {
        ISA {
            graph: Arc::new(RwLock::new(ProvenanceGraph::new())),
            nodes: FxHashMap::default(),
        }
    }
}

impl ISA {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an ISA backed by the given shared provenance graph.
    #[must_use]
    pub fn with_graph(graph: Arc<RwLock<ProvenanceGraph>>) -> Self {
        ISA {
            graph,
            nodes: FxHashMap::default(),
        }
    }

    /// Returns a reference to the shared provenance graph.
    #[must_use]
    pub fn graph(&self) -> &Arc<RwLock<ProvenanceGraph>> {
        &self.graph
    }

    /// Adds an instruction to the provenance graph and records its node index.
    /// Returns the node index in the graph.
    pub fn add_instruction(&mut self, instruction: Instruction) -> usize {
        let id = instruction.id;
        let mut graph = self.graph.write().expect("provenance graph lock poisoned");
        let node_idx = graph.add_node(instruction, 0, &[]);
        self.nodes.insert(id, node_idx);
        node_idx
    }

    /// Records an existing provenance graph node in this ISA.
    pub fn add_node(&mut self, instruction_id: u64, node_index: usize) {
        self.nodes.insert(instruction_id, node_index);
    }

    /// Returns the node index for an instruction ID, if present.
    #[must_use]
    pub fn node_index(&self, id: &u64) -> Option<usize> {
        self.nodes.get(id).copied()
    }

    /// Returns a clone of the instruction with the given ID, if present.
    #[must_use]
    pub fn get(&self, id: &u64) -> Option<Instruction> {
        let &node_idx = self.nodes.get(id)?;
        let graph = self.read_graph();
        Some(graph.instruction(node_idx).clone())
    }

    #[must_use]
    pub fn contains(&self, id: &u64) -> bool {
        self.nodes.contains_key(id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns a read-locked view of this ISA, enabling zero-clone
    /// instruction access for the lifetime of the returned guard.
    #[must_use]
    pub fn lock(&self) -> LockedISA<'_> {
        LockedISA {
            graph: self.read_graph(),
            nodes: &self.nodes,
        }
    }

    fn read_graph(&self) -> RwLockReadGuard<'_, ProvenanceGraph> {
        self.graph.read().expect("provenance graph lock poisoned")
    }

    /// Returns an iterator over pairs of instruction IDs and node IDs.
    pub fn node_entries(&self) -> impl Iterator<Item = (&u64, &usize)> {
        self.nodes.iter()
    }

    /// Returns all instructions as owned clones.
    #[must_use]
    pub fn instructions(&self) -> Vec<Instruction> {
        let graph = self.read_graph();
        self.nodes
            .values()
            .map(|&idx| graph.instruction(idx).clone())
            .collect()
    }

    #[must_use]
    pub fn satisfies(&self, requirements: &ISARequirements) -> bool {
        let graph = self.read_graph();
        for constraint in requirements.constraints.values() {
            let Some(&node_idx) = self.nodes.get(&constraint.id) else {
                return false;
            };

            let instruction = graph.instruction(node_idx);

            if !constraint.is_satisfied_by(instruction) {
                return false;
            }
        }
        true
    }
}

impl FromIterator<Instruction> for ISA {
    fn from_iter<T: IntoIterator<Item = Instruction>>(iter: T) -> Self {
        let mut isa = ISA::new();
        for instruction in iter {
            isa.add_instruction(instruction);
        }
        isa
    }
}

impl Display for ISA {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let graph = self.read_graph();
        for &node_idx in self.nodes.values() {
            let instruction = graph.instruction(node_idx);
            writeln!(f, "{instruction}")?;
        }
        Ok(())
    }
}

impl Add<ISA> for ISA {
    type Output = ISA;

    fn add(self, other: ISA) -> ISA {
        let mut combined = self;
        if Arc::ptr_eq(&combined.graph, &other.graph) {
            // Same graph: just merge node maps
            for (id, node_idx) in other.nodes {
                combined.nodes.insert(id, node_idx);
            }
        } else {
            // Different graphs: copy instructions into combined's graph
            let other_graph = other.read_graph();
            let mut self_graph = combined
                .graph
                .write()
                .expect("provenance graph lock poisoned");
            for (id, node_idx) in &other.nodes {
                let instruction = other_graph.instruction(*node_idx).clone();
                let new_idx = self_graph.add_node(instruction, 0, &[]);
                combined.nodes.insert(*id, new_idx);
            }
        }
        combined
    }
}

/// A read-locked view of an ISA. Holds the graph read lock for the
/// lifetime of this struct, enabling zero-clone instruction access.
pub struct LockedISA<'a> {
    graph: RwLockReadGuard<'a, ProvenanceGraph>,
    nodes: &'a FxHashMap<u64, usize>,
}

impl LockedISA<'_> {
    /// Returns a reference to the instruction with the given ID, if present.
    #[must_use]
    pub fn get(&self, id: &u64) -> Option<&Instruction> {
        let &node_idx = self.nodes.get(id)?;
        Some(self.graph.instruction(node_idx))
    }
}

#[derive(Default)]
pub struct ISARequirements {
    constraints: FxHashMap<u64, InstructionConstraint>,
}

impl ISARequirements {
    #[must_use]
    pub fn new() -> Self {
        ISARequirements {
            constraints: FxHashMap::default(),
        }
    }

    pub fn add_constraint(&mut self, constraint: InstructionConstraint) {
        self.constraints.insert(constraint.id, constraint);
    }
}

impl FromIterator<InstructionConstraint> for ISARequirements {
    fn from_iter<T: IntoIterator<Item = InstructionConstraint>>(iter: T) -> Self {
        let mut reqs = ISARequirements::new();
        for constraint in iter {
            reqs.add_constraint(constraint);
        }
        reqs
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Instruction {
    id: u64,
    encoding: Encoding,
    metrics: Metrics,
    source: usize,
    properties: Option<FxHashMap<u64, u64>>,
}

impl Instruction {
    #[must_use]
    pub fn fixed_arity(
        id: u64,
        encoding: Encoding,
        arity: u64,
        time: u64,
        space: Option<u64>,
        length: Option<u64>,
        error_rate: f64,
    ) -> Self {
        let length = length.unwrap_or(arity);
        let space = space.unwrap_or(length);

        Instruction {
            id,
            encoding,
            metrics: Metrics::FixedArity {
                arity,
                length,
                space,
                time,
                error_rate,
            },
            source: 0,
            properties: None,
        }
    }

    #[must_use]
    pub fn variable_arity(
        id: u64,
        encoding: Encoding,
        time_fn: VariableArityFunction<u64>,
        space_fn: VariableArityFunction<u64>,
        length_fn: Option<VariableArityFunction<u64>>,
        error_rate_fn: VariableArityFunction<f64>,
    ) -> Self {
        let length_fn = length_fn.unwrap_or_else(|| space_fn.clone());

        Instruction {
            id,
            encoding,
            metrics: Metrics::VariableArity {
                length_fn,
                space_fn,
                time_fn,
                error_rate_fn,
            },
            source: 0,
            properties: None,
        }
    }

    #[must_use]
    pub fn with_id(&self, id: u64) -> Self {
        let mut new_instruction = self.clone();
        // reset source for new instruction
        new_instruction.source = 0;
        new_instruction.id = id;
        new_instruction
    }

    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    #[must_use]
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    #[must_use]
    pub fn arity(&self) -> Option<u64> {
        match &self.metrics {
            Metrics::FixedArity { arity, .. } => Some(*arity),
            Metrics::VariableArity { .. } => None,
        }
    }

    #[must_use]
    pub fn space(&self, arity: Option<u64>) -> Option<u64> {
        match &self.metrics {
            Metrics::FixedArity { space, .. } => Some(*space),
            Metrics::VariableArity { space_fn, .. } => arity.map(|a| space_fn.evaluate(a)),
        }
    }

    #[must_use]
    pub fn length(&self, arity: Option<u64>) -> Option<u64> {
        match &self.metrics {
            Metrics::FixedArity { length, .. } => Some(*length),
            Metrics::VariableArity { length_fn, .. } => arity.map(|a| length_fn.evaluate(a)),
        }
    }

    #[must_use]
    pub fn time(&self, arity: Option<u64>) -> Option<u64> {
        match &self.metrics {
            Metrics::FixedArity { time, .. } => Some(*time),
            Metrics::VariableArity { time_fn, .. } => arity.map(|a| time_fn.evaluate(a)),
        }
    }

    #[must_use]
    pub fn error_rate(&self, arity: Option<u64>) -> Option<f64> {
        match &self.metrics {
            Metrics::FixedArity { error_rate, .. } => Some(*error_rate),
            Metrics::VariableArity { error_rate_fn, .. } => {
                arity.map(|a| error_rate_fn.evaluate(a))
            }
        }
    }

    #[must_use]
    pub fn expect_space(&self, arity: Option<u64>) -> u64 {
        self.space(arity)
            .expect("Instruction does not support variable arity")
    }

    #[must_use]
    pub fn expect_length(&self, arity: Option<u64>) -> u64 {
        self.length(arity)
            .expect("Instruction does not support variable arity")
    }

    #[must_use]
    pub fn expect_time(&self, arity: Option<u64>) -> u64 {
        self.time(arity)
            .expect("Instruction does not support variable arity")
    }

    #[must_use]
    pub fn expect_error_rate(&self, arity: Option<u64>) -> f64 {
        self.error_rate(arity)
            .expect("Instruction does not support variable arity")
    }

    pub fn set_source(&mut self, provenance: usize) {
        self.source = provenance;
    }

    #[must_use]
    pub fn source(&self) -> usize {
        self.source
    }

    pub fn set_property(&mut self, key: u64, value: u64) {
        if let Some(ref mut properties) = self.properties {
            properties.insert(key, value);
        } else {
            let mut properties = FxHashMap::default();
            properties.insert(key, value);
            self.properties = Some(properties);
        }
    }

    #[must_use]
    pub fn get_property(&self, key: &u64) -> Option<u64> {
        self.properties.as_ref()?.get(key).copied()
    }

    #[must_use]
    pub fn has_property(&self, key: &u64) -> bool {
        self.properties
            .as_ref()
            .is_some_and(|props| props.contains_key(key))
    }

    #[must_use]
    pub fn get_property_or(&self, key: &u64, default: u64) -> u64 {
        self.get_property(key).unwrap_or(default)
    }
}

impl Display for Instruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = instruction_name(self.id).unwrap_or("??");
        match self.metrics {
            Metrics::FixedArity { arity, .. } => {
                write!(f, "{name} |{:?}| arity: {arity}", self.encoding)
            }
            Metrics::VariableArity { .. } => write!(f, "{name} |{:?}|", self.encoding),
        }
    }
}

#[derive(Clone)]
pub struct InstructionConstraint {
    id: u64,
    encoding: Encoding,
    arity: Option<u64>,
    error_rate_fn: Option<ConstraintBound<f64>>,
    properties: FxHashSet<u64>,
}

impl InstructionConstraint {
    #[must_use]
    pub fn new(
        id: u64,
        encoding: Encoding,
        arity: Option<u64>,
        error_rate_fn: Option<ConstraintBound<f64>>,
    ) -> Self {
        InstructionConstraint {
            id,
            encoding,
            arity,
            error_rate_fn,
            properties: FxHashSet::default(),
        }
    }

    /// Adds a property requirement to the constraint.
    pub fn add_property(&mut self, property: u64) {
        self.properties.insert(property);
    }

    /// Checks if the constraint requires a specific property.
    #[must_use]
    pub fn has_property(&self, property: &u64) -> bool {
        self.properties.contains(property)
    }

    /// Returns the set of required properties.
    #[must_use]
    pub fn properties(&self) -> &FxHashSet<u64> {
        &self.properties
    }

    /// Returns the instruction ID this constraint applies to.
    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Checks whether a given instruction satisfies this constraint.
    #[must_use]
    pub fn is_satisfied_by(&self, instruction: &Instruction) -> bool {
        if instruction.encoding != self.encoding {
            return false;
        }

        match &instruction.metrics {
            Metrics::FixedArity {
                arity, error_rate, ..
            } => {
                // Constraint requires variable arity but instruction is fixed
                let Some(constraint_arity) = self.arity else {
                    return false;
                };
                if *arity != constraint_arity {
                    return false;
                }
                if let Some(ref bound) = self.error_rate_fn
                    && !bound.evaluate(error_rate)
                {
                    return false;
                }
            }
            Metrics::VariableArity { error_rate_fn, .. } => {
                if let (Some(constraint_arity), Some(bound)) = (self.arity, &self.error_rate_fn)
                    && !bound.evaluate(&error_rate_fn.evaluate(constraint_arity))
                {
                    return false;
                }
            }
        }

        for prop in &self.properties {
            if !instruction.has_property(prop) {
                return false;
            }
        }

        true
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Encoding {
    #[default]
    Physical,
    Logical,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Metrics {
    FixedArity {
        arity: u64,
        length: u64,
        space: u64,
        time: u64,
        error_rate: f64,
    },
    VariableArity {
        length_fn: VariableArityFunction<u64>,
        space_fn: VariableArityFunction<u64>,
        time_fn: VariableArityFunction<u64>,
        error_rate_fn: VariableArityFunction<f64>,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub enum VariableArityFunction<T> {
    Constant {
        value: T,
    },
    Linear {
        slope: T,
    },
    BlockLinear {
        block_size: u64,
        slope: T,
        offset: T,
    },
    #[serde(skip)]
    Generic {
        func: Arc<dyn Fn(u64) -> T + Send + Sync>,
    },
}

impl<T: Add<Output = T> + std::ops::Mul<Output = T> + Copy + FromPrimitive>
    VariableArityFunction<T>
{
    pub fn constant(value: T) -> Self {
        VariableArityFunction::Constant { value }
    }

    pub fn linear(slope: T) -> Self {
        VariableArityFunction::Linear { slope }
    }

    pub fn block_linear(block_size: u64, slope: T, offset: T) -> Self {
        VariableArityFunction::BlockLinear {
            block_size,
            slope,
            offset,
        }
    }

    pub fn generic(func: impl Fn(u64) -> T + Send + Sync + 'static) -> Self {
        VariableArityFunction::Generic {
            func: Arc::new(func),
        }
    }

    pub fn generic_from_arc(func: Arc<dyn Fn(u64) -> T + Send + Sync>) -> Self {
        VariableArityFunction::Generic { func }
    }

    pub fn evaluate(&self, arity: u64) -> T {
        match self {
            VariableArityFunction::Constant { value } => *value,
            VariableArityFunction::Linear { slope } => {
                *slope * T::from_u64(arity).expect("Failed to convert u64 to target type")
            }
            VariableArityFunction::BlockLinear {
                block_size,
                slope,
                offset,
            } => {
                let blocks = arity.div_ceil(*block_size);
                *slope * T::from_u64(blocks).expect("Failed to convert u64 to target type")
                    + *offset
            }
            VariableArityFunction::Generic { func } => func(arity),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConstraintBound<T> {
    LessThan(T),
    LessEqual(T),
    Equal(T),
    GreaterThan(T),
    GreaterEqual(T),
}

impl<T: PartialOrd + PartialEq> ConstraintBound<T> {
    pub fn less_than(value: T) -> Self {
        ConstraintBound::LessThan(value)
    }

    pub fn less_equal(value: T) -> Self {
        ConstraintBound::LessEqual(value)
    }

    pub fn equal(value: T) -> Self {
        ConstraintBound::Equal(value)
    }

    pub fn greater_than(value: T) -> Self {
        ConstraintBound::GreaterThan(value)
    }

    pub fn greater_equal(value: T) -> Self {
        ConstraintBound::GreaterEqual(value)
    }

    pub fn evaluate(&self, other: &T) -> bool {
        match self {
            ConstraintBound::LessThan(v) => other < v,
            ConstraintBound::LessEqual(v) => other <= v,
            ConstraintBound::Equal(v) => other == v,
            ConstraintBound::GreaterThan(v) => other > v,
            ConstraintBound::GreaterEqual(v) => other >= v,
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
            let mut sub_groups: FxHashMap<(Encoding, Vec<(u64, u64)>), Vec<usize>> =
                FxHashMap::default();
            for &idx in &node_indices {
                let instr = &self.nodes[idx].instruction;
                let mut prop_vec: Vec<(u64, u64)> = instr
                    .properties
                    .as_ref()
                    .map(|p| {
                        let mut v: Vec<_> = p.iter().map(|(&k, &v)| (k, v)).collect();
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
                (min_idx..self.nodes.len())
                    .filter(|&node_idx| {
                        let instr = &self.nodes[node_idx].instruction;
                        instr.id == constraint.id() && constraint.is_satisfied_by(instr)
                    })
                    .map(|node_idx| (constraint.id(), node_idx))
                    .collect()
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
