// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use core::panic;
use log::warn;
use qsc_fir::fir::PackageId;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use std::slice::from_ref;
use std::{cmp::max, hash::Hasher};
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

    #[must_use]
    pub fn display_with_groups(&self) -> impl Display {
        // Groups rendered only in tests since the current line rendering
        // doesn't look good enough to be user-facing.
        CircuitDisplay {
            circuit: self,
            render_locations: true,
            render_groups: true,
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
    pub fn source_location(&self) -> Option<&SourceLocation> {
        match self {
            Self::Measurement(measurement) => measurement.metadata.as_ref(),
            Self::Unitary(unitary) => unitary.metadata.as_ref(),
            Self::Ket(ket) => ket.metadata.as_ref(),
        }
        .and_then(|m| m.source.as_ref())
    }

    #[must_use]
    pub fn source_location_mut(&mut self) -> &mut Option<SourceLocation> {
        let md = match self {
            Self::Measurement(measurement) => &mut measurement.metadata,
            Self::Unitary(unitary) => &mut unitary.metadata,
            Self::Ket(ket) => &mut ket.metadata,
        };

        if md.is_none() {
            md.replace(Metadata {
                source: None,
                scope_location: None,
            });
        }

        if let Some(md) = md {
            &mut md.source
        } else {
            unreachable!()
        }
    }

    #[must_use]
    pub fn scope_location_mut(&mut self) -> &mut Option<SourceLocation> {
        let md = match self {
            Self::Measurement(measurement) => &mut measurement.metadata,
            Self::Unitary(unitary) => &mut unitary.metadata,
            Self::Ket(ket) => &mut ket.metadata,
        };

        if md.is_none() {
            md.replace(Metadata {
                source: None,
                scope_location: None,
            });
        }

        if let Some(md) = md {
            &mut md.scope_location
        } else {
            unreachable!()
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

    #[must_use]
    pub fn targets_mut(&mut self) -> &mut Vec<Register> {
        match self {
            Operation::Measurement(m) => &mut m.qubits,
            Operation::Unitary(u) => &mut u.targets,
            Operation::Ket(k) => &mut k.targets,
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
    pub metadata: Option<Metadata>,
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
    pub metadata: Option<Metadata>,
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
    pub metadata: Option<Metadata>,
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
#[serde(rename_all = "camelCase")]
/// The schema of `Metadata` may change and its contents
/// are never meant to be persisted in a .qsc file.
pub struct Metadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The location in the source code that this operation originated from.
    pub source: Option<SourceLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Only populated if this operation represents a scope group.
    pub scope_location: Option<SourceLocation>,
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
    max_depth_above_axis: u8,
    current_top_offset: u8,
    current_bottom_offset: u8,
    max_depth_below_axis: u8,
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

#[derive(Debug, Clone)]
enum CircuitObject {
    Blank, // TODO: blank is silly, get rid of it
    Wire,
    WireStart,
    Vertical,
    VerticalDashed,
    Horizontal,
    TopLeftCorner,
    TopRightCorner,
    BottomLeftCorner,
    BottomRightCorner,
    Object(String),
    GroupLabel(String),
}

impl RowBuilder {
    fn add_object_to_row_wire(&mut self, column: usize, object: &str) {
        self.add_to_row_wire(column, CircuitObject::Object(object.to_string()));
    }

    fn increment_current_top_offset(&mut self) {
        self.current_top_offset += 1;
        self.max_depth_above_axis = max(self.max_depth_above_axis, self.current_top_offset + 1);
    }

    fn increment_current_bottom_offset(&mut self) {
        self.current_bottom_offset += 1;
        self.max_depth_below_axis = max(self.max_depth_below_axis, self.current_bottom_offset + 1);
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

    fn add_gate(&mut self, column: usize, operation: &Operation) {
        let gate_label = self.operation_label(operation);

        self.add_object(column, gate_label.as_str());
    }

    fn operation_label(&self, operation: &Operation) -> String {
        let mut gate_label = String::new();
        gate_label.push_str(&operation.gate());
        if operation.is_adjoint() {
            gate_label.push('\'');
        }

        if !operation.args().is_empty() {
            let args = operation.args().join(", ");
            let _ = write!(&mut gate_label, "({args})");
        }

        if self.render_locations
            && let Some(SourceLocation::Resolved(loc)) = operation.source_location()
        {
            let _ = write!(&mut gate_label, "@{}:{}:{}", loc.file, loc.line, loc.column);
        }

        self.add_object_to_row_wire(column, gate_label.as_str());
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
        row_row.insert(i16::from(self.current_top_offset), obj);
        self.next_column = column + 1;
    }

    fn add_to_current_bottom(&mut self, column: usize, obj: CircuitObject) {
        let row_row = self.objects.entry(column).or_default();
        row_row.insert(-i16::from(self.current_bottom_offset), obj);
        self.next_column = column + 1;
    }

    fn expand_rows(mut self) -> Vec<Row> {
        for column_objects in self.objects.values_mut() {
            // Do the wire row (row 0)
            if let Some(object) = column_objects.get(&0) {
                // If we encountered a vertical, we need to fill in the rest of the column
                if matches!(object, CircuitObject::Vertical) {
                    for r in 0..self.max_depth_above_axis {
                        column_objects.insert(i16::from(r), CircuitObject::Vertical);
                    }
                    for r in 0..self.max_depth_below_axis {
                        column_objects.insert(-i16::from(r), CircuitObject::Vertical);
                    }
                }
            }

            let mut top_corner_height = None;
            let mut bottom_corner_height = None;
            // Do the rows above zero
            for height_offset_from_top in 1..self.max_depth_above_axis {
                if let Some(object) = column_objects.get(&i16::from(height_offset_from_top)) {
                    // if we encountered a box corner, we need to fill in the rest of the column
                    if matches!(
                        object,
                        CircuitObject::TopLeftCorner | CircuitObject::TopRightCorner
                    ) {
                        top_corner_height.replace(height_offset_from_top);
                    }
                }
            }

            // Do the rows below zero
            for height_offset_from_bottom in 1..self.max_depth_below_axis {
                if let Some(object) = column_objects.get(&-i16::from(height_offset_from_bottom)) {
                    // if we encountered a box corner, we need to fill in the rest of the column
                    if matches!(
                        object,
                        CircuitObject::BottomLeftCorner | CircuitObject::BottomRightCorner
                    ) {
                        bottom_corner_height.replace(height_offset_from_bottom);
                    }
                }
            }

            if let Some(top_corner_height) = top_corner_height {
                for r in (top_corner_height + 1)..self.max_depth_above_axis {
                    column_objects.insert(i16::from(r), CircuitObject::Vertical);
                }
                // Do zero as well
                column_objects.insert(0, CircuitObject::Vertical);

                if bottom_corner_height.is_none() {
                    // Do the rows below zero
                    for r in 1..self.max_depth_below_axis {
                        column_objects.insert(-i16::from(r), CircuitObject::Vertical);
                    }
                }
            }

            if let Some(bottom_corner_height) = bottom_corner_height {
                for r in bottom_corner_height + 1..self.max_depth_below_axis {
                    column_objects.insert(-i16::from(r), CircuitObject::Vertical);
                }
                // Do zero as well
                column_objects.insert(0, CircuitObject::Vertical);

                if top_corner_height.is_none() {
                    // Do the rows above zero
                    for r in 1..self.max_depth_above_axis {
                        column_objects.insert(i16::from(r), CircuitObject::Vertical);
                    }
                }
            }
        }

        let mut top_rows = Vec::new();
        // Do the rows above zero
        for height_offset_from_top in 1..self.max_depth_above_axis {
            let mut row_objects = FxHashMap::default();
            for (column, column_objects) in &mut self.objects {
                if let Some(object) = column_objects.remove(&i16::from(height_offset_from_top)) {
                    row_objects.insert(*column, object);
                }
            }
            top_rows.push(Row {
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
        let mid_row = Row {
            wire: self.wire,
            objects: row_objects,
        };

        let mut bottom_rows = vec![];
        // Do the rows below zero
        for height_offset_from_bottom in 1..self.max_depth_below_axis {
            let mut row_objects = FxHashMap::default();
            for (column, column_objects) in &mut self.objects {
                if let Some(object) = column_objects.remove(&-i16::from(height_offset_from_bottom))
                {
                    row_objects.insert(*column, object);
                }
            }
            bottom_rows.push(Row {
                wire: Wire::None,
                objects: row_objects,
            });
        }

        let mut rows = vec![];
        rows.extend(top_rows);
        rows.push(mid_row);

        bottom_rows.reverse();
        rows.extend(bottom_rows);

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
                s.write_str(&columns[0].fmt_qubit_label(label))?;
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
const BOTTOM_LEFT_CORNER: [char; 3] = [' ', '└', '─']; // "   └───"
const BOTTOM_RIGHT_CORNER: [char; 3] = ['─', '┘', ' ']; // "───┘   "

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

    /// "q_0  "
    #[allow(clippy::doc_markdown)]
    fn fmt_qubit_label(&self, label: &str) -> String {
        let column_width = self.column_width;
        let s = format!("{label:<column_width$}");
        s
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
            CircuitObject::Vertical => CLASSICAL_WIRE_CROSS,
            CircuitObject::WireStart => CLASSICAL_WIRE_START,
            CircuitObject::VerticalDashed => CLASSICAL_WIRE_DASHED_CROSS,
            o @ (CircuitObject::Blank
            | CircuitObject::TopLeftCorner
            | CircuitObject::TopRightCorner
            | CircuitObject::BottomLeftCorner
            | CircuitObject::BottomRightCorner
            | CircuitObject::Horizontal
            | CircuitObject::GroupLabel(_)) => {
                unreachable!("unexpected object on blank row: {o:?}")
            }
            CircuitObject::Object(_) => unreachable!("case should have been handled earlier"),
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
            CircuitObject::Vertical => QUBIT_WIRE_CROSS,
            CircuitObject::VerticalDashed => QUBIT_WIRE_DASHED_CROSS,
            CircuitObject::WireStart
            | CircuitObject::Blank
            | CircuitObject::TopLeftCorner
            | CircuitObject::TopRightCorner
            | CircuitObject::BottomLeftCorner
            | CircuitObject::BottomRightCorner
            | CircuitObject::Horizontal
            | CircuitObject::Object(_)
            | CircuitObject::GroupLabel(_) => unreachable!(),
        };

        self.expand_template(&template)
    }

    fn fmt_object(&self, circuit_object: Option<&CircuitObject>) -> String {
        let circuit_object = circuit_object.unwrap_or(&CircuitObject::Blank);
        if let CircuitObject::Object(label) = circuit_object {
            return self.fmt_on_blank(label.as_str());
        }

        if let CircuitObject::GroupLabel(label) = circuit_object {
            // Technically we're not on a qubit wire, but here we're
            // repurposing the qubit wire line character for the horizontal box line
            return self.fmt_on_qubit_wire(label.as_str());
        }

        let template = match circuit_object {
            CircuitObject::WireStart => CLASSICAL_WIRE_START,
            CircuitObject::Blank => BLANK,
            CircuitObject::Vertical => VERTICAL,
            CircuitObject::VerticalDashed => VERTICAL_DASHED,
            CircuitObject::TopLeftCorner => TOP_LEFT_CORNER,
            CircuitObject::TopRightCorner => TOP_RIGHT_CORNER,
            CircuitObject::Horizontal => QUBIT_WIRE,
            CircuitObject::BottomLeftCorner => BOTTOM_LEFT_CORNER,
            CircuitObject::BottomRightCorner => BOTTOM_RIGHT_CORNER,
            o @ CircuitObject::Wire => {
                unreachable!("unexpected object on blank row: {o:?}")
            }
            CircuitObject::Object(_) | CircuitObject::GroupLabel(_) => {
                unreachable!("case should have been handled earlier")
            }
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
        self.add_grid(1, &self.circuit.component_grid, &mut rows, &register_to_row);

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

type Rows = (Vec<RowBuilder>, FxHashMap<(usize, Option<usize>), usize>);

impl CircuitDisplay<'_> {
    /// Identifies qubits that require gap rows for multi-qubit operations.
    fn identify_qubits_with_gap_rows(&self) -> FxHashSet<usize> {
        // Keep track of which qubits have the qubit after them in the same multi-qubit operation,
        // because those qubits need to get a gap row below them.
        let mut qubits_with_gap_row_below = FxHashSet::default();

        for col in &self.circuit.component_grid {
            for op in &col.components {
                if !op.children().is_empty() {
                    continue;
                }
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
    fn initialize_rows(&self, qubits_with_gap_row_below: &FxHashSet<usize>) -> Rows {
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
                max_depth_above_axis: 1,
                max_depth_below_axis: 0,
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
                    max_depth_above_axis: 1,
                    max_depth_below_axis: 0,
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
                    max_depth_above_axis: 1,
                    current_top_offset: 0,
                    current_bottom_offset: 0,
                    max_depth_below_axis: 0,
                    objects: FxHashMap::default(),
                    next_column: 1,
                    render_locations: self.render_locations,
                });
            }
        }

        (rows, register_to_row)
    }

    fn add_grid(
        &self,
        start_column: usize,
        component_grid: &ComponentGrid,
        rows: &mut [RowBuilder],
        register_to_row: &FxHashMap<(usize, Option<usize>), usize>,
    ) -> usize {
        let mut curr_column = start_column;
        for column_operations in component_grid {
            let offset = self.add_column(rows, register_to_row, curr_column, column_operations);
            curr_column += offset;
        }
        curr_column - start_column
    }

    fn add_column(
        &self,
        rows: &mut [RowBuilder],
        register_to_row: &FxHashMap<(usize, Option<usize>), usize>,
        column: usize,
        col: &ComponentColumn,
    ) -> usize {
        let mut col_width = 0;
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

            if op.children().is_empty() {
                add_operation_to_rows(op, rows, &target_rows, &control_rows, column, begin, end);
                col_width = max(col_width, 1);
            } else {
                let offset = self.add_boxed_group(rows, register_to_row, column, op, op.children());
                col_width = max(col_width, offset);
            }
        }

        for column in column..(column + col_width) {
            for r in &mut *rows {
                if r.current_top_offset > 0 {
                    for o in 1..=r.current_top_offset {
                        r.objects
                            .entry(column)
                            .or_default()
                            .entry(i16::from(o))
                            .or_insert(CircuitObject::Horizontal);
                    }
                }

                if r.current_bottom_offset > 0 {
                    for o in 1..=r.current_bottom_offset {
                        r.objects
                            .entry(column)
                            .or_default()
                            .entry(-i16::from(o))
                            .or_insert(CircuitObject::Horizontal);
                    }
                }
            }
        }
        col_width
    }

    fn add_boxed_group(
        &self,
        rows: &mut [RowBuilder],
        register_to_row: &FxHashMap<(usize, Option<usize>), usize>,
        column: usize,
        op: &Operation,
        children: &Vec<ComponentColumn>,
    ) -> usize {
        assert!(
            !op.children().is_empty(),
            "must only be called for an operation with children"
        );
        // TODO: draw control lines
        // assert!(
        //     !op.is_controlled(),
        //     "rendering controlled boxes not supported"
        // );
        assert!(
            !op.is_measurement(),
            "rendering measurement boxes not supported"
        );

        let mut all_registers = registers(op, true);
        all_registers.extend(registers(op, false));

        let mut offset = 0;
        if self.render_groups {
            add_box_start(op, rows, &all_registers, register_to_row, column);
            offset += 1;
        }

        offset += self.add_grid(column + offset, children, rows, register_to_row);

        if self.render_groups {
            add_box_end(op, rows, &all_registers, register_to_row, column + offset);
            offset += 1;
        }
        offset
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
            row.add_gate(column, operation);
        }
    }

    if operation.is_controlled() || operation.is_measurement() {
        for i in controls {
            let row = &mut rows[*i];
            if matches!(row.wire, Wire::Qubit { .. }) && operation.is_measurement() {
                row.add_measurement(column, operation.source_location());
            } else {
                row.add_object_to_row_wire(column, "●");
            }
        }

        // If we have a control wire, draw vertical lines spanning all
        // control and target wires and crossing any in between
        // (vertical lines may overlap if there are multiple controls/targets,
        // this is ok in practice)
        #[allow(clippy::needless_range_loop)]
        for i in begin..end {
            let row = &mut rows[i];
            let existing = row
                .objects
                .get(&column)
                .cloned()
                .unwrap_or_default()
                .remove(&0);
            if let Some(existing) = existing {
                // TODO: this definitely doesn't work
                if let CircuitObject::Object(_) = existing {
                    if i == begin {
                        for sr in 1..=row.current_bottom_offset {
                            // add vertical to subrows below axis
                            let row_row = row.objects.entry(column).or_default();
                            row_row.insert(-i16::from(sr), CircuitObject::Vertical);
                        }
                    } else if i == end - 1 {
                        for sr in 1..=row.current_top_offset {
                            // add vertical to subrows above axis
                            let row_row = row.objects.entry(column).or_default();
                            row_row.insert(i16::from(sr), CircuitObject::Vertical);
                        }
                    } else {
                        // crossing wire, leave as is
                    }
                }
            } else {
                row.add_to_row_wire(column, CircuitObject::Vertical);
            }
        }
    } else {
        // No control wire. Draw dashed vertical lines to connect
        // target wires if there are multiple targets
        for row in &mut rows[begin..end] {
            if !row.objects.contains_key(&column) {
                row.add_to_row_wire(column, CircuitObject::VerticalDashed);
            }
        }
    }
}

fn add_box_start(
    operation: &Operation,
    rows: &mut [RowBuilder],
    registers: &[Register],
    register_to_row: &FxHashMap<(usize, Option<usize>), usize>,
    column: usize,
) {
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
            add_box_start(operation, rows, group, register_to_row, column);
        }
        // add dashed vertical lines between groups
        for i in 0..(groups.len() - 1) {
            let last_reg_of_group = groups[i]
                .last()
                .expect("group must have at least one register");
            let next_reg_of_group = groups[i + 1]
                .first()
                .expect("group must have at least one register");
            let last_row = *register_to_row
                .get(&(last_reg_of_group.qubit, last_reg_of_group.result))
                .expect("register must map to a row");
            let next_row = *register_to_row
                .get(&(next_reg_of_group.qubit, next_reg_of_group.result))
                .expect("register must map to a row");
            for row in &mut rows[(last_row + 1)..next_row] {
                // TODO: should this be column + 1?
                row.add_to_row_wire(column + 1, CircuitObject::VerticalDashed);
            }
        }
        return;
    }

    // Handle single group

    let first_register = registers
        .first()
        .expect("there should at least be one register in group");
    let last_register = registers
        .last()
        .expect("there should at least be one register in group");

    let first_row = *register_to_row
        .get(&(first_register.qubit, first_register.result))
        .expect("register must map to a row");

    let last_row = *register_to_row
        .get(&(last_register.qubit, last_register.result))
        .expect("register must map to a row");

    // Add the vertical line for the box start
    rows[first_row].increment_current_top_offset();
    rows[last_row].increment_current_bottom_offset();
    add_vertical_box_border(rows, column, first_row, last_row, true);

    // Add label to the top
    let label = group_label(operation);
    rows[first_row].add_to_current_top(column + 1, CircuitObject::GroupLabel(label));
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
    let bottom = if is_start {
        CircuitObject::BottomLeftCorner
    } else {
        CircuitObject::BottomRightCorner
    };
    rows[first_row].add_to_current_top(column, top);
    let second_from_top = first_row.saturating_add(1);
    let second_from_bottom = last_row.saturating_sub(1);
    if second_from_bottom >= second_from_top {
        for row in &mut rows[second_from_top..=second_from_bottom] {
            row.add_to_row_wire(column, CircuitObject::Vertical);
        }
    }
    rows[last_row].add_to_current_bottom(column, bottom);
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

    if let Some(SourceLocation::Resolved(loc)) = operation.source_location() {
        let _ = write!(&mut gate_label, "@{loc}");
    }

    gate_label.push(']');
    gate_label
}

fn add_box_end(
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
            add_box_end(operation, rows, group, register_to_row, column);
        }
        return;
    }

    let first = *register_to_row
        .get(&(
            registers
                .first()
                .expect("registers should not be empty")
                .qubit,
            registers
                .first()
                .expect("registers should not be empty")
                .result,
        ))
        .expect("register must map to a row");
    let last = *register_to_row
        .get(&(
            registers
                .last()
                .expect("registers should not be empty")
                .qubit,
            registers
                .last()
                .expect("registers should not be empty")
                .result,
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
                label.len() + 1
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
                    CircuitObject::Object(string) | CircuitObject::GroupLabel(string) => {
                        Some(string.len() + 4)
                    }
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
            u.targets = merge_unitary_registers(from_ref(base));
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
                new_qubits[group_idx].declarations.extend(q.declarations);
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
