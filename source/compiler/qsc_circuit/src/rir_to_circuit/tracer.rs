// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::builder::{WireMap, WireMapBuilder};

pub(crate) struct FixedQubitRegisterMapBuilder {
    remapper: WireMapBuilder,
}
impl FixedQubitRegisterMapBuilder {
    pub(crate) fn new(num_qubits: usize) -> Self {
        let mut remapper = WireMapBuilder::default();

        for id in 0..num_qubits {
            remapper.map_qubit(id, None); // TODO: source location
        }
        Self { remapper }
    }

    pub(crate) fn link_result_to_qubit(&mut self, q: usize, r: usize) {
        self.remapper.link_result_to_qubit(q, r);
    }

    pub(crate) fn register_map(&self) -> &WireMap {
        self.remapper.current()
    }

    pub(crate) fn into_register_map(self) -> WireMap {
        self.remapper.into_wire_map()
    }
}
