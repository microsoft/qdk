// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::compile::{self, compile};
use miette::Diagnostic;

use qsc_ast::ast;
use qsc_data_structures::{
    error::WithSource, language_features::LanguageFeatures, source::SourceMap,
    target::TargetCapabilityFlags,
};

use qsc_frontend::{
    compile::{Dependencies, OpenPackageStore, PackageStore},
    incremental::Increment,
};
use qsc_hir::hir::PackageId;
use qsc_passes::{PackageType, PassContext};

/// An incremental Q# compiler.
pub struct Compiler {
    /// A package store that contains the current, mutable, `CompileUnit`
    /// as well as all its immutable dependencies.
    store: OpenPackageStore,
    /// The ID of the source package. The source package
    /// is made up of the initial sources passed in when creating the compiler.
    source_package_id: PackageId,
    /// Context for passes that is reused across incremental compilations.
    passes: PassContext,
    /// The frontend incremental compiler.
    frontend: qsc_frontend::incremental::Compiler,
}

/// An incremental compiler error.
pub type Errors = Vec<compile::Error>;

impl Compiler {
    /// Creates a new incremental compiler, compiling the passed in sources.
    /// # Errors
    /// If compiling the sources fails, compiler errors are returned.
    pub fn new(
        sources: SourceMap,
        package_type: PackageType,
        capabilities: TargetCapabilityFlags,
        language_features: LanguageFeatures,
        mut store: PackageStore,
        dependencies: &Dependencies,
    ) -> Result<Self, Errors> {
        let (mut unit, errors) = compile(
            &store,
            dependencies,
            sources,
            package_type,
            capabilities,
            language_features,
        );
        if !errors.is_empty() {
            return Err(errors);
        }

        // make the user code fully public, so increments on top of this can access them
        unit.expose();

        let mut dependencies = dependencies.iter().map(Clone::clone).collect::<Vec<_>>();

        let source_package_id = store.insert(unit);
        dependencies.push((source_package_id, None));

        let frontend = qsc_frontend::incremental::Compiler::new(
            &store,
            &dependencies[..],
            capabilities,
            language_features,
        );
        let store = store.open();

        Ok(Self {
            store,
            source_package_id,
            frontend,
            passes: PassContext::default(),
        })
    }

    pub fn with_package_store(
        store: PackageStore,
        source_package_id: PackageId,
        capabilities: TargetCapabilityFlags,
        language_features: LanguageFeatures,
        dependencies: &Dependencies,
    ) -> Result<Self, Errors> {
        let mut dependencies = dependencies.iter().map(Clone::clone).collect::<Vec<_>>();

        dependencies.push((source_package_id, None));

        let frontend = qsc_frontend::incremental::Compiler::new(
            &store,
            &dependencies[..],
            capabilities,
            language_features,
        );
        let store = store.open();

        Ok(Self {
            store,
            source_package_id,
            frontend,
            passes: PassContext::default(),
        })
    }

    /// Compiles Q# fragments. Fragments are Q# code that can contain
    /// top-level statements as well as namespaces. A notebook cell
    /// or an interpreter entry is an example of fragments.
    ///
    /// This method returns the AST and HIR packages that were created as a result of
    /// the compilation, however it does *not* update the current compilation.
    ///
    /// The caller can use the returned packages to perform passes,
    /// get information about the newly added items, or do other modifications.
    /// It is then the caller's responsibility to merge
    /// these packages into the current `CompileUnit` using the `update()` method.
    pub fn compile_fragments_fail_fast(
        &mut self,
        source_name: &str,
        source_contents: &str,
    ) -> Result<Increment, Errors> {
        self.compile_fragments(source_name, source_contents, fail_on_error)
    }

    /// Compiles Q# ast fragments. Fragments are Q# code that can contain
    /// top-level statements as well as namespaces. A notebook cell
    /// or an interpreter entry is an example of fragments.
    ///
    /// This method returns the AST and HIR packages that were created as a result of
    /// the compilation, however it does *not* update the current compilation.
    ///
    /// The caller can use the returned packages to perform passes,
    /// get information about the newly added items, or do other modifications.
    /// It is then the caller's responsibility to merge
    /// these packages into the current `CompileUnit` using the `update()` method.
    pub fn compile_ast_fragments_fail_fast(
        &mut self,
        source_name: &str,
        source_contents: &str,
        package: ast::Package,
    ) -> Result<Increment, Errors> {
        self.compile_ast_fragments(source_name, source_contents, package, fail_on_error)
    }

    /// Compiles Q# fragments. See [`compile_fragments_fail_fast`] for more details.
    ///
    /// This method calls an accumulator function with any errors returned
    /// from each of the stages (parsing, lowering).
    /// If the accumulator succeeds, compilation continues.
    /// If the accumulator returns an error, compilation stops and the
    /// error is returned to the caller.
    pub fn compile_fragments<F>(
        &mut self,
        source_name: &str,
        source_contents: &str,
        mut accumulate_errors: F,
    ) -> Result<Increment, Errors>
    where
        F: FnMut(Errors) -> Result<(), Errors>,
    {
        let (core, unit) = self.store.get_open_mut();

        let mut errors = false;
        let mut increment =
            self.frontend
                .compile_fragments(unit, source_name, source_contents, |e| {
                    errors = errors || !e.is_empty();
                    accumulate_errors(into_errors(e))
                })?;

        // Even if we don't fail fast, skip passes if there were compilation errors.
        if !errors {
            let pass_errors = self.passes.run_default_passes(
                &mut increment.hir,
                &mut unit.assigner,
                core,
                PackageType::Lib,
            );

            accumulate_errors(into_errors_with_source(pass_errors, &unit.sources))?;
        }

        Ok(increment)
    }

    /// Compiles Q# ast fragments. See [`compile_ast_fragments_fail_fast`] for more details.
    ///
    /// This method calls an accumulator function with any errors returned
    /// from each of the stages (parsing, lowering).
    /// If the accumulator succeeds, compilation continues.
    /// If the accumulator returns an error, compilation stops and the
    /// error is returned to the caller.
    pub fn compile_ast_fragments<F>(
        &mut self,
        source_name: &str,
        source_contents: &str,
        package: ast::Package,
        mut accumulate_errors: F,
    ) -> Result<Increment, Errors>
    where
        F: FnMut(Errors) -> Result<(), Errors>,
    {
        let (core, unit) = self.store.get_open_mut();

        let mut errors = false;
        let mut increment = self.frontend.compile_ast_fragments(
            unit,
            source_name,
            source_contents,
            package,
            |e| {
                errors = errors || !e.is_empty();
                accumulate_errors(into_errors(e))
            },
        )?;

        // Even if we don't fail fast, skip passes if there were compilation errors.
        if !errors {
            let pass_errors = self.passes.run_default_passes(
                &mut increment.hir,
                &mut unit.assigner,
                core,
                PackageType::Lib,
            );

            accumulate_errors(into_errors_with_source(pass_errors, &unit.sources))?;
        }

        Ok(increment)
    }

    /// Compiles an entry expression.
    ///
    /// This method returns the AST and HIR packages that were created as a result of
    /// the compilation, however it does *not* update the current compilation.
    ///
    /// The caller can use the returned packages to perform passes,
    /// get information about the newly added items, or do other modifications.
    /// It is then the caller's responsibility to merge
    /// these packages into the current `CompileUnit` using the `update()` method.
    pub fn compile_entry_expr(&mut self, expr: &str) -> Result<Increment, Errors> {
        let (core, unit) = self.store.get_open_mut();

        let mut increment = self
            .frontend
            .compile_entry_expr(unit, expr)
            .map_err(into_errors)?;

        let pass_errors = self.passes.run_default_passes(
            &mut increment.hir,
            &mut unit.assigner,
            core,
            PackageType::Lib,
        );

        if !pass_errors.is_empty() {
            return Err(into_errors_with_source(pass_errors, &unit.sources));
        }

        Ok(increment)
    }

    /// Updates the current compilation with the AST and HIR packages,
    /// and any associated context, returned from a previous incremental compilation.
    /// Entry expressions are ignored.
    pub fn update(&mut self, new: Increment) {
        let (_, unit) = self.store.get_open_mut();

        self.frontend.update(unit, new);
    }

    /// Returns a reference to the underlying package store.
    #[must_use]
    pub fn package_store(&self) -> &PackageStore {
        self.store.package_store()
    }

    /// Returns ID of the current `CompileUnit`.
    #[must_use]
    pub fn package_id(&self) -> PackageId {
        self.store.open_package_id()
    }

    /// Returns the ID of the source package created from the sources
    /// passed in during initial creation.
    #[must_use]
    pub fn source_package_id(&self) -> PackageId {
        self.source_package_id
    }

    /// Consumes the incremental compiler and returns an immutable package store.
    /// This method can be used to finalize the compilation.
    #[must_use]
    pub fn into_package_store(self) -> (PackageStore, PackageId) {
        self.store.into_package_store()
    }
}

fn into_errors_with_source<T>(errors: Vec<T>, sources: &SourceMap) -> Errors
where
    compile::ErrorKind: From<T>,
{
    errors
        .into_iter()
        .map(|e| WithSource::from_map(sources, e.into()))
        .collect()
}

fn into_errors<T>(errors: Vec<WithSource<T>>) -> Errors
where
    compile::ErrorKind: From<T>,
    T: Diagnostic + Send + Sync,
{
    errors
        .into_iter()
        .map(qsc_data_structures::error::WithSource::into_with_source)
        .collect()
}

fn fail_on_error(errors: Errors) -> Result<(), Errors> {
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(())
}
