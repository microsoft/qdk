// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The instruction record and the flat store that holds a sequence of them.

use rustc_hash::FxHashSet;

use crate::angle::{AngleArena, AngleExpr, AngleId};
use crate::classical::{
    Expr, ExprArena, ExprId, ParamValue, Parameter, ParameterId, Var, VarId, VarType,
};
use crate::control_flow::{BlockId, ControlFlow, LoopIndex};
use crate::gate::StandardGate;
use crate::operands::{Clbit, EMPTY_OPERANDS, OperandTable, OperandsId, Qubit};
use crate::operation::PackedOperation;

/// A block of instructions referenced by a control-flow construct.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Block {
    pub instructions: Vec<Instruction>,
}

/// One instruction in a circuit.
///
/// Operands are referenced by handle rather than owned, and parameters are
/// boxed only when the operation has any, so an unparameterized standard gate
/// carries no heap payload at all.
#[derive(Clone, Debug, PartialEq)]
pub struct Instruction {
    pub operation: PackedOperation,
    pub qubits: OperandsId,
    pub clbits: OperandsId,
    pub params: Option<Box<[ParamValue]>>,
}

impl Instruction {
    /// Whether this instruction holds no heap payload.
    #[must_use]
    pub fn is_inline(&self) -> bool {
        self.operation.is_inline() && self.params.is_none()
    }
}

/// Errors that can arise while building a circuit.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("operation expects {expected} qubits but {actual} were given")]
    QubitArity { expected: u8, actual: usize },
    #[error("qubit index {index} is outside the circuit's {width} qubits")]
    QubitOutOfRange { index: u32, width: u32 },
    #[error("clbit index {index} is outside the circuit's {width} classical bits")]
    ClbitOutOfRange { index: u32, width: u32 },
    #[error("block {0} does not exist")]
    UnknownBlock(u32),
    #[error("expression {0} does not exist")]
    UnknownExpr(u32),
    #[error("angle expression {0} does not exist")]
    UnknownAngle(u32),
    #[error("variable {0} does not exist")]
    UnknownVar(u32),
    #[error("parameter {0} does not exist")]
    UnknownParameter(u32),
    #[error("a moment cannot hold two instructions that both act on qubit {0}")]
    MomentOverlap(u32),
    #[error("no value supplied for parameter '{0}'")]
    UnboundParameter(String),
}

/// A flat sequence of instructions over a fixed number of qubits and classical bits.
#[derive(Clone, Debug)]
pub struct CircuitStore {
    qubit_count: u32,
    clbit_count: u32,
    instructions: Vec<Instruction>,
    qubit_operands: OperandTable<Qubit>,
    clbit_operands: OperandTable<Clbit>,
    blocks: Vec<Block>,
    parameters: Vec<Parameter>,
    vars: Vec<Var>,
    exprs: ExprArena,
    angles: AngleArena,
    global_phase: f64,
}

impl CircuitStore {
    /// Creates an empty circuit of the given width.
    #[must_use]
    pub fn new(qubit_count: u32, clbit_count: u32) -> Self {
        Self {
            qubit_count,
            clbit_count,
            instructions: Vec::new(),
            qubit_operands: OperandTable::new(),
            clbit_operands: OperandTable::new(),
            blocks: Vec::new(),
            parameters: Vec::new(),
            vars: Vec::new(),
            exprs: ExprArena::new(),
            angles: AngleArena::new(),
            global_phase: 0.0,
        }
    }

    /// Number of qubits.
    #[must_use]
    pub const fn qubit_count(&self) -> u32 {
        self.qubit_count
    }

    /// Number of classical bits.
    #[must_use]
    pub const fn clbit_count(&self) -> u32 {
        self.clbit_count
    }

    /// Number of stored instructions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.instructions.len()
    }

    /// Whether the circuit holds no instructions.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }

    /// Instructions in order.
    pub fn instructions(&self) -> impl Iterator<Item = &Instruction> {
        self.instructions.iter()
    }

    /// Returns the instruction at `index`, or `None` if out of range.
    #[must_use]
    pub fn instruction(&self, index: usize) -> Option<&Instruction> {
        self.instructions.get(index)
    }

    /// Qubit operands for an instruction.
    #[must_use]
    pub fn qubits_of(&self, instruction: &Instruction) -> Option<&[Qubit]> {
        self.qubit_operands.get(instruction.qubits)
    }

    /// Classical-bit operands for an instruction.
    #[must_use]
    pub fn clbits_of(&self, instruction: &Instruction) -> Option<&[Clbit]> {
        self.clbit_operands.get(instruction.clbits)
    }

    /// Number of distinct qubit operand lists stored.
    #[must_use]
    pub fn qubit_operand_list_count(&self) -> usize {
        self.qubit_operands.len()
    }

    /// Number of distinct classical-bit operand lists stored.
    #[must_use]
    pub fn clbit_operand_list_count(&self) -> usize {
        self.clbit_operands.len()
    }

    /// Reserves room for `additional` further instructions.
    ///
    /// Reserving before a bulk append separates amortized vector growth from
    /// per-instruction cost, which is what makes the allocation-free claim for
    /// the standard-gate path measurable. Operand storage is deliberately not
    /// reserved proportionally, because interning makes it grow with the number
    /// of distinct operand patterns rather than with instruction count.
    pub fn reserve(&mut self, additional: usize) {
        self.instructions.reserve(additional);
    }

    /// Reserves room for `patterns` additional distinct qubit operand lists.
    pub fn reserve_qubit_patterns(&mut self, patterns: usize, operands_per_pattern: usize) {
        self.qubit_operands
            .reserve(patterns, patterns * operands_per_pattern);
    }

    /// Appends a standard gate over qubit operands only.
    ///
    /// # Errors
    ///
    /// Returns an error when the operand count does not match the gate's arity,
    /// or when an operand lies outside the circuit's width.
    pub fn push_standard(
        &mut self,
        gate: StandardGate,
        qubits: &[Qubit],
    ) -> Result<(), StoreError> {
        self.push(PackedOperation::Standard(gate), qubits, &[], None)
    }

    /// Appends the same standard gate once per operand list.
    ///
    /// Crossing a language boundary once for a batch is what keeps a large
    /// build from paying a crossing per gate.
    ///
    /// # Errors
    ///
    /// Returns an error on the first operand list that fails validation. Lists
    /// appended before that point remain in the circuit.
    pub fn extend_standard<'a, I>(
        &mut self,
        gate: StandardGate,
        operands: I,
    ) -> Result<(), StoreError>
    where
        I: IntoIterator<Item = &'a [Qubit]>,
    {
        let operands = operands.into_iter();
        let (lower, _) = operands.size_hint();
        self.reserve(lower);
        for qubits in operands {
            self.push_standard(gate, qubits)?;
        }
        Ok(())
    }

    /// Appends a sequence of standard gates paired with their operands.
    ///
    /// # Errors
    ///
    /// Returns an error on the first pair that fails validation. Pairs appended
    /// before that point remain in the circuit.
    pub fn extend<'a, I>(&mut self, gates: I) -> Result<(), StoreError>
    where
        I: IntoIterator<Item = (StandardGate, &'a [Qubit])>,
    {
        let gates = gates.into_iter();
        let (lower, _) = gates.size_hint();
        self.reserve(lower);
        for (gate, qubits) in gates {
            self.push_standard(gate, qubits)?;
        }
        Ok(())
    }

    /// Builds a circuit from a sequence of standard gates and their operands.
    ///
    /// # Errors
    ///
    /// Returns an error on the first pair that fails validation.
    pub fn from_standard_gates<'a, I>(
        qubit_count: u32,
        clbit_count: u32,
        gates: I,
    ) -> Result<Self, StoreError>
    where
        I: IntoIterator<Item = (StandardGate, &'a [Qubit])>,
    {
        let mut store = Self::new(qubit_count, clbit_count);
        store.extend(gates)?;
        Ok(store)
    }

    /// Declares a compile-time parameter.
    pub fn declare_parameter(&mut self, name: impl Into<String>) -> ParameterId {
        let id = u32::try_from(self.parameters.len()).expect("parameter count exceeded u32 range");
        self.parameters.push(Parameter { name: name.into() });
        ParameterId(id)
    }

    /// Declares a runtime classical variable.
    pub fn declare_var(&mut self, name: impl Into<String>, ty: VarType) -> VarId {
        let id = u32::try_from(self.vars.len()).expect("variable count exceeded u32 range");
        self.vars.push(Var {
            name: name.into(),
            ty,
        });
        VarId(id)
    }

    /// Declared compile-time parameters.
    #[must_use]
    pub fn parameters(&self) -> &[Parameter] {
        &self.parameters
    }

    /// Declared runtime classical variables.
    #[must_use]
    pub fn vars(&self) -> &[Var] {
        &self.vars
    }

    /// Appends a standard gate carrying angle parameters.
    ///
    /// # Errors
    ///
    /// Returns an error when operand validation fails, or when a parameter
    /// handle is not declared on this circuit.
    pub fn push_parameterized(
        &mut self,
        gate: StandardGate,
        qubits: &[Qubit],
        params: &[ParamValue],
    ) -> Result<(), StoreError> {
        self.push(
            PackedOperation::Standard(gate),
            qubits,
            &[],
            Some(params.into()),
        )
    }

    /// Parameters actually referenced by at least one instruction, in
    /// declaration order.
    ///
    /// A declared but unused parameter is not reported, because binding it
    /// would have no effect and requiring a value for it would be surprising.
    /// Neither is a parameter a for loop binds, because the loop supplies its
    /// value and asking the caller for one would be asking for the wrong thing.
    #[must_use]
    pub fn referenced_parameters(&self) -> Vec<ParameterId> {
        let mut seen = vec![false; self.parameters.len()];
        let mut roots: Vec<AngleId> = Vec::new();
        let mut supplied_by_loop = vec![false; self.parameters.len()];
        let mut pending: Vec<&[Instruction]> = vec![&self.instructions];
        while let Some(instructions) = pending.pop() {
            for instruction in instructions {
                for value in instruction.params.as_deref().unwrap_or(&[]) {
                    match value {
                        ParamValue::Literal(_) => {}
                        ParamValue::Parameter(id) => {
                            if let Some(slot) = seen.get_mut(id.0 as usize) {
                                *slot = true;
                            }
                        }
                        ParamValue::Expr(id) => roots.push(*id),
                    }
                }
                if let PackedOperation::ControlFlow(control) = &instruction.operation {
                    if let ControlFlow::For(loop_) = &**control
                        && let Some(LoopIndex::Parameter(id)) = loop_.index
                        && let Some(slot) = supplied_by_loop.get_mut(id.0 as usize)
                    {
                        *slot = true;
                    }
                    for block in control.blocks() {
                        if let Some(body) = self.block(block) {
                            pending.push(&body.instructions);
                        }
                    }
                }
            }
        }
        for (index, used) in self
            .angles
            .reachable_parameters(&roots, self.parameters.len())
            .into_iter()
            .enumerate()
        {
            seen[index] |= used;
        }
        seen.iter()
            .enumerate()
            .filter(|(index, used)| **used && !supplied_by_loop[*index])
            .map(|(index, _)| {
                ParameterId(u32::try_from(index).expect("parameter count fits in u32"))
            })
            .collect()
    }

    /// Substitutes values for parameters, producing a fully bound circuit.
    ///
    /// # Errors
    ///
    /// Returns an error when a referenced parameter has no supplied value.
    pub fn bind_parameters(&self, values: &[(ParameterId, f64)]) -> Result<Self, StoreError> {
        let mut lookup = vec![None; self.parameters.len()];
        for (id, value) in values {
            if let Some(slot) = lookup.get_mut(id.0 as usize) {
                *slot = Some(*value);
            } else {
                return Err(StoreError::UnknownParameter(id.0));
            }
        }

        for id in self.referenced_parameters() {
            if lookup[id.0 as usize].is_none() {
                return Err(StoreError::UnboundParameter(
                    self.parameters[id.0 as usize].name.clone(),
                ));
            }
        }

        let evaluated = self.angles.evaluate_all(&lookup);
        // A parameter a loop supplies has no value here, so it survives binding
        // rather than being folded away. Rewriting the arena in place keeps
        // every stored handle valid, since the node count does not change.
        let angles = AngleArena::from_nodes(
            self.angles
                .nodes()
                .iter()
                .map(|node| match *node {
                    AngleExpr::Parameter(id) => {
                        lookup[id.0 as usize].map_or(AngleExpr::Parameter(id), AngleExpr::Literal)
                    }
                    other => other,
                })
                .collect(),
        )
        .expect("rewriting nodes in place preserves their order");

        let substitute = |instruction: &Instruction| -> Instruction {
            let params = instruction.params.as_deref().map(|values| {
                values
                    .iter()
                    .map(|value| match value {
                        ParamValue::Literal(literal) => ParamValue::Literal(*literal),
                        ParamValue::Parameter(id) => lookup[id.0 as usize]
                            .map_or(ParamValue::Parameter(*id), ParamValue::Literal),
                        ParamValue::Expr(id) => evaluated[id.0 as usize]
                            .map_or(ParamValue::Expr(*id), ParamValue::Literal),
                    })
                    .collect::<Box<[ParamValue]>>()
            });
            Instruction {
                operation: instruction.operation.clone(),
                qubits: instruction.qubits,
                clbits: instruction.clbits,
                params,
            }
        };

        let mut bound = self.clone();
        bound.angles = angles;
        bound.instructions = self.instructions.iter().map(substitute).collect();
        bound.blocks = self
            .blocks
            .iter()
            .map(|block| Block {
                instructions: block.instructions.iter().map(substitute).collect(),
            })
            .collect();
        Ok(bound)
    }

    /// Stores a condition expression node and returns its handle.
    pub fn push_expr(&mut self, expr: Expr) -> ExprId {
        self.exprs.push(expr)
    }

    /// Stores an angle expression node and returns its handle.
    ///
    /// # Panics
    ///
    /// Panics if the node references a handle this circuit does not hold.
    pub fn push_angle(&mut self, expr: AngleExpr) -> AngleId {
        self.angles.push(expr)
    }

    /// The circuit's angle expressions.
    #[must_use]
    pub const fn angles(&self) -> &AngleArena {
        &self.angles
    }

    /// The phase applied to the circuit as a whole, in radians.
    ///
    /// Unobservable when the circuit is run on its own, and observable when it
    /// is controlled or used as a subroutine, which is why it is recorded
    /// rather than dropped.
    #[must_use]
    pub const fn global_phase(&self) -> f64 {
        self.global_phase
    }

    /// Sets the phase applied to the circuit as a whole.
    pub fn set_global_phase(&mut self, phase: f64) {
        self.global_phase = normalize_phase(phase);
    }

    /// Adds to the phase applied to the circuit as a whole.
    ///
    /// Rewriting one gate into others often leaves a phase behind, and a
    /// rewrite should not have to know what earlier ones contributed.
    pub fn shift_global_phase(&mut self, delta: f64) {
        self.global_phase = normalize_phase(self.global_phase + delta);
    }

    /// Installs a restored angle arena, checking it against this circuit.
    ///
    /// The arena's own shape is validated before it gets here; what only this
    /// circuit can check is that the parameters it names were declared.
    pub(crate) fn install_angles(&mut self, angles: AngleArena) -> Result<(), StoreError> {
        for node in angles.nodes() {
            if let AngleExpr::Parameter(id) = node
                && self.parameters.get(id.0 as usize).is_none()
            {
                return Err(StoreError::UnknownParameter(id.0));
            }
        }
        self.angles = angles;
        Ok(())
    }

    /// The circuit's condition expressions.
    #[must_use]
    pub const fn exprs(&self) -> &ExprArena {
        &self.exprs
    }

    /// Creates an empty block and returns its handle.
    pub fn new_block(&mut self) -> BlockId {
        let id = u32::try_from(self.blocks.len()).expect("block count exceeded u32 range");
        self.blocks.push(Block::default());
        BlockId(id)
    }

    /// Returns a block, or `None` if the handle is unknown.
    #[must_use]
    pub fn block(&self, id: BlockId) -> Option<&Block> {
        self.blocks.get(id.0 as usize)
    }

    /// Number of blocks.
    #[must_use]
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Appends an instruction into a block.
    ///
    /// # Errors
    ///
    /// Returns an error when the block is unknown, or when the operation fails
    /// the same operand validation the top-level path applies.
    pub fn push_into_block(
        &mut self,
        block: BlockId,
        operation: PackedOperation,
        qubits: &[Qubit],
        clbits: &[Clbit],
        params: Option<Box<[ParamValue]>>,
    ) -> Result<(), StoreError> {
        if self.block(block).is_none() {
            return Err(StoreError::UnknownBlock(block.0));
        }
        let instruction = self.build_instruction(operation, qubits, clbits, params)?;
        let index = block.0 as usize;
        self.blocks[index].instructions.push(instruction);
        Ok(())
    }

    /// Appends a control-flow construct at the top level.
    ///
    /// # Errors
    ///
    /// Returns an error when a referenced block or expression does not exist.
    pub fn push_control_flow(&mut self, control: ControlFlow) -> Result<(), StoreError> {
        self.validate_control_flow(&control)?;
        self.instructions.push(Instruction {
            operation: PackedOperation::ControlFlow(Box::new(control)),
            qubits: EMPTY_OPERANDS,
            clbits: EMPTY_OPERANDS,
            params: None,
        });
        Ok(())
    }

    /// Appends a control-flow construct into a block.
    ///
    /// # Errors
    ///
    /// Returns an error when a referenced block or expression does not exist.
    pub fn push_control_flow_into_block(
        &mut self,
        block: BlockId,
        control: ControlFlow,
    ) -> Result<(), StoreError> {
        if self.block(block).is_none() {
            return Err(StoreError::UnknownBlock(block.0));
        }
        self.validate_control_flow(&control)?;
        let index = block.0 as usize;
        self.blocks[index].instructions.push(Instruction {
            operation: PackedOperation::ControlFlow(Box::new(control)),
            qubits: EMPTY_OPERANDS,
            clbits: EMPTY_OPERANDS,
            params: None,
        });
        Ok(())
    }

    /// Removes the most recently appended top-level instruction.
    ///
    /// Used when a builder replaces a construct it already emitted, such as a
    /// conditional gaining an alternative branch.
    pub fn pop_last_instruction(&mut self) -> Option<Instruction> {
        self.instructions.pop()
    }

    /// Removes the most recently appended instruction from a block.
    ///
    /// A builder that replaces a construct has to remove it from wherever it
    /// put it, which is the enclosing block when the construct is nested.
    pub fn pop_last_instruction_from_block(&mut self, block: BlockId) -> Option<Instruction> {
        self.blocks.get_mut(block.0 as usize)?.instructions.pop()
    }

    pub(crate) fn validate_control_flow(&self, control: &ControlFlow) -> Result<(), StoreError> {
        for block in control.blocks() {
            if self.block(block).is_none() {
                return Err(StoreError::UnknownBlock(block.0));
            }
        }
        for expr in control_flow_conditions(control) {
            if self.exprs.get(expr).is_none() {
                return Err(StoreError::UnknownExpr(expr.0));
            }
        }
        if let ControlFlow::For(loop_) = control {
            match loop_.index {
                Some(LoopIndex::Parameter(id)) if self.parameters.get(id.0 as usize).is_none() => {
                    return Err(StoreError::UnknownParameter(id.0));
                }
                Some(LoopIndex::Var(id)) if self.vars.get(id.0 as usize).is_none() => {
                    return Err(StoreError::UnknownVar(id.0));
                }
                _ => {}
            }
        }
        if let ControlFlow::Moment(body) = control {
            self.validate_moment(*body)?;
        }
        Ok(())
    }

    /// Checks that a moment's instructions can actually run at the same time.
    ///
    /// Simultaneity is the whole of what a moment asserts over a box, so an
    /// overlap is refused here rather than left for a target to discover.
    fn validate_moment(&self, body: BlockId) -> Result<(), StoreError> {
        let Some(block) = self.block(body) else {
            return Err(StoreError::UnknownBlock(body.0));
        };
        let mut seen = FxHashSet::default();
        for instruction in &block.instructions {
            for qubit in self.qubits_of(instruction).unwrap_or(&[]) {
                if !seen.insert(*qubit) {
                    return Err(StoreError::MomentOverlap(qubit.0));
                }
            }
        }
        Ok(())
    }

    fn validate_params(&self, params: &[ParamValue]) -> Result<(), StoreError> {
        for value in params {
            match value {
                ParamValue::Literal(_) => {}
                ParamValue::Parameter(id) => {
                    if self.parameters.get(id.0 as usize).is_none() {
                        return Err(StoreError::UnknownParameter(id.0));
                    }
                }
                ParamValue::Expr(id) => {
                    if self.angles.get(*id).is_none() {
                        return Err(StoreError::UnknownAngle(id.0));
                    }
                }
            }
        }
        Ok(())
    }

    /// Appends an operation over the given operands.
    ///
    /// # Errors
    ///
    /// Returns an error when the operand count does not match the operation's
    /// arity, or when an operand lies outside the circuit's width.
    pub fn push(
        &mut self,
        operation: PackedOperation,
        qubits: &[Qubit],
        clbits: &[Clbit],
        params: Option<Box<[ParamValue]>>,
    ) -> Result<(), StoreError> {
        let instruction = self.build_instruction(operation, qubits, clbits, params)?;
        self.instructions.push(instruction);
        Ok(())
    }

    /// Validates operands and interns them, producing an instruction record.
    fn build_instruction(
        &mut self,
        operation: PackedOperation,
        qubits: &[Qubit],
        clbits: &[Clbit],
        params: Option<Box<[ParamValue]>>,
    ) -> Result<Instruction, StoreError> {
        // Checked here rather than only in the control-flow entry points, so no
        // caller can install a construct pointing at a block or expression that
        // does not exist.
        if let PackedOperation::ControlFlow(control) = &operation {
            self.validate_control_flow(control)?;
        }
        if let Some(values) = params.as_deref() {
            self.validate_params(values)?;
        }
        let expected = operation.qubit_arity();
        if usize::from(expected) != qubits.len() {
            return Err(StoreError::QubitArity {
                expected,
                actual: qubits.len(),
            });
        }
        for qubit in qubits {
            if qubit.0 >= self.qubit_count {
                return Err(StoreError::QubitOutOfRange {
                    index: qubit.0,
                    width: self.qubit_count,
                });
            }
        }
        for clbit in clbits {
            if clbit.0 >= self.clbit_count {
                return Err(StoreError::ClbitOutOfRange {
                    index: clbit.0,
                    width: self.clbit_count,
                });
            }
        }

        let qubits = self.qubit_operands.insert(qubits);
        let clbits = if clbits.is_empty() {
            EMPTY_OPERANDS
        } else {
            self.clbit_operands.insert(clbits)
        };
        Ok(Instruction {
            operation,
            qubits,
            clbits,
            params,
        })
    }
}

/// Reduces a phase to one turn.
///
/// A phase and the same phase plus a full turn describe the same circuit, so
/// they are stored identically and compare equal. Matches how Qiskit holds it,
/// which keeps a round trip through that library stable.
fn normalize_phase(phase: f64) -> f64 {
    if phase.is_finite() {
        phase.rem_euclid(std::f64::consts::TAU)
    } else {
        phase
    }
}

/// Condition expressions referenced by a control-flow construct.
fn control_flow_conditions(control: &ControlFlow) -> Vec<ExprId> {
    match control {
        ControlFlow::IfElse(if_else) => vec![if_else.condition],
        ControlFlow::While(loop_) => vec![loop_.condition],
        ControlFlow::Switch(switch) => vec![switch.selector],
        ControlFlow::For(_)
        | ControlFlow::Box(_)
        | ControlFlow::Moment(_)
        | ControlFlow::Break
        | ControlFlow::Continue => Vec::new(),
    }
}
