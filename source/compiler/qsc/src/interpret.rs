// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod circuit_tests;
mod debug;
#[cfg(test)]
mod debugger_tests;
#[cfg(test)]
mod package_tests;
#[cfg(test)]
mod tests;

use std::{cell::RefCell, rc::Rc};

pub use qsc_eval::{
    StepAction, StepResult,
    debug::Frame,
    noise::PauliNoise,
    output::{self, GenericReceiver},
    val::Closure,
    val::Range as ValueRange,
    val::Result,
    val::Value,
};
use qsc_hir::{global, ty};
use qsc_linter::{HirLint, Lint, LintKind, LintLevel};
use qsc_lowerer::{
    map_fir_local_item_to_hir, map_fir_package_to_hir, map_hir_local_item_to_fir,
    map_hir_package_to_fir,
};
use qsc_partial_eval::ProgramEntry;
use qsc_rca::PackageStoreComputeProperties;

use crate::{
    error::{self, WithStack},
    incremental::Compiler,
    location::Location,
};
use debug::format_call_stack;
use miette::Diagnostic;
use num_bigint::BigUint;
use num_complex::Complex;
use qsc_circuit::{
    Builder as CircuitBuilder, Circuit, Config as CircuitConfig,
    operations::entry_expr_for_qubit_operation,
};
use qsc_codegen::qir::{fir_to_qir, fir_to_qir_from_callable};
use qsc_data_structures::{
    functors::FunctorApp,
    language_features::LanguageFeatures,
    line_column::{Encoding, Range},
    span::Span,
    target::TargetCapabilityFlags,
};
use qsc_eval::{
    Env, ErrorBehavior, State, VariableInfo,
    backend::{Backend, Chain as BackendChain, SparseSim},
    output::Receiver,
    val,
};
use qsc_fir::fir::{self, ExecGraph, Global, PackageStoreLookup};
use qsc_fir::{
    fir::{Block, BlockId, Expr, ExprId, Package, PackageId, Pat, PatId, Stmt, StmtId},
    visit::{self, Visitor},
};
use qsc_frontend::{
    compile::{CompileUnit, Dependencies, PackageStore, Source, SourceMap},
    error::WithSource,
    incremental::Increment,
};
use qsc_passes::{PackageType, PassContext};
use rustc_hash::FxHashSet;
use thiserror::Error;

impl Error {
    #[must_use]
    pub fn stack_trace(&self) -> Option<&String> {
        match &self {
            Error::Eval(err) => err.stack_trace(),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Compile(#[from] crate::compile::Error),
    #[error(transparent)]
    #[diagnostic(transparent)]
    Pass(#[from] WithSource<qsc_passes::Error>),
    #[error("runtime error")]
    #[diagnostic(transparent)]
    Eval(#[from] WithStack<WithSource<qsc_eval::Error>>),
    #[error("circuit error")]
    #[diagnostic(transparent)]
    Circuit(#[from] qsc_circuit::Error),
    #[error("entry point not found")]
    #[diagnostic(code("Qsc.Interpret.NoEntryPoint"))]
    NoEntryPoint,
    #[error("unsupported runtime capabilities for code generation")]
    #[diagnostic(code("Qsc.Interpret.UnsupportedRuntimeCapabilities"))]
    UnsupportedRuntimeCapabilities,
    #[error("expression does not evaluate to an operation")]
    #[diagnostic(code("Qsc.Interpret.NotAnOperation"))]
    #[diagnostic(help("provide the name of a callable or a lambda expression"))]
    NotAnOperation,
    #[error("value is not a global callable")]
    #[diagnostic(code("Qsc.Interpret.NotACallable"))]
    NotACallable,
    #[error("partial evaluation error")]
    #[diagnostic(transparent)]
    PartialEvaluation(#[from] WithSource<qsc_partial_eval::Error>),
}

/// A Q# interpreter.
pub struct Interpreter {
    /// The incremental Q# compiler.
    compiler: Compiler,
    /// The target capabilities used for compilation.
    capabilities: TargetCapabilityFlags,
    /// The number of lines that have so far been compiled.
    /// This field is used to generate a unique label
    /// for each line evaluated with `eval_fragments`.
    lines: u32,
    // The FIR store
    fir_store: fir::PackageStore,
    /// FIR lowerer
    lowerer: qsc_lowerer::Lowerer,
    /// The execution graph for the last expression evaluated.
    expr_graph: Option<ExecGraph>,
    /// Checking if an `ItemId` corresponds to the `Std.OpenQASM.Angle.Angle` UDT
    /// is an expensive operation. So, we cache the id to avoid incurring that cost.
    angle_ty_cache: RefCell<Option<crate::hir::ItemId>>,
    /// Checking if an `ItemId` corresponds to the `Std.Math.Complex` UDT
    /// is an expensive operation. So, we cache the id to avoid incurring that cost.
    complex_ty_cache: RefCell<Option<crate::hir::ItemId>>,
    /// The ID of the current package.
    /// This ID is valid both for the FIR store and the `PackageStore`.
    package: PackageId,
    /// The ID of the source package. The source package
    /// is made up of the initial sources passed in when creating the interpreter.
    /// This ID is valid both for the FIR store and the `PackageStore`.
    source_package: PackageId,
    /// The default simulator backend.
    sim: BackendChain<SparseSim, CircuitBuilder>,
    /// The quantum seed, if any. This is cached here so that it can be used in calls to
    /// `run_internal` which use a passed instance of the simulator instead of the one above.
    quantum_seed: Option<u64>,
    /// The classical seed, if any. This needs to be passed to the evaluator for use in intrinsic
    /// calls that produce classical random numbers.
    classical_seed: Option<u64>,
    /// The evaluator environment.
    env: Env,
}

pub type InterpretResult = std::result::Result<Value, Vec<Error>>;

/// Indicates whether an UDT is an `OpenQASM` `Angle` or a `Complex` number.
/// This information is needed in the Python interop layer to give special
/// treatment to the instances of these UDTs.
pub enum UdtKind {
    /// `Std.OpenQASM.Angle.Angle`
    Angle,
    /// `Std.Math.Complex`
    Complex,
    /// A normal UDT, see the other variants for the special cases.
    Udt,
}

/// An item tagged with its name and the namespace it was defined in.
pub struct TaggedItem {
    pub item_id: qsc_hir::hir::ItemId,
    pub name: Rc<str>,
    pub namespace: Vec<Rc<str>>,
}

impl Interpreter {
    /// Creates a new incremental compiler, compiling the passed in sources.
    /// # Errors
    /// If compiling the sources fails, compiler errors are returned.
    pub fn new(
        sources: SourceMap,
        package_type: PackageType,
        capabilities: TargetCapabilityFlags,
        language_features: LanguageFeatures,
        store: PackageStore,
        dependencies: &Dependencies,
    ) -> std::result::Result<Self, Vec<Error>> {
        Self::new_internal(
            false,
            sources,
            package_type,
            capabilities,
            language_features,
            store,
            dependencies,
        )
    }

    /// Creates a new incremental compiler with debugging stmts enabled, compiling the passed in sources.
    /// # Errors
    /// If compiling the sources fails, compiler errors are returned.
    pub fn new_with_debug(
        sources: SourceMap,
        package_type: PackageType,
        capabilities: TargetCapabilityFlags,
        language_features: LanguageFeatures,
        store: PackageStore,
        dependencies: &Dependencies,
    ) -> std::result::Result<Self, Vec<Error>> {
        Self::new_internal(
            true,
            sources,
            package_type,
            capabilities,
            language_features,
            store,
            dependencies,
        )
    }

    fn new_internal(
        dbg: bool,
        sources: SourceMap,
        package_type: PackageType,
        capabilities: TargetCapabilityFlags,
        language_features: LanguageFeatures,
        store: PackageStore,
        dependencies: &Dependencies,
    ) -> std::result::Result<Self, Vec<Error>> {
        let compiler = Compiler::new(
            sources,
            package_type,
            capabilities,
            language_features,
            store,
            dependencies,
        )
        .map_err(into_errors)?;

        let mut fir_store = fir::PackageStore::new();
        for (id, unit) in compiler.package_store() {
            let pkg = qsc_lowerer::Lowerer::new()
                .with_debug(dbg)
                .lower_package(&unit.package, &fir_store);
            fir_store.insert(map_hir_package_to_fir(id), pkg);
        }

        let source_package_id = compiler.source_package_id();
        let package_id = compiler.package_id();

        let package = map_hir_package_to_fir(package_id);
        if capabilities != TargetCapabilityFlags::all() {
            let _ = PassContext::run_fir_passes_on_fir(
                &fir_store,
                map_hir_package_to_fir(source_package_id),
                capabilities,
            )
            .map_err(|caps_errors| {
                let source_package = compiler
                    .package_store()
                    .get(source_package_id)
                    .expect("package should exist in the package store");

                caps_errors
                    .into_iter()
                    .map(|error| Error::Pass(WithSource::from_map(&source_package.sources, error)))
                    .collect::<Vec<_>>()
            })?;
        }

        Ok(Self {
            compiler,
            lines: 0,
            capabilities,
            fir_store,
            lowerer: qsc_lowerer::Lowerer::new().with_debug(dbg),
            expr_graph: None,
            angle_ty_cache: None.into(),
            complex_ty_cache: None.into(),
            env: Env::default(),
            sim: sim_circuit_backend(),
            quantum_seed: None,
            classical_seed: None,
            package,
            source_package: map_hir_package_to_fir(source_package_id),
        })
    }

    pub fn from(
        dbg: bool,
        store: PackageStore,
        source_package_id: qsc_hir::hir::PackageId,
        capabilities: TargetCapabilityFlags,
        language_features: LanguageFeatures,
        dependencies: &Dependencies,
    ) -> std::result::Result<Self, Vec<Error>> {
        let compiler = Compiler::from(
            store,
            source_package_id,
            capabilities,
            language_features,
            dependencies,
        )
        .map_err(into_errors)?;

        let mut fir_store = fir::PackageStore::new();
        for (id, unit) in compiler.package_store() {
            let mut lowerer = qsc_lowerer::Lowerer::new().with_debug(dbg);
            let pkg = lowerer.lower_package(&unit.package, &fir_store);
            fir_store.insert(map_hir_package_to_fir(id), pkg);
        }

        let source_package_id = compiler.source_package_id();
        let package_id = compiler.package_id();

        let package = map_hir_package_to_fir(package_id);
        if capabilities != TargetCapabilityFlags::all() {
            let _ = PassContext::run_fir_passes_on_fir(
                &fir_store,
                map_hir_package_to_fir(source_package_id),
                capabilities,
            )
            .map_err(|caps_errors| {
                let source_package = compiler
                    .package_store()
                    .get(source_package_id)
                    .expect("package should exist in the package store");

                caps_errors
                    .into_iter()
                    .map(|error| Error::Pass(WithSource::from_map(&source_package.sources, error)))
                    .collect::<Vec<_>>()
            })?;
        }

        Ok(Self {
            compiler,
            lines: 0,
            capabilities,
            fir_store,
            lowerer: qsc_lowerer::Lowerer::new().with_debug(dbg),
            expr_graph: None,
            angle_ty_cache: None.into(),
            complex_ty_cache: None.into(),
            env: Env::default(),
            sim: sim_circuit_backend(),
            quantum_seed: None,
            classical_seed: None,
            package,
            source_package: map_hir_package_to_fir(source_package_id),
        })
    }

    /// Given a package ID, returns all the global items in the package.
    /// Note this does not currently include re-exports.
    fn package_globals(&self, package_id: PackageId) -> Vec<(Vec<Rc<str>>, Rc<str>, Value)> {
        let mut exported_items = Vec::new();
        let package = &self
            .compiler
            .package_store()
            .get(map_fir_package_to_hir(package_id))
            .expect("package should exist in the package store")
            .package;
        for global in global::iter_package(Some(map_fir_package_to_hir(package_id)), package) {
            if let global::Kind::Callable(term) = global.kind {
                let store_item_id = fir::StoreItemId {
                    package: package_id,
                    item: fir::LocalItemId::from(usize::from(term.id.item)),
                };
                exported_items.push((
                    global.namespace,
                    global.name,
                    Value::Global(store_item_id, FunctorApp::default()),
                ));
            }
        }
        exported_items
    }

    /// Get the global callables defined in the user source passed into initialization of the interpreter as `Value` instances.
    pub fn source_globals(&self) -> Vec<(Vec<Rc<str>>, Rc<str>, Value)> {
        self.package_globals(self.source_package)
    }

    /// Get the global callables defined in the open package being interpreted as `Value` instances, which will include any items
    /// defined by calls to `eval_fragments` and the like.
    pub fn user_globals(&self) -> Vec<(Vec<Rc<str>>, Rc<str>, Value)> {
        self.package_globals(self.package)
    }

    /// Get the input and output types of a given value representing a global item.
    /// # Panics
    /// Panics if the item is not callable or a type that can be invoked as a callable.
    pub fn global_callable_ty(&self, item_id: &Value) -> Option<(ty::Ty, ty::Ty)> {
        let Value::Global(item_id, _) = item_id else {
            panic!("value is not a global callable");
        };
        let package_id = map_fir_package_to_hir(item_id.package);
        let unit = self
            .compiler
            .package_store()
            .get(package_id)
            .expect("package should exist in the package store");
        let item = unit
            .package
            .items
            .get(qsc_hir::hir::LocalItemId::from(usize::from(item_id.item)))?;
        match &item.kind {
            qsc_hir::hir::ItemKind::Callable(decl) => {
                Some((decl.input.ty.clone(), decl.output.clone()))
            }
            qsc_hir::hir::ItemKind::Ty(_, udt) => {
                // We don't handle UDTs, so we return an error type that prevents later code from processing this item.
                Some((udt.get_pure_ty(), ty::Ty::Err))
            }
            _ => panic!("item is not callable"),
        }
    }

    /// Given a package ID, returns all the types in the package.
    /// Note this does not currently include re-exports.
    fn package_types(&self, package_id: PackageId) -> Vec<TaggedItem> {
        let mut exported_items = Vec::new();
        let package = &self
            .compiler
            .package_store()
            .get(map_fir_package_to_hir(package_id))
            .expect("package should exist in the package store")
            .package;
        for global in global::iter_package(Some(map_fir_package_to_hir(package_id)), package) {
            if let global::Kind::Ty(ty) = global.kind {
                exported_items.push(TaggedItem {
                    item_id: ty.id,
                    name: global.name,
                    namespace: global.namespace,
                });
            }
        }
        exported_items
    }

    /// Get the global UDTs defined in the user source passed into initialization of the interpreter.
    pub fn source_types(&self) -> Vec<TaggedItem> {
        self.package_types(self.source_package)
    }

    /// Get the global UDTs defined in the open package being interpreted, which will include any items
    /// defined by calls to `eval_fragments` and the like.
    pub fn user_types(&self) -> Vec<TaggedItem> {
        self.package_types(self.package)
    }

    pub fn udt_ty_from_store_item_id(
        &self,
        store_item_id: crate::fir::StoreItemId,
    ) -> (&ty::Udt, UdtKind) {
        self.udt_ty_from_item_id(&crate::hir::ItemId {
            package: Some(map_fir_package_to_hir(store_item_id.package)),
            item: map_fir_local_item_to_hir(store_item_id.item),
        })
    }

    /// Get the type of a UDT given its `item_id`.
    /// # Panics
    /// Panics if the item is not a UDT.
    pub fn udt_ty_from_item_id(&self, item_id: &crate::hir::ItemId) -> (&ty::Udt, UdtKind) {
        let crate::hir::ItemId {
            package: package_id_opt,
            item: local_item_id,
        } = item_id;

        let package_id = if let Some(package_id) = package_id_opt {
            package_id
        } else {
            &self.compiler.package_id()
        };

        let unit = self
            .compiler
            .package_store()
            .get(*package_id)
            .expect("package should exist in the package store");

        let item = unit
            .package
            .items
            .get(*local_item_id)
            .expect("item should be in this package");

        let parent = item.parent.map(|parent| {
            &unit
                .package
                .items
                .get(parent)
                .expect("parent should exist")
                .kind
        });

        let qsc_hir::hir::ItemKind::Ty(_, udt) = &item.kind else {
            panic!("item is not a UDT")
        };

        let kind = if let Some(id) = &*self.angle_ty_cache.borrow()
            && id == item_id
        {
            UdtKind::Angle
        } else if let Some(id) = &*self.complex_ty_cache.borrow()
            && id == item_id
        {
            UdtKind::Complex
        } else if let Some(qsc_hir::hir::ItemKind::Namespace(namespace, _)) = parent {
            let namespace: Vec<_> = namespace.into();
            let namespace: Vec<&str> = namespace.iter().map(|ident| &**ident).collect();
            if matches!(&namespace[..], &["Std", "OpenQASM", "Angle"]) && &*udt.name == "Angle" {
                *self.angle_ty_cache.borrow_mut() = Some(*item_id);
                UdtKind::Angle
            } else if matches!(&namespace[..], &["Std", "Core"]) && &*udt.name == "Complex" {
                *self.complex_ty_cache.borrow_mut() = Some(*item_id);
                UdtKind::Complex
            } else {
                UdtKind::Udt
            }
        } else {
            UdtKind::Udt
        };

        (udt, kind)
    }

    /// Returns the [`fir::StoreItemId`] for the `Std.OpenQASM.Angle.Angle` UDT.
    ///
    /// This function intended to be used from
    /// `source/pip/src/interpreter/data_interop.rs::pyobj_to_value`
    /// to tag the angles coming from Python with the correct `StoreItemId`.
    pub fn get_angle_id(&self) -> fir::StoreItemId {
        if let Some(id) = &*self.angle_ty_cache.borrow() {
            let crate::hir::ItemId {
                package: hir_package_id_opt,
                item: hir_local_item_id,
            } = id;
            let package_id = if let Some(package_id) = hir_package_id_opt {
                package_id
            } else {
                &self.compiler.package_id()
            };
            let fir_package_id = map_hir_package_to_fir(*package_id);
            let fir_local_item_id = map_hir_local_item_to_fir(*hir_local_item_id);
            crate::fir::StoreItemId {
                package: fir_package_id,
                item: fir_local_item_id,
            }
        } else {
            // SAFETY: This function is intended to be used when receiving Python objects
            //         in the interop layer. The only way to send a Python object to Q# is
            //         as the argument of a function call. When performing type checking
            //         for this function call in the interop layer, there are two cases:
            //
            //           1. The input type is not `Std.OpenQASM.Angle.Angle` and we return
            //              an error.
            //           2. The input type is `Std.OpenQASM.Angle.Angle`. To verify that
            //              the input type is indeed `Angle`, we call `udt_ty_from_item_id`,
            //              which caches the `Angle` UDT's `LocalItemId`.
            //
            //         So, if we proceed to execute the function's body, it's guaranteed
            //         that we have already cached `Std.OpenQASM.Angle.Angle`'s `LocalItemId`.
            //         Therefore, this else-branch is unreachable.
            unreachable!("`self.angle_ty_cache` should be set by `udt_ty_from_item_id`")
        }
    }

    /// Returns the [`fir::StoreItemId`] for the `Std.Math.Complex` UDT.
    ///
    /// This function intended to be used from
    /// `source/pip/src/interpreter/data_interop.rs::pyobj_to_value`
    /// to tag the complex numbers coming from Python with the correct
    /// `StoreItemId`.
    pub fn get_complex_id(&self) -> crate::fir::StoreItemId {
        if let Some(id) = &*self.complex_ty_cache.borrow() {
            let crate::hir::ItemId {
                package: hir_package_id_opt,
                item: hir_local_item_id,
            } = id;

            let package_id = if let Some(package_id) = hir_package_id_opt {
                package_id
            } else {
                &self.compiler.package_id()
            };

            let fir_package_id = map_hir_package_to_fir(*package_id);
            let fir_local_item_id = map_hir_local_item_to_fir(*hir_local_item_id);

            crate::fir::StoreItemId {
                package: fir_package_id,
                item: fir_local_item_id,
            }
        } else {
            // SAFETY: This function is intended to be used when receiving Python objects
            //         in the interop layer. The only way to send a Python object to Q# is
            //         as the argument of a function call. When performing type checking
            //         for this function call in the interop layer, there are two cases:
            //
            //           1. The input type is not `Std.Math.Complex` and we return an error.
            //           2. The input type is `Std.Math.Complex`. To verify that the input
            //              type is indeed `Complex`, we call `udt_ty_from_item_id`, which
            //              caches the `Complex` UDT's `LocalItemId`.
            //
            //         So, if we proceed to execute the function's body, it's guaranteed
            //         that we have already cached `Std.Math.Complex`'s `LocalItemId`.
            //         Therefore, this else-branch is unreachable.
            unreachable!("`self.complex_ty_cache` should be set by `udt_ty_from_item_id`")
        }
    }

    pub fn set_quantum_seed(&mut self, seed: Option<u64>) {
        self.quantum_seed = seed;
        self.sim.set_seed(seed);
    }

    pub fn set_classical_seed(&mut self, seed: Option<u64>) {
        self.classical_seed = seed;
    }

    pub fn check_source_lints(&self) -> Vec<Lint> {
        if let Some(compile_unit) = self
            .compiler
            .package_store()
            .get(self.compiler.source_package_id())
        {
            qsc_linter::run_lints(
                self.compiler.package_store(),
                compile_unit,
                // see https://github.com/microsoft/qsharp/pull/1627 for context
                // on why we override this config
                Some(&[qsc_linter::LintOrGroupConfig::Lint(
                    qsc_linter::LintConfig {
                        kind: LintKind::Hir(HirLint::NeedlessOperation),
                        level: LintLevel::Warn,
                    },
                )]),
            )
        } else {
            Vec::new()
        }
    }

    /// Executes the entry expression until the end of execution.
    /// # Errors
    /// Returns a vector of errors if evaluating the entry point fails.
    pub fn eval_entry(&mut self, receiver: &mut impl Receiver) -> InterpretResult {
        let graph = self.get_entry_exec_graph()?;
        self.expr_graph = Some(graph.clone());
        eval(
            self.source_package,
            self.classical_seed,
            graph,
            self.compiler.package_store(),
            &self.fir_store,
            &mut Env::default(),
            &mut self.sim,
            receiver,
        )
    }

    /// Executes the entry expression until the end of execution, using the given simulator backend
    /// and a new instance of the environment.
    pub fn eval_entry_with_sim(
        &mut self,
        sim: &mut impl Backend<ResultType = impl Into<val::Result>>,
        receiver: &mut impl Receiver,
    ) -> InterpretResult {
        let graph = self.get_entry_exec_graph()?;
        self.expr_graph = Some(graph.clone());
        if self.quantum_seed.is_some() {
            sim.set_seed(self.quantum_seed);
        }
        eval(
            self.source_package,
            self.classical_seed,
            graph,
            self.compiler.package_store(),
            &self.fir_store,
            &mut Env::default(),
            sim,
            receiver,
        )
    }

    fn get_entry_exec_graph(&self) -> std::result::Result<ExecGraph, Vec<Error>> {
        let unit = self.fir_store.get(self.source_package);
        if unit.entry.is_some() {
            return Ok(unit.entry_exec_graph.clone());
        }
        Err(vec![Error::NoEntryPoint])
    }

    /// # Errors
    /// If the parsing of the fragments fails, an error is returned.
    /// If the compilation of the fragments fails, an error is returned.
    /// If there is a runtime error when interpreting the fragments, an error is returned.
    pub fn eval_fragments(
        &mut self,
        receiver: &mut impl Receiver,
        fragments: &str,
    ) -> InterpretResult {
        let label = self.next_line_label();

        let mut increment = self
            .compiler
            .compile_fragments_fail_fast(&label, fragments)
            .map_err(into_errors)?;

        // Clear the entry expression, as we are evaluating fragments and a fragment with a `@EntryPoint` attribute
        // should not change what gets executed.
        increment.clear_entry();

        self.eval_increment(receiver, increment)
    }

    /// It is assumed that if there were any parse errors on the fragments, the caller would have
    /// already handled them. This function is intended to be used in cases where the caller wants
    /// to handle the parse errors themselves.
    ///  # Errors
    /// If the compilation of the fragments fails, an error is returned.
    /// If there is a runtime error when interpreting the fragments, an error is returned.
    pub fn eval_ast_fragments(
        &mut self,
        receiver: &mut impl Receiver,
        fragments: &str,
        package: qsc_ast::ast::Package,
    ) -> InterpretResult {
        let label = self.next_line_label();

        let increment = self
            .compiler
            .compile_ast_fragments_fail_fast(&label, fragments, package)
            .map_err(into_errors)?;

        self.eval_increment(receiver, increment)
    }

    fn eval_increment(
        &mut self,
        receiver: &mut impl Receiver,
        increment: Increment,
    ) -> InterpretResult {
        let (graph, _) = self.lower(&increment)?;
        self.expr_graph = Some(graph.clone());

        // Updating the compiler state with the new AST/HIR nodes
        // is not necessary for the interpreter to function, as all
        // the state required for evaluation already exists in the
        // FIR store. It could potentially save some memory
        // *not* to do hold on to the AST/HIR, but it is done
        // here to keep the package stores consistent.
        self.compiler.update(increment);

        eval(
            self.package,
            self.classical_seed,
            graph,
            self.compiler.package_store(),
            &self.fir_store,
            &mut self.env,
            &mut self.sim,
            receiver,
        )
    }

    /// Invokes the given callable with the given arguments using the current environment, simlator, and compilation.
    pub fn invoke(
        &mut self,
        receiver: &mut impl Receiver,
        callable: Value,
        args: Value,
    ) -> InterpretResult {
        qsc_eval::invoke(
            self.package,
            self.classical_seed,
            &self.fir_store,
            &mut self.env,
            &mut self.sim,
            receiver,
            callable,
            args,
        )
        .map_err(|(error, call_stack)| {
            eval_error(
                self.compiler.package_store(),
                &self.fir_store,
                call_stack,
                error,
            )
        })
    }

    // Invokes the given callable with the given arguments using the current compilation but with a fresh
    // environment and simulator configured with the given noise, if any.
    pub fn invoke_with_noise(
        &mut self,
        receiver: &mut impl Receiver,
        callable: Value,
        args: Value,
        noise: Option<PauliNoise>,
        qubit_loss: Option<f64>,
    ) -> InterpretResult {
        let mut sim = match noise {
            Some(noise) => SparseSim::new_with_noise(&noise),
            None => SparseSim::new(),
        };
        if let Some(loss) = qubit_loss {
            sim.set_loss(loss);
        }
        self.invoke_with_sim(&mut sim, receiver, callable, args)
    }

    /// Runs the given entry expression on a new instance of the environment and simulator,
    /// but using the current compilation.
    pub fn run(
        &mut self,
        receiver: &mut impl Receiver,
        expr: Option<&str>,
        noise: Option<PauliNoise>,
        qubit_loss: Option<f64>,
    ) -> InterpretResult {
        let mut sim = match noise {
            Some(noise) => SparseSim::new_with_noise(&noise),
            None => SparseSim::new(),
        };
        if let Some(loss) = qubit_loss {
            sim.set_loss(loss);
        }
        self.run_with_sim(&mut sim, receiver, expr)
    }

    /// Gets the current quantum state of the simulator.
    pub fn get_quantum_state(&mut self) -> (Vec<(BigUint, Complex<f64>)>, usize) {
        self.sim.capture_quantum_state()
    }

    /// Get the current circuit representation of the program.
    pub fn get_circuit(&self) -> Circuit {
        self.sim.chained.snapshot()
    }

    /// Performs QIR codegen using the given entry expression on a new instance of the environment
    /// and simulator but using the current compilation.
    pub fn qirgen(&mut self, expr: &str) -> std::result::Result<String, Vec<Error>> {
        if self.capabilities == TargetCapabilityFlags::all() {
            return Err(vec![Error::UnsupportedRuntimeCapabilities]);
        }

        // Compile the expression. This operation will set the expression as
        // the entry-point in the FIR store.
        let (graph, compute_properties) = self.compile_entry_expr(expr)?;

        let Some(compute_properties) = compute_properties else {
            // This can only happen if capability analysis was not run. This would be a bug
            // and we are in a bad state and can't proceed.
            panic!("internal error: compute properties not set after lowering entry expression");
        };
        let package = self.fir_store.get(self.package);
        let entry = ProgramEntry {
            exec_graph: graph,
            expr: (
                self.package,
                package
                    .entry
                    .expect("package must have an entry expression"),
            )
                .into(),
        };
        // Generate QIR
        fir_to_qir(
            &self.fir_store,
            self.capabilities,
            Some(compute_properties),
            &entry,
        )
        .map_err(|e| {
            let hir_package_id = match e.span() {
                Some(span) => span.package,
                None => map_fir_package_to_hir(self.package),
            };
            let source_package = self
                .compiler
                .package_store()
                .get(hir_package_id)
                .expect("package should exist in the package store");
            vec![Error::PartialEvaluation(WithSource::from_map(
                &source_package.sources,
                e,
            ))]
        })
    }

    /// Performs QIR codegen using the given callable with the given arguments on a new instance of the environment
    /// and simulator but using the current compilation.
    pub fn qirgen_from_callable(
        &mut self,
        callable: &Value,
        args: Value,
    ) -> std::result::Result<String, Vec<Error>> {
        if self.capabilities == TargetCapabilityFlags::all() {
            return Err(vec![Error::UnsupportedRuntimeCapabilities]);
        }

        let Value::Global(store_item_id, _) = callable else {
            return Err(vec![Error::NotACallable]);
        };

        fir_to_qir_from_callable(
            &self.fir_store,
            self.capabilities,
            None,
            *store_item_id,
            args,
        )
        .map_err(|e| {
            let hir_package_id = match e.span() {
                Some(span) => span.package,
                None => map_fir_package_to_hir(self.package),
            };
            let source_package = self
                .compiler
                .package_store()
                .get(hir_package_id)
                .expect("package should exist in the package store");
            vec![Error::PartialEvaluation(WithSource::from_map(
                &source_package.sources,
                e,
            ))]
        })
    }

    /// Generates a circuit representation for the program.
    ///
    /// `entry` can be the current entrypoint, an entry expression, or any operation
    /// that takes qubits.
    ///
    /// An operation can be specified by its name or a lambda expression that only takes qubits.
    /// e.g. `Sample.Main` , `qs => H(qs[0])`
    ///
    /// If `simulate` is specified, the program is simulated and the resulting
    /// circuit is returned (a.k.a. trace mode). Otherwise, the circuit is generated without
    /// simulation. In this case circuit generation may fail if the program contains dynamic
    /// behavior (quantum operations that are dependent on measurement results).
    pub fn circuit(
        &mut self,
        entry: CircuitEntryPoint,
        simulate: bool,
    ) -> std::result::Result<Circuit, Vec<Error>> {
        let (entry_expr, invoke_params) = match entry {
            CircuitEntryPoint::Operation(operation_expr) => {
                let (item, functor_app) = self.eval_to_operation(&operation_expr)?;
                let expr = entry_expr_for_qubit_operation(item, functor_app, &operation_expr)
                    .map_err(|e| vec![e.into()])?;
                (Some(expr), None)
            }
            CircuitEntryPoint::EntryExpr(expr) => (Some(expr), None),
            CircuitEntryPoint::Callable(call_val, args_val) => (None, Some((call_val, args_val))),
            CircuitEntryPoint::EntryPoint => (None, None),
        };

        let circuit = if simulate {
            let mut sim = sim_circuit_backend();

            match invoke_params {
                Some((callable, args)) => {
                    let mut sink = std::io::sink();
                    let mut out = GenericReceiver::new(&mut sink);

                    self.invoke_with_sim(&mut sim, &mut out, callable, args)?;
                }
                None => self.run_with_sim_no_output(entry_expr, &mut sim)?,
            }

            sim.chained.finish()
        } else {
            let mut sim = CircuitBuilder::new(CircuitConfig {
                max_operations: CircuitConfig::DEFAULT_MAX_OPERATIONS,
            });

            match invoke_params {
                Some((callable, args)) => {
                    let mut sink = std::io::sink();
                    let mut out = GenericReceiver::new(&mut sink);

                    self.invoke_with_sim(&mut sim, &mut out, callable, args)?;
                }
                None => self.run_with_sim_no_output(entry_expr, &mut sim)?,
            }

            sim.finish()
        };

        Ok(circuit)
    }

    /// Sets the entry expression for the interpreter.
    pub fn set_entry_expr(&mut self, entry_expr: &str) -> std::result::Result<(), Vec<Error>> {
        let (graph, _) = self.compile_entry_expr(entry_expr)?;
        self.expr_graph = Some(graph);
        Ok(())
    }

    /// Runs the given entry expression on the given simulator with a new instance of the environment
    /// but using the current compilation.
    pub fn run_with_sim(
        &mut self,
        sim: &mut impl Backend<ResultType = impl Into<val::Result>>,
        receiver: &mut impl Receiver,
        expr: Option<&str>,
    ) -> InterpretResult {
        let graph = if let Some(expr) = expr {
            let (graph, _) = self.compile_entry_expr(expr)?;
            self.expr_graph = Some(graph.clone());
            graph
        } else {
            self.expr_graph.clone().ok_or(vec![Error::NoEntryPoint])?
        };

        if self.quantum_seed.is_some() {
            sim.set_seed(self.quantum_seed);
        }

        eval(
            self.package,
            self.classical_seed,
            graph,
            self.compiler.package_store(),
            &self.fir_store,
            &mut Env::default(),
            sim,
            receiver,
        )
    }

    fn run_with_sim_no_output(
        &mut self,
        entry_expr: Option<String>,
        sim: &mut impl Backend<ResultType = impl Into<val::Result>>,
    ) -> std::result::Result<(), Vec<Error>> {
        let mut sink = std::io::sink();
        let mut out = GenericReceiver::new(&mut sink);

        let (package_id, graph) = if let Some(entry_expr) = entry_expr {
            // entry expression is provided
            let (graph, _) = self.compile_entry_expr(&entry_expr)?;
            (self.package, graph)
        } else {
            // no entry expression, use the entrypoint in the package
            (self.source_package, self.get_entry_exec_graph()?)
        };

        if self.quantum_seed.is_some() {
            sim.set_seed(self.quantum_seed);
        }

        eval(
            package_id,
            self.classical_seed,
            graph,
            self.compiler.package_store(),
            &self.fir_store,
            &mut Env::default(),
            sim,
            &mut out,
        )?;

        Ok(())
    }

    /// Invokes the given callable with the given arguments on the given simulator with a new instance of the environment
    /// but using the current compilation.
    pub fn invoke_with_sim(
        &mut self,
        sim: &mut impl Backend<ResultType = impl Into<val::Result>>,
        receiver: &mut impl Receiver,
        callable: Value,
        args: Value,
    ) -> InterpretResult {
        qsc_eval::invoke(
            self.package,
            self.classical_seed,
            &self.fir_store,
            &mut Env::default(),
            sim,
            receiver,
            callable,
            args,
        )
        .map_err(|(error, call_stack)| {
            eval_error(
                self.compiler.package_store(),
                &self.fir_store,
                call_stack,
                error,
            )
        })
    }

    fn compile_entry_expr(
        &mut self,
        expr: &str,
    ) -> std::result::Result<(ExecGraph, Option<PackageStoreComputeProperties>), Vec<Error>> {
        let increment = self
            .compiler
            .compile_entry_expr(expr)
            .map_err(into_errors)?;

        // `lower` will update the entry expression in the FIR store,
        // and it will always return an empty list of statements.
        let (graph, compute_properties) = self.lower(&increment)?;

        // The AST and HIR packages in `increment` only contain an entry
        // expression and no statements. The HIR *can* contain items if the entry
        // expression defined any items.
        assert!(increment.hir.stmts.is_empty());
        assert!(increment.ast.package.nodes.is_empty());

        // Updating the compiler state with the new AST/HIR nodes
        // is not necessary for the interpreter to function, as all
        // the state required for evaluation already exists in the
        // FIR store. It could potentially save some memory
        // *not* to do hold on to the AST/HIR, but it is done
        // here to keep the package stores consistent.
        self.compiler.update(increment);

        Ok((graph, compute_properties))
    }

    fn lower(
        &mut self,
        unit_addition: &qsc_frontend::incremental::Increment,
    ) -> core::result::Result<(ExecGraph, Option<PackageStoreComputeProperties>), Vec<Error>> {
        if self.capabilities != TargetCapabilityFlags::all() {
            return self.run_fir_passes(unit_addition);
        }

        self.lower_and_update_package(unit_addition);
        Ok((self.lowerer.take_exec_graph().into(), None))
    }

    fn lower_and_update_package(&mut self, unit: &qsc_frontend::incremental::Increment) {
        {
            let fir_package = self.fir_store.get_mut(self.package);
            self.lowerer
                .lower_and_update_package(fir_package, &unit.hir);
        }
        let fir_package: &Package = self.fir_store.get(self.package);
        qsc_fir::validate::validate(fir_package, &self.fir_store);
    }

    fn run_fir_passes(
        &mut self,
        unit: &qsc_frontend::incremental::Increment,
    ) -> std::result::Result<(ExecGraph, Option<PackageStoreComputeProperties>), Vec<Error>> {
        self.lower_and_update_package(unit);

        let cap_results =
            PassContext::run_fir_passes_on_fir(&self.fir_store, self.package, self.capabilities);

        let compute_properties = cap_results.map_err(|caps_errors| {
            // if there are errors, convert them to interpreter errors
            // and revert the update to the lowerer/FIR store.
            let fir_package = self.fir_store.get_mut(self.package);
            self.lowerer.revert_last_increment(fir_package);

            let source_package = self
                .compiler
                .package_store()
                .get(map_fir_package_to_hir(self.package))
                .expect("package should exist in the package store");

            caps_errors
                .into_iter()
                .map(|error| Error::Pass(WithSource::from_map(&source_package.sources, error)))
                .collect::<Vec<_>>()
        })?;

        let graph = self.lowerer.take_exec_graph();
        Ok((graph.into(), Some(compute_properties)))
    }

    fn next_line_label(&mut self) -> String {
        let label = format!("line_{}", self.lines);
        self.lines += 1;
        label
    }

    /// Evaluate the name of an operation, or any expression that evaluates to a callable,
    /// and return the Item ID and function application for the callable.
    /// Examples: "Microsoft.Quantum.Diagnostics.DumpMachine", "(qs: Qubit[]) => H(qs[0])",
    /// "Controlled SWAP"
    fn eval_to_operation(
        &mut self,
        operation_expr: &str,
    ) -> std::result::Result<(&qsc_hir::hir::Item, FunctorApp), Vec<Error>> {
        let mut sink = std::io::sink();
        let mut out = GenericReceiver::new(&mut sink);
        let (store_item_id, functor_app) = match self.eval_fragments(&mut out, operation_expr)? {
            Value::Closure(b) => (b.id, b.functor),
            Value::Global(item_id, functor_app) => (item_id, functor_app),
            _ => return Err(vec![Error::NotAnOperation]),
        };
        let package = map_fir_package_to_hir(store_item_id.package);
        let local_item_id = crate::hir::LocalItemId::from(usize::from(store_item_id.item));
        let unit = self
            .compiler
            .package_store()
            .get(package)
            .expect("package should exist in the package store");
        let item = unit
            .package
            .items
            .get(local_item_id)
            .expect("item should exist in the package");
        Ok((item, functor_app))
    }
}

fn sim_circuit_backend() -> BackendChain<SparseSim, CircuitBuilder> {
    BackendChain::new(
        SparseSim::new(),
        CircuitBuilder::new(CircuitConfig {
            max_operations: CircuitConfig::DEFAULT_MAX_OPERATIONS,
        }),
    )
}

/// Describes the entry point for circuit generation.
pub enum CircuitEntryPoint {
    /// An operation. This must be a callable name or a lambda
    /// expression that only takes qubits as arguments.
    /// The callable name must be visible in the current package.
    Operation(String),
    /// An explicitly provided entry expression.
    EntryExpr(String),
    /// A global callable with arguments.
    Callable(Value, Value),
    /// The entry point for the current package.
    EntryPoint,
}

/// A debugger that enables step-by-step evaluation of code
/// and inspecting state in the interpreter.
pub struct Debugger {
    interpreter: Interpreter,
    /// The encoding (utf-8 or utf-16) used for character offsets
    /// in line/character positions returned by the Interpreter.
    position_encoding: Encoding,
    /// The current state of the evaluator.
    state: State,
}

impl Debugger {
    pub fn new(
        sources: SourceMap,
        capabilities: TargetCapabilityFlags,
        position_encoding: Encoding,
        language_features: LanguageFeatures,
        store: PackageStore,
        dependencies: &Dependencies,
    ) -> std::result::Result<Self, Vec<Error>> {
        let interpreter = Interpreter::new_with_debug(
            sources,
            PackageType::Exe,
            capabilities,
            language_features,
            store,
            dependencies,
        )?;
        let source_package_id = interpreter.source_package;
        let unit = interpreter.fir_store.get(source_package_id);
        let entry_exec_graph = unit.entry_exec_graph.clone();
        Ok(Self {
            interpreter,
            position_encoding,
            state: State::new(
                source_package_id,
                entry_exec_graph,
                None,
                ErrorBehavior::StopOnError,
            ),
        })
    }

    pub fn from(interpreter: Interpreter, position_encoding: Encoding) -> Self {
        let source_package_id = interpreter.source_package;
        let unit = interpreter.fir_store.get(source_package_id);
        let entry_exec_graph = unit.entry_exec_graph.clone();
        Self {
            interpreter,
            position_encoding,
            state: State::new(
                source_package_id,
                entry_exec_graph,
                None,
                ErrorBehavior::StopOnError,
            ),
        }
    }

    /// Resumes execution with specified `StepAction`.
    /// # Errors
    /// Returns a vector of errors if evaluating the entry point fails.
    pub fn eval_step(
        &mut self,
        receiver: &mut impl Receiver,
        breakpoints: &[StmtId],
        step: StepAction,
    ) -> std::result::Result<StepResult, Vec<Error>> {
        self.state
            .eval(
                &self.interpreter.fir_store,
                &mut self.interpreter.env,
                &mut self.interpreter.sim,
                receiver,
                breakpoints,
                step,
            )
            .map_err(|(error, call_stack)| {
                eval_error(
                    self.interpreter.compiler.package_store(),
                    &self.interpreter.fir_store,
                    call_stack,
                    error,
                )
            })
    }

    #[must_use]
    pub fn get_stack_frames(&self) -> Vec<StackFrame> {
        let frames = self.state.get_stack_frames();

        frames
            .iter()
            .map(|frame| {
                let callable = self
                    .interpreter
                    .fir_store
                    .get_global(frame.id)
                    .expect("frame should exist");
                let functor = format!("{}", frame.functor);
                let name = match callable {
                    Global::Callable(decl) => decl.name.name.to_string(),
                    Global::Udt => "udt".into(),
                };

                StackFrame {
                    name,
                    functor,
                    location: Location::from(
                        frame.span,
                        map_fir_package_to_hir(frame.id.package),
                        self.interpreter.compiler.package_store(),
                        self.position_encoding,
                    ),
                }
            })
            .collect()
    }

    pub fn capture_quantum_state(&mut self) -> (Vec<(BigUint, Complex<f64>)>, usize) {
        self.interpreter.sim.capture_quantum_state()
    }

    pub fn circuit(&self) -> Circuit {
        self.interpreter.get_circuit()
    }

    #[must_use]
    pub fn get_breakpoints(&self, path: &str) -> Vec<BreakpointSpan> {
        let unit = self.source_package();

        if let Some(source) = unit.sources.find_by_name(path) {
            let package = self
                .interpreter
                .fir_store
                .get(self.interpreter.source_package);
            let mut collector = BreakpointCollector::new(
                &unit.sources,
                source.offset,
                package,
                self.position_encoding,
            );
            collector.visit_package(package, &self.interpreter.fir_store);
            let mut spans: Vec<_> = collector.statements.into_iter().collect();

            // Sort by start position (line first, column next)
            spans.sort_by_key(|s| (s.range.start.line, s.range.start.column));
            spans
        } else {
            Vec::new()
        }
    }

    #[must_use]
    pub fn get_locals(&self, frame_id: usize) -> Vec<VariableInfo> {
        self.interpreter
            .env
            .get_variables_in_frame(frame_id)
            .into_iter()
            .filter(|v| !v.name.starts_with('@'))
            .collect()
    }

    fn source_package(&self) -> &CompileUnit {
        self.interpreter
            .compiler
            .package_store()
            .get(map_fir_package_to_hir(self.interpreter.source_package))
            .expect("Could not load package")
    }
}

/// Wrapper function for `qsc_eval::eval` that handles error conversion.
#[allow(clippy::too_many_arguments)]
fn eval(
    package: PackageId,
    classical_seed: Option<u64>,
    exec_graph: ExecGraph,
    package_store: &PackageStore,
    fir_store: &fir::PackageStore,
    env: &mut Env,
    sim: &mut impl Backend<ResultType = impl Into<val::Result>>,
    receiver: &mut impl Receiver,
) -> InterpretResult {
    qsc_eval::eval(
        package,
        classical_seed,
        exec_graph,
        fir_store,
        env,
        sim,
        receiver,
    )
    .map_err(|(error, call_stack)| eval_error(package_store, fir_store, call_stack, error))
}

/// Represents a stack frame for debugging.
pub struct StackFrame {
    /// The name of the callable.
    pub name: String,
    /// The functor of the callable.
    pub functor: String,
    /// The source location of the call site.
    pub location: Location,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BreakpointSpan {
    /// The id of the statement representing the breakpoint location.
    pub id: u32,
    /// The source range of the call site.
    pub range: Range,
}

struct BreakpointCollector<'a> {
    statements: FxHashSet<BreakpointSpan>,
    sources: &'a SourceMap,
    offset: u32,
    package: &'a Package,
    position_encoding: Encoding,
}

impl<'a> BreakpointCollector<'a> {
    fn new(
        sources: &'a SourceMap,
        offset: u32,
        package: &'a Package,
        position_encoding: Encoding,
    ) -> Self {
        Self {
            statements: FxHashSet::default(),
            sources,
            offset,
            package,
            position_encoding,
        }
    }

    fn get_source(&self, offset: u32) -> &Source {
        self.sources
            .find_by_offset(offset)
            .expect("Couldn't find source file")
    }

    fn add_stmt(&mut self, stmt: &fir::Stmt) {
        let source: &Source = self.get_source(stmt.span.lo);
        if source.offset == self.offset {
            let span = stmt.span - source.offset;
            if span != Span::default() {
                let bps = BreakpointSpan {
                    id: stmt.id.into(),
                    range: Range::from_span(self.position_encoding, &source.contents, &span),
                };
                self.statements.insert(bps);
            }
        }
    }
}

impl<'a> Visitor<'a> for BreakpointCollector<'a> {
    fn visit_stmt(&mut self, stmt: StmtId) {
        let stmt_res = self.get_stmt(stmt);
        match stmt_res.kind {
            fir::StmtKind::Expr(expr) | fir::StmtKind::Local(_, _, expr) => {
                self.add_stmt(stmt_res);
                visit::walk_expr(self, expr);
            }
            fir::StmtKind::Item(_) | fir::StmtKind::Semi(_) => {
                self.add_stmt(stmt_res);
            }
        }
    }

    fn get_block(&self, id: BlockId) -> &'a Block {
        self.package
            .blocks
            .get(id)
            .expect("couldn't find block in FIR")
    }

    fn get_expr(&self, id: ExprId) -> &'a Expr {
        self.package
            .exprs
            .get(id)
            .expect("couldn't find expr in FIR")
    }

    fn get_pat(&self, id: PatId) -> &'a Pat {
        self.package.pats.get(id).expect("couldn't find pat in FIR")
    }

    fn get_stmt(&self, id: StmtId) -> &'a Stmt {
        self.package
            .stmts
            .get(id)
            .expect("couldn't find stmt in FIR")
    }
}

fn eval_error(
    package_store: &PackageStore,
    fir_store: &fir::PackageStore,
    call_stack: Vec<Frame>,
    error: qsc_eval::Error,
) -> Vec<Error> {
    let stack_trace = if call_stack.is_empty() {
        None
    } else {
        Some(format_call_stack(
            package_store,
            fir_store,
            call_stack,
            &error,
        ))
    };

    vec![error::from_eval(error, package_store, stack_trace).into()]
}

#[must_use]
pub fn into_errors(errors: Vec<crate::compile::Error>) -> Vec<Error> {
    errors
        .into_iter()
        .map(|error| Error::Compile(error.into_with_source()))
        .collect::<Vec<_>>()
}
