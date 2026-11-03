// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Lowering a circuit directly to the runtime IR and on to QIR.
//!
//! This path does not go through Q# compilation or partial evaluation. A
//! circuit is already close to the runtime IR: a flat instruction sequence over
//! dense qubit indices, which is what the IR wants. What the IR adds is an entry
//! point, intrinsic declarations, explicit result registers, and output
//! recording, all of which this module supplies.
//!
//! Not every circuit can be lowered here. A gate with no QIR intrinsic needs to
//! be decomposed first, and some control-flow constructs are not lowered yet.
//! Both are reported as typed errors naming what was found, rather than being
//! dropped or approximated.

pub mod decompose;
mod emit;

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
pub(crate) use emit::{GateLowering, gate_lowering};

use emit::Emitter;
use qsc_circuit_store::{
    BinaryOp, BlockId as CircuitBlockId, CircuitStore, ControlFlow, Expr, ExprId, ForLoop, IfElse,
    Instruction as CircuitInstruction, Literal, LoopIndex, PackedOperation, ParamValue,
};
use qsc_data_structures::target::TargetCapabilityFlags;
use qsc_rir::rir::{
    BlockId, ConditionCode, Instruction, Literal as RirLiteral, Operand, Prim, Program, Ty,
    Variable,
};
use rustc_hash::FxHashMap;

/// Why a circuit could not be lowered.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("gate '{gate}' has no QIR intrinsic and must be decomposed before lowering")]
    UndecomposedGate { gate: &'static str },
    #[error("operation '{operation}' cannot be lowered directly")]
    UnsupportedOperation { operation: &'static str },
    #[error("'{construct}' is not lowered by the direct path yet")]
    UnsupportedConstruct { construct: &'static str },
    #[error("'{construct}' requires the {capability} capability, which the target profile lacks")]
    MissingCapability {
        construct: &'static str,
        capability: &'static str,
    },
    #[error("condition cannot be lowered: {reason}")]
    UnsupportedCondition { reason: &'static str },
    #[error("parameter '{name}' has no value; bind parameters before lowering")]
    UnboundParameter { name: String },
}

/// Lowers a circuit to the runtime IR for a target profile.
///
/// # Errors
///
/// Returns an error naming the gate or construct that could not be lowered, or
/// the capability the profile is missing.
pub fn circuit_to_rir(
    store: &CircuitStore,
    capabilities: TargetCapabilityFlags,
) -> Result<Program, Error> {
    Lowering::new(store, capabilities).run()
}

/// Lowers a circuit and emits QIR for a target profile.
///
/// # Errors
///
/// Returns an error naming the gate or construct that could not be lowered, or
/// the capability the profile is missing.
pub fn circuit_to_qir(
    store: &CircuitStore,
    capabilities: TargetCapabilityFlags,
) -> Result<String, Error> {
    let mut program = circuit_to_rir(store, capabilities)?;
    qsc_rir::passes::check_and_transform(&mut program);
    Ok(
        if capabilities <= qsc_data_structures::target::Profile::AdaptiveRIF.into() {
            crate::qir::v1::ToQir::<String>::to_qir(&program, &program)
        } else {
            crate::qir::v2::ToQir::<String>::to_qir(&program, &program)
        },
    )
}

struct Lowering<'a> {
    store: &'a CircuitStore,
    emit: Emitter,
    /// Values bound to loop variables by the iteration being unrolled.
    bindings: FxHashMap<u32, Literal>,
    /// Values bound to compile-time parameters by an enclosing unrolled loop.
    parameter_bindings: FxHashMap<u32, f64>,
    /// Every angle expression evaluated under `parameter_bindings`.
    ///
    /// Recomputed when those bindings change rather than per instruction, so a
    /// loop body costs one pass over the arena per iteration instead of one
    /// per gate.
    angle_values: Vec<Option<f64>>,
}

impl<'a> Lowering<'a> {
    fn new(store: &'a CircuitStore, capabilities: TargetCapabilityFlags) -> Self {
        Self {
            store,
            emit: Emitter::new(capabilities, store.qubit_count(), store.clbit_count()),
            bindings: FxHashMap::default(),
            parameter_bindings: FxHashMap::default(),
            angle_values: Vec::new(),
        }
    }

    fn run(mut self) -> Result<Program, Error> {
        self.refresh_angles();
        let entry_block = self.emit.start();
        let tail = self.lower_sequence(self.store.instructions(), entry_block)?;
        Ok(self.emit.finish(tail))
    }

    /// Lowers a run of instructions, returning the block execution ends in.
    ///
    /// Control flow can split the sequence across several blocks, so the block
    /// that follows is not always the one it started in.
    fn lower_sequence<'i>(
        &mut self,
        instructions: impl Iterator<Item = &'i CircuitInstruction>,
        start: BlockId,
    ) -> Result<BlockId, Error> {
        let mut current = start;
        for instruction in instructions {
            current = self.lower_instruction(instruction, current)?;
        }
        Ok(current)
    }

    fn lower_instruction(
        &mut self,
        instruction: &CircuitInstruction,
        current: BlockId,
    ) -> Result<BlockId, Error> {
        let qubits: Vec<u32> = self
            .store
            .qubits_of(instruction)
            .unwrap_or(&[])
            .iter()
            .map(|qubit| qubit.0)
            .collect();
        let clbits: Vec<u32> = self
            .store
            .clbits_of(instruction)
            .unwrap_or(&[])
            .iter()
            .map(|clbit| clbit.0)
            .collect();

        match &instruction.operation {
            PackedOperation::Standard(gate) => {
                let angles = self.angles(instruction)?;
                self.emit
                    .emit_standard(*gate, &qubits, &clbits, &angles, current)?;
                Ok(current)
            }
            PackedOperation::Reset => {
                self.emit.emit_reset(&qubits, current);
                Ok(current)
            }
            PackedOperation::ControlFlow(control) => self.lower_control_flow(control, current),
            PackedOperation::Barrier { .. } => {
                self.emit.emit_barrier(current);
                Ok(current)
            }
            PackedOperation::Delay { .. } => {
                Err(Error::UnsupportedOperation { operation: "delay" })
            }
            PackedOperation::Custom(_) => Err(Error::UnsupportedOperation {
                operation: "custom operation",
            }),
            PackedOperation::Unitary(_) => Err(Error::UnsupportedOperation {
                operation: "explicit unitary",
            }),
        }
    }

    /// Resolves an instruction's angles, refusing any that are still symbolic.
    fn angles(&self, instruction: &CircuitInstruction) -> Result<Vec<f64>, Error> {
        instruction
            .params
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|param| match param {
                ParamValue::Literal(value) => Ok(*value),
                ParamValue::Parameter(id) => {
                    self.parameter_bindings.get(&id.0).copied().ok_or_else(|| {
                        Error::UnboundParameter {
                            name: self.store.parameters()[id.0 as usize].name.clone(),
                        }
                    })
                }
                ParamValue::Expr(id) => self
                    .angle_values
                    .get(id.0 as usize)
                    .copied()
                    .flatten()
                    .ok_or_else(|| Error::UnboundParameter {
                        name: self.unbound_in_angle(*id),
                    }),
            })
            .collect()
    }

    /// Names a parameter an angle expression still depends on.
    fn unbound_in_angle(&self, id: qsc_circuit_store::AngleId) -> String {
        let parameters = self.store.parameters();
        self.store
            .angles()
            .reachable_parameters(&[id], parameters.len())
            .iter()
            .enumerate()
            .find(|(index, used)| {
                **used
                    && !self
                        .parameter_bindings
                        .contains_key(&u32::try_from(*index).expect("parameter count fits in u32"))
            })
            .map_or_else(
                || "angle expression".to_string(),
                |(index, _)| parameters[index].name.clone(),
            )
    }

    /// Re-evaluates every angle expression under the current bindings.
    fn refresh_angles(&mut self) {
        let mut lookup = vec![None; self.store.parameters().len()];
        for (id, value) in &self.parameter_bindings {
            if let Some(slot) = lookup.get_mut(*id as usize) {
                *slot = Some(*value);
            }
        }
        self.angle_values = self.store.angles().evaluate_all(&lookup);
    }

    fn lower_control_flow(
        &mut self,
        control: &ControlFlow,
        current: BlockId,
    ) -> Result<BlockId, Error> {
        match control {
            // A moment says its instructions run at the same time. The runtime
            // representation has no notion of simultaneity, so the body lowers
            // in order, which is a valid schedule for instructions the store
            // has already checked act on disjoint qubits.
            ControlFlow::Moment(body) => {
                let block = self.store.block(*body).ok_or(Error::UnsupportedConstruct {
                    construct: "moment with an unknown body",
                })?;
                let instructions: Vec<CircuitInstruction> = block.instructions.clone();
                self.lower_sequence(instructions.iter(), current)
            }
            // A box is a grouping construct with no runtime effect, so its body
            // lowers inline. A duration is a scheduling instruction, which this
            // path cannot honour, so it is refused rather than dropped.
            ControlFlow::Box(boxed) => {
                if boxed.duration.is_some() {
                    return Err(Error::UnsupportedConstruct {
                        construct: "box with duration",
                    });
                }
                let body = self
                    .store
                    .block(boxed.body)
                    .ok_or(Error::UnsupportedConstruct {
                        construct: "box with an unknown body",
                    })?;
                let instructions: Vec<CircuitInstruction> = body.instructions.clone();
                self.lower_sequence(instructions.iter(), current)
            }
            ControlFlow::IfElse(if_else) => self.lower_conditional(if_else, current),
            ControlFlow::While(_) => Err(Error::UnsupportedConstruct { construct: "while" }),
            ControlFlow::For(loop_) => self.unroll(loop_, current),
            ControlFlow::Switch(_) => Err(Error::UnsupportedConstruct {
                construct: "switch",
            }),
            ControlFlow::Break => Err(Error::UnsupportedConstruct { construct: "break" }),
            ControlFlow::Continue => Err(Error::UnsupportedConstruct {
                construct: "continue",
            }),
        }
    }

    /// Reads a block's instructions, so they can be lowered without borrowing.
    fn body_of(
        &self,
        block: CircuitBlockId,
        construct: &'static str,
    ) -> Result<Vec<CircuitInstruction>, Error> {
        Ok(self
            .store
            .block(block)
            .ok_or(Error::UnsupportedConstruct { construct })?
            .instructions
            .clone())
    }

    fn lower_conditional(&mut self, if_else: &IfElse, current: BlockId) -> Result<BlockId, Error> {
        let (operand, prim) = self.lower_expr(if_else.condition, current)?;
        if prim != Prim::Boolean {
            return Err(Error::UnsupportedCondition {
                reason: "condition is not a boolean",
            });
        }

        // A condition already decided selects its branch outright. That is what
        // makes an unrolled loop with a fixed test cost nothing, and it needs no
        // branching capability.
        if let Operand::Literal(RirLiteral::Bool(taken)) = operand {
            let chosen = if taken {
                Some(if_else.then_block)
            } else {
                if_else.else_block
            };
            let Some(block) = chosen else {
                return Ok(current);
            };
            let body = self.body_of(block, "conditional with an unknown body")?;
            return self.lower_sequence(body.iter(), current);
        }

        self.require(TargetCapabilityFlags::Adaptive, "if", "Adaptive")?;
        let condition = match operand {
            Operand::Variable(variable) => variable,
            Operand::Literal(literal) => {
                let variable = self.new_variable(Ty::Prim(Prim::Boolean));
                self.push(
                    current,
                    Instruction::Store(Operand::Literal(literal), variable),
                );
                variable
            }
        };
        let then_start = self.new_block();
        let join = self.new_block();
        let else_start = if if_else.else_block.is_some() {
            self.new_block()
        } else {
            join
        };
        self.push(
            current,
            Instruction::Branch(condition, then_start, else_start, None),
        );

        let then_body = self.body_of(if_else.then_block, "conditional with an unknown body")?;
        let then_tail = self.lower_sequence(then_body.iter(), then_start)?;
        self.push(then_tail, Instruction::Jump(join));

        if let Some(else_block) = if_else.else_block {
            let else_body = self.body_of(else_block, "conditional with an unknown body")?;
            let else_tail = self.lower_sequence(else_body.iter(), else_start)?;
            self.push(else_tail, Instruction::Jump(join));
        }
        Ok(join)
    }

    /// Repeats a loop body once per value.
    ///
    /// The values are known when the circuit is built, so unrolling avoids the
    /// backwards branching that most target profiles lack.
    fn unroll(&mut self, loop_: &ForLoop, current: BlockId) -> Result<BlockId, Error> {
        let body = self.body_of(loop_.body, "for with an unknown body")?;
        let mut block = current;
        for value in &loop_.values {
            let restore = self.bind_index(loop_.index, *value);
            block = self.lower_sequence(body.iter(), block)?;
            self.unbind_index(loop_.index, restore);
        }
        Ok(block)
    }

    /// Binds the loop index for one iteration, returning what it displaced.
    fn bind_index(&mut self, index: Option<LoopIndex>, value: Literal) -> Option<IndexBinding> {
        match index? {
            LoopIndex::Var(var) => Some(IndexBinding::Var(self.bindings.insert(var.0, value))),
            LoopIndex::Parameter(id) => {
                let previous = self.parameter_bindings.insert(id.0, numeric(value));
                self.refresh_angles();
                Some(IndexBinding::Parameter(previous))
            }
        }
    }

    /// Restores what the loop index displaced.
    fn unbind_index(&mut self, index: Option<LoopIndex>, restore: Option<IndexBinding>) {
        match (index, restore) {
            (Some(LoopIndex::Var(var)), Some(IndexBinding::Var(previous))) => {
                match previous {
                    Some(outer) => self.bindings.insert(var.0, outer),
                    None => self.bindings.remove(&var.0),
                };
            }
            (Some(LoopIndex::Parameter(id)), Some(IndexBinding::Parameter(previous))) => {
                match previous {
                    Some(outer) => self.parameter_bindings.insert(id.0, outer),
                    None => self.parameter_bindings.remove(&id.0),
                };
                self.refresh_angles();
            }
            _ => {}
        }
    }

    /// Lowers a condition to a boolean variable to branch on.
    fn lower_expr(&mut self, expr: ExprId, current: BlockId) -> Result<(Operand, Prim), Error> {
        let node = *self
            .store
            .exprs()
            .get(expr)
            .ok_or(Error::UnsupportedCondition {
                reason: "condition refers to a missing expression",
            })?;
        match node {
            Expr::Literal(literal) => Ok(literal_operand(literal)),
            Expr::Clbit(clbit) => {
                let read = self.emit.declare(
                    "__quantum__rt__read_result",
                    qsc_rir::builder::read_result_decl(),
                );
                let variable = self.new_variable(Ty::Prim(Prim::Boolean));
                self.push(
                    current,
                    Instruction::Call(
                        read,
                        vec![Operand::Literal(RirLiteral::Result(clbit))],
                        Some(variable),
                        None,
                    ),
                );
                Ok((Operand::Variable(variable), Prim::Boolean))
            }
            Expr::Not(inner) => {
                let (operand, prim) = self.lower_expr(inner, current)?;
                if prim != Prim::Boolean {
                    return Err(Error::UnsupportedCondition {
                        reason: "negation of a non-boolean",
                    });
                }
                Ok((self.boolean_not(operand, current), Prim::Boolean))
            }
            Expr::Binary(op, left, right) => self.lower_binary(op, left, right, current),
            // A loop variable holds the value of the iteration being unrolled.
            Expr::Var(id) => self
                .bindings
                .get(&id.0)
                .copied()
                .map(literal_operand)
                .ok_or(Error::UnsupportedCondition {
                    reason: "classical variables are not lowered yet",
                }),
        }
    }

    fn lower_binary(
        &mut self,
        op: BinaryOp,
        left: ExprId,
        right: ExprId,
        current: BlockId,
    ) -> Result<(Operand, Prim), Error> {
        let (mut left_operand, mut left_prim) = self.lower_expr(left, current)?;
        let (mut right_operand, mut right_prim) = self.lower_expr(right, current)?;

        if matches!(op, BinaryOp::And | BinaryOp::Or) {
            if left_prim != Prim::Boolean || right_prim != Prim::Boolean {
                return Err(Error::UnsupportedCondition {
                    reason: "logical operation on a non-boolean",
                });
            }
            return Ok((
                self.boolean_binary(op, left_operand, right_operand, current),
                Prim::Boolean,
            ));
        }

        // A measurement result reads as a boolean while the surface writes the
        // comparison against 0 or 1, so the literal is narrowed to match rather
        // than the result being widened.
        if left_prim == Prim::Boolean && right_prim == Prim::Integer {
            right_operand = narrow_to_bool(&right_operand)?;
            right_prim = Prim::Boolean;
        } else if left_prim == Prim::Integer && right_prim == Prim::Boolean {
            left_operand = narrow_to_bool(&left_operand)?;
            left_prim = Prim::Boolean;
        }
        if left_prim != right_prim {
            return Err(Error::UnsupportedCondition {
                reason: "operands of a comparison have different types",
            });
        }

        let condition = match op {
            BinaryOp::Eq => Some(ConditionCode::Eq),
            BinaryOp::Ne => Some(ConditionCode::Ne),
            BinaryOp::Lt => Some(ConditionCode::Slt),
            BinaryOp::Le => Some(ConditionCode::Sle),
            BinaryOp::Gt => Some(ConditionCode::Sgt),
            BinaryOp::Ge => Some(ConditionCode::Sge),
            _ => None,
        };

        // Settled before any capability is demanded, because a comparison of two
        // known values costs the target nothing. The runtime IR also rejects an
        // instruction with no variable operands, so leaving one is not an option.
        if let Some(code) = condition
            && let (Operand::Literal(left), Operand::Literal(right)) =
                (&left_operand, &right_operand)
            && let Some(result) = fold_comparison(code, left, right)
        {
            return Ok((Operand::Literal(RirLiteral::Bool(result)), Prim::Boolean));
        }

        if left_prim == Prim::Integer {
            self.require(
                TargetCapabilityFlags::IntegerComputations,
                "integer comparison",
                "IntegerComputations",
            )?;
        }
        if left_prim == Prim::Double {
            self.require(
                TargetCapabilityFlags::FloatingPointComputations,
                "floating-point comparison",
                "FloatingPointComputations",
            )?;
        }

        if let Some(code) = condition {
            // Comparing a measurement result against a constant is the common
            // case, and the comparison carries no information the result does
            // not already carry, so it is folded away as the compiler does.
            if let Some(folded) =
                fold_boolean_comparison(code, &left_operand, &right_operand, left_prim)
            {
                return match folded {
                    Folded::Same(operand) => Ok((operand, Prim::Boolean)),
                    Folded::Negated(operand) => {
                        let variable = self.new_variable(Ty::Prim(Prim::Boolean));
                        self.push(current, Instruction::LogicalNot(operand, variable));
                        Ok((Operand::Variable(variable), Prim::Boolean))
                    }
                };
            }
            // LLVM's signed i1 ordering treats true as -1. Express bit ordering
            // as Boolean logic instead, with false < true.
            if left_prim == Prim::Boolean
                && matches!(
                    op,
                    BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
                )
            {
                let logical = if matches!(op, BinaryOp::Lt | BinaryOp::Gt) {
                    BinaryOp::And
                } else {
                    BinaryOp::Or
                };
                if matches!(op, BinaryOp::Lt | BinaryOp::Le) {
                    left_operand = self.boolean_not(left_operand, current);
                } else {
                    right_operand = self.boolean_not(right_operand, current);
                }
                return Ok((
                    self.boolean_binary(logical, left_operand, right_operand, current),
                    Prim::Boolean,
                ));
            }
            let variable = self.new_variable(Ty::Prim(Prim::Boolean));
            self.push(
                current,
                Instruction::Icmp(code, left_operand, right_operand, variable),
            );
            return Ok((Operand::Variable(variable), Prim::Boolean));
        }

        Err(Error::UnsupportedCondition {
            reason: "arithmetic in a condition is not lowered yet",
        })
    }

    fn boolean_not(&mut self, operand: Operand, current: BlockId) -> Operand {
        if let Operand::Literal(RirLiteral::Bool(value)) = operand {
            return Operand::Literal(RirLiteral::Bool(!value));
        }
        let variable = self.new_variable(Ty::Prim(Prim::Boolean));
        self.push(current, Instruction::LogicalNot(operand, variable));
        Operand::Variable(variable)
    }

    fn boolean_binary(
        &mut self,
        op: BinaryOp,
        left: Operand,
        right: Operand,
        current: BlockId,
    ) -> Operand {
        if let (
            Operand::Literal(RirLiteral::Bool(left)),
            Operand::Literal(RirLiteral::Bool(right)),
        ) = (left, right)
        {
            let value = match op {
                BinaryOp::And => left && right,
                BinaryOp::Or => left || right,
                _ => unreachable!("Boolean connective"),
            };
            return Operand::Literal(RirLiteral::Bool(value));
        }
        let variable = self.new_variable(Ty::Prim(Prim::Boolean));
        let instruction = match op {
            BinaryOp::And => Instruction::LogicalAnd(left, right, variable),
            BinaryOp::Or => Instruction::LogicalOr(left, right, variable),
            _ => unreachable!("Boolean connective"),
        };
        self.push(current, instruction);
        Operand::Variable(variable)
    }

    fn require(
        &self,
        capability: TargetCapabilityFlags,
        construct: &'static str,
        name: &'static str,
    ) -> Result<(), Error> {
        self.emit.require(capability, construct, name)
    }

    fn new_variable(&mut self, ty: Ty) -> Variable {
        self.emit.new_variable(ty)
    }

    fn new_block(&mut self) -> BlockId {
        self.emit.new_block()
    }

    fn push(&mut self, block: BlockId, instruction: Instruction) {
        self.emit.push(block, instruction);
    }
}

/// The result of folding a boolean comparison against a constant.
enum Folded {
    Same(Operand),
    Negated(Operand),
}

/// Folds a boolean compared with a boolean constant into the value itself.
///
/// Returns `None` when the comparison carries information of its own and has to
/// be emitted.
fn fold_boolean_comparison(
    code: ConditionCode,
    left: &Operand,
    right: &Operand,
    prim: Prim,
) -> Option<Folded> {
    if prim != Prim::Boolean {
        return None;
    }
    let (value, constant) = match (left, right) {
        (other, Operand::Literal(RirLiteral::Bool(constant)))
        | (Operand::Literal(RirLiteral::Bool(constant)), other) => (other, *constant),
        _ => return None,
    };
    let same = match code {
        ConditionCode::Eq => constant,
        ConditionCode::Ne => !constant,
        _ => return None,
    };
    Some(if same {
        Folded::Same(*value)
    } else {
        Folded::Negated(*value)
    })
}

/// Settles a comparison of two known values, if both are of the same kind.
fn fold_comparison(code: ConditionCode, left: &RirLiteral, right: &RirLiteral) -> Option<bool> {
    let ordering = match (left, right) {
        (RirLiteral::Bool(left), RirLiteral::Bool(right)) => left.cmp(right),
        (RirLiteral::Integer(left), RirLiteral::Integer(right)) => left.cmp(right),
        (RirLiteral::Double(left), RirLiteral::Double(right)) => left.partial_cmp(right)?,
        _ => return None,
    };
    Some(match code {
        ConditionCode::Eq => ordering.is_eq(),
        ConditionCode::Ne => ordering.is_ne(),
        ConditionCode::Slt => ordering.is_lt(),
        ConditionCode::Sle => ordering.is_le(),
        ConditionCode::Sgt => ordering.is_gt(),
        ConditionCode::Sge => ordering.is_ge(),
    })
}

/// What a loop index displaced, so it can be put back when the body ends.
enum IndexBinding {
    Var(Option<Literal>),
    Parameter(Option<f64>),
}

/// The numeric reading of a literal bound to an angle.
fn numeric(literal: Literal) -> f64 {
    match literal {
        Literal::Bool(value) => f64::from(u8::from(value)),
        #[allow(clippy::cast_precision_loss)]
        Literal::Int(value) => value as f64,
        Literal::Float(value) => value,
    }
}

/// The runtime form of a classical literal.
fn literal_operand(literal: Literal) -> (Operand, Prim) {
    match literal {
        Literal::Bool(value) => (Operand::Literal(RirLiteral::Bool(value)), Prim::Boolean),
        Literal::Int(value) => (Operand::Literal(RirLiteral::Integer(value)), Prim::Integer),
        Literal::Float(value) => (Operand::Literal(RirLiteral::Double(value)), Prim::Double),
    }
}

/// Narrows an integer literal used as a measurement comparison to a boolean.
fn narrow_to_bool(operand: &Operand) -> Result<Operand, Error> {
    match operand {
        Operand::Literal(RirLiteral::Integer(0)) => Ok(Operand::Literal(RirLiteral::Bool(false))),
        Operand::Literal(RirLiteral::Integer(1)) => Ok(Operand::Literal(RirLiteral::Bool(true))),
        _ => Err(Error::UnsupportedCondition {
            reason: "a measurement result can only be compared with 0 or 1",
        }),
    }
}
