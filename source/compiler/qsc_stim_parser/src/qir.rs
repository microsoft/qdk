// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::target;

use crate::parser::*;
use std::collections::HashSet;
use std::fmt::Write;

enum InstructionKind {
    PauliGate,
    SingleQubitCliffordGate,
    TwoQubitCliffordGate,
    NoiseChannel,
    CollapsingGate,
    PairMeasurementGate,
    GeneralizedPauliProductGate,
    ControlFlow,
    Annotations,
    CustomInstruction,
}

struct Emitter {
    qir_output: String,
    num_qubits: u32,
    num_results: u32,
    last_preselect_begin: Option<u32>,
    num_preselect_expects: u32,
    used_intrinsics: HashSet<String>,
    labels: Vec<String>,
}

impl Emitter {
    fn new() -> Self {
        Self {
            qir_output: String::new(),
            num_qubits: 0,
            num_results: 0,
            last_preselect_begin: None,
            num_preselect_expects: 0,
            used_intrinsics: HashSet::new(),
            labels: Vec::new(),
        }
    }

    fn emit_circuit(&mut self, circuit: &Circuit) {
        let items = &circuit.items;
        for item in items {
            self.emit_item(&item);
        }
    }

    fn emit_item(&mut self, item: &Item) {
        match item {
            Item::Block(block) => self.emit_block(block),
            Item::Line(line) => self.emit_line(line),
        }
    }

    fn emit_block(&mut self, block: &Block) {
        let Block {
            block_instruction,
            items,
            ..
        } = block;

        self.emit_instruction(block_instruction);
        for item in items {
            self.emit_item(item);
        }
    }

    fn emit_line(&mut self, line: &Line) {
        let Line { instruction, .. } = line;
        self.emit_instruction(instruction);
    }

    fn emit_instruction(&mut self, instruction: &Instruction) {
        match self.instruction_kind(instruction.name) {
            InstructionKind::PauliGate => {
                self.emit_pauli_gate(instruction);
            }
            InstructionKind::SingleQubitCliffordGate => {
                self.emit_single_qubit_clifford_gate(instruction);
            }
            InstructionKind::TwoQubitCliffordGate => {
                self.emit_two_qubit_clifford_gate(instruction);
            }
            InstructionKind::NoiseChannel => {
                self.emit_noise_channel(instruction);
            }
            InstructionKind::CollapsingGate => {
                self.emit_collapsing_gate(instruction);
            }
            InstructionKind::PairMeasurementGate => {
                self.emit_pair_measurement_gate(instruction);
            }
            InstructionKind::GeneralizedPauliProductGate => {
                self.emit_generalized_pauli_product_gate(instruction);
            }
            InstructionKind::ControlFlow => {
                self.emit_control_flow(instruction);
            }
            InstructionKind::Annotations => {
                self.emit_annotations(instruction);
            }
            InstructionKind::CustomInstruction => {
                self.emit_custom_instruction(instruction);
            }
        }
    }

    fn emit_pauli_gate(&mut self, instruction: &Instruction) {
        let gate = instruction.name.to_lowercase();
        for target in &instruction.targets {
            write!(self.qir_output, "__quantum__qis__{}__body", gate).unwrap();
            self.emit_target(&target);
        }
    }

    fn emit_single_qubit_clifford_gate(&mut self, instruction: &Instruction) {
        let gate = instruction.name.to_lowercase();
        if gate == "h" || gate == "s" {
            for target in &instruction.targets {
                write!(self.qir_output, "__quantum__qis__{}__body", gate).unwrap();
                self.emit_target(&target);
            }
        } else if gate == "sqrt_x" {
            // decomposed into H S H
            for target in &instruction.targets {
                write!(self.qir_output, "__quantum__qis__h__body").unwrap();
                self.emit_target(&target);
                write!(self.qir_output, "__quantum__qis__s__body").unwrap();
                self.emit_target(&target);
                write!(self.qir_output, "__quantum__qis__h__body").unwrap();
                self.emit_target(&target);
            }
        }
    }

    fn emit_two_qubit_clifford_gate(&mut self, instruction: &Instruction) {
        let gate = instruction.name.to_lowercase();
        if gate == "cz" {
            let targets = &instruction.targets;
            for pair in targets.chunks(2) {
                let [control, target] = pair else {
                    unreachable!()
                };
                write!(self.qir_output, "__quantum__qis__cz__body").unwrap();
                self.emit_target(&control);
                self.emit_target(&target);
            }
        }
    }

    fn emit_noise_channel(&mut self, instruction: &Instruction) {}

    fn emit_collapsing_gate(&mut self, instruction: &Instruction) {
        let gate = instruction.name.to_lowercase();
        if gate == "r" {
            for target in &instruction.targets {
                write!(self.qir_output, "__quantum__qis__reset__body").unwrap();
                self.emit_target(&target);
            }
        } else if gate == "mr" {
            for target in &instruction.targets {
                write!(self.qir_output, "__quantum__qis__mresetz__body").unwrap();
                self.emit_target(&target);
            }
        } else if gate == "mrx" {
            // decomposed into H MRZ H
            for target in &instruction.targets {
                write!(self.qir_output, "__quantum__qis__h__body").unwrap();
                self.emit_target(&target);
                write!(self.qir_output, "__quantum__qis__mresetz__body").unwrap();
                self.emit_target(&target);
                write!(self.qir_output, "__quantum__qis__h__body").unwrap();
                self.emit_target(&target);
            }
        }
    }

    fn emit_pair_measurement_gate(&mut self, instruction: &Instruction) {}

    fn emit_generalized_pauli_product_gate(&mut self, instruction: &Instruction) {}

    fn emit_control_flow(&mut self, instruction: &Instruction) {}

    fn emit_annotations(&mut self, instruction: &Instruction) {}

    fn emit_custom_instruction(&mut self, instruction: &Instruction) {
        let instructionName = instruction.name.to_lowercase();
        if instructionName == "#!preselect_begin" {
            self.last_preselect_begin = match self.last_preselect_begin {
                None => Some(0),
                Some(n) => Some(n + 1),
            };
            write!(
                self.qir_output,
                "preselect_begin_{}:",
                self.last_preselect_begin.unwrap() // It shouldn't be none!
            )
            .unwrap();
        } else if instructionName == "#!preselect_expect" {
            write!(self.qir_output, "preselect_r{}", self.num_preselect_expects).unwrap();
            self.num_preselect_expects += 1;
            write!(self.qir_output, " ").unwrap(); // whitespace
            write!(
                self.qir_output,
                "= call i1 @__quantum__qis__read_result__body"
            )
            .unwrap();
            self.emit_target(&instruction.targets[0]);
            // EMIT BREAK, br i1 %preselect_r1, label %preselect_fail_1, label %continue_1
            // HAVE IT THE OTHER WAY AROUDN IF TARGETS[1] IS 1
        }
    }

    fn emit_targets(&mut self, targets: &[Target]) {
        write!(self.qir_output, "(").unwrap();
        for target in targets {
            self.emit_target(target);
            write!(self.qir_output, ", ").unwrap();
        }
        self.qir_output.pop(); // remove trailing whitespace
        self.qir_output.pop(); // remove trailing comma
        write!(self.qir_output, ")\n").unwrap();
    }

    // fn emit_target(&mut self, target: &Target) {
    //     match target.kind {
    //         TargetKind::Qubit { negated, value } => {
    //             if negated {
    //                 write!(self.qir_output, "!q{}", value).unwrap();
    //             } else {
    //                 write!(self.qir_output, "q{}", value).unwrap();
    //             }
    //         }
    //         TargetKind::MeasurementRecord { value } => {
    //             write!(self.qir_output, "m{}", value).unwrap();
    //         }
    //         TargetKind::SweepBit { value } => {
    //             write!(self.qir_output, "s{}", value).unwrap();
    //         }
    //         TargetKind::Pauli { negated, pauli, value } => {
    //             if negated {
    //                 write!(self.qir_output, "!").unwrap();
    //             }
    //             match pauli {
    //                 Pauli::X => write!(self.qir_output, "X").unwrap(),
    //                 Pauli::Y => write!(self.qir_output, "Y").unwrap(),
    //                 Pauli::Z => write!(self.qir_output, "Z").unwrap(),
    //             }
    //             write!(self.qir_output, "{}", value).unwrap();
    //         }
    //         TargetKind::Combiner => {
    //             write!(self.qir_output, "combiner").unwrap();
    //         }
    //     }
    // }

    fn instruction_kind(name: &str) -> InstructionKind {
        match name {
            // Pauli Gates
            "I" | "X" | "Y" | "Z" => InstructionKind::PauliGate,

            // Single Qubit Clifford Gates
            "C_NXYZ" | "C_NZYX" | "C_XNYZ" | "C_XYNZ" | "C_XYZ" | "C_ZNYX" | "C_ZYNX" | "C_ZYX"
            | "H" | "H_NXY" | "H_NXZ" | "H_NYZ" | "H_XY" | "H_XZ" | "H_YZ" | "S" | "SQRT_X"
            | "SQRT_X_DAG" | "SQRT_Y" | "SQRT_Y_DAG" | "SQRT_Z" | "SQRT_Z_DAG" | "S_DAG" => {
                InstructionKind::SingleQubitCliffordGate
            }

            // Two Qubit Clifford Gates
            "CNOT" | "CX" | "CXSWAP" | "CY" | "CZ" | "CZSWAP" | "II" | "ISWAP" | "ISWAP_DAG"
            | "SQRT_XX" | "SQRT_XX_DAG" | "SQRT_YY" | "SQRT_YY_DAG" | "SQRT_ZZ" | "SQRT_ZZ_DAG"
            | "SWAP" | "SWAPCX" | "SWAPCZ" | "XCX" | "XCY" | "XCZ" | "YCX" | "YCY" | "YCZ"
            | "ZCX" | "ZCY" | "ZCZ" => InstructionKind::TwoQubitCliffordGate,

            // Noise Channels
            "CORRELATED_ERROR"
            | "DEPOLARIZE1"
            | "DEPOLARIZE2"
            | "E"
            | "ELSE_CORRELATED_ERROR"
            | "HERALDED_ERASE"
            | "HERALDED_PAULI_CHANNEL_1"
            | "II_ERROR"
            | "I_ERROR"
            | "PAULI_CHANNEL_1"
            | "PAULI_CHANNEL_2"
            | "X_ERROR"
            | "Y_ERROR"
            | "Z_ERROR" => InstructionKind::NoiseChannel,

            // Collapsing Gates
            "M" | "MR" | "MRX" | "MRY" | "MRZ" | "MX" | "MY" | "MZ" | "R" | "RX" | "RY" | "RZ" => {
                InstructionKind::CollapsingGate
            }

            // Pair Measurement Gates
            "MXX" | "MYY" | "MZZ" => InstructionKind::PairMeasurementGate,

            // Generalized Pauli Product Gates
            "MPP" | "SPP" | "SPP_DAG" => InstructionKind::GeneralizedPauliProductGate,

            // Control Flow
            "REPEAT" => InstructionKind::ControlFlow,

            // Annotations
            "DETECTOR" | "MPAD" | "OBSERVABLE_INCLUDE" | "QUBIT_COORDS" | "SHIFT_COORDS"
            | "TICK" => InstructionKind::Annotations,

            "#!preselect_begin" | "#!preselect_expect" => InstructionKind::CustomInstruction,
            _ => InstructionKind::CustomInstruction,
        }
    }

    fn append_header(&mut self) {}

    fn append_footer(&mut self) {}

    fn into_qir(&mut self, circuit: &Circuit) -> String {
        self.emit_circuit(circuit);
        self.append_header();
        self.append_footer();
        self.qir_output
    }
}

pub fn emit_qir(circuit: &Circuit) -> String {
    Emitter::new().into_qir(circuit)
}
