// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{AngleExpression, Circuit};
use qsc_circuit_store::{
    CircuitStore, ControlFlow, Expr, ExprId, LoopIndex, PackedOperation, ParamValue,
};

#[derive(Clone, Debug)]
pub enum StoredAngle {
    Literal(f64),
    Symbol {
        id: qsc_circuit_store::ParameterId,
        name: String,
    },
    Expression(AngleExpression),
}

#[derive(Clone, Debug)]
pub struct ConditionView {
    /// One of `literal`, `clbit`, `variable`, `not`, or `binary`.
    pub kind: String,
    pub op: Option<String>,
    pub clbit: Option<u32>,
    pub name: Option<String>,
    pub value: Option<qsc_circuit_store::Literal>,
    pub operands: Vec<ConditionView>,
}

/// How deep a condition may nest before reading it is refused.
///
/// A condition is built a few nodes at a time by the authoring surface, so this
/// only bounds trees arriving from a restored document, where nothing else
/// limits how deeply they nest.
const MAX_CONDITION_DEPTH: usize = 64;

fn condition_view(store: &CircuitStore, expr: ExprId, depth: usize) -> Option<ConditionView> {
    if depth >= MAX_CONDITION_DEPTH {
        return None;
    }
    let leaf = |kind: &str| ConditionView {
        kind: kind.to_string(),
        op: None,
        clbit: None,
        name: None,
        value: None,
        operands: Vec::new(),
    };
    Some(match *store.exprs().get(expr)? {
        Expr::Literal(literal) => ConditionView {
            value: Some(literal),
            ..leaf("literal")
        },
        Expr::Clbit(index) => ConditionView {
            clbit: Some(index),
            ..leaf("clbit")
        },
        Expr::Var(id) => ConditionView {
            name: Some(store.vars().get(id.0 as usize)?.name.clone()),
            ..leaf("variable")
        },
        Expr::Not(inner) => ConditionView {
            operands: vec![condition_view(store, inner, depth + 1)?],
            ..leaf("not")
        },
        Expr::Binary(op, left, right) => ConditionView {
            op: Some(binary_op_name(op).to_string()),
            operands: vec![
                condition_view(store, left, depth + 1)?,
                condition_view(store, right, depth + 1)?,
            ],
            ..leaf("binary")
        },
    })
}

const fn binary_op_name(op: qsc_circuit_store::BinaryOp) -> &'static str {
    use qsc_circuit_store::BinaryOp;
    match op {
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
    }
}

/// The expression a control-flow construct branches on, if it has one.
const fn control_condition(control: &ControlFlow) -> Option<ExprId> {
    match control {
        ControlFlow::IfElse(if_else) => Some(if_else.condition),
        ControlFlow::While(loop_) => Some(loop_.condition),
        ControlFlow::Switch(switch) => Some(switch.selector),
        ControlFlow::For(_)
        | ControlFlow::Box(_)
        | ControlFlow::Moment(_)
        | ControlFlow::Break
        | ControlFlow::Continue => None,
    }
}

#[derive(Clone, Debug)]
pub struct InstructionView {
    pub name: String,
    pub qubits: Vec<u32>,
    pub clbits: Vec<u32>,
    pub params: Vec<StoredAngle>,
    /// Blocks a control-flow construct transfers into, empty for anything else.
    ///
    /// Reported as identifiers rather than as their contents, so that reading
    /// an instruction costs the same whatever it contains.
    pub blocks: Vec<u32>,
    pub condition: Option<ConditionView>,
    /// Values a for loop runs over.
    pub values: Vec<qsc_circuit_store::Literal>,
    /// Name of the index a for loop binds.
    pub loop_index: Option<String>,
    /// Label carried by a box.
    pub label: Option<String>,
    /// Values each case of a switch matches, aligned with the leading blocks.
    pub cases: Vec<Vec<qsc_circuit_store::Literal>>,
    /// Block a switch takes when no case matches.
    pub default_block: Option<u32>,
}

impl Circuit {
    /// Builds a read-only view of a stored instruction.
    pub fn view_of(&self, instruction: &qsc_circuit_store::Instruction) -> InstructionView {
        let control = match &instruction.operation {
            PackedOperation::ControlFlow(control) => Some(control.as_ref()),
            _ => None,
        };
        InstructionView {
            name: instruction.operation.name().to_string(),
            qubits: self
                .store
                .qubits_of(instruction)
                .unwrap_or(&[])
                .iter()
                .map(|q| q.0)
                .collect(),
            clbits: self
                .store
                .clbits_of(instruction)
                .unwrap_or(&[])
                .iter()
                .map(|c| c.0)
                .collect(),
            params: self.stored_angles(instruction),
            blocks: control.map_or_else(Vec::new, |control| {
                control.blocks().into_iter().map(|block| block.0).collect()
            }),
            condition: control
                .and_then(control_condition)
                .and_then(|expr| condition_view(&self.store, expr, 0)),
            values: match control {
                Some(ControlFlow::For(loop_)) => loop_.values.clone(),
                _ => Vec::new(),
            },
            loop_index: match control {
                Some(ControlFlow::For(loop_)) => loop_.index.map(|index| match index {
                    LoopIndex::Parameter(id) => self.store.parameters()[id.0 as usize].name.clone(),
                    LoopIndex::Var(id) => self.store.vars()[id.0 as usize].name.clone(),
                }),
                _ => None,
            },
            label: match control {
                Some(ControlFlow::Box(boxed)) => boxed.label.clone(),
                _ => None,
            },
            cases: match control {
                Some(ControlFlow::Switch(switch)) => switch
                    .cases
                    .iter()
                    .map(|(values, _)| values.clone())
                    .collect(),
                _ => Vec::new(),
            },
            default_block: match control {
                Some(ControlFlow::Switch(switch)) => switch.default.map(|block| block.0),
                _ => None,
            },
        }
    }

    /// Resolves an instruction's angles into their Python forms.
    fn stored_angles(&self, instruction: &qsc_circuit_store::Instruction) -> Vec<StoredAngle> {
        instruction
            .params
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|value| match value {
                ParamValue::Literal(literal) => StoredAngle::Literal(*literal),
                ParamValue::Parameter(id) => StoredAngle::Symbol {
                    id: *id,
                    name: self.store.parameters()[id.0 as usize].name.clone(),
                },
                ParamValue::Expr(id) => {
                    StoredAngle::Expression(AngleExpression::extract(&self.store, *id))
                }
            })
            .collect()
    }
}
