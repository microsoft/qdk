// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::ptr;

use qsc_circuit_store::ParameterId;

use crate::{
    QdkCircuit, QdkDiagnostic, QdkStatus,
    boundary::{boundary, input_text, output, required, slot},
    handle::Handle,
};

/// A parameter reference registered with its owning circuit.
pub struct QdkParameter {
    pub(crate) circuit: *const QdkCircuit,
    pub(crate) inner: Handle<ParameterId>,
}

/// Declares a parameter and returns a dependent handle. Release it before
/// releasing the circuit. The name is length-delimited UTF-8.
///
/// # Safety
/// The circuit and input bytes are live; output slots are writable and disjoint.
/// Objects cannot be accessed concurrently.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_parameter(
    circuit: *mut QdkCircuit,
    name: *const u8,
    name_len: usize,
    out: *mut *mut QdkParameter,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides live input allocations and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let owner = required(circuit, "circuit")?;
            let name = input_text(name, name_len)?;
            let mut value = owner.inner.access()?;
            // Register before mutation; any later unwinding drops this lease.
            let parameter = QdkParameter {
                circuit,
                inner: Handle::child(ParameterId(0), &owner.inner.lifetime, None)?,
            };
            *parameter.inner.access()? = value.store_mut().declare_parameter(name);
            *out = crate::handle::publish(Box::new(parameter), |value| &value.inner);
            Ok(())
        })
    }
}

/// Returns the owner-local parameter ID. The number is not a cross-circuit
/// identity and using it later requires a live matching circuit.
///
/// # Safety
/// The parameter is live and outputs are writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_parameter_id(
    parameter: *const QdkParameter,
    out: *mut u32,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: the live dependent retains a registration on its circuit.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let parameter = required(parameter, "parameter")?;
            let _owner = required(parameter.circuit, "circuit")?.inner.access()?;
            *out = parameter.inner.access()?.0;
            Ok(())
        })
    }
}

/// Releases a dependent parameter and clears its slot. NULL is a no-op.
/// Release is permitted even if its circuit is poisoned.
///
/// # Safety
/// The slot contains NULL or a live owning parameter handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_parameter_free(
    parameter: *mut *mut QdkParameter,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns the slot and the dependent handle it contains.
    unsafe {
        boundary(error, None, || {
            if parameter.is_null() {
                return Ok(());
            }
            let slot = slot(parameter, "parameter slot")?;
            if slot.is_null() {
                return Ok(());
            }
            required(*slot, "parameter")?.inner.require_releasable()?;
            let owned = *slot;
            *slot = ptr::null_mut();
            drop(Box::from_raw(owned));
            Ok(())
        })
    }
}
