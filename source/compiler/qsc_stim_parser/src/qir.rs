// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
use crate::parser::*;
use std::collections::HashMap;
use std::fmt::Write;

#[derive(Clone, Copy)]
enum Operand {
    /// A qubit operand, carrying the raw Stim qubit index.
    Qubit(u32),
    /// A result operand — the writer allocates the next result ID.
    Result,
}

struct QirWriter {
    output: String,
    qubit_map: HashMap<u32, u32>,
    num_results: u32,
    used_intrinsics: HashMap<String, usize>,
}

impl QirWriter {
    fn new() -> Self {
        Self {
            output: String::new(),
            qubit_map: HashMap::new(),
            num_results: 0,
            used_intrinsics: HashMap::new(),
        }
    }

    // Writes: `  call void @__quantum__qis__{intrinsic}__body(ptr inttoptr (i64 N to ptr), ...)`
    // Resolves qubit indices via the qubit map and allocates result IDs internally.
    fn write_call(&mut self, intrinsic: &str, operands: &[Operand]) {
        write!(
            self.output,
            "  call void @__quantum__qis__{intrinsic}__body("
        )
        .unwrap();
        for (i, &operand) in operands.iter().enumerate() {
            if i > 0 {
                write!(self.output, ", ").unwrap();
            }
            self.write_operand(operand);
        }
        writeln!(self.output, ")").unwrap();
        self.used_intrinsics
            .insert(intrinsic.to_string(), operands.len());
    }

    // Resolves an Operand to its QIR ID and writes: `ptr inttoptr (i64 N to ptr)`
    fn write_operand(&mut self, operand: Operand) {
        let id = match operand {
            Operand::Qubit(stim_index) => self.map_qubit(stim_index),
            Operand::Result => self.next_result(),
        };
        write!(self.output, "ptr inttoptr (i64 {id} to ptr)").unwrap();
    }

    fn write_header(&mut self) {
        writeln!(self.output, "define i64 @ENTRYPOINT__main() #0 {{").unwrap();
        writeln!(
            self.output,
            "  call void @__quantum__rt__initialize(ptr null)"
        )
        .unwrap();
    }

    fn write_record_output(&mut self) {
        let num_results = self.num_results;
        writeln!(
            self.output,
            "  call void @__quantum__rt__array_record_output(i64 {num_results}, ptr null)"
        )
        .unwrap();
        for i in 0..num_results {
            writeln!(
                self.output,
                "  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 {i} to ptr), ptr null)"
            )
            .unwrap();
        }
    }

    fn write_declarations(&mut self) {
        writeln!(self.output).unwrap();
        writeln!(self.output, "declare void @__quantum__rt__initialize(ptr)").unwrap();
        writeln!(self.output, "declare void @__quantum__rt__array_record_output(i64, ptr)").unwrap();
        writeln!(self.output, "declare void @__quantum__rt__result_record_output(ptr, ptr)").unwrap();
        for (intrinsic, arity) in &self.used_intrinsics {
            let params = (0..*arity).map(|_| "ptr").collect::<Vec<_>>().join(", ");
            writeln!(
                self.output,
                "declare void @__quantum__qis__{intrinsic}__body({params})"
            )
            .unwrap();
        }
    }

    fn write_footer(&mut self) {
        self.write_record_output();
        writeln!(self.output, "  ret i64 0").unwrap();
        writeln!(self.output, "}}").unwrap();
        self.write_declarations();

        let num_qubits = self.qubit_map.len();
        let num_results = self.num_results;
        writeln!(self.output).unwrap();
        writeln!(
            self.output,
            "attributes #0 = {{ \"entry_point\" \"output_labeling_schema\" \"qir_profiles\"=\"adaptive_profile\" \"required_num_qubits\"=\"{num_qubits}\" \"required_num_results\"=\"{num_results}\" }}"
        ).unwrap();
        writeln!(self.output, "attributes #1 = {{ \"irreversible\" }}").unwrap();
        writeln!(self.output).unwrap();
        writeln!(self.output, "; module flags").unwrap();
        writeln!(self.output).unwrap();
        writeln!(
            self.output,
            "!llvm.module.flags = !{{!0, !1, !2, !3, !4, !5, !6, !7}}"
        )
        .unwrap();
        writeln!(self.output).unwrap();
        writeln!(
            self.output,
            "!0 = !{{i32 1, !\"qir_major_version\", i32 2}}"
        )
        .unwrap();
        writeln!(
            self.output,
            "!1 = !{{i32 7, !\"qir_minor_version\", i32 1}}"
        )
        .unwrap();
        writeln!(
            self.output,
            "!2 = !{{i32 1, !\"dynamic_qubit_management\", i1 false}}"
        )
        .unwrap();
        writeln!(
            self.output,
            "!3 = !{{i32 1, !\"dynamic_result_management\", i1 false}}"
        )
        .unwrap();
        writeln!(
            self.output,
            "!4 = !{{i32 5, !\"int_computations\", !{{!\"i64\"}}}}"
        )
        .unwrap();
        writeln!(
            self.output,
            "!5 = !{{i32 5, !\"float_computations\", !{{!\"double\"}}}}"
        )
        .unwrap();
        writeln!(
            self.output,
            "!6 = !{{i32 7, !\"backwards_branching\", i2 3}}"
        )
        .unwrap();
        writeln!(self.output, "!7 = !{{i32 1, !\"arrays\", i1 true}}").unwrap();
    }

    // Maps a Stim qubit index to a dense 0-based QIR qubit ID.
    fn map_qubit(&mut self, stim_index: u32) -> u32 {
        let next_id = self.qubit_map.len() as u32;
        *self.qubit_map.entry(stim_index).or_insert(next_id)
    }

    // Allocates the next result ID.
    fn next_result(&mut self) -> u32 {
        let id = self.num_results;
        self.num_results += 1;
        id
    }
}

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

struct Compiler {
    writer: QirWriter,
    last_preselect_begin: Option<u32>,
    num_preselect_expects: u32,
}

impl Compiler {
    fn new() -> Self {
        Self {
            writer: QirWriter::new(),
            last_preselect_begin: None,
            num_preselect_expects: 0,
        }
    }

    fn compile_circuit(&mut self, circuit: &Circuit) {
        for item in &circuit.items {
            self.compile_item(item);
        }
    }

    fn compile_item(&mut self, item: &Item) {
        match item {
            Item::Block(block) => self.compile_block(block),
            Item::Line(line) => self.compile_line(line),
        }
    }

    fn compile_block(&mut self, block: &Block) {
        let Block {
            block_instruction,
            items,
            ..
        } = block;

        self.compile_instruction(block_instruction);
        for item in items {
            self.compile_item(item);
        }
    }

    fn compile_line(&mut self, line: &Line) {
        let Line { instruction, .. } = line;
        self.compile_instruction(instruction);
    }

    fn compile_instruction(&mut self, instruction: &Instruction) {
        match Self::instruction_kind(&instruction.name) {
            InstructionKind::PauliGate => {
                self.compile_pauli_gate(instruction);
            }
            InstructionKind::SingleQubitCliffordGate => {
                self.compile_single_qubit_clifford_gate(instruction);
            }
            InstructionKind::TwoQubitCliffordGate => {
                self.compile_two_qubit_clifford_gate(instruction);
            }
            InstructionKind::NoiseChannel => {
                self.compile_noise_channel(instruction);
            }
            InstructionKind::CollapsingGate => {
                self.compile_collapsing_gate(instruction);
            }
            InstructionKind::PairMeasurementGate => {
                self.compile_pair_measurement_gate(instruction);
            }
            InstructionKind::GeneralizedPauliProductGate => {
                self.compile_generalized_pauli_product_gate(instruction);
            }
            InstructionKind::ControlFlow => {
                self.compile_control_flow(instruction);
            }
            InstructionKind::Annotations => {
                self.compile_annotations(instruction);
            }
            InstructionKind::CustomInstruction => {
                self.compile_custom_instruction(instruction);
            }
        }
    }

    fn compile_pauli_gate(&mut self, instruction: &Instruction) {
        let gate = instruction.name.to_lowercase();
        for target in &instruction.targets {
            let TargetKind::Qubit { value, .. } = target.kind else {
                continue;
            };
            self.writer.write_call(&gate, &[Operand::Qubit(value)]);
        }
    }

    fn compile_single_qubit_clifford_gate(&mut self, instruction: &Instruction) {
        let gate = instruction.name.to_lowercase();
        if gate == "h" || gate == "s" {
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                self.writer.write_call(&gate, &[Operand::Qubit(value)]);
            }
        } else if gate == "sqrt_x" {
            // decomposed into H S H
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                let q = Operand::Qubit(value);
                self.writer.write_call("h", &[q]);
                self.writer.write_call("s", &[q]);
                self.writer.write_call("h", &[q]);
            }
        }
    }

    fn compile_two_qubit_clifford_gate(&mut self, instruction: &Instruction) {
        let gate = instruction.name.to_lowercase();
        if gate == "cz" {
            let targets = &instruction.targets;
            for pair in targets.chunks(2) {
                let TargetKind::Qubit { value: v0, .. } = pair[0].kind else {
                    continue;
                };
                let TargetKind::Qubit { value: v1, .. } = pair[1].kind else {
                    continue;
                };
                self.writer
                    .write_call(&gate, &[Operand::Qubit(v0), Operand::Qubit(v1)]);
            }
        }
    }

    fn compile_noise_channel(&mut self, instruction: &Instruction) {}

    fn compile_collapsing_gate(&mut self, instruction: &Instruction) {
        let gate = instruction.name.to_lowercase();
        if gate == "r" {
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                self.writer.write_call("reset", &[Operand::Qubit(value)]);
            }
        } else if gate == "mr" {
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                self.writer
                    .write_call("mresetz", &[Operand::Qubit(value), Operand::Result]);
            }
        } else if gate == "mrx" {
            // decomposed into H MRZ H
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                let q = Operand::Qubit(value);
                self.writer.write_call("h", &[q]);
                self.writer.write_call("mresetz", &[q, Operand::Result]);
                self.writer.write_call("h", &[q]);
            }
        }
    }

    fn compile_pair_measurement_gate(&mut self, instruction: &Instruction) {}

    fn compile_generalized_pauli_product_gate(&mut self, instruction: &Instruction) {}

    fn compile_control_flow(&mut self, instruction: &Instruction) {}

    fn compile_annotations(&mut self, instruction: &Instruction) {}

    fn compile_custom_instruction(&mut self, instruction: &Instruction) {
        let instruction_name = instruction.name.to_lowercase();
        if instruction_name == "#!preselect_begin" {
            self.last_preselect_begin = match self.last_preselect_begin {
                None => Some(0),
                Some(n) => Some(n + 1),
            };
            writeln!(
                self.writer.output,
                "preselect_begin_{}:",
                self.last_preselect_begin.unwrap()
            )
            .unwrap();
        } else if instruction_name == "#!preselect_expect" {
            write!(
                self.writer.output,
                "preselect_r{}",
                self.num_preselect_expects
            )
            .unwrap();
            self.num_preselect_expects += 1;
            write!(
                self.writer.output,
                " = call i1 @__quantum__qis__read_result__body("
            )
            .unwrap();
            let TargetKind::Qubit { value, .. } = instruction.targets[0].kind else {
                return;
            };
            // read_result takes a result operand
            let result_id = self.writer.map_qubit(value);
            write!(self.writer.output, "ptr inttoptr (i64 {result_id} to ptr)").unwrap();
            writeln!(self.writer.output, ")").unwrap();
            // EMIT BREAK, br i1 %preselect_r1, label %preselect_fail_1, label %continue_1
            // HAVE IT THE OTHER WAY AROUND IF TARGETS[1] IS 1
        }
    }

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

    fn into_qir(mut self, circuit: &Circuit) -> String {
        self.writer.write_header();
        self.compile_circuit(circuit);
        self.writer.write_footer();
        self.writer.output
    }
}

pub fn compile_to_qir(circuit: &Circuit) -> String {
    Compiler::new().into_qir(circuit)
}
