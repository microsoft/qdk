// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use qdk_simulators::noise_config::{NoiseConfig, NoiseTable, encode_pauli};

use crate::parser::*;
use miette::Diagnostic;
use qsc_data_structures::span::Span;
use rustc_hash::FxHashMap;
use std::fmt::Write;
use thiserror::Error;

#[derive(Clone, Copy)]
enum Operand {
    /// A qubit operand, carrying the raw Stim qubit index.
    Qubit(u32),
    /// A result operand — the writer allocates the next result ID.
    Result,
    /// A reference to an already-allocated result, carrying its QIR result ID.
    ExistingResult(u32),
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

    fn write_fmt(&mut self, args: std::fmt::Arguments) {
        self.output
            .write_fmt(args)
            .expect("writing to a String should be infallible");
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
        write!(self, "  call void @{intrinsic}(");
        for (i, &operand) in operands.iter().enumerate() {
            if i > 0 {
                write!(self, ", ");
            }
            self.write_operand(operand);
        }
        writeln!(self, ")");
        let params = (0..operands.len())
            .map(|_| "ptr")
            .collect::<Vec<_>>()
            .join(", ");
        self.used_intrinsics
            .entry(intrinsic.to_string())
            .or_insert_with(|| format!("declare void @{intrinsic}({params})"));
    }

    fn write_noise_intrinsic(&mut self, name: &str, qubits: &[u32]) {
        write!(self, "  call void @{name}(");
        for (i, &qubit) in qubits.iter().enumerate() {
            if i > 0 {
                write!(self, ", ");
            }
            // Register the qubit so it is reflected in `required_num_qubits`, but emit
            // the raw Stim index as the operand.
            let id = self.map_qubit(qubit);
            write!(self, "ptr inttoptr (i64 {id} to ptr)");
        }
        writeln!(self, ")");
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
            Operand::ExistingResult(result_id) => result_id,
        };
        write!(self, "ptr inttoptr (i64 {id} to ptr)");
    }

    // Writes a label: `{name}:`
    fn write_label(&mut self, name: &str) {
        writeln!(self, "{name}:");
    }

    // Writes: `  br i1 %{cond}, label %{true_label}, label %{false_label}`
    fn write_branch(&mut self, cond: &str, true_label: &str, false_label: &str) {
        writeln!(
            self,
            "  br i1 %{cond}, label %{true_label}, label %{false_label}"
        );
    }

    // Writes: `  br label %{label}`
    fn write_jump(&mut self, label: &str) {
        writeln!(self, "  br label %{label}");
    }

    // Writes: `  %{dest} = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 N to ptr))`
    fn write_read_result(&mut self, dest: &str, operand: Operand) {
        write!(self, "  %{dest} = call i1 @__quantum__rt__read_result(");
        self.write_operand(operand);
        writeln!(self, ")");
        self.used_intrinsics
            .entry("__quantum__rt__read_result".to_string())
            .or_insert_with(|| "declare i1 @__quantum__rt__read_result(ptr)".to_string());
    }

    fn write_header(&mut self) {
        writeln!(self, "define i64 @ENTRYPOINT__main() #0 {{");
        writeln!(self, "  call void @__quantum__rt__initialize(ptr null)");
        self.used_intrinsics
            .entry("__quantum__rt__initialize".to_string())
            .or_insert_with(|| "declare void @__quantum__rt__initialize(ptr)".to_string());
    }

    fn write_record_output(&mut self) {
        let num_results = self.num_results;
        writeln!(
            self,
            "  call void @__quantum__rt__array_record_output(i64 {num_results}, ptr null)"
        );
        self.used_intrinsics
            .entry("__quantum__rt__array_record_output".to_string())
            .or_insert_with(|| {
                "declare void @__quantum__rt__array_record_output(i64, ptr)".to_string()
            });
        for i in 0..num_results {
            writeln!(
                self,
                "  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 {i} to ptr), ptr null)"
            );
        }
        self.used_intrinsics
            .entry("__quantum__rt__result_record_output".to_string())
            .or_insert_with(|| {
                "declare void @__quantum__rt__result_record_output(ptr, ptr)".to_string()
            });
    }

    fn write_declarations(&mut self) {
        writeln!(self);
        let decls: Vec<String> = self.used_intrinsics.values().cloned().collect();
        for decl in decls {
            writeln!(self, "{decl}");
        }
    }

    fn write_footer(&mut self) {
        self.write_record_output();
        writeln!(self, "  ret i64 0");
        writeln!(self, "}}");
        self.write_declarations();

        let num_qubits = self.qubit_map.len();
        let num_results = self.num_results;
        writeln!(self);
        writeln!(
            self,
            "attributes #0 = {{ \"entry_point\" \"output_labeling_schema\" \"qir_profiles\"=\"adaptive_profile\" \"required_num_qubits\"=\"{num_qubits}\" \"required_num_results\"=\"{num_results}\" }}"
        );
        writeln!(self, "attributes #1 = {{ \"irreversible\" }}");
        writeln!(self);
        writeln!(self, "; module flags");
        writeln!(self);
        if self.has_noise_intrinsic {
            writeln!(self, "attributes #2 = {{ \"qdk_noise\" }}");
            writeln!(self);
        }
        writeln!(
            self,
            "!llvm.module.flags = !{{!0, !1, !2, !3, !4, !5, !6, !7}}"
        );
        writeln!(self);
        writeln!(self, "!0 = !{{i32 1, !\"qir_major_version\", i32 2}}");
        writeln!(self, "!1 = !{{i32 7, !\"qir_minor_version\", i32 1}}");
        writeln!(
            self,
            "!2 = !{{i32 1, !\"dynamic_qubit_management\", i1 false}}"
        );
        writeln!(
            self,
            "!3 = !{{i32 1, !\"dynamic_result_management\", i1 false}}"
        );
        writeln!(
            self,
            "!4 = !{{i32 5, !\"int_computations\", !{{!\"i64\"}}}}"
        );
        writeln!(
            self,
            "!5 = !{{i32 5, !\"float_computations\", !{{!\"double\"}}}}"
        );
        writeln!(self, "!6 = !{{i32 7, !\"backwards_branching\", i2 3}}");
        writeln!(self, "!7 = !{{i32 1, !\"arrays\", i1 true}}");
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
    independent: bool,
}

#[derive(Clone, Debug, Error, Diagnostic)]
pub enum Error {
    #[error("unsupported instruction: {name}")]
    #[diagnostic(code("Stim.UnsupportedInstruction"))]
    UnsupportedInstruction {
        name: String,
        #[label]
        span: Span,
    },
    #[error("unknown instruction: {name}")]
    #[diagnostic(code("Stim.UnknownInstruction"))]
    UnknownInstruction {
        name: String,
        #[label]
        span: Span,
    },
    #[error("unsupported argument in instruction: {instruction}")]
    #[diagnostic(code("Stim.UnsupportedArgument"))]
    UnsupportedArgument {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("unsupported target in instruction: {instruction}")]
    #[diagnostic(code("Stim.UnsupportedTarget"))]
    UnsupportedTarget {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("missing probability argument in instruction: {instruction}")]
    #[diagnostic(code("Stim.MissingProbability"))]
    MissingProbability {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("instruction {instruction} requires an even number of qubit targets")]
    #[diagnostic(code("Stim.OddQubitCount"))]
    OddQubitCount {
        instruction: String,
        #[label]
        span: Span,
    },
}

struct Compiler<'noise> {
    writer: QirWriter,
    last_preselect_begin: Option<u32>,
    num_preselect_expects: u32,
    noise: &'noise mut NoiseConfig<f64, f64>,
    current_correlated_group: Option<CorrelatedGroup>,
    num_noise_intrinsics: u32,
    errors: Vec<Error>,
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
            errors: Vec::new(),
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

        match instruction.name.as_str() {
            // Pauli Gates
            "I" => (),
            "X" | "Y" | "Z" => self.broadcast(instruction, |s, q| {
                s.op(&instruction.name.to_lowercase(), q);
            }),

            // Single Qubit Clifford Gates
            "C_NXYZ" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; H 0; S 0; S 0
                s.op_adj("s", q);
                s.op("h", q);
                s.op("z", q);
            }),
            "C_NZYX" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; H 0; S 0; S 0; S 0
                s.op("z", q);
                s.op("h", q);
                s.op_adj("s", q);
            }),
            "C_XNYZ" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): S 0; H 0
                s.op("s", q);
                s.op("h", q);
            }),
            "C_XYNZ" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): S 0; H 0; S 0; S 0
                s.op("s", q);
                s.op("h", q);
                s.op("z", q);
            }),
            "C_XYZ" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; H 0
                s.op_adj("s", q);
                s.op("h", q);
            }),
            "C_ZNYX" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): H 0; S 0; S 0; S 0
                s.op("h", q);
                s.op_adj("s", q);
            }),
            "C_ZYNX" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; H 0; S 0
                s.op("z", q);
                s.op("h", q);
                s.op("s", q);
            }),
            "C_ZYX" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): H 0; S 0
                s.op("h", q);
                s.op("s", q);
            }),
            "H" | "H_XZ" => self.broadcast(instruction, |s, q| s.op("h", q)),
            "H_NXY" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): S 0; H 0; S 0; S 0; H 0
                s.op("s", q);
                s.op("x", q);
            }),
            "H_NXZ" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; H 0; S 0; S 0
                s.op("z", q);
                s.op("h", q);
                s.op("z", q);
            }),
            "H_NYZ" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; H 0; S 0; H 0
                s.op("z", q);
                s.op("sx", q);
            }),
            "H_XY" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): H 0; S 0; S 0; H 0; S 0
                s.op("x", q);
                s.op("s", q);
            }),
            "H_YZ" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): H 0; S 0; H 0; S 0; S 0
                s.op("sx", q);
                s.op("z", q);
            }),
            "S" | "SQRT_Z" => self.broadcast(instruction, |s, q| s.op("s", q)),
            "SQRT_X" => self.broadcast(instruction, |s, q| s.op("sx", q)),
            "SQRT_X_DAG" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): S 0; H 0; S 0
                s.op("s", q);
                s.op("h", q);
                s.op("s", q);
            }),
            "SQRT_Y" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; H 0
                s.op("z", q);
                s.op("h", q);
            }),
            "SQRT_Y_DAG" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): H 0; S 0; S 0
                s.op("h", q);
                s.op("z", q);
            }),
            "S_DAG" | "SQRT_Z_DAG" => self.broadcast(instruction, |s, q| s.op_adj("s", q)),

            // Two Qubit Clifford Gates
            "CX" | "CNOT" | "ZCX" => self.broadcast_pair(instruction, |s, q0, q1| {
                s.op_2("cx", q0, q1);
            }),
            "CXSWAP" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): CX 1 0; CX 0 1
                s.op_2("cx", q1, q0);
                s.op_2("cx", q0, q1);
            }),
            "CY" | "ZCY" => self.broadcast_pair(instruction, |s, q0, q1| s.op_2("cy", q0, q1)),
            "CZ" | "ZCZ" => self.broadcast_pair(instruction, |s, q0, q1| s.op_2("cz", q0, q1)),
            "CZSWAP" | "SWAPCZ" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): H 0; CX 0 1; CX 1 0; H 1
                s.op("h", q0);
                s.op_2("cx", q0, q1);
                s.op_2("cx", q1, q0);
                s.op("h", q1);
            }),
            "II" => (),
            "ISWAP" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): H 0; CX 0 1; CX 1 0; H 1; S 1; S 0
                s.op("h", q0);
                s.op_2("cx", q0, q1);
                s.op_2("cx", q1, q0);
                s.op("h", q1);
                s.op("s", q1);
                s.op("s", q0);
            }),
            "ISWAP_DAG" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; S 1; S 1; S 1; H 1; CX 1 0; CX 0 1; H 0
                s.op_adj("s", q0);
                s.op_adj("s", q1);
                s.op("h", q1);
                s.op_2("cx", q1, q0);
                s.op_2("cx", q0, q1);
                s.op("h", q0);
            }),
            "SQRT_XX" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): H 0; CX 0 1; H 1; S 0; S 1; H 0; H 1
                s.op("h", q0);
                s.op_2("cx", q0, q1);
                s.op("h", q1);
                s.op("s", q0);
                s.op("s", q1);
                s.op("h", q0);
                s.op("h", q1);
            }),
            "SQRT_XX_DAG" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): H 0; CX 0 1; H 1; S 0; S 0; S 0; S 1; S 1; S 1; H 0; H 1
                s.op("h", q0);
                s.op_2("cx", q0, q1);
                s.op("h", q1);
                s.op_adj("s", q0);
                s.op_adj("s", q1);
                s.op("h", q0);
                s.op("h", q1);
            }),
            "SQRT_YY" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; S 1; S 1; S 1; H 0; CX 0 1; H 1; S 0; S 1; H 0; H 1; S 0; S 1
                s.op_adj("s", q0); // S 0; S 0; S 0
                s.op_adj("s", q1); // S 1; S 1; S 1
                s.op("h", q0);
                s.op_2("cx", q0, q1);
                s.op("h", q1);
                s.op("s", q0);
                s.op("s", q1);
                s.op("h", q0);
                s.op("h", q1);
                s.op("s", q0);
                s.op("s", q1);
            }),
            "SQRT_YY_DAG" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; S 1; H 0; CX 0 1; H 1; S 0; S 1; H 0; H 1; S 0; S 1; S 1; S 1
                s.op_adj("s", q0); // S 0; S 0; S 0
                s.op("s", q1); // S 1
                s.op("h", q0);
                s.op_2("cx", q0, q1);
                s.op("h", q1);
                s.op("s", q0);
                s.op("s", q1);
                s.op("h", q0);
                s.op("h", q1);
                s.op("s", q0);
                s.op_adj("s", q1); // S 1; S 1; S 1
            }),
            "SQRT_ZZ" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): H 1; CX 0 1; H 1; S 0; S 1
                s.op("h", q1);
                s.op_2("cx", q0, q1);
                s.op("h", q1);
                s.op("s", q0);
                s.op("s", q1);
            }),
            "SQRT_ZZ_DAG" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): H 1; CX 0 1; H 1; S 0; S 0; S 0; S 1; S 1; S 1
                s.op("h", q1);
                s.op_2("cx", q0, q1);
                s.op("h", q1);
                s.op_adj("s", q0);
                s.op_adj("s", q1);
            }),
            "SWAP" => self.broadcast_pair(instruction, |s, q0, q1| s.op_2("swap", q0, q1)),
            "SWAPCX" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): CX 0 1; CX 1 0
                s.op_2("cx", q0, q1);
                s.op_2("cx", q1, q0);
            }),
            "XCX" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): H 0; CX 0 1; H 0
                s.op("h", q0);
                s.op_2("cx", q0, q1);
                s.op("h", q0);
            }),
            "XCY" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): H 0; S 1; S 1; S 1; CX 0 1; H 0; S 1
                s.op("h", q0);
                s.op_adj("s", q1);
                s.op_2("cx", q0, q1);
                s.op("h", q0);
                s.op("s", q1);
            }),
            "XCZ" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): CX 1 0
                s.op_2("cx", q1, q0);
            }),
            "YCX" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; H 1; CX 1 0; S 0; H 1
                s.op_adj("s", q0);
                s.op("h", q1);
                s.op_2("cx", q1, q0);
                s.op("s", q0);
                s.op("h", q1);
            }),
            "YCY" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; S 1; S 1; S 1; H 0; CX 0 1; H 0; S 0; S 1
                s.op_adj("s", q0);
                s.op_adj("s", q1);
                s.op("h", q0);
                s.op_2("cx", q0, q1);
                s.op("h", q0);
                s.op("s", q0);
                s.op("s", q1);
            }),
            "YCZ" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; CX 1 0; S 0
                s.op_adj("s", q0);
                s.op_2("cx", q1, q0);
                s.op("s", q0);
            }),

            // Noise Channels
            "E" | "CORRELATED_ERROR" | "ELSE_CORRELATED_ERROR" => {
                self.accumulate_correlated_error(instruction)
            }
            "DEPOLARIZE1" => self.compile_depolarize_1(instruction),
            "DEPOLARIZE2" => self.compile_depolarize_2(instruction),
            "HERALDED_ERASE"
            | "HERALDED_PAULI_CHANNEL_1"
            | "II_ERROR"
            | "I_ERROR"
            | "PAULI_CHANNEL_1"
            | "PAULI_CHANNEL_2" => self.unsupported(instruction),
            "X_ERROR" | "Y_ERROR" | "Z_ERROR" | "LOSS_ERROR" => {
                self.compile_fault_error(instruction)
            }

            // Collapsing Gates
            "M" | "MZ" => self.broadcast(instruction, |s, q| s.op_measure("m", q)),
            "MR" | "MRZ" => self.broadcast(instruction, |s, q| s.op_measure("mresetz", q)),
            "MRX" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): H 0; M 0; R 0; H 0
                s.op("h", q); // X -> Z
                s.op_measure("mresetz", q); // MRZ
                s.op("h", q); // Z -> X
            }),
            "MRY" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; H 0; M 0; R 0; H 0; S 0
                s.op_adj("s", q); // Y -> X
                s.op("h", q); // X -> Z
                s.op_measure("mresetz", q); // MRZ
                s.op("h", q); // Z -> X
                s.op("s", q); // X -> Y
            }),
            "MX" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): H 0; M 0; H 0
                s.op("h", q); // X -> Z
                s.op_measure("m", q); // MZ
                s.op("h", q); // Z -> X
            }),
            "MY" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; H 0; M 0; H 0; S 0
                s.op_adj("s", q); // Y -> X
                s.op("h", q); // X -> Z
                s.op_measure("m", q); // MZ
                s.op("h", q); // Z -> X
                s.op("s", q); // X -> Y
            }),
            "R" | "RZ" => self.broadcast(instruction, |s, q| s.op("reset", q)),
            "RX" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): R 0; H 0
                s.op("reset", q); // RZ
                s.op("h", q); // Z -> X
            }),
            "RY" => self.broadcast(instruction, |s, q| {
                // Stim decomposition (into H, S, CX, M, R): R 0; H 0; S 0
                s.op("reset", q); // RZ
                s.op("h", q); // Z -> X
                s.op("s", q); // X -> Y
            }),

            // Pair Measurement Gates
            "MXX" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): CX 0 1; H 0; M 0; H 0; CX 0 1
                s.op_2("cx", q0, q1);
                s.op("h", q0);
                s.op_measure("m", q0);
                s.op("h", q0);
                s.op_2("cx", q0, q1);
            }),
            "MYY" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 1; CX 0 1; H 0; M 0; S 1; S 1; H 0; CX 0 1; S 0; S 1
                s.op("s", q0);
                s.op("s", q1);
                s.op_2("cx", q0, q1);
                s.op("h", q0);
                s.op_measure("m", q0);
                s.op("z", q1);
                s.op("h", q0);
                s.op_2("cx", q0, q1);
                s.op("s", q0);
                s.op("s", q1);
            }),
            "MZZ" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): CX 0 1; M 1; CX 0 1
                s.op_2("cx", q0, q1);
                s.op_measure("m", q1);
                s.op_2("cx", q0, q1);
            }),

            // Generalized Pauli Product Gates
            "MPP" | "SPP" | "SPP_DAG" => self.unsupported(instruction),

            // Control Flow
            "REPEAT" => self.unsupported(instruction),

            // Annotations
            "DETECTOR" | "MPAD" | "OBSERVABLE_INCLUDE" | "QUBIT_COORDS" | "SHIFT_COORDS"
            | "TICK" => (),

            // Custom Instructions
            "!rhai" => (),
            "#!preselect_begin" => self.compile_preselect_begin(),
            "#!preselect_expect" => self.compile_preselect_expect(instruction),

            _ => self.unknown(instruction),
        }
    }

    fn broadcast(&mut self, instruction: &Instruction, mut f: impl FnMut(&mut Self, u32)) {
        self.unsupported_args(instruction); // Temporary error
        for target in &instruction.targets {
            let Some(q) = self.expect_qubit(instruction, target) else {
                continue;
            };
            f(self, q);
        }
    }

    fn broadcast_pair(
        &mut self,
        instruction: &Instruction,
        mut f: impl FnMut(&mut Self, u32, u32),
    ) {
        self.unsupported_args(instruction); // Temporary error
        let targets = &instruction.targets;
        for pair in targets.chunks(2) {
            let Some(q0) = self.expect_qubit(instruction, &pair[0]) else {
                continue;
            };
            let Some(q1) = self.expect_qubit(instruction, &pair[1]) else {
                continue;
            };
            f(self, q0, q1);
        }
    }

    fn op(&mut self, intrinsic: &str, qubit: u32) {
        self.writer
            .write_qis_call(intrinsic, &[Operand::Qubit(qubit)]);
    }

    fn op_adj(&mut self, intrinsic: &str, qubit: u32) {
        self.writer
            .write_qis_adj_call(intrinsic, &[Operand::Qubit(qubit)]);
    }

    fn op_measure(&mut self, intrinsic: &str, qubit: u32) {
        self.writer
            .write_qis_call(intrinsic, &[Operand::Qubit(qubit), Operand::Result]);
    }

    fn op_2(&mut self, intrinsic: &str, q0: u32, q1: u32) {
        self.writer
            .write_qis_call(intrinsic, &[Operand::Qubit(q0), Operand::Qubit(q1)]);
    }

    fn compile_fault_error(&mut self, instruction: &Instruction) {
        let gate = instruction.name.to_lowercase();
        let fault = match gate.as_str() {
            "x_error" => FaultChar::X,
            "y_error" => FaultChar::Y,
            "z_error" => FaultChar::Z,
            _ => FaultChar::Loss,
        };
        let Some(probability) = self.expect_probability(instruction) else {
            return;
        };
        for target in &instruction.targets {
            let Some(value) = self.expect_qubit(instruction, target) else {
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
    }

    fn compile_depolarize_1(&mut self, instruction: &Instruction) {
        let Some(probability) = self.expect_probability(instruction) else {
            return;
        };
        let each = probability / 3.0;
        for target in &instruction.targets {
            let Some(value) = self.expect_qubit(instruction, target) else {
                continue;
            };
            {
                let group = self
                    .current_correlated_group
                    .get_or_insert_with(CorrelatedGroup::default);
                group.independent = true;
                for fault in [FaultChar::X, FaultChar::Y, FaultChar::Z] {
                    group.rows.push(CorrelatedRow {
                        terms: vec![(value, fault)],
                        probability: each,
                    });
                }
            }
            self.finish_correlated_group(); // one independent 1-qubit table per target
        }
    }

    fn compile_depolarize_2(&mut self, instruction: &Instruction) {
        if !instruction.targets.len().is_multiple_of(2) {
            self.push_error(Error::OddQubitCount {
                instruction: instruction.name.clone(),
                span: instruction.span,
            });
            return;
        }
        let Some(probability) = self.expect_probability(instruction) else {
            return;
        };
        let each = probability / 15.0;
        for pair in instruction.targets.chunks(2) {
            let Some(q0) = self.expect_qubit(instruction, &pair[0]) else {
                continue;
            };
            let Some(q1) = self.expect_qubit(instruction, &pair[1]) else {
                continue;
            };
            {
                let group = self
                    .current_correlated_group
                    .get_or_insert_with(CorrelatedGroup::default);
                group.independent = true;
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

    fn compile_preselect_begin(&mut self) {
        self.last_preselect_begin = match self.last_preselect_begin {
            None => Some(0),
            Some(n) => Some(n + 1),
        };
        let id = self
            .last_preselect_begin
            .expect("last_preselect_begin was just set to Some above");
        let label = format!("preselect_begin_{id}");
        self.writer.write_jump(&label); // terminate the previous block
        self.writer.write_label(&label); // start the new block
    }

    fn compile_preselect_expect(&mut self, instruction: &Instruction) {
        self.unsupported_args(instruction); // Temporary error
        let id = self
            .last_preselect_begin
            .expect("PRESELECT_EXPECT must be preceded by a PRESELECT_BEGIN");
        let reg = format!("preselect_r{}", self.num_preselect_expects);
        self.num_preselect_expects += 1;

        // First target: a measurement record (`rec[-N]`) selecting which result to read.
        let Some(offset) = self.expect_measurement_record(instruction, &instruction.targets[0])
        else {
            return;
        };
        // Second target: the expected value (0 or 1) as a plain uint.
        let Some(expected) = self.expect_qubit(instruction, &instruction.targets[1]) else {
            return;
        };

        // `rec[-N]` references the N-th most recent measurement; guard against integer underflow
        let Some(result_id) = self.writer.num_results.checked_sub(offset) else {
            self.push_error(Error::UnsupportedTarget {
                instruction: instruction.name.clone(),
                span: instruction.targets[0].span,
            });
            return;
        };

        // Read the result into %reg
        self.writer
            .write_read_result(&reg, Operand::ExistingResult(result_id));

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

    fn accumulate_correlated_error(&mut self, instruction: &Instruction) {
        let Some(probability) = self.expect_probability(instruction) else {
            return;
        };
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
        // Stim's correlated error chain is sequential: a row only fires if no earlier row did.
        // Independent groups (e.g. depolarizing channels) keep each row's raw probability.
        let mut remaining_probability = 1.0;
        for row in &group.rows {
            // Build the fault string over the group columns; untouched qubits are `I`.
            let mut chars = vec!['I'; columns.len()];
            for &(qubit, fault) in &row.terms {
                let idx = column_index[&qubit];
                chars[idx] = fault.as_char();
            }
            let pauli: String = chars.into_iter().collect();
            pauli_strings.push(encode_pauli(&pauli));
            let output_probability = if group.independent {
                row.probability
            } else {
                let p = remaining_probability * row.probability;
                remaining_probability *= 1.0 - row.probability;
                p
            };
            probabilities.push(output_probability);
        }

        let table = NoiseTable {
            qubits: columns.len() as u32,
            pauli_strings,
            probabilities,
        };
        self.noise.intrinsics.insert(id, table);

        self.writer.write_noise_intrinsic(&name, &columns);
    }

    fn expect_qubit(&mut self, instruction: &Instruction, target: &Target) -> Option<u32> {
        let TargetKind::Qubit {
            value,
            negated: false,
        } = target.kind
        else {
            self.push_error(Error::UnsupportedTarget {
                instruction: instruction.name.clone(),
                span: target.span,
            });
            return None;
        };
        Some(value)
    }

    fn expect_measurement_record(
        &mut self,
        instruction: &Instruction,
        target: &Target,
    ) -> Option<u32> {
        let TargetKind::MeasurementRecord { value } = target.kind else {
            self.push_error(Error::UnsupportedTarget {
                instruction: instruction.name.clone(),
                span: target.span,
            });
            return None;
        };
        Some(value)
    }

    fn expect_probability(&mut self, instruction: &Instruction) -> Option<f64> {
        let Some(&probability) = instruction.args.first() else {
            self.push_error(Error::MissingProbability {
                instruction: instruction.name.clone(),
                span: instruction.span,
            });
            return None;
        };
        Some(probability)
    }

    fn unsupported(&mut self, instruction: &Instruction) {
        self.push_error(Error::UnsupportedInstruction {
            name: instruction.name.clone(),
            span: instruction.span,
        });
    }

    fn unsupported_args(&mut self, instruction: &Instruction) {
        if !instruction.args.is_empty() {
            self.push_error(Error::UnsupportedArgument {
                instruction: instruction.name.clone(),
                span: instruction.span,
            });
        }
    }

    fn unknown(&mut self, instruction: &Instruction) {
        self.push_error(Error::UnknownInstruction {
            name: instruction.name.clone(),
            span: instruction.span,
        });
    }

    fn push_error(&mut self, error: Error) {
        self.errors.push(error);
    }

    fn into_qir(mut self, circuit: &Circuit) -> Result<String, Vec<Error>> {
        self.writer.write_header();
        self.compile_circuit(circuit);
        self.finish_correlated_group();
        self.writer.write_footer();
        if self.errors.is_empty() {
            Ok(self.writer.output)
        } else {
            Err(self.errors)
        }
    }
}

pub fn compile_to_qir(
    circuit: &Circuit,
    noise: &mut NoiseConfig<f64, f64>,
) -> Result<String, Vec<Error>> {
    Compiler::new(noise).into_qir(circuit)
}
