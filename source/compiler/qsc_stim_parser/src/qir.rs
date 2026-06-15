// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
use crate::parser::*;
use rustc_hash::FxHashMap;
use std::fmt::Write;

pub struct NoiseTable {
    pub name: String,
    pub qubits: u32,
    pub entries: Vec<(String, f64)>, // (pauli_string, probability)
}

pub struct StimCompilationResult {
    pub qir: String,
    pub noise_tables: Vec<NoiseTable>,
}

#[derive(Clone, Copy)]
enum Operand {
    /// A qubit operand, carrying the raw Stim qubit index.
    Qubit(u32),
    /// A result operand — the writer allocates the next result ID.
    Result,
}

struct QirWriter {
    output: String,
    qubit_map: FxHashMap<u32, u32>,
    num_results: u32,
    used_intrinsics: FxHashMap<String, String>,
}

impl QirWriter {
    fn new() -> Self {
        Self {
            output: String::new(),
            qubit_map: FxHashMap::default(),
            num_results: 0,
            used_intrinsics: FxHashMap::default(),
        }
    }

    // Writes: `  call void @__quantum__qis__{intrinsic}__body(ptr inttoptr (i64 N to ptr), ...)`
    // Resolves qubit indices via the qubit map and allocates result IDs internally.
    // If `attr_group` is provided, appends ` #N` to the declaration.
    fn write_call(&mut self, intrinsic: &str, operands: &[Operand], attr_group: Option<u32>) {
        self.write_named_call(
            &format!("__quantum__qis__{intrinsic}__body"),
            operands,
            attr_group,
        );
    }

    // Writes: `  call void @{name}(ptr inttoptr (i64 N to ptr), ...)` using the raw function name.
    // If `attr_group` is provided, appends ` #N` to the declaration.
    fn write_named_call(&mut self, name: &str, operands: &[Operand], attr_group: Option<u32>) {
        write!(self.output, "  call void @{name}(").unwrap();
        for (i, &operand) in operands.iter().enumerate() {
            if i > 0 {
                write!(self.output, ", ").unwrap();
            }
            self.write_operand(operand);
        }
        writeln!(self.output, ")").unwrap();
        let params = (0..operands.len())
            .map(|_| "ptr")
            .collect::<Vec<_>>()
            .join(", ");
        let attr_suffix = attr_group.map_or(String::new(), |n| format!(" #{n}"));
        self.used_intrinsics
            .entry(name.to_string())
            .or_insert_with(|| format!("declare void @{name}({params}){attr_suffix}"));
    }

    // Resolves an Operand to its QIR ID and writes: `ptr inttoptr (i64 N to ptr)`
    fn write_operand(&mut self, operand: Operand) {
        let id = match operand {
            Operand::Qubit(stim_index) => self.map_qubit(stim_index),
            Operand::Result => self.next_result(),
        };
        write!(self.output, "ptr inttoptr (i64 {id} to ptr)").unwrap();
    }

    // Writes a label: `{name}:`
    fn write_label(&mut self, name: &str) {
        writeln!(self.output, "{name}:").unwrap();
    }

    // Writes: `  br i1 %{cond}, label %{true_label}, label %{false_label}`
    fn write_branch(&mut self, cond: &str, true_label: &str, false_label: &str) {
        writeln!(
            self.output,
            "  br i1 %{cond}, label %{true_label}, label %{false_label}"
        )
        .unwrap();
    }

    // Writes: `  br label %{label}`
    fn write_jump(&mut self, label: &str) {
        writeln!(self.output, "  br label %{label}").unwrap();
    }

    // Writes: `  %{dest} = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 N to ptr))`
    fn write_read_result(&mut self, dest: &str, operand: Operand) {
        write!(
            self.output,
            "  %{dest} = call i1 @__quantum__rt__read_result("
        )
        .unwrap();
        self.write_operand(operand);
        writeln!(self.output, ")").unwrap();
        self.used_intrinsics
            .entry("__quantum__rt__read_result".to_string())
            .or_insert_with(|| "declare i1 @__quantum__rt__read_result(ptr)".to_string());
    }

    fn write_header(&mut self) {
        writeln!(self.output, "define i64 @ENTRYPOINT__main() #0 {{").unwrap();
        writeln!(
            self.output,
            "  call void @__quantum__rt__initialize(ptr null)"
        )
        .unwrap();
        self.used_intrinsics
            .entry("__quantum__rt__initialize".to_string())
            .or_insert_with(|| "declare void @__quantum__rt__initialize(ptr)".to_string());
    }

    fn write_record_output(&mut self) {
        let num_results = self.num_results;
        writeln!(
            self.output,
            "  call void @__quantum__rt__array_record_output(i64 {num_results}, ptr null)"
        )
        .unwrap();
        self.used_intrinsics
            .entry("__quantum__rt__array_record_output".to_string())
            .or_insert_with(|| {
                "declare void @__quantum__rt__array_record_output(i64, ptr)".to_string()
            });
        for i in 0..num_results {
            writeln!(
                self.output,
                "  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 {i} to ptr), ptr null)"
            )
            .unwrap();
        }
        self.used_intrinsics
            .entry("__quantum__rt__result_record_output".to_string())
            .or_insert_with(|| {
                "declare void @__quantum__rt__result_record_output(ptr, ptr)".to_string()
            });
    }

    fn write_declarations(&mut self) {
        writeln!(self.output).unwrap();
        for decl in self.used_intrinsics.values() {
            writeln!(self.output, "{decl}").unwrap();
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
        writeln!(self.output, "attributes #2 = {{ \"qdk_noise\" }}").unwrap();
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

    // Maps a Stim qubit index to a 0-based QIR qubit ID.
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
    noise_tables: Vec<NoiseTable>,
    // Buffered CORRELATED_ERROR / ELSE_CORRELATED_ERROR chain: (probability, [(pauli, qubit)]).
    pending_correlated: Vec<(f64, Vec<(char, u32)>)>,
}

impl Compiler {
    fn new() -> Self {
        Self {
            writer: QirWriter::new(),
            last_preselect_begin: None,
            num_preselect_expects: 0,
            noise_tables: Vec::new(),
            pending_correlated: Vec::new(),
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
        // A pending CORRELATED_ERROR chain ends as soon as a non-chain instruction begins.
        if !Self::is_correlated_error(&instruction.name) {
            self.flush_correlated_error();
        }
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
            self.writer
                .write_call(&gate, &[Operand::Qubit(value)], None);
        }
    }

    fn compile_single_qubit_clifford_gate(&mut self, instruction: &Instruction) {
        let gate = instruction.name.to_lowercase();
        if gate == "h" || gate == "s" {
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                self.writer
                    .write_call(&gate, &[Operand::Qubit(value)], None);
            }
        } else if gate == "sqrt_x" {
            // decomposed into H S H
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                let q = Operand::Qubit(value);
                self.writer.write_call("h", &[q], None);
                self.writer.write_call("s", &[q], None);
                self.writer.write_call("h", &[q], None);
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
                    .write_call(&gate, &[Operand::Qubit(v0), Operand::Qubit(v1)], None);
            }
        }
    }

    fn compile_noise_channel(&mut self, instruction: &Instruction) {
        if !Self::is_correlated_error(&instruction.name) {
            return;
        }
        // A new CORRELATED_ERROR (alias E) starts a fresh chain; flush the previous one.
        if instruction.name != "ELSE_CORRELATED_ERROR" {
            self.flush_correlated_error();
        }
        let probability = instruction.args.first().copied().unwrap_or(0.0);
        let paulis = instruction
            .targets
            .iter()
            .filter_map(|t| match &t.kind {
                TargetKind::Pauli { pauli, value, .. } => {
                    let c = match pauli {
                        Pauli::X => 'X',
                        Pauli::Y => 'Y',
                        Pauli::Z => 'Z',
                    };
                    Some((c, *value))
                }
                _ => None,
            })
            .collect();
        self.pending_correlated.push((probability, paulis));
    }

    fn is_correlated_error(name: &str) -> bool {
        matches!(name, "CORRELATED_ERROR" | "E" | "ELSE_CORRELATED_ERROR")
    }

    // Emits the buffered CORRELATED_ERROR chain as a `correlated_noise_intrinsic_N` call
    // (a `qdk_noise` insertion point) plus a noise table mapping Pauli strings to probabilities.
    fn flush_correlated_error(&mut self) {
        if self.pending_correlated.is_empty() {
            return;
        }
        let chain = std::mem::take(&mut self.pending_correlated);

        // The qubits touched by the chain, sorted and deduplicated, define the intrinsic's
        // qubit arguments and the position of each qubit in the Pauli strings.
        let mut qubits: Vec<u32> = chain
            .iter()
            .flat_map(|(_, paulis)| paulis.iter().map(|(_, q)| *q))
            .collect();
        qubits.sort_unstable();
        qubits.dedup();

        let entries = chain
            .iter()
            .map(|(probability, paulis)| {
                let mut pauli_string = vec![b'I'; qubits.len()];
                for (pauli, q) in paulis {
                    let pos = qubits.iter().position(|x| x == q).expect("qubit in chain");
                    pauli_string[pos] = *pauli as u8;
                }
                (
                    String::from_utf8(pauli_string).expect("ascii pauli string"),
                    *probability,
                )
            })
            .collect();

        let name = format!("correlated_noise_intrinsic_{}", self.noise_tables.len() + 1);
        let operands: Vec<Operand> = qubits.iter().map(|&q| Operand::Qubit(q)).collect();
        self.writer.write_named_call(&name, &operands, Some(2));
        self.noise_tables.push(NoiseTable {
            name,
            qubits: qubits.len() as u32,
            entries,
        });
    }

    fn compile_collapsing_gate(&mut self, instruction: &Instruction) {
        let gate = instruction.name.to_lowercase();
        if gate == "r" {
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                self.writer
                    .write_call("reset", &[Operand::Qubit(value)], Some(1));
            }
        } else if gate == "mr" {
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                self.writer.write_call(
                    "mresetz",
                    &[Operand::Qubit(value), Operand::Result],
                    Some(1),
                );
            }
        } else if gate == "mrx" {
            // decomposed into H MRZ H
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                let q = Operand::Qubit(value);
                self.writer.write_call("h", &[q], None);
                self.writer
                    .write_call("mresetz", &[q, Operand::Result], Some(1));
                self.writer.write_call("h", &[q], None);
            }
        }
    }

    fn compile_pair_measurement_gate(&mut self, _instruction: &Instruction) {}

    fn compile_generalized_pauli_product_gate(&mut self, _instruction: &Instruction) {}

    fn compile_control_flow(&mut self, _instruction: &Instruction) {}

    fn compile_annotations(&mut self, _instruction: &Instruction) {}

    fn compile_custom_instruction(&mut self, instruction: &Instruction) {
        let instruction_name = instruction.name.to_lowercase();
        if instruction_name == "#!preselect_begin" {
            self.last_preselect_begin = match self.last_preselect_begin {
                None => Some(0),
                Some(n) => Some(n + 1),
            };
            let id = self.last_preselect_begin.unwrap();
            let label = format!("preselect_begin_{id}");
            self.writer.write_jump(&label); // terminate the previous block
            self.writer.write_label(&label); // start the new block
        } else if instruction_name == "#!preselect_expect" {
            let id = self.last_preselect_begin.unwrap();
            let reg = format!("preselect_r{}", self.num_preselect_expects);
            self.num_preselect_expects += 1;

            // First target: which result to read
            let TargetKind::Qubit {
                value: result_id, ..
            } = instruction.targets[0].kind
            else {
                return;
            };
            // Second target: expected value (0 or 1)
            let TargetKind::Qubit {
                value: expected, ..
            } = instruction.targets[1].kind
            else {
                return;
            };

            // Read the result into %reg
            self.writer
                .write_read_result(&reg, Operand::Qubit(result_id));

            let begin_label = format!("preselect_begin_{id}");
            let continue_label = format!("preselect_continue_{id}");

            // Branch: if result matches expected → continue, else → retry
            if expected == 0 {
                // expected 0: if read is true (1) → mismatch → retry
                self.writer
                    .write_branch(&reg, &begin_label, &continue_label);
            } else {
                // expected 1: if read is true (1) → match → continue
                self.writer
                    .write_branch(&reg, &continue_label, &begin_label);
            }

            self.writer.write_label(&continue_label);
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

    fn into_qir(mut self, circuit: &Circuit) -> StimCompilationResult {
        self.writer.write_header();
        self.compile_circuit(circuit);
        self.flush_correlated_error();
        self.writer.write_footer();
        StimCompilationResult {
            qir: self.writer.output,
            noise_tables: self.noise_tables,
        }
    }
}

pub fn compile_to_qir(circuit: &Circuit) -> StimCompilationResult {
    Compiler::new().into_qir(circuit)
}
