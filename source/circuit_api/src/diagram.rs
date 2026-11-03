// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{AngleExpression, Circuit};
use qsc_circuit_store::{PackedOperation, ParamValue, StandardGate};

/// Chooses the column an instruction is drawn in.
///
/// An instruction goes in the earliest column that is at or after the next free
/// column of every qubit it touches, so independent instructions share a column
/// while an instruction can never be drawn to the left of an earlier one on the
/// same wire. Scanning left for the first unoccupied column would allow exactly
/// that, and would draw the circuit out of order.
fn column_for(frontier: &[usize], qubits: &[usize]) -> usize {
    qubits.iter().map(|q| frontier[*q]).max().unwrap_or(0)
}

/// How a standard gate is drawn in the diagram.
///
/// The diagram carries its own naming vocabulary, shared with the diagrams
/// produced for Q# programs, and its consumers match on those exact names. This
/// API names gates the way its Python users expect, so the two vocabularies have
/// to be translated rather than passed through, otherwise the same gate would be
/// drawn two different ways depending on which front end produced the circuit.
struct DiagramGate {
    /// Name in the diagram's vocabulary.
    name: &'static str,
    /// Whether the diagram draws this as the adjoint of `name`.
    is_adjoint: bool,
    /// How many leading qubit operands are controls rather than targets.
    controls: usize,
}

impl DiagramGate {
    const fn new(name: &'static str, is_adjoint: bool, controls: usize) -> Self {
        Self {
            name,
            is_adjoint,
            controls,
        }
    }
}

/// Translates a gate into the diagram's vocabulary.
///
/// Exhaustive on purpose, so that adding a gate to the store forces a decision
/// about how it is drawn instead of silently falling back to a generic box.
fn diagram_gate(gate: StandardGate) -> DiagramGate {
    match gate {
        StandardGate::I => DiagramGate::new("I", false, 0),
        StandardGate::X => DiagramGate::new("X", false, 0),
        StandardGate::Y => DiagramGate::new("Y", false, 0),
        StandardGate::Z => DiagramGate::new("Z", false, 0),
        StandardGate::H => DiagramGate::new("H", false, 0),
        StandardGate::S => DiagramGate::new("S", false, 0),
        StandardGate::SDag => DiagramGate::new("S", true, 0),
        StandardGate::SqrtX => DiagramGate::new("SX", false, 0),
        StandardGate::SqrtXDag => DiagramGate::new("SX", true, 0),
        StandardGate::T => DiagramGate::new("T", false, 0),
        StandardGate::TDag => DiagramGate::new("T", true, 0),
        // Controlled gates are drawn as the base gate with a control wire, which
        // is how the diagram shows CNOT and friends.
        StandardGate::Cx => DiagramGate::new("X", false, 1),
        StandardGate::Cy => DiagramGate::new("Y", false, 1),
        StandardGate::Cz => DiagramGate::new("Z", false, 1),
        StandardGate::Ccx => DiagramGate::new("X", false, 2),
        StandardGate::Swap => DiagramGate::new("SWAP", false, 0),
        StandardGate::Cswap => DiagramGate::new("SWAP", false, 1),
        StandardGate::Rx => DiagramGate::new("Rx", false, 0),
        StandardGate::Ry => DiagramGate::new("Ry", false, 0),
        StandardGate::Rz => DiagramGate::new("Rz", false, 0),
        StandardGate::Rxx => DiagramGate::new("Rxx", false, 0),
        StandardGate::Ryy => DiagramGate::new("Ryy", false, 0),
        StandardGate::Rzz => DiagramGate::new("Rzz", false, 0),
        StandardGate::Crx => DiagramGate::new("Rx", false, 1),
        StandardGate::Cry => DiagramGate::new("Ry", false, 1),
        StandardGate::Crz => DiagramGate::new("Rz", false, 1),
        // Measurements and preparations are drawn by their own operation kinds,
        // so the name here is only a fallback if they are ever drawn as boxes.
        StandardGate::MeasureX | StandardGate::MeasureY | StandardGate::MeasureZ => {
            DiagramGate::new("Measure", false, 0)
        }
        StandardGate::PrepareX => DiagramGate::new("+", false, 0),
        StandardGate::PrepareY => DiagramGate::new("i", false, 0),
        StandardGate::PrepareZ => DiagramGate::new("0", false, 0),
    }
}

impl Circuit {
    /// Renders the arguments of an instruction, resolving parameters to names.
    fn diagram_args(&self, instruction: &qsc_circuit_store::Instruction) -> Vec<String> {
        instruction
            .params
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|value| match value {
                ParamValue::Literal(literal) => format!("{literal}"),
                ParamValue::Parameter(id) => self.store.parameters()[id.0 as usize].name.clone(),
                ParamValue::Expr(id) => AngleExpression::extract(&self.store, *id)
                    .text()
                    .to_string(),
            })
            .collect()
    }

    /// Builds the diagram operation for one instruction.
    ///
    /// `results_per_qubit` counts the results already drawn on each qubit and is
    /// advanced when this instruction produces one.
    fn diagram_operation(
        &self,
        instruction: &qsc_circuit_store::Instruction,
        spanned: &[usize],
        results_per_qubit: &mut [usize],
    ) -> qsc_circuit::Operation {
        use qsc_circuit::{Ket, Measurement, Operation, Register, Unitary};

        let mut args = self.diagram_args(instruction);
        let standard = match &instruction.operation {
            PackedOperation::Standard(gate) => Some(*gate),
            _ => None,
        };

        // Operations that put a qubit into a known state are drawn as that
        // state. A reset is drawn the same way it is drawn for a Q# program.
        let ket_label = match &instruction.operation {
            PackedOperation::Reset => Some("0"),
            PackedOperation::Standard(
                gate @ (StandardGate::PrepareX | StandardGate::PrepareY | StandardGate::PrepareZ),
            ) => Some(diagram_gate(*gate).name),
            _ => None,
        };
        if let Some(label) = ket_label {
            return Operation::Ket(Ket {
                gate: label.to_string(),
                args,
                children: Vec::new(),
                targets: spanned.iter().map(|q| Register::quantum(*q)).collect(),
                metadata: None,
            });
        }

        if let Some(
            gate @ (StandardGate::MeasureX | StandardGate::MeasureY | StandardGate::MeasureZ),
        ) = standard
        {
            let qubit = spanned[0];
            let result = results_per_qubit[qubit];
            results_per_qubit[qubit] += 1;
            // The diagram has no basis concept, so a non-Z basis is recorded as
            // an argument rather than dropped.
            match gate {
                StandardGate::MeasureX => args.push("X basis".to_string()),
                StandardGate::MeasureY => args.push("Y basis".to_string()),
                _ => {}
            }
            // The diagram addresses results per qubit, so the circuit's own
            // classical bit index only survives as an argument.
            for clbit in self.store.clbits_of(instruction).unwrap_or(&[]) {
                let index = clbit.0;
                args.push(format!("c[{index}]"));
            }
            return Operation::Measurement(Measurement {
                gate: "Measure".to_string(),
                args,
                children: Vec::new(),
                qubits: spanned.iter().map(|q| Register::quantum(*q)).collect(),
                results: vec![Register::classical(qubit, result)],
                metadata: None,
            });
        }

        let drawn = match standard {
            Some(gate) => diagram_gate(gate),
            // Everything else is drawn as a plain box under its own name, which
            // is what the diagram does for operations it has no notation for.
            None => DiagramGate::new("", false, 0),
        };
        let name = if drawn.name.is_empty() {
            instruction.operation.name().to_string()
        } else {
            drawn.name.to_string()
        };
        let split = drawn.controls.min(spanned.len());
        Operation::Unitary(Unitary {
            gate: name,
            args,
            children: Vec::new(),
            targets: spanned[split..]
                .iter()
                .map(|q| Register::quantum(*q))
                .collect(),
            controls: spanned[..split]
                .iter()
                .map(|q| Register::quantum(*q).into())
                .collect(),
            is_adjoint: drawn.is_adjoint,
            is_conditional: false,
            metadata: None,
        })
    }

    /// Builds the diagram representation of this circuit.
    ///
    /// Lossy by design. The diagram ties a measurement result to the qubit that
    /// produced it, while this circuit addresses classical bits independently,
    /// so the classical bit index is carried in the operation arguments rather
    /// than in the diagram's own result addressing. Control-flow bodies are
    /// drawn as a labelled placeholder, because the diagram has no nesting that
    /// matches the block model.
    #[must_use]
    pub fn build_diagram(&self) -> qsc_circuit::Circuit {
        use qsc_circuit::{Circuit as Diagram, ComponentColumn, Operation, Qubit};

        let width = self.store.qubit_count() as usize;
        let mut results_per_qubit = vec![0usize; width];
        // The next column each qubit is free to be drawn in.
        let mut frontier = vec![0usize; width];
        let mut columns: Vec<Vec<Operation>> = Vec::new();

        for instruction in self.store.instructions() {
            let qubits = self.store.qubits_of(instruction).unwrap_or(&[]);

            // A control-flow construct spans every qubit, since its body may
            // touch any of them and the diagram cannot show the nesting.
            let spanned: Vec<usize> = if qubits.is_empty() {
                (0..width).collect()
            } else {
                qubits.iter().map(|q| q.0 as usize).collect()
            };

            let column = column_for(&frontier, &spanned);
            while columns.len() <= column {
                columns.push(Vec::new());
            }
            for qubit in &spanned {
                frontier[*qubit] = column + 1;
            }

            let operation = self.diagram_operation(instruction, &spanned, &mut results_per_qubit);
            columns[column].push(operation);
        }

        Diagram {
            qubits: (0..width)
                .map(|id| Qubit {
                    id,
                    num_results: results_per_qubit[id],
                    declarations: Vec::new(),
                })
                .collect(),
            component_grid: columns
                .into_iter()
                .map(|components| ComponentColumn { components })
                .collect(),
        }
    }
}
