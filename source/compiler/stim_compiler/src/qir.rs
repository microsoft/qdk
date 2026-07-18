// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use qdk_simulators::noise_config::{LossPolicy, NoiseConfig, NoiseTable, encode_pauli};

use crate::parser::*;
use miette::Diagnostic;
use qsc_data_structures::span::Span;
use rustc_hash::FxHashMap;
use std::fmt::Write;
use std::slice::Chunks;
use thiserror::Error;

struct QirWriter {
    output: String,
    used_intrinsics: FxHashMap<String, String>,
    defined_functions: FxHashMap<String, String>,
    has_noise_intrinsic: bool,
}

impl QirWriter {
    fn new() -> Self {
        Self {
            output: String::new(),
            used_intrinsics: FxHashMap::default(),
            defined_functions: FxHashMap::default(),
            has_noise_intrinsic: false,
        }
    }

    fn write_fmt(&mut self, args: std::fmt::Arguments) {
        self.output
            .write_fmt(args)
            .expect("writing to a String should be infallible");
    }

    fn declare(&mut self, name: &str, declaration: impl FnOnce() -> String) {
        self.used_intrinsics
            .entry(name.to_string())
            .or_insert_with(declaration);
    }

    /// `__quantum__qis__{intrinsic}__body`
    fn write_qis_call(&mut self, intrinsic: &str, ids: &[u32]) {
        self.call_intrinsic(&format!("__quantum__qis__{intrinsic}__body"), ids, false);
    }

    /// `__quantum__qis__{intrinsic}__adj`
    fn write_qis_adj_call(&mut self, intrinsic: &str, ids: &[u32]) {
        self.call_intrinsic(&format!("__quantum__qis__{intrinsic}__adj"), ids, false);
    }

    /// `noise_intrinsic_{id}`
    fn write_noise_call(&mut self, name: &str, ids: &[u32]) {
        self.call_intrinsic(name, ids, true);
    }

    fn call_intrinsic(&mut self, intrinsic: &str, ids: &[u32], noise: bool) {
        self.write_call(intrinsic, ids);
        let attribute = if noise { " #2" } else { "" };
        self.declare(intrinsic, || {
            let params = vec!["ptr"; ids.len()].join(", ");
            format!("declare void @{intrinsic}({params}){attribute}")
        });
        if noise {
            self.has_noise_intrinsic = true;
        }
    }

    // Writes: `  call void @{name}(ptr inttoptr (i64 N to ptr), ...)` without declaring `name`.
    fn write_call(&mut self, name: &str, ids: &[u32]) {
        write!(self, "  call void @{name}(");
        for (i, &id) in ids.iter().enumerate() {
            if i > 0 {
                write!(self, ", ");
            }
            self.write_ptr(id);
        }
        writeln!(self, ")");
    }

    fn call_internal_helper(
        &mut self,
        name: &str,
        ids: &[u32],
        definition: impl FnOnce() -> String,
    ) {
        self.defined_functions
            .entry(name.to_string())
            .or_insert_with(definition);
        self.write_call(name, ids);
    }

    fn write_classical_control(&mut self, pauli: &str, result_id: u32, qubit: u32) {
        self.declare("__quantum__rt__read_result", || {
            "declare i1 @__quantum__rt__read_result(ptr)".to_string()
        });
        self.declare(&format!("__quantum__qis__{pauli}__body"), || {
            format!("declare void @__quantum__qis__{pauli}__body(ptr)")
        });
        let name = format!("classical_control_c{pauli}");
        self.call_internal_helper(&name, &[result_id, qubit], || {
            Self::classical_control_def(pauli)
        });
    }

    fn classical_control_def(pauli: &str) -> String {
        format!(
            "define void @classical_control_c{pauli}(ptr %result, ptr %qubit) {{
block_c{pauli}_entry:
  %result_val = call i1 @__quantum__rt__read_result(ptr %result)
  br i1 %result_val, label %block_c{pauli}_apply, label %block_c{pauli}_exit
block_c{pauli}_apply:
  call void @__quantum__qis__{pauli}__body(ptr %qubit)
  br label %block_c{pauli}_exit
block_c{pauli}_exit:
  ret void
}}"
        )
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

    // Writes: `  %{dest} = call i1 @{intrinsic}(ptr inttoptr (i64 N to ptr))`
    fn write_read(&mut self, dest: &str, intrinsic: &str, id: u32) {
        write!(self, "  %{dest} = call i1 @{intrinsic}(");
        self.write_ptr(id);
        writeln!(self, ")");
        self.declare(intrinsic, || format!("declare i1 @{intrinsic}(ptr)"));
    }

    // Writes: `  %{dest} = or i1 %{lhs}, %{rhs}`
    fn write_or(&mut self, dest: &str, lhs: &str, rhs: &str) {
        writeln!(self, "  %{dest} = or i1 %{lhs}, %{rhs}");
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
        self.declare("__quantum__rt__initialize", || {
            "declare void @__quantum__rt__initialize(ptr)".to_string()
        });
    }

    fn write_record_output(&mut self, num_results: u32) {
        writeln!(
            self,
            "  call void @__quantum__rt__array_record_output(i64 {num_results}, ptr null)"
        );
        self.declare("__quantum__rt__array_record_output", || {
            "declare void @__quantum__rt__array_record_output(i64, ptr)".to_string()
        });
        for i in 0..num_results {
            writeln!(
                self,
                "  call void @__quantum__rt__result_record_output(ptr inttoptr (i64 {i} to ptr), ptr null)"
            );
        }
        self.declare("__quantum__rt__result_record_output", || {
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

    fn write_definitions(&mut self) {
        let definitions: Vec<String> = self.defined_functions.values().cloned().collect();
        for definition in definitions {
            writeln!(self);
            writeln!(self, "{definition}");
        }
    }

    fn write_footer(&mut self, num_qubits: u32, num_results: u32) {
        self.write_record_output(num_results);
        writeln!(self, "  ret i64 0");
        writeln!(self, "}}");
        self.write_definitions();
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
    fn from_instruction_name(name: &str) -> Self {
        match name {
            "X_ERROR" => Self::X,
            "Y_ERROR" => Self::Y,
            "Z_ERROR" => Self::Z,
            "LOSS_ERROR" => Self::Loss,
            _ => unreachable!("unknown error name: {name}"),
        }
    }

    fn from_pauli(pauli: Pauli) -> Self {
        match pauli {
            Pauli::X => Self::X,
            Pauli::Y => Self::Y,
            Pauli::Z => Self::Z,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::X => "X",
            Self::Y => "Y",
            Self::Z => "Z",
            Self::Loss => "L",
        }
    }
}

#[derive(Clone, Debug, Error, Diagnostic)]
pub enum Error {
    #[error("unsupported instruction: {name}")]
    #[diagnostic(code("Qdk.Stim.Compiler.UnsupportedInstruction"))]
    UnsupportedInstruction {
        name: String,
        #[label]
        span: Span,
    },
    #[error("unknown instruction: {name}")]
    #[diagnostic(code("Qdk.Stim.Compiler.UnknownInstruction"))]
    UnknownInstruction {
        name: String,
        #[label]
        span: Span,
    },
    #[error("unsupported argument in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Compiler.UnsupportedArgument"))]
    UnsupportedArgument {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("unsupported target in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Compiler.UnsupportedTarget"))]
    UnsupportedTarget {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("measurement record target in an unsupported position in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Compiler.MisplacedMeasurementRecord"))]
    MisplacedMeasurementRecord {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("target cannot be negated in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Compiler.NegatedTarget"))]
    NegatedTarget {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error(
        "controlled instruction {instruction} requires a qubit target, but both targets are measurement records"
    )]
    #[diagnostic(code("Qdk.Stim.Compiler.MeasurementRecordWithoutQubit"))]
    MeasurementRecordWithoutQubit {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("missing probability argument in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Compiler.MissingProbability"))]
    MissingProbability {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("instruction {instruction} requires an even number of targets")]
    #[diagnostic(code("Qdk.Stim.Compiler.OddTargetCount"))]
    OddTargetCount {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error(
        "else_correlated_error must be preceded by a correlated_error or else_correlated_error instruction"
    )]
    #[diagnostic(code("Qdk.Stim.Compiler.OrphanedElseCorrelatedError"))]
    OrphanedElseCorrelatedError {
        #[label]
        span: Span,
    },
    #[error("measurement record is out of bounds")]
    #[diagnostic(code("Qdk.Stim.Compiler.MeasurementRecordOutOfBounds"))]
    MeasurementRecordOutOfBounds {
        #[label]
        span: Span,
    },
    #[error("measurement record refers to a measurement outside the enclosing SELECT block")]
    #[diagnostic(code("Qdk.Stim.Compiler.MeasurementRecordOutOfScope"))]
    MeasurementRecordOutOfScope {
        #[label]
        span: Span,
    },
    #[error("require must appear inside a SELECT block")]
    #[diagnostic(code("Qdk.Stim.Compiler.RequireOutsideSelectBlock"))]
    RequireOutsideSelectBlock {
        #[label]
        span: Span,
    },
    #[error("select instruction must start a block")]
    #[diagnostic(code("Qdk.Stim.Compiler.SelectWithoutBlock"))]
    SelectWithoutBlock {
        #[label]
        span: Span,
    },
}

// This enum keeps track of which side of a controlled operation the measurement record is allowed to appear on.
// For example, in `CX rec[-1] 0', the measurement record comes on the first side, while in 'XCZ 0 rec[-1]' it's the opposite.
#[derive(Clone, Copy)]
enum AllowedRecPosition {
    First,
    Second,
    Either,
}

impl AllowedRecPosition {
    fn allows_first(self) -> bool {
        matches!(self, AllowedRecPosition::First | AllowedRecPosition::Either)
    }
    fn allows_second(self) -> bool {
        matches!(
            self,
            AllowedRecPosition::Second | AllowedRecPosition::Either
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    TopLevel,
    Select(u32), // The u32 is a unique id for the select scope
}

struct IdMap {
    qubit_map: FxHashMap<u32, u32>,
    record_scopes: Vec<Scope>,                   // indexed by result id
    scope_parents: Vec<Scope>,                   // indexed by select scope id, value = parent scope
    scope_stack: Vec<Scope>, // active nested scopes; last() = current, empty = top level
    name_counters: FxHashMap<&'static str, u32>, // prefix -> next index
}

impl IdMap {
    fn new() -> Self {
        Self {
            qubit_map: FxHashMap::default(),
            record_scopes: Vec::new(),
            scope_parents: Vec::new(),
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

    fn enter_select_scope(&mut self) {
        let parent = self.current_scope();
        let id = self.scope_parents.len() as u32;
        self.scope_parents.push(parent);
        self.scope_stack.push(Scope::Select(id));
    }

    fn exit_select_scope(&mut self) {
        self.scope_stack.pop();
    }

    fn current_scope(&self) -> Scope {
        self.scope_stack.last().copied().unwrap_or(Scope::TopLevel)
    }

    fn scope_of_record(&self, id: u32) -> Scope {
        match self.record_scopes.get(id as usize) {
            Some(&scope) => scope,
            None => unreachable!("record id not found"), // this is a compiler invariant
        }
    }

    fn parent_of(&self, scope: Scope) -> Scope {
        let Scope::Select(id) = scope else {
            return Scope::TopLevel;
        };
        self.scope_parents
            .get(id as usize)
            .copied()
            .expect("cannot get the parent of a scope that has not been entered")
    }

    fn is_descendant_or_equal(&self, scope: Scope, ancestor: Scope) -> bool {
        if scope == ancestor {
            return true;
        }
        match scope {
            Scope::Select(_) => self.is_descendant_or_equal(self.parent_of(scope), ancestor),
            Scope::TopLevel => false,
        }
    }

    fn record_in_scope(&self, record_id: u32) -> bool {
        self.is_descendant_or_equal(self.scope_of_record(record_id), self.current_scope())
    }

    fn allocate_record(&mut self) -> u32 {
        let id = self.record_scopes.len() as u32;
        let scope = self.current_scope();
        self.record_scopes.push(scope);
        id
    }

    fn allocate_qubit(&mut self, stim_index: u32) -> u32 {
        let next_id = self.qubit_map.len() as u32;
        *self.qubit_map.entry(stim_index).or_insert(next_id)
    }

    fn num_results(&self) -> u32 {
        self.record_scopes.len() as u32
    }

    fn num_qubits(&self) -> u32 {
        self.qubit_map.len() as u32
    }
}

fn select_label(scope: u32) -> String {
    format!("select_{scope}")
}

struct CorrelatedRow {
    terms: Vec<(FaultChar, u32)>,
    probability: f64,
}

#[derive(PartialEq, Eq, Hash)]
struct NoiseKey {
    qubits: u32,
    pauli_strings: Vec<u64>,
    probability_bits: Vec<u64>,
    on_loss: u32,
}

impl NoiseKey {
    fn from_table(table: &NoiseTable<f64>) -> Self {
        Self {
            qubits: table.qubits,
            pauli_strings: table.pauli_strings.clone(),
            probability_bits: table.probabilities.iter().map(|p| p.to_bits()).collect(),
            on_loss: table.on_loss.as_u32(),
        }
    }
}

struct NoiseAccumulator<'noise> {
    config: &'noise mut NoiseConfig<f64, f64>,
    intrinsic_ids: FxHashMap<NoiseKey, u32>,
    current_correlated_group: Option<Vec<CorrelatedRow>>,
}

impl<'noise> NoiseAccumulator<'noise> {
    fn new(config: &'noise mut NoiseConfig<f64, f64>) -> Self {
        Self {
            config,
            intrinsic_ids: FxHashMap::default(),
            current_correlated_group: None,
        }
    }

    fn get_or_insert_intrinsic(&mut self, noise_table: NoiseTable<f64>) -> String {
        let key = NoiseKey::from_table(&noise_table);
        let Some(id) = self.intrinsic_ids.get(&key) else {
            let next_id = self.config.intrinsics.len() as u32;
            self.intrinsic_ids.insert(key, next_id);
            self.config.intrinsics.insert(next_id, noise_table);
            return format!("noise_intrinsic_{next_id}");
        };
        format!("noise_intrinsic_{id}")
    }

    fn push_correlated_row(&mut self, row: CorrelatedRow) {
        self.current_correlated_group
            .get_or_insert_with(Vec::new)
            .push(row)
    }

    fn flush_correlated_group(&mut self) -> Option<(NoiseTable<f64>, Vec<u32>)> {
        let rows = self.current_correlated_group.take()?;
        let qubits = self.collect_qubits(&rows);
        let pauli_string_width = qubits.len();
        let column_of_qubit = self.build_column_map(&qubits);

        let mut pauli_strings = Vec::new();
        let mut probabilities = Vec::new();
        let mut remaining_probability = 1.0;
        for row in rows {
            let mut pauli_string_chars = vec!["I"; pauli_string_width];
            for (fault, qubit) in row.terms {
                pauli_string_chars[column_of_qubit[&qubit]] = fault.as_str();
            }
            pauli_strings.push(encode_pauli(&pauli_string_chars.concat()));
            probabilities.push(remaining_probability * row.probability); // each row fires only if all previous ones didn't
            remaining_probability *= 1.0 - row.probability;
        }
        Some((
            NoiseTable {
                qubits: pauli_string_width as u32,
                pauli_strings,
                probabilities,
                on_loss: LossPolicy::Skip, // required field; Skip is the default policy
            },
            qubits,
        ))
    }

    fn collect_qubits(&self, rows: &[CorrelatedRow]) -> Vec<u32> {
        let mut qubits: Vec<u32> = rows
            .iter()
            .flat_map(|row| row.terms.iter().map(|(_, qubit)| *qubit))
            .collect();
        qubits.sort_unstable();
        qubits.dedup();
        qubits
    }

    fn build_column_map(&self, qubits: &[u32]) -> FxHashMap<u32, usize> {
        qubits
            .iter()
            .enumerate()
            .map(|(column, &qubit)| (qubit, column))
            .collect()
    }
}

struct Compiler<'noise> {
    writer: QirWriter,
    errors: Vec<Error>,
    id_map: IdMap,
    noise_accumulator: NoiseAccumulator<'noise>,
}

impl<'noise> Compiler<'noise> {
    fn new(noise: &'noise mut NoiseConfig<f64, f64>) -> Self {
        Self {
            writer: QirWriter::new(),
            errors: Vec::new(),
            id_map: IdMap::new(),
            noise_accumulator: NoiseAccumulator::new(noise),
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

        self.id_map.enter_select_scope();
        self.compile_instruction(block_instruction);
        for item in items {
            self.compile_item(item);
        }
        self.id_map.exit_select_scope();
    }

    fn compile_line(&mut self, line: &Line) {
        let Line { instruction, .. } = line;
        self.compile_instruction(instruction);
    }

    fn compile_instruction(&mut self, instruction: &Instruction) {
        if self.noise_accumulator.current_correlated_group.is_some()
            && instruction.name != "ELSE_CORRELATED_ERROR"
        {
            self.finish_correlated_noise();
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
            "CX" | "CNOT" | "ZCX" => self.broadcast_controlled(
                instruction,
                AllowedRecPosition::First,
                |s, q0, q1| {
                    s.op_2("cx", q0, q1);
                },
                "x",
            ),
            "CXSWAP" => self.broadcast_pair(instruction, |s, q0, q1| {
                // Stim decomposition (into H, S, CX, M, R): CX 1 0; CX 0 1
                s.op_2("cx", q1, q0);
                s.op_2("cx", q0, q1);
            }),
            "CY" | "ZCY" => self.broadcast_controlled(
                instruction,
                AllowedRecPosition::First,
                |s, q0, q1| {
                    s.op_2("cy", q0, q1);
                },
                "y",
            ),
            "CZ" | "ZCZ" => self.broadcast_controlled(
                instruction,
                AllowedRecPosition::Either,
                |s, q0, q1| {
                    s.op_2("cz", q0, q1);
                },
                "z",
            ),
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
            "XCZ" => self.broadcast_controlled(
                instruction,
                AllowedRecPosition::Second,
                |s, q0, q1| {
                    // Stim decomposition (into H, S, CX, M, R): CX 1 0
                    s.op_2("cx", q1, q0);
                },
                "x",
            ),
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
            "YCZ" => self.broadcast_controlled(
                instruction,
                AllowedRecPosition::Second,
                |s, q0, q1| {
                    // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; CX 1 0; S 0
                    s.op_adj("s", q0);
                    s.op_2("cx", q1, q0);
                    s.op("s", q0);
                },
                "y",
            ),

            // Noise Channels
            "E" | "CORRELATED_ERROR" => self.accumulate_correlated_noise(instruction),
            "ELSE_CORRELATED_ERROR" => self.continue_correlated_noise(instruction),

            "DEPOLARIZE1" => self.broadcast_noise(instruction, |s, q, p| {
                let table = NoiseTable {
                    qubits: 1,
                    pauli_strings: ["X", "Y", "Z"].map(encode_pauli).to_vec(),
                    probabilities: vec![p / 3.0; 3],
                    on_loss: LossPolicy::Skip, // required field; Skip is the default policy
                };
                s.op_noise(table, &[q]);
            }),
            "DEPOLARIZE2" => self.broadcast_pair_noise(instruction, |s, q0, q1, p| {
                let table = NoiseTable {
                    qubits: 2,
                    pauli_strings: [
                        "IX", "IY", "IZ", "XI", "XX", "XY", "XZ", "YI", "YX", "YY", "YZ", "ZI",
                        "ZX", "ZY", "ZZ",
                    ]
                    .map(encode_pauli)
                    .to_vec(),
                    probabilities: vec![p / 15.0; 15],
                    on_loss: LossPolicy::Skip, // required field; Skip is the default policy
                };
                s.op_noise(table, &[q0, q1]);
            }),
            "HERALDED_ERASE" | "HERALDED_PAULI_CHANNEL_1" => self.unsupported(instruction),
            "II_ERROR" | "I_ERROR" => (),
            "PAULI_CHANNEL_1" | "PAULI_CHANNEL_2" => self.unsupported(instruction),
            "X_ERROR" | "Y_ERROR" | "Z_ERROR" | "LOSS_ERROR" => {
                let fault = FaultChar::from_instruction_name(&instruction.name);
                self.broadcast_noise(instruction, |s, q, p| {
                    let table = NoiseTable {
                        qubits: 1,
                        pauli_strings: vec![encode_pauli(fault.as_str())],
                        probabilities: vec![p],
                        on_loss: LossPolicy::Skip, // required field; Skip is the default policy
                    };
                    s.op_noise(table, &[q]);
                });
            }

            // Collapsing Gates
            "M" | "MZ" => self.broadcast_measure(instruction, |s, q, invert| {
                s.op_measure("m", q, invert);
            }),
            "MR" | "MRZ" => self.broadcast_measure(instruction, |s, q, invert| {
                s.op_measure_reset("mresetz", q, invert);
            }),
            "MRX" => self.broadcast_measure(instruction, |s, q, invert| {
                // Stim decomposition (into H, S, CX, M, R): H 0; M 0; R 0; H 0
                s.op("h", q); // X -> Z
                s.op_measure_reset("mresetz", q, invert); // MRZ
                s.op("h", q); // Z -> X
            }),
            "MRY" => self.broadcast_measure(instruction, |s, q, invert| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; H 0; M 0; R 0; H 0; S 0
                s.op_adj("s", q); // Y -> X
                s.op("h", q); // X -> Z
                s.op_measure_reset("mresetz", q, invert); // MRZ
                s.op("h", q); // Z -> X
                s.op("s", q); // X -> Y
            }),
            "MX" => self.broadcast_measure(instruction, |s, q, invert| {
                // Stim decomposition (into H, S, CX, M, R): H 0; M 0; H 0
                s.op("h", q); // X -> Z
                s.op_measure("m", q, invert); // MZ
                s.op("h", q); // Z -> X
            }),
            "MY" => self.broadcast_measure(instruction, |s, q, invert| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; H 0; M 0; H 0; S 0
                s.op_adj("s", q); // Y -> X
                s.op("h", q); // X -> Z
                s.op_measure("m", q, invert); // MZ
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
            "MXX" => self.broadcast_pair_measure(instruction, |s, q0, q1, invert| {
                // Stim decomposition (into H, S, CX, M, R): CX 0 1; H 0; M 0; H 0; CX 0 1
                s.op_2("cx", q0, q1);
                s.op("h", q0);
                s.op_measure("m", q0, invert);
                s.op("h", q0);
                s.op_2("cx", q0, q1);
            }),
            "MYY" => self.broadcast_pair_measure(instruction, |s, q0, q1, invert| {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 1; CX 0 1; H 0; M 0; S 1; S 1; H 0; CX 0 1; S 0; S 1
                s.op("s", q0);
                s.op("s", q1);
                s.op_2("cx", q0, q1);
                s.op("h", q0);
                s.op_measure("m", q0, invert);
                s.op("z", q1);
                s.op("h", q0);
                s.op_2("cx", q0, q1);
                s.op("s", q0);
                s.op("s", q1);
            }),
            "MZZ" => self.broadcast_pair_measure(instruction, |s, q0, q1, invert| {
                // Stim decomposition (into H, S, CX, M, R): CX 0 1; M 1; CX 0 1
                s.op_2("cx", q0, q1);
                s.op_measure("m", q1, invert);
                s.op_2("cx", q0, q1);
            }),

            // Generalized Pauli Product Gates
            "MPP" | "SPP" | "SPP_DAG" => self.unsupported(instruction),

            // Control Flow
            "REPEAT" => self.unsupported(instruction),
            "SELECT" => self.compile_select(instruction),
            "REQUIRE" => self.compile_require(instruction),

            // Annotations
            "DETECTOR" | "MPAD" | "OBSERVABLE_INCLUDE" | "QUBIT_COORDS" | "SHIFT_COORDS"
            | "TICK" => (),

            _ => self.unknown(instruction),
        }
    }

    fn for_each_qubit(&mut self, instruction: &Instruction, mut f: impl FnMut(&mut Self, u32)) {
        for target in &instruction.targets {
            let Some((q, _)) = self.expect_qubit(instruction, target, false) else {
                continue;
            };
            f(self, q);
        }
    }

    fn for_each_negated_qubit(
        &mut self,
        instruction: &Instruction,
        mut f: impl FnMut(&mut Self, u32, bool),
    ) {
        for target in &instruction.targets {
            let Some((q, negated)) = self.expect_qubit(instruction, target, true) else {
                continue;
            };
            f(self, q, negated);
        }
    }

    fn broadcast(&mut self, instruction: &Instruction, f: impl FnMut(&mut Self, u32)) {
        self.unsupported_args(instruction);
        self.for_each_qubit(instruction, f);
    }

    fn broadcast_measure(
        &mut self,
        instruction: &Instruction,
        f: impl FnMut(&mut Self, u32, bool),
    ) {
        self.unsupported_args(instruction);
        self.for_each_negated_qubit(instruction, f);
    }

    fn broadcast_noise(
        &mut self,
        instruction: &Instruction,
        mut f: impl FnMut(&mut Self, u32, f64),
    ) {
        let Some(probability) = self.expect_probability(instruction) else {
            return;
        };
        self.for_each_qubit(instruction, |s, q| f(s, q, probability));
    }

    fn accumulate_correlated_noise(&mut self, instruction: &Instruction) {
        let Some(probability) = self.expect_probability(instruction) else {
            return;
        };
        let mut terms = Vec::with_capacity(instruction.targets.len());

        for target in &instruction.targets {
            let Some((fault, qubit)) = self.expect_fault_char(instruction, target) else {
                continue;
            };

            terms.push((fault, qubit));
        }

        let row = CorrelatedRow { probability, terms };

        self.noise_accumulator.push_correlated_row(row);
    }

    fn continue_correlated_noise(&mut self, instruction: &Instruction) {
        if self.noise_accumulator.current_correlated_group.is_none() {
            self.push_error(Error::OrphanedElseCorrelatedError {
                span: instruction.span,
            });
            return;
        }
        self.accumulate_correlated_noise(instruction);
    }

    fn finish_correlated_noise(&mut self) {
        let Some((noise_table, qubits)) = self.noise_accumulator.flush_correlated_group() else {
            return;
        };

        self.op_noise(noise_table, &qubits);
    }

    fn for_each_pair(&mut self, instruction: &Instruction, mut f: impl FnMut(&mut Self, u32, u32)) {
        let Some(pairs) = self.expect_target_pairs(instruction) else {
            return;
        };
        for pair in pairs {
            let Some((q0, _)) = self.expect_qubit(instruction, &pair[0], false) else {
                continue;
            };
            let Some((q1, _)) = self.expect_qubit(instruction, &pair[1], false) else {
                continue;
            };
            f(self, q0, q1);
        }
    }

    fn for_each_negated_pair(
        &mut self,
        instruction: &Instruction,
        mut f: impl FnMut(&mut Self, u32, u32, bool),
    ) {
        let Some(pairs) = self.expect_target_pairs(instruction) else {
            return;
        };
        for pair in pairs {
            let Some((q0, neg0)) = self.expect_qubit(instruction, &pair[0], true) else {
                continue;
            };
            let Some((q1, neg1)) = self.expect_qubit(instruction, &pair[1], true) else {
                continue;
            };
            f(self, q0, q1, neg0 ^ neg1);
        }
    }

    fn broadcast_pair(&mut self, instruction: &Instruction, f: impl FnMut(&mut Self, u32, u32)) {
        self.unsupported_args(instruction);
        self.for_each_pair(instruction, f);
    }

    fn broadcast_pair_measure(
        &mut self,
        instruction: &Instruction,
        f: impl FnMut(&mut Self, u32, u32, bool),
    ) {
        self.unsupported_args(instruction);
        self.for_each_negated_pair(instruction, f);
    }

    fn broadcast_pair_noise(
        &mut self,
        instruction: &Instruction,
        mut f: impl FnMut(&mut Self, u32, u32, f64),
    ) {
        let Some(probability) = self.expect_probability(instruction) else {
            return;
        };
        self.for_each_pair(instruction, |s, q0, q1| f(s, q0, q1, probability));
    }

    fn broadcast_controlled(
        &mut self,
        instruction: &Instruction,
        allowed_rec_position: AllowedRecPosition,
        mut quantum: impl FnMut(&mut Self, u32, u32),
        classical_pauli: &str,
    ) {
        self.unsupported_args(instruction);
        let Some(pairs) = self.expect_target_pairs(instruction) else {
            return;
        };
        for pair in pairs {
            match (&pair[0].kind, &pair[1].kind) {
                (TargetKind::Qubit { .. }, TargetKind::Qubit { .. }) => {
                    let Some((control, _)) = self.expect_qubit(instruction, &pair[0], false) else {
                        continue;
                    };
                    let Some((target, _)) = self.expect_qubit(instruction, &pair[1], false) else {
                        continue;
                    };
                    quantum(self, control, target);
                }
                (TargetKind::MeasurementRecord { .. }, TargetKind::Qubit { .. })
                    if allowed_rec_position.allows_first() =>
                {
                    self.classical_control(instruction, &pair[0], &pair[1], classical_pauli);
                }
                (TargetKind::Qubit { .. }, TargetKind::MeasurementRecord { .. })
                    if allowed_rec_position.allows_second() =>
                {
                    self.classical_control(instruction, &pair[1], &pair[0], classical_pauli);
                }
                (TargetKind::MeasurementRecord { .. }, TargetKind::MeasurementRecord { .. }) => {
                    self.push_error(Error::MeasurementRecordWithoutQubit {
                        instruction: instruction.name.clone(),
                        span: Span {
                            lo: pair[0].span.lo,
                            hi: pair[1].span.hi,
                        },
                    });
                }
                // A `rec` that reached here sits on a side this gate doesn't allow
                (TargetKind::MeasurementRecord { .. }, _) => {
                    self.push_error(Error::MisplacedMeasurementRecord {
                        instruction: instruction.name.clone(),
                        span: pair[0].span,
                    });
                }
                (_, TargetKind::MeasurementRecord { .. }) => {
                    self.push_error(Error::MisplacedMeasurementRecord {
                        instruction: instruction.name.clone(),
                        span: pair[1].span,
                    });
                }
                _ => self.push_error(Error::UnsupportedTarget {
                    instruction: instruction.name.clone(),
                    span: pair[0].span,
                }),
            }
        }
    }

    fn classical_control(
        &mut self,
        instruction: &Instruction,
        rec_target: &Target,
        qubit_target: &Target,
        pauli: &str,
    ) {
        let Some((offset, negated)) = self.expect_measurement_record(instruction, rec_target)
        else {
            return;
        };
        if negated {
            self.push_error(Error::NegatedTarget {
                instruction: instruction.name.clone(),
                span: rec_target.span,
            });
            return;
        }
        let Some(result_id) = self.resolve_record_offset(rec_target, offset) else {
            return;
        };
        let Some((target, _)) = self.expect_qubit(instruction, qubit_target, false) else {
            return;
        };
        let qubit = self.id_map.allocate_qubit(target);
        self.writer.write_classical_control(pauli, result_id, qubit);
    }

    fn op(&mut self, intrinsic: &str, qubit: u32) {
        let q = self.id_map.allocate_qubit(qubit);
        self.writer.write_qis_call(intrinsic, &[q]);
    }

    fn op_adj(&mut self, intrinsic: &str, qubit: u32) {
        let q = self.id_map.allocate_qubit(qubit);
        self.writer.write_qis_adj_call(intrinsic, &[q]);
    }

    fn op_measure(&mut self, intrinsic: &str, qubit: u32, invert: bool) {
        let q = self.id_map.allocate_qubit(qubit);
        if invert {
            self.writer.write_qis_call("x", &[q]);
        }
        let r = self.id_map.allocate_record();
        self.writer.write_qis_call(intrinsic, &[q, r]);
        if invert {
            self.writer.write_qis_call("x", &[q]);
        }
    }

    fn op_measure_reset(&mut self, intrinsic: &str, qubit: u32, invert: bool) {
        let q = self.id_map.allocate_qubit(qubit);
        if invert {
            self.writer.write_qis_call("x", &[q]);
        }
        let r = self.id_map.allocate_record();
        self.writer.write_qis_call(intrinsic, &[q, r]);
    }

    fn op_2(&mut self, intrinsic: &str, q0: u32, q1: u32) {
        let q0 = self.id_map.allocate_qubit(q0);
        let q1 = self.id_map.allocate_qubit(q1);
        self.writer.write_qis_call(intrinsic, &[q0, q1]);
    }

    fn op_noise(&mut self, table: NoiseTable<f64>, qubits: &[u32]) {
        let ids: Vec<u32> = qubits
            .iter()
            .map(|&qubit| self.id_map.allocate_qubit(qubit))
            .collect();
        let name = self.noise_accumulator.get_or_insert_intrinsic(table);
        self.writer.write_noise_call(&name, &ids);
    }

    fn compile_select(&mut self, instruction: &Instruction) {
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

        let Scope::Select(scope) = self.id_map.current_scope() else {
            self.push_error(Error::SelectWithoutBlock {
                span: instruction.span,
            });
            return;
        };

        let label = select_label(scope);
        self.writer.write_jump(&label); // terminate the previous block
        self.writer.write_label(&label); // start the new block
    }

    fn compile_require(&mut self, instruction: &Instruction) {
        if matches!(self.id_map.current_scope(), Scope::TopLevel) {
            self.push_error(Error::RequireOutsideSelectBlock {
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

        let mut loss_registers = Vec::new();
        let mut result_registers = Vec::new();
        for target in &instruction.targets {
            let Some((offset, negated)) = self.expect_measurement_record(instruction, target)
            else {
                return;
            };
            let Some(result_id) = self.resolve_record_offset(target, offset) else {
                return;
            };

            loss_registers.push(self.read_loss_register(result_id));
            result_registers.push(self.read_result_register(result_id, negated));
        }

        let loss = self.reduce_registers(&loss_registers, "loss", QirWriter::write_or);
        let parity = self.reduce_registers(&result_registers, "parity", QirWriter::write_xor);

        let restart = self.id_map.fresh_name("restart");
        self.writer.write_or(&restart, &loss, &parity);

        let Scope::Select(scope) = self.id_map.current_scope() else {
            unreachable!("REQUIRE runs inside a select block");
        };
        let restart_label = select_label(scope);
        let continue_label = self.id_map.fresh_name("continue");
        self.writer
            .write_branch(&restart, &restart_label, &continue_label);
        self.writer.write_label(&continue_label);
    }

    fn read_loss_register(&mut self, result_id: u32) -> String {
        let loss_register = self.id_map.fresh_name("l");
        self.writer
            .write_read(&loss_register, "__quantum__rt__read_loss", result_id);
        loss_register
    }

    fn read_result_register(&mut self, result_id: u32, negated: bool) -> String {
        let result_register = self.id_map.fresh_name("r");
        self.writer
            .write_read(&result_register, "__quantum__rt__read_result", result_id);

        if negated {
            let not_register = self.id_map.fresh_name("n");
            self.writer.write_not(&not_register, &result_register);
            not_register
        } else {
            result_register
        }
    }

    fn reduce_registers(
        &mut self,
        registers: &[String],
        prefix: &'static str,
        combine: fn(&mut QirWriter, &str, &str, &str),
    ) -> String {
        let (first, rest) = registers
            .split_first()
            .expect("REQUIRE always has at least one target");
        let mut acc = first.clone();
        for reg in rest {
            let temp = self.id_map.fresh_name(prefix);
            combine(&mut self.writer, &temp, &acc, reg);
            acc = temp;
        }
        acc
    }

    fn resolve_record_offset(&mut self, target: &Target, offset: u32) -> Option<u32> {
        let num_results = self.id_map.num_results();
        let Some(result_id) = num_results.checked_sub(offset) else {
            self.push_error(Error::MeasurementRecordOutOfBounds { span: target.span });
            return None;
        };

        if !self.id_map.record_in_scope(result_id) {
            self.push_error(Error::MeasurementRecordOutOfScope { span: target.span });
            return None;
        }
        Some(result_id)
    }

    fn expect_qubit(
        &mut self,
        instruction: &Instruction,
        target: &Target,
        allow_negated: bool,
    ) -> Option<(u32, bool)> {
        let TargetKind::Qubit { value, negated } = target.kind else {
            self.push_error(Error::UnsupportedTarget {
                instruction: instruction.name.clone(),
                span: target.span,
            });
            return None;
        };

        if negated && !allow_negated {
            self.push_error(Error::NegatedTarget {
                instruction: instruction.name.clone(),
                span: target.span,
            });
            return None;
        }
        Some((value, negated))
    }

    fn expect_fault_char(
        &mut self,
        instruction: &Instruction,
        target: &Target,
    ) -> Option<(FaultChar, u32)> {
        match target.kind {
            TargetKind::Loss { value } => Some((FaultChar::Loss, value)),
            TargetKind::Pauli {
                pauli,
                value,
                negated,
            } => {
                if negated {
                    self.push_error(Error::NegatedTarget {
                        instruction: instruction.name.clone(),
                        span: target.span,
                    });
                    return None;
                }
                Some((FaultChar::from_pauli(pauli), value))
            }
            _ => {
                self.push_error(Error::UnsupportedTarget {
                    instruction: instruction.name.clone(),
                    span: target.span,
                });
                None
            }
        }
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

    fn expect_target_pairs<'a>(
        &mut self,
        instruction: &'a Instruction,
    ) -> Option<Chunks<'a, Target>> {
        if !instruction.targets.len().is_multiple_of(2) {
            self.push_error(Error::OddTargetCount {
                instruction: instruction.name.clone(),
                span: instruction.span,
            });
            return None;
        }
        Some(instruction.targets.chunks(2))
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
        self.finish_correlated_noise();
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
