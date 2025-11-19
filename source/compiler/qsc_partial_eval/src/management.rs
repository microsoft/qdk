// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::rc::Rc;

use num_bigint::BigUint;
use num_complex::Complex;
use qsc_data_structures::index_map::IndexMap;
use qsc_eval::{
    backend::Backend,
    val::{Qubit, QubitRef, Result, Value},
};
use qsc_rir::rir::{BlockId, CallableId, VariableId};
use rustc_hash::FxHashSet;

/// Manages IDs for resources needed while performing partial evaluation.
#[derive(Default)]
pub struct ResourceManager {
    qubits_in_use: Vec<bool>,
    qubit_id_map: IndexMap<usize, usize>,
    qubit_tracker: FxHashSet<Rc<Qubit>>,
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
        self.qubits_in_use.len()
    }

    /// Count of results registers used.
    pub fn result_register_count(&self) -> usize {
        self.next_result_register
    }

    /// Allocates a qubit by favoring available qubit IDs before using new ones.
    pub fn allocate_qubit(&mut self) -> QubitRef {
        let qubit = if let Some(qubit) = self.qubits_in_use.iter().position(|in_use| !in_use) {
            self.qubits_in_use[qubit] = true;
            qubit
        } else {
            self.qubits_in_use.push(true);
            self.qubits_in_use.len() - 1
        };
        let mut next_id = 0;
        // Iterate through the sequence of integers until we find one that is not present in the map.
        // This means that integer id is available for use as the qubit id that will map to the newly allocated qubit.
        loop {
            if !self.qubit_id_map.contains_key(next_id) {
                self.qubit_id_map.insert(next_id, qubit);
                break;
            }
            next_id += 1;
        }
        let q = Rc::new(Qubit(next_id));
        self.qubit_tracker.insert(Rc::clone(&q));
        q.into()
    }

    /// Releases a qubit ID for future use.
    pub fn release_qubit(&mut self, q: &QubitRef) {
        let qubit = self.map_qubit(q);
        self.qubits_in_use[qubit] = false;

        let q = q.deref();
        self.qubit_id_map.remove(q.0);
        self.qubit_tracker.remove(&q);
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
    fn qubit_is_zero(&mut self, _q: usize) -> bool {
        // Because `qubit_is_zero` is called on every qubit release, this must return
        // true to avoid a panic.
        true
    }

    // Needed for calls to `DumpMachine` and `DumpRegister`.
    fn capture_quantum_state(&mut self) -> (Vec<(BigUint, Complex<f64>)>, usize) {
        (Vec::new(), 0)
    }

    // Only intrinsic functions are supported here since they're the only ones that will be classically evaluated.
    fn custom_intrinsic(
        &mut self,
        name: &str,
        _arg: Value,
    ) -> Option<std::result::Result<Value, String>> {
        match name {
            "BeginEstimateCaching" => Some(Ok(Value::Bool(true))),
            "EndEstimateCaching"
            | "EnableMemoryComputeArchitecture"
            | "GlobalPhase"
            | "ConfigurePauliNoise"
            | "ConfigureQubitLoss"
            | "ApplyIdleNoise" => Some(Ok(Value::unit())),
            _ => None,
        }
    }
}
