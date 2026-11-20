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
use rustc_hash::{FxHashMap, FxHashSet};

/// Boolean facts used only for resource validity. They never specialize emitted code.
#[derive(Clone)]
pub(crate) enum ResourcePredicate {
    Flag(usize),
    Constant(bool),
    Not(Box<Self>),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
}

impl ResourcePredicate {
    fn evaluate(&self, flags: &FxHashMap<usize, bool>) -> Option<bool> {
        match self {
            Self::Flag(flag) => flags.get(flag).copied(),
            Self::Constant(value) => Some(*value),
            Self::Not(value) => value.evaluate(flags).map(|value| !value),
            Self::And(lhs, rhs) => match (lhs.evaluate(flags), rhs.evaluate(flags)) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            },
            Self::Or(lhs, rhs) => match (lhs.evaluate(flags), rhs.evaluate(flags)) {
                (Some(true), _) | (_, Some(true)) => Some(true),
                (Some(false), Some(false)) => Some(false),
                _ => None,
            },
        }
    }
}

#[derive(Clone, Default)]
struct ResourcePath {
    flags: FxHashMap<usize, bool>,
    // Retaining the Rc prevents pointer reuse while its release fact is live.
    released: FxHashMap<*const Qubit, Rc<Qubit>>,
}

#[derive(Clone)]
struct ResourceValidity(Vec<ResourcePath>);

impl Default for ResourceValidity {
    fn default() -> Self {
        Self(vec![ResourcePath::default()])
    }
}

impl ResourceValidity {
    fn merge(&mut self, other: Self) {
        for path in other.0 {
            if let Some(existing) = self
                .0
                .iter_mut()
                .find(|existing| existing.flags == path.flags)
            {
                existing.released.extend(path.released);
            } else {
                self.0.push(path);
            }
        }
    }
}

/// Manages IDs for resources needed while performing partial evaluation.
#[derive(Default)]
pub struct ResourceManager {
    num_qubits: usize,
    free_qubits: BinaryHeap<Reverse<usize>>,
    free_qubit_ids: BinaryHeap<Reverse<usize>>,
    qubit_id_map: IndexMap<usize, usize>,
    qubit_tracker: FxHashSet<Rc<Qubit>>,
    delayed_release_qubits: DelayedQubitReleaseStack,
    next_callable: CallableId,
    next_block: BlockId,
    next_result_register: usize,
    next_var: usize,
    validity: ResourceValidity,
    next_resource_flag: usize,
}

#[derive(Clone)]
pub(crate) struct QubitStateSnapshot {
    num_qubits: usize,
    free_qubits: BinaryHeap<Reverse<usize>>,
    free_qubit_ids: BinaryHeap<Reverse<usize>>,
    qubit_id_map: IndexMap<usize, usize>,
    qubit_tracker: FxHashSet<Rc<Qubit>>,
    validity: ResourceValidity,
}

impl ResourceManager {
    /// Register a caller-supplied qubit before evaluating any statements.
    /// This is an input boundary, not a way to revive released allocations.
    pub(crate) fn register_input_qubit(&mut self, q: &QubitRef) -> bool {
        let Some(q) = q.try_deref() else {
            return false;
        };
        if self.qubit_id_map.contains_key(q.0) {
            return self
                .qubit_tracker
                .get(&q)
                .is_some_and(|active| Rc::ptr_eq(active, &q));
        }
        let slot = self.num_qubits;
        self.num_qubits += 1;
        self.qubit_id_map.insert(q.0, slot);
        self.qubit_tracker.insert(q);
        true
    }

    pub(crate) fn save_qubit_state(&self) -> QubitStateSnapshot {
        QubitStateSnapshot {
            num_qubits: self.num_qubits,
            free_qubits: self.free_qubits.clone(),
            free_qubit_ids: self.free_qubit_ids.clone(),
            qubit_id_map: self.qubit_id_map.clone(),
            qubit_tracker: self.qubit_tracker.clone(),
            validity: self.validity.clone(),
        }
    }

    pub(crate) fn restore_qubit_state(&mut self, state: QubitStateSnapshot) {
        self.num_qubits = state.num_qubits;
        self.free_qubits = state.free_qubits;
        self.free_qubit_ids = state.free_qubit_ids;
        self.qubit_id_map = state.qubit_id_map;
        self.qubit_tracker = state.qubit_tracker;
        self.validity = state.validity;
    }

    pub(crate) fn merge_qubit_state(&mut self, other: QubitStateSnapshot) {
        self.validity.merge(other.validity);
        // Mutually exclusive branches can allocate distinct identities with the
        // same ID. Keep the current mapping and tracked identity on overlap.
        self.num_qubits = self.num_qubits.max(other.num_qubits);
        for (id, slot) in other.qubit_id_map.iter() {
            if !self.qubit_id_map.contains_key(id) {
                self.qubit_id_map.insert(id, *slot);
            }
        }
        self.qubit_tracker.extend(other.qubit_tracker);

        let occupied: FxHashSet<_> = self.qubit_id_map.iter().map(|(_, slot)| *slot).collect();
        self.free_qubits = (0..self.num_qubits)
            .filter(|slot| !occupied.contains(slot))
            .map(Reverse)
            .collect();
        // Released logical IDs have no surviving allocation identity. Different
        // branches can release different IDs for the same physical capacity, so
        // their union need not have the same size as the physical free pool.
        // Choose exactly one unused logical ID per free slot instead.
        self.free_qubit_ids = (0..)
            .filter(|id| !self.qubit_id_map.contains_key(*id))
            .take(self.free_qubits.len())
            .map(Reverse)
            .collect();
    }

    pub fn is_qubit_live(&self, q: &QubitRef) -> bool {
        q.try_deref().is_some_and(|q| {
            // A snapshot can keep a released qubit alive after its numeric ID
            // has been reused. Require the allocation currently tracked for it.
            self.is_qubit_reserved(&q)
                && self
                    .validity
                    .0
                    .iter()
                    .all(|path| !path.released.contains_key(&Rc::as_ptr(&q)))
        })
    }

    pub(crate) fn is_qubit_reserved(&self, q: &Rc<Qubit>) -> bool {
        self.qubit_id_map.contains_key(q.0)
            && self
                .qubit_tracker
                .get(q)
                .is_some_and(|active| Rc::ptr_eq(active, q))
    }

    pub(crate) fn qubit_entry_paths<'a>(
        &'a self,
        qubit: &'a Rc<Qubit>,
    ) -> impl Iterator<Item = (&'a FxHashMap<usize, bool>, bool)> {
        self.validity
            .0
            .iter()
            .map(move |path| (&path.flags, path.released.contains_key(&Rc::as_ptr(qubit))))
    }

    pub(crate) fn new_resource_flag(&mut self) -> usize {
        let flag = self.next_resource_flag;
        self.next_resource_flag += 1;
        flag
    }

    pub(crate) fn assign_resource_flag(&mut self, flag: usize, value: Option<&ResourcePredicate>) {
        for path in &mut self.validity.0 {
            if let Some(value) = value.and_then(|value| value.evaluate(&path.flags)) {
                path.flags.insert(flag, value);
            } else {
                path.flags.remove(&flag);
            }
        }
    }

    pub(crate) fn restrict_resource_paths(&mut self, predicate: &ResourcePredicate, value: bool) {
        self.validity.0.retain(|path| {
            predicate
                .evaluate(&path.flags)
                .is_none_or(|actual| actual == value)
        });
    }

    pub(crate) fn forget_resource_flags(&mut self, flags: impl Iterator<Item = usize>) {
        for flag in flags {
            for path in &mut self.validity.0 {
                path.flags.remove(&flag);
            }
        }
        let paths = std::mem::replace(&mut self.validity, ResourceValidity(Vec::new()));
        self.validity.merge(paths);
    }

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
            // A delayed physical slot can be reused, but the previous allocation
            // identity remains released even while aliases keep it alive.
            let identity = Rc::new(qubit);
            self.qubit_tracker.replace(Rc::clone(&identity));
            return identity.into();
        }

        let (qubit, next_id) = match (self.free_qubits.pop(), self.free_qubit_ids.pop()) {
            (Some(Reverse(qubit)), Some(Reverse(id))) => (qubit, id),
            (None, None) => {
                let qubit = self.num_qubits;
                let mut id = qubit;
                while self.qubit_id_map.contains_key(id) {
                    id += 1;
                }
                self.num_qubits += 1;
                (qubit, id)
            }
            _ => unreachable!("free qubit indices and IDs should be available together"),
        };
        self.qubit_id_map.insert(next_id, qubit);
        let q = Rc::new(Qubit(next_id));
        self.qubit_tracker.insert(Rc::clone(&q));
        q.into()
    }

    /// Releases a qubit ID for future use.
    pub fn release_qubit(&mut self, q: &QubitRef) {
        let q = q.deref();
        for path in &mut self.validity.0 {
            path.released.insert(Rc::as_ptr(&q), Rc::clone(&q));
        }
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

#[cfg(test)]
mod input_qubit_tests {
    use super::*;

    #[test]
    fn imported_qubit_preserves_identity_across_release_and_reuse() {
        let mut manager = ResourceManager::default();
        let external = Rc::new(Qubit(0));
        let original = QubitRef::from(&external);
        assert!(manager.register_input_qubit(&original));
        assert!(manager.register_input_qubit(&original));
        assert!(manager.is_qubit_live(&original));
        let conflicting = Rc::new(Qubit(0));
        assert!(!manager.register_input_qubit(&QubitRef::from(&conflicting)));
        let snapshot = manager.save_qubit_state();
        manager.release_qubit(&original);
        assert!(!manager.is_qubit_live(&original));
        let replacement = manager.allocate_qubit();
        let replacement_owner = replacement.deref();
        assert_eq!(replacement_owner.0, external.0, "the numeric ID is reused");
        assert!(!Rc::ptr_eq(&replacement_owner, &external));
        assert!(manager.is_qubit_live(&replacement));
        assert!(!manager.is_qubit_live(&original));
        manager.restore_qubit_state(snapshot);
        assert!(manager.is_qubit_live(&original));
        assert!(!manager.is_qubit_live(&replacement));
        let fresh = manager.allocate_qubit();
        assert_eq!(manager.map_qubit(&fresh), 1);
        assert!(manager.is_qubit_live(&original));
    }

    #[test]
    fn imported_expired_qubit_is_rejected() {
        let q = Rc::new(Qubit(2));
        let reference = QubitRef::from(&q);
        drop(q);
        assert!(!ResourceManager::default().register_input_qubit(&reference));
    }

    #[test]
    fn imported_sparse_ids_and_new_allocations_use_distinct_slots() {
        let mut manager = ResourceManager::default();
        let first = Rc::new(Qubit(42));
        let second = Rc::new(Qubit(0));
        let first_ref = QubitRef::from(&first);
        let second_ref = QubitRef::from(&second);
        assert!(manager.register_input_qubit(&first_ref));
        assert!(manager.register_input_qubit(&second_ref));
        let fresh = manager.allocate_qubit();
        assert_eq!(manager.map_qubit(&first_ref), 0);
        assert_eq!(manager.map_qubit(&second_ref), 1);
        assert_eq!(manager.map_qubit(&fresh), 2);
        assert!(manager.is_qubit_live(&fresh));
    }

    fn assert_free_pools(manager: &ResourceManager, slots: &[usize], ids: &[usize]) {
        let mut actual_slots: Vec<_> = manager.free_qubits.iter().map(|q| q.0).collect();
        let mut actual_ids: Vec<_> = manager.free_qubit_ids.iter().map(|q| q.0).collect();
        actual_slots.sort_unstable();
        actual_ids.sort_unstable();
        assert_eq!(actual_slots, slots);
        assert_eq!(actual_ids, ids);
        assert_eq!(actual_slots.len(), actual_ids.len());
        let mut occupied = FxHashSet::default();
        for (id, slot) in manager.qubit_id_map.iter() {
            assert!(*slot < manager.num_qubits);
            assert!(occupied.insert(*slot));
            assert!(!actual_slots.contains(slot));
            assert!(!actual_ids.contains(&id));
            assert!(manager.qubit_tracker.contains(&Qubit(id)));
        }
        assert_eq!(
            manager.qubit_tracker.len(),
            manager.qubit_id_map.iter().count()
        );
        assert_eq!(occupied.len() + actual_slots.len(), manager.num_qubits);
    }

    #[test]
    fn imported_id_at_fresh_slot_does_not_collide() {
        let mut manager = ResourceManager::default();
        let external = Rc::new(Qubit(1));
        let input = QubitRef::from(&external);
        assert!(manager.register_input_qubit(&input));
        let fresh = manager.allocate_qubit();
        assert_eq!(fresh.deref().0, 2);
        assert_eq!(manager.map_qubit(&fresh), 1);
        assert_eq!(manager.qubit_count(), 2);
        assert!(manager.is_qubit_live(&input));
        assert!(manager.is_qubit_live(&fresh));
        assert_free_pools(&manager, &[], &[]);
    }

    #[test]
    fn restore_recovers_free_pools_and_capacity() {
        let mut manager = ResourceManager::default();
        let q = manager.allocate_qubit();
        manager.release_qubit(&q);
        let snapshot = manager.save_qubit_state();
        let replacement = manager.allocate_qubit();
        let extra = manager.allocate_qubit();
        manager.restore_qubit_state(snapshot);
        assert_eq!(manager.qubit_count(), 1);
        assert!(!manager.is_qubit_live(&replacement));
        assert!(!manager.is_qubit_live(&extra));
        assert_free_pools(&manager, &[0], &[0]);
        let reused = manager.allocate_qubit();
        assert_eq!(manager.map_qubit(&reused), 0);
        assert_eq!(manager.qubit_count(), 1);
        assert_free_pools(&manager, &[], &[]);
    }

    #[test]
    fn merge_preserves_released_peak_with_implicit_false_branch() {
        let mut manager = ResourceManager::default();
        let before = manager.save_qubit_state();
        let first = manager.allocate_qubit();
        let second = manager.allocate_qubit();
        manager.release_qubit(&first);
        manager.release_qubit(&second);
        let after_true = manager.save_qubit_state();
        manager.restore_qubit_state(before);
        manager.merge_qubit_state(after_true);
        assert_eq!(manager.qubit_count(), 2);
        assert_free_pools(&manager, &[0, 1], &[0, 1]);
        let a = manager.allocate_qubit();
        let b = manager.allocate_qubit();
        assert_eq!(manager.map_qubit(&a), 0);
        assert_eq!(manager.map_qubit(&b), 1);
        assert_eq!(manager.qubit_count(), 2);
        assert!(!manager.is_qubit_live(&first));
        assert!(!manager.is_qubit_live(&second));
        assert_free_pools(&manager, &[], &[]);
    }

    #[test]
    fn merge_accepts_released_branch_locals_and_unequal_peaks() {
        let mut manager = ResourceManager::default();
        let shared = manager.allocate_qubit();
        let shared_owner = shared.deref();
        let before = manager.save_qubit_state();
        let locals: Vec<_> = (0..3).map(|_| manager.allocate_qubit()).collect();
        let true_local_owner = locals[0].deref();
        for q in &locals {
            manager.release_qubit(q);
        }
        let after_true = manager.save_qubit_state();
        manager.restore_qubit_state(before);
        let false_local = manager.allocate_qubit();
        assert_eq!(false_local.deref().0, true_local_owner.0);
        manager.release_qubit(&false_local);
        manager.merge_qubit_state(after_true);
        assert_eq!(manager.qubit_count(), 4);
        assert!(manager.is_qubit_live(&shared));
        assert!(Rc::ptr_eq(
            manager
                .qubit_tracker
                .get(&Qubit(0))
                .expect("shared qubit remains tracked"),
            &shared_owner
        ));
        assert!(!manager.is_qubit_live(&false_local));
        assert_free_pools(&manager, &[1, 2, 3], &[1, 2, 3]);
    }

    #[test]
    fn merge_sparse_ids_keeps_logical_and_physical_free_pools_separate() {
        let mut manager = ResourceManager::default();
        let external = Rc::new(Qubit(2));
        let input = QubitRef::from(&external);
        assert!(manager.register_input_qubit(&input));
        let before = manager.save_qubit_state();
        let first = manager.allocate_qubit();
        let second = manager.allocate_qubit();
        assert_eq!(first.deref().0, 1);
        assert_eq!(second.deref().0, 3);
        manager.release_qubit(&first);
        manager.release_qubit(&second);
        let after_true = manager.save_qubit_state();
        manager.restore_qubit_state(before);
        manager.merge_qubit_state(after_true);
        assert_eq!(manager.qubit_count(), 3);
        assert_free_pools(&manager, &[1, 2], &[0, 1]);
        let reused_first = manager.allocate_qubit();
        let reused_second = manager.allocate_qubit();
        assert_eq!(reused_first.deref().0, 0);
        assert_eq!(reused_second.deref().0, 1);
        assert!(manager.is_qubit_live(&input));
        assert_free_pools(&manager, &[], &[]);
    }

    #[test]
    fn merge_divergent_sparse_id_histories_rebuilds_only_needed_ids() {
        let mut manager = ResourceManager::default();
        let external = Rc::new(Qubit(2));
        let input = QubitRef::from(&external);
        assert!(manager.register_input_qubit(&input));
        let before = manager.save_qubit_state();
        let true_local = manager.allocate_qubit();
        assert_eq!(true_local.deref().0, 1);
        manager.release_qubit(&true_local);
        let after_true = manager.save_qubit_state();

        manager.restore_qubit_state(before);
        manager.release_qubit(&input);
        let false_first = manager.allocate_qubit();
        let false_second = manager.allocate_qubit();
        assert_eq!(false_first.deref().0, 2);
        assert_eq!(false_second.deref().0, 1);
        manager.release_qubit(&false_first);
        manager.release_qubit(&false_second);
        manager.merge_qubit_state(after_true);

        assert_eq!(manager.qubit_count(), 2);
        // The true path still reserves the input's slot, but the false path
        // released this identity, so an unconditional use is invalid.
        assert!(!manager.is_qubit_live(&input));
        assert_eq!(manager.map_qubit(&input), 0);
        assert!(!manager.is_qubit_live(&true_local));
        assert!(!manager.is_qubit_live(&false_first));
        assert!(!manager.is_qubit_live(&false_second));
        assert_free_pools(&manager, &[1], &[0]);
        let reused = manager.allocate_qubit();
        assert_eq!(manager.map_qubit(&reused), 1);
        assert_eq!(reused.deref().0, 0);
        assert_eq!(manager.qubit_count(), 2);
        assert!(!manager.is_qubit_live(&input));
        assert_free_pools(&manager, &[], &[]);
    }

    #[test]
    fn merge_accepts_different_valid_free_id_namespaces() {
        let mut manager = ResourceManager::default();
        let external = Rc::new(Qubit(2));
        let input = QubitRef::from(&external);
        assert!(manager.register_input_qubit(&input));
        let local = manager.allocate_qubit();
        manager.release_qubit(&local);
        let after_true = manager.save_qubit_state();

        // A valid alternate exit state with two released allocations. Its free
        // logical names differ from the true branch's, but its capacity agrees.
        // Unioning the names would leave {1, 3} for only one free physical slot
        // after the live input (ID 2, slot 0) is restored by the join.
        manager.restore_qubit_state(QubitStateSnapshot {
            num_qubits: 2,
            free_qubits: [Reverse(0), Reverse(1)].into_iter().collect(),
            free_qubit_ids: [Reverse(2), Reverse(3)].into_iter().collect(),
            qubit_id_map: IndexMap::default(),
            qubit_tracker: FxHashSet::default(),
            validity: Default::default(),
        });
        assert_free_pools(&manager, &[0, 1], &[2, 3]);
        manager.merge_qubit_state(after_true);
        assert_eq!(manager.qubit_count(), 2);
        assert!(manager.is_qubit_live(&input));
        assert_free_pools(&manager, &[1], &[0]);
        let reused = manager.allocate_qubit();
        assert_eq!(manager.map_qubit(&reused), 1);
        assert!(manager.is_qubit_live(&input));
        assert_free_pools(&manager, &[], &[]);
    }

    #[test]
    fn merge_keeps_current_identity_for_overlapping_branch_ids() {
        let mut manager = ResourceManager::default();
        let before = manager.save_qubit_state();
        let first = manager.allocate_qubit();
        let first_allocation = first.deref();
        let after_true = manager.save_qubit_state();
        manager.restore_qubit_state(before);
        let second = manager.allocate_qubit();
        assert_eq!(first.deref().0, second.deref().0);
        assert!(!Rc::ptr_eq(&first.deref(), &second.deref()));
        manager.merge_qubit_state(after_true);
        assert!(!manager.is_qubit_live(&first));
        assert!(manager.is_qubit_live(&second));
        assert!(!Rc::ptr_eq(&first_allocation, &second.deref()));
        assert_eq!(manager.map_qubit(&second), 0);
        assert_eq!(manager.qubit_count(), 1);
        assert_free_pools(&manager, &[], &[]);
    }

    #[test]
    fn merge_keeps_current_mappings_for_overlapping_branch_ids() {
        let mut manager = ResourceManager::default();
        let first = manager.allocate_qubit();
        let second = manager.allocate_qubit();
        let snapshot = manager.save_qubit_state();
        manager.swap_qubit_ids(first.deref().0, second.deref().0);
        manager.merge_qubit_state(snapshot);
        assert_eq!(manager.map_qubit(&first), 1);
        assert_eq!(manager.map_qubit(&second), 0);
        assert!(manager.is_qubit_live(&first));
        assert!(manager.is_qubit_live(&second));
        assert_free_pools(&manager, &[], &[]);
    }
}
