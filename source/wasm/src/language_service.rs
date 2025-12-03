// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    diagnostic::VSDiagnostic,
    line_column::{ILocation, IPosition, IRange, Location, Position, Range},
    project_system::ProjectHost,
    serializable_type,
    test_discovery::TestDescriptor,
};
use qsc::{
    self, LanguageFeatures, PackageType, line_column::Encoding, linter::LintOrGroupConfig,
    target::Profile,
};
use qsc_project::Manifest;
use qsls::protocol::{DiagnosticUpdate, TestCallable, TestCallables};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;

#[wasm_bindgen]
pub struct LanguageService(qsls::LanguageService);

#[wasm_bindgen]
impl LanguageService {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)] // wasm-bindgen requires constructor to be explicitly defined
    pub fn new() -> Self {
        LanguageService(qsls::LanguageService::new(Encoding::Utf16))
    }

    pub fn start_background_work(
        &mut self,
        diagnostics_callback: &DiagnosticsCallback,
        test_callables_callback: &TestCallableCallback,
        host: ProjectHost,
    ) -> js_sys::Promise {
        let diagnostics_callback = diagnostics_callback
            .dyn_ref::<js_sys::Function>()
            .expect("expected a valid JS function")
            .clone();

        let diagnostics_callback = move |update: DiagnosticUpdate| {
            let diags = update
                .errors
                .iter()
                .map(|err| VSDiagnostic::from_ls_error(&update.uri, err))
                .collect::<Vec<_>>();
            let _ = diagnostics_callback
                .call3(
                    &JsValue::NULL,
                    &update.uri.into(),
                    &update.version.into(),
                    &serde_wasm_bindgen::to_value(&diags)
                        .expect("conversion to VSDiagnostic should succeed"),
                )
                .expect("callback should succeed");
        };

        let test_callables_callback = test_callables_callback
            .dyn_ref::<js_sys::Function>()
            .expect("expected a valid JS function")
            .clone();

        let test_callables_callback = move |update: TestCallables| {
            let callables = update
                .callables
                .iter()
                .map(
                    |TestCallable {
                         compilation_uri,
                         callable_name,
                         location,
                         friendly_name,
                     }|
                     -> TestDescriptor {
                        TestDescriptor {
                            compilation_uri: compilation_uri.to_string(),
                            callable_name: callable_name.to_string(),
                            location: location.clone().into(),
                            friendly_name: friendly_name.to_string(),
                        }
                    },
                )
                .collect::<Vec<_>>();

            let _ = test_callables_callback
                .call1(
                    &JsValue::NULL,
                    &serde_wasm_bindgen::to_value(&callables)
                        .expect("conversion to TestCallables should succeed"),
                )
                .expect("callback should succeed");
        };
        let mut worker =
            self.0
                .create_update_worker(diagnostics_callback, test_callables_callback, host);

        future_to_promise(async move {
            worker.run().await;
            Ok(JsValue::undefined())
        })
    }

    pub fn stop_background_work(&mut self) {
        self.0.stop_updates();
    }

    pub fn update_configuration(&mut self, config: IWorkspaceConfiguration) {
        let config: WorkspaceConfiguration = config.into();
        self.0
            .update_configuration(qsls::protocol::WorkspaceConfigurationUpdate {
                target_profile: config
                    .targetProfile
                    .map(|s| Profile::from_str(&s).expect("invalid target profile")),
                package_type: config.packageType.map(|s| match s.as_str() {
                    "lib" => PackageType::Lib,
                    "exe" => PackageType::Exe,
                    _ => panic!("invalid package type"),
                }),
                language_features: config
                    .languageFeatures
                    .map(|features| features.iter().collect::<LanguageFeatures>()),
                lints_config: config.lints,
                dev_diagnostics: config.devDiagnostics,
            });
    }

    pub fn update_document(&mut self, uri: &str, version: u32, text: &str, language_id: &str) {
        self.0.update_document(uri, version, text, language_id);
    }

    pub fn close_document(&mut self, uri: &str, language_id: &str) {
        self.0.close_document(uri, language_id);
    }

    pub fn update_notebook_document(
        &mut self,
        notebook_uri: &str,
        notebook_metadata: INotebookMetadata,
        cells: Vec<ICell>,
    ) {
        let cells: Vec<Cell> = cells.into_iter().map(std::convert::Into::into).collect();
        let notebook_metadata: NotebookMetadata = notebook_metadata.into();
        let manifest: Option<Manifest> = notebook_metadata
            .manifest
            .and_then(|manifest| serde_json::from_str(&manifest).ok());

        // If no features were passed in as an argument, use the features from the manifest.
        // this way we prefer the features from the argument over those from the manifest.
        let language_features: Vec<String> = match (notebook_metadata.languageFeatures, &manifest) {
            (Some(language_features), _) => language_features,
            (_, Some(manifest)) => manifest.language_features.clone(),
            (None, None) => vec![],
        };

        self.0.update_notebook_document(
            notebook_uri,
            qsls::protocol::NotebookMetadata {
                target_profile: notebook_metadata
                    .targetProfile
                    .map(|s| Profile::from_str(&s).expect("invalid target profile")),
                language_features: LanguageFeatures::from_iter(language_features),
                manifest,
                project_root: notebook_metadata.projectRoot,
            },
            cells
                .iter()
                .map(|s| (s.uri.as_ref(), s.version, s.code.as_ref())),
        );
    }

    pub fn close_notebook_document(&mut self, notebook_uri: &str) {
        self.0.close_notebook_document(notebook_uri);
    }

    pub fn get_code_actions(&self, uri: &str, range: IRange) -> Vec<ICodeAction> {
        let range: Range = range.into();
        let code_actions = self.0.get_code_actions(uri, range.into());
        code_actions
            .into_iter()
            .map(|code_action| Into::<CodeAction>::into(code_action).into())
            .collect()
    }

    pub fn get_completions(&self, uri: &str, position: IPosition) -> ICompletionList {
        let position: Position = position.into();
        let completion_list = self.0.get_completions(uri, position.into());
        CompletionList {
            items: completion_list
                .items
                .into_iter()
                .map(|i| CompletionItem {
                    label: i.label,
                    kind: (match i.kind {
                        qsls::protocol::CompletionItemKind::Function => "function",
                        qsls::protocol::CompletionItemKind::Interface => "interface",
                        qsls::protocol::CompletionItemKind::Keyword => "keyword",
                        qsls::protocol::CompletionItemKind::Module => "module",
                        qsls::protocol::CompletionItemKind::Property => "property",
                        qsls::protocol::CompletionItemKind::Variable => "variable",
                        qsls::protocol::CompletionItemKind::TypeParameter => "typeParameter",
                        qsls::protocol::CompletionItemKind::Field => "field",
                        qsls::protocol::CompletionItemKind::Class => "class",
                    })
                    .to_string(),
                    sortText: i.sort_text,
                    detail: i.detail,
                    additionalTextEdits: i.additional_text_edits.map(|edits| {
                        edits
                            .into_iter()
                            .map(|edit| TextEdit {
                                range: edit.range.into(),
                                newText: edit.new_text,
                            })
                            .collect()
                    }),
                })
                .collect(),
        }
        .into()
    }

    pub fn get_definition(&self, uri: &str, position: IPosition) -> Option<ILocation> {
        let position: Position = position.into();
        let definition = self.0.get_definition(uri, position.into());
        definition.map(|definition| Location::from(definition).into())
    }

    pub fn get_references(
        &self,
        uri: &str,
        position: IPosition,
        include_declaration: bool,
    ) -> Vec<ILocation> {
        let position: Position = position.into();
        let locations = self
            .0
            .get_references(uri, position.into(), include_declaration);
        locations
            .into_iter()
            .map(|loc| Location::from(loc).into())
            .collect()
    }

    pub fn get_format_changes(&self, uri: &str) -> Vec<ITextEdit> {
        let edits = self.0.get_format_changes(uri);
        edits
            .into_iter()
            .map(|edit| {
                TextEdit {
                    range: edit.range.into(),
                    newText: edit.new_text,
                }
                .into()
            })
            .collect()
    }

    pub fn get_hover(&self, uri: &str, position: IPosition) -> Option<IHover> {
        let position: Position = position.into();
        let hover = self.0.get_hover(uri, position.into());
        hover.map(|hover| {
            Hover {
                contents: hover.contents,
                span: hover.span.into(),
            }
            .into()
        })
    }

    pub fn get_signature_help(&self, uri: &str, position: IPosition) -> Option<ISignatureHelp> {
        let position: Position = position.into();
        let sig_help = self.0.get_signature_help(uri, position.into());
        sig_help.map(|sig_help| {
            SignatureHelp {
                signatures: sig_help
                    .signatures
                    .into_iter()
                    .map(|sig| SignatureInformation {
                        label: sig.label,
                        documentation: sig.documentation.unwrap_or_default(),
                        parameters: sig
                            .parameters
                            .into_iter()
                            .map(|param| ParameterInformation {
                                label: param.label,
                                documentation: param.documentation.unwrap_or_default(),
                            })
                            .collect(),
                    })
                    .collect(),
                activeSignature: sig_help.active_signature,
                activeParameter: sig_help.active_parameter,
            }
            .into()
        })
    }

    pub fn get_rename(&self, uri: &str, position: IPosition, new_name: &str) -> IWorkspaceEdit {
        let position: Position = position.into();
        let locations = self.0.get_rename(uri, position.into());

        let mut renames: FxHashMap<String, Vec<TextEdit>> = FxHashMap::default();
        for l in locations {
            renames
                .entry(l.source.to_string())
                .or_default()
                .push(TextEdit {
                    range: l.range.into(),
                    newText: new_name.to_string(),
                });
        }

        let workspace_edit = WorkspaceEdit {
            changes: renames.into_iter().collect(),
        };

        workspace_edit.into()
    }

    pub fn prepare_rename(&self, uri: &str, position: IPosition) -> Option<ITextEdit> {
        let position: Position = position.into();
        let result = self.0.prepare_rename(uri, position.into());
        result.map(|r| {
            TextEdit {
                range: r.0.into(),
                newText: r.1,
            }
            .into()
        })
    }

    pub fn get_code_lenses(&self, uri: &str) -> Vec<ICodeLens> {
        let code_lenses = self.0.get_code_lenses(uri);
        code_lenses
            .into_iter()
            .map(|lens| {
                let range = lens.range.into();
                let (command, args) = match lens.command {
                    qsls::protocol::CodeLensCommand::Histogram(expr) => {
                        ("histogram", Some(CodeLensArg::Expr((None, expr))))
                    }
                    qsls::protocol::CodeLensCommand::Debug(expr) => {
                        ("debug", Some(CodeLensArg::Expr((None, expr))))
                    }
                    qsls::protocol::CodeLensCommand::Run(expr) => {
                        ("run", Some(CodeLensArg::Expr((None, expr))))
                    }
                    qsls::protocol::CodeLensCommand::Estimate(expr) => {
                        ("estimate", Some(CodeLensArg::Expr((None, expr))))
                    }
                    qsls::protocol::CodeLensCommand::Circuit(op_info) => (
                        "circuit",
                        Some(CodeLensArg::Operation((
                            None,
                            OperationInfo {
                                operation: op_info.operation,
                                total_num_qubits: op_info.total_num_qubits,
                            },
                        ))),
                    ),
                    qsls::protocol::CodeLensCommand::Flamegraph(op_info) => (
                        "flamegraph",
                        Some(CodeLensArg::Operation((
                            None,
                            OperationInfo {
                                operation: op_info.operation,
                                total_num_qubits: op_info.total_num_qubits,
                            },
                        ))),
                    ),
                };
                CodeLens {
                    range,
                    command: command.to_string(),
                    args,
                }
                .into()
            })
            .collect()
    }
}

serializable_type! {
    WorkspaceConfiguration,
    {
        pub targetProfile: Option<String>,
        pub packageType: Option<String>,
        pub languageFeatures: Option<Vec<String>>,
        pub lints: Option<Vec<LintOrGroupConfig>>,
        pub devDiagnostics: Option<bool>,
    },
    r#"export interface IWorkspaceConfiguration {
        targetProfile?: TargetProfile;
        packageType?: "exe" | "lib";
        languageFeatures?: LanguageFeatures[];
        lints?: ({ lint: string; level: string } | { group: string; level: string })[];
        devDiagnostics?: boolean;
    }"#,
    IWorkspaceConfiguration
}

serializable_type! {
    CodeAction,
    {
        pub title: String,
        pub edit: Option<WorkspaceEdit>,
        pub kind: Option<String>,
        pub is_preferred: Option<bool>,
    },
    r#"export interface ICodeAction {
        title: string;
        edit?: IWorkspaceEdit;
        kind?: "Empty" | "QuickFix" | "Refactor" | "RefactorExtract" | "RefactorInline" | "RefactorMove" | "RefactorRewrite" | "Source" | "SourceOrganizeImports" | "SourceFixAll" | "Notebook";
        isPreferred?: boolean;
    }"#,
    ICodeAction
}

impl From<qsls::protocol::CodeAction> for CodeAction {
    fn from(code_action: qsls::protocol::CodeAction) -> Self {
        let kind = code_action.kind.map(|kind| {
            use qsls::protocol::CodeActionKind;
            match kind {
                CodeActionKind::Empty => "Empty",
                CodeActionKind::QuickFix => "QuickFix",
                CodeActionKind::Refactor => "Refactor",
                CodeActionKind::RefactorExtract => "RefactorExtract",
                CodeActionKind::RefactorInline => "RefactorInline",
                CodeActionKind::RefactorMove => "RefactorMove",
                CodeActionKind::RefactorRewrite => "RefactorRewrite",
                CodeActionKind::Source => "Source",
                CodeActionKind::SourceOrganizeImports => "SourceOrganizeImports",
                CodeActionKind::SourceFixAll => "SourceFixAll",
                CodeActionKind::Notebook => "Notebook",
            }
            .to_string()
        });

        Self {
            title: code_action.title,
            edit: code_action.edit.map(Into::into),
            kind,
            is_preferred: code_action.is_preferred,
        }
    }
}

serializable_type! {
    CompletionList,
    {
        pub items: Vec<CompletionItem>,
    },
    r#"export interface ICompletionList {
        items: ICompletionItem[]
    }"#,
    ICompletionList
}

serializable_type! {
    CompletionItem,
    {
        pub label: String,
        pub kind: String,
        pub sortText: Option<String>,
        pub detail: Option<String>,
        pub additionalTextEdits: Option<Vec<TextEdit>>,
    },
    r#"export interface ICompletionItem {
        label: string;
        kind: "function" | "interface" | "keyword" | "module" | "property" | "variable" | "typeParameter" | "field" | "class";
        sortText?: string;
        detail?: string;
        additionalTextEdits?: ITextEdit[];
    }"#
}

serializable_type! {
    TextEdit,
    {
        pub range: Range,
        pub newText: String,
    },
    r#"export interface ITextEdit {
        range: IRange;
        newText: string;
    }"#,
    ITextEdit
}

impl From<qsls::protocol::TextEdit> for TextEdit {
    fn from(text_edit: qsls::protocol::TextEdit) -> Self {
        Self {
            range: text_edit.range.into(),
            newText: text_edit.new_text,
        }
    }
}

serializable_type! {
    Hover,
    {
        pub contents: String,
        pub span: Range,
    },
    r#"export interface IHover {
        contents: string;
        span: IRange
    }"#,
    IHover
}

serializable_type! {
    SignatureHelp,
    {
        signatures: Vec<SignatureInformation>,
        activeSignature: u32,
        activeParameter: u32,
    },
    r#"export interface ISignatureHelp {
        signatures: ISignatureInformation[];
        activeSignature: number;
        activeParameter: number;
    }"#,
    ISignatureHelp
}

serializable_type! {
    SignatureInformation,
    {
        label: String,
        documentation: String,
        parameters: Vec<ParameterInformation>,
    },
    r#"export interface ISignatureInformation {
        label: string;
        documentation: string;
        parameters: IParameterInformation[];
    }"#
}

serializable_type! {
    ParameterInformation,
    {
        label: (u32,u32),
        documentation: String,
    },
    r#"export interface IParameterInformation {
        label: [number, number];
        documentation: string;
    }"#
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum CodeLensArg {
    Expr((Option<String>, String)),
    Operation((Option<String>, OperationInfo)),
}

serializable_type! {
    CodeLens,
    {
        range: Range,
        command: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        args: Option<CodeLensArg>,
    },
    r#"export type ICodeLens = {
        range: IRange;
        command: "histogram" | "estimate" | "debug" | "run" | "circuit" | "flamegraph";
        args?: any[];
    }"#,
    ICodeLens
}

serializable_type! {
    OperationInfo,
    {
        pub operation: String,
        #[serde(rename = "totalNumQubits")]
        pub total_num_qubits: u32,
    },
    r#"export interface IOperationInfo {
        operation: string;
        totalNumQubits: number;
    }"#,
    IOperationInfo
}

serializable_type! {
    WorkspaceEdit,
    {
        changes: Vec<(String, Vec<TextEdit>)>,
    },
    r#"export interface IWorkspaceEdit {
        changes: [string, ITextEdit[]][];
    }"#,
    IWorkspaceEdit
}

impl From<qsls::protocol::WorkspaceEdit> for WorkspaceEdit {
    fn from(workspace_edit: qsls::protocol::WorkspaceEdit) -> Self {
        Self {
            changes: workspace_edit
                .changes
                .into_iter()
                .map(|(uri, edits)| (uri, edits.into_iter().map(Into::into).collect()))
                .collect(),
        }
    }
}

serializable_type! {
    Cell,
    {
        pub uri: String,
        pub version: u32,
        pub code: String
    },
    r#"export interface ICell {
        uri: string;
        version: number;
        code: string;
    }"#,
    ICell
}

serializable_type! {
    NotebookMetadata,
    {
        pub targetProfile: Option<String>,
        pub languageFeatures: Option<Vec<String>>,
        pub manifest: Option<String>,
        pub projectRoot: Option<String>,
    },
    r#"export interface INotebookMetadata {
        targetProfile?: "base" | "adaptive_ri" | "adaptive_rif" | "unrestricted";
        languageFeatures?: "v2-preview-syntax"[];
        manifest?: string;
        projectRoot?: string;
    }"#,
    INotebookMetadata
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(
        typescript_type = "(uri: string, version: number | undefined, diagnostics: VSDiagnostic[]) => void"
    )]
    pub type DiagnosticsCallback;
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "(callables: ITestDescriptor[]) => void")]
    pub type TestCallableCallback;
}
