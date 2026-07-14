// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Structural equality and hashing shared by every node, type, and value class.
//!
//! Two trees parsed from the same source are different Python objects, so
//! identity equality makes them compare unequal, which surprises anyone coming
//! from `openqasm3`. These helpers compare and hash a class through a declared
//! list of participating attributes instead.
//!
//! Source positions are excluded, so the same construct at two different
//! offsets compares equal. Attributes are read through their Python getters, so
//! equality cannot drift from the accessor it names.

use pyo3::prelude::*;
use pyo3::types::{PyList, PyTuple};

/// Compares two objects by concrete type, then by each participating attribute.
///
/// Comparing the type first is what keeps two empty-bodied classes, such as
/// `BreakStatement` and `ContinueStatement`, from comparing equal.
pub(crate) fn structural_eq(
    slf: &Bound<'_, PyAny>,
    other: &Bound<'_, PyAny>,
    fields: &[&str],
) -> PyResult<bool> {
    if !slf.get_type().is(other.get_type()) {
        return Ok(false);
    }
    for name in fields {
        if !slf.getattr(*name)?.eq(other.getattr(*name)?)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Hashes the concrete type together with each participating attribute.
pub(crate) fn structural_hash(slf: &Bound<'_, PyAny>, fields: &[&str]) -> PyResult<isize> {
    let py = slf.py();
    let mut values: Vec<Bound<'_, PyAny>> = Vec::with_capacity(fields.len() + 1);
    values.push(slf.get_type().into_any());
    for name in fields {
        values.push(hashable(&slf.getattr(*name)?)?);
    }
    PyTuple::new(py, values)?.hash()
}

/// Converts a child list into a tuple, because a Python list is unhashable.
fn hashable<'py>(value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    if value.is_instance_of::<PyList>() {
        return Ok(
            PyTuple::new(value.py(), value.try_iter()?.collect::<PyResult<Vec<_>>>()?)?.into_any(),
        );
    }
    Ok(value.clone())
}
