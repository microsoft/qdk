// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::index_map::IndexMap;
use qsc_partial_eval::rir::InstructionMetadata;

use crate::{
    Qubit, Register,
    rir_to_circuit::{Op, OperationKind},
};

pub(crate) struct BlockBuilder {
    operations: Vec<Op>,
}

impl BlockBuilder {
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
        }
    }

    pub fn push(&mut self, op: Op) {
        self.operations.push(op);
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
    pub(crate) fn new(num_qubits: u32) -> Self {
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
}

pub(crate) trait RegisterMap {
    fn result_register(&self, result_id: u32) -> Register;
    fn result_idx_for_qubit(&self, qubit_id: u32, result_id: u32) -> usize;
    fn result_registers(&self, results: Vec<u32>) -> Vec<(usize, usize)>;
    fn into_qubits(self) -> Vec<Qubit>;
}

impl RegisterMap for FixedQubitRegisterMap {
    fn result_register(&self, result_id: u32) -> Register {
        let qubit_id = self
            .results
            .get(usize::try_from(result_id).expect("result id should fit into usize"))
            .copied()
            .expect("result should be linked to a qubit");

        let qubit_result_idx = self.result_idx_for_qubit(qubit_id, result_id);

        Register {
            qubit: usize::try_from(qubit_id).expect("qubit id should fit in usize"),
            result: Some(qubit_result_idx),
        }
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

    fn result_registers(&self, results: Vec<u32>) -> Vec<(usize, usize)> {
        results
            .into_iter()
            .map(|r| self.result_register(r))
            .map(|r| {
                (
                    r.qubit,
                    r.result.expect("result register must have result idx"),
                )
            })
            .collect::<Vec<_>>()
    }

    fn into_qubits(self) -> Vec<Qubit> {
        self.qubits
            .into_iter()
            .map(|(q, results)| Qubit {
                id: q.id,
                num_results: results.len(),
            })
            .collect()
    }
}

impl Tracer for BlockBuilder {
    type ResultType = u32;
    type QubitType = u32;

    fn gate<T: RegisterMap>(
        &mut self,
        register_map: &T,
        name: &str,
        is_adjoint: bool,
        inputs: GateInputs,
        args: Vec<String>,
        metadata: Option<InstructionMetadata>,
    ) {
        let GateInputs {
            target_qubits,
            control_qubits,
            control_results,
        } = inputs;

        let target_qubits = target_qubits
            .iter()
            .map(|q| usize::try_from(*q).expect("qubit index should fit into usize"))
            .collect();

        let control_results = control_results
            .iter()
            .filter_map(|reg| {
                let reg = register_map.result_register(*reg);
                reg.result.map(|r| (reg.qubit, r))
            })
            .collect();

        let control_qubits = control_qubits
            .iter()
            .map(|q| usize::try_from(*q).expect("qubit index should fit into usize"))
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

    fn m<T: RegisterMap>(
        &mut self,
        register_map: &T,
        qubit: Self::QubitType,
        result: Self::ResultType,
        metadata: Option<InstructionMetadata>,
    ) {
        // Qubit-result mappings should have been established already
        let qubits = vec![usize::try_from(qubit).expect("qubit id should fit in usize")];
        register_map.result_idx_for_qubit(qubit, result);
        let result_registers = [register_map.result_register(result)];

        self.push(Op {
            kind: OperationKind::Measurement { metadata },
            label: "M".to_string(),
            target_qubits: vec![],
            control_qubits: qubits,
            target_results: result_registers
                .iter()
                .map(|r| {
                    (
                        r.qubit,
                        r.result.expect("result register must have result idx"),
                    )
                })
                .collect(),
            control_results: vec![],
            is_adjoint: false,
            args: vec![],
        });
    }

    fn mresetz<T: RegisterMap>(
        &mut self,
        register_map: &T,
        qubit: Self::QubitType,
        result: Self::ResultType,
        metadata: Option<InstructionMetadata>,
    ) {
        // Qubit-result mappings should have been established already
        let qubits = vec![usize::try_from(qubit).expect("qubit id should fit in usize")];
        register_map.result_idx_for_qubit(qubit, result);
        let result_registers = [register_map.result_register(result)];

        self.push(Op {
            kind: OperationKind::Measurement {
                metadata: metadata.clone(),
            },
            label: "MResetZ".to_string(),
            target_qubits: vec![],
            control_qubits: qubits.clone(),
            target_results: result_registers
                .iter()
                .map(|r| {
                    (
                        r.qubit,
                        r.result.expect("result register must have result idx"),
                    )
                })
                .collect(),
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

    fn reset<T: RegisterMap>(
        &mut self,
        _register_map: &T,
        qubit: Self::QubitType,
        metadata: Option<InstructionMetadata>,
    ) {
        self.push(Op {
            kind: OperationKind::Ket { metadata },
            label: "0".to_string(),
            target_qubits: vec![usize::try_from(qubit).expect("qubit id should fit in usize")],
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

pub(crate) struct GateInputs {
    pub target_qubits: Vec<u32>,
    pub control_qubits: Vec<u32>,
    pub control_results: Vec<u32>,
}

pub(crate) trait Tracer {
    type ResultType;
    type QubitType;

    fn gate<T: RegisterMap>(
        &mut self,
        register_map: &T,
        name: &str,
        is_adjoint: bool,
        inputs: GateInputs,
        args: Vec<String>,
        metadata: Option<InstructionMetadata>,
    );

    fn m<T: RegisterMap>(
        &mut self,
        register_map: &T,
        q: Self::QubitType,
        r: Self::ResultType,
        metadata: Option<InstructionMetadata>,
    );

    fn mresetz<T: RegisterMap>(
        &mut self,
        register_map: &T,
        q: Self::QubitType,
        r: Self::ResultType,
        metadata: Option<InstructionMetadata>,
    );

    fn reset<T: RegisterMap>(
        &mut self,
        register_map: &T,
        q: Self::QubitType,
        metadata: Option<InstructionMetadata>,
    );

    // Results only get associated with qubits when a measurement occurs
    // fn result_allocate(&mut self) -> Self::ResultType;
    // fn qubit_allocate(&mut self) -> Self::QubitType;
    // fn qubit_release(&mut self, q: Self::QubitType) -> bool;
    // fn qubit_swap_id(&mut self, q0: Self::QubitType, q1: Self::QubitType);
}
