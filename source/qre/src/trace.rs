// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::fmt::{Display, Formatter};

use rustc_hash::{FxHashMap, FxHashSet};

use crate::{Error, EstimationCollection, EstimationResult, FactoryResult, ISA, Instruction};

pub mod instruction_ids;
use instruction_ids::instruction_name;
#[cfg(test)]
mod tests;

mod transforms;
pub use transforms::{LatticeSurgery, PSSPC, TraceTransform};

#[derive(Clone, Default)]
pub struct Trace {
    block: Block,
    base_error: f64,
    compute_qubits: u64,
    memory_qubits: Option<u64>,
    resource_states: Option<FxHashMap<u64, u64>>,
    properties: FxHashMap<String, Property>,
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

    pub fn set_property(&mut self, key: String, value: Property) {
        self.properties.insert(key, value);
    }

    #[must_use]
    pub fn get_property(&self, key: &str) -> Option<&Property> {
        self.properties.get(key)
    }

    #[must_use]
    pub fn deep_iter(&self) -> TraceIterator<'_> {
        TraceIterator::new(&self.block)
    }

    #[must_use]
    pub fn depth(&self) -> u64 {
        self.block.depth()
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn estimate(&self, isa: &ISA, max_error: Option<f64>) -> Result<EstimationResult, Error> {
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
                let rate = get_error_rate_by_id(isa, *state_id)?;
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
            let instr = get_instruction(isa, gate.id)?;

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

        result.add_runtime(
            self.block
                .depth_and_used(Some(&|op: &Gate| {
                    let instr = get_instruction(isa, op.id)?;
                    Ok(instr.expect_time(Some(op.qubits.len() as u64)))
                }))?
                .0,
        );

        // ------------------------------------------------------------------
        // Factory overhead estimation. Each factory produces states at
        // a certain rate, so we need enough copies to meet the demand.
        // ------------------------------------------------------------------
        for (factory, count) in &factories {
            let instr = get_instruction(isa, *factory)?;
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

            result.add_qubits(copies * factory_space);
            result.add_factory_result(
                *factory,
                FactoryResult::new(copies, runs, *count, factory_error_rate),
            );
        }

        // Memory qubits
        if let Some(memory_qubits) = self.memory_qubits {
            // We need a MEMORY instruction in our ISA
            let memory = isa
                .get(&instruction_ids::MEMORY)
                .ok_or(Error::InstructionNotFound(instruction_ids::MEMORY))?;

            result.add_qubits(memory.expect_space(Some(memory_qubits)));

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

        result.set_isa(isa.clone());

        // Copy properties from the trace to the result
        for (key, value) in &self.properties {
            result.set_property(key.clone(), value.clone());
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
                writeln!(f, "@resource_state({res_id}, {amount})")?;
            }
        }
        write!(f, "{}", self.block)
    }
}

#[derive(Clone, Debug)]
pub enum Operation {
    GateOperation(Gate),
    BlockOperation(Block),
}

#[derive(Clone, Debug)]
pub struct Gate {
    id: u64,
    qubits: Vec<u64>,
    params: Vec<f64>,
}

#[derive(Clone, Debug)]
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
                    writeln!(f, "{indent_str}  {name}({params:?})({qubits:?})")?;
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
                        None => 1,
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
                None => {
                    self.stack.pop();
                }
            }
        }
    }
}

#[derive(Clone)]
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

fn get_instruction(isa: &ISA, id: u64) -> Result<&Instruction, Error> {
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

fn get_error_rate_by_id(isa: &ISA, id: u64) -> Result<f64, Error> {
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
) -> EstimationCollection {
    let total_jobs = traces.len() * isas.len();
    let num_isas = isas.len();

    // Shared atomic counter acts as a lock-free work queue.  Workers call
    // fetch_add to claim the next job index.
    let next_job = std::sync::atomic::AtomicUsize::new(0);

    let mut collection = EstimationCollection::new();
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

                    if let Ok(estimation) = traces[trace_idx].estimate(isas[isa_idx], max_error) {
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
        for local_results in rx {
            collection.extend(local_results.into_iter());
        }
    });

    collection
}
