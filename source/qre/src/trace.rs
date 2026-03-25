// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    collections::hash_map::DefaultHasher,
    fmt::{Display, Formatter},
    hash::{Hash, Hasher},
    iter::repeat_with,
    sync::{Arc, RwLock, atomic::AtomicUsize},
    vec,
};

use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use crate::{
    ConstraintBound, Encoding, Error, EstimationCollection, EstimationResult, FactoryResult, ISA,
    ISARequirements, Instruction, InstructionConstraint, LockedISA, ProvenanceGraph, ResultSummary,
    property_keys::{
        LOGICAL_COMPUTE_QUBITS, LOGICAL_MEMORY_QUBITS, PHYSICAL_COMPUTE_QUBITS,
        PHYSICAL_FACTORY_QUBITS, PHYSICAL_MEMORY_QUBITS,
    },
};

pub mod instruction_ids;
use instruction_ids::instruction_name;
#[cfg(test)]
mod tests;

mod transforms;
pub use transforms::{LatticeSurgery, PSSPC, TraceTransform};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Trace {
    block: Block,
    base_error: f64,
    compute_qubits: u64,
    memory_qubits: Option<u64>,
    resource_states: Option<FxHashMap<u64, u64>>,
    properties: FxHashMap<u64, Property>,
}

impl Trace {
    #[must_use]
    pub fn new(compute_qubits: u64) -> Self {
        Self {
            compute_qubits,
            ..Default::default()
        }
    }

    #[must_use]
    pub fn clone_empty(&self, compute_qubits: Option<u64>) -> Self {
        Self {
            block: Block::default(),
            base_error: self.base_error,
            compute_qubits: compute_qubits.unwrap_or(self.compute_qubits),
            memory_qubits: self.memory_qubits,
            resource_states: self.resource_states.clone(),
            properties: self.properties.clone(),
        }
    }

    #[must_use]
    pub fn compute_qubits(&self) -> u64 {
        self.compute_qubits
    }

    pub fn add_operation(&mut self, id: u64, qubits: Vec<u64>, params: Vec<f64>) {
        self.block.add_operation(id, qubits, params);
    }

    pub fn add_block(&mut self, repetitions: u64) -> &mut Block {
        self.block.add_block(repetitions)
    }

    #[must_use]
    pub fn base_error(&self) -> f64 {
        self.base_error
    }

    pub fn increment_base_error(&mut self, amount: f64) {
        self.base_error += amount;
    }

    #[must_use]
    pub fn memory_qubits(&self) -> Option<u64> {
        self.memory_qubits
    }

    #[must_use]
    pub fn has_memory_qubits(&self) -> bool {
        self.memory_qubits.is_some()
    }

    pub fn set_memory_qubits(&mut self, qubits: u64) {
        self.memory_qubits = Some(qubits);
    }

    pub fn increment_memory_qubits(&mut self, amount: u64) {
        if amount == 0 {
            return;
        }
        let current = self.memory_qubits.get_or_insert(0);
        *current += amount;
    }

    #[must_use]
    pub fn total_qubits(&self) -> u64 {
        self.compute_qubits + self.memory_qubits.unwrap_or(0)
    }

    pub fn increment_resource_state(&mut self, resource_id: u64, amount: u64) {
        if amount == 0 {
            return;
        }
        let states = self.resource_states.get_or_insert_with(FxHashMap::default);
        *states.entry(resource_id).or_default() += amount;
    }

    #[must_use]
    pub fn get_resource_states(&self) -> Option<&FxHashMap<u64, u64>> {
        self.resource_states.as_ref()
    }

    #[must_use]
    pub fn get_resource_state_count(&self, resource_id: u64) -> u64 {
        if let Some(states) = &self.resource_states
            && let Some(count) = states.get(&resource_id)
        {
            return *count;
        }
        0
    }

    pub fn set_property(&mut self, key: u64, value: Property) {
        self.properties.insert(key, value);
    }

    #[must_use]
    pub fn get_property(&self, key: u64) -> Option<&Property> {
        self.properties.get(&key)
    }

    #[must_use]
    pub fn has_property(&self, key: u64) -> bool {
        self.properties.contains_key(&key)
    }

    #[must_use]
    pub fn deep_iter(&self) -> TraceIterator<'_> {
        TraceIterator::new(&self.block)
    }

    /// Returns the set of instruction IDs required by this trace, along with
    /// their arity constraints if available.  We take the actual arity from the
    /// instruction, and if we see instructions with the same ID but different
    /// arities, we mark them as variable arity in the returned requirements.
    /// If `max_error` is provided, also adds error rate constraints based on
    /// the instruction usage volume and the maximum allowed error.  These error
    /// rate constraints can be used for instruction pruning during estimation.
    #[allow(clippy::cast_precision_loss)]
    #[must_use]
    pub fn required_instruction_ids(&self, max_error: Option<f64>) -> ISARequirements {
        let mut constraints = FxHashMap::<u64, (InstructionConstraint, u64)>::default();

        let mut update_constraints = |id: u64, arity: u64, added_volume: u64| {
            constraints
                .entry(id)
                .and_modify(|(constraint, volume)| {
                    if let Some(prev_arity) = constraint.arity()
                        && prev_arity != arity
                    {
                        constraint.set_arity(None);
                    }
                    *volume += added_volume;
                })
                .or_insert({
                    let constraint =
                        InstructionConstraint::new(id, Encoding::Logical, Some(arity), None);
                    (constraint, added_volume)
                });
        };

        for (gate, mult) in self.deep_iter() {
            let arity = gate.qubits.len() as u64;
            update_constraints(gate.id, arity, mult * arity);
        }
        if let Some(ref rs) = self.resource_states {
            for (res_id, count) in rs {
                update_constraints(*res_id, 1, *count);
            }
        }
        if let Some(memory_qubits) = self.memory_qubits {
            update_constraints(instruction_ids::MEMORY, memory_qubits, memory_qubits);
        }

        if let Some(max_error) = max_error {
            constraints
                .into_values()
                .map(|(mut c, volume)| {
                    c.set_error_rate(Some(ConstraintBound::less_equal(
                        max_error / (volume as f64),
                    )));
                    c
                })
                .collect()
        } else {
            constraints.into_values().map(|(c, _)| c).collect()
        }
    }

    #[must_use]
    pub fn depth(&self) -> u64 {
        self.block.depth()
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::too_many_lines
    )]
    pub fn estimate(&self, isa: &ISA, max_error: Option<f64>) -> Result<EstimationResult, Error> {
        let locked = isa.lock();
        let max_error = max_error.unwrap_or(1.0);

        if self.base_error > max_error {
            return Err(Error::MaximumErrorExceeded {
                actual_error: self.base_error,
                max_error,
            });
        }

        let mut result = EstimationResult::new();

        // base error starts with the error already present in the trace
        result.add_error(self.base_error);

        // Counts how many magic state factories are needed per resource state ID
        let mut factories: FxHashMap<u64, u64> = FxHashMap::default();

        // This will track the number of physical qubits per logical qubit while
        // processing all the instructions.  Normally, we assume that the number
        // is always the same.
        let mut qubit_counts: Vec<f64> = vec![];

        // ------------------------------------------------------------------
        // Add errors from resource states. Allow callable error rates.
        // ------------------------------------------------------------------
        if let Some(resource_states) = &self.resource_states {
            for (state_id, count) in resource_states {
                let rate = get_error_rate_by_id(&locked, *state_id)?;
                let actual_error = result.add_error(rate * (*count as f64));
                if actual_error > max_error {
                    return Err(Error::MaximumErrorExceeded {
                        actual_error,
                        max_error,
                    });
                }
                factories.insert(*state_id, *count);
            }
        }

        // ------------------------------------------------------------------
        // Gate error accumulation using recursion over block structure.
        // Each block contributes repetitions * internal_gate_errors.
        // Missing instructions raise an error. Callable rates use arity.
        // ------------------------------------------------------------------
        for (gate, mult) in self.deep_iter() {
            let instr = get_instruction(&locked, gate.id)?;

            let arity = gate.qubits.len() as u64;

            let rate = instr.expect_error_rate(Some(arity));

            let qubit_count = instr.expect_space(Some(arity)) as f64 / arity as f64;

            if let Err(i) = qubit_counts.binary_search_by(|qc| qc.total_cmp(&qubit_count)) {
                qubit_counts.insert(i, qubit_count);
            }

            let actual_error = result.add_error(rate * (mult as f64));
            if actual_error > max_error {
                return Err(Error::MaximumErrorExceeded {
                    actual_error,
                    max_error,
                });
            }
        }

        let total_compute_qubits = (self.compute_qubits() as f64
            * qubit_counts.last().copied().unwrap_or(1.0))
        .ceil() as u64;
        result.add_qubits(total_compute_qubits);
        result.set_property(
            PHYSICAL_COMPUTE_QUBITS,
            Property::Int(total_compute_qubits.cast_signed()),
        );

        result.add_runtime(
            self.block
                .depth_and_used(Some(&|op: &Gate| {
                    let instr = get_instruction(&locked, op.id)?;
                    Ok(instr.expect_time(Some(op.qubits.len() as u64)))
                }))?
                .0,
        );

        // ------------------------------------------------------------------
        // Factory overhead estimation. Each factory produces states at
        // a certain rate, so we need enough copies to meet the demand.
        // ------------------------------------------------------------------
        let mut total_factory_qubits = 0;
        for (factory, count) in &factories {
            let instr = get_instruction(&locked, *factory)?;
            let factory_time = get_time(instr)?;
            let factory_space = get_space(instr)?;
            let factory_error_rate = get_error_rate(instr)?;
            let runs = result.runtime() / factory_time;

            if runs == 0 {
                return Err(Error::FactoryTimeExceedsAlgorithmRuntime {
                    id: *factory,
                    factory_time,
                    algorithm_runtime: result.runtime(),
                });
            }

            let copies = count.div_ceil(runs);

            total_factory_qubits += copies * factory_space;
            result.add_factory_result(
                *factory,
                FactoryResult::new(copies, runs, *count, factory_error_rate),
            );
        }
        result.add_qubits(total_factory_qubits);
        result.set_property(
            PHYSICAL_FACTORY_QUBITS,
            Property::Int(total_factory_qubits.cast_signed()),
        );

        // Memory qubits
        if let Some(memory_qubits) = self.memory_qubits {
            // We need a MEMORY instruction in our ISA
            let memory = locked
                .get(&instruction_ids::MEMORY)
                .ok_or(Error::InstructionNotFound(instruction_ids::MEMORY))?;

            let memory_space = memory.expect_space(Some(memory_qubits));
            result.add_qubits(memory_space);
            result.set_property(
                PHYSICAL_MEMORY_QUBITS,
                Property::Int(memory_space.cast_signed()),
            );

            // The number of rounds for the memory qubits to stay alive with
            // respect to the total runtime of the algorithm.
            let rounds = result
                .runtime()
                .div_ceil(memory.expect_time(Some(memory_qubits)));

            let actual_error =
                result.add_error(rounds as f64 * memory.expect_error_rate(Some(memory_qubits)));
            if actual_error > max_error {
                return Err(Error::MaximumErrorExceeded {
                    actual_error,
                    max_error,
                });
            }
        }

        // Make main trace metrics properties to access them from the result
        result.set_property(
            LOGICAL_COMPUTE_QUBITS,
            Property::Int(self.compute_qubits.cast_signed()),
        );
        result.set_property(
            LOGICAL_MEMORY_QUBITS,
            Property::Int(self.memory_qubits.unwrap_or(0).cast_signed()),
        );

        // Copy properties from the trace to the result
        for (key, value) in &self.properties {
            result.set_property(*key, value.clone());
        }

        Ok(result)
    }
}

impl Display for Trace {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "@compute_qubits({})", self.compute_qubits())?;

        if let Some(memory_qubits) = self.memory_qubits {
            writeln!(f, "@memory_qubits({memory_qubits})")?;
        }
        if self.base_error > 0.0 {
            writeln!(f, "@base_error({})", self.base_error)?;
        }
        if let Some(resource_states) = &self.resource_states {
            for (res_id, amount) in resource_states {
                writeln!(
                    f,
                    "@resource_state({}, {amount})",
                    instruction_name(*res_id).unwrap_or("??")
                )?;
            }
        }
        write!(f, "{}", self.block)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    GateOperation(Gate),
    BlockOperation(Block),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gate {
    id: u64,
    qubits: Vec<u64>,
    params: Vec<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block {
    operations: Vec<Operation>,
    repetitions: u64,
}

impl Default for Block {
    fn default() -> Self {
        Self {
            operations: Vec::new(),
            repetitions: 1,
        }
    }
}

impl Block {
    pub fn add_operation(&mut self, id: u64, qubits: Vec<u64>, params: Vec<f64>) {
        self.operations
            .push(Operation::gate_operation(id, qubits, params));
    }

    pub fn add_block(&mut self, repetitions: u64) -> &mut Block {
        self.operations
            .push(Operation::block_operation(repetitions));

        match self.operations.last_mut() {
            Some(Operation::BlockOperation(b)) => b,
            _ => unreachable!("Last operation must be a block operation"),
        }
    }

    pub fn write(&self, f: &mut Formatter<'_>, indent: usize) -> std::fmt::Result {
        let indent_str = " ".repeat(indent);
        if self.repetitions == 1 {
            writeln!(f, "{indent_str}{{")?;
        } else {
            writeln!(f, "{indent_str}repeat {} {{", self.repetitions)?;
        }

        for op in &self.operations {
            match op {
                Operation::GateOperation(Gate { id, qubits, params }) => {
                    let name = instruction_name(*id).unwrap_or("??");
                    write!(f, "{indent_str}  {name}")?;
                    if !params.is_empty() {
                        write!(
                            f,
                            "({})",
                            params
                                .iter()
                                .map(f64::to_string)
                                .collect::<Vec<_>>()
                                .join(", ")
                        )?;
                    }
                    writeln!(
                        f,
                        "({})",
                        qubits
                            .iter()
                            .map(u64::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )?;
                }
                Operation::BlockOperation(b) => {
                    b.write(f, indent + 2)?;
                }
            }
        }
        writeln!(f, "{indent_str}}}")
    }

    fn depth_and_used<FnDuration: Fn(&Gate) -> Result<u64, Error>>(
        &self,
        duration_fn: Option<&FnDuration>,
    ) -> Result<(u64, FxHashSet<u64>), Error> {
        let mut qubit_depths: FxHashMap<u64, u64> = FxHashMap::default();
        let mut all_used = FxHashSet::default();

        for op in &self.operations {
            match op {
                Operation::GateOperation(gate) => {
                    let start_time = gate
                        .qubits
                        .iter()
                        .filter_map(|q| qubit_depths.get(q))
                        .max()
                        .copied()
                        .unwrap_or(0);

                    let duration = match duration_fn {
                        Some(f) => f(gate)?,
                        _ => 1,
                    };

                    let end_time = start_time + duration;
                    for q in &gate.qubits {
                        qubit_depths.insert(*q, end_time);
                        all_used.insert(*q);
                    }
                }
                Operation::BlockOperation(block) => {
                    let (duration, used) = block.depth_and_used(duration_fn)?;
                    if used.is_empty() {
                        continue;
                    }

                    let start_time = used
                        .iter()
                        .filter_map(|q| qubit_depths.get(q))
                        .max()
                        .copied()
                        .unwrap_or(0);

                    let end_time = start_time + duration;
                    for q in &used {
                        qubit_depths.insert(*q, end_time);
                    }
                    all_used.extend(used);
                }
            }
        }

        let max_depth = qubit_depths.values().max().copied().unwrap_or(0);
        Ok((max_depth * self.repetitions, all_used))
    }

    #[must_use]
    pub fn depth(&self) -> u64 {
        self.depth_and_used::<fn(&Gate) -> Result<u64, Error>>(None)
            .expect("Duration function is None")
            .0
    }
}

impl Display for Block {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.write(f, 0)
    }
}

impl Operation {
    fn gate_operation(id: u64, qubits: Vec<u64>, params: Vec<f64>) -> Self {
        Operation::GateOperation(Gate { id, qubits, params })
    }

    fn block_operation(repetitions: u64) -> Self {
        Operation::BlockOperation(Block {
            operations: Vec::new(),
            repetitions,
        })
    }
}

pub struct TraceIterator<'a> {
    stack: Vec<(std::slice::Iter<'a, Operation>, u64)>,
}

impl<'a> TraceIterator<'a> {
    fn new(block: &'a Block) -> Self {
        Self {
            stack: vec![(block.operations.iter(), 1)],
        }
    }
}

impl<'a> Iterator for TraceIterator<'a> {
    type Item = (&'a Gate, u64);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (iter, multiplier) = self.stack.last_mut()?;
            match iter.next() {
                Some(op) => match op {
                    Operation::GateOperation(g) => return Some((g, *multiplier)),
                    Operation::BlockOperation(block) => {
                        let new_multiplier = *multiplier * block.repetitions;
                        self.stack.push((block.operations.iter(), new_multiplier));
                    }
                },
                _ => {
                    self.stack.pop();
                }
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Property {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl Property {
    #[must_use]
    pub fn new_bool(b: bool) -> Self {
        Property::Bool(b)
    }

    #[must_use]
    pub fn new_int(i: i64) -> Self {
        Property::Int(i)
    }

    #[must_use]
    pub fn new_float(f: f64) -> Self {
        Property::Float(f)
    }

    #[must_use]
    pub fn new_str(s: String) -> Self {
        Property::Str(s)
    }

    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Property::Bool(b) => Some(*b),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Property::Int(i) => Some(*i),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Property::Float(f) => Some(*f),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Property::Str(s) => Some(s),
            _ => None,
        }
    }

    #[must_use]
    pub fn is_bool(&self) -> bool {
        matches!(self, Property::Bool(_))
    }

    #[must_use]
    pub fn is_int(&self) -> bool {
        matches!(self, Property::Int(_))
    }

    #[must_use]
    pub fn is_float(&self) -> bool {
        matches!(self, Property::Float(_))
    }

    #[must_use]
    pub fn is_str(&self) -> bool {
        matches!(self, Property::Str(_))
    }
}

impl Display for Property {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Property::Bool(b) => write!(f, "{b}"),
            Property::Int(i) => write!(f, "{i}"),
            Property::Float(fl) => write!(f, "{fl}"),
            Property::Str(s) => write!(f, "{s}"),
        }
    }
}

// Some helper functions to extract instructions and their metrics together with
// error handling

fn get_instruction<'a>(isa: &'a LockedISA<'_>, id: u64) -> Result<&'a Instruction, Error> {
    isa.get(&id).ok_or(Error::InstructionNotFound(id))
}

fn get_space(instruction: &Instruction) -> Result<u64, Error> {
    instruction
        .space(None)
        .ok_or(Error::CannotExtractSpace(instruction.id()))
}

fn get_time(instruction: &Instruction) -> Result<u64, Error> {
    instruction
        .time(None)
        .ok_or(Error::CannotExtractTime(instruction.id()))
}

fn get_error_rate(instruction: &Instruction) -> Result<f64, Error> {
    instruction
        .error_rate(None)
        .ok_or(Error::CannotExtractErrorRate(instruction.id()))
}

fn get_error_rate_by_id(isa: &LockedISA<'_>, id: u64) -> Result<f64, Error> {
    let instr = get_instruction(isa, id)?;
    instr
        .error_rate(None)
        .ok_or(Error::CannotExtractErrorRate(id))
}

/// Estimates all (trace, ISA) combinations in parallel, returning only the
/// successful results collected into an [`EstimationCollection`].
///
/// This uses a shared atomic counter as a lock-free work queue.  Each worker
/// thread atomically claims the next job index, maps it to a `(trace, isa)`
/// pair, and runs the estimation.  This keeps all available cores busy until
/// the last job completes.
///
/// # Work distribution
///
/// Jobs are numbered `0 .. traces.len() * isas.len()`.  For job index `j`:
///   - `trace_idx = j / isas.len()`
///   - `isa_idx   = j % isas.len()`
///
/// Each worker accumulates results locally and sends them back over a bounded
/// channel once it runs out of work, avoiding contention on the shared
/// collection.
#[must_use]
pub fn estimate_parallel<'a>(
    traces: &[&'a Trace],
    isas: &[&'a ISA],
    max_error: Option<f64>,
    post_process: bool,
) -> EstimationCollection {
    let total_jobs = traces.len() * isas.len();
    let num_isas = isas.len();

    // Shared atomic counter acts as a lock-free work queue.  Workers call
    // fetch_add to claim the next job index.
    let next_job = AtomicUsize::new(0);

    let mut collection = EstimationCollection::new();
    collection.set_total_jobs(total_jobs);

    std::thread::scope(|scope| {
        let num_threads = std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(1);

        // Bounded channel so each worker can send its batch of results back
        // to the main thread without unbounded buffering.
        let (tx, rx) = std::sync::mpsc::sync_channel(num_threads);

        for _ in 0..num_threads {
            let tx = tx.clone();
            let next_job = &next_job;
            scope.spawn(move || {
                let mut local_results = Vec::new();
                loop {
                    // Atomically claim the next job.  Relaxed ordering is
                    // sufficient because there is no dependent data between
                    // jobs — each (trace, isa) pair is independent.
                    let job = next_job.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if job >= total_jobs {
                        break;
                    }

                    // Map the flat job index to a (trace, ISA) pair.
                    let trace_idx = job / num_isas;
                    let isa_idx = job % num_isas;

                    if let Ok(mut estimation) = traces[trace_idx].estimate(isas[isa_idx], max_error)
                    {
                        estimation.set_isa_index(isa_idx);
                        estimation.set_trace_index(trace_idx);

                        local_results.push(estimation);
                    }
                }
                // Send all results from this worker in one batch.
                let _ = tx.send(local_results);
            });
        }
        // Drop the cloned sender so the receiver iterator terminates once all
        // workers have finished.
        drop(tx);

        // Collect results from all workers into the shared collection.
        let mut successful = 0;
        for local_results in rx {
            if post_process {
                for result in &local_results {
                    collection.push_summary(ResultSummary {
                        trace_index: result.trace_index().unwrap_or(0),
                        isa_index: result.isa_index().unwrap_or(0),
                        qubits: result.qubits(),
                        runtime: result.runtime(),
                    });
                }
            }
            successful += local_results.len();
            collection.extend(local_results.into_iter());
        }
        collection.set_successful_estimates(successful);
    });

    // Attach ISAs only to Pareto-surviving results, avoiding O(M) HashMap
    // clones for discarded results.
    for result in collection.iter_mut() {
        if let Some(idx) = result.isa_index() {
            result.set_isa(isas[idx].clone());
        }
    }

    collection
}

/// A node in the provenance graph along with pre-computed (space, time) values
/// for pruning.
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct NodeProfile {
    node_index: usize,
    space: u64,
    time: u64,
}

/// A single entry in a combination of instruction choices for estimation.
#[derive(Clone, Copy, Hash, Eq, PartialEq)]
struct CombinationEntry {
    instruction_id: u64,
    node: NodeProfile,
}

/// Per-slot pruning witnesses: maps a context hash to the `(space, time)`
/// pairs observed in successful estimations.
type SlotWitnesses = RwLock<FxHashMap<u64, Vec<(u64, u64)>>>;

/// Computes a hash of the combination context (all slots except the excluded
/// one).  Two combinations that agree on every slot except `exclude_idx`
/// produce the same context hash.
fn combination_context_hash(combination: &[CombinationEntry], exclude_idx: usize) -> u64 {
    let mut hasher = DefaultHasher::new();
    for (i, entry) in combination.iter().enumerate() {
        if i != exclude_idx {
            entry.instruction_id.hash(&mut hasher);
            entry.node.node_index.hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Checks whether a combination is dominated by a previously successful one.
///
/// A combination is prunable if, for any instruction slot, there exists a
/// successful combination with the same instructions in all other slots and
/// an instruction at that slot with `space <=` and `time <=`.
fn is_dominated(combination: &[CombinationEntry], trace_pruning: &[SlotWitnesses]) -> bool {
    for (slot_idx, entry) in combination.iter().enumerate() {
        let ctx_hash = combination_context_hash(combination, slot_idx);
        let map = trace_pruning[slot_idx]
            .read()
            .expect("Pruning lock poisoned");
        if map.get(&ctx_hash).is_some_and(|w| {
            w.iter()
                .any(|&(ws, wt)| ws <= entry.node.space && wt <= entry.node.time)
        }) {
            return true;
        }
    }
    false
}

/// Records a successful estimation as a pruning witness for future
/// combinations.
fn record_success(combination: &[CombinationEntry], trace_pruning: &[SlotWitnesses]) {
    for (slot_idx, entry) in combination.iter().enumerate() {
        let ctx_hash = combination_context_hash(combination, slot_idx);
        let mut map = trace_pruning[slot_idx]
            .write()
            .expect("Pruning lock poisoned");
        map.entry(ctx_hash)
            .or_default()
            .push((entry.node.space, entry.node.time));
    }
}

#[derive(Default)]
struct ISAIndex {
    index: FxHashMap<Vec<CombinationEntry>, usize>,
    isas: Vec<ISA>,
}

impl From<ISAIndex> for Vec<ISA> {
    fn from(value: ISAIndex) -> Self {
        value.isas
    }
}

impl ISAIndex {
    pub fn push(&mut self, combination: &Vec<CombinationEntry>, isa: &ISA) -> usize {
        if let Some(&idx) = self.index.get(combination) {
            idx
        } else {
            let idx = self.isas.len();
            self.isas.push(isa.clone());
            self.index.insert(combination.clone(), idx);
            idx
        }
    }
}

/// Generates the cartesian product of `id_and_nodes` and pushes each
/// combination directly into `jobs`, avoiding intermediate allocations.
///
/// The cartesian product is enumerated using mixed-radix indexing.  Given
/// dimensions with sizes `[n0, n1, n2, …]`, the total number of combinations
/// is `n0 * n1 * n2 * …`.  Each combination index `i` in `0..total` uniquely
/// identifies one element from every dimension: the index into dimension `d` is
/// `(i / (n0 * n1 * … * n(d-1))) % nd`, which we compute incrementally by
/// repeatedly taking `i % nd` and then dividing `i` by `nd`.  This is
/// analogous to extracting digits from a number in a mixed-radix system.
fn push_cartesian_product(
    id_and_nodes: &[(u64, Vec<NodeProfile>)],
    trace_idx: usize,
    jobs: &mut Vec<(usize, Vec<CombinationEntry>)>,
    max_slots: &mut usize,
) {
    // The product of all dimension sizes gives the total number of
    // combinations.  If any dimension is empty the product is zero and there
    // are no valid combinations to generate.
    let total: usize = id_and_nodes.iter().map(|(_, nodes)| nodes.len()).product();
    if total == 0 {
        return;
    }

    *max_slots = (*max_slots).max(id_and_nodes.len());
    jobs.reserve(total);

    // Enumerate every combination by treating the combination index `i` as a
    // mixed-radix number.  The inner loop "peels off" one digit per dimension:
    //   node_idx = i % nodes.len()   — selects this dimension's element
    //   i       /= nodes.len()       — shifts to the next dimension's digit
    // After processing all dimensions, `i` is exhausted (becomes 0), and
    // `combo` contains exactly one entry per instruction id.
    for mut i in 0..total {
        let mut combo = Vec::with_capacity(id_and_nodes.len());
        for (id, nodes) in id_and_nodes {
            let node_idx = i % nodes.len();
            i /= nodes.len();
            let profile = nodes[node_idx];
            combo.push(CombinationEntry {
                instruction_id: *id,
                node: profile,
            });
        }
        jobs.push((trace_idx, combo));
    }
}

#[must_use]
#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
pub fn estimate_with_graph(
    traces: &[&Trace],
    graph: &Arc<RwLock<ProvenanceGraph>>,
    max_error: Option<f64>,
    post_process: bool,
) -> EstimationCollection {
    let max_error = max_error.unwrap_or(1.0);

    // Phase 1: Pre-compute all (trace_index, combination) jobs sequentially.
    // This reads the provenance graph once per trace and generates the
    // cartesian product of Pareto-filtered nodes.  Each node carries
    // pre-computed (space, time) values for dominance pruning in Phase 2.
    let mut jobs: Vec<(usize, Vec<CombinationEntry>)> = Vec::new();

    // Use the maximum number of instruction slots across all combinations to
    // size the pruning witness structure.  This will updated while we generate
    // jobs.
    let mut max_slots = 0;

    for (trace_idx, trace) in traces.iter().enumerate() {
        if trace.base_error() > max_error {
            continue;
        }

        let required = trace.required_instruction_ids(Some(max_error));

        let graph_lock = graph.read().expect("Graph lock poisoned");
        let id_and_nodes: Vec<_> = required
            .constraints()
            .iter()
            .filter_map(|constraint| {
                graph_lock.pareto_nodes(constraint.id()).map(|nodes| {
                    (
                        constraint.id(),
                        nodes
                            .iter()
                            .filter(|&&node| {
                                // Filter out nodes that don't meet the constraint bounds.
                                let instruction = graph_lock.instruction(node);
                                constraint.error_rate().is_none_or(|c| {
                                    c.evaluate(&instruction.error_rate(Some(1)).unwrap_or(0.0))
                                })
                            })
                            .map(|&node| {
                                let instruction = graph_lock.instruction(node);
                                let space = instruction.space(Some(1)).unwrap_or(0);
                                let time = instruction.time(Some(1)).unwrap_or(0);
                                NodeProfile {
                                    node_index: node,
                                    space,
                                    time,
                                }
                            })
                            .collect::<Vec<_>>(),
                    )
                })
            })
            .collect();
        drop(graph_lock);

        if id_and_nodes.len() != required.len() {
            // If any required instruction is missing from the graph, we can't
            // run any estimation for this trace.
            continue;
        }

        push_cartesian_product(&id_and_nodes, trace_idx, &mut jobs, &mut max_slots);
    }

    // Sort jobs so that combinations with smaller total (space + time) are
    // processed first.  This maximises the effectiveness of dominance pruning
    // because successful "cheap" combinations establish witnesses that let us
    // skip more expensive ones.
    jobs.sort_by_key(|(_, combo)| {
        combo
            .iter()
            .map(|entry| entry.node.space + entry.node.time)
            .sum::<u64>()
    });

    let total_jobs = jobs.len();

    // Phase 2: Run estimations in parallel with dominance-based pruning.
    //
    // For each instruction slot in a combination, we track (space, time)
    // witnesses from successful estimations keyed by the "context", which is a
    // hash of the node indices in all *other* slots.  Before running an
    // estimation, we check every slot: if a witness with space ≤ and time ≤
    // exists for that context, the combination is dominated and skipped.
    let next_job = AtomicUsize::new(0);

    let pruning_witnesses: Vec<Vec<_>> = repeat_with(|| {
        repeat_with(|| RwLock::new(FxHashMap::default()))
            .take(max_slots)
            .collect()
    })
    .take(traces.len())
    .collect();

    // There are no explicit ISAs in this estimation function, as we create them
    // on the fly from the graph nodes.  For successful jobs, we will attach the
    // ISAs to the results collection in a vector with the ISA index addressing
    // that vector.  In order to avoid storing duplicate ISAs we hash the ISA
    // index.
    let isa_index = Arc::new(RwLock::new(ISAIndex::default()));

    let mut collection = EstimationCollection::new();
    collection.set_total_jobs(total_jobs);

    std::thread::scope(|scope| {
        let num_threads = std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(1);

        let (tx, rx) = std::sync::mpsc::sync_channel(num_threads);

        for _ in 0..num_threads {
            let tx = tx.clone();
            let next_job = &next_job;
            let jobs = &jobs;
            let pruning_witnesses = &pruning_witnesses;
            let isa_index = Arc::clone(&isa_index);
            scope.spawn(move || {
                let mut local_results = Vec::new();
                loop {
                    let job_idx = next_job.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if job_idx >= total_jobs {
                        break;
                    }

                    let (trace_idx, combination) = &jobs[job_idx];

                    // Dominance pruning: skip if a cheaper instruction at any
                    // slot already succeeded with the same surrounding context.
                    if is_dominated(combination, &pruning_witnesses[*trace_idx]) {
                        continue;
                    }

                    let mut isa = ISA::with_graph(graph.clone());
                    for entry in combination {
                        isa.add_node(entry.instruction_id, entry.node.node_index);
                    }

                    if let Ok(mut result) = traces[*trace_idx].estimate(&isa, Some(max_error)) {
                        let isa_idx = isa_index
                            .write()
                            .expect("RwLock should not be poisoned")
                            .push(combination, &isa);
                        result.set_isa_index(isa_idx);

                        result.set_trace_index(*trace_idx);

                        local_results.push(result);
                        record_success(combination, &pruning_witnesses[*trace_idx]);
                    }
                }
                let _ = tx.send(local_results);
            });
        }
        drop(tx);

        let mut successful = 0;
        for local_results in rx {
            if post_process {
                for result in &local_results {
                    collection.push_summary(ResultSummary {
                        trace_index: result.trace_index().unwrap_or(0),
                        isa_index: result.isa_index().unwrap_or(0),
                        qubits: result.qubits(),
                        runtime: result.runtime(),
                    });
                }
            }
            successful += local_results.len();
            collection.extend(local_results.into_iter());
        }
        collection.set_successful_estimates(successful);
    });

    let isa_index = Arc::try_unwrap(isa_index)
        .ok()
        .expect("all threads joined; Arc refcount should be 1")
        .into_inner()
        .expect("RwLock should not be poisoned");

    // Attach ISAs only to Pareto-surviving results, avoiding O(M) HashMap
    // clones for discarded results.
    for result in collection.iter_mut() {
        if let Some(idx) = result.isa_index() {
            result.set_isa(isa_index.isas[idx].clone());
        }
    }

    collection.set_isas(isa_index.into());

    collection
}
