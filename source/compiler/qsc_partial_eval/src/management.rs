// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{ops::Range, rc::Rc};

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
    qubits_in_use: Vec<bool>,
    reserved_qubits: FxHashSet<usize>,
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
        self.qubits_in_use.len()
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

        let qubit = if let Some(qubit) = self.qubits_in_use.iter().position(|in_use| !in_use) {
            self.qubits_in_use[qubit] = true;
            qubit
        } else {
            self.qubits_in_use.push(true);
            self.qubits_in_use.len() - 1
        };
        self.allocate_qubit_at(qubit)
    }

    /// Dedicated storage remains unavailable to ordinary allocations even after its leases end.
    pub fn reserve_qubits(&mut self, count: usize) -> Range<usize> {
        let start = self.qubits_in_use.len();
        self.qubits_in_use.resize(start + count, true);
        let slots = start..self.qubits_in_use.len();
        self.reserved_qubits.extend(slots.clone());
        slots
    }

    pub fn allocate_reserved_qubit(&mut self, slots: &Range<usize>) -> Option<QubitRef> {
        // Local-qubit lifetime proofs exclude parallel/delayed-release scopes.
        if self.is_delaying_release() {
            return None;
        }
        let slot = slots.clone().find(|slot| {
            self.reserved_qubits.contains(slot)
                && !self
                    .qubit_id_map
                    .iter()
                    .any(|(_, physical)| physical == slot)
        })?;
        Some(self.allocate_qubit_at(slot))
    }

    fn allocate_qubit_at(&mut self, qubit: usize) -> QubitRef {
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
        let physical = self.qubit_id_map[qubit.0];
        if !self.reserved_qubits.contains(&physical) {
            self.qubits_in_use[physical] = false;
        }
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

#[cfg(test)]
mod tests {
    use super::ResourceManager;

    #[test]
    fn released_reservations_cannot_be_reused_by_ordinary_allocations() {
        let mut manager = ResourceManager::default();
        let caller = manager.allocate_qubit();
        let slots = manager.reserve_qubits(2);
        let first = manager
            .allocate_reserved_qubit(&slots)
            .expect("first lease");
        let second = manager
            .allocate_reserved_qubit(&slots)
            .expect("second lease");
        assert_eq!(
            (manager.map_qubit(&first), manager.map_qubit(&second)),
            (1, 2)
        );
        assert!(manager.allocate_reserved_qubit(&slots).is_none());
        manager.release_qubit(&first);
        manager.release_qubit(&second);
        assert!(first.try_deref().is_none());
        assert!(second.try_deref().is_none());
        let later = manager.allocate_qubit();
        assert_eq!(manager.map_qubit(&later), 3);
        manager.release_qubit(&caller);
        let reused = manager.allocate_qubit();
        assert_eq!(manager.map_qubit(&reused), 0);
        assert_eq!(manager.qubit_count(), 4);
    }

    #[test]
    fn independent_reservations_reuse_only_their_own_released_slots() {
        let mut manager = ResourceManager::default();
        let caller = manager.allocate_qubit();
        manager.release_qubit(&caller);
        let outer_slots = manager.reserve_qubits(1);
        let helper_slots = manager.reserve_qubits(1);
        let outer = manager
            .allocate_reserved_qubit(&outer_slots)
            .expect("outer lease");
        let helper = manager
            .allocate_reserved_qubit(&helper_slots)
            .expect("helper lease");
        assert_eq!(
            (manager.map_qubit(&outer), manager.map_qubit(&helper)),
            (1, 2)
        );
        manager.release_qubit(&outer);
        let repeated = manager
            .allocate_reserved_qubit(&outer_slots)
            .expect("repeated lease");
        assert_eq!(manager.map_qubit(&repeated), 1);
        assert_eq!(manager.map_qubit(&helper), 2);
        let ordinary = manager.allocate_qubit();
        assert_eq!(manager.map_qubit(&ordinary), 0);
        assert_eq!(manager.qubit_count(), 3);
    }

    #[test]
    fn delayed_release_keeps_ordinary_accounting_and_excludes_reserved_leases() {
        let mut manager = ResourceManager::default();
        let slots = manager.reserve_qubits(1);
        manager.start_delayed_release_layer(None);
        assert!(manager.allocate_reserved_qubit(&slots).is_none());
        let ordinary = manager.allocate_qubit();
        assert_eq!(manager.map_qubit(&ordinary), 1);
        manager.release_qubit(&ordinary);
        manager.end_delayed_release_layer();
        assert!(ordinary.try_deref().is_none());
        let reserved_qubit = manager
            .allocate_reserved_qubit(&slots)
            .expect("reserved lease");
        assert_eq!(manager.map_qubit(&reserved_qubit), 0);
        manager.release_qubit(&reserved_qubit);
        let ordinary = manager.allocate_qubit();
        assert_eq!(manager.map_qubit(&ordinary), 1);
        assert_eq!(manager.qubit_count(), 2);
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
