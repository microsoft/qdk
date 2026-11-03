// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::ptr;

use qsc_circuit_store::{AngleExpr, AngleId, AngleOp, ParamValue, ParameterId};

use crate::{
    QDK_WRONG_OWNER, QdkAngle, QdkBytes, QdkCircuit, QdkDiagnostic, QdkParameter, QdkStatus,
    boundary::{Error, boundary, copy_out, input_slice, output, required, slot},
    handle::Handle,
};

pub const QDK_ANGLE_COPY: u32 = 0;
pub const QDK_ANGLE_ADD: u32 = 1;
pub const QDK_ANGLE_SUB: u32 = 2;
pub const QDK_ANGLE_MUL: u32 = 3;
pub const QDK_ANGLE_DIV: u32 = 4;
pub const QDK_ANGLE_NEG: u32 = 5;

/// A circuit-owned compile-time angle expression, not a runtime value.
pub struct QdkAngleExpression {
    pub(crate) circuit: *const QdkCircuit,
    pub(crate) inner: Handle<AngleId>,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct QdkAngleNode {
    /// `QDK_ANGLE_LITERAL`, `QDK_ANGLE_PARAMETER` or `QDK_ANGLE_EXPRESSION`.
    pub kind: u32,
    /// Arithmetic tag for expression nodes.
    pub operation: u32,
    pub literal: f64,
    pub parameter_id: u32,
    /// Earlier node positions in this copied, root-last array.
    pub left: u32,
    pub right: u32,
}

#[repr(C)]
pub struct QdkParameterBinding {
    pub parameter: *const QdkParameter,
    pub value: f64,
}

fn intern(value: ParamValue, circuit: &mut qdk_circuit_api::Circuit) -> AngleId {
    match value {
        ParamValue::Literal(value) => circuit.store_mut().push_angle(AngleExpr::Literal(value)),
        ParamValue::Parameter(id) => circuit.store_mut().push_angle(AngleExpr::Parameter(id)),
        ParamValue::Expr(id) => id,
    }
}

/// Builds existing compile-time arithmetic. COPY and NEG ignore right; binary
/// operators require it. Every referenced handle must belong to this circuit.
/// # Safety
/// Circuit and non-NULL input handles are live; outputs are disjoint writable slots.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_angle(
    circuit: *mut QdkCircuit,
    operation: u32,
    left: *const QdkAngle,
    right: *const QdkAngle,
    out: *mut *mut QdkAngleExpression,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides live disjoint handles, descriptors and outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let owner = required(circuit, "circuit")?;
            let mut value = owner.inner.access()?;
            let op = match operation {
                QDK_ANGLE_COPY | QDK_ANGLE_NEG => None,
                QDK_ANGLE_ADD => Some(AngleOp::Add),
                QDK_ANGLE_SUB => Some(AngleOp::Sub),
                QDK_ANGLE_MUL => Some(AngleOp::Mul),
                QDK_ANGLE_DIV => Some(AngleOp::Div),
                tag => {
                    return Err(Error::invalid(format!(
                        "unknown angle arithmetic tag {tag}"
                    )));
                }
            };
            let left = crate::operation::angle(required(left, "left angle")?, circuit)?;
            let right = op
                .map(|_| crate::operation::angle(required(right, "right angle")?, circuit))
                .transpose()?;
            let result = QdkAngleExpression {
                circuit,
                inner: Handle::child(AngleId(0), &owner.inner.lifetime, None)?,
            };
            let left = intern(left, &mut value);
            let id = match (op, right) {
                (Some(op), Some(right)) => {
                    let right = intern(right, &mut value);
                    value
                        .store_mut()
                        .push_angle(AngleExpr::Binary(op, left, right))
                }
                _ if operation == QDK_ANGLE_NEG => {
                    value.store_mut().push_angle(AngleExpr::Neg(left))
                }
                _ => left,
            };
            *result.inner.access()? = id;
            *out = crate::handle::publish(Box::new(result), |value| &value.inner);
            Ok(())
        })
    }
}

/// Copies reachable expression nodes in root-last order, using earlier array
/// positions for child references. Parameter IDs remain local to the circuit.
/// # Safety
/// Expression is live; output buffers cover capacity and are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_angle_expression_nodes(
    expression: *const QdkAngleExpression,
    data: *mut QdkAngleNode,
    capacity: usize,
    needed: *mut usize,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live registered expression and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            *output(needed)? = 0;
            let expression = required(expression, "angle expression")?;
            let value = required(expression.circuit, "circuit")?.inner.access()?;
            let id = *expression.inner.access()?;
            let extracted = qdk_circuit_api::AngleExpression::extract(value.store(), id);
            let nodes: Vec<_> = extracted
                .nodes()
                .iter()
                .map(|node| match *node {
                    AngleExpr::Literal(literal) => QdkAngleNode {
                        kind: crate::QDK_ANGLE_LITERAL,
                        literal,
                        ..Default::default()
                    },
                    AngleExpr::Parameter(id) => QdkAngleNode {
                        kind: crate::QDK_ANGLE_PARAMETER,
                        parameter_id: id.0,
                        ..Default::default()
                    },
                    AngleExpr::Neg(left) => QdkAngleNode {
                        kind: crate::QDK_ANGLE_EXPRESSION,
                        operation: QDK_ANGLE_NEG,
                        left: left.0,
                        ..Default::default()
                    },
                    AngleExpr::Binary(op, left, right) => QdkAngleNode {
                        kind: crate::QDK_ANGLE_EXPRESSION,
                        operation: match op {
                            AngleOp::Add => QDK_ANGLE_ADD,
                            AngleOp::Sub => QDK_ANGLE_SUB,
                            AngleOp::Mul => QDK_ANGLE_MUL,
                            AngleOp::Div => QDK_ANGLE_DIV,
                        },
                        left: left.0,
                        right: right.0,
                        ..Default::default()
                    },
                })
                .collect();
            copy_out(&nodes, data, capacity, needed)
        })
    }
}

/// Releases an expression handle, including during poisoned-owner teardown.
/// # Safety
/// Slot contains NULL or a live owning expression and is exclusively accessible.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_angle_expression_free(
    expression: *mut *mut QdkAngleExpression,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns this slot and any live handle it contains.
    unsafe {
        boundary(error, None, || {
            if expression.is_null() {
                return Ok(());
            }
            let slot = slot(expression, "expression slot")?;
            if !slot.is_null() {
                required(*slot, "expression")?.inner.require_releasable()?;
                let owned = *slot;
                *slot = ptr::null_mut();
                drop(Box::from_raw(owned));
            }
            Ok(())
        })
    }
}

/// Copies referenced parameter IDs in declaration order; unused declarations
/// are omitted. IDs are local to this circuit.
/// # Safety
/// Circuit is live; output buffers cover capacity and are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_parameters(
    circuit: *const QdkCircuit,
    data: *mut u32,
    capacity: usize,
    needed: *mut usize,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live circuit and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            *output(needed)? = 0;
            let value = required(circuit, "circuit")?.inner.access()?;
            let ids: Vec<_> = value
                .store()
                .referenced_parameters()
                .iter()
                .map(|id| id.0)
                .collect();
            copy_out(&ids, data, capacity, needed)
        })
    }
}

/// Obtains a dependent handle for an existing owner-local parameter ID.
/// # Safety
/// Circuit is live; output slots are writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_parameter_at(
    circuit: *mut QdkCircuit,
    id: u32,
    out: *mut *mut QdkParameter,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live circuit and disjoint output slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let owner = required(circuit, "circuit")?;
            let value = owner.inner.access()?;
            if value.store().parameters().get(id as usize).is_none() {
                return Err(Error::invalid(format!("parameter {id} does not exist")));
            }
            *out = crate::handle::publish(
                Box::new(QdkParameter {
                    circuit,
                    inner: Handle::child(ParameterId(id), &owner.inner.lifetime, None)?,
                }),
                |value| &value.inner,
            );
            Ok(())
        })
    }
}

/// Copies a parameter's name as owned UTF-8 bytes.
/// # Safety
/// Parameter is live; output slots are writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_parameter_name(
    parameter: *const QdkParameter,
    out: *mut QdkBytes,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: the live dependent keeps its circuit allocation registered.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let parameter = required(parameter, "parameter")?;
            let value = required(parameter.circuit, "circuit")?.inner.access()?;
            let id = *parameter.inner.access()?;
            let name = &value
                .store()
                .parameters()
                .get(id.0 as usize)
                .ok_or_else(|| Error::invalid("parameter does not exist"))?
                .name;
            *out = name.as_bytes().to_vec().into();
            Ok(())
        })
    }
}

/// Returns a fully bound new circuit without modifying the input. Requires a
/// closed circuit and values for all referenced non-loop parameters.
/// # Safety
/// Circuit, bindings and parameter handles are live; outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_bind(
    circuit: *const QdkCircuit,
    bindings: *const QdkParameterBinding,
    count: usize,
    out: *mut *mut QdkCircuit,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: all input allocations and handles satisfy the caller contract.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let value = required(circuit, "circuit")?.inner.access()?;
            value.require_closed("bind").map_err(Error::from)?;
            let bindings: Vec<_> = input_slice(bindings, count)?
                .iter()
                .map(|binding| {
                    let parameter = required(binding.parameter, "parameter")?;
                    if parameter.circuit != circuit {
                        return Err(Error::new(
                            QDK_WRONG_OWNER,
                            "binding parameter belongs to another circuit",
                        ));
                    }
                    Ok((*parameter.inner.access()?, binding.value))
                })
                .collect::<Result<_, Error>>()?;
            let bound = value.bind_parameters(&bindings).map_err(Error::from)?;
            *out = Box::into_raw(Box::new(QdkCircuit {
                inner: Handle::new(bound),
            }));
            Ok(())
        })
    }
}
