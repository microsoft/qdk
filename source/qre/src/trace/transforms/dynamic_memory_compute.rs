// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

use crate::{
    Error, Property, Trace, TraceTransform,
    instruction_ids::{MEMORY, READ_FROM_MEMORY, WRITE_TO_MEMORY},
    property_keys::{ALGORITHM_COMPUTE_QUBITS, ALGORITHM_MEMORY_QUBITS},
    trace::{Block, Operation},
};

#[cfg(test)]
mod tests;

/// Specifies the compute capacity either as an absolute qubit count or as a
/// percentage of the input trace's compute qubits.
pub enum ComputeCapacity {
    /// An absolute number of compute qubits.
    Absolute(u64),
    /// A percentage (0.0–1.0) of the input trace's compute qubits, rounded
    /// down but clamped to at least 1.
    Percentage(f64),
}

/// Strategy for selecting which qubit to evict from the compute area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EvictionStrategy {
    /// Evict the first suitable qubit found (fast, but no optimization).
    FirstAvailable,
    /// Evict the least recently used qubit.
    #[default]
    LeastRecentlyUsed,
    /// Evict the least frequently used qubit.
    LeastFrequentlyUsed,
}

pub struct DynamicMemoryCompute {
    /// How the compute capacity is specified.
    compute_capacity: ComputeCapacity,
    /// Which eviction strategy to use.
    eviction_strategy: EvictionStrategy,
}

impl DynamicMemoryCompute {
    #[must_use]
    #[allow(unused)]
    pub fn new(compute_capacity: u64) -> Self {
        Self {
            compute_capacity: ComputeCapacity::Absolute(compute_capacity),
            eviction_strategy: EvictionStrategy::default(),
        }
    }

    #[must_use]
    #[allow(unused)]
    pub fn with_percentage(percentage: f64) -> Self {
        Self {
            compute_capacity: ComputeCapacity::Percentage(percentage),
            eviction_strategy: EvictionStrategy::default(),
        }
    }

    #[must_use]
    #[allow(unused)]
    pub fn with_strategy(mut self, strategy: EvictionStrategy) -> Self {
        self.eviction_strategy = strategy;
        self
    }

    /// Resolve the effective capacity for a given trace.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    fn effective_capacity(&self, trace: &Trace) -> u64 {
        match self.compute_capacity {
            ComputeCapacity::Absolute(n) => n,
            ComputeCapacity::Percentage(p) => {
                let raw = (trace.compute_qubits() as f64 * p).floor() as u64;
                raw.max(1)
            }
        }
    }
}

/// Tracks qubit usage for eviction decisions.
#[derive(Clone)]
enum EvictionTracker {
    /// No tracking — pick the first available candidate.
    FirstAvailable,
    /// Track recency via a deque (most recent at front).
    Lru(VecDeque<u64>),
    /// Track frequency via a counter map.
    Lfu(FxHashMap<u64, u64>),
}

impl EvictionTracker {
    #[expect(clippy::cast_possible_truncation)]
    fn new(strategy: EvictionStrategy, capacity: u64) -> Self {
        match strategy {
            EvictionStrategy::FirstAvailable => Self::FirstAvailable,
            EvictionStrategy::LeastRecentlyUsed => {
                Self::Lru(VecDeque::with_capacity(capacity as usize))
            }
            EvictionStrategy::LeastFrequentlyUsed => Self::Lfu(FxHashMap::default()),
        }
    }

    /// Record that `qubit` was just used (placed into or accessed in compute).
    fn touch(&mut self, qubit: u64) {
        match self {
            Self::FirstAvailable => {}
            Self::Lru(deque) => {
                if let Some(pos) = deque.iter().position(|&q| q == qubit) {
                    deque.remove(pos);
                }
                deque.push_front(qubit);
            }
            Self::Lfu(freq) => {
                *freq.entry(qubit).or_insert(0) += 1;
            }
        }
    }

    /// Record that `qubit` was removed from the compute area.
    fn remove(&mut self, qubit: u64) {
        match self {
            Self::FirstAvailable => {}
            Self::Lru(deque) => {
                if let Some(pos) = deque.iter().position(|&q| q == qubit) {
                    deque.remove(pos);
                }
            }
            Self::Lfu(freq) => {
                freq.remove(&qubit);
            }
        }
    }

    /// Pick which qubit to evict from the compute area.  `candidates` are all
    /// occupied qubits not in `needed`.
    fn pick_eviction(&self, slot_to_qubit: &[Option<u64>], needed: &FxHashSet<u64>) -> u64 {
        match self {
            Self::FirstAvailable => {
                // Pick the first slot whose occupant is not in `needed`.
                for q in slot_to_qubit.iter().flatten() {
                    if !needed.contains(q) {
                        return *q;
                    }
                }
                unreachable!("gate arity check guarantees an eviction candidate exists");
            }
            Self::Lru(deque) => {
                // Least recently used is at the back of the deque.
                for q in deque.iter().rev() {
                    if !needed.contains(q) {
                        return *q;
                    }
                }
                unreachable!("gate arity check guarantees an eviction candidate exists");
            }
            Self::Lfu(freq) => {
                // Least frequently used: find the qubit with the lowest
                // frequency among candidates.
                let mut best: Option<(u64, u64)> = None; // (qubit, freq)
                for q in slot_to_qubit.iter().flatten() {
                    if needed.contains(q) {
                        continue;
                    }
                    let f = freq.get(q).copied().unwrap_or(0);
                    if best.is_none_or(|(_, bf)| f < bf) {
                        best = Some((*q, f));
                    }
                }
                best.expect("gate arity check guarantees an eviction candidate exists")
                    .0
            }
        }
    }
}

/// Mutable state threaded through the recursive block processing.
#[derive(Clone)]
struct TransformState {
    /// Which logical qubit occupies each compute slot (`None` = empty).
    slot_to_qubit: Vec<Option<u64>>,
    /// Reverse map: logical qubit → compute slot index.
    qubit_to_slot: FxHashMap<u64, u64>,
    /// Logical qubit → memory location ID (only for qubits in memory).
    qubit_to_memory: FxHashMap<u64, u64>,
    /// All logical qubits encountered so far (across the whole trace).
    known_qubits: FxHashSet<u64>,
    /// Pool of freed memory location IDs available for reuse.
    free_memory_slots: Vec<u64>,
    /// Next fresh memory qubit ID to allocate (high-water mark).
    next_memory_id: u64,
    /// Tracks qubit usage for eviction decisions.
    eviction_tracker: EvictionTracker,
}

impl TransformState {
    #[expect(clippy::cast_possible_truncation)]
    fn new(capacity: u64, strategy: EvictionStrategy) -> Self {
        Self {
            slot_to_qubit: vec![None; capacity as usize],
            qubit_to_slot: FxHashMap::default(),
            qubit_to_memory: FxHashMap::default(),
            known_qubits: FxHashSet::default(),
            free_memory_slots: Vec::new(),
            next_memory_id: capacity,
            eviction_tracker: EvictionTracker::new(strategy, capacity),
        }
    }
}

impl TraceTransform for DynamicMemoryCompute {
    fn transform(&self, trace: &Trace) -> Result<Trace, Error> {
        if trace.has_memory_qubits() {
            return Err(Error::UnsupportedInstruction {
                id: MEMORY,
                name: "DynamicMemory",
            });
        }

        let capacity = self.effective_capacity(trace);

        if capacity >= trace.compute_qubits() {
            return Ok(trace.clone());
        }

        if capacity == 0 {
            return Err(Error::ZeroComputeCapacity);
        }

        let mut transformed = trace.clone_empty(Some(capacity));
        let mut state = TransformState::new(capacity, self.eviction_strategy);

        process_block(
            &mut state,
            &trace.block,
            transformed.root_block_mut(),
            capacity,
        )?;

        let num_known = state.known_qubits.len() as u64;
        if num_known > capacity {
            transformed.set_memory_qubits(num_known - capacity);
        }
        // Replace the pre-placement values copied by `clone_empty`.
        transformed.set_property(
            ALGORITHM_COMPUTE_QUBITS,
            Property::Int(capacity.cast_signed()),
        );
        transformed.set_property(
            ALGORITHM_MEMORY_QUBITS,
            Property::Int(transformed.memory_qubits().unwrap_or(0).cast_signed()),
        );

        Ok(transformed)
    }
}

/// Recursively process every operation in `input`, emitting transformed
/// operations into `output`.
#[expect(clippy::cast_possible_truncation)]
fn process_block(
    state: &mut TransformState,
    input: &Block,
    output: &mut Block,
    capacity: u64,
) -> Result<(), Error> {
    for op in &input.operations {
        match op {
            Operation::GateOperation(gate) => {
                let needed: FxHashSet<u64> = gate.qubits.iter().copied().collect();

                if needed.len() as u64 > capacity {
                    return Err(Error::GateArityExceedsCapacity {
                        id: gate.id,
                        arity: needed.len() as u64,
                        capacity,
                    });
                }

                // Bring every qubit needed by this gate into the compute area.
                for &qubit in &gate.qubits {
                    // Skip qubits that are already in the compute area.
                    if state.qubit_to_slot.contains_key(&qubit) {
                        state.eviction_tracker.touch(qubit);
                        continue;
                    }

                    let is_new = state.known_qubits.insert(qubit);

                    let slot = if let Some(s) = find_free_slot(&state.slot_to_qubit) {
                        s
                    } else {
                        let evicted = state
                            .eviction_tracker
                            .pick_eviction(&state.slot_to_qubit, &needed);
                        let evict_slot = *state
                            .qubit_to_slot
                            .get(&evicted)
                            .expect("evicted qubit must be in compute");

                        let mem = allocate_memory_slot(
                            &mut state.free_memory_slots,
                            &mut state.next_memory_id,
                        );
                        output.add_operation(WRITE_TO_MEMORY, vec![evict_slot, mem], vec![]);
                        state.qubit_to_memory.insert(evicted, mem);
                        state.qubit_to_slot.remove(&evicted);
                        state.slot_to_qubit[evict_slot as usize] = None;
                        state.eviction_tracker.remove(evicted);

                        evict_slot
                    };

                    if is_new {
                        // First encounter: place directly (lazy placement).
                    } else {
                        // Previously evicted — read back from memory.
                        let mem_location = state
                            .qubit_to_memory
                            .remove(&qubit)
                            .expect("qubit must be in memory");
                        output.add_operation(READ_FROM_MEMORY, vec![mem_location, slot], vec![]);
                        state.free_memory_slots.push(mem_location);
                    }

                    state.slot_to_qubit[slot as usize] = Some(qubit);
                    state.qubit_to_slot.insert(qubit, slot);
                    state.eviction_tracker.touch(qubit);
                }

                // Emit the gate with remapped compute-slot qubit IDs.
                let mapped_qubits: Vec<u64> = gate
                    .qubits
                    .iter()
                    .map(|q| {
                        *state
                            .qubit_to_slot
                            .get(q)
                            .expect("qubit must be in compute area")
                    })
                    .collect();
                output.add_operation(gate.id, mapped_qubits, gate.params.clone());
            }

            Operation::BlockOperation(inner) => {
                // A block that never runs must not change placement.
                if inner.repetitions == 0 {
                    continue;
                }

                // The first iteration of a repeated block starts from the
                // state before the block and brings the body's qubits into
                // compute, so it is emitted on its own.  Under LRU eviction
                // every later iteration then starts with the same qubits
                // resident in the same recency order, so one copy of the body
                // reproduces the unrolled memory traffic exactly.
                // Compute-slot and memory-location labels can still permute
                // between iterations, so label-based metrics such as
                // per-qubit depth are approximate; apply `Unroll` first to
                // avoid this.
                let mut repetitions = inner.repetitions;
                if repetitions > 1 {
                    process_block(state, inner, output.add_block(1), capacity)?;
                    repetitions -= 1;
                }
                process_block(state, inner, output.add_block(repetitions), capacity)?;
            }
        }
    }
    Ok(())
}

/// Emit `WRITE_TO_MEMORY` / `READ_FROM_MEMORY` operations into `block` that
/// restore the compute-area layout to `entry`.
///
/// Phase 1 writes all "displaced" qubits (in compute but not at their entry
/// slot) to **fresh** memory locations.  This avoids clobbering memory
/// locations that Phase 2 reads from.
///
/// Phase 2 reads every entry qubit that is missing from its slot back from
/// wherever it currently resides in memory.
#[allow(unused)]
#[expect(clippy::cast_possible_truncation)]
fn emit_restore(
    state: &mut TransformState,
    entry: &TransformState,
    block: &mut Block,
    capacity: u64,
) {
    // Phase 1: evict displaced qubits to fresh memory.
    for slot in 0..capacity as usize {
        let current_q = state.slot_to_qubit[slot];
        let entry_q = entry.slot_to_qubit[slot];

        if current_q == entry_q {
            continue;
        }

        if let Some(q) = current_q {
            // Always allocate a fresh memory slot to avoid overwriting a
            // location that Phase 2 will read from.
            let mem = allocate_memory_slot(&mut state.free_memory_slots, &mut state.next_memory_id);
            block.add_operation(WRITE_TO_MEMORY, vec![slot as u64, mem], vec![]);
            state.qubit_to_memory.insert(q, mem);
            state.qubit_to_slot.remove(&q);
            state.slot_to_qubit[slot] = None;
        }
    }

    // Phase 2: load entry qubits back into their slots.
    for slot in 0..capacity as usize {
        let entry_q = entry.slot_to_qubit[slot];

        if state.slot_to_qubit[slot] == entry_q {
            continue;
        }

        if let Some(q) = entry_q {
            let mem = state
                .qubit_to_memory
                .remove(&q)
                .expect("entry qubit must be in memory for restore");
            block.add_operation(READ_FROM_MEMORY, vec![mem, slot as u64], vec![]);
            state.free_memory_slots.push(mem);
            state.slot_to_qubit[slot] = Some(q);
            state.qubit_to_slot.insert(q, slot as u64);
        }
    }
}

/// Return the index of the first empty compute slot, if any.
fn find_free_slot(slot_to_qubit: &[Option<u64>]) -> Option<u64> {
    slot_to_qubit
        .iter()
        .position(Option::is_none)
        .map(|i| i as u64)
}

/// Allocate a memory location, reusing a freed one when possible.
fn allocate_memory_slot(free_memory_slots: &mut Vec<u64>, next_memory_id: &mut u64) -> u64 {
    if let Some(slot) = free_memory_slots.pop() {
        slot
    } else {
        let slot = *next_memory_id;
        *next_memory_id += 1;
        slot
    }
}
