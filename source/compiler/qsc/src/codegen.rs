// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

pub mod qsharp {
    pub use qsc_codegen::qsharp::write_package_string;
    pub use qsc_codegen::qsharp::write_stmt_string;
}

pub mod qir {
    use qsc_codegen::qir::{fir_to_qir, fir_to_rir};
    use qsc_eval::val::Value;
    use qsc_fir::fir::Package;

    use qsc_data_structures::{
        error::WithSource, functors::FunctorApp, language_features::LanguageFeatures,
        source::SourceMap, target::TargetCapabilityFlags,
    };
    use qsc_frontend::compile::{Dependencies, PackageStore};
    use qsc_partial_eval::{PartialEvalConfig, ProgramEntry};
    use qsc_passes::{PackageType, PassContext, run_rca_for_callable};
    use rustc_hash::FxHashSet;

    use crate::interpret::Error;

    /// Flat Intermediate Representation (FIR) ready for QIR/RIR code generation.
    ///
    /// Contains:
    /// - `fir_store`: Complete lowered FIR package store after all compiler passes
    /// - `fir_package_id`: Main package ID within the store
    /// - `compute_properties`: Resource analysis (qubit/instruction counts, etc.)
    ///
    /// Invariants (when created with full pipeline):
    /// - No type parameters remain (monomorphization complete)
    /// - No return statements (return unification complete)
    /// - Resolvable callable values specialized; valid residue deferred to RCA and partial evaluation
    /// - No UDT types (UDT erasure complete)
    /// - Execution graphs fully populated
    pub struct CodegenFir {
        pub fir_store: qsc_fir::fir::PackageStore,
        pub fir_package_id: qsc_fir::fir::PackageId,
        pub compute_properties: qsc_rca::PackageStoreComputeProperties,
        /// Non-fatal diagnostics surfaced while transforming the FIR (for
        /// example, warn-and-delegate diagnostics for early-return shapes the
        /// pipeline cannot convert). These are carried alongside the codegen
        /// FIR rather than dropped so the codegen caller can surface them.
        pub warnings: Vec<Error>,
    }

    /// Dispatch signal indicating which QIR-generation route a consumer of
    /// [`prepare_codegen_fir_from_callable_args`] must take.
    ///
    /// - `SyntheticEntry`: the prepared FIR carries a self-contained synthetic
    ///   entry expression; build QIR via `entry_from_codegen_fir` + `fir_to_qir`.
    /// - `ReinvokeOriginal`: the original target must be re-invoked through
    ///   `fir_to_qir_from_callable` with the recorded callable id and args.
    pub enum CallableArgsBackend {
        SyntheticEntry,
        ReinvokeOriginal {
            callable: qsc_fir::fir::StoreItemId,
            functor: FunctorApp,
            args: Value,
        },
    }

    /// Pre-computed type information for a reachable global callable: its formal
    /// arrow type and the generic type parameters that type is quantified over.
    /// Cached so a call site can infer the callable's concrete generic arguments
    /// from the type expected at that site.
    #[derive(Clone)]
    struct CallableValueInfo {
        /// Semantic declaration, retaining nominal identity until admission.
        formal_ty: qsc_fir::ty::Ty,
        /// Declaration with unresolved functors resolved to their
        /// declared lower bounds (and only unconstrained functors to `Empty`).
        ty: qsc_fir::ty::Ty,
        generics: Vec<qsc_fir::ty::TypeParameter>,
        parameters: Vec<(qsc_hir::ty::ParamId, qsc_hir::ty::TypeParameter)>,
        nominal_types: std::rc::Rc<NominalTypes>,
        span: qsc_data_structures::span::Span,
        sources: SourceMap,
    }

    type NominalTypes =
        rustc_hash::FxHashMap<qsc_fir::fir::ItemId, (qsc_hir::ty::Udt, qsc_fir::ty::Ty)>;

    #[derive(Default)]
    struct RuntimeParameters(
        std::collections::BTreeMap<qsc_hir::ty::ParamId, qsc_hir::ty::TypeParameter>,
    );

    impl RuntimeParameters {
        fn ty(&mut self, ty: &qsc_hir::ty::Ty) {
            use qsc_hir::ty::{ClassConstraint, FunctorSet, Ty, TypeParameter};
            match ty {
                Ty::Param { id, name, bounds } => {
                    let parameter = self.0.entry(*id).or_insert_with(|| TypeParameter::Ty {
                        name: name.clone(),
                        bounds: Default::default(),
                    });
                    let TypeParameter::Ty {
                        bounds: collected, ..
                    } = parameter
                    else {
                        return;
                    };
                    let mut added = Vec::new();
                    for bound in &bounds.0 {
                        if !collected.0.contains(bound) {
                            added.push(bound.clone());
                        }
                    }
                    collected.0 = collected
                        .0
                        .iter()
                        .cloned()
                        .chain(added.iter().cloned())
                        .collect();
                    for bound in added {
                        match bound {
                            ClassConstraint::Exp { power } => self.ty(&power),
                            ClassConstraint::Iterable { item } => self.ty(&item),
                            _ => {}
                        }
                    }
                }
                Ty::Array(item) => self.ty(item),
                Ty::Tuple(items) => items.iter().for_each(|item| self.ty(item)),
                Ty::Arrow(arrow) => {
                    self.ty(&arrow.input.borrow());
                    self.ty(&arrow.output.borrow());
                    if let FunctorSet::Param(id, required) = *arrow.functors.borrow() {
                        self.0
                            .entry(id)
                            .and_modify(|parameter| {
                                if let TypeParameter::Functor(bound) = parameter {
                                    *bound = bound.union(&required);
                                }
                            })
                            .or_insert(TypeParameter::Functor(required));
                    }
                }
                Ty::Prim(_) | Ty::Udt(_, _) | Ty::Infer(_) | Ty::Err => {}
            }
        }
    }

    impl<'a> qsc_hir::visit::Visitor<'a> for RuntimeParameters {
        fn visit_callable_decl(&mut self, decl: &'a qsc_hir::hir::CallableDecl) {
            for (index, parameter) in decl.generics.iter().enumerate() {
                let id = qsc_hir::ty::ParamId::from(index);
                match parameter {
                    qsc_hir::ty::TypeParameter::Ty { name, bounds } => {
                        self.ty(&qsc_hir::ty::Ty::Param {
                            id,
                            name: name.clone(),
                            bounds: bounds.clone(),
                        });
                    }
                    qsc_hir::ty::TypeParameter::Functor(_) => {
                        self.0.insert(id, parameter.clone());
                    }
                }
            }
            self.ty(&decl.output);
            qsc_hir::visit::walk_callable_decl(self, decl);
        }

        fn visit_expr(&mut self, expr: &'a qsc_hir::hir::Expr) {
            self.ty(&expr.ty);
            if let qsc_hir::hir::ExprKind::Var(_, args) = &expr.kind {
                for arg in args {
                    if let qsc_hir::ty::GenericArg::Ty(ty) = arg {
                        self.ty(ty);
                    }
                }
            }
            qsc_hir::visit::walk_expr(self, expr);
        }

        fn visit_pat(&mut self, pat: &'a qsc_hir::hir::Pat) {
            self.ty(&pat.ty);
            qsc_hir::visit::walk_pat(self, pat);
        }

        fn visit_block(&mut self, block: &'a qsc_hir::hir::Block) {
            self.ty(&block.ty);
            qsc_hir::visit::walk_block(self, block);
        }
    }

    type RuntimeCallableResult<T> = Result<T, Box<Error>>;
    type RuntimeTypeArgs = rustc_hash::FxHashMap<qsc_fir::ty::ParamId, qsc_fir::ty::GenericArg>;

    fn hir_runtime_ty(ty: &qsc_fir::ty::Ty, info: &CallableValueInfo) -> qsc_hir::ty::Ty {
        use qsc_fir::ty::{FunctorSet, Prim, Ty};
        match ty {
            Ty::Array(item) => qsc_hir::ty::Ty::Array(Box::new(hir_runtime_ty(item, info))),
            Ty::Tuple(items) => qsc_hir::ty::Ty::Tuple(
                items
                    .iter()
                    .map(|item| hir_runtime_ty(item, info))
                    .collect(),
            ),
            Ty::Arrow(arrow) => qsc_hir::ty::Ty::Arrow(std::rc::Rc::new(qsc_hir::ty::Arrow {
                kind: match arrow.kind {
                    qsc_fir::fir::CallableKind::Function => qsc_hir::hir::CallableKind::Function,
                    qsc_fir::fir::CallableKind::Operation => qsc_hir::hir::CallableKind::Operation,
                },
                input: std::cell::RefCell::new(hir_runtime_ty(&arrow.input, info)),
                output: std::cell::RefCell::new(hir_runtime_ty(&arrow.output, info)),
                functors: std::cell::RefCell::new(match arrow.functors {
                    FunctorSet::Value(value) => {
                        qsc_hir::ty::FunctorSet::Value(hir_runtime_functors(value))
                    }
                    _ => qsc_hir::ty::FunctorSet::Infer(Default::default()),
                }),
            })),
            Ty::Prim(prim) => qsc_hir::ty::Ty::Prim(match prim {
                Prim::BigInt => qsc_hir::ty::Prim::BigInt,
                Prim::Bool => qsc_hir::ty::Prim::Bool,
                Prim::Double => qsc_hir::ty::Prim::Double,
                Prim::Int => qsc_hir::ty::Prim::Int,
                Prim::Pauli => qsc_hir::ty::Prim::Pauli,
                Prim::Qubit => qsc_hir::ty::Prim::Qubit,
                Prim::Range => qsc_hir::ty::Prim::Range,
                Prim::RangeFrom => qsc_hir::ty::Prim::RangeFrom,
                Prim::RangeTo => qsc_hir::ty::Prim::RangeTo,
                Prim::RangeFull => qsc_hir::ty::Prim::RangeFull,
                Prim::Result => qsc_hir::ty::Prim::Result,
                Prim::String => qsc_hir::ty::Prim::String,
            }),
            Ty::Udt(qsc_fir::fir::Res::Item(id)) => qsc_hir::ty::Ty::Udt(
                info.nominal_types
                    .get(id)
                    .map_or_else(|| "unknown".into(), |(udt, _)| udt.name.clone()),
                qsc_hir::hir::Res::Item(qsc_hir::hir::ItemId {
                    package: qsc_lowerer::map_fir_package_to_hir(id.package),
                    item: qsc_lowerer::map_fir_local_item_to_hir(id.item),
                }),
            ),
            Ty::Infer(_) | Ty::Param(_) => qsc_hir::ty::Ty::Infer(Default::default()),
            Ty::Err | Ty::Udt(_) => qsc_hir::ty::Ty::Err,
        }
    }

    fn hir_runtime_functors(value: qsc_fir::ty::FunctorSetValue) -> qsc_hir::ty::FunctorSetValue {
        match value {
            qsc_fir::ty::FunctorSetValue::Empty => qsc_hir::ty::FunctorSetValue::Empty,
            qsc_fir::ty::FunctorSetValue::Adj => qsc_hir::ty::FunctorSetValue::Adj,
            qsc_fir::ty::FunctorSetValue::Ctl => qsc_hir::ty::FunctorSetValue::Ctl,
            qsc_fir::ty::FunctorSetValue::CtlAdj => qsc_hir::ty::FunctorSetValue::CtlAdj,
        }
    }

    fn finalize_runtime_type_args(
        callable: qsc_fir::fir::StoreItemId,
        info: &CallableValueInfo,
        mut inferred: RuntimeTypeArgs,
    ) -> RuntimeCallableResult<RuntimeTypeArgs> {
        use qsc_fir::ty::{FunctorSet, GenericArg};
        let mut defaulted = FxHashSet::default();
        let mut candidates = rustc_hash::FxHashMap::default();
        for (id, parameter) in &info.parameters {
            let fir_id = qsc_fir::ty::ParamId::from(usize::from(*id));
            let arg = inferred.entry(fir_id).or_insert_with(|| {
                defaulted.insert(*id);
                match parameter {
                    qsc_hir::ty::TypeParameter::Ty { .. } => GenericArg::Ty(qsc_fir::ty::Ty::UNIT),
                    qsc_hir::ty::TypeParameter::Functor(required) => {
                        GenericArg::Functor(FunctorSet::Value(lower_runtime_functors(*required)))
                    }
                }
            });
            let before = arg.clone();
            default_unresolved_generic_arg(arg);
            if before != *arg {
                defaulted.insert(*id);
            }
            candidates.insert(
                *id,
                match arg {
                    GenericArg::Ty(ty) => qsc_hir::ty::GenericArg::Ty(hir_runtime_ty(ty, info)),
                    GenericArg::Functor(FunctorSet::Value(value)) => {
                        qsc_hir::ty::GenericArg::Functor(qsc_hir::ty::FunctorSet::Value(
                            hir_runtime_functors(*value),
                        ))
                    }
                    GenericArg::Functor(_) => qsc_hir::ty::GenericArg::Functor(
                        qsc_hir::ty::FunctorSet::Infer(Default::default()),
                    ),
                },
            );
        }
        let udts = info
            .nominal_types
            .iter()
            .map(|(id, (udt, _))| {
                (
                    qsc_hir::hir::ItemId {
                        package: qsc_lowerer::map_fir_package_to_hir(id.package),
                        item: qsc_lowerer::map_fir_local_item_to_hir(id.item),
                    },
                    udt.clone(),
                )
            })
            .collect();
        if let Err(errors) = qsc_frontend::typeck::validate_instantiation(
            &info.parameters,
            &candidates,
            &udts,
            info.span,
        ) {
            let Some(error) = errors.into_iter().next() else {
                return Err(Box::new(Error::InvalidRuntimeCallable(callable)));
            };
            let insufficient = error.parameter.is_some_and(|id| {
                let mut dependencies = RuntimeParameters::default();
                if let Some((_, qsc_hir::ty::TypeParameter::Ty { name, bounds })) = info
                    .parameters
                    .iter()
                    .find(|(parameter, _)| *parameter == id)
                {
                    dependencies.ty(&qsc_hir::ty::Ty::Param {
                        id,
                        name: name.clone(),
                        bounds: bounds.clone(),
                    });
                }
                defaulted.contains(&id) || dependencies.0.keys().any(|id| defaulted.contains(id))
            });
            let error = WithSource::from_map(&info.sources, error);
            return Err(Box::new(if insufficient {
                Error::RuntimeCallableInsufficientEvidence { callable, error }
            } else {
                Error::RuntimeCallableConstraint { callable, error }
            }));
        }
        Ok(inferred)
    }

    fn validate_concrete_runtime_type(
        callable: qsc_fir::fir::StoreItemId,
        info: &CallableValueInfo,
        ty: &qsc_fir::ty::Ty,
    ) -> RuntimeCallableResult<()> {
        use qsc_hir::ty::{GenericArg, ParamId, TypeParameter};
        let id = ParamId::default();
        let parameters = [(
            id,
            TypeParameter::Ty {
                name: "callable".into(),
                bounds: Default::default(),
            },
        )];
        let candidates =
            rustc_hash::FxHashMap::from_iter([(id, GenericArg::Ty(hir_runtime_ty(ty, info)))]);
        let udts = info
            .nominal_types
            .iter()
            .map(|(id, (udt, _))| {
                (
                    qsc_hir::hir::ItemId {
                        package: qsc_lowerer::map_fir_package_to_hir(id.package),
                        item: qsc_lowerer::map_fir_local_item_to_hir(id.item),
                    },
                    udt.clone(),
                )
            })
            .collect();
        if let Err(errors) =
            qsc_frontend::typeck::validate_instantiation(&parameters, &candidates, &udts, info.span)
        {
            let Some(mut error) = errors.into_iter().next() else {
                return Err(Box::new(Error::InvalidRuntimeCallable(callable)));
            };
            error.parameter = None;
            return Err(Box::new(Error::RuntimeCallableConstraint {
                callable,
                error: WithSource::from_map(&info.sources, error),
            }));
        }
        Ok(())
    }

    fn fir_callable_id(item: qsc_hir::hir::ItemId) -> qsc_fir::fir::StoreItemId {
        qsc_fir::fir::StoreItemId {
            package: qsc_lowerer::map_hir_package_to_fir(item.package),
            item: qsc_lowerer::map_hir_local_item_to_fir(item.item),
        }
    }

    fn lower_runtime_functors(
        functors: qsc_hir::ty::FunctorSetValue,
    ) -> qsc_fir::ty::FunctorSetValue {
        match functors {
            qsc_hir::ty::FunctorSetValue::Empty => qsc_fir::ty::FunctorSetValue::Empty,
            qsc_hir::ty::FunctorSetValue::Adj => qsc_fir::ty::FunctorSetValue::Adj,
            qsc_hir::ty::FunctorSetValue::Ctl => qsc_fir::ty::FunctorSetValue::Ctl,
            qsc_hir::ty::FunctorSetValue::CtlAdj => qsc_fir::ty::FunctorSetValue::CtlAdj,
        }
    }

    struct ConcreteTargetSignature {
        generic_args: Vec<qsc_fir::ty::GenericArg>,
        inferred: RuntimeTypeArgs,
        arrow: qsc_fir::ty::Arrow,
    }

    /// Extracts the entry point expression from codegen FIR.
    ///
    /// Forms a `ProgramEntry` suitable for downstream codegen (QIR, RIR generation)
    /// by combining the entry expression and its associated execution graph.
    pub(crate) fn entry_from_codegen_fir(prepared_fir: &CodegenFir) -> ProgramEntry {
        let package = prepared_fir.fir_store.get(prepared_fir.fir_package_id);
        ProgramEntry {
            exec_graph: package.entry_exec_graph.clone(),
            expr: (
                prepared_fir.fir_package_id,
                package
                    .entry
                    .expect("package must have an entry expression"),
            )
                .into(),
        }
    }

    fn lower_to_fir(
        package_store: &PackageStore,
        package_id: qsc_hir::hir::PackageId,
        package_override: Option<&qsc_hir::hir::Package>,
    ) -> (
        qsc_fir::fir::PackageStore,
        qsc_fir::fir::PackageId,
        qsc_fir::assigner::Assigner,
    ) {
        if let Some(package_override) = package_override {
            let mut fir_store = qsc_fir::fir::PackageStore::new();
            let mut fir_assigner = qsc_fir::assigner::Assigner::new();

            for (id, unit) in package_store {
                let hir_package = if id == package_id {
                    package_override
                } else {
                    &unit.package
                };

                let fir_id = qsc_lowerer::map_hir_package_to_fir(id);
                let mut lowerer = qsc_lowerer::Lowerer::new();
                let fir_package = if id == package_id {
                    let mut fir_package = Package {
                        id: fir_id,
                        ..Package::default()
                    };
                    lowerer.lower_and_update_package(&mut fir_package, hir_package);
                    fir_package.entry_exec_graph = lowerer.take_exec_graph();
                    fir_package
                } else {
                    lowerer.lower_package(hir_package, &fir_store, fir_id)
                };
                if id == package_id {
                    fir_assigner = lowerer.into_assigner();
                }
                fir_store.insert(fir_id, fir_package);
            }

            (
                fir_store,
                qsc_lowerer::map_hir_package_to_fir(package_id),
                fir_assigner,
            )
        } else {
            qsc_passes::lower_hir_to_fir(package_store, package_id)
        }
    }

    /// Runs the full FIR transformation pipeline through all stages.
    ///
    /// Applies compiler passes (monomorphization, defunctionalization, UDT erasure, etc.)
    /// to produce codegen-ready FIR satisfying full invariants.
    pub fn run_codegen_pipeline(
        package_store: &PackageStore,
        fir_store: &mut qsc_fir::fir::PackageStore,
        fir_package_id: qsc_fir::fir::PackageId,
    ) -> Result<Vec<Error>, Vec<Error>> {
        run_codegen_pipeline_to(
            package_store,
            fir_store,
            fir_package_id,
            qsc_fir_transforms::PipelineStage::Full,
            &[],
        )
    }

    /// Runs the FIR pipeline up to a specified stage with optional item pinning.
    ///
    /// Allows fine-grained control over pipeline execution:
    /// - `stage`: Which pipeline stage to stop at (e.g., `PipelineStage::Full` for all passes)
    /// - `pinned_items`: Callables to preserve even if not reached from entry
    ///   (useful for callable arguments that might otherwise be eliminated by DCE)
    ///
    /// This is critical for higher-order function support: when a callable is passed
    /// as an argument, it may not be directly reachable from entry and would normally be
    /// removed during dead-code elimination. Pinning preserves these for specialization.
    pub fn run_codegen_pipeline_to(
        package_store: &PackageStore,
        fir_store: &mut qsc_fir::fir::PackageStore,
        fir_package_id: qsc_fir::fir::PackageId,
        stage: qsc_fir_transforms::PipelineStage,
        pinned_items: &[qsc_fir::fir::StoreItemId],
    ) -> Result<Vec<Error>, Vec<Error>> {
        // CONTRACT: On success, `run_pipeline_to` with `PipelineStage::Full` produces FIR
        // satisfying `InvariantLevel::PostAll`:
        //   - No `Ty::Param` in reachable code (monomorphization completed).
        //   - No `ExprKind::Return` in reachable code (return unification completed), except in
        //     callables the pipeline deliberately left un-rewritten because their early-return
        //     shape is not convertible; those are reported as non-fatal warnings and retain a
        //     residual `Return`. The invariant checker skips exactly the residual-`Return`
        //     checks for that skip-set while enforcing every other invariant on them.
        //   - Resolvable callable values specialized; residue remains subject to RCA and partial evaluation.
        //   - No `Ty::Udt` / `ExprKind::Struct`; `Field::Path` only on tuple records
        //     (UDT erasure completed).
        //   - All exec-graph ranges populated (exec-graph rebuild completed).
        // Downstream codegen (QIR lowering, partial evaluation) assumes these invariants hold.
        // See `qsc_fir_transforms::invariants::check` for the authoritative checker.
        let pipeline_result = qsc_fir_transforms::run_pipeline_to_with_diagnostics(
            fir_store,
            fir_package_id,
            stage,
            pinned_items,
        );
        if !pipeline_result.errors.is_empty() {
            return Err(pipeline_result
                .errors
                .into_iter()
                .map(|error| {
                    Error::FirTransform(crate::compile::attach_fir_transform_source(
                        package_store,
                        error,
                    ))
                })
                .collect());
        }

        // Surface non-fatal pipeline warnings to the caller rather than dropping
        // them, mirroring how the language-service path forwards them.
        Ok(pipeline_result
            .warnings
            .into_iter()
            .map(|warning| {
                Error::FirTransform(crate::compile::attach_fir_transform_source(
                    package_store,
                    warning,
                ))
            })
            .collect())
    }

    /// Runs the body-only signature-preserving FIR sub-pipeline on the
    /// pinned `ReinvokeOriginal` target bodies.
    ///
    /// The main `Full` pipeline (run rooted at the entry) never return-unifies
    /// the pinned target because it is not entry-reachable, so its early
    /// returns inside dynamic branches survive and trip the RCA
    /// `ReturnWithinDynamicScope` gate. This re-roots `return_unify` and the
    /// tuple passes at `seeds` (the pinned target plus its transitive callees)
    /// so the early returns become flag-guarded forward control flow before
    /// capability validation runs. Diagnostics are mapped with the same
    /// contract as [`run_codegen_pipeline_to`].
    fn run_codegen_signature_preserving_subpipeline(
        package_store: &PackageStore,
        _package_id: qsc_hir::hir::PackageId,
        fir_store: &mut qsc_fir::fir::PackageStore,
        fir_package_id: qsc_fir::fir::PackageId,
        seeds: &[qsc_fir::fir::StoreItemId],
    ) -> Result<(), Vec<Error>> {
        let pipeline_result = qsc_fir_transforms::run_signature_preserving_subpipeline(
            fir_store,
            fir_package_id,
            seeds,
        );
        if !pipeline_result.errors.is_empty() {
            return Err(pipeline_result
                .errors
                .into_iter()
                .map(|error| {
                    Error::FirTransform(crate::compile::attach_fir_transform_source(
                        package_store,
                        error,
                    ))
                })
                .collect());
        }

        Ok(())
    }

    fn map_pass_errors(
        package_store: &PackageStore,
        package_id: qsc_hir::hir::PackageId,
        errors: Vec<qsc_passes::Error>,
    ) -> Vec<Error> {
        errors
            .into_iter()
            .map(|e| {
                // A capability error can point into a dependency, so resolve it
                // against the package that actually owns its span.
                let owner = match &e {
                    qsc_passes::Error::CapabilitiesCk(inner) => {
                        qsc_lowerer::map_fir_package_to_hir(inner.span().package)
                    }
                    _ => package_id,
                };
                let source_package = package_store
                    .get(owner)
                    .expect("package should be in store");
                Error::Pass(WithSource::from_map(&source_package.sources, e))
            })
            .collect()
    }

    fn validate_callable_capabilities(
        package_store: &PackageStore,
        fir_store: &qsc_fir::fir::PackageStore,
        compute_properties: &qsc_rca::PackageStoreComputeProperties,
        callable: qsc_fir::fir::StoreItemId,
        capabilities: TargetCapabilityFlags,
    ) -> Result<(), Vec<Error>> {
        let errors = run_rca_for_callable(fir_store, compute_properties, callable, capabilities);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(map_pass_errors(
                package_store,
                qsc_lowerer::map_fir_package_to_hir(callable.package),
                errors,
            ))
        }
    }

    /// Returns true if a type is, or structurally contains, a callable arrow type.
    ///
    /// Arrays, tuples, and UDT pure types are traversed recursively so callers can
    /// detect callable fields even before UDT erasure has normalized the type shape.
    fn ty_contains_arrow(ty: &qsc_fir::ty::Ty, fir_store: &qsc_fir::fir::PackageStore) -> bool {
        match ty {
            qsc_fir::ty::Ty::Array(item) => ty_contains_arrow(item, fir_store),
            qsc_fir::ty::Ty::Arrow(_) => true,
            qsc_fir::ty::Ty::Tuple(items) => {
                items.iter().any(|item| ty_contains_arrow(item, fir_store))
            }
            qsc_fir::ty::Ty::Udt(res) => {
                let qsc_fir::fir::Res::Item(item_id) = res else {
                    return false;
                };
                let package = fir_store.get(item_id.package);
                let item = package
                    .items
                    .get(item_id.item)
                    .expect("UDT item should exist");
                let qsc_fir::fir::ItemKind::Ty(_, udt) = &item.kind else {
                    return false;
                };
                ty_contains_arrow(&udt.get_pure_ty(), fir_store)
            }
            qsc_fir::ty::Ty::Infer(_)
            | qsc_fir::ty::Ty::Param(_)
            | qsc_fir::ty::Ty::Prim(_)
            | qsc_fir::ty::Ty::Err => false,
        }
    }

    fn callable_has_arrow_input(
        fir_store: &qsc_fir::fir::PackageStore,
        callable: qsc_hir::hir::ItemId,
    ) -> bool {
        use qsc_fir::fir::{Global, PackageLookup};

        let callable_store_id = qsc_fir::fir::StoreItemId {
            package: qsc_lowerer::map_hir_package_to_fir(callable.package),
            item: qsc_lowerer::map_hir_local_item_to_fir(callable.item),
        };

        let package = fir_store.get(callable_store_id.package);
        let Some(Global::Callable(callable_decl)) = package.get_global(callable_store_id.item)
        else {
            panic!("callable should exist in lowered package");
        };

        ty_contains_arrow(&package.get_pat(callable_decl.input).ty, fir_store)
    }

    fn seed_entry_with_callable(
        fir_store: &mut qsc_fir::fir::PackageStore,
        fir_package_id: qsc_fir::fir::PackageId,
        callable: qsc_hir::hir::ItemId,
    ) {
        let callable_store_id = qsc_fir::fir::StoreItemId {
            package: qsc_lowerer::map_hir_package_to_fir(callable.package),
            item: qsc_lowerer::map_hir_local_item_to_fir(callable.item),
        };

        let (span, ty) = {
            use qsc_fir::fir::{Global, PackageLookup};

            let package = fir_store.get(callable_store_id.package);
            let Some(Global::Callable(callable_decl)) = package.get_global(callable_store_id.item)
            else {
                panic!("callable should exist in lowered package");
            };

            let input = package.get_pat(callable_decl.input).ty.clone();
            let ty = qsc_fir::ty::Ty::Arrow(Box::new(qsc_fir::ty::Arrow {
                kind: callable_decl.kind,
                input: Box::new(input),
                output: Box::new(callable_decl.output.clone()),
                functors: qsc_fir::ty::FunctorSet::Value(callable_decl.functors),
            }));

            (callable_decl.span, ty)
        };

        let entry_expr_id =
            qsc_fir::assigner::Assigner::from_package(fir_store.get(fir_package_id)).next_expr();
        let package = fir_store.get_mut(fir_package_id);
        package.exprs.insert(
            entry_expr_id,
            qsc_fir::fir::Expr {
                id: entry_expr_id,
                span,
                ty,
                kind: qsc_fir::fir::ExprKind::Var(
                    qsc_fir::fir::Res::Item(qsc_fir::fir::ItemId {
                        package: callable_store_id.package,
                        item: callable_store_id.item,
                    }),
                    Vec::new(),
                ),
                exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                    ..qsc_fir::fir::ExecGraphIdx::ZERO,
            },
        );
        package.entry = Some(entry_expr_id);
        package.entry_exec_graph = Default::default();
    }

    fn callable_expr_span_and_ty(
        fir_store: &qsc_fir::fir::PackageStore,
        callable_store_id: qsc_fir::fir::StoreItemId,
    ) -> (qsc_fir::fir::PackageSpan, qsc_fir::ty::Ty) {
        use qsc_fir::fir::{Global, PackageLookup};

        let package = fir_store.get(callable_store_id.package);
        let Some(Global::Callable(callable_decl)) = package.get_global(callable_store_id.item)
        else {
            panic!("callable should exist in lowered package");
        };

        let input = package.get_pat(callable_decl.input).ty.clone();
        let ty = qsc_fir::ty::Ty::Arrow(Box::new(qsc_fir::ty::Arrow {
            kind: callable_decl.kind,
            input: Box::new(input),
            output: Box::new(callable_decl.output.clone()),
            functors: qsc_fir::ty::FunctorSet::Value(callable_decl.functors),
        }));

        (callable_decl.span, ty)
    }

    pub(super) fn seed_entry_with_callables(
        fir_store: &mut qsc_fir::fir::PackageStore,
        fir_package_id: qsc_fir::fir::PackageId,
        callables: &FxHashSet<qsc_fir::fir::StoreItemId>,
    ) {
        if callables.is_empty() {
            return;
        }

        let mut assigner = qsc_fir::assigner::Assigner::from_package(fir_store.get(fir_package_id));

        let mut entry_exprs = Vec::with_capacity(callables.len());
        let mut entry_tys = Vec::with_capacity(callables.len());
        let mut entry_span = None;

        for callable in callables {
            let (span, ty) = callable_expr_span_and_ty(fir_store, *callable);
            let expr_id = assigner.next_expr();
            let package = fir_store.get_mut(fir_package_id);
            package.exprs.insert(
                expr_id,
                qsc_fir::fir::Expr {
                    id: expr_id,
                    span,
                    ty: ty.clone(),
                    kind: qsc_fir::fir::ExprKind::Var(
                        qsc_fir::fir::Res::Item(qsc_fir::fir::ItemId {
                            package: callable.package,
                            item: callable.item,
                        }),
                        Vec::new(),
                    ),
                    exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                        ..qsc_fir::fir::ExecGraphIdx::ZERO,
                },
            );
            entry_exprs.push(expr_id);
            entry_tys.push(ty);
            entry_span.get_or_insert(span);
        }

        let entry_expr_id = if entry_exprs.len() == 1 {
            entry_exprs[0]
        } else {
            let entry_expr_id = assigner.next_expr();
            let package = fir_store.get_mut(fir_package_id);
            package.exprs.insert(
                entry_expr_id,
                qsc_fir::fir::Expr {
                    id: entry_expr_id,
                    span: entry_span.expect("tuple entry should have a span"),
                    ty: qsc_fir::ty::Ty::Tuple(entry_tys),
                    kind: qsc_fir::fir::ExprKind::Tuple(entry_exprs),
                    exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                        ..qsc_fir::fir::ExecGraphIdx::ZERO,
                },
            );
            entry_expr_id
        };

        let package = fir_store.get_mut(fir_package_id);
        package.entry = Some(entry_expr_id);
        package.entry_exec_graph = Default::default();
    }

    /// Builds a pre-computed map of normalized callable value types for all
    /// `Global` and `Closure` values in `args`.
    ///
    /// This allows `lower_value_to_expr` to look up arrow types without holding an immutable
    /// reference to the package store while also mutating a package.
    fn build_callable_type_map(
        package_store: &PackageStore,
        fir_store: &qsc_fir::fir::PackageStore,
        callables: &FxHashSet<qsc_fir::fir::StoreItemId>,
        target: qsc_fir::fir::StoreItemId,
    ) -> RuntimeCallableResult<rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>>
    {
        use qsc_fir::fir::{Global, PackageLookup};
        use qsc_hir::visit::Visitor;

        let nominal_types = std::rc::Rc::new(
            package_store
                .iter()
                .flat_map(|(package, unit)| {
                    unit.package
                        .items
                        .iter()
                        .filter_map(move |(item, declaration)| {
                            if let qsc_hir::hir::ItemKind::Ty(_, udt) = &declaration.kind {
                                let id = qsc_fir::fir::ItemId {
                                    package: qsc_lowerer::map_hir_package_to_fir(package),
                                    item: qsc_lowerer::map_hir_local_item_to_fir(item),
                                };
                                let qsc_fir::fir::ItemKind::Ty(_, lowered) =
                                    &fir_store.get(id.package).items.get(id.item)?.kind
                                else {
                                    return None;
                                };
                                let structural = lowered.get_pure_ty();
                                Some((id, (udt.clone(), structural)))
                            } else {
                                None
                            }
                        })
                })
                .collect(),
        );

        let callables: FxHashSet<_> = callables.iter().copied().chain([target]).collect();
        let mut map =
            rustc_hash::FxHashMap::with_capacity_and_hasher(callables.len(), Default::default());
        for id in &callables {
            let Some(package) = fir_store
                .iter()
                .find_map(|(package_id, package)| (package_id == id.package).then_some(package))
            else {
                return Err(Box::new(Error::InvalidRuntimeCallable(*id)));
            };
            let Some(Global::Callable(callable_decl)) = package.get_global(id.item) else {
                return Err(Box::new(Error::InvalidRuntimeCallable(*id)));
            };
            let (_, ty) = callable_expr_span_and_ty(fir_store, *id);
            let Some(qsc_hir::hir::Item {
                kind: qsc_hir::hir::ItemKind::Callable(declaration),
                ..
            }) = package_store
                .get(qsc_lowerer::map_fir_package_to_hir(id.package))
                .and_then(|unit| {
                    unit.package
                        .items
                        .get(qsc_lowerer::map_fir_local_item_to_hir(id.item))
                })
            else {
                return Err(Box::new(Error::InvalidRuntimeCallable(*id)));
            };
            let mut parameters = RuntimeParameters::default();
            parameters.visit_callable_decl(declaration);
            let parameters: Vec<_> = parameters.0.into_iter().collect();
            let formal_ty = ty;
            let declared_functors = parameters
                .iter()
                .filter_map(|(id, parameter)| {
                    if let qsc_hir::ty::TypeParameter::Functor(required) = parameter {
                        Some((
                            qsc_fir::ty::ParamId::from(usize::from(*id)),
                            qsc_fir::ty::GenericArg::Functor(qsc_fir::ty::FunctorSet::Value(
                                lower_runtime_functors(*required),
                            )),
                        ))
                    } else {
                        None
                    }
                })
                .collect();
            let normalized_ty = resolve_params_with_inferred(&formal_ty, &declared_functors);
            map.insert(
                *id,
                CallableValueInfo {
                    formal_ty,
                    ty: normalized_ty,
                    generics: callable_decl.generics.clone(),
                    parameters,
                    nominal_types: std::rc::Rc::clone(&nominal_types),
                    span: declaration.name.span,
                    sources: package_store
                        .get(qsc_lowerer::map_fir_package_to_hir(id.package))
                        .map_or_else(SourceMap::default, |unit| unit.sources.clone()),
                },
            );
        }
        Ok(map)
    }

    fn nominal_structural_ty<'a>(
        ty: &qsc_fir::ty::Ty,
        callable_types: &'a rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> Option<&'a qsc_fir::ty::Ty> {
        let qsc_fir::ty::Ty::Udt(qsc_fir::fir::Res::Item(id)) = ty else {
            return None;
        };
        callable_types
            .values()
            .next()?
            .nominal_types
            .get(id)
            .map(|(_, ty)| ty)
    }

    fn erase_runtime_type_evidence(
        store: &qsc_fir::fir::PackageStore,
        callable_types: &mut rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
        signature: &mut ConcreteTargetSignature,
    ) {
        for info in callable_types.values_mut() {
            info.formal_ty = resolve_udt_ty(store, &info.formal_ty);
            info.ty = resolve_udt_ty(store, &info.ty);
            info.nominal_types = Default::default();
        }
        *signature.arrow.input = resolve_udt_ty(store, &signature.arrow.input);
        *signature.arrow.output = resolve_udt_ty(store, &signature.arrow.output);
        for arg in &mut signature.generic_args {
            if let qsc_fir::ty::GenericArg::Ty(ty) = arg {
                *ty = resolve_udt_ty(store, ty);
            }
        }
    }

    /// Normalizes concrete runtime callable type copies before synthetic-entry lowering.
    ///
    /// Interpreter-created callable values can retain inferred functor parameters
    /// in their lowered body node types even when the callable itself is concrete.
    /// Those stale parameters would violate post-monomorphization invariants once
    /// the callable is made entry-reachable. Generic callable signatures are left
    /// intact so monomorphization can still infer and create concrete
    /// specializations from closure targets.
    fn normalize_callable_signatures(
        fir_store: &mut qsc_fir::fir::PackageStore,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) {
        use qsc_fir::fir::{CallableImpl, Global, PackageLookup};

        let normalized: Vec<_> = callable_types
            .iter()
            .map(|(id, info)| {
                let package = fir_store.get(id.package);
                let Some(Global::Callable(callable_decl)) = package.get_global(id.item) else {
                    panic!("callable should exist in lowered package");
                };
                let normalized_signature = if callable_decl.generics.is_empty() {
                    let qsc_fir::ty::Ty::Arrow(arrow) = &info.ty else {
                        panic!("callable value should have an arrow type");
                    };
                    Some((arrow.input.as_ref().clone(), arrow.output.as_ref().clone()))
                } else {
                    None
                };
                let mut inferred = rustc_hash::FxHashMap::default();
                let _ = infer_generic_ty_args(
                    &info.formal_ty,
                    &info.ty,
                    &mut inferred,
                    GenericInferenceSource::RuntimeValue,
                );
                (*id, callable_decl.input, normalized_signature, inferred)
            })
            .collect();

        for (id, input_pat_id, normalized_signature, inferred) in normalized {
            let package = fir_store.get_mut(id.package);
            let normalized_output_ty = if let Some((input_ty, output_ty)) = normalized_signature {
                normalize_pattern_node_types(package, input_pat_id, &input_ty);
                Some(output_ty)
            } else {
                None
            };
            let qsc_fir::fir::ItemKind::Callable(callable_decl) = &mut package
                .items
                .get_mut(id.item)
                .expect("callable item should exist")
                .kind
            else {
                panic!("callable should exist in lowered package");
            };
            if let Some(output_ty) = normalized_output_ty {
                callable_decl.output = output_ty;
            }
            let CallableImpl::Spec(spec_impl) = &callable_decl.implementation else {
                continue;
            };
            let mut block_ids = vec![spec_impl.body.block];
            block_ids.extend(
                spec_impl
                    .adj
                    .iter()
                    .chain(spec_impl.ctl.iter())
                    .chain(spec_impl.ctl_adj.iter())
                    .map(|spec| spec.block),
            );

            for block_id in block_ids {
                normalize_block_node_types(package, block_id, &inferred);
            }
        }
    }

    fn infer_closure_generic_args(
        closure: &qsc_eval::val::Closure,
        expected_ty: Option<&qsc_fir::ty::Ty>,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> RuntimeCallableResult<RuntimeTypeArgs> {
        let info = callable_types
            .get(&closure.id)
            .expect("closure callable type should be pre-computed");
        let mut inferred = rustc_hash::FxHashMap::default();
        if let Some(capture_tys) =
            closure_capture_ty_hints(&info.formal_ty, closure.fixed_args.len())
        {
            for (capture, formal_ty) in closure.fixed_args.iter().zip(&capture_tys) {
                let Some(actual_ty) =
                    value_ty_for_inference(capture, Some(formal_ty), callable_types)
                else {
                    return Err(Box::new(Error::RuntimeCallableTypeMismatch {
                        expected: Box::new(formal_ty.clone()),
                        actual: Box::new(qsc_fir::ty::Ty::Err),
                    }));
                };
                let mut candidate = inferred.clone();
                let actual_ty = align_runtime_callable_input(formal_ty, &actual_ty);
                if !infer_generic_ty_args(
                    formal_ty,
                    &actual_ty,
                    &mut candidate,
                    GenericInferenceSource::RuntimeValue,
                ) {
                    return Err(Box::new(Error::RuntimeCallableTypeMismatch {
                        expected: Box::new(formal_ty.clone()),
                        actual: Box::new(actual_ty),
                    }));
                }
                inferred = candidate;
            }
        }

        let expected_base_ty = expected_ty.and_then(|expected_ty| {
            callable_ty_before_runtime_functor(expected_ty, closure.functor)
        });
        if let Some(expected_ty) = expected_base_ty.as_ref() {
            let formal_closure_ty =
                partial_applied_closure_ty(&info.formal_ty, closure.fixed_args.len());
            let aligned_expected = align_runtime_callable_input(&formal_closure_ty, expected_ty);
            let mut candidate = inferred.clone();
            if !infer_generic_ty_args(
                &formal_closure_ty,
                &aligned_expected,
                &mut candidate,
                GenericInferenceSource::ExpectedType,
            ) {
                return Err(Box::new(Error::RuntimeCallableTypeMismatch {
                    expected: Box::new(expected_ty.clone()),
                    actual: Box::new(resolve_params_preserving_uninferred(
                        &formal_closure_ty,
                        &inferred,
                    )),
                }));
            }
            inferred = candidate;
        }
        Ok(inferred)
    }

    fn closure_ty_for_inference(
        closure: &qsc_eval::val::Closure,
        expected_ty: Option<&qsc_fir::ty::Ty>,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> Option<qsc_fir::ty::Ty> {
        let info = callable_types
            .get(&closure.id)
            .expect("closure callable type should be pre-computed");
        let full_ty = if info.parameters.is_empty() {
            info.ty.clone()
        } else {
            let inferred = infer_closure_generic_args(closure, expected_ty, callable_types).ok()?;
            resolve_params_preserving_uninferred(&info.formal_ty, &inferred)
        };
        let partial_ty = partial_applied_closure_ty(&full_ty, closure.fixed_args.len());
        Some(callable_ty_with_runtime_functor(
            &partial_ty,
            closure.functor,
        ))
    }

    fn resolve_params_with_inferred(
        ty: &qsc_fir::ty::Ty,
        inferred: &rustc_hash::FxHashMap<qsc_fir::ty::ParamId, qsc_fir::ty::GenericArg>,
    ) -> qsc_fir::ty::Ty {
        resolve_params(ty, inferred, UninferredFunctorPolicy::Empty)
    }

    fn resolve_params_preserving_uninferred(
        ty: &qsc_fir::ty::Ty,
        inferred: &rustc_hash::FxHashMap<qsc_fir::ty::ParamId, qsc_fir::ty::GenericArg>,
    ) -> qsc_fir::ty::Ty {
        resolve_params(ty, inferred, UninferredFunctorPolicy::Preserve)
    }

    #[derive(Clone, Copy)]
    enum UninferredFunctorPolicy {
        Empty,
        Preserve,
    }

    fn resolve_params(
        ty: &qsc_fir::ty::Ty,
        inferred: &rustc_hash::FxHashMap<qsc_fir::ty::ParamId, qsc_fir::ty::GenericArg>,
        policy: UninferredFunctorPolicy,
    ) -> qsc_fir::ty::Ty {
        use qsc_fir::ty::{Arrow, FunctorSet, FunctorSetValue, GenericArg, Ty};

        match ty {
            Ty::Arrow(arrow) => {
                let unresolved = match policy {
                    UninferredFunctorPolicy::Empty => FunctorSet::Value(FunctorSetValue::Empty),
                    UninferredFunctorPolicy::Preserve => arrow.functors,
                };
                let functors = match arrow.functors {
                    FunctorSet::Param(param) => match inferred.get(&param) {
                        Some(GenericArg::Functor(functors)) => *functors,
                        _ => unresolved,
                    },
                    FunctorSet::Infer(_) => unresolved,
                    FunctorSet::Value(value) => FunctorSet::Value(value),
                };
                Ty::Arrow(Box::new(Arrow {
                    kind: arrow.kind,
                    input: Box::new(resolve_params(&arrow.input, inferred, policy)),
                    output: Box::new(resolve_params(&arrow.output, inferred, policy)),
                    functors,
                }))
            }
            Ty::Tuple(items) => Ty::Tuple(
                items
                    .iter()
                    .map(|item| resolve_params(item, inferred, policy))
                    .collect(),
            ),
            Ty::Array(item) => Ty::Array(Box::new(resolve_params(item, inferred, policy))),
            Ty::Param(param) => match inferred.get(param) {
                Some(GenericArg::Ty(ty)) => ty.clone(),
                _ => ty.clone(),
            },
            Ty::Infer(_) if matches!(policy, UninferredFunctorPolicy::Empty) => Ty::UNIT,
            Ty::Err | Ty::Infer(_) | Ty::Prim(_) | Ty::Udt(_) => ty.clone(),
        }
    }

    fn normalize_pattern_node_types(
        package: &mut qsc_fir::fir::Package,
        pat_id: qsc_fir::fir::PatId,
        normalized_ty: &qsc_fir::ty::Ty,
    ) {
        use qsc_fir::fir::{PackageLookup, PatKind};
        use qsc_fir::ty::Ty;

        let child_ids = match (&package.get_pat(pat_id).kind, normalized_ty) {
            (PatKind::Tuple(child_ids), Ty::Tuple(child_tys)) => {
                assert_eq!(child_ids.len(), child_tys.len());
                Some((child_ids.clone(), child_tys.clone()))
            }
            _ => None,
        };
        package
            .pats
            .get_mut(pat_id)
            .expect("callable input pattern should exist")
            .ty = normalized_ty.clone();
        if let Some((child_ids, child_tys)) = child_ids {
            for (child_id, child_ty) in child_ids.into_iter().zip(&child_tys) {
                normalize_pattern_node_types(package, child_id, child_ty);
            }
        }
    }

    fn normalize_block_node_types(
        package: &mut qsc_fir::fir::Package,
        block_id: qsc_fir::fir::BlockId,
        inferred: &rustc_hash::FxHashMap<qsc_fir::ty::ParamId, qsc_fir::ty::GenericArg>,
    ) {
        let stmt_ids = package
            .blocks
            .get_mut(block_id)
            .expect("callable block should exist")
            .stmts
            .clone();

        for stmt_id in stmt_ids {
            let stmt = package
                .stmts
                .get(stmt_id)
                .expect("callable statement should exist");
            let (pat_id, expr_id) = match stmt.kind {
                qsc_fir::fir::StmtKind::Expr(expr_id) | qsc_fir::fir::StmtKind::Semi(expr_id) => {
                    (None, Some(expr_id))
                }
                qsc_fir::fir::StmtKind::Local(_, pat_id, expr_id) => (Some(pat_id), Some(expr_id)),
                qsc_fir::fir::StmtKind::Item(_) => (None, None),
            };

            if let Some(pat_id) = pat_id {
                normalize_pat_node_types(package, pat_id, inferred);
            }
            if let Some(expr_id) = expr_id {
                normalize_expr_node_types(package, expr_id, inferred);
            }
        }

        let block = package
            .blocks
            .get_mut(block_id)
            .expect("callable block should exist");
        block.ty = resolve_params_with_inferred(&block.ty, inferred);
    }

    fn normalize_pat_node_types(
        package: &mut qsc_fir::fir::Package,
        pat_id: qsc_fir::fir::PatId,
        inferred: &rustc_hash::FxHashMap<qsc_fir::ty::ParamId, qsc_fir::ty::GenericArg>,
    ) {
        let child_pats = {
            let pat = package
                .pats
                .get_mut(pat_id)
                .expect("callable pattern should exist");
            pat.ty = resolve_params_with_inferred(&pat.ty, inferred);
            match &pat.kind {
                qsc_fir::fir::PatKind::Tuple(pats) => pats.clone(),
                qsc_fir::fir::PatKind::Bind(_) | qsc_fir::fir::PatKind::Discard => Vec::new(),
            }
        };
        for child_pat in child_pats {
            normalize_pat_node_types(package, child_pat, inferred);
        }
    }

    fn normalize_expr_node_types(
        package: &mut qsc_fir::fir::Package,
        expr_id: qsc_fir::fir::ExprId,
        inferred: &rustc_hash::FxHashMap<qsc_fir::ty::ParamId, qsc_fir::ty::GenericArg>,
    ) {
        let (child_exprs, child_blocks) = {
            let expr = package
                .exprs
                .get_mut(expr_id)
                .expect("callable expression should exist");
            expr.ty = resolve_params_with_inferred(&expr.ty, inferred);
            if let qsc_fir::fir::ExprKind::Var(_, generic_args) = &mut expr.kind {
                for arg in generic_args {
                    match arg {
                        qsc_fir::ty::GenericArg::Ty(ty) => {
                            *ty = resolve_params_with_inferred(ty, inferred);
                        }
                        qsc_fir::ty::GenericArg::Functor(qsc_fir::ty::FunctorSet::Param(param)) => {
                            if let Some(inferred) = inferred.get(param) {
                                *arg = inferred.clone();
                            }
                        }
                        qsc_fir::ty::GenericArg::Functor(_) => {}
                    }
                }
            }
            child_nodes_for_expr_kind(&expr.kind)
        };

        for child_expr in child_exprs {
            normalize_expr_node_types(package, child_expr, inferred);
        }
        for child_block in child_blocks {
            normalize_block_node_types(package, child_block, inferred);
        }
    }

    fn child_nodes_for_expr_kind(
        kind: &qsc_fir::fir::ExprKind,
    ) -> (Vec<qsc_fir::fir::ExprId>, Vec<qsc_fir::fir::BlockId>) {
        let mut exprs = Vec::new();
        let mut blocks = Vec::new();
        match kind {
            qsc_fir::fir::ExprKind::Array(child_exprs)
            | qsc_fir::fir::ExprKind::ArrayLit(child_exprs)
            | qsc_fir::fir::ExprKind::Tuple(child_exprs) => exprs.extend(child_exprs.iter()),
            qsc_fir::fir::ExprKind::ArrayRepeat(left, right)
            | qsc_fir::fir::ExprKind::Assign(left, right)
            | qsc_fir::fir::ExprKind::AssignOp(_, left, right)
            | qsc_fir::fir::ExprKind::BinOp(_, left, right)
            | qsc_fir::fir::ExprKind::Call(left, right)
            | qsc_fir::fir::ExprKind::Index(left, right)
            | qsc_fir::fir::ExprKind::AssignField(left, _, right)
            | qsc_fir::fir::ExprKind::UpdateField(left, _, right) => {
                exprs.push(*left);
                exprs.push(*right);
            }
            qsc_fir::fir::ExprKind::AssignIndex(first, second, third)
            | qsc_fir::fir::ExprKind::UpdateIndex(first, second, third) => {
                exprs.push(*first);
                exprs.push(*second);
                exprs.push(*third);
            }
            qsc_fir::fir::ExprKind::Block(block_id) => blocks.push(*block_id),
            qsc_fir::fir::ExprKind::Closure(_, _)
            | qsc_fir::fir::ExprKind::Hole
            | qsc_fir::fir::ExprKind::Lit(_)
            | qsc_fir::fir::ExprKind::Var(_, _) => {}
            qsc_fir::fir::ExprKind::Fail(expr_id)
            | qsc_fir::fir::ExprKind::Field(expr_id, _)
            | qsc_fir::fir::ExprKind::Return(expr_id)
            | qsc_fir::fir::ExprKind::UnOp(_, expr_id) => exprs.push(*expr_id),
            qsc_fir::fir::ExprKind::If(cond, body, otherwise) => {
                exprs.push(*cond);
                exprs.push(*body);
                if let Some(otherwise) = otherwise {
                    exprs.push(*otherwise);
                }
            }
            qsc_fir::fir::ExprKind::Range(start, step, end) => {
                exprs.extend([start, step, end].into_iter().flatten().copied());
            }
            qsc_fir::fir::ExprKind::Struct(_, copy, fields) => {
                if let Some(copy) = copy {
                    exprs.push(*copy);
                }
                exprs.extend(fields.iter().map(|field| field.value));
            }
            qsc_fir::fir::ExprKind::String(components) => {
                exprs.extend(components.iter().filter_map(|component| match component {
                    qsc_fir::fir::StringComponent::Expr(expr_id) => Some(*expr_id),
                    qsc_fir::fir::StringComponent::Lit(_) => None,
                }));
            }
            qsc_fir::fir::ExprKind::While(cond, block_id) => {
                exprs.push(*cond);
                blocks.push(*block_id);
            }
            qsc_fir::fir::ExprKind::Parallel(limit, body) => {
                if let Some(limit) = limit {
                    exprs.push(*limit);
                }
                exprs.push(*body);
            }
        }
        (exprs, blocks)
    }

    /// Seeds the package entry with a synthetic `Call(target, args)` expression.
    ///
    /// Builds args from validated runtime values and the instantiated target signature.
    fn seed_entry_with_call_to_target(
        fir_store: &mut qsc_fir::fir::PackageStore,
        fir_package_id: qsc_fir::fir::PackageId,
        target_callable: qsc_fir::fir::StoreItemId,
        functor: FunctorApp,
        args: &Value,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
        signature: ConcreteTargetSignature,
    ) {
        use qsc_fir::fir::{Global, PackageLookup};

        let package = fir_store.get(target_callable.package);
        let Some(Global::Callable(callable_decl)) = package.get_global(target_callable.item) else {
            panic!("target callable must exist in lowered package");
        };
        let span = callable_decl.span;
        let ConcreteTargetSignature {
            generic_args,
            arrow,
            ..
        } = signature;
        let output_ty = *arrow.output.clone();

        // Build assigner from the package's current ID counters.
        let mut assigner = qsc_fir::assigner::Assigner::from_package(fir_store.get(fir_package_id));

        // Get the package mutably and build args expression matching the input type.
        // Capture let-bindings emitted while lowering closure arguments are collected
        // here so they can be placed ahead of the synthetic call in a block.
        let mut pending_stmts: Vec<qsc_fir::fir::StmtId> = Vec::new();
        let package = fir_store.get_mut(fir_package_id);
        let args_expr_id = build_synthetic_args(
            package,
            &mut assigner,
            &arrow.input,
            args,
            callable_types,
            &mut pending_stmts,
        );

        let invoked_ty = qsc_fir::ty::Ty::Arrow(Box::new(arrow));
        let base_ty = callable_ty_before_runtime_functor(&invoked_ty, functor)
            .expect("validated controlled target signature");
        // The item reference has the original input; the wrappers supply the
        // target's controlled layers rather than those of any callable argument.
        let callee_expr_id = assigner.next_expr();
        package.exprs.insert(
            callee_expr_id,
            qsc_fir::fir::Expr {
                id: callee_expr_id,
                span,
                ty: base_ty.clone(),
                kind: qsc_fir::fir::ExprKind::Var(
                    qsc_fir::fir::Res::Item(qsc_fir::fir::ItemId {
                        package: target_callable.package,
                        item: target_callable.item,
                    }),
                    generic_args,
                ),
                exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                    ..qsc_fir::fir::ExecGraphIdx::ZERO,
            },
        );
        let callee_expr_id =
            wrap_expr_with_functor_app(package, &mut assigner, callee_expr_id, &base_ty, functor);

        // Create Call expression: Call(callee, args) with output type.
        let call_expr_id = assigner.next_expr();
        package.exprs.insert(
            call_expr_id,
            qsc_fir::fir::Expr {
                id: call_expr_id,
                span,
                ty: output_ty.clone(),
                kind: qsc_fir::fir::ExprKind::Call(callee_expr_id, args_expr_id),
                exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                    ..qsc_fir::fir::ExecGraphIdx::ZERO,
            },
        );

        // When closure arguments produced capture let-bindings, the entry must
        // execute those bindings before the call. Wrap the bindings and the call
        // in a block whose trailing expression yields the call's value. Without
        // pending bindings, keep the bare call as the entry to avoid churn.
        let entry_expr_id = if pending_stmts.is_empty() {
            call_expr_id
        } else {
            let call_stmt_id = assigner.next_stmt();
            package.stmts.insert(
                call_stmt_id,
                qsc_fir::fir::Stmt {
                    id: call_stmt_id,
                    span,
                    kind: qsc_fir::fir::StmtKind::Expr(call_expr_id),
                    exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                        ..qsc_fir::fir::ExecGraphIdx::ZERO,
                },
            );
            pending_stmts.push(call_stmt_id);

            let block_id = assigner.next_block();
            package.blocks.insert(
                block_id,
                qsc_fir::fir::Block {
                    id: block_id,
                    span,
                    ty: output_ty.clone(),
                    stmts: pending_stmts,
                },
            );

            let block_expr_id = assigner.next_expr();
            package.exprs.insert(
                block_expr_id,
                qsc_fir::fir::Expr {
                    id: block_expr_id,
                    span,
                    ty: output_ty,
                    kind: qsc_fir::fir::ExprKind::Block(block_id),
                    exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                        ..qsc_fir::fir::ExecGraphIdx::ZERO,
                },
            );
            block_expr_id
        };

        // Set entry to the synthetic call (optionally wrapped in a block).
        package.entry = Some(entry_expr_id);
        package.entry_exec_graph = Default::default();
    }

    fn validate_runtime_callable_values_for_target(
        fir_store: &qsc_fir::fir::PackageStore,
        target_callable: qsc_fir::fir::StoreItemId,
        functor: FunctorApp,
        args: &Value,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> RuntimeCallableResult<ConcreteTargetSignature> {
        use qsc_fir::fir::{Global, PackageLookup};

        let package = fir_store.get(target_callable.package);
        let Some(Global::Callable(callable_decl)) = package.get_global(target_callable.item) else {
            return Err(Box::new(Error::InvalidRuntimeCallable(target_callable)));
        };
        let required = match (functor.adjoint, functor.controlled > 0) {
            (false, false) => qsc_fir::ty::FunctorSetValue::Empty,
            (true, false) => qsc_fir::ty::FunctorSetValue::Adj,
            (false, true) => qsc_fir::ty::FunctorSetValue::Ctl,
            (true, true) => qsc_fir::ty::FunctorSetValue::CtlAdj,
        };
        if callable_decl.functors.intersect(&required) != required {
            return Err(Box::new(Error::InvalidRuntimeCallableFunctor {
                callable: target_callable,
                functor,
            }));
        }
        let mut formal_input_ty = package.get_pat(callable_decl.input).ty.clone();
        for _ in 0..functor.controlled {
            formal_input_ty = qsc_fir::ty::Ty::Tuple(vec![
                qsc_fir::ty::Ty::Array(Box::new(qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Qubit))),
                formal_input_ty,
            ]);
        }
        let formal_output_ty = callable_decl.output.clone();
        validate_runtime_callable_shapes(args, callable_types)?;
        validate_runtime_argument_structure(args, &formal_input_ty, callable_types)?;
        let target_arrow = instantiate_synthetic_target_arrow(
            target_callable,
            &callable_types[&target_callable],
            &qsc_fir::ty::Arrow {
                kind: callable_decl.kind,
                functors: qsc_fir::ty::FunctorSet::Value(callable_decl.functors),
                input: Box::new(formal_input_ty),
                output: Box::new(formal_output_ty),
            },
            args,
            callable_types,
        )?;
        validate_runtime_callable_args(args, &target_arrow.arrow.input, callable_types)?;
        Ok(target_arrow)
    }

    /// Preserves the ordinary shape and concrete-slot diagnostics before generic
    /// inference examines the complete argument tree.
    fn validate_runtime_argument_structure(
        value: &Value,
        formal_ty: &qsc_fir::ty::Ty,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> RuntimeCallableResult<()> {
        use qsc_fir::ty::Ty;
        match (value, formal_ty) {
            (Value::Tuple(values, _), Ty::Tuple(items)) if values.len() == items.len() => {
                for (value, item) in values.iter().zip(items) {
                    validate_runtime_argument_structure(value, item, callable_types)?;
                }
            }
            (Value::Array(values), Ty::Array(item)) => {
                for value in values.iter() {
                    validate_runtime_argument_structure(value, item, callable_types)?;
                }
            }
            (_, Ty::Tuple(_) | Ty::Array(_) | Ty::Prim(_)) => {
                validate_runtime_callable_args(value, formal_ty, callable_types)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_runtime_callable_shapes(
        value: &Value,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> RuntimeCallableResult<()> {
        match value {
            Value::Array(values) => {
                for value in values.iter() {
                    validate_runtime_callable_shapes(value, callable_types)?;
                }
            }
            Value::Tuple(values, _) => {
                for value in values.iter() {
                    validate_runtime_callable_shapes(value, callable_types)?;
                }
            }
            Value::Closure(closure) => {
                validate_runtime_closure_shape(closure, callable_types)?;
                validate_runtime_callable_functor(closure.id, closure.functor, callable_types)?;
                for capture in closure.fixed_args.iter() {
                    validate_runtime_callable_shapes(capture, callable_types)?;
                }
                validate_runtime_closure_captures(closure, callable_types)?;
            }
            Value::Global(id, functor) => {
                if !callable_types.contains_key(id) {
                    return Err(Box::new(Error::InvalidRuntimeCallable(*id)));
                }
                validate_runtime_callable_functor(*id, *functor, callable_types)?;
            }
            Value::BigInt(_)
            | Value::Bool(_)
            | Value::Double(_)
            | Value::Int(_)
            | Value::Pauli(_)
            | Value::Qubit(_)
            | Value::Range(_)
            | Value::Result(_)
            | Value::String(_)
            | Value::Var(_) => {}
        }
        Ok(())
    }

    fn validate_runtime_closure_captures(
        closure: &qsc_eval::val::Closure,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> RuntimeCallableResult<()> {
        if closure.fixed_args.is_empty() {
            return Ok(());
        }
        let Some(info) = callable_types.get(&closure.id) else {
            return Err(Box::new(Error::InvalidRuntimeCallable(closure.id)));
        };
        let qsc_fir::ty::Ty::Arrow(arrow) = &info.formal_ty else {
            return Err(Box::new(Error::InvalidRuntimeClosure {
                callable: closure.id,
            }));
        };
        let qsc_fir::ty::Ty::Tuple(items) = arrow.input.as_ref() else {
            return Err(Box::new(Error::InvalidRuntimeClosure {
                callable: closure.id,
            }));
        };
        let mut inferred = rustc_hash::FxHashMap::default();
        for (capture, formal_ty) in closure.fixed_args.iter().zip(items) {
            let Some(actual_ty) = value_ty_for_inference(capture, Some(formal_ty), callable_types)
            else {
                return Err(Box::new(Error::RuntimeCallableTypeMismatch {
                    expected: Box::new(formal_ty.clone()),
                    actual: Box::new(qsc_fir::ty::Ty::Err),
                }));
            };
            let mut candidate = inferred.clone();
            let aligned_actual_ty = align_runtime_callable_input(formal_ty, &actual_ty);
            if !infer_generic_ty_args(
                formal_ty,
                &aligned_actual_ty,
                &mut candidate,
                GenericInferenceSource::RuntimeValue,
            ) {
                return Err(Box::new(Error::RuntimeCallableTypeMismatch {
                    expected: Box::new(formal_ty.clone()),
                    actual: Box::new(actual_ty),
                }));
            }
            inferred = candidate;
        }
        Ok(())
    }

    fn validate_runtime_callable_functor(
        callable: qsc_fir::fir::StoreItemId,
        functor: FunctorApp,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> RuntimeCallableResult<()> {
        let Some(info) = callable_types.get(&callable) else {
            return Err(Box::new(Error::InvalidRuntimeCallable(callable)));
        };
        let qsc_fir::ty::Ty::Arrow(arrow) = &info.ty else {
            return Err(Box::new(Error::InvalidRuntimeCallable(callable)));
        };
        let qsc_fir::ty::FunctorSet::Value(declared) = arrow.functors else {
            return Ok(());
        };
        let required = match (functor.adjoint, functor.controlled > 0) {
            (false, false) => qsc_fir::ty::FunctorSetValue::Empty,
            (true, false) => qsc_fir::ty::FunctorSetValue::Adj,
            (false, true) => qsc_fir::ty::FunctorSetValue::Ctl,
            (true, true) => qsc_fir::ty::FunctorSetValue::CtlAdj,
        };
        if declared.intersect(&required) == required {
            Ok(())
        } else {
            Err(Box::new(Error::InvalidRuntimeCallableFunctor {
                callable,
                functor,
            }))
        }
    }

    fn validate_runtime_callable_args(
        value: &Value,
        expected_ty: &qsc_fir::ty::Ty,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> RuntimeCallableResult<()> {
        use qsc_fir::ty::Ty;

        if let Some(structural) = nominal_structural_ty(expected_ty, callable_types) {
            let actual =
                value_ty_for_inference(value, Some(expected_ty), callable_types).unwrap_or(Ty::Err);
            if actual != *expected_ty {
                return Err(Box::new(Error::RuntimeCallableTypeMismatch {
                    expected: Box::new(expected_ty.clone()),
                    actual: Box::new(actual),
                }));
            }
            let unwrapped;
            let value = if let Value::Tuple(values, Some(_)) = value {
                unwrapped = Value::Tuple(values.clone(), None);
                &unwrapped
            } else {
                value
            };
            return validate_runtime_callable_args(value, structural, callable_types);
        }
        if let Ty::Tuple(items) = expected_ty {
            if let Value::Tuple(_, Some(_)) = value {
                let actual = value_ty_for_inference(value, None, callable_types).unwrap_or(Ty::Err);
                return Err(Box::new(Error::RuntimeCallableTypeMismatch {
                    expected: Box::new(expected_ty.clone()),
                    actual: Box::new(actual),
                }));
            }
            let Value::Tuple(values, _) = value else {
                return Err(Box::new(Error::RuntimeCallableArgumentShapeMismatch {
                    expected: Box::new(expected_ty.clone()),
                    actual_len: 1,
                    expected_len: items.len(),
                }));
            };
            if values.len() != items.len() {
                return Err(Box::new(Error::RuntimeCallableArgumentShapeMismatch {
                    expected: Box::new(expected_ty.clone()),
                    actual_len: values.len(),
                    expected_len: items.len(),
                }));
            }
            for (value, item) in values.iter().zip(items) {
                validate_runtime_callable_args(value, item, callable_types)?;
            }
            return Ok(());
        }

        match (value, expected_ty) {
            (Value::Array(values), Ty::Array(item)) => {
                for value in values.iter() {
                    validate_runtime_callable_args(value, item, callable_types)?;
                }
            }
            (Value::Closure(closure), _) => {
                validate_runtime_closure_shape(closure, callable_types)?;
                validate_runtime_callable_type(value, expected_ty, callable_types)?;
            }
            (Value::Global(..), _) => {
                validate_runtime_callable_type(value, expected_ty, callable_types)?;
            }
            (_, Ty::Arrow(_)) => {
                let actual = value_ty_for_inference(value, None, callable_types).unwrap_or(Ty::Err);
                return Err(Box::new(Error::RuntimeCallableTypeMismatch {
                    expected: Box::new(expected_ty.clone()),
                    actual: Box::new(actual),
                }));
            }
            (_, Ty::Prim(_) | Ty::Array(_)) => {
                let actual = value_ty_for_inference(value, Some(expected_ty), callable_types)
                    .unwrap_or(Ty::Err);
                if actual != *expected_ty {
                    return Err(Box::new(Error::RuntimeCallableTypeMismatch {
                        expected: Box::new(expected_ty.clone()),
                        actual: Box::new(actual),
                    }));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_runtime_closure_shape(
        closure: &qsc_eval::val::Closure,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> RuntimeCallableResult<()> {
        if closure.fixed_args.is_empty() {
            return Ok(());
        }
        let Some(info) = callable_types.get(&closure.id) else {
            return Err(Box::new(Error::InvalidRuntimeCallable(closure.id)));
        };
        let qsc_fir::ty::Ty::Arrow(arrow) = &info.formal_ty else {
            return Err(Box::new(Error::InvalidRuntimeClosure {
                callable: closure.id,
            }));
        };
        let qsc_fir::ty::Ty::Tuple(items) = arrow.input.as_ref() else {
            return Err(Box::new(Error::InvalidRuntimeClosure {
                callable: closure.id,
            }));
        };
        if items.len() <= closure.fixed_args.len() {
            return Err(Box::new(Error::InvalidRuntimeClosure {
                callable: closure.id,
            }));
        }
        Ok(())
    }

    fn validate_runtime_callable_type(
        value: &Value,
        expected_ty: &qsc_fir::ty::Ty,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> RuntimeCallableResult<()> {
        let (full_ty, _) = runtime_callable_instantiation(value, expected_ty, callable_types)?;
        let (actual_ty, functor) = match value {
            Value::Closure(closure) => {
                let hints = closure_capture_ty_hints(&full_ty, closure.fixed_args.len())
                    .unwrap_or_default();
                for (capture, hint) in closure.fixed_args.iter().zip(&hints) {
                    validate_runtime_callable_args(capture, hint, callable_types)?;
                }
                (
                    partial_applied_closure_ty(&full_ty, closure.fixed_args.len()),
                    closure.functor,
                )
            }
            Value::Global(_, functor) => (full_ty, *functor),
            _ => return Err(Box::new(Error::NotACallable)),
        };
        let actual_ty = callable_ty_with_runtime_functor(&actual_ty, functor);
        if callable_ty_satisfies_expected(&actual_ty, expected_ty) {
            Ok(())
        } else {
            Err(Box::new(Error::RuntimeCallableTypeMismatch {
                expected: Box::new(expected_ty.clone()),
                actual: Box::new(actual_ty),
            }))
        }
    }

    fn runtime_callable_instantiation(
        value: &Value,
        expected_ty: &qsc_fir::ty::Ty,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> RuntimeCallableResult<(qsc_fir::ty::Ty, RuntimeTypeArgs)> {
        let (id, functor) = match value {
            Value::Global(id, functor) => (*id, *functor),
            Value::Closure(closure) => (closure.id, closure.functor),
            _ => return Err(Box::new(Error::NotACallable)),
        };
        let info = callable_types
            .get(&id)
            .ok_or_else(|| Box::new(Error::InvalidRuntimeCallable(id)))?;
        let inferred = if let Value::Closure(closure) = value {
            infer_closure_generic_args(closure, Some(expected_ty), callable_types)?
        } else {
            let expected_base = callable_ty_before_runtime_functor(expected_ty, functor)
                .ok_or_else(|| {
                    Box::new(Error::RuntimeCallableTypeMismatch {
                        expected: Box::new(expected_ty.clone()),
                        actual: Box::new(info.ty.clone()),
                    })
                })?;
            let expected_base = align_runtime_callable_input(&info.formal_ty, &expected_base);
            let mut inferred = RuntimeTypeArgs::default();
            if !infer_generic_ty_args(
                &info.formal_ty,
                &expected_base,
                &mut inferred,
                GenericInferenceSource::ExpectedType,
            ) {
                return Err(Box::new(Error::RuntimeCallableTypeMismatch {
                    expected: Box::new(expected_ty.clone()),
                    actual: Box::new(info.ty.clone()),
                }));
            }
            inferred
        };
        let inferred = finalize_runtime_type_args(id, info, inferred)?;
        let full_ty = resolve_params_with_inferred(&info.formal_ty, &inferred);
        validate_concrete_runtime_type(id, info, &full_ty)?;
        Ok((full_ty, inferred))
    }

    /// Follows frontend unification's expected/actual direction recursively,
    /// including within callable inputs (rather than introducing variance).
    fn callable_ty_satisfies_expected(
        actual_ty: &qsc_fir::ty::Ty,
        expected_ty: &qsc_fir::ty::Ty,
    ) -> bool {
        use qsc_fir::ty::{FunctorSet, Ty};

        let actual_ty = align_runtime_callable_input(expected_ty, actual_ty);
        match (&actual_ty, expected_ty) {
            (Ty::Arrow(actual), Ty::Arrow(expected)) => {
                actual.kind == expected.kind
                    && callable_ty_satisfies_expected(&actual.input, &expected.input)
                    && callable_ty_satisfies_expected(&actual.output, &expected.output)
                    && match (actual.functors, expected.functors) {
                        (FunctorSet::Value(actual), FunctorSet::Value(expected)) => {
                            actual.intersect(&expected) == expected
                        }
                        (actual, expected) => actual == expected,
                    }
            }
            (Ty::Array(actual), Ty::Array(expected)) => {
                callable_ty_satisfies_expected(actual, expected)
            }
            (Ty::Tuple(actual), Ty::Tuple(expected)) => {
                actual.len() == expected.len()
                    && actual
                        .iter()
                        .zip(expected)
                        .all(|(actual, expected)| callable_ty_satisfies_expected(actual, expected))
            }
            (actual, expected) => actual == expected,
        }
    }

    fn align_runtime_callable_input(
        formal_ty: &qsc_fir::ty::Ty,
        actual_ty: &qsc_fir::ty::Ty,
    ) -> qsc_fir::ty::Ty {
        let (qsc_fir::ty::Ty::Arrow(formal), qsc_fir::ty::Ty::Arrow(actual)) =
            (formal_ty, actual_ty)
        else {
            return actual_ty.clone();
        };
        let aligned_input = match (formal.input.as_ref(), actual.input.as_ref()) {
            (qsc_fir::ty::Ty::Tuple(items), actual_input)
                if items.len() == 1 && !matches!(actual_input, qsc_fir::ty::Ty::Tuple(_)) =>
            {
                qsc_fir::ty::Ty::Tuple(vec![actual_input.clone()])
            }
            (formal_input, qsc_fir::ty::Ty::Tuple(items))
                if items.len() == 1 && !matches!(formal_input, qsc_fir::ty::Ty::Tuple(_)) =>
            {
                items[0].clone()
            }
            _ => return actual_ty.clone(),
        };
        qsc_fir::ty::Ty::Arrow(Box::new(qsc_fir::ty::Arrow {
            kind: actual.kind,
            input: Box::new(aligned_input),
            output: actual.output.clone(),
            functors: actual.functors,
        }))
    }

    /// Infers concrete generic arguments for the synthetic target invocation and
    /// returns the target's concrete signature.
    ///
    /// The synthetic entry is built before the normal monomorphization pass can
    /// specialize the target for these runtime arguments. Instantiating the
    /// arrow here keeps the synthetic call structurally concrete, so later FIR
    /// passes do not see unresolved type or functor parameters.
    fn instantiate_synthetic_target_arrow(
        callable: qsc_fir::fir::StoreItemId,
        info: &CallableValueInfo,
        formal_arrow: &qsc_fir::ty::Arrow,
        args: &Value,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> RuntimeCallableResult<ConcreteTargetSignature> {
        let inferred =
            infer_target_generic_args(callable, info, &formal_arrow.input, args, callable_types)?;
        let generic_args = (0..info.generics.len())
            .map(|idx| inferred[&qsc_fir::ty::ParamId::from(idx)].clone())
            .collect();
        let instantiated_arrow = qsc_fir::ty::Arrow {
            input: Box::new(resolve_params_with_inferred(&formal_arrow.input, &inferred)),
            output: Box::new(resolve_params_with_inferred(
                &formal_arrow.output,
                &inferred,
            )),
            ..*formal_arrow
        };
        validate_concrete_runtime_type(
            callable,
            info,
            &qsc_fir::ty::Ty::Arrow(Box::new(instantiated_arrow.clone())),
        )?;
        Ok(ConcreteTargetSignature {
            generic_args,
            inferred,
            arrow: instantiated_arrow,
        })
    }

    /// Builds an args expression matching the target's input type.
    ///
    /// Requires validated tuple shapes and FIR-lowerable runtime values.
    fn build_synthetic_args(
        package: &mut qsc_fir::fir::Package,
        assigner: &mut qsc_fir::assigner::Assigner,
        input_ty: &qsc_fir::ty::Ty,
        args: &Value,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
        pending_stmts: &mut Vec<qsc_fir::fir::StmtId>,
    ) -> qsc_fir::fir::ExprId {
        match input_ty {
            qsc_fir::ty::Ty::Tuple(elem_tys) if elem_tys.is_empty() => {
                // Unit input — create empty tuple expression.
                let expr_id = assigner.next_expr();
                package.exprs.insert(
                    expr_id,
                    qsc_fir::fir::Expr {
                        id: expr_id,
                        span: package.synthetic_span(),
                        ty: qsc_fir::ty::Ty::Tuple(Vec::new()),
                        kind: qsc_fir::fir::ExprKind::Tuple(Vec::new()),
                        exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                            ..qsc_fir::fir::ExecGraphIdx::ZERO,
                    },
                );
                expr_id
            }
            qsc_fir::ty::Ty::Tuple(elem_tys) => {
                let Value::Tuple(arg_elems, _) = args else {
                    unreachable!("validated tuple argument");
                };
                assert_eq!(elem_tys.len(), arg_elems.len(), "validated tuple arity");

                // Element-wise matching: lower each arg against its declared type.
                let mut elem_ids = Vec::with_capacity(elem_tys.len());
                for (elem_ty, arg_val) in elem_tys.iter().zip(arg_elems.iter()) {
                    elem_ids.push(build_synthetic_args(
                        package,
                        assigner,
                        elem_ty,
                        arg_val,
                        callable_types,
                        pending_stmts,
                    ));
                }
                let expr_id = assigner.next_expr();
                package.exprs.insert(
                    expr_id,
                    qsc_fir::fir::Expr {
                        id: expr_id,
                        span: package.synthetic_span(),
                        ty: input_ty.clone(),
                        kind: qsc_fir::fir::ExprKind::Tuple(elem_ids),
                        exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                            ..qsc_fir::fir::ExecGraphIdx::ZERO,
                    },
                );
                expr_id
            }
            _ => lower_value_to_expr(
                package,
                assigner,
                args,
                Some(input_ty),
                callable_types,
                pending_stmts,
            ),
        }
    }

    /// Replaces UDT types with their pure structural FIR type, recursively.
    ///
    /// Synthetic call construction operates on the post-erasure shape so callable
    /// fields hidden inside UDTs can be discovered by defunctionalization.
    fn resolve_udt_ty(
        fir_store: &qsc_fir::fir::PackageStore,
        ty: &qsc_fir::ty::Ty,
    ) -> qsc_fir::ty::Ty {
        match ty {
            qsc_fir::ty::Ty::Udt(qsc_fir::fir::Res::Item(item_id)) => {
                let package = fir_store.get(item_id.package);
                let item = package
                    .items
                    .get(item_id.item)
                    .expect("UDT item should exist");
                let qsc_fir::fir::ItemKind::Ty(_, udt) = &item.kind else {
                    return ty.clone();
                };
                resolve_udt_ty(fir_store, &udt.get_pure_ty())
            }
            qsc_fir::ty::Ty::Tuple(elems) => qsc_fir::ty::Ty::Tuple(
                elems
                    .iter()
                    .map(|elem| resolve_udt_ty(fir_store, elem))
                    .collect(),
            ),
            qsc_fir::ty::Ty::Array(elem) => {
                qsc_fir::ty::Ty::Array(Box::new(resolve_udt_ty(fir_store, elem)))
            }
            qsc_fir::ty::Ty::Arrow(arrow) => qsc_fir::ty::Ty::Arrow(Box::new(qsc_fir::ty::Arrow {
                kind: arrow.kind,
                input: Box::new(resolve_udt_ty(fir_store, &arrow.input)),
                output: Box::new(resolve_udt_ty(fir_store, &arrow.output)),
                functors: arrow.functors,
            })),
            _ => ty.clone(),
        }
    }

    /// Builds concrete generic args from a target callable's input and the
    /// runtime argument values supplied to the synthetic entry.
    ///
    /// After merging available evidence, unobserved type leaves default to `Unit`
    /// and functor parameters retain their declared lower bounds. Admission fails
    /// if the resulting candidates do not satisfy every retained constraint.
    fn infer_target_generic_args(
        callable: qsc_fir::fir::StoreItemId,
        info: &CallableValueInfo,
        formal_input_ty: &qsc_fir::ty::Ty,
        args: &Value,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> RuntimeCallableResult<RuntimeTypeArgs> {
        if info.parameters.is_empty() {
            return Ok(Default::default());
        }
        let mut arg_map = rustc_hash::FxHashMap::default();
        let actual_input_ty = value_ty_for_inference(args, Some(formal_input_ty), callable_types)
            .unwrap_or(qsc_fir::ty::Ty::Err);
        if !infer_generic_ty_args(
            formal_input_ty,
            &actual_input_ty,
            &mut arg_map,
            GenericInferenceSource::RuntimeValue,
        ) {
            return Err(Box::new(Error::RuntimeCallableTypeMismatch {
                expected: Box::new(formal_input_ty.clone()),
                actual: Box::new(actual_input_ty),
            }));
        }
        for (idx, param) in info.generics.iter().enumerate() {
            let id = qsc_fir::ty::ParamId::from(idx);
            if let (
                qsc_fir::ty::TypeParameter::Functor(required),
                Some(qsc_fir::ty::GenericArg::Functor(qsc_fir::ty::FunctorSet::Value(actual))),
            ) = (param, arg_map.get(&id))
                && actual.intersect(required) != *required
            {
                let mut required_args = arg_map.clone();
                required_args.insert(
                    id,
                    qsc_fir::ty::GenericArg::Functor(qsc_fir::ty::FunctorSet::Value(*required)),
                );
                return Err(Box::new(Error::RuntimeCallableTypeMismatch {
                    expected: Box::new(resolve_params_with_inferred(
                        formal_input_ty,
                        &required_args,
                    )),
                    actual: Box::new(actual_input_ty),
                }));
            }
        }

        finalize_runtime_type_args(callable, info, arg_map)
    }

    /// Defaults only the unobserved leaves, after evidence from the whole value
    /// tree has been merged. `Infer` is a local placeholder for an empty array's
    /// unknown element type and must never reach the synthetic FIR.
    fn default_unresolved_generic_arg(arg: &mut qsc_fir::ty::GenericArg) {
        fn default_ty(ty: &mut qsc_fir::ty::Ty) {
            use qsc_fir::ty::{FunctorSet, FunctorSetValue, Ty};
            match ty {
                Ty::Param(_) | Ty::Infer(_) => *ty = Ty::UNIT,
                Ty::Array(item) => default_ty(item),
                Ty::Tuple(items) => items.iter_mut().for_each(default_ty),
                Ty::Arrow(arrow) => {
                    default_ty(&mut arrow.input);
                    default_ty(&mut arrow.output);
                    if !matches!(arrow.functors, FunctorSet::Value(_)) {
                        arrow.functors = FunctorSet::Value(FunctorSetValue::Empty);
                    }
                }
                Ty::Err | Ty::Prim(_) | Ty::Udt(_) => {}
            }
        }
        if let qsc_fir::ty::GenericArg::Ty(ty) = arg {
            default_ty(ty);
        }
    }

    /// Returns true when a type contains unresolved or error leaves.
    fn ty_contains_unresolved(ty: &qsc_fir::ty::Ty) -> bool {
        match ty {
            qsc_fir::ty::Ty::Param(_) | qsc_fir::ty::Ty::Infer(_) | qsc_fir::ty::Ty::Err => true,
            qsc_fir::ty::Ty::Array(item) => ty_contains_unresolved(item),
            qsc_fir::ty::Ty::Arrow(arrow) => {
                !matches!(arrow.functors, qsc_fir::ty::FunctorSet::Value(_))
                    || ty_contains_unresolved(&arrow.input)
                    || ty_contains_unresolved(&arrow.output)
            }
            qsc_fir::ty::Ty::Tuple(items) => items.iter().any(ty_contains_unresolved),
            qsc_fir::ty::Ty::Prim(_) | qsc_fir::ty::Ty::Udt(_) => false,
        }
    }

    /// Reconstructs the best FIR type shape available from an interpreter value.
    ///
    /// Supplies generic inference and runtime validation. Identities that cannot
    /// be lowered into synthetic FIR, such as qubits or dynamic variables, can
    /// still expose enough type information to instantiate the target arrow.
    /// Callable values use the expected type as generic parameter evidence while
    /// retaining declared concrete types and capabilities for validation.
    fn value_ty_for_inference(
        value: &Value,
        expected_ty: Option<&qsc_fir::ty::Ty>,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> Option<qsc_fir::ty::Ty> {
        // Concrete declaration slots retain their nominal type even when the
        // runtime representation has no tag. A generic slot cannot invent it.
        if !matches!(value, Value::Tuple(_, Some(_)))
            && let Some(expected) = expected_ty
            && let Some(structural) = nominal_structural_ty(expected, callable_types)
        {
            let actual = value_ty_for_inference(value, Some(structural), callable_types)?;
            return callable_ty_satisfies_expected(&actual, structural).then(|| expected.clone());
        }
        match value {
            Value::Int(_) => Some(qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Int)),
            Value::Double(_) => Some(qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Double)),
            Value::Bool(_) => Some(qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Bool)),
            Value::BigInt(_) => Some(qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::BigInt)),
            Value::Pauli(_) => Some(qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Pauli)),
            Value::Qubit(_) => Some(qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Qubit)),
            Value::Range(_) => Some(qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Range)),
            Value::Result(_) => Some(qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Result)),
            Value::String(_) => Some(qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::String)),
            Value::Tuple(values, nominal) => {
                if let Some(id) = nominal {
                    let ty = qsc_fir::ty::Ty::Udt(qsc_fir::fir::Res::Item(qsc_fir::fir::ItemId {
                        package: id.package,
                        item: id.item,
                    }));
                    if callable_types
                        .values()
                        .any(|info| !info.nominal_types.is_empty())
                    {
                        return Some(ty);
                    }
                }
                let expected_items = match expected_ty {
                    Some(qsc_fir::ty::Ty::Tuple(items)) if items.len() == values.len() => {
                        Some(items.as_slice())
                    }
                    _ => None,
                };
                values
                    .iter()
                    .enumerate()
                    .map(|(idx, value)| {
                        value_ty_for_inference(
                            value,
                            expected_items.map(|items| &items[idx]),
                            callable_types,
                        )
                    })
                    .collect::<Option<Vec<_>>>()
                    .map(qsc_fir::ty::Ty::Tuple)
            }
            Value::Array(values) => {
                let expected_item = match expected_ty {
                    Some(qsc_fir::ty::Ty::Array(item)) => Some(item.as_ref()),
                    _ => None,
                };
                let mut item_ty = expected_item
                    .cloned()
                    .unwrap_or(qsc_fir::ty::Ty::Infer(Default::default()));
                // An expected concrete type is a hint, not evidence: retain
                // incompatible values so validation can report the mismatch.
                if let Some((first, rest)) = values.split_first() {
                    item_ty = value_ty_for_inference(first, expected_item, callable_types)?;
                    for value in rest {
                        let next = value_ty_for_inference(value, expected_item, callable_types)?;
                        item_ty = merge_inferred_tys(&item_ty, &next)?;
                    }
                }
                Some(qsc_fir::ty::Ty::Array(Box::new(item_ty)))
            }
            Value::Global(id, functor) => {
                let info = callable_types.get(id)?;
                let expected_base_ty = expected_ty.and_then(|expected_ty| {
                    callable_ty_before_runtime_functor(expected_ty, *functor)
                });
                if let Some(expected_ty) = expected_base_ty.as_ref()
                    && let Some(generic_args) = infer_partial_global_generic_args(
                        &info.generics,
                        &info.formal_ty,
                        expected_ty,
                    )
                {
                    let base_ty = instantiate_formal_ty(&info.formal_ty, &generic_args);
                    return Some(callable_ty_with_runtime_functor(&base_ty, *functor));
                }
                Some(callable_ty_with_runtime_functor(&info.ty, *functor))
            }
            Value::Closure(closure) => {
                closure_ty_for_inference(closure, expected_ty, callable_types)
            }
            Value::Var(var) => Some(qsc_fir::ty::Ty::Prim(match var.ty {
                qsc_eval::val::VarTy::Boolean => qsc_fir::ty::Prim::Bool,
                qsc_eval::val::VarTy::Integer => qsc_fir::ty::Prim::Int,
                qsc_eval::val::VarTy::Double => qsc_fir::ty::Prim::Double,
                qsc_eval::val::VarTy::Qubit => qsc_fir::ty::Prim::Qubit,
            })),
        }
    }

    /// Lowers an interpreter `Value` into a FIR expression for the synthetic entry.
    ///
    /// Scalar values become literals, aggregate values are lowered recursively, and
    /// callable values are represented by global or closure variables with their
    /// runtime functor application preserved.
    #[allow(clippy::too_many_lines)]
    fn lower_value_to_expr(
        package: &mut qsc_fir::fir::Package,
        assigner: &mut qsc_fir::assigner::Assigner,
        value: &Value,
        expected_ty: Option<&qsc_fir::ty::Ty>,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
        pending_stmts: &mut Vec<qsc_fir::fir::StmtId>,
    ) -> qsc_fir::fir::ExprId {
        let (kind, ty) = match value {
            Value::Int(n) => (
                qsc_fir::fir::ExprKind::Lit(qsc_fir::fir::Lit::Int(*n)),
                qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Int),
            ),
            Value::Double(d) => (
                qsc_fir::fir::ExprKind::Lit(qsc_fir::fir::Lit::Double(*d)),
                qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Double),
            ),
            Value::Bool(b) => (
                qsc_fir::fir::ExprKind::Lit(qsc_fir::fir::Lit::Bool(*b)),
                qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Bool),
            ),
            Value::BigInt(b) => (
                qsc_fir::fir::ExprKind::Lit(qsc_fir::fir::Lit::BigInt(b.clone())),
                qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::BigInt),
            ),
            Value::Pauli(p) => (
                qsc_fir::fir::ExprKind::Lit(qsc_fir::fir::Lit::Pauli(*p)),
                qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Pauli),
            ),
            Value::Result(qsc_eval::val::Result::Val(b)) => (
                qsc_fir::fir::ExprKind::Lit(qsc_fir::fir::Lit::Result(if *b {
                    qsc_fir::fir::Result::One
                } else {
                    qsc_fir::fir::Result::Zero
                })),
                qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Result),
            ),
            Value::String(s) => (
                qsc_fir::fir::ExprKind::String(vec![qsc_fir::fir::StringComponent::Lit(s.clone())]),
                qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::String),
            ),
            Value::Tuple(vs, _) => {
                let elem_ty_hints = match expected_ty {
                    Some(qsc_fir::ty::Ty::Tuple(elem_tys)) if elem_tys.len() == vs.len() => {
                        Some(elem_tys)
                    }
                    _ => None,
                };
                let mut lowered_ids = Vec::with_capacity(vs.len());
                let mut lowered_tys = Vec::with_capacity(vs.len());
                for (idx, v) in vs.iter().enumerate() {
                    let id = lower_value_to_expr(
                        package,
                        assigner,
                        v,
                        elem_ty_hints.map(|elem_tys| &elem_tys[idx]),
                        callable_types,
                        pending_stmts,
                    );
                    lowered_tys.push(package.exprs.get(id).expect("just inserted").ty.clone());
                    lowered_ids.push(id);
                }
                (
                    qsc_fir::fir::ExprKind::Tuple(lowered_ids),
                    qsc_fir::ty::Ty::Tuple(lowered_tys),
                )
            }
            Value::Array(vs) => {
                // Decompose the declared array type so empty (and nested-empty)
                // arrays can recover their real element type instead of `Ty::Err`.
                let inner_hint: Option<&qsc_fir::ty::Ty> = match expected_ty {
                    Some(qsc_fir::ty::Ty::Array(inner)) => Some(inner.as_ref()),
                    _ => None,
                };
                let mut lowered_ids = Vec::with_capacity(vs.len());
                for v in vs.iter() {
                    lowered_ids.push(lower_value_to_expr(
                        package,
                        assigner,
                        v,
                        inner_hint,
                        callable_types,
                        pending_stmts,
                    ));
                }
                let elem_ty = match lowered_ids.first() {
                    Some(id) => package.exprs.get(*id).expect("just inserted").ty.clone(),
                    // For an empty array the element type is the declared array's
                    // element type, not the nested element hint.
                    None => inner_hint.cloned().unwrap_or(qsc_fir::ty::Ty::Err),
                };
                (
                    qsc_fir::fir::ExprKind::Array(lowered_ids),
                    qsc_fir::ty::Ty::Array(Box::new(elem_ty)),
                )
            }
            Value::Range(r) => {
                let lower_opt = |opt: Option<i64>,
                                 pkg: &mut qsc_fir::fir::Package,
                                 a: &mut qsc_fir::assigner::Assigner|
                 -> Option<qsc_fir::fir::ExprId> {
                    opt.map(|n| {
                        let id = a.next_expr();
                        let span = pkg.synthetic_span();
                        pkg.exprs.insert(
                            id,
                            qsc_fir::fir::Expr {
                                id,
                                span,
                                ty: qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Int),
                                kind: qsc_fir::fir::ExprKind::Lit(qsc_fir::fir::Lit::Int(n)),
                                exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                                    ..qsc_fir::fir::ExecGraphIdx::ZERO,
                            },
                        );
                        id
                    })
                };
                let start = lower_opt(r.start, package, assigner);
                let step = lower_opt(Some(r.step), package, assigner);
                let end = lower_opt(r.end, package, assigner);
                (
                    qsc_fir::fir::ExprKind::Range(start, step, end),
                    qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Range),
                )
            }
            Value::Global(id, functor) => {
                return lower_global_to_expr(package, assigner, *id, *functor, callable_types);
            }
            Value::Closure(c) => {
                return lower_closure_to_expr(package, assigner, c, callable_types, pending_stmts);
            }
            _ => panic!("cannot lower {value:?} to FIR expression"),
        };

        let expr_id = assigner.next_expr();
        package.exprs.insert(
            expr_id,
            qsc_fir::fir::Expr {
                id: expr_id,
                span: package.synthetic_span(),
                ty,
                kind,
                exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                    ..qsc_fir::fir::ExecGraphIdx::ZERO,
            },
        );
        expr_id
    }

    /// Lowers a global callable value to a FIR variable expression.
    ///
    /// The callable's stored `FunctorApp` is applied as FIR functor wrappers so
    /// adjoint and controlled runtime values survive the synthetic entry path.
    fn lower_global_to_expr(
        package: &mut qsc_fir::fir::Package,
        assigner: &mut qsc_fir::assigner::Assigner,
        id: qsc_fir::fir::StoreItemId,
        functor: FunctorApp,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> qsc_fir::fir::ExprId {
        let ty = callable_types
            .get(&id)
            .expect("Global callable type must be pre-computed")
            .ty
            .clone();
        let expr_id = assigner.next_expr();
        package.exprs.insert(
            expr_id,
            qsc_fir::fir::Expr {
                id: expr_id,
                span: package.synthetic_span(),
                ty: ty.clone(),
                kind: qsc_fir::fir::ExprKind::Var(
                    qsc_fir::fir::Res::Item(qsc_fir::fir::ItemId {
                        package: id.package,
                        item: id.item,
                    }),
                    Vec::new(),
                ),
                exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                    ..qsc_fir::fir::ExecGraphIdx::ZERO,
            },
        );
        wrap_expr_with_functor_app(package, assigner, expr_id, &ty, functor)
    }

    fn instantiate_formal_ty(
        formal_ty: &qsc_fir::ty::Ty,
        generic_args: &[qsc_fir::ty::GenericArg],
    ) -> qsc_fir::ty::Ty {
        let inferred = generic_args
            .iter()
            .enumerate()
            .map(|(idx, arg)| (qsc_fir::ty::ParamId::from(idx), arg.clone()))
            .collect();
        resolve_params_with_inferred(formal_ty, &inferred)
    }

    fn callable_ty_before_runtime_functor(
        ty: &qsc_fir::ty::Ty,
        functor: FunctorApp,
    ) -> Option<qsc_fir::ty::Ty> {
        let mut current = ty.clone();
        for _ in 0..functor.controlled {
            let qsc_fir::ty::Ty::Arrow(arrow) = current else {
                return None;
            };
            let qsc_fir::ty::Ty::Tuple(inputs) = arrow.input.as_ref() else {
                return None;
            };
            let [controls, input] = inputs.as_slice() else {
                return None;
            };
            if !matches!(
                controls,
                qsc_fir::ty::Ty::Array(item)
                    if matches!(item.as_ref(), qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Qubit))
            ) {
                return None;
            }
            current = qsc_fir::ty::Ty::Arrow(Box::new(qsc_fir::ty::Arrow {
                kind: arrow.kind,
                input: Box::new(input.clone()),
                output: arrow.output.clone(),
                functors: arrow.functors,
            }));
        }
        Some(current)
    }

    fn callable_ty_with_runtime_functor(
        ty: &qsc_fir::ty::Ty,
        functor: FunctorApp,
    ) -> qsc_fir::ty::Ty {
        let mut current = ty.clone();
        for _ in 0..functor.controlled {
            current = controlled_callable_ty(&current);
        }
        current
    }

    /// Collects provisional generic arguments for a reference to a generic global
    /// callable by matching its formal type against the type expected at
    /// the use site.
    ///
    /// Returns `None` when the callable is non-generic or the two types do not
    /// structurally match; otherwise returns one `GenericArg` per declared
    /// parameter in declaration order. Capability validation is performed on the
    /// reconstructed callable, in the actual-to-expected direction. This is only
    /// evidence gathering; `runtime_callable_instantiation` owns final admission.
    fn infer_partial_global_generic_args(
        generics: &[qsc_fir::ty::TypeParameter],
        formal_ty: &qsc_fir::ty::Ty,
        expected_ty: &qsc_fir::ty::Ty,
    ) -> Option<Vec<qsc_fir::ty::GenericArg>> {
        if generics.is_empty() {
            return None;
        }
        let mut arg_map = rustc_hash::FxHashMap::default();
        if !infer_generic_ty_args(
            formal_ty,
            expected_ty,
            &mut arg_map,
            GenericInferenceSource::ExpectedType,
        ) {
            return None;
        }
        generics
            .iter()
            .enumerate()
            .map(
                |(idx, param)| match (param, arg_map.get(&qsc_fir::ty::ParamId::from(idx))) {
                    (
                        qsc_fir::ty::TypeParameter::Ty { .. },
                        Some(qsc_fir::ty::GenericArg::Ty(ty)),
                    ) if !ty_contains_unresolved(ty) => {
                        Some(qsc_fir::ty::GenericArg::Ty(ty.clone()))
                    }
                    (
                        qsc_fir::ty::TypeParameter::Functor(_),
                        Some(qsc_fir::ty::GenericArg::Functor(functors)),
                    ) => Some(qsc_fir::ty::GenericArg::Functor(*functors)),
                    _ => None,
                },
            )
            .collect()
    }

    #[derive(Clone, Copy)]
    enum GenericInferenceSource {
        RuntimeValue,
        /// Collect parameter evidence only; validate the reconstructed callable
        /// separately instead of asking the expected type to supply its functors.
        ExpectedType,
    }

    /// Structurally matches a formal type against evidence, recording each
    /// type/functor parameter binding in `arg_map`. Runtime value evidence must
    /// also supply concrete functors required by the formal type.
    ///
    /// Returns `false` on any structural mismatch or on conflicting bindings for
    /// the same parameter (see [`record_inferred_arg`]).
    fn infer_generic_ty_args(
        formal: &qsc_fir::ty::Ty,
        actual: &qsc_fir::ty::Ty,
        arg_map: &mut rustc_hash::FxHashMap<qsc_fir::ty::ParamId, qsc_fir::ty::GenericArg>,
        source: GenericInferenceSource,
    ) -> bool {
        if matches!(
            actual,
            qsc_fir::ty::Ty::Param(_) | qsc_fir::ty::Ty::Infer(_)
        ) {
            return true;
        }
        match (formal, actual) {
            (qsc_fir::ty::Ty::Param(_), qsc_fir::ty::Ty::Param(_) | qsc_fir::ty::Ty::Infer(_))
            | (qsc_fir::ty::Ty::Err, qsc_fir::ty::Ty::Err) => true,
            (qsc_fir::ty::Ty::Param(param), _) => {
                record_inferred_arg(*param, qsc_fir::ty::GenericArg::Ty(actual.clone()), arg_map)
            }
            (qsc_fir::ty::Ty::Array(formal), qsc_fir::ty::Ty::Array(actual)) => {
                infer_generic_ty_args(formal, actual, arg_map, source)
            }
            (qsc_fir::ty::Ty::Arrow(formal), qsc_fir::ty::Ty::Arrow(actual)) => {
                formal.kind == actual.kind
                    && infer_generic_ty_args(&formal.input, &actual.input, arg_map, source)
                    && infer_generic_ty_args(&formal.output, &actual.output, arg_map, source)
                    && infer_generic_functor_args(formal.functors, actual.functors, arg_map, source)
            }
            (qsc_fir::ty::Ty::Tuple(formal), qsc_fir::ty::Ty::Tuple(actual))
                if formal.len() == actual.len() =>
            {
                formal
                    .iter()
                    .zip(actual)
                    .all(|(formal, actual)| infer_generic_ty_args(formal, actual, arg_map, source))
            }
            (qsc_fir::ty::Ty::Prim(formal), qsc_fir::ty::Ty::Prim(actual)) => formal == actual,
            (qsc_fir::ty::Ty::Udt(formal), qsc_fir::ty::Ty::Udt(actual)) => formal == actual,
            (qsc_fir::ty::Ty::Infer(formal), qsc_fir::ty::Ty::Infer(actual)) => formal == actual,
            _ => false,
        }
    }

    /// Unifies a formal functor set against an actual one, recording the binding
    /// when the formal side is a functor parameter. Runtime value evidence must
    /// include concrete functors required by the formal type; expected-type
    /// evidence only binds parameters and does not validate capabilities.
    /// An unresolved actual set supplies no evidence and does not bind a parameter.
    fn infer_generic_functor_args(
        formal: qsc_fir::ty::FunctorSet,
        actual: qsc_fir::ty::FunctorSet,
        arg_map: &mut rustc_hash::FxHashMap<qsc_fir::ty::ParamId, qsc_fir::ty::GenericArg>,
        source: GenericInferenceSource,
    ) -> bool {
        if matches!(
            actual,
            qsc_fir::ty::FunctorSet::Param(_) | qsc_fir::ty::FunctorSet::Infer(_)
        ) {
            return true;
        }
        match formal {
            qsc_fir::ty::FunctorSet::Param(param) => {
                record_inferred_arg(param, qsc_fir::ty::GenericArg::Functor(actual), arg_map)
            }
            qsc_fir::ty::FunctorSet::Value(required) => {
                matches!(source, GenericInferenceSource::ExpectedType)
                    || matches!(actual, qsc_fir::ty::FunctorSet::Value(actual)
                        if actual.intersect(&required) == required)
            }
            qsc_fir::ty::FunctorSet::Infer(_) => formal == actual,
        }
    }

    /// Records the inferred argument for a generic parameter, returning whether
    /// it is consistent. Repeated type bindings merge unobserved array elements
    /// without discarding any concrete evidence.
    fn record_inferred_arg(
        param: qsc_fir::ty::ParamId,
        arg: qsc_fir::ty::GenericArg,
        arg_map: &mut rustc_hash::FxHashMap<qsc_fir::ty::ParamId, qsc_fir::ty::GenericArg>,
    ) -> bool {
        if let Some(existing) = arg_map.get_mut(&param) {
            if let (qsc_fir::ty::GenericArg::Ty(existing), qsc_fir::ty::GenericArg::Ty(ty)) =
                (&mut *existing, &arg)
            {
                if let Some(merged) = merge_inferred_tys(existing, ty) {
                    *existing = merged;
                    return true;
                }
                return false;
            }
            *existing == arg
        } else {
            arg_map.insert(param, arg);
            true
        }
    }

    /// Merges independent value evidence, distinguishing unknown leaves from
    /// incompatible concrete shapes. Callable evidence retains only capabilities
    /// shared by every value; required capabilities are validated separately.
    fn merge_inferred_tys(
        first: &qsc_fir::ty::Ty,
        second: &qsc_fir::ty::Ty,
    ) -> Option<qsc_fir::ty::Ty> {
        use qsc_fir::ty::{Arrow, FunctorSet, Ty};
        match (first, second) {
            (Ty::Param(_) | Ty::Infer(_), _) => Some(second.clone()),
            (_, Ty::Param(_) | Ty::Infer(_)) => Some(first.clone()),
            (Ty::Array(first), Ty::Array(second)) => {
                merge_inferred_tys(first, second).map(|item| Ty::Array(Box::new(item)))
            }
            (Ty::Arrow(first), Ty::Arrow(second)) if first.kind == second.kind => {
                let functors = match (first.functors, second.functors) {
                    (FunctorSet::Value(first), FunctorSet::Value(second)) => {
                        FunctorSet::Value(first.intersect(&second))
                    }
                    (first, second) if first == second => first,
                    _ => return None,
                };
                Some(Ty::Arrow(Box::new(Arrow {
                    kind: first.kind,
                    input: Box::new(merge_inferred_tys(&first.input, &second.input)?),
                    output: Box::new(merge_inferred_tys(&first.output, &second.output)?),
                    functors,
                })))
            }
            (Ty::Tuple(first), Ty::Tuple(second)) if first.len() == second.len() => first
                .iter()
                .zip(second)
                .map(|(first, second)| merge_inferred_tys(first, second))
                .collect::<Option<Vec<_>>>()
                .map(Ty::Tuple),
            _ if first == second => Some(first.clone()),
            _ => None,
        }
    }

    /// Wraps a callable expression with the FIR functor operations in `functor`.
    ///
    /// Adjoint is applied before each controlled application to match the runtime
    /// `FunctorApp` representation used by interpreter values.
    fn wrap_expr_with_functor_app(
        package: &mut qsc_fir::fir::Package,
        assigner: &mut qsc_fir::assigner::Assigner,
        expr_id: qsc_fir::fir::ExprId,
        ty: &qsc_fir::ty::Ty,
        functor: FunctorApp,
    ) -> qsc_fir::fir::ExprId {
        let mut current_id = expr_id;
        let mut current_ty = ty.clone();
        if functor.adjoint {
            current_id = wrap_expr_with_functor(
                package,
                assigner,
                current_id,
                &current_ty,
                qsc_fir::fir::Functor::Adj,
            );
        }
        for _ in 0..functor.controlled {
            current_ty = controlled_callable_ty(&current_ty);
            current_id = wrap_expr_with_functor(
                package,
                assigner,
                current_id,
                &current_ty,
                qsc_fir::fir::Functor::Ctl,
            );
        }
        current_id
    }

    fn controlled_callable_ty(ty: &qsc_fir::ty::Ty) -> qsc_fir::ty::Ty {
        let qsc_fir::ty::Ty::Arrow(arrow) = ty else {
            panic!("controlled callable should have an arrow type, found {ty:?}");
        };
        qsc_fir::ty::Ty::Arrow(Box::new(qsc_fir::ty::Arrow {
            kind: arrow.kind,
            input: Box::new(qsc_fir::ty::Ty::Tuple(vec![
                qsc_fir::ty::Ty::Array(Box::new(qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Qubit))),
                arrow.input.as_ref().clone(),
            ])),
            output: arrow.output.clone(),
            functors: arrow.functors,
        }))
    }

    /// Creates a FIR unary functor expression around an existing callable expression.
    fn wrap_expr_with_functor(
        package: &mut qsc_fir::fir::Package,
        assigner: &mut qsc_fir::assigner::Assigner,
        inner_id: qsc_fir::fir::ExprId,
        ty: &qsc_fir::ty::Ty,
        functor: qsc_fir::fir::Functor,
    ) -> qsc_fir::fir::ExprId {
        let expr_id = assigner.next_expr();
        package.exprs.insert(
            expr_id,
            qsc_fir::fir::Expr {
                id: expr_id,
                span: package.synthetic_span(),
                ty: ty.clone(),
                kind: qsc_fir::fir::ExprKind::UnOp(qsc_fir::fir::UnOp::Functor(functor), inner_id),
                exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                    ..qsc_fir::fir::ExecGraphIdx::ZERO,
            },
        );
        expr_id
    }

    /// Lowers a closure value to a FIR expression for the synthetic entry.
    ///
    /// A captureless closure becomes a `Var` reference to its underlying callable.
    /// A capturing closure becomes an `ExprKind::Closure` value: each captured value
    /// is lowered and bound to a fresh local (collected in `pending_stmts`), and the
    /// closure expression references those locals so partial evaluation rebuilds the
    /// captured arguments in their original leading order. The runtime functor
    /// application is preserved in both cases.
    fn lower_closure_to_expr(
        package: &mut qsc_fir::fir::Package,
        assigner: &mut qsc_fir::assigner::Assigner,
        closure: &qsc_eval::val::Closure,
        callable_types: &rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
        pending_stmts: &mut Vec<qsc_fir::fir::StmtId>,
    ) -> qsc_fir::fir::ExprId {
        // Full type of the underlying lifted callable, whose input is the tuple
        // `(captures.., explicit_input)` when the closure has captures.
        let full_ty = callable_types[&closure.id].ty.clone();

        if closure.fixed_args.is_empty() {
            // Captureless closure: a direct `Var` reference to the callable suffices;
            // defunctionalization specializes it without any capture context.
            let kind = qsc_fir::fir::ExprKind::Var(
                qsc_fir::fir::Res::Item(qsc_fir::fir::ItemId {
                    package: closure.id.package,
                    item: closure.id.item,
                }),
                Vec::new(),
            );
            let expr_id = assigner.next_expr();
            package.exprs.insert(
                expr_id,
                qsc_fir::fir::Expr {
                    id: expr_id,
                    span: package.synthetic_span(),
                    ty: full_ty.clone(),
                    kind,
                    exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                        ..qsc_fir::fir::ExecGraphIdx::ZERO,
                },
            );
            return wrap_expr_with_functor_app(
                package,
                assigner,
                expr_id,
                &full_ty,
                closure.functor,
            );
        }

        // Capturing closure: materialize each capture as an in-scope local, then
        // build an `ExprKind::Closure` value referencing those locals. The capture
        // bindings are emitted in their original leading order so partial evaluation
        // reconstructs the closure's fixed arguments correctly.
        let capture_ty_hints = closure_capture_ty_hints(&full_ty, closure.fixed_args.len());
        let mut capture_locals = Vec::with_capacity(closure.fixed_args.len());
        for (idx, capture) in closure.fixed_args.iter().enumerate() {
            let value_expr_id = lower_value_to_expr(
                package,
                assigner,
                capture,
                capture_ty_hints
                    .as_ref()
                    .map(|capture_tys| &capture_tys[idx]),
                callable_types,
                pending_stmts,
            );
            let value_ty = package
                .exprs
                .get(value_expr_id)
                .expect("just inserted")
                .ty
                .clone();
            let (stmt_id, local_var_id) =
                bind_value_as_local(package, assigner, value_expr_id, &value_ty);
            pending_stmts.push(stmt_id);
            capture_locals.push(local_var_id);
        }

        // The closure value's type is the partially applied arrow that drops the
        // leading captures from the lifted callable's input.
        let closure_ty = partial_applied_closure_ty(&full_ty, closure.fixed_args.len());
        let expr_id = assigner.next_expr();
        package.exprs.insert(
            expr_id,
            qsc_fir::fir::Expr {
                id: expr_id,
                span: package.synthetic_span(),
                ty: closure_ty.clone(),
                kind: qsc_fir::fir::ExprKind::Closure(capture_locals, closure.id.item),
                exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                    ..qsc_fir::fir::ExecGraphIdx::ZERO,
            },
        );
        wrap_expr_with_functor_app(package, assigner, expr_id, &closure_ty, closure.functor)
    }

    /// Returns declared types for the captured prefix of a lifted closure callable.
    ///
    /// Capturing closures are lowered as callables whose input tuple starts with
    /// the fixed capture values followed by the explicit argument. These hints let
    /// captured values, including empty arrays, keep the types from the lowered
    /// callable signature when they are reconstructed in the synthetic entry.
    fn closure_capture_ty_hints(
        full_ty: &qsc_fir::ty::Ty,
        capture_count: usize,
    ) -> Option<Vec<qsc_fir::ty::Ty>> {
        let qsc_fir::ty::Ty::Arrow(arrow) = full_ty else {
            return None;
        };
        let qsc_fir::ty::Ty::Tuple(elems) = arrow.input.as_ref() else {
            return None;
        };
        (elems.len() >= capture_count).then(|| elems[..capture_count].to_vec())
    }

    /// Binds a lowered value expression to a fresh immutable local.
    ///
    /// Returns the `Local` statement and the new local variable id so the caller can
    /// place the statement in the synthetic entry block and reference the local from
    /// a closure capture list.
    fn bind_value_as_local(
        package: &mut qsc_fir::fir::Package,
        assigner: &mut qsc_fir::assigner::Assigner,
        value_expr_id: qsc_fir::fir::ExprId,
        value_ty: &qsc_fir::ty::Ty,
    ) -> (qsc_fir::fir::StmtId, qsc_fir::fir::LocalVarId) {
        let span = package.synthetic_span();
        let local_var_id = assigner.next_local();

        let pat_id = assigner.next_pat();
        package.pats.insert(
            pat_id,
            qsc_fir::fir::Pat {
                id: pat_id,
                span,
                ty: value_ty.clone(),
                kind: qsc_fir::fir::PatKind::Bind(qsc_fir::fir::Ident {
                    id: local_var_id,
                    span,
                    name: "capture".into(),
                }),
            },
        );

        let stmt_id = assigner.next_stmt();
        package.stmts.insert(
            stmt_id,
            qsc_fir::fir::Stmt {
                id: stmt_id,
                span,
                kind: qsc_fir::fir::StmtKind::Local(
                    qsc_fir::fir::Mutability::Immutable,
                    pat_id,
                    value_expr_id,
                ),
                exec_graph_range: qsc_fir::fir::ExecGraphIdx::ZERO
                    ..qsc_fir::fir::ExecGraphIdx::ZERO,
            },
        );

        (stmt_id, local_var_id)
    }

    /// Computes the externally visible arrow type for a capturing closure value.
    ///
    /// The lifted callable's input is the tuple `(captures.., explicit_input)`;
    /// dropping the leading captures yields the closure type the target parameter
    /// expects. The explicit input occupies a single trailing slot, so a one-element
    /// remainder is unwrapped back to that element's type.
    fn partial_applied_closure_ty(
        full_ty: &qsc_fir::ty::Ty,
        capture_count: usize,
    ) -> qsc_fir::ty::Ty {
        if capture_count == 0 {
            return full_ty.clone();
        }
        let qsc_fir::ty::Ty::Arrow(arrow) = full_ty else {
            // A closure value with captures should always have an arrow type; a
            // non-arrow here signals an upstream lowering invariant break.
            debug_assert!(
                false,
                "partial_applied_closure_ty: expected an arrow type for a closure with {capture_count} capture(s), found {full_ty}"
            );
            return full_ty.clone();
        };
        let new_input = match arrow.input.as_ref() {
            qsc_fir::ty::Ty::Tuple(elems) if elems.len() > capture_count => {
                let rest = &elems[capture_count..];
                if rest.len() == 1 {
                    rest[0].clone()
                } else {
                    qsc_fir::ty::Ty::Tuple(rest.to_vec())
                }
            }
            other => {
                // The arrow input must be a tuple with at least one slot left
                // after dropping the captured prefix; anything else means the
                // capture count disagrees with the lowered signature.
                debug_assert!(
                    false,
                    "partial_applied_closure_ty: arrow input {other} cannot drop {capture_count} captured element(s)"
                );
                other.clone()
            }
        };
        qsc_fir::ty::Ty::Arrow(Box::new(qsc_fir::ty::Arrow {
            kind: arrow.kind,
            input: Box::new(new_input),
            output: arrow.output.clone(),
            functors: arrow.functors,
        }))
    }

    fn collect_concrete_qsharp_callables(
        value: &Value,
        callables: &mut FxHashSet<qsc_fir::fir::StoreItemId>,
    ) {
        match value {
            Value::Array(values) => values
                .iter()
                .for_each(|value| collect_concrete_qsharp_callables(value, callables)),
            Value::Closure(closure) => {
                if !callables.contains(&closure.id) {
                    callables.insert(closure.id);
                }
                closure
                    .fixed_args
                    .iter()
                    .for_each(|value| collect_concrete_qsharp_callables(value, callables));
            }
            Value::Global(store_item_id, _) => {
                if !callables.contains(store_item_id) {
                    callables.insert(*store_item_id);
                }
            }
            Value::Tuple(values, _) => values
                .iter()
                .for_each(|value| collect_concrete_qsharp_callables(value, callables)),
            Value::BigInt(_)
            | Value::Bool(_)
            | Value::Double(_)
            | Value::Int(_)
            | Value::Pauli(_)
            | Value::Qubit(_)
            | Value::Range(_)
            | Value::Result(_)
            | Value::String(_)
            | Value::Var(_) => {}
        }
    }

    /// Prepares codegen FIR when a callable is invoked with concrete argument values.
    ///
    /// Uses a synthetic `Call(Var(target), args)` entry expression when callable
    /// args or a generic target's args can be represented as FIR values, making
    /// the concrete target signature and args entry-reachable for full pipeline
    /// participation. Non-generic targets without callable args retain direct
    /// reinvocation. Falls back to a
    /// pin-based approach when args contain runtime identities that cannot be
    /// represented as FIR values.
    ///
    /// The original target is pinned for DCE survival so that `fir_to_qir_from_callable`
    /// can still use the original ID for partial evaluation.
    pub fn prepare_codegen_fir_from_callable_args(
        package_store: &PackageStore,
        callable: qsc_hir::hir::ItemId,
        args: &Value,
        capabilities: TargetCapabilityFlags,
    ) -> Result<(CodegenFir, CallableArgsBackend), Vec<Error>> {
        prepare_codegen_fir_from_callable_args_with_functor(
            package_store,
            callable,
            FunctorApp::default(),
            args,
            capabilities,
        )
    }

    /// Prepares an invocation of the selected runtime specialization, preserving
    /// its functors in either the synthetic entry or the reinvocation backend.
    /// Semantic generic constraints are checked before either backend admits the
    /// invocation; nominal erasure and concrete callable cloning cannot bypass them.
    pub fn prepare_codegen_fir_from_callable_args_with_functor(
        package_store: &PackageStore,
        callable: qsc_hir::hir::ItemId,
        functor: FunctorApp,
        args: &Value,
        capabilities: TargetCapabilityFlags,
    ) -> Result<(CodegenFir, CallableArgsBackend), Vec<Error>> {
        let mut concrete_callables = FxHashSet::default();
        collect_concrete_qsharp_callables(args, &mut concrete_callables);

        let target_callable = fir_callable_id(callable);

        let target_is_generic = package_store
            .get(callable.package)
            .and_then(|unit| unit.package.items.get(callable.item))
            .is_some_and(|item| {
                matches!(&item.kind, qsc_hir::hir::ItemKind::Callable(decl)
                    if !decl.generics.is_empty())
            });
        // A generic target needs the inferred signature on an actual call site
        // for monomorphization, even when no argument contains a callable value.
        if concrete_callables.is_empty() && !target_is_generic {
            let codegen_fir = prepare_codegen_fir_from_callable_with_args(
                package_store,
                callable,
                functor,
                capabilities,
                Some(args),
            )?;
            return Ok((
                codegen_fir,
                CallableArgsBackend::ReinvokeOriginal {
                    callable: target_callable,
                    functor,
                    args: args.clone(),
                },
            ));
        }

        // Runtime identities (allocated qubits, dynamic values, and closures
        // that capture them) cannot be reconstructed as FIR literals, so they
        // keep the pin-based approach where partial evaluation supplies the
        // original values at QIR generation time. Fully lowerable values flow
        // into the self-contained synthetic entry below.
        if !value_is_fir_lowerable(args) {
            let (codegen_fir, callable, args) = prepare_codegen_fir_from_callable_args_pinned(
                package_store,
                callable,
                functor,
                capabilities,
                args,
                concrete_callables,
            )?;
            return Ok((
                codegen_fir,
                CallableArgsBackend::ReinvokeOriginal {
                    callable,
                    functor,
                    args,
                },
            ));
        }

        let (mut fir_store, fir_package_id, _assigner) =
            lower_to_fir(package_store, callable.package, None);

        // Pre-compute callable value types before normalizing concrete callable
        // bodies, so closure values still expose the original generic target
        // signatures needed by monomorphization.
        let mut callable_types = build_callable_type_map(
            package_store,
            &fir_store,
            &concrete_callables,
            target_callable,
        )
        .map_err(|error| vec![*error])?;
        let mut target_arrow = validate_runtime_callable_values_for_target(
            &fir_store,
            target_callable,
            functor,
            args,
            &callable_types,
        )
        .map_err(|error| vec![*error])?;
        let args = concretize_runtime_closure_values(
            &mut fir_store,
            fir_package_id,
            args,
            &target_arrow.arrow.input,
            &mut callable_types,
        )
        .map_err(|error| vec![*error])?;
        erase_runtime_type_evidence(&fir_store, &mut callable_types, &mut target_arrow);
        normalize_callable_signatures(&mut fir_store, &callable_types);

        // Build synthetic Call(Var(target), args) as the entry expression.
        // This makes the target and all callable args entry-reachable for pipeline transforms.
        seed_entry_with_call_to_target(
            &mut fir_store,
            fir_package_id,
            target_callable,
            functor,
            &args,
            &callable_types,
            target_arrow,
        );

        // FIR-lowerable callable values — whether passed directly, captured by a
        // closure, or wrapped inside a UDT field — lower into a self-contained
        // synthetic entry that is evaluated directly. Field-typed callables hidden
        // inside a UDT collapse during defunctionalization and UDT erasure so the
        // entry's argument shape stays aligned with the specialized body.

        // The self-contained synthetic entry consumes the specialized clone
        // directly, so the original target is free to be removed by dead-code
        // elimination and does not need to be pinned.
        let warnings = run_codegen_pipeline_to(
            package_store,
            &mut fir_store,
            fir_package_id,
            qsc_fir_transforms::PipelineStage::Full,
            &[],
        )?;

        // Validate capabilities across the whole reachable program (the synthetic
        // entry and everything it specializes), mirroring the entry-expression path.
        let compute_properties =
            PassContext::run_fir_passes_on_fir(&fir_store, fir_package_id, capabilities)
                .map_err(|errors| map_pass_errors(package_store, callable.package, errors))?;

        Ok((
            CodegenFir {
                fir_store,
                fir_package_id,
                compute_properties,
                warnings,
            },
            CallableArgsBackend::SyntheticEntry,
        ))
    }

    /// Materializes validated callable instances before erasure, including lifted
    /// lambdas with inherited parameters. Each instance owns its concrete target;
    /// values instantiated at different types never mutate a shared declaration.
    fn concretize_runtime_closure_values(
        store: &mut qsc_fir::fir::PackageStore,
        destination: qsc_fir::fir::PackageId,
        value: &Value,
        expected: &qsc_fir::ty::Ty,
        callable_types: &mut rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> RuntimeCallableResult<Value> {
        use qsc_fir::ty::Ty;
        if let Some(structural) = nominal_structural_ty(expected, callable_types).cloned() {
            return concretize_runtime_closure_values(
                store,
                destination,
                value,
                &structural,
                callable_types,
            );
        }
        Ok(match (value, expected) {
            (Value::Array(values), Ty::Array(element)) => Value::Array(std::rc::Rc::new(
                values
                    .iter()
                    .map(|value| {
                        concretize_runtime_closure_values(
                            store,
                            destination,
                            value,
                            element,
                            callable_types,
                        )
                    })
                    .collect::<RuntimeCallableResult<_>>()?,
            )),
            (Value::Tuple(values, udt), Ty::Tuple(elements)) => Value::Tuple(
                values
                    .iter()
                    .zip(elements)
                    .map(|(value, element)| {
                        concretize_runtime_closure_values(
                            store,
                            destination,
                            value,
                            element,
                            callable_types,
                        )
                    })
                    .collect::<RuntimeCallableResult<_>>()?,
                udt.clone(),
            ),
            (Value::Global(..) | Value::Closure(_), _) => concretize_runtime_callable_value(
                store,
                destination,
                value,
                expected,
                callable_types,
            )?,
            _ => value.clone(),
        })
    }

    fn concretize_runtime_callable_value(
        store: &mut qsc_fir::fir::PackageStore,
        destination: qsc_fir::fir::PackageId,
        value: &Value,
        expected: &qsc_fir::ty::Ty,
        callable_types: &mut rustc_hash::FxHashMap<qsc_fir::fir::StoreItemId, CallableValueInfo>,
    ) -> RuntimeCallableResult<Value> {
        let (original, functor, captures) = match value {
            Value::Global(id, functor) => (*id, *functor, &[][..]),
            Value::Closure(closure) => (closure.id, closure.functor, closure.fixed_args.as_ref()),
            _ => return Err(Box::new(Error::NotACallable)),
        };
        let info = callable_types
            .get(&original)
            .expect("validated closure")
            .clone();
        let (full_ty, inferred) = runtime_callable_instantiation(value, expected, callable_types)?;
        let capture_types = closure_capture_ty_hints(&full_ty, captures.len()).unwrap_or_default();
        let fixed_args = captures
            .iter()
            .zip(&capture_types)
            .map(|(value, ty)| {
                concretize_runtime_closure_values(store, destination, value, ty, callable_types)
            })
            .collect::<RuntimeCallableResult<_>>()?;
        let id = if !info.parameters.is_empty() || ty_contains_unresolved(&info.formal_ty) {
            let id =
                clone_concrete_runtime_callable(store, destination, original, &inferred, &full_ty);
            callable_types.insert(
                id,
                CallableValueInfo {
                    formal_ty: full_ty.clone(),
                    ty: full_ty,
                    generics: Vec::new(),
                    parameters: Vec::new(),
                    nominal_types: std::rc::Rc::clone(&info.nominal_types),
                    span: info.span,
                    sources: info.sources.clone(),
                },
            );
            id
        } else {
            original
        };
        Ok(if matches!(value, Value::Global(..)) {
            Value::Global(id, functor)
        } else {
            Value::Closure(Box::new(qsc_eval::val::Closure {
                id,
                fixed_args,
                functor,
            }))
        })
    }

    fn clone_concrete_runtime_callable(
        store: &mut qsc_fir::fir::PackageStore,
        destination: qsc_fir::fir::PackageId,
        original: qsc_fir::fir::StoreItemId,
        inferred: &RuntimeTypeArgs,
        concrete_ty: &qsc_fir::ty::Ty,
    ) -> qsc_fir::fir::StoreItemId {
        use qsc_fir::fir::{CallableImpl, ItemKind, PackageLookup, StoreItemId};
        let source = store.get(original.package).clone();
        let package = store.get_mut(destination);
        let mut cloner = qsc_fir_transforms::FirCloner::from_assigner(
            qsc_fir::assigner::Assigner::from_package(package),
        );
        let target = cloner.clone_nested_item(&source, original.item, package);
        let ItemKind::Callable(decl) = &mut package
            .items
            .get_mut(target)
            .expect("cloned closure target")
            .kind
        else {
            unreachable!("validated callable");
        };
        decl.generics.clear();
        decl.output = resolve_params_with_inferred(&decl.output, inferred);
        let input = decl.input;
        let implementation = decl.implementation.clone();
        normalize_pat_node_types(package, input, inferred);
        if let CallableImpl::Spec(specs) = implementation {
            for spec in std::iter::once(&specs.body)
                .chain(specs.adj.iter())
                .chain(specs.ctl.iter())
                .chain(specs.ctl_adj.iter())
            {
                if let Some(input) = spec.input {
                    normalize_pat_node_types(package, input, inferred);
                }
                normalize_block_node_types(package, spec.block, inferred);
            }
        }
        let qsc_fir::ty::Ty::Arrow(arrow) = concrete_ty else {
            unreachable!("closure arrow")
        };
        let input_ty = package.get_pat(input).ty.clone();
        assert_eq!(&input_ty, arrow.input.as_ref());
        StoreItemId {
            package: destination,
            item: target,
        }
    }

    /// Pin-based fallback for callable args containing non-lowerable closure captures.
    ///
    /// Seeds concrete (non-arrow-input) callables into the entry for reachability,
    /// pins arrow-input callables and the target for DCE survival, and lets
    /// `fir_to_qir_from_callable` handle specialization at QIR generation time.
    fn prepare_codegen_fir_from_callable_args_pinned(
        package_store: &PackageStore,
        callable: qsc_hir::hir::ItemId,
        functor: FunctorApp,
        capabilities: TargetCapabilityFlags,
        args: &Value,
        mut concrete_callables: FxHashSet<qsc_fir::fir::StoreItemId>,
    ) -> Result<(CodegenFir, qsc_fir::fir::StoreItemId, Value), Vec<Error>> {
        let (mut fir_store, fir_package_id, _assigner) =
            lower_to_fir(package_store, callable.package, None);

        let mut target_callable = qsc_fir::fir::StoreItemId {
            package: qsc_lowerer::map_hir_package_to_fir(callable.package),
            item: qsc_lowerer::map_hir_local_item_to_fir(callable.item),
        };
        let mut callable_types = build_callable_type_map(
            package_store,
            &fir_store,
            &concrete_callables,
            target_callable,
        )
        .map_err(|error| vec![*error])?;
        let mut signature = validate_runtime_callable_values_for_target(
            &fir_store,
            target_callable,
            functor,
            args,
            &callable_types,
        )
        .map_err(|error| vec![*error])?;
        let args = concretize_runtime_closure_values(
            &mut fir_store,
            fir_package_id,
            args,
            &signature.arrow.input,
            &mut callable_types,
        )
        .map_err(|error| vec![*error])?;
        if !signature.inferred.is_empty() {
            let base = callable_ty_before_runtime_functor(
                &qsc_fir::ty::Ty::Arrow(Box::new(signature.arrow.clone())),
                functor,
            )
            .ok_or_else(|| {
                vec![Error::InvalidRuntimeCallableFunctor {
                    callable: target_callable,
                    functor,
                }]
            })?;
            target_callable = clone_concrete_runtime_callable(
                &mut fir_store,
                fir_package_id,
                target_callable,
                &signature.inferred,
                &base,
            );
        }
        erase_runtime_type_evidence(&fir_store, &mut callable_types, &mut signature);
        normalize_callable_signatures(&mut fir_store, &callable_types);
        concrete_callables.clear();
        collect_concrete_qsharp_callables(&args, &mut concrete_callables);

        let mut pinned_callables: Vec<qsc_fir::fir::StoreItemId> = Vec::new();
        concrete_callables.retain(|store_item_id| {
            let hir_item_id = qsc_hir::hir::ItemId {
                package: qsc_lowerer::map_fir_package_to_hir(store_item_id.package),
                item: qsc_lowerer::map_fir_local_item_to_hir(store_item_id.item),
            };
            if callable_has_arrow_input(&fir_store, hir_item_id) {
                pinned_callables.push(*store_item_id);
                false
            } else {
                true
            }
        });

        seed_entry_with_callables(&mut fir_store, fir_package_id, &concrete_callables);
        pinned_callables.push(target_callable);
        let warnings = run_codegen_pipeline_to(
            package_store,
            &mut fir_store,
            fir_package_id,
            qsc_fir_transforms::PipelineStage::Full,
            &pinned_callables,
        )?;
        // The pinned target body is not entry-reachable, so the main
        // pipeline above did not return-unify it. Re-root the body-only
        // signature-preserving sub-pipeline at the pinned callables so early
        // returns inside dynamic branches become flag-guarded forward control
        // flow. This must run BEFORE `analyze_all` so RCA sees the
        // post-return-unify shape (no `ReturnWithinDynamicScope`) and
        // `validate_callable_capabilities` passes under Adaptive profiles.
        run_codegen_signature_preserving_subpipeline(
            package_store,
            callable.package,
            &mut fir_store,
            fir_package_id,
            &pinned_callables,
        )?;
        let compute_properties = qsc_rca::Analyzer::init(&fir_store, capabilities).analyze_all();
        validate_callable_capabilities(
            package_store,
            &fir_store,
            &compute_properties,
            target_callable,
            capabilities,
        )?;

        Ok((
            CodegenFir {
                fir_store,
                fir_package_id,
                compute_properties,
                warnings,
            },
            target_callable,
            args,
        ))
    }

    /// Returns `true` if a value can be reconstructed inside the synthetic entry
    /// as FIR literals and callable references.
    ///
    /// Runtime identities such as allocated qubits and dynamic measurement results
    /// have no classical literal form and therefore cannot be lowered.
    fn value_is_fir_lowerable(value: &Value) -> bool {
        match value {
            Value::Int(_)
            | Value::Double(_)
            | Value::Bool(_)
            | Value::BigInt(_)
            | Value::Pauli(_)
            | Value::String(_)
            | Value::Range(_)
            | Value::Result(qsc_eval::val::Result::Val(_))
            | Value::Global(..) => true,
            Value::Tuple(vs, _) => vs.iter().all(value_is_fir_lowerable),
            Value::Array(vs) => vs.iter().all(value_is_fir_lowerable),
            Value::Closure(c) => c.fixed_args.iter().all(value_is_fir_lowerable),
            _ => false,
        }
    }

    fn prepare_codegen_fir_inner(
        package_store: &PackageStore,
        package_id: qsc_hir::hir::PackageId,
        package_override: Option<&qsc_hir::hir::Package>,
        capabilities: TargetCapabilityFlags,
    ) -> Result<CodegenFir, Vec<Error>> {
        let (fir_store, fir_package_id, _) =
            lower_to_fir(package_store, package_id, package_override);

        prepare_codegen_fir_from_lowered_store(
            package_store,
            package_id,
            fir_store,
            fir_package_id,
            capabilities,
        )
    }

    fn prepare_codegen_fir_from_lowered_store(
        package_store: &PackageStore,
        package_id: qsc_hir::hir::PackageId,
        mut fir_store: qsc_fir::fir::PackageStore,
        fir_package_id: qsc_fir::fir::PackageId,
        capabilities: TargetCapabilityFlags,
    ) -> Result<CodegenFir, Vec<Error>> {
        let warnings = run_codegen_pipeline(package_store, &mut fir_store, fir_package_id)?;

        let compute_properties =
            PassContext::run_fir_passes_on_fir(&fir_store, fir_package_id, capabilities)
                .map_err(|errors| map_pass_errors(package_store, package_id, errors))?;

        Ok(CodegenFir {
            fir_store,
            fir_package_id,
            compute_properties,
            warnings,
        })
    }

    pub fn prepare_codegen_fir(
        package_store: &PackageStore,
        package_id: qsc_hir::hir::PackageId,
        capabilities: TargetCapabilityFlags,
    ) -> Result<CodegenFir, Vec<Error>> {
        prepare_codegen_fir_inner(package_store, package_id, None, capabilities)
    }

    pub fn prepare_codegen_fir_from_fir_store(
        package_store: &PackageStore,
        package_id: qsc_hir::hir::PackageId,
        fir_store: &qsc_fir::fir::PackageStore,
        fir_package_id: qsc_fir::fir::PackageId,
        capabilities: TargetCapabilityFlags,
    ) -> Result<CodegenFir, Vec<Error>> {
        prepare_codegen_fir_from_lowered_store(
            package_store,
            package_id,
            fir_store.clone(),
            fir_package_id,
            capabilities,
        )
    }

    /// Prepares codegen FIR for a single callable without inline arguments.
    ///
    /// Used when a callable is referenced but its concrete argument values are not yet known.
    /// For callables with arrow-typed inputs, skips the full pipeline to preserve abstract
    /// higher-order structure that will be specialized later via `prepare_codegen_fir_from_callable_args`.
    pub fn prepare_codegen_fir_from_callable(
        package_store: &PackageStore,
        callable: qsc_hir::hir::ItemId,
        capabilities: TargetCapabilityFlags,
    ) -> Result<CodegenFir, Vec<Error>> {
        prepare_codegen_fir_from_callable_with_args(
            package_store,
            callable,
            FunctorApp::default(),
            capabilities,
            None,
        )
    }

    fn prepare_codegen_fir_from_callable_with_args(
        package_store: &PackageStore,
        callable: qsc_hir::hir::ItemId,
        functor: FunctorApp,
        capabilities: TargetCapabilityFlags,
        args: Option<&Value>,
    ) -> Result<CodegenFir, Vec<Error>> {
        let (mut fir_store, fir_package_id, _assigner) =
            lower_to_fir(package_store, callable.package, None);

        if let Some(args) = args {
            let target = qsc_fir::fir::StoreItemId {
                package: qsc_lowerer::map_hir_package_to_fir(callable.package),
                item: qsc_lowerer::map_hir_local_item_to_fir(callable.item),
            };
            let mut callables = FxHashSet::from_iter([target]);
            collect_concrete_qsharp_callables(args, &mut callables);
            let callable_types =
                build_callable_type_map(package_store, &fir_store, &callables, target)
                    .map_err(|error| vec![*error])?;
            validate_runtime_callable_values_for_target(
                &fir_store,
                qsc_fir::fir::StoreItemId {
                    package: qsc_lowerer::map_hir_package_to_fir(callable.package),
                    item: qsc_lowerer::map_hir_local_item_to_fir(callable.item),
                },
                functor,
                args,
                &callable_types,
            )
            .map_err(|error| vec![*error])?;
        }

        if callable_has_arrow_input(&fir_store, callable) {
            // Callable-based codegen receives the concrete callable arguments later through
            // partially_evaluate_call. Running the FIR transform pipeline from a bare callable
            // reference loses that higher-order call-site information and can leave functor-
            // parameterized arrow types unspecialized.
            return Ok(CodegenFir {
                compute_properties: qsc_rca::Analyzer::init(&fir_store, capabilities).analyze_all(),
                fir_store,
                fir_package_id,
                warnings: Vec::new(),
            });
        }

        seed_entry_with_callable(&mut fir_store, fir_package_id, callable);
        let warnings = run_codegen_pipeline(package_store, &mut fir_store, fir_package_id)?;

        let compute_properties = qsc_rca::Analyzer::init(&fir_store, capabilities).analyze_all();
        validate_callable_capabilities(
            package_store,
            &fir_store,
            &compute_properties,
            qsc_fir::fir::StoreItemId {
                package: qsc_lowerer::map_hir_package_to_fir(callable.package),
                item: qsc_lowerer::map_hir_local_item_to_fir(callable.item),
            },
            capabilities,
        )?;

        Ok(CodegenFir {
            fir_store,
            fir_package_id,
            compute_properties,
            warnings,
        })
    }

    fn compile_to_codegen_fir(
        sources: SourceMap,
        language_features: LanguageFeatures,
        capabilities: TargetCapabilityFlags,
        package_store: &mut PackageStore,
        dependencies: &Dependencies,
    ) -> Result<(qsc_hir::hir::PackageId, CodegenFir), Vec<Error>> {
        if capabilities == TargetCapabilityFlags::all() {
            return Err(vec![Error::UnsupportedRuntimeCapabilities]);
        }

        let (unit, errors) = crate::compile::compile(
            package_store,
            dependencies,
            sources,
            PackageType::Exe,
            capabilities,
            language_features,
        );
        if !errors.is_empty() {
            return Err(errors.iter().map(|e| Error::Compile(e.clone())).collect());
        }

        let package_id = package_store.insert(unit);
        let prepared_fir = prepare_codegen_fir(package_store, package_id, capabilities)?;
        Ok((package_id, prepared_fir))
    }

    pub fn get_qir_from_ast(
        store: &mut PackageStore,
        dependencies: &Dependencies,
        ast_package: qsc_ast::ast::Package,
        sources: SourceMap,
        capabilities: TargetCapabilityFlags,
    ) -> Result<String, Vec<Error>> {
        if capabilities == TargetCapabilityFlags::all() {
            return Err(vec![Error::UnsupportedRuntimeCapabilities]);
        }

        let (unit, errors) = crate::compile::compile_ast(
            store,
            dependencies,
            ast_package,
            sources,
            PackageType::Exe,
            capabilities,
        );

        // Ensure it compiles before trying to add it to the store.
        if !errors.is_empty() {
            return Err(errors.iter().map(|e| Error::Compile(e.clone())).collect());
        }

        let package_id = store.insert(unit);
        let prepared_fir = prepare_codegen_fir(store, package_id, capabilities)?;
        let entry = entry_from_codegen_fir(&prepared_fir);
        let CodegenFir {
            fir_store,
            compute_properties,
            ..
        } = prepared_fir;

        fir_to_qir(&fir_store, capabilities, &compute_properties, &entry).map_err(|e| {
            let source_package_id = e.span().package;
            let source_package = store
                .get(source_package_id)
                .expect("package should be in store");
            vec![Error::PartialEvaluation(WithSource::from_map(
                &source_package.sources,
                e,
            ))]
        })
    }

    pub fn get_rir(
        sources: SourceMap,
        language_features: LanguageFeatures,
        capabilities: TargetCapabilityFlags,
        mut package_store: PackageStore,
        dependencies: &Dependencies,
    ) -> Result<Vec<String>, Vec<Error>> {
        let (_, prepared_fir) = compile_to_codegen_fir(
            sources,
            language_features,
            capabilities,
            &mut package_store,
            dependencies,
        )?;
        let entry = entry_from_codegen_fir(&prepared_fir);
        let CodegenFir {
            fir_store,
            compute_properties,
            ..
        } = prepared_fir;

        let (raw, ssa) = fir_to_rir(
            &fir_store,
            capabilities,
            &compute_properties,
            &entry,
            PartialEvalConfig {
                generate_debug_metadata: true,
            },
        )
        .map_err(|e| {
            let source_package_id = e.span().package;
            let source_package = package_store
                .get(source_package_id)
                .expect("package should be in store");
            vec![Error::PartialEvaluation(WithSource::from_map(
                &source_package.sources,
                e,
            ))]
        })?;
        Ok(vec![raw.to_string(), ssa.to_string()])
    }

    pub fn get_qir(
        sources: SourceMap,
        language_features: LanguageFeatures,
        capabilities: TargetCapabilityFlags,
        mut package_store: PackageStore,
        dependencies: &Dependencies,
    ) -> Result<String, Vec<Error>> {
        let (_, prepared_fir) = compile_to_codegen_fir(
            sources,
            language_features,
            capabilities,
            &mut package_store,
            dependencies,
        )?;
        let entry = entry_from_codegen_fir(&prepared_fir);
        let CodegenFir {
            fir_store,
            compute_properties,
            ..
        } = prepared_fir;

        fir_to_qir(&fir_store, capabilities, &compute_properties, &entry).map_err(|e| {
            let source_package_id = e.span().package;
            let source_package = package_store
                .get(source_package_id)
                .expect("package should be in store");
            vec![Error::PartialEvaluation(WithSource::from_map(
                &source_package.sources,
                e,
            ))]
        })
    }
}
