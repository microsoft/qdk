// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_circuit_store::{BinaryOp, Expr, ExprId, Literal, VarId, VarType};

use crate::{
    QDK_WRONG_OWNER, QdkCircuit, QdkDiagnostic, QdkStatus,
    boundary::{Error, boundary, input_text, output, release, required},
    handle::Handle,
};

pub const QDK_VALUE_BOOL: u32 = 1;
pub const QDK_VALUE_INT: u32 = 2;
pub const QDK_VALUE_FLOAT: u32 = 3;
pub const QDK_EXPR_LITERAL: u32 = 1;
pub const QDK_EXPR_CLBIT: u32 = 2;
pub const QDK_EXPR_VARIABLE: u32 = 3;
pub const QDK_EXPR_NOT: u32 = 4;
pub const QDK_EXPR_BINARY: u32 = 5;
pub const QDK_EXPR_REGISTER_EQUAL: u32 = 6;
pub const QDK_BINARY_EQ: u32 = 1;
pub const QDK_BINARY_NE: u32 = 2;
pub const QDK_BINARY_LT: u32 = 3;
pub const QDK_BINARY_LE: u32 = 4;
pub const QDK_BINARY_GT: u32 = 5;
pub const QDK_BINARY_GE: u32 = 6;
pub const QDK_BINARY_AND: u32 = 7;
pub const QDK_BINARY_OR: u32 = 8;
pub const QDK_BINARY_ADD: u32 = 9;
pub const QDK_BINARY_SUB: u32 = 10;
pub const QDK_BINARY_MUL: u32 = 11;

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct QdkLiteral {
    pub kind: u32,
    pub boolean: u8,
    pub integer: i64,
    pub real: f64,
}

impl QdkLiteral {
    pub(crate) fn decode(self) -> Result<Literal, Error> {
        match self.kind {
            QDK_VALUE_BOOL if self.boolean <= 1 => Ok(Literal::Bool(self.boolean != 0)),
            QDK_VALUE_INT => Ok(Literal::Int(self.integer)),
            QDK_VALUE_FLOAT => Ok(Literal::Float(self.real)),
            _ => Err(Error::invalid("invalid literal kind or boolean value")),
        }
    }
}

impl From<Literal> for QdkLiteral {
    fn from(value: Literal) -> Self {
        match value {
            Literal::Bool(boolean) => Self {
                kind: QDK_VALUE_BOOL,
                boolean: u8::from(boolean),
                ..Default::default()
            },
            Literal::Int(integer) => Self {
                kind: QDK_VALUE_INT,
                integer,
                ..Default::default()
            },
            Literal::Float(real) => Self {
                kind: QDK_VALUE_FLOAT,
                real,
                ..Default::default()
            },
        }
    }
}

pub struct QdkVariable {
    pub(crate) circuit: *const QdkCircuit,
    pub(crate) inner: Handle<VarId>,
}

#[derive(Default)]
#[repr(C)]
pub struct QdkVariableInfo {
    pub id: u32,
    pub value_type: u32,
    pub name: crate::QdkBytes,
}

pub struct QdkExpression {
    pub(crate) circuit: *const QdkCircuit,
    pub(crate) inner: Handle<(ExprId, VarType)>,
}

/// Initialize unused fields to zero. `REGISTER_EQUAL` uses `register_name` and
/// `register_value`; it preserves the facade's per-bit register condition policy.
#[derive(Default)]
#[repr(C)]
pub struct QdkExpressionInput {
    pub kind: u32,
    pub literal: QdkLiteral,
    pub clbit: u32,
    pub variable: *const QdkVariable,
    pub operation: u32,
    pub left: *const QdkExpression,
    pub right: *const QdkExpression,
    pub register_name: *const u8,
    pub register_name_len: usize,
    pub register_value: i64,
}

/// Copied node fields; operand and variable IDs remain local to the circuit.
/// Use `qdk_circuit_variable_at` with the same circuit to recover variable metadata.
#[derive(Default)]
#[repr(C)]
pub struct QdkExpressionInfo {
    pub kind: u32,
    pub value_type: u32,
    pub literal: QdkLiteral,
    pub clbit: u32,
    pub variable_id: u32,
    pub operation: u32,
    pub left_id: u32,
    pub right_id: u32,
}

pub(crate) const fn type_tag(ty: VarType) -> u32 {
    match ty {
        VarType::Bool => QDK_VALUE_BOOL,
        VarType::Int => QDK_VALUE_INT,
        VarType::Float => QDK_VALUE_FLOAT,
    }
}

fn literal_type(literal: Literal) -> VarType {
    match literal {
        Literal::Bool(_) => VarType::Bool,
        Literal::Int(_) => VarType::Int,
        Literal::Float(_) => VarType::Float,
    }
}

pub(crate) fn expression_type(
    store: &qsc_circuit_store::CircuitStore,
    mut id: ExprId,
) -> Result<VarType, Error> {
    for _ in 0..store.exprs().nodes().len() {
        match *store
            .exprs()
            .get(id)
            .ok_or_else(|| Error::invalid("expression does not exist"))?
        {
            Expr::Literal(literal) => return Ok(literal_type(literal)),
            Expr::Var(id) => {
                return store
                    .vars()
                    .get(id.0 as usize)
                    .map(|v| v.ty)
                    .ok_or_else(|| Error::invalid("variable does not exist"));
            }
            Expr::Binary(BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul, left, _) => id = left,
            Expr::Clbit(_) | Expr::Not(_) | Expr::Binary(_, _, _) => return Ok(VarType::Bool),
        }
    }
    Err(Error::invalid("runtime expression contains a cycle"))
}

/// Declares existing native typed runtime state. Target support remains a
/// compiler concern; declaring a variable does not initialize its runtime value.
/// # Safety
/// Circuit and name bytes are live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_variable(
    circuit: *mut QdkCircuit,
    name: *const u8,
    name_len: usize,
    value_type: u32,
    out: *mut *mut QdkVariable,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides live disjoint handles, input bytes and output slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let owner = required(circuit, "circuit")?;
            let mut value = owner.inner.access()?;
            let ty = match value_type {
                QDK_VALUE_BOOL => VarType::Bool,
                QDK_VALUE_INT => VarType::Int,
                QDK_VALUE_FLOAT => VarType::Float,
                tag => {
                    return Err(Error::invalid(format!(
                        "unknown runtime variable type {tag}"
                    )));
                }
            };
            let name = input_text(name, name_len)?;
            let variable = QdkVariable {
                circuit,
                inner: Handle::child(VarId(0), &owner.inner.lifetime, None)?,
            };
            *variable.inner.access()? = value.store_mut().declare_var(name, ty);
            *out = crate::handle::publish(Box::new(variable), |value| &value.inner);
            Ok(())
        })
    }
}

/// Looks up an existing runtime variable by owner-local ID without declaring
/// or changing it. IDs from loop metadata or expression nodes must be used
/// with their original circuit; they are not cross-circuit identities.
/// An unknown ID returns `QDK_INVALID_ARGUMENT`. Release the dependent handle
/// before its circuit, or let its callback scope dispose it.
/// # Safety
/// Circuit is live; writable output slots are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_variable_at(
    circuit: *mut QdkCircuit,
    id: u32,
    out: *mut *mut QdkVariable,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live circuit and disjoint output slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let owner = required(circuit, "circuit")?;
            let value = owner.inner.access()?;
            if value.store().vars().get(id as usize).is_none() {
                return Err(Error::invalid(format!("variable {id} does not exist")));
            }
            *out = crate::handle::publish(
                Box::new(QdkVariable {
                    circuit,
                    inner: Handle::child(VarId(id), &owner.inner.lifetime, None)?,
                }),
                |value| &value.inner,
            );
            Ok(())
        })
    }
}

/// Copies variable identity, declared type and owned UTF-8 name.
/// # Safety
/// Variable is live; writable output slots are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_variable_info(
    variable: *const QdkVariable,
    out: *mut QdkVariableInfo,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: the live registered variable retains its root circuit.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let variable = required(variable, "variable")?;
            let value = required(variable.circuit, "circuit")?.inner.access()?;
            let id = *variable.inner.access()?;
            let var = value
                .store()
                .vars()
                .get(id.0 as usize)
                .ok_or_else(|| Error::invalid("variable does not exist"))?;
            *out = QdkVariableInfo {
                id: id.0,
                value_type: type_tag(var.ty),
                name: var.name.as_bytes().to_vec().into(),
            };
            Ok(())
        })
    }
}

/// Returns a registered child expression for traversal. Unary nodes accept
/// index zero; binary nodes accept zero and one.
/// # Safety
/// Expression is live; output slots are writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_expression_operand(
    expression: *const QdkExpression,
    index: usize,
    out: *mut *mut QdkExpression,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: the live registered expression retains its root circuit.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let expression = required(expression, "expression")?;
            let owner = required(expression.circuit, "circuit")?;
            let value = owner.inner.access()?;
            let (id, _) = *expression.inner.access()?;
            let node = *value
                .store()
                .exprs()
                .get(id)
                .ok_or_else(|| Error::invalid("expression does not exist"))?;
            let ((Expr::Not(child) | Expr::Binary(_, child, _), 0)
            | (Expr::Binary(_, _, child), 1)) = (node, index)
            else {
                return Err(Error::invalid("expression operand index does not exist"));
            };
            let ty = expression_type(value.store(), child)?;
            *out = crate::handle::publish(
                Box::new(QdkExpression {
                    circuit: expression.circuit,
                    inner: Handle::child((child, ty), &owner.inner.lifetime, None)?,
                }),
                |value| &value.inner,
            );
            Ok(())
        })
    }
}

pub(crate) fn binary_tag(op: BinaryOp) -> u32 {
    match op {
        BinaryOp::Eq => QDK_BINARY_EQ,
        BinaryOp::Ne => QDK_BINARY_NE,
        BinaryOp::Lt => QDK_BINARY_LT,
        BinaryOp::Le => QDK_BINARY_LE,
        BinaryOp::Gt => QDK_BINARY_GT,
        BinaryOp::Ge => QDK_BINARY_GE,
        BinaryOp::And => QDK_BINARY_AND,
        BinaryOp::Or => QDK_BINARY_OR,
        BinaryOp::Add => QDK_BINARY_ADD,
        BinaryOp::Sub => QDK_BINARY_SUB,
        BinaryOp::Mul => QDK_BINARY_MUL,
    }
}

fn binary(tag: u32, left: VarType, right: VarType) -> Result<(BinaryOp, VarType), Error> {
    let op = match tag {
        QDK_BINARY_EQ => BinaryOp::Eq,
        QDK_BINARY_NE => BinaryOp::Ne,
        QDK_BINARY_LT => BinaryOp::Lt,
        QDK_BINARY_LE => BinaryOp::Le,
        QDK_BINARY_GT => BinaryOp::Gt,
        QDK_BINARY_GE => BinaryOp::Ge,
        QDK_BINARY_AND => BinaryOp::And,
        QDK_BINARY_OR => BinaryOp::Or,
        QDK_BINARY_ADD => BinaryOp::Add,
        QDK_BINARY_SUB => BinaryOp::Sub,
        QDK_BINARY_MUL => BinaryOp::Mul,
        _ => {
            return Err(Error::invalid(format!(
                "unknown runtime binary operation {tag}"
            )));
        }
    };
    if left != right {
        return Err(Error::invalid(
            "runtime binary operands must have matching types",
        ));
    }
    let ty = match op {
        BinaryOp::Eq | BinaryOp::Ne => VarType::Bool,
        BinaryOp::And | BinaryOp::Or if left == VarType::Bool => VarType::Bool,
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge if left != VarType::Bool => {
            VarType::Bool
        }
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul if left != VarType::Bool => left,
        _ => {
            return Err(Error::invalid(
                "runtime operands have the wrong type for this operation",
            ));
        }
    };
    Ok((op, ty))
}

pub(crate) unsafe fn expression(
    value: *const QdkExpression,
    circuit: *const QdkCircuit,
) -> Result<(ExprId, VarType), Error> {
    // SAFETY: handles are live by the enclosing exported function's contract.
    let value = unsafe { required(value, "expression")? };
    if value.circuit != circuit {
        return Err(Error::new(
            QDK_WRONG_OWNER,
            "expression belongs to another circuit",
        ));
    }
    Ok(*value.inner.access()?)
}

/// Creates an existing native runtime expression with structural/type checks.
/// This does not promise that a target compiler supports the resulting program.
/// # Safety
/// Circuit and descriptor inputs are live; outputs are writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_expression(
    circuit: *mut QdkCircuit,
    input: *const QdkExpressionInput,
    out: *mut *mut QdkExpression,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides live disjoint descriptors, handles and outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let owner = required(circuit, "circuit")?;
            let mut value = owner.inner.access()?;
            let input = required(input, "expression input")?;
            let result = QdkExpression {
                circuit,
                inner: Handle::child((ExprId(0), VarType::Bool), &owner.inner.lifetime, None)?,
            };
            let (node, ty) = match input.kind {
                QDK_EXPR_LITERAL => {
                    let literal = input.literal.decode()?;
                    (Expr::Literal(literal), literal_type(literal))
                }
                QDK_EXPR_CLBIT => {
                    if input.clbit >= value.store().clbit_count() {
                        return Err(Error::invalid("classical bit is outside this circuit"));
                    }
                    (Expr::Clbit(input.clbit), VarType::Bool)
                }
                QDK_EXPR_VARIABLE => {
                    let variable = required(input.variable, "variable")?;
                    if variable.circuit != circuit {
                        return Err(Error::new(
                            QDK_WRONG_OWNER,
                            "variable belongs to another circuit",
                        ));
                    }
                    let id = *variable.inner.access()?;
                    let ty = value
                        .store()
                        .vars()
                        .get(id.0 as usize)
                        .ok_or_else(|| Error::invalid("variable does not exist"))?
                        .ty;
                    (Expr::Var(id), ty)
                }
                QDK_EXPR_NOT => {
                    let (id, ty) = expression(input.left, circuit)?;
                    if ty != VarType::Bool {
                        return Err(Error::invalid("logical not requires a boolean"));
                    }
                    (Expr::Not(id), VarType::Bool)
                }
                QDK_EXPR_BINARY => {
                    let (left, left_ty) = expression(input.left, circuit)?;
                    let (right, right_ty) = expression(input.right, circuit)?;
                    let (op, ty) = binary(input.operation, left_ty, right_ty)?;
                    (Expr::Binary(op, left, right), ty)
                }
                QDK_EXPR_REGISTER_EQUAL => {
                    let name = input_text(input.register_name, input.register_name_len)?;
                    let id = value
                        .register_condition(name, input.register_value)
                        .map_err(Error::from)?;
                    *result.inner.access()? = (id, VarType::Bool);
                    *out = crate::handle::publish(Box::new(result), |value| &value.inner);
                    return Ok(());
                }
                tag => {
                    return Err(Error::invalid(format!(
                        "unknown runtime expression kind {tag}"
                    )));
                }
            };
            let id = value.store_mut().push_expr(node);
            *result.inner.access()? = (id, ty);
            *out = crate::handle::publish(Box::new(result), |value| &value.inner);
            Ok(())
        })
    }
}

/// Copies runtime expression node fields without exposing a native borrow.
/// # Safety
/// Expression is live; output slots are writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_expression_info(
    expression: *const QdkExpression,
    out: *mut QdkExpressionInfo,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live registered expression and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let expression = required(expression, "expression")?;
            let value = required(expression.circuit, "circuit")?.inner.access()?;
            let (id, ty) = *expression.inner.access()?;
            let node = value
                .store()
                .exprs()
                .get(id)
                .ok_or_else(|| Error::invalid("expression does not exist"))?;
            out.value_type = type_tag(ty);
            out.kind = match *node {
                Expr::Literal(literal) => {
                    out.literal = literal.into();
                    QDK_EXPR_LITERAL
                }
                Expr::Clbit(clbit) => {
                    out.clbit = clbit;
                    QDK_EXPR_CLBIT
                }
                Expr::Var(id) => {
                    out.variable_id = id.0;
                    QDK_EXPR_VARIABLE
                }
                Expr::Not(id) => {
                    out.left_id = id.0;
                    QDK_EXPR_NOT
                }
                Expr::Binary(op, left, right) => {
                    out.operation = binary_tag(op);
                    out.left_id = left.0;
                    out.right_id = right.0;
                    QDK_EXPR_BINARY
                }
            };
            Ok(())
        })
    }
}

/// Releases an expression, including during poisoned-owner teardown.
/// # Safety
/// Slot contains NULL or a live owning expression and is exclusively accessible.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_expression_free(
    expression: *mut *mut QdkExpression,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns the slot and any live handle it contains.
    unsafe { boundary(error, None, || release(expression, |value| &value.inner)) }
}

/// Releases a runtime variable handle.
/// # Safety
/// Slot contains NULL or a live owning variable and is exclusively accessible.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_variable_free(
    variable: *mut *mut QdkVariable,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns the slot and any live handle it contains.
    unsafe { boundary(error, None, || release(variable, |value| &value.inner)) }
}
