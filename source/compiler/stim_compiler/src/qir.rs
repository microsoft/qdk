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

struct QirWriter {
    output: String,
    used_intrinsics: FxHashMap<String, String>,
    has_noise_intrinsic: bool,
}

impl QirWriter {
    fn new() -> Self {
        Self {
            output: String::new(),
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
    fn write_qis_call(&mut self, intrinsic: &str, ids: &[u32]) {
        self.write_raw_call(&format!("__quantum__qis__{intrinsic}__body"), ids);
    }

    /// `__quantum__qis__{intrinsic}__adj`
    fn write_qis_adj_call(&mut self, intrinsic: &str, ids: &[u32]) {
        self.write_raw_call(&format!("__quantum__qis__{intrinsic}__adj"), ids);
    }

    // Writes: `  call void @{intrinsic}(ptr inttoptr (i64 N to ptr), ...)`
    fn write_raw_call(&mut self, intrinsic: &str, ids: &[u32]) {
        write!(self, "  call void @{intrinsic}(");
        for (i, &id) in ids.iter().enumerate() {
            if i > 0 {
                write!(self, ", ");
            }
            self.write_ptr(id);
        }
        writeln!(self, ")");
        let params = (0..ids.len()).map(|_| "ptr").collect::<Vec<_>>().join(", ");
        self.used_intrinsics
            .entry(intrinsic.to_string())
            .or_insert_with(|| format!("declare void @{intrinsic}({params})"));
    }

    fn write_noise_intrinsic(&mut self, name: &str, ids: &[u32]) {
        write!(self, "  call void @{name}(");
        for (i, &id) in ids.iter().enumerate() {
            if i > 0 {
                write!(self, ", ");
            }
            write!(self, "ptr inttoptr (i64 {id} to ptr)");
        }
        writeln!(self, ")");
        let params = (0..ids.len()).map(|_| "ptr").collect::<Vec<_>>().join(", ");
        self.used_intrinsics
            .entry(name.to_string())
            .or_insert_with(|| format!("declare void @{name}({params}) #2"));
        self.has_noise_intrinsic = true;
    }

    // writes: `ptr inttoptr (i64 N to ptr)`
    fn write_ptr(&mut self, id: u32) {
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
    fn write_read_result(&mut self, dest: &str, id: u32) {
        write!(self, "  %{dest} = call i1 @__quantum__rt__read_result(");
        self.write_ptr(id);
        writeln!(self, ")");
        self.used_intrinsics
            .entry("__quantum__rt__read_result".to_string())
            .or_insert_with(|| "declare i1 @__quantum__rt__read_result(ptr)".to_string());
    }

    // Writes: `  %{dest} = xor i1 %{lhs}, %{rhs}`
    fn write_xor(&mut self, dest: &str, lhs: &str, rhs: &str) {
        writeln!(self, "  %{dest} = xor i1 %{lhs}, %{rhs}");
    }

    // Writes: `  %{dest} = xor i1 %{operand}, true`
    fn write_not(&mut self, dest: &str, operand: &str) {
        writeln!(self, "  %{dest} = xor i1 %{operand}, true");
    }

    fn write_header(&mut self) {
        writeln!(self, "define i64 @ENTRYPOINT__main() #0 {{");
        writeln!(self, "  call void @__quantum__rt__initialize(ptr null)");
        self.used_intrinsics
            .entry("__quantum__rt__initialize".to_string())
            .or_insert_with(|| "declare void @__quantum__rt__initialize(ptr)".to_string());
    }

    fn write_record_output(&mut self, num_results: u32) {
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

    fn write_footer(&mut self, num_qubits: u32, num_results: u32) {
        self.write_record_output(num_results);
        writeln!(self, "  ret i64 0");
        writeln!(self, "}}");
        self.write_declarations();

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
    #[error(
        "else_correlated_error must be preceded by a correlated_error or else_correlated_error instruction"
    )]
    #[diagnostic(code("Stim.OrphanedElseCorrelatedError"))]
    OrphanedElseCorrelatedError {
        #[label]
        span: Span,
    },
    #[error("measurement record is out of bounds")]
    #[diagnostic(code("Stim.MeasurementRecordOutOfBounds"))]
    MeasurementRecordOutOfBounds {
        #[label]
        span: Span,
    },
    #[error("measurement record refers to a measurement outside the enclosing PREPARE block")]
    #[diagnostic(code("Stim.MeasurementRecordOutOfScope"))]
    MeasurementRecordOutOfScope {
        #[label]
        span: Span,
    },
    #[error("require must appear inside a PREPARE block")]
    #[diagnostic(code("Stim.RequireOutsidePrepareBlock"))]
    RequireOutsidePrepareBlock {
        #[label]
        span: Span,
    },
    #[error("prepare instruction must start a block")]
    #[diagnostic(code("Stim.PrepareWithoutBlock"))]
    PrepareWithoutBlock {
        #[label]
        span: Span,
    },
}

struct IdMap {
    qubit_map: FxHashMap<u32, u32>,
    record_map: Vec<Option<u32>>, // index = result id, value = owning scope (if none then the result is not in a block)
    scope_stack: Vec<u32>,        // active nested scopes; last() = current
    name_counters: FxHashMap<&'static str, u32>, // prefix -> next index
}

impl IdMap {
    fn new() -> Self {
        Self {
            qubit_map: FxHashMap::default(),
            record_map: Vec::new(),
            scope_stack: Vec::new(),
            name_counters: FxHashMap::default(),
        }
    }

    fn fresh_name(&mut self, prefix: &'static str) -> String {
        let counter = self.name_counters.entry(prefix).or_insert(0);
        let id = *counter;
        *counter += 1;
        format!("{prefix}_{id}")
    }

    fn enter_scope(&mut self) {
        let counter = self.name_counters.entry("scope").or_insert(0);
        let id = *counter;
        *counter += 1;
        self.scope_stack.push(id);
    }

    fn exit_scope(&mut self) {
        self.scope_stack.pop();
    }

    fn current_scope(&self) -> Option<u32> {
        self.scope_stack.last().copied()
    }

    fn in_prepare_block(&self) -> bool {
        !self.scope_stack.is_empty()
    }

    fn scope_of(&self, id: u32) -> Option<u32> {
        match self.record_map.get(id as usize) {
            Some(&scope) => scope,
            None => unreachable!("record id not found"), // this is a compiler invariant
        }
    }

    fn allocate_record(&mut self) -> u32 {
        let id = self.record_map.len() as u32;
        let current_scope = self.current_scope();
        self.record_map.push(current_scope);
        id
    }

    fn allocate_qubit(&mut self, stim_index: u32) -> u32 {
        let next_id = self.qubit_map.len() as u32;
        *self.qubit_map.entry(stim_index).or_insert(next_id)
    }

    fn num_results(&self) -> u32 {
        self.record_map.len() as u32
    }

    fn num_qubits(&self) -> u32 {
        self.qubit_map.len() as u32
    }
}

fn prepare_label(scope: u32) -> String {
    format!("prepare_{scope}")
}

struct Compiler<'noise> {
    writer: QirWriter,
    noise: &'noise mut NoiseConfig<f64, f64>,
    current_correlated_group: Option<CorrelatedGroup>,
    num_noise_intrinsics: u32,
    errors: Vec<Error>,
    id_map: IdMap,
}

impl<'noise> Compiler<'noise> {
    fn new(noise: &'noise mut NoiseConfig<f64, f64>) -> Self {
        Self {
            writer: QirWriter::new(),
            noise,
            current_correlated_group: None,
            num_noise_intrinsics: 0,
            errors: Vec::new(),
            id_map: IdMap::new(),
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

        self.id_map.enter_scope();
        self.compile_instruction(block_instruction);
        for item in items {
            self.compile_item(item);
        }
        self.id_map.exit_scope();
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
            "E" | "CORRELATED_ERROR" => self.accumulate_correlated_error(instruction),
            "ELSE_CORRELATED_ERROR" => {
                if self.current_correlated_group.is_none() {
                    self.push_error(Error::OrphanedElseCorrelatedError {
                        span: instruction.span,
                    });
                } else {
                    self.accumulate_correlated_error(instruction);
                }
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
            "PREPARE" => self.compile_prepare(instruction),
            "REQUIRE" => self.compile_require(instruction),

            // Annotations
            "DETECTOR" | "MPAD" | "OBSERVABLE_INCLUDE" | "QUBIT_COORDS" | "SHIFT_COORDS"
            | "TICK" => (),

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
        let q = self.id_map.allocate_qubit(qubit);
        self.writer.write_qis_call(intrinsic, &[q]);
    }

    fn op_adj(&mut self, intrinsic: &str, qubit: u32) {
        let q = self.id_map.allocate_qubit(qubit);
        self.writer.write_qis_adj_call(intrinsic, &[q]);
    }

    fn op_measure(&mut self, intrinsic: &str, qubit: u32) {
        let q = self.id_map.allocate_qubit(qubit);
        let r = self.id_map.allocate_record();
        self.writer.write_qis_call(intrinsic, &[q, r]);
    }

    fn op_2(&mut self, intrinsic: &str, q0: u32, q1: u32) {
        let q0 = self.id_map.allocate_qubit(q0);
        let q1 = self.id_map.allocate_qubit(q1);
        self.writer.write_qis_call(intrinsic, &[q0, q1]);
    }

    fn compile_prepare(&mut self, instruction: &Instruction) {
        if !instruction.targets.is_empty() {
            self.push_error(Error::UnsupportedTarget {
                instruction: instruction.name.clone(),
                span: instruction
                    .targets
                    .first()
                    .map(|t| t.span)
                    .unwrap_or(instruction.span),
            });
            return;
        }

        if !instruction.args.is_empty() {
            self.push_error(Error::UnsupportedArgument {
                instruction: instruction.name.clone(),
                span: instruction.span,
            });
            return;
        }

        let Some(scope) = self.id_map.current_scope() else {
            self.push_error(Error::PrepareWithoutBlock {
                span: instruction.span,
            });
            return;
        };

        let label = prepare_label(scope);
        self.writer.write_jump(&label); // terminate the previous block
        self.writer.write_label(&label); // start the new block
    }

    fn compile_require(&mut self, instruction: &Instruction) {
        if !self.id_map.in_prepare_block() {
            self.push_error(Error::RequireOutsidePrepareBlock {
                span: instruction.span,
            });
            return;
        }

        if instruction.targets.is_empty() {
            self.push_error(Error::UnsupportedTarget {
                instruction: instruction.name.clone(),
                span: instruction.span,
            });
            return;
        }

        let mut read_registers = Vec::new();
        for target in &instruction.targets {
            let Some((offset, negated)) = self.expect_measurement_record(instruction, target)
            else {
                return;
            };
            let Some(result_id) = self.resolve_record_offset(target, offset) else {
                return;
            };
            let read_register = self.id_map.fresh_name("r");
            self.writer.write_read_result(&read_register, result_id);

            let term = if negated {
                let not_register = self.id_map.fresh_name("r");
                self.writer.write_not(&not_register, &read_register);
                not_register
            } else {
                read_register
            };
            read_registers.push(term);
        }

        let Some((first, rest)) = read_registers.split_first() else {
            unreachable!("REQUIRE always has at least one target");
        };

        let mut parity = first.clone();
        for reg in rest {
            let temp = self.id_map.fresh_name("x");
            self.writer.write_xor(&temp, &parity, reg);
            parity = temp;
        }

        let Some(scope) = self.id_map.current_scope() else {
            unreachable!("REQUIRE runs inside a prepare block");
        };
        let restart_label = prepare_label(scope);
        let continue_label = self.id_map.fresh_name("continue");
        self.writer
            .write_branch(&parity, &restart_label, &continue_label);
        self.writer.write_label(&continue_label);
    }

    fn resolve_record_offset(&mut self, target: &Target, offset: u32) -> Option<u32> {
        let num_results = self.id_map.num_results();
        let Some(result_id) = num_results.checked_sub(offset) else {
            self.push_error(Error::MeasurementRecordOutOfBounds { span: target.span });
            return None;
        };

        if self.id_map.scope_of(result_id) != self.id_map.current_scope() {
            self.push_error(Error::MeasurementRecordOutOfScope { span: target.span });
            return None;
        }
        Some(result_id)
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

        let column_ids: Vec<u32> = columns
            .iter()
            .map(|&stim_index| self.id_map.allocate_qubit(stim_index))
            .collect();
        self.writer.write_noise_intrinsic(&name, &column_ids);
    }

    fn expect_qubit(&mut self, instruction: &Instruction, target: &Target) -> Option<u32> {
        // TODO: lacks support for negated qubits and pauli targets
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
    ) -> Option<(u32, bool)> {
        let TargetKind::MeasurementRecord { negated, value } = target.kind else {
            self.push_error(Error::UnsupportedTarget {
                instruction: instruction.name.clone(),
                span: target.span,
            });
            return None;
        };
        Some((value, negated))
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
        self.writer
            .write_footer(self.id_map.num_qubits(), self.id_map.num_results());
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
