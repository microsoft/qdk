// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Bridges a Python include source (a `dict`, a `Callable`, or `None`) to the
//! Rust [`SourceResolver`] trait so `parse`/`analyze` can resolve `include`
//! directives from caller-supplied sources.

use pyo3::prelude::*;
use pyo3::types::PyDict;
use qdk_openqasm::io::{self, SourceResolver, SourceResolverContext};
use qdk_openqasm::span::Span;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use super::source::SourceDocument;

pub(crate) struct SnapshotResolver {
    sources: FxHashMap<Arc<str>, (Arc<str>, Arc<str>)>,
    ctx: SourceResolverContext,
}

impl SnapshotResolver {
    pub(crate) fn from_document(document: &SourceDocument) -> Self {
        let mut sources = FxHashMap::default();
        for (path, text, aliases) in document.resolved_sources() {
            let resolved = (path.clone(), text);
            sources.insert(path, resolved.clone());
            for alias in aliases.iter() {
                sources.insert(alias.clone(), resolved.clone());
            }
        }
        Self {
            sources,
            ctx: SourceResolverContext::default(),
        }
    }
}

impl SourceResolver for SnapshotResolver {
    fn ctx(&mut self) -> &mut SourceResolverContext {
        &mut self.ctx
    }

    fn resolve(
        &mut self,
        path: &Arc<str>,
        original_path: &Arc<str>,
    ) -> miette::Result<(Arc<str>, Arc<str>), io::Error> {
        self.sources
            .get(path)
            .cloned()
            .ok_or_else(|| not_found(original_path))
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use qdk_openqasm::parser::SourceStatus;

    struct RenamingResolver {
        source: Arc<str>,
        ctx: SourceResolverContext,
    }

    impl SourceResolver for RenamingResolver {
        fn ctx(&mut self) -> &mut SourceResolverContext {
            &mut self.ctx
        }

        fn resolve(
            &mut self,
            path: &Arc<str>,
            original_path: &Arc<str>,
        ) -> miette::Result<(Arc<str>, Arc<str>), io::Error> {
            if path.as_ref() == "pkg/defs.inc" {
                Ok(("memory://defs.inc".into(), self.source.clone()))
            } else {
                Err(not_found(original_path))
            }
        }
    }

    #[test]
    fn snapshot_resolver_replays_successful_paths_and_aliases_only() {
        let source = "gate local q { x q; }";
        let mut original_resolver = RenamingResolver {
            source: source.into(),
            ctx: SourceResolverContext::default(),
        };
        let result = qdk_openqasm::parse_source(
            "OPENQASM 3.0; include \"../defs.inc\";",
            "pkg/app/main.qasm",
            Some(&mut original_resolver),
        );
        assert_eq!(
            result.source_snapshot.files()[1].status,
            SourceStatus::Resolved
        );
        let document = SourceDocument::from_snapshot(&result.source_snapshot);
        let mut resolver = SnapshotResolver::from_document(&document);

        let alias = resolver
            .resolve(&"pkg/defs.inc".into(), &"../defs.inc".into())
            .expect("normalized alias should resolve");
        let resolved_path = resolver
            .resolve(&"memory://defs.inc".into(), &"memory://defs.inc".into())
            .expect("resolved path should resolve");
        let missing = resolver.resolve(&"new.inc".into(), &"new.inc".into());

        assert_eq!(alias.0.as_ref(), "memory://defs.inc");
        assert_eq!(alias.1.as_ref(), source);
        assert_eq!(resolved_path, alias);
        assert!(missing.is_err(), "new paths should remain unresolved");
    }
}
