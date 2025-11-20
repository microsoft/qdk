// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use qsc_fir::fir::PackageId;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use std::cmp::max;
use std::{
    fmt::{Display, Write},
    hash::Hash,
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

impl Circuit {
    #[must_use]
    pub fn display_basic(&self) -> impl Display {
        CircuitDisplay {
            circuit: self,
            render_locations: false,
            render_groups: false,
        }
    }
}

impl Display for Circuit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            CircuitDisplay {
                circuit: self,
                render_locations: true,
                render_groups: true,
            }
        )
    }
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

    pub fn gate_mut(&mut self) -> &mut String {
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

    /// Returns the source location for the operation.
    #[must_use]
    pub fn source_location(&self) -> &Option<SourceLocation> {
        match self {
            Operation::Measurement(m) => &m.source,
            Operation::Unitary(u) => &u.source,
            Operation::Ket(k) => &k.source,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Only if this operation represents a scope group.
    pub scope_location: Option<SourceLocation>,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub declarations: Vec<SourceLocation>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum SourceLocation {
    Resolved(ResolvedSourceLocation),
    #[serde(skip)]
    Unresolved(PackageOffset),
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub struct PackageOffset {
    pub package_id: PackageId,
    pub offset: u32,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ResolvedSourceLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

impl Display for ResolvedSourceLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.column)
    }
}

/// First by column, then by row.
type ObjectsByColumnAndRow = FxHashMap<usize, FxHashMap<i16, CircuitObject>>;

struct RowBuilder {
    wire: Wire,
    max_height_above_zero: u8,
    current_top_offset: u8,
    current_bottom_offset: u8,
    max_height_below_zero: u8,
    objects: ObjectsByColumnAndRow,
    next_column: usize,
    render_locations: bool,
}

#[derive(Clone)]
enum Wire {
    None, // TODO: abolish
    Qubit { label: String },
    Classical { start_column: Option<usize> },
}

#[derive(Debug)]
enum CircuitObject {
    Blank, // TODO: blank is silly, get rid of it
    Wire,
    WireCross,
    WireStart,
    DashedCross,
    Vertical,
    VerticalDashed,
    TopLeftCorner,
    TopRightCorner,
    Object(String),
}

impl RowBuilder {
    fn add_object_to_row_wire(&mut self, column: usize, object: &str) {
        self.add_to_row_wire(column, CircuitObject::Object(object.to_string()));
    }

    fn add_object_at_current_height(&mut self, column: usize, object: &str) {
        let obj = CircuitObject::Object(object.to_string());
        self.add_to_current_top(column, obj);
    }

    fn increment_current_top_offset(&mut self) {
        self.current_top_offset += 1;
        self.max_height_above_zero = max(self.max_height_above_zero, self.current_top_offset + 1);
    }

    fn increment_current_bottom_offset(&mut self) {
        self.current_bottom_offset += 1;
        self.max_height_below_zero =
            max(self.max_height_below_zero, self.current_bottom_offset + 1);
    }

    fn decrement_current_top_offset(&mut self) {
        if self.current_top_offset > 0 {
            self.current_top_offset -= 1;
        }
    }

    fn decrement_current_bottom_offset(&mut self) {
        if self.current_bottom_offset > 0 {
            self.current_bottom_offset -= 1;
        }
    }

    fn add_measurement(&mut self, column: usize, source: Option<&SourceLocation>) {
        let mut gate_label = String::from("M");
        if self.render_locations
            && let Some(SourceLocation::Resolved(loc)) = source
        {
            let _ = write!(&mut gate_label, "@{loc}");
        }
        self.add_to_row_wire(column, CircuitObject::Object(gate_label.clone()));
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

        if self.render_locations
            && let Some(SourceLocation::Resolved(loc)) = source
        {
            let _ = write!(&mut gate_label, "@{}:{}:{}", loc.file, loc.line, loc.column);
        }

        self.add_object_to_row_wire(column, gate_label.as_str());
    }

    fn add_vertical(&mut self, column: usize) {
        if !self.objects.contains_key(&column) {
            match self.wire {
                Wire::None => self.add_to_row_wire(column, CircuitObject::Vertical),
                Wire::Qubit { .. } => self.add_to_row_wire(column, CircuitObject::WireCross),
                Wire::Classical { start_column } => {
                    if start_column.is_some() {
                        self.add_to_row_wire(column, CircuitObject::WireCross);
                    } else {
                        self.add_to_row_wire(column, CircuitObject::Vertical);
                    }
                }
            }
        }
    }

    fn add_dashed_vertical(&mut self, column: usize) {
        if !self.objects.contains_key(&column) {
            match self.wire {
                Wire::None => self.add_to_row_wire(column, CircuitObject::VerticalDashed),
                Wire::Qubit { .. } => self.add_to_row_wire(column, CircuitObject::DashedCross),
                Wire::Classical { start_column } => {
                    if start_column.is_some() {
                        self.add_to_row_wire(column, CircuitObject::DashedCross);
                    } else {
                        self.add_to_row_wire(column, CircuitObject::VerticalDashed);
                    }
                }
            }
        }
    }

    fn start_classical(&mut self, column: usize) {
        self.add_to_row_wire(column, CircuitObject::WireStart);
        if let Wire::Classical { start_column } = &mut self.wire {
            start_column.replace(column);
        }
    }

    fn add_to_row_wire(&mut self, column: usize, circuit_object: CircuitObject) {
        let row_row = self.objects.entry(column).or_default();
        row_row.insert(0, circuit_object);
        self.next_column = column + 1;
    }

    fn add_to_current_top(&mut self, column: usize, obj: CircuitObject) {
        let row_row = self.objects.entry(column).or_default();
        row_row.insert(self.current_top_offset as i16, obj);
        self.next_column = column + 1;
    }

    fn add_to_current_bottom(&mut self, column: usize, obj: CircuitObject) {
        let row_row = self.objects.entry(column).or_default();
        row_row.insert(-(self.current_bottom_offset as i16), obj);
        self.next_column = column + 1;
    }

    fn expand_rows(mut self) -> Vec<Row> {
        let mut rows = Vec::new();

        // Do the rows above zero
        for height_offset_from_top in 1..self.max_height_above_zero {
            let mut row_objects = FxHashMap::default();
            for (column, column_objects) in &mut self.objects {
                if let Some(object) = column_objects.remove(&(height_offset_from_top as i16)) {
                    // if we encountered a box corner, we need to fill in the rest of the column
                    if matches!(
                        object,
                        CircuitObject::TopLeftCorner | CircuitObject::TopRightCorner
                    ) {
                        for row_below in height_offset_from_top..self.max_height_above_zero {
                            column_objects.insert(row_below as i16, CircuitObject::Vertical);
                        }
                        // Do zero as well
                        column_objects.insert(0, CircuitObject::Vertical);
                    }
                    row_objects.insert(*column, object);
                }
            }
            rows.push(Row {
                wire: Wire::None,
                objects: row_objects,
            });
        }

        // Do the wire row (row 0)
        let idx = 0;
        let mut row_objects = FxHashMap::default();
        for (column, column_objects) in &mut self.objects {
            if let Some(object) = column_objects.remove(&idx) {
                row_objects.insert(*column, object);
            }
        }
        rows.push(Row {
            wire: self.wire,
            objects: row_objects,
        });

        // Do the rows below zero
        for height_offset_from_bottom in (1..self.max_height_below_zero).rev() {
            let mut row_objects = FxHashMap::default();
            for (column, column_objects) in &mut self.objects {
                if let Some(object) = column_objects.remove(&-(height_offset_from_bottom as i16)) {
                    row_objects.insert(*column, object);
                }
            }
            rows.push(Row {
                wire: Wire::None,
                objects: row_objects,
            });
        }

        rows
    }
}

type ObjectsByColumn = FxHashMap<usize, CircuitObject>;

struct Row {
    wire: Wire,
    objects: ObjectsByColumn,
}

impl Row {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>, columns: &[Column]) -> std::fmt::Result {
        // Temporary string so we can trim whitespace at the end
        let mut s = String::new();
        match &self.wire {
            Wire::None => {
                for (column_index, column) in columns.iter().enumerate() {
                    let obj = self.objects.get(&column_index);
                    s.write_str(&column.fmt_object(obj))?;
                }
            }
            Wire::Qubit { label } => {
                s.write_str(&fmt_qubit_label(label))?;
                for (column_index, column) in columns.iter().enumerate().skip(1) {
                    let obj = self.objects.get(&column_index);

                    s.write_str(&column.fmt_object_on_qubit_wire(obj))?;
                }
            }
            Wire::Classical { start_column } => {
                for (column_index, column) in columns.iter().enumerate() {
                    let obj = self.objects.get(&column_index);

                    if let Some(start) = *start_column
                        && column_index > start
                    {
                        s.write_str(&column.fmt_object_on_classical_wire(obj))?;
                    } else {
                        s.write_str(&column.fmt_object(obj))?;
                    }
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
const TOP_LEFT_CORNER: [char; 3] = [' ', '┌', '─']; // "   ┌───"
const TOP_RIGHT_CORNER: [char; 3] = ['─', '┐', ' ']; // "───┐   "

/// "q_0  "
#[allow(clippy::doc_markdown)]
fn fmt_qubit_label(label: &str) -> String {
    let rest = MIN_COLUMN_WIDTH - 1;
    format!("{label: <rest$} ")
}

struct Column {
    column_width: usize,
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

    /// "   A   "
    fn fmt_on_blank(&self, obj: &str) -> String {
        let column_width = self.column_width;
        format!("{: ^column_width$}", format!(" {obj} "))
    }

    fn expand_template(&self, template: &[char; 3]) -> String {
        let half_width = self.column_width / 2;
        let left = template[0].to_string().repeat(half_width);
        let right = template[2].to_string().repeat(half_width);

        format!("{left}{}{right}", template[1])
    }

    fn fmt_object_on_classical_wire(&self, circuit_object: Option<&CircuitObject>) -> String {
        let circuit_object = circuit_object.unwrap_or(&CircuitObject::Wire);

        if let CircuitObject::Object(label) = circuit_object {
            return self.fmt_on_classical_wire(label.as_str());
        }

        let template = match circuit_object {
            CircuitObject::Wire => CLASSICAL_WIRE,
            CircuitObject::WireCross => CLASSICAL_WIRE_CROSS,
            CircuitObject::WireStart => CLASSICAL_WIRE_START,
            CircuitObject::DashedCross => CLASSICAL_WIRE_DASHED_CROSS,
            CircuitObject::Vertical
            | CircuitObject::VerticalDashed
            | CircuitObject::Blank
            | CircuitObject::TopLeftCorner
            | CircuitObject::TopRightCorner
            | CircuitObject::Object(_) => unreachable!(),
        };

        self.expand_template(&template)
    }

    fn fmt_object_on_qubit_wire(&self, circuit_object: Option<&CircuitObject>) -> String {
        let circuit_object = circuit_object.unwrap_or(&CircuitObject::Wire);
        if let CircuitObject::Object(label) = circuit_object {
            return self.fmt_on_qubit_wire(label.as_str());
        }

        let template = match circuit_object {
            CircuitObject::Wire => QUBIT_WIRE,
            CircuitObject::WireCross => QUBIT_WIRE_CROSS,
            CircuitObject::DashedCross => QUBIT_WIRE_DASHED_CROSS,
            CircuitObject::Vertical => QUBIT_WIRE_CROSS,
            CircuitObject::WireStart
            | CircuitObject::VerticalDashed
            | CircuitObject::Blank
            | CircuitObject::TopLeftCorner
            | CircuitObject::TopRightCorner
            | CircuitObject::Object(_) => unreachable!(),
        };

        self.expand_template(&template)
    }

    fn fmt_object(&self, circuit_object: Option<&CircuitObject>) -> String {
        let circuit_object = circuit_object.unwrap_or(&CircuitObject::Blank);
        if let CircuitObject::Object(label) = circuit_object {
            return self.fmt_on_blank(label.as_str());
        }

        let template = match circuit_object {
            CircuitObject::WireStart => CLASSICAL_WIRE_START,
            CircuitObject::Blank => BLANK,
            CircuitObject::Vertical => VERTICAL,
            CircuitObject::VerticalDashed => VERTICAL_DASHED,
            CircuitObject::TopLeftCorner => TOP_LEFT_CORNER,
            CircuitObject::TopRightCorner => TOP_RIGHT_CORNER,
            CircuitObject::Wire
            | CircuitObject::WireCross
            | CircuitObject::DashedCross
            | CircuitObject::Object(_) => unreachable!("unexpected object on blank row"),
        };

        self.expand_template(&template)
    }
}

struct CircuitDisplay<'a> {
    circuit: &'a Circuit,
    render_locations: bool,
    render_groups: bool,
}

impl Display for CircuitDisplay<'_> {
    /// Formats the circuit into a diagram.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Identify qubits that require gap rows
        let qubits_with_gap_row_below = self.identify_qubits_with_gap_rows();

        // Initialize rows for qubits and classical wires
        let (mut rows, register_to_row) = self.initialize_rows(&qubits_with_gap_row_below);

        // Add operations to the diagram
        self.add_operations_to_rows(1, &self.circuit.component_grid, &mut rows, &register_to_row);

        // Finalize the diagram by extending wires and formatting columns
        let columns = finalize_columns(&rows);

        // Draw the diagram
        for row in rows {
            let subrows = row.expand_rows();
            for subrow in subrows {
                subrow.fmt(f, &columns)?;
            }
        }

        Ok(())
    }
}

impl CircuitDisplay<'_> {
    /// Identifies qubits that require gap rows for multi-qubit operations.
    fn identify_qubits_with_gap_rows(&self) -> FxHashSet<usize> {
        // Keep track of which qubits have the qubit after them in the same multi-qubit operation,
        // because those qubits need to get a gap row below them.
        let mut qubits_with_gap_row_below = FxHashSet::default();

        for col in &self.circuit.component_grid {
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
        qubits_with_gap_row_below
    }

    /// Initializes rows for qubits and classical wires.
    fn initialize_rows(
        &self,
        qubits_with_gap_row_below: &FxHashSet<usize>,
    ) -> (Vec<RowBuilder>, FxHashMap<(usize, Option<usize>), usize>) {
        // Maintain a mapping from from Registers in the Circuit schema
        // to row in the diagram
        let mut register_to_row = FxHashMap::default();

        let mut rows = vec![];
        for q in &self.circuit.qubits {
            let mut label = format!("q_{}", q.id);
            if self.render_locations {
                let mut first = true;
                for loc in &q.declarations {
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
            }

            rows.push(RowBuilder {
                wire: Wire::Qubit { label },
                max_height_above_zero: 1,
                max_height_below_zero: 0,
                current_top_offset: 0,
                current_bottom_offset: 0,
                objects: FxHashMap::default(),
                next_column: 1,
                render_locations: self.render_locations,
            });

            // associate this qubit register with this row
            register_to_row.insert((q.id, None), rows.len() - 1);

            for i in 0..q.num_results {
                rows.push(RowBuilder {
                    wire: Wire::Classical { start_column: None },
                    max_height_above_zero: 1,
                    max_height_below_zero: 0,
                    current_top_offset: 0,
                    current_bottom_offset: 0,
                    objects: FxHashMap::default(),
                    next_column: 1,
                    render_locations: self.render_locations,
                });

                // associate this result register with this row
                register_to_row.insert((q.id, Some(i)), rows.len() - 1);
            }

            let qubit_bottom_padding = q.num_results;

            // If this qubit has no result wires, but it is in a multi-qubit operation with
            // the next qubit, we add an empty row to make room for the vertical connector.
            if qubits_with_gap_row_below.contains(&q.id) && qubit_bottom_padding == 0 {
                rows.push(RowBuilder {
                    wire: Wire::None,
                    max_height_above_zero: 1,
                    current_top_offset: 0,
                    current_bottom_offset: 0,
                    max_height_below_zero: 0,
                    objects: FxHashMap::default(),
                    next_column: 1,
                    render_locations: self.render_locations,
                });
            }
        }

        (rows, register_to_row)
    }

    /// Adds operations to the diagram.
    fn add_operations_to_rows(
        &self,
        start_column: usize,
        component_grid: &ComponentGrid,
        rows: &mut [RowBuilder],
        register_to_row: &FxHashMap<(usize, Option<usize>), usize>,
    ) -> usize {
        let mut column = start_column;
        let mut next_column = start_column;
        for col in component_grid {
            for op in &col.components {
                let target_rows = get_row_indexes(op, register_to_row, true);
                let control_rows = get_row_indexes(op, register_to_row, false);

                let mut all_rows = target_rows.clone();
                all_rows.extend(control_rows.iter());
                all_rows.sort_unstable();

                // We'll need to know the entire range of rows for this operation so we can
                // figure out the starting column and also so we can draw any
                // vertical lines that cross wires.
                let (begin, end) = all_rows.split_first().map_or((0, 0), |(first, tail)| {
                    (*first, tail.last().unwrap_or(first) + 1)
                });

                let children = op.children();
                if children.is_empty() {
                    add_operation_to_rows(
                        op,
                        rows,
                        &target_rows,
                        &control_rows,
                        column,
                        begin,
                        end,
                    );
                    next_column = max(next_column, column + 1);
                } else {
                    let mut all_registers = registers(op, true);
                    all_registers.extend(registers(op, false));

                    let mut offset = 0;
                    if self.render_groups {
                        add_box_start_to_rows(
                            op,
                            rows,
                            &all_registers,
                            register_to_row,
                            column + offset,
                            // begin,
                            // end,
                        );
                        offset += 1;
                    }
                    offset += self.add_operations_to_rows(
                        column + offset,
                        children,
                        rows,
                        register_to_row,
                    );
                    if self.render_groups {
                        add_box_end_to_rows(
                            op,
                            rows,
                            &all_registers,
                            register_to_row,
                            column + offset,
                        );
                        offset += 1;
                    }
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
    rows: &mut [RowBuilder],
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
                row.add_object_to_row_wire(column, "●");
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

fn add_box_start_to_rows(
    operation: &Operation,
    rows: &mut [RowBuilder],
    registers: &[Register],
    register_to_row: &FxHashMap<(usize, Option<usize>), usize>,
    column: usize,
    // begin: usize,
    // end: usize,
) {
    assert!(
        !operation.children().is_empty(),
        "must only be called for an operation with children"
    );
    assert!(
        !operation.is_controlled(),
        "rendering controlled boxes not supported"
    );
    assert!(
        !operation.is_measurement(),
        "rendering measurement boxes not supported"
    );

    let mut qubits_with_box_padding_bottom = FxHashSet::default();
    let mut qubits_with_box_padding_top = FxHashSet::default();

    for reg in registers {
        let qubit = reg.qubit;
        let next_qubit = qubit + 1;
        let prev_qubit = qubit.wrapping_sub(1);

        if qubits_with_box_padding_bottom.contains(&qubit) {
            continue;
        }

        if registers.iter().any(|t| t.qubit == next_qubit) {
            qubits_with_box_padding_bottom.insert(qubit);
        }

        if registers.iter().any(|t| t.qubit == prev_qubit) {
            qubits_with_box_padding_top.insert(qubit);
        }
    }

    let mut registers = registers.to_vec();
    registers.sort_unstable_by_key(|r| (r.qubit, r.result));

    // Split into groups of consecutive registers
    let mut groups: Vec<Vec<Register>> = vec![];
    let mut current_group: Vec<Register> = vec![];
    for reg in &registers {
        if let Some(last_reg) = current_group.last() {
            if reg.qubit == last_reg.qubit || reg.qubit == last_reg.qubit + 1 {
                current_group.push(reg.clone());
            } else {
                groups.push(current_group);
                current_group = vec![reg.clone()];
            }
        } else {
            current_group.push(reg.clone());
        }
    }
    if !current_group.is_empty() {
        groups.push(current_group);
    }

    if groups.len() > 1 {
        for group in &groups {
            add_box_start_to_rows(operation, rows, &group, register_to_row, column);
        }
        // add dashed vertical lines between groups
        for i in 0..(groups.len() - 1) {
            let last_reg_of_group = groups[i].last().unwrap();
            let next_reg_of_group = groups[i + 1].first().unwrap();
            let last_row = *register_to_row
                .get(&(last_reg_of_group.qubit, last_reg_of_group.result))
                .expect("register must map to a row");
            let next_row = *register_to_row
                .get(&(next_reg_of_group.qubit, next_reg_of_group.result))
                .expect("register must map to a row");
            for row in &mut rows[(last_row + 1)..next_row] {
                row.add_dashed_vertical(column + 1);
            }
        }
        return;
    }

    let first = *register_to_row
        .get(&(
            registers.first().unwrap().qubit,
            registers.first().unwrap().result,
        ))
        .expect("register must map to a row");
    let last = *register_to_row
        .get(&(
            registers.last().unwrap().qubit,
            registers.last().unwrap().result,
        ))
        .expect("register must map to a row");

    // Add the vertical line for the box start
    rows[first].increment_current_top_offset();
    rows[last].increment_current_bottom_offset();
    add_vertical_box_border(rows, column, first, last, true);

    // Add label to the first row
    let gate_label = group_label(operation);
    rows[first].add_object_at_current_height(column + 1, gate_label.as_str());
}

fn add_vertical_box_border(
    rows: &mut [RowBuilder],
    column: usize,
    first_row: usize,
    last_row: usize,
    is_start: bool,
) {
    let top = if is_start {
        CircuitObject::TopLeftCorner
    } else {
        CircuitObject::TopRightCorner
    };
    let bottom = if is_start { "└" } else { "┘" };
    rows[first_row].add_to_current_top(column, top);
    for row in &mut rows[(first_row)..(last_row)] {
        row.add_vertical(column);
    }
    rows[last_row].add_to_current_bottom(column, CircuitObject::Object(bottom.to_string()));
}

fn group_label(operation: &Operation) -> String {
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
    gate_label
}

fn add_box_end_to_rows(
    operation: &Operation,
    rows: &mut [RowBuilder],
    registers: &[Register],
    register_to_row: &FxHashMap<(usize, Option<usize>), usize>,
    column: usize,
) {
    assert!(
        !operation.children().is_empty(),
        "must only be called for an operation with children"
    );

    let mut registers = registers.to_vec();
    registers.sort_unstable_by_key(|r| (r.qubit, r.result));

    // Split into groups of consecutive registers
    let mut groups: Vec<Vec<Register>> = vec![];
    let mut current_group: Vec<Register> = vec![];
    for reg in &registers {
        if let Some(last_reg) = current_group.last() {
            if reg.qubit == last_reg.qubit || reg.qubit == last_reg.qubit + 1 {
                current_group.push(reg.clone());
            } else {
                groups.push(current_group);
                current_group = vec![reg.clone()];
            }
        } else {
            current_group.push(reg.clone());
        }
    }
    if !current_group.is_empty() {
        groups.push(current_group);
    }

    if groups.len() > 1 {
        for group in &groups {
            add_box_end_to_rows(operation, rows, &group, register_to_row, column);
        }
        return;
    }

    let first = *register_to_row
        .get(&(
            registers.first().unwrap().qubit,
            registers.first().unwrap().result,
        ))
        .expect("register must map to a row");
    let last = *register_to_row
        .get(&(
            registers.last().unwrap().qubit,
            registers.last().unwrap().result,
        ))
        .expect("register must map to a row");

    // Add the vertical line for the box start
    add_vertical_box_border(rows, column, first, last, false);
    rows[first].decrement_current_top_offset();
    rows[last].decrement_current_bottom_offset();
}

/// Finalizes the columns by calculating their widths.
fn finalize_columns(rows: &[RowBuilder]) -> Vec<Column> {
    // Find the end column for the whole circuit so that
    // all qubit wires will extend until the end
    let end_column = rows
        .iter()
        .max_by_key(|r| r.next_column)
        .map_or(1, |r| r.next_column);

    let longest_qubit_label = rows
        .iter()
        .map(|r| {
            if let Wire::Qubit { label } = &r.wire {
                label.len()
            } else {
                0
            }
        })
        .chain(std::iter::once(MIN_COLUMN_WIDTH))
        .max()
        .unwrap_or_default();

    // To be able to fit long-named operations, we calculate the required width for each column,
    // based on the maximum length needed for gates, where a gate X is printed as "- X -".
    std::iter::once(longest_qubit_label)
        .chain((1..end_column).map(|column| {
            rows.iter()
                .filter_map(|row| row.objects.get(&column))
                .flat_map(|row_row| row_row.values())
                .filter_map(|object| match object {
                    CircuitObject::Object(string) => Some(string.len() + 4),
                    _ => None,
                })
                .chain(std::iter::once(MIN_COLUMN_WIDTH))
                .max()
                .expect("Column width should be at least 1")
        }))
        .map(Column::new)
        .collect()
}

/// Gets the row indexes for the targets or controls of an operation.
fn get_row_indexes(
    operation: &Operation,
    register_to_row: &FxHashMap<(usize, Option<usize>), usize>,
    is_target: bool,
) -> Vec<usize> {
    let registers = registers(operation, is_target);

    registers
        .into_iter()
        .filter_map(|reg| {
            let reg = (reg.qubit, reg.result);
            register_to_row.get(&reg).copied()
        })
        .collect()
}

fn registers(operation: &Operation, is_target: bool) -> Vec<Register> {
    match operation {
        Operation::Measurement(m) => {
            if is_target {
                m.results.clone()
            } else {
                m.qubits.clone()
            }
        }
        Operation::Unitary(u) => {
            if is_target {
                u.targets.clone()
            } else {
                u.controls.clone()
            }
        }
        Operation::Ket(k) => {
            if is_target {
                k.targets.clone()
            } else {
                vec![]
            }
        }
    }
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
pub fn operation_list_to_grid(mut operations: Vec<Operation>, num_qubits: usize) -> ComponentGrid {
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
                    m.children =
                        operation_list_to_grid(m.children.remove(0).components, num_qubits);
                }
                Operation::Unitary(u) => {
                    u.children =
                        operation_list_to_grid(u.children.remove(0).components, num_qubits);
                }
                Operation::Ket(k) => {
                    k.children =
                        operation_list_to_grid(k.children.remove(0).components, num_qubits);
                }
            }
        }
    }

    // Convert the operations into a component grid
    let mut component_grid = vec![];
    for col in remove_padding(operation_list_to_padded_array(operations, num_qubits)) {
        let column = ComponentColumn { components: col };
        component_grid.push(column);
    }
    component_grid
}

/// Converts a list of operations into a padded 2D array of operations.
///
/// # Arguments
///
/// * `operations` - A vector of operations to be converted.
/// * `num_qubits` - The number of qubits in the circuit.
///
/// # Returns
///
/// A 2D vector of optional operations padded with `None`.
fn operation_list_to_padded_array(
    operations: Vec<Operation>,
    num_qubits: usize,
) -> Vec<Vec<Option<Operation>>> {
    if operations.is_empty() {
        return vec![];
    }

    let grouped_ops = group_operations(&operations, num_qubits);
    let aligned_ops = transform_to_col_row(align_ops(grouped_ops));

    // Need to convert to optional operations so we can
    // take operations out without messing up the indexing
    let mut operations = operations.into_iter().map(Some).collect::<Vec<_>>();
    aligned_ops
        .into_iter()
        .map(|col| {
            col.into_iter()
                .map(|op_idx| op_idx.and_then(|idx| operations[idx].take()))
                .collect()
        })
        .collect()
}

/// Removes padding (`None` values) from a 2D array of operations.
///
/// # Arguments
///
/// * `operations` - A 2D vector of optional operations padded with `None`.
///
/// # Returns
///
/// A 2D vector of operations without `None` values.
fn remove_padding(operations: Vec<Vec<Option<Operation>>>) -> Vec<Vec<Operation>> {
    operations
        .into_iter()
        .map(|col| col.into_iter().flatten().collect())
        .collect()
}

/// Transforms a row-col 2D array into an equivalent col-row 2D array.
///
/// # Arguments
///
/// * `aligned_ops` - A 2D vector of optional usize values in row-col format.
///
/// # Returns
///
/// A 2D vector of optional usize values in col-row format.
fn transform_to_col_row(aligned_ops: Vec<Vec<Option<usize>>>) -> Vec<Vec<Option<usize>>> {
    if aligned_ops.is_empty() {
        return vec![];
    }

    let num_rows = aligned_ops.len();
    let num_cols = aligned_ops
        .iter()
        .map(std::vec::Vec::len)
        .max()
        .unwrap_or(0);

    let mut col_row_array = vec![vec![None; num_rows]; num_cols];

    for (row, row_data) in aligned_ops.into_iter().enumerate() {
        for (col, value) in row_data.into_iter().enumerate() {
            col_row_array[col][row] = value;
        }
    }

    col_row_array
}

/// Groups operations by their respective registers.
///
/// # Arguments
///
/// * `operations` - A slice of operations to be grouped.
/// * `num_qubits` - The number of qubits in the circuit.
///
/// # Returns
///
/// A 2D vector of indices where `groupedOps[i][j]` is the index of the operations
/// at register `i` and column `j` (not yet aligned/padded).
fn group_operations(operations: &[Operation], num_qubits: usize) -> Vec<Vec<usize>> {
    let mut grouped_ops = vec![vec![]; num_qubits];

    let max_q_id = match num_qubits {
        0 => 0,
        _ => num_qubits - 1,
    };

    for (instr_idx, op) in operations.iter().enumerate() {
        let ctrls = match op {
            Operation::Measurement(m) => &m.qubits,
            Operation::Unitary(u) => &u.controls,
            Operation::Ket(_) => &vec![],
        };
        let targets = match op {
            Operation::Measurement(m) => &m.results,
            Operation::Unitary(u) => &u.targets,
            Operation::Ket(k) => &k.targets,
        };
        let q_regs: Vec<_> = ctrls
            .iter()
            .chain(targets)
            .filter(|reg| !reg.is_classical())
            .collect();
        let q_reg_idx_list: Vec<_> = q_regs.iter().map(|reg| reg.qubit).collect();
        let cls_controls: Vec<_> = ctrls.iter().filter(|reg| reg.is_classical()).collect();
        let is_classically_controlled = !cls_controls.is_empty();

        if !is_classically_controlled && q_regs.is_empty() {
            continue;
        }

        let (min_reg_idx, max_reg_idx) = if is_classically_controlled {
            (0, max_q_id)
        } else {
            q_reg_idx_list
                .into_iter()
                .fold(None, |acc, x| match acc {
                    None => Some((x, x)),
                    Some((min, max)) => Some((min.min(x), max.max(x))),
                })
                .unwrap_or((0, max_q_id))
        };

        for reg_ops in grouped_ops
            .iter_mut()
            .take(max_reg_idx + 1)
            .skip(min_reg_idx)
        {
            reg_ops.push(instr_idx);
        }
    }

    grouped_ops
}

/// Aligns operations by padding registers with `None` to make sure that multiqubit
/// gates are in the same column.
///
/// # Arguments
///
/// * `ops` - A 2D vector of usize values representing the operations.
///
/// # Returns
///
/// A 2D vector of optional usize values representing the aligned operations.
fn align_ops(ops: Vec<Vec<usize>>) -> Vec<Vec<Option<usize>>> {
    let mut max_num_ops = ops.iter().map(std::vec::Vec::len).max().unwrap_or(0);
    let mut col = 0;
    let mut padded_ops: Vec<Vec<Option<usize>>> = ops
        .into_iter()
        .map(|reg_ops| reg_ops.into_iter().map(Some).collect())
        .collect();

    while col < max_num_ops {
        for reg_idx in 0..padded_ops.len() {
            if padded_ops[reg_idx].len() <= col {
                continue;
            }

            // Represents the gate at padded_ops[reg_idx][col]
            let op_idx = padded_ops[reg_idx][col];

            // The vec of where in each register the gate appears
            let targets_pos: Vec<_> = padded_ops
                .iter()
                .map(|reg_ops| reg_ops.iter().position(|&x| x == op_idx))
                .collect();
            // The maximum column index of the gate in the target registers
            let gate_max_col = targets_pos
                .iter()
                .filter_map(|&pos| pos)
                .max()
                .unwrap_or(usize::MAX);

            if col < gate_max_col {
                padded_ops[reg_idx].insert(col, None);
                max_num_ops = max_num_ops.max(padded_ops[reg_idx].len());
            }
        }
        col += 1;
    }

    padded_ops
}
