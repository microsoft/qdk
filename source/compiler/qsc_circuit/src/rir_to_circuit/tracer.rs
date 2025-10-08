// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::index_map::IndexMap;
use qsc_partial_eval::rir::InstructionMetadata;

use crate::{
    Qubit,
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
    /// result id -> qubit id
    results: IndexMap<usize, u32>,
    /// qubit decl, result idx -> result id
    qubits: Vec<(Qubit, Vec<u32>)>,
}

impl FixedQubitRegisterMap {
    pub fn new(num_qubits: u32) -> Self {
        Self {
            qubits: (0..num_qubits)
                .map(|id| {
                    (
                        Qubit {
                            id: usize::try_from(id).expect("qubit id should fit in usize"),
                            num_results: 0,
                        },
                        vec![],
                    )
                })
                .collect::<Vec<_>>(),
            results: IndexMap::default(),
        }
    }

    pub fn link_result_to_qubit(&mut self, qubit_id: u32, result_id: u32) -> usize {
        self.results.insert(
            result_id
                .try_into()
                .expect("result id should fit into usize"),
            qubit_id,
        );
        let result_ids_for_qubit =
            &mut self.qubits[usize::try_from(qubit_id).expect("qubit id should fit in usize")].1;
        let qubit_result_idx = result_ids_for_qubit
            .iter_mut()
            .enumerate()
            .find(|(_, qubit_r)| **qubit_r == result_id)
            .map(|(a, _)| a);

        qubit_result_idx.unwrap_or_else(|| {
            result_ids_for_qubit.push(result_id);
            result_ids_for_qubit.len() - 1
        })
    }

    pub fn into_qubits(self) -> Vec<Qubit> {
        self.qubits
            .into_iter()
            .map(|(q, results)| Qubit {
                id: q.id,
                num_results: results.len(),
            })
            .collect()
    }

    fn result_idx_for_qubit(&self, qubit_id: u32, result_id: u32) -> usize {
        let q = *self
            .results
            .get(
                result_id
                    .try_into()
                    .expect("result id should fit into usize"),
            )
            .expect("result should be linked to a qubit");
        assert_eq!(q, qubit_id, "result should be linked to the correct qubit");

        let result_ids_for_qubit =
            &self.qubits[usize::try_from(qubit_id).expect("qubit id should fit in usize")].1;

        let qubit_result_idx = result_ids_for_qubit
            .iter()
            .enumerate()
            .find(|(_, qubit_r)| **qubit_r == result_id)
            .map(|(a, _)| a);

        qubit_result_idx.expect("result should be linked to the qubit")
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

impl RegisterMap for FixedQubitRegisterMap {
    type QubitType = u32;
    type ResultType = u32;

    fn qubit_register(&self, qubit_id: Self::QubitType) -> QubitRegister {
        QubitRegister(usize::try_from(qubit_id).expect("qubit id should fit in usize"))
    }

    fn result_register(&self, result_id: u32) -> ResultRegister {
        let qubit_id = self
            .results
            .get(usize::try_from(result_id).expect("result id should fit into usize"))
            .copied()
            .expect("result should be linked to a qubit");

        let qubit_result_idx = self.result_idx_for_qubit(qubit_id, result_id);

        ResultRegister(
            usize::try_from(qubit_id).expect("qubit id should fit in usize"),
            qubit_result_idx,
        )
    }
}

impl Tracer for BlockBuilder {
    fn gate<ResultType: Copy, QubitType: Copy, T>(
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

    fn m<ResultType: Copy, QubitType: Copy, T>(
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

    fn mresetz<ResultType: Copy, QubitType: Copy, T>(
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

    fn reset<ResultType: Copy, QubitType: Copy, T>(
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

    // In eval, we need the result id generated by this function.
    // but in RIR->circuit, the result id is passed in!
    // fn result_allocate(&mut self) -> Self::ResultType {
    //     todo!()
    // }

    // fn qubit_allocate(&mut self) -> Self::QubitType {
    //     todo!()
    // }

    // fn qubit_release(&mut self, _q: Self::QubitType) -> bool {
    //     todo!()
    // }

    // fn qubit_swap_id(&mut self, _q0: Self::QubitType, _q1: Self::QubitType) {
    //     todo!()
    // }
}

pub(crate) struct GateInputs<QubitType, ResultType> {
    pub target_qubits: Vec<QubitType>,
    pub control_qubits: Vec<QubitType>,
    pub control_results: Vec<ResultType>,
}

pub(crate) trait Tracer {
    fn gate<ResultType: Copy, QubitType: Copy, T>(
        &mut self,
        register_map: &T,
        name: &str,
        is_adjoint: bool,
        inputs: GateInputs<QubitType, ResultType>,
        args: Vec<String>,
        metadata: Option<InstructionMetadata>,
    ) where
        T: RegisterMap<ResultType = ResultType, QubitType = QubitType>;

    fn m<ResultType: Copy, QubitType: Copy, T>(
        &mut self,
        register_map: &T,
        q: QubitType,
        r: ResultType,
        metadata: Option<InstructionMetadata>,
    ) where
        T: RegisterMap<ResultType = ResultType, QubitType = QubitType>;

    fn mresetz<ResultType: Copy, QubitType: Copy, T>(
        &mut self,
        register_map: &T,
        q: QubitType,
        r: ResultType,
        metadata: Option<InstructionMetadata>,
    ) where
        T: RegisterMap<ResultType = ResultType, QubitType = QubitType>;

    fn reset<ResultType: Copy, QubitType: Copy, T>(
        &mut self,
        register_map: &T,
        q: QubitType,
        metadata: Option<InstructionMetadata>,
    ) where
        T: RegisterMap<ResultType = ResultType, QubitType = QubitType>;

    // Results only get associated with qubits when a measurement occurs
    // fn result_allocate(&mut self) -> Self::ResultType;
    // fn qubit_allocate(&mut self) -> Self::QubitType;
    // fn qubit_release(&mut self, q: Self::QubitType) -> bool;
    // fn qubit_swap_id(&mut self, q0: Self::QubitType, q1: Self::QubitType);
}
