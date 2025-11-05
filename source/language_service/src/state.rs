// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::protocol::{DocumentStatusDiagnostic, TestCallable};

use super::compilation::Compilation;
use super::protocol::{
    DiagnosticUpdate, ErrorKind, NotebookMetadata, TestCallables, WorkspaceConfigurationUpdate,
};
use log::{debug, trace};
use miette::Diagnostic;
use qsc::line_column::Encoding;
use qsc::{LanguageFeatures, PackageType, compile, project, target::Profile};
use qsc_linter::LintOrGroupConfig;
use qsc_project::{FileSystemAsync, JSProjectHost, PackageCache, Project, ProjectType};
use rustc_hash::{FxHashMap, FxHashSet};

use std::path::Path;
use std::{cell::RefCell, fmt::Debug, mem::take, path::PathBuf, rc::Rc, sync::Arc, vec};

#[derive(Default, Debug)]
pub(super) struct CompilationState {
    /// A `CompilationUri` is an identifier for a unique compilation.
    /// It is NOT required to be a uri that represents an actual document.
    ///
    /// For single Q# documents, the `CompilationUri` is the same as the
    /// document uri.
    ///
    /// For notebooks, the `CompilationUri` is the notebook uri.
    ///
    /// The `CompilationUri` is used when compilation-level errors get reported
    /// to the client. Compilation-level errors are defined as errors without
    /// an associated source document.
    ///
    /// `PartialConfiguration` contains configuration overrides for this
    /// compilation, explicitly specified through a project manifest (not currently implemented)
    /// or notebook metadata.
    compilations: FxHashMap<CompilationUri, (Compilation, PartialConfiguration)>,
    /// All the documents that we were told about by the client.
    ///
    /// This map doesn't necessarily contain ALL the documents that
    /// make up a compilation - only the ones that are currently open.
    open_documents: FxHashMap<DocumentUri, OpenDocument>,
}

type CompilationUri = Arc<str>;
type DocumentUri = Arc<str>;

#[derive(Debug)]
struct OpenDocument {
    /// This version is the document version provided by the client.
    /// It increases strictly with each text change, though this knowledge should
    /// not be important. The version is only ever used when publishing
    /// diagnostics to help the client associate the list of diagnostics
    /// with a snapshot of the document.
    pub version: u32,
    pub compilation: CompilationUri,
    pub latest_str_content: Arc<str>,
}

#[derive(Debug, Clone)]
struct Configuration {
    pub target_profile: Profile,
    pub package_type: PackageType,
    pub language_features: LanguageFeatures,
    pub lints_config: Vec<LintOrGroupConfig>,
    /// Enables non-user-facing developer diagnostics.
    pub dev_diagnostics: bool,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            target_profile: Profile::Unrestricted,
            package_type: PackageType::Lib,
            language_features: LanguageFeatures::default(),
            lints_config: Vec::default(),
            dev_diagnostics: false,
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct PartialConfiguration {
    pub target_profile: Option<Profile>,
    pub package_type: Option<PackageType>,
    pub language_features: Option<LanguageFeatures>,
    pub lints_config: Vec<LintOrGroupConfig>,
}

pub(super) struct CompilationStateUpdater<'a> {
    /// Compilation state which is shared with readers. It can only be accessed
    /// by dynamically borrowing. Mutable references to `CompilationState` should not
    /// be held across `await` points since that can cause readers to be denied access.
    state: Rc<RefCell<CompilationState>>,
    /// Workspace-wide configuration settings. These can include compiler settings that
    /// affect error checking and other language server behavior.
    ///
    /// Some settings can be set both at the compilation scope and at the workspace scope.
    /// Compilation-scoped settings take precedence over workspace-scoped settings.
    configuration: Configuration,
    /// Documents that we have previously published diagnostics about. We need to
    /// keep track of this so we can clear diagnostics from them when documents are removed
    /// from a compilation or when a recompilation occurs.
    documents_with_diagnostics: FxHashSet<DocumentUri>,
    /// Callback which will receive diagnostics (compilation errors)
    /// whenever a (re-)compilation occurs.
    diagnostics_receiver: Box<dyn Fn(DiagnosticUpdate) + 'a>,
    /// Callback which will receive test callables whenever a (re-)compilation occurs.
    test_callable_receiver: Box<dyn Fn(TestCallables) + 'a>,
    cache: RefCell<PackageCache>,
    /// Functions to interact with the host filesystem for project system operations.
    project_host: Box<dyn JSProjectHost>,
    /// Encoding for converting between line/column and byte offsets.
    position_encoding: Encoding,
}

impl<'a> CompilationStateUpdater<'a> {
    pub fn new(
        state: Rc<RefCell<CompilationState>>,
        diagnostics_receiver: impl Fn(DiagnosticUpdate) + 'a,
        test_callable_receiver: impl Fn(TestCallables) + 'a,
        project_host: impl JSProjectHost + 'static,
        position_encoding: Encoding,
    ) -> Self {
        Self {
            state,
            configuration: Configuration::default(),
            documents_with_diagnostics: FxHashSet::default(),
            diagnostics_receiver: Box::new(diagnostics_receiver),
            test_callable_receiver: Box::new(test_callable_receiver),
            cache: RefCell::default(),
            project_host: Box::new(project_host),
            position_encoding,
        }
    }

    /// Updates the workspace configuration. If any compiler settings are updated,
    /// a recompilation may be triggered, which will result in a new set of diagnostics
    /// being published.
    pub fn update_configuration(&mut self, configuration: WorkspaceConfigurationUpdate) {
        let need_recompile = self.apply_configuration(configuration);

        // Some configuration options require a recompilation as they impact error checking
        if need_recompile {
            self.recompile_all();
        }
    }

    pub(super) async fn update_document(
        &mut self,
        uri: &str,
        version: u32,
        text: &str,
        language_id: &str,
    ) {
        let doc_uri: Arc<str> = Arc::from(uri);
        let text: Arc<str> = Arc::from(text);

        let project = self
            .load_project_from_doc_uri(&doc_uri, &text, language_id)
            .await;

        let compilation_uri = project.path.clone();

        let prev_compilation_uri = self.with_state_mut(|state| {
            state
                .open_documents
                .insert(
                    doc_uri.clone(),
                    OpenDocument {
                        version,
                        compilation: compilation_uri.clone(),
                        latest_str_content: text,
                    },
                )
                .map(|d| d.compilation)
        });

        // If a document switched compilations, we may need to remove the compilation
        // it previously belonged to.
        if let Some(prev_compilation_uri) = prev_compilation_uri
            && prev_compilation_uri != compilation_uri
        {
            self.maybe_close_project(&prev_compilation_uri);
        }

        self.insert_buffer_aware_compilation(project);

        self.publish_diagnostics_and_test_callables();
    }

    async fn load_project_from_doc_uri(
        &mut self,
        doc_uri: &Arc<str>,
        text: &Arc<str>,
        language_id: &str,
    ) -> Project {
        match self
            .load_manifest_or_openqasm_project(doc_uri, language_id)
            .await
        {
            Ok(Some(p)) => p,
            Ok(None) => Project::from_single_file(doc_uri.clone(), text.clone()),
            Err(errors) => Project {
                errors,
                ..Project::from_single_file(doc_uri.clone(), text.clone())
            },
        }
    }

    /// Attempts to resolve a manifest for the given document uri.
    /// If a manifest is found, returns the manifest uri along
    /// with the sources for the project
    async fn load_manifest(
        &self,
        doc_uri: &Arc<str>,
    ) -> Result<Option<Project>, Vec<project::Error>> {
        let dir = self.project_host.find_manifest_directory(doc_uri).await;

        self.load_manifest_from_dir(dir).await
    }

    async fn load_manifest_or_openqasm_project(
        &self,
        doc_uri: &Arc<str>,
        language_id: &str,
    ) -> Result<Option<Project>, Vec<project::Error>> {
        if is_openqasm_file(language_id) {
            return Ok(Some(
                self.project_host
                    .load_openqasm_project(Path::new(doc_uri.as_ref()), None)
                    .await,
            ));
        }
        self.load_manifest(doc_uri).await
    }

    async fn load_manifest_from_dir(
        &self,
        dir: Option<Arc<str>>,
    ) -> Result<Option<Project>, Vec<project::Error>> {
        if let Some(dir) = dir {
            let dir = PathBuf::from(dir.to_string());
            let res = self
                .project_host
                .load_project(&dir, Some(&self.cache))
                .await;
            match res {
                Ok(proj) => Ok(Some(proj)),
                Err(e) => {
                    debug!("failed to load manifest: {e:?}, defaulting to single-file mode");
                    Err(e)
                }
            }
        } else {
            trace!("Running in single file mode");
            Ok(None)
        }
    }

    /// This function takes a `LoadedProject` and creates a compilation out of them.
    /// It checks currently open documents and uses those buffers instead of any
    /// sources provided in the vector, effectively prioritizing open document contents
    /// over fs contents.
    fn insert_buffer_aware_compilation(&mut self, mut loaded_project: Project) {
        self.with_state_mut(|state| {
            // replace source with one from memory if it exists
            // this is what prioritizes open buffers over what exists on the fs for a
            // given document
            let root_sources = match loaded_project.project_type {
                ProjectType::OpenQASM(ref mut sources) => sources,
                ProjectType::QSharp(ref mut package_graph_sources) => {
                    &mut package_graph_sources.root.sources
                }
            };
            for (l_uri, source) in root_sources {
                if let Some(doc) = state.open_documents.get(l_uri) {
                    trace!("{l_uri} is open, using source from open document");
                    *source = doc.latest_str_content.clone();
                }
            }

            let (configuration, compilation_overrides) = match loaded_project.project_type {
                ProjectType::QSharp(ref package_graph_sources) => {
                    let compilation_overrides = PartialConfiguration {
                        target_profile: loaded_project.target_profile,
                        language_features: Some(package_graph_sources.root.language_features),
                        lints_config: loaded_project.lints,
                        package_type: package_graph_sources.root.package_type.map(|x| match x {
                            qsc_project::PackageType::Exe => qsc::PackageType::Exe,
                            qsc_project::PackageType::Lib => qsc::PackageType::Lib,
                        }),
                    };

                    (
                        merge_configurations(&compilation_overrides, &self.configuration),
                        compilation_overrides,
                    )
                }
                ProjectType::OpenQASM(..) => {
                    (self.configuration.clone(), PartialConfiguration::default())
                }
            };

            let compilation = match loaded_project.project_type {
                ProjectType::OpenQASM(sources) => Compilation::new_qasm(
                    configuration.package_type,
                    sources,
                    loaded_project.errors,
                    &loaded_project.name,
                ),
                ProjectType::QSharp(package_graph_sources) => Compilation::new(
                    configuration.package_type,
                    configuration.target_profile,
                    configuration.language_features,
                    &configuration.lints_config,
                    package_graph_sources,
                    loaded_project.errors,
                    &loaded_project.name,
                ),
            };

            state
                .compilations
                .insert(loaded_project.path, (compilation, compilation_overrides));
        });
    }

    pub(super) async fn close_document(&mut self, uri: &str, language_id: &str) {
        let project = self
            .load_manifest_or_openqasm_project(&uri.into(), language_id)
            .await;

        let removed_compilation = self.remove_open_document(uri);

        if !removed_compilation {
            // If the project is still open, update it so that it
            // uses the disk contents instead of the open buffer contents
            // for this document
            if let Ok(Some(project)) = project {
                self.insert_buffer_aware_compilation(project);
            }
        }

        self.publish_diagnostics_and_test_callables();
    }

    /// Removes a document from the open documents map. If the
    /// document was the last open document in a compilation,
    /// the compilation is also removed.
    fn remove_open_document(&mut self, uri: &str) -> bool {
        let existing_compilation_uri = self.with_state_mut(|state| {
            state.compilations.remove(uri);

            state
                .open_documents
                .remove(uri)
                .expect("document should exist")
                .compilation
        });
        self.maybe_close_project(&existing_compilation_uri)
    }

    fn maybe_close_project(&mut self, compilation_uri: &Arc<str>) -> bool {
        self.with_state_mut(|state| {
            // if there are no remaining open documents with the project's compilation URI
            if state
                .open_documents
                .iter()
                .all(|(_uri, doc)| doc.compilation != *compilation_uri)
            {
                trace!("closing project {:?}", compilation_uri);
                state.compilations.remove(compilation_uri);
                return true;
            }
            false
        })
    }

    pub(super) async fn update_notebook_document<'b, I>(
        &mut self,
        notebook_uri: &str,
        notebook_metadata: &NotebookMetadata,
        cells: I,
    ) where
        I: Iterator<Item = (&'b str, u32, &'b str)>, // uri, version, text - basically DidChangeTextDocumentParams in LSP
    {
        let notebook_metadata = notebook_metadata.clone();
        let configuration = self.configuration.clone();

        // Load the project from any provided project directory, ignoring errors if they occur. Those are reported by the interpreter
        // in notebook environments.
        let project = self
            .load_manifest_from_dir(notebook_metadata.project_root.clone().map(Arc::from))
            .await
            .ok()
            .flatten();

        self.with_state_mut(|state| {
            let compilation_uri: Arc<str> = notebook_uri.into();
            // First remove all previously known cells for this notebook
            state
                .open_documents
                .retain(|_, open_doc| notebook_uri != open_doc.compilation.as_ref());

            let notebook_configuration = PartialConfiguration {
                target_profile: notebook_metadata.target_profile,
                package_type: None,
                language_features: Some(notebook_metadata.language_features),
                lints_config: notebook_metadata
                    .manifest
                    .map(|manifest| manifest.lints)
                    .unwrap_or_default(),
            };
            let configuration = merge_configurations(&notebook_configuration, &configuration);

            // Compile the notebook and add each cell into the document map
            let compilation = Compilation::new_notebook(
                cells.map(|(cell_uri, version, cell_contents)| {
                    trace!("update_notebook_document: cell: {cell_uri} {version}");
                    state.open_documents.insert(
                        (*cell_uri).into(),
                        OpenDocument {
                            version,
                            compilation: compilation_uri.clone(),
                            latest_str_content: Arc::from(cell_contents),
                        },
                    );
                    (Arc::from(cell_uri), Arc::from(cell_contents))
                }),
                configuration.target_profile,
                configuration.language_features,
                &configuration.lints_config,
                project,
            );

            state.compilations.insert(
                compilation_uri.clone(),
                (compilation, notebook_configuration),
            );
        });
        self.publish_diagnostics_and_test_callables();
    }

    pub(super) fn close_notebook_document(&mut self, notebook_uri: &str) {
        self.with_state_mut(|state| {
            trace!("close_notebook_document: {notebook_uri}");

            // Cells for the notebook are kept in the open documents map.
            // First remove all the cells for the notebook from the open
            // documents map.
            state
                .open_documents
                .retain(|_, open_doc| notebook_uri != open_doc.compilation.as_ref());

            // Then remove the notebook itself from the compilations map
            state.compilations.remove(notebook_uri);
        });

        self.publish_diagnostics_and_test_callables();
    }

    // It gets really messy knowing when to clear diagnostics
    // when the document changes ownership between compilations, etc.
    // So let's do it the simplest way possible. Republish all the diagnostics every time.
    fn publish_diagnostics_and_test_callables(&mut self) {
        self.publish_test_callables();
        let last_docs_with_diags = take(&mut self.documents_with_diagnostics);
        let mut docs_with_diags = FxHashSet::default();

        self.with_state(|state| {
            for (compilation_uri, compilation) in &state.compilations {
                trace!("publishing diagnostics for {compilation_uri}");

                if compilation_uri.starts_with(qsc_project::GITHUB_SCHEME) {
                    // If the compilation URI is a GitHub virtual document URI,
                    // that's a signal that this is a source file from a GitHub package
                    // that is open in the editor.
                    // We can't discover the manifest and load the project for these files.
                    // So they end up in their own single-file compilation.
                    // Don't publish diagnostics for these, as they will contain spurious errors.
                    continue;
                }

                let mut compilation_diags_by_doc = map_errors_to_docs(
                    compilation_uri,
                    &compilation.0.compile_errors,
                    &compilation.0.project_errors,
                );

                if self.configuration.dev_diagnostics {
                    // Add the document status diagnostic for all open documents too
                    for (uri, open_document) in &state.open_documents {
                        if &open_document.compilation == compilation_uri {
                            compilation_diags_by_doc
                                .entry(uri.clone())
                                .or_default()
                                .push(ErrorKind::DocumentStatus(DocumentStatusDiagnostic {
                                    compilation_name: open_document.compilation.to_string(),
                                    document_version: open_document.version,
                                }));
                        }
                    }
                }

                for (uri, diags) in compilation_diags_by_doc {
                    if !docs_with_diags.insert(uri.clone()) {
                        // We already published diagnostics for this document for
                        // a different compilation.
                        // When the same document is included in multiple compilations,
                        // only report the errors for one of them, the goal being
                        // a less confusing user experience.
                        continue;
                    }

                    self.publish_diagnostics_for_doc(state, &uri, diags);
                }
            }

            // Clear diagnostics from any documents that previously had diagnostics
            for uri in last_docs_with_diags.difference(&docs_with_diags) {
                self.publish_diagnostics_for_doc(state, uri, vec![]);
            }
        });

        self.documents_with_diagnostics = docs_with_diags;
    }

    fn publish_diagnostics_for_doc(
        &self,
        state: &CompilationState,
        uri: &str,
        errors: Vec<ErrorKind>,
    ) {
        let version = state.open_documents.get(uri).map(|d| d.version);
        trace!(
            "publishing diagnostics for {uri} {version:?}): {} errors",
            errors.len()
        );
        (self.diagnostics_receiver)(DiagnosticUpdate {
            uri: uri.into(),
            version,
            errors,
        });
    }

    fn apply_configuration(&mut self, configuration: WorkspaceConfigurationUpdate) -> bool {
        let mut need_recompile = false;

        if let Some(package_type) = configuration.package_type {
            need_recompile |= self.configuration.package_type != package_type;
            self.configuration.package_type = package_type;
        }

        if let Some(target_profile) = configuration.target_profile {
            need_recompile |= self.configuration.target_profile != target_profile;
            self.configuration.target_profile = target_profile;
        }

        if let Some(language_features) = configuration.language_features {
            need_recompile |= self.configuration.language_features != language_features;
            self.configuration.language_features = language_features;
        }

        if let Some(lints_config) = configuration.lints_config {
            need_recompile |= self.configuration.lints_config != lints_config;
            self.configuration.lints_config = lints_config;
        }

        if let Some(dev_diagnostics) = configuration.dev_diagnostics {
            need_recompile |= self.configuration.dev_diagnostics != dev_diagnostics;
            self.configuration.dev_diagnostics = dev_diagnostics;
        }

        // Possible optimization: some projects will have overrides for these configurations,
        // so workspace updates won't impact them. We could exclude those projects
        // from recompilation, but we don't right now.
        trace!("need_recompile after configuration update: {need_recompile}");
        need_recompile
    }

    /// Recompiles the currently known documents with
    /// the current configuration. Publishes updated
    /// diagnostics for all documents.
    fn recompile_all(&mut self) {
        self.with_state_mut(|state| {
            for (compilation, package_specific_configuration) in state.compilations.values_mut() {
                let configuration =
                    merge_configurations(package_specific_configuration, &self.configuration);
                let lints_config = package_specific_configuration.lints_config.clone();
                compilation.recompile(
                    configuration.package_type,
                    configuration.target_profile,
                    configuration.language_features,
                    &lints_config,
                );
            }
        });

        self.publish_diagnostics_and_test_callables();
    }

    /// Borrows the compilation state immutably and invokes `f`.
    /// Warning: This function is not reentrant. For dynamic borrow safety,
    /// don't call `with_state` from within `with_state` or `with_state_mut`.
    /// Use a direct reference to the state instead.
    /// This function may also not be async since holding a borrow across
    /// `await` points will interfere with other borrowers.
    fn with_state<F, T>(&self, f: F) -> T
    where
        F: FnOnce(&CompilationState) -> T,
    {
        let state = self.state.borrow();
        f(&state)
    }

    /// Borrows the compilation state immutably and invokes `f`.
    /// Warning: This function is not reentrant.  For dynamic borrow safety,
    /// don't call `with_state_mut` from within `with_state` or `with_state_mut`.
    /// Use a direct reference to the state instead.
    /// This function may also not be async since holding a borrow across
    /// `await` points will interfere with other borrowers.
    fn with_state_mut<F, T>(&self, f: F) -> T
    where
        F: FnOnce(&mut CompilationState) -> T,
    {
        let mut state = self.state.borrow_mut();
        f(&mut state)
    }

    fn publish_test_callables(&self) {
        self.with_state(|state| {
            // get test callables from each compilation
            let callables: Vec<_> = state
                .compilations
                .iter()
                .flat_map(|(compilation_uri, (compilation, _))| {
                    compilation.test_cases.iter().map(move |(name, span)| {
                        Some(TestCallable {
                            compilation_uri: Arc::from(compilation_uri.as_ref()),
                            callable_name: Arc::from(name.as_ref()),
                            location: crate::qsc_utils::into_location(
                                self.position_encoding,
                                compilation,
                                *span,
                                compilation.user_package_id,
                            ),
                            // notebooks don't have human readable names -- we use this
                            // to filter them out in the test explorer
                            friendly_name: compilation.friendly_project_name()?,
                        })
                    })
                })
                .flatten()
                .collect();

            (self.test_callable_receiver)(TestCallables { callables });
        });
    }
}

fn is_openqasm_file(language_id: &str) -> bool {
    language_id == "openqasm"
}

impl CompilationState {
    pub(crate) fn get_compilation(&self, uri: &str) -> Option<&Compilation> {
        let compilation_uri = &self
            .open_documents
            .get(uri)
            .as_ref()
            .map(|x| x.compilation.clone())?;

        trace!("document: {uri} compilation_uri: {compilation_uri}");

        Some(&self.compilations.get(compilation_uri).unwrap_or_else(|| {
            panic!("document associated with compilation that hasn't been initialized ({compilation_uri})" ,)
        }).0)
    }
}

fn map_errors_to_docs(
    compilation_uri: &Arc<str>,
    compile_errors: &Vec<compile::Error>,
    project_errors: &Vec<project::Error>,
) -> FxHashMap<Arc<str>, Vec<ErrorKind>> {
    let mut map = FxHashMap::default();

    for err in compile_errors {
        // Use the compilation_uri as a location for span-less errors
        let doc = err
            .labels()
            .into_iter()
            .flatten()
            .next()
            .map_or(compilation_uri, |l| {
                let (source, _) = err.resolve_span(l.inner());
                &source.name
            });

        map.entry(doc.clone())
            .or_insert_with(Vec::new)
            .push(ErrorKind::from(err.clone()));
    }

    for err in project_errors {
        let doc = err
            .path()
            .map_or(compilation_uri.clone(), |path| path.to_string().into());

        map.entry(doc.clone())
            .or_insert_with(Vec::new)
            .push(ErrorKind::from(err.clone()));
    }

    map
}

/// Merges workspace configuration with any compilation-specific overrides.
fn merge_configurations(
    compilation_overrides: &PartialConfiguration,
    workspace_scope: &Configuration,
) -> Configuration {
    let mut merged_lints = workspace_scope.lints_config.clone();
    let mut override_lints = compilation_overrides.lints_config.clone();
    override_lints.retain(|override_lint| {
        for merged_lint in &mut merged_lints {
            match (merged_lint, override_lint) {
                (
                    LintOrGroupConfig::Lint(lint_config),
                    LintOrGroupConfig::Lint(lint_config_override),
                ) => {
                    if lint_config.kind == lint_config_override.kind {
                        lint_config.level = lint_config_override.level;
                        return false;
                    }
                }
                (
                    LintOrGroupConfig::Group(group_config),
                    LintOrGroupConfig::Group(group_config_override),
                ) => {
                    if group_config.lint_group == group_config_override.lint_group {
                        group_config.level = group_config_override.level;
                        return false;
                    }
                }
                _ => (),
            }
        }
        true
    });
    merged_lints.extend(override_lints);

    Configuration {
        target_profile: compilation_overrides
            .target_profile
            .unwrap_or(workspace_scope.target_profile),
        package_type: compilation_overrides
            .package_type
            .unwrap_or(workspace_scope.package_type),
        language_features: compilation_overrides
            .language_features
            .unwrap_or(workspace_scope.language_features),
        lints_config: merged_lints,
        dev_diagnostics: workspace_scope.dev_diagnostics,
    }
}
