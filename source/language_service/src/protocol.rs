// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use miette::Diagnostic;
use qsc::line_column::Range;
use qsc::location::Location;
use qsc::{LanguageFeatures, PackageType, project::Manifest, target::Profile};
use qsc::{compile, project};
use qsc_linter::LintOrGroupConfig;
use thiserror::Error;

/// A change to the workspace configuration
#[derive(Clone, Debug, Default)]
pub struct WorkspaceConfigurationUpdate {
    pub target_profile: Option<Profile>,
    pub package_type: Option<PackageType>,
    pub language_features: Option<LanguageFeatures>,
    pub lints_config: Option<Vec<LintOrGroupConfig>>,
    pub dev_diagnostics: Option<bool>,
}

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum ErrorKind {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Compile(#[from] compile::Error),
    #[error(transparent)]
    #[diagnostic(transparent)]
    Project(#[from] project::Error),
    #[error(transparent)]
    #[diagnostic(transparent)]
    DocumentStatus(#[from] DocumentStatusDiagnostic),
}

/// Document status is a non-user facing, info-level diagnostic meant for
/// development and debugging purposes.
/// When enabled, this diagnostic is always published for open documents,
/// and communicates the status of the document as understood by the language service.
#[derive(Clone, Debug, Diagnostic, Error)]
#[error("[qdk-status] compilation={compilation_name}, version={document_version}")]
#[diagnostic(severity(info), code("Qdk.Dev.DocumentStatus"))]
pub struct DocumentStatusDiagnostic {
    pub(crate) compilation_name: String,
    pub(crate) document_version: u32,
}

#[derive(Debug)]
pub struct DiagnosticUpdate {
    pub uri: String,
    pub version: Option<u32>,
    pub errors: Vec<ErrorKind>,
}

#[derive(Debug)]
pub struct TestCallable {
    /// This is a string that represents the interpreter-ready name of the test callable.
    /// i.e. "Main.TestCase". Call it by adding parens to the end, e.g. `Main.TestCase()`
    pub callable_name: Arc<str>,
    /// A string that represents the originating compilation URI of this callable
    pub compilation_uri: Arc<str>,
    pub location: Location,
    /// A human readable name that represents the compilation.
    pub friendly_name: Arc<str>,
}

#[derive(Debug)]
pub struct TestCallables {
    pub callables: Vec<TestCallable>,
}

#[derive(Debug)]
pub enum CodeActionKind {
    Empty,
    QuickFix,
    Refactor,
    RefactorExtract,
    RefactorInline,
    RefactorMove,
    RefactorRewrite,
    Source,
    SourceOrganizeImports,
    SourceFixAll,
    Notebook,
}

#[derive(Debug)]
pub struct CodeAction {
    pub title: String,
    pub edit: Option<WorkspaceEdit>,
    pub kind: Option<CodeActionKind>,
    pub is_preferred: Option<bool>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum CompletionItemKind {
    // It would have been nice to match the numeric values to the ones used by
    // VS Code and Monaco, but unfortunately those two disagree on the values.
    // So we define our own unique enum here to reduce confusion.
    Function,
    Interface,
    Keyword,
    Module,
    Property,
    Variable,
    TypeParameter,
    Field,
    Class,
}

#[derive(Debug, Default)]
pub struct CompletionList {
    pub items: Vec<CompletionItem>,
}

#[derive(Debug)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
    pub sort_text: Option<String>,
    pub detail: Option<String>,
    pub additional_text_edits: Option<Vec<TextEdit>>,
}

impl CompletionItem {
    #[must_use]
    pub fn new(label: String, kind: CompletionItemKind) -> Self {
        CompletionItem {
            label,
            kind,
            sort_text: None,
            detail: None,
            additional_text_edits: None,
        }
    }
}

impl PartialEq for CompletionItem {
    // exclude sort text for comparison
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label
            && self.kind == other.kind
            && self.detail == other.detail
            && self.additional_text_edits == other.additional_text_edits
    }
}

impl Eq for CompletionItem {}

use std::hash::{Hash, Hasher};
use std::sync::Arc;

impl Hash for CompletionItem {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // use only user-visible fields for hashing to
        // dedup items that look exactly the same.
        self.label.hash(state);
        self.kind.hash(state);
        self.detail.hash(state);
    }
}

#[derive(Debug, PartialEq)]
pub struct Hover {
    pub contents: String,
    pub span: Range,
}

#[derive(Debug)]
pub struct WorkspaceEdit {
    pub changes: Vec<(String, Vec<TextEdit>)>,
}

#[derive(Debug, PartialEq)]
pub struct TextEdit {
    pub new_text: String,
    pub range: Range,
}

#[derive(Debug, PartialEq)]
pub struct SignatureHelp {
    pub signatures: Vec<SignatureInformation>,
    pub active_signature: u32,
    pub active_parameter: u32,
}

#[derive(Debug, PartialEq)]
pub struct SignatureInformation {
    pub label: String,
    pub documentation: Option<String>,
    pub parameters: Vec<ParameterInformation>,
}

#[derive(Debug, PartialEq)]
pub struct ParameterInformation {
    /// The start and end offsets into the [`SignatureInformation::label`].
    /// They  use utf-8 or utf-16 code units depending on the
    /// configuration of the language service.
    pub label: (u32, u32),
    pub documentation: Option<String>,
}

#[derive(Default, Clone)]
pub struct NotebookMetadata {
    pub target_profile: Option<Profile>,
    pub language_features: LanguageFeatures,
    pub manifest: Option<Manifest>,
    pub project_root: Option<String>,
}

#[derive(Debug)]
pub struct CodeLens {
    pub range: Range,
    pub command: CodeLensCommand,
}

#[derive(Debug)]
pub enum CodeLensCommand {
    Histogram(String),
    Debug(String),
    Run(String),
    Estimate(String),
    Circuit(OperationInfo),
    Flamegraph(OperationInfo),
}

#[derive(Debug)]
pub struct OperationInfo {
    pub operation: String,
    pub total_num_qubits: u32,
}
