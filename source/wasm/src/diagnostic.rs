// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    line_column::{Location, Range},
    serializable_type,
};
use miette::{Diagnostic, LabeledSpan, Severity};
use qsc::{self, SourceName, Span, error::WithSource, interpret, project};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{fmt::Write, iter};
use wasm_bindgen::prelude::*;

serializable_type! {
    VSDiagnostic,
    {
        pub range: Range,
        pub message: String,
        pub severity: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub code: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub uri: Option<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pub related: Vec<Related>
    },
    r#"export interface VSDiagnostic {
        range: IRange,
        message: string;
        severity: "error" | "warning" | "info"
        code?: string;
        uri?: string;
        related?: IRelatedInformation[];
    }"#
}

serializable_type! {
    Related,
    {
        pub location: Location,
        pub message: String,
    },
    r#"export interface IRelatedInformation {
        location: ILocation,
        message: string;
    }"#
}

serializable_type! {
    /// Representation of an error that can be used to display a squiggle
    /// in the editor or to populate the Problems view in VS Code.
    QSharpError,
    {
        document: String,
        diagnostic: VSDiagnostic,
        stack: Option<String>,
    },
    r#"export interface IQSharpError {
        /** Source URI or name */
        document: string;
        diagnostic: VSDiagnostic;
        stack?: string;
    }"#,
    IQSharpError
}

impl VSDiagnostic {
    pub fn json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("serializing VSDiagnostic should succeed")
    }

    /// Creates a [`VSDiagnostic`] from a compiler error. See `VSDiagnostic::new()` for details.
    fn from_compile_error(source_name: &str, err: &qsc::compile::Error) -> Self {
        let labels = error_labels(err);

        Self::new(labels, source_name, err)
    }

    /// Creates a [`VSDiagnostic`] from a language service error.
    pub(crate) fn from_ls_error(source_name: &str, err: &qsls::protocol::ErrorKind) -> Self {
        match err {
            qsls::protocol::ErrorKind::Compile(e) => Self::from_compile_error(source_name, e),
            qsls::protocol::ErrorKind::Project(e) => Self::new(Vec::new(), source_name, e),
            e @ qsls::protocol::ErrorKind::DocumentStatus { .. } => {
                Self::new(Vec::new(), source_name, e)
            }
        }
    }

    /// Creates a [`VSDiagnostic`] using the information from a [`miette::Diagnostic`].
    /// The error message, code and severity are straightforwardly generated,
    /// while mapping label spans is a little trickier.
    ///
    /// While a [`miette::Diagnostic`] can be associated with zero or more spans,
    /// a [`VSDiagnostic`] is intended to be shown as a squiggle in a specific document,
    /// and therefore needs to have at least one span that falls in the current document.
    ///
    /// If the first label's span falls in the current document, that span will be
    /// used for the squiggle. Some errors are not associated with a span
    /// at all, e.g. "entry point not found". Some other errors, e.g. some runtime errors,
    /// originate outside the current file. In those cases, a default span is
    /// used just to make the squiggle visible in the current document.
    ///
    /// Any labels with associated messages are included as "related information"
    /// objects in the [`VSDiagnostic`], whether they fall in the current document or not.
    /// Editors can display these as links to other source locations.
    fn new<T>(labels: Vec<Label>, source_name: &str, err: &T) -> VSDiagnostic
    where
        T: Diagnostic,
    {
        let mut labels = labels.into_iter().peekable();

        let default = qsc::line_column::Range {
            start: qsc::line_column::Position { line: 0, column: 0 },
            end: qsc::line_column::Position { line: 0, column: 1 },
        };
        let range = labels
            .peek()
            .filter(|l| l.source_name.as_ref() == source_name)
            .map_or(default, |l| l.range);

        let related: Vec<Related> = labels
            .filter_map(|label| {
                match label.message {
                    Some(message) => Some(Related {
                        // Here, the stdlib/core files could be mapped to
                        // "qsharp-library-source" uris to allow for navigation
                        // in VS Code, but currently only the file path is returned.
                        location: Location {
                            source: label.source_name.to_string(),
                            span: label.range.into(),
                        },
                        message,
                    }),
                    None => None,
                }
            })
            .collect();

        // e.g. "runtime error"
        let mut message = err.to_string();
        for source in iter::successors(std::error::Error::source(&err), |e| e.source()) {
            // e.g. " Qubit0 released while not in |0⟩ state"
            write!(message, ": {source}").expect("message should be writable");
        }
        if let Some(help) = err.help() {
            // e.g. "qubits should be returned to the |0⟩ state before being released to satisfy the assumption that allocated qubits start in the |0⟩ state"
            write!(message, "\n\nhelp: {help}").expect("message should be writable");
        }

        // e.g. Qsc.Eval.ReleasedQubitNotZero
        let code = err.code().map(|c| c.to_string());

        // e.g. https://aka.ms/qdk.qir
        let uri = err.url().map(|u| u.to_string());

        Self {
            range: range.into(),
            severity: (match err.severity().unwrap_or(Severity::Error) {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Advice => "info",
            })
            .to_string(),
            message,
            code,
            uri,
            related,
        }
    }
}

struct Label {
    pub source_name: SourceName,
    pub range: qsc::line_column::Range,
    pub message: Option<String>,
}

fn error_labels<T>(e: &WithSource<T>) -> Vec<Label>
where
    T: Diagnostic + Send + Sync,
{
    e.labels()
        .into_iter()
        .flatten()
        .map(|l| resolve_label(e, &l))
        .collect()
}

fn resolve_label<T>(e: &WithSource<T>, labeled_span: &LabeledSpan) -> Label
where
    T: Diagnostic + Send + Sync,
{
    let (source, span) = e.resolve_span(labeled_span.inner());
    let start = u32::try_from(span.offset()).expect("offset should fit in u32");
    let len = u32::try_from(span.len()).expect("length should fit in u32");
    let range = qsc::line_column::Range::from_span(
        qsc::line_column::Encoding::Utf16,
        &source.contents,
        &Span {
            lo: start,
            hi: start + len,
        },
    );

    Label {
        source_name: source.name.clone(),
        range,
        message: labeled_span.label().map(ToString::to_string),
    }
}

pub fn interpret_errors_to_run_result(errs: &[interpret::Error]) -> serde_json::Value {
    let qsharp_errors = interpret_errors_into_qsharp_errors(errs);
    let mut vs_diagnostic = qsharp_errors[0].diagnostic.json();
    vs_diagnostic
        .as_object_mut()
        .expect("VSDiagnostic should be an object")
        .insert("errors".to_string(), json!(qsharp_errors));
    vs_diagnostic
}

/// Converts interpreter errors into the error type that is suitable for
/// display as squiggles in the editor, and in the Problems view in VS Code
pub fn interpret_errors_into_qsharp_errors(errs: &[interpret::Error]) -> Vec<QSharpError> {
    let default_uri = "<project>";
    errs.iter()
        .map(|err| {
            let labels = interpret_error_labels(err);

            let doc = labels
                .first()
                .map_or_else(|| default_uri.to_string(), |l| l.source_name.to_string());

            let vsdiagnostic = VSDiagnostic::new(labels, &doc, err);

            let stack_trace = if let interpret::Error::Eval(_) = err {
                err.stack_trace()
            } else {
                None
            };

            QSharpError {
                document: doc,
                diagnostic: vsdiagnostic,
                stack: stack_trace.cloned(),
            }
        })
        .collect()
}

pub fn project_errors_into_qsharp_errors(
    project_dir: &str,
    errs: &[project::Error],
) -> Vec<QSharpError> {
    errs.iter()
        .map(|err| {
            let doc_uri = err.path().map_or(project_dir, |p| p.as_str());

            let vsdiagnostic = VSDiagnostic::new(Vec::default(), doc_uri, err);

            QSharpError {
                document: doc_uri.to_string(),
                diagnostic: vsdiagnostic,
                stack: None,
            }
        })
        .collect()
}

fn interpret_error_labels(err: &interpret::Error) -> Vec<Label> {
    match err {
        interpret::Error::Eval(e) => error_labels(e.error()),
        interpret::Error::Compile(e) => error_labels(e),
        interpret::Error::Pass(e) => error_labels(e),
        interpret::Error::PartialEvaluation(e) => error_labels(e),
        interpret::Error::NoEntryPoint
        | interpret::Error::UnsupportedRuntimeCapabilities
        | interpret::Error::Circuit(_)
        | interpret::Error::NotAnOperation
        | interpret::Error::NotACallable => Vec::new(),
    }
}
