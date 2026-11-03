// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    ptr,
    rc::{Rc, Weak},
};

use qsc_circuit_store::{PackedOperation, ParamValue};

use crate::{
    QDK_ANGLE_EXPRESSION, QDK_ANGLE_LITERAL, QDK_ANGLE_PARAMETER, QDK_OPERATION_BARRIER,
    QDK_OPERATION_CONTROL_FLOW, QDK_OPERATION_CUSTOM, QDK_OPERATION_DELAY, QDK_OPERATION_RESET,
    QDK_OPERATION_STANDARD, QDK_OPERATION_UNITARY, QDK_WRONG_OWNER, QDK_WRONG_VARIANT,
    QdkAngleExpression, QdkBytes, QdkCircuit, QdkComplex, QdkDiagnostic, QdkParameter, QdkStatus,
    boundary::{Error, boundary, copy_out, output, required, slot},
    handle::{Handle, Lifetime},
    operation::{gate_tag, time_tag},
};

/// An independent instruction copy. It can outlive its source circuit.
pub struct QdkInstruction {
    pub(crate) owner: Weak<Lifetime>,
    pub(crate) operation: PackedOperation,
    qubits: Vec<u32>,
    clbits: Vec<u32>,
    params: Vec<ParamValue>,
}

pub(crate) fn check_owner(record: &QdkInstruction, owner: &QdkCircuit) -> Result<(), Error> {
    if record
        .owner
        .upgrade()
        .is_none_or(|record_owner| !Rc::ptr_eq(&record_owner, &owner.inner.lifetime))
    {
        Err(Error::new(
            QDK_WRONG_OWNER,
            "instruction belongs to another circuit",
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn copy_instruction(
    owner: &QdkCircuit,
    store: &qsc_circuit_store::CircuitStore,
    instruction: &qsc_circuit_store::Instruction,
) -> Result<Box<QdkInstruction>, Error> {
    let qubits = store
        .qubits_of(instruction)
        .ok_or_else(|| Error::invalid("instruction has invalid qubit operands"))?
        .iter()
        .map(|q| q.0)
        .collect();
    let clbits = store
        .clbits_of(instruction)
        .ok_or_else(|| Error::invalid("instruction has invalid classical operands"))?
        .iter()
        .map(|c| c.0)
        .collect();
    Ok(Box::new(QdkInstruction {
        owner: Rc::downgrade(&owner.inner.lifetime),
        operation: instruction.operation.clone(),
        qubits,
        clbits,
        params: instruction.params.as_deref().unwrap_or(&[]).to_vec(),
    }))
}

#[derive(Default)]
#[repr(C)]
pub struct QdkCircuitInfo {
    pub qubits: u32,
    pub clbits: u32,
    pub instructions: usize,
    pub parameters: usize,
    pub blocks: usize,
    pub quantum_registers: usize,
    pub classical_registers: usize,
    pub global_phase: f64,
}

#[derive(Default)]
#[repr(C)]
pub struct QdkInstructionInfo {
    pub kind: u32,
    /// Zero for a non-standard operation.
    pub gate: u32,
    pub estimator_id: u32,
    pub qubit_count: usize,
    pub clbit_count: usize,
    pub param_count: usize,
    pub block_count: usize,
    pub matrix_len: usize,
    /// Duration and unit apply only to delay operations.
    pub duration: f64,
    pub time_unit: u32,
}

/// A copied angle value. IDs are local to the original circuit, not global
/// identities; use `qdk_instruction_parameter` or
/// `qdk_instruction_angle_expression` to obtain a checked live handle.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct QdkStoredAngle {
    pub kind: u32,
    pub literal: f64,
    pub id: u32,
}

/// Copies circuit dimensions and counts.
/// # Safety
/// The circuit is live; writable outputs are disjoint from it and each other.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_info(
    circuit: *const QdkCircuit,
    out: *mut QdkCircuitInfo,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live handle and disjoint output slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let value = required(circuit, "circuit")?.inner.access()?;
            let store = value.store();
            *out = QdkCircuitInfo {
                qubits: store.qubit_count(),
                clbits: store.clbit_count(),
                instructions: store.len(),
                parameters: store.parameters().len(),
                blocks: store.block_count(),
                quantum_registers: value.quantum_registers().len(),
                classical_registers: value.classical_registers().len(),
                global_phase: store.global_phase(),
            };
            Ok(())
        })
    }
}

/// Reserves native instruction storage without changing circuit contents.
/// # Safety
/// The circuit is live and exclusively accessible; error output is disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_reserve(
    circuit: *mut QdkCircuit,
    additional: usize,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live exclusively accessible handle.
    unsafe {
        boundary(error, None, || {
            let mut value = required(circuit, "circuit")?.inner.access()?;
            let total = value
                .store()
                .len()
                .checked_add(additional)
                .ok_or_else(|| Error::invalid("instruction capacity overflows"))?;
            if total > (isize::MAX as usize) / std::mem::size_of::<qsc_circuit_store::Instruction>()
            {
                return Err(Error::invalid(
                    "instruction capacity exceeds the addressable range",
                ));
            }
            value.store_mut().reserve(additional);
            Ok(())
        })
    }
}

/// Sets numeric global phase using the native normalization rules. Non-finite
/// phase values remain representable.
/// # Safety
/// The circuit is live and exclusively accessible; error output is disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_set_global_phase(
    circuit: *mut QdkCircuit,
    phase: f64,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live exclusively accessible handle.
    unsafe {
        boundary(error, None, || {
            required(circuit, "circuit")?
                .inner
                .access()?
                .store_mut()
                .set_global_phase(phase);
            Ok(())
        })
    }
}

/// Copies a top-level instruction, including owned operation payloads.
/// # Safety
/// The circuit is live; output slots are writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_instruction(
    circuit: *const QdkCircuit,
    index: usize,
    out: *mut *mut QdkInstruction,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live handle and disjoint output slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let owner = required(circuit, "circuit")?;
            let value = owner.inner.access()?;
            let store = value.store();
            let instruction = store
                .instruction(index)
                .ok_or_else(|| Error::invalid(format!("instruction {index} does not exist")))?;
            *out = Box::into_raw(copy_instruction(owner, store, instruction)?);
            Ok(())
        })
    }
}

/// Copies instruction metadata. Variant-specific payload access is separate.
/// # Safety
/// Instruction is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_instruction_info(
    instruction: *const QdkInstruction,
    out: *mut QdkInstructionInfo,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live record and disjoint output slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let value = required(instruction, "instruction")?;
            let kind = match &value.operation {
                PackedOperation::Standard(gate) => {
                    out.gate = gate_tag(*gate);
                    QDK_OPERATION_STANDARD
                }
                PackedOperation::Reset => QDK_OPERATION_RESET,
                PackedOperation::Barrier { .. } => QDK_OPERATION_BARRIER,
                PackedOperation::Delay { duration, unit } => {
                    out.duration = *duration;
                    out.time_unit = time_tag(*unit);
                    QDK_OPERATION_DELAY
                }
                PackedOperation::Custom(_) => QDK_OPERATION_CUSTOM,
                PackedOperation::Unitary(unitary) => {
                    out.matrix_len = unitary.matrix.len();
                    QDK_OPERATION_UNITARY
                }
                PackedOperation::ControlFlow(control) => {
                    out.block_count = control.blocks().len();
                    QDK_OPERATION_CONTROL_FLOW
                }
            };
            out.kind = kind;
            out.estimator_id = value.operation.instruction_id().into();
            out.qubit_count = value.qubits.len();
            out.clbit_count = value.clbits.len();
            out.param_count = value.params.len();
            Ok(())
        })
    }
}

/// Copies the native operation name as owned UTF-8 bytes.
/// # Safety
/// Instruction is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_instruction_name(
    instruction: *const QdkInstruction,
    out: *mut QdkBytes,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides valid disjoint inputs and outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            *out = required(instruction, "instruction")?
                .operation
                .name()
                .as_bytes()
                .to_vec()
                .into();
            Ok(())
        })
    }
}

/// Copies ordered qubit operands. Insufficient capacity reports needed length
/// without partial writes. NULL with zero capacity is a size query.
/// # Safety
/// Instruction is live; outputs cover their declared capacity and are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_instruction_qubits(
    instruction: *const QdkInstruction,
    data: *mut u32,
    capacity: usize,
    needed: *mut usize,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides valid disjoint buffers and output slots.
    unsafe {
        boundary(error, None, || {
            *output(needed)? = 0;
            copy_out(
                &required(instruction, "instruction")?.qubits,
                data,
                capacity,
                needed,
            )
        })
    }
}

/// Copies ordered classical operands under the same rules as the qubit query.
/// # Safety
/// Instruction is live; outputs cover their declared capacity and are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_instruction_clbits(
    instruction: *const QdkInstruction,
    data: *mut u32,
    capacity: usize,
    needed: *mut usize,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides valid disjoint buffers and output slots.
    unsafe {
        boundary(error, None, || {
            *output(needed)? = 0;
            copy_out(
                &required(instruction, "instruction")?.clbits,
                data,
                capacity,
                needed,
            )
        })
    }
}

/// Copies typed stored angles; no live parameter handle escapes this record.
/// # Safety
/// Instruction is live; outputs cover their declared capacity and are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_instruction_angles(
    instruction: *const QdkInstruction,
    data: *mut QdkStoredAngle,
    capacity: usize,
    needed: *mut usize,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides valid disjoint buffers and output slots.
    unsafe {
        boundary(error, None, || {
            *output(needed)? = 0;
            let values: Vec<_> = required(instruction, "instruction")?
                .params
                .iter()
                .map(|value| match value {
                    ParamValue::Literal(literal) => QdkStoredAngle {
                        kind: QDK_ANGLE_LITERAL,
                        literal: *literal,
                        id: 0,
                    },
                    ParamValue::Parameter(id) => QdkStoredAngle {
                        kind: QDK_ANGLE_PARAMETER,
                        literal: 0.0,
                        id: id.0,
                    },
                    ParamValue::Expr(id) => QdkStoredAngle {
                        kind: QDK_ANGLE_EXPRESSION,
                        literal: 0.0,
                        id: id.0,
                    },
                })
                .collect();
            copy_out(&values, data, capacity, needed)
        })
    }
}

/// Copies a unitary's row-major complex matrix; other variants are refused.
/// # Safety
/// Instruction is live; outputs cover their declared capacity and are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_instruction_matrix(
    instruction: *const QdkInstruction,
    data: *mut QdkComplex,
    capacity: usize,
    needed: *mut usize,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides valid disjoint buffers and output slots.
    unsafe {
        boundary(error, None, || {
            *output(needed)? = 0;
            let PackedOperation::Unitary(unitary) =
                &required(instruction, "instruction")?.operation
            else {
                return Err(Error::new(
                    QDK_WRONG_VARIANT,
                    "instruction is not a unitary matrix",
                ));
            };
            let values: Vec<_> = unitary
                .matrix
                .iter()
                .map(|(real, imag)| QdkComplex {
                    real: *real,
                    imag: *imag,
                })
                .collect();
            copy_out(&values, data, capacity, needed)
        })
    }
}

/// Recovers a live parameter handle from a copied record, checking the original
/// circuit identity rather than interpreting an ID in a different circuit.
/// # Safety
/// Instruction/circuit are live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_instruction_parameter(
    instruction: *const QdkInstruction,
    circuit: *mut QdkCircuit,
    index: usize,
    out: *mut *mut QdkParameter,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides live handles and disjoint output slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let record = required(instruction, "instruction")?;
            let owner = required(circuit, "circuit")?;
            let _value = owner.inner.access()?;
            check_owner(record, owner)?;
            let param = record
                .params
                .get(index)
                .ok_or_else(|| Error::invalid("angle index does not exist"))?;
            let ParamValue::Parameter(id) = param else {
                return Err(Error::new(
                    QDK_WRONG_VARIANT,
                    "angle is not a parameter reference",
                ));
            };
            *out = crate::handle::publish(
                Box::new(QdkParameter {
                    circuit,
                    inner: Handle::child(*id, &owner.inner.lifetime, None)?,
                }),
                |value| &value.inner,
            );
            Ok(())
        })
    }
}

/// Recovers a stored arithmetic expression at an angle index in a copied record.
/// The record must belong to this live circuit, including after import or clone;
/// a record from the source of a clone is not a record from the clone itself.
/// Non-expression angles return `QDK_WRONG_VARIANT`. Release the dependent
/// handle before its circuit, or let its callback scope dispose it.
/// # Safety
/// Instruction/circuit are live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_instruction_angle_expression(
    instruction: *const QdkInstruction,
    circuit: *mut QdkCircuit,
    index: usize,
    out: *mut *mut QdkAngleExpression,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides live handles and disjoint output slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let record = required(instruction, "instruction")?;
            let owner = required(circuit, "circuit")?;
            let _value = owner.inner.access()?;
            check_owner(record, owner)?;
            let param = record
                .params
                .get(index)
                .ok_or_else(|| Error::invalid("angle index does not exist"))?;
            let ParamValue::Expr(id) = param else {
                return Err(Error::new(
                    QDK_WRONG_VARIANT,
                    "angle is not an arithmetic expression",
                ));
            };
            *out = crate::handle::publish(
                Box::new(QdkAngleExpression {
                    circuit,
                    inner: Handle::child(*id, &owner.inner.lifetime, None)?,
                }),
                |value| &value.inner,
            );
            Ok(())
        })
    }
}

/// Releases an independent instruction record and clears its slot.
/// # Safety
/// Slot contains NULL or a live owning record and is exclusively accessible.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_instruction_free(
    instruction: *mut *mut QdkInstruction,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns this slot and any record it contains.
    unsafe {
        boundary(error, None, || {
            if instruction.is_null() {
                return Ok(());
            }
            let slot = slot(instruction, "instruction slot")?;
            if !slot.is_null() {
                let owned = *slot;
                *slot = ptr::null_mut();
                drop(Box::from_raw(owned));
            }
            Ok(())
        })
    }
}
