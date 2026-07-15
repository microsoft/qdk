// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Bridges a Python include source (a `dict`, a `Callable`, or `None`) to the
//! Rust [`SourceResolver`] trait so `parse`/`analyze` can resolve `include`
//! directives from caller-supplied sources.

use pyo3::prelude::*;
use pyo3::types::PyDict;
use qdk_openqasm::io::{self, SourceResolver, SourceResolverContext};
use qdk_openqasm::span::Span;
use std::sync::Arc;

/// A [`SourceResolver`] backed by a Python object.
///
/// The held object is one of:
/// - a `dict[str, str]` mapping include paths to their contents,
/// - a `Callable[[str], str]` returning contents for a requested path, or
/// - `None`, which resolves nothing (every include is reported as not found).
pub(crate) struct PySourceResolver {
    obj: Py<PyAny>,
    ctx: SourceResolverContext,
}

impl PySourceResolver {
    /// Creates a resolver from a Python object (`dict`, callable, or `None`).
    pub fn new(obj: Py<PyAny>) -> Self {
        PySourceResolver {
            obj,
            ctx: SourceResolverContext::default(),
        }
    }
}

fn not_found(path: &Arc<str>) -> io::Error {
    io::Error(io::ErrorKind::NotFound(
        Span::default(),
        format!("Could not resolve include file: {path}"),
    ))
}

fn io_error(path: &Arc<str>, message: &str) -> io::Error {
    io::Error(io::ErrorKind::IO(
        Span::default(),
        format!("Error resolving include file {path}: {message}"),
    ))
}

impl SourceResolver for PySourceResolver {
    fn ctx(&mut self) -> &mut SourceResolverContext {
        &mut self.ctx
    }

    fn resolve(
        &mut self,
        path: &Arc<str>,
        original_path: &Arc<str>,
    ) -> miette::Result<(Arc<str>, Arc<str>), io::Error> {
        Python::attach(|py| {
            let obj = self.obj.bind(py);

            if obj.is_none() {
                return Err(not_found(original_path));
            }

            if let Ok(dict) = obj.cast::<PyDict>() {
                return match dict.get_item(path.as_ref()) {
                    Ok(Some(value)) => {
                        let source = value
                            .extract::<String>()
                            .map_err(|err| io_error(original_path, &err.to_string()))?;
                        Ok((path.clone(), Arc::from(source.as_str())))
                    }
                    Ok(None) => Err(not_found(original_path)),
                    Err(err) => Err(io_error(original_path, &err.to_string())),
                };
            }

            if obj.is_callable() {
                let result = obj
                    .call1((path.as_ref(),))
                    .map_err(|err| io_error(original_path, &err.to_string()))?;
                if result.is_none() {
                    return Err(not_found(original_path));
                }
                let source = result
                    .extract::<String>()
                    .map_err(|err| io_error(original_path, &err.to_string()))?;
                return Ok((path.clone(), Arc::from(source.as_str())));
            }

            Err(io_error(
                original_path,
                "includes must be a dict[str, str], a callable, or None",
            ))
        })
    }
}
