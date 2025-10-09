// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_partial_eval::rir::InstructionMetadata;

use crate::{
    Qubit,
    builder::Remapper,
    rir_to_circuit::{Op, OperationKind},
};

pub(crate) struct BlockBuilder {
    max_ops: usize,
    max_ops_exceeded: bool,
    operations: Vec<Op>,
}

impl BlockBuilder {
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

pub(crate) struct FixedQubitRegisterMap {
    remapper: Remapper,
}
impl FixedQubitRegisterMap {
    pub(crate) fn new(num_qubits: usize) -> Self {
        let mut remapper = Remapper::default();

        for id in 0..num_qubits {
            remapper.map_qubit(id);
        }
        Self { remapper }
    }

    pub(crate) fn into_qubits(self) -> Vec<Qubit> {
        self.remapper.into_qubits()
    }

    pub(crate) fn link_result_to_qubit(&mut self, q: usize, r: usize) {
        self.remapper.link_result_to_qubit(q, r);
    }
}

impl RegisterMap for FixedQubitRegisterMap {
    type ResultType = usize;
    type QubitType = usize;

    fn qubit_register(&self, qubit_id: Self::QubitType) -> QubitRegister {
        self.remapper.qubit_register(qubit_id)
    }

    fn result_register(&self, result_id: Self::ResultType) -> ResultRegister {
        self.remapper.result_register(result_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ResultRegister(pub usize, pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct QubitRegister(pub usize);

impl From<usize> for QubitRegister {
    fn from(value: usize) -> Self {
        QubitRegister(value)
    }
}

impl From<QubitRegister> for usize {
    fn from(value: QubitRegister) -> Self {
        value.0
    }
}

pub(crate) trait RegisterMap {
    type ResultType;
    type QubitType;

    fn qubit_register(&self, qubit_id: Self::QubitType) -> QubitRegister;
    fn result_register(&self, result_id: Self::ResultType) -> ResultRegister;
}

impl BlockBuilder {
    pub fn gate<ResultType: Copy, QubitType: Copy, T>(
        &mut self,
        register_map: &T,
        name: &str,
        is_adjoint: bool,
        inputs: GateInputs<QubitType, ResultType>,
        args: Vec<String>,
        metadata: Option<InstructionMetadata>,
    ) where
        T: RegisterMap<ResultType = ResultType, QubitType = QubitType>,
    {
        let GateInputs {
            target_qubits,
            control_qubits,
            control_results,
        } = inputs;

        let target_qubits = target_qubits
            .iter()
            .map(|q| register_map.qubit_register(*q))
            .collect();

        let control_results = control_results
            .iter()
            .map(|reg| register_map.result_register(*reg))
            .collect();

        let control_qubits = control_qubits
            .iter()
            .map(|q| register_map.qubit_register(*q))
            .collect();

        self.push(Op {
            kind: OperationKind::Unitary { metadata },
            label: name.to_string(),
            target_qubits,
            control_qubits,
            target_results: vec![],
            control_results,
            is_adjoint,
            args,
        });
    }

    pub fn m<ResultType: Copy, QubitType: Copy, T>(
        &mut self,
        register_map: &T,
        qubit: QubitType,
        result: ResultType,
        metadata: Option<InstructionMetadata>,
    ) where
        T: RegisterMap<ResultType = ResultType, QubitType = QubitType>,
    {
        // Qubit-result mappings should have been established already
        let qubits = vec![register_map.qubit_register(qubit)];
        let results = vec![register_map.result_register(result)];

        self.push(Op {
            kind: OperationKind::Measurement { metadata },
            label: "M".to_string(),
            target_qubits: vec![],
            control_qubits: qubits,
            target_results: results,
            control_results: vec![],
            is_adjoint: false,
            args: vec![],
        });
    }

    pub fn mresetz<ResultType: Copy, QubitType: Copy, T>(
        &mut self,
        register_map: &T,
        qubit: QubitType,
        result: ResultType,
        metadata: Option<InstructionMetadata>,
    ) where
        T: RegisterMap<ResultType = ResultType, QubitType = QubitType>,
    {
        // Qubit-result mappings should have been established already
        let qubits = vec![register_map.qubit_register(qubit)];
        let result_registers = vec![register_map.result_register(result)];

        self.push(Op {
            kind: OperationKind::Measurement {
                metadata: metadata.clone(),
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
            kind: OperationKind::Ket { metadata },
            label: "0".to_string(),
            target_qubits: qubits,
            control_qubits: vec![],
            target_results: vec![],
            control_results: vec![],
            is_adjoint: false,
            args: vec![],
        });
    }

    pub fn reset<ResultType: Copy, QubitType: Copy, T>(
        &mut self,
        register_map: &T,
        qubit: QubitType,
        metadata: Option<InstructionMetadata>,
    ) where
        T: RegisterMap<ResultType = ResultType, QubitType = QubitType>,
    {
        self.push(Op {
            kind: OperationKind::Ket { metadata },
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

pub(crate) struct GateInputs<QubitType, ResultType> {
    pub target_qubits: Vec<QubitType>,
    pub control_qubits: Vec<QubitType>,
    pub control_results: Vec<ResultType>,
}
