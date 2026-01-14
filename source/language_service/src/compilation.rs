// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use log::trace;
use qsc::{
    CompileUnit, LanguageFeatures, PackageStore, PackageType, PassContext, SourceMap, Span, ast,
    compile,
    display::Lookup,
    error::WithSource,
    hir::{self, PackageId, Res},
    incremental::Compiler,
    line_column::{Encoding, Position, Range},
    openqasm::{
        CompileRawQasmResult, CompilerConfig, OutputSemantics, ProgramType, QubitSemantics,
        compiler::compile_to_qsharp_ast_with_config,
    },
    packages::{BuildableProgram, prepare_package_store},
    project, resolve,
    target::Profile,
};
use qsc_linter::{LintLevel, LintOrGroupConfig};
use qsc_project::{PackageGraphSources, Project, ProjectType};
use std::mem::take;
use std::sync::Arc;

/// Represents an immutable compilation state that can be used
/// to implement language service features.
#[derive(Debug)]
pub(crate) struct Compilation {
    /// Package store, containing the current package and all its dependencies.
    pub package_store: PackageStore,
    /// The `PackageId` of the user package. User code
    /// is non-library code, i.e. all code except the std and core libs.
    pub user_package_id: PackageId,
    pub project_errors: Vec<project::Error>,
    pub compile_errors: Vec<compile::Error>,
    pub kind: CompilationKind,
    pub test_cases: Vec<(String, Span)>,
}

#[derive(Debug)]
pub(crate) enum CompilationKind {
    /// An open Q# project.
    /// In an `OpenProject` compilation, the user package contains
    /// one or more sources, and a target profile.
    OpenProject {
        package_graph_sources: PackageGraphSources,
        /// a human-readable name for the package (not a unique URI -- meant to be read by humans)
        friendly_name: Arc<str>,
    },
    /// A Q# notebook. In a notebook compilation, the user package
    /// contains multiple `Source`s, with each source corresponding
    /// to a cell.
    Notebook { project: Option<Project> },
    OpenQASM {
        sources: Vec<(Arc<str>, Arc<str>)>,
        /// a human-readable name for the package (not a unique URI -- meant to be read by humans)
        friendly_name: Arc<str>,
    },
}

impl Compilation {
    /// Creates a new `Compilation` by compiling sources.
    pub(crate) fn new(
        package_type: PackageType,
        target_profile: Profile,
        language_features: LanguageFeatures,
        lints_config: &[LintOrGroupConfig],
        package_graph_sources: PackageGraphSources,
        project_errors: Vec<project::Error>,
        friendly_name: &Arc<str>,
    ) -> Self {
        let mut buildable_program =
            prepare_package_store(target_profile.into(), package_graph_sources.clone());

        let mut compile_errors = take(&mut buildable_program.dependency_errors);

        let BuildableProgram {
            store: mut package_store,
            user_code,
            user_code_dependencies,
            ..
        } = buildable_program;
        let user_code = SourceMap::new(user_code.sources, None);

        let (unit, mut this_errors) = compile::compile(
            &package_store,
            &user_code_dependencies,
            user_code,
            package_type,
            target_profile.into(),
            language_features,
        );

        compile_errors.append(&mut this_errors);

        let package_id = package_store.insert(unit);
        let unit = package_store
            .get(package_id)
            .expect("expected to find user package");

        run_fir_passes(
            &mut compile_errors,
            target_profile,
            &package_store,
            package_id,
            unit,
        );

        run_linter_passes(&mut compile_errors, &package_store, unit, lints_config);

        let test_cases = unit.package.get_test_callables();

        Self {
            package_store,
            user_package_id: package_id,
            kind: CompilationKind::OpenProject {
                package_graph_sources,
                friendly_name: friendly_name.clone(),
            },
            compile_errors,
            project_errors,
            test_cases,
        }
    }

    /// Creates a new `Compilation` by compiling sources from notebook cells.
    pub(crate) fn new_notebook<I>(
        cells: I,
        target_profile: Profile,
        language_features: LanguageFeatures,
        lints_config: &[LintOrGroupConfig],
        project: Option<Project>,
    ) -> Self
    where
        I: Iterator<Item = (Arc<str>, Arc<str>)>,
    {
        trace!("compiling dependencies");

        let (sources, dependencies, store, mut errors) = match &project {
            Some(p) if p.errors.is_empty() => {
                trace!("using buildable program from project");
                let ProjectType::QSharp(sources) = &p.project_type else {
                    unreachable!("Project type should be Q#")
                };
                let buildable_program =
                    prepare_package_store(target_profile.into(), sources.clone());

                (
                    SourceMap::new(buildable_program.user_code.sources, None),
                    buildable_program.user_code_dependencies,
                    buildable_program.store,
                    buildable_program.dependency_errors,
                )
            }
            _ => {
                // If no project is specified, or if the project has errors, compile stdlib only.
                // Any project errors will be handled below.
                trace!("compiling stdlib only");
                let (std_id, store) =
                    qsc::compile::package_store_with_stdlib(target_profile.into());
                (
                    SourceMap::default(),
                    vec![(std_id, None)],
                    store,
                    Vec::new(),
                )
            }
        };

        trace!("compiling notebook");
        let mut compiler = match Compiler::new(
            sources,
            PackageType::Lib,
            target_profile.into(),
            language_features,
            store,
            &dependencies,
        ) {
            Ok(compiler) => compiler,
            Err(user_errors) => {
                errors.extend(user_errors);
                // Because there were errors in the user code project, we need to create a new compiler with no sources
                // to do a best effort compilation of the cells.
                trace!("falling back stdlib only only after user code project errors");
                let (std_id, store) =
                    qsc::compile::package_store_with_stdlib(target_profile.into());

                Compiler::new(
                    SourceMap::default(),
                    PackageType::Lib,
                    target_profile.into(),
                    language_features,
                    store,
                    &[(std_id, None)],
                )
                .expect("standard library should compile without errors")
            }
        };

        for (name, contents) in cells {
            trace!("compiling cell {name}");
            let increment = compiler
                .compile_fragments(&name, &contents, |cell_errors| {
                    errors.extend(cell_errors);
                    Ok(()) // accumulate errors without failing
                })
                .expect("compile_fragments_acc_errors should not fail");

            compiler.update(increment);
        }

        let (package_store, package_id) = compiler.into_package_store();
        let unit = package_store
            .get(package_id)
            .expect("expected to find user package");

        run_fir_passes(
            &mut errors,
            target_profile,
            &package_store,
            package_id,
            unit,
        );

        run_linter_passes(&mut errors, &package_store, unit, lints_config);

        let test_cases = unit.package.get_test_callables();

        Self {
            package_store,
            user_package_id: package_id,
            compile_errors: errors,
            project_errors: project.as_ref().map_or_else(Vec::new, |p| p.errors.clone()),
            kind: CompilationKind::Notebook { project },
            test_cases,
        }
    }

    pub(crate) fn new_qasm(
        package_type: PackageType,
        sources: Vec<(Arc<str>, Arc<str>)>,
        project_errors: Vec<project::Error>,
        friendly_name: &Arc<str>,
    ) -> Self {
        let config = CompilerConfig::new(
            QubitSemantics::Qiskit,
            OutputSemantics::OpenQasm,
            ProgramType::File,
            Some("program".into()),
            None,
        );
        let res = qsc::openqasm::semantic::parse_sources(&sources);
        let unit = compile_to_qsharp_ast_with_config(res, config);
        let target_profile = unit.profile();
        let CompileRawQasmResult(store, source_package_id, _, _sig, mut compile_errors) =
            qsc::openqasm::compile_openqasm(unit, package_type);

        let compile_unit = store
            .get(source_package_id)
            .expect("expected to find user package");

        run_fir_passes(
            &mut compile_errors,
            target_profile,
            &store,
            source_package_id,
            compile_unit,
        );

        Self {
            package_store: store,
            user_package_id: source_package_id,
            kind: CompilationKind::OpenQASM {
                sources,
                friendly_name: friendly_name.clone(),
            },
            compile_errors,
            project_errors,
            test_cases: vec![],
        }
    }

    /// Returns a human-readable compilation name if one exists.
    /// Notebooks don't have human-readable compilation names.
    pub fn friendly_project_name(&self) -> Option<Arc<str>> {
        match &self.kind {
            CompilationKind::OpenProject { friendly_name, .. }
            | CompilationKind::OpenQASM { friendly_name, .. } => Some(friendly_name.clone()),
            CompilationKind::Notebook { .. } => None,
        }
    }

    /// Gets the `CompileUnit` associated with user (non-library) code.
    pub fn user_unit(&self) -> &CompileUnit {
        self.package_store
            .get(self.user_package_id)
            .expect("expected to find user package")
    }

    pub(crate) fn source_range_to_package_span(
        &self,
        source_name: &str,
        source_range: Range,
        position_encoding: Encoding,
    ) -> Span {
        let sources = &self.user_unit().sources;
        let lo = source_position_to_package_offset(
            sources,
            source_name,
            source_range.start,
            position_encoding,
        );
        let hi = source_position_to_package_offset(
            sources,
            source_name,
            source_range.end,
            position_encoding,
        );
        Span { lo, hi }
    }

    /// Gets the span of the whole source file.
    pub(crate) fn package_span_of_source(&self, source_name: &str) -> Span {
        let unit = self.user_unit();

        let source = unit
            .sources
            .find_by_name(source_name)
            .expect("source should exist in the user source map");

        let len = u32::try_from(source.contents.len()).expect("source length should fit into u32");

        Span {
            lo: source.offset,
            hi: source.offset + len,
        }
    }

    /// Regenerates the compilation with the same sources but the passed in workspace configuration options.
    pub fn recompile(
        &mut self,
        package_type: PackageType,
        target_profile: Profile,
        language_features: LanguageFeatures,
        lints_config: &[LintOrGroupConfig],
    ) {
        let new = match self.kind {
            CompilationKind::OpenProject {
                ref package_graph_sources,
                ref friendly_name,
            } => Self::new(
                package_type,
                target_profile,
                language_features,
                lints_config,
                package_graph_sources.clone(),
                Vec::new(), // project errors will stay the same
                friendly_name,
            ),
            CompilationKind::Notebook { ref project } => {
                let sources = self
                    .user_unit()
                    .sources
                    .iter()
                    .map(|source| (source.name.clone(), source.contents.clone()));

                Self::new_notebook(
                    sources,
                    target_profile,
                    language_features,
                    lints_config,
                    project.clone(),
                )
            }
            CompilationKind::OpenQASM {
                ref sources,
                ref friendly_name,
            } => Self::new_qasm(
                package_type,
                sources.clone(),
                Vec::new(), // project errors will stay the same
                friendly_name,
            ),
        };

        self.package_store = new.package_store;
        self.user_package_id = new.user_package_id;
        self.test_cases = new.test_cases;
        self.compile_errors = new.compile_errors;
    }
}

/// Runs the passes required for code generation
/// appending any errors to the `errors` vector.
/// This function only runs passes if there are no compile
/// errors in the package and if the target profile is not `Base`
/// or `Unrestricted`.
fn run_fir_passes(
    errors: &mut Vec<WithSource<compile::ErrorKind>>,
    target_profile: Profile,
    package_store: &PackageStore,
    package_id: PackageId,
    unit: &CompileUnit,
) {
    if !errors.is_empty() {
        // can't run passes on a package with errors
        return;
    }

    if target_profile == Profile::Unrestricted {
        // no point in running passes on unrestricted profile
        return;
    }

    let (fir_store, fir_package_id) = qsc::lower_hir_to_fir(package_store, package_id);
    let caps_results =
        PassContext::run_fir_passes_on_fir(&fir_store, fir_package_id, target_profile.into());
    if let Err(caps_errors) = caps_results {
        for err in caps_errors {
            let err = WithSource::from_map(&unit.sources, compile::ErrorKind::Pass(err));
            errors.push(err);
        }
    }
}

/// Compute new lints and append them to the errors Vec.
/// Lints are only computed if the errors vector is empty. For performance
/// reasons we don't want to waste time running lints every few keystrokes,
/// if the user is in the middle of typing a statement, for example.
fn run_linter_passes(
    errors: &mut Vec<WithSource<compile::ErrorKind>>,
    package_store: &PackageStore,
    unit: &CompileUnit,
    config: &[LintOrGroupConfig],
) {
    if errors.is_empty() {
        let lints = qsc::linter::run_lints(package_store, unit, Some(config));
        let lints = lints
            .into_iter()
            .filter(|lint| !matches!(lint.level, LintLevel::Allow))
            .map(|lint| WithSource::from_map(&unit.sources, qsc::compile::ErrorKind::Lint(lint)));
        errors.extend(lints);
    }
}

impl Lookup for Compilation {
    /// Looks up the type of a node in user code
    fn get_ty(&self, id: ast::NodeId) -> Option<&hir::ty::Ty> {
        self.user_unit().ast.tys.terms.get(id)
    }

    /// Looks up the resolution of a node in user code
    fn get_res(&self, id: ast::NodeId) -> Option<&resolve::Res> {
        self.user_unit().ast.names.get(id)
    }

    /// Returns the hir `Item` node referred to by `item_id`,
    /// along with the `Package` and `PackageId` for the package
    /// that it was found in.
    fn resolve_item_relative_to_user_package(
        &self,
        item_id: &hir::ItemId,
    ) -> (&hir::Item, &hir::Package, hir::ItemId) {
        self.resolve_item(item_id)
    }

    /// Returns the hir `Item` node referred to by `res`.
    /// `Res`s can resolve to external packages, and the references
    /// are relative, so here we also need the
    /// local `PackageId` that the `res` itself came from.
    fn resolve_item_res(&self, res: &hir::Res) -> (&hir::Item, hir::ItemId) {
        match res {
            hir::Res::Item(item_id) => {
                let (item, _, resolved_item_id) = self.resolve_item(item_id);
                (item, resolved_item_id)
            }
            _ => panic!("expected to find item"),
        }
    }

    /// Returns the hir `Item` node referred to by `item_id`.
    /// `ItemId`s can refer to external packages, and the references
    /// are relative, so here we also need the local `PackageId`
    /// that the `ItemId` originates from.
    fn resolve_item(&self, item_id: &hir::ItemId) -> (&hir::Item, &hir::Package, hir::ItemId) {
        // If the `ItemId` contains a package id, use that.
        // Lack of a package id means the item is in the
        // same package as the one this `ItemId` reference
        // came from. So use the local package id passed in.
        let package_id = item_id.package;
        let package = &self
            .package_store
            .get(package_id)
            .expect("package should exist in store")
            .package;

        let mut item: &hir::Item = package
            .items
            .get(item_id.item)
            .expect("item id should exist");

        // follow chain of exports, if it is an aexport
        while let hir::ItemKind::Export(
            _,
            Res::Item(hir::ItemId {
                package: id,
                item: local_item_id,
            }),
        ) = &item.kind
        {
            let package: &hir::Package = &self
                .package_store
                .get(*id)
                .expect("package should exist in store")
                .package;

            item = package
                .items
                .get(*local_item_id)
                .expect("exported item should exist");
        }
        (
            item,
            package,
            hir::ItemId {
                package: package_id,
                item: item_id.item,
            },
        )
    }
}

/// Maps a source position from the user package
/// to a package (`SourceMap`) offset.
pub(super) fn source_position_to_package_offset(
    sources: &SourceMap,
    source_name: &str,
    source_position: Position,
    position_encoding: Encoding,
) -> u32 {
    let source = sources
        .find_by_name(source_name)
        .expect("source should exist in the user source map");

    let mut offset =
        source_position.to_utf8_byte_offset(position_encoding, source.contents.as_ref());

    let len = u32::try_from(source.contents.len()).expect("source length should fit into u32");
    if offset > len {
        // This can happen if the document contents are out of sync with the client's view.
        // we don't want to accidentally return an offset into the next file -
        // remap to the end of the current file.
        trace!(
            "offset {offset} out of bounds for {}, using end offset instead",
            source.name
        );
        offset = len;
    }

    source.offset + offset
}
