// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    cell::Cell,
    mem,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr, slice, str,
};

use crate::{
    QDK_INTERNAL_ERROR, QDK_INVALID_ARGUMENT, QDK_OK, QDK_UNFINISHED, QdkDiagnostic, QdkStatus,
};

pub(crate) struct Error {
    pub status: QdkStatus,
    pub message: String,
}

impl Error {
    pub fn new(status: QdkStatus, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(QDK_INVALID_ARGUMENT, message)
    }
}

impl From<qdk_circuit_api::CircuitError> for Error {
    fn from(error: qdk_circuit_api::CircuitError) -> Self {
        let status = if matches!(error, qdk_circuit_api::CircuitError::Unfinished(_)) {
            QDK_UNFINISHED
        } else {
            QDK_INVALID_ARGUMENT
        };
        Self::new(status, error.to_string())
    }
}

impl From<qsc_circuit_store::StoreError> for Error {
    fn from(error: qsc_circuit_store::StoreError) -> Self {
        Self::invalid(error.to_string())
    }
}

pub(crate) unsafe fn copy_out<T: Copy>(
    values: &[T],
    data: *mut T,
    capacity: usize,
    needed: *mut usize,
) -> Result<(), Error> {
    // SAFETY: the caller provides disjoint writable output allocations.
    unsafe {
        *output(needed)? = values.len();
        if capacity < values.len() {
            return Err(Error::new(
                crate::QDK_BUFFER_TOO_SMALL,
                "output buffer is too small",
            ));
        }
        if capacity
            .checked_mul(mem::size_of::<T>())
            .is_none_or(|bytes| bytes > isize::MAX as usize)
        {
            return Err(Error::invalid(
                "output capacity exceeds the addressable range",
            ));
        }
        if capacity != 0 && (data.is_null() || !data.is_aligned()) {
            return Err(Error::invalid(
                "nonempty output buffer must be non-NULL and aligned",
            ));
        }
        if !values.is_empty() {
            ptr::copy_nonoverlapping(values.as_ptr(), data, values.len());
        }
        Ok(())
    }
}

/// The optional poison flag belongs to any owner the closure might mutate.
pub(crate) unsafe fn boundary(
    error: *mut *mut QdkDiagnostic,
    poison: Option<&Cell<bool>>,
    action: impl FnOnce() -> Result<(), Error>,
) -> QdkStatus {
    // SAFETY: caller provides a valid, disjoint error slot or NULL.
    unsafe {
        if !error.is_null() {
            *error = ptr::null_mut();
        }
    }
    let result = catch_unwind(AssertUnwindSafe(action));
    let failure = match result {
        Ok(Ok(())) => return QDK_OK,
        Ok(Err(error)) => error,
        Err(_) => {
            if let Some(poison) = poison {
                poison.set(true);
            }
            Error::new(
                QDK_INTERNAL_ERROR,
                "internal Rust panic; a possibly mutated owner is poisoned",
            )
        }
    };
    if !error.is_null() {
        // SAFETY: same writable diagnostic output slot as above.
        unsafe {
            *error = Box::into_raw(Box::new(QdkDiagnostic {
                message: failure.message,
            }));
        }
    }
    failure.status
}

pub(crate) unsafe fn required<'a, T>(value: *const T, name: &str) -> Result<&'a T, Error> {
    if value.is_null() || !value.is_aligned() {
        return Err(Error::invalid(format!(
            "{name} must be non-NULL and aligned"
        )));
    }
    // SAFETY: allocation validity/extent and lifetime are caller obligations.
    Ok(unsafe { &*value })
}

pub(crate) unsafe fn slot<'a, T>(value: *mut T, name: &str) -> Result<&'a mut T, Error> {
    if value.is_null() || !value.is_aligned() {
        return Err(Error::invalid(format!(
            "{name} must be non-NULL and aligned"
        )));
    }
    // SAFETY: caller guarantees exclusive access to a valid output slot.
    Ok(unsafe { &mut *value })
}

pub(crate) unsafe fn output<'a, T: Default>(value: *mut T) -> Result<&'a mut T, Error> {
    if value.is_null() || !value.is_aligned() {
        return Err(Error::invalid("output must be non-NULL and aligned"));
    }
    // SAFETY: output slots may be uninitialized. Initialize through the raw
    // pointer before forming a reference; the caller guarantees writable extent.
    unsafe {
        value.write(T::default());
        Ok(&mut *value)
    }
}

pub(crate) unsafe fn release<T, U>(
    value: *mut *mut T,
    handle: impl Fn(&T) -> &crate::handle::Handle<U>,
) -> Result<(), Error> {
    // SAFETY: caller owns this slot and any non-NULL live handle it contains.
    unsafe {
        if value.is_null() {
            return Ok(());
        }
        let slot = slot(value, "owning slot")?;
        if slot.is_null() {
            return Ok(());
        }
        handle(required(*slot, "handle")?).require_releasable()?;
        let owned = *slot;
        *slot = ptr::null_mut();
        drop(Box::from_raw(owned));
        Ok(())
    }
}

pub(crate) unsafe fn input_slice<'a, T>(data: *const T, len: usize) -> Result<&'a [T], Error> {
    if len == 0 {
        return Ok(&[]);
    }
    if len
        .checked_mul(mem::size_of::<T>())
        .is_none_or(|bytes| bytes > isize::MAX as usize)
    {
        return Err(Error::invalid("array length exceeds the addressable range"));
    }
    if data.is_null() || !data.is_aligned() {
        return Err(Error::invalid(
            "nonempty array must be non-NULL and aligned",
        ));
    }
    // SAFETY: allocation extent and initialized elements are caller obligations;
    // null/alignment/size preconditions are checked above.
    Ok(unsafe { slice::from_raw_parts(data, len) })
}

pub(crate) unsafe fn input_text<'a>(data: *const u8, len: usize) -> Result<&'a str, Error> {
    // SAFETY: shares input_slice's caller contract.
    str::from_utf8(unsafe { input_slice(data, len)? })
        .map_err(|error| Error::invalid(format!("input is not UTF-8: {error}")))
}
