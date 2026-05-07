use rustc_hash::FxHashMap;
use std::hash::Hash;


/// Pool of qubits for a specific type.
#[derive(Default)]
pub struct QubitPool {
    current_in_use: usize,
    pub max_in_use: usize,
}

impl QubitPool {
    pub fn allocate(&mut self) {
        self.current_in_use += 1;
        if self.current_in_use > self.max_in_use {
            self.max_in_use = self.current_in_use;
        }
    }

    pub fn release(&mut self) {
        self.current_in_use -= 1;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QubitType {
    Compute,
    Memory,
}

/// Maintains a mapping from qubit id to qubit type.
#[derive(Default)]
pub struct TypedQubitPools {
    /// Qubit pool for each qubit type.
    qubit_pools: FxHashMap<QubitType, QubitPool>,
    /// Maps untyped qubit id to qubit type.
    pub qubit_type_map: FxHashMap<usize, QubitType>,
}

impl TypedQubitPools {
    /// Allocates typed qubit of given type.
    pub fn allocate(&mut self, untyped_qubit_id: usize, qubit_type: QubitType) {
        self.qubit_pools.entry(qubit_type).or_default().allocate();
        self.qubit_type_map.insert(untyped_qubit_id, qubit_type);
    }

    /// Release a qubit back to whichever pool it belongs to, removing its mapping.
    pub fn release(&mut self, untyped_qubit_id: usize) {
        if let Some(qubit_type) = self.qubit_type_map.remove(&untyped_qubit_id) {
            if let Some(pool) = self.qubit_pools.get_mut(&qubit_type) {
                pool.release();
            }
        }
    }

    pub fn get_qubit_type(&self, untyped_qubit_id: usize) -> QubitType {
        self.qubit_type_map.get(&untyped_qubit_id).unwrap().clone()
    }

    pub fn max_in_use(&self, qubit_type: QubitType) -> usize {
        self.qubit_pools.get(&qubit_type).unwrap().max_in_use
    }
}
