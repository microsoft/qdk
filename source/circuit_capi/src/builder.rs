// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{ptr, rc::Rc};

use qdk_circuit_api::{BodyBuilder, BodyKind, Circuit, SwitchBuilder, SwitchCaseBuilder};
use qsc_circuit_store::{ControlFlow, LoopIndex, ParameterId, VarId, VarType};

use crate::{
    QDK_INVALID_STATE, QDK_WRONG_OWNER, QDK_WRONG_VARIANT, QdkCircuit, QdkDiagnostic,
    QdkExpression, QdkLiteral, QdkOperation, QdkParameter, QdkStatus, QdkVariable,
    boundary::{Error, boundary, input_slice, input_text, output, release, required},
    expression::expression,
    handle::{Handle, Lifetime},
};

pub const QDK_BODY_IF: u32 = 1;
pub const QDK_BODY_WHILE: u32 = 2;
pub const QDK_BODY_MOMENT: u32 = 3;
pub const QDK_BODY_BOX: u32 = 4;
pub const QDK_BODY_SWITCH: u32 = 5;
pub const QDK_LOOP_PARAMETER: u32 = 1;
pub const QDK_LOOP_RUNTIME: u32 = 2;
pub const QDK_JUMP_BREAK: u32 = 1;
pub const QDK_JUMP_CONTINUE: u32 = 2;

pub(crate) enum Builder {
    Body(BodyBuilder),
    Switch(SwitchBuilder),
    Case {
        body: SwitchCaseBuilder,
        switch: *const QdkBuilder,
    },
}

impl Builder {
    fn current(&self, circuit: &Circuit) -> bool {
        match self {
            Self::Body(body) => body.is_current(circuit),
            Self::Switch(body) => body.is_current(circuit),
            Self::Case { body, .. } => body.is_current(circuit),
        }
    }

    fn complete(&self) -> bool {
        match self {
            Self::Body(body) => body.is_complete(),
            Self::Switch(body) => body.is_complete(),
            Self::Case { body, .. } => body.is_complete(),
        }
    }
}

/// A checked construction handle. Finish/cancel before release; closed children
/// may remain readable but still prevent releasing their parents.
pub struct QdkBuilder {
    pub(crate) circuit: *const QdkCircuit,
    parent: *const QdkBuilder,
    pub(crate) inner: Handle<Builder>,
}

#[derive(Default)]
#[repr(C)]
pub struct QdkBodyOptions {
    pub kind: u32,
    pub condition: *const QdkExpression,
    /// NULL means no label; a non-NULL empty input means an empty label.
    pub label: *const u8,
    pub label_len: usize,
}

#[repr(C)]
pub struct QdkForOptions {
    pub values: *const QdkLiteral,
    pub value_count: usize,
    /// NULL selects a generated unused name.
    pub name: *const u8,
    pub name_len: usize,
    pub index_kind: u32,
}

/// Exactly one owning index handle is populated, according to kind. It is
/// independent of the builder but still registered with the circuit.
#[derive(Default)]
#[repr(C)]
pub struct QdkLoopIndex {
    pub kind: u32,
    pub parameter: *mut QdkParameter,
    pub variable: *mut QdkVariable,
}

unsafe fn parent_lifetime(
    circuit: *const QdkCircuit,
    value: &Circuit,
    parent: *const QdkBuilder,
) -> Result<Option<Rc<Lifetime>>, Error> {
    if parent.is_null() {
        value
            .require_closed("begin a root body in")
            .map_err(Error::from)?;
        return Ok(None);
    }
    // SAFETY: parent must be a live handle; its registration retains the root.
    let parent = unsafe { required(parent, "parent builder")? };
    if parent.circuit != circuit {
        return Err(Error::new(
            QDK_WRONG_OWNER,
            "parent belongs to another circuit",
        ));
    }
    if !parent.inner.access()?.current(value) {
        return Err(Error::new(
            QDK_INVALID_STATE,
            "parent is not the current open construction scope",
        ));
    }
    Ok(Some(Rc::clone(&parent.inner.lifetime)))
}

fn owning_builder(
    circuit: *const QdkCircuit,
    parent: *const QdkBuilder,
    native: Builder,
    root: &Rc<Lifetime>,
    parent_lifetime: Option<&Rc<Lifetime>>,
) -> Result<Box<QdkBuilder>, Error> {
    let result = Box::new(QdkBuilder {
        circuit,
        parent,
        inner: Handle::child(native, root, parent_lifetime)?,
    });
    result.inner.lifetime.unfinished.set(true);
    Ok(result)
}

/// Begins if/while/moment/box/switch construction. A nested body must name its
/// currently open parent; NULL parent begins a root scope. Appends use this
/// scope until it finishes or cancels.
/// # Safety
/// Circuit, optional parent/condition and input bytes are live; outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_begin(
    circuit: *mut QdkCircuit,
    parent: *const QdkBuilder,
    options: *const QdkBodyOptions,
    out: *mut *mut QdkBuilder,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides live disjoint handles, options and output slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let owner = required(circuit, "circuit")?;
            let mut value = owner.inner.access()?;
            let lifetime = parent_lifetime(circuit, &value, parent)?;
            let options = required(options, "body options")?;
            let native = match options.kind {
                QDK_BODY_IF | QDK_BODY_WHILE => {
                    let (condition, ty) = expression(options.condition, circuit)?;
                    if ty != VarType::Bool {
                        return Err(Error::invalid(
                            "a conditional requires a boolean expression",
                        ));
                    }
                    let kind = if options.kind == QDK_BODY_IF {
                        BodyKind::IfThen { condition }
                    } else {
                        BodyKind::While { condition }
                    };
                    Builder::Body(value.body(kind))
                }
                QDK_BODY_MOMENT => Builder::Body(value.body(BodyKind::Moment)),
                QDK_BODY_BOX => {
                    let label = if options.label.is_null() && options.label_len == 0 {
                        None
                    } else {
                        Some(input_text(options.label, options.label_len)?.into())
                    };
                    Builder::Body(value.body(BodyKind::Box { label }))
                }
                QDK_BODY_SWITCH => {
                    let (selector, _) = expression(options.condition, circuit)?;
                    Builder::Switch(value.switch(selector))
                }
                tag => return Err(Error::invalid(format!("unknown body kind {tag}"))),
            };
            let result = owning_builder(
                circuit,
                parent,
                native,
                &owner.inner.lifetime,
                lifetime.as_ref(),
            )?;
            match &mut *result.inner.access()? {
                Builder::Body(body) => body.enter(&mut value),
                Builder::Switch(body) => body.enter(&mut value),
                Builder::Case { .. } => unreachable!("case is constructed separately"),
            }
            .map_err(Error::from)?;
            *out = crate::handle::publish(result, |value| &value.inner);
            Ok(())
        })
    }
}

/// Opens the alternative branch of a successfully completed if body.
/// # Safety
/// The conditional and its parent circuit are live; outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_builder_otherwise(
    conditional: *const QdkBuilder,
    out: *mut *mut QdkBuilder,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live registered builder and disjoint output slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let conditional = required(conditional, "conditional")?;
            let owner = required(conditional.circuit, "circuit")?;
            let mut value = owner.inner.access()?;
            let lifetime = parent_lifetime(conditional.circuit, &value, conditional.parent)?;
            let old = conditional.inner.access()?;
            let Builder::Body(old) = &*old else {
                return Err(Error::new(
                    QDK_WRONG_VARIANT,
                    "builder is not a conditional",
                ));
            };
            let body = old.otherwise(&mut value).map_err(Error::from)?;
            let result = owning_builder(
                conditional.circuit,
                conditional.parent,
                Builder::Body(body),
                &owner.inner.lifetime,
                lifetime.as_ref(),
            )?;
            let mut native = result.inner.access()?;
            let Builder::Body(body) = &mut *native else {
                unreachable!("constructed a body");
            };
            body.enter(&mut value).map_err(Error::from)?;
            drop(native);
            *out = crate::handle::publish(result, |value| &value.inner);
            Ok(())
        })
    }
}

/// Opens a case of the current switch. `default_case` is 0 for a nonempty value
/// list or 1 for the default (which must have no values).
/// # Safety
/// Switch and input values are live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_switch_case(
    switch: *const QdkBuilder,
    values: *const QdkLiteral,
    count: usize,
    default_case: u8,
    out: *mut *mut QdkBuilder,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live switch, declared input extent and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let parent = required(switch, "switch")?;
            let owner = required(parent.circuit, "circuit")?;
            let mut value = owner.inner.access()?;
            let parent_value = parent.inner.access()?;
            let Builder::Switch(native) = &*parent_value else {
                return Err(Error::new(QDK_WRONG_VARIANT, "builder is not a switch"));
            };
            let matched = match default_case {
                0 => Some(
                    input_slice(values, count)?
                        .iter()
                        .map(|value| value.decode())
                        .collect::<Result<_, _>>()?,
                ),
                1 if count == 0 => None,
                _ => return Err(Error::invalid("invalid default-case flag or values")),
            };
            let body = native.case(&mut value, matched).map_err(Error::from)?;
            let result = owning_builder(
                parent.circuit,
                switch,
                Builder::Case { body, switch },
                &owner.inner.lifetime,
                Some(&parent.inner.lifetime),
            )?;
            let mut native = result.inner.access()?;
            let Builder::Case { body, .. } = &mut *native else {
                unreachable!("constructed a case");
            };
            body.enter(&mut value).map_err(Error::from)?;
            drop(native);
            *out = crate::handle::publish(result, |value| &value.inner);
            Ok(())
        })
    }
}

/// Begins a compile-time-parameter or runtime-variable for loop. The index
/// handle is returned separately and must also be released by the caller.
/// # Safety
/// Circuit, parent and options/values are live; output slots are writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_for_begin(
    circuit: *mut QdkCircuit,
    parent: *const QdkBuilder,
    options: *const QdkForOptions,
    out: *mut *mut QdkBuilder,
    index: *mut QdkLoopIndex,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides disjoint live handles, descriptors, buffers and output slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let index = output(index)?;
            let owner = required(circuit, "circuit")?;
            let mut value = owner.inner.access()?;
            let lifetime = parent_lifetime(circuit, &value, parent)?;
            let options = required(options, "for options")?;
            if !matches!(options.index_kind, QDK_LOOP_PARAMETER | QDK_LOOP_RUNTIME) {
                return Err(Error::invalid("unknown loop index kind"));
            }
            let values = input_slice(options.values, options.value_count)?
                .iter()
                .map(|value| value.decode())
                .collect::<Result<_, _>>()?;
            let name = if options.name.is_null() && options.name_len == 0 {
                value.next_loop_name()
            } else {
                input_text(options.name, options.name_len)?.into()
            };
            let mut parameter = None;
            let mut variable = None;
            let loop_index = if options.index_kind == QDK_LOOP_PARAMETER {
                let handle = Box::new(QdkParameter {
                    circuit,
                    inner: Handle::child(ParameterId(0), &owner.inner.lifetime, None)?,
                });
                let id = value.store_mut().declare_parameter(name);
                *handle.inner.access()? = id;
                parameter = Some(handle);
                LoopIndex::Parameter(id)
            } else {
                let handle = Box::new(QdkVariable {
                    circuit,
                    inner: Handle::child(VarId(0), &owner.inner.lifetime, None)?,
                });
                let id = value.store_mut().declare_var(name, VarType::Int);
                *handle.inner.access()? = id;
                variable = Some(handle);
                LoopIndex::Var(id)
            };
            let body = value.body(BodyKind::For {
                index: loop_index,
                values,
            });
            let result = owning_builder(
                circuit,
                parent,
                Builder::Body(body),
                &owner.inner.lifetime,
                lifetime.as_ref(),
            )?;
            let mut native = result.inner.access()?;
            let Builder::Body(body) = &mut *native else {
                unreachable!("constructed a for body");
            };
            body.enter(&mut value).map_err(Error::from)?;
            drop(native);
            *index = QdkLoopIndex {
                kind: options.index_kind,
                parameter: parameter.map_or(ptr::null_mut(), |value| {
                    crate::handle::publish(value, |value| &value.inner)
                }),
                variable: variable.map_or(ptr::null_mut(), |value| {
                    crate::handle::publish(value, |value| &value.inner)
                }),
            };
            *out = crate::handle::publish(result, |value| &value.inner);
            Ok(())
        })
    }
}

unsafe fn transition(builder: *const QdkBuilder, cancel: bool) -> Result<(), Error> {
    // SAFETY: live registrations keep the root and any enclosing switch allocated.
    unsafe {
        let builder = required(builder, "builder")?;
        let mut value = required(builder.circuit, "circuit")?.inner.access()?;
        let mut native = builder.inner.access()?;
        if !native.current(&value) {
            return Err(Error::new(
                QDK_INVALID_STATE,
                "builder is spent or not the current open scope",
            ));
        }
        let result = match &mut *native {
            Builder::Body(body) => {
                if cancel {
                    body.cancel(&mut value)
                } else {
                    body.finish(&mut value)
                }
            }
            Builder::Switch(body) => {
                if cancel {
                    body.cancel(&mut value)
                } else {
                    body.finish(&mut value)
                }
            }
            Builder::Case { body, switch } => {
                if cancel {
                    body.cancel(&mut value)
                } else {
                    let mut parent = required(*switch, "switch")?.inner.access()?;
                    let Builder::Switch(parent) = &mut *parent else {
                        return Err(Error::new(QDK_WRONG_VARIANT, "case parent is not a switch"));
                    };
                    body.finish(&mut value, parent)
                }
            }
        };
        if native.complete() {
            builder.inner.lifetime.unfinished.set(false);
        }
        result.map_err(Error::from)
    }
}

/// Finishes the current body. Closed child handles may remain allocated.
/// # Safety
/// Builder is live and exclusively accessible; error output is disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_builder_finish(
    builder: *mut QdkBuilder,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live builder and disjoint diagnostic output.
    unsafe { boundary(error, None, || transition(builder, false)) }
}

/// Cancels the current body without publishing its construct.
/// # Safety
/// Builder is live and exclusively accessible; error output is disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_builder_cancel(
    builder: *mut QdkBuilder,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live builder and disjoint diagnostic output.
    unsafe { boundary(error, None, || transition(builder, true)) }
}

/// Appends through an explicitly open builder, refusing spent or non-current
/// handles rather than accidentally editing another construction scope.
/// # Safety
/// Builder, operation and its buffers/handles are live; error output is disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_builder_append(
    builder: *mut QdkBuilder,
    operation: *const QdkOperation,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: registered builder retains its root; operation inputs satisfy append's contract.
    unsafe {
        boundary(error, None, || {
            let builder = required(builder, "builder")?;
            let mut value = required(builder.circuit, "circuit")?.inner.access()?;
            let native = builder.inner.access()?;
            if !native.current(&value) {
                return Err(Error::new(
                    QDK_INVALID_STATE,
                    "builder is spent or not the current open scope",
                ));
            }
            let decoded =
                crate::operation::decode(required(operation, "operation")?, builder.circuit)?;
            value.append(
                decoded.operation,
                &decoded.qubits,
                &decoded.clbits,
                decoded.params,
            )?;
            Ok(())
        })
    }
}

/// Appends break or continue to the current loop context.
/// # Safety
/// Circuit is live and exclusively accessible; error output is disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_jump(
    circuit: *mut QdkCircuit,
    kind: u32,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live circuit and disjoint diagnostic output.
    unsafe {
        boundary(error, None, || {
            let mut value = required(circuit, "circuit")?.inner.access()?;
            let (control, name) = match kind {
                QDK_JUMP_BREAK => (ControlFlow::Break, "break"),
                QDK_JUMP_CONTINUE => (ControlFlow::Continue, "continue"),
                _ => return Err(Error::invalid("unknown loop jump kind")),
            };
            value.append_jump(control, name).map_err(Error::from)
        })
    }
}

/// Releases a completed builder. Unfinished release is refused except for
/// drop-only teardown of a poisoned owner; live children always prevent release.
/// # Safety
/// Slot contains NULL or a live owning builder and is exclusively accessible.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_builder_free(
    builder: *mut *mut QdkBuilder,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns this slot and its live registered handle.
    unsafe { boundary(error, None, || release(builder, |value| &value.inner)) }
}
