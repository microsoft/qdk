// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_qhdg::{Angle, AngleOp, NodeOrigin, Type};

use crate::{
    QDK_ANGLE_ADD, QDK_ANGLE_COPY, QDK_ANGLE_DIV, QDK_ANGLE_MUL, QDK_ANGLE_NEG, QDK_ANGLE_SUB,
    QDK_WRONG_VARIANT, QdkBytes, QdkDiagnostic, QdkStatus,
    boundary::{Error, boundary, input_slice, input_text, output, release, required},
    handle::Handle,
};

use super::tags::{
    FIR_KINDS, QDK_ORIGIN_CLBIT, QDK_ORIGIN_FIR, QDK_ORIGIN_GLOBAL_PHASE, QDK_ORIGIN_INSTRUCTION,
    QDK_ORIGIN_NONE, QDK_ORIGIN_QUBIT, decode, encode,
};

pub const QDK_TYPE_UNIT: u32 = 1;
pub const QDK_TYPE_BOOL: u32 = 2;
pub const QDK_TYPE_INT: u32 = 3;
pub const QDK_TYPE_DOUBLE: u32 = 4;
pub const QDK_TYPE_STRING: u32 = 5;
pub const QDK_TYPE_RESULT: u32 = 6;
pub const QDK_TYPE_QUBIT: u32 = 7;
pub const QDK_TYPE_UNKNOWN: u32 = 8;
pub const QDK_TYPE_ARRAY: u32 = 9;
pub const QDK_TYPE_TUPLE: u32 = 10;
pub const QDK_TYPE_LOCATION: u32 = 11;
pub const QDK_GRAPH_ANGLE_LITERAL: u32 = 1;
pub const QDK_GRAPH_ANGLE_PARAMETER: u32 = 2;
pub const QDK_GRAPH_ANGLE_EXPRESSION: u32 = 3;

/// Length-delimited borrowed UTF-8 input; NULL is accepted when len is zero.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct QdkString {
    pub data: *const u8,
    pub len: usize,
}

impl QdkString {
    pub(crate) unsafe fn read<'a>(self) -> Result<&'a str, Error> {
        // SAFETY: caller supplies the initialized byte extent for this call.
        unsafe { input_text(self.data, self.len) }
    }

    pub(crate) fn borrowed(text: &str) -> Self {
        Self {
            data: text.as_ptr(),
            len: text.len(),
        }
    }
}

/// Independent owned graph type. Nested elements are copied, not borrowed.
pub struct QdkGraphType {
    pub(super) inner: Handle<Type>,
}

#[derive(Default)]
#[repr(C)]
pub struct QdkGraphTypeInfo {
    pub kind: u32,
    pub child_count: usize,
}

/// Independent named graph angle, not a circuit-owned parameter expression.
pub struct QdkGraphAngle {
    pub(super) inner: Handle<Angle>,
}

#[repr(C)]
pub struct QdkGraphAngleInput {
    pub kind: u32,
    pub literal: f64,
    pub name: QdkString,
    /// Existing `QDK_ANGLE` arithmetic tags. COPY and NEG ignore right.
    pub operation: u32,
    pub left: *const QdkGraphAngle,
    pub right: *const QdkGraphAngle,
}

#[derive(Default)]
#[repr(C)]
pub struct QdkGraphAngleInfo {
    pub kind: u32,
    pub literal: f64,
    pub operation: u32,
    pub child_count: usize,
    pub parameter_count: usize,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct QdkGraphOrigin {
    /// `QDK_ORIGIN_NONE` indicates no origin.
    pub kind: u32,
    pub index: u32,
    /// Used only for `QDK_ORIGIN_FIR`.
    pub package: u32,
    pub fir_kind: u32,
}

impl QdkGraphOrigin {
    pub(super) fn native(self) -> Result<Option<NodeOrigin>, Error> {
        Ok(Some(match self.kind {
            QDK_ORIGIN_NONE => return Ok(None),
            QDK_ORIGIN_INSTRUCTION => NodeOrigin::Instruction(self.index),
            QDK_ORIGIN_QUBIT => NodeOrigin::Qubit(self.index),
            QDK_ORIGIN_CLBIT => NodeOrigin::Clbit(self.index),
            QDK_ORIGIN_GLOBAL_PHASE => NodeOrigin::GlobalPhase,
            QDK_ORIGIN_FIR => NodeOrigin::Fir {
                package: self.package,
                kind: decode(self.fir_kind, FIR_KINDS)?,
                local_id: self.index,
            },
            tag => return Err(Error::invalid(format!("unknown origin tag {tag}"))),
        }))
    }

    pub(super) fn of(origin: Option<NodeOrigin>) -> Result<Self, Error> {
        let mut result = Self::default();
        let (kind, index) = match origin {
            None => (QDK_ORIGIN_NONE, 0),
            Some(NodeOrigin::Instruction(index)) => (QDK_ORIGIN_INSTRUCTION, index),
            Some(NodeOrigin::Qubit(index)) => (QDK_ORIGIN_QUBIT, index),
            Some(NodeOrigin::Clbit(index)) => (QDK_ORIGIN_CLBIT, index),
            Some(NodeOrigin::GlobalPhase) => (QDK_ORIGIN_GLOBAL_PHASE, 0),
            Some(NodeOrigin::Fir {
                package,
                kind,
                local_id,
            }) => {
                result.package = package;
                result.fir_kind = encode(kind, FIR_KINDS)?;
                (QDK_ORIGIN_FIR, local_id)
            }
        };
        result.kind = kind;
        result.index = index;
        Ok(result)
    }
}

/// Creates an owned type. ARRAY/LOCATION take one child, TUPLE takes any
/// number, and primitive kinds take none. Children remain caller-owned.
/// # Safety
/// Child array and handles are live; writable output slots are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_type_new(
    kind: u32,
    children: *const *const QdkGraphType,
    count: usize,
    out: *mut *mut QdkGraphType,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller guarantees input extent and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let children = input_slice(children, count)?;
            let ty = match kind {
                QDK_TYPE_ARRAY | QDK_TYPE_LOCATION => {
                    if children.len() != 1 {
                        return Err(Error::invalid(
                            "array/location needs exactly one child type",
                        ));
                    }
                    let child =
                        Box::new(required(children[0], "child type")?.inner.access()?.clone());
                    if kind == QDK_TYPE_ARRAY {
                        Type::Array(child)
                    } else {
                        Type::Location(child)
                    }
                }
                QDK_TYPE_TUPLE => Type::Tuple(
                    children
                        .iter()
                        .map(|&child| Ok(required(child, "child type")?.inner.access()?.clone()))
                        .collect::<Result<_, Error>>()?,
                ),
                _ => {
                    if !children.is_empty() {
                        return Err(Error::invalid("primitive type cannot have children"));
                    }
                    match kind {
                        QDK_TYPE_UNIT => Type::Unit,
                        QDK_TYPE_BOOL => Type::Bool,
                        QDK_TYPE_INT => Type::Int,
                        QDK_TYPE_DOUBLE => Type::Double,
                        QDK_TYPE_STRING => Type::String,
                        QDK_TYPE_RESULT => Type::Result,
                        QDK_TYPE_QUBIT => Type::Qubit,
                        QDK_TYPE_UNKNOWN => Type::Unknown,
                        _ => return Err(Error::invalid(format!("unknown graph type {kind}"))),
                    }
                }
            };
            *out = Box::into_raw(Box::new(QdkGraphType {
                inner: Handle::new(ty),
            }));
            Ok(())
        })
    }
}

/// Copies the type's kind and number of nested children.
/// # Safety
/// Type is live; output slots are writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_type_info(
    value: *const QdkGraphType,
    out: *mut QdkGraphTypeInfo,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: live handle and disjoint output slots are caller obligations.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let value = required(value, "type")?.inner.access()?;
            let (kind, child_count) = match &*value {
                Type::Unit => (QDK_TYPE_UNIT, 0),
                Type::Bool => (QDK_TYPE_BOOL, 0),
                Type::Int => (QDK_TYPE_INT, 0),
                Type::Double => (QDK_TYPE_DOUBLE, 0),
                Type::String => (QDK_TYPE_STRING, 0),
                Type::Result => (QDK_TYPE_RESULT, 0),
                Type::Qubit => (QDK_TYPE_QUBIT, 0),
                Type::Unknown => (QDK_TYPE_UNKNOWN, 0),
                Type::Array(_) => (QDK_TYPE_ARRAY, 1),
                Type::Location(_) => (QDK_TYPE_LOCATION, 1),
                Type::Tuple(elements) => (QDK_TYPE_TUPLE, elements.len()),
            };
            *out = QdkGraphTypeInfo { kind, child_count };
            Ok(())
        })
    }
}

/// Copies a nested type into an independent owning handle.
/// # Safety
/// Type is live; output slots are writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_type_child(
    value: *const QdkGraphType,
    index: usize,
    out: *mut *mut QdkGraphType,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: live handle and disjoint outputs are caller obligations.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let value = required(value, "type")?.inner.access()?;
            let child = match &*value {
                Type::Array(child) | Type::Location(child) if index == 0 => Some(child.as_ref()),
                Type::Tuple(children) => children.get(index),
                _ => None,
            }
            .ok_or_else(|| Error::invalid("type child index is out of range"))?;
            *out = Box::into_raw(Box::new(QdkGraphType {
                inner: Handle::new(child.clone()),
            }));
            Ok(())
        })
    }
}

/// Releases an independent graph type and clears its slot.
/// # Safety
/// Slot contains NULL or a live exclusively owned type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_type_free(
    value: *mut *mut QdkGraphType,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns the slot and its live allocation.
    unsafe { boundary(error, None, || release(value, |value| &value.inner)) }
}

/// Creates a named literal/parameter/arithmetic angle, copying all operands.
/// # Safety
/// Descriptor and its selected inputs are live; output slots are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_angle_new(
    input: *const QdkGraphAngleInput,
    out: *mut *mut QdkGraphAngle,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller guarantees selected input extent and live handles.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let input = required(input, "angle input")?;
            let angle = match input.kind {
                QDK_GRAPH_ANGLE_LITERAL => Angle::literal(input.literal),
                QDK_GRAPH_ANGLE_PARAMETER => Angle::Parameter {
                    name: input.name.read()?.to_owned(),
                },
                QDK_GRAPH_ANGLE_EXPRESSION => {
                    let left = required(input.left, "left angle")?.inner.access()?.clone();
                    match input.operation {
                        QDK_ANGLE_COPY => left,
                        QDK_ANGLE_NEG => Angle::Neg {
                            operand: Box::new(left),
                        },
                        operation => {
                            let op = match operation {
                                QDK_ANGLE_ADD => AngleOp::Add,
                                QDK_ANGLE_SUB => AngleOp::Sub,
                                QDK_ANGLE_MUL => AngleOp::Mul,
                                QDK_ANGLE_DIV => AngleOp::Div,
                                _ => return Err(Error::invalid("unknown graph angle arithmetic")),
                            };
                            let right = required(input.right, "right angle")?
                                .inner
                                .access()?
                                .clone();
                            Angle::Binary {
                                op,
                                left: Box::new(left),
                                right: Box::new(right),
                            }
                        }
                    }
                }
                _ => return Err(Error::invalid("unknown graph angle kind")),
            };
            *out = Box::into_raw(Box::new(QdkGraphAngle {
                inner: Handle::new(angle),
            }));
            Ok(())
        })
    }
}

/// Copies angle shape, literal value and referenced-parameter count.
/// # Safety
/// Angle is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_angle_info(
    angle: *const QdkGraphAngle,
    out: *mut QdkGraphAngleInfo,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: live handle and disjoint output slots are caller obligations.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let angle = required(angle, "angle")?.inner.access()?;
            let mut info = QdkGraphAngleInfo {
                parameter_count: angle.parameters().len(),
                ..Default::default()
            };
            match &*angle {
                Angle::Literal { value } => {
                    info.kind = QDK_GRAPH_ANGLE_LITERAL;
                    info.literal = value.get();
                }
                Angle::Parameter { .. } => info.kind = QDK_GRAPH_ANGLE_PARAMETER,
                Angle::Neg { .. } => {
                    info.kind = QDK_GRAPH_ANGLE_EXPRESSION;
                    info.operation = QDK_ANGLE_NEG;
                    info.child_count = 1;
                }
                Angle::Binary { op, .. } => {
                    info.kind = QDK_GRAPH_ANGLE_EXPRESSION;
                    info.operation = match op {
                        AngleOp::Add => QDK_ANGLE_ADD,
                        AngleOp::Sub => QDK_ANGLE_SUB,
                        AngleOp::Mul => QDK_ANGLE_MUL,
                        AngleOp::Div => QDK_ANGLE_DIV,
                    };
                    info.child_count = 2;
                }
            }
            *out = info;
            Ok(())
        })
    }
}

/// Copies an angle's child (left=0, right=1) into an independent handle.
/// # Safety
/// Angle is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_angle_child(
    angle: *const QdkGraphAngle,
    index: usize,
    out: *mut *mut QdkGraphAngle,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live handle and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let angle = required(angle, "angle")?.inner.access()?;
            let child = match (&*angle, index) {
                (Angle::Neg { operand } | Angle::Binary { left: operand, .. }, 0) => operand,
                (Angle::Binary { right, .. }, 1) => right,
                _ => return Err(Error::new(QDK_WRONG_VARIANT, "angle has no such child")),
            };
            *out = Box::into_raw(Box::new(QdkGraphAngle {
                inner: Handle::new((**child).clone()),
            }));
            Ok(())
        })
    }
}

/// Copies one referenced parameter name in native first-seen order.
/// # Safety
/// Angle is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_angle_parameter(
    angle: *const QdkGraphAngle,
    index: usize,
    out: *mut QdkBytes,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live handle and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let angle = required(angle, "angle")?.inner.access()?;
            let parameters = angle.parameters();
            let name = parameters
                .get(index)
                .ok_or_else(|| Error::invalid("parameter index is out of range"))?;
            *out = name.as_bytes().to_vec().into();
            Ok(())
        })
    }
}

/// Releases an independent graph angle and clears its slot.
/// # Safety
/// Slot contains NULL or a live exclusively owned angle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_angle_free(
    value: *mut *mut QdkGraphAngle,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns the slot and live allocation.
    unsafe { boundary(error, None, || release(value, |value| &value.inner)) }
}
