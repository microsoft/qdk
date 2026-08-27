// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Python-spelling helpers shared by every `__repr__` in the `qdk.openqasm`
//! bindings.
//!
//! Rust's `Display` and `Debug` spellings are not Python spellings: `true` is
//! not `True`, and `Some("3.0")` is not a value any Python user recognizes.
//! These helpers keep the projections honest so a `repr` can be read, and
//! usually pasted, as Python.

use pyo3::prelude::*;

/// Renders a `bool` the way Python spells it.
pub(crate) fn py_bool(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

/// Renders a string as a Python string literal.
pub(crate) fn py_str(value: &str) -> String {
    format!("{value:?}")
}

/// Renders an `f64` the way Python spells it.
///
/// Rust prints a whole float as `100` and a non-number as `NaN`, neither of which
/// is how Python renders the same value.
pub(crate) fn py_float(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_positive() {
            "inf".to_string()
        } else {
            "-inf".to_string()
        };
    }
    let rendered = format!("{value}");
    if rendered.contains(['.', 'e']) {
        rendered
    } else {
        format!("{rendered}.0")
    }
}

/// Renders an optional string as a Python string literal or `None`.
pub(crate) fn py_opt_str(value: Option<&str>) -> String {
    value.map_or_else(|| "None".to_string(), py_str)
}

/// Strips the raw-identifier prefix a Rust field needs but Python never sees.
///
/// A field declared `r#type` is exposed to Python as `type`, so both the label
/// and the attribute lookup have to use the Python spelling.
pub(crate) fn attr_name(name: &str) -> &str {
    name.strip_prefix("r#").unwrap_or(name)
}

/// Renders one of a node's own attributes using Python's `repr`.
///
/// Reading through the generated getter rather than the Rust field keeps one
/// spelling for every field type: a `String`, a `u32`, a pyclass enum, an
/// optional child, and a `Py<PyAny>` all convert exactly as they do for a
/// caller, so a `repr` can never disagree with the accessor beside it.
pub(crate) fn py_attr(node: &Bound<'_, PyAny>, name: &str) -> String {
    node.getattr(attr_name(name))
        .and_then(|value| value.repr())
        .map_or_else(
            |_| "<unrepresentable>".to_string(),
            |value| value.to_string(),
        )
}

/// Renders a child list as a length summary rather than expanding it.
///
/// A recursive `repr` over a tree with hundreds of thousands of nodes is
/// unusable, so lists report only how many children they hold.
pub(crate) fn py_attr_len(node: &Bound<'_, PyAny>, name: &str) -> String {
    node.getattr(attr_name(name))
        .and_then(|value| value.len())
        .map_or_else(|_| "[? items]".to_string(), py_items)
}

/// The Python-visible name of a node field, for use as a `repr` label.
pub(crate) fn py_label(name: &str) -> &str {
    attr_name(name)
}

/// Renders a count of child nodes as a compact, non-recursive summary.
pub(crate) fn py_items(count: usize) -> String {
    format!("[{count} items]")
}
