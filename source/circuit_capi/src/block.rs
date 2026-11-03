// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_circuit_store::{BlockId, ControlFlow, LoopIndex, PackedOperation};

use crate::{
    QDK_BODY_BOX, QDK_BODY_IF, QDK_BODY_MOMENT, QDK_BODY_SWITCH, QDK_BODY_WHILE,
    QDK_LOOP_PARAMETER, QDK_LOOP_RUNTIME, QDK_WRONG_VARIANT, QdkBuilder, QdkBytes, QdkCircuit,
    QdkDiagnostic, QdkExpression, QdkInstruction, QdkLiteral, QdkStatus,
    boundary::{Error, boundary, copy_out, output, release, required},
    builder::Builder,
    expression::expression_type,
    handle::Handle,
    inspection::{check_owner, copy_instruction},
};

pub const QDK_BODY_FOR: u32 = 6;
pub const QDK_BODY_BREAK: u32 = 7;
pub const QDK_BODY_CONTINUE: u32 = 8;

pub struct QdkBlock {
    circuit: *const QdkCircuit,
    inner: Handle<BlockId>,
}

#[derive(Default)]
#[repr(C)]
pub struct QdkControlInfo {
    pub kind: u32,
    pub loop_index_kind: u32,
    pub loop_index_id: u32,
    pub value_count: usize,
    pub case_count: usize,
    pub has_default: u8,
    pub has_label: u8,
    pub has_duration: u8,
    pub duration: f64,
}

fn control(instruction: &QdkInstruction) -> Result<&ControlFlow, Error> {
    match &instruction.operation {
        PackedOperation::ControlFlow(control) => Ok(control),
        _ => Err(Error::new(
            QDK_WRONG_VARIANT,
            "instruction is not control flow",
        )),
    }
}

/// Copies control-flow metadata without borrowing the native program.
/// # Safety
/// Instruction is live; outputs are writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_instruction_control(
    instruction: *const QdkInstruction,
    out: *mut QdkControlInfo,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live copied instruction and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            out.kind = match control(required(instruction, "instruction")?)? {
                ControlFlow::IfElse(_) => QDK_BODY_IF,
                ControlFlow::While(_) => QDK_BODY_WHILE,
                ControlFlow::For(loop_) => {
                    if let Some(index) = loop_.index {
                        let (kind, id) = match index {
                            LoopIndex::Parameter(id) => (QDK_LOOP_PARAMETER, id.0),
                            LoopIndex::Var(id) => (QDK_LOOP_RUNTIME, id.0),
                        };
                        out.loop_index_kind = kind;
                        out.loop_index_id = id;
                    }
                    out.value_count = loop_.values.len();
                    QDK_BODY_FOR
                }
                ControlFlow::Switch(switch) => {
                    out.case_count = switch.cases.len();
                    out.has_default = u8::from(switch.default.is_some());
                    QDK_BODY_SWITCH
                }
                ControlFlow::Box(boxed) => {
                    out.has_label = u8::from(boxed.label.is_some());
                    out.has_duration = u8::from(boxed.duration.is_some());
                    out.duration = boxed.duration.unwrap_or(0.0);
                    QDK_BODY_BOX
                }
                ControlFlow::Moment(_) => QDK_BODY_MOMENT,
                ControlFlow::Break => QDK_BODY_BREAK,
                ControlFlow::Continue => QDK_BODY_CONTINUE,
            };
            Ok(())
        })
    }
}

/// Copies for-loop values or the values of one switch case. For-loop queries
/// require `case_index` zero; default switch bodies have no case-value list.
/// # Safety
/// Instruction is live; output buffers cover capacity and are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_instruction_values(
    instruction: *const QdkInstruction,
    case_index: usize,
    data: *mut QdkLiteral,
    capacity: usize,
    needed: *mut usize,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides valid disjoint buffers and output slots.
    unsafe {
        boundary(error, None, || {
            *output(needed)? = 0;
            let values = match control(required(instruction, "instruction")?)? {
                ControlFlow::For(loop_) if case_index == 0 => &loop_.values,
                ControlFlow::Switch(switch) => {
                    &switch
                        .cases
                        .get(case_index)
                        .ok_or_else(|| Error::invalid("case index does not exist"))?
                        .0
                }
                _ => {
                    return Err(Error::new(
                        QDK_WRONG_VARIANT,
                        "instruction has no value list at this index",
                    ));
                }
            };
            let values: Vec<_> = values.iter().copied().map(QdkLiteral::from).collect();
            copy_out(&values, data, capacity, needed)
        })
    }
}

/// Copies a box label. No label yields empty bytes; `has_label` in control
/// metadata distinguishes it from an explicitly empty label.
/// # Safety
/// Instruction is live; output slots are writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_instruction_label(
    instruction: *const QdkInstruction,
    out: *mut QdkBytes,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live record and disjoint output slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let ControlFlow::Box(boxed) = control(required(instruction, "instruction")?)? else {
                return Err(Error::new(QDK_WRONG_VARIANT, "instruction is not a box"));
            };
            *out = boxed
                .label
                .as_deref()
                .unwrap_or("")
                .as_bytes()
                .to_vec()
                .into();
            Ok(())
        })
    }
}

/// Obtains a registered block reference from an instruction's ordered branches.
/// The record must come from the supplied live circuit.
/// # Safety
/// Record and circuit are live; output slots are writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_instruction_block(
    instruction: *const QdkInstruction,
    circuit: *mut QdkCircuit,
    index: usize,
    out: *mut *mut QdkBlock,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides live disjoint handles and output slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let record = required(instruction, "instruction")?;
            let owner = required(circuit, "circuit")?;
            let _value = owner.inner.access()?;
            check_owner(record, owner)?;
            let id = *control(record)?
                .blocks()
                .get(index)
                .ok_or_else(|| Error::invalid("block index does not exist"))?;
            *out = crate::handle::publish(
                Box::new(QdkBlock {
                    circuit,
                    inner: Handle::child(id, &owner.inner.lifetime, None)?,
                }),
                |value| &value.inner,
            );
            Ok(())
        })
    }
}

/// Obtains a read-only block reference from a body or case builder.
/// # Safety
/// Builder is live; output slots are writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_builder_block(
    builder: *const QdkBuilder,
    out: *mut *mut QdkBlock,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: the live builder registration retains its root circuit.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let builder = required(builder, "builder")?;
            let owner = required(builder.circuit, "circuit")?;
            let _value = owner.inner.access()?;
            let native = builder.inner.access()?;
            let id = match &*native {
                Builder::Body(body) => body.block(),
                Builder::Case { body, .. } => body.block(),
                Builder::Switch(_) => {
                    return Err(Error::new(
                        QDK_WRONG_VARIANT,
                        "a switch has separate case blocks",
                    ));
                }
            };
            *out = crate::handle::publish(
                Box::new(QdkBlock {
                    circuit: builder.circuit,
                    inner: Handle::child(id, &owner.inner.lifetime, None)?,
                }),
                |value| &value.inner,
            );
            Ok(())
        })
    }
}

/// Copies a condition or switch selector as a registered expression handle.
/// # Safety
/// Record/circuit are live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_instruction_condition(
    instruction: *const QdkInstruction,
    circuit: *mut QdkCircuit,
    out: *mut *mut QdkExpression,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides live disjoint handles and output slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let record = required(instruction, "instruction")?;
            let owner = required(circuit, "circuit")?;
            let value = owner.inner.access()?;
            check_owner(record, owner)?;
            let id = match control(record)? {
                ControlFlow::IfElse(branch) => branch.condition,
                ControlFlow::While(loop_) => loop_.condition,
                ControlFlow::Switch(switch) => switch.selector,
                _ => {
                    return Err(Error::new(
                        QDK_WRONG_VARIANT,
                        "instruction has no condition or selector",
                    ));
                }
            };
            let ty = expression_type(value.store(), id)?;
            *out = crate::handle::publish(
                Box::new(QdkExpression {
                    circuit,
                    inner: Handle::child((id, ty), &owner.inner.lifetime, None)?,
                }),
                |value| &value.inner,
            );
            Ok(())
        })
    }
}

/// Returns the number of instructions in a registered block.
/// # Safety
/// Block is live; output slots are writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_block_len(
    block: *const QdkBlock,
    out: *mut usize,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: block registration keeps its root circuit live.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let block = required(block, "block")?;
            let value = required(block.circuit, "circuit")?.inner.access()?;
            let id = *block.inner.access()?;
            *out = value
                .store()
                .block(id)
                .ok_or_else(|| Error::invalid("block does not exist"))?
                .instructions
                .len();
            Ok(())
        })
    }
}

/// Copies one instruction from a registered block.
/// # Safety
/// Block is live; output slots are writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_block_instruction(
    block: *const QdkBlock,
    index: usize,
    out: *mut *mut QdkInstruction,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: block registration keeps its root circuit live.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let block = required(block, "block")?;
            let owner = required(block.circuit, "circuit")?;
            let value = owner.inner.access()?;
            let id = *block.inner.access()?;
            let native = value
                .store()
                .block(id)
                .ok_or_else(|| Error::invalid("block does not exist"))?;
            let instruction = native
                .instructions
                .get(index)
                .ok_or_else(|| Error::invalid("block instruction does not exist"))?;
            *out = Box::into_raw(copy_instruction(owner, value.store(), instruction)?);
            Ok(())
        })
    }
}

/// Releases a block reference, including during poisoned-owner teardown.
/// # Safety
/// Slot contains NULL or a live owning block reference.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_block_free(
    block: *mut *mut QdkBlock,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns this slot and its live registered handle.
    unsafe { boundary(error, None, || release(block, |value| &value.inner)) }
}
