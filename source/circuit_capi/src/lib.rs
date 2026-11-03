// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Checked C boundary. Pointer validity, allocation extent and absence of
//! concurrent access remain caller obligations; nulls, sizes and state are checked.

mod angle;
mod block;
mod boundary;
mod builder;
mod expression;
mod graph;
mod handle;
mod inspection;
mod operation;
mod output;
mod parameter;
mod pass;
mod pipeline;
mod register;

pub use angle::*;
pub use block::*;
pub use builder::*;
pub use expression::*;
pub use graph::*;
pub use inspection::*;
pub use operation::*;
pub use output::*;
pub use parameter::*;
pub use pass::*;
pub use pipeline::*;
pub use register::*;

use std::ptr;

use qdk_circuit_api::Circuit;

use boundary::{Error, boundary, output, required};
use handle::Handle;

pub type QdkStatus = u32;
pub const QDK_OK: QdkStatus = 0;
pub const QDK_INVALID_ARGUMENT: QdkStatus = 1;
pub const QDK_IN_USE: QdkStatus = 2;
pub const QDK_UNFINISHED: QdkStatus = 3;
pub const QDK_POISONED: QdkStatus = 4;
pub const QDK_INTERNAL_ERROR: QdkStatus = 5;
pub const QDK_ACTIVE: QdkStatus = 6;
pub const QDK_BORROWED: QdkStatus = 7;
pub const QDK_BUFFER_TOO_SMALL: QdkStatus = 8;
pub const QDK_WRONG_OWNER: QdkStatus = 9;
pub const QDK_WRONG_VARIANT: QdkStatus = 10;
pub const QDK_INVALID_STATE: QdkStatus = 11;
pub const QDK_CALLBACK_ERROR: QdkStatus = 12;
pub const QDK_ALREADY_SET: QdkStatus = 13;

/// Opaque owning circuit. Release children before releasing their circuit.
pub struct QdkCircuit {
    inner: Handle<Circuit>,
}

/// Owned diagnostic, released only with `qdk_diagnostic_free`.
pub struct QdkDiagnostic {
    message: String,
}

/// Owned bytes, not NUL-terminated. Release only with `qdk_bytes_free`.
#[repr(C)]
pub struct QdkBytes {
    pub data: *mut u8,
    pub len: usize,
}

impl Default for QdkBytes {
    fn default() -> Self {
        Self {
            data: ptr::null_mut(),
            len: 0,
        }
    }
}

impl From<Vec<u8>> for QdkBytes {
    fn from(value: Vec<u8>) -> Self {
        if value.is_empty() {
            return Self::default();
        }
        let bytes = value.into_boxed_slice();
        Self {
            len: bytes.len(),
            data: Box::into_raw(bytes).cast(),
        }
    }
}

/// Creates an owning circuit. On failure, `out` is NULL.
///
/// Every `error` output is optional; when non-NULL it receives an owned
/// diagnostic on failure and NULL on success. Release its previous value
/// before reusing the output slot.
///
/// # Safety
/// Non-NULL pointers must address writable, aligned output slots. Slots must
/// not alias one another or any input object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_new(
    qubits: u32,
    clbits: u32,
    out: *mut *mut QdkCircuit,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: the caller supplies valid output slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            *out = Box::into_raw(Box::new(QdkCircuit {
                inner: Handle::new(Circuit::new(qubits, clbits)),
            }));
            Ok(())
        })
    }
}

/// Copies a closed circuit, including registers and symbolic state.
///
/// # Safety
/// `circuit` must be a live library handle. Output slots obey
/// `qdk_circuit_new`'s contract. No concurrent access is allowed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_clone(
    circuit: *const QdkCircuit,
    out: *mut *mut QdkCircuit,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: input and output validity is required by the caller contract.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let source = required(circuit, "circuit")?.inner.access()?;
            let copy = source.try_clone().map_err(Error::from)?;
            *out = Box::into_raw(Box::new(QdkCircuit {
                inner: Handle::new(copy),
            }));
            Ok(())
        })
    }
}

/// Releases an owning circuit and sets its slot to NULL. NULL is a no-op.
/// `InUse`, `Active` or `Borrowed` leaves the owning slot and circuit intact.
/// Poisoned owners remain releasable once all children have been released.
///
/// # Safety
/// A non-NULL slot must contain NULL or a live owning library handle, used
/// exclusively. Previously released pointers must never be passed again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_free(
    circuit: *mut *mut QdkCircuit,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: the caller owns this slot and any non-NULL handle it contains.
    unsafe {
        boundary(error, None, || {
            if circuit.is_null() {
                return Ok(());
            }
            let slot = boundary::slot(circuit, "circuit slot")?;
            if slot.is_null() {
                return Ok(());
            }
            required(*slot, "circuit")?.inner.require_releasable()?;
            let owned = *slot;
            *slot = ptr::null_mut();
            drop(Box::from_raw(owned));
            Ok(())
        })
    }
}

/// Copies a diagnostic into owned UTF-8 bytes.
///
/// # Safety
/// `diagnostic` is live; `out` and optional `error` are disjoint writable
/// slots. No concurrent access or freeing of `diagnostic` is allowed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_diagnostic_message(
    diagnostic: *const QdkDiagnostic,
    out: *mut QdkBytes,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: validity and non-aliasing are caller obligations.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            *out = required(diagnostic, "diagnostic")?
                .message
                .as_bytes()
                .to_vec()
                .into();
            Ok(())
        })
    }
}

/// Creates an owned UTF-8 diagnostic, including for a foreign callback failure.
/// NULL data is valid only for an empty message.
///
/// # Safety
/// The input covers `len` initialized bytes for this call. Output slots are
/// valid, writable and disjoint from the input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_diagnostic_new(
    data: *const u8,
    len: usize,
    out: *mut *mut QdkDiagnostic,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: the caller guarantees allocation extent and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let message = boundary::input_text(data, len)?.to_owned();
            *out = Box::into_raw(Box::new(QdkDiagnostic { message }));
            Ok(())
        })
    }
}

/// Copies diagnostic UTF-8 bytes without a terminator. Sets `needed` to the
/// complete byte length. Insufficient capacity returns `QDK_BUFFER_TOO_SMALL`
/// without writing any bytes; NULL data with zero capacity is a size query.
///
/// # Safety
/// The diagnostic is live. Output pointers cover the declared capacity and
/// are disjoint from the diagnostic and one another.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_diagnostic_copy(
    diagnostic: *const QdkDiagnostic,
    data: *mut u8,
    capacity: usize,
    needed: *mut usize,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides valid, disjoint allocations and a live diagnostic.
    unsafe {
        boundary(error, None, || {
            let needed = output(needed)?;
            let message = &required(diagnostic, "diagnostic")?.message;
            boundary::copy_out(message.as_bytes(), data, capacity, needed)
        })
    }
}

/// Releases a diagnostic and sets its slot to NULL. NULL is a no-op.
///
/// # Safety
/// A non-NULL slot contains NULL or a live diagnostic allocated by this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_diagnostic_free(diagnostic: *mut *mut QdkDiagnostic) {
    // SAFETY: the caller supplies an exclusively owned diagnostic slot.
    unsafe {
        if let Some(slot) = diagnostic.as_mut()
            && !slot.is_null()
        {
            drop(Box::from_raw(*slot));
            *slot = ptr::null_mut();
        }
    }
}

/// Releases owned bytes and clears the descriptor. NULL is a no-op.
///
/// # Safety
/// The descriptor must be unchanged since a successful library output, not
/// already released, and exclusively owned by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_bytes_free(bytes: *mut QdkBytes) {
    // SAFETY: the descriptor retains the original boxed slice's pointer/length.
    unsafe {
        if let Some(bytes) = bytes.as_mut() {
            if !bytes.data.is_null() {
                drop(Box::from_raw(ptr::slice_from_raw_parts_mut(
                    bytes.data, bytes.len,
                )));
            }
            *bytes = QdkBytes::default();
        }
    }
}

#[cfg(test)]
mod tests;
