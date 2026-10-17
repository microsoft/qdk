// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{cmp::Reverse, collections::BinaryHeap, rc::Rc};

use num_bigint::BigUint;
use num_complex::Complex;
use qsc_data_structures::index_map::IndexMap;
use qsc_eval::{
    DelayedQubitReleaseStack,
    backend::Backend,
    val::{Qubit, QubitRef, Result, Value},
};
use qsc_rir::rir::{BlockId, CallableId, VariableId};
use rustc_hash::FxHashSet;

/// Manages IDs for resources needed while performing partial evaluation.
#[derive(Default)]
pub struct ResourceManager {
    num_qubits: usize,
    free_qubits: BinaryHeap<Reverse<usize>>,
    free_qubit_ids: BinaryHeap<Reverse<usize>>,
    next_qubit_id: usize,
    qubit_id_map: IndexMap<usize, usize>,
    qubit_tracker: FxHashSet<Rc<Qubit>>,
    delayed_release_qubits: DelayedQubitReleaseStack,
    next_callable: CallableId,
    next_block: BlockId,
    next_result_register: usize,
    next_var: usize,
}

impl ResourceManager {
    pub fn map_qubit(&self, q: &QubitRef) -> usize {
        let q = q.deref();
        *self
            .qubit_id_map
            .get(q.0)
            .expect("qubit id should be in map")
    }

    /// Count of qubits used.
    pub fn qubit_count(&self) -> usize {
        self.num_qubits
    }

    /// Count of results registers used.
    pub fn result_register_count(&self) -> usize {
        self.next_result_register
    }

    /// Allocates a qubit by favoring available qubit IDs before using new ones.
    pub fn allocate_qubit(&mut self) -> QubitRef {
        if let Some(qubit) = self.delayed_release_qubits.allocate_delayed_qubit() {
            return self
                .qubit_tracker
                .get(&qubit)
                .expect("qubit should be in map")
                .into();
        }

        let qubit = if let Some(Reverse(qubit)) = self.free_qubits.pop() {
            qubit
        } else {
            let qubit = self.num_qubits;
            self.num_qubits += 1;
            qubit
        };
        let next_id = if let Some(Reverse(id)) = self.free_qubit_ids.pop() {
            id
        } else {
            let id = self.next_qubit_id;
            self.next_qubit_id += 1;
            id
        };
        self.qubit_id_map.insert(next_id, qubit);
        let q = Rc::new(Qubit(next_id));
        self.qubit_tracker.insert(Rc::clone(&q));
        q.into()
    }

    /// Releases a qubit ID for future use.
    pub fn release_qubit(&mut self, q: &QubitRef) {
        let q = q.deref();
        if !self.delayed_release_qubits.delay_release_qubit(*q) {
            self.free_qubit_id(*q);
        }
    }

    pub fn start_delayed_release_layer(&mut self, limit: Option<usize>) {
        self.delayed_release_qubits.add_layer(limit);
    }

    pub fn end_delayed_release_layer(&mut self) {
        for qubit in self.delayed_release_qubits.remove_layer() {
            self.free_qubit_id(qubit);
        }
    }

    fn free_qubit_id(&mut self, qubit: Qubit) {
        self.free_qubits.push(Reverse(self.qubit_id_map[qubit.0]));
        self.free_qubit_ids.push(Reverse(qubit.0));
        self.qubit_id_map.remove(qubit.0);
        self.qubit_tracker.remove(&qubit);
    }

    pub fn is_delaying_release(&self) -> bool {
        !self.delayed_release_qubits.is_empty()
    }

    /// Gets the next block ID.
    pub fn next_block(&mut self) -> BlockId {
        let id = self.next_block;
        self.next_block = id.successor();
        id
    }

    /// Gets the next callable ID.
    pub fn next_callable(&mut self) -> CallableId {
        let id = self.next_callable;
        self.next_callable = id.successor();
        id
    }

    /// Gets the next result register ID.
    pub fn next_result_register(&mut self) -> Result {
        let result_id = self.next_result_register;
        self.next_result_register += 1;
        Result::Id(result_id)
    }

    /// Gets the next variable ID.
    pub fn next_var(&mut self) -> VariableId {
        let var_id = self.next_var;
        self.next_var += 1;
        var_id.into()
    }

    pub fn swap_qubit_ids(&mut self, q0: usize, q1: usize) {
        let id0 = *self
            .qubit_id_map
            .get(q0)
            .expect("qubit id should be in map");
        let id1 = *self
            .qubit_id_map
            .get(q1)
            .expect("qubit id should be in map");
        self.qubit_id_map.insert(q0, id1);
        self.qubit_id_map.insert(q1, id0);
    }
}

/// Custom backend meant to panic when most of its methods are called.
/// Since the partial evaluator is meant to generate instructions for most quantum operations, but we are also using
/// the evaluator for computations that are purely classical, the role of this backend is to catch instances of
/// quantum operations being simulated when they should not.
#[derive(Default)]
pub struct QuantumIntrinsicsChecker {}

impl Backend for QuantumIntrinsicsChecker {
    fn qubit_is_zero(&mut self, _q: usize) -> std::result::Result<bool, String> {
        // Because `qubit_is_zero` is called on every qubit release, this must return
        // true to avoid a panic.
        Ok(true)
    }

    // Needed for calls to `DumpMachine` and `DumpRegister`.
    fn capture_quantum_state(
        &mut self,
    ) -> std::result::Result<(Vec<(BigUint, Complex<f64>)>, usize), String> {
        Ok((Vec::new(), 0))
    }

    // Only intrinsic functions are supported here since they're the only ones that will be classically evaluated.
    fn custom_intrinsic(
        &mut self,
        name: &str,
        _arg: Value,
        _globals: &impl qsc_fir::fir::PackageStoreLookup,
    ) -> Option<std::result::Result<Value, String>> {
        match name {
            "BeginEstimateCaching" => Some(Ok(Value::Bool(true))),
            "IsResourceEstimating" => Some(Ok(Value::Bool(false))),
            "EndEstimateCaching"
            | "EnableMemoryComputeArchitecture"
            | "Load"
            | "Store"
            | "GlobalPhase"
            | "ConfigurePauliNoise"
            | "ConfigureQubitLoss"
            | "ApplyIdleNoise"
            | "PostSelectZ" => Some(Ok(Value::unit())),
            _ => None,
        }
    }
}
