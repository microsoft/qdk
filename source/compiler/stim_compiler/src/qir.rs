// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use qdk_simulators::noise_config::{
    LossPolicy, NoiseConfig, NoiseTable, PauliAndLossString, encode_pauli,
};

use crate::parser::Pauli;
use crate::semantic;
use Pauli::{X, Y, Z};
use miette::Diagnostic;
use qsc_data_structures::span::Span;
use rustc_hash::{FxHashMap, FxHashSet};
use std::f64::consts::PI;
use std::fmt::Write;
use thiserror::Error;

type StimQubitId = u32;
type QubitId = u32;
type ResultId = u32;

type Radians = f64;

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
    fn write_qis_call(&mut self, intrinsic: &str, qubits: &[QubitId]) {
        self.call_intrinsic(&format!("__quantum__qis__{intrinsic}__body"), qubits);
    }

    /// `__quantum__qis__{intrinsic}__adj`
    fn write_qis_adj_call(&mut self, intrinsic: &str, qubits: &[QubitId]) {
        self.call_intrinsic(&format!("__quantum__qis__{intrinsic}__adj"), qubits);
    }

    fn call_intrinsic(&mut self, intrinsic: &str, qubits: &[QubitId]) {
        self.write_call(intrinsic, qubits);
        self.declare(intrinsic, || {
            let params = vec!["ptr"; qubits.len()].join(", ");
            format!("declare void @{intrinsic}({params})")
        });
    }

    fn write_measure_call(&mut self, intrinsic: &str, qubit: QubitId, result: ResultId) {
        let name = format!("__quantum__qis__{intrinsic}__body");
        self.write_call(&name, &[qubit, result]);
        self.declare(&name, || format!("declare void @{name}(ptr, ptr)"));
    }

    fn write_readout_noise_call(&mut self, probability: f64, result_id: ResultId) {
        let name = "__quantum__rt__readout_noise";
        writeln!(
            self,
            "  call void @{name}(double {probability:?}, double {probability:?}, ptr inttoptr (i64 {result_id} to ptr))"
        );

        self.declare(name, || {
            format!("declare void @{name}(double, double, ptr) #2")
        });
        self.has_noise_intrinsic = true;
    }

    /// `noise_intrinsic_{id}`
    fn write_noise_call(&mut self, name: &str, qubits: &[QubitId]) {
        self.call_noise_intrinsic(name, qubits);
    }

    // Writes: `  call void @{name}(double {angle:?}, ptr inttoptr (i64 N to ptr), ...)`
    fn write_rotation_call(&mut self, intrinsic: &str, angle: Radians, qubits: &[QubitId]) {
        let name = format!("__quantum__qis__{intrinsic}__body");
        write!(self, "  call void @{name}(double {angle:?}");
        for &qubit in qubits {
            write!(self, ", ");
            self.write_ptr(qubit);
        }
        writeln!(self, ")");
        self.declare(&name, || {
            let params = vec!["ptr"; qubits.len()].join(", ");
            format!("declare void @{name}(double, {params})")
        });
    }

    fn call_noise_intrinsic(&mut self, intrinsic: &str, qubits: &[QubitId]) {
        self.write_call(intrinsic, qubits);
        let attribute = " #2";
        self.declare(intrinsic, || {
            let params = vec!["ptr"; qubits.len()].join(", ");
            format!("declare void @{intrinsic}({params}){attribute}")
        });
        self.has_noise_intrinsic = true;
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

    fn write_classical_control(&mut self, pauli: &str, result_id: ResultId, qubit: QubitId) {
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
    fn write_read(&mut self, dest: &str, intrinsic: &str, result_id: ResultId) {
        write!(self, "  %{dest} = call i1 @{intrinsic}(");
        self.write_ptr(result_id);
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
        let mut decls: Vec<String> = self.used_intrinsics.values().cloned().collect();
        decls.sort_unstable();
        for decl in decls {
            writeln!(self, "{decl}");
        }
    }

    fn write_definitions(&mut self) {
        let mut definitions: Vec<String> = self.defined_functions.values().cloned().collect();
        definitions.sort_unstable();
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
        if self.has_noise_intrinsic {
            writeln!(self, "attributes #2 = {{ \"qdk_noise\" }}");
        }
        writeln!(self);
        writeln!(self, "; module flags");
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

#[derive(Clone, Debug, Error, Diagnostic)]
pub enum Error {
    #[error("unsupported instruction: {name}")]
    #[diagnostic(code("Qdk.Stim.Compiler.UnsupportedInstruction"))]
    UnsupportedInstruction {
        name: String,
        #[label]
        span: Span,
    },
    #[error("{instruction} must appear inside a SELECT block")]
    #[diagnostic(code("Qdk.Stim.Compiler.InstructionOutsideSelectBlock"))]
    InstructionOutsideSelectBlock {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("NOTLEAKED cannot reference a record produced by PEEK_LOSS")]
    #[diagnostic(code("Qdk.Stim.Compiler.NotLeakedOnPeekLoss"))]
    NotLeakedOnPeekLoss {
        #[label]
        span: Span,
    },
    #[error("all measurement records referenced by {instruction} are out of scope")]
    #[diagnostic(code("Qdk.Stim.Compiler.AllMeasurementRecordsOutOfScope"))]
    AllMeasurementRecordsOutOfScope {
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
}

#[derive(Clone, Copy)]
enum Scope {
    TopLevel,
    Select { id: u32, first_record: ResultId },
}

struct IdMap {
    qubit_map: FxHashMap<StimQubitId, QubitId>,
    name_counters: FxHashMap<&'static str, u32>, // prefix -> next index
    record_count: u32,                           // number of allocated measurement records
    peek_loss_record_ids: FxHashSet<u32>,        // record ids produced by PEEK_LOSS
    scope_stack: Vec<Scope>, // active nested scopes; last() = current, empty = top level
    next_scope_id: u32,      // used to generate unique ids for scopes
}

impl IdMap {
    fn new() -> Self {
        Self {
            qubit_map: FxHashMap::default(),
            name_counters: FxHashMap::default(),
            record_count: 0,
            peek_loss_record_ids: FxHashSet::default(),
            scope_stack: Vec::new(),
            next_scope_id: 0,
        }
    }

    fn fresh_name(&mut self, prefix: &'static str) -> String {
        let counter = self.name_counters.entry(prefix).or_insert(0);
        let id = *counter;
        *counter += 1;
        format!("{prefix}_{id}")
    }

    fn enter_select_scope(&mut self) {
        let id = self.next_scope_id;
        self.scope_stack.push(Scope::Select {
            id,
            first_record: self.record_count,
        });
        self.next_scope_id += 1;
    }

    fn exit_select_scope(&mut self) {
        self.scope_stack.pop();
    }

    fn current_scope(&self) -> Scope {
        self.scope_stack.last().copied().unwrap_or(Scope::TopLevel)
    }

    fn record_in_scope(&self, record_id: ResultId) -> bool {
        let Scope::Select { first_record, .. } = self.current_scope() else {
            return true;
        };
        record_id >= first_record
    }

    fn allocate_record(&mut self) -> ResultId {
        let id = self.record_count;
        self.record_count += 1;
        id
    }

    fn allocate_qubit(&mut self, stim_qubit: StimQubitId) -> QubitId {
        let next_id = self.qubit_map.len() as u32;
        *self.qubit_map.entry(stim_qubit).or_insert(next_id)
    }

    fn num_qubits(&self) -> u32 {
        self.qubit_map.len() as u32
    }
}

fn select_label(scope: u32) -> String {
    format!("select_{scope}")
}

struct CorrelatedRow {
    faults: Vec<semantic::Fault>,
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
            .push(row);
    }

    fn build_noise_table(
        &self,
        num_qubits: u32,
        pauli_strings: Vec<PauliAndLossString>,
        probabilities: Vec<f64>,
    ) -> NoiseTable<f64> {
        NoiseTable {
            qubits: num_qubits,
            pauli_strings,
            probabilities,
            on_loss: LossPolicy::Skip, // required field; Skip is the default policy
        }
    }

    fn flush_correlated_group(&mut self) -> (NoiseTable<f64>, Vec<StimQubitId>) {
        let rows = self
            .current_correlated_group
            .take()
            .expect("a correlated group must be present to flush"); // this is a compiler invariant

        let qubits = self.collect_qubits(&rows);
        let pauli_string_width = qubits.len();
        let column_of_qubit = self.build_column_map(&qubits);

        let mut pauli_strings = Vec::new();
        let mut probabilities = Vec::new();
        let mut remaining_probability = 1.0;
        for row in rows {
            let mut pauli_string_chars = vec!["I"; pauli_string_width];
            for fault in row.faults {
                pauli_string_chars[column_of_qubit[&fault.qubit]] = fault.kind.as_str();
            }
            pauli_strings.push(encode_pauli(&pauli_string_chars.concat()));
            probabilities.push(remaining_probability * row.probability); // each row fires only if all previous ones didn't
            remaining_probability *= 1.0 - row.probability;
        }
        let noise_table =
            self.build_noise_table(pauli_string_width as u32, pauli_strings, probabilities);
        (noise_table, qubits)
    }

    fn collect_qubits(&self, rows: &[CorrelatedRow]) -> Vec<StimQubitId> {
        let mut qubits: Vec<StimQubitId> = rows
            .iter()
            .flat_map(|row| row.faults.iter().map(|fault| fault.qubit))
            .collect();
        qubits.sort_unstable();
        qubits.dedup();
        qubits
    }

    fn build_column_map(&self, qubits: &[StimQubitId]) -> FxHashMap<StimQubitId, usize> {
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

    fn into_qir(mut self, circuit: &semantic::Circuit) -> Result<String, Vec<Error>> {
        self.writer.write_header();
        self.compile_circuit(circuit);
        self.finish_correlated_noise();
        self.writer
            .write_footer(self.id_map.num_qubits(), self.id_map.record_count);
        if self.errors.is_empty() {
            Ok(self.writer.output)
        } else {
            Err(self.errors)
        }
    }

    fn compile_circuit(&mut self, circuit: &semantic::Circuit) {
        for item in &circuit.items {
            self.compile_item(item);
        }
    }

    fn compile_item(&mut self, item: &semantic::Item) {
        match item {
            semantic::Item::Block(block) => self.compile_block(block),
            semantic::Item::Instruction(instruction) => {
                self.compile_instruction(instruction);
            }
        }
    }

    fn compile_block(&mut self, block: &semantic::Block) {
        // Block boundaries interrupt E/ELSE_CORRELATED_ERROR chains.
        self.finish_correlated_noise();
        match block {
            semantic::Block::RepeatBlock { count, body } => {
                let error_count_before_repeat = self.errors.len();
                for _ in 0..*count {
                    for item in body {
                        self.compile_item(item);
                    }
                    self.finish_correlated_noise();

                    // Avoid repeating error reporting
                    if self.errors.len() > error_count_before_repeat {
                        return;
                    }
                }
            }
            semantic::Block::SelectBlock { body } => {
                self.id_map.enter_select_scope();
                let Scope::Select { id: scope_id, .. } = self.id_map.current_scope() else {
                    unreachable!("select scope was just entered");
                };

                let label = select_label(scope_id);
                self.writer.write_jump(&label); // terminate the previous block
                self.writer.write_label(&label); // start the new block
                for item in body {
                    self.compile_item(item);
                }
                self.finish_correlated_noise();
                self.id_map.exit_select_scope();
            }
        }
    }

    fn compile_instruction(&mut self, instruction: &semantic::Instruction) {
        let continues_correlated_error = matches!(
            &instruction.kind,
            semantic::InstructionKind::Noise(semantic::Noise::CorrelatedError {
                kind: semantic::CorrelatedErrorKind::Else,
                ..
            })
        );
        if self.noise_accumulator.current_correlated_group.is_some() && !continues_correlated_error
        {
            self.finish_correlated_noise();
        }

        match &instruction.kind {
            semantic::InstructionKind::Reset { qubit, basis } => {
                self.compile_reset(*qubit, *basis);
            }
            semantic::InstructionKind::SingleQubitGate { qubit, gate } => {
                self.compile_single_qubit_gate(*qubit, *gate);
            }
            semantic::InstructionKind::TwoQubitGate { q0, q1, gate } => {
                self.compile_two_qubit_gate(*q0, *q1, *gate);
            }
            semantic::InstructionKind::ThreeQubitGate { q0, q1, q2, gate } => {
                self.compile_three_qubit_gate(*q0, *q1, *q2, *gate);
            }
            semantic::InstructionKind::PauliProductGate { product, gate } => {
                self.compile_pauli_product_gate(product, *gate);
            }
            semantic::InstructionKind::ClassicallyControlledPauli {
                control,
                target,
                pauli,
            } => self.compile_classical_control(*control, *target, *pauli),
            semantic::InstructionKind::Noise(noise) => {
                self.compile_noise(instruction.span, noise);
            }
            semantic::InstructionKind::SingleQubitMeasurement {
                reset,
                observable,
                readout_noise,
                negated,
                qubit,
            } => self.compile_single_qubit_measurement(
                *reset,
                *observable,
                *readout_noise,
                *negated,
                *qubit,
            ),
            semantic::InstructionKind::TwoQubitMeasurement {
                readout_noise,
                observable,
                negated,
                q0,
                q1,
            } => {
                self.compile_two_qubit_measurement(*readout_noise, *observable, *negated, *q0, *q1)
            }
            semantic::InstructionKind::PauliProductMeasurement {
                readout_noise,
                product,
            } => self.compile_pauli_product_measurement(*readout_noise, product),
            semantic::InstructionKind::PeekLoss {
                readout_noise,
                qubit,
            } => {
                let result_id = self.emit_peek_loss(*qubit);
                self.emit_optional_readout_noise(*readout_noise, result_id);
            }
            semantic::InstructionKind::Require { records } => {
                self.compile_require(instruction.span, records);
            }
            semantic::InstructionKind::NotLeaked { records } => {
                self.compile_not_leaked(instruction.span, records);
            }
            semantic::InstructionKind::Annotation(semantic::Annotation::MeasurementPadding {
                ..
            }) => self.unsupported("MPAD", instruction.span),
            semantic::InstructionKind::Annotation(_) => {}
            semantic::InstructionKind::SingleQubitRotation { axis, angle, qubit } => {
                let intrinsic = match axis {
                    X => "rx",
                    Y => "ry",
                    Z => "rz",
                };
                self.emit_rotation(intrinsic, *angle, *qubit);
            }
            semantic::InstructionKind::TwoQubitRotation {
                axis,
                angle,
                q0,
                q1,
            } => {
                let intrinsic = match axis {
                    semantic::PauliPair::XX => "rxx",
                    semantic::PauliPair::YY => "ryy",
                    semantic::PauliPair::ZZ => "rzz",
                };
                self.emit_two_qubit_rotation(intrinsic, *angle, *q0, *q1);
            }
            semantic::InstructionKind::U3 {
                theta,
                phi,
                lambda,
                qubit,
            } => {
                self.emit_rotation("rz", *lambda, *qubit);
                self.emit_rotation("ry", *theta, *qubit);
                self.emit_rotation("rz", *phi, *qubit);
            }
            semantic::InstructionKind::PauliProductRotation { angle, product } => {
                self.compile_pauli_product_rotation(*angle, product);
            }
        }
    }

    fn compile_reset(&mut self, qubit: StimQubitId, basis: Pauli) {
        self.emit_gate("reset", qubit);
        match basis {
            X => {
                // Stim decomposition (into H, S, CX, M, R): R 0; H 0
                self.emit_gate("h", qubit); // Z -> X
            }
            Y => {
                // Stim decomposition (into H, S, CX, M, R): R 0; H 0; S 0
                self.emit_gate("h", qubit); // Z -> X
                self.emit_gate("s", qubit); // X -> Y
            }
            Z => {}
        }
    }

    fn compile_single_qubit_gate(
        &mut self,
        qubit: StimQubitId,
        gate: semantic::SingleQubitGateKind,
    ) {
        match gate {
            semantic::SingleQubitGateKind::I => {}
            semantic::SingleQubitGateKind::X => self.emit_gate("x", qubit),
            semantic::SingleQubitGateKind::Y => self.emit_gate("y", qubit),
            semantic::SingleQubitGateKind::Z => self.emit_gate("z", qubit),
            semantic::SingleQubitGateKind::C_NXYZ => {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; H 0; S 0; S 0
                self.emit_adjoint_gate("s", qubit);
                self.emit_gate("h", qubit);
                self.emit_gate("z", qubit);
            }
            semantic::SingleQubitGateKind::C_NZYX => {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; H 0; S 0; S 0; S 0
                self.emit_gate("z", qubit);
                self.emit_gate("h", qubit);
                self.emit_adjoint_gate("s", qubit);
            }
            semantic::SingleQubitGateKind::C_XNYZ => {
                // Stim decomposition (into H, S, CX, M, R): S 0; H 0
                self.emit_gate("s", qubit);
                self.emit_gate("h", qubit);
            }
            semantic::SingleQubitGateKind::C_XYNZ => {
                // Stim decomposition (into H, S, CX, M, R): S 0; H 0; S 0; S 0
                self.emit_gate("s", qubit);
                self.emit_gate("h", qubit);
                self.emit_gate("z", qubit);
            }
            semantic::SingleQubitGateKind::C_XYZ => {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; H 0
                self.emit_adjoint_gate("s", qubit);
                self.emit_gate("h", qubit);
            }
            semantic::SingleQubitGateKind::C_ZNYX => {
                // Stim decomposition (into H, S, CX, M, R): H 0; S 0; S 0; S 0
                self.emit_gate("h", qubit);
                self.emit_adjoint_gate("s", qubit);
            }
            semantic::SingleQubitGateKind::C_ZYNX => {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; H 0; S 0
                self.emit_gate("z", qubit);
                self.emit_gate("h", qubit);
                self.emit_gate("s", qubit);
            }
            semantic::SingleQubitGateKind::C_ZYX => {
                // Stim decomposition (into H, S, CX, M, R): H 0; S 0
                self.emit_gate("h", qubit);
                self.emit_gate("s", qubit);
            }
            semantic::SingleQubitGateKind::H => self.emit_gate("h", qubit),
            semantic::SingleQubitGateKind::H_NXY => {
                // Stim decomposition (into H, S, CX, M, R): S 0; H 0; S 0; S 0; H 0
                self.emit_gate("s", qubit);
                self.emit_gate("x", qubit);
            }
            semantic::SingleQubitGateKind::H_NXZ => {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; H 0; S 0; S 0
                self.emit_gate("z", qubit);
                self.emit_gate("h", qubit);
                self.emit_gate("z", qubit);
            }
            semantic::SingleQubitGateKind::H_NYZ => {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; H 0; S 0; H 0
                self.emit_gate("z", qubit);
                self.emit_gate("sx", qubit);
            }
            semantic::SingleQubitGateKind::H_XY => {
                // Stim decomposition (into H, S, CX, M, R): H 0; S 0; S 0; H 0; S 0
                self.emit_gate("x", qubit);
                self.emit_gate("s", qubit);
            }
            semantic::SingleQubitGateKind::H_YZ => {
                // Stim decomposition (into H, S, CX, M, R): H 0; S 0; H 0; S 0; S 0
                self.emit_gate("sx", qubit);
                self.emit_gate("z", qubit);
            }
            semantic::SingleQubitGateKind::S => self.emit_gate("s", qubit),
            semantic::SingleQubitGateKind::SQRT_X => self.emit_gate("sx", qubit),
            semantic::SingleQubitGateKind::SQRT_X_DAG => {
                // Stim decomposition (into H, S, CX, M, R): S 0; H 0; S 0
                self.emit_gate("s", qubit);
                self.emit_gate("h", qubit);
                self.emit_gate("s", qubit);
            }
            semantic::SingleQubitGateKind::SQRT_Y => {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; H 0
                self.emit_gate("z", qubit);
                self.emit_gate("h", qubit);
            }
            semantic::SingleQubitGateKind::SQRT_Y_DAG => {
                // Stim decomposition (into H, S, CX, M, R): H 0; S 0; S 0
                self.emit_gate("h", qubit);
                self.emit_gate("z", qubit);
            }
            semantic::SingleQubitGateKind::S_DAG => self.emit_adjoint_gate("s", qubit),
            semantic::SingleQubitGateKind::T => self.emit_gate("t", qubit),
            semantic::SingleQubitGateKind::T_DAG => self.emit_adjoint_gate("t", qubit),
        }
    }

    fn compile_two_qubit_gate(
        &mut self,
        q0: StimQubitId,
        q1: StimQubitId,
        gate: semantic::TwoQubitGateKind,
    ) {
        match gate {
            semantic::TwoQubitGateKind::CX => self.emit_two_qubit_gate("cx", q0, q1),
            semantic::TwoQubitGateKind::CXSWAP => {
                // Stim decomposition (into H, S, CX, M, R): CX 1 0; CX 0 1
                self.emit_two_qubit_gate("cx", q1, q0);
                self.emit_two_qubit_gate("cx", q0, q1);
            }
            semantic::TwoQubitGateKind::CY => self.emit_two_qubit_gate("cy", q0, q1),
            semantic::TwoQubitGateKind::CZ => self.emit_two_qubit_gate("cz", q0, q1),
            semantic::TwoQubitGateKind::CZSWAP => {
                // Stim decomposition (into H, S, CX, M, R): H 0; CX 0 1; CX 1 0; H 1
                self.emit_gate("h", q0);
                self.emit_two_qubit_gate("cx", q0, q1);
                self.emit_two_qubit_gate("cx", q1, q0);
                self.emit_gate("h", q1);
            }
            semantic::TwoQubitGateKind::II => {}
            semantic::TwoQubitGateKind::ISWAP => {
                // Stim decomposition (into H, S, CX, M, R): H 0; CX 0 1; CX 1 0; H 1; S 1; S 0
                self.emit_gate("h", q0);
                self.emit_two_qubit_gate("cx", q0, q1);
                self.emit_two_qubit_gate("cx", q1, q0);
                self.emit_gate("h", q1);
                self.emit_gate("s", q1);
                self.emit_gate("s", q0);
            }
            semantic::TwoQubitGateKind::ISWAP_DAG => {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; S 1; S 1; S 1; H 1; CX 1 0; CX 0 1; H 0
                self.emit_adjoint_gate("s", q0);
                self.emit_adjoint_gate("s", q1);
                self.emit_gate("h", q1);
                self.emit_two_qubit_gate("cx", q1, q0);
                self.emit_two_qubit_gate("cx", q0, q1);
                self.emit_gate("h", q0);
            }
            semantic::TwoQubitGateKind::SQRT_XX => {
                // Stim decomposition (into H, S, CX, M, R): H 0; CX 0 1; H 1; S 0; S 1; H 0; H 1
                self.emit_gate("h", q0);
                self.emit_two_qubit_gate("cx", q0, q1);
                self.emit_gate("h", q1);
                self.emit_gate("s", q0);
                self.emit_gate("s", q1);
                self.emit_gate("h", q0);
                self.emit_gate("h", q1);
            }
            semantic::TwoQubitGateKind::SQRT_XX_DAG => {
                // Stim decomposition (into H, S, CX, M, R): H 0; CX 0 1; H 1; S 0; S 0; S 0; S 1; S 1; S 1; H 0; H 1
                self.emit_gate("h", q0);
                self.emit_two_qubit_gate("cx", q0, q1);
                self.emit_gate("h", q1);
                self.emit_adjoint_gate("s", q0);
                self.emit_adjoint_gate("s", q1);
                self.emit_gate("h", q0);
                self.emit_gate("h", q1);
            }
            semantic::TwoQubitGateKind::SQRT_YY => {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; S 1; S 1; S 1; H 0; CX 0 1; H 1; S 0; S 1; H 0; H 1; S 0; S 1
                self.emit_adjoint_gate("s", q0); // S 0; S 0; S 0
                self.emit_adjoint_gate("s", q1); // S 1; S 1; S 1
                self.emit_gate("h", q0);
                self.emit_two_qubit_gate("cx", q0, q1);
                self.emit_gate("h", q1);
                self.emit_gate("s", q0);
                self.emit_gate("s", q1);
                self.emit_gate("h", q0);
                self.emit_gate("h", q1);
                self.emit_gate("s", q0);
                self.emit_gate("s", q1);
            }
            semantic::TwoQubitGateKind::SQRT_YY_DAG => {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; S 1; H 0; CX 0 1; H 1; S 0; S 1; H 0; H 1; S 0; S 1; S 1; S 1
                self.emit_adjoint_gate("s", q0); // S 0; S 0; S 0
                self.emit_gate("s", q1); // S 1
                self.emit_gate("h", q0);
                self.emit_two_qubit_gate("cx", q0, q1);
                self.emit_gate("h", q1);
                self.emit_gate("s", q0);
                self.emit_gate("s", q1);
                self.emit_gate("h", q0);
                self.emit_gate("h", q1);
                self.emit_gate("s", q0);
                self.emit_adjoint_gate("s", q1); // S 1; S 1; S 1
            }
            semantic::TwoQubitGateKind::SQRT_ZZ => {
                // Stim decomposition (into H, S, CX, M, R): H 1; CX 0 1; H 1; S 0; S 1
                self.emit_gate("h", q1);
                self.emit_two_qubit_gate("cx", q0, q1);
                self.emit_gate("h", q1);
                self.emit_gate("s", q0);
                self.emit_gate("s", q1);
            }
            semantic::TwoQubitGateKind::SQRT_ZZ_DAG => {
                // Stim decomposition (into H, S, CX, M, R): H 1; CX 0 1; H 1; S 0; S 0; S 0; S 1; S 1; S 1
                self.emit_gate("h", q1);
                self.emit_two_qubit_gate("cx", q0, q1);
                self.emit_gate("h", q1);
                self.emit_adjoint_gate("s", q0);
                self.emit_adjoint_gate("s", q1);
            }
            semantic::TwoQubitGateKind::SWAP => self.emit_two_qubit_gate("swap", q0, q1),
            semantic::TwoQubitGateKind::SWAPCX => {
                // Stim decomposition (into H, S, CX, M, R): CX 0 1; CX 1 0
                self.emit_two_qubit_gate("cx", q0, q1);
                self.emit_two_qubit_gate("cx", q1, q0);
            }
            semantic::TwoQubitGateKind::XCX => {
                // Stim decomposition (into H, S, CX, M, R): H 0; CX 0 1; H 0
                self.emit_gate("h", q0);
                self.emit_two_qubit_gate("cx", q0, q1);
                self.emit_gate("h", q0);
            }
            semantic::TwoQubitGateKind::XCY => {
                // Stim decomposition (into H, S, CX, M, R): H 0; S 1; S 1; S 1; CX 0 1; H 0; S 1
                self.emit_gate("h", q0);
                self.emit_adjoint_gate("s", q1);
                self.emit_two_qubit_gate("cx", q0, q1);
                self.emit_gate("h", q0);
                self.emit_gate("s", q1);
            }
            semantic::TwoQubitGateKind::XCZ => {
                // Stim decomposition (into H, S, CX, M, R): CX 1 0
                self.emit_two_qubit_gate("cx", q1, q0);
            }
            semantic::TwoQubitGateKind::YCX => {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; H 1; CX 1 0; S 0; H 1
                self.emit_adjoint_gate("s", q0);
                self.emit_gate("h", q1);
                self.emit_two_qubit_gate("cx", q1, q0);
                self.emit_gate("s", q0);
                self.emit_gate("h", q1);
            }
            semantic::TwoQubitGateKind::YCY => {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; S 1; S 1; S 1; H 0; CX 0 1; H 0; S 0; S 1
                self.emit_adjoint_gate("s", q0);
                self.emit_adjoint_gate("s", q1);
                self.emit_gate("h", q0);
                self.emit_two_qubit_gate("cx", q0, q1);
                self.emit_gate("h", q0);
                self.emit_gate("s", q0);
                self.emit_gate("s", q1);
            }
            semantic::TwoQubitGateKind::YCZ => {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; CX 1 0; S 0
                self.emit_adjoint_gate("s", q0);
                self.emit_two_qubit_gate("cx", q1, q0);
                self.emit_gate("s", q0);
            }
            semantic::TwoQubitGateKind::CH => {
                // Clifft decomposition: R_Y(0.25 pi) 1; CX 0 1; R_Y(-0.25 pi) 1
                self.emit_rotation("ry", 0.25 * PI, q1);
                self.emit_two_qubit_gate("cx", q0, q1);
                self.emit_rotation("ry", -0.25 * PI, q1);
            }
        }
    }

    fn compile_three_qubit_gate(
        &mut self,
        q0: StimQubitId,
        q1: StimQubitId,
        q2: StimQubitId,
        gate: semantic::ThreeQubitGateKind,
    ) {
        match gate {
            semantic::ThreeQubitGateKind::CCZ => {
                // Clifft decomposition: H 2; CCX 0 1 2; H 2
                self.emit_gate("h", q2);
                self.emit_three_qubit_gate("ccx", q0, q1, q2);
                self.emit_gate("h", q2);
            }
            semantic::ThreeQubitGateKind::CCX => self.emit_three_qubit_gate("ccx", q0, q1, q2),
        }
    }

    fn compile_pauli_product_gate(
        &mut self,
        product: &semantic::PauliProduct,
        gate: semantic::PauliProductGateKind,
    ) {
        if product.factors.is_empty() {
            return;
        }
        let (intrinsic, adjoint) = match gate {
            semantic::PauliProductGateKind::S => ("s", false),
            semantic::PauliProductGateKind::S_DAG => ("s", true),
            semantic::PauliProductGateKind::T => ("t", false),
            semantic::PauliProductGateKind::T_DAG => ("t", true),
        };
        self.decompose_pauli_product_operation(product, |compiler, qubit, negated| {
            if adjoint ^ negated {
                compiler.emit_adjoint_gate(intrinsic, qubit);
            } else {
                compiler.emit_gate(intrinsic, qubit);
            }
        });
    }

    fn compile_pauli_product_rotation(&mut self, angle: Radians, product: &semantic::PauliProduct) {
        if product.factors.is_empty() {
            return;
        }
        self.decompose_pauli_product_operation(product, |compiler, qubit, negated| {
            compiler.emit_rotation("rz", if negated { -angle } else { angle }, qubit);
        });
    }

    fn compile_classical_control(
        &mut self,
        control: semantic::MeasurementRecord,
        target: StimQubitId,
        pauli: Pauli,
    ) {
        let result_id = self.resolve_record(control);
        let qubit = self.id_map.allocate_qubit(target);
        self.writer
            .write_classical_control(&pauli.as_str().to_ascii_lowercase(), result_id, qubit);
    }

    fn compile_noise(&mut self, instruction_span: Span, noise: &semantic::Noise) {
        match noise {
            semantic::Noise::CorrelatedError {
                kind,
                probability,
                faults,
            } => match kind {
                semantic::CorrelatedErrorKind::Initial => {
                    self.accumulate_correlated_noise(*probability, faults);
                }
                semantic::CorrelatedErrorKind::Else => {
                    self.continue_correlated_noise(instruction_span, *probability, faults);
                }
            },
            semantic::Noise::Depolarize2 {
                probability,
                q0,
                q1,
            } => {
                let table = self.noise_accumulator.build_noise_table(
                    2,
                    [
                        "IX", "IY", "IZ", "XI", "XX", "XY", "XZ", "YI", "YX", "YY", "YZ", "ZI",
                        "ZX", "ZY", "ZZ",
                    ]
                    .map(encode_pauli)
                    .to_vec(),
                    vec![*probability / 15.0; 15],
                );
                self.emit_noise(table, &[*q0, *q1]);
            }
            semantic::Noise::HeraldedErase { .. } => {
                self.unsupported("HERALDED_ERASE", instruction_span);
            }
            semantic::Noise::HeraldedPauliChannel1 { .. } => {
                self.unsupported("HERALDED_PAULI_CHANNEL_1", instruction_span);
            }
            semantic::Noise::PauliChannel1 {
                probabilities,
                qubit,
            } => {
                let table = self.noise_accumulator.build_noise_table(
                    1,
                    ["X", "Y", "Z"].map(encode_pauli).to_vec(),
                    probabilities.to_vec(),
                );
                self.emit_noise(table, &[*qubit]);
            }
            semantic::Noise::PauliChannel2 {
                probabilities,
                q0,
                q1,
            } => {
                let table = self.noise_accumulator.build_noise_table(
                    2,
                    [
                        "IX", "IY", "IZ", "XI", "XX", "XY", "XZ", "YI", "YX", "YY", "YZ", "ZI",
                        "ZX", "ZY", "ZZ",
                    ]
                    .map(encode_pauli)
                    .to_vec(),
                    probabilities.to_vec(),
                );
                self.emit_noise(table, &[*q0, *q1]);
            }
            semantic::Noise::SingleQubitNoise {
                kind,
                probability,
                qubit,
            } => match kind {
                semantic::SingleQubitNoiseKind::Depolarize => {
                    let table = self.noise_accumulator.build_noise_table(
                        1,
                        ["X", "Y", "Z"].map(encode_pauli).to_vec(),
                        vec![*probability / 3.0; 3],
                    );
                    self.emit_noise(table, &[*qubit]);
                }
                semantic::SingleQubitNoiseKind::Fault(kind) => {
                    let table = self.noise_accumulator.build_noise_table(
                        1,
                        vec![encode_pauli(kind.as_str())],
                        vec![*probability],
                    );
                    self.emit_noise(table, &[*qubit]);
                }
            },
        }
    }

    fn compile_single_qubit_measurement(
        &mut self,
        reset: bool,
        observable: Pauli,
        readout_noise: f64,
        negated: bool,
        qubit: StimQubitId,
    ) {
        let result_id = match (observable, reset) {
            (Z, false) => self.emit_measurement("m", qubit, negated),
            (Z, true) => self.emit_measurement_and_reset("mresetz", qubit, negated),
            (X, false) => {
                // Stim decomposition (into H, S, CX, M, R): H 0; M 0; H 0
                self.emit_gate("h", qubit); // X -> Z
                let result_id = self.emit_measurement("m", qubit, negated); // MZ
                self.emit_gate("h", qubit); // Z -> X
                result_id
            }
            (X, true) => {
                // Stim decomposition (into H, S, CX, M, R): H 0; M 0; R 0; H 0
                self.emit_gate("h", qubit); // X -> Z
                let result_id = self.emit_measurement_and_reset("mresetz", qubit, negated); // MRZ
                self.emit_gate("h", qubit); // Z -> X
                result_id
            }
            (Y, false) => {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; H 0; M 0; H 0; S 0
                self.emit_adjoint_gate("s", qubit); // Y -> X
                self.emit_gate("h", qubit); // X -> Z
                let result_id = self.emit_measurement("m", qubit, negated); // MZ
                self.emit_gate("h", qubit); // Z -> X
                self.emit_gate("s", qubit); // X -> Y
                result_id
            }
            (Y, true) => {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 0; S 0; H 0; M 0; R 0; H 0; S 0
                self.emit_adjoint_gate("s", qubit); // Y -> X
                self.emit_gate("h", qubit); // X -> Z
                let result_id = self.emit_measurement_and_reset("mresetz", qubit, negated); // MRZ
                self.emit_gate("h", qubit); // Z -> X
                self.emit_gate("s", qubit); // X -> Y
                result_id
            }
        };
        self.emit_optional_readout_noise(readout_noise, result_id);
    }

    fn compile_two_qubit_measurement(
        &mut self,
        readout_noise: f64,
        observable: semantic::PauliPair,
        negated: bool,
        q0: StimQubitId,
        q1: StimQubitId,
    ) {
        let result_id = match observable {
            semantic::PauliPair::XX => {
                // Stim decomposition (into H, S, CX, M, R): CX 0 1; H 0; M 0; H 0; CX 0 1
                self.emit_two_qubit_gate("cx", q0, q1);
                self.emit_gate("h", q0);
                let result_id = self.emit_measurement("m", q0, negated);
                self.emit_gate("h", q0);
                self.emit_two_qubit_gate("cx", q0, q1);
                result_id
            }
            semantic::PauliPair::YY => {
                // Stim decomposition (into H, S, CX, M, R): S 0; S 1; CX 0 1; H 0; M 0; S 1; S 1; H 0; CX 0 1; S 0; S 1
                self.emit_gate("s", q0);
                self.emit_gate("s", q1);
                self.emit_two_qubit_gate("cx", q0, q1);
                self.emit_gate("h", q0);
                let result_id = self.emit_measurement("m", q0, negated);
                self.emit_gate("z", q1);
                self.emit_gate("h", q0);
                self.emit_two_qubit_gate("cx", q0, q1);
                self.emit_gate("s", q0);
                self.emit_gate("s", q1);
                result_id
            }
            semantic::PauliPair::ZZ => {
                // Stim decomposition (into H, S, CX, M, R): CX 0 1; M 1; CX 0 1
                self.emit_two_qubit_gate("cx", q0, q1);
                let result_id = self.emit_measurement("m", q1, negated);
                self.emit_two_qubit_gate("cx", q0, q1);
                result_id
            }
        };
        self.emit_optional_readout_noise(readout_noise, result_id);
    }

    fn compile_pauli_product_measurement(
        &mut self,
        readout_noise: f64,
        product: &semantic::PauliProduct,
    ) {
        if product.factors.is_empty() {
            return;
        }
        self.decompose_pauli_product_operation(product, |compiler, qubit, negated| {
            let result_id = compiler.emit_measurement("m", qubit, negated);
            compiler.emit_optional_readout_noise(readout_noise, result_id);
        });
    }

    fn compile_require(
        &mut self,
        instruction_span: Span,
        records: &[semantic::NegatableMeasurementRecord],
    ) {
        let Some(scope_id) = self.expect_select_scope_id("REQUIRE", instruction_span) else {
            return;
        };
        let result_ids = records
            .iter()
            .map(|negatable_record| self.resolve_record(negatable_record.record))
            .collect::<Vec<_>>();

        if !self.validate_record_scoping("REQUIRE", instruction_span, &result_ids) {
            return;
        }

        let mut loss_registers = Vec::new();
        let mut result_registers = Vec::new();
        for (&result_id, record) in result_ids.iter().zip(records) {
            loss_registers.push(self.read_loss_register(result_id));
            result_registers.push(self.read_result_register(result_id, record.negated));
        }

        let loss = self.reduce_registers(&loss_registers, "loss", QirWriter::write_or);
        let parity = self.reduce_registers(&result_registers, "parity", QirWriter::write_xor);

        let restart = self.id_map.fresh_name("restart");
        self.writer.write_or(&restart, &loss, &parity);

        let restart_label = select_label(scope_id);
        let continue_label = self.id_map.fresh_name("continue");
        self.writer
            .write_branch(&restart, &restart_label, &continue_label);
        self.writer.write_label(&continue_label);
    }

    fn compile_not_leaked(
        &mut self,
        instruction_span: Span,
        records: &[semantic::MeasurementRecord],
    ) {
        let Some(scope_id) = self.expect_select_scope_id("NOTLEAKED", instruction_span) else {
            return;
        };
        let result_ids = records
            .iter()
            .map(|record| self.resolve_record(*record))
            .collect::<Vec<_>>();
        if !self.validate_record_scoping("NOTLEAKED", instruction_span, &result_ids) {
            return;
        }

        let mut has_error = false;
        for (&result_id, record) in result_ids.iter().zip(records) {
            if self.id_map.peek_loss_record_ids.contains(&result_id) {
                self.push_error(Error::NotLeakedOnPeekLoss { span: record.span });
                has_error = true;
            }
        }
        if has_error {
            return;
        }

        let loss_registers: Vec<String> = result_ids
            .into_iter()
            .map(|result_id| self.read_loss_register(result_id))
            .collect();
        let loss = self.reduce_registers(&loss_registers, "loss", QirWriter::write_or);

        let restart_label = select_label(scope_id);
        let continue_label = self.id_map.fresh_name("continue");
        self.writer
            .write_branch(&loss, &restart_label, &continue_label);
        self.writer.write_label(&continue_label);
    }

    fn accumulate_correlated_noise(&mut self, probability: f64, faults: &[semantic::Fault]) {
        self.noise_accumulator.push_correlated_row(CorrelatedRow {
            probability,
            faults: faults.to_vec(),
        });
    }

    fn continue_correlated_noise(
        &mut self,
        instruction_span: Span,
        probability: f64,
        faults: &[semantic::Fault],
    ) {
        if self.noise_accumulator.current_correlated_group.is_none() {
            self.push_error(Error::OrphanedElseCorrelatedError {
                span: instruction_span,
            });
            return;
        }
        self.accumulate_correlated_noise(probability, faults);
    }

    fn finish_correlated_noise(&mut self) {
        if self.noise_accumulator.current_correlated_group.is_none() {
            return;
        }
        let (noise_table, qubits) = self.noise_accumulator.flush_correlated_group();
        self.emit_noise(noise_table, &qubits);
    }

    /// Runs an operation on a Pauli product by reducing it to one qubit. Each factor is
    /// first rotated to the Z basis, then CNOTs combine their parity onto the first
    /// qubit. After `operation` runs on that qubit, the CNOTs and rotations are reversed.
    fn decompose_pauli_product_operation(
        &mut self,
        product: &semantic::PauliProduct,
        operation: impl FnOnce(&mut Self, StimQubitId, bool),
    ) {
        let focus_qubit = product.factors[0].qubit;
        for factor in &product.factors {
            self.rotate_to_z_basis(factor.pauli, factor.qubit);
        }
        for factor in product.factors.iter().skip(1) {
            self.emit_two_qubit_gate("cx", factor.qubit, focus_qubit);
        }
        operation(self, focus_qubit, product.negated);
        for factor in product.factors.iter().skip(1).rev() {
            self.emit_two_qubit_gate("cx", factor.qubit, focus_qubit);
        }
        for factor in product.factors.iter().rev() {
            self.rotate_from_z_basis(factor.pauli, factor.qubit);
        }
    }

    fn resolve_record(&self, record: semantic::MeasurementRecord) -> ResultId {
        self.id_map.record_count - record.offset
    }

    fn expect_select_scope_id(&mut self, instruction: &str, instruction_span: Span) -> Option<u32> {
        let Scope::Select { id, .. } = self.id_map.current_scope() else {
            self.push_error(Error::InstructionOutsideSelectBlock {
                instruction: instruction.to_string(),
                span: instruction_span,
            });
            return None;
        };
        Some(id)
    }

    fn validate_record_scoping(
        &mut self,
        instruction_name: &str,
        instruction_span: Span,
        result_ids: &[ResultId],
    ) -> bool {
        if result_ids
            .iter()
            .any(|&result_id| self.id_map.record_in_scope(result_id))
        {
            true
        } else {
            self.push_error(Error::AllMeasurementRecordsOutOfScope {
                instruction: instruction_name.to_string(),
                span: instruction_span,
            });
            false
        }
    }

    fn unsupported(&mut self, instruction_name: &str, instruction_span: Span) {
        if self.errors.iter().any(|error| {
            matches!(
                error,
                Error::UnsupportedInstruction {
                    name: existing_name,
                    span,
                } if existing_name == instruction_name && *span == instruction_span
            )
        }) {
            return;
        }

        self.push_error(Error::UnsupportedInstruction {
            name: instruction_name.to_string(),
            span: instruction_span,
        });
    }

    fn rotate_to_z_basis(&mut self, pauli: Pauli, qubit: u32) {
        match pauli {
            X => self.emit_gate("h", qubit),
            Y => {
                self.emit_adjoint_gate("s", qubit);
                self.emit_gate("h", qubit);
            }
            Z => (),
        }
    }

    fn rotate_from_z_basis(&mut self, pauli: Pauli, qubit: u32) {
        match pauli {
            X => self.emit_gate("h", qubit),
            Y => {
                self.emit_gate("h", qubit);
                self.emit_gate("s", qubit);
            }
            Z => (),
        }
    }

    fn emit_gate(&mut self, intrinsic: &str, qubit: StimQubitId) {
        let q = self.id_map.allocate_qubit(qubit);
        self.writer.write_qis_call(intrinsic, &[q]);
    }

    fn emit_adjoint_gate(&mut self, intrinsic: &str, qubit: StimQubitId) {
        let q = self.id_map.allocate_qubit(qubit);
        self.writer.write_qis_adj_call(intrinsic, &[q]);
    }

    fn emit_two_qubit_gate(&mut self, intrinsic: &str, q0: StimQubitId, q1: StimQubitId) {
        let q0 = self.id_map.allocate_qubit(q0);
        let q1 = self.id_map.allocate_qubit(q1);
        self.writer.write_qis_call(intrinsic, &[q0, q1]);
    }

    fn emit_three_qubit_gate(
        &mut self,
        intrinsic: &str,
        q0: StimQubitId,
        q1: StimQubitId,
        q2: StimQubitId,
    ) {
        let q0 = self.id_map.allocate_qubit(q0);
        let q1 = self.id_map.allocate_qubit(q1);
        let q2 = self.id_map.allocate_qubit(q2);
        self.writer.write_qis_call(intrinsic, &[q0, q1, q2]);
    }

    fn emit_rotation(&mut self, intrinsic: &str, angle: Radians, qubit: StimQubitId) {
        let qubit = self.id_map.allocate_qubit(qubit);
        self.writer.write_rotation_call(intrinsic, angle, &[qubit]);
    }

    fn emit_two_qubit_rotation(
        &mut self,
        intrinsic: &str,
        angle: Radians,
        q0: StimQubitId,
        q1: StimQubitId,
    ) {
        let q0 = self.id_map.allocate_qubit(q0);
        let q1 = self.id_map.allocate_qubit(q1);
        self.writer.write_rotation_call(intrinsic, angle, &[q0, q1]);
    }

    fn emit_measurement(&mut self, intrinsic: &str, qubit: StimQubitId, negated: bool) -> ResultId {
        let q = self.id_map.allocate_qubit(qubit);
        if negated {
            self.writer.write_qis_call("x", &[q]);
        }
        let r = self.id_map.allocate_record();
        self.writer.write_measure_call(intrinsic, q, r);
        if negated {
            self.writer.write_qis_call("x", &[q]);
        }
        r
    }

    fn emit_measurement_and_reset(
        &mut self,
        intrinsic: &str,
        qubit: StimQubitId,
        negated: bool,
    ) -> ResultId {
        let q = self.id_map.allocate_qubit(qubit);
        if negated {
            self.writer.write_qis_call("x", &[q]);
        }
        let r = self.id_map.allocate_record();
        self.writer.write_measure_call(intrinsic, q, r);
        r
    }

    fn emit_peek_loss(&mut self, qubit: StimQubitId) -> ResultId {
        let q = self.id_map.allocate_qubit(qubit);
        let r = self.id_map.allocate_record();
        self.id_map.peek_loss_record_ids.insert(r);
        self.writer.write_qis_call("peek_loss", &[q, r]);
        r
    }

    fn emit_noise(&mut self, table: NoiseTable<f64>, qubits: &[StimQubitId]) {
        let ids: Vec<QubitId> = qubits
            .iter()
            .map(|&qubit| self.id_map.allocate_qubit(qubit))
            .collect();
        let name = self.noise_accumulator.get_or_insert_intrinsic(table);
        self.writer.write_noise_call(&name, &ids);
    }

    fn emit_optional_readout_noise(&mut self, probability: f64, result_id: ResultId) {
        if probability > 0.0 {
            self.writer.write_readout_noise_call(probability, result_id);
        }
    }

    fn read_loss_register(&mut self, result_id: ResultId) -> String {
        let loss_register = self.id_map.fresh_name("l");
        self.writer
            .write_read(&loss_register, "__quantum__rt__read_loss", result_id);
        loss_register
    }

    fn read_result_register(&mut self, result_id: ResultId, negated: bool) -> String {
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

    fn push_error(&mut self, error: Error) {
        self.errors.push(error);
    }
}

pub fn compile_to_qir(
    circuit: &semantic::Circuit,
    noise: &mut NoiseConfig<f64, f64>,
) -> Result<String, Vec<Error>> {
    Compiler::new(noise).into_qir(circuit)
}
