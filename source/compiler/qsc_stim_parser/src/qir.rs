// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qdk_simulators::noise_config::{NoiseConfig, NoiseTable, encode_pauli};

use crate::parser::*;
use rustc_hash::FxHashMap;
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
    qubit_map: FxHashMap<u32, u32>,
    num_results: u32,
    used_intrinsics: FxHashMap<String, String>,
    has_noise_intrinsic: bool,
}

impl QirWriter {
    fn new() -> Self {
        Self {
            output: String::new(),
            qubit_map: FxHashMap::default(),
            num_results: 0,
            used_intrinsics: FxHashMap::default(),
            has_noise_intrinsic: false,
        }
    }

    /// `__quantum__qis__{intrinsic}__body`
    fn write_qis_call(&mut self, intrinsic: &str, operands: &[Operand]) {
        self.write_raw_call(&format!("__quantum__qis__{intrinsic}__body"), operands);
    }

    /// `__quantum__qis__{intrinsic}__adj`
    fn write_qis_adj_call(&mut self, intrinsic: &str, operands: &[Operand]) {
        self.write_raw_call(&format!("__quantum__qis__{intrinsic}__adj"), operands);
    }

    // Writes: `  call void @{intrinsic}(ptr inttoptr (i64 N to ptr), ...)`
    // Resolves qubit indices via the qubit map and allocates result IDs internally.
    fn write_raw_call(&mut self, intrinsic: &str, operands: &[Operand]) {
        write!(self.output, "  call void @{intrinsic}(").unwrap();
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
        self.used_intrinsics
            .entry(intrinsic.to_string())
            .or_insert_with(|| format!("declare void @{intrinsic}({params})"));
    }

    fn write_noise_intrinsic(&mut self, name: &str, qubits: &[u32]) {
        write!(self.output, "  call void @{name}(").unwrap();
        for (i, &qubit) in qubits.iter().enumerate() {
            if i > 0 {
                write!(self.output, ", ").unwrap();
            }
            // Register the qubit so it is reflected in `required_num_qubits`, but emit
            // the raw Stim index as the operand.
            let id = self.map_qubit(qubit);
            write!(self.output, "ptr inttoptr (i64 {id} to ptr)").unwrap();
        }
        writeln!(self.output, ")").unwrap();
        let params = (0..qubits.len())
            .map(|_| "ptr")
            .collect::<Vec<_>>()
            .join(", ");
        self.used_intrinsics
            .entry(name.to_string())
            .or_insert_with(|| format!("declare void @{name}({params}) #2"));
        self.has_noise_intrinsic = true;
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
        writeln!(self.output).unwrap();
        writeln!(self.output, "; module flags").unwrap();
        writeln!(self.output).unwrap();
        if self.has_noise_intrinsic {
            writeln!(self.output, "attributes #2 = {{ \"qdk_noise\" }}").unwrap();
            writeln!(self.output).unwrap();
        }
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

/// A single fault term (`X`, `Y`, `Z`, or `L`) applied to a qubit.
#[derive(Clone, Copy)]
enum FaultChar {
    X,
    Y,
    Z,
    Loss,
}

impl FaultChar {
    fn as_char(self) -> char {
        match self {
            FaultChar::X => 'X',
            FaultChar::Y => 'Y',
            FaultChar::Z => 'Z',
            FaultChar::Loss => 'L',
        }
    }
}

struct CorrelatedRow {
    terms: Vec<(u32, FaultChar)>,
    probability: f64,
}

/// An accumulating `CORRELATED_ERROR` / `ELSE_CORRELATED_ERROR` group.
#[derive(Default)]
struct CorrelatedGroup {
    rows: Vec<CorrelatedRow>,
}

struct Compiler<'noise> {
    writer: QirWriter,
    last_preselect_begin: Option<u32>,
    num_preselect_expects: u32,
    noise: &'noise mut NoiseConfig<f64, f64>,
    current_correlated_group: Option<CorrelatedGroup>,
    num_noise_intrinsics: u32,
}

impl<'noise> Compiler<'noise> {
    fn new(noise: &'noise mut NoiseConfig<f64, f64>) -> Self {
        Self {
            writer: QirWriter::new(),
            last_preselect_begin: None,
            num_preselect_expects: 0,
            noise,
            current_correlated_group: None,
            num_noise_intrinsics: 0,
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
        if self.current_correlated_group.is_some() && instruction.name != "ELSE_CORRELATED_ERROR" {
            self.finish_correlated_group();
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
            self.writer.write_qis_call(&gate, &[Operand::Qubit(value)]);
        }
    }

    fn compile_single_qubit_clifford_gate(&mut self, instruction: &Instruction) {
        let gate = instruction.name.to_lowercase();
        if gate == "h" || gate == "s" {
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                self.writer.write_qis_call(&gate, &[Operand::Qubit(value)]);
            }
        } else if gate == "sqrt_x" {
            // decomposed into H S H
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                let q = Operand::Qubit(value);
                self.writer.write_qis_call("sx", &[q]);
            }
        } else if gate == "s_dag" {
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                self.writer
                    .write_qis_adj_call("s", &[Operand::Qubit(value)]);
            }
        }
    }

    fn compile_two_qubit_clifford_gate(&mut self, instruction: &Instruction) {
        let gate = instruction.name.to_lowercase();
        if gate == "cz" || gate == "cx" || gate == "cy" {
            let targets = &instruction.targets;
            for pair in targets.chunks(2) {
                let TargetKind::Qubit { value: v0, .. } = pair[0].kind else {
                    continue;
                };
                let TargetKind::Qubit { value: v1, .. } = pair[1].kind else {
                    continue;
                };
                self.writer
                    .write_qis_call(&gate, &[Operand::Qubit(v0), Operand::Qubit(v1)]);
            }
        } else if gate == "swap" {
            let targets = &instruction.targets;
            for pair in targets.chunks(2) {
                let TargetKind::Qubit { value: v0, .. } = pair[0].kind else {
                    continue;
                };
                let TargetKind::Qubit { value: v1, .. } = pair[1].kind else {
                    continue;
                };
                self.writer
                    .write_qis_call("swap", &[Operand::Qubit(v0), Operand::Qubit(v1)]);
            }
        }
    }

    fn compile_noise_channel(&mut self, instruction: &Instruction) {
        let gate = instruction.name.to_lowercase();
        if gate == "correlated_error" || gate == "else_correlated_error" {
            self.accumulate_correlated_error(instruction);
        } else if gate == "x_error"
            || gate == "y_error"
            || gate == "z_error"
            || gate == "loss_error"
        {
            let fault = match gate.as_str() {
                "x_error" => FaultChar::X,
                "y_error" => FaultChar::Y,
                "z_error" => FaultChar::Z,
                _ => FaultChar::Loss,
            };
            let probability = instruction.args[0];
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                self.current_correlated_group
                    .get_or_insert_with(CorrelatedGroup::default)
                    .rows
                    .push(CorrelatedRow {
                        terms: vec![(value, fault)],
                        probability,
                    });
                self.finish_correlated_group(); // one independent table per qubit
            }
        } else if gate == "depolarize1" {
            let each = instruction.args[0] / 3.0;
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                {
                    let group = self
                        .current_correlated_group
                        .get_or_insert_with(CorrelatedGroup::default);
                    for fault in [FaultChar::X, FaultChar::Y, FaultChar::Z] {
                        group.rows.push(CorrelatedRow {
                            terms: vec![(value, fault)],
                            probability: each,
                        });
                    }
                }
                self.finish_correlated_group(); // one independent 1-qubit table per target
            }
        } else if gate == "depolarize2" {
            let each = instruction.args[0] / 15.0;
            for pair in instruction.targets.chunks(2) {
                let TargetKind::Qubit { value: q0, .. } = pair[0].kind else {
                    continue;
                };
                let TargetKind::Qubit { value: q1, .. } = pair[1].kind else {
                    continue;
                };
                {
                    let group = self
                        .current_correlated_group
                        .get_or_insert_with(CorrelatedGroup::default);
                    // All 16 (p0, p1) combos except (I, I); None means identity on that qubit.
                    let options = [
                        None,
                        Some(FaultChar::X),
                        Some(FaultChar::Y),
                        Some(FaultChar::Z),
                    ];
                    for p0 in options {
                        for p1 in options {
                            if p0.is_none() && p1.is_none() {
                                continue; // skip identity
                            }
                            let mut terms = Vec::new();
                            if let Some(f) = p0 {
                                terms.push((q0, f));
                            }
                            if let Some(f) = p1 {
                                terms.push((q1, f));
                            }
                            group.rows.push(CorrelatedRow {
                                terms,
                                probability: each,
                            });
                        }
                    }
                }
                self.finish_correlated_group(); // one independent 2-qubit table per pair
            }
        }
    }

    fn accumulate_correlated_error(&mut self, instruction: &Instruction) {
        let probability = instruction.args[0];
        let mut terms = Vec::new();
        for target in &instruction.targets {
            match &target.kind {
                TargetKind::Pauli { pauli, value, .. } => {
                    let fault = match pauli {
                        Pauli::X => FaultChar::X,
                        Pauli::Y => FaultChar::Y,
                        Pauli::Z => FaultChar::Z,
                    };
                    terms.push((*value, fault));
                }
                TargetKind::Loss { value } => {
                    terms.push((*value, FaultChar::Loss));
                }
                _ => {}
            }
        }

        self.current_correlated_group
            .get_or_insert_with(CorrelatedGroup::default)
            .rows
            .push(CorrelatedRow { terms, probability })
    }

    fn finish_correlated_group(&mut self) {
        let Some(group) = self.current_correlated_group.take() else {
            return;
        };
        if group.rows.is_empty() {
            return;
        }

        let id = self.num_noise_intrinsics;
        self.num_noise_intrinsics += 1;
        let name = format!("noise_intrinsic_{id}");

        // Columns are the sorted union of every qubit touched by the group.
        let mut columns: Vec<u32> = group
            .rows
            .iter()
            .flat_map(|row| row.terms.iter().map(|(qubit, _)| *qubit))
            .collect();
        columns.sort_unstable();
        columns.dedup();

        let column_index: FxHashMap<u32, usize> = columns
            .iter()
            .enumerate()
            .map(|(i, &qubit)| (qubit, i))
            .collect();

        let mut pauli_strings = Vec::with_capacity(group.rows.len());
        let mut probabilities = Vec::with_capacity(group.rows.len());
        for row in &group.rows {
            // Build the fault string over the group columns; untouched qubits are `I`.
            let mut chars = vec!['I'; columns.len()];
            for &(qubit, fault) in &row.terms {
                let idx = column_index[&qubit];
                chars[idx] = fault.as_char();
            }
            let pauli: String = chars.into_iter().collect();
            pauli_strings.push(encode_pauli(&pauli));
            probabilities.push(row.probability);
        }

        let table = NoiseTable {
            qubits: columns.len() as u32,
            pauli_strings,
            probabilities,
        };
        self.noise.intrinsics.insert(id, table);

        self.writer.write_noise_intrinsic(&name, &columns);
    }

    fn compile_collapsing_gate(&mut self, instruction: &Instruction) {
        let gate = instruction.name.to_lowercase();
        if gate == "r" {
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                self.writer
                    .write_qis_call("reset", &[Operand::Qubit(value)]);
            }
        } else if gate == "m" {
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                self.writer
                    .write_qis_call("m", &[Operand::Qubit(value), Operand::Result]);
            }
        } else if gate == "mr" {
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                self.writer
                    .write_qis_call("mresetz", &[Operand::Qubit(value), Operand::Result]);
            }
        } else if gate == "mrx" {
            // decomposed into H MRZ H
            for target in &instruction.targets {
                let TargetKind::Qubit { value, .. } = target.kind else {
                    continue;
                };
                let q = Operand::Qubit(value);
                self.writer.write_qis_call("h", &[q]);
                self.writer.write_qis_call("mresetz", &[q, Operand::Result]);
                self.writer.write_qis_call("h", &[q]);
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
            | "Z_ERROR"
            | "LOSS_ERROR" => InstructionKind::NoiseChannel,

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
        self.finish_correlated_group();
        self.writer.write_footer();
        self.writer.output
    }
}

pub fn compile_to_qir(circuit: &Circuit, noise: &mut NoiseConfig<f64, f64>) -> String {
    Compiler::new(noise).into_qir(circuit)
}
