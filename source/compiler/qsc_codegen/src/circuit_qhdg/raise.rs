// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Raising a QHDG graph back to a circuit.
//!
//! This direction is best-effort, and it is the only place in the conversion
//! where that is true. A snapshot of either representation restores exactly; a
//! crossing between them can lose, because the graph expresses programs the
//! circuit surface has no form for.
//!
//! The subset that raises is the one the lowering produces: straight-line
//! quantum operations over allocated qubits, with literal parameters and
//! measurement results stored to classical bits. Anything else returns a
//! diagnostic naming the construct rather than a bare failure, because the
//! subset boundary moves while the API is experimental and a caller needs to
//! know which side of it they landed on.
//!
//! Reading the graph is not done here. It is shared with code generation, so the
//! two cannot disagree about program order or qubit numbering. What is left is
//! the part that is genuinely about circuits: turning a graph operation name
//! into a circuit operation, and refusing the names that have none.

use std::collections::BTreeMap;

use qsc_circuit_store::{
    AngleExpr, AngleId, AngleOp as StoreAngleOp, BlockId, CircuitStore, Clbit, ControlFlow, Expr,
    IfElse, PackedOperation, ParamValue, ParameterId, Qubit, StandardGate,
};
use qsc_qhdg::{Angle, AngleOp, Graph};

use super::{
    BARRIER, CircuitOrigin, RESET, THETA, gate_from_name,
    reader::{ReadConditional, ReadError, ReadInstruction, ReadItem, read_program},
};

/// Why a graph could not be expressed as a circuit.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RaiseError {
    /// The graph holds an operation the circuit surface has no form for.
    ///
    /// This is the ordinary outside-the-subset case, and it names the construct
    /// so a caller can tell what to change.
    #[error("operation '{operation}' has no circuit form")]
    Unsupported { operation: String },
    /// The graph is not shaped like anything the lowering produces.
    #[error("the graph is not circuit-shaped: {reason}")]
    Malformed { reason: String },
    /// The recovered circuit was rejected by the store's own validation.
    #[error("the raised circuit was rejected: {0}")]
    Rejected(String),
}

impl From<ReadError> for RaiseError {
    fn from(error: ReadError) -> Self {
        match error {
            ReadError::Vocabulary { operation } => Self::Unsupported { operation },
            ReadError::Region { kind } => Self::Unsupported {
                operation: format!("{kind} region"),
            },
            ReadError::Malformed { reason } => Self::Malformed { reason },
        }
    }
}

/// A circuit recovered from a graph, with what each instruction came from.
///
/// The origins are positional: `origins[i]` describes `circuit.instruction(i)`.
/// A conditional is one instruction there, so its entry describes the whole
/// construct and the instructions inside its blocks are not indexed here.
/// An entry is `None` when the node the instruction came from carries no
/// circuit origin, which is what a graph that did not originate as a circuit,
/// or a node a pass introduced, looks like from here.
pub struct RaisedCircuit {
    pub circuit: CircuitStore,
    pub origins: Vec<Option<CircuitOrigin>>,
}

/// Raises a graph to a circuit when it lies inside the circuit-expressible subset.
///
/// # Errors
///
/// Returns [`RaiseError::Unsupported`] naming the construct when the graph holds
/// something the circuit cannot express, [`RaiseError::Malformed`] when the
/// graph is not shaped like a lowered circuit, and [`RaiseError::Rejected`] when
/// the recovered instructions fail the store's own validation.
pub fn graph_to_circuit(graph: &Graph) -> Result<CircuitStore, RaiseError> {
    graph_to_circuit_with_origins(graph).map(|raised| raised.circuit)
}

/// Raises a graph to a circuit, reporting what each recovered instruction came
/// from.
///
/// # Errors
///
/// Returns the same diagnostics as [`graph_to_circuit`].
pub fn graph_to_circuit_with_origins(graph: &Graph) -> Result<RaisedCircuit, RaiseError> {
    let program = read_program(graph)?;
    let mut store = CircuitStore::new(program.qubit_count, program.clbit_count);
    store.set_global_phase(program.global_phase);
    let mut raise = Raise {
        store,
        // A parameter is named in the graph and indexed in the store, so the
        // same name must resolve to one declaration rather than one per
        // appearance.
        parameters: BTreeMap::new(),
    };
    let origins = raise.items(&program.items, None)?;
    Ok(RaisedCircuit {
        circuit: raise.store,
        origins,
    })
}

/// Rebuilding state, so the store and its parameter table travel together
/// through a nesting the graph carries and the store's own API does not.
struct Raise {
    store: CircuitStore,
    parameters: BTreeMap<String, ParameterId>,
}

impl Raise {
    /// Rebuilds a run of steps, at the top level or into a block.
    fn items(
        &mut self,
        items: &[ReadItem],
        into: Option<BlockId>,
    ) -> Result<Vec<Option<CircuitOrigin>>, RaiseError> {
        let mut origins = Vec::with_capacity(items.len());
        for item in items {
            origins.push(match item {
                ReadItem::Instruction(instruction) => self.instruction(instruction, into)?,
                ReadItem::Conditional(conditional) => self.conditional(conditional, into)?,
                // A circuit has blocks and no repetition, so there is nothing
                // to raise a loop into. Refusing by name is what a caller can
                // act on; unrolling it here would invent a trip count.
                ReadItem::Loop(_) => {
                    return Err(RaiseError::Unsupported {
                        operation: THETA.to_string(),
                    });
                }
            });
        }
        Ok(origins)
    }

    fn instruction(
        &mut self,
        instruction: &ReadInstruction,
        into: Option<BlockId>,
    ) -> Result<Option<CircuitOrigin>, RaiseError> {
        let arity = u8::try_from(instruction.qubits.len()).map_err(|_| RaiseError::Malformed {
            reason: format!(
                "'{}' names more qubits than an instruction can hold",
                instruction.operation
            ),
        })?;
        let operation = operation_from_name(&instruction.operation, arity)?;
        let qubits = instruction
            .qubits
            .iter()
            .map(|qubit| Qubit(*qubit))
            .collect::<Vec<_>>();
        let clbits = instruction
            .clbits
            .iter()
            .map(|clbit| Clbit(*clbit))
            .collect::<Vec<_>>();
        let params = if instruction.params.is_empty() {
            None
        } else {
            Some(
                instruction
                    .params
                    .iter()
                    .map(|angle| self.param_value(angle))
                    .collect::<Box<[_]>>(),
            )
        };
        match into {
            Some(block) => self
                .store
                .push_into_block(block, operation, &qubits, &clbits, params),
            None => self.store.push(operation, &qubits, &clbits, params),
        }
        .map_err(|error| RaiseError::Rejected(error.to_string()))?;
        Ok(instruction.origin)
    }

    fn conditional(
        &mut self,
        conditional: &ReadConditional,
        into: Option<BlockId>,
    ) -> Result<Option<CircuitOrigin>, RaiseError> {
        let then_block = self.store.new_block();
        self.items(&conditional.then_items, Some(then_block))?;
        let else_block = match &conditional.else_items {
            Some(items) => {
                let block = self.store.new_block();
                self.items(items, Some(block))?;
                Some(block)
            }
            None => None,
        };
        let condition = self.store.push_expr(Expr::Clbit(conditional.clbit));
        let control = ControlFlow::IfElse(IfElse {
            condition,
            then_block,
            else_block,
        });
        match into {
            Some(block) => self.store.push_control_flow_into_block(block, control),
            None => self.store.push_control_flow(control),
        }
        .map_err(|error| RaiseError::Rejected(error.to_string()))?;
        Ok(conditional.origin)
    }

    /// The circuit's form of a graph angle, declaring whatever it names.
    ///
    /// A literal stays a literal. Anything else becomes a node in the store's
    /// angle arena, built leaves first because the arena refuses a forward
    /// reference.
    fn param_value(&mut self, angle: &Angle) -> ParamValue {
        match angle {
            Angle::Literal { value } => ParamValue::Literal(value.get()),
            Angle::Parameter { name } => ParamValue::Parameter(self.declare(name)),
            _ => ParamValue::Expr(self.angle_id(angle)),
        }
    }

    fn angle_id(&mut self, angle: &Angle) -> AngleId {
        let expr = match angle {
            Angle::Literal { value } => AngleExpr::Literal(value.get()),
            Angle::Parameter { name } => AngleExpr::Parameter(self.declare(name)),
            Angle::Neg { operand } => AngleExpr::Neg(self.angle_id(operand)),
            Angle::Binary { op, left, right } => AngleExpr::Binary(
                match op {
                    AngleOp::Add => StoreAngleOp::Add,
                    AngleOp::Sub => StoreAngleOp::Sub,
                    AngleOp::Mul => StoreAngleOp::Mul,
                    AngleOp::Div => StoreAngleOp::Div,
                },
                self.angle_id(left),
                self.angle_id(right),
            ),
        };
        self.store.push_angle(expr)
    }

    fn declare(&mut self, name: &str) -> ParameterId {
        if let Some(id) = self.parameters.get(name) {
            return *id;
        }
        let id = self.store.declare_parameter(name);
        self.parameters.insert(name.to_string(), id);
        id
    }
}

/// The circuit operation a graph node name stands for.
///
/// An unrecognised name is the outside-the-subset case rather than a bug, so it
/// returns the naming diagnostic instead of panicking.
fn operation_from_name(name: &str, qubit_arity: u8) -> Result<PackedOperation, RaiseError> {
    if name == RESET {
        return Ok(PackedOperation::Reset);
    }
    if name == BARRIER {
        return Ok(PackedOperation::Barrier { qubit_arity });
    }
    gate_from_name(name)
        .map(PackedOperation::Standard)
        .ok_or_else(|| RaiseError::Unsupported {
            operation: name.to_string(),
        })
}

/// Every gate the conversion names.
///
/// The reverse mapping searches this rather than repeating the forward table, so
/// the two directions cannot disagree about a gate that appears in both. A gate
/// added to the store but not added here still lowers, and raising reports it by
/// name instead of guessing; the exhaustiveness test exists to catch that first.
pub(crate) const ALL_GATES: [StandardGate; 32] = [
    StandardGate::I,
    StandardGate::X,
    StandardGate::Y,
    StandardGate::Z,
    StandardGate::H,
    StandardGate::S,
    StandardGate::SDag,
    StandardGate::SqrtX,
    StandardGate::SqrtXDag,
    StandardGate::T,
    StandardGate::TDag,
    StandardGate::Cx,
    StandardGate::Cy,
    StandardGate::Cz,
    StandardGate::Swap,
    StandardGate::Ccx,
    StandardGate::Cswap,
    StandardGate::Rx,
    StandardGate::Ry,
    StandardGate::Rz,
    StandardGate::Rxx,
    StandardGate::Ryy,
    StandardGate::Rzz,
    StandardGate::Crx,
    StandardGate::Cry,
    StandardGate::Crz,
    StandardGate::MeasureX,
    StandardGate::MeasureY,
    StandardGate::MeasureZ,
    StandardGate::PrepareX,
    StandardGate::PrepareY,
    StandardGate::PrepareZ,
];
