// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use log::warn;
use qsc_data_structures::debug::MetadataPackageSpan;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use std::{
    cmp::{self, max},
    fmt::{Display, Write},
    hash::{Hash, Hasher},
    ops::Not,
    vec,
};

/// Current format version.
pub const CURRENT_VERSION: usize = 1;

/// Representation of a quantum circuit group.
#[derive(Clone, Serialize, Deserialize, Default, Debug)]
pub struct CircuitGroup {
    pub circuits: Vec<Circuit>,
    pub version: usize,
}

impl Display for CircuitGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for circuit in &self.circuits {
            writeln!(f, "{circuit}")?;
        }
        Ok(())
    }
}

/// Representation of a quantum circuit.
#[derive(Clone, Serialize, Deserialize, Default, Debug)]
pub struct Circuit {
    pub qubits: Vec<Qubit>,
    #[serde(rename = "componentGrid")]
    pub component_grid: ComponentGrid,
}

/// Type alias for a grid of components.
pub type ComponentGrid = Vec<ComponentColumn>;

/// Representation of a column in the component grid.
#[derive(Clone, Serialize, Deserialize, Default, Debug)]
pub struct ComponentColumn {
    pub components: Vec<Component>,
}

/// Union type for components.
pub type Component = Operation;

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum SourceLocation {
    Resolved(ResolvedSourceLocation),
    #[serde(skip)]
    Unresolved(MetadataPackageSpan),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ResolvedSourceLocation {
    // Use ILocation in wasm, this is hella confusing
    pub file: String,
    pub line: u32,
    pub column: u32,
}

impl Display for ResolvedSourceLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO: pretty sure we have to add 1 here
        write!(f, "{}:{}:{}", self.file, self.line, self.column)
    }
}

/// Union type for operations.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(tag = "kind")]
pub enum Operation {
    #[serde(rename = "measurement")]
    Measurement(Measurement),
    #[serde(rename = "unitary")]
    Unitary(Unitary),
    #[serde(rename = "ket")]
    Ket(Ket),
}

impl Operation {
    /// Returns the gate name of the operation.
    #[must_use]
    pub fn gate(&self) -> String {
        match self {
            Operation::Measurement(m) => m.gate.clone(),
            Operation::Unitary(u) => u.gate.clone(),
            #[allow(clippy::unicode_not_nfc)]
            Operation::Ket(k) => format!("|{}〉", k.gate),
        }
    }

    fn gate_mut(&mut self) -> &mut String {
        match self {
            Self::Measurement(measurement) => &mut measurement.gate,
            Self::Unitary(unitary) => &mut unitary.gate,
            Self::Ket(ket) => &mut ket.gate,
        }
    }

    /// Returns the arguments for the operation.
    #[must_use]
    pub fn args(&self) -> Vec<String> {
        match self {
            Operation::Measurement(m) => m.args.clone(),
            Operation::Unitary(u) => u.args.clone(),
            Operation::Ket(k) => k.args.clone(),
        }
    }

    pub fn args_mut(&mut self) -> &mut Vec<String> {
        match self {
            Self::Measurement(measurement) => &mut measurement.args,
            Self::Unitary(unitary) => &mut unitary.args,
            Self::Ket(ket) => &mut ket.args,
        }
    }

    #[must_use]
    pub fn source(&self) -> &Option<SourceLocation> {
        match self {
            Self::Measurement(measurement) => &measurement.source,
            Self::Unitary(unitary) => &unitary.source,
            Self::Ket(ket) => &ket.source,
        }
    }

    #[must_use]
    pub fn source_mut(&mut self) -> &mut Option<SourceLocation> {
        match self {
            Self::Measurement(measurement) => &mut measurement.source,
            Self::Unitary(unitary) => &mut unitary.source,
            Self::Ket(ket) => &mut ket.source,
        }
    }

    /// Returns the children for the operation.
    #[must_use]
    pub fn children(&self) -> &ComponentGrid {
        match self {
            Operation::Measurement(m) => &m.children,
            Operation::Unitary(u) => &u.children,
            Operation::Ket(k) => &k.children,
        }
    }

    /// Returns the children for the operation.
    #[must_use]
    pub fn children_mut(&mut self) -> &mut ComponentGrid {
        match self {
            Operation::Measurement(m) => &mut m.children,
            Operation::Unitary(u) => &mut u.children,
            Operation::Ket(k) => &mut k.children,
        }
    }

    /// Returns if the operation is a controlled operation.
    #[must_use]
    pub fn is_controlled(&self) -> bool {
        match self {
            Operation::Measurement(_) | Operation::Ket(_) => false,
            Operation::Unitary(u) => !u.controls.is_empty(),
        }
    }

    /// Returns if the operation is a measurement operation.
    #[must_use]
    pub fn is_measurement(&self) -> bool {
        match self {
            Operation::Measurement(_) => true,
            Operation::Unitary(_) | Operation::Ket(_) => false,
        }
    }

    /// Returns if the operation is an adjoint operation.
    #[must_use]
    pub fn is_adjoint(&self) -> bool {
        match self {
            Operation::Measurement(_) | Operation::Ket(_) => false,
            Operation::Unitary(u) => u.is_adjoint,
        }
    }
}

/// Representation of a measurement operation.
#[derive(Clone, Serialize, Deserialize, Default, Debug)]
pub struct Measurement {
    pub gate: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub children: ComponentGrid,
    pub qubits: Vec<Register>,
    pub results: Vec<Register>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceLocation>,
}

/// Representation of a unitary operation.
#[derive(Clone, Serialize, Deserialize, Default, Debug)]
pub struct Unitary {
    pub gate: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub children: ComponentGrid,
    pub targets: Vec<Register>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub controls: Vec<Register>,
    #[serde(rename = "isAdjoint")]
    #[serde(skip_serializing_if = "Not::not")]
    #[serde(default)]
    pub is_adjoint: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceLocation>,
}

/// Representation of a gate that will set the target to a specific state.
#[derive(Clone, Serialize, Deserialize, Default, Debug)]
pub struct Ket {
    pub gate: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub children: ComponentGrid,
    pub targets: Vec<Register>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceLocation>,
}

#[derive(Serialize, Deserialize, Debug, Eq, Hash, PartialEq, Clone)]
pub struct Register {
    pub qubit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<usize>,
}

impl Register {
    #[must_use]
    pub fn quantum(qubit_id: usize) -> Self {
        Self {
            qubit: qubit_id,
            result: None,
        }
    }

    #[must_use]
    pub fn classical(qubit_id: usize, result_id: usize) -> Self {
        Self {
            qubit: qubit_id,
            result: Some(result_id),
        }
    }

    #[must_use]
    pub fn is_classical(&self) -> bool {
        self.result.is_some()
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Qubit {
    pub id: usize,
    #[serde(rename = "numResults")]
    #[serde(default)]
    pub num_results: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declarations: Option<Vec<SourceLocation>>,
}

#[derive(Clone, Debug, Copy)]
pub struct Config {
    /// How the circuit is generated
    pub generation_method: GenerationMethod,
    pub tracer_config: TracerConfig,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GenerationMethod {
    /// Simulate the program and trace the actual gate calls. Nondeterministic
    Simulate,
    /// Evaluate the classical parts. Will fail if branching on measurement occurs
    ClassicalEval,
    /// Compile the program and transform to a circuit without any evaluation.
    /// Only works for `AdaptiveRIF` compliant programs.
    Static,
}

#[derive(Clone, Debug, Copy)]
#[allow(clippy::struct_excessive_bools)]
pub struct TracerConfig {
    /// Maximum number of operations the builder will add to the circuit
    pub max_operations: usize,
    /// Show the source code locations of operations and qubit declarations
    /// in the circuit diagram
    pub locations: bool,
    /// Detect repeated motifs in the circuit and group them into sub-circuits
    pub loop_detection: bool,
    pub group_scopes: bool,
    /// Collapse qubit registers into single qubits
    pub collapse_qubit_registers: bool,
}

impl TracerConfig {
    /// Set to the current UI limit + 1 so that it still triggers
    /// the "this circuit has too many gates" warning in the UI.
    /// (see npm\qsharp\ux\circuit.tsx)
    ///
    /// A more refined way to do this might be to communicate the
    /// "limit exceeded" state up to the UI somehow.
    const DEFAULT_MAX_OPERATIONS: usize = 10001;
}

impl Default for TracerConfig {
    fn default() -> Self {
        Self {
            max_operations: Self::DEFAULT_MAX_OPERATIONS,
            locations: true,
            loop_detection: false,
            group_scopes: true,
            collapse_qubit_registers: false,
        }
    }
}

type ObjectsByColumn = FxHashMap<usize, CircuitObject>;

struct Row {
    wire: Wire,
    objects: ObjectsByColumn,
    next_column: usize,
}

enum Wire {
    Qubit { label: String },
    Classical { start_column: Option<usize> },
}

enum CircuitObject {
    Blank,
    Wire,
    WireCross,
    WireStart,
    DashedCross,
    Vertical,
    VerticalDashed,
    Object(String),
}

impl Row {
    fn add_object(&mut self, column: usize, object: &str) {
        self.add(column, CircuitObject::Object(object.to_string()));
    }

    fn add_measurement(&mut self, column: usize, source: Option<&SourceLocation>) {
        let mut gate_label = String::from("M");
        if let Some(SourceLocation::Resolved(loc)) = source {
            let _ = write!(&mut gate_label, "@{loc}");
        }
        self.add(column, CircuitObject::Object(gate_label.to_string()));
    }

    fn add_gate(
        &mut self,
        column: usize,
        gate: &str,
        args: &[String],
        is_adjoint: bool,
        source: Option<&SourceLocation>,
    ) {
        let mut gate_label = String::new();
        gate_label.push_str(gate);
        if is_adjoint {
            gate_label.push('\'');
        }

        if !args.is_empty() {
            let args = args.join(", ");
            let _ = write!(&mut gate_label, "({args})");
        }

        if let Some(SourceLocation::Resolved(loc)) = source {
            let _ = write!(&mut gate_label, "@{}:{}:{}", loc.file, loc.line, loc.column);
        }

        self.add_object(column, gate_label.as_str());
    }

    fn add_vertical(&mut self, column: usize) {
        if !self.objects.contains_key(&column) {
            match self.wire {
                Wire::Qubit { .. } => self.add(column, CircuitObject::WireCross),
                Wire::Classical { start_column } => {
                    if start_column.is_some() {
                        self.add(column, CircuitObject::WireCross);
                    } else {
                        self.add(column, CircuitObject::Vertical);
                    }
                }
            }
        }
    }

    fn add_dashed_vertical(&mut self, column: usize) {
        if !self.objects.contains_key(&column) {
            match self.wire {
                Wire::Qubit { .. } => self.add(column, CircuitObject::DashedCross),
                Wire::Classical { start_column } => {
                    if start_column.is_some() {
                        self.add(column, CircuitObject::DashedCross);
                    } else {
                        self.add(column, CircuitObject::VerticalDashed);
                    }
                }
            }
        }
    }

    fn start_classical(&mut self, column: usize) {
        self.add(column, CircuitObject::WireStart);
        if let Wire::Classical { start_column } = &mut self.wire {
            start_column.replace(column);
        }
    }

    fn add(&mut self, column: usize, circuit_object: CircuitObject) {
        self.objects.insert(column, circuit_object);
        self.next_column = column + 1;
    }

    fn fmt(&self, f: &mut std::fmt::Formatter<'_>, columns: &[Column]) -> std::fmt::Result {
        // Temporary string so we can trim whitespace at the end
        let mut s = String::new();
        match &self.wire {
            Wire::Qubit { label } => {
                s.write_str(&fmt_qubit_label(label))?;
                for (column_index, column) in columns.iter().enumerate().skip(1) {
                    let val = self.objects.get(&column_index);
                    let object = val.unwrap_or(&CircuitObject::Wire);

                    s.write_str(&column.fmt_qubit_circuit_object(object))?;
                }
            }
            Wire::Classical { start_column } => {
                for (column_index, column) in columns.iter().enumerate() {
                    let val = self.objects.get(&column_index);

                    let object = match (val, start_column) {
                        (Some(v), _) => v,
                        (None, Some(s)) if column_index > *s => &CircuitObject::Wire,
                        _ => &CircuitObject::Blank,
                    };

                    s.write_str(&column.fmt_classical_circuit_object(object))?;
                }
            }
        }
        writeln!(f, "{}", s.trim_end())?;
        Ok(())
    }
}

const MIN_COLUMN_WIDTH: usize = 7;

const QUBIT_WIRE: [char; 3] = ['─', '─', '─']; // "───────"
const CLASSICAL_WIRE: [char; 3] = ['═', '═', '═']; // "═══════"
const QUBIT_WIRE_CROSS: [char; 3] = ['─', '┼', '─']; // "───┼───"
const CLASSICAL_WIRE_CROSS: [char; 3] = ['═', '╪', '═']; // "═══╪═══"
const CLASSICAL_WIRE_START: [char; 3] = [' ', '╘', '═']; // "   ╘═══"
const QUBIT_WIRE_DASHED_CROSS: [char; 3] = ['─', '┆', '─']; // "───┆───"
const CLASSICAL_WIRE_DASHED_CROSS: [char; 3] = ['═', '┆', '═']; // "═══┆═══"
const VERTICAL_DASHED: [char; 3] = [' ', '┆', ' ']; // "   ┆   "
const VERTICAL: [char; 3] = [' ', '│', ' ']; // "   │   "
const BLANK: [char; 3] = [' ', ' ', ' ']; // "       "

/// "q_0  "
#[allow(clippy::doc_markdown)]
fn fmt_qubit_label(label: &str) -> String {
    let rest = MIN_COLUMN_WIDTH - 1;
    format!("{label: <rest$} ")
}

struct Column {
    column_width: usize,
}

impl Default for Column {
    fn default() -> Self {
        Self {
            column_width: MIN_COLUMN_WIDTH,
        }
    }
}

impl Column {
    fn new(column_width: usize) -> Self {
        // Column widths should be odd numbers for this struct to work well
        let odd_column_width = column_width | 1;
        Self {
            column_width: odd_column_width,
        }
    }

    /// "── A ──"
    fn fmt_on_qubit_wire(&self, obj: &str) -> String {
        let column_width = self.column_width;
        format!("{:─^column_width$}", format!(" {obj} "))
    }

    /// "══ A ══"
    fn fmt_on_classical_wire(&self, obj: &str) -> String {
        let column_width = self.column_width;
        format!("{:═^column_width$}", format!(" {obj} "))
    }

    fn expand_template(&self, template: &[char; 3]) -> String {
        let half_width = self.column_width / 2;
        let left = template[0].to_string().repeat(half_width);
        let right = template[2].to_string().repeat(half_width);

        format!("{left}{}{right}", template[1])
    }

    fn fmt_classical_circuit_object(&self, circuit_object: &CircuitObject) -> String {
        if let CircuitObject::Object(label) = circuit_object {
            return self.fmt_on_classical_wire(label.as_str());
        }

        let template = match circuit_object {
            CircuitObject::Blank => BLANK,
            CircuitObject::Wire => CLASSICAL_WIRE,
            CircuitObject::WireCross => CLASSICAL_WIRE_CROSS,
            CircuitObject::WireStart => CLASSICAL_WIRE_START,
            CircuitObject::DashedCross => CLASSICAL_WIRE_DASHED_CROSS,
            CircuitObject::Vertical => VERTICAL,
            CircuitObject::VerticalDashed => VERTICAL_DASHED,
            CircuitObject::Object(_) => unreachable!("This case is covered in the early return."),
        };

        self.expand_template(&template)
    }

    fn fmt_qubit_circuit_object(&self, circuit_object: &CircuitObject) -> String {
        if let CircuitObject::Object(label) = circuit_object {
            return self.fmt_on_qubit_wire(label.as_str());
        }

        let template = match circuit_object {
            CircuitObject::WireStart // This should never happen
            | CircuitObject::Blank => BLANK,
            CircuitObject::Wire => QUBIT_WIRE,
            CircuitObject::WireCross => QUBIT_WIRE_CROSS,
            CircuitObject::DashedCross => QUBIT_WIRE_DASHED_CROSS,
            CircuitObject::Vertical => VERTICAL,
            CircuitObject::VerticalDashed => VERTICAL_DASHED,
            CircuitObject::Object(_) => unreachable!("This case is covered in the early return."),
        };

        self.expand_template(&template)
    }
}

impl Display for Circuit {
    /// Formats the circuit into a diagram.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut rows = vec![];

        // Maintain a mapping from from Registers in the Circuit schema
        // to row in the diagram
        let mut register_to_row = FxHashMap::default();

        // Keep track of which qubits have the qubit after them in the same multi-qubit operation,
        // because those qubits need to get a gap row below them.
        let mut qubits_with_gap_row_below = FxHashSet::default();

        // Identify qubits that require gap rows
        self.identify_qubits_with_gap_rows(&mut qubits_with_gap_row_below);

        // Initialize rows for qubits and classical wires
        self.initialize_rows(&mut rows, &mut register_to_row, &qubits_with_gap_row_below);

        // Add operations to the diagram
        Self::add_operations_to_diagram(1, &self.component_grid, &mut rows, &register_to_row);

        // Finalize the diagram by extending wires and formatting columns
        let columns = finalize_columns(&rows);

        // Draw the diagram
        for row in rows {
            row.fmt(f, &columns)?;
        }

        Ok(())
    }
}

impl Circuit {
    /// Identifies qubits that require gap rows for multi-qubit operations.
    fn identify_qubits_with_gap_rows(&self, qubits_with_gap_row_below: &mut FxHashSet<usize>) {
        for col in &self.component_grid {
            for op in &col.components {
                let targets = match op {
                    Operation::Measurement(m) => &m.qubits,
                    Operation::Unitary(u) => &u.targets,
                    Operation::Ket(k) => &k.targets,
                };
                for target in targets {
                    let qubit = target.qubit;

                    if qubits_with_gap_row_below.contains(&qubit) {
                        continue;
                    }

                    let next_qubit = qubit + 1;

                    // Check if the next qubit is also in this operation.
                    if targets.iter().any(|t| t.qubit == next_qubit) {
                        qubits_with_gap_row_below.insert(qubit);
                    }
                }
            }
        }
    }

    /// Initializes rows for qubits and classical wires.
    fn initialize_rows(
        &self,
        rows: &mut Vec<Row>,
        register_to_row: &mut FxHashMap<(usize, Option<usize>), usize>,
        qubits_with_gap_row_below: &FxHashSet<usize>,
    ) {
        for q in &self.qubits {
            let mut label = format!("q_{}", q.id);
            let mut first = true;
            for loc in q.declarations.iter().flatten() {
                if let SourceLocation::Resolved(loc) = loc {
                    if first {
                        label.push('@');
                        first = false;
                    } else {
                        label.push_str(", ");
                    }
                    let _ = write!(&mut label, "{loc}");
                }
            }
            rows.push(Row {
                wire: Wire::Qubit { label },
                objects: FxHashMap::default(),
                next_column: 1,
            });

            register_to_row.insert((q.id, None), rows.len() - 1);

            // If this qubit has no children, but it is in a multi-qubit operation with
            // the next qubit, we add an empty row to make room for the vertical connector.
            // We can just use a classical wire type for this row since the wire won't actually be rendered.
            let extra_rows = if qubits_with_gap_row_below.contains(&q.id) {
                cmp::max(1, q.num_results)
            } else {
                q.num_results
            };

            for i in 0..extra_rows {
                rows.push(Row {
                    wire: Wire::Classical { start_column: None },
                    objects: FxHashMap::default(),
                    next_column: 1,
                });

                register_to_row.insert((q.id, Some(i)), rows.len() - 1);
            }
        }
    }

    /// Adds operations to the diagram.
    fn add_operations_to_diagram(
        start_column: usize,
        component_grid: &ComponentGrid,
        rows: &mut [Row],
        register_to_row: &FxHashMap<(usize, Option<usize>), usize>,
    ) -> usize {
        let mut column = start_column;
        let mut next_column = start_column;
        for col in component_grid {
            for op in &col.components {
                let targets = get_row_indexes(op, register_to_row, true);
                let controls = get_row_indexes(op, register_to_row, false);

                let mut all_rows = targets.clone();
                all_rows.extend(controls.iter());
                all_rows.sort_unstable();

                // We'll need to know the entire range of rows for this operation so we can
                // figure out the starting column and also so we can draw any
                // vertical lines that cross wires.
                let (begin, end) = all_rows.split_first().map_or((0, 0), |(first, tail)| {
                    (*first, tail.last().unwrap_or(first) + 1)
                });

                let children = op.children();
                if children.is_empty() {
                    add_operation_to_rows(op, rows, &targets, &controls, column, begin, end);
                    next_column = max(next_column, column + 1);
                } else {
                    let mut offset = 0;
                    add_operation_box_start_to_rows(
                        op,
                        rows,
                        &targets,
                        &controls,
                        column + offset,
                        begin,
                        end,
                    );
                    offset += 2;
                    offset += Self::add_operations_to_diagram(
                        column + offset,
                        children,
                        rows,
                        register_to_row,
                    );
                    add_operation_box_end_to_rows(op, rows, &targets, column + offset);
                    offset += 1;
                    next_column = max(next_column, column + offset);
                }
            }
            column = next_column;
        }
        next_column - start_column
    }
}

/// Adds a single operation to the rows.
fn add_operation_to_rows(
    operation: &Operation,
    rows: &mut [Row],
    targets: &[usize],
    controls: &[usize],
    column: usize,
    begin: usize,
    end: usize,
) {
    for i in targets {
        let row = &mut rows[*i];
        if matches!(row.wire, Wire::Classical { .. })
            && matches!(operation, Operation::Measurement(_))
        {
            row.start_classical(column);
        } else {
            row.add_gate(
                column,
                &operation.gate(),
                &operation.args(),
                operation.is_adjoint(),
                operation.source().as_ref(),
            );
        }
    }

    if operation.is_controlled() || operation.is_measurement() {
        for i in controls {
            let row = &mut rows[*i];
            if matches!(row.wire, Wire::Qubit { .. }) && operation.is_measurement() {
                row.add_measurement(column, operation.source().as_ref());
            } else {
                row.add_object(column, "●");
            }
        }

        // If we have a control wire, draw vertical lines spanning all
        // control and target wires and crossing any in between
        // (vertical lines may overlap if there are multiple controls/targets,
        // this is ok in practice)
        for row in &mut rows[begin..end] {
            row.add_vertical(column);
        }
    } else {
        // No control wire. Draw dashed vertical lines to connect
        // target wires if there are multiple targets
        for row in &mut rows[begin..end] {
            row.add_dashed_vertical(column);
        }
    }
}

fn add_operation_box_start_to_rows(
    operation: &Operation,
    rows: &mut [Row],
    targets: &[usize],
    controls: &[usize],
    column: usize,
    begin: usize,
    end: usize,
) {
    assert!(
        !operation.children().is_empty(),
        "must only be called for an operation with children"
    );
    for i in targets {
        let row = &mut rows[*i];
        row.add_object(column, "[[");

        let mut gate_label = String::new();
        gate_label.push('[');
        gate_label.push_str(&operation.gate());
        if operation.is_adjoint() {
            gate_label.push('\'');
        }
        let args = operation.args();

        if !args.is_empty() {
            let args = args.join(", ");
            let _ = write!(&mut gate_label, "({args})");
        }

        if let Some(SourceLocation::Resolved(loc)) = operation.source() {
            let _ = write!(&mut gate_label, "@{loc}");
        }

        gate_label.push(']');

        row.add_object(column + 1, gate_label.as_str());
    }

    if operation.is_controlled() || operation.is_measurement() {
        for i in controls {
            let row = &mut rows[*i];
            if matches!(row.wire, Wire::Qubit { .. }) && operation.is_measurement() {
                row.add_measurement(column + 1, operation.source().as_ref());
            } else {
                row.add_object(column + 1, "●");
            }
        }

        // If we have a control wire, draw vertical lines spanning all
        // control and target wires and crossing any in between
        // (vertical lines may overlap if there are multiple controls/targets,
        // this is ok in practice)
        for row in &mut rows[begin..end] {
            row.add_vertical(column + 1);
        }
    } else {
        // No control wire. Draw dashed vertical lines to connect
        // target wires if there are multiple targets
        for row in &mut rows[begin..end] {
            row.add_dashed_vertical(column + 1);
        }
    }
}

fn add_operation_box_end_to_rows(
    operation: &Operation,
    rows: &mut [Row],
    targets: &[usize],
    column: usize,
) {
    assert!(
        !operation.children().is_empty(),
        "must only be called for an operation with children"
    );
    for i in targets {
        let row = &mut rows[*i];
        row.add_object(column, "]]");
    }
}

/// Finalizes the columns by calculating their widths.
fn finalize_columns(rows: &[Row]) -> Vec<Column> {
    // Find the end column for the whole circuit so that
    // all qubit wires will extend until the end
    let end_column = rows
        .iter()
        .max_by_key(|r| r.next_column)
        .map_or(1, |r| r.next_column);

    // To be able to fit long-named operations, we calculate the required width for each column,
    // based on the maximum length needed for gates, where a gate X is printed as "- X -".
    (0..end_column)
        .map(|column| {
            Column::new(
                rows.iter()
                    .filter_map(|row| row.objects.get(&column))
                    .filter_map(|object| match object {
                        CircuitObject::Object(string) => Some(string.len() + 4),
                        _ => None,
                    })
                    .chain(std::iter::once(MIN_COLUMN_WIDTH))
                    .max()
                    .expect("Column width should be at least 1"),
            )
        })
        .collect()
}

/// Gets the row indexes for the targets or controls of an operation.
fn get_row_indexes(
    operation: &Operation,
    register_to_row: &FxHashMap<(usize, Option<usize>), usize>,
    is_target: bool,
) -> Vec<usize> {
    let registers = match operation {
        Operation::Measurement(m) => {
            if is_target {
                &m.results
            } else {
                &m.qubits
            }
        }
        Operation::Unitary(u) => {
            if is_target {
                &u.targets
            } else {
                &u.controls
            }
        }
        Operation::Ket(k) => {
            if is_target {
                &k.targets
            } else {
                &vec![]
            }
        }
    };

    registers
        .iter()
        .filter_map(|reg| {
            let reg = (reg.qubit, reg.result);
            register_to_row.get(&reg).copied()
        })
        .collect()
}

/// Converts a list of operations into a 2D grid of operations in col-row format.
/// Operations will be left-justified as much as possible in the resulting grid.
/// Children operations are recursively converted into a grid.
///
/// # Arguments
///
/// * `operations` - A vector of operations to be converted.
/// * `num_qubits` - The number of qubits in the circuit.
///
/// # Returns
///
/// A component grid representing the operations.
#[must_use]
pub fn operation_list_to_grid(
    operations: Vec<Operation>,
    qubits: &[Qubit],
    loop_detection: bool,
) -> ComponentGrid {
    let operations = if loop_detection {
        collapse_repetition(operations)
    } else {
        operations
    };

    operation_list_to_grid_inner(operations, qubits)
}

fn collapse_repetition(mut operations: Vec<Operation>) -> Vec<Operation> {
    for op in &mut operations {
        if !op.children().is_empty() {
            assert_eq!(
                op.children().len(),
                1,
                "children should be a single list at this point"
            );
            let mut first = op.children_mut().remove(0);
            first.components = collapse_repetition(first.components);
            op.children_mut().push(first);
        }
    }
    collapse_repetition_base(operations)
}

fn operation_list_to_grid_inner(
    mut operations: Vec<Operation>,
    qubits: &[Qubit],
) -> Vec<ComponentColumn> {
    for op in &mut operations {
        // The children data structure is a grid, so checking if it is
        // length 1 is actually checking if it has a single column,
        // or in other words, we are checking if its children are in a single list.
        // If the operation has children in a single list, it needs to be converted to a grid.
        // If it was already converted to a grid, but the grid was still a single list,
        // then doing it again won't effect anything.
        if op.children().len() == 1 {
            match op {
                Operation::Measurement(m) => {
                    let child_vec = m.children.remove(0).components; // owns
                    m.children = operation_list_to_grid_inner(child_vec, qubits);
                }
                Operation::Unitary(u) => {
                    let child_vec = u.children.remove(0).components;
                    u.children = operation_list_to_grid_inner(child_vec, qubits);
                }
                Operation::Ket(k) => {
                    let child_vec = k.children.remove(0).components;
                    k.children = operation_list_to_grid_inner(child_vec, qubits);
                }
            }
        }
    }

    // Convert the operations into a component grid
    operation_list_to_grid_base(operations, qubits)
}

#[derive(Debug)]
struct RowInfo {
    register: Register,
    next_available_column: usize,
}

fn get_row_for_register(register: &Register, rows: &[RowInfo]) -> usize {
    rows.iter()
        .position(|r| r.register == *register)
        .unwrap_or_else(|| panic!("register {register:?} not found in rows {rows:?}"))
}

fn operation_list_to_grid_base(
    operations: Vec<Operation>,
    qubits: &[Qubit],
) -> Vec<ComponentColumn> {
    let mut rows = vec![];
    for q in qubits {
        rows.push(RowInfo {
            register: Register::quantum(q.id),
            next_available_column: 0,
        });
        for i in 0..q.num_results {
            rows.push(RowInfo {
                register: Register::classical(q.id, i),
                next_available_column: 0,
            });
        }
    }

    let mut columns: Vec<ComponentColumn> = vec![];

    for op in operations {
        // get the entire range that this operation spans
        let targets = match &op {
            Operation::Measurement(m) => &m.qubits,
            Operation::Unitary(u) => &u.targets,
            Operation::Ket(k) => &k.targets,
        };
        let controls = match &op {
            Operation::Measurement(m) => &m.results,
            Operation::Unitary(u) => &u.controls,
            Operation::Ket(_) => &vec![],
        };
        let mut all_rows = targets
            .iter()
            .chain(controls.iter())
            .map(|r| get_row_for_register(r, &rows))
            .collect::<Vec<_>>();
        all_rows.sort_unstable();
        let (begin, end) = all_rows.split_first().map_or((0, 0), |(first, tail)| {
            (*first, tail.last().unwrap_or(first) + 1)
        });
        // find the earliest column that all rows in this range are available
        let column = rows[begin..end]
            .iter()
            .map(|r| r.next_available_column)
            .max()
            .unwrap_or(0);
        // assign this operation to that column
        // and update the rows to mark them as occupied until the next column
        for r in &mut rows[begin..end] {
            r.next_available_column = column + 1;
        }
        if columns.len() <= column {
            columns.resize_with(column + 1, || ComponentColumn { components: vec![] });
        }
        columns[column].components.push(op);
    }

    columns
}

fn make_repeated_parent(base: &Operation, count: usize) -> Operation {
    debug_assert!(count > 1);
    let mut parent = base.clone();
    let mut children = vec![];
    let mut tail_children = vec![];

    for i in 0..count {
        if i == 0 {
            children.push(base.clone());
        } else {
            tail_children.push(base.clone());
        }
    }

    if !tail_children.is_empty() {
        let mut tail = base.clone();
        *tail.children_mut() = vec![ComponentColumn {
            components: tail_children,
        }];
        *tail.gate_mut() = format!("{}(×{})", tail.gate(), count - 1);
        if let Operation::Unitary(u) = &mut tail {
            // Merge targets and controls into targets; clear controls
            u.targets = merge_unitary_registers(&[base.clone()]);
            u.controls.clear();
        } else {
            warn!("merging targets/controls is only implemented for unitaries");
        }

        children.push(tail);
    }

    let child_columns: ComponentGrid = vec![ComponentColumn {
        components: children,
    }];
    match &mut parent {
        Operation::Measurement(m) => {
            warn!("collapsing repeated measurements may not be correct");
            m.children = child_columns;
            m.gate = format!("{}(×{})", m.gate, count);
        }
        Operation::Unitary(u) => {
            u.children = child_columns;
            u.gate = format!("{}(×{})", u.gate, count);
            // Merge targets and controls into targets; clear controls
            let mut seen: FxHashSet<(usize, Option<usize>)> = FxHashSet::default();
            let mut merged: Vec<Register> = Vec::new();
            for r in u.targets.iter().chain(u.controls.iter()) {
                let key = (r.qubit, r.result);
                if seen.insert(key) {
                    merged.push(r.clone());
                }
            }
            u.targets = merged;
            u.controls.clear();
        }
        Operation::Ket(k) => {
            warn!("collapsing repeated kets may not be correct");
            k.children = child_columns;
            k.gate = format!("{}(×{})", k.gate, count);
        }
    }
    parent
}

/// Counts how many times a motif repeats starting at a given position.
fn count_motif_repeats(hashes: &[u64], start_pos: usize, motif_len: usize) -> usize {
    let len = hashes.len();
    let mut repeats = 1usize;

    'outer: loop {
        let start_next = start_pos + repeats * motif_len;
        let end_next = start_next + motif_len;
        if end_next > len {
            break;
        }
        for k in 0..motif_len {
            if hashes[start_pos + k] != hashes[start_next + k] {
                break 'outer;
            }
        }
        repeats += 1;
    }

    repeats
}

/// Finds the best repeating motif starting at a given position.
fn find_best_motif(hashes: &[u64], start_pos: usize) -> (usize, usize) {
    let len = hashes.len();
    let remaining = len - start_pos;
    let mut best_motif_len = 1usize;
    let mut best_repeats = 1usize;
    let max_motif_len = (remaining / 2).max(1);

    for motif_len in 1..=max_motif_len {
        if motif_len * 2 > remaining {
            break;
        }

        let repeats = count_motif_repeats(hashes, start_pos, motif_len);

        if repeats > 1 {
            let total = repeats * motif_len;
            let best_total = best_repeats * best_motif_len;
            if total > best_total || (total == best_total && motif_len < best_motif_len) {
                best_motif_len = motif_len;
                best_repeats = repeats;
            }
        }
    }

    (best_motif_len, best_repeats)
}

/// Creates a label for a complex motif by joining gate names and truncating if necessary.
fn create_motif_label(operations: &[Operation], motif_len: usize) -> String {
    let motif_gates: Vec<String> = operations[..motif_len]
        .iter()
        .map(|op| match op {
            Operation::Measurement(m) => m.gate.clone(),
            Operation::Unitary(u) => u.gate.clone(),
            Operation::Ket(k) => k.gate.clone(),
        })
        .collect();

    let mut label_prefix = motif_gates.join(" ");
    if label_prefix.chars().count() > 5 {
        let truncated: String = label_prefix.chars().take(5).collect();
        label_prefix = format!("{truncated}...");
    }
    label_prefix
}

/// Merges targets and controls from repeated unitary operations.
fn merge_unitary_registers(repeated_slice: &[Operation]) -> Vec<Register> {
    let mut seen: FxHashSet<(usize, Option<usize>)> = FxHashSet::default();
    let mut merged = Vec::new();

    for op in repeated_slice {
        if let Operation::Unitary(child) = op {
            for r in child.targets.iter().chain(child.controls.iter()) {
                let key = (r.qubit, r.result);
                if seen.insert(key) {
                    merged.push(r.clone());
                }
            }
        }
    }

    merged
}

/// Merges targets from repeated ket operations.
fn merge_ket_targets(repeated_slice: &[Operation]) -> Vec<Register> {
    let mut tgt_seen: FxHashSet<(usize, Option<usize>)> = FxHashSet::default();
    let mut new_targets = Vec::new();

    for op in repeated_slice {
        if let Operation::Ket(child) = op {
            for r in &child.targets {
                let key = (r.qubit, r.result);
                if tgt_seen.insert(key) {
                    new_targets.push(r.clone());
                }
            }
        }
    }

    new_targets
}

/// Creates a parent operation for a complex motif (length > 1).
fn make_complex_motif_parent(
    base: &Operation,
    repeated_slice: &[Operation],
    motif_len: usize,
    repeats: usize,
) -> Operation {
    let label_prefix = create_motif_label(repeated_slice, motif_len);
    let mut children = Vec::with_capacity(repeats * motif_len);
    let mut tail_children = vec![];

    for (i, op) in repeated_slice.iter().enumerate() {
        if i < motif_len {
            children.push(op.clone());
        } else {
            tail_children.push(op.clone());
        }
    }

    if !tail_children.is_empty() {
        let mut tail = base.clone();
        *tail.children_mut() = vec![ComponentColumn {
            components: tail_children,
        }];
        *tail.gate_mut() = format!("{}(×{})", tail.gate(), repeats - 1);
        if let Operation::Unitary(u) = &mut tail {
            // Merge targets and controls into targets; clear controls
            u.targets = merge_unitary_registers(repeated_slice);
            u.controls.clear();
        } else {
            warn!("collapsing repeated measurements/kets may not be correct");
        }

        children.push(tail);
    }

    let mut parent = base.clone();
    match &mut parent {
        Operation::Measurement(m) => {
            m.children = vec![ComponentColumn {
                components: children,
            }];
            m.gate = format!("{label_prefix}(×{repeats})");
        }
        Operation::Unitary(u) => {
            u.children = vec![ComponentColumn {
                components: children,
            }];
            u.gate = format!("{label_prefix}(×{repeats})");
            u.targets = merge_unitary_registers(repeated_slice);
            u.controls.clear();
        }
        Operation::Ket(k) => {
            k.children = vec![ComponentColumn {
                components: children,
            }];
            k.gate = format!("{label_prefix}(×{repeats})");
            k.targets = merge_ket_targets(repeated_slice);
        }
    }

    parent
}

#[allow(clippy::needless_pass_by_value)]
fn collapse_repetition_base(operations: Vec<Operation>) -> Vec<Operation> {
    // Extended: detect repeating motifs of length > 1 as well (e.g. A B A B A B -> (A B)(3)).
    // Strategy: scan list; for each start index find longest total repeated sequence comprising
    // repeats (>1) of a motif whose operations are all the same variant type. Prefer the match
    // with the greatest total collapsed length; tie-breaker smaller motif length.
    let len = operations.len();
    let hashes = operations.iter().map(hash_operation).collect::<Vec<u64>>();
    let mut i = 0;
    let mut result: Vec<Operation> = Vec::new();

    while i < len {
        let (best_motif_len, best_repeats) = find_best_motif(&hashes, i);

        if best_repeats > 1 {
            let base = &operations[i];
            let parent = if best_motif_len == 1 {
                make_repeated_parent(base, best_repeats)
            } else {
                let repeated_slice = &operations[i..i + best_repeats * best_motif_len];
                make_complex_motif_parent(base, repeated_slice, best_motif_len, best_repeats)
            };

            result.push(parent);
            i += best_repeats * best_motif_len;
        } else {
            // No pattern; push single op
            result.push(operations[i].clone());
            i += 1;
        }
    }

    result
}

fn hash_operation(op: &Operation) -> u64 {
    let args = op.args();
    let non_metadata_args = args
        .iter()
        .filter(|arg| !arg.starts_with("metadata="))
        .collect::<Vec<_>>();

    let more = match op {
        Operation::Measurement(measurement) => {
            ("m", measurement.qubits.clone(), measurement.results.clone())
        }
        Operation::Unitary(unitary) => ("u", unitary.controls.clone(), unitary.targets.clone()),
        Operation::Ket(ket) => ("k", vec![], ket.targets.clone()),
    };
    let data = (
        op.gate(),
        op.is_adjoint(),
        op.is_controlled(),
        op.is_measurement(),
        non_metadata_args,
        op.children()
            .iter()
            .map(|child| {
                child
                    .components
                    .iter()
                    .map(hash_operation)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
        more,
    );
    // standard hash
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}

/// Groups qubits into a single register. Collapses operations accordingly.
#[must_use]
pub fn group_qubits(
    operations: Vec<Operation>,
    qubits: Vec<Qubit>,
    qubit_ids_to_group: &[usize],
) -> (Vec<Operation>, Vec<Qubit>) {
    let (qubit_map, new_qubits) = get_qubit_map(qubits, qubit_ids_to_group);

    assert!(qubit_map.values().collect::<FxHashSet<_>>().len() == 1);

    let new_operations = operations
        .into_iter()
        .map(|op| map_operation(qubit_ids_to_group, &qubit_map, op))
        .collect::<Vec<_>>();

    (new_operations, new_qubits)
}

fn map_operation(
    qubit_ids_to_group: &[usize],
    qubit_map: &FxHashMap<usize, usize>,
    mut op: Operation,
) -> Operation {
    for child_column in op.children_mut() {
        let children = &mut child_column.components;
        for child in children {
            *child = map_operation(qubit_ids_to_group, qubit_map, child.clone());
        }
    }

    let mut remapped_controls = vec![];
    let mut remapped_targets = vec![];
    let gate = match &mut op {
        Operation::Measurement(m) => {
            m.qubits = map_to_group(qubit_map, &mut remapped_controls, &m.qubits);
            m.results = map_to_group(qubit_map, &mut remapped_targets, &m.results);
            &mut m.gate
        }
        Operation::Unitary(u) => {
            u.targets = map_to_group(qubit_map, &mut remapped_targets, &u.targets);
            u.controls = map_to_group(qubit_map, &mut remapped_controls, &u.controls);

            if !remapped_controls.is_empty() && !remapped_targets.is_empty() {
                let new_id = qubit_map
                    .values()
                    .next()
                    .copied()
                    .expect("should be present");
                // remove from controls if it is also a target
                u.controls.retain(|r| r.qubit != new_id);
                u.gate = format!("C{}", u.gate);
            }
            &mut u.gate
        }
        Operation::Ket(k) => {
            k.targets = map_to_group(qubit_map, &mut remapped_targets, &k.targets);
            &mut k.gate
        }
    };

    if !remapped_controls.is_empty() || !remapped_targets.is_empty() {
        let remapped_qubit_idxs =
            remapped_qubit_indices(qubit_ids_to_group, &remapped_controls, &remapped_targets);
        *gate = format!("{gate} (q{remapped_qubit_idxs:?})");
    }

    op
}

fn map_to_group(
    qubit_map: &FxHashMap<usize, usize>,
    remapped_qubits: &mut Vec<usize>,
    registers: &[Register],
) -> Vec<Register> {
    registers
        .iter()
        .map(|r| {
            let new_id = qubit_map.get(&r.qubit);
            if let Some(new_id) = new_id {
                remapped_qubits.push(r.qubit);
                Register {
                    qubit: *new_id,
                    result: r.result,
                }
            } else {
                r.clone()
            }
        })
        .collect()
}

fn get_qubit_map(
    qubits: Vec<Qubit>,
    qubit_ids_to_group: &[usize],
) -> (FxHashMap<usize, usize>, Vec<Qubit>) {
    let mut qubit_map = FxHashMap::default();
    let mut group_idx: Option<usize> = None;
    let mut new_qubits: Vec<Qubit> = vec![];
    for q in qubits {
        if qubit_ids_to_group.contains(&q.id) {
            if let Some(group_idx) = group_idx {
                qubit_map.insert(q.id, group_idx);
                new_qubits[group_idx].num_results += q.num_results;
                if let Some(d) = q.declarations {
                    match &mut new_qubits[group_idx].declarations {
                        Some(v) => v.extend(d.clone()),
                        None => new_qubits[group_idx].declarations = Some(d.clone()),
                    }
                }
            } else {
                group_idx = Some(new_qubits.len());
                qubit_map.insert(q.id, new_qubits.len());
                new_qubits.push(Qubit {
                    id: q.id, // Use the first qubit's ID as the group ID
                    num_results: q.num_results,
                    declarations: q.declarations.clone(),
                });
            }
        } else {
            new_qubits.push(q.clone());
        }
    }
    (qubit_map, new_qubits)
}

fn remapped_qubit_indices(
    qubit_ids_to_group: &[usize],
    remapped_controls: &[usize],
    remapped_targets: &[usize],
) -> Vec<usize> {
    remapped_controls
        .iter()
        .chain(remapped_targets.iter())
        .map(|id| {
            qubit_ids_to_group
                .iter()
                .position(|&x| x == *id)
                .expect("should be present")
        })
        .collect::<Vec<_>>()
}
