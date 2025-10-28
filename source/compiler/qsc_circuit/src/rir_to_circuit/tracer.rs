// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::debug::InstructionMetadata;

use crate::{
    builder::{RegisterMap, RegisterMapBuilder},
    rir_to_circuit::{DbgLocationKind, Op, OperationKind},
};

pub(crate) struct CircuitBuilder {
    max_ops: usize,
    max_ops_exceeded: bool,
    operations: Vec<Op>,
}

impl CircuitBuilder {
    pub(crate) fn new(max_operations: usize) -> Self {
        Self {
            max_ops: max_operations,
            max_ops_exceeded: false,
            operations: vec![],
        }
    }

    pub fn push(&mut self, op: Op) {
        if self.max_ops_exceeded || self.operations.len() >= self.max_ops {
            // Stop adding gates and leave the circuit as is
            self.max_ops_exceeded = true;
            return;
        }

        self.operations.push(op);
    }

    pub fn operations(&self) -> &Vec<Op> {
        &self.operations
    }

    pub fn into_operations(self) -> Vec<Op> {
        self.operations
    }
}

pub(crate) struct FixedQubitRegisterMapBuilder {
    remapper: RegisterMapBuilder,
}
impl FixedQubitRegisterMapBuilder {
    pub(crate) fn new(num_qubits: usize) -> Self {
        let mut remapper = RegisterMapBuilder::default();

        for id in 0..num_qubits {
            remapper.map_qubit(id, None);
        }
        Self { remapper }
    }

    pub(crate) fn link_result_to_qubit(&mut self, q: usize, r: usize) {
        self.remapper.link_result_to_qubit(q, r);
    }

    pub(crate) fn register_map(&self) -> &RegisterMap {
        self.remapper.current()
    }

    pub(crate) fn into_register_map(self) -> RegisterMap {
        self.remapper.into_register_map()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ResultRegister(pub usize, pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct WireId(pub usize);

impl From<usize> for WireId {
    fn from(value: usize) -> Self {
        WireId(value)
    }
}

impl From<WireId> for usize {
    fn from(value: WireId) -> Self {
        value.0
    }
}

pub(crate) struct GateLabel<'a> {
    pub name: &'a str,
    pub is_adjoint: bool,
}

impl CircuitBuilder {
    pub fn gate(
        &mut self,
        register_map: &RegisterMap,
        gate_label: &GateLabel,
        inputs: &GateInputs,
        control_results: &[usize],
        args: Vec<String>,
        metadata: Option<InstructionMetadata>,
    ) {
        let target_qubits = inputs
            .targets
            .iter()
            .map(|q| register_map.qubit_register(*q))
            .collect();

        let control_qubits = inputs
            .controls
            .iter()
            .map(|q| register_map.qubit_register(*q))
            .collect();

        let control_results = control_results
            .iter()
            .map(|r| register_map.result_register(*r))
            .collect();

        self.push(Op {
            kind: OperationKind::Unitary {
                location: metadata
                    .and_then(|md| md.dbg_location)
                    .map(DbgLocationKind::Unresolved),
            },
            label: gate_label.name.to_string(),
            target_qubits,
            control_qubits,
            target_results: vec![],
            control_results,
            is_adjoint: gate_label.is_adjoint,
            args,
        });
    }

    pub fn m(
        &mut self,
        register_map: &RegisterMap,
        qubit: usize,
        result: usize,
        metadata: Option<InstructionMetadata>,
    ) {
        // Qubit-result mappings should have been established already
        let qubits = vec![register_map.qubit_register(qubit)];
        let results = vec![register_map.result_register(result)];

        self.push(Op {
            kind: OperationKind::Measurement {
                location: metadata
                    .and_then(|md| md.dbg_location)
                    .map(DbgLocationKind::Unresolved),
            },
            label: "M".to_string(),
            target_qubits: vec![],
            control_qubits: qubits,
            target_results: results,
            control_results: vec![],
            is_adjoint: false,
            args: vec![],
        });
    }

    pub fn mresetz(
        &mut self,
        register_map: &RegisterMap,
        qubit: usize,
        result: usize,
        metadata: Option<InstructionMetadata>,
    ) {
        // Qubit-result mappings should have been established already
        let qubits = vec![register_map.qubit_register(qubit)];
        let result_registers = vec![register_map.result_register(result)];

        self.push(Op {
            kind: OperationKind::Measurement {
                location: metadata
                    .as_ref()
                    .and_then(|md| md.dbg_location)
                    .map(DbgLocationKind::Unresolved),
            },
            label: "MResetZ".to_string(),
            target_qubits: vec![],
            control_qubits: qubits.clone(),
            target_results: result_registers,
            control_results: vec![],
            is_adjoint: false,
            args: vec![],
        });

        self.push(Op {
            kind: OperationKind::Ket {
                location: metadata
                    .and_then(|md| md.dbg_location)
                    .map(DbgLocationKind::Unresolved),
            },
            label: "0".to_string(),
            target_qubits: qubits,
            control_qubits: vec![],
            target_results: vec![],
            control_results: vec![],
            is_adjoint: false,
            args: vec![],
        });
    }

    pub fn reset(
        &mut self,
        register_map: &RegisterMap,
        qubit: usize,
        metadata: Option<InstructionMetadata>,
    ) {
        self.push(Op {
            kind: OperationKind::Ket {
                location: metadata
                    .and_then(|md| md.dbg_location)
                    .map(DbgLocationKind::Unresolved),
            },
            label: "0".to_string(),
            target_qubits: vec![register_map.qubit_register(qubit)],
            control_qubits: vec![],
            target_results: vec![],
            control_results: vec![],
            is_adjoint: false,
            args: vec![],
        });
    }
}

pub struct GateInputs<'a> {
    pub targets: &'a [usize],
    pub controls: &'a [usize],
}
