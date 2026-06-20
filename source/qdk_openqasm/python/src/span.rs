// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A source [`Span`] projection exposed to Python.

use pyo3::prelude::*;

/// A half-open `[lo, hi)` byte range into a source string.
#[pyclass(
    module = "qdk_openqasm_parser._native",
    frozen,
    eq,
    hash,
    from_py_object
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    /// The inclusive start offset, in bytes.
    #[pyo3(get)]
    pub lo: u32,
    /// The exclusive end offset, in bytes.
    #[pyo3(get)]
    pub hi: u32,
}

impl From<qdk_openqasm_parser::Span> for Span {
    fn from(span: qdk_openqasm_parser::Span) -> Self {
        Span {
            lo: span.lo,
            hi: span.hi,
        }
    }
}

#[pymethods]
impl Span {
    #[new]
    fn new(lo: u32, hi: u32) -> Self {
        Span { lo, hi }
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __repr__(&self) -> String {
        format!("Span(lo={}, hi={})", self.lo, self.hi)
    }
}
