// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A plain, miette-free projection of the crate's layered diagnostics.
//!
//! [`Diagnostic`] mirrors the information carried by a
//! [`WithSource<crate::error::Error>`](crate::compat::WithSource) (message,
//! severity, code, labels, and related diagnostics) without exposing any
//! `miette` types. This keeps downstream consumers (such as language bindings)
//! decoupled from `miette` internals.

use crate::compat::Span;

/// The severity of a [`Diagnostic`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Advice,
}

/// A labeled region of source associated with a [`Diagnostic`].
#[derive(Clone, Debug)]
pub struct Label {
    /// The span the label points at.
    pub span: Span,
    /// An optional message describing the label.
    pub message: Option<String>,
}

/// A plain projection of a layered diagnostic.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    /// The primary, human-readable message.
    pub message: String,
    /// The diagnostic's severity.
    pub severity: Severity,
    /// An optional machine-readable code (e.g. `Qasm.Parse.Token`).
    pub code: Option<String>,
    /// Source labels attached to the diagnostic.
    pub labels: Vec<Label>,
    /// Related diagnostics, projected recursively.
    pub related: Vec<Diagnostic>,
}

impl From<crate::compat::WithSource<crate::error::Error>> for Diagnostic {
    fn from(value: crate::compat::WithSource<crate::error::Error>) -> Self {
        project(&value)
    }
}

/// Projects any `miette::Diagnostic` into the plain [`Diagnostic`] struct,
/// recursing through related diagnostics.
fn project(diag: &dyn miette::Diagnostic) -> Diagnostic {
    let message = diag.to_string();

    let severity = match diag.severity() {
        Some(miette::Severity::Advice) => Severity::Advice,
        Some(miette::Severity::Warning) => Severity::Warning,
        // miette treats `None` as the default severity, which is `Error`.
        _ => Severity::Error,
    };

    let code = diag.code().map(|code| code.to_string());

    let labels = diag
        .labels()
        .into_iter()
        .flatten()
        .map(|label| Label {
            span: Span {
                lo: u32::try_from(label.offset()).unwrap_or(u32::MAX),
                hi: u32::try_from(label.offset().saturating_add(label.len())).unwrap_or(u32::MAX),
            },
            message: label.label().map(ToString::to_string),
        })
        .collect();

    let related = diag.related().into_iter().flatten().map(project).collect();

    Diagnostic {
        message,
        severity,
        code,
        labels,
        related,
    }
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, Severity};

    #[test]
    fn projects_message_severity_and_labels_from_parse_error() {
        // Missing terminating semicolon produces a labeled parser error.
        let result = crate::parse_source("qubit q", "test.qasm");
        let errors = result.all_errors();
        assert!(
            !errors.is_empty(),
            "expected the erroneous source to produce diagnostics"
        );

        let diagnostic: Diagnostic = errors
            .into_iter()
            .next()
            .expect("at least one error")
            .into();

        assert!(
            !diagnostic.message.is_empty(),
            "message should be populated"
        );
        assert_eq!(diagnostic.severity, Severity::Error);
        assert!(
            !diagnostic.labels.is_empty(),
            "parser errors should carry at least one label"
        );
    }
}
