// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Plain, `miette`-free Python projections of the crate's layered diagnostics.
//!
//! The crate reports errors as `miette::Diagnostic`s (specifically
//! `WithSource<Error>`). These types flatten that surface into simple, owned
//! Python objects so callers do not need any `miette` knowledge to inspect
//! parse and analysis diagnostics.

use crate::qasm_ast::span::Span;
use miette::{
    Diagnostic as MietteDiagnostic, GraphicalReportHandler, GraphicalTheme, LabeledSpan,
    NamedSource, Severity as MietteSeverity, SourceCode, SourceSpan,
};
use pyo3::prelude::*;
use std::fmt;
use std::io::IsTerminal;

/// The severity of a [`Diagnostic`].
#[pyclass(module = "qdk._native", eq, eq_int, frozen, skip_from_py_object)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Severity {
    Error,
    Warning,
    Advice,
}

impl From<MietteSeverity> for Severity {
    fn from(value: MietteSeverity) -> Self {
        match value {
            MietteSeverity::Error => Severity::Error,
            MietteSeverity::Warning => Severity::Warning,
            MietteSeverity::Advice => Severity::Advice,
        }
    }
}

impl From<Severity> for MietteSeverity {
    fn from(value: Severity) -> Self {
        match value {
            Severity::Error => MietteSeverity::Error,
            Severity::Warning => MietteSeverity::Warning,
            Severity::Advice => MietteSeverity::Advice,
        }
    }
}

/// A labeled region of source associated with a [`Diagnostic`].
#[pyclass(module = "qdk._native", frozen, skip_from_py_object)]
#[derive(Clone)]
pub(crate) struct Label {
    /// The span the label points at.
    #[pyo3(get)]
    pub span: Span,
    /// An optional message describing the label.
    #[pyo3(get)]
    pub message: Option<String>,
}

#[pymethods]
impl Label {
    fn __repr__(&self) -> String {
        format!("Label(span={:?}, message={:?})", self.span, self.message)
    }
}

/// A plain projection of a layered diagnostic.
#[pyclass(module = "qdk._native", frozen, skip_from_py_object)]
#[derive(Clone)]
pub(crate) struct Diagnostic {
    /// The primary, human-readable message.
    #[pyo3(get)]
    pub message: String,
    /// The diagnostic's severity.
    #[pyo3(get)]
    pub severity: Severity,
    /// An optional machine-readable code (e.g. `Qasm.Parse.Token`).
    #[pyo3(get)]
    pub code: Option<String>,
    /// Source labels attached to the diagnostic.
    #[pyo3(get)]
    pub labels: Vec<Label>,
    /// Related diagnostics, projected recursively.
    #[pyo3(get)]
    pub related: Vec<Diagnostic>,
    /// The pretty, source-annotated rendering of the diagnostic.
    ///
    /// This is the `miette` graphical rendering (no color, fixed width), the
    /// same style used elsewhere in the QDK to display diagnostics.
    rendered: String,
    /// The name of the source the diagnostic refers to, if any.
    source_name: Option<String>,
    /// The full source text the diagnostic refers to, if any.
    ///
    /// Retained so [`Diagnostic::render`] can redraw the source-annotated form
    /// with caller-chosen color, Unicode, and width settings.
    source_text: Option<String>,
}

#[pymethods]
impl Diagnostic {
    /// The pretty, source-annotated rendering of the diagnostic.
    fn __str__(&self) -> String {
        self.rendered.clone()
    }

    /// Renders the diagnostic to its pretty, source-annotated form.
    ///
    /// Unlike `str(diagnostic)`, which is a fixed no-color rendering, this lets
    /// the caller control the output for the current terminal:
    ///
    /// * `color` - emit ANSI color. When `None`, color is enabled only if
    ///   standard output is a terminal and `NO_COLOR` is unset.
    /// * `unicode` - use Unicode box-drawing (`True`) or ASCII (`False`).
    ///   Defaults to `True`.
    /// * `width` - wrap width in columns. Defaults to 80.
    #[pyo3(signature = (*, color=None, unicode=None, width=None))]
    fn render(&self, color: Option<bool>, unicode: Option<bool>, width: Option<usize>) -> String {
        let color = color.unwrap_or_else(color_auto_enabled);
        let unicode = unicode.unwrap_or(true);
        let theme = match (unicode, color) {
            (true, true) => GraphicalTheme::unicode(),
            (true, false) => GraphicalTheme::unicode_nocolor(),
            (false, true) => GraphicalTheme::ascii(),
            (false, false) => GraphicalTheme::none(),
        };
        let mut handler = GraphicalReportHandler::new_themed(theme);
        if let Some(width) = width {
            handler = handler.with_width(width);
        }
        let renderable = RenderableDiagnostic::new(self);
        let mut out = String::new();
        handler
            .render_report(&mut out, &renderable)
            .expect("rendering a diagnostic into a String should not fail");
        out
    }

    fn __repr__(&self) -> String {
        format!(
            "Diagnostic(severity={:?}, message={:?})",
            self.severity, self.message
        )
    }
}

/// Returns whether ANSI color should be emitted by default.
///
/// Mirrors the common convention: color when standard output is a terminal and
/// the `NO_COLOR` environment variable (see <https://no-color.org>) is unset.
fn color_auto_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}

/// A `miette::Diagnostic` view over a projected [`Diagnostic`], used to redraw
/// the source-annotated form on demand from the retained flat fields and source.
struct RenderableDiagnostic<'a> {
    diag: &'a Diagnostic,
    source: Option<NamedSource<String>>,
}

impl<'a> RenderableDiagnostic<'a> {
    fn new(diag: &'a Diagnostic) -> Self {
        let source = diag.source_text.as_ref().map(|text| {
            let name = diag.source_name.as_deref().unwrap_or("<source>");
            NamedSource::new(name, text.clone())
        });
        RenderableDiagnostic { diag, source }
    }
}

impl fmt::Debug for RenderableDiagnostic<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.diag.message)
    }
}

impl fmt::Display for RenderableDiagnostic<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.diag.message)
    }
}

impl std::error::Error for RenderableDiagnostic<'_> {}

impl MietteDiagnostic for RenderableDiagnostic<'_> {
    fn code<'b>(&'b self) -> Option<Box<dyn fmt::Display + 'b>> {
        self.diag
            .code
            .as_ref()
            .map(|code| Box::new(code.clone()) as Box<dyn fmt::Display>)
    }

    fn severity(&self) -> Option<MietteSeverity> {
        Some(self.diag.severity.into())
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        if self.diag.labels.is_empty() {
            return None;
        }
        let labels = self.diag.labels.iter().map(|label| {
            let len = (label.span.hi - label.span.lo) as usize;
            LabeledSpan::new(label.message.clone(), label.span.lo as usize, len)
        });
        Some(Box::new(labels))
    }

    fn source_code(&self) -> Option<&dyn SourceCode> {
        self.source.as_ref().map(|source| source as &dyn SourceCode)
    }
}

/// Projects a `miette::Diagnostic` (such as the crate's `WithSource<Error>`)
/// into the plain [`Diagnostic`] surface exposed to Python.
pub(crate) fn diagnostic_from(diag: &dyn MietteDiagnostic) -> Diagnostic {
    let severity = diag.severity().map_or(Severity::Error, Severity::from);
    let code = diag.code().map(|code| code.to_string());
    let labels = diag.labels().map_or_else(Vec::new, |labels| {
        labels
            .map(|labeled| {
                let lo = u32::try_from(labeled.offset()).expect("offset should fit into u32");
                let len = u32::try_from(labeled.len()).expect("length should fit into u32");
                Label {
                    span: Span { lo, hi: lo + len },
                    message: labeled.label().map(ToString::to_string),
                }
            })
            .collect()
    });
    let related = diag
        .related()
        .map_or_else(Vec::new, |related| related.map(diagnostic_from).collect());
    let (source_name, source_text) = extract_source(diag);
    Diagnostic {
        message: diag.to_string(),
        severity,
        code,
        labels,
        related,
        rendered: render_diagnostic(diag),
        source_name,
        source_text,
    }
}

/// Extracts the source name and full text a diagnostic refers to, if available.
///
/// Reads the whole source starting at offset 0 so the retained text aligns with
/// the diagnostic's absolute label offsets.
fn extract_source(diag: &dyn MietteDiagnostic) -> (Option<String>, Option<String>) {
    let Some(source_code) = diag.source_code() else {
        return (None, None);
    };
    let span = SourceSpan::new(0.into(), 0);
    let Ok(contents) = source_code.read_span(&span, 0, usize::MAX) else {
        return (None, None);
    };
    let text = String::from_utf8_lossy(contents.data()).into_owned();
    let name = contents.name().map(ToString::to_string);
    (name, Some(text))
}

/// Renders a diagnostic to its pretty, source-annotated form.
///
/// Uses the `miette` graphical handler with a no-color theme and a fixed width
/// so the output is deterministic and independent of the terminal environment.
fn render_diagnostic(diag: &dyn MietteDiagnostic) -> String {
    let mut out = String::new();
    let handler =
        GraphicalReportHandler::new_themed(GraphicalTheme::unicode_nocolor()).with_width(80);
    // Rendering only fails if the underlying writer fails; a `String` never does.
    handler
        .render_report(&mut out, diag)
        .expect("rendering a diagnostic into a String should not fail");
    out
}
